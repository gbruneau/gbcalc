/* ui.c -- gbcalc terminal UI.
 *
 * Dependency-free: raw mode via termios, drawing via ANSI escape sequences.
 * Layout follows the project spec -- display on top, functions in the
 * middle, number entry at the bottom.
 *
 * No colours or glyphs are hardcoded here; every one comes from theme.c.
 */

#define _POSIX_C_SOURCE 200809L

#include "calc.h"
#include "theme.h"
#include "ui.h"

#include <errno.h>
#include <poll.h>
#include <signal.h>
#include <stdarg.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <sys/ioctl.h>
#include <termios.h>
#include <unistd.h>

#define W_INNER   72		/* usable width inside the display frame */
#define W_FRAME   (W_INNER + 2)
#define H_CONTENT 20		/* fixed number of rendered lines */
#define MIN_COLS  W_FRAME
#define MIN_ROWS  H_CONTENT

#define NFUNC_ROWS 6
#define NPAD_ROWS  6
#define NROWS      (NFUNC_ROWS + NPAD_ROWS)
#define MAX_COLS   6

/* ------------------------------------------------------------- actions */

typedef enum {
	AK_NONE = 0,
	AK_DIGIT, AK_POINT, AK_EE, AK_SIGN, AK_BACKSPACE,
	AK_CLEAR, AK_ALLCLEAR,
	AK_OP, AK_LPAREN, AK_RPAREN, AK_EQUALS,
	AK_UNARY, AK_CONST,
	AK_BASE, AK_DRG, AK_DRG_CONV, AK_INV,
	AK_STO, AK_RCL, AK_MADD, AK_MEXC, AK_MCLR, AK_ANS,
	AK_QUIT, AK_HELP
} ActKind;

typedef struct {
	ActKind k;
	int arg;
} Action;

#define BF_HEXDIGIT 0x01	/* only usable while the base is hexadecimal */

typedef struct {
	const char *label;
	Action act;
	Action inv;		/* AK_NONE => INV does not change this key */
	const char *keys;	/* characters that activate it */
	unsigned flags;
	StyleId cat;		/* function category -- drives its colours */
} Btn;

#define NA {AK_NONE, 0}

