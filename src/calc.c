/* calc.c -- gbcalc calculator engine. */

#include "calc.h"

#include <ctype.h>
#include <math.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

#define PI_D 3.14159265358979323846
#define E_D  2.71828182845904523536

/* Largest magnitude that survives a round trip through int64_t. */
#define I64_LIMIT 9.2233720368547758e18

/* Maximum number of significant digits accepted per base. */
static int max_digits(Base b)
{
	switch (b) {
	case BASE_HEX: return 16;
	case BASE_BIN: return 64;
	default:       return 18;
	}
}

void calc_init(Calc *c)
{
	memset(c, 0, sizeof *c);
	c->base = BASE_DEC;
	c->angle = ANG_DEG;
}

static void fail(Calc *c, const char *msg)
{
	c->error = true;
	c->errmsg = msg;
	c->entering = false;
	c->entry[0] = '\0';
	c->sp = 0;
	c->paren_depth = 0;
	c->value_ready = false;
	c->op_pending = false;
}

/* Reject non-finite results as soon as they appear. */
static double guard(Calc *c, double v, const char *msg)
{
	if (isnan(v)) {
		fail(c, msg ? msg : "Error: undefined");
		return 0.0;
	}
	if (isinf(v)) {
		fail(c, "Error: overflow");
		return 0.0;
	}
	return v;
}

static int digit_of(char ch)
{
	if (ch >= '0' && ch <= '9')
		return ch - '0';
	if (ch >= 'a' && ch <= 'f')
		return ch - 'a' + 10;
	if (ch >= 'A' && ch <= 'F')
		return ch - 'A' + 10;
	return -1;
}

/* Parse the entry buffer according to the active base. */
static double entry_value(const Calc *c)
{
	const char *s = c->entry;
	uint64_t u = 0;
	int neg = 0;

	if (*s == '\0')
		return 0.0;
	if (c->base == BASE_DEC)
		return strtod(s, NULL);

	if (*s == '-') {
		neg = 1;
		s++;
	}
	for (; *s; s++) {
		int d = digit_of(*s);
		if (d < 0 || d >= (int)c->base)
			continue;
		u = u * (unsigned)c->base + (unsigned)d;
	}
	return neg ? -(double)u : (double)u;
}

double calc_current(const Calc *c)
{
	return c->entering ? entry_value(c) : c->acc;
}

/* Consume the pending entry and return it as the current operand. */
static double take(Calc *c)
{
	double v = calc_current(c);

	c->acc = v;
	c->entering = false;
	c->entry[0] = '\0';
	c->entry_has_exp = false;
	return v;
}

/* Round to the precision the display shows, so a value that reads as "30"
 * behaves like 30 even when the double is 29.999999999999996 (as trig
 * results tend to be). Genuine fractions such as 1.5 are unaffected. */
static double snap_display(double v)
{
	char buf[64];

	if (!isfinite(v))
		return v;
	if (v == trunc(v) && fabs(v) < 1e15)
		return v;
	snprintf(buf, sizeof buf, "%.12g", v);
	return strtod(buf, NULL);
}

/* Bitwise operands must be exact integers -- silently truncating a
 * fraction here would hide a mistake rather than report it. */
static int64_t to_i64(Calc *c, double v)
{
	v = snap_display(v);
	if (!isfinite(v) || v != trunc(v) || fabs(v) >= I64_LIMIT) {
		fail(c, "Error: not an integer");
		return 0;
	}
	return (int64_t)v;
}

/* ---------------------------------------------------------------- entry */

/* Count significant digits already typed (ignores sign, point, exponent). */
static int count_digits(const Calc *c)
{
	int n = 0;
	for (const char *s = c->entry; *s; s++) {
		if (*s == 'e' || *s == 'E')
			break;
		if (digit_of(*s) >= 0)
			n++;
	}
	return n;
}

static void entry_push(Calc *c, char ch)
{
	size_t len = strlen(c->entry);

	if (len >= CALC_ENTRY_MAX)
		return;
	c->entry[len] = ch;
	c->entry[len + 1] = '\0';
}

