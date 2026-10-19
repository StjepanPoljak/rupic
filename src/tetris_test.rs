// Headless smoke tests running the real TETRIS.hex game image.
use std::collections::HashMap;
use std::path::Path;
use crate::{ Board, DEFAULT_BOARD };
use crate::board_test::SharedBuf;
use crate::common::mcu::MCU;
use crate::common::time::{ Time, US, MS, S };

const PLAYERXPOS: usize = 0x72;
const PLAYERYPOS: usize = 0x73;
const PLAYERROTATION: usize = 0x6f;

// Loads the Tetris schematic; returns the PIC's index.
fn load_tetris(board: &mut Board) -> usize {
    crate::kicad::load(board, Path::new(DEFAULT_BOARD)).unwrap();
    board.find_component("U1").unwrap()
}

struct Tetris {
    board: Board,
    pic_idx: usize
}

impl Tetris {
    fn new() -> Self {
        let mut board = Board::new_headless();
        let pic_idx = load_tetris(&mut board);
        board.init();

        Self { board, pic_idx }
    }

    fn run(&mut self, duration: Time) {
        let end = self.board.now() + duration;
        while self.board.now() < end {
            self.board.step();
        }
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

    fn run_until_pc(&mut self, pc: usize, timeout: Time) -> bool {
        let end = self.board.now() + timeout;
        while self.board.now() < end {
            self.board.step();
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
    assert!(t.run_until_pc(INTRO_WAIT, 7 * S), "title screen never waits for input");
    t.press(b' ');
    assert!(t.run_until_pc(LMAIN, 7 * S), "space did not start the game");
    t
}

#[test]
fn piece_falls_on_timer1() {
    let mut t = started_game();
    let y0 = t.mem(PLAYERYPOS);
    t.run(1400 * MS);
    let y1 = t.mem(PLAYERYPOS);
    assert!(y1 < y0, "piece did not fall: y {} -> {}", y0, y1);
}

#[test]
fn keys_move_piece_once_per_press() {
    let mut t = started_game();
    let mut check = |t: &mut Tetris, key: u8, dx: i32| {
        // land the press at varying points of the main loop / redraws
        t.run(US + (t.board.now() / US * 7919) % (66 * MS));
        let x0 = t.mem(PLAYERXPOS) as i32;
        t.press(key);
        t.run(530 * MS);
        let x1 = t.mem(PLAYERXPOS) as i32;
        assert_eq!(x1 - x0, dx, "key {:?}: x {} -> {}", key as char, x0, x1);
    };
    check(&mut t, b'a', -1);
    check(&mut t, b'a', -1);
    check(&mut t, b'd', 1);
    check(&mut t, b'd', 1);
    check(&mut t, b'd', 1);
    assert_eq!(t.board.contentions(), 0);
}

#[test]
fn rotate_and_drop() {
    let mut t = started_game();
    let r0 = t.mem(PLAYERROTATION);
    t.press(b' ');
    t.run(270 * MS);
    assert_ne!(t.mem(PLAYERROTATION), r0, "space did not rotate");

    let y0 = t.mem(PLAYERYPOS);
    t.press(b's');
    t.run(200 * MS);
    assert!(t.mem(PLAYERYPOS) < y0, "s did not move the piece down");
}

// Writes to the LCD found in a VCD of LCD_RS, LCD_RW, LCD_E and LCD_DB, as
// (time, RS, DB): the values when E falls with RW = 0.
fn lcd_writes(vcd: &str) -> Vec<(Time, char, u8)> {
    let mut names = HashMap::new();
    let mut values: HashMap<&str, &str> = HashMap::new();
    let mut writes = vec![];
    let mut time = 0;
    for line in vcd.lines() {
        let words: Vec<&str> = line.split_whitespace().collect();
        let (name, value) = match words[..] {
            ["$var", _, _, id, name, ..] => {
                names.insert(id, name);
                continue;
            },
            [word] if word.starts_with('#') => {
                time = word[1..].parse().unwrap();
                continue;
            },
            [value, id] if value.starts_with('b') => (names[id], &value[1..]),
            [word] if !word.starts_with('$') => (names[&word[1..]], &word[..1]),
            _ => continue
        };
        if name == "LCD_E" && values.get("LCD_E") == Some(&"1") && value == "0" && values["LCD_RW"] == "0" {
            writes.push((time, values["LCD_RS"].chars().next().unwrap(), u8::from_str_radix(values["LCD_DB"], 2).unwrap()));
        }
        values.insert(name, value);
    }
    writes
}

#[test]
fn vcd_shows_lcd_setup() {
    let mut board = Board::new_headless();
    load_tetris(&mut board);
    let buf = SharedBuf::default();
    board.trace(Box::new(buf.clone()), &["LCD_RS", "LCD_RW", "LCD_E", "LCD_DB"]).unwrap();
    board.init();
    // LCD_Setup starts after about 1.04 s and is done by 1.06 s.
    while board.now() < 1100 * MS {
        board.step();
    }
    board.stop_trace().unwrap();

    let writes = lcd_writes(&buf.text());
    let setup: Vec<(char, u8)> = writes.iter().take(7).map(|(_, rs, db)| (*rs, *db)).collect();
    assert_eq!(setup, vec![('0', 0x30), ('0', 0x30), ('0', 0x0c), ('0', 0x01), ('0', 0x06), ('0', 0x34), ('0', 0x36)],
               "first LCD writes: {:x?}", &writes[..writes.len().min(10)]);
    assert!(writes.len() > 7, "no LCD writes after the setup");
}

