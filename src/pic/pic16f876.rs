use crate::pic::regs::*;
use crate::common::common::*;
use std::io::{self};
use crate::common::byte_data::*;
use crate::{ Bus, Component, GpioGroup };

include!("./test.rs");

pub static mut SCREEN : [[u8; 256]; 64] = [[0u8; 256]; 64];
static mut PORTC_COUNT : u8 = 0;
static mut COORD : (usize, usize) = (0, 0);

fn byte_to_array(byte: u8, array: &mut [u8]) {
    for i in 0..=7 {
        array[7 - i] = byte & (1 << i);
    }
}

pub struct PIC16F876Bus {
    pub rom: [u16; 0x2000],
    pub ram: [u8; 512],
    pub porta: GpioGroup,
    pub portb: GpioGroup,
    pub portc: GpioGroup
}

impl GpioGroup {
    fn new(width: usize) -> Self {
        Self { pin_values: 0x0,
               io_mask: 0x0,
               width,
               output_pending: false
        }
    }
}

impl PIC16F876Bus {
    pub fn new() -> Self {
        PIC16F876Bus { rom: [0x3fffu16; 0x2000],
                       ram: [0u8; 512],
                       porta: GpioGroup::new(8),
                       portb: GpioGroup::new(8),
                       portc: GpioGroup::new(8)
        }
    }
}

pub struct PIC16F876 {
    pub core: P16,
    pub bus: PIC16F876Bus
}

impl PIC16F876 {
    pub fn new() -> Self {
        Self { core: P16::new(), bus: PIC16F876Bus::new() }
    }
}

pub struct P16 {
    pc: u16,
    w: u8,
    stack: [u16; 8],
    sp: u8
}

pub struct RegDump {
    value: usize,
    bit_width: usize
}

pub struct MemDump {
    values: Vec<usize>,
    bit_width: usize
}

impl MemDump {
    fn get_val(&self, idx: usize) -> usize {
        self.values[idx]
    }
}

pub trait MCU {
    fn load_rom(&mut self, byte_data: &ByteData);
    fn pc(&self) -> usize;
    fn dump_mem(&self, addr: usize, item_count: usize) -> Option<MemDump> { None }
    fn dump_regs(&self) -> Option<HashMap<&str, RegDump>> { None }
}

impl MCU for PIC16F876 {
    fn load_rom(&mut self, byte_data: &ByteData) {
        self.bus.load_rom(byte_data);
    }

    fn pc(&self) -> usize {
        self.core.pc as usize
    }

    fn dump_mem(&self, addr: usize, item_count: usize) -> Option<MemDump> {
        let mut vec: Vec<usize> = vec![];
        for i in 0..=item_count {
            vec.push(self.bus.read((addr + i) as u16) as usize);
        }
        Some(MemDump{values: vec, bit_width: 8})
    }

    fn dump_regs(&self) -> Option<HashMap<&str, RegDump>> {
        let mut res = HashMap::<&str, RegDump>::new();
        res.insert("W", RegDump { value: self.core.w as usize, bit_width: 8 });
        res.insert("PC", RegDump { value: self.pc() as usize, bit_width: 14 });
        res.insert("STATUS", RegDump { value: self.core.get_status(&self.bus) as usize, bit_width: 8 });
        Some(res)
    }
}

impl Bus for PIC16F876Bus {
    fn read(&self, addr: u16) -> u8 {
        let mut val = self.ram[addr as usize];
        
        let read_reg = match addr {
            val if val == Register::PORTA as u16 => Some(Register::PORTA),
            val if val == Register::TRISA as u16 => Some(Register::TRISA),
            val if val == Register::PORTB as u16 => Some(Register::PORTB),
            val if val == Register::TRISB as u16 => Some(Register::TRISB),
            val if val == Register::PORTC as u16 => Some(Register::PORTC),
            val if val == Register::TRISC as u16 => Some(Register::TRISC),
            _                                    => None
        };

        if let Some(reg) = read_reg {
            if reg == Register::PORTC {
                val |= 0x80; // busy flag emulation
            }
        }

        val
    }

