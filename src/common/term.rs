use std::io::{ stdin, stdout, Read, Write };
use termion::raw::IntoRawMode;
use termion::{ cursor, clear, terminal_size };
use std::sync::mpsc::Receiver;

#[derive(Eq, PartialEq, Debug, Copy, Clone)]
pub enum KeyEvent {
    Key(u8),
    Escape
}

pub fn clear() {
    stdout().write_all(format!("{}", clear::All).as_bytes());
    stdout().flush().unwrap();
}

pub fn cursor_hide() {
    stdout().write_all(format!("{}", cursor::Hide).as_bytes());
    stdout().flush().unwrap();
}

pub fn cursor_show() {
    stdout().write_all(format!("{}", cursor::Show).as_bytes());
    stdout().flush().unwrap();
}

fn draw_char(x: u16, y: u16, c: char) {
    stdout()
        .write_all(format!("{}{}", cursor::Goto(x + 1, y + 1), c).as_bytes())
        .unwrap();
    stdout().flush().unwrap();
}

pub fn draw(x: u16, y: u16, state: bool) {
    draw_char(x, y, if state { '\u{0020}' } else { '\u{2588}' })
}

pub fn init_term(main_tid: libc::pthread_t) -> Receiver<KeyEvent> {
    clear();
    cursor_hide();

    let (tx, rx) = std::sync::mpsc::channel();

    std::thread::spawn(move || {
        let mut is_escape = false;
        let raw = stdout().into_raw_mode().unwrap();
        const QUIT_BYTE : u8 = 'x' as u8;

        for byte in stdin().bytes() {
            let b = byte.unwrap();

            if !is_escape && b == 0x01 {
                is_escape = true;
                continue;
            } else if is_escape {
                is_escape = false;
                match b {
                    QUIT_BYTE => { break; },
                    0x01 => (),
                    _ => { continue; }
                };
            } else {
                tx.send(KeyEvent::Key(b)).unwrap();
            }
        }
        cursor_show();
        drop(raw);
        unsafe { libc::pthread_kill(main_tid, libc::SIGINT); }
    });
    rx
}