/* Functions (middle section). */
static const Btn row_f0[] = {
	{"DRG",  {AK_DRG, 0},        {AK_DRG_CONV, 0},   "d", 0, ST_KEY_MODE},
	{"INV",  {AK_INV, 0},        NA,                 "i", 0, ST_KEY_MODE},
	{"sin",  {AK_UNARY, U_SIN},  {AK_UNARY, U_ASIN}, "s", 0, ST_KEY_SCI},
	{"cos",  {AK_UNARY, U_COS},  {AK_UNARY, U_ACOS}, "c", 0, ST_KEY_SCI},
	{"tan",  {AK_UNARY, U_TAN},  {AK_UNARY, U_ATAN}, "t", 0, ST_KEY_SCI},
	{"x!",   {AK_UNARY, U_FACT}, NA,                 "!", 0, ST_KEY_SCI},
};
static const Btn row_f1[] = {
	{"1/x",  {AK_UNARY, U_RECIP}, NA,                  "v", 0, ST_KEY_SCI},
	{"x^2",  {AK_UNARY, U_SQR},   {AK_UNARY, U_SQRT},  "",  0, ST_KEY_SCI},
	{"sqrt", {AK_UNARY, U_SQRT},  {AK_UNARY, U_SQR},   "r", 0, ST_KEY_SCI},
	{"x^y",  {AK_OP, OP_POW},     {AK_OP, OP_ROOT},    "^", 0, ST_KEY_OPERATOR},
	{"ln",   {AK_UNARY, U_LN},    {AK_UNARY, U_EXP},   "l", 0, ST_KEY_SCI},
	{"log",  {AK_UNARY, U_LOG10}, {AK_UNARY, U_EXP10}, "g", 0, ST_KEY_SCI},
};
static const Btn row_f2[] = {
	{"e^x",  {AK_UNARY, U_EXP},   {AK_UNARY, U_LN},    "",  0, ST_KEY_SCI},
	{"10^x", {AK_UNARY, U_EXP10}, {AK_UNARY, U_LOG10}, "",  0, ST_KEY_SCI},
	{"EE",   {AK_EE, 0},          NA,                  "e", 0, ST_KEY_SCI},
	{"pi",   {AK_CONST, C_PI},    NA,                  "p", 0, ST_KEY_SCI},
	{"e",    {AK_CONST, C_E},     NA,                  "E", 0, ST_KEY_SCI},
	{"%",    {AK_UNARY, U_PCT},   NA,                  "%", 0, ST_KEY_SCI},
};
static const Btn row_f3[] = {
	{"(",    {AK_LPAREN, 0}, NA, "(", 0, ST_KEY_PAREN},
	{")",    {AK_RPAREN, 0}, NA, ")", 0, ST_KEY_PAREN},
	{"STO",  {AK_STO, 0},    NA, "m", 0, ST_KEY_MEMORY},
	{"RCL",  {AK_RCL, 0},    NA, "n", 0, ST_KEY_MEMORY},
	{"M+",   {AK_MADD, 0},   NA, "M", 0, ST_KEY_MEMORY},
	{"MX",   {AK_MEXC, 0},   NA, "X", 0, ST_KEY_MEMORY},
};
static const Btn row_f4[] = {
	{"DEC",  {AK_BASE, BASE_DEC}, NA, "D", 0, ST_KEY_MODE},
	{"HEX",  {AK_BASE, BASE_HEX}, NA, "H", 0, ST_KEY_MODE},
	{"BIN",  {AK_BASE, BASE_BIN}, NA, "B", 0, ST_KEY_MODE},
	{"AND",  {AK_OP, OP_AND},     NA, "&", 0, ST_KEY_BITWISE},
	{"OR",   {AK_OP, OP_OR},      NA, "|", 0, ST_KEY_BITWISE},
	{"XOR",  {AK_OP, OP_XOR},     NA, "#", 0, ST_KEY_BITWISE},
};
static const Btn row_f5[] = {
	{"NOT",  {AK_UNARY, U_NOT}, NA, "~",  0, ST_KEY_BITWISE},
	{"<<",   {AK_OP, OP_SHL},   NA, "<",  0, ST_KEY_BITWISE},
	{">>",   {AK_OP, OP_SHR},   NA, ">",  0, ST_KEY_BITWISE},
	{"MOD",  {AK_OP, OP_MOD},   NA, "\\", 0, ST_KEY_BITWISE},
	{"MC",   {AK_MCLR, 0},      NA, "K",  0, ST_KEY_MEMORY},
	{"ANS",  {AK_ANS, 0},       NA, "A",  0, ST_KEY_MEMORY},
};

/* Number entry (bottom section). */
static const Btn row_n0[] = {
	{"A", {AK_DIGIT, 10}, NA, "aA", BF_HEXDIGIT, ST_KEY_HEXDIGIT},
	{"B", {AK_DIGIT, 11}, NA, "bB", BF_HEXDIGIT, ST_KEY_HEXDIGIT},
	{"C", {AK_DIGIT, 12}, NA, "cC", BF_HEXDIGIT, ST_KEY_HEXDIGIT},
	{"D", {AK_DIGIT, 13}, NA, "dD", BF_HEXDIGIT, ST_KEY_HEXDIGIT},
	{"E", {AK_DIGIT, 14}, NA, "eE", BF_HEXDIGIT, ST_KEY_HEXDIGIT},
	{"F", {AK_DIGIT, 15}, NA, "fF", BF_HEXDIGIT, ST_KEY_HEXDIGIT},
};
static const Btn row_n1[] = {
	{"7", {AK_DIGIT, 7},   NA, "7", 0, ST_KEY_DIGIT},
	{"8", {AK_DIGIT, 8},   NA, "8", 0, ST_KEY_DIGIT},
	{"9", {AK_DIGIT, 9},   NA, "9", 0, ST_KEY_DIGIT},
	{"/", {AK_OP, OP_DIV}, NA, "/", 0, ST_KEY_OPERATOR},
};
static const Btn row_n2[] = {
	{"4", {AK_DIGIT, 4},   NA, "4", 0, ST_KEY_DIGIT},
	{"5", {AK_DIGIT, 5},   NA, "5", 0, ST_KEY_DIGIT},
	{"6", {AK_DIGIT, 6},   NA, "6", 0, ST_KEY_DIGIT},
	{"*", {AK_OP, OP_MUL}, NA, "*", 0, ST_KEY_OPERATOR},
};
static const Btn row_n3[] = {
	{"1", {AK_DIGIT, 1},   NA, "1", 0, ST_KEY_DIGIT},
	{"2", {AK_DIGIT, 2},   NA, "2", 0, ST_KEY_DIGIT},
	{"3", {AK_DIGIT, 3},   NA, "3", 0, ST_KEY_DIGIT},
	{"-", {AK_OP, OP_SUB}, NA, "-", 0, ST_KEY_OPERATOR},
};
static const Btn row_n4[] = {
	{"0",   {AK_DIGIT, 0},   NA, "0", 0, ST_KEY_DIGIT},
	{".",   {AK_POINT, 0},   NA, ".", 0, ST_KEY_DIGIT},
	{"+/-", {AK_SIGN, 0},    NA, "_", 0, ST_KEY_DIGIT},
	{"+",   {AK_OP, OP_ADD}, NA, "+", 0, ST_KEY_OPERATOR},
};
static const Btn row_n5[] = {
	{"C",   {AK_CLEAR, 0},     NA, "",  0, ST_KEY_EDIT},
	{"AC",  {AK_ALLCLEAR, 0},  NA, "",  0, ST_KEY_EDIT},
	{"DEL", {AK_BACKSPACE, 0}, NA, "",  0, ST_KEY_EDIT},
	{"=",   {AK_EQUALS, 0},    NA, "=", 0, ST_KEY_EQUALS},
};

