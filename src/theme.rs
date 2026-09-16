//! gbcalc theme loading and SGR generation.
//!
//! Every colour, attribute and border glyph the UI draws comes from here.
//! The built-in default theme is itself written in the theme file format and
//! parsed at startup, so a user theme file is just a partial override of it.

use std::fmt::Write as _;
use std::fs;

const DEFAULT_THEMEDIR: &str = match option_env!("GBCALC_THEMEDIR") {
    Some(v) => v,
    None => "/usr/local/share/gbcalc/themes",
};

const N_STYLES: usize = 22;
const N_STATES: usize = 4;

/// One style slot per thing the UI can paint. Key slots are the calculator
/// function categories -- that is what a theme colours by.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum StyleId {
    // frame and display
    Frame = 0,
    Title,
    Status, // pending operator, paren depth
    StatusMode, // angle unit and number base
    StatusInv,  // the INV latch indicator
    StatusMem,  // the M indicator
    Display,
    DisplayError,
    Aux, // the alternate-base line
    Hint,
    HelpTitle,
    HelpText,
    // key categories
    KeyDigit,    // 0-9 . +/-
    KeyHexdigit, // A-F
    KeyOperator, // + - * / x^y
    KeySci,      // trig, logs, roots, constants
    KeyBitwise,  // AND OR XOR NOT << >> MOD
    KeyMode,     // DRG INV DEC HEX BIN
    KeyMemory,   // STO RCL M+ MX MC ANS
    KeyEdit,     // C AC DEL
    KeyEquals,
    KeyParen,
}

/// Visual state a slot can be drawn in.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum StyleState {
    Normal = 0,
    Focus,
    Active,
    Disabled,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum ColorMode {
    Auto,
    TrueColor,
    C256,
    C16,
    None,
}

/// Border and key glyphs, also theme-controlled.
pub struct ThemeChrome {
    pub tl: String,
    pub tr: String,
    pub bl: String,
    pub br: String,
    pub h: String,
    pub v: String,
    pub key_left: String,
    pub key_right: String,
    pub title: String,
}

// ------------------------------------------------------------------ model

#[derive(Clone, Copy, PartialEq, Eq, Default)]
enum ColorKind {
    #[default]
    None,
    Idx,
    Rgb,
}

#[derive(Clone, Copy, Default)]
struct Color {
    kind: ColorKind,
    idx: i32,
    r: i32,
    g: i32,
    b: i32,
}

const ATTR_BOLD: u32 = 0x01;
const ATTR_DIM: u32 = 0x02;
const ATTR_ITALIC: u32 = 0x04;
const ATTR_UNDER: u32 = 0x08;
const ATTR_REVERSE: u32 = 0x10;
const ATTR_SWAP: u32 = 0x20; // request: exchange fg and bg
const ATTR_CLEAR: u32 = 0x40; // request: drop inherited attributes
const ATTR_SWAPPED: u32 = 0x80; // result: the exchange was applied

const SET_FG: u32 = 0x1;
const SET_BG: u32 = 0x2;

#[derive(Clone, Copy, Default)]
struct Style {
    fg: Color,
    bg: Color,
    attrs: u32,
    set: u32,
}

pub struct Theme {
    base: [Style; N_STYLES],
    variant: [[Style; N_STATES]; N_STYLES],
    have_variant: [[bool; N_STATES]; N_STYLES],
    xform: [Style; N_STATES],

    sgr: [[String; N_STATES]; N_STYLES],
    mode: ColorMode,
    effective: ColorMode,
    name: String,
    chrome: ThemeChrome,
}

// --------------------------------------------------------- default theme

/// Written in the theme file format and parsed by the same code that reads
/// user themes, so the format can never drift from the defaults.
const DEFAULT_THEME: &str = "\
# gbcalc default theme
#
# Syntax:  <slot> [fg=<colour>] [bg=<colour>] [attributes...]
# Colours: #rrggbb, #rgb, 0-255 (palette index), a colour name
#          (black red green yellow blue magenta cyan white, and the
#          bright* variants), or `default` for the terminal's own.
# Attrs:   bold dim italic underline reverse swap none
#
# A slot may be suffixed .focus, .active or .disabled to style that state
# directly; otherwise the state.* transforms below are applied to the base
# style. `key` sets every key category at once, so later, more specific
# lines override it.
#
# A user theme only needs the lines it wants to change.

