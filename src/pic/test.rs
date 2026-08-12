use std::collections::HashMap;
use crate::Board;

const TESTS_PER_INSN : usize = 25;

struct TestCase {
    name: String,
    source: Vec<String>,
    regs: Vec<u16>
}

fn concat_lines(lines: &Vec<String>) -> String {
    lines.iter().fold("".to_string(), |acc, x| format!("{}{}\n", acc, x)) + "endl:\nGOTO endl\nEND"
}

fn get_reg(reg: &str, dump: &str) -> io::Result<u16> {
    let re = regex::Regex::new(&format!(r"^{} = ([0-9a-fx]+)$", reg)).unwrap();

    for line in dump.lines() {
        if let Some(caps) = re.captures(&line) {
            return Ok(u16::from_str_radix(&caps[1].trim_start_matches("0x"), 16).unwrap());
        }
    }
    return Err(std::io::Error::other(format!("Could not find '{}'", reg)));
}

fn get_reg_at(addr: u16, dump: &str) -> io::Result<u16> {
    get_reg(&format!(r".*\[{:#x}\]", addr), dump)
}

fn get_status(dump: &str) -> io::Result<u16> {
    let re = regex::Regex::new(r"\s+status\s+=\s+([0-9a-fx]+)\s+").unwrap();

    for line in dump.lines() {
        if let Some(caps) = re.captures(&line) {
            return Ok(u16::from_str_radix(&caps[1].trim_start_matches("0x"), 16).unwrap());
        }
    }
    return Err(std::io::Error::other(format!("Could not find 'status'.")));
}

fn print_file(file_buf: &std::path::PathBuf) -> io::Result<()> {
    println!("{:?}:", file_buf);
    std::fs::read_to_string(file_buf)?.lines().for_each(|l| println!("\t{}", l));
    Ok(())
}

struct GPSimData {
    src: std::path::PathBuf,
    hex: std::path::PathBuf,
    cod: std::path::PathBuf,
    comm: std::path::PathBuf
}

#[derive(PartialEq, Eq)]
struct TestRegs {
    w: u8,
    pc: u16,
    status: u8,
    endl: u16,
    regs: HashMap<u16, u8>
}

fn run_gpsim(test_case: &TestCase, gpsim_data: &GPSimData) -> io::Result<TestRegs> {
    let comm_source = test_case
        .regs
        .iter()
        .fold("break e endl\nrun\ndump".to_string(),
              | acc, x | format!("{}\nreg({})", acc, x));

    let _ = std::fs::write(&gpsim_data.comm, format!("{}\nquit", comm_source).to_string());

    print_file(&gpsim_data.comm)?;

    let gpsim_out = std::process::Command::new("gpsim")
        .args(["-c", gpsim_data.comm.to_str().unwrap(), gpsim_data.cod.to_str().unwrap()]).output()?;

    let gpsim_dump = String::from_utf8_lossy(&gpsim_out.stdout).to_string();
    let mut reg_map = HashMap::<u16, u8>::new();
    for reg in &test_case.regs {
        reg_map.insert(*reg, get_reg_at(*reg, &gpsim_dump)? as u8);
    }

    let break_pc = get_reg("pc", &gpsim_dump)?;
    
    Ok(TestRegs { w: get_reg("W", &gpsim_dump)? as u8,
                  pc: break_pc,
                  status: get_status(&gpsim_dump)? as u8,
                  endl: break_pc,
                  regs: reg_map })
}

fn print_diff(test_regs: &TestRegs, gpsim_test_regs: &TestRegs) {
    println!("{:>8} {:>8} {:>8}", "reg", "test", "gpsim");
    println!("{:>8} {:>8x} {:>8x}", "W", test_regs.w, gpsim_test_regs.w);
    println!("{:>8} {:>8x} {:>8x}", "PC", test_regs.pc, gpsim_test_regs.pc);
    println!("{:>8} {:>8x} {:>8x}", "STATUS", test_regs.status, gpsim_test_regs.status);
    println!("{:>8} {:>8x} {:>8x}", "endl", test_regs.endl, gpsim_test_regs.endl);

    for (reg, gpsim_val) in &gpsim_test_regs.regs {
        println!("{:>8x} {:>8x} {:>8x}", reg, test_regs.regs[&reg], gpsim_val);
    }
}