typedef struct {
	const Btn *btns;
	int n;
} Row;

static const Row g_rows[NROWS] = {
	{row_f0, 6}, {row_f1, 6}, {row_f2, 6},
	{row_f3, 6}, {row_f4, 6}, {row_f5, 6},
	{row_n0, 6}, {row_n1, 4}, {row_n2, 4},
	{row_n3, 4}, {row_n4, 4}, {row_n5, 4},
};

/* ------------------------------------------------------------ terminal */

static struct termios g_saved_tio;
static bool g_tio_saved;
static volatile sig_atomic_t g_resized = 1;
static volatile sig_atomic_t g_stop;

static int g_cols = 80, g_lines = 24;
static int g_ox, g_oy;			/* frame origin, 1-based */
static struct { int x, y, w; } g_rect[NROWS][MAX_COLS];

static int g_fr, g_fc = 3;		/* focused row / column ("=" by default) */
static bool g_help;

static void term_restore(void)
{
	if (!g_tio_saved)
		return;
	/* Disable mouse reporting, show cursor, leave the alternate screen. */
	(void)!write(STDOUT_FILENO, "\x1b[?1006l\x1b[?1000l\x1b[?25h"
				    "\x1b[0m\x1b[?1049l", 27);
	tcsetattr(STDIN_FILENO, TCSAFLUSH, &g_saved_tio);
	g_tio_saved = false;
}

static void on_signal(int sig)
{
	if (sig == SIGWINCH) {
		g_resized = 1;
		return;
	}
	g_stop = 1;
}

static bool term_setup(void)
{
	struct termios tio;
	struct sigaction sa;

	if (!isatty(STDIN_FILENO) || !isatty(STDOUT_FILENO)) {
		fprintf(stderr, "gbcalc: stdin/stdout must be a terminal\n");
		return false;
	}
	if (tcgetattr(STDIN_FILENO, &g_saved_tio) != 0) {
		perror("gbcalc: tcgetattr");
		return false;
	}
	tio = g_saved_tio;
	tio.c_iflag &= ~(tcflag_t)(IXON | ICRNL | INLCR | IGNCR | BRKINT | ISTRIP);
	tio.c_lflag &= ~(tcflag_t)(ECHO | ICANON | IEXTEN | ISIG);
	tio.c_oflag &= ~(tcflag_t)OPOST;
	tio.c_cc[VMIN] = 1;
	tio.c_cc[VTIME] = 0;
	if (tcsetattr(STDIN_FILENO, TCSAFLUSH, &tio) != 0) {
		perror("gbcalc: tcsetattr");
		return false;
	}
	g_tio_saved = true;
	atexit(term_restore);

	memset(&sa, 0, sizeof sa);
	sa.sa_handler = on_signal;
	sigaction(SIGINT, &sa, NULL);
	sigaction(SIGTERM, &sa, NULL);
	sigaction(SIGHUP, &sa, NULL);
	sigaction(SIGWINCH, &sa, NULL);

	(void)!write(STDOUT_FILENO, "\x1b[?1049h\x1b[?25l"
				    "\x1b[?1000h\x1b[?1006h", 25);
	return true;
}

