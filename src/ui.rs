//! gbcalc terminal UI.
//!
//! Dependency-free: raw mode via termios, drawing via ANSI escape sequences.
//! Layout follows the project spec -- display on top, functions in the
//! middle, number entry at the bottom.
//!
//! No colours or glyphs are hardcoded here; every one comes from `theme`.

use crate::calc::{Base, Calc, Konst, Op, Unary};
use crate::theme::{StyleId, StyleState, Theme};

use std::fmt::Write as _;
use std::io::Write as _;
use std::sync::atomic::{AtomicBool, Ordering};

const W_INNER: i32 = 72; // usable width inside the display frame
const W_FRAME: i32 = W_INNER + 2;
const H_CONTENT: i32 = 20; // fixed number of rendered lines
const MIN_COLS: i32 = W_FRAME;
const MIN_ROWS: i32 = H_CONTENT;

const NFUNC_ROWS: usize = 6;
const NROWS: usize = 12;
const MAX_COLS: usize = 6;

// ------------------------------------------------------------- actions

#[derive(Clone, Copy, PartialEq, Eq)]
enum Action {
    None,
    Digit(u32),
    Point,
    Ee,
    Sign,
    Backspace,
    Clear,
    AllClear,
    Op(Op),
    LParen,
    RParen,
    Equals,
    Unary(Unary),
    Const(Konst),
    Base(Base),
    Drg,
    DrgConv,
    Inv,
    Sto,
    Rcl,
    Madd,
    Mexc,
    Mclr,
    Ans,
}

struct Btn {
    label: &'static str,
    act: Action,
    inv: Action, // Action::None => INV does not change this key
    keys: &'static str,
    hexdigit_only: bool,
    cat: StyleId,
}

const NA: Action = Action::None;

// Functions (middle section).
const ROW_F0: [Btn; 6] = [
    Btn { label: "DRG", act: Action::Drg, inv: Action::DrgConv, keys: "d", hexdigit_only: false, cat: StyleId::KeyMode },
    Btn { label: "INV", act: Action::Inv, inv: NA, keys: "i", hexdigit_only: false, cat: StyleId::KeyMode },
    Btn { label: "sin", act: Action::Unary(Unary::Sin), inv: Action::Unary(Unary::Asin), keys: "s", hexdigit_only: false, cat: StyleId::KeySci },
    Btn { label: "cos", act: Action::Unary(Unary::Cos), inv: Action::Unary(Unary::Acos), keys: "c", hexdigit_only: false, cat: StyleId::KeySci },
    Btn { label: "tan", act: Action::Unary(Unary::Tan), inv: Action::Unary(Unary::Atan), keys: "t", hexdigit_only: false, cat: StyleId::KeySci },
    Btn { label: "x!", act: Action::Unary(Unary::Fact), inv: NA, keys: "!", hexdigit_only: false, cat: StyleId::KeySci },
];
const ROW_F1: [Btn; 6] = [
    Btn { label: "1/x", act: Action::Unary(Unary::Recip), inv: NA, keys: "v", hexdigit_only: false, cat: StyleId::KeySci },
    Btn { label: "x^2", act: Action::Unary(Unary::Sqr), inv: Action::Unary(Unary::Sqrt), keys: "", hexdigit_only: false, cat: StyleId::KeySci },
    Btn { label: "sqrt", act: Action::Unary(Unary::Sqrt), inv: Action::Unary(Unary::Sqr), keys: "r", hexdigit_only: false, cat: StyleId::KeySci },
    Btn { label: "x^y", act: Action::Op(Op::Pow), inv: Action::Op(Op::Root), keys: "^", hexdigit_only: false, cat: StyleId::KeyOperator },
    Btn { label: "ln", act: Action::Unary(Unary::Ln), inv: Action::Unary(Unary::Exp), keys: "l", hexdigit_only: false, cat: StyleId::KeySci },
    Btn { label: "log", act: Action::Unary(Unary::Log10), inv: Action::Unary(Unary::Exp10), keys: "g", hexdigit_only: false, cat: StyleId::KeySci },
];
const ROW_F2: [Btn; 6] = [
    Btn { label: "e^x", act: Action::Unary(Unary::Exp), inv: Action::Unary(Unary::Ln), keys: "", hexdigit_only: false, cat: StyleId::KeySci },
    Btn { label: "10^x", act: Action::Unary(Unary::Exp10), inv: Action::Unary(Unary::Log10), keys: "", hexdigit_only: false, cat: StyleId::KeySci },
    Btn { label: "EE", act: Action::Ee, inv: NA, keys: "e", hexdigit_only: false, cat: StyleId::KeySci },
    Btn { label: "pi", act: Action::Const(Konst::Pi), inv: NA, keys: "p", hexdigit_only: false, cat: StyleId::KeySci },
    Btn { label: "e", act: Action::Const(Konst::E), inv: NA, keys: "E", hexdigit_only: false, cat: StyleId::KeySci },
    Btn { label: "%", act: Action::Unary(Unary::Pct), inv: NA, keys: "%", hexdigit_only: false, cat: StyleId::KeySci },
];
const ROW_F3: [Btn; 6] = [
    Btn { label: "(", act: Action::LParen, inv: NA, keys: "(", hexdigit_only: false, cat: StyleId::KeyParen },
    Btn { label: ")", act: Action::RParen, inv: NA, keys: ")", hexdigit_only: false, cat: StyleId::KeyParen },
    Btn { label: "STO", act: Action::Sto, inv: NA, keys: "m", hexdigit_only: false, cat: StyleId::KeyMemory },
    Btn { label: "RCL", act: Action::Rcl, inv: NA, keys: "n", hexdigit_only: false, cat: StyleId::KeyMemory },
    Btn { label: "M+", act: Action::Madd, inv: NA, keys: "M", hexdigit_only: false, cat: StyleId::KeyMemory },
    Btn { label: "MX", act: Action::Mexc, inv: NA, keys: "X", hexdigit_only: false, cat: StyleId::KeyMemory },
];
const ROW_F4: [Btn; 6] = [
    Btn { label: "DEC", act: Action::Base(Base::Dec), inv: NA, keys: "D", hexdigit_only: false, cat: StyleId::KeyMode },
    Btn { label: "HEX", act: Action::Base(Base::Hex), inv: NA, keys: "H", hexdigit_only: false, cat: StyleId::KeyMode },
    Btn { label: "BIN", act: Action::Base(Base::Bin), inv: NA, keys: "B", hexdigit_only: false, cat: StyleId::KeyMode },
    Btn { label: "AND", act: Action::Op(Op::And), inv: NA, keys: "&", hexdigit_only: false, cat: StyleId::KeyBitwise },
    Btn { label: "OR", act: Action::Op(Op::Or), inv: NA, keys: "|", hexdigit_only: false, cat: StyleId::KeyBitwise },
    Btn { label: "XOR", act: Action::Op(Op::Xor), inv: NA, keys: "#", hexdigit_only: false, cat: StyleId::KeyBitwise },
];
const ROW_F5: [Btn; 6] = [
    Btn { label: "NOT", act: Action::Unary(Unary::Not), inv: NA, keys: "~", hexdigit_only: false, cat: StyleId::KeyBitwise },
    Btn { label: "<<", act: Action::Op(Op::Shl), inv: NA, keys: "<", hexdigit_only: false, cat: StyleId::KeyBitwise },
    Btn { label: ">>", act: Action::Op(Op::Shr), inv: NA, keys: ">", hexdigit_only: false, cat: StyleId::KeyBitwise },
    Btn { label: "MOD", act: Action::Op(Op::Mod), inv: NA, keys: "\\", hexdigit_only: false, cat: StyleId::KeyBitwise },
    Btn { label: "MC", act: Action::Mclr, inv: NA, keys: "K", hexdigit_only: false, cat: StyleId::KeyMemory },
    Btn { label: "ANS", act: Action::Ans, inv: NA, keys: "A", hexdigit_only: false, cat: StyleId::KeyMemory },
];

