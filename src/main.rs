use std::io::{ self };
use std::fmt;
use std::sync::atomic::{ Ordering, AtomicU64, AtomicBool };
use std::sync::mpsc::Receiver;
use std::collections::VecDeque;
use std::time::{ Duration, Instant };

static SIGINT: AtomicBool = AtomicBool::new(false);

mod pic;
mod common;
mod hw;

#[cfg(test)]
mod tetris_test;

use crate::pic::pic16f876::PIC16F876;
use crate::hw::st7920::{ ST7920, ST7920_DATA, ST7920_CONTROL };
use crate::common::byte_data::ByteData;
use crate::common::component::Component;
use crate::common::mcu::MCU;
use crate::common::term::{ init_term, draw, present, KeyEvent };
use crate::hw::keypad::Keypad;

const KEYPAD_HOLD_STEPS: u32 = 100_000;

// Board::run checks the throttle and the frame timer every SYNC_STEPS steps.
const SYNC_STEPS: u32 = 1_000;
const FRAME_TIME: Duration = Duration::from_millis(16);
// When the host falls further behind real time than this, stop catching up.
const MAX_LAG: Duration = Duration::from_millis(100);

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
    headless: bool,
    cycles: u64,
    cycle_time: Option<Duration>
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
        Self { components: vec![], needs_term: false, term_rx: None, events: VecDeque::new(), headless: false,
               cycles: 0, cycle_time: None }
    }

    // Makes run() sleep so that each cycle reported by the components'
    // step() takes cycle_time of wall-clock time. Unthrottled by default.
    fn set_cycle_time(&mut self, cycle_time: Duration) {
        self.cycle_time = Some(cycle_time);
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
            self.cycles += self.components[bc_idx].component.step() as u64;

            let bc = &mut self.components[bc_idx];
            if let Some(display) = bc.component.as_display_mut() {
                if !self.headless {
                    display.redraw(draw);
                }
            }

            let bc = &self.components[bc_idx];
            let deliveries: Vec<_> = bc.subscribers
                .iter().flat_map(|sub| sub.routes.iter()
                    .filter(|route| bc.component.output_pending(route.src))
                    .map(|route| {
                        let values = bc.component
                            .get_gpio_values(route.src)
                            .expect("Gpio group not found.");
                        (sub.bc_idx, route.dst, values)})).collect();

            for (target_idx, gpio_idx, values) in deliveries {
                self.components[target_idx].component.receive_input(gpio_idx, values);
            }

            let bc = &mut self.components[bc_idx];
            bc.component.clear_output_pending();
        }
        got_bp
    }

    fn run(&mut self) {
        let draws = self.needs_term && !self.headless;
        let mut last_frame = Instant::now();
        // Wall-clock time at which self.cycles had the value sync_cycles.
        let (mut sync_time, mut sync_cycles) = (Instant::now(), self.cycles);
        let mut steps: u32 = 0;

        loop {
            if SIGINT.load(Ordering::Relaxed) {
                break;
            }

            if self.step() {
                if draws {
                    present();
                }
                println!("Got breakpoint!");
                break;
            }

            steps = steps.wrapping_add(1);
            if steps % SYNC_STEPS != 0 {
                continue;
            }

            if let Some(cycle_time) = self.cycle_time {
                let target = Duration::from_nanos(
                    (cycle_time.as_nanos() as u64).saturating_mul(self.cycles - sync_cycles));
                let elapsed = sync_time.elapsed();
                if elapsed < target {
                    std::thread::sleep(target - elapsed);
                } else if elapsed - target > MAX_LAG {
                    (sync_time, sync_cycles) = (Instant::now(), self.cycles);
                }
            }

            if draws && last_frame.elapsed() >= FRAME_TIME {
                present();
                last_frame = Instant::now();
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
    
    board.subscribe(display_idx, pic_idx, vec![(2, ST7920_DATA),       // PORTC -> LCD data
                                              (0, ST7920_CONTROL)]);  // PORTA -> LCD RS/RW/E
    board.subscribe(pic_idx, display_idx, vec![(ST7920_DATA, 2)]);    // LCD data -> PORTC
    board.subscribe(pic_idx, keypad_idx, vec![(0, 1)]);  // keypad -> PORTB
    board.set_cycle_time(Duration::from_micros(1)); // 4 MHz crystal, Fosc/4 per cycle
    board.init();

    unsafe { install_interrupt_signal() };
    board.run();

    Ok(())
}