static void term_size(void)
{
	struct winsize ws;

	if (ioctl(STDOUT_FILENO, TIOCGWINSZ, &ws) == 0 && ws.ws_col > 0) {
		g_cols = ws.ws_col;
		g_lines = ws.ws_row;
	}
}

/* --------------------------------------------------------- out buffer */

static char *g_buf;
static size_t g_len, g_cap;

static void ob(const char *s, size_t n)
{
	if (g_len + n + 1 > g_cap) {
		size_t cap = g_cap ? g_cap * 2 : 8192;
		char *p;

		while (cap < g_len + n + 1)
			cap *= 2;
		p = realloc(g_buf, cap);
		if (!p)
			return;
		g_buf = p;
		g_cap = cap;
	}
	memcpy(g_buf + g_len, s, n);
	g_len += n;
	g_buf[g_len] = '\0';
}

static void os(const char *s)
{
	ob(s, strlen(s));
}

static void of(const char *fmt, ...)
{
	char tmp[512];
	va_list ap;
	int n;

	va_start(ap, fmt);
	n = vsnprintf(tmp, sizeof tmp, fmt, ap);
	va_end(ap);
	if (n > 0)
		ob(tmp, (size_t)n < sizeof tmp ? (size_t)n : sizeof tmp - 1);
}

static void at(int y, int x)
{
	of("\x1b[%d;%dH", y, x);
}

/* Select a themed style, or return to plain text. */
static void style(StyleId id, StyleState st)
{
	os(theme_sgr(id, st));
}

static void style_off(void)
{
	os(theme_sgr_reset());
}

static void repeat(const char *s, int n)
{
	for (int i = 0; i < n; i++)
		os(s);
}

/* ----------------------------------------------------------- rendering */

static void layout(void)
{
	int y;

	term_size();
	g_ox = (g_cols - W_FRAME) / 2 + 1;
	if (g_ox < 1)
		g_ox = 1;
	g_oy = (g_lines - H_CONTENT) / 2 + 1;
	if (g_oy < 1)
		g_oy = 1;

	/* Rows start after the frame (5 lines) plus a blank separator. */
	y = g_oy + 6;
	for (int r = 0; r < NROWS; r++) {
		int n = g_rows[r].n;
		int w = W_INNER / n;
		int pad = (W_INNER - w * n) / 2;

		if (r == NFUNC_ROWS)
			y++;		/* blank line between the two sections */
		for (int i = 0; i < n; i++) {
			g_rect[r][i].x = g_ox + 1 + pad + i * w;
			g_rect[r][i].y = y;
			g_rect[r][i].w = w;
		}
		y++;
	}
}

/* Is this button showing an engine mode that is currently on? */
static bool btn_active(const Calc *c, const Btn *b)
{
	switch (b->act.k) {
	case AK_BASE: return (int)c->base == b->act.arg;
	case AK_INV:  return c->inv;
	default:      return false;
	}
}

static bool btn_enabled(const Calc *c, const Btn *b)
{
	if (b->flags & BF_HEXDIGIT)
		return c->base == BASE_HEX;
	if (b->act.k == AK_POINT || b->act.k == AK_EE)
		return c->base == BASE_DEC;
	if (b->act.k == AK_DIGIT)
		return b->act.arg < (int)c->base;
	return true;
}

static void draw_btn(const Calc *c, int r, int i)
{
	const Btn *b = &g_rows[r].btns[i];
	const ThemeChrome *ch = theme_chrome();
	int lb = ch->key_left[0] ? 1 : 0;
	int rb = ch->key_right[0] ? 1 : 0;
	int inner = g_rect[r][i].w - lb - rb;
	int len = (int)strlen(b->label);
	int lpad, rpad;
	StyleState st;

	if (r == g_fr && i == g_fc)
		st = SS_FOCUS;			/* focus outranks the rest */
	else if (!btn_enabled(c, b))
		st = SS_DISABLED;
	else if (btn_active(c, b))
		st = SS_ACTIVE;
	else
		st = SS_NORMAL;

	if (len > inner)
		len = inner;
	lpad = (inner - len) / 2;
	rpad = inner - len - lpad;

	at(g_rect[r][i].y, g_rect[r][i].x);
	style(b->cat, st);
	os(ch->key_left);
	repeat(" ", lpad);
	ob(b->label, (size_t)len);
	repeat(" ", rpad);
	os(ch->key_right);
	style_off();
}

