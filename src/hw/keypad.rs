use std::collections::HashMap;
use crate::common::component::Component;
use crate::KeyInput;
use crate::common::bus::{ BusError };

// Models a keypad encoder (e.g. 74C922): a pressed key puts its code on the
// output pins together with the strobe ("data available") bit. A terminal
// only reports presses, so a key is released after hold_steps steps.
pub struct Keypad {
    key_map: HashMap<u8, u8>,
    pin_values: u8,
    strobe_mask: u8,
    hold_steps: u32,
    hold_remaining: u32,
    output_pending: bool
}

impl Keypad {
    pub fn new(strobe_mask: u8, hold_steps: u32) -> Self {
        Self { key_map: HashMap::<u8, u8>::new(),
               pin_values: 0x0,
               strobe_mask,
               hold_steps,
               hold_remaining: 0,
               output_pending: false }
    }

    pub fn key_to_pin(&mut self, key: u8, pin_values: u8) {
        self.key_map.insert(key, pin_values);
    }
}

impl KeyInput for Keypad {
    fn get_key(&mut self, key: u8) {
        if let Some(new_pin_values) = self.key_map.get(&key) {
            self.pin_values = *new_pin_values | self.strobe_mask;
            self.hold_remaining = self.hold_steps;
            self.output_pending = true;
        }
    }
}

impl Component for Keypad {
    fn init(&mut self) { }

    fn step(&mut self) -> u32 {
        if self.hold_remaining > 0 {
            self.hold_remaining -= 1;
            if self.hold_remaining == 0 {
                self.pin_values = 0x0;
                self.output_pending = true;
            }
        }
        0
    }

    fn receive_input(&mut self, _gpio_idx: usize, _pin_values: usize) { }
    fn output_pending(&self, _gpio_idx: usize) -> bool { self.output_pending }
    fn clear_output_pending(&mut self) { self.output_pending = false; }
    fn get_gpio_values(&self, _gpio_idx: usize) -> Result<usize, BusError> { Ok(self.pin_values as usize) }

    fn as_key_input(&self) -> Option<&dyn KeyInput> { Some(self) }
    fn as_key_input_mut(&mut self) -> Option<&mut dyn KeyInput> { Some(self) }
}