// Number entry (bottom section).
const ROW_N0: [Btn; 6] = [
    Btn { label: "A", act: Action::Digit(10), inv: NA, keys: "aA", hexdigit_only: true, cat: StyleId::KeyHexdigit },
    Btn { label: "B", act: Action::Digit(11), inv: NA, keys: "bB", hexdigit_only: true, cat: StyleId::KeyHexdigit },
    Btn { label: "C", act: Action::Digit(12), inv: NA, keys: "cC", hexdigit_only: true, cat: StyleId::KeyHexdigit },
    Btn { label: "D", act: Action::Digit(13), inv: NA, keys: "dD", hexdigit_only: true, cat: StyleId::KeyHexdigit },
    Btn { label: "E", act: Action::Digit(14), inv: NA, keys: "eE", hexdigit_only: true, cat: StyleId::KeyHexdigit },
    Btn { label: "F", act: Action::Digit(15), inv: NA, keys: "fF", hexdigit_only: true, cat: StyleId::KeyHexdigit },
];
const ROW_N1: [Btn; 4] = [
    Btn { label: "7", act: Action::Digit(7), inv: NA, keys: "7", hexdigit_only: false, cat: StyleId::KeyDigit },
    Btn { label: "8", act: Action::Digit(8), inv: NA, keys: "8", hexdigit_only: false, cat: StyleId::KeyDigit },
    Btn { label: "9", act: Action::Digit(9), inv: NA, keys: "9", hexdigit_only: false, cat: StyleId::KeyDigit },
    Btn { label: "/", act: Action::Op(Op::Div), inv: NA, keys: "/", hexdigit_only: false, cat: StyleId::KeyOperator },
];
const ROW_N2: [Btn; 4] = [
    Btn { label: "4", act: Action::Digit(4), inv: NA, keys: "4", hexdigit_only: false, cat: StyleId::KeyDigit },
    Btn { label: "5", act: Action::Digit(5), inv: NA, keys: "5", hexdigit_only: false, cat: StyleId::KeyDigit },
    Btn { label: "6", act: Action::Digit(6), inv: NA, keys: "6", hexdigit_only: false, cat: StyleId::KeyDigit },
    Btn { label: "*", act: Action::Op(Op::Mul), inv: NA, keys: "*", hexdigit_only: false, cat: StyleId::KeyOperator },
];
const ROW_N3: [Btn; 4] = [
    Btn { label: "1", act: Action::Digit(1), inv: NA, keys: "1", hexdigit_only: false, cat: StyleId::KeyDigit },
    Btn { label: "2", act: Action::Digit(2), inv: NA, keys: "2", hexdigit_only: false, cat: StyleId::KeyDigit },
    Btn { label: "3", act: Action::Digit(3), inv: NA, keys: "3", hexdigit_only: false, cat: StyleId::KeyDigit },
    Btn { label: "-", act: Action::Op(Op::Sub), inv: NA, keys: "-", hexdigit_only: false, cat: StyleId::KeyOperator },
];
const ROW_N4: [Btn; 4] = [
    Btn { label: "0", act: Action::Digit(0), inv: NA, keys: "0", hexdigit_only: false, cat: StyleId::KeyDigit },
    Btn { label: ".", act: Action::Point, inv: NA, keys: ".", hexdigit_only: false, cat: StyleId::KeyDigit },
    Btn { label: "+/-", act: Action::Sign, inv: NA, keys: "_", hexdigit_only: false, cat: StyleId::KeyDigit },
    Btn { label: "+", act: Action::Op(Op::Add), inv: NA, keys: "+", hexdigit_only: false, cat: StyleId::KeyOperator },
];
const ROW_N5: [Btn; 4] = [
    Btn { label: "C", act: Action::Clear, inv: NA, keys: "", hexdigit_only: false, cat: StyleId::KeyEdit },
    Btn { label: "AC", act: Action::AllClear, inv: NA, keys: "", hexdigit_only: false, cat: StyleId::KeyEdit },
    Btn { label: "DEL", act: Action::Backspace, inv: NA, keys: "", hexdigit_only: false, cat: StyleId::KeyEdit },
    Btn { label: "=", act: Action::Equals, inv: NA, keys: "=", hexdigit_only: false, cat: StyleId::KeyEquals },
];

