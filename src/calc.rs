//! gbcalc calculator engine (pure, no I/O).
//!
//! An immediate-execution algebraic calculator with operator precedence,
//! parentheses, a scientific function set, one memory register and
//! decimal / hexadecimal / binary entry and display modes.

const PI: f64 = std::f64::consts::PI;
const E: f64 = std::f64::consts::E;

/// Largest magnitude that survives a round trip through i64.
const I64_LIMIT: f64 = 9.223_372_036_854_776e18;

const STACK_MAX: usize = 32;
const ENTRY_MAX: usize = 72;
const HISTORY_MAX: usize = 200;

/// Number base for entry and display. Values are the radix itself.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Base {
    Dec = 10,
    Hex = 16,
    Bin = 2,
}

impl Base {
    fn radix(self) -> i64 {
        self as i64
    }
}

/// Angle unit used by the trigonometric functions.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum AngleMode {
    Deg,
    Rad,
    Grad,
}

/// Binary operators, listed low precedence first (see `prec`).
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum Op {
    #[default]
    None,
    LParen, // stack barrier, never applied
    Or,
    Xor,
    And,
    Shl,
    Shr,
    Add,
    Sub,
    Mul,
    Div,
    Mod,
    Pow,
    Root, // a^(1/b) -- INV of x^y
}

/// Unary functions.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Unary {
    Sin,
    Cos,
    Tan,
    Asin,
    Acos,
    Atan,
    Ln,
    Exp,
    Log10,
    Exp10,
    Sqrt,
    Sqr,
    Recip,
    Fact,
    Not,
    Pct,
}

/// Named constants.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Konst {
    Pi,
    E,
}

#[derive(Clone, Copy, Debug)]
struct Frame {
    value: f64,
    op: Op,
}

pub struct Calc {
    acc: f64,             // value shown when not entering
    entry: String,        // digits as typed
    entering: bool,       // entry buffer holds the value
    entry_has_exp: bool,  // EE pressed, digits go to exponent

    stack: Vec<Frame>,
    pub paren_depth: i32,

    pub base: Base,
    pub angle: AngleMode,
    pub inv: bool, // second-function latch, cleared after use

    pub mem: f64,
    pub mem_set: bool,
    last_ans: f64,

    pub error: bool,
    pub errmsg: Option<&'static str>, // valid while error is set

    /// Completed calculations, oldest first, as "expr = result" text.
    pub history: Vec<String>,

    // Entry state machine bookkeeping.
    value_ready: bool, // an operand is available (digit, unary, ")")
    op_pending: bool,  // last keypress was a binary operator
}

/// Maximum number of significant digits accepted per base.
fn max_digits(b: Base) -> usize {
    match b {
        Base::Hex => 16,
        Base::Bin => 64,
        Base::Dec => 18,
    }
}

fn digit_of(ch: char) -> Option<u32> {
    match ch {
        '0'..='9' => Some(ch as u32 - '0' as u32),
        'a'..='f' => Some(ch as u32 - 'a' as u32 + 10),
        'A'..='F' => Some(ch as u32 - 'A' as u32 + 10),
        _ => None,
    }
}

/// Parse the longest leading valid-float prefix of `s`, à la C's `strtod`
/// (which silently ignores trailing garbage rather than failing to parse).
fn strtod_prefix(s: &str) -> f64 {
    let b = s.as_bytes();
    let n = b.len();
    let mut i = 0;

    if i < n && (b[i] == b'-' || b[i] == b'+') {
        i += 1;
    }
    let int_start = i;
    while i < n && b[i].is_ascii_digit() {
        i += 1;
    }
    let int_digits = i - int_start;
    let mut frac_digits = 0;

    if i < n && b[i] == b'.' {
        let dot = i;
        i += 1;
        let frac_start = i;
        while i < n && b[i].is_ascii_digit() {
            i += 1;
        }
        frac_digits = i - frac_start;
        if int_digits == 0 && frac_digits == 0 {
            i = dot; // lone '.': back off, it is not part of a number
        }
    }
    if int_digits == 0 && frac_digits == 0 {
        return 0.0;
    }

    let mantissa_end = i;
    if i < n && (b[i] == b'e' || b[i] == b'E') {
        let mut j = i + 1;
        if j < n && (b[j] == b'-' || b[j] == b'+') {
            j += 1;
        }
        let exp_start = j;
        while j < n && b[j].is_ascii_digit() {
            j += 1;
        }
        if j > exp_start {
            i = j;
        } else {
            i = mantissa_end;
        }
    }
    s[..i].parse::<f64>().unwrap_or(0.0)
}

impl Default for Calc {
    fn default() -> Self {
        Calc {
            acc: 0.0,
            entry: String::new(),
            entering: false,
            entry_has_exp: false,
            stack: Vec::with_capacity(STACK_MAX),
            paren_depth: 0,
            base: Base::Dec,
            angle: AngleMode::Deg,
            inv: false,
            mem: 0.0,
            mem_set: false,
            last_ans: 0.0,
            error: false,
            errmsg: None,
            history: Vec::new(),
            value_ready: false,
            op_pending: false,
        }
    }
}

impl Calc {
    pub fn new() -> Self {
        Calc::default()
    }

    fn fail(&mut self, msg: &'static str) {
        self.error = true;
        self.errmsg = Some(msg);
        self.entering = false;
        self.entry.clear();
        self.stack.clear();
        self.paren_depth = 0;
        self.value_ready = false;
        self.op_pending = false;
    }

