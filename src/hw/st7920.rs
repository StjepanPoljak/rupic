use crate::{ Display, Component };
use crate::common::byte_data::ByteData;
use termion::{ terminal_size };

fn draw_rot(ha: u16, va: u16, state: bool, draw_raw: fn(u16, u16, bool)) {
    let (width, height) = terminal_size().unwrap();
    let (w_pos, h_pos) = (width / 2 - 8, height / 2 - 16);

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
    need_redraw: bool
}

impl ST7920 {
    pub fn new() -> Self {
        Self { screen: [[0u8; 256]; 64],
               state: ST7920DataState::VerticalAddress,
               coord: Coordinate { VA: 0, HA: 0 },
               need_redraw: false }
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

    fn step(&mut self) -> u32 { 0 }

    fn receive_input(&mut self, gpio_idx: usize, pin_values: usize) {
        if pin_values & 0x80 != 0 && !self.state.is_data() {
            if self.state == ST7920DataState::VerticalAddress {
                self.coord.VA = (pin_values & 0x7f) as usize;
                self.state = ST7920DataState::HorizontalAddress;
            } else {
                self.coord.HA = (((pin_values & 0x7f) as usize) * 16) as usize;
                self.state = ST7920DataState::DataPart1;
            }
        } else if self.state == ST7920DataState::DataPart1 {
            byte_to_array(pin_values as u8, &mut self.screen[self.coord.VA][self.coord.HA..=(self.coord.HA + 7)]);
            self.state = ST7920DataState::DataPart2;
        } else if self.state == ST7920DataState::DataPart2 {
            byte_to_array(pin_values as u8, &mut self.screen[self.coord.VA][(self.coord.HA + 8)..=(self.coord.HA + 15)]);
            self.state = ST7920DataState::VerticalAddress;
            self.need_redraw = true;
        }
    }

    fn output_pending(&self) -> bool { false }
    fn clear_output_pending(&mut self) { }

    fn as_display(&self) -> Option<&dyn Display> { Some(self) }
    fn as_display_mut(&mut self) -> Option<&mut dyn Display> { Some(self) }
}