/* Begin a fresh entry unless one is already in progress. */
static void entry_begin(Calc *c)
{
	if (!c->entering) {
		c->entry[0] = '\0';
		c->entry_has_exp = false;
		c->entering = true;
	}
}

void calc_digit(Calc *c, int d)
{
	static const char sym[] = "0123456789ABCDEF";

	if (c->error || d < 0 || d >= (int)c->base)
		return;

	entry_begin(c);

	/* Exponent digits are capped at two, mantissa digits at the base limit. */
	if (c->entry_has_exp) {
		const char *e = strpbrk(c->entry, "eE");
		int n = 0;
		for (const char *s = e + 1; *s; s++)
			if (digit_of(*s) >= 0)
				n++;
		if (n >= 2)
			return;
	} else if (count_digits(c) >= max_digits(c->base)) {
		return;
	}

	/* Replace a lone leading zero rather than accumulating "000". */
	if (!c->entry_has_exp && strcmp(c->entry, "0") == 0)
		c->entry[0] = '\0';
	else if (!c->entry_has_exp && strcmp(c->entry, "-0") == 0)
		c->entry[1] = '\0';

	entry_push(c, sym[d]);
	c->value_ready = true;
	c->op_pending = false;
}

void calc_point(Calc *c)
{
	if (c->error || c->base != BASE_DEC)
		return;

	entry_begin(c);
	if (c->entry_has_exp || strchr(c->entry, '.'))
		return;
	if (c->entry[0] == '\0' || strcmp(c->entry, "-") == 0)
		entry_push(c, '0');
	entry_push(c, '.');
	c->value_ready = true;
	c->op_pending = false;
}

void calc_ee(Calc *c)
{
	if (c->error || c->base != BASE_DEC)
		return;

	entry_begin(c);
	if (c->entry_has_exp)
		return;
	if (c->entry[0] == '\0' || strcmp(c->entry, "-") == 0)
		entry_push(c, '1');
	entry_push(c, 'e');
	c->entry_has_exp = true;
	c->value_ready = true;
	c->op_pending = false;
}

void calc_sign(Calc *c)
{
	if (c->error)
		return;

	if (c->entering && c->entry_has_exp) {
		/* Toggle the exponent sign in place. */
		char *e = strpbrk(c->entry, "eE");
		size_t at = (size_t)(e - c->entry) + 1;

		if (c->entry[at] == '-') {
			memmove(c->entry + at, c->entry + at + 1,
				strlen(c->entry + at + 1) + 1);
		} else if (strlen(c->entry) < CALC_ENTRY_MAX) {
			memmove(c->entry + at + 1, c->entry + at,
				strlen(c->entry + at) + 1);
			c->entry[at] = '-';
		}
		return;
	}

	if (c->entering) {
		if (c->entry[0] == '-') {
			memmove(c->entry, c->entry + 1, strlen(c->entry));
		} else if (strlen(c->entry) < CALC_ENTRY_MAX) {
			memmove(c->entry + 1, c->entry, strlen(c->entry) + 1);
			c->entry[0] = '-';
		}
		return;
	}

	c->acc = -c->acc;
	c->value_ready = true;
}

void calc_backspace(Calc *c)
{
	size_t len;

	if (c->error) {
		calc_all_clear(c);
		return;
	}
	if (!c->entering) {
		c->acc = 0.0;
		c->value_ready = false;
		return;
	}

	len = strlen(c->entry);
	if (len == 0) {
		c->entering = false;
		c->acc = 0.0;
		return;
	}
	if (c->entry[len - 1] == 'e' || c->entry[len - 1] == 'E')
		c->entry_has_exp = false;
	c->entry[len - 1] = '\0';
	if (c->entry[0] == '\0' || strcmp(c->entry, "-") == 0) {
		c->entry[0] = '\0';
		c->entering = false;
		c->acc = 0.0;
	}
}

