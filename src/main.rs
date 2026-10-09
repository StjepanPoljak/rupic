use std::io::{ self };
use std::fmt;
use std::sync::atomic::{ Ordering, AtomicU64, AtomicBool };
use std::sync::mpsc::Receiver;
use std::collections::VecDeque;

static SIGINT: AtomicBool = AtomicBool::new(false);

mod pic;
mod common;
mod hw;

#[cfg(test)]
mod tetris_test;

use crate::pic::pic16f876::PIC16F876;
use crate::hw::st7920::ST7920;
use crate::common::byte_data::ByteData;
use crate::common::component::Component;
use crate::common::mcu::MCU;
use crate::common::term::{ init_term, draw, KeyEvent };
use crate::hw::keypad::Keypad;

const KEYPAD_HOLD_STEPS: u32 = 100_000;

static MAIN_TID: AtomicU64 = AtomicU64::new(0);

pub trait Display {
    fn redraw(&mut self, draw: fn(u16, u16, bool));
}

pub trait KeyInput {
    fn get_key(&mut self, key: u8);
}

#[derive(Debug)]
pub enum BoardError {
    ComponentNotFound
}

impl fmt::Display for BoardError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::ComponentNotFound => write!(f, "Component not found.")
        }
    }
}

impl std::error::Error for BoardError {}

pub struct Breakpoint {
    addr: usize
}

// Routes a source component's gpio group (src) to a gpio group (dst) of the
// subscribed component, e.g. keypad group 0 -> PIC PORTB (group 1).
pub struct GpioRoute {
    src: usize,
    dst: usize
}

pub struct Subscriber {
    bc_idx: usize,
    routes: Vec<GpioRoute>
}

pub struct BoardComponent {
    component: Box<dyn Component>,
    breakpoints: Vec<Breakpoint>,
    subscribers: Vec<Subscriber>
}

pub struct Board {
    components: Vec<BoardComponent>,
    needs_term: bool,
    term_rx: Option<Receiver<KeyEvent>>,
    events: VecDeque<KeyEvent>,
    headless: bool
}

extern "C" fn handler(_sig: libc::c_int) {
    SIGINT.store(true, Ordering::SeqCst);
}

pub unsafe fn install_interrupt_signal() {
    let mut sa: libc::sigaction = std::mem::zeroed();
    sa.sa_sigaction = handler as *const() as usize;
    libc::sigemptyset(&mut sa.sa_mask);
    sa.sa_flags = 0;
    libc::sigaction(libc::SIGINT, &sa, std::ptr::null_mut());
}

impl Board {
    fn new() -> Self {
        Self { components: vec![], needs_term: false, term_rx: None, events: VecDeque::new(), headless: false }
    }

    // No terminal, no drawing; input is fed through push_key().
    #[allow(unused)]
    fn new_headless() -> Self {
        Self { headless: true, ..Self::new() }
    }

    #[allow(unused)]
    fn push_key(&mut self, key: u8) {
        self.events.push_back(KeyEvent::Key(key));
    }

    fn add_component(&mut self, component: Box<dyn Component>) -> usize {
        if let Some(_) = component.as_display() {
            self.needs_term = true;
        }
        self.components.push(BoardComponent { component, breakpoints: vec![], subscribers: vec![] });
        self.components.len() - 1
    }

    fn add_breakpoint(&mut self, bc_idx: usize, addr: usize) -> (usize, usize) {
        let bc = &mut self.components[bc_idx];
        bc.breakpoints.push(Breakpoint{ addr });
        (bc_idx, bc.breakpoints.len() - 1)
    }

    fn init(&mut self) {
        self.components.iter_mut().for_each(|bc| bc.component.init());
        if self.needs_term && !self.headless {
            self.term_rx = Some(init_term(unsafe { libc::pthread_self() }));
        }
    }

