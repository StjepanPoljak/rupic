use std::collections::HashMap;
use crate::common::component::Component;
use crate::common::term::KeyEvent;
use crate::KeyInput;
use crate::common::bus::{ BusError };

pub struct Keypad {
    key_map: HashMap<u8, u8>,
    pin_values: u8,
    output_pending: bool
}

impl Keypad {
    pub fn new() -> Self { Self { key_map: HashMap::<u8, u8>::new(), pin_values: 0x0, output_pending: false } }
    pub fn key_to_pin(&mut self, key: u8, pin_values: u8) {
        self.key_map.insert(key, pin_values);
    }
}

impl KeyInput for Keypad {
    fn get_key(&mut self, key: u8) {
        if let Some(new_pin_values) = self.key_map.get(&key) {
            self.pin_values = *new_pin_values;
            self.output_pending = true;
        }
    }
}

impl Component for Keypad {
    fn init(&mut self) { }
    fn step(&mut self) -> u32 { 0 }

    fn receive_input(&mut self, gpio_idx: usize, pin_values: usize) { }
    fn output_pending(&self) -> bool { self.output_pending }
    fn clear_output_pending(&mut self) { self.output_pending = false; }
    fn get_gpio_values(&self, gpio_idx: usize) -> Result<usize, BusError> { Ok(self.pin_values as usize) }

    fn as_key_input(&self) -> Option<&dyn KeyInput> { Some(self) }
    fn as_key_input_mut(&mut self) -> Option<&mut dyn KeyInput> { Some(self) }
}