void calc_clear_entry(Calc *c)
{
	c->error = false;
	c->errmsg = NULL;
	c->entry[0] = '\0';
	c->entering = false;
	c->entry_has_exp = false;
	c->acc = 0.0;
	c->value_ready = false;
}

void calc_all_clear(Calc *c)
{
	calc_clear_entry(c);
	c->sp = 0;
	c->paren_depth = 0;
	c->op_pending = false;
	c->inv = false;
}

/* ------------------------------------------------------------ operators */

static int prec(Op op)
{
	switch (op) {
	case OP_LPAREN: return 0;
	case OP_OR:     return 1;
	case OP_XOR:    return 2;
	case OP_AND:    return 3;
	case OP_SHL:
	case OP_SHR:    return 4;
	case OP_ADD:
	case OP_SUB:    return 5;
	case OP_MUL:
	case OP_DIV:
	case OP_MOD:    return 6;
	case OP_POW:
	case OP_ROOT:   return 7;
	default:        return 0;
	}
}

static bool right_assoc(Op op)
{
	return op == OP_POW || op == OP_ROOT;
}

static double apply(Calc *c, double a, Op op, double b)
{
	int64_t ia, ib;
	unsigned sh;

	switch (op) {
	case OP_ADD: return guard(c, a + b, NULL);
	case OP_SUB: return guard(c, a - b, NULL);
	case OP_MUL: return guard(c, a * b, NULL);
	case OP_DIV:
		if (b == 0.0) {
			fail(c, "Error: divide by zero");
			return 0.0;
		}
		return guard(c, a / b, NULL);
	case OP_MOD:
		if (b == 0.0) {
			fail(c, "Error: divide by zero");
			return 0.0;
		}
		return guard(c, fmod(a, b), NULL);
	case OP_POW:
		return guard(c, pow(a, b), "Error: domain");
	case OP_ROOT:
		if (b == 0.0) {
			fail(c, "Error: domain");
			return 0.0;
		}
		return guard(c, pow(a, 1.0 / b), "Error: domain");
	case OP_AND:
	case OP_OR:
	case OP_XOR:
	case OP_SHL:
	case OP_SHR:
		ia = to_i64(c, a);
		ib = to_i64(c, b);
		if (c->error)
			return 0.0;
		if (op == OP_AND)
			return (double)(ia & ib);
		if (op == OP_OR)
			return (double)(ia | ib);
		if (op == OP_XOR)
			return (double)(ia ^ ib);
		if (ib < 0 || ib > 63) {
			fail(c, "Error: shift count");
			return 0.0;
		}
		sh = (unsigned)ib;
		if (op == OP_SHL)
			return (double)(int64_t)((uint64_t)ia << sh);
		return (double)(int64_t)((uint64_t)ia >> sh);
	default:
		return b;
	}
}

/* Collapse pending operators that bind at least as tightly as `op`. */
static double reduce(Calc *c, double rhs, Op op)
{
	while (c->sp > 0 && c->stack[c->sp - 1].op != OP_LPAREN) {
		Op top = c->stack[c->sp - 1].op;

		if (prec(top) < prec(op))
			break;
		if (prec(top) == prec(op) && right_assoc(op))
			break;
		c->sp--;
		rhs = apply(c, c->stack[c->sp].value, top, rhs);
		if (c->error)
			return 0.0;
	}
	return rhs;
}

void calc_op(Calc *c, Op op)
{
	double rhs;

	if (c->error || op == OP_NONE || op == OP_LPAREN)
		return;

	/* Two operators in a row: the second one replaces the first. */
	if (c->op_pending && c->sp > 0 && c->stack[c->sp - 1].op != OP_LPAREN) {
		c->sp--;
		rhs = c->stack[c->sp].value;
	} else {
		rhs = take(c);
	}

	rhs = reduce(c, rhs, op);
	if (c->error)
		return;

	if (c->sp >= CALC_STACK_MAX) {
		fail(c, "Error: too deep");
		return;
	}
	c->stack[c->sp].value = rhs;
	c->stack[c->sp].op = op;
	c->sp++;

	c->acc = rhs;
	c->entering = false;
	c->entry[0] = '\0';
	c->entry_has_exp = false;
	c->op_pending = true;
	c->value_ready = false;
}

