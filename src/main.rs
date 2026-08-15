use std::fs::File;
use std::io::{ self, Read };

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
        loop {
	    if SIGINT.swap(false, Ordering::SeqCst) {
		unsafe {
		    for (i, row) in (*(&raw const screen)).iter().enumerate() {

		    for (j, val) in row.iter().enumerate() {
			    print!("{}", if screen[i][j] == 0 { '*' } else { ' ' });
		    }
			println!("");
		}
		}
//		std::fs::write("state.txt", "hello").unwrap();
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
//    pic.load_rom(&ByteData::new_from_intel_hex("/home/stjepan/Develop/TetrisDevice/TETRIS.hex")?);
    pic.load_rom(&ByteData::new_from_intel_hex("./TETRIS.hex")?);
    let pic_idx = board.add_component(Box::new(pic));
//    board.add_breakpoint(pic_idx, 0x5);
    board.init_components();
    board.run();

    Ok(())
}
