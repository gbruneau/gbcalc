/* calc.h -- gbcalc calculator engine (pure, no I/O).
 *
 * An immediate-execution algebraic calculator with operator precedence,
 * parentheses, a scientific function set, one memory register and
 * decimal / hexadecimal / binary entry and display modes.
 */
#ifndef GBCALC_CALC_H
#define GBCALC_CALC_H

#include <stdbool.h>
#include <stddef.h>
#include <stdint.h>

/* Number base for entry and display. Values are the radix itself. */
typedef enum { BASE_DEC = 10, BASE_HEX = 16, BASE_BIN = 2 } Base;

/* Angle unit used by the trigonometric functions. */
typedef enum { ANG_DEG, ANG_RAD, ANG_GRAD } AngleMode;

/* Binary operators, listed low precedence first (see prec() in calc.c). */
typedef enum {
	OP_NONE = 0,
	OP_LPAREN,		/* stack barrier, never applied */
	OP_OR,
	OP_XOR,
	OP_AND,
	OP_SHL,
	OP_SHR,
	OP_ADD,
	OP_SUB,
	OP_MUL,
	OP_DIV,
	OP_MOD,
	OP_POW,
	OP_ROOT			/* a^(1/b) -- INV of x^y */
} Op;

/* Unary functions. */
typedef enum {
	U_SIN, U_COS, U_TAN,
	U_ASIN, U_ACOS, U_ATAN,
	U_LN, U_EXP, U_LOG10, U_EXP10,
	U_SQRT, U_SQR, U_RECIP,
	U_FACT, U_NOT, U_PCT
} Unary;

/* Named constants. */
typedef enum { C_PI, C_E } Konst;

#define CALC_STACK_MAX 32	/* pending operators / paren depth */
#define CALC_ENTRY_MAX 72	/* long enough for 64 binary digits */

typedef struct {
	double value;		/* left-hand operand */
	Op op;			/* operator awaiting its right-hand operand */
} Frame;

typedef struct {
	double acc;				/* value shown when not entering */
	char entry[CALC_ENTRY_MAX + 1];		/* digits as typed */
	bool entering;				/* entry buffer holds the value */
	bool entry_has_exp;			/* EE pressed, digits go to exponent */

	Frame stack[CALC_STACK_MAX];
	int sp;
	int paren_depth;

	Base base;
	AngleMode angle;
	bool inv;			/* second-function latch, cleared after use */

	double mem;
	bool mem_set;
	double last_ans;

	bool error;
	const char *errmsg;		/* static string, valid while error is set */

	/* Entry state machine bookkeeping. */
	bool value_ready;		/* an operand is available (digit, unary, ")") */
	bool op_pending;		/* last keypress was a binary operator */
} Calc;

void calc_init(Calc *c);

/* --- entry ------------------------------------------------------------- */
void calc_digit(Calc *c, int d);	/* d in 0..15, ignored if >= base */
void calc_point(Calc *c);
void calc_ee(Calc *c);
void calc_sign(Calc *c);		/* +/- on mantissa, or on exponent after EE */
void calc_backspace(Calc *c);
void calc_clear_entry(Calc *c);		/* C  */
void calc_all_clear(Calc *c);		/* AC */

/* --- operators -------------------------------------------------------- */
void calc_op(Calc *c, Op op);
void calc_lparen(Calc *c);
void calc_rparen(Calc *c);
void calc_equals(Calc *c);
void calc_unary(Calc *c, Unary u);
void calc_const(Calc *c, Konst k);

/* --- modes and memory ------------------------------------------------- */
void calc_set_base(Calc *c, Base b);
void calc_cycle_base(Calc *c);
void calc_cycle_angle(Calc *c);
void calc_convert_angle(Calc *c);	/* reinterpret the value in the next unit */
void calc_toggle_inv(Calc *c);

void calc_mem_store(Calc *c);
void calc_mem_recall(Calc *c);
void calc_mem_add(Calc *c);
void calc_mem_exchange(Calc *c);
void calc_mem_clear(Calc *c);
void calc_recall_ans(Calc *c);

/* --- queries / formatting --------------------------------------------- */
double calc_current(const Calc *c);	/* the value the display represents */

/* Main display text (error message, entry as typed, or formatted value). */
void calc_display(const Calc *c, char *out, size_t n);

/* Value rendered in a specific base; writes "-" when not representable. */
void calc_render_base(const Calc *c, Base b, char *out, size_t n);

const char *calc_angle_name(const Calc *c);
const char *calc_base_name(const Calc *c);
const char *calc_op_symbol(Op op);
Op calc_pending_op(const Calc *c);	/* innermost pending operator, or OP_NONE */

#endif /* GBCALC_CALC_H */