/* Right-align `s` in a field of `w`, clipping the front if it is too long. */
static void draw_right(const char *s, int w)
{
	int len = (int)strlen(s);

	if (len >= w) {
		os("..");
		os(s + len - (w - 2));
		return;
	}
	repeat(" ", w - len);
	os(s);
}

/* Every frame line is exactly W_INNER columns between the two verticals. */
static void draw_frame(const Calc *c)
{
	const ThemeChrome *ch = theme_chrome();
	char disp[160], a[96], b[96], aux[256];
	Op pend = calc_pending_op(c);
	int tlen = (int)strlen(ch->title);
	int used;

	/* Top border, with the title inlaid. */
	at(g_oy, g_ox);
	style(ST_FRAME, SS_NORMAL);
	os(ch->tl);
	if (tlen > 0 && tlen < W_INNER - 4) {
		os(ch->h);
		os(" ");
		style(ST_TITLE, SS_NORMAL);
		os(ch->title);
		style(ST_FRAME, SS_NORMAL);
		os(" ");
		repeat(ch->h, W_INNER - 3 - tlen);
	} else {
		repeat(ch->h, W_INNER);
	}
	os(ch->tr);
	style_off();

	/* Status line: angle unit, base, INV latch, pending operator, depth. */
	at(g_oy + 1, g_ox);
	style(ST_FRAME, SS_NORMAL);
	os(ch->v);
	style_off();
	os(" ");
	used = 0;

	style(ST_STATUS_MODE, SS_NORMAL);
	of("%s  %s", calc_angle_name(c), calc_base_name(c));
	used += (int)strlen(calc_angle_name(c)) + 2 +
		(int)strlen(calc_base_name(c));
	if (c->inv) {
		style(ST_STATUS_INV, SS_NORMAL);
		os("  INV");
		used += 5;
	}
	if (pend != OP_NONE) {
		style(ST_STATUS, SS_NORMAL);
		of("  %s", calc_op_symbol(pend));
		used += 2 + (int)strlen(calc_op_symbol(pend));
	}
	if (c->paren_depth > 0) {
		char depth[16];
		int n = snprintf(depth, sizeof depth, "  (%d", c->paren_depth);

		style(ST_STATUS, SS_NORMAL);
		os(depth);
		used += n;
	}
	style_off();
	if (used > W_INNER - 3)
		used = W_INNER - 3;
	repeat(" ", W_INNER - 3 - used);

	style(ST_STATUS_MEM, SS_NORMAL);
	os(c->mem_set ? "M" : " ");
	style_off();
	os(" ");
	style(ST_FRAME, SS_NORMAL);
	os(ch->v);
	style_off();

	/* Main display. */
	calc_display(c, disp, sizeof disp);
	at(g_oy + 2, g_ox);
	style(ST_FRAME, SS_NORMAL);
	os(ch->v);
	style(c->error ? ST_DISPLAY_ERROR : ST_DISPLAY, SS_NORMAL);
	os(" ");
	draw_right(disp, W_INNER - 2);
	os(" ");
	style(ST_FRAME, SS_NORMAL);
	os(ch->v);
	style_off();

	/* The current value in the two bases we are not in. */
	if (c->error) {
		aux[0] = '\0';
	} else if (c->base == BASE_DEC) {
		calc_render_base(c, BASE_HEX, a, sizeof a);
		calc_render_base(c, BASE_BIN, b, sizeof b);
		snprintf(aux, sizeof aux, "hex %s   bin %s", a, b);
	} else if (c->base == BASE_HEX) {
		calc_render_base(c, BASE_DEC, a, sizeof a);
		calc_render_base(c, BASE_BIN, b, sizeof b);
		snprintf(aux, sizeof aux, "dec %s   bin %s", a, b);
	} else {
		calc_render_base(c, BASE_DEC, a, sizeof a);
		calc_render_base(c, BASE_HEX, b, sizeof b);
		snprintf(aux, sizeof aux, "dec %s   hex %s", a, b);
	}
	if ((int)strlen(aux) > W_INNER - 2)
		aux[W_INNER - 2] = '\0';

	at(g_oy + 3, g_ox);
	style(ST_FRAME, SS_NORMAL);
	os(ch->v);
	style(ST_AUX, SS_NORMAL);
	of(" %-*s ", W_INNER - 2, aux);
	style(ST_FRAME, SS_NORMAL);
	os(ch->v);
	style_off();

	/* Bottom border. */
	at(g_oy + 4, g_ox);
	style(ST_FRAME, SS_NORMAL);
	os(ch->bl);
	repeat(ch->h, W_INNER);
	os(ch->br);
	style_off();
}

