use std::fs::File;
use std::io::{ self, Read };

mod pic;
mod common;

use crate::pic::pic16f876::{Bus, Component, MCU};
use crate::pic::pic16f876::PIC16F876Bus;
use crate::pic::pic16f876::PIC16F876;
use crate::common::byte_data::*;

pub struct Breakpoint {
    addr: usize
}

pub struct BoardComponent {
    component: Box<dyn Component>,
    breakpoints: Vec<Breakpoint>
}

pub struct Board {
    components: Vec<BoardComponent>
}

impl Board {
    fn new() -> Self {
        Self { components: vec![] }
    }

    fn add_component(&mut self, component: Box<dyn Component>) -> usize {
        self.components.push(BoardComponent { component, breakpoints: vec![] });
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