void calc_lparen(Calc *c)
{
	if (c->error)
		return;

	/* "2(" reads as "2*(" rather than silently dropping the 2. */
	if (c->value_ready && !c->op_pending)
		calc_op(c, OP_MUL);
	if (c->error)
		return;

	if (c->sp >= CALC_STACK_MAX) {
		fail(c, "Error: too deep");
		return;
	}
	c->stack[c->sp].value = 0.0;
	c->stack[c->sp].op = OP_LPAREN;
	c->sp++;
	c->paren_depth++;

	c->entering = false;
	c->entry[0] = '\0';
	c->acc = 0.0;
	c->op_pending = false;
	c->value_ready = false;
}

void calc_rparen(Calc *c)
{
	double v;

	if (c->error || c->paren_depth == 0)
		return;

	v = c->op_pending && c->sp > 0 && c->stack[c->sp - 1].op != OP_LPAREN
		? c->stack[--c->sp].value	/* dangling operator: drop it */
		: take(c);

	v = reduce(c, v, OP_LPAREN);
	if (c->error)
		return;
	if (c->sp > 0 && c->stack[c->sp - 1].op == OP_LPAREN)
		c->sp--;
	c->paren_depth--;

	c->acc = v;
	c->entering = false;
	c->entry[0] = '\0';
	c->op_pending = false;
	c->value_ready = true;
}

void calc_equals(Calc *c)
{
	double v;

	if (c->error)
		return;

	v = c->op_pending && c->sp > 0 && c->stack[c->sp - 1].op != OP_LPAREN
		? c->stack[--c->sp].value
		: take(c);

	/* Close any open parentheses implicitly, then drain the stack. */
	while (c->sp > 0) {
		v = reduce(c, v, OP_LPAREN);
		if (c->error)
			return;
		if (c->sp > 0 && c->stack[c->sp - 1].op == OP_LPAREN)
			c->sp--;
	}

	c->paren_depth = 0;
	c->acc = v;
	c->last_ans = v;
	c->entering = false;
	c->entry[0] = '\0';
	c->op_pending = false;
	c->value_ready = false;	/* a following "(" starts a new expression */
}

/* ---------------------------------------------------------------- unary */

static double to_rad(const Calc *c, double x)
{
	switch (c->angle) {
	case ANG_DEG:  return x * PI_D / 180.0;
	case ANG_GRAD: return x * PI_D / 200.0;
	default:       return x;
	}
}

static double from_rad(const Calc *c, double x)
{
	switch (c->angle) {
	case ANG_DEG:  return x * 180.0 / PI_D;
	case ANG_GRAD: return x * 200.0 / PI_D;
	default:       return x;
	}
}

