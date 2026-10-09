// Core tests that don't need gpasm/gpsim: programs are hand-encoded opcodes.
use crate::common::bus::Bus;
use crate::common::component::Component;

const STATUS: u16 = 0x03;
const C: u8 = 0x01;
const Z: u8 = 0x04;

fn movlw(k: u8) -> u16 { 0x3000 | k as u16 }
fn movwf(f: u8) -> u16 { 0x0080 | f as u16 }
fn movf_w(f: u8) -> u16 { 0x0800 | f as u16 }
fn bsf(f: u8, b: u8) -> u16 { 0x1400 | (b as u16) << 7 | f as u16 }
fn bcf(f: u8, b: u8) -> u16 { 0x1000 | (b as u16) << 7 | f as u16 }
fn rlf_f(f: u8) -> u16 { 0x0D80 | f as u16 }
fn rrf_f(f: u8) -> u16 { 0x0C80 | f as u16 }

fn run(program: &[u16], steps: usize) -> PIC16F876 {
    let mut pic = PIC16F876::new();
    pic.bus.rom[..program.len()].copy_from_slice(program);
    pic.init();
    for _ in 0..steps {
        pic.step();
    }
    pic
}

fn status(pic: &PIC16F876) -> u8 {
    pic.bus.read(STATUS)
}

#[test]
fn rlf_shifts_carry_in() {
    let pic = run(&[movlw(0x81), movwf(0x20), bsf(0x03, 0), rlf_f(0x20)], 4);
    assert_eq!(pic.bus.read(0x20), 0x03);
    assert_ne!(status(&pic) & C, 0);
}

#[test]
fn rrf_shifts_carry_in() {
    let pic = run(&[movlw(0x02), movwf(0x20), bsf(0x03, 0), rrf_f(0x20)], 4);
    assert_eq!(pic.bus.read(0x20), 0x81);
    assert_eq!(status(&pic) & C, 0);
}

#[test]
fn literal_logic_sets_z() {
    for insn in [0x3900u16 /* ANDLW 0 */, 0x3A0F /* XORLW 0x0F */] {
        let pic = run(&[movlw(0x0F), insn], 2);
        assert_eq!(pic.core.w, 0);
        assert_ne!(status(&pic) & Z, 0, "{:#x}", insn);
    }
}

#[test]
fn sublw_flags() {
    // 0x05 - 0x06: borrow -> C clear
    let pic = run(&[movlw(0x06), 0x3C05], 2);
    assert_eq!(pic.core.w, 0xFF);
    assert_eq!(status(&pic) & (C | Z), 0);
    // 0x06 - 0x06 = 0: no borrow -> C and Z set
    let pic = run(&[movlw(0x06), 0x3C06], 2);
    assert_eq!(status(&pic) & (C | Z), C | Z);
}

#[test]
fn addlw_flags() {
    let pic = run(&[movlw(0x80), 0x3E80], 2);
    assert_eq!(pic.core.w, 0);
    assert_eq!(status(&pic) & (C | Z), C | Z);
}

#[test]
fn common_ram_and_pclath_mirrored_in_bank1() {
    // bank 1: write 0x70 and PCLATH, bank 0: read them back
    let pic = run(&[bsf(0x03, 5), movlw(0x5A), movwf(0x70), movwf(0x0A),
                    bcf(0x03, 5), movf_w(0x70)], 6);
    assert_eq!(pic.core.w, 0x5A);
    assert_eq!(pic.bus.read(0x0A), 0x5A);
}

#[test]
fn trisb_and_portb_are_distinct() {
    let mut pic = run(&[bsf(0x03, 5), movlw(0x1F), movwf(0x06), bcf(0x03, 5),
                        movlw(0xA0), movwf(0x06)], 6);
    assert_eq!(pic.bus.read(0x86), 0x1F);
    pic.receive_input(1, 0x15);
    // RB7..RB5 driven by the latch, RB4..RB0 by the external input
    assert_eq!(pic.bus.read(0x06), 0xA0 | 0x15);
}

#[test]
fn movwf_pcl_jumps_to_w() {
    // PCLATH = 0, MOVLW 0x10, MOVWF PCL -> next fetch at 0x10
    let pic = run(&[movlw(0x10), movwf(0x02)], 2);
    assert_eq!(pic.core.pc, 0x10);
}

#[test]
fn addwf_pcl_table_lookup() {
    // 0: MOVLW 1; 1: CALL 3; 2: GOTO 2; 3: ADDWF PCL,F; 4: RETLW 0xAA; 5: RETLW 0xBB
    let pic = run(&[movlw(1), 0x2003, 0x2802, 0x0782, 0x34AA, 0x34BB], 5);
    assert_eq!(pic.core.w, 0xBB);
    assert_eq!(pic.core.pc, 0x02);
}

#[test]
fn timer1_counts_with_prescaler() {
    // T1CON = 0x31: TMR1ON, internal clock, 1:8 prescaler
    let pic = run(&[movlw(0x31), movwf(0x10)], 2 + 8 * 100);
    let tmr1 = (pic.bus.read(0x0F) as u16) << 8 | pic.bus.read(0x0E) as u16;
    assert_eq!(tmr1, 100);
}