    // bc_idx receives the gpio groups of out_bc_idx, as (src, dst) pairs.
    fn subscribe(&mut self, bc_idx: usize, out_bc_idx: usize, routes: Vec<(usize, usize)>) {
        let out_bc = &mut self.components[out_bc_idx];
        let routes = routes.into_iter().map(|(src, dst)| GpioRoute { src, dst }).collect();
        out_bc.subscribers.push(Subscriber{ bc_idx, routes });
    }

    fn get_component(&mut self, bc_idx: usize) -> io::Result<&mut dyn Component> {
        let bc = self.components.get_mut(bc_idx).ok_or(io::Error::other(format!("Component with index {bc_idx} does not exist.")))?;
        Ok(bc.component.as_mut())
    }

    fn step(&mut self) -> bool {
        if let Some(rx) = &self.term_rx {
            while let Ok(event) = rx.try_recv() {
                self.events.push_back(event);
            }
        }

        let mut got_bp = false;
        for bc_idx in 0..self.components.len() {
            let bc = &mut self.components[bc_idx];
            if let Some(key_input) = bc.component.as_key_input_mut() {
                if let Some(KeyEvent::Key(key)) = self.events.pop_front() {
                    key_input.get_key(key);
                }
            }
            
            let bc = &self.components[bc_idx];
            if let Some(mcu) = bc.component.as_mcu() {
                let mcu_pc = mcu.pc();
                if bc.breakpoints.iter().any(|bp| bp.addr == mcu_pc) {
                    got_bp = true;
                }
            }
            self.components[bc_idx].component.step();

            let bc = &mut self.components[bc_idx];
            if let Some(display) = bc.component.as_display_mut() {
                if !self.headless {
                    display.redraw(draw);
                }
            }

            let bc = &self.components[bc_idx];
            if bc.component.output_pending() {
                let deliveries: Vec<_> = bc.subscribers
                    .iter().flat_map(|sub| sub.routes.iter().map(|route| {
                        let values = bc.component
                            .get_gpio_values(route.src)
                            .expect("Gpio group not found.");
                        (sub.bc_idx, route.dst, values)})).collect();

                for (target_idx, gpio_idx, values) in deliveries {
                    self.components[target_idx].component.receive_input(gpio_idx, values);
                }
            }

            let bc = &mut self.components[bc_idx];
            bc.component.clear_output_pending();
        }
        got_bp
    }

    fn run(&mut self) {

        loop {
            if SIGINT.load(Ordering::Relaxed) {
                break;
            }

            if self.step() {
                println!("Got breakpoint!");
                break;
            }
        }
    }
}

fn main() -> io::Result<()> {

    let mut board = Board::new();

    let mut pic: PIC16F876 = PIC16F876::new();
    pic.load_rom(&ByteData::new_from_intel_hex("./TETRIS.hex")?);
    let pic_idx = board.add_component(Box::new(pic));

    let display: ST7920 = ST7920::new();
    let display_idx = board.add_component(Box::new(display));

    // The game expects a 74C922-style encoder: key code on RB0..RB3 and
    // "data available" on RB4. A terminal only reports presses, so each one
    // is held for KEYPAD_HOLD_STEPS instructions (~100 ms at 4 MHz).
    let mut keypad: Keypad = Keypad::new(0x10, KEYPAD_HOLD_STEPS);
    keypad.key_to_pin('a' as u8, 0x3);
    keypad.key_to_pin('d' as u8, 0x4);
    keypad.key_to_pin('s' as u8, 0x2);
    keypad.key_to_pin(' ' as u8, 0x5);
    keypad.key_to_pin('k' as u8, 0x6);
    let keypad_idx = board.add_component(Box::new(keypad));
    
    board.subscribe(display_idx, pic_idx, vec![(2, 2)]); // PORTC -> LCD data
    board.subscribe(pic_idx, keypad_idx, vec![(0, 1)]);  // keypad -> PORTB
    board.init();

    unsafe { install_interrupt_signal() };
    board.run();

    Ok(())
}
