use crate::Display;
use crate::common::byte_data::ByteData;
use crate::common::component::{ Component, Ctx };
use crate::common::net::Level;
use crate::common::term::term_size;

const PINS: [&str; 11] = ["RS", "RW", "E", "DB0", "DB1", "DB2", "DB3", "DB4", "DB5", "DB6", "DB7"];
const RW: usize = 1;
const E: usize = 2;
const DB0: usize = 3;

// Byte driven on the data bus during a read. The game waits for bit 7 to be
// *set* (BTFSS PORTCSTATUS, BF in LCD_Wait), the inverse of the datasheet busy
// flag, and polls with RS = 1 when writing data. Answer every read with it.
const READ_VALUE: u8 = 0x80;

fn draw_rot(ha: u16, va: u16, state: bool, draw_raw: fn(u16, u16, bool)) {
    let (width, height) = term_size();
    let (w_pos, h_pos) = ((width / 2).saturating_sub(8), (height / 2).saturating_sub(16));

    if ha / 4 > 32 {
        if va < 8 {
            draw_raw(w_pos + (va + 8) as u16, h_pos + 32 - ((ha / 4) - 32) as u16, state);
        }
    } else if ha / 4 < 64 {
        draw_raw(w_pos + va as u16, h_pos + 32 - (ha / 4) as u16, state);
    }
}


fn byte_to_array(byte: u8, array: &mut [u8]) {
    for i in 0..=7 {
        array[7 - i] = byte & (1 << i);
    }
}

#[derive(PartialEq, Eq, Clone, Debug)]
enum ST7920DataState {
    VerticalAddress,
    HorizontalAddress,
    DataPart1,
    DataPart2
}

impl ST7920DataState {
    fn is_data(&self) -> bool {
        *self == ST7920DataState::DataPart1 || *self == ST7920DataState::DataPart2
    }
}

struct Coordinate {
    VA: usize,
    HA: usize
}

pub struct ST7920 {
    pub screen: [[u8; 256]; 64],
    pub state: ST7920DataState,
    pub coord: Coordinate,
    need_redraw: bool,
    rw: bool,
    e: bool,
    // Level of DB0..DB7, as seen on the bus.
    data: u8,
    // Whether the data bus is driven (during a read).
    reading: bool
}

impl ST7920 {
    pub fn new() -> Self {
        Self { screen: [[0u8; 256]; 64],
               state: ST7920DataState::VerticalAddress,
               coord: Coordinate { VA: 0, HA: 0 },
               need_redraw: false,
               rw: false,
               e: false,
               data: 0,
               reading: false }
    }

    // Decodes a byte written by the MCU (RW = 0, latched when E falls).
    // RS is ignored: commands and data are told apart by their order.
    fn write(&mut self, value: u8) {
        if value & 0x80 != 0 && !self.state.is_data() {
            if self.state == ST7920DataState::VerticalAddress {
                self.coord.VA = (value & 0x7f) as usize;
                self.state = ST7920DataState::HorizontalAddress;
            } else {
                self.coord.HA = (((value & 0x7f) as usize) * 16) as usize;
                self.state = ST7920DataState::DataPart1;
            }
        } else if self.state == ST7920DataState::DataPart1 {
            byte_to_array(value, &mut self.screen[self.coord.VA][self.coord.HA..=(self.coord.HA + 7)]);
            self.state = ST7920DataState::DataPart2;
        } else if self.state == ST7920DataState::DataPart2 {
            byte_to_array(value, &mut self.screen[self.coord.VA][(self.coord.HA + 8)..=(self.coord.HA + 15)]);
            self.state = ST7920DataState::VerticalAddress;
            self.need_redraw = true;
        }
    }

    fn print_screen(&self) {
        for i in (0..=9).map(|x| 4 * (3 + x)) {
            for j in if i <= (4 * (3 + 4)) { 0..32 } else { 32..64 } {
                print!("{}", if unsafe { self.screen[i % 32][j * 4] } == 0 { '\u{2588}' } else { '\u{0020}' });
            }
            println!("");
        }
    }
}

impl Display for ST7920 {
    fn redraw(&mut self, draw: fn(u16, u16, bool)) {
        if !self.need_redraw {
            return;
        }
        for ha in (self.coord.HA..=self.coord.HA + 15).step_by(4) {
            draw_rot(ha as u16, (self.coord.VA / 4) as u16, self.screen[self.coord.VA][ha] != 0, draw);
        }
        self.need_redraw = false;
    }
}

impl Component for ST7920 {

    fn init(&mut self) { }

    fn pins(&self) -> &[&'static str] {
        &PINS
    }

    fn input_changed(&mut self, pin: usize, level: Level, ctx: &mut Ctx) {
        if pin >= DB0 {
            let bit = 1 << (pin - DB0);
            self.data = if level.is_high() { self.data | bit } else { self.data & !bit };
            return;
        }

        let e = self.e;
        match pin {
            RW => self.rw = level.is_high(),
            E => self.e = level.is_high(),
            _ => return
        }
        if e && !self.e && !self.rw {
            self.write(self.data);
        }

        // The data bus is driven only while a read is strobed (RW = 1, E = 1).
        let reading = self.rw && self.e;
        if reading != self.reading {
            self.reading = reading;
            for bit in 0..8 {
                let level = if reading { Level::from_bit(READ_VALUE & (1 << bit) != 0) } else { Level::Z };
                ctx.drive(DB0 + bit, level);
            }
        }
    }

    fn as_display(&self) -> Option<&dyn Display> { Some(self) }
    fn as_display_mut(&mut self) -> Option<&mut dyn Display> { Some(self) }
}
