use std::io::{ self, Write };
use std::fmt;
use std::sync::atomic::Ordering;
use std::sync::mpsc::Receiver;
use std::collections::{ VecDeque, BinaryHeap };
use std::cmp::Reverse;
use std::time::{ Duration, Instant };

use crate::SIGINT;
use crate::common::component::{ Component, Ctx };
use crate::common::net::Level;
use crate::common::term::{ init_term, draw, present, KeyEvent };
use crate::common::time::{ Time, NS };
use crate::common::vcd::Vcd;

// Board::run checks the throttle and the frame timer every SYNC_STEPS steps.
const SYNC_STEPS: u32 = 1_000;
const FRAME_TIME: Duration = Duration::from_millis(16);
// When the host falls further behind real time than this, stop catching up.
const MAX_LAG: Duration = Duration::from_millis(100);

#[derive(Debug)]
pub enum BoardError {
    ComponentNotFound,
    NoSuchSignal(String)
}

impl fmt::Display for BoardError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::ComponentNotFound => write!(f, "Component not found."),
            Self::NoSuchSignal(name) => write!(f, "No net or bus called {name}.")
        }
    }
}

impl std::error::Error for BoardError {}

pub struct Breakpoint {
    addr: usize
}

pub struct PinState {
    net: Option<usize>,
    // Level the pin drives onto its net now.
    drive: Level,
    // Level the pin drives once its scheduled drive events are done.
    scheduled: Level
}

pub struct BoardComponent {
    name: String,
    component: Box<dyn Component>,
    breakpoints: Vec<Breakpoint>,
    pins: Vec<PinState>
}

// A signal connecting pins, given as (bc_idx, pin) pairs.
pub struct Net {
    name: String,
    pins: Vec<(usize, usize)>,
    level: Level
}

#[derive(PartialEq, Eq, PartialOrd, Ord, Debug)]
enum EventKind {
    // A clocked component's next step is due.
    Clock(usize),
    // A wakeup asked for by a component is due.
    Wake(usize),
    // A pin starts driving a level: (bc_idx, pin, level).
    Drive(usize, usize, Level)
}

pub struct Board {
    components: Vec<BoardComponent>,
    nets: Vec<Net>,
    // Buses made with bus(): (name, nets LSB first).
    buses: Vec<(String, Vec<usize>)>,
    vcd: Option<Vcd>,
    contentions: u64,
    needs_term: bool,
    term_rx: Option<Receiver<KeyEvent>>,
    events: VecDeque<KeyEvent>,
    headless: bool,
    // Pending events, ordered by time, then by insertion order (seq).
    queue: BinaryHeap<Reverse<(Time, u64, EventKind)>>,
    seq: u64,
    now: Time,
    realtime: bool
}

impl Board {
    pub fn new() -> Self {
        Self { components: vec![], nets: vec![], buses: vec![], vcd: None, contentions: 0, needs_term: false, term_rx: None, events: VecDeque::new(), headless: false,
               queue: BinaryHeap::new(), seq: 0, now: 0, realtime: false }
    }

    // Makes run() sleep so that simulated time follows wall-clock time.
    // Unthrottled by default.
    pub fn set_realtime(&mut self, realtime: bool) {
        self.realtime = realtime;
    }

    // No terminal, no drawing; input is fed through push_key().
    #[allow(unused)]
    pub fn new_headless() -> Self {
        Self { headless: true, ..Self::new() }
    }

    #[allow(unused)]
    pub fn push_key(&mut self, key: u8) {
        self.events.push_back(KeyEvent::Key(key));
    }

    #[allow(unused)]
    pub fn now(&self) -> Time {
        self.now
    }

    // Times a net went into contention (Level::X).
    #[allow(unused)]
    pub fn contentions(&self) -> u64 {
        self.contentions
    }

    #[allow(unused)]
    pub fn net_level(&self, name: &str) -> Option<Level> {
        self.nets.iter().find(|net| net.name == name).map(|net| net.level)
    }

    // The name is used to refer to the component's pins, as "NAME.PIN".
    pub fn add_component(&mut self, name: &str, component: Box<dyn Component>) -> usize {
        if let Some(_) = component.as_display() {
            self.needs_term = true;
        }
        let pins = component.pins().iter()
            .map(|_| PinState { net: None, drive: Level::Z, scheduled: Level::Z }).collect();
        self.components.push(BoardComponent { name: name.to_string(), component, breakpoints: vec![], pins });
        self.components.len() - 1
    }

    // Resolves "NAME.PIN" to (bc_idx, pin).
    fn find_pin(&self, pin_name: &str) -> (usize, usize) {
        let (name, pin) = pin_name.split_once('.').unwrap_or_else(|| panic!("Pin {pin_name} is not NAME.PIN."));
        let bc_idx = self.components.iter().position(|bc| bc.name == name)
            .unwrap_or_else(|| panic!("No component called {name}."));
        let pin = self.components[bc_idx].component.pins().iter().position(|p| *p == pin)
            .unwrap_or_else(|| panic!("Component {name} has no pin {pin}."));
        (bc_idx, pin)
    }

