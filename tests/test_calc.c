/* test_calc.c -- unit tests for the gbcalc engine (no terminal needed). */

#include "calc.h"

#include <math.h>
#include <stdio.h>
#include <string.h>

/* M_PI is not in C99's <math.h>; spell it out. */
#define PI_REF 3.14159265358979323846

static int g_fail;
static int g_run;

static void check_num(const char *what, double got, double want)
{
	g_run++;
	if (fabs(got - want) > 1e-9 * (fabs(want) > 1.0 ? fabs(want) : 1.0)) {
		printf("FAIL %-34s got %.17g want %.17g\n", what, got, want);
		g_fail++;
	}
}

static void check_str(const char *what, const char *got, const char *want)
{
	g_run++;
	if (strcmp(got, want) != 0) {
		printf("FAIL %-34s got \"%s\" want \"%s\"\n", what, got, want);
		g_fail++;
	}
}

static void check_bool(const char *what, bool got, bool want)
{
	g_run++;
	if (got != want) {
		printf("FAIL %-34s got %s want %s\n", what,
		       got ? "true" : "false", want ? "true" : "false");
		g_fail++;
	}
}

/* Type a run of decimal digits (and '.') into the entry buffer. */
static void type(Calc *c, const char *s)
{
	for (; *s; s++) {
		if (*s == '.')
			calc_point(c);
		else if (*s >= '0' && *s <= '9')
			calc_digit(c, *s - '0');
		else if (*s >= 'a' && *s <= 'f')
			calc_digit(c, *s - 'a' + 10);
		else if (*s >= 'A' && *s <= 'F')
			calc_digit(c, *s - 'A' + 10);
	}
}

static double eval2(const char *a, Op op, const char *b)
{
	Calc c;

	calc_init(&c);
	type(&c, a);
	calc_op(&c, op);
	type(&c, b);
	calc_equals(&c);
	return calc_current(&c);
}

static void test_basic_arithmetic(void)
{
	check_num("2+3", eval2("2", OP_ADD, "3"), 5.0);
	check_num("7-9", eval2("7", OP_SUB, "9"), -2.0);
	check_num("6*7", eval2("6", OP_MUL, "7"), 42.0);
	check_num("9/4", eval2("9", OP_DIV, "4"), 2.25);
	check_num("2^10", eval2("2", OP_POW, "10"), 1024.0);
	check_num("17 mod 5", eval2("17", OP_MOD, "5"), 2.0);
}

static void test_precedence(void)
{
	Calc c;

	/* 2 + 3 * 4 = 14, not 20. */
	calc_init(&c);
	type(&c, "2");
	calc_op(&c, OP_ADD);
	type(&c, "3");
	calc_op(&c, OP_MUL);
	type(&c, "4");
	calc_equals(&c);
	check_num("2+3*4", calc_current(&c), 14.0);

	/* 2 * 3 + 4 = 10, and the running total shows 6 at the '+'. */
	calc_init(&c);
	type(&c, "2");
	calc_op(&c, OP_MUL);
	type(&c, "3");
	calc_op(&c, OP_ADD);
	check_num("2*3 running total", calc_current(&c), 6.0);
	type(&c, "4");
	calc_equals(&c);
	check_num("2*3+4", calc_current(&c), 10.0);

	/* Power is right associative: 2^3^2 = 2^9 = 512. */
	calc_init(&c);
	type(&c, "2");
	calc_op(&c, OP_POW);
	type(&c, "3");
	calc_op(&c, OP_POW);
	type(&c, "2");
	calc_equals(&c);
	check_num("2^3^2", calc_current(&c), 512.0);
}

static void test_parens(void)
{
	Calc c;

	/* (2 + 3) * 4 = 20 */
	calc_init(&c);
	calc_lparen(&c);
	type(&c, "2");
	calc_op(&c, OP_ADD);
	type(&c, "3");
	calc_rparen(&c);
	calc_op(&c, OP_MUL);
	type(&c, "4");
	calc_equals(&c);
	check_num("(2+3)*4", calc_current(&c), 20.0);

	/* 2(3+4) is read as 2*(3+4) = 14 */
	calc_init(&c);
	type(&c, "2");
	calc_lparen(&c);
	type(&c, "3");
	calc_op(&c, OP_ADD);
	type(&c, "4");
	calc_rparen(&c);
	calc_equals(&c);
	check_num("2(3+4)", calc_current(&c), 14.0);

	/* Nested, and '=' closes anything still open. */
	calc_init(&c);
	type(&c, "1");
	calc_op(&c, OP_ADD);
	calc_lparen(&c);
	type(&c, "2");
	calc_op(&c, OP_MUL);
	calc_lparen(&c);
	type(&c, "3");
	calc_op(&c, OP_ADD);
	type(&c, "4");
	calc_equals(&c);
	check_num("1+(2*(3+4 =", calc_current(&c), 15.0);

	/* An unbalanced ')' is ignored rather than corrupting the stack. */
	calc_init(&c);
	type(&c, "5");
	calc_rparen(&c);
	calc_op(&c, OP_ADD);
	type(&c, "1");
	calc_equals(&c);
	check_num("5)+1", calc_current(&c), 6.0);
}