    /// Reject non-finite results as soon as they appear.
    fn guard(&mut self, v: f64, msg: Option<&'static str>) -> f64 {
        if v.is_nan() {
            self.fail(msg.unwrap_or("Error: undefined"));
            return 0.0;
        }
        if v.is_infinite() {
            self.fail("Error: overflow");
            return 0.0;
        }
        v
    }

    /// Parse the entry buffer according to the active base.
    fn entry_value(&self) -> f64 {
        let s = self.entry.as_str();

        if s.is_empty() {
            return 0.0;
        }
        if self.base == Base::Dec {
            return strtod_prefix(s);
        }

        let (neg, digits) = match s.strip_prefix('-') {
            Some(rest) => (true, rest),
            None => (false, s),
        };
        let mut u: u64 = 0;
        for ch in digits.chars() {
            if let Some(d) = digit_of(ch)
                && (d as i64) < self.base.radix() {
                    u = u.wrapping_mul(self.base.radix() as u64).wrapping_add(d as u64);
                }
        }
        if neg {
            -(u as f64)
        } else {
            u as f64
        }
    }

    pub fn current(&self) -> f64 {
        if self.entering {
            self.entry_value()
        } else {
            self.acc
        }
    }

    /// Consume the pending entry and return it as the current operand.
    fn take(&mut self) -> f64 {
        let v = self.current();

        self.acc = v;
        self.entering = false;
        self.entry.clear();
        self.entry_has_exp = false;
        v
    }

    // ------------------------------------------------------------ entry --

    /// Count significant digits already typed (ignores sign, point, exponent).
    fn count_digits(&self) -> usize {
        let mut n = 0;
        for ch in self.entry.chars() {
            if ch == 'e' || ch == 'E' {
                break;
            }
            if digit_of(ch).is_some() {
                n += 1;
            }
        }
        n
    }

    fn entry_push(&mut self, ch: char) {
        if self.entry.len() >= ENTRY_MAX {
            return;
        }
        self.entry.push(ch);
    }

    /// Begin a fresh entry unless one is already in progress.
    fn entry_begin(&mut self) {
        if !self.entering {
            self.entry.clear();
            self.entry_has_exp = false;
            self.entering = true;
        }
    }

    pub fn digit(&mut self, d: u32) {
        const SYM: &[u8; 16] = b"0123456789ABCDEF";

        if self.error || (d as i64) >= self.base.radix() {
            return;
        }

        self.entry_begin();

        // Exponent digits are capped at two, mantissa digits at the base limit.
        if self.entry_has_exp {
            let e_pos = self.entry.find(['e', 'E']).unwrap();
            let n = self.entry[e_pos + 1..]
                .chars()
                .filter(|&c| digit_of(c).is_some())
                .count();
            if n >= 2 {
                return;
            }
        } else if self.count_digits() >= max_digits(self.base) {
            return;
        }

        // Replace a lone leading zero rather than accumulating "000".
        if !self.entry_has_exp && self.entry == "0" {
            self.entry.clear();
        } else if !self.entry_has_exp && self.entry == "-0" {
            self.entry.truncate(1);
        }

        self.entry_push(SYM[d as usize] as char);
        self.value_ready = true;
        self.op_pending = false;
    }

    pub fn point(&mut self) {
        if self.error || self.base != Base::Dec {
            return;
        }

        self.entry_begin();
        if self.entry_has_exp || self.entry.contains('.') {
            return;
        }
        if self.entry.is_empty() || self.entry == "-" {
            self.entry_push('0');
        }
        self.entry_push('.');
        self.value_ready = true;
        self.op_pending = false;
    }

    pub fn ee(&mut self) {
        if self.error || self.base != Base::Dec {
            return;
        }

        self.entry_begin();
        if self.entry_has_exp {
            return;
        }
        if self.entry.is_empty() || self.entry == "-" {
            self.entry_push('1');
        }
        self.entry_push('e');
        self.entry_has_exp = true;
        self.value_ready = true;
        self.op_pending = false;
    }

    pub fn sign(&mut self) {
        if self.error {
            return;
        }

        if self.entering && self.entry_has_exp {
            // Toggle the exponent sign in place.
            let e_pos = self.entry.find(['e', 'E']).unwrap();
            let at = e_pos + 1;

            if self.entry[at..].starts_with('-') {
                self.entry.remove(at);
            } else if self.entry.len() < ENTRY_MAX {
                self.entry.insert(at, '-');
            }
            return;
        }

        if self.entering {
            if self.entry.starts_with('-') {
                self.entry.remove(0);
            } else if self.entry.len() < ENTRY_MAX {
                self.entry.insert(0, '-');
            }
            return;
        }

        self.acc = -self.acc;
        self.value_ready = true;
    }

    pub fn backspace(&mut self) {
        if self.error {
            self.all_clear();
            return;
        }
        if !self.entering {
            self.acc = 0.0;
            self.value_ready = false;
            return;
        }

        if self.entry.is_empty() {
            self.entering = false;
            self.acc = 0.0;
            return;
        }
        let last = self.entry.pop().unwrap();
        if last == 'e' || last == 'E' {
            self.entry_has_exp = false;
        }
        if self.entry.is_empty() || self.entry == "-" {
            self.entry.clear();
            self.entering = false;
            self.acc = 0.0;
        }
    }

    pub fn clear_entry(&mut self) {
        self.error = false;
        self.errmsg = None;
        self.entry.clear();
        self.entering = false;
        self.entry_has_exp = false;
        self.acc = 0.0;
        self.value_ready = false;
    }

    pub fn all_clear(&mut self) {
        self.clear_entry();
        self.stack.clear();
        self.paren_depth = 0;
        self.op_pending = false;
        self.inv = false;
    }

    // -------------------------------------------------------- operators --