fn rows() -> [&'static [Btn]; NROWS] {
    [
        &ROW_F0, &ROW_F1, &ROW_F2, &ROW_F3, &ROW_F4, &ROW_F5, &ROW_N0, &ROW_N1, &ROW_N2, &ROW_N3,
        &ROW_N4, &ROW_N5,
    ]
}

// ------------------------------------------------------------ terminal

static RESIZED: AtomicBool = AtomicBool::new(true);
static STOP: AtomicBool = AtomicBool::new(false);

extern "C" fn on_signal(sig: std::os::raw::c_int) {
    if sig == libc::SIGWINCH {
        RESIZED.store(true, Ordering::SeqCst);
    } else {
        STOP.store(true, Ordering::SeqCst);
    }
}

fn raw_write(s: &str) {
    unsafe {
        libc::write(libc::STDOUT_FILENO, s.as_ptr() as *const libc::c_void, s.len());
    }
}

struct RawTerm {
    saved: libc::termios,
}

impl RawTerm {
    fn setup() -> Result<RawTerm, i32> {
        let saved: libc::termios;
        unsafe {
            if libc::isatty(libc::STDIN_FILENO) == 0 || libc::isatty(libc::STDOUT_FILENO) == 0 {
                eprintln!("gbcalc: stdin/stdout must be a terminal");
                return Err(1);
            }

            let mut tio0: libc::termios = std::mem::zeroed();
            if libc::tcgetattr(libc::STDIN_FILENO, &mut tio0) != 0 {
                eprintln!("gbcalc: tcgetattr: {}", std::io::Error::last_os_error());
                return Err(1);
            }
            saved = tio0;

            let mut tio = saved;
            tio.c_iflag &=
                !(libc::IXON | libc::ICRNL | libc::INLCR | libc::IGNCR | libc::BRKINT | libc::ISTRIP);
            tio.c_lflag &= !(libc::ECHO | libc::ICANON | libc::IEXTEN | libc::ISIG);
            tio.c_oflag &= !libc::OPOST;
            tio.c_cc[libc::VMIN] = 1;
            tio.c_cc[libc::VTIME] = 0;
            if libc::tcsetattr(libc::STDIN_FILENO, libc::TCSAFLUSH, &tio) != 0 {
                eprintln!("gbcalc: tcsetattr: {}", std::io::Error::last_os_error());
                return Err(1);
            }

            let mut sa: libc::sigaction = std::mem::zeroed();
            sa.sa_sigaction = on_signal as *const () as usize;
            libc::sigemptyset(&mut sa.sa_mask);
            sa.sa_flags = 0;
            libc::sigaction(libc::SIGINT, &sa, std::ptr::null_mut());
            libc::sigaction(libc::SIGTERM, &sa, std::ptr::null_mut());
            libc::sigaction(libc::SIGHUP, &sa, std::ptr::null_mut());
            libc::sigaction(libc::SIGWINCH, &sa, std::ptr::null_mut());

            raw_write("\x1b[?1049h\x1b[?25l\x1b[?1000h\x1b[?1006h");
        }
        Ok(RawTerm { saved })
    }

    fn restore(&self) {
        // Disable mouse reporting, show cursor, leave the alternate screen.
        raw_write("\x1b[?1006l\x1b[?1000l\x1b[?25h\x1b[0m\x1b[?1049l");
        unsafe {
            libc::tcsetattr(libc::STDIN_FILENO, libc::TCSAFLUSH, &self.saved);
        }
    }
}

impl Drop for RawTerm {
    fn drop(&mut self) {
        self.restore();
    }
}

fn term_size() -> (i32, i32) {
    let mut ws: libc::winsize = unsafe { std::mem::zeroed() };
    let ok = unsafe { libc::ioctl(libc::STDOUT_FILENO, libc::TIOCGWINSZ, &mut ws) };
    if ok == 0 && ws.ws_col > 0 {
        (ws.ws_col as i32, ws.ws_row as i32)
    } else {
        (80, 24)
    }
}

// --------------------------------------------------------------- input

const KEY_UP: i32 = 0x100;
const KEY_DOWN: i32 = 0x101;
const KEY_LEFT: i32 = 0x102;
const KEY_RIGHT: i32 = 0x103;
const KEY_HOME: i32 = 0x104;
const KEY_END: i32 = 0x105;
const KEY_DELETE: i32 = 0x106;
const KEY_MOUSE: i32 = 0x107;
const KEY_NONE: i32 = -1;

struct Input {
    pending: Option<u8>,
    mx: i32,
    my: i32,
}

impl Input {
    fn new() -> Input {
        Input { pending: None, mx: 0, my: 0 }
    }

