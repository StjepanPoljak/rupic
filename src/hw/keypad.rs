use std::collections::HashMap;
use crate::common::component::{ Component, Ctx };
use crate::KeyInput;
use crate::common::net::Level;
use crate::common::time::Time;

const PINS: [&str; 5] = ["D0", "D1", "D2", "D3", "DA"];
const DA: usize = 4;

// Models a keypad encoder (e.g. 74C922): a pressed key puts its code on
// D0..D3 and sets DA ("data available"). A terminal only reports presses,
// so a key is released hold_time after its press.
pub struct Keypad {
    key_map: HashMap<u8, u8>,
    hold_time: Time,
    release_at: Option<Time>
}

impl Keypad {
    pub fn new(hold_time: Time) -> Self {
        Self { key_map: HashMap::<u8, u8>::new(),
               hold_time,
               release_at: None }
    }

    pub fn key_to_pin(&mut self, key: u8, code: u8) {
        self.key_map.insert(key, code);
    }

    fn drive(&self, code: u8, available: bool, ctx: &mut Ctx) {
        for bit in 0..DA {
            ctx.drive(bit, Level::from_bit(code & (1 << bit) != 0));
        }
        ctx.drive(DA, Level::from_bit(available));
    }
}

impl KeyInput for Keypad {
    fn get_key(&mut self, key: u8, ctx: &mut Ctx) {
        if let Some(code) = self.key_map.get(&key) {
            self.drive(*code, true, ctx);
            self.release_at = Some(ctx.now() + self.hold_time);
            ctx.wake_after(self.hold_time);
        }
    }
}

impl Component for Keypad {
    fn init(&mut self) { }

    fn pins(&self) -> &[&'static str] {
        &PINS
    }

    // A new press moves release_at: the wakeups of earlier presses are stale.
    fn wake(&mut self, ctx: &mut Ctx) {
        if self.release_at.is_some_and(|release_at| ctx.now() >= release_at) {
            self.release_at = None;
            self.drive(0, false, ctx);
        }
    }

    fn as_key_input(&self) -> Option<&dyn KeyInput> { Some(self) }
    fn as_key_input_mut(&mut self) -> Option<&mut dyn KeyInput> { Some(self) }
}
