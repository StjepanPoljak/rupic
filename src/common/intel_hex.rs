use crate::common::byte_data::*;
use crate::common::common::*;
use std::io::{self};
use std::io::Read;

#[derive(Clone, PartialEq)]
enum IntelHexRecord {
    Data { addr: u16, bytes: Vec<u8> },
    EndOfFile,
    ExtendedSegment(u16),
    StartSegment, // ignored
    ExtendedLinear(u16),
    StartLinear, // ignored
}

impl IntelHexRecord {
    fn new_from_line_tuple(line_tuple: (usize, &str)) -> io::Result<Self> {
        let (pos, line) = line_tuple;

        if line.len() < 9 {
            return Err(io::Error::other("Input too short."));
        }

        if line.chars().nth(0).unwrap() != ':' {
            return Err(io::Error::other("Invalid entry."));
        }

        let mut col: usize = 1;

        let cnt = parse_hex_word8(line, pos, &mut col)?;
        let addr = parse_hex_word16(line, pos, &mut col)?;
        let rtype = parse_hex_word8(line, pos, &mut col)?;
        let data : Vec<u8> = (1..=cnt)
            .map(|_| parse_hex_word8(line, pos, &mut col))
            .collect::<io::Result<Vec<_>>>()?;

        let chk_orig = parse_hex_word8(line, pos, &mut col)?;

        let mut chk: u8 = cnt;
        chk = chk.wrapping_add(((addr & 0xff00) >> 8) as u8);
        chk = chk.wrapping_add((addr & 0xff) as u8);
        chk = chk.wrapping_add(rtype);

        for byte in &data {
            chk = chk.wrapping_add(*byte);
        }

        if chk_orig != 0u8.wrapping_sub(chk) {
            return Err(io::Error::other("Invalid checksum."));
        }

        match rtype {
            0x0 => Ok(IntelHexRecord::Data { addr, bytes: data }),
            0x1 => Ok(IntelHexRecord::EndOfFile),
            0x2 => Ok(IntelHexRecord::ExtendedSegment(addr)),
            0x3 => Ok(IntelHexRecord::StartSegment),
            0x4 => Ok(IntelHexRecord::ExtendedLinear(addr)),
            0x5 => Ok(IntelHexRecord::StartLinear),
            _ => Err(io::Error::other("Invalid record type."))
        }
    }
}

impl ByteData {
    pub fn new_from_intel_hex(path: &str) -> io::Result<Self> {
        let mut file = std::fs::File::open(path)?;
        let mut res = String::new();
        file.read_to_string(&mut res)?;

        let records = res
            .lines()
            .enumerate()
            .map(IntelHexRecord::new_from_line_tuple).collect::<io::Result<Vec<_>>>()?;

        if records.is_empty() {
            return Err(io::Error::other("Empty record list."));
        }

        if *records.last().unwrap() != IntelHexRecord::EndOfFile {
            return Err(io::Error::other("No EOF found."));
        }

        let mut res = ByteData::new();
        let mut base : usize = 0x0;
        for rec in records.iter() {
            match rec {
                IntelHexRecord::Data { addr, bytes } => {
                    res.push(ByteDataBlock { address: *addr as usize, bytes: bytes.clone() });
                },
                IntelHexRecord::ExtendedLinear(new_base) => {
                    base = (*new_base) as usize;
                },
                _ => ()
            }
        }

        Ok(res)
    }

}