    /// Poll stdin, returning what `libc::poll` returned (so callers needing
    /// to distinguish EINTR can read `errno` immediately afterwards).
    fn poll_stdin(&self, ms: i32) -> i32 {
        if self.pending.is_some() {
            return 1;
        }
        let mut pfd = libc::pollfd {
            fd: libc::STDIN_FILENO,
            events: libc::POLLIN,
            revents: 0,
        };
        unsafe { libc::poll(&mut pfd, 1, ms) }
    }

    fn ready(&self, ms: i32) -> bool {
        self.poll_stdin(ms) > 0
    }

    fn read_byte(&mut self) -> i32 {
        if let Some(b) = self.pending.take() {
            return b as i32;
        }
        let mut ch: u8 = 0;
        let n = unsafe { libc::read(libc::STDIN_FILENO, &mut ch as *mut u8 as *mut libc::c_void, 1) };
        if n == 1 {
            ch as i32
        } else {
            KEY_NONE
        }
    }

    fn unread_byte(&mut self, ch: i32) {
        if ch != KEY_NONE {
            self.pending = Some(ch as u8);
        }
    }

    /// Decode "\x1b[<b;x;yM" / "...m" SGR mouse reports.
    fn read_mouse(&mut self) -> i32 {
        let mut buf = Vec::with_capacity(31);
        loop {
            if buf.len() >= 31 {
                return KEY_NONE;
            }
            if !self.ready(50) {
                return KEY_NONE;
            }
            let ch = self.read_byte();
            if ch == KEY_NONE {
                return KEY_NONE;
            }
            if ch == b'M' as i32 || ch == b'm' as i32 {
                if ch == b'm' as i32 {
                    return KEY_NONE; // ignore releases
                }
                let s = match std::str::from_utf8(&buf) {
                    Ok(s) => s,
                    Err(_) => return KEY_NONE,
                };
                let parts: Vec<&str> = s.split(';').collect();
                if parts.len() == 3
                    && let (Ok(b), Ok(x), Ok(y)) =
                        (parts[0].parse::<i32>(), parts[1].parse::<i32>(), parts[2].parse::<i32>())
                        && b & 0x43 == 0 {
                            // plain left press
                            self.mx = x;
                            self.my = y;
                            return KEY_MOUSE;
                        }
                return KEY_NONE;
            }
            buf.push(ch as u8);
        }
    }

    fn read_key(&mut self) -> i32 {
        let ch = self.read_byte();
        if ch != 0x1b {
            return ch;
        }
        if !self.ready(30) {
            return 0x1b; // bare Esc
        }
        let ch = self.read_byte();
        if ch != '[' as i32 && ch != 'O' as i32 {
            self.unread_byte(ch); // not a sequence: Esc, then that key
            return 0x1b;
        }
        if !self.ready(30) {
            return 0x1b;
        }
        let ch = self.read_byte();
        match ch as u8 as char {
            'A' => KEY_UP,
            'B' => KEY_DOWN,
            'C' => KEY_RIGHT,
            'D' => KEY_LEFT,
            'H' => KEY_HOME,
            'F' => KEY_END,
            '<' => self.read_mouse(),
            '3' => {
                if self.ready(30) {
                    let t = self.read_byte();
                    if t == '~' as i32 {
                        return KEY_DELETE;
                    }
                    self.unread_byte(t);
                }
                KEY_NONE
            }
            _ => {
                // Swallow the rest of any unrecognised CSI sequence.
                let mut ch = ch;
                while (b'0' as i32..=b'?' as i32).contains(&ch) && self.ready(10) {
                    ch = self.read_byte();
                }
                KEY_NONE
            }
        }
    }
}

// ------------------------------------------------------------- geometry

#[derive(Clone, Copy, Default)]
struct Rect {
    x: i32,
    y: i32,
    w: i32,
}

/// Full-screen overlays that replace the keypad while shown.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Overlay {
    None,
    Help,
    About,
    History,
}

struct Ui {
    cols: i32,
    lines: i32,
    ox: i32,
    oy: i32,
    rect: [[Rect; MAX_COLS]; NROWS],
    fr: usize,
    fc: usize,
    overlay: Overlay,
    hist_msg: Option<String>, // status line shown under the history overlay
    out: String,
    input: Input,
}

impl Ui {
    fn new() -> Ui {
        Ui {
            cols: 80,
            lines: 24,
            ox: 0,
            oy: 0,
            rect: [[Rect::default(); MAX_COLS]; NROWS],
            fr: NROWS - 1,
            fc: 3, // start focus on "=" so Enter evaluates straight away
            overlay: Overlay::None,
            hist_msg: None,
            out: String::new(),
            input: Input::new(),
        }
    }

    // ----------------------------------------------------- out buffer

    fn os(&mut self, s: &str) {
        self.out.push_str(s);
    }

    fn at(&mut self, y: i32, x: i32) {
        let _ = write!(self.out, "\x1b[{y};{x}H");
    }

    fn style(&mut self, theme: &Theme, id: StyleId, st: StyleState) {
        let sgr = theme.sgr(id, st).to_string();
        self.os(&sgr);
    }

    fn style_off(&mut self) {
        let s = crate::theme::sgr_reset();
        self.os(s);
    }

    fn repeat(&mut self, s: &str, n: i32) {
        for _ in 0..n.max(0) {
            self.os(s);
        }
    }

    // ----------------------------------------------------------- layout

