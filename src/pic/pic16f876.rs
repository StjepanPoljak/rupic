use crate::pic::regs::*;
use crate::common::common::*;
use std::io::{self};
use crate::common::byte_data::*;

include!("./test.rs");

pub struct PIC16F876State {
    pc: u16,
    w: u8,
    program: [u16; 0x2000],
    regs: [u8; 512],
    stack: [u16; 8],
    sp: u8
}

impl PIC16F876State {
    pub fn new() -> Self {
        Self { pc: 0x0,
               w: 0x0,
               program: [0x3fffu16; 0x2000],
               regs: [0u8; 512],
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

    fn set_reg(&mut self, reg: u16, val: u8) {
        self.regs[reg as usize] = val
    }

    fn get_reg(&self, reg: u16) -> u8 {
        self.regs[reg as usize]
    }

    fn get_full_addr(&self, addr: u16) -> u16 {
        ((self.get_pclath() as u16 & 0x18) << 8) | (addr & 0x07ff)
    }

    fn get_status(&self) -> u8 {
        self.regs[Register::STATUS as usize]
    }

    fn get_pclath(&self) -> u8 {
        self.regs[Register::PCLATH as usize]
    }

    fn update_status_bit(&mut self, bit: StatusBit, val: bool) {
        if val == true {
            self.regs[Register::STATUS as usize] |= 1 << (bit as u8);
        } else {
            self.regs[Register::STATUS as usize] &= !(1 << (bit as u8));
        }
    }

    pub fn init(&mut self) {
        self.set_reg(Register::STATUS as u16, 0x18);
        self.set_reg(Register::PCLATH as u16, 0x0);
    }

    fn exec_wf_raw<F>(&mut self, bytes: u16, f: F) -> (u16, u16, u16)
    where F: Fn(u16, u16) -> u16 {
        let lsb = (bytes & 0xff) as u8;
        let reg = (lsb as u16) & 0x7F;
        let res = f(self.get_reg(reg) as u16, self.w as u16);
        let ret = (self.get_reg(reg) as u16, self.w as u16, res);
        match lsb & 0x80 {
            0x00 => { self.w = res as u8; },
            0x80 => { self.set_reg(reg, res as u8); },
            _ => ()
        };
        ret
    }

    fn exec_wf<F>(&mut self, bytes: u16, f: F)
    where F: Fn(u16, u16) -> u16 {
        _ = self.exec_wf_raw(bytes, f);
    }

    fn exec_wf_z_raw<F>(&mut self, bytes: u16, f: F) -> (u16, u16, u16)
    where F: Fn(u16, u16) -> u16 {
        let res = self.exec_wf_raw(bytes, f);
        self.update_status_bit(StatusBit::Z, res.2 as u8 == 0);
        res
    }

    fn exec_wf_z<F>(&mut self, bytes: u16, f: F)
    where F: Fn(u16, u16) -> u16 {
        _ = self.exec_wf_z_raw(bytes, f);
    }

    fn exec_wf_sz<F>(&mut self, bytes: u16, f: F) -> usize
    where F: Fn(u16, u16) -> u16 {
        let mut cycles = 1;
        let res = self.exec_wf_raw(bytes, f);
        if res.2 as u8 == 0 {
            self.pc += 1;
            cycles = 2;
        }
        cycles
    }

    fn exec_wf_c<F>(&mut self, bytes: u16, f: F)
    where F: Fn(u16, u16) -> u16 {
        let res = self.exec_wf_raw(bytes, f);
        self.update_status_bit(StatusBit::C, res.2 > 0xff);
    }

    fn exec_wf_c_rrf<F>(&mut self, bytes: u16, f: F)
    where F: Fn(u16, u16) -> u16 {
        let res = self.exec_wf_raw(bytes, f);
        self.update_status_bit(StatusBit::C, (res.0 & 0x01) == 0x01);
    }

    fn update_add_bits(&mut self, res: (u16, u16, u16)) {
        self.update_status_bit(StatusBit::C, res.2 > 0xff);
        self.update_status_bit(StatusBit::DC, ((res.0 & 0xf) as u16) + ((res.1 & 0xf) as u16) > 0x0f);
    }

    fn exec_addwf(&mut self, bytes: u16) {
        let res = self.exec_wf_z_raw(bytes, |f, w| f + w);
        self.update_add_bits(res);
    }

    fn update_sub_bits(&mut self, res: (u16, u16, u16)) {
        self.update_status_bit(StatusBit::C, !(res.0 < res.1));
        self.update_status_bit(StatusBit::DC, ((res.0 & 0xf) as u16) >= ((res.1 & 0xf) as u16));
    }

    fn exec_subwf(&mut self, bytes: u16) {
        let res = self.exec_wf_z_raw(bytes, |f, w| f.wrapping_sub(w));
        self.update_sub_bits(res);
    }

    fn exec_bittest(&mut self, bytes: u16) -> BitStatus {
        let reg = bytes & 0x007F;
        let pos = ((bytes as u16) & 0x0380) >> 7;
        if self.get_reg(reg) & (1 << pos) != 0 {
            return BitStatus::Set;
        }
        return BitStatus::Clear;
    }

    fn exec_bitop<F>(&mut self, bytes: u16, f: F)
    where F: Fn(u8, u8) -> u8 {
        let reg = bytes & 0x007F;
        let pos = ((bytes as u16) & 0x0380) >> 7;
        let res = f(self.get_reg(reg), 1 << pos);
        self.set_reg(reg, res as u8);
    }

    fn exec_zero_insn(&mut self, msb: u8, lsb: u8) -> usize {
        match lsb {
            0x09 => { /* RETFIE */
                         trace_insn("RETFIE");
                         return 2; },
            0x08 => { /* RETURN */
                         trace_insn("RETURN");
                         self.pop();
                         return 2; },
            0x64 => { /* CLRWDT */
                         trace_insn("CLRWDT");
                         return 1; },
            0x63 => { /* SLEEP */
                         trace_insn("SLEEP");
                         return 2; },
            _    => { /* NOP */
                         trace_insn("NOP");
                         return 1; }
        }
    }

    fn exec_insn(&mut self, bytes: u16) -> io::Result<usize> {
        let mut cycles = 1;
        let mut insn = (bytes & 0x3f00) >> 8;
        let mut executed = true;

        match insn {
            0x07 => { /* ADDWF */
                         trace_insn("ADDWF");
                         self.exec_addwf(bytes); },
            0x05 => { /* ANDWF */
                         trace_insn("ANDWF");
                         self.exec_wf_z(bytes, |f, w| f & w); },
            0x01 => { /* CLRF | CLRW */
                         trace_insn("CLRF | CLRW");
                         self.exec_wf_z(bytes, |_, _| 0); },
            0x09 => { /* COMF */
                         trace_insn("COMF");
                         self.exec_wf_z(bytes, |f, _| !f); },
            0x03 => { /* DECF */
                         trace_insn("DECF");
                         self.exec_wf_z(bytes, |f, _| f.wrapping_sub(1)); },
            0x0b => { /* DECFSZ */
                         trace_insn("DECFSZ");
                         cycles = self.exec_wf_sz(bytes, |f, _| f.wrapping_sub(1)); },
            0x0a => { /* INCF */
                         trace_insn("INCF");
                         self.exec_wf_z(bytes, |f, _| f + 1); },
            0x0f => { /* INCFSZ */
                         trace_insn("INCFSZ");
                         cycles = self.exec_wf_sz(bytes, |f, _| f + 1); },
            0x04 => { /* IORWF */
                         trace_insn("IORWF");
                         self.exec_wf_z(bytes, |f, w| f | w); },
            0x08 => { /* MOVF */
                         trace_insn("MOVF");
                         self.exec_wf_z(bytes, |f, _| f); },
            0x0d => { /* RLF */
                         trace_insn("RLF");
                         self.exec_wf_c(bytes, |f, _| f << 1); },
            0x0c => { /* RRF */
                         trace_insn("RRF");
                         self.exec_wf_c_rrf(bytes, |f, _| f >> 1); },
            0x02 => { /* SUBWF */
                         trace_insn("SUBWF");
                         self.exec_subwf(bytes); },
            0x0e => { /* SWAPF */
                         trace_insn("SWAPF");
                         self.exec_wf(bytes, |f, _| (f << 4) | (f >> 4)); },
            0x06 => { /* XORWF */
                         trace_insn("XORF");
                         self.exec_wf_z(bytes, |f, w| f ^ w); },
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
                    self.exec_wf(bytes, |_, w| w);
                } else {
                    cycles = self.exec_zero_insn(insn as u8, (bytes & 0xff) as u8)
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
                         self.update_add_bits(res); },
            0x1e => { /* SUBLW */
                         trace_insn("SUBLW");
                         let op1 = bytes & 0xff;
                         let res = (op1.wrapping_sub(self.w as u16), op1, self.w as u16);
                         self.w = res.0 as u8;
                         self.update_add_bits(res); },
            _    => { executed = false; }
        }

        if executed {
            return Ok(cycles);
        }

        executed = true;
        let old_insn = insn & 0xf00;
        insn = insn >> 1;

        match insn {
            0xc => { if old_insn == 0 {
                     /* MOVLW */
                        trace_insn("MOVLW");
                        self.w = (bytes & 0xff) as u8; } else {
                     /* RETLW */
                        trace_insn("RETLW");
                        self.w = (bytes & 0xff) as u8;
                        self.pop();
                        cycles = 2; } },
            0x4 => { /* BCF */
                        trace_insn("BCF");
                        self.exec_bitop(bytes, |a, b| a & !b); },
            0x5 => { /* BSF */
                        trace_insn("BSF");
                        self.exec_bitop(bytes, |a, b| a | b); },
            0x6 => { /* BTFSC */
                        trace_insn("BTFSC");
                        if self.exec_bittest(bytes) == BitStatus::Clear {
                            self.pc += 1;
                            cycles = 2;
                        } },
            0x7 => { /* BTFSS */
                        trace_insn("BTFSS");
                        if self.exec_bittest(bytes) == BitStatus::Set {
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

        let addr = self.get_full_addr(bytes);

        match insn {
            0x4 => { /* CALL */
                        trace_insn("CALL");
                        self.push();
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

    pub fn load_rom(&mut self, byte_data: &ByteData) {
        for ByteDataBlock { address: addr, bytes  } in byte_data {
            let start = (addr / 2) as usize;
            let end = ((addr / 2) as usize) + (bytes.len() / 2);
            let bytes16 = bytes
                .chunks_exact(2)
                .map(|c| u16::from_le_bytes ([c[0], c[1]]) & 0x3fff)
                .collect::<Vec<u16>>();
            self.program[start..end].copy_from_slice(&bytes16);
        }
    }

    pub fn run_until(&mut self, breakl: Option<u16>) -> io::Result<()> {

        loop {
            if let Some(addr) = breakl {
                if self.pc == addr {
                    println!("Ran until address {:#x}.", addr);
                    break;
                }
            }
            self.exec_insn(self.program[self.pc as usize])?;
            self.pc += 1;
            if self.pc >= 8192 {
                println!("Ran through whole program memory. Stopping.");
                break;
            }
        }

        Ok(())
    }

    pub fn run(&mut self) -> io::Result<()> {
        self.run_until(None)
    }
}