    fn prec(op: Op) -> i32 {
        match op {
            Op::LParen => 0,
            Op::Or => 1,
            Op::Xor => 2,
            Op::And => 3,
            Op::Shl | Op::Shr => 4,
            Op::Add | Op::Sub => 5,
            Op::Mul | Op::Div | Op::Mod => 6,
            Op::Pow | Op::Root => 7,
            _ => 0,
        }
    }

    fn right_assoc(op: Op) -> bool {
        op == Op::Pow || op == Op::Root
    }

    /// Bitwise operands must be exact integers -- silently truncating a
    /// fraction here would hide a mistake rather than report it.
    fn to_i64(&mut self, v: f64) -> i64 {
        let v = snap_display(v);
        if !v.is_finite() || v != v.trunc() || v.abs() >= I64_LIMIT {
            self.fail("Error: not an integer");
            return 0;
        }
        v as i64
    }

    fn apply(&mut self, a: f64, op: Op, b: f64) -> f64 {
        match op {
            Op::Add => {
                let r = a + b;
                self.guard(r, None)
            }
            Op::Sub => {
                let r = a - b;
                self.guard(r, None)
            }
            Op::Mul => {
                let r = a * b;
                self.guard(r, None)
            }
            Op::Div => {
                if b == 0.0 {
                    self.fail("Error: divide by zero");
                    return 0.0;
                }
                let r = a / b;
                self.guard(r, None)
            }
            Op::Mod => {
                if b == 0.0 {
                    self.fail("Error: divide by zero");
                    return 0.0;
                }
                let r = a % b;
                self.guard(r, None)
            }
            Op::Pow => {
                let r = a.powf(b);
                self.guard(r, Some("Error: domain"))
            }
            Op::Root => {
                if b == 0.0 {
                    self.fail("Error: domain");
                    return 0.0;
                }
                let r = a.powf(1.0 / b);
                self.guard(r, Some("Error: domain"))
            }
            Op::And | Op::Or | Op::Xor | Op::Shl | Op::Shr => {
                let ia = self.to_i64(a);
                let ib = self.to_i64(b);
                if self.error {
                    return 0.0;
                }
                match op {
                    Op::And => (ia & ib) as f64,
                    Op::Or => (ia | ib) as f64,
                    Op::Xor => (ia ^ ib) as f64,
                    _ => {
                        if !(0..=63).contains(&ib) {
                            self.fail("Error: shift count");
                            return 0.0;
                        }
                        let sh = ib as u32;
                        if op == Op::Shl {
                            ((ia as u64) << sh) as i64 as f64
                        } else {
                            ((ia as u64) >> sh) as i64 as f64
                        }
                    }
                }
            }
            _ => b,
        }
    }

    /// Collapse pending operators that bind at least as tightly as `op`.
    fn reduce(&mut self, mut rhs: f64, op: Op) -> f64 {
        while let Some(top_frame) = self.stack.last() {
            if top_frame.op == Op::LParen {
                break;
            }
            let top = top_frame.op;
            if Self::prec(top) < Self::prec(op) {
                break;
            }
            if Self::prec(top) == Self::prec(op) && Self::right_assoc(op) {
                break;
            }
            let frame = self.stack.pop().unwrap();
            rhs = self.apply(frame.value, top, rhs);
            if self.error {
                return 0.0;
            }
        }
        rhs
    }

    pub fn op(&mut self, op: Op) {
        if self.error || op == Op::None || op == Op::LParen {
            return;
        }

        // Two operators in a row: the second one replaces the first.
        let rhs = if self.op_pending
            && self
                .stack
                .last()
                .map(|f| f.op != Op::LParen)
                .unwrap_or(false)
        {
            self.stack.pop().unwrap().value
        } else {
            self.take()
        };

        let rhs = self.reduce(rhs, op);
        if self.error {
            return;
        }

        if self.stack.len() >= STACK_MAX {
            self.fail("Error: too deep");
            return;
        }
        self.stack.push(Frame { value: rhs, op });

        self.acc = rhs;
        self.entering = false;
        self.entry.clear();
        self.entry_has_exp = false;
        self.op_pending = true;
        self.value_ready = false;
    }

    pub fn lparen(&mut self) {
        if self.error {
            return;
        }

        // "2(" reads as "2*(" rather than silently dropping the 2.
        if self.value_ready && !self.op_pending {
            self.op(Op::Mul);
        }
        if self.error {
            return;
        }

        if self.stack.len() >= STACK_MAX {
            self.fail("Error: too deep");
            return;
        }
        self.stack.push(Frame {
            value: 0.0,
            op: Op::LParen,
        });
        self.paren_depth += 1;

        self.entering = false;
        self.entry.clear();
        self.acc = 0.0;
        self.op_pending = false;
        self.value_ready = false;
    }

    pub fn rparen(&mut self) {
        if self.error || self.paren_depth == 0 {
            return;
        }

        let mut v = if self.op_pending
            && self
                .stack
                .last()
                .map(|f| f.op != Op::LParen)
                .unwrap_or(false)
        {
            self.stack.pop().unwrap().value // dangling operator: drop it
        } else {
            self.take()
        };

        v = self.reduce(v, Op::LParen);
        if self.error {
            return;
        }
        if self.stack.last().map(|f| f.op == Op::LParen).unwrap_or(false) {
            self.stack.pop();
        }
        self.paren_depth -= 1;

        self.acc = v;
        self.entering = false;
        self.entry.clear();
        self.op_pending = false;
        self.value_ready = true;
    }

