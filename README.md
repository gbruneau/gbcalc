# gbcalc

A TUI scientific calculator written in C — the function set of `xcalc`, plus
decimal / hexadecimal / binary modes, in a terminal.

```
╭─ gbcalc ──────────────────────────────────────────────────────────────╮
│ DEG  DEC  +  (2                                                     M │
│                                                                     5 │
│ hex 5   bin 101                                                       │
╰───────────────────────────────────────────────────────────────────────╯

 [   DRG    ][   INV    ][   sin    ][   cos    ][   tan    ][    x!    ]
 [   1/x    ][   x^2    ][   sqrt   ][   x^y    ][    ln    ][   log    ]
 [   e^x    ][   10^x   ][    EE    ][    pi    ][    e     ][    %     ]
 [    (     ][    )     ][   STO    ][   RCL    ][    M+    ][    MX    ]
 [   DEC    ][   HEX    ][   BIN    ][   AND    ][    OR    ][   XOR    ]
 [   NOT    ][    <<    ][    >>    ][   MOD    ][    MC    ][   ANS    ]

 [    A     ][    B     ][    C     ][    D     ][    E     ][    F     ]
 [       7        ][       8        ][       9        ][       /        ]
 [       4        ][       5        ][       6        ][       *        ]
 [       1        ][       2        ][       3        ][       -        ]
 [       0        ][       .        ][      +/-       ][       +        ]
 [       C        ][       AC       ][      DEL       ][       =        ]
```

Display on top, functions in the middle, number entry at the bottom.

## Build

No dependencies beyond a C99 compiler and libm — no ncurses.

```sh
make            # build ./gbcalc
make test       # run the engine unit tests
make run        # build and launch
make install    # optional, PREFIX=/usr/local
```

Needs a terminal of at least 74x20. It tells you and recovers on resize if
the window is smaller.

## Using it

Every button works three ways: its shortcut key, arrow keys + Enter, or a
mouse click. Focus starts on `=`, so Enter evaluates straight away. Press
`?` for the bindings, `q` to quit.

| Keys | |
|---|---|
| `0`–`9` `.` `_` | digits, decimal point, sign (`_` is `+/-`) |
| `a`–`f` | hexadecimal digits (HEX mode only) |
| `+` `-` `*` `/` `^` `\` | add, subtract, multiply, divide, power, MOD |
| `(` `)` `=` | grouping, evaluate |
| `s` `c` `t` | sin, cos, tan |
| `i` | INV latch — the next key uses its second function |
| `l` `g` `r` `v` `!` `%` | ln, log, sqrt, 1/x, x!, percent |
| `e` `E` `p` | EE exponent entry, constant e, pi |
| `d` | cycle DEG / RAD / GRAD (with INV: convert the value) |
| `Tab` / `D` `H` `B` | cycle or pick DEC / HEX / BIN |
| `&` `\|` `#` `~` `<` `>` | AND, OR, XOR, NOT, shift left, shift right |
| `m` `n` `M` `X` `K` `A` | STO, RCL, M+, MX, MC, ANS |
| `Esc` `Delete` `Backspace` | C (clear entry), AC (all clear), DEL |

`INV` is a one-shot latch: `i` then `s` gives `asin`, `i` then `r` gives
`x^2`, `i` then `^` gives the y-th root. The status line shows the angle
unit, base, a latched `INV`, the pending operator, open-paren depth, and `M`
when memory holds a value.

The line under the display always shows the current value in the two bases
you are not in, so hex/binary conversion needs no extra keystrokes.

## Behaviour worth knowing

**Operator precedence.** Unlike `xcalc`, which evaluates strictly left to
right, gbcalc applies normal precedence: `2 + 3 * 4` is `14`, not `20`.
`^` is right-associative, so `2^3^2` is `512`. Bitwise operators bind looser
than arithmetic, matching C: `1 + 2 AND 3` is `3`. Parentheses nest, `=`
closes any that are still open, and a number directly before `(` means
multiplication — `2(3+4)` is `14`.

**Hex and binary** are 64-bit integer views. Negative values show as the
two's-complement pattern, so `NOT 0` reads `FFFF FFFF FFFF FFFF` in hex
while the decimal display shows `-1`. Switching base truncates toward zero:
`10.9` becomes `A`. A non-integer has no hex or binary form, shown as `-`.

**Bitwise operands must be integers.** `1.5 AND 1` is an error rather than a
silent truncation to `1 AND 1`.

**What you see is what you compute with.** Values are reconciled to the
displayed precision before integer conversion, so `asin(0.5)` — a double
that is really `29.999999999999996` — displays as `30`, reads as hex `1E`,
and behaves as `30` in bitwise operations.

**Errors** replace the display with a message (`Error: divide by zero`,
`Error: domain`, `Error: overflow`) and ignore further input until `C` or
`AC`. Domain errors are caught up front, so `sqrt(-1)`, `ln(-1)`, `asin(2)`
and `(-1)!` report rather than producing NaN.

`x!` uses `tgamma(x+1)`, so it also works for non-integers; integer results
are rounded to exact values.

## Layout

- `src/calc.[ch]` — the engine: entry state machine, precedence stack,
  functions, bases, memory, formatting. No I/O, so it is unit-testable.
- `src/ui.c` — terminal handling and drawing: raw mode via `termios`,
  output via ANSI escape sequences, a single button table that drives
  rendering, keyboard shortcuts, focus navigation and mouse hit-testing.
- `src/main.c` — argument parsing.
- `tests/test_calc.c` — engine tests, no terminal required.

`--no-color` (or `NO_COLOR` in the environment) disables colour.