    fn write(&mut self, addr: u16, val: u8) {
        self.ram[addr as usize] = val;

        let write_reg = match addr {
            val if val == Register::PORTA as u16 => Some(Register::PORTA),
            val if val == Register::TRISA as u16 => Some(Register::TRISA),
            val if val == Register::PORTB as u16 => Some(Register::PORTB),
            val if val == Register::TRISB as u16 => Some(Register::TRISB),
            val if val == Register::PORTC as u16 => Some(Register::PORTC),
            val if val == Register::TRISC as u16 => Some(Register::TRISC),
            _                                    => None
        };
        if let Some(reg) = write_reg {
            if reg == Register::PORTC { unsafe {
                if val & 0x80 != 0 && PORTC_COUNT < 2 {
                    if PORTC_COUNT == 0 {
                        COORD.0 = (val & 0x7f) as usize;
                        PORTC_COUNT = 1;
                    } else {
                        COORD.1 = (((val & 0x7f) as usize) * 16) as usize;
                        PORTC_COUNT = 2;
                    }
                } else if PORTC_COUNT == 2 {
                    byte_to_array(val, &mut SCREEN[COORD.0][COORD.1..=(COORD.1 + 7)]);
                    PORTC_COUNT = 3;
                } else if PORTC_COUNT == 3 {
                    byte_to_array(val, &mut SCREEN[COORD.0][(COORD.1 + 8)..=(COORD.1 + 15)]);
                    PORTC_COUNT = 0;
                }}
            }
        }
    }

    fn fetch(&self, pc: u16) -> u16 {
        self.rom[pc as usize]
    }

    fn get_gpio_group(&self, gpio_idx: usize) -> io::Result<&GpioGroup> {
        match gpio_idx {
            0 => Ok(&self.porta),
            1 => Ok(&self.portb),
            2 => Ok(&self.portc),
            _ => Err(io::Error::other(format!("Group with index {gpio_idx} does not exist.")))
        }
    }

    fn set_gpio_group(&mut self, gpio_idx: usize, value: usize) -> io::Result<()> {
        match gpio_idx {
            0 => Ok(self.porta.pin_values = value),
            1 => Ok(self.portb.pin_values = value),
            2 => Ok(self.portc.pin_values = value),
            _ => Err(io::Error::other(format!("Group with index {gpio_idx} does not exist.")))
        }
    }
}

impl PIC16F876Bus {
    pub fn load_rom(&mut self, byte_data: &ByteData) {
        for ByteDataBlock { address: addr, bytes  } in byte_data {
            let start = (addr / 2) as usize;
            let end = ((addr / 2) as usize) + (bytes.len() / 2);
            let bytes16 = bytes
                .chunks_exact(2)
                .map(|c| u16::from_le_bytes ([c[0], c[1]]) & 0x3fff)
                .collect::<Vec<u16>>();

            if end >= 0x2000 {
                println!("discarding {:#x}..{:#x} <- len: {}", start, end, bytes16.len());
                continue;
            }
            self.rom[start..end].copy_from_slice(&bytes16);
        }
    }
}

impl Component for PIC16F876 {
    fn init(&mut self) {
        self.bus.write(Register::STATUS as u16, 0x18);
        self.bus.write(Register::PCLATH as u16, 0x0);
    }

    fn step(&mut self) -> u32 {
        let insn = self.bus.fetch(self.core.pc);
        let cycles = self.core.exec_insn(insn, &mut self.bus).expect(format!("Unknown instruction: {:#x}", insn).as_str());
        self.core.pc += 1;
        cycles as u32
    }

    fn as_mcu(&self) -> Option<&dyn MCU> { Some(self) }
    fn as_mcu_mut(&mut self) -> Option<&mut dyn MCU> { Some(self) }
}

impl P16 {
    pub fn new() -> Self {
        Self { pc: 0x0,
               w: 0x0,
               stack: [0u16; 8],
               sp: 0 }
    }

    fn push(&mut self) -> io::Result<()> {
        if self.sp >= 8 {
            return Err(io::Error::other("Stack overflow."));
        }
        self.stack[self.sp as usize] = self.pc;
        self.sp += 1;
        Ok(())
    }

    fn pop(&mut self) -> io::Result<()> {
        if self.sp == 0 {
            return Err(io::Error::other("Stack underflow."));
        }
        self.sp -= 1;
        self.pc = self.stack[self.sp as usize];
        Ok(())
    }
/*
    fn get_full_addr(&self, addr: u16, bus: &mut impl Bus) -> u16 {
        ((self.get_pclath(bus) as u16 & 0x18) << 8) | (addr & 0x07ff)
    }
     */
    fn get_status_bit(&self, bit: StatusBit, bus: &mut impl Bus) -> bool {
        self.get_status(bus) & (1 << (bit as u8)) != 0 as u8
    }

    fn direct_addr(&self, addr: u16, bus: &mut impl Bus) -> u16 {
        let rp0 = self.get_status_bit(StatusBit::RP0, bus) as u16;
        let rp1 = self.get_status_bit(StatusBit::RP1, bus) as u16;

        ((rp1 << 1) | rp0) << 7 | (addr as u16 & 0x7F)
    }

