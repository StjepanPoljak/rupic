// Runs boards/spiram: the PIC bit-bangs SPI to a 23LC1024 (spiram_test.asm).
use std::fs::File;
use std::io::{ BufWriter, Write };
use std::path::Path;
use std::process::Command;
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
// Where the firmware writes, 16 bytes before the end of the array.
const ADDR: [u8; 3] = [0x01, 0xff, 0xf0];

fn mem(board: &mut Board, pic_idx: usize, addr: usize) -> u8 {
    let mcu = board.get_component(pic_idx).unwrap().as_mcu().unwrap();
    mcu.dump_mem(addr, 0).unwrap().values[0] as u8
}

// Runs spiram_test.asm to the end, tracing the named nets to trace if
// given. Returns the board and the PIC's index.
fn run_firmware(trace: Option<(Box<dyn Write>, &[&str])>) -> (Board, usize) {
    let mut board = Board::new_headless();
    crate::kicad::load(&mut board, Path::new(BOARD)).unwrap();
    let pic_idx = board.find_component("U1").unwrap();
    if let Some((out, names)) = trace {
        board.trace(out, names).unwrap();
    }
    board.init();

    let timeout: Time = 100 * MS;
    while mem(&mut board, pic_idx, DONE) != 1 {
        assert!(board.now() < timeout, "firmware didn't finish");
        board.step();
    }
    board.stop_trace().unwrap();
    (board, pic_idx)
}

fn pattern() -> Vec<u8> {
    (0..COUNT as u8).map(|i| i ^ PATTERN).collect()
}

#[test]
fn pic_writes_and_reads_back_spi_ram() {
    let (mut board, pic_idx) = run_firmware(None);
    // About 11 instruction cycles per bit: ~90 us per byte at 4 MHz.
    eprintln!("spiram_test.asm done after {} us", board.now() / US);

    assert_eq!(mem(&mut board, pic_idx, MODE_BEFORE), 0x40, "sequential mode after power-up");
    let buffer: Vec<u8> = (0..COUNT).map(|i| mem(&mut board, pic_idx, BUFFER + i)).collect();
    assert_eq!(buffer, pattern());
    assert_eq!(mem(&mut board, pic_idx, ERRORS), 0);
    // The write started 16 bytes before the end of the array.
    assert_eq!(mem(&mut board, pic_idx, WRAPPED), 16 ^ PATTERN);
    assert_eq!(mem(&mut board, pic_idx, MODE_AFTER), 0x00, "byte mode");
    assert_eq!(board.contentions(), 0);
}

// Runs sigrok's SPI decoder (mode 0) on a VCD file; returns the annotations
// of class, as (first sample, text). The VCD input is downsampled from the
// trace's 1 ps to 1 ns per sample.
fn sigrok_spi(vcd: &Path, class: &str) -> Vec<(u64, String)> {
    let out = Command::new("sigrok-cli")
        .args(["-i", vcd.to_str().unwrap(), "-I", "vcd:downsample=1000",
               "-P", "spi:clk=SPI_SCK:mosi=SPI_MOSI:miso=SPI_MISO:cs=RAM_CS:cpol=0:cpha=0",
               "-A", &format!("spi={class}"), "--protocol-decoder-samplenum"])
        .output()
        .expect("sigrok-cli isn't installed");
    assert!(out.status.success(), "sigrok-cli: {}", String::from_utf8_lossy(&out.stderr));
    // Lines look like "226000-4322000 spi-1: 02 01 FF".
    String::from_utf8(out.stdout).unwrap().lines().map(|line| {
        let (samples, text) = line.split_once(" spi-1:").unwrap_or_else(|| panic!("sigrok-cli: {line}"));
        let start = samples.split_once('-').unwrap().0.parse().unwrap();
        (start, text.trim().to_string())
    }).collect()
}

fn hex_bytes(text: &str) -> Vec<u8> {
    text.split_whitespace().map(|byte| u8::from_str_radix(byte, 16).unwrap()).collect()
}

// Decodes the firmware's SPI traffic with sigrok, independently of the
// emulator, and compares every transaction (CS low to CS high) with what
// spiram_test.asm and the 23LC1024 datasheet say should be on the bus.
#[test]
fn sigrok_decodes_expected_spi_traffic() {
    let dir = tempfile::tempdir().unwrap();
    let vcd = dir.path().join("spi.vcd");
    let out = Box::new(BufWriter::new(File::create(&vcd).unwrap()));
    run_firmware(Some((out, &["RAM_CS", "SPI_SCK", "SPI_MOSI", "SPI_MISO"])));

    let warnings = sigrok_spi(&vcd, "warnings");
    assert!(warnings.is_empty(), "sigrok warnings: {warnings:?}");

    let mosi = sigrok_spi(&vcd, "mosi-transfer");
    let miso = sigrok_spi(&vcd, "miso-transfer");
    assert_eq!(mosi.len(), miso.len());
    // CS floats until the PIC drives it (the board's pull-up isn't
    // emulated), which sigrok decodes as an empty transaction.
    let transactions: Vec<(Vec<u8>, Vec<u8>)> = mosi.iter().zip(&miso)
        .map(|((mosi_start, mosi), (miso_start, miso))| {
            assert_eq!(mosi_start, miso_start);
            (hex_bytes(mosi), hex_bytes(miso))
        })
        .filter(|(mosi, _)| !mosi.is_empty())
        .collect();

    // A command, its address, then data; SO stays high-Z (read as 0) until
    // the RAM has data to send.
    let cmd = |bytes: &[u8], data_out: &[u8], data_in: &[u8]| {
        let mosi = [bytes, data_out].concat();
        let miso = [vec![0; bytes.len()], data_in.to_vec()].concat();
        (mosi, miso)
    };
    let write = [&[0x02][..], &ADDR].concat();
    let read = [&[0x03][..], &ADDR].concat();
    let expected = vec![
        cmd(&[0x05], &[0], &[0x40]),                          // RDMR: sequential
        cmd(&write, &pattern(), &[0; COUNT]),                 // WRITE across the end
        cmd(&read, &[0; COUNT], &pattern()),                  // READ it back
        cmd(&[0x01, 0x00], &[], &[]),                         // WRMR: byte mode
        cmd(&[0x03, 0, 0, 0], &[0], &[16 ^ PATTERN]),         // READ 0x000000
        cmd(&[0x05], &[0], &[0x00]),                          // RDMR: byte mode
    ];
    for (i, (got, want)) in transactions.iter().zip(&expected).enumerate() {
        assert_eq!(got, want, "transaction {i} (MOSI, MISO):\n got  {got:02x?}\n want {want:02x?}");
    }
    assert_eq!(transactions.len(), expected.len(), "transactions: {transactions:02x?}");
}