name             tokyo-night
title            gbcalc
border           rounded
brackets         [ ]

# --- frame and display ---------------------------------------------
frame            fg=#7aa2f7 bold
title.style      fg=#7dcfff bold
status           fg=#7aa2f7
status.mode      fg=#7dcfff bold
status.inv       fg=#bb9af7 bold
status.mem       fg=#e0af68 bold
display          fg=#c0caf5 bold
display.error    fg=#f7768e bold
aux              fg=#7dcfff dim
hint             fg=#565f89
help.title       fg=#7dcfff bold
help.text        fg=#a9b1d6

# --- key categories ------------------------------------------------
key              fg=#16161e bg=#a9b1d6
key.digit        fg=#16161e bg=#a9b1d6
key.hexdigit     fg=#16161e bg=#73daca
key.operator     fg=#16161e bg=#ff9e64
key.sci          fg=#c0caf5 bg=#3b4261
key.bitwise      fg=#16161e bg=#bb9af7
key.mode         fg=#16161e bg=#7dcfff
key.memory       fg=#16161e bg=#e0af68
key.edit         fg=#16161e bg=#f7768e
key.equals       fg=#16161e bg=#9ece6a bold
key.paren        fg=#c0caf5 bg=#414868

# --- states --------------------------------------------------------
state.focus      swap bold
state.active     fg=#16161e bg=#9ece6a bold
state.disabled   fg=#414868 bg=default dim none
";

// ------------------------------------------------------- colour maps

const NAMED: &[(&str, i32)] = &[
    ("black", 0),
    ("red", 1),
    ("green", 2),
    ("yellow", 3),
    ("blue", 4),
    ("magenta", 5),
    ("cyan", 6),
    ("white", 7),
    ("brightblack", 8),
    ("brightred", 9),
    ("brightgreen", 10),
    ("brightyellow", 11),
    ("brightblue", 12),
    ("brightmagenta", 13),
    ("brightcyan", 14),
    ("brightwhite", 15),
    ("grey", 8),
    ("gray", 8),
];

/// xterm's first sixteen entries, for degrading true colour to 16 colours.
const BASE16: [[i32; 3]; 16] = [
    [0, 0, 0],
    [205, 0, 0],
    [0, 205, 0],
    [205, 205, 0],
    [0, 0, 238],
    [205, 0, 205],
    [0, 205, 205],
    [229, 229, 229],
    [127, 127, 127],
    [255, 0, 0],
    [0, 255, 0],
    [255, 255, 0],
    [92, 92, 255],
    [255, 0, 255],
    [0, 255, 255],
    [255, 255, 255],
];

fn clamp255(v: i32) -> i32 {
    v.clamp(0, 255)
}

/// Nearest entry in the 6x6x6 cube or the grayscale ramp.
fn quantize256(r: i32, g: i32, b: i32) -> i32 {
    let dr = (r - g).abs();
    let dg = (g - b).abs();
    let db = (r - b).abs();

    if dr < 12 && dg < 12 && db < 12 {
        if r < 8 {
            return 16;
        }
        if r > 248 {
            return 231;
        }
        return 232 + (r - 8) * 24 / 241;
    }
    16 + 36 * (r * 5 / 255) + 6 * (g * 5 / 255) + (b * 5 / 255)
}

fn quantize16(r: i32, g: i32, b: i32) -> i32 {
    let mut best = 7;
    let mut bestd = i32::MAX;

    for (i, c) in BASE16.iter().enumerate() {
        let dr = r - c[0];
        let dg = g - c[1];
        let db = b - c[2];
        let d = dr * dr + dg * dg + db * db;

        if d < bestd {
            bestd = d;
            best = i as i32;
        }
    }
    best
}

// ------------------------------------------------------------ parsing

