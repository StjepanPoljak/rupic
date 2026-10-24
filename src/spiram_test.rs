// Runs boards/spiram: the PIC bit-bangs SPI to a 23LC1024 (spiram_test.asm).
use std::path::Path;
use crate::Board;
use crate::common::time::{ Time, MS, US };

const BOARD: &str = "boards/spiram/spiram.kicad_sch";

// File registers of spiram_test.asm.
const DONE: usize = 0x20;
const MODE_BEFORE: usize = 0x21;
const MODE_AFTER: usize = 0x22;
const ERRORS: usize = 0x23;
const WRAPPED: usize = 0x24;
const BUFFER: usize = 0x40;
const COUNT: usize = 32;
const PATTERN: u8 = 0xa5;

fn mem(board: &mut Board, pic_idx: usize, addr: usize) -> u8 {
    let mcu = board.get_component(pic_idx).unwrap().as_mcu().unwrap();
    mcu.dump_mem(addr, 0).unwrap().values[0] as u8
}

#[test]
fn pic_writes_and_reads_back_spi_ram() {
    let mut board = Board::new_headless();
    crate::kicad::load(&mut board, Path::new(BOARD)).unwrap();
    let pic_idx = board.find_component("U1").unwrap();
    board.init();

    let timeout: Time = 100 * MS;
    while mem(&mut board, pic_idx, DONE) != 1 {
        assert!(board.now() < timeout, "firmware didn't finish");
        board.step();
    }
    // About 11 instruction cycles per bit: ~90 us per byte at 4 MHz.
    eprintln!("spiram_test.asm done after {} us", board.now() / US);

    assert_eq!(mem(&mut board, pic_idx, MODE_BEFORE), 0x40, "sequential mode after power-up");
    let buffer: Vec<u8> = (0..COUNT).map(|i| mem(&mut board, pic_idx, BUFFER + i)).collect();
    let expected: Vec<u8> = (0..COUNT as u8).map(|i| i ^ PATTERN).collect();
    assert_eq!(buffer, expected);
    assert_eq!(mem(&mut board, pic_idx, ERRORS), 0);
    // The write started 16 bytes before the end of the array.
    assert_eq!(mem(&mut board, pic_idx, WRAPPED), 16 ^ PATTERN);
    assert_eq!(mem(&mut board, pic_idx, MODE_AFTER), 0x00, "byte mode");
    assert_eq!(board.contentions(), 0);
}
