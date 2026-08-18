use crate::common::component::Component;
use crate::common::bus::{ Bus, GpioGroup };
use std::io::{ self };
use crate::common::byte_data::ByteData;

#[derive(PartialEq, Eq, Clone, Debug)]
enum ST7920DataState {
    VerticalAddress,
    HorizontalAddress,
    DataPart1,
    DataPart2
}

fn byte_to_array(byte: u8, array: &mut [u8]) {
    for i in 0..=7 {
        array[7 - i] = byte & (1 << i);
    }
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
    pub coord: Coordinate
}

impl ST7920 {
    pub fn new() -> Self {
        Self { screen: [[0u8; 256]; 64],
               state: ST7920DataState::VerticalAddress,
               coord: Coordinate { VA: 0, HA: 0 } }
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

impl Component for ST7920 {

    fn init(&mut self) {}

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
            byte_to_array(pin_values as u8, &mut self.screen[self.coord.VA][self.coord.HA..=(self.coord.HA + 7)]);//SCREEN[COORD.0][COORD.1..=(COORD.1 + 7)]);
            self.state = ST7920DataState::DataPart2;
        } else if self.state == ST7920DataState::DataPart2 {
            byte_to_array(pin_values as u8, &mut self.screen[self.coord.VA][(self.coord.HA + 8)..=(self.coord.HA + 15)]);
            self.state = ST7920DataState::VerticalAddress;
        }
    }

    fn output_pending(&self) -> bool { false }
    fn clear_output_pending(&mut self) { }
}