    pub fn equals(&mut self) {
        if self.error {
            return;
        }

        // Snapshot the operator/operand stack for the history entry below.
        // A dangling trailing operator (last keypress was "op", not a digit)
        // is about to be dropped rather than applied -- exclude its frame so
        // the logged expression matches what actually gets computed.
        let dangling = self.op_pending
            && self.stack.last().map(|f| f.op != Op::LParen).unwrap_or(false);
        let mut frames = self.stack.clone();
        if dangling {
            frames.pop();
        }
        let open_parens = self.paren_depth;
        let last_operand = self.current();

        let mut v = if self.op_pending
            && self
                .stack
                .last()
                .map(|f| f.op != Op::LParen)
                .unwrap_or(false)
        {
            self.stack.pop().unwrap().value
        } else {
            self.take()
        };

        // Close any open parentheses implicitly, then drain the stack.
        while !self.stack.is_empty() {
            v = self.reduce(v, Op::LParen);
            if self.error {
                return;
            }
            if self.stack.last().map(|f| f.op == Op::LParen).unwrap_or(false) {
                self.stack.pop();
            }
        }

        self.paren_depth = 0;
        self.acc = v;
        self.last_ans = v;
        self.entering = false;
        self.entry.clear();
        self.op_pending = false;
        self.value_ready = false; // a following "(" starts a new expression

        // A bare "=" that changed nothing (no operator was ever applied)
        // isn't worth a history entry.
        if !frames.is_empty() || last_operand != v {
            self.record_history(&frames, open_parens, last_operand, v);
        }
    }

    /// Append an "expr = result" line to `history`, formatted in the base
    /// active at the time of the calculation.
    fn record_history(&mut self, frames: &[Frame], open_parens: i32, last: f64, result: f64) {
        let mut expr = String::new();
        for f in frames {
            if f.op == Op::LParen {
                if !expr.is_empty() && !expr.ends_with(' ') {
                    expr.push(' ');
                }
                expr.push('(');
                continue;
            }
            if !expr.is_empty() && !expr.ends_with('(') {
                expr.push(' ');
            }
            expr.push_str(&render_value(f.value, self.base));
            expr.push(' ');
            expr.push_str(Self::op_symbol(f.op));
        }
        if !expr.is_empty() && !expr.ends_with('(') {
            expr.push(' ');
        }
        expr.push_str(&render_value(last, self.base));
        for _ in 0..open_parens.max(0) {
            expr.push(')');
        }

        self.history.push(format!("{expr} = {}", render_value(result, self.base)));
        if self.history.len() > HISTORY_MAX {
            self.history.remove(0);
        }
    }

    // ------------------------------------------------------------ unary --

    fn to_rad(&self, x: f64) -> f64 {
        match self.angle {
            AngleMode::Deg => x * PI / 180.0,
            AngleMode::Grad => x * PI / 200.0,
            AngleMode::Rad => x,
        }
    }

    fn from_rad(&self, x: f64) -> f64 {
        match self.angle {
            AngleMode::Deg => x * 180.0 / PI,
            AngleMode::Grad => x * 200.0 / PI,
            AngleMode::Rad => x,
        }
    }

    pub fn unary(&mut self, u: Unary) {
        if self.error {
            return;
        }

        let x = self.take();
        let r = match u {
            Unary::Sin => self.to_rad(x).sin(),
            Unary::Cos => self.to_rad(x).cos(),
            Unary::Tan => self.to_rad(x).tan(),
            Unary::Asin => {
                if !(-1.0..=1.0).contains(&x) {
                    self.fail("Error: domain");
                    return;
                }
                self.from_rad(x.asin())
            }
            Unary::Acos => {
                if !(-1.0..=1.0).contains(&x) {
                    self.fail("Error: domain");
                    return;
                }
                self.from_rad(x.acos())
            }
            Unary::Atan => self.from_rad(x.atan()),
            Unary::Ln => {
                if x <= 0.0 {
                    self.fail("Error: domain");
                    return;
                }
                x.ln()
            }
            Unary::Log10 => {
                if x <= 0.0 {
                    self.fail("Error: domain");
                    return;
                }
                x.log10()
            }
            Unary::Exp => x.exp(),
            Unary::Exp10 => 10.0_f64.powf(x),
            Unary::Sqrt => {
                if x < 0.0 {
                    self.fail("Error: domain");
                    return;
                }
                x.sqrt()
            }
            Unary::Sqr => x * x,
            Unary::Recip => {
                if x == 0.0 {
                    self.fail("Error: divide by zero");
                    return;
                }
                1.0 / x
            }
            Unary::Fact => {
                // tgamma(x+1) extends x! to non-integers; poles at negative ints.
                if x < 0.0 && x == x.floor() {
                    self.fail("Error: domain");
                    return;
                }
                if x > 170.0 {
                    self.fail("Error: overflow");
                    return;
                }
                let mut r = unsafe { tgamma(x + 1.0) };
                if x >= 0.0 && x == x.floor() {
                    r = r.round();
                }
                r
            }
            Unary::Not => {
                let i = self.to_i64(x);
                if self.error {
                    return;
                }
                (!i) as f64
            }
            Unary::Pct => x / 100.0,
        };

        let r = self.guard(r, Some("Error: domain"));
        if self.error {
            return;
        }
        self.acc = r;
        self.value_ready = true;
        self.op_pending = false;
    }

    pub fn konst(&mut self, k: Konst) {
        if self.error {
            return;
        }

        // A constant replaces whatever was being typed.
        self.entering = false;
        self.entry.clear();
        self.entry_has_exp = false;
        self.acc = if k == Konst::Pi { PI } else { E };
        self.value_ready = true;
        self.op_pending = false;
    }

    // ------------------------------------------------------ modes, memory --

