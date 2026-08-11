use std::io::{self};

#[derive(Clone, Copy, Eq, PartialEq)]
pub enum BitStatus {
    Set,
    Clear
}

pub fn trace_insn(msg: &str) {
    if let Ok(debug_insn) = unsafe { std::env::var("DEBUG_INSN") } {
        println!("{}", msg);
    }
}

pub fn parse_hex_digit(ch: char, pos: usize, col: &mut usize) -> io::Result<u8> {
    let d = ch.to_digit(16).ok_or(io::Error::other(format!("Invalid hex digit '{}' at line:{} col:{}", ch, pos, col)))?;
    *col += 1;
    Ok(d as u8)
}

pub fn parse_hex_word8(line: &str, pos: usize, col: &mut usize) -> io::Result<u8> {
    let d1 = parse_hex_digit(line.chars().nth(*col).unwrap(), pos, col)?;
    let d2 = parse_hex_digit(line.chars().nth(*col).unwrap(), pos, col)?;
    Ok(d1 << 4 | d2)
}

pub fn parse_hex_word16(line: &str, pos: usize, col: &mut usize) -> io::Result<u16> {
    let w1 = parse_hex_word8(line, pos, col)? as u16;
    let w2 = parse_hex_word8(line, pos, col)? as u16;
    Ok(w1 << 8 | w2)
}