void calc_unary(Calc *c, Unary u)
{
	double x, r;
	int64_t i;

	if (c->error)
		return;

	x = take(c);

	switch (u) {
	case U_SIN:   r = sin(to_rad(c, x)); break;
	case U_COS:   r = cos(to_rad(c, x)); break;
	case U_TAN:   r = tan(to_rad(c, x)); break;
	case U_ASIN:
		if (x < -1.0 || x > 1.0) {
			fail(c, "Error: domain");
			return;
		}
		r = from_rad(c, asin(x));
		break;
	case U_ACOS:
		if (x < -1.0 || x > 1.0) {
			fail(c, "Error: domain");
			return;
		}
		r = from_rad(c, acos(x));
		break;
	case U_ATAN:  r = from_rad(c, atan(x)); break;
	case U_LN:
		if (x <= 0.0) {
			fail(c, "Error: domain");
			return;
		}
		r = log(x);
		break;
	case U_LOG10:
		if (x <= 0.0) {
			fail(c, "Error: domain");
			return;
		}
		r = log10(x);
		break;
	case U_EXP:   r = exp(x); break;
	case U_EXP10: r = pow(10.0, x); break;
	case U_SQRT:
		if (x < 0.0) {
			fail(c, "Error: domain");
			return;
		}
		r = sqrt(x);
		break;
	case U_SQR:   r = x * x; break;
	case U_RECIP:
		if (x == 0.0) {
			fail(c, "Error: divide by zero");
			return;
		}
		r = 1.0 / x;
		break;
	case U_FACT:
		/* tgamma(x+1) extends x! to non-integers; poles at negative ints. */
		if (x < 0.0 && x == floor(x)) {
			fail(c, "Error: domain");
			return;
		}
		if (x > 170.0) {
			fail(c, "Error: overflow");
			return;
		}
		r = tgamma(x + 1.0);
		if (x >= 0.0 && x == floor(x))
			r = round(r);
		break;
	case U_NOT:
		i = to_i64(c, x);
		if (c->error)
			return;
		r = (double)(~i);
		break;
	case U_PCT:   r = x / 100.0; break;
	default:      r = x; break;
	}

	r = guard(c, r, "Error: domain");
	if (c->error)
		return;
	c->acc = r;
	c->value_ready = true;
	c->op_pending = false;
}

void calc_const(Calc *c, Konst k)
{
	if (c->error)
		return;

	/* A constant replaces whatever was being typed. */
	c->entering = false;
	c->entry[0] = '\0';
	c->entry_has_exp = false;
	c->acc = (k == C_PI) ? PI_D : E_D;
	c->value_ready = true;
	c->op_pending = false;
}

/* -------------------------------------------------------- modes, memory */

void calc_set_base(Calc *c, Base b)
{
	if (c->base == b)
		return;
	if (c->entering)
		take(c);		/* freeze the typed digits before switching */
	if (b != BASE_DEC)
		c->acc = trunc(c->acc);
	c->base = b;
}

void calc_cycle_base(Calc *c)
{
	switch (c->base) {
	case BASE_DEC: calc_set_base(c, BASE_HEX); break;
	case BASE_HEX: calc_set_base(c, BASE_BIN); break;
	default:       calc_set_base(c, BASE_DEC); break;
	}
}

void calc_cycle_angle(Calc *c)
{
	c->angle = (AngleMode)((c->angle + 1) % 3);
}

void calc_convert_angle(Calc *c)
{
	/* Keep the angle, change its unit: the number is rescaled. */
	double rad = to_rad(c, calc_current(c));

	take(c);
	calc_cycle_angle(c);
	c->acc = from_rad(c, rad);
	c->value_ready = true;
}

void calc_toggle_inv(Calc *c)
{
	c->inv = !c->inv;
}

void calc_mem_store(Calc *c)
{
	if (c->error)
		return;
	c->mem = calc_current(c);
	c->mem_set = true;
}

void calc_mem_recall(Calc *c)
{
	if (c->error)
		return;
	c->entering = false;
	c->entry[0] = '\0';
	c->acc = c->mem;
	c->value_ready = true;
	c->op_pending = false;
}

void calc_mem_add(Calc *c)
{
	if (c->error)
		return;
	c->mem = guard(c, c->mem + calc_current(c), NULL);
	c->mem_set = true;
}

void calc_mem_exchange(Calc *c)
{
	double v;

	if (c->error)
		return;
	v = calc_current(c);
	c->entering = false;
	c->entry[0] = '\0';
	c->acc = c->mem;
	c->mem = v;
	c->mem_set = true;
	c->value_ready = true;
	c->op_pending = false;
}

void calc_mem_clear(Calc *c)
{
	c->mem = 0.0;
	c->mem_set = false;
}

void calc_recall_ans(Calc *c)
{
	if (c->error)
		return;
	c->entering = false;
	c->entry[0] = '\0';
	c->acc = c->last_ans;
	c->value_ready = true;
	c->op_pending = false;
}

/* ----------------------------------------------------------- formatting */