fn parse_hex_color(s: &str) -> Option<Color> {
    let len = s.len();
    if len != 3 && len != 6 {
        return None;
    }
    let mut v = [0i32; 6];
    for (i, ch) in s.chars().enumerate() {
        if !ch.is_ascii_hexdigit() {
            return None;
        }
        v[i] = if ch.is_ascii_digit() {
            ch as i32 - '0' as i32
        } else {
            ch.to_ascii_lowercase() as i32 - 'a' as i32 + 10
        };
    }
    let (r, g, b) = if len == 3 {
        (v[0] * 17, v[1] * 17, v[2] * 17)
    } else {
        (v[0] * 16 + v[1], v[2] * 16 + v[3], v[4] * 16 + v[5])
    };
    Some(Color {
        kind: ColorKind::Rgb,
        idx: 0,
        r,
        g,
        b,
    })
}

fn parse_color(s: &str) -> Option<Color> {
    if let Some(hex) = s.strip_prefix('#') {
        return parse_hex_color(hex);
    }

    let low = s.to_ascii_lowercase();
    if low == "default" || low == "none" || low == "-" {
        return Some(Color::default()); // kind = ColorKind::None
    }
    if low.chars().next().is_some_and(|c| c.is_ascii_digit()) {
        return low.parse::<i32>().ok().filter(|&n| (0..=255).contains(&n)).map(|n| Color {
            kind: ColorKind::Idx,
            idx: n,
            ..Default::default()
        });
    }
    NAMED.iter().find(|(name, _)| *name == low).map(|&(_, idx)| Color {
        kind: ColorKind::Idx,
        idx,
        ..Default::default()
    })
}

fn parse_attr(s: &str, attrs: &mut u32) -> bool {
    match s {
        "bold" => *attrs |= ATTR_BOLD,
        "dim" => *attrs |= ATTR_DIM,
        "italic" => *attrs |= ATTR_ITALIC,
        "underline" => *attrs |= ATTR_UNDER,
        "reverse" => *attrs |= ATTR_REVERSE,
        "swap" => *attrs |= ATTR_SWAP,
        "none" => *attrs |= ATTR_CLEAR,
        _ => return false,
    }
    true
}

/// Slot name table. "title.style" avoids colliding with the `title` text
/// directive.
const SLOTS: &[(&str, StyleId)] = &[
    ("frame", StyleId::Frame),
    ("title.style", StyleId::Title),
    ("status", StyleId::Status),
    ("status.mode", StyleId::StatusMode),
    ("status.inv", StyleId::StatusInv),
    ("status.mem", StyleId::StatusMem),
    ("display", StyleId::Display),
    ("display.error", StyleId::DisplayError),
    ("aux", StyleId::Aux),
    ("hint", StyleId::Hint),
    ("help.title", StyleId::HelpTitle),
    ("help.text", StyleId::HelpText),
    ("key.digit", StyleId::KeyDigit),
    ("key.hexdigit", StyleId::KeyHexdigit),
    ("key.operator", StyleId::KeyOperator),
    ("key.sci", StyleId::KeySci),
    ("key.bitwise", StyleId::KeyBitwise),
    ("key.mode", StyleId::KeyMode),
    ("key.memory", StyleId::KeyMemory),
    ("key.edit", StyleId::KeyEdit),
    ("key.equals", StyleId::KeyEquals),
    ("key.paren", StyleId::KeyParen),
];

fn lookup_slot(name: &str) -> Option<StyleId> {
    SLOTS.iter().find(|(n, _)| *n == name).map(|&(_, id)| id)
}

/// Apply `src` on top of `dst`.
fn merge(dst: &mut Style, src: &Style) {
    if src.set & SET_FG != 0 {
        dst.fg = src.fg;
        dst.set |= SET_FG;
    }
    if src.set & SET_BG != 0 {
        dst.bg = src.bg;
        dst.set |= SET_BG;
    }
    if src.attrs & ATTR_CLEAR != 0 {
        dst.attrs = src.attrs & !ATTR_CLEAR;
    } else {
        dst.attrs |= src.attrs;
    }
}