static void draw_hint(void)
{
	static const char hint[] =
		"arrows move  enter press  tab base  esc C  bksp DEL  ? help  q quit";
	int len = (int)(sizeof hint - 1);
	int pad = (W_INNER - len) / 2;

	at(g_oy + H_CONTENT - 1, g_ox + 1 + (pad > 0 ? pad : 0));
	style(ST_HINT, SS_NORMAL);
	os(hint);
	style_off();
}

static void draw_help(void)
{
	static const char *lines[] = {
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
		"  Esc            C (clear entry)   Delete  AC (all clear)",
		"  Backspace      DEL               Ctrl-L  redraw",
		"  arrows + Enter operate every button; mouse clicks work too",
		"",
		"  ?  close help          q / Ctrl-C  quit",
		NULL
	};
	int n = 0, top, left;

	while (lines[n])
		n++;
	top = (g_lines - n - 2) / 2 + 1;
	if (top < 1)
		top = 1;
	left = (g_cols - 70) / 2 + 1;
	if (left < 1)
		left = 1;

	for (int i = 0; i < n && top + i <= g_lines; i++) {
		at(top + i, left);
		style(i == 0 ? ST_HELP_TITLE : ST_HELP_TEXT, SS_NORMAL);
		os(lines[i]);
		style_off();
	}

	at(top + n + 1, left);
	style(ST_HINT, SS_NORMAL);
	of("  theme: %s", theme_name());
	style_off();
}

static void render(const Calc *c)
{
	g_len = 0;
	os(theme_sgr_reset());
	os("\x1b[2J");

	if (g_cols < MIN_COLS || g_lines < MIN_ROWS) {
		at(1, 1);
		of("gbcalc needs at least %dx%d, terminal is %dx%d",
		   MIN_COLS, MIN_ROWS, g_cols, g_lines);
		at(2, 1);
		os("resize the window, or press q to quit");
	} else if (g_help) {
		draw_help();
	} else {
		draw_frame(c);
		for (int r = 0; r < NROWS; r++)
			for (int i = 0; i < g_rows[r].n; i++)
				draw_btn(c, r, i);
		draw_hint();
	}

	at(g_lines, g_cols);
	if (g_len > 0)
		(void)!write(STDOUT_FILENO, g_buf, g_len);
}

/* --------------------------------------------------------------- input */

enum {
	KEY_NONE = -1,
	KEY_UP = 0x100, KEY_DOWN, KEY_LEFT, KEY_RIGHT,
	KEY_HOME, KEY_END, KEY_DELETE, KEY_MOUSE
};

static int g_mx, g_my;

/* One byte of lookahead, so a byte read while probing an escape sequence
 * that turns out not to be one is not lost (e.g. Esc followed by Tab). */
static int g_pending = KEY_NONE;

/* True if another byte is available within `ms` milliseconds. */
static bool ready(int ms)
{
	struct pollfd p = { STDIN_FILENO, POLLIN, 0 };

	if (g_pending != KEY_NONE)
		return true;
	return poll(&p, 1, ms) > 0;
}

static int read_byte(void)
{
	unsigned char ch;
	ssize_t n;

	if (g_pending != KEY_NONE) {
		int c = g_pending;

		g_pending = KEY_NONE;
		return c;
	}
	n = read(STDIN_FILENO, &ch, 1);
	if (n == 1)
		return ch;
	return KEY_NONE;
}

static void unread_byte(int ch)
{
	if (ch != KEY_NONE)
		g_pending = ch;
}

/* Decode "\x1b[<b;x;yM" / "...m" SGR mouse reports. */
static int read_mouse(void)
{
	char buf[32];
	size_t i = 0;
	int b, x, y;

	while (i < sizeof buf - 1) {
		int ch;

		if (!ready(50))
			return KEY_NONE;
		ch = read_byte();
		if (ch == KEY_NONE)
			return KEY_NONE;
		if (ch == 'M' || ch == 'm') {
			buf[i] = '\0';
			if (ch == 'm')
				return KEY_NONE;	/* ignore releases */
			if (sscanf(buf, "%d;%d;%d", &b, &x, &y) == 3 &&
			    (b & 0x43) == 0) {		/* plain left press */
				g_mx = x;
				g_my = y;
				return KEY_MOUSE;
			}
			return KEY_NONE;
		}
		buf[i++] = (char)ch;
	}
	return KEY_NONE;
}