static void test_operator_replacement(void)
{
	Calc c;

	/* Pressing two operators in a row keeps only the second. */
	calc_init(&c);
	type(&c, "8");
	calc_op(&c, OP_ADD);
	calc_op(&c, OP_MUL);
	type(&c, "2");
	calc_equals(&c);
	check_num("8 + * 2", calc_current(&c), 16.0);

	/* Replacement must not lose an already reduced left operand. */
	calc_init(&c);
	type(&c, "2");
	calc_op(&c, OP_ADD);
	type(&c, "3");
	calc_op(&c, OP_MUL);
	calc_op(&c, OP_ADD);
	type(&c, "4");
	calc_equals(&c);
	check_num("2+3 * + 4", calc_current(&c), 9.0);
}

static void test_unary(void)
{
	Calc c;
	char buf[64];

	calc_init(&c);			/* degrees by default */
	type(&c, "30");
	calc_unary(&c, U_SIN);
	check_num("sin 30 deg", calc_current(&c), 0.5);

	calc_init(&c);
	calc_cycle_angle(&c);		/* RAD */
	check_str("angle name", calc_angle_name(&c), "RAD");
	calc_const(&c, C_PI);
	calc_unary(&c, U_SIN);
	check_num("sin pi rad", calc_current(&c), 0.0);

	calc_init(&c);
	type(&c, "5");
	calc_unary(&c, U_FACT);
	check_num("5!", calc_current(&c), 120.0);

	calc_init(&c);
	type(&c, "16");
	calc_unary(&c, U_SQRT);
	check_num("sqrt 16", calc_current(&c), 4.0);

	calc_init(&c);
	type(&c, "1000");
	calc_unary(&c, U_LOG10);
	check_num("log 1000", calc_current(&c), 3.0);

	calc_init(&c);
	type(&c, "8");
	calc_unary(&c, U_RECIP);
	calc_display(&c, buf, sizeof buf);
	check_str("1/8 display", buf, "0.125");

	calc_init(&c);
	type(&c, "50");
	calc_unary(&c, U_PCT);
	check_num("50%", calc_current(&c), 0.5);

	/* A unary result feeds straight into the next operator. */
	calc_init(&c);
	type(&c, "9");
	calc_unary(&c, U_SQRT);
	calc_op(&c, OP_ADD);
	type(&c, "1");
	calc_equals(&c);
	check_num("sqrt9+1", calc_current(&c), 4.0);
}

static void test_errors(void)
{
	Calc c;
	char buf[64];

	calc_init(&c);
	type(&c, "1");
	calc_op(&c, OP_DIV);
	type(&c, "0");
	calc_equals(&c);
	check_bool("1/0 sets error", c.error, true);
	calc_display(&c, buf, sizeof buf);
	check_str("1/0 message", buf, "Error: divide by zero");

	/* Further input is ignored until cleared. */
	type(&c, "5");
	check_bool("error swallows input", c.error, true);
	calc_clear_entry(&c);
	check_bool("C clears error", c.error, false);
	check_num("value after clear", calc_current(&c), 0.0);

	calc_init(&c);
	type(&c, "2");
	calc_unary(&c, U_ASIN);
	check_bool("asin 2 domain error", c.error, true);

	calc_init(&c);
	type(&c, "1");
	calc_sign(&c);
	calc_unary(&c, U_LN);
	check_bool("ln -1 domain error", c.error, true);

	calc_init(&c);
	type(&c, "1");
	calc_sign(&c);
	calc_unary(&c, U_FACT);
	check_bool("(-1)! domain error", c.error, true);
}