fn set_border(chrome: &mut ThemeChrome, kind: &str) {
    let (tl, tr, bl, br, h, v) = match kind {
        "square" => ("\u{250c}", "\u{2510}", "\u{2514}", "\u{2518}", "\u{2500}", "\u{2502}"),
        "heavy" => ("\u{250f}", "\u{2513}", "\u{2517}", "\u{251b}", "\u{2501}", "\u{2503}"),
        "double" => ("\u{2554}", "\u{2557}", "\u{255a}", "\u{255d}", "\u{2550}", "\u{2551}"),
        "ascii" => ("+", "+", "+", "+", "-", "|"),
        "none" => (" ", " ", " ", " ", " ", " "),
        _ => ("\u{256d}", "\u{256e}", "\u{2570}", "\u{256f}", "\u{2500}", "\u{2502}"), // rounded
    };
    chrome.tl = tl.to_string();
    chrome.tr = tr.to_string();
    chrome.bl = bl.to_string();
    chrome.br = br.to_string();
    chrome.h = h.to_string();
    chrome.v = v.to_string();
}

/// Strip a trailing state suffix, returning the state it named.
fn split_state(name: &str) -> (&str, StyleState) {
    const SUFFIXES: &[(&str, StyleState)] = &[
        (".focus", StyleState::Focus),
        (".active", StyleState::Active),
        (".disabled", StyleState::Disabled),
    ];
    for &(suffix, state) in SUFFIXES {
        if let Some(stripped) = name.strip_suffix(suffix)
            && !stripped.is_empty() {
                return (stripped, state);
            }
    }
    (name, StyleState::Normal)
}

/// Split off the first whitespace-delimited token, returning it and the
/// (left-trimmed) remainder of the line verbatim -- mirrors `strtok_r`'s
/// behaviour of leaving later text unsplit until tokenized again.
fn split_first(line: &str) -> (&str, &str) {
    let line = line.trim_start_matches([' ', '\t', '\r']);
    match line.find([' ', '\t', '\r']) {
        Some(i) => (&line[..i], line[i..].trim_start_matches([' ', '\t', '\r'])),
        None => (line, ""),
    }
}

impl Theme {
    fn parse_buffer(&mut self, text: &str, origin: &str) -> Result<(), String> {
        for (i, raw_line) in text.split('\n').enumerate() {
            let lineno = i + 1;

            // A '#' starts a comment only at a token boundary -- inside a
            // token it introduces a colour, as in fg=#7aa2f7.
            let mut line = raw_line;
            let bytes = line.as_bytes();
            for (pos, &b) in bytes.iter().enumerate() {
                if b == b'#' && (pos == 0 || (bytes[pos - 1] as char).is_whitespace()) {
                    line = &line[..pos];
                    break;
                }
            }

            let (tok, rest) = split_first(line);
            if tok.is_empty() {
                continue;
            }

            // Text directives take the rest of the line verbatim.
            if tok == "name" || tok == "title" {
                let rest = rest.trim_end_matches([' ', '\t', '\r']);
                if tok == "name" {
                    self.name = rest.to_string();
                } else {
                    self.chrome.title = rest.to_string();
                }
                continue;
            }
            if tok == "border" {
                let kind = rest.split_whitespace().next().unwrap_or("rounded");
                set_border(&mut self.chrome, kind);
                continue;
            }
            if tok == "brackets" {
                let mut it = rest.split_whitespace();
                let l = it.next();
                let r = it.next();
                match l {
                    None | Some("none") => {
                        self.chrome.key_left.clear();
                        self.chrome.key_right.clear();
                    }
                    Some(l) => {
                        self.chrome.key_left = l.to_string();
                        self.chrome.key_right = r.unwrap_or(l).to_string();
                    }
                }
                continue;
            }

            let (slot_name, state) = split_state(tok);
            let slot_name = slot_name.to_string();

            let mut st = Style::default();
            for word in rest.split_whitespace() {
                if let Some(color) = word.strip_prefix("fg=") {
                    match parse_color(color) {
                        Some(c) => {
                            st.fg = c;
                            st.set |= SET_FG;
                        }
                        None => return Err(format!("{origin}:{lineno}: bad value '{word}'")),
                    }
                } else if let Some(color) = word.strip_prefix("bg=") {
                    match parse_color(color) {
                        Some(c) => {
                            st.bg = c;
                            st.set |= SET_BG;
                        }
                        None => return Err(format!("{origin}:{lineno}: bad value '{word}'")),
                    }
                } else if !parse_attr(word, &mut st.attrs) {
                    return Err(format!("{origin}:{lineno}: bad value '{word}'"));
                }
            }

            if slot_name == "state" {
                // "state" alone is meaningless; needs a suffix.
                if state == StyleState::Normal {
                    return Err(format!(
                        "{origin}:{lineno}: state needs .focus, .active or .disabled"
                    ));
                }
                merge(&mut self.xform[state as usize], &st);
                continue;
            }
            if slot_name == "key" {
                for k in key_slot_ids() {
                    if state == StyleState::Normal {
                        merge(&mut self.base[k as usize], &st);
                    } else {
                        merge(&mut self.variant[k as usize][state as usize], &st);
                        self.have_variant[k as usize][state as usize] = true;
                    }
                }
                continue;
            }
            let id = match lookup_slot(&slot_name) {
                Some(id) => id,
                None => return Err(format!("{origin}:{lineno}: unknown slot '{slot_name}'")),
            };
            if state == StyleState::Normal {
                merge(&mut self.base[id as usize], &st);
            } else {
                merge(&mut self.variant[id as usize][state as usize], &st);
                self.have_variant[id as usize][state as usize] = true;
            }
        }
        Ok(())
    }