    // Connects pins, given as "NAME.PIN", with a new net. Panics if a pin
    // doesn't exist or is already connected.
    pub fn net(&mut self, name: &str, pin_names: &[&str]) {
        let net_idx = self.nets.len();
        let mut pins = vec![];
        for pin_name in pin_names {
            let (bc_idx, pin) = self.find_pin(pin_name);
            let state = &mut self.components[bc_idx].pins[pin];
            assert!(state.net.is_none(), "Pin {pin_name} is already connected.");
            state.net = Some(net_idx);
            pins.push((bc_idx, pin));
        }
        self.nets.push(Net { name: name.to_string(), pins, level: Level::Z });
    }

    // Connects a bus of width nets, called name0, name1, ...; net i
    // connects pin i of every prefix, e.g. bus("D", 8, &["PIC.RC", "LCD.DB"])
    // connects PIC.RC0 to LCD.DB0 with net D0, and so on.
    pub fn bus(&mut self, name: &str, width: usize, prefixes: &[&str]) {
        self.buses.push((name.to_string(), (self.nets.len()..self.nets.len() + width).collect()));
        for i in 0..width {
            let pin_names: Vec<String> = prefixes.iter().map(|prefix| format!("{prefix}{i}")).collect();
            let pin_names: Vec<&str> = pin_names.iter().map(String::as_str).collect();
            self.net(&format!("{name}{i}"), &pin_names);
        }
    }

    // Starts a VCD trace of the named nets and buses to out; a bus is
    // traced as one vector. No names traces every bus, and every net that
    // isn't part of one.
    pub fn trace(&mut self, out: Box<dyn Write>, names: &[&str]) -> Result<(), Box<dyn std::error::Error>> {
        let signals: Vec<(String, Vec<usize>)> = if names.is_empty() {
            let in_bus: Vec<usize> = self.buses.iter().flat_map(|(_, nets)| nets.clone()).collect();
            let mut signals = self.buses.clone();
            signals.extend(self.nets.iter().enumerate().filter(|(net_idx, _)| !in_bus.contains(net_idx))
                           .map(|(net_idx, net)| (net.name.clone(), vec![net_idx])));
            signals
        } else {
            names.iter().map(|name| {
                if let Some(bus) = self.buses.iter().find(|(bus_name, _)| bus_name == name) {
                    Ok(bus.clone())
                } else if let Some(net_idx) = self.nets.iter().position(|net| net.name == *name) {
                    Ok((name.to_string(), vec![net_idx]))
                } else {
                    Err(BoardError::NoSuchSignal(name.to_string()))
                }
            }).collect::<Result<_, _>>()?
        };
        self.vcd = Some(Vcd::new(out, &signals, self.nets.len())?);
        Ok(())
    }

    // Writes out what's left of the trace and stops it.
    pub fn stop_trace(&mut self) -> io::Result<()> {
        match self.vcd.take() {
            Some(mut vcd) => {
                vcd.dump(self.now, |net_idx| self.nets[net_idx].level)?;
                vcd.flush()
            },
            None => Ok(())
        }
    }

    pub fn add_breakpoint(&mut self, bc_idx: usize, addr: usize) -> (usize, usize) {
        let bc = &mut self.components[bc_idx];
        bc.breakpoints.push(Breakpoint{ addr });
        (bc_idx, bc.breakpoints.len() - 1)
    }

    pub fn init(&mut self) {
        self.components.iter_mut().for_each(|bc| bc.component.init());
        for bc_idx in 0..self.components.len() {
            if self.components[bc_idx].component.clock_period().is_some() {
                self.schedule(self.now, EventKind::Clock(bc_idx));
            }
        }
        if self.needs_term && !self.headless {
            self.term_rx = Some(init_term(unsafe { libc::pthread_self() }));
        }
    }

    pub fn get_component(&mut self, bc_idx: usize) -> io::Result<&mut dyn Component> {
        let bc = self.components.get_mut(bc_idx).ok_or(io::Error::other(format!("Component with index {bc_idx} does not exist.")))?;
        Ok(bc.component.as_mut())
    }

    fn schedule(&mut self, time: Time, kind: EventKind) {
        self.queue.push(Reverse((time, self.seq, kind)));
        self.seq += 1;
    }

    // Runs f on a component, then schedules what it asked for: wakeups
    // and pin drives. A drive without delay still reaches the net at the
    // current time, within the same Board::step().
    fn dispatch<F>(&mut self, bc_idx: usize, f: F)
    where F: FnOnce(&mut dyn Component, &mut Ctx) {
        let mut ctx = Ctx::new(self.now);
        f(self.components[bc_idx].component.as_mut(), &mut ctx);
        for time in ctx.take_wakeups() {
            self.schedule(time, EventKind::Wake(bc_idx));
        }
        for (pin, level, time) in ctx.take_drives() {
            let state = &mut self.components[bc_idx].pins[pin];
            if state.net.is_none() || state.scheduled == level {
                continue;
            }
            state.scheduled = level;
            self.schedule(time, EventKind::Drive(bc_idx, pin, level));
        }
        self.redraw(bc_idx);
    }

