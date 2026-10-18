use crate::common::mcu::{ MCU };
use crate::{ Display, KeyInput };
use crate::common::bus::{ BusError };
use crate::common::time::{ Time, Ctx };

pub trait Component {
    fn init(&mut self);

    // Time between two steps, or None for a passive component, which only
    // runs when it receives input or when a wakeup it asked for is due.
    fn clock_period(&self) -> Option<Time> { None }
    // Runs one step of a clocked component; returns the clock periods used.
    fn step(&mut self, _ctx: &mut Ctx) -> u32 { 1 }
    // Called when a wakeup asked for with Ctx::wake_after() is due.
    fn wake(&mut self, _ctx: &mut Ctx) { }

    fn receive_input(&mut self, gpio_idx: usize, pin_values: usize) {}
    fn output_pending(&self, gpio_idx: usize) -> bool { false }
    fn clear_output_pending(&mut self) { }
    fn get_gpio_values(&self, gpio_idx: usize) -> Result<usize, BusError> { Err(BusError::NotImplemented) }

    fn as_mcu(&self) -> Option<&dyn MCU> { None }
    fn as_mcu_mut(&mut self) -> Option<&mut dyn MCU> { None }

    fn as_display(&self) -> Option<&dyn Display> { None }
    fn as_display_mut(&mut self) -> Option<&mut dyn Display> { None }

    fn as_key_input(&self) -> Option<&dyn KeyInput> { None }
    fn as_key_input_mut(&mut self) -> Option<&mut dyn KeyInput> { None }
}