    // ----------------------------------------------------- SGR generation

    fn resolve(&self, id: StyleId, st: StyleState) -> Style {
        let mut out = self.base[id as usize];

        if st == StyleState::Normal {
            return out;
        }
        if self.have_variant[id as usize][st as usize] {
            merge(&mut out, &self.variant[id as usize][st as usize]);
            return out;
        }

        // Derive the state from the base style using the global transform.
        let xform = self.xform[st as usize];
        if xform.attrs & ATTR_SWAP != 0 {
            if out.set & SET_FG != 0 && out.set & SET_BG != 0 {
                std::mem::swap(&mut out.fg, &mut out.bg);
                out.attrs |= ATTR_SWAPPED;
            } else {
                out.attrs |= ATTR_REVERSE;
            }
        }
        merge(&mut out, &xform);
        out.attrs &= !(ATTR_SWAP | ATTR_CLEAR);
        out
    }

    fn append_color(&self, buf: &mut String, c: &Color, is_bg: bool) {
        let base = if is_bg { 40 } else { 30 };
        let ext = if is_bg { 48 } else { 38 };

        let idx = match c.kind {
            ColorKind::None => return,
            ColorKind::Rgb => {
                let (r, g, b) = (clamp255(c.r), clamp255(c.g), clamp255(c.b));
                if self.effective == ColorMode::TrueColor {
                    let _ = write!(buf, ";{ext};2;{r};{g};{b}");
                    return;
                }
                if self.effective == ColorMode::C256 {
                    quantize256(r, g, b)
                } else {
                    quantize16(r, g, b)
                }
            }
            ColorKind::Idx => {
                if self.effective == ColorMode::C16 && c.idx > 15 {
                    c.idx % 16
                } else {
                    c.idx
                }
            }
        };

        if idx < 8 {
            let _ = write!(buf, ";{}", base + idx);
        } else if idx < 16 {
            let _ = write!(buf, ";{}", base + 60 + idx - 8);
        } else {
            let _ = write!(buf, ";{ext};5;{idx}");
        }
    }

    fn build_sgr(&self, id: StyleId, st: StyleState) -> String {
        let s = self.resolve(id, st);
        let mut buf = String::from("\x1b[0");

        if s.attrs & ATTR_BOLD != 0 {
            buf.push_str(";1");
        }
        if s.attrs & ATTR_DIM != 0 {
            buf.push_str(";2");
        }
        if s.attrs & ATTR_ITALIC != 0 {
            buf.push_str(";3");
        }
        if s.attrs & ATTR_UNDER != 0 {
            buf.push_str(";4");
        }
        // Without colour, a swap can only be expressed as reverse video.
        if (s.attrs & ATTR_REVERSE != 0)
            || (self.effective == ColorMode::None && s.attrs & ATTR_SWAPPED != 0)
        {
            buf.push_str(";7");
        }
        if self.effective != ColorMode::None {
            self.append_color(&mut buf, &s.fg, false);
            self.append_color(&mut buf, &s.bg, true);
        }
        buf.push('m');
        buf
    }

