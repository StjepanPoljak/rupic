// Value of a net, or of a pin driving it. Z: nobody drives the net.
// X: drivers disagree (bus contention); never driven by a pin.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Debug)]
pub enum Level {
    Low,
    High,
    Z,
    X
}

impl Level {
    pub fn from_bit(bit: bool) -> Self {
        if bit { Level::High } else { Level::Low }
    }

    // Floating and contended inputs read as 0.
    pub fn is_high(self) -> bool {
        self == Level::High
    }

    // Value of a net given the levels its pins drive.
    pub fn resolve(drives: impl Iterator<Item = Level>) -> Self {
        drives.fold(Level::Z, |net, drive| match (net, drive) {
            (net, Level::Z) => net,
            (Level::Z, drive) => drive,
            (net, drive) if net == drive => net,
            _ => Level::X
        })
    }
}