/* Render `u` in `base`, inserting a space every `grp` digits. */
static void fmt_ubase(uint64_t u, int base, int grp, char *out, size_t n)
{
	static const char sym[] = "0123456789ABCDEF";
	char digits[65];
	int nd = 0;
	size_t o = 0;

	if (n == 0)
		return;
	if (u == 0)
		digits[nd++] = '0';
	while (u != 0 && nd < (int)sizeof digits) {
		digits[nd++] = sym[u % (unsigned)base];
		u /= (unsigned)base;
	}
	for (int i = nd - 1; i >= 0 && o + 1 < n; i--) {
		out[o++] = digits[i];
		if (i > 0 && i % grp == 0 && o + 1 < n)
			out[o++] = ' ';
	}
	out[o] = '\0';
}

static void fmt_dec(double v, char *out, size_t n)
{
	if (v == 0.0) {			/* also normalises -0 */
		snprintf(out, n, "0");
		return;
	}
	if (v == floor(v) && fabs(v) < 1e15) {
		snprintf(out, n, "%.0f", v);
		return;
	}
	snprintf(out, n, "%.12g", v);
}

void calc_render_base(const Calc *c, Base b, char *out, size_t n)
{
	double v = calc_current(c);

	if (b == BASE_DEC) {
		fmt_dec(v, out, n);
		return;
	}
	/* Hex and binary show the 64-bit two's-complement pattern. */
	v = snap_display(v);
	if (!isfinite(v) || v != trunc(v) || fabs(v) >= I64_LIMIT) {
		snprintf(out, n, "-");
		return;
	}
	fmt_ubase((uint64_t)(int64_t)v, (int)b, b == BASE_HEX ? 4 : 8, out, n);
}

/* Group the raw entry text the same way a finished value would be grouped. */
static void fmt_entry_grouped(const char *s, int grp, char *out, size_t n)
{
	size_t len = strlen(s), o = 0;

	for (size_t i = 0; i < len && o + 1 < n; i++) {
		size_t rest = len - i - 1;	/* digits after this one */

		out[o++] = (char)toupper((unsigned char)s[i]);
		if (rest > 0 && rest % (size_t)grp == 0 && o + 1 < n)
			out[o++] = ' ';
	}
	out[o] = '\0';
}

void calc_display(const Calc *c, char *out, size_t n)
{
	if (c->error) {
		snprintf(out, n, "%s", c->errmsg ? c->errmsg : "Error");
		return;
	}
	if (c->entering && c->entry[0] != '\0') {
		if (c->base == BASE_DEC)
			snprintf(out, n, "%s", c->entry);
		else
			fmt_entry_grouped(c->entry, c->base == BASE_HEX ? 4 : 8,
					  out, n);
		return;
	}
	calc_render_base(c, c->base, out, n);
}

const char *calc_angle_name(const Calc *c)
{
	switch (c->angle) {
	case ANG_DEG:  return "DEG";
	case ANG_RAD:  return "RAD";
	default:       return "GRAD";
	}
}

const char *calc_base_name(const Calc *c)
{
	switch (c->base) {
	case BASE_HEX: return "HEX";
	case BASE_BIN: return "BIN";
	default:       return "DEC";
	}
}

const char *calc_op_symbol(Op op)
{
	switch (op) {
	case OP_ADD:  return "+";
	case OP_SUB:  return "-";
	case OP_MUL:  return "*";
	case OP_DIV:  return "/";
	case OP_MOD:  return "mod";
	case OP_POW:  return "^";
	case OP_ROOT: return "root";
	case OP_AND:  return "and";
	case OP_OR:   return "or";
	case OP_XOR:  return "xor";
	case OP_SHL:  return "<<";
	case OP_SHR:  return ">>";
	default:      return "";
	}
}

Op calc_pending_op(const Calc *c)
{
	if (c->sp > 0 && c->stack[c->sp - 1].op != OP_LPAREN)
		return c->stack[c->sp - 1].op;
	return OP_NONE;
}