    fn rebuild(&mut self) {
        self.effective = if self.mode == ColorMode::Auto {
            detect_color_mode()
        } else {
            self.mode
        };
        for id in ALL_STYLE_IDS {
            for &st in &[
                StyleState::Normal,
                StyleState::Focus,
                StyleState::Active,
                StyleState::Disabled,
            ] {
                self.sgr[id as usize][st as usize] = self.build_sgr(id, st);
            }
        }
    }

    // --------------------------------------------------------------- api

    /// Install the built-in default theme.
    pub fn new() -> Theme {
        let mut chrome = ThemeChrome {
            tl: String::new(),
            tr: String::new(),
            bl: String::new(),
            br: String::new(),
            h: String::new(),
            v: String::new(),
            key_left: "[".to_string(),
            key_right: "]".to_string(),
            title: "gbcalc".to_string(),
        };
        set_border(&mut chrome, "rounded");

        let mut theme = Theme {
            base: [Style::default(); N_STYLES],
            variant: [[Style::default(); N_STATES]; N_STYLES],
            have_variant: [[false; N_STATES]; N_STYLES],
            xform: [Style::default(); N_STATES],
            sgr: std::array::from_fn(|_| std::array::from_fn(|_| String::new())),
            mode: ColorMode::Auto,
            effective: ColorMode::TrueColor,
            name: "builtin".to_string(),
            chrome,
        };

        if let Err(e) = theme.parse_buffer(DEFAULT_THEME, "<builtin>") {
            eprintln!("gbcalc: built-in theme: {e}");
        }
        theme.rebuild();
        theme
    }

    /// Merge a theme file over the current theme.
    pub fn load_file(&mut self, path: &str) -> Result<(), String> {
        let text = fs::read_to_string(path).map_err(|_| format!("cannot read '{path}'"))?;
        self.parse_buffer(&text, path)?;
        self.rebuild();
        Ok(())
    }

    /// Resolve `name` against the theme search path, then load it. A name
    /// that looks like a path (contains '/' or ends in .conf) is used
    /// directly.
    pub fn load_named(&mut self, name: &str) -> Result<(), String> {
        if name.contains('/') || name.ends_with(".conf") {
            return self.load_file(name);
        }
        if name.len() > 128 {
            return Err("theme name is too long".to_string());
        }

        for dir in theme_dirs() {
            let path = format!("{dir}/{name}.conf");
            if fs::metadata(&path).is_ok() {
                return self.load_file(&path);
            }
        }
        Err(format!(
            "no theme named '{name}' on the theme path (try --list-themes)"
        ))
    }

    /// Load the user's default theme if one exists. Missing is not an error.
    pub fn load_user_default(&mut self) {
        let Some(path) = user_theme_conf_path() else {
            return;
        };
        if fs::metadata(&path).is_err() {
            return;
        }
        if let Err(e) = self.load_file(&path) {
            eprintln!("gbcalc: {e}");
        }
    }

    pub fn set_color_mode(&mut self, m: ColorMode) {
        self.mode = m;
        self.rebuild();
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn chrome(&self) -> &ThemeChrome {
        &self.chrome
    }

    /// SGR escape sequence for a slot in a state, always valid.
    pub fn sgr(&self, id: StyleId, st: StyleState) -> &str {
        &self.sgr[id as usize][st as usize]
    }
}

impl Default for Theme {
    fn default() -> Self {
        Theme::new()
    }
}

/// Sequence that returns the terminal to its default appearance.
pub fn sgr_reset() -> &'static str {
    "\x1b[0m"
}

