use crate::common::component::{ Component, Ctx };
use crate::common::net::Level;
use crate::common::time::{ Time, NS };

const PINS: [&str; 5] = ["CS", "SO", "SI", "SCK", "HOLD"];
const CS: usize = 0;
const SO: usize = 1;
const SI: usize = 2;
const SCK: usize = 3;
const HOLD: usize = 4;

const SIZE: usize = 128 * 1024;
// 24-bit addresses; the upper 7 bits are ignored.
const ADDR_MASK: u32 = SIZE as u32 - 1;
const PAGE_SIZE: u32 = 32;

const READ: u8 = 0x03;
const WRITE: u8 = 0x02;
const RDMR: u8 = 0x05;
const WRMR: u8 = 0x01;
const EDIO: u8 = 0x3b;
const EQIO: u8 = 0x38;
const RSTIO: u8 = 0xff;

// Mode register, bits 7..6.
const MODE_BYTE: u8 = 0x00;
const MODE_PAGE: u8 = 0x80;
const MODE_SEQUENTIAL: u8 = 0x40;

// Maximum delays from the datasheet (DS20005142C, AC characteristics,
// I-temp): output valid after SCK falls, output disable after CS rises,
// output valid after HOLD rises. SO releases as soon as HOLD falls.
const T_V: Time = 25 * NS;
const T_DIS: Time = 20 * NS;
const T_HV: Time = 50 * NS;

#[derive(PartialEq, Eq, Clone, Copy, Debug)]
enum Phase {
    Command,
    // Address bytes still to come, then the data phase of the command.
    Address(u8),
    ReadData,
    WriteData,
    ReadMode,
    WriteMode,
    // Until CS rises: after an unknown command, or the one byte of byte mode.
    Ignore
}

// Microchip 23LC1024: 128K x 8 SPI SRAM, SPI mode 0 or 3. SI is sampled on
// the rising edge of SCK, SO changes after the falling edge. The SDI and
// SQI modes (EDIO, EQIO) aren't emulated. CS and HOLD only act when driven
// low, so a floating HOLD doesn't pause the chip. HOLD acts at once, even
// with SCK high (the real chip waits for SCK to be low).
pub struct SRAM23LC1024 {
    mem: Vec<u8>,
    mode: u8,
    selected: bool,
    held: bool,
    sck: bool,
    si: bool,
    phase: Phase,
    cmd: u8,
    addr: u32,
    shift_in: u8,
    bits_in: u8,
    shift_out: u8,
    bits_out: u8,
    // Level SO drives while selected and not held.
    so: Level
}

impl SRAM23LC1024 {
    pub fn new() -> Self {
        Self { mem: vec![0; SIZE],
               mode: MODE_SEQUENTIAL,
               selected: false,
               held: false,
               sck: false,
               si: false,
               phase: Phase::Command,
               cmd: 0,
               addr: 0,
               shift_in: 0,
               bits_in: 0,
               shift_out: 0,
               bits_out: 0,
               so: Level::Z }
    }

    #[allow(unused)]
    pub fn mem(&self) -> &[u8] {
        &self.mem
    }

    // Address of the byte after addr: sequential mode wraps at the end of
    // the array, page mode within the 32-byte page.
    fn next_addr(&self, addr: u32) -> u32 {
        match self.mode {
            MODE_PAGE => (addr & !(PAGE_SIZE - 1)) | ((addr + 1) & (PAGE_SIZE - 1)),
            _ => (addr + 1) & ADDR_MASK
        }
    }

    fn set_so(&mut self, level: Level, delay: Time, ctx: &mut Ctx) {
        self.so = level;
        ctx.drive_after(SO, level, delay);
    }

    fn command(&mut self, cmd: u8) {
        self.cmd = cmd;
        self.phase = match cmd {
            READ | WRITE => Phase::Address(3),
            RDMR => Phase::ReadMode,
            WRMR => Phase::WriteMode,
            RSTIO => Phase::Ignore,
            EDIO | EQIO => {
                eprintln!("23LC1024: SDI/SQI mode ({cmd:#04x}) isn't emulated, ignored");
                Phase::Ignore
            },
            _ => {
                eprintln!("23LC1024: unknown command {cmd:#04x}, ignored");
                Phase::Ignore
            }
        };
    }

    fn byte_in(&mut self, byte: u8) {
        match self.phase {
            Phase::Command => self.command(byte),
            Phase::Address(left) => {
                self.addr = ((self.addr << 8) | byte as u32) & ADDR_MASK;
                self.phase = match (left, self.cmd) {
                    (1, READ) => Phase::ReadData,
                    (1, _) => Phase::WriteData,
                    (left, _) => Phase::Address(left - 1)
                };
            },
            Phase::WriteData => {
                self.mem[self.addr as usize] = byte;
                self.addr = self.next_addr(self.addr);
                if self.mode == MODE_BYTE {
                    self.phase = Phase::Ignore;
                }
            },
            Phase::WriteMode => {
                if byte & 0xc0 == 0xc0 {
                    eprintln!("23LC1024: reserved mode {byte:#04x}, ignored");
                } else {
                    self.mode = byte & 0xc0;
                }
                self.phase = Phase::Ignore;
            },
            Phase::ReadData | Phase::ReadMode | Phase::Ignore => { }
        }
    }

