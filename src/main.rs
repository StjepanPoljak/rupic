use std::fs::File;
use std::io::{ self };

mod pic;
mod common;

use crate::pic::pic16f876::{MCU, SCREEN};
use crate::pic::pic16f876::PIC16F876Bus;
use crate::pic::pic16f876::PIC16F876;
use crate::common::byte_data::*;


pub struct GpioGroup {
    pin_values: usize,
    io_mask: usize,
    width: usize,
    output_pending: bool
}
// TODO: needs to be reworked to support larger sizes
pub trait Bus {
    fn read(&self, addr: u16) -> u8;
    fn write(&mut self, addr: u16, val: u8);
    fn fetch(&self, pc: u16) -> u16;

    fn get_gpio_group(&self, gpio_idx: usize) -> io::Result<&GpioGroup>;
    fn set_gpio_group(&mut self, gpio_idx: usize, value: usize) -> io::Result<()>;
}

pub trait Component {
    fn init(&mut self);
    fn step(&mut self) -> u32;

    fn receive_input(&mut self, gpio: &GpioGroup) {}
    fn output_pending(&self) -> bool { false }
    fn clear_output_pending(&mut self) { }

    fn as_mcu(&self) -> Option<&dyn MCU> { None }
    fn as_mcu_mut(&mut self) -> Option<&mut dyn MCU> { None }
}

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
    unsafe { install_interrupt_signal() };
    let mut pic: PIC16F876 = PIC16F876::new();
    pic.load_rom(&ByteData::new_from_intel_hex("./TETRIS.hex")?);
    let pic_idx = board.add_component(Box::new(pic));
//    board.add_breakpoint(pic_idx, 0x5);
    board.init_components();
    board.run();

    Ok(())
}