    fn indirect_addr(&self, bus: &mut impl Bus) -> u16 {
        let irp = self.get_status_bit(StatusBit::IRP, bus) as u16;
        (irp << 8) | bus.read(Register::FSR as u16) as u16
    }
/*
    fn resolve_addr(&self, addr: u16, bus: &mut impl Bus) -> u16 {
        let rp0_bit = if self.get_status_bit(StatusBit::RP0, bus) { 1 } else { 0 };
        let rp1_bit = if self.get_status_bit(StatusBit::RP1, bus) { 2 } else { 0 };
        let bank = rp0_bit | rp1_bit;
//      let bank = if rp0_bit { 1 } else { 0 };
        (bank << 7) | addr
    }
*/
    fn get_status(&self, bus: &impl Bus) -> u8 {
        bus.read(Register::STATUS as u16)
    }

    fn get_pclath(&self, bus: &mut impl Bus) -> u8 {
        bus.read(Register::PCLATH as u16)
    }

    fn read_reg(&self, addr: u16, bus: &mut impl Bus) -> u8 {
        if addr == (Register::INDF as u16) {
            let real_addr = self.indirect_addr(bus);
            bus.read(real_addr)
        } else if addr == (Register::STATUS as u16) {
            bus.read(addr)
        } else if addr == (Register::PCL as u16) {
            (self.pc & 0xff) as u8
        } else if addr == (Register::FSR as u16) {
            bus.read(addr)
        } else {
            let real_addr = self.direct_addr(addr, bus);
            bus.read(real_addr)
        }
    }

    fn write_reg(&mut self, addr: u16, value: u8, bus: &mut impl Bus) {
        if addr == (Register::INDF as u16) {
            let real_addr = self.indirect_addr(bus);
            bus.write(real_addr, value)
        } else if addr == (Register::STATUS as u16) {
            bus.write(addr, value)
        } else if addr == (Register::PCL as u16) {
            self.pc = ((self.get_pclath(bus) as u16 & 0x1F) << 8) | value as u16;
        } else if addr == (Register::FSR as u16) {
            bus.write(addr, value);
        } else {
            let real_addr = self.direct_addr(addr, bus);
            bus.write(real_addr, value)
        }
    }

    fn update_status_bit(&mut self, bit: StatusBit, val: bool, bus: &mut impl Bus) {
        let old_reg = self.get_status(bus);
        if val == true {
            bus.write(Register::STATUS as u16, old_reg | 1 << (bit as u8));
        } else {
            bus.write(Register::STATUS as u16, old_reg & !(1 << (bit as u8)));
        }
    }

    fn exec_wf_raw<F>(&mut self, bytes: u16, f: F, bus: &mut impl Bus) -> (u16, u16, u16)
    where F: Fn(u16, u16) -> u16 {
        let lsb = (bytes & 0xff) as u8;
        let reg = (lsb as u16) & 0x7F;
        let res = f(self.read_reg(reg, bus) as u16, self.w as u16);
        let ret = (self.read_reg(reg, bus) as u16, self.w as u16, res);
        match lsb & 0x80 {
            0x00 => { self.w = res as u8; },
            0x80 => { self.write_reg(reg, res as u8, bus); },
            _ => ()
        };
        ret
    }

    fn exec_wf<F>(&mut self, bytes: u16, f: F, bus: &mut impl Bus)
    where F: Fn(u16, u16) -> u16 {
        _ = self.exec_wf_raw(bytes, f, bus);
    }

    fn exec_wf_z_raw<F>(&mut self, bytes: u16, f: F, bus: &mut impl Bus) -> (u16, u16, u16)
    where F: Fn(u16, u16) -> u16 {
        let res = self.exec_wf_raw(bytes, f, bus);
        self.update_status_bit(StatusBit::Z, res.2 as u8 == 0, bus);
        res
    }

    fn exec_wf_z<F>(&mut self, bytes: u16, f: F, bus: &mut impl Bus)
    where F: Fn(u16, u16) -> u16 {
        _ = self.exec_wf_z_raw(bytes, f, bus);
    }

    fn exec_wf_sz<F>(&mut self, bytes: u16, f: F, bus: &mut impl Bus) -> usize
    where F: Fn(u16, u16) -> u16 {
        let mut cycles = 1;
        let res = self.exec_wf_raw(bytes, f, bus);
        if res.2 as u8 == 0 {
            self.pc += 1;
            cycles = 2;
        }
        cycles
    }

    fn exec_wf_c<F>(&mut self, bytes: u16, f: F, bus: &mut impl Bus)
    where F: Fn(u16, u16) -> u16 {
        let res = self.exec_wf_raw(bytes, f, bus);
        self.update_status_bit(StatusBit::C, res.2 > 0xff, bus);
    }

