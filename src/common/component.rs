use crate::common::mcu::{ MCU };
use crate::{ Display, KeyInput };
use crate::common::net::Level;
use crate::common::time::Time;

// Things a component can ask of the board while it handles an event.
pub struct Ctx {
    now: Time,
    wakeups: Vec<Time>,
    drives: Vec<(usize, Level, Time)>
}

impl Ctx {
    pub fn new(now: Time) -> Self {
        Self { now, wakeups: vec![], drives: vec![] }
    }

    pub fn now(&self) -> Time {
        self.now
    }

    // Asks for a call to Component::wake() delay from now.
    pub fn wake_after(&mut self, delay: Time) {
        self.wakeups.push(self.now + delay);
    }

    // Drives a pin (an index into Component::pins()); Level::Z releases it.
    pub fn drive(&mut self, pin: usize, level: Level) {
        self.drive_after(pin, level, 0);
    }

    // Same, but the net sees the change delay from now.
    pub fn drive_after(&mut self, pin: usize, level: Level, delay: Time) {
        self.drives.push((pin, level, self.now + delay));
    }

    pub fn take_wakeups(&mut self) -> Vec<Time> {
        std::mem::take(&mut self.wakeups)
    }

    pub fn take_drives(&mut self) -> Vec<(usize, Level, Time)> {
        std::mem::take(&mut self.drives)
    }
}

pub trait Component {
    fn init(&mut self);

    // Pin names; a pin is identified by its index in this list.
    fn pins(&self) -> &[&'static str] { &[] }

    // Time between two steps, or None for a passive component, which only
    // runs when one of its inputs changes or a wakeup it asked for is due.
    fn clock_period(&self) -> Option<Time> { None }
    // Called once after init(), at the start of simulated time, e.g. to
    // drive the pins' first levels.
    fn start(&mut self, _ctx: &mut Ctx) { }
    // Runs one step of a clocked component; returns the clock periods used.
    fn step(&mut self, _ctx: &mut Ctx) -> u32 { 1 }
    // Called when a wakeup asked for with Ctx::wake_after() is due.
    fn wake(&mut self, _ctx: &mut Ctx) { }
    // Called when the net connected to a pin changes level, including
    // changes the component caused by driving the pin itself.
    fn input_changed(&mut self, _pin: usize, _level: Level, _ctx: &mut Ctx) { }

    fn as_mcu(&self) -> Option<&dyn MCU> { None }
    fn as_mcu_mut(&mut self) -> Option<&mut dyn MCU> { None }

    fn as_display(&self) -> Option<&dyn Display> { None }
    fn as_display_mut(&mut self) -> Option<&mut dyn Display> { None }

    fn as_key_input(&self) -> Option<&dyn KeyInput> { None }
    fn as_key_input_mut(&mut self) -> Option<&mut dyn KeyInput> { None }
}
