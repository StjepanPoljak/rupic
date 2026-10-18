use std::io::{ self };
use std::sync::atomic::{ Ordering, AtomicU64, AtomicBool };

static SIGINT: AtomicBool = AtomicBool::new(false);

mod pic;
mod common;
mod hw;
mod board;

#[cfg(test)]
mod tetris_test;
#[cfg(test)]
mod board_test;

use crate::board::Board;
use crate::pic::pic16f876::PIC16F876;
use crate::hw::st7920::ST7920;
use crate::common::byte_data::ByteData;
use crate::common::mcu::MCU;
use crate::common::component::Ctx;
use crate::common::time::{ Time, MS };
use crate::hw::keypad::Keypad;

// About 100k instructions of the game at 4 MHz.
const KEYPAD_HOLD: Time = 130 * MS;

static MAIN_TID: AtomicU64 = AtomicU64::new(0);

pub trait Display {
    fn redraw(&mut self, draw: fn(u16, u16, bool));
}

pub trait KeyInput {
    fn get_key(&mut self, key: u8, ctx: &mut Ctx);
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

// Puts the Tetris board on board: the PIC running TETRIS.hex, the LCD and
// the keypad. Returns the PIC's index.
fn add_tetris(board: &mut Board) -> io::Result<usize> {
    let mut pic: PIC16F876 = PIC16F876::new();
    pic.load_rom(&ByteData::new_from_intel_hex("./TETRIS.hex")?);
    pic.set_fosc(4_000_000);
    let pic_idx = board.add_component("PIC", Box::new(pic));

    board.add_component("LCD", Box::new(ST7920::new()));

    // The game expects a 74C922-style encoder: key code on RB0..RB3 and
    // "data available" on RB4. A terminal only reports presses, so each one
    // is held for KEYPAD_HOLD.
    let mut keypad: Keypad = Keypad::new(KEYPAD_HOLD);
    keypad.key_to_pin('a' as u8, 0x3);
    keypad.key_to_pin('d' as u8, 0x4);
    keypad.key_to_pin('s' as u8, 0x2);
    keypad.key_to_pin(' ' as u8, 0x5);
    keypad.key_to_pin('k' as u8, 0x6);
    board.add_component("KEYPAD", Box::new(keypad));

    board.net("LCD_RS", &["PIC.RA0", "LCD.RS"]);
    board.net("LCD_RW", &["PIC.RA1", "LCD.RW"]);
    board.net("LCD_E", &["PIC.RA2", "LCD.E"]);
    board.bus("LCD_DB", 8, &["PIC.RC", "LCD.DB"]);
    board.bus("KEY_D", 4, &["PIC.RB", "KEYPAD.D"]);
    board.net("KEY_DA", &["PIC.RB4", "KEYPAD.DA"]);
    Ok(pic_idx)
}

fn main() -> io::Result<()> {
    let mut board = Board::new();
    add_tetris(&mut board)?;
    board.set_realtime(true);
    board.init();

    unsafe { install_interrupt_signal() };
    board.run();

    Ok(())
}