    pub fn set_base(&mut self, b: Base) {
        if self.base == b {
            return;
        }
        if self.entering {
            self.take(); // freeze the typed digits before switching
        }
        if b != Base::Dec {
            self.acc = self.acc.trunc();
        }
        self.base = b;
    }

    pub fn cycle_base(&mut self) {
        let next = match self.base {
            Base::Dec => Base::Hex,
            Base::Hex => Base::Bin,
            Base::Bin => Base::Dec,
        };
        self.set_base(next);
    }

    pub fn cycle_angle(&mut self) {
        self.angle = match self.angle {
            AngleMode::Deg => AngleMode::Rad,
            AngleMode::Rad => AngleMode::Grad,
            AngleMode::Grad => AngleMode::Deg,
        };
    }

    /// Keep the angle, change its unit: the number is rescaled.
    pub fn convert_angle(&mut self) {
        let rad = self.to_rad(self.current());

        self.take();
        self.cycle_angle();
        self.acc = self.from_rad(rad);
        self.value_ready = true;
    }

    pub fn toggle_inv(&mut self) {
        self.inv = !self.inv;
    }

    pub fn mem_store(&mut self) {
        if self.error {
            return;
        }
        self.mem = self.current();
        self.mem_set = true;
    }

    pub fn mem_recall(&mut self) {
        if self.error {
            return;
        }
        self.entering = false;
        self.entry.clear();
        self.acc = self.mem;
        self.value_ready = true;
        self.op_pending = false;
    }

    pub fn mem_add(&mut self) {
        if self.error {
            return;
        }
        let sum = self.mem + self.current();
        self.mem = self.guard(sum, None);
        self.mem_set = true;
    }

    pub fn mem_exchange(&mut self) {
        if self.error {
            return;
        }
        let v = self.current();
        self.entering = false;
        self.entry.clear();
        self.acc = self.mem;
        self.mem = v;
        self.mem_set = true;
        self.value_ready = true;
        self.op_pending = false;
    }

    pub fn mem_clear(&mut self) {
        self.mem = 0.0;
        self.mem_set = false;
    }

    pub fn recall_ans(&mut self) {
        if self.error {
            return;
        }
        self.entering = false;
        self.entry.clear();
        self.acc = self.last_ans;
        self.value_ready = true;
        self.op_pending = false;
    }

    // ------------------------------------------------------- formatting --

    /// Value rendered in a specific base; `"-"` when not representable.
    pub fn render_base(&self, b: Base) -> String {
        render_value(self.current(), b)
    }

    /// Main display text (error message, entry as typed, or formatted value).
    pub fn display(&self) -> String {
        if self.error {
            return self.errmsg.unwrap_or("Error").to_string();
        }
        if self.entering && !self.entry.is_empty() {
            return if self.base == Base::Dec {
                self.entry.clone()
            } else {
                fmt_entry_grouped(&self.entry, if self.base == Base::Hex { 4 } else { 8 })
            };
        }
        self.render_base(self.base)
    }

    pub fn angle_name(&self) -> &'static str {
        match self.angle {
            AngleMode::Deg => "DEG",
            AngleMode::Rad => "RAD",
            AngleMode::Grad => "GRAD",
        }
    }

    pub fn base_name(&self) -> &'static str {
        match self.base {
            Base::Hex => "HEX",
            Base::Bin => "BIN",
            Base::Dec => "DEC",
        }
    }

    pub fn op_symbol(op: Op) -> &'static str {
        match op {
            Op::Add => "+",
            Op::Sub => "-",
            Op::Mul => "*",
            Op::Div => "/",
            Op::Mod => "mod",
            Op::Pow => "^",
            Op::Root => "root",
            Op::And => "and",
            Op::Or => "or",
            Op::Xor => "xor",
            Op::Shl => "<<",
            Op::Shr => ">>",
            _ => "",
        }
    }

    /// Innermost pending operator, or `Op::None`.
    pub fn pending_op(&self) -> Op {
        match self.stack.last() {
            Some(f) if f.op != Op::LParen => f.op,
            _ => Op::None,
        }
    }
}

/// Round to the precision the display shows, so a value that reads as "30"
/// behaves like 30 even when the double is 29.999999999999996 (as trig
/// results tend to be). Genuine fractions such as 1.5 are unaffected.
fn snap_display(v: f64) -> f64 {
    if !v.is_finite() {
        return v;
    }
    if v == v.trunc() && v.abs() < 1e15 {
        return v;
    }
    format_g(v, 12).parse::<f64>().unwrap_or(v)
}

/// Minimal `%.<prec>g` formatter (C semantics): `prec` significant digits,
/// fixed or scientific notation chosen automatically, trailing zeros trimmed.
fn format_g(v: f64, prec: usize) -> String {
    if v == 0.0 {
        return "0".to_string();
    }
    let prec = prec.max(1);
    let exp = v.abs().log10().floor() as i32;

    if exp < -4 || exp >= prec as i32 {
        // Scientific notation, `prec` significant digits.
        let digits = prec.saturating_sub(1);
        let s = format!("{:.*e}", digits, v);
        // Rust prints e.g. "1.234e2"; normalize to "1.234e+02" style trimmed
        // of trailing zeros in the mantissa, matching %g's trimming.
        let (mantissa, exp_part) = s.split_once('e').unwrap();
        let mantissa = trim_trailing_zeros(mantissa);
        let exp_val: i32 = exp_part.parse().unwrap();
        format!("{}e{}{:02}", mantissa, if exp_val < 0 { "-" } else { "+" }, exp_val.abs())
    } else {
        let decimals = (prec as i32 - 1 - exp).max(0) as usize;
        let s = format!("{:.*}", decimals, v);
        trim_trailing_zeros(&s)
    }
}

