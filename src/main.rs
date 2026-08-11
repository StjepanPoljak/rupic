use std::fs::File;
use std::io::{ self, Read };

mod pic;
mod common;

use crate::pic::pic16f876::PIC16F876State;
use crate::common::byte_data::*;

fn main() -> io::Result<()> {
    let mut pic: PIC16F876State = PIC16F876State::new();
    pic.init();
    let byte_data = ByteData::new_from_intel_hex("old/movlw.hex")?;
    pic.load_rom(&byte_data);
    pic.run()
}
