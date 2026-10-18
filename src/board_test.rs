// Net and scheduler tests with scripted components.
use std::cell::RefCell;
use std::rc::Rc;
use crate::Board;
use crate::common::component::{ Component, Ctx };
use crate::common::net::Level::{ self, Low, High, Z, X };
use crate::common::time::{ Time, NS };

const PERIOD: Time = 10 * NS;

// Clocked every PERIOD; step i drives drives[i], as (pin, level, delay).
struct Script {
    drives: Vec<Vec<(usize, Level, Time)>>,
    idx: usize
}

impl Component for Script {
    fn init(&mut self) { }
    fn pins(&self) -> &[&'static str] { &["P0", "P1"] }
    fn clock_period(&self) -> Option<Time> { Some(PERIOD) }

    fn step(&mut self, ctx: &mut Ctx) -> u32 {
        for (pin, level, delay) in self.drives.get(self.idx).cloned().unwrap_or_default() {
            ctx.drive_after(pin, level, delay);
        }
        self.idx += 1;
        1
    }
}

// Records the input changes it sees, as (time, pin, level).
struct Probe {
    seen: Rc<RefCell<Vec<(Time, usize, Level)>>>
}

impl Component for Probe {
    fn init(&mut self) { }
    fn pins(&self) -> &[&'static str] { &["IN0", "IN1"] }

    fn input_changed(&mut self, pin: usize, level: Level, ctx: &mut Ctx) {
        self.seen.borrow_mut().push((ctx.now(), pin, level));
    }
}

type Seen = Rc<RefCell<Vec<(Time, usize, Level)>>>;

// Components A and B run the scripts; PROBE watches.
fn board(a: Vec<Vec<(usize, Level, Time)>>, b: Vec<Vec<(usize, Level, Time)>>) -> (Board, Seen) {
    let seen = Rc::new(RefCell::new(vec![]));
    let mut board = Board::new_headless();
    board.add_component("A", Box::new(Script { drives: a, idx: 0 }));
    board.add_component("B", Box::new(Script { drives: b, idx: 0 }));
    board.add_component("PROBE", Box::new(Probe { seen: seen.clone() }));
    (board, seen)
}

fn run_until(board: &mut Board, time: Time) {
    while board.now() < time {
        board.step();
    }
}

#[test]
fn net_follows_tri_state_driver() {
    let (mut b, seen) = board(vec![vec![(0, High, 0)], vec![(0, Z, 0)], vec![], vec![(0, Low, 0)]], vec![]);
    b.net("N", &["A.P0", "B.P0", "PROBE.IN0"]);
    b.init();
    run_until(&mut b, 5 * PERIOD);
    assert_eq!(*seen.borrow(), vec![(0, 0, High), (PERIOD, 0, Z), (3 * PERIOD, 0, Low)]);
}

#[test]
fn two_drivers_disagreeing_give_x() {
    let (mut b, seen) = board(vec![vec![(0, High, 0)]], vec![vec![], vec![(0, Low, 0)], vec![(0, Z, 0)]]);
    b.net("N", &["A.P0", "B.P0", "PROBE.IN0"]);
    b.init();
    run_until(&mut b, 4 * PERIOD);
    assert_eq!(*seen.borrow(), vec![(0, 0, High), (PERIOD, 0, X), (2 * PERIOD, 0, High)]);
    assert_eq!(b.contentions(), 1);
}

#[test]
fn delayed_drive_reaches_net_later() {
    let (mut b, seen) = board(vec![vec![(0, High, 55 * NS)]], vec![]);
    b.net("N", &["A.P0", "PROBE.IN0"]);
    b.init();
    run_until(&mut b, 50 * NS);
    assert_eq!(b.net_level("N"), Some(Z));
    run_until(&mut b, 100 * NS);
    assert_eq!(b.net_level("N"), Some(High));
    assert_eq!(*seen.borrow(), vec![(55 * NS, 0, High)]);
}

#[test]
fn agreeing_drivers_change_nothing() {
    let (mut b, seen) = board(vec![vec![(0, High, 0)], vec![(0, High, 0)]], vec![vec![], vec![], vec![(0, High, 0)]]);
    b.net("N", &["A.P0", "B.P0", "PROBE.IN0"]);
    b.init();
    run_until(&mut b, 4 * PERIOD);
    assert_eq!(*seen.borrow(), vec![(0, 0, High)]);
    assert_eq!(b.contentions(), 0);
}

#[test]
fn bus_connects_pins_by_index() {
    let (mut b, seen) = board(vec![vec![(1, High, 0)]], vec![]);
    b.bus("D", 2, &["A.P", "PROBE.IN"]);
    b.init();
    run_until(&mut b, PERIOD);
    assert_eq!(b.net_level("D0"), Some(Z));
    assert_eq!(b.net_level("D1"), Some(High));
    assert_eq!(*seen.borrow(), vec![(0, 1, High)]);
}

#[test]
#[should_panic(expected = "already connected")]
fn pin_on_two_nets_panics() {
    let (mut b, _) = board(vec![], vec![]);
    b.net("N1", &["A.P0", "PROBE.IN0"]);
    b.net("N2", &["A.P0", "B.P0"]);
}

#[test]
#[should_panic(expected = "has no pin")]
fn unknown_pin_panics() {
    let (mut b, _) = board(vec![], vec![]);
    b.net("N", &["A.P7"]);
}
