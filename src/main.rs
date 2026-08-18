use std::fs::File;
use std::io::{ self };
use std::fmt;
use std::error::Error;

mod pic;
mod common;
mod hw;

use crate::pic::pic16f876::{ SCREEN };
use crate::pic::pic16f876::PIC16F876Bus;
use crate::pic::pic16f876::PIC16F876;
use crate::hw::st7920::ST7920;
use crate::common::byte_data::ByteData;
use crate::common::component::Component;
use crate::common::bus::{ Bus, GpioGroup };
use crate::common::mcu::{ MCU };


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

pub struct Subscriber {
    bc_idx: usize,
    gpio_idx: Vec<usize>
}

pub struct BoardComponent {
    component: Box<dyn Component>,
    breakpoints: Vec<Breakpoint>,
    subscribers: Vec<Subscriber>
}

pub struct Board {
    components: Vec<BoardComponent>
}

use std::sync::atomic::{AtomicBool, Ordering};

static SIGINT: AtomicBool = AtomicBool::new(false);

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

fn print_screen() {
    for i in (0..=9).map(|x| 4 * (3 + x)) {
        for j in if i <= (4 * (3 + 4)) { 0..32 } else { 32..64 } {
            print!("{}", if unsafe { SCREEN[i % 32][j * 4] } == 0 { '\u{2588}' } else { '\u{0020}' });
        }
        println!("");
    }
}

impl Board {
    fn new() -> Self {
        Self { components: vec![] }
    }

    fn add_component(&mut self, component: Box<dyn Component>) -> usize {
        self.components.push(BoardComponent { component, breakpoints: vec![], subscribers: vec![] });
        self.components.len() - 1
    }

    fn add_breakpoint(&mut self, bc_idx: usize, addr: usize) -> (usize, usize) {
        let bc = &mut self.components[bc_idx];
        bc.breakpoints.push(Breakpoint{ addr });
        (bc_idx, bc.breakpoints.len() - 1)
    }

    fn init_components(&mut self) {
        self.components.iter_mut().for_each(|bc| bc.component.init());
    }

    fn subscribe(&mut self, bc_idx: usize, out_bc_idx: usize, gpio_idx: Vec<usize>) {
        let out_bc = &mut self.components[out_bc_idx];
        out_bc.subscribers.push(Subscriber{ bc_idx, gpio_idx });
    }

    fn get_component(&mut self, bc_idx: usize) -> io::Result<&mut dyn Component> {
        let bc = self.components.get_mut(bc_idx).ok_or(io::Error::other(format!("Component with index {bc_idx} does not exist.")))?;
        Ok(bc.component.as_mut())
    }

    fn step(&mut self) -> bool {
        let mut got_bp = false;
        for bc_idx in 0..self.components.len() {
            let bc = &self.components[bc_idx];
            if let Some(mcu) = bc.component.as_mcu() {
                let mcu_pc = mcu.pc();
                if bc.breakpoints.iter().any(|bp| bp.addr == mcu_pc) {
                    got_bp = true;
                }
            }
            self.components[bc_idx].component.step();

            let bc = &self.components[bc_idx];
            if bc.component.output_pending() {
                let deliveries: Vec<_> = bc.subscribers
                    .iter().flat_map(|sub| sub.gpio_idx.iter().map(|&gpio_idx| {
                        let values = bc.component
                            .get_gpio_values(gpio_idx)
                            .expect("Gpio group not found.");
                        (sub.bc_idx, gpio_idx, values)})).collect();

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
        println!("Press CTRL+C to get screen printout and exit.");
        loop {
            if SIGINT.swap(false, Ordering::SeqCst) {
                println!("");
                print_screen();
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

    let mut display: ST7920 = ST7920::new();
    let display_idx = board.add_component(Box::new(display));

    board.subscribe(display_idx, pic_idx, vec![2]);
//    board.add_breakpoint(pic_idx, 0x5);
    board.init_components();

    unsafe { install_interrupt_signal() };
    board.run();

    Ok(())
}