    fn layout(&mut self) {
        let (cols, lines) = term_size();
        self.cols = cols;
        self.lines = lines;

        self.ox = ((self.cols - W_FRAME) / 2 + 1).max(1);
        self.oy = ((self.lines - H_CONTENT) / 2 + 1).max(1);

        // Rows start after the frame (5 lines) plus a blank separator.
        let mut y = self.oy + 6;
        for (r, row) in rows().iter().enumerate() {
            let n = row.len() as i32;
            let w = W_INNER / n;
            let pad = (W_INNER - w * n) / 2;

            if r == NFUNC_ROWS {
                y += 1; // blank line between the two sections
            }
            for i in 0..row.len() {
                self.rect[r][i] = Rect {
                    x: self.ox + 1 + pad + i as i32 * w,
                    y,
                    w,
                };
            }
            y += 1;
        }
    }

    fn focus_clamp(&mut self) {
        if self.fr >= NROWS {
            self.fr = NROWS - 1;
        }
        let n = rows()[self.fr].len();
        if self.fc >= n {
            self.fc = n - 1;
        }
    }

    /// Move to `row`, keeping the button nearest the current horizontal centre.
    fn focus_row(&mut self, row: i32) {
        let row = if row < 0 {
            NROWS - 1
        } else if row as usize >= NROWS {
            0
        } else {
            row as usize
        };

        let cx = self.rect[self.fr][self.fc].x + self.rect[self.fr][self.fc].w / 2;
        let mut best = 0;
        let mut bestd = i32::MAX;
        for i in 0..rows()[row].len() {
            let d = (self.rect[row][i].x + self.rect[row][i].w / 2 - cx).abs();
            if d < bestd {
                bestd = d;
                best = i;
            }
        }
        self.fr = row;
        self.fc = best;
    }

    fn hit_test(&self, x: i32, y: i32) -> Option<(usize, usize)> {
        for r in 0..NROWS {
            for i in 0..rows()[r].len() {
                let rc = self.rect[r][i];
                if y == rc.y && x >= rc.x && x < rc.x + rc.w {
                    return Some((r, i));
                }
            }
        }
        None
    }

    // --------------------------------------------------------- rendering

    /// Is this button showing an engine mode that is currently on?
    fn btn_active(calc: &Calc, b: &Btn) -> bool {
        match b.act {
            Action::Base(want) => calc.base == want,
            Action::Inv => calc.inv,
            _ => false,
        }
    }

    fn btn_enabled(calc: &Calc, b: &Btn) -> bool {
        if b.hexdigit_only {
            return calc.base == Base::Hex;
        }
        match b.act {
            Action::Point | Action::Ee => calc.base == Base::Dec,
            Action::Digit(d) => (d as i64) < (calc.base as i64),
            _ => true,
        }
    }

    fn draw_btn(&mut self, calc: &Calc, theme: &Theme, r: usize, i: usize) {
        let b = &rows()[r][i];
        let chrome = theme.chrome();
        let lb = if !chrome.key_left.is_empty() { 1 } else { 0 };
        let rb = if !chrome.key_right.is_empty() { 1 } else { 0 };
        let rc = self.rect[r][i];
        let inner = rc.w - lb - rb;
        let mut len = b.label.len() as i32;

        let st = if r == self.fr && i == self.fc {
            StyleState::Focus
        } else if !Self::btn_enabled(calc, b) {
            StyleState::Disabled
        } else if Self::btn_active(calc, b) {
            StyleState::Active
        } else {
            StyleState::Normal
        };

        if len > inner {
            len = inner;
        }
        let lpad = (inner - len) / 2;
        let rpad = inner - len - lpad;

        self.at(rc.y, rc.x);
        self.style(theme, b.cat, st);
        let key_left = chrome.key_left.clone();
        let key_right = chrome.key_right.clone();
        self.os(&key_left);
        self.repeat(" ", lpad);
        self.os(&b.label[..len as usize]);
        self.repeat(" ", rpad);
        self.os(&key_right);
        self.style_off();
    }

    /// Right-align `s` in a field of `w`, clipping the front if it is too long.
    fn draw_right(&mut self, s: &str, w: i32) {
        let len = s.len() as i32;

        if len >= w {
            self.os("..");
            let keep = (w - 2).max(0) as usize;
            self.os(&s[s.len() - keep..]);
            return;
        }
        self.repeat(" ", w - len);
        self.os(s);
    }

