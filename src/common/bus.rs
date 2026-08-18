use std::io::{ self };
use std::fmt;
use std::error::Error;

#[derive(Debug)]
pub enum BusError {
    InvalidGpioGroup,
    NotImplemented
}

impl fmt::Display for BusError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidGpioGroup => write!(f, "Invalid GpioGroup."),
	    Self::NotImplemented => write!(f, "Method not implemented.")
        }
    }
}

impl std::error::Error for BusError {}

pub struct GpioGroup {
    pub pin_values: usize,
    pub width: usize,
    pub output_pending: bool
}

// TODO: needs to be reworked to support larger sizes
pub trait Bus {
    fn read(&self, addr: u16) -> u8;
    fn write(&mut self, addr: u16, val: u8);
    fn fetch(&self, pc: u16) -> u16;

    fn get_gpio_group(&self, gpio_idx: usize) -> Result<&GpioGroup, BusError> { Err(BusError::NotImplemented) }
    fn set_gpio_group(&mut self, gpio_idx: usize, value: usize) -> Result<(), BusError> { Err(BusError::NotImplemented) }
}
