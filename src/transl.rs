fn parse_u8(s: &str) -> io::Result<u8> {
    if let Some(hex) = s.strip_prefix("0x") {
        return u8::from_str_radix(hex, 16).map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e));
    } else if let Some(binary_s) = s.strip_prefix("B'") {
        if let Some(binary) = binary_s.strip_suffix("'") {
            return u8::from_str_radix(binary, 2).map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e));
        } else {
            return Err(io::Error::other(format!("Unclosed ': {:?}", s)));
        }
    } else {
        return s.parse().map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e));
    }
}

fn to_wf_lsb(tokens: &Vec<String>) -> io::Result<u8> {
    let f = parse_u8(tokens.get(1).ok_or(io::Error::other(format!("Missing f operand for {:?}", tokens[0])))?)?;
    let d = parse_u8(tokens.get(2).ok_or(io::Error::other(format!("Missing d operand for {:?}", tokens[0])))?)?;
    if d > 0xf {
        return Err(io::Error::other(format!("d operand larger than 0xf ({:#x})", d)));
    }
    if f > 0x7f {
        return Err(io::Error::other(format!("f operand larger than 0x7f ({:#x})", d)));
    }
    Ok((d << 7) | f)
}

pub fn transl_to_insn(tokens: &Vec<String>) -> io::Result<u16> {
    let insn_tok = tokens.get(0).ok_or(io::Error::other("Empty token list"))?.as_str();
    let msb_wf = match insn_tok {
        "ADDWF"  => Some(0x7),
        "ANDWF"  => Some(0x5),
        "CLRF"   => Some(0x1),
        "CLRW"   => Some(0x1),
        "COMF"   => Some(0x9),
        "DECF"   => Some(0x3),
        "DECFSZ" => Some(0xb),
        "INCF"   => Some(0xa),
        "INCFSZ" => Some(0xf),
        "IORWF"  => Some(0x4),
        "MOVF"   => Some(0x8),
        "MOVWF"  => Some(0x0),
        "RLF"    => Some(0xd),
        "RRF"    => Some(0xc),
        "SUBWF"  => Some(0x2),
        "SWAPWF" => Some(0xe),
        "XORWF"  => Some(0x6),
        _        => None
    };

    if let Some(msb) = msb_wf {
        let lsb = match msb {
            0x1 => if tokens[0] == "CLRF" {
                       to_wf_lsb(&vec![tokens[0].clone(), tokens[1].clone(), "1".to_string()])?
                   } else {
                       to_wf_lsb(&vec![tokens[0].clone(), "0".to_string(), "0".to_string()])? },
            0x0 => to_wf_lsb(&vec![tokens[0].clone(), "1".to_string(), tokens[1].clone()])?,
            _   => to_wf_lsb(tokens)? };

        return Ok((msb << 8) | lsb as u16);
    }

    Err(io::Error::other(format!("Invalid instruction: {:?}", tokens[0])))
}
