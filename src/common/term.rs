use std::io::{ stdin, stdout, Read, Write };
use termion::raw::IntoRawMode;
use termion::{ cursor, clear, terminal_size };
use std::sync::mpsc::Receiver;
use std::sync::atomic::{ AtomicU32, Ordering };
use std::cell::RefCell;

// Terminal size as (width << 16 | height), refreshed by present().
static TERM_SIZE: AtomicU32 = AtomicU32::new(0);

thread_local! {
    // Output of draw() since the last present().
    static FRAME: RefCell<Vec<u8>> = RefCell::new(Vec::new());
}

#[derive(Eq, PartialEq, Debug, Copy, Clone)]
pub enum KeyEvent {
    Key(u8),
    Escape
}

pub fn clear() {
    stdout().write_all(format!("{}", cursor::Goto(1, 1)).as_bytes()).unwrap();
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

pub fn term_size() -> (u16, u16) {
    let size = TERM_SIZE.load(Ordering::Relaxed);
    ((size >> 16) as u16, size as u16)
}

fn update_term_size() {
    if let Ok((width, height)) = terminal_size() {
        TERM_SIZE.store((width as u32) << 16 | height as u32, Ordering::Relaxed);
    }
}

// Writes out everything drawn since the last call, once per frame.
pub fn present() {
    FRAME.with_borrow_mut(|frame| {
        if !frame.is_empty() {
            let mut out = stdout().lock();
            out.write_all(frame).unwrap();
            out.flush().unwrap();
            frame.clear();
        }
    });
    update_term_size();
}

fn draw_char(x: u16, y: u16, c: char) {
    FRAME.with_borrow_mut(|frame| {
        write!(frame, "{}{}", cursor::Goto(x + 1, y + 1), c).unwrap();
    });
}

pub fn draw(x: u16, y: u16, state: bool) {
    draw_char(x, y, if state { '\u{0020}' } else { '\u{2588}' })
}

pub fn init_term(main_tid: libc::pthread_t) -> Receiver<KeyEvent> {
    clear();
    cursor_hide();
    update_term_size();

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
        println!("");
        clear();
        unsafe { libc::pthread_kill(main_tid, libc::SIGINT); }
    });
    rx
}
