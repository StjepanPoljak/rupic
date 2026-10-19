use crate::common::component::{ Component, Ctx };
use crate::common::net::Level;

// A power rail (VDD, GND) as a constant driver on its net.
pub struct Rail {
    level: Level
}

impl Rail {
    pub fn new(level: Level) -> Self {
        Self { level }
    }
}

impl Component for Rail {
    fn init(&mut self) { }

    fn pins(&self) -> &[&'static str] {
        &["OUT"]
    }

    fn start(&mut self, ctx: &mut Ctx) {
        ctx.drive(0, self.level);
    }
}