    // Next bit on SO, after a falling edge of SCK.
    fn bit_out(&mut self, ctx: &mut Ctx) {
        if self.bits_out == 0 {
            self.shift_out = match self.phase {
                Phase::ReadData => {
                    let byte = self.mem[self.addr as usize];
                    self.addr = self.next_addr(self.addr);
                    byte
                },
                Phase::ReadMode => self.mode,
                _ => return
            };
            self.bits_out = 8;
        }
        let level = Level::from_bit(self.shift_out & 0x80 != 0);
        self.shift_out <<= 1;
        self.bits_out -= 1;
        self.set_so(level, T_V, ctx);
        if self.bits_out == 0 && self.phase == Phase::ReadData && self.mode == MODE_BYTE {
            self.phase = Phase::Ignore;
        }
    }

    fn sck_rose(&mut self) {
        self.shift_in = (self.shift_in << 1) | self.si as u8;
        self.bits_in += 1;
        if self.bits_in == 8 {
            self.bits_in = 0;
            self.byte_in(self.shift_in);
        }
    }

    fn sck_fell(&mut self, ctx: &mut Ctx) {
        match self.phase {
            Phase::ReadData | Phase::ReadMode => self.bit_out(ctx),
            // The last bit of a byte-mode read stays on SO for the master
            // to sample; release it on the next falling edge.
            Phase::Ignore if self.so != Level::Z => self.set_so(Level::Z, T_DIS, ctx),
            _ => { }
        }
    }
}

impl Component for SRAM23LC1024 {
    fn init(&mut self) { }

    fn pins(&self) -> &[&'static str] {
        &PINS
    }