const ALL_STYLE_IDS: [StyleId; N_STYLES] = [
    StyleId::Frame,
    StyleId::Title,
    StyleId::Status,
    StyleId::StatusMode,
    StyleId::StatusInv,
    StyleId::StatusMem,
    StyleId::Display,
    StyleId::DisplayError,
    StyleId::Aux,
    StyleId::Hint,
    StyleId::HelpTitle,
    StyleId::HelpText,
    StyleId::KeyDigit,
    StyleId::KeyHexdigit,
    StyleId::KeyOperator,
    StyleId::KeySci,
    StyleId::KeyBitwise,
    StyleId::KeyMode,
    StyleId::KeyMemory,
    StyleId::KeyEdit,
    StyleId::KeyEquals,
    StyleId::KeyParen,
];

fn key_slot_ids() -> [StyleId; 10] {
    [
        StyleId::KeyDigit,
        StyleId::KeyHexdigit,
        StyleId::KeyOperator,
        StyleId::KeySci,
        StyleId::KeyBitwise,
        StyleId::KeyMode,
        StyleId::KeyMemory,
        StyleId::KeyEdit,
        StyleId::KeyEquals,
        StyleId::KeyParen,
    ]
}

// --------------------------------------------------------- search path

/// Directories searched for named themes, most specific first. The
/// environment list is capped so a hostile `GBCALC_THEME_PATH` cannot push
/// out the built-in fallback locations.
const THEME_ENV_MAX: usize = 9;

fn theme_dirs() -> Vec<String> {
    let mut dirs = Vec::new();

    if let Ok(env) = std::env::var("GBCALC_THEME_PATH")
        && !env.is_empty() {
            for part in env.split(':').take(THEME_ENV_MAX) {
                if !part.is_empty() {
                    dirs.push(part.to_string());
                }
            }
        }
    if let Ok(xdg) = std::env::var("XDG_CONFIG_HOME") {
        if !xdg.is_empty() {
            dirs.push(format!("{xdg}/gbcalc/themes"));
        }
    } else if let Ok(home) = std::env::var("HOME")
        && !home.is_empty() {
            dirs.push(format!("{home}/.config/gbcalc/themes"));
        }
    dirs.push("themes".to_string());
    dirs.push(DEFAULT_THEMEDIR.to_string());
    dirs
}

fn user_theme_conf_path() -> Option<String> {
    if let Ok(xdg) = std::env::var("XDG_CONFIG_HOME")
        && !xdg.is_empty() {
            return Some(format!("{xdg}/gbcalc/theme.conf"));
        }
    if let Ok(home) = std::env::var("HOME")
        && !home.is_empty() {
            return Some(format!("{home}/.config/gbcalc/theme.conf"));
        }
    None
}

pub fn detect_color_mode() -> ColorMode {
    if std::env::var_os("NO_COLOR").is_some() {
        return ColorMode::None;
    }
    if let Ok(ct) = std::env::var("COLORTERM")
        && (ct.contains("truecolor") || ct.contains("24bit")) {
            return ColorMode::TrueColor;
        }
    let term = std::env::var("TERM").unwrap_or_default();
    if term.is_empty() || term == "dumb" {
        return ColorMode::None;
    }
    if term.contains("direct") {
        return ColorMode::TrueColor;
    }
    if term.contains("256color") {
        return ColorMode::C256;
    }
    ColorMode::C16
}

pub fn parse_color_mode(s: &str) -> Option<ColorMode> {
    match s {
        "auto" => Some(ColorMode::Auto),
        "truecolor" | "24bit" => Some(ColorMode::TrueColor),
        "256" => Some(ColorMode::C256),
        "16" | "ansi" => Some(ColorMode::C16),
        "none" | "off" => Some(ColorMode::None),
        _ => None,
    }
}

/// The built-in theme, which doubles as the format's documentation.
pub fn dump() -> &'static str {
    DEFAULT_THEME
}