static void test_entry_editing(void)
{
	Calc c;
	char buf[64];

	calc_init(&c);
	type(&c, "12.50");
	calc_display(&c, buf, sizeof buf);
	check_str("entry shown as typed", buf, "12.50");
	check_num("entry value", calc_current(&c), 12.5);

	calc_backspace(&c);
	calc_display(&c, buf, sizeof buf);
	check_str("after DEL", buf, "12.5");

	/* A second '.' is refused. */
	calc_point(&c);
	calc_display(&c, buf, sizeof buf);
	check_str("second point refused", buf, "12.5");

	/* Sign toggles in place while typing. */
	calc_sign(&c);
	calc_display(&c, buf, sizeof buf);
	check_str("sign while typing", buf, "-12.5");
	calc_sign(&c);
	calc_display(&c, buf, sizeof buf);
	check_str("sign toggles back", buf, "12.5");

	/* Leading zeros collapse. */
	calc_init(&c);
	type(&c, "0007");
	calc_display(&c, buf, sizeof buf);
	check_str("leading zeros", buf, "7");

	/* EE entry with a negative exponent. */
	calc_init(&c);
	type(&c, "1.5");
	calc_ee(&c);
	calc_sign(&c);
	type(&c, "3");
	check_num("1.5e-3", calc_current(&c), 1.5e-3);
	calc_sign(&c);
	check_num("1.5e3", calc_current(&c), 1.5e3);
}

static void test_bases(void)
{
	Calc c;
	char buf[128];

	calc_init(&c);
	type(&c, "255");
	calc_set_base(&c, BASE_HEX);
	calc_display(&c, buf, sizeof buf);
	check_str("255 as hex", buf, "FF");
	calc_render_base(&c, BASE_BIN, buf, sizeof buf);
	check_str("255 as bin", buf, "11111111");

	/* Hex digits only count in hex mode. */
	calc_init(&c);
	calc_set_base(&c, BASE_HEX);
	type(&c, "1f");
	check_num("hex 1F", calc_current(&c), 31.0);
	calc_display(&c, buf, sizeof buf);
	check_str("hex entry grouped", buf, "1F");

	calc_init(&c);
	type(&c, "9");
	calc_digit(&c, 12);		/* 'C' in decimal mode: ignored */
	check_num("hex digit ignored in dec", calc_current(&c), 9.0);

	/* Switching to hex truncates rather than rounding. */
	calc_init(&c);
	type(&c, "10.9");
	calc_set_base(&c, BASE_HEX);
	calc_display(&c, buf, sizeof buf);
	check_str("10.9 truncated to hex", buf, "A");

	/* Grouping every four hex digits. */
	calc_init(&c);
	type(&c, "1193046");		/* 0x123456 */
	calc_render_base(&c, BASE_HEX, buf, sizeof buf);
	check_str("hex grouping", buf, "12 3456");

	/* Negatives use the 64-bit two's complement pattern. */
	calc_init(&c);
	calc_set_base(&c, BASE_HEX);
	type(&c, "0");
	calc_unary(&c, U_NOT);
	calc_display(&c, buf, sizeof buf);
	check_str("NOT 0 in hex", buf, "FFFF FFFF FFFF FFFF");
	check_num("NOT 0 value", calc_current(&c), -1.0);

	/* The base views must agree with the main display: asin(0.5) lands on
	 * 29.999999999999996, shows as "30", and so must read as hex 1E. */
	calc_init(&c);
	type(&c, "0.5");
	calc_unary(&c, U_ASIN);
	calc_display(&c, buf, sizeof buf);
	check_str("asin 0.5 display", buf, "30");
	calc_render_base(&c, BASE_HEX, buf, sizeof buf);
	check_str("asin 0.5 as hex", buf, "1E");

	/* ... and it must be usable as a bitwise operand. */
	calc_op(&c, OP_AND);
	type(&c, "31");
	calc_equals(&c);
	check_bool("asin 0.5 AND 31 ok", c.error, false);
	check_num("asin 0.5 AND 31", calc_current(&c), 30.0);

	/* A non-integer has no hex or binary form. */
	calc_init(&c);
	type(&c, "0.5");
	calc_render_base(&c, BASE_HEX, buf, sizeof buf);
	check_str("0.5 has no hex", buf, "-");

	calc_init(&c);
	type(&c, "0.5");
	calc_render_base(&c, BASE_DEC, buf, sizeof buf);
	check_str("0.5 in dec", buf, "0.5");

	/* Base cycling. */
	calc_init(&c);
	check_str("base dec", calc_base_name(&c), "DEC");
	calc_cycle_base(&c);
	check_str("base hex", calc_base_name(&c), "HEX");
	calc_cycle_base(&c);
	check_str("base bin", calc_base_name(&c), "BIN");
	calc_cycle_base(&c);
	check_str("base wraps", calc_base_name(&c), "DEC");
}