    fn exec_wf_c_rrf<F>(&mut self, bytes: u16, f: F, bus: &mut impl Bus)
    where F: Fn(u16, u16) -> u16 {
        let res = self.exec_wf_raw(bytes, f, bus);
        self.update_status_bit(StatusBit::C, (res.0 & 0x01) == 0x01, bus);
    }

    fn update_add_bits(&mut self, res: (u16, u16, u16), bus: &mut impl Bus) {
        self.update_status_bit(StatusBit::C, res.2 > 0xff, bus);
        self.update_status_bit(StatusBit::DC, ((res.0 & 0xf) as u16) + ((res.1 & 0xf) as u16) > 0x0f, bus);
    }

    fn exec_addwf(&mut self, bytes: u16, bus: &mut impl Bus) {
        let res = self.exec_wf_z_raw(bytes, |f, w| f + w, bus);
        self.update_add_bits(res, bus);
    }

    fn update_sub_bits(&mut self, res: (u16, u16, u16), bus: &mut impl Bus) {
        self.update_status_bit(StatusBit::C, !(res.0 < res.1), bus);
        self.update_status_bit(StatusBit::DC, ((res.0 & 0xf) as u16) >= ((res.1 & 0xf) as u16), bus);
    }

    fn exec_subwf(&mut self, bytes: u16, bus: &mut impl Bus) {
        let res = self.exec_wf_z_raw(bytes, |f, w| f.wrapping_sub(w), bus);
        self.update_sub_bits(res, bus);
    }

    fn exec_bittest(&mut self, bytes: u16, bus: &mut impl Bus) -> BitStatus {
        let reg = bytes & 0x007F;
        let pos = ((bytes as u16) & 0x0380) >> 7;
        if self.read_reg(reg, bus) & (1 << pos) != 0 {
            return BitStatus::Set;
        }
        return BitStatus::Clear;
    }

    fn exec_bitop<F>(&mut self, bytes: u16, f: F, bus: &mut impl Bus)
    where F: Fn(u8, u8) -> u8 {
        let reg = bytes & 0x007F;
        let pos = ((bytes as u16) & 0x0380) >> 7;
        let res = f(self.read_reg(reg, bus), 1 << pos);
        self.write_reg(reg, res as u8, bus);
    }

    fn exec_zero_insn(&mut self, msb: u8, lsb: u8, bus: &mut impl Bus) -> usize {
        match lsb {
            0x09 => { /* RETFIE */
                         trace_insn("RETFIE");
                         return 2; },
            0x08 => { /* RETURN */
                         trace_insn("RETURN");
                         let _ = self.pop();
                         return 2; },
            0x64 => { /* CLRWDT */
                         trace_insn("CLRWDT");
                         // TODO: should also reset WDT and prescaler
                         self.update_status_bit(StatusBit::TO, true, bus);
                         self.update_status_bit(StatusBit::PD, true, bus);
                         return 1; },
            0x63 => { /* SLEEP */
                         trace_insn("SLEEP");
                         return 2; },
            _    => { /* NOP */
                         trace_insn("NOP");
                         return 1; }
        }
    }