/// Print the theme search path and the files found in it.
pub fn list() {
    println!("theme search path:");
    for dir in theme_dirs() {
        println!("  {dir}");
        let Ok(entries) = fs::read_dir(&dir) else {
            continue;
        };
        let mut found = 0;
        for name in entries
            .filter_map(|e| e.ok())
            .filter_map(|e| e.file_name().into_string().ok())
            .filter_map(|n| n.strip_suffix(".conf").map(|s| s.to_string()))
        {
            println!("      {name}");
            found += 1;
        }
        if found == 0 {
            println!("      (none)");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn truecolor_theme() -> Theme {
        let mut t = Theme::new();
        t.set_color_mode(ColorMode::TrueColor);
        t
    }

    #[test]
    fn metadata_and_chrome() {
        let t = truecolor_theme();
        assert_eq!(t.name(), "tokyo-night");
        assert_eq!(t.chrome().title, "gbcalc");
        assert_eq!(t.chrome().tl, "\u{256d}");
        assert_eq!(t.chrome().key_left, "[");
        assert_eq!(t.chrome().key_right, "]");
    }

    #[test]
    fn frame_style() {
        let t = truecolor_theme();
        assert_eq!(
            t.sgr(StyleId::Frame, StyleState::Normal),
            "\x1b[0;1;38;2;122;162;247m"
        );
    }

    #[test]
    fn focus_swaps_fg_and_bg() {
        let t = truecolor_theme();
        assert_eq!(
            t.sgr(StyleId::KeyDigit, StyleState::Focus),
            "\x1b[0;1;38;2;169;177;214;48;2;22;22;30m"
        );
    }

    #[test]
    fn disabled_drops_bg_but_keeps_base_attrs() {
        // `none` in `state.disabled` clears whatever was in the (empty)
        // xform accumulator at parse time, not the base style it is later
        // merged onto -- so the base's `bold` survives alongside `dim`.
        // This matches the C original's `merge()` bit for bit.
        let t = truecolor_theme();
        assert_eq!(
            t.sgr(StyleId::Frame, StyleState::Disabled),
            "\x1b[0;1;2;38;2;65;72;104m"
        );
    }

    #[test]
    fn active_state_is_explicit_override() {
        let t = truecolor_theme();
        assert_eq!(
            t.sgr(StyleId::KeyDigit, StyleState::Active),
            "\x1b[0;1;38;2;22;22;30;48;2;158;206;106m"
        );
    }

    #[test]
    fn color_mode_reduces_precision() {
        let mut t = Theme::new();
        t.set_color_mode(ColorMode::C16);
        // fg=#7aa2f7 quantizes to the nearest of the 16 base colours.
        let got = t.sgr(StyleId::Frame, StyleState::Normal);
        assert!(got.starts_with("\x1b[0;1;"));
        assert!(!got.contains(";2;")); // no truecolor triplet
    }

    #[test]
    fn none_mode_drops_color_but_keeps_reverse() {
        let mut t = Theme::new();
        t.set_color_mode(ColorMode::None);
        assert_eq!(t.sgr(StyleId::KeyDigit, StyleState::Focus), "\x1b[0;1;7m");
    }

    #[test]
    fn bad_slot_reports_line_number() {
        let mut t = Theme::new();
        let err = t.load_file("/nonexistent/gbcalc-theme.conf").unwrap_err();
        assert!(err.contains("cannot read"));
    }

    #[test]
    fn unknown_slot_is_rejected() {
        let mut t = Theme::new();
        let err = t.parse_buffer("bogus fg=red\n", "<test>").unwrap_err();
        assert_eq!(err, "<test>:1: unknown slot 'bogus'");
    }

    #[test]
    fn state_without_suffix_is_rejected() {
        let mut t = Theme::new();
        let err = t.parse_buffer("state bold\n", "<test>").unwrap_err();
        assert_eq!(
            err,
            "<test>:1: state needs .focus, .active or .disabled"
        );
    }

    #[test]
    fn comment_only_inside_token_boundary() {
        let mut t = Theme::new(); // base theme already has "frame ... bold"
        t.parse_buffer("frame fg=#010203 # a comment\n", "<test>").unwrap();
        t.set_color_mode(ColorMode::TrueColor);
        assert_eq!(
            t.sgr(StyleId::Frame, StyleState::Normal),
            "\x1b[0;1;38;2;1;2;3m"
        );
    }
}