static void test_bitwise(void)
{
	Calc c;

	check_num("12 AND 10", eval2("12", OP_AND, "10"), 8.0);
	check_num("12 OR 10", eval2("12", OP_OR, "10"), 14.0);
	check_num("12 XOR 10", eval2("12", OP_XOR, "10"), 6.0);
	check_num("1 << 8", eval2("1", OP_SHL, "8"), 256.0);
	check_num("256 >> 4", eval2("256", OP_SHR, "4"), 16.0);

	/* AND binds looser than +, matching C. */
	calc_init(&c);
	type(&c, "1");
	calc_op(&c, OP_ADD);
	type(&c, "2");
	calc_op(&c, OP_AND);
	type(&c, "3");
	calc_equals(&c);
	check_num("1+2 AND 3", calc_current(&c), 3.0);

	/* An out-of-range shift count is an error, not undefined behaviour. */
	calc_init(&c);
	type(&c, "1");
	calc_op(&c, OP_SHL);
	type(&c, "64");
	calc_equals(&c);
	check_bool("1<<64 errors", c.error, true);

	/* Bitwise ops need integers. */
	calc_init(&c);
	type(&c, "1.5");
	calc_op(&c, OP_AND);
	type(&c, "1");
	calc_equals(&c);
	check_bool("1.5 AND 1 errors", c.error, true);
}

static void test_memory_and_ans(void)
{
	Calc c;

	calc_init(&c);
	type(&c, "42");
	calc_mem_store(&c);
	check_bool("memory flagged", c.mem_set, true);
	calc_all_clear(&c);
	calc_mem_recall(&c);
	check_num("RCL", calc_current(&c), 42.0);

	type(&c, "8");
	calc_mem_add(&c);
	calc_all_clear(&c);
	calc_mem_recall(&c);
	check_num("M+", calc_current(&c), 50.0);

	/* MX swaps display and memory. */
	calc_init(&c);
	type(&c, "3");
	calc_mem_store(&c);
	calc_clear_entry(&c);
	type(&c, "7");
	calc_mem_exchange(&c);
	check_num("MX display", calc_current(&c), 3.0);
	check_num("MX memory", c.mem, 7.0);

	calc_mem_clear(&c);
	check_bool("MC clears flag", c.mem_set, false);

	/* ANS recalls the last '=' result. */
	calc_init(&c);
	type(&c, "6");
	calc_op(&c, OP_MUL);
	type(&c, "7");
	calc_equals(&c);
	calc_all_clear(&c);
	calc_recall_ans(&c);
	check_num("ANS", calc_current(&c), 42.0);
}

static void test_angle_conversion(void)
{
	Calc c;

	/* INV+DRG keeps the angle and changes its unit: 180 deg -> pi rad. */
	calc_init(&c);
	type(&c, "180");
	calc_convert_angle(&c);
	check_str("unit after convert", calc_angle_name(&c), "RAD");
	check_num("180 deg in rad", calc_current(&c), PI_REF);

	calc_convert_angle(&c);
	check_str("unit after 2nd convert", calc_angle_name(&c), "GRAD");
	check_num("pi rad in grad", calc_current(&c), 200.0);
}

static void test_display_formatting(void)
{
	Calc c;
	char buf[64];

	calc_init(&c);
	type(&c, "1");
	calc_op(&c, OP_DIV);
	type(&c, "3");
	calc_equals(&c);
	calc_display(&c, buf, sizeof buf);
	check_str("1/3", buf, "0.333333333333");

	calc_init(&c);
	type(&c, "0");
	calc_op(&c, OP_SUB);
	type(&c, "0");
	calc_equals(&c);
	calc_display(&c, buf, sizeof buf);
	check_str("no negative zero", buf, "0");

	calc_init(&c);
	type(&c, "2");
	calc_op(&c, OP_POW);
	type(&c, "100");
	calc_equals(&c);
	calc_display(&c, buf, sizeof buf);
	check_str("2^100", buf, "1.26765060023e+30");
}

int main(void)
{
	test_basic_arithmetic();
	test_precedence();
	test_parens();
	test_operator_replacement();
	test_unary();
	test_errors();
	test_entry_editing();
	test_bases();
	test_bitwise();
	test_memory_and_ans();
	test_angle_conversion();
	test_display_formatting();

	printf("%s: %d checks, %d failure%s\n", g_fail ? "FAILED" : "ok",
	       g_run, g_fail, g_fail == 1 ? "" : "s");
	return g_fail ? 1 : 0;
}