    // Applies a pin's new drive level and, if the net's level changes,
    // tells every pin on the net.
    fn drive(&mut self, bc_idx: usize, pin: usize, level: Level) {
        let state = &mut self.components[bc_idx].pins[pin];
        state.drive = level;
        let net_idx = state.net.unwrap();

        let net = &self.nets[net_idx];
        let new_level = Level::resolve(net.pins.iter().map(|(bc_idx, pin)| self.components[*bc_idx].pins[*pin].drive));
        if new_level == net.level {
            return;
        }
        if new_level == Level::X {
            self.contentions += 1;
            eprintln!("{} ns: bus contention on net {}", self.now / NS, net.name);
        }
        self.nets[net_idx].level = new_level;
        if let Some(vcd) = &mut self.vcd {
            vcd.net_changed(net_idx);
        }

        for (bc_idx, pin) in self.nets[net_idx].pins.clone() {
            self.dispatch(bc_idx, |c, ctx| c.input_changed(pin, new_level, ctx));
        }
    }

    fn redraw(&mut self, bc_idx: usize) {
        if self.headless {
            return;
        }
        if let Some(display) = self.components[bc_idx].component.as_display_mut() {
            display.redraw(draw);
        }
    }

    // Handles every event due at the next event time, which becomes the
    // current time. Returns true when an MCU reached a breakpoint.
    pub fn step(&mut self) -> bool {
        if let Some(rx) = &self.term_rx {
            while let Ok(event) = rx.try_recv() {
                self.events.push_back(event);
            }
        }

        for bc_idx in 0..self.components.len() {
            if self.components[bc_idx].component.as_key_input().is_none() {
                continue;
            }
            if let Some(KeyEvent::Key(key)) = self.events.pop_front() {
                self.dispatch(bc_idx, |c, ctx| c.as_key_input_mut().unwrap().get_key(key, ctx));
            }
        }

        let Some(Reverse((time, _, _))) = self.queue.peek() else {
            return false;
        };
        self.now = *time;

        let mut got_bp = false;
        while let Some(Reverse((time, _, _))) = self.queue.peek() {
            if *time != self.now {
                break;
            }
            let Reverse((_, _, kind)) = self.queue.pop().unwrap();
            match kind {
                EventKind::Clock(bc_idx) => {
                    let bc = &self.components[bc_idx];
                    if let Some(mcu) = bc.component.as_mcu() {
                        let mcu_pc = mcu.pc();
                        if bc.breakpoints.iter().any(|bp| bp.addr == mcu_pc) {
                            got_bp = true;
                        }
                    }
                    let mut cycles = 0;
                    self.dispatch(bc_idx, |c, ctx| cycles = c.step(ctx));
                    let period = self.components[bc_idx].component.clock_period().unwrap();
                    self.schedule(self.now + cycles.max(1) as Time * period, EventKind::Clock(bc_idx));
                },
                EventKind::Wake(bc_idx) => {
                    self.dispatch(bc_idx, |c, ctx| c.wake(ctx));
                },
                EventKind::Drive(bc_idx, pin, level) => {
                    self.drive(bc_idx, pin, level);
                }
            }
        }

        if let Some(vcd) = &mut self.vcd {
            if let Err(err) = vcd.dump(self.now, |net_idx| self.nets[net_idx].level) {
                eprintln!("VCD trace stopped: {err}");
                self.vcd = None;
            }
        }
        got_bp
    }

    pub fn run(&mut self) {
        let draws = self.needs_term && !self.headless;
        let mut last_frame = Instant::now();
        // Wall-clock time at which the simulated time was sync_now.
        let (mut sync_time, mut sync_now) = (Instant::now(), self.now);
        let mut steps: u32 = 0;

        loop {
            if SIGINT.load(Ordering::Relaxed) {
                break;
            }

            if self.step() {
                if draws {
                    present();
                }
                println!("Got breakpoint!");
                break;
            }

            steps = steps.wrapping_add(1);
            if steps % SYNC_STEPS != 0 {
                continue;
            }

            if self.realtime {
                let target = Duration::from_nanos((self.now - sync_now) / NS);
                let elapsed = sync_time.elapsed();
                if elapsed < target {
                    std::thread::sleep(target - elapsed);
                } else if elapsed - target > MAX_LAG {
                    (sync_time, sync_now) = (Instant::now(), self.now);
                }
            }

            if draws && last_frame.elapsed() >= FRAME_TIME {
                present();
                last_frame = Instant::now();
            }
        }
    }
}
