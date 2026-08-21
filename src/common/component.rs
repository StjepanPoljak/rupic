use crate::common::mcu::{ MCU };
use crate::{ Display, KeyInput };
use crate::common::bus::{ BusError };

pub trait Component {
    fn init(&mut self);
    fn step(&mut self) -> u32;

    fn receive_input(&mut self, gpio_idx: usize, pin_values: usize) {}
    fn output_pending(&self) -> bool { false }
    fn clear_output_pending(&mut self) { }
    fn get_gpio_values(&self, gpio_idx: usize) -> Result<usize, BusError> { Err(BusError::NotImplemented) }

    fn as_mcu(&self) -> Option<&dyn MCU> { None }
    fn as_mcu_mut(&mut self) -> Option<&mut dyn MCU> { None }

    fn as_display(&self) -> Option<&dyn Display> { None }
    fn as_display_mut(&mut self) -> Option<&mut dyn Display> { None }

    fn as_key_input(&self) -> Option<&dyn KeyInput> { None }
    fn as_key_input_mut(&mut self) -> Option<&mut dyn KeyInput> { None }
}
