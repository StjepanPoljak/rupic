// Simulated time in picoseconds. A u64 lasts ~213 days of simulated time.
pub type Time = u64;

pub const PS: Time = 1;
pub const NS: Time = 1_000 * PS;
pub const US: Time = 1_000 * NS;
pub const MS: Time = 1_000 * US;
pub const S: Time = 1_000 * MS;
