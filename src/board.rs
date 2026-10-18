use std::io::{ self };
use std::fmt;
use std::sync::atomic::Ordering;
use std::sync::mpsc::Receiver;
use std::collections::{ VecDeque, BinaryHeap };
use std::cmp::Reverse;
use std::time::{ Duration, Instant };

use crate::SIGINT;
use crate::common::component::Component;
use crate::common::term::{ init_term, draw, present, KeyEvent };
use crate::common::time::{ Time, Ctx, NS };

// Board::run checks the throttle and the frame timer every SYNC_STEPS steps.
const SYNC_STEPS: u32 = 1_000;
const FRAME_TIME: Duration = Duration::from_millis(16);
// When the host falls further behind real time than this, stop catching up.
const MAX_LAG: Duration = Duration::from_millis(100);

#[derive(Debug)]
pub enum BoardError {
    ComponentNotFound
}

impl fmt::Display for BoardError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::ComponentNotFound => write!(f, "Component not found.")
        }
    }
}

impl std::error::Error for BoardError {}

pub struct Breakpoint {
    addr: usize
}

// Routes a source component's gpio group (src) to a gpio group (dst) of the
// subscribed component, e.g. keypad group 0 -> PIC PORTB (group 1).
pub struct GpioRoute {
    src: usize,
    dst: usize
}

pub struct Subscriber {
    bc_idx: usize,
    routes: Vec<GpioRoute>
}

pub struct BoardComponent {
    component: Box<dyn Component>,
    breakpoints: Vec<Breakpoint>,
    subscribers: Vec<Subscriber>
}

#[derive(PartialEq, Eq, PartialOrd, Ord, Debug)]
enum EventKind {
    // A clocked component's next step is due.
    Clock(usize),
    // A wakeup asked for by a component is due.
    Wake(usize)
}

pub struct Board {
    components: Vec<BoardComponent>,
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
        Self { components: vec![], needs_term: false, term_rx: None, events: VecDeque::new(), headless: false,
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

    pub fn add_component(&mut self, component: Box<dyn Component>) -> usize {
        if let Some(_) = component.as_display() {
            self.needs_term = true;
        }
        self.components.push(BoardComponent { component, breakpoints: vec![], subscribers: vec![] });
        self.components.len() - 1
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

    // bc_idx receives the gpio groups of out_bc_idx, as (src, dst) pairs.
    pub fn subscribe(&mut self, bc_idx: usize, out_bc_idx: usize, routes: Vec<(usize, usize)>) {
        let out_bc = &mut self.components[out_bc_idx];
        let routes = routes.into_iter().map(|(src, dst)| GpioRoute { src, dst }).collect();
        out_bc.subscribers.push(Subscriber{ bc_idx, routes });
    }

    pub fn get_component(&mut self, bc_idx: usize) -> io::Result<&mut dyn Component> {
        let bc = self.components.get_mut(bc_idx).ok_or(io::Error::other(format!("Component with index {bc_idx} does not exist.")))?;
        Ok(bc.component.as_mut())
    }

    fn schedule(&mut self, time: Time, kind: EventKind) {
        self.queue.push(Reverse((time, self.seq, kind)));
        self.seq += 1;
    }

    // Runs f on a component, then takes care of what it asked for: wakeups,
    // and outputs to deliver to the subscribers.
    fn dispatch<F>(&mut self, bc_idx: usize, f: F)
    where F: FnOnce(&mut dyn Component, &mut Ctx) {
        let mut ctx = Ctx::new(self.now);
        f(self.components[bc_idx].component.as_mut(), &mut ctx);
        for time in ctx.take_wakeups() {
            self.schedule(time, EventKind::Wake(bc_idx));
        }
        self.deliver(bc_idx);
    }

    // Delivers the pending outputs of bc_idx, then those of the components
    // that received them, until nothing is pending.
    fn deliver(&mut self, bc_idx: usize) {
        let mut sources = vec![bc_idx];
        while let Some(bc_idx) = sources.pop() {
            let bc = &self.components[bc_idx];
            let deliveries: Vec<_> = bc.subscribers
                .iter().flat_map(|sub| sub.routes.iter()
                    .filter(|route| bc.component.output_pending(route.src))
                    .map(|route| {
                        let values = bc.component
                            .get_gpio_values(route.src)
                            .expect("Gpio group not found.");
                        (sub.bc_idx, route.dst, values)})).collect();
            self.components[bc_idx].component.clear_output_pending();
            self.redraw(bc_idx);

            for (target_idx, gpio_idx, values) in deliveries {
                self.components[target_idx].component.receive_input(gpio_idx, values);
                sources.push(target_idx);
            }
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
                }
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