static int read_key(void)
{
	int ch = read_byte();

	if (ch != 0x1b)
		return ch;
	if (!ready(30))
		return 0x1b;			/* bare Esc */
	ch = read_byte();
	if (ch != '[' && ch != 'O') {
		unread_byte(ch);	/* not a sequence: Esc, then that key */
		return 0x1b;
	}
	if (!ready(30))
		return 0x1b;
	ch = read_byte();
	switch (ch) {
	case 'A': return KEY_UP;
	case 'B': return KEY_DOWN;
	case 'C': return KEY_RIGHT;
	case 'D': return KEY_LEFT;
	case 'H': return KEY_HOME;
	case 'F': return KEY_END;
	case '<': return read_mouse();
	case '3':
		if (ready(30)) {
			int t = read_byte();

			if (t == '~')
				return KEY_DELETE;
			unread_byte(t);
		}
		return KEY_NONE;
	default:
		/* Swallow the rest of any unrecognised CSI sequence. */
		while (ch >= '0' && ch <= '?' && ready(10))
			ch = read_byte();
		return KEY_NONE;
	}
}

/* ---------------------------------------------------------- navigation */

static void focus_clamp(void)
{
	if (g_fr < 0)
		g_fr = 0;
	if (g_fr >= NROWS)
		g_fr = NROWS - 1;
	if (g_fc < 0)
		g_fc = 0;
	if (g_fc >= g_rows[g_fr].n)
		g_fc = g_rows[g_fr].n - 1;
}

/* Move to `row`, keeping the button nearest the current horizontal centre. */
static void focus_row(int row)
{
	int cx, best = 0, bestd = 1 << 30;

	if (row < 0)
		row = NROWS - 1;
	if (row >= NROWS)
		row = 0;

	cx = g_rect[g_fr][g_fc].x + g_rect[g_fr][g_fc].w / 2;
	for (int i = 0; i < g_rows[row].n; i++) {
		int d = g_rect[row][i].x + g_rect[row][i].w / 2 - cx;

		if (d < 0)
			d = -d;
		if (d < bestd) {
			bestd = d;
			best = i;
		}
	}
	g_fr = row;
	g_fc = best;
}

static bool hit_test(int x, int y, int *row, int *col)
{
	for (int r = 0; r < NROWS; r++)
		for (int i = 0; i < g_rows[r].n; i++)
			if (y == g_rect[r][i].y && x >= g_rect[r][i].x &&
			    x < g_rect[r][i].x + g_rect[r][i].w) {
				*row = r;
				*col = i;
				return true;
			}
	return false;
}

/* ---------------------------------------------------------- activation */

enum { ACT_OK = 0, ACT_QUIT, ACT_HELP };

static int run_action(Calc *c, Action a)
{
	switch (a.k) {
	case AK_DIGIT:     calc_digit(c, a.arg); break;
	case AK_POINT:     calc_point(c); break;
	case AK_EE:        calc_ee(c); break;
	case AK_SIGN:      calc_sign(c); break;
	case AK_BACKSPACE: calc_backspace(c); break;
	case AK_CLEAR:     calc_clear_entry(c); break;
	case AK_ALLCLEAR:  calc_all_clear(c); break;
	case AK_OP:        calc_op(c, (Op)a.arg); break;
	case AK_LPAREN:    calc_lparen(c); break;
	case AK_RPAREN:    calc_rparen(c); break;
	case AK_EQUALS:    calc_equals(c); break;
	case AK_UNARY:     calc_unary(c, (Unary)a.arg); break;
	case AK_CONST:     calc_const(c, (Konst)a.arg); break;
	case AK_BASE:      calc_set_base(c, (Base)a.arg); break;
	case AK_DRG:       calc_cycle_angle(c); break;
	case AK_DRG_CONV:  calc_convert_angle(c); break;
	case AK_INV:       calc_toggle_inv(c); break;
	case AK_STO:       calc_mem_store(c); break;
	case AK_RCL:       calc_mem_recall(c); break;
	case AK_MADD:      calc_mem_add(c); break;
	case AK_MEXC:      calc_mem_exchange(c); break;
	case AK_MCLR:      calc_mem_clear(c); break;
	case AK_ANS:       calc_recall_ans(c); break;
	case AK_QUIT:      return ACT_QUIT;
	case AK_HELP:      return ACT_HELP;
	default:           break;
	}
	return ACT_OK;
}

