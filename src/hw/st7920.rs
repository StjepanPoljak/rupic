use crate::common::component::Component;
use crate::common::bus::{ Bus, GpioGroup };
use crate::{ MAIN_TID };
use std::io::{ self };
use crate::common::byte_data::ByteData;
use std::io::{ stdin, stdout, Read, Write };
use termion::raw::IntoRawMode;
use termion::{ cursor, clear, terminal_size };
use std::sync::atomic::{ Ordering, AtomicU64 };

fn clear() {
    stdout().write_all(format!("{}", clear::All).as_bytes());
    stdout().flush().unwrap();
}

fn cursor_hide() {
    stdout().write_all(format!("{}", cursor::Hide).as_bytes());
    stdout().flush().unwrap();
}

fn cursor_show() {
    stdout().write_all(format!("{}", cursor::Show).as_bytes());
    stdout().flush().unwrap();
}

fn draw_raw(x: u16, y: u16, c: char) {
    stdout()
        .write_all(format!("{}{}", cursor::Goto(x + 1, y + 1), c).as_bytes())
        .unwrap();
    stdout().flush().unwrap();
}

fn draw(ha: u16, va: u16, c: char) {
    let (width, height) = terminal_size().unwrap();

    if ha / 4 > 32 {
        if va < 8 {
            draw_raw((ha / 4 - 32) as u16, (va + 8) as u16, c);
        }
    } else {
        draw_raw((ha / 4) as u16, va as u16, c);
    }
}

fn draw_rot(ha: u16, va: u16, c: char) {
    let (width, height) = terminal_size().unwrap();
    let (w_pos, h_pos) = (width / 2 - 8, height / 2 - 16);

    if ha / 4 > 32 {
        if va < 8 {
            draw_raw(w_pos + (va + 8) as u16, h_pos + 32 - ((ha / 4) - 32) as u16, c);
        }
    } else if ha / 4 < 64 {
        draw_raw(w_pos + va as u16, h_pos + 32 - (ha / 4) as u16, c);
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

    fn init(&mut self) {

//        println!("Press CTRL+A x to exit.");
//      let (width, height) = terminal_size().unwrap();

//      println!("width: {}, height: {}", width, height);
        clear();
        cursor_hide();

        std::thread::spawn(move || {
            let mut is_escape = false;
            let raw = stdout().into_raw_mode().unwrap();
            const QUIT_BYTE : u8 = 'x' as u8;

            for byte in stdin().bytes() {
                let b = byte.unwrap();

                if !is_escape && b == 0x01 {
                    is_escape = true;
                    continue;
                } else if is_escape {
                    is_escape = false;
                    match b {
                        QUIT_BYTE => { break; },
                        0x01 => (),
                        _ => { continue; }
                    };
                }
            }
            cursor_show();
            drop(raw);
            let tid = MAIN_TID.load(Ordering::SeqCst) as libc::pthread_t;
            unsafe { libc::pthread_kill(tid, libc::SIGINT); }
        });

    }

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
            for ha in (self.coord.HA..=self.coord.HA + 15).step_by(4) {
                if self.screen[self.coord.VA][ha] != 0 {
                    draw_rot(ha as u16, (self.coord.VA / 4) as u16, '\u{0020}');
                } else {
                    draw_rot(ha as u16, (self.coord.VA / 4) as u16, '\u{2588}');
                }

            }
            self.state = ST7920DataState::VerticalAddress;
        }
    }

    fn output_pending(&self) -> bool { false }
    fn clear_output_pending(&mut self) { }
}