fn trim_trailing_zeros(s: &str) -> String {
    if !s.contains('.') {
        return s.to_string();
    }
    let s = s.trim_end_matches('0');
    let s = s.trim_end_matches('.');
    if s.is_empty() || s == "-" {
        "0".to_string()
    } else {
        s.to_string()
    }
}

// libm's `tgamma`, which extends the factorial to reals -- there is no
// equivalent in Rust's standard library.
#[link(name = "m")]
unsafe extern "C" {
    fn tgamma(x: f64) -> f64;
}

/// Render `u` in `base`, inserting a space every `grp` digits.
fn fmt_ubase(mut u: u64, base: u32, grp: usize) -> String {
    const SYM: &[u8; 16] = b"0123456789ABCDEF";
    let mut digits = Vec::new();

    if u == 0 {
        digits.push(b'0');
    }
    while u != 0 {
        digits.push(SYM[(u % base as u64) as usize]);
        u /= base as u64;
    }
    digits.reverse();

    let mut out = String::new();
    let nd = digits.len();
    for (idx, &d) in digits.iter().enumerate() {
        out.push(d as char);
        let remaining = nd - idx - 1;
        if remaining > 0 && remaining % grp == 0 {
            out.push(' ');
        }
    }
    out
}

/// Render `v` in base `b`; `"-"` when not representable (see `render_base`).
fn render_value(v: f64, b: Base) -> String {
    if b == Base::Dec {
        return fmt_dec(v);
    }
    // Hex and binary show the 64-bit two's-complement pattern.
    let v = snap_display(v);
    if !v.is_finite() || v != v.trunc() || v.abs() >= I64_LIMIT {
        return "-".to_string();
    }
    fmt_ubase(v as i64 as u64, b.radix() as u32, if b == Base::Hex { 4 } else { 8 })
}

fn fmt_dec(v: f64) -> String {
    if v == 0.0 {
        // Also normalises -0.
        return "0".to_string();
    }
    if v == v.trunc() && v.abs() < 1e15 {
        return format!("{:.0}", v);
    }
    format_g(v, 12)
}