static int press(Calc *c, const Btn *b)
{
	Action a = b->act;
	bool use_inv = c->inv && b->inv.k != AK_NONE;
	int rc;

	if (!btn_enabled(c, b))
		return ACT_OK;
	if (use_inv)
		a = b->inv;

	rc = run_action(c, a);

	/* INV is a one-shot latch, but pressing INV itself must not clear it. */
	if (b->act.k != AK_INV)
		c->inv = false;
	return rc;
}

/* Find the button bound to `ch`, honouring base-dependent bindings. */
static const Btn *find_key(const Calc *c, int ch)
{
	int hexdigit = -1;

	if (ch < 0 || ch > 0xff)
		return NULL;

	/* In HEX mode the letters a-f are digits and outrank function keys. */
	if (c->base == BASE_HEX) {
		if (ch >= 'a' && ch <= 'f')
			hexdigit = ch - 'a' + 10;
		else if (ch >= 'A' && ch <= 'F')
			hexdigit = ch - 'A' + 10;
		if (hexdigit >= 0)
			return &row_n0[hexdigit - 10];
	}

	for (int r = 0; r < NROWS; r++)
		for (int i = 0; i < g_rows[r].n; i++) {
			const Btn *b = &g_rows[r].btns[i];

			if (b->flags & BF_HEXDIGIT)
				continue;	/* only reachable in HEX mode */
			if (b->keys[0] && strchr(b->keys, ch))
				return b;
		}
	return NULL;
}

/* ----------------------------------------------------------------- run */

int ui_run(void)
{
	Calc calc;
	bool quit = false;

	calc_init(&calc);
	if (!term_setup())
		return 1;

	/* Start focus on "=" so Enter evaluates straight away. */
	g_fr = NROWS - 1;
	g_fc = 3;

	while (!quit && !g_stop) {
		int key;

		if (g_resized) {
			g_resized = 0;
			layout();
			focus_clamp();
		}
		render(&calc);

		if (!ready(-1)) {
			if (errno == EINTR)
				continue;
			break;
		}
		key = read_key();

		switch (key) {
		case KEY_NONE:
			continue;
		case KEY_UP:    focus_row(g_fr - 1); continue;
		case KEY_DOWN:  focus_row(g_fr + 1); continue;
		case KEY_LEFT:
			g_fc = (g_fc - 1 + g_rows[g_fr].n) % g_rows[g_fr].n;
			continue;
		case KEY_RIGHT:
			g_fc = (g_fc + 1) % g_rows[g_fr].n;
			continue;
		case KEY_HOME:  g_fc = 0; continue;
		case KEY_END:   g_fc = g_rows[g_fr].n - 1; continue;
		case KEY_DELETE:
			calc_all_clear(&calc);
			continue;
		case KEY_MOUSE: {
			int r, i;

			if (g_help) {
				g_help = false;
			} else if (hit_test(g_mx, g_my, &r, &i)) {
				g_fr = r;
				g_fc = i;
				if (press(&calc, &g_rows[r].btns[i]) == ACT_QUIT)
					quit = true;
			}
			continue;
		}
		default:
			break;
		}

		if (g_help) {
			/* Any key closes help, except quit which still quits. */
			if (key == 'q' || key == 3)
				quit = true;
			else
				g_help = false;
			continue;
		}

		switch (key) {
		case 'q':
		case 3:				/* Ctrl-C */
		case 4:				/* Ctrl-D */
			quit = true;
			continue;
		case '?':
			g_help = true;
			continue;
		case 12:			/* Ctrl-L */
			g_resized = 1;
			continue;
		case 0x1b:
			calc_clear_entry(&calc);
			continue;
		case 127:
		case 8:
			calc_backspace(&calc);
			continue;
		case '\t':
			calc_cycle_base(&calc);
			continue;
		case '\r':
		case '\n':
		case ' ':
			if (press(&calc, &g_rows[g_fr].btns[g_fc]) == ACT_QUIT)
				quit = true;
			continue;
		default:
			break;
		}

		{
			const Btn *b = find_key(&calc, key);

			if (b && press(&calc, b) == ACT_QUIT)
				quit = true;
		}
	}

	term_restore();
	free(g_buf);
	return 0;
}
