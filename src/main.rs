use std::fs::File;
use std::io::{ self, Read };

mod pic;
mod common;

use crate::pic::pic16f876::{Bus, Component, MCU};
use crate::pic::pic16f876::PIC16F876Bus;
use crate::pic::pic16f876::PIC16F876;
use crate::common::byte_data::*;

pub struct Breakpoint {
    c_idx: usize,
    addr: usize
}

pub struct Board {
    components: Vec<Box<dyn Component>>,
    breakpoints: Vec<Breakpoint>
}

impl Board {
    fn new() -> Self {
        Self { components: vec![], breakpoints: vec![] }
    }

    fn add_component(&mut self, component: Box<dyn Component>) -> usize {
        self.components.push(component);
        self.components.len() - 1
    }

    fn add_breakpoint(&mut self, c_idx: usize, addr: usize) -> usize {
        self.breakpoints.push(Breakpoint{c_idx, addr});
        self.breakpoints.len() - 1
    }

    fn init_components(&mut self) {
        self.components.iter_mut().for_each(|c| c.init());
    }

    fn step(&mut self) -> bool {
        let mut got_bp = false;
        for (c_idx, c) in self.components.iter_mut().enumerate() {
            if let Some(mcu) = c.as_mcu() {
                let mcu_pc = mcu.pc();
                for bp in &self.breakpoints {
                    if bp.c_idx == c_idx && mcu_pc == bp.addr {
                        got_bp = true;
                    }
                }
            }
            let _ = c.step();
        }

        return got_bp;
    }

    fn run(&mut self) {
        loop {
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
    pic.load_rom(&ByteData::new_from_intel_hex("old/movlw.hex")?);

    let pic_idx = board.add_component(Box::new(pic));
    board.add_breakpoint(pic_idx, 0x5);
    board.init_components();
    board.run();

    Ok(())
}