    fn exec_insn(&mut self, bytes: u16, bus: &mut impl Bus) -> io::Result<usize> {
        let mut cycles = 1;
        let mut insn = (bytes & 0x3f00) >> 8;
        let mut executed = true;

        match insn {
            0x07 => { /* ADDWF */
                         trace_insn("ADDWF");
                         self.exec_addwf(bytes, bus); },
            0x05 => { /* ANDWF */
                         trace_insn("ANDWF");
                         self.exec_wf_z(bytes, |f, w| f & w, bus); },
            0x01 => { /* CLRF | CLRW */
                         trace_insn("CLRF | CLRW");
                         self.exec_wf_z(bytes, |_, _| 0, bus); },
            0x09 => { /* COMF */
                         trace_insn("COMF");
                         self.exec_wf_z(bytes, |f, _| !f, bus); },
            0x03 => { /* DECF */
                         trace_insn("DECF");
                         self.exec_wf_z(bytes, |f, _| f.wrapping_sub(1), bus); },
            0x0b => { /* DECFSZ */
                         trace_insn("DECFSZ");
                         cycles = self.exec_wf_sz(bytes, |f, _| f.wrapping_sub(1), bus); },
            0x0a => { /* INCF */
                         trace_insn("INCF");
                         self.exec_wf_z(bytes, |f, _| f + 1, bus); },
            0x0f => { /* INCFSZ */
                         trace_insn("INCFSZ");
                         cycles = self.exec_wf_sz(bytes, |f, _| f + 1, bus); },
            0x04 => { /* IORWF */
                         trace_insn("IORWF");
                         self.exec_wf_z(bytes, |f, w| f | w, bus);
            },
            0x08 => { /* MOVF */
                         trace_insn("MOVF");
                         self.exec_wf_z(bytes, |f, _| f, bus); },
            0x0d => { /* RLF */
                         trace_insn("RLF");
                         self.exec_wf_c(bytes, |f, _| f << 1, bus); },
            0x0c => { /* RRF */
                         trace_insn("RRF");
                         self.exec_wf_c_rrf(bytes, |f, _| f >> 1, bus); },
            0x02 => { /* SUBWF */
                         trace_insn("SUBWF");
                         self.exec_subwf(bytes, bus); },
            0x0e => { /* SWAPF */
                         trace_insn("SWAPF");
                         self.exec_wf(bytes, |f, _| (f << 4) | (f >> 4), bus); },
            0x06 => { /* XORWF */
                         trace_insn("XORF");
                         self.exec_wf_z(bytes, |f, w| f ^ w, bus); },
            0x39 => { /* ANDLW */
                         trace_insn("ANDLW");
                         self.w = (bytes & 0xff) as u8 & self.w; },
            0x38 => { /* IORLW */
                         trace_insn("IORLW");
                         self.w = (bytes & 0xff) as u8 | self.w; },
            0x3a => { /* XORLW */
                         trace_insn("XORLW");
                         self.w = (bytes & 0xff) as u8 ^ self.w; },
            0x00 => {
                if bytes & 0x80 != 0 {
                    /* MOVWF */
                    trace_insn("MOVWF");
                    self.exec_wf(bytes, |_, w| w, bus);
                } else {
                    cycles = self.exec_zero_insn(insn as u8, (bytes & 0xff) as u8, bus)
                } },
            _ => { executed = false; }
        }

        if executed {
            return Ok(cycles);
        }

        executed = true;
        insn = insn >> 1;

        match insn {
            0x1f => { /* ADDLW */
                         trace_insn("ADDLW");
                         let op1 = bytes & 0xff;
                         let res = (op1 + (self.w as u16), op1, self.w as u16);
                         self.w = res.0 as u8;
                         self.update_add_bits(res, bus); },
            0x1e => { /* SUBLW */
                         trace_insn("SUBLW");
                         let op1 = bytes & 0xff;
                         let res = (op1.wrapping_sub(self.w as u16), op1, self.w as u16);
                         self.w = res.0 as u8;
                         self.update_add_bits(res, bus); },
            _    => { executed = false; }
        }

        if executed {
            return Ok(cycles);
        }

        executed = true;
        insn = insn >> 1;

        match insn {
            0xc => { /* MOVLW */
                        trace_insn("MOVLW");
                        self.w = (bytes & 0xff) as u8; },
            0xd => { /* RETLW */
                        trace_insn("RETLW");
                        self.w = (bytes & 0xff) as u8;
                        let _ = self.pop();
                        cycles = 2; },
            0x4 => { /* BCF */
                        trace_insn("BCF");
                        self.exec_bitop(bytes, |a, b| a & !b, bus); },
            0x5 => { /* BSF */
                        trace_insn("BSF");
                        self.exec_bitop(bytes, |a, b| a | b, bus); },
            0x6 => { /* BTFSC */
                        trace_insn("BTFSC");
                        if self.exec_bittest(bytes, bus) == BitStatus::Clear {
                            self.pc += 1;
                            cycles = 2;
                        } },
            0x7 => { /* BTFSS */
                        trace_insn("BTFSS");
                        if self.exec_bittest(bytes, bus) == BitStatus::Set {
                            self.pc += 1;
                            cycles = 2;
                        } },
            _   => { executed = false; }
        }

        if executed {
            return Ok(cycles);
        }

        executed = true;
        insn = insn >> 1;

        let addr = ((self.get_pclath(bus) as u16 & 0x18) << 8) | (bytes & 0x07ff);
//        let addr = self.get_full_addr(bytes, bus);

        match insn {
            0x4 => { /* CALL */
                        trace_insn("CALL");
                        let _ = self.push();
                        self.pc = addr - 1;
                        cycles = 2;
            },
            0x5 => { /* GOTO */
                        trace_insn("GOTO");
                        self.pc = addr - 1;
                        cycles = 2;
            },
            _ => { executed = false; }
        }

        if executed {
            return Ok(cycles);
        }

        return Err(io::Error::other("Unknown instruction."));
    }
}