    fn input_changed(&mut self, pin: usize, level: Level, ctx: &mut Ctx) {
        match pin {
            CS => {
                let selected = level == Level::Low;
                if selected == self.selected {
                    return;
                }
                self.selected = selected;
                if selected {
                    self.phase = Phase::Command;
                    (self.bits_in, self.bits_out) = (0, 0);
                } else if self.so != Level::Z {
                    self.set_so(Level::Z, T_DIS, ctx);
                }
            },
            SI => self.si = level.is_high(),
            SCK => {
                let sck = level.is_high();
                if sck == self.sck {
                    return;
                }
                self.sck = sck;
                if !self.selected || self.held {
                    return;
                }
                if sck {
                    self.sck_rose();
                } else {
                    self.sck_fell(ctx);
                }
            },
            HOLD => {
                let held = level == Level::Low;
                if held == self.held {
                    return;
                }
                self.held = held;
                if self.selected && self.so != Level::Z {
                    let (level, delay) = if held { (Level::Z, 0) } else { (self.so, T_HV) };
                    ctx.drive_after(SO, level, delay);
                }
            },
            _ => { }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const HALF_PERIOD: Time = 50 * NS;

    // Drives the chip's pins as an SPI master (mode 0 unless the clock is
    // left high), keeping track of the level on SO.
    struct Master {
        ram: SRAM23LC1024,
        now: Time,
        // Drives of SO, as (time, level), oldest first.
        so: Vec<(Time, Level)>
    }

    impl Master {
        fn new() -> Self {
            let mut master = Self { ram: SRAM23LC1024::new(), now: 0, so: vec![] };
            master.set(CS, Level::High);
            master.set(SCK, Level::Low);
            master
        }

        fn set(&mut self, pin: usize, level: Level) {
            let mut ctx = Ctx::new(self.now);
            self.ram.input_changed(pin, level, &mut ctx);
            self.so.extend(ctx.take_drives().into_iter().map(|(pin, level, time)| {
                assert_eq!(pin, SO);
                (time, level)
            }));
        }

        fn wait(&mut self, time: Time) {
            self.now += time;
        }

        fn so_level(&self) -> Level {
            self.so.iter().filter(|(time, _)| *time <= self.now).last().map_or(Level::Z, |(_, level)| *level)
        }

        fn xfer(&mut self, byte: u8) -> u8 {
            let mut read = 0;
            for bit in (0..8).rev() {
                self.set(SI, Level::from_bit(byte & (1 << bit) != 0));
                self.set(SCK, Level::High);
                read = (read << 1) | self.so_level().is_high() as u8;
                self.wait(HALF_PERIOD);
                self.set(SCK, Level::Low);
                self.wait(HALF_PERIOD);
            }
            read
        }

        // Runs one transaction; returns the bytes read during it.
        fn transaction(&mut self, bytes: &[u8]) -> Vec<u8> {
            self.set(CS, Level::Low);
            self.wait(HALF_PERIOD);
            let read = bytes.iter().map(|byte| self.xfer(*byte)).collect();
            self.set(CS, Level::High);
            self.wait(HALF_PERIOD);
            read
        }

        fn write(&mut self, addr: u32, data: &[u8]) {
            let mut bytes = vec![WRITE, (addr >> 16) as u8, (addr >> 8) as u8, addr as u8];
            bytes.extend_from_slice(data);
            self.transaction(&bytes);
        }

        fn read(&mut self, addr: u32, len: usize) -> Vec<u8> {
            let mut bytes = vec![READ, (addr >> 16) as u8, (addr >> 8) as u8, addr as u8];
            bytes.resize(4 + len, 0);
            self.transaction(&bytes)[4..].to_vec()
        }
    }

    #[test]
    fn sequential_write_and_read_back() {
        let mut m = Master::new();
        assert_eq!(m.transaction(&[RDMR, 0]), vec![0, MODE_SEQUENTIAL]);
        m.write(0x1234, &[1, 2, 3, 4]);
        assert_eq!(m.read(0x1234, 4), vec![1, 2, 3, 4]);
        assert_eq!(m.read(0x1233, 6), vec![0, 1, 2, 3, 4, 0]);
        assert_eq!(m.so_level(), Level::Z);
    }

    #[test]
    fn sequential_mode_wraps_at_end_of_array() {
        let mut m = Master::new();
        // The upper 7 address bits are ignored: 0xfffffe is 0x1fffe.
        m.write(0xfffffe, &[1, 2, 3]);
        assert_eq!(m.ram.mem()[0x1fffe..], [1, 2]);
        assert_eq!(m.ram.mem()[0], 3);
        assert_eq!(m.read(0x1ffff, 2), vec![2, 3]);
    }

    #[test]
    fn page_mode_wraps_within_page() {
        let mut m = Master::new();
        m.transaction(&[WRMR, MODE_PAGE]);
        assert_eq!(m.transaction(&[RDMR, 0, 0]), vec![0, MODE_PAGE, MODE_PAGE]);
        m.write(0x3e, &[1, 2, 3]);
        assert_eq!(m.ram.mem()[0x20], 3);
        assert_eq!(m.ram.mem()[0x40], 0);
        assert_eq!(m.read(0x3f, 2), vec![2, 3]);
    }

    #[test]
    fn byte_mode_transfers_one_byte() {
        let mut m = Master::new();
        m.write(0x10, &[1, 2]);
        m.transaction(&[WRMR, MODE_BYTE]);
        m.write(0x10, &[7, 8]);
        assert_eq!(m.ram.mem()[0x10..0x12], [7, 2]);
        assert_eq!(m.read(0x10, 2), vec![7, 0]);
    }

    #[test]
    fn so_follows_falling_edge_after_t_v() {
        let mut m = Master::new();
        m.write(0, &[0x80]);
        m.set(CS, Level::Low);
        for byte in [READ, 0, 0, 0] {
            m.xfer(byte);
        }
        // Already after the falling edge that shifted out bit 7.
        let fell = m.now - HALF_PERIOD;
        assert_eq!(m.so.last(), Some(&(fell + T_V, Level::High)));
        m.xfer(0);
        m.set(CS, Level::High);
        assert_eq!(m.so.last(), Some(&(m.now + T_DIS, Level::Z)));
    }

    #[test]
    fn mode_3_clock_idles_high() {
        let mut m = Master::new();
        m.write(5, &[0xa5]);
        m.set(SCK, Level::High);
        m.set(CS, Level::Low);
        m.wait(HALF_PERIOD);
        // Each bit starts on a falling edge; SI is sampled on the rising one.
        let mut read = 0;
        for byte in [READ, 0, 0, 5, 0] {
            read = 0;
            for bit in (0..8).rev() {
                m.set(SCK, Level::Low);
                m.set(SI, Level::from_bit(byte & (1 << bit) != 0));
                m.wait(HALF_PERIOD);
                m.set(SCK, Level::High);
                read = (read << 1) | m.so_level().is_high() as u8;
                m.wait(HALF_PERIOD);
            }
        }
        assert_eq!(read, 0xa5);
    }

    #[test]
    fn hold_pauses_transfer() {
        let mut m = Master::new();
        m.write(0, &[0xff]);
        m.set(CS, Level::Low);
        for byte in [READ, 0, 0, 0] {
            m.xfer(byte);
        }
        m.set(HOLD, Level::Low);
        m.wait(HALF_PERIOD);
        assert_eq!(m.so_level(), Level::Z);
        // Ignored while held.
        m.xfer(0);
        m.set(HOLD, Level::High);
        m.wait(HALF_PERIOD);
        assert_eq!(m.xfer(0), 0xff);
    }

    #[test]
    fn unknown_command_is_ignored() {
        let mut m = Master::new();
        assert_eq!(m.transaction(&[0x42, WRITE, 0, 0, 0, 1]), vec![0; 6]);
        assert_eq!(m.ram.mem()[0], 0);
        m.write(0, &[9]);
        assert_eq!(m.read(0, 1), vec![9]);
    }
}
