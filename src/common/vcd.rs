// Value change dump (IEEE 1364 VCD) of chosen nets and buses, viewable in
// GTKWave. Time is written in picoseconds, the board's time unit.
use std::io::{ self, Write };

use crate::common::net::Level;
use crate::common::time::Time;

// A traced net (one bit) or bus (its nets, LSB first).
struct Signal {
    id: String,
    nets: Vec<usize>,
    // Value last written, as VCD bit characters, MSB first.
    last: String
}

pub struct Vcd {
    out: Box<dyn Write>,
    signals: Vec<Signal>,
    // For each net, the signals it is part of.
    net_signals: Vec<Vec<usize>>,
    dirty: Vec<usize>,
    // Time of the last "#time" line, if any.
    time: Option<Time>
}

fn level_char(level: Level) -> char {
    match level {
        Level::Low => '0',
        Level::High => '1',
        Level::Z => 'z',
        Level::X => 'x'
    }
}

// Short identifier codes made of the printable characters '!'..='~'.
fn id_code(mut n: usize) -> String {
    let mut id = String::new();
    loop {
        id.push((b'!' + (n % 94) as u8) as char);
        n /= 94;
        if n == 0 {
            return id;
        }
        n -= 1;
    }
}

impl Vcd {
    // Writes the header. signals are (name, nets LSB first); net_count is
    // the number of nets on the board.
    pub fn new(mut out: Box<dyn Write>, signals: &[(String, Vec<usize>)], net_count: usize) -> io::Result<Self> {
        writeln!(out, "$version pic16f876 board $end")?;
        writeln!(out, "$timescale 1ps $end")?;
        writeln!(out, "$scope module board $end")?;
        let mut net_signals = vec![vec![]; net_count];
        let mut traced = vec![];
        for (sig_idx, (name, nets)) in signals.iter().enumerate() {
            let id = id_code(sig_idx);
            if nets.len() == 1 {
                writeln!(out, "$var wire 1 {id} {name} $end")?;
            } else {
                writeln!(out, "$var wire {} {id} {name} [{}:0] $end", nets.len(), nets.len() - 1)?;
            }
            for net_idx in nets {
                net_signals[*net_idx].push(sig_idx);
            }
            traced.push(Signal { id, nets: nets.clone(), last: String::new() });
        }
        writeln!(out, "$upscope $end")?;
        writeln!(out, "$enddefinitions $end")?;
        let dirty = (0..traced.len()).collect();
        Ok(Self { out, signals: traced, net_signals, dirty, time: None })
    }

    pub fn net_changed(&mut self, net_idx: usize) {
        self.dirty.extend(&self.net_signals[net_idx]);
    }

    // Writes the signals that changed since the last call, as of time now.
    // A net that changed and changed back within the same time is skipped.
    pub fn dump(&mut self, now: Time, levels: impl Fn(usize) -> Level) -> io::Result<()> {
        let mut dirty = std::mem::take(&mut self.dirty);
        dirty.sort_unstable();
        dirty.dedup();
        for sig_idx in dirty {
            let signal = &mut self.signals[sig_idx];
            let value: String = signal.nets.iter().rev().map(|net_idx| level_char(levels(*net_idx))).collect();
            if value == signal.last {
                continue;
            }
            if self.time != Some(now) {
                writeln!(self.out, "#{now}")?;
                self.time = Some(now);
            }
            if signal.nets.len() == 1 {
                writeln!(self.out, "{value}{}", signal.id)?;
            } else {
                writeln!(self.out, "b{value} {}", signal.id)?;
            }
            signal.last = value;
        }
        Ok(())
    }

    pub fn flush(&mut self) -> io::Result<()> {
        self.out.flush()
    }
}
