use std::collections::HashMap;
use crate::common::byte_data::ByteData;

pub struct RegDump {
    pub value: usize,
    pub bit_width: usize
}

pub struct MemDump {
    pub values: Vec<usize>,
    pub bit_width: usize
}

pub trait MCU {
    fn load_rom(&mut self, byte_data: &ByteData);
    fn pc(&self) -> usize;
    fn dump_mem(&self, addr: usize, item_count: usize) -> Option<MemDump> { None }
    fn dump_regs(&self) -> Option<HashMap<&str, RegDump>> { None }
}
