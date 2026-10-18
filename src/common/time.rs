// Simulated time in picoseconds. A u64 lasts ~213 days of simulated time.
pub type Time = u64;

pub const PS: Time = 1;
pub const NS: Time = 1_000 * PS;
pub const US: Time = 1_000 * NS;
pub const MS: Time = 1_000 * US;
pub const S: Time = 1_000 * MS;

// Things a component can ask of the board while it handles an event.
pub struct Ctx {
    now: Time,
    wakeups: Vec<Time>
}

impl Ctx {
    pub fn new(now: Time) -> Self {
        Self { now, wakeups: vec![] }
    }

    pub fn now(&self) -> Time {
        self.now
    }

    // Asks for a call to Component::wake() delay from now.
    pub fn wake_after(&mut self, delay: Time) {
        self.wakeups.push(self.now + delay);
    }

    pub fn take_wakeups(&mut self) -> Vec<Time> {
        std::mem::take(&mut self.wakeups)
    }
}
