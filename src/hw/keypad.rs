use std::collections::HashMap;
use crate::common::component::Component;
use crate::KeyInput;
use crate::common::bus::{ BusError };
use crate::common::time::{ Time, Ctx };

// Models a keypad encoder (e.g. 74C922): a pressed key puts its code on the
// output pins together with the strobe ("data available") bit. A terminal
// only reports presses, so a key is released hold_time after its press.
pub struct Keypad {
    key_map: HashMap<u8, u8>,
    pin_values: u8,
    strobe_mask: u8,
    hold_time: Time,
    release_at: Option<Time>,
    output_pending: bool
}

impl Keypad {
    pub fn new(strobe_mask: u8, hold_time: Time) -> Self {
        Self { key_map: HashMap::<u8, u8>::new(),
               pin_values: 0x0,
               strobe_mask,
               hold_time,
               release_at: None,
               output_pending: false }
    }

    pub fn key_to_pin(&mut self, key: u8, pin_values: u8) {
        self.key_map.insert(key, pin_values);
    }
}

impl KeyInput for Keypad {
    fn get_key(&mut self, key: u8, ctx: &mut Ctx) {
        if let Some(new_pin_values) = self.key_map.get(&key) {
            self.pin_values = *new_pin_values | self.strobe_mask;
            self.release_at = Some(ctx.now() + self.hold_time);
            ctx.wake_after(self.hold_time);
            self.output_pending = true;
        }
    }
}

impl Component for Keypad {
    fn init(&mut self) { }

    // A new press moves release_at: the wakeups of earlier presses are stale.
    fn wake(&mut self, ctx: &mut Ctx) {
        if self.release_at.is_some_and(|release_at| ctx.now() >= release_at) {
            self.release_at = None;
            self.pin_values = 0x0;
            self.output_pending = true;
        }
    }

    fn receive_input(&mut self, _gpio_idx: usize, _pin_values: usize) { }
    fn output_pending(&self, _gpio_idx: usize) -> bool { self.output_pending }
    fn clear_output_pending(&mut self) { self.output_pending = false; }
    fn get_gpio_values(&self, _gpio_idx: usize) -> Result<usize, BusError> { Ok(self.pin_values as usize) }

    fn as_key_input(&self) -> Option<&dyn KeyInput> { Some(self) }
    fn as_key_input_mut(&mut self) -> Option<&mut dyn KeyInput> { Some(self) }
}