    /// Every frame line is exactly W_INNER columns between the two verticals.
    fn draw_frame(&mut self, calc: &Calc, theme: &Theme) {
        let chrome_title = theme.chrome().title.clone();
        let chrome_h = theme.chrome().h.clone();
        let chrome_v = theme.chrome().v.clone();
        let chrome_tl = theme.chrome().tl.clone();
        let chrome_tr = theme.chrome().tr.clone();
        let chrome_bl = theme.chrome().bl.clone();
        let chrome_br = theme.chrome().br.clone();
        let pend = calc.pending_op();
        let tlen = chrome_title.chars().count() as i32;
        let (ox, oy) = (self.ox, self.oy);

        // Top border, with the title inlaid.
        self.at(oy, ox);
        self.style(theme, StyleId::Frame, StyleState::Normal);
        self.os(&chrome_tl);
        if tlen > 0 && tlen < W_INNER - 4 {
            self.os(&chrome_h);
            self.os(" ");
            self.style(theme, StyleId::Title, StyleState::Normal);
            self.os(&chrome_title);
            self.style(theme, StyleId::Frame, StyleState::Normal);
            self.os(" ");
            self.repeat(&chrome_h, W_INNER - 3 - tlen);
        } else {
            self.repeat(&chrome_h, W_INNER);
        }
        self.os(&chrome_tr);
        self.style_off();

        // Status line: angle unit, base, INV latch, pending operator, depth.
        self.at(oy + 1, ox);
        self.style(theme, StyleId::Frame, StyleState::Normal);
        self.os(&chrome_v);
        self.style_off();
        self.os(" ");
        let mut used;

        self.style(theme, StyleId::StatusMode, StyleState::Normal);
        let angle_name = calc.angle_name();
        let base_name = calc.base_name();
        let mode_str = format!("{angle_name}  {base_name}");
        used = mode_str.len() as i32;
        self.os(&mode_str);
        if calc.inv {
            self.style(theme, StyleId::StatusInv, StyleState::Normal);
            self.os("  INV");
            used += 5;
        }
        if pend != Op::None {
            let sym = Calc::op_symbol(pend);
            self.style(theme, StyleId::Status, StyleState::Normal);
            let s = format!("  {sym}");
            used += s.len() as i32;
            self.os(&s);
        }
        if calc.paren_depth > 0 {
            let depth = format!("  ({}", calc.paren_depth);
            used += depth.len() as i32;
            self.style(theme, StyleId::Status, StyleState::Normal);
            self.os(&depth);
        }
        self.style_off();
        if used > W_INNER - 3 {
            used = W_INNER - 3;
        }
        self.repeat(" ", W_INNER - 3 - used);

        self.style(theme, StyleId::StatusMem, StyleState::Normal);
        self.os(if calc.mem_set { "M" } else { " " });
        self.style_off();
        self.os(" ");
        self.style(theme, StyleId::Frame, StyleState::Normal);
        self.os(&chrome_v);
        self.style_off();

        // Main display.
        let disp = calc.display();
        self.at(oy + 2, ox);
        self.style(theme, StyleId::Frame, StyleState::Normal);
        self.os(&chrome_v);
        self.style(
            theme,
            if calc.error { StyleId::DisplayError } else { StyleId::Display },
            StyleState::Normal,
        );
        self.os(" ");
        self.draw_right(&disp, W_INNER - 2);
        self.os(" ");
        self.style(theme, StyleId::Frame, StyleState::Normal);
        self.os(&chrome_v);
        self.style_off();

        // The current value in the two bases we are not in.
        let mut aux = if calc.error {
            String::new()
        } else if calc.base == Base::Dec {
            format!("hex {}   bin {}", calc.render_base(Base::Hex), calc.render_base(Base::Bin))
        } else if calc.base == Base::Hex {
            format!("dec {}   bin {}", calc.render_base(Base::Dec), calc.render_base(Base::Bin))
        } else {
            format!("dec {}   hex {}", calc.render_base(Base::Dec), calc.render_base(Base::Hex))
        };
        if aux.len() as i32 > W_INNER - 2 {
            aux.truncate((W_INNER - 2) as usize);
        }

        self.at(oy + 3, ox);
        self.style(theme, StyleId::Frame, StyleState::Normal);
        self.os(&chrome_v);
        self.style(theme, StyleId::Aux, StyleState::Normal);
        let line = format!(" {:<width$} ", aux, width = (W_INNER - 2) as usize);
        self.os(&line);
        self.style(theme, StyleId::Frame, StyleState::Normal);
        self.os(&chrome_v);
        self.style_off();

        // Bottom border.
        self.at(oy + 4, ox);
        self.style(theme, StyleId::Frame, StyleState::Normal);
        self.os(&chrome_bl);
        self.repeat(&chrome_h, W_INNER);
        self.os(&chrome_br);
        self.style_off();
    }

    fn draw_hint(&mut self, theme: &Theme) {
        const HINT: &str = "arrows move  enter press  tab base  esc C  bksp DEL  ? help  q quit";
        let len = HINT.len() as i32;
        let pad = (W_INNER - len) / 2;

        self.at(self.oy + H_CONTENT - 1, self.ox + 1 + pad.max(0));
        self.style(theme, StyleId::Hint, StyleState::Normal);
        self.os(HINT);
        self.style_off();
    }

    /// One line of a full-screen overlay: `id` styles everything but the
    /// title, which always uses `StyleId::HelpTitle`.
    fn draw_overlay_line(&mut self, theme: &Theme, y: i32, x: i32, text: &str, i: usize) {
        self.at(y, x);
        self.style(
            theme,
            if i == 0 { StyleId::HelpTitle } else { StyleId::HelpText },
            StyleState::Normal,
        );
        self.os(text);
        self.style_off();
    }

    /// Lay out and draw `lines` as a centred full-screen overlay; returns
    /// `(top, left, n)` so callers can place extra content below it.
    fn draw_overlay(&mut self, theme: &Theme, lines: &[&str]) -> (i32, i32, i32) {
        let n = lines.len() as i32;
        let top = ((self.lines - n - 2) / 2 + 1).max(1);
        let left = ((self.cols - 70) / 2 + 1).max(1);

        for (i, line) in lines.iter().enumerate() {
            if top + i as i32 > self.lines {
                break;
            }
            self.draw_overlay_line(theme, top + i as i32, left, line, i);
        }
        (top, left, n)
    }

    fn draw_help(&mut self, theme: &Theme) {
        const LINES: &[&str] = &[
            "gbcalc key bindings",
            "",
            "  0-9 . +/-      digits, decimal point, sign (+/- is '_')",
            "  a-f            hexadecimal digits (HEX mode only)",
            "  + - * /        arithmetic      ^  power (INV: y-th root)",
            "  ( )  =         grouping and evaluate (Enter on '=' works too)",
            "  e  E           EE exponent entry, constant e     p  pi",
            "",
            "  s c t          sin cos tan     i  INV latch (2nd function)",
            "  l g            ln log          r  sqrt (INV: x^2)",
            "  v  !  %        1/x  x!  percent",
            "  d              cycle DEG / RAD / GRAD  (INV: convert value)",
            "",
            "  Tab            cycle DEC / HEX / BIN   D H B  pick directly",
            "  & | # ~        AND OR XOR NOT          <  >   shift left/right",
            "  \\              MOD",
            "",
            "  m n M X K A    STO RCL M+ MX MC ANS",
            "  I              about gbcalc",
            "  h              history            S  (inside) save to file",
            "  Esc            C (clear entry)   Delete  AC (all clear)",
            "  Backspace      DEL               Ctrl-L  redraw",
            "  arrows + Enter operate every button; mouse clicks work too",
            "",
            "  ?  close help          q / Ctrl-C  quit",
        ];
        let (top, left, n) = self.draw_overlay(theme, LINES);

        self.at(top + n + 1, left);
        self.style(theme, StyleId::Hint, StyleState::Normal);
        let s = format!("  theme: {}", theme.name());
        self.os(&s);
        self.style_off();
    }

