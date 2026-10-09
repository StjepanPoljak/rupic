// Headless smoke tests running the real TETRIS.hex game image.
use crate::{ Board, KEYPAD_HOLD_STEPS };
use crate::common::byte_data::ByteData;
use crate::common::mcu::MCU;
use crate::hw::keypad::Keypad;
use crate::hw::st7920::ST7920;
use crate::pic::pic16f876::PIC16F876;

const PLAYERXPOS: usize = 0x72;
const PLAYERYPOS: usize = 0x73;
const PLAYERROTATION: usize = 0x6f;

struct Tetris {
    board: Board,
    pic_idx: usize,
    steps: u64
}

impl Tetris {
    fn new() -> Self {
        let mut board = Board::new_headless();

        let mut pic = PIC16F876::new();
        pic.load_rom(&ByteData::new_from_intel_hex("./TETRIS.hex").unwrap());
        let pic_idx = board.add_component(Box::new(pic));
        let display_idx = board.add_component(Box::new(ST7920::new()));

        let mut keypad = Keypad::new(0x10, KEYPAD_HOLD_STEPS);
        keypad.key_to_pin(b'a', 0x3);
        keypad.key_to_pin(b'd', 0x4);
        keypad.key_to_pin(b's', 0x2);
        keypad.key_to_pin(b' ', 0x5);
        let keypad_idx = board.add_component(Box::new(keypad));

        board.subscribe(display_idx, pic_idx, vec![(2, 2)]);
        board.subscribe(pic_idx, keypad_idx, vec![(0, 1)]);
        board.init();

        Self { board, pic_idx, steps: 0 }
    }

    fn run(&mut self, steps: u64) {
        for _ in 0..steps {
            self.board.step();
        }
        self.steps += steps;
    }

    fn mem(&mut self, addr: usize) -> u8 {
        let mcu = self.board.get_component(self.pic_idx).unwrap().as_mcu().unwrap();
        mcu.dump_mem(addr, 0).unwrap().values[0] as u8
    }

    fn pc(&mut self) -> usize {
        self.board.get_component(self.pic_idx).unwrap().as_mcu().unwrap().pc()
    }

    fn press(&mut self, key: u8) {
        self.board.push_key(key);
    }

    fn run_until_pc(&mut self, pc: usize, max_steps: u64) -> bool {
        for _ in 0..max_steps {
            self.board.step();
            self.steps += 1;
            if self.pc() == pc {
                return true;
            }
        }
        false
    }
}

const INTRO_WAIT: usize = 0x17;
const LMAIN: usize = 0x36;

fn started_game() -> Tetris {
    let mut t = Tetris::new();
    assert!(t.run_until_pc(INTRO_WAIT, 5_000_000), "title screen never waits for input");
    t.press(b' ');
    assert!(t.run_until_pc(LMAIN, 5_000_000), "space did not start the game");
    t
}

#[test]
fn piece_falls_on_timer1() {
    let mut t = started_game();
    let y0 = t.mem(PLAYERYPOS);
    t.run(1_000_000);
    let y1 = t.mem(PLAYERYPOS);
    assert!(y1 < y0, "piece did not fall: y {} -> {}", y0, y1);
}

#[test]
fn keys_move_piece_once_per_press() {
    let mut t = started_game();
    let mut check = |t: &mut Tetris, key: u8, dx: i32| {
        // land the press at varying points of the main loop / redraws
        t.run(1 + (t.steps * 7919) % 50_000);
        let x0 = t.mem(PLAYERXPOS) as i32;
        t.press(key);
        t.run(400_000);
        let x1 = t.mem(PLAYERXPOS) as i32;
        assert_eq!(x1 - x0, dx, "key {:?}: x {} -> {}", key as char, x0, x1);
    };
    check(&mut t, b'a', -1);
    check(&mut t, b'a', -1);
    check(&mut t, b'd', 1);
    check(&mut t, b'd', 1);
    check(&mut t, b'd', 1);
}

#[test]
fn rotate_and_drop() {
    let mut t = started_game();
    let r0 = t.mem(PLAYERROTATION);
    t.press(b' ');
    t.run(200_000);
    assert_ne!(t.mem(PLAYERROTATION), r0, "space did not rotate");

    let y0 = t.mem(PLAYERYPOS);
    t.press(b's');
    t.run(150_000);
    assert!(t.mem(PLAYERYPOS) < y0, "s did not move the piece down");
}