fn run_test<F>(test_case: &TestCase, test: F) -> io::Result<()>
where F: Fn(&String) {
    unsafe { std::env::set_var("DEBUG_INSN", "1"); }
    let dir = tempfile::tempdir()?;

    let base_path = dir.path().join(&test_case.name);

    let gpsim_data = GPSimData {
        src: base_path.with_extension("asm"),
        hex: base_path.with_extension("hex"),
        cod: base_path.with_extension("cod"),
        comm: dir.path().join(format!("{}.cmd", &test_case.name)) };

    let _ = std::fs::write(&gpsim_data.src, concat_lines(&test_case.source).to_string());

    print_file(&gpsim_data.src)?;

    let gpasm_out = std::process::Command::new("gpasm")
        .args(["-p16f876", "-o", gpsim_data.hex.to_str().unwrap(), gpsim_data.src.to_str().unwrap()]).output()?;

    assert!(
        gpasm_out.status.success(),
        "gpasm failed:\n{}",
        String::from_utf8_lossy(&gpasm_out.stdout)
    );

    print_file(&gpsim_data.hex)?;

    let gpsim_test_regs = run_gpsim(&test_case, &gpsim_data)?;

    let mut board = Board::new();

    let mut pic: PIC16F876 = PIC16F876::new();
    pic.load_rom(&ByteData::new_from_intel_hex(&gpsim_data.hex.to_str().unwrap())?);

    let pic_idx = board.add_component(Box::new(pic));
    board.add_breakpoint(pic_idx, gpsim_test_regs.endl as usize);
    board.init_components();
    board.run();

    let pic_mcu = board.get_component(pic_idx)?
        .as_mcu()
        .ok_or(io::Error::other("Could not convert component to MCU."))?;

    let reg_dump = pic_mcu.dump_regs().unwrap();

    let mut reg_map = HashMap::<u16, u8>::new();
    for mem_addr in &test_case.regs {
        let mem_dump = pic_mcu.dump_mem(*mem_addr as usize, 1).unwrap();
        reg_map.insert(*mem_addr as u16, mem_dump.get_val(0) as u8);
    }

    let test_regs = TestRegs {
        w: reg_dump.get("W").unwrap().value as u8,
                  pc: reg_dump.get("PC").unwrap().value as u16,
                  status: reg_dump.get("STATUS").unwrap().value as u8,
                  endl: gpsim_test_regs.endl,
                  regs: reg_map };


    print_diff(&test_regs, &gpsim_test_regs);

    assert!(test_regs == gpsim_test_regs, "Registers differ.");

    test(&(std::fs::read_to_string(&gpsim_data.hex)?));

    Ok(())
}

fn get_wf_src(insn: &str, op1: u8, op2: u8, addr: u16, d: u8) -> Vec<String> {
    assert!(d <= 1, "Invalid direction bit.");
    vec![ format!("MOVLW {:#x}", op1),
          format!("MOVWF {:#x}", addr),
          format!("MOVLW {:#x}", op2),
          format!("{} {:#x}, {:#x}", insn, addr, d),
          "NOP".to_string() ]
}

fn get_f_src(insn: &str, val: u8, addr: u16) -> Vec<String> {
    vec![ format!("MOVLW {:#x}", val),
          format!("MOVWF {:#x}", addr),
          format!("{} {:#x}", insn, addr) ]
}

fn get_w_src(insn: &str, val: u8) -> Vec<String> {
    vec![ format!("MOVLW {:#x}", val),
          format!("{}", insn) ]
}

fn get_bit_op_src(insn: &str, val: u8, addr: u16, bit: u8) -> Vec<String> {
    assert!(bit < 8, "Invalid bit position.");
    vec![ format!("MOVLW {:#x}", val),
          format!("MOVWF {:#x}", addr),
          format!("{} {:#x}, {:#x}", insn, addr, bit),
          "NOP".to_string() ]
}

fn test_wf(insn: &str) {
    for _ in 0..TESTS_PER_INSN {
        let op1 : u8 = rand::random_range(0..255);
        let op2 : u8 = rand::random_range(0..255);
        let addr : u16 = rand::random_range(0x20..0x7f);
        let wf : u8 = rand::random_range(0..1);
        run_test(&TestCase { name: insn.to_string(),
                             source: get_wf_src(insn, op1, op2, addr, wf),
                             regs: vec![ addr ] },
                 |hex| assert!(!hex.is_empty())).unwrap();
    }
}