    fn draw_about(&mut self, theme: &Theme) {
        let version = format!("  version {}", crate::GBCALC_VERSION);
        let lines: [&str; 12] = [
            "gbcalc -- about",
            "",
            "  A TUI scientific calculator: display on top, functions in",
            "  the middle, number entry at the bottom -- the function set",
            "  of xcalc, plus decimal / hexadecimal / binary modes.",
            "",
            "  One memory register and a saveable calculation history.",
            "",
            version.as_str(),
            "  by Guy Bruneau",
            "",
            "  any key  close about          q / Ctrl-C  quit",
        ];
        self.draw_overlay(theme, &lines);
    }

    fn draw_history(&mut self, calc: &Calc, theme: &Theme) {
        let mut lines: Vec<String> = vec!["gbcalc history".to_string(), String::new()];

        if calc.history.is_empty() {
            lines.push("  (empty -- results appear here after '=')".to_string());
        } else {
            // Budget rows for title/blank/blank/hint/status, then show the
            // most recent entries that fit, oldest of those first.
            let budget = (self.lines - 6).max(1) as usize;
            let start = calc.history.len().saturating_sub(budget);
            for entry in &calc.history[start..] {
                lines.push(format!("  {entry}"));
            }
        }
        lines.push(String::new());
        lines.push("  S  save to file          any other key  close history".to_string());

        let refs: Vec<&str> = lines.iter().map(String::as_str).collect();
        let (top, left, n) = self.draw_overlay(theme, &refs);

        if let Some(msg) = self.hist_msg.clone() {
            self.at(top + n + 1, left);
            self.style(theme, StyleId::Hint, StyleState::Normal);
            let s = format!("  {msg}");
            self.os(&s);
            self.style_off();
        }
    }

    fn render(&mut self, calc: &Calc, theme: &Theme) {
        self.out.clear();
        let reset = crate::theme::sgr_reset().to_string();
        self.os(&reset);
        self.os("\x1b[2J");

        if self.cols < MIN_COLS || self.lines < MIN_ROWS {
            self.at(1, 1);
            let s = format!(
                "gbcalc needs at least {MIN_COLS}x{MIN_ROWS}, terminal is {}x{}",
                self.cols, self.lines
            );
            self.os(&s);
            self.at(2, 1);
            self.os("resize the window, or press q to quit");
        } else {
            match self.overlay {
                Overlay::Help => self.draw_help(theme),
                Overlay::About => self.draw_about(theme),
                Overlay::History => self.draw_history(calc, theme),
                Overlay::None => {
                    self.draw_frame(calc, theme);
                    for r in 0..NROWS {
                        for i in 0..rows()[r].len() {
                            self.draw_btn(calc, theme, r, i);
                        }
                    }
                    self.draw_hint(theme);
                }
            }
        }

        self.at(self.lines, self.cols);
        let mut stdout = std::io::stdout();
        let _ = stdout.write_all(self.out.as_bytes());
        let _ = stdout.flush();
    }
}

// ---------------------------------------------------------- activation

fn run_action(calc: &mut Calc, a: Action) {
    match a {
        Action::None => {}
        Action::Digit(d) => calc.digit(d),
        Action::Point => calc.point(),
        Action::Ee => calc.ee(),
        Action::Sign => calc.sign(),
        Action::Backspace => calc.backspace(),
        Action::Clear => calc.clear_entry(),
        Action::AllClear => calc.all_clear(),
        Action::Op(op) => calc.op(op),
        Action::LParen => calc.lparen(),
        Action::RParen => calc.rparen(),
        Action::Equals => calc.equals(),
        Action::Unary(u) => calc.unary(u),
        Action::Const(k) => calc.konst(k),
        Action::Base(b) => calc.set_base(b),
        Action::Drg => calc.cycle_angle(),
        Action::DrgConv => calc.convert_angle(),
        Action::Inv => calc.toggle_inv(),
        Action::Sto => calc.mem_store(),
        Action::Rcl => calc.mem_recall(),
        Action::Madd => calc.mem_add(),
        Action::Mexc => calc.mem_exchange(),
        Action::Mclr => calc.mem_clear(),
        Action::Ans => calc.recall_ans(),
    }
}

/// Activate `b`, honouring the INV latch. No button is bound to quitting or
/// help -- those are handled directly by the key-dispatch loop in `run`.
fn press(calc: &mut Calc, b: &Btn) {
    let use_inv = calc.inv && b.inv != Action::None;
    let a = if use_inv { b.inv } else { b.act };

    if !Ui::btn_enabled(calc, b) {
        return;
    }

    run_action(calc, a);

    // INV is a one-shot latch, but pressing INV itself must not clear it.
    if b.act != Action::Inv {
        calc.inv = false;
    }
}

/// Find the button bound to `ch`, honouring base-dependent bindings.
/// Is `ch` bound to some non-hexdigit button (case-sensitive)?
fn key_bound_elsewhere(ch: u8) -> bool {
    for row in rows() {
        for b in row {
            if !b.hexdigit_only && !b.keys.is_empty() && b.keys.bytes().any(|k| k == ch) {
                return true;
            }
        }
    }
    false
}