/// Group the raw entry text the same way a finished value would be grouped.
fn fmt_entry_grouped(s: &str, grp: usize) -> String {
    let chars: Vec<char> = s.chars().collect();
    let len = chars.len();
    let mut out = String::new();

    for (i, &ch) in chars.iter().enumerate() {
        let rest = len - i - 1;
        out.push(ch.to_ascii_uppercase());
        if rest > 0 && rest.is_multiple_of(grp) {
            out.push(' ');
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    const PI_REF: f64 = 3.141_592_653_589_793_238_46;

    fn approx(got: f64, want: f64) {
        let tol = 1e-9 * if want.abs() > 1.0 { want.abs() } else { 1.0 };
        assert!(
            (got - want).abs() <= tol,
            "got {got:.17} want {want:.17}"
        );
    }

    /// Type a run of decimal digits (and '.') into the entry buffer.
    fn type_str(c: &mut Calc, s: &str) {
        for ch in s.chars() {
            match ch {
                '.' => c.point(),
                '0'..='9' => c.digit(ch as u32 - '0' as u32),
                'a'..='f' => c.digit(ch as u32 - 'a' as u32 + 10),
                'A'..='F' => c.digit(ch as u32 - 'A' as u32 + 10),
                _ => {}
            }
        }
    }

    fn eval2(a: &str, op: Op, b: &str) -> f64 {
        let mut c = Calc::new();
        type_str(&mut c, a);
        c.op(op);
        type_str(&mut c, b);
        c.equals();
        c.current()
    }

    #[test]
    fn basic_arithmetic() {
        approx(eval2("2", Op::Add, "3"), 5.0);
        approx(eval2("7", Op::Sub, "9"), -2.0);
        approx(eval2("6", Op::Mul, "7"), 42.0);
        approx(eval2("9", Op::Div, "4"), 2.25);
        approx(eval2("2", Op::Pow, "10"), 1024.0);
        approx(eval2("17", Op::Mod, "5"), 2.0);
    }

    #[test]
    fn precedence() {
        let mut c = Calc::new();
        type_str(&mut c, "2");
        c.op(Op::Add);
        type_str(&mut c, "3");
        c.op(Op::Mul);
        type_str(&mut c, "4");
        c.equals();
        approx(c.current(), 14.0);

        let mut c = Calc::new();
        type_str(&mut c, "2");
        c.op(Op::Mul);
        type_str(&mut c, "3");
        c.op(Op::Add);
        approx(c.current(), 6.0);
        type_str(&mut c, "4");
        c.equals();
        approx(c.current(), 10.0);

        // Power is right associative: 2^3^2 = 2^9 = 512.
        let mut c = Calc::new();
        type_str(&mut c, "2");
        c.op(Op::Pow);
        type_str(&mut c, "3");
        c.op(Op::Pow);
        type_str(&mut c, "2");
        c.equals();
        approx(c.current(), 512.0);
    }

    #[test]
    fn parens() {
        let mut c = Calc::new();
        c.lparen();
        type_str(&mut c, "2");
        c.op(Op::Add);
        type_str(&mut c, "3");
        c.rparen();
        c.op(Op::Mul);
        type_str(&mut c, "4");
        c.equals();
        approx(c.current(), 20.0);

        let mut c = Calc::new();
        type_str(&mut c, "2");
        c.lparen();
        type_str(&mut c, "3");
        c.op(Op::Add);
        type_str(&mut c, "4");
        c.rparen();
        c.equals();
        approx(c.current(), 14.0);

        let mut c = Calc::new();
        type_str(&mut c, "1");
        c.op(Op::Add);
        c.lparen();
        type_str(&mut c, "2");
        c.op(Op::Mul);
        c.lparen();
        type_str(&mut c, "3");
        c.op(Op::Add);
        type_str(&mut c, "4");
        c.equals();
        approx(c.current(), 15.0);

        let mut c = Calc::new();
        type_str(&mut c, "5");
        c.rparen();
        c.op(Op::Add);
        type_str(&mut c, "1");
        c.equals();
        approx(c.current(), 6.0);
    }

    #[test]
    fn operator_replacement() {
        let mut c = Calc::new();
        type_str(&mut c, "8");
        c.op(Op::Add);
        c.op(Op::Mul);
        type_str(&mut c, "2");
        c.equals();
        approx(c.current(), 16.0);

        let mut c = Calc::new();
        type_str(&mut c, "2");
        c.op(Op::Add);
        type_str(&mut c, "3");
        c.op(Op::Mul);
        c.op(Op::Add);
        type_str(&mut c, "4");
        c.equals();
        approx(c.current(), 9.0);
    }

    #[test]
    fn unary() {
        let mut c = Calc::new(); // degrees by default
        type_str(&mut c, "30");
        c.unary(Unary::Sin);
        approx(c.current(), 0.5);

        let mut c = Calc::new();
        c.cycle_angle(); // RAD
        assert_eq!(c.angle_name(), "RAD");
        c.konst(Konst::Pi);
        c.unary(Unary::Sin);
        approx(c.current(), 0.0);

        let mut c = Calc::new();
        type_str(&mut c, "5");
        c.unary(Unary::Fact);
        approx(c.current(), 120.0);

        let mut c = Calc::new();
        type_str(&mut c, "16");
        c.unary(Unary::Sqrt);
        approx(c.current(), 4.0);

        let mut c = Calc::new();
        type_str(&mut c, "1000");
        c.unary(Unary::Log10);
        approx(c.current(), 3.0);

        let mut c = Calc::new();
        type_str(&mut c, "8");
        c.unary(Unary::Recip);
        assert_eq!(c.display(), "0.125");

        let mut c = Calc::new();
        type_str(&mut c, "50");
        c.unary(Unary::Pct);
        approx(c.current(), 0.5);

        let mut c = Calc::new();
        type_str(&mut c, "9");
        c.unary(Unary::Sqrt);
        c.op(Op::Add);
        type_str(&mut c, "1");
        c.equals();
        approx(c.current(), 4.0);
    }

    #[test]
    fn errors() {
        let mut c = Calc::new();
        type_str(&mut c, "1");
        c.op(Op::Div);
        type_str(&mut c, "0");
        c.equals();
        assert!(c.error);
        assert_eq!(c.display(), "Error: divide by zero");

        type_str(&mut c, "5");
        assert!(c.error);
        c.clear_entry();
        assert!(!c.error);
        approx(c.current(), 0.0);

        let mut c = Calc::new();
        type_str(&mut c, "2");
        c.unary(Unary::Asin);
        assert!(c.error);

        let mut c = Calc::new();
        type_str(&mut c, "1");
        c.sign();
        c.unary(Unary::Ln);
        assert!(c.error);

        let mut c = Calc::new();
        type_str(&mut c, "1");
        c.sign();
        c.unary(Unary::Fact);
        assert!(c.error);
    }

    #[test]
    fn entry_editing() {
        let mut c = Calc::new();
        type_str(&mut c, "12.50");
        assert_eq!(c.display(), "12.50");
        approx(c.current(), 12.5);

        c.backspace();
        assert_eq!(c.display(), "12.5");

        c.point();
        assert_eq!(c.display(), "12.5");

        c.sign();
        assert_eq!(c.display(), "-12.5");
        c.sign();
        assert_eq!(c.display(), "12.5");

        let mut c = Calc::new();
        type_str(&mut c, "0007");
        assert_eq!(c.display(), "7");

        let mut c = Calc::new();
        type_str(&mut c, "1.5");
        c.ee();
        c.sign();
        type_str(&mut c, "3");
        approx(c.current(), 1.5e-3);
        c.sign();
        approx(c.current(), 1.5e3);
    }

    #[test]
    fn bases() {
        let mut c = Calc::new();
        type_str(&mut c, "255");
        c.set_base(Base::Hex);
        assert_eq!(c.display(), "FF");
        assert_eq!(c.render_base(Base::Bin), "11111111");

        let mut c = Calc::new();
        c.set_base(Base::Hex);
        type_str(&mut c, "1f");
        approx(c.current(), 31.0);
        assert_eq!(c.display(), "1F");

        let mut c = Calc::new();
        type_str(&mut c, "9");
        c.digit(12); // 'C' in decimal mode: ignored
        approx(c.current(), 9.0);

        let mut c = Calc::new();
        type_str(&mut c, "10.9");
        c.set_base(Base::Hex);
        assert_eq!(c.display(), "A");

        let mut c = Calc::new();
        type_str(&mut c, "1193046"); // 0x123456
        assert_eq!(c.render_base(Base::Hex), "12 3456");

        let mut c = Calc::new();
        c.set_base(Base::Hex);
        type_str(&mut c, "0");
        c.unary(Unary::Not);
        assert_eq!(c.display(), "FFFF FFFF FFFF FFFF");
        approx(c.current(), -1.0);

        let mut c = Calc::new();
        type_str(&mut c, "0.5");
        c.unary(Unary::Asin);
        assert_eq!(c.display(), "30");
        assert_eq!(c.render_base(Base::Hex), "1E");

        c.op(Op::And);
        type_str(&mut c, "31");
        c.equals();
        assert!(!c.error);
        approx(c.current(), 30.0);

        let mut c = Calc::new();
        type_str(&mut c, "0.5");
        assert_eq!(c.render_base(Base::Hex), "-");

        let mut c = Calc::new();
        type_str(&mut c, "0.5");
        assert_eq!(c.render_base(Base::Dec), "0.5");

        let mut c = Calc::new();
        assert_eq!(c.base_name(), "DEC");
        c.cycle_base();
        assert_eq!(c.base_name(), "HEX");
        c.cycle_base();
        assert_eq!(c.base_name(), "BIN");
        c.cycle_base();
        assert_eq!(c.base_name(), "DEC");
    }

    #[test]
    fn bitwise() {
        approx(eval2("12", Op::And, "10"), 8.0);
        approx(eval2("12", Op::Or, "10"), 14.0);
        approx(eval2("12", Op::Xor, "10"), 6.0);
        approx(eval2("1", Op::Shl, "8"), 256.0);
        approx(eval2("256", Op::Shr, "4"), 16.0);

        let mut c = Calc::new();
        type_str(&mut c, "1");
        c.op(Op::Add);
        type_str(&mut c, "2");
        c.op(Op::And);
        type_str(&mut c, "3");
        c.equals();
        approx(c.current(), 3.0);

        let mut c = Calc::new();
        type_str(&mut c, "1");
        c.op(Op::Shl);
        type_str(&mut c, "64");
        c.equals();
        assert!(c.error);

        let mut c = Calc::new();
        type_str(&mut c, "1.5");
        c.op(Op::And);
        type_str(&mut c, "1");
        c.equals();
        assert!(c.error);
    }

    #[test]
    fn memory_and_ans() {
        let mut c = Calc::new();
        type_str(&mut c, "42");
        c.mem_store();
        assert!(c.mem_set);
        c.all_clear();
        c.mem_recall();
        approx(c.current(), 42.0);

        type_str(&mut c, "8");
        c.mem_add();
        c.all_clear();
        c.mem_recall();
        approx(c.current(), 50.0);

        let mut c = Calc::new();
        type_str(&mut c, "3");
        c.mem_store();
        c.clear_entry();
        type_str(&mut c, "7");
        c.mem_exchange();
        approx(c.current(), 3.0);
        approx(c.mem, 7.0);

        c.mem_clear();
        assert!(!c.mem_set);

        let mut c = Calc::new();
        type_str(&mut c, "6");
        c.op(Op::Mul);
        type_str(&mut c, "7");
        c.equals();
        c.all_clear();
        c.recall_ans();
        approx(c.current(), 42.0);
    }

    #[test]
    fn angle_conversion() {
        let mut c = Calc::new();
        type_str(&mut c, "180");
        c.convert_angle();
        assert_eq!(c.angle_name(), "RAD");
        approx(c.current(), PI_REF);

        c.convert_angle();
        assert_eq!(c.angle_name(), "GRAD");
        approx(c.current(), 200.0);
    }

    #[test]
    fn display_formatting() {
        let mut c = Calc::new();
        type_str(&mut c, "1");
        c.op(Op::Div);
        type_str(&mut c, "3");
        c.equals();
        assert_eq!(c.display(), "0.333333333333");

        let mut c = Calc::new();
        type_str(&mut c, "0");
        c.op(Op::Sub);
        type_str(&mut c, "0");
        c.equals();
        assert_eq!(c.display(), "0");

        let mut c = Calc::new();
        type_str(&mut c, "2");
        c.op(Op::Pow);
        type_str(&mut c, "100");
        c.equals();
        assert_eq!(c.display(), "1.26765060023e+30");
    }

    #[test]
    fn history() {
        // A bare "=" with no operator applied logs nothing.
        let mut c = Calc::new();
        type_str(&mut c, "42");
        c.equals();
        assert!(c.history.is_empty());
        c.equals(); // repeat presses stay silent too
        assert!(c.history.is_empty());

        // Precedence is reflected in the logged expression, not just typed order.
        let mut c = Calc::new();
        type_str(&mut c, "2");
        c.op(Op::Add);
        type_str(&mut c, "3");
        c.op(Op::Mul);
        type_str(&mut c, "4");
        c.equals();
        assert_eq!(c.history, vec!["2 + 3 * 4 = 14"]);

        // Parentheses are reconstructed around the grouped operands.
        let mut c = Calc::new();
        type_str(&mut c, "1");
        c.op(Op::Add);
        c.lparen();
        type_str(&mut c, "2");
        c.op(Op::Mul);
        type_str(&mut c, "3");
        c.equals();
        assert_eq!(c.history, vec!["1 + (2 * 3) = 7"]);

        // A dangling trailing operator ("2 + 3 *" then "=") is dropped, and
        // the logged expression matches what actually got computed (2 + 3).
        let mut c = Calc::new();
        type_str(&mut c, "2");
        c.op(Op::Add);
        type_str(&mut c, "3");
        c.op(Op::Mul);
        c.equals();
        assert_eq!(c.history, vec!["2 + 3 = 5"]);

        // Multiple equals presses append, oldest first.
        let mut c = Calc::new();
        type_str(&mut c, "1");
        c.op(Op::Add);
        type_str(&mut c, "1");
        c.equals();
        type_str(&mut c, "5");
        c.op(Op::Mul);
        type_str(&mut c, "5");
        c.equals();
        assert_eq!(c.history, vec!["1 + 1 = 2", "5 * 5 = 25"]);
    }
}