fn test_f(insn: &str) {
    for _ in 0..TESTS_PER_INSN {
        let val : u8 = rand::random_range(0..255);
        let addr : u16 = rand::random_range(0x20..0x7f);
        run_test(&TestCase { name: insn.to_string(),
                             source: get_f_src(insn, val, addr),
                             regs: vec![ addr ] },
                 |hex| assert!(!hex.is_empty()));
    }
}

fn test_w(insn: &str) {
    for _ in 0..TESTS_PER_INSN {
        let val : u8 = rand::random_range(0..255);
        run_test(&TestCase { name: insn.to_string(),
                             source: get_w_src(insn, val),
                             regs: vec![ ] },
                 |hex| assert!(!hex.is_empty())).unwrap();
    }
}

fn test_bit_op(insn: &str) {
    for _ in 0..TESTS_PER_INSN {
        let val : u8 = rand::random_range(0..255);
        let addr : u16 = rand::random_range(0x20..0x7f);
        let bit : u8 = rand::random_range(0..7);
        run_test(&TestCase { name: insn.to_string(),
                             source: get_bit_op_src(insn, val, addr, bit),
                             regs: vec![ addr ] },
                 |hex| assert!(!hex.is_empty())).unwrap();
    }
}

#[test]
fn test_addwf() {
    test_wf("ADDWF");
}

#[test]
fn test_subwf() {
    test_wf("SUBWF");
}

#[test]
fn test_andwf() {
    test_wf("ANDWF");
}

#[test]
fn test_iorwf() {
    test_wf("IORWF");
}

#[test]
fn test_comf() {
    test_wf("COMF");
}

#[test]
fn test_decf() {
    test_wf("DECF");
}

#[test]
fn test_decfsz() {
    test_wf("DECFSZ");
}

#[test]
fn test_incf() {
    test_wf("INCF");
}

#[test]
fn test_incfsz() {
    test_wf("INCFSZ");
}

#[test]
fn test_rlf() {
    test_wf("RLF");
}

#[test]
fn test_rrf() {
    test_wf("RRF");
}

#[test]
fn test_movf() {
    test_wf("MOVF");
}

#[test]
fn test_swapf() {
    test_wf("SWAPF");
}

#[test]
fn test_clrf() {
    test_f("CLRF");
}

#[test]
fn test_clrw() {
    test_w("CLRW");
}

#[test]
fn test_bcf() {
    test_bit_op("BCF");
}

#[test]
fn test_bsf() {
    test_bit_op("BSF");
}

#[test]
fn test_btfsc() {
    test_bit_op("BTFSC");
}

#[test]
fn test_btfss() {
    test_bit_op("BTFSS");
}

#[test]
fn test_call() {
    for _ in 0..TESTS_PER_INSN {
        let val : u8 = rand::random_range(0..255);
        let org : u16 = rand::random_range(50..8191);
        run_test(&TestCase { name: "CALL".to_string(),
                             source: vec![
                                 "PCLATH EQU 0x0A",
                                 "MOVLW HIGH fn",
                                 "MOVWF PCLATH",
                                 "CALL fn",
                                 &format!("MOVLW {:#x}", val),
                                 "MOVLW HIGH endl",
                                 "MOVWF PCLATH",
                                 "GOTO endl",
                                 &format!("ORG {:#x}", org),
                                 "fn:",
                                 "RETURN"].iter().map(|s| s.to_string()).collect(),
                             regs: vec![] },
                 |hex| assert!(!hex.is_empty())).unwrap();
    }
}

#[test]
fn test_nested_call() {
    for _ in 0..TESTS_PER_INSN {
        let val : u8 = rand::random_range(0..255);
        let val2 : u8 = rand::random_range(0..255);
        let org : u16 = rand::random_range(50..8191);
        run_test(&TestCase { name: "CALL".to_string(),
                             source: vec![
                                 "PCLATH EQU 0x0A",
                                 "MOVLW HIGH fn2",
                                 "MOVWF PCLATH",
                                 "CALL fn2",
                                 &format!("MOVLW {:#x}", val),
                                 "MOVLW HIGH endl",
                                 "MOVWF PCLATH",
                                 "GOTO endl",
                                 "fn1:",
                                 &format!("MOVLW {:#x}", val2),
                                 "RETURN",
                                 &format!("ORG {:#x}", org),
                                 "fn2:",
                                 "MOVLW HIGH fn1",
                                 "MOVWF PCLATH",
                                 "CALL fn1",
                                 "RETURN"].iter().map(|s| s.to_string()).collect(),
                             regs: vec![] },
                 |hex| assert!(!hex.is_empty())).unwrap();
    }
}
