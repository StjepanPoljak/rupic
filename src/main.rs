use std::fs::File;
use std::io::{ self, Read };
use std::range::Range;

mod pic;
mod common;

use crate::pic::pic16f876::{Bus, Component, MCU, screen};
use crate::pic::pic16f876::PIC16F876Bus;
use crate::pic::pic16f876::PIC16F876;
use crate::common::byte_data::*;

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

fn print_block<I>(range: I, block_no: u8)
where
    I: Iterator<Item = usize> {
    for i in range.map(|x| 4 * (3 + x)) {
        for j in if block_no == 0 { (0..32) } else { (32..64) } {
            print!("{}", if unsafe { screen[i % 32][j * 4] } == 0 { '\u{2588}' } else { '\u{0020}' });
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
        let mut bc = &mut self.components[bc_idx];
        bc.breakpoints.push(Breakpoint{ addr });
        (bc_idx, bc.breakpoints.len() - 1)
    }

    fn init_components(&mut self) {
        self.components.iter_mut().for_each(|bc| bc.component.init());
    }

    fn subscribe(&mut self, bc_idx: usize, out_bc_idx: usize, gpio_idx: Vec<usize>) {
        let mut out_bc = &mut self.components[out_bc_idx];
        out_bc.subscribers.push(Subscriber{ bc_idx, gpio_idx });
    }

    fn get_component(&mut self, bc_idx: usize) -> io::Result<&mut dyn Component> {
        let bc = self.components.get_mut(bc_idx).ok_or(io::Error::other(format!("Component with index {bc_idx} does not exist.")))?;
        Ok(bc.component.as_mut())
    }

    fn step(&mut self) -> bool {
        let mut got_bp = false;
        for (bc_idx, bc) in self.components.iter_mut().enumerate() {
            if let Some(mcu) = bc.component.as_mcu() {
                let mcu_pc = mcu.pc();
                for bp in &bc.breakpoints {
                    if mcu_pc == bp.addr {
                        got_bp = true;
                    }
                }
            }
            let _ = bc.component.step();
        }

        return got_bp;
    }
    
    fn run(&mut self) {
        println!("Press CTRL+C to get screen printout and exit.");
        loop {
            if SIGINT.swap(false, Ordering::SeqCst) {
                println!("");
                print_block((0..=4), 0);
                print_block((5..=9), 1);
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
    unsafe { install_interrupt_signal() };
    let mut pic: PIC16F876 = PIC16F876::new();
    pic.load_rom(&ByteData::new_from_intel_hex("./TETRIS.hex")?);
    let pic_idx = board.add_component(Box::new(pic));
//    board.add_breakpoint(pic_idx, 0x5);
    board.init_components();
    board.run();

    Ok(())
}