fn find_key(calc: &Calc, ch: i32) -> Option<&'static Btn> {
    if !(0..=0xff).contains(&ch) {
        return None;
    }
    let ch = ch as u8;

    // In HEX mode the letters a-f are digits and outrank function keys.
    // Uppercase A-F do too, unless that exact letter is also a function key
    // (D DEC, B BIN, A ANS) -- otherwise the base could never be switched
    // back by keyboard once HEX is entered.
    if calc.base == Base::Hex {
        if (b'a'..=b'f').contains(&ch) {
            return Some(&ROW_N0[(ch - b'a') as usize]);
        }
        if (b'A'..=b'F').contains(&ch) && !key_bound_elsewhere(ch) {
            return Some(&ROW_N0[(ch - b'A') as usize]);
        }
    }

    for row in rows() {
        for b in row {
            if b.hexdigit_only {
                continue; // only reachable in HEX mode
            }
            if !b.keys.is_empty() && b.keys.bytes().any(|k| k == ch) {
                return Some(b);
            }
        }
    }
    None
}

/// Write the calculation history to `$XDG_CONFIG_HOME/gbcalc/history.txt`
/// (or `~/.config/gbcalc/history.txt`), overwriting any previous save.
/// Returns the path written to.
fn save_history(calc: &Calc) -> Result<String, String> {
    let dir = crate::theme::config_dir()
        .ok_or_else(|| "no HOME or XDG_CONFIG_HOME set".to_string())?;
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;

    let path = format!("{dir}/history.txt");
    let mut body = calc.history.join("\n");
    if !body.is_empty() {
        body.push('\n');
    }
    std::fs::write(&path, body).map_err(|e| e.to_string())?;
    Ok(path)
}

// ----------------------------------------------------------------- run

/// Runs the interactive calculator until the user quits. Returns an exit
/// code. Styling comes from `theme`, so load one before calling.
pub fn run(theme: &Theme) -> i32 {
    let term = match RawTerm::setup() {
        Ok(t) => t,
        Err(code) => return code,
    };

    let mut calc = Calc::new();
    let mut ui = Ui::new();
    let mut quit = false;

    while !quit && !STOP.load(Ordering::SeqCst) {
        if RESIZED.swap(false, Ordering::SeqCst) {
            ui.layout();
            ui.focus_clamp();
        }
        ui.render(&calc, theme);

        let r = ui.input.poll_stdin(-1);
        if r <= 0 {
            if r < 0 && std::io::Error::last_os_error().raw_os_error() == Some(libc::EINTR) {
                continue;
            }
            break;
        }
        let key = ui.input.read_key();

        match key {
            KEY_NONE => continue,
            KEY_UP => {
                ui.focus_row(ui.fr as i32 - 1);
                continue;
            }
            KEY_DOWN => {
                ui.focus_row(ui.fr as i32 + 1);
                continue;
            }
            KEY_LEFT => {
                let n = rows()[ui.fr].len();
                ui.fc = (ui.fc + n - 1) % n;
                continue;
            }
            KEY_RIGHT => {
                let n = rows()[ui.fr].len();
                ui.fc = (ui.fc + 1) % n;
                continue;
            }
            KEY_HOME => {
                ui.fc = 0;
                continue;
            }
            KEY_END => {
                ui.fc = rows()[ui.fr].len() - 1;
                continue;
            }
            KEY_DELETE => {
                calc.all_clear();
                continue;
            }
            KEY_MOUSE => {
                if ui.overlay != Overlay::None {
                    ui.overlay = Overlay::None;
                    ui.hist_msg = None;
                } else if let Some((r, i)) = ui.hit_test(ui.input.mx, ui.input.my) {
                    ui.fr = r;
                    ui.fc = i;
                    press(&mut calc, &rows()[r][i]);
                }
                continue;
            }
            _ => {}
        }

        if ui.overlay != Overlay::None {
            // Any key closes an overlay, except quit which still quits;
            // inside history, S saves it to a file without closing.
            if key == 'q' as i32 || key == 3 {
                quit = true;
            } else if ui.overlay == Overlay::History && key == 'S' as i32 {
                ui.hist_msg = Some(match save_history(&calc) {
                    Ok(path) => format!("saved to {path}"),
                    Err(e) => format!("save failed: {e}"),
                });
            } else {
                ui.overlay = Overlay::None;
                ui.hist_msg = None;
            }
            continue;
        }

        match key {
            k if k == 'q' as i32 || k == 3 || k == 4 => {
                quit = true;
                continue;
            }
            k if k == '?' as i32 => {
                ui.overlay = Overlay::Help;
                continue;
            }
            k if k == 'I' as i32 => {
                ui.overlay = Overlay::About;
                continue;
            }
            k if k == 'h' as i32 => {
                ui.overlay = Overlay::History;
                ui.hist_msg = None;
                continue;
            }
            12 => {
                RESIZED.store(true, Ordering::SeqCst);
                continue;
            }
            0x1b => {
                calc.clear_entry();
                continue;
            }
            127 | 8 => {
                calc.backspace();
                continue;
            }
            k if k == '\t' as i32 => {
                calc.cycle_base();
                continue;
            }
            k if k == '\r' as i32 || k == '\n' as i32 || k == ' ' as i32 => {
                press(&mut calc, &rows()[ui.fr][ui.fc]);
                continue;
            }
            _ => {}
        }

        if let Some(b) = find_key(&calc, key) {
            press(&mut calc, b);
        }
    }

    drop(term);
    0
}
