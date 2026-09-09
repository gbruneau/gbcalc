/* theme.c -- gbcalc theme loading and SGR generation. */

#define _POSIX_C_SOURCE 200809L

#include "theme.h"

#include <ctype.h>
#include <dirent.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

#ifndef GBCALC_THEMEDIR
#define GBCALC_THEMEDIR "/usr/local/share/gbcalc/themes"
#endif

/* ------------------------------------------------------------- model */

typedef enum { CK_NONE, CK_IDX, CK_RGB } ColorKind;

typedef struct {
	ColorKind kind;
	int idx;
	int r, g, b;
} Color;

#define ATTR_BOLD    0x01u
#define ATTR_DIM     0x02u
#define ATTR_ITALIC  0x04u
#define ATTR_UNDER   0x08u
#define ATTR_REVERSE 0x10u
#define ATTR_SWAP    0x20u	/* request: exchange fg and bg */
#define ATTR_CLEAR   0x40u	/* request: drop inherited attributes */
#define ATTR_SWAPPED 0x80u	/* result: the exchange was applied */

#define SET_FG 0x1u
#define SET_BG 0x2u

typedef struct {
	Color fg, bg;
	unsigned attrs;
	unsigned set;
} Style;

static Style g_base[ST_COUNT];
static Style g_variant[ST_COUNT][SS_COUNT];
static bool g_have_variant[ST_COUNT][SS_COUNT];
static Style g_xform[SS_COUNT];

static char g_sgr[ST_COUNT][SS_COUNT][96];
static ColorMode g_mode = CM_AUTO;
static ColorMode g_effective = CM_TRUECOLOR;
static char g_name[64];
static ThemeChrome g_chrome;

/* --------------------------------------------------- default theme */

/* Written in the theme file format and parsed by the same code that reads
 * user themes, so the format can never drift from the defaults. */
static const char DEFAULT_THEME[] =
"# gbcalc default theme\n"
"#\n"
"# Syntax:  <slot> [fg=<colour>] [bg=<colour>] [attributes...]\n"
"# Colours: #rrggbb, #rgb, 0-255 (palette index), a colour name\n"
"#          (black red green yellow blue magenta cyan white, and the\n"
"#          bright* variants), or `default` for the terminal's own.\n"
"# Attrs:   bold dim italic underline reverse swap none\n"
"#\n"
"# A slot may be suffixed .focus, .active or .disabled to style that state\n"
"# directly; otherwise the state.* transforms below are applied to the base\n"
"# style. `key` sets every key category at once, so later, more specific\n"
"# lines override it.\n"
"#\n"
"# A user theme only needs the lines it wants to change.\n"
"\n"
"name             tokyo-night\n"
"title            gbcalc\n"
"border           rounded\n"
"brackets         [ ]\n"
"\n"
"# --- frame and display ---------------------------------------------\n"
"frame            fg=#7aa2f7 bold\n"
"title.style      fg=#7dcfff bold\n"
"status           fg=#7aa2f7\n"
"status.mode      fg=#7dcfff bold\n"
"status.inv       fg=#bb9af7 bold\n"
"status.mem       fg=#e0af68 bold\n"
"display          fg=#c0caf5 bold\n"
"display.error    fg=#f7768e bold\n"
"aux              fg=#7dcfff dim\n"
"hint             fg=#565f89\n"
"help.title       fg=#7dcfff bold\n"
"help.text        fg=#a9b1d6\n"
"\n"
"# --- key categories ------------------------------------------------\n"
"key              fg=#16161e bg=#a9b1d6\n"
"key.digit        fg=#16161e bg=#a9b1d6\n"
"key.hexdigit     fg=#16161e bg=#73daca\n"
"key.operator     fg=#16161e bg=#ff9e64\n"
"key.sci          fg=#c0caf5 bg=#3b4261\n"
"key.bitwise      fg=#16161e bg=#bb9af7\n"
"key.mode         fg=#16161e bg=#7dcfff\n"
"key.memory       fg=#16161e bg=#e0af68\n"
"key.edit         fg=#16161e bg=#f7768e\n"
"key.equals       fg=#16161e bg=#9ece6a bold\n"
"key.paren        fg=#c0caf5 bg=#414868\n"
"\n"
"# --- states --------------------------------------------------------\n"
"state.focus      swap bold\n"
"state.active     fg=#16161e bg=#9ece6a bold\n"
"state.disabled   fg=#414868 bg=default dim none\n";

/* ------------------------------------------------------- colour maps */

struct named_color {
	const char *name;
	int idx;
};

static const struct named_color NAMED[] = {
	{ "black", 0 },   { "red", 1 },     { "green", 2 },   { "yellow", 3 },
	{ "blue", 4 },    { "magenta", 5 }, { "cyan", 6 },    { "white", 7 },
	{ "brightblack", 8 },   { "brightred", 9 },      { "brightgreen", 10 },
	{ "brightyellow", 11 }, { "brightblue", 12 },    { "brightmagenta", 13 },
	{ "brightcyan", 14 },   { "brightwhite", 15 },
	{ "grey", 8 },    { "gray", 8 },
};

/* xterm's first sixteen entries, for degrading true colour to 16 colours. */
static const int BASE16[16][3] = {
	{ 0, 0, 0 },       { 205, 0, 0 },     { 0, 205, 0 },     { 205, 205, 0 },
	{ 0, 0, 238 },     { 205, 0, 205 },   { 0, 205, 205 },   { 229, 229, 229 },
	{ 127, 127, 127 }, { 255, 0, 0 },     { 0, 255, 0 },     { 255, 255, 0 },
	{ 92, 92, 255 },   { 255, 0, 255 },   { 0, 255, 255 },   { 255, 255, 255 },
};

static int clamp255(int v)
{
	return v < 0 ? 0 : (v > 255 ? 255 : v);
}

/* Nearest entry in the 6x6x6 cube or the grayscale ramp. */
static int quantize256(int r, int g, int b)
{
	int dr = r > g ? r - g : g - r;
	int dg = g > b ? g - b : b - g;
	int db = r > b ? r - b : b - r;

	if (dr < 12 && dg < 12 && db < 12) {
		if (r < 8)
			return 16;
		if (r > 248)
			return 231;
		return 232 + (r - 8) * 24 / 241;
	}
	return 16 + 36 * (r * 5 / 255) + 6 * (g * 5 / 255) + (b * 5 / 255);
}

static int quantize16(int r, int g, int b)
{
	int best = 7, bestd = 1 << 30;

	for (int i = 0; i < 16; i++) {
		int dr = r - BASE16[i][0];
		int dg = g - BASE16[i][1];
		int db = b - BASE16[i][2];
		int d = dr * dr + dg * dg + db * db;

		if (d < bestd) {
			bestd = d;
			best = i;
		}
	}
	return best;
}

/* ------------------------------------------------------------ parsing */

static bool parse_hex_color(const char *s, Color *out)
{
	size_t len = strlen(s);
	int v[6];

	if (len != 3 && len != 6)
		return false;
	for (size_t i = 0; i < len; i++) {
		if (!isxdigit((unsigned char)s[i]))
			return false;
		v[i] = isdigit((unsigned char)s[i])
			? s[i] - '0'
			: (tolower((unsigned char)s[i]) - 'a' + 10);
	}
	out->kind = CK_RGB;
	if (len == 3) {
		out->r = v[0] * 17;
		out->g = v[1] * 17;
		out->b = v[2] * 17;
	} else {
		out->r = v[0] * 16 + v[1];
		out->g = v[2] * 16 + v[3];
		out->b = v[4] * 16 + v[5];
	}
	return true;
}

static bool parse_color(const char *s, Color *out)
{
	char low[32];
	size_t i;

	memset(out, 0, sizeof *out);
	if (*s == '#')
		return parse_hex_color(s + 1, out);

	for (i = 0; i + 1 < sizeof low && s[i]; i++)
		low[i] = (char)tolower((unsigned char)s[i]);
	low[i] = '\0';

	if (strcmp(low, "default") == 0 || strcmp(low, "none") == 0 ||
	    strcmp(low, "-") == 0) {
		out->kind = CK_NONE;
		return true;
	}
	if (isdigit((unsigned char)low[0])) {
		char *end;
		long n = strtol(low, &end, 10);

		if (*end != '\0' || n < 0 || n > 255)
			return false;
		out->kind = CK_IDX;
		out->idx = (int)n;
		return true;
	}
	for (size_t k = 0; k < sizeof NAMED / sizeof NAMED[0]; k++)
		if (strcmp(low, NAMED[k].name) == 0) {
			out->kind = CK_IDX;
			out->idx = NAMED[k].idx;
			return true;
		}
	return false;
}

static bool parse_attr(const char *s, unsigned *attrs)
{
	if (strcmp(s, "bold") == 0)           *attrs |= ATTR_BOLD;
	else if (strcmp(s, "dim") == 0)       *attrs |= ATTR_DIM;
	else if (strcmp(s, "italic") == 0)    *attrs |= ATTR_ITALIC;
	else if (strcmp(s, "underline") == 0) *attrs |= ATTR_UNDER;
	else if (strcmp(s, "reverse") == 0)   *attrs |= ATTR_REVERSE;
	else if (strcmp(s, "swap") == 0)      *attrs |= ATTR_SWAP;
	else if (strcmp(s, "none") == 0)      *attrs |= ATTR_CLEAR;
	else return false;
	return true;
}

/* Slot name table. "title.style" avoids colliding with the `title` text
 * directive. */
static const struct {
	const char *name;
	StyleId id;
} SLOTS[] = {
	{ "frame",          ST_FRAME },
	{ "title.style",    ST_TITLE },
	{ "status",         ST_STATUS },
	{ "status.mode",    ST_STATUS_MODE },
	{ "status.inv",     ST_STATUS_INV },
	{ "status.mem",     ST_STATUS_MEM },
	{ "display",        ST_DISPLAY },
	{ "display.error",  ST_DISPLAY_ERROR },
	{ "aux",            ST_AUX },
	{ "hint",           ST_HINT },
	{ "help.title",     ST_HELP_TITLE },
	{ "help.text",      ST_HELP_TEXT },
	{ "key.digit",      ST_KEY_DIGIT },
	{ "key.hexdigit",   ST_KEY_HEXDIGIT },
	{ "key.operator",   ST_KEY_OPERATOR },
	{ "key.sci",        ST_KEY_SCI },
	{ "key.bitwise",    ST_KEY_BITWISE },
	{ "key.mode",       ST_KEY_MODE },
	{ "key.memory",     ST_KEY_MEMORY },
	{ "key.edit",       ST_KEY_EDIT },
	{ "key.equals",     ST_KEY_EQUALS },
	{ "key.paren",      ST_KEY_PAREN },
};

static bool lookup_slot(const char *name, StyleId *id)
{
	for (size_t i = 0; i < sizeof SLOTS / sizeof SLOTS[0]; i++)
		if (strcmp(name, SLOTS[i].name) == 0) {
			*id = SLOTS[i].id;
			return true;
		}
	return false;
}

static bool is_key_slot(StyleId id)
{
	return id >= ST_KEY_DIGIT && id <= ST_KEY_PAREN;
}

/* Apply `src` on top of `dst`. */
static void merge(Style *dst, const Style *src)
{
	if (src->set & SET_FG) {
		dst->fg = src->fg;
		dst->set |= SET_FG;
	}
	if (src->set & SET_BG) {
		dst->bg = src->bg;
		dst->set |= SET_BG;
	}
	if (src->attrs & ATTR_CLEAR)
		dst->attrs = src->attrs & ~ATTR_CLEAR;
	else
		dst->attrs |= src->attrs;
}

static void set_border(const char *kind)
{
	ThemeChrome *c = &g_chrome;

	if (strcmp(kind, "square") == 0) {
		c->tl = "\xe2\x94\x8c"; c->tr = "\xe2\x94\x90";
		c->bl = "\xe2\x94\x94"; c->br = "\xe2\x94\x98";
		c->h = "\xe2\x94\x80";  c->v = "\xe2\x94\x82";
	} else if (strcmp(kind, "heavy") == 0) {
		c->tl = "\xe2\x94\x8f"; c->tr = "\xe2\x94\x93";
		c->bl = "\xe2\x94\x97"; c->br = "\xe2\x94\x9b";
		c->h = "\xe2\x94\x81";  c->v = "\xe2\x94\x83";
	} else if (strcmp(kind, "double") == 0) {
		c->tl = "\xe2\x95\x94"; c->tr = "\xe2\x95\x97";
		c->bl = "\xe2\x95\x9a"; c->br = "\xe2\x95\x9d";
		c->h = "\xe2\x95\x90";  c->v = "\xe2\x95\x91";
	} else if (strcmp(kind, "ascii") == 0) {
		c->tl = "+"; c->tr = "+"; c->bl = "+"; c->br = "+";
		c->h = "-";  c->v = "|";
	} else if (strcmp(kind, "none") == 0) {
		c->tl = " "; c->tr = " "; c->bl = " "; c->br = " ";
		c->h = " ";  c->v = " ";
	} else {	/* rounded */
		c->tl = "\xe2\x95\xad"; c->tr = "\xe2\x95\xae";
		c->bl = "\xe2\x95\xb0"; c->br = "\xe2\x95\xaf";
		c->h = "\xe2\x94\x80";  c->v = "\xe2\x94\x82";
	}
}

/* Strip a trailing state suffix, returning the state it named. */
static StyleState split_state(char *name)
{
	static const struct {
		const char *suffix;
		StyleState st;
	} SUFFIX[] = {
		{ ".focus", SS_FOCUS },
		{ ".active", SS_ACTIVE },
		{ ".disabled", SS_DISABLED },
	};

	for (size_t i = 0; i < sizeof SUFFIX / sizeof SUFFIX[0]; i++) {
		size_t nl = strlen(name), sl = strlen(SUFFIX[i].suffix);

		if (nl > sl && strcmp(name + nl - sl, SUFFIX[i].suffix) == 0) {
			name[nl - sl] = '\0';
			return SUFFIX[i].st;
		}
	}
	return SS_NORMAL;
}

static bool parse_buffer(const char *text, const char *origin,
			 char *err, size_t errn)
{
	const char *p = text;
	int lineno = 0;

	while (*p) {
		char line[512], slot[64];
		const char *nl = strchr(p, '\n');
		size_t len = nl ? (size_t)(nl - p) : strlen(p);
		char *tok, *save = NULL;
		Style st;
		StyleId id;
		StyleState state;

		lineno++;
		if (len >= sizeof line)
			len = sizeof line - 1;
		memcpy(line, p, len);
		line[len] = '\0';
		p = nl ? nl + 1 : p + strlen(p);

		/* A '#' starts a comment only at a token boundary -- inside a
		 * token it introduces a colour, as in fg=#7aa2f7. */
		for (char *q = line; *q; q++)
			if (*q == '#' &&
			    (q == line || isspace((unsigned char)q[-1]))) {
				*q = '\0';
				break;
			}

		tok = strtok_r(line, " \t\r", &save);
		if (!tok)
			continue;

		/* Text directives take the rest of the line verbatim. */
		if (strcmp(tok, "name") == 0 || strcmp(tok, "title") == 0) {
			char *rest = save ? save : (char *)"";

			while (*rest == ' ' || *rest == '\t')
				rest++;
			for (char *e = rest + strlen(rest);
			     e > rest && isspace((unsigned char)e[-1]); e--)
				e[-1] = '\0';
			if (strcmp(tok, "name") == 0)
				snprintf(g_name, sizeof g_name, "%s", rest);
			else
				snprintf(g_chrome.title, sizeof g_chrome.title,
					 "%s", rest);
			continue;
		}
		if (strcmp(tok, "border") == 0) {
			char *kind = strtok_r(NULL, " \t\r", &save);

			set_border(kind ? kind : "rounded");
			continue;
		}
		if (strcmp(tok, "brackets") == 0) {
			char *l = strtok_r(NULL, " \t\r", &save);
			char *r = strtok_r(NULL, " \t\r", &save);

			if (!l || strcmp(l, "none") == 0) {
				g_chrome.key_left[0] = '\0';
				g_chrome.key_right[0] = '\0';
			} else {
				snprintf(g_chrome.key_left,
					 sizeof g_chrome.key_left, "%s", l);
				snprintf(g_chrome.key_right,
					 sizeof g_chrome.key_right, "%s",
					 r ? r : l);
			}
			continue;
		}

		snprintf(slot, sizeof slot, "%s", tok);
		state = split_state(slot);

		memset(&st, 0, sizeof st);
		while ((tok = strtok_r(NULL, " \t\r", &save)) != NULL) {
			if (strncmp(tok, "fg=", 3) == 0) {
				if (!parse_color(tok + 3, &st.fg))
					goto bad_value;
				st.set |= SET_FG;
			} else if (strncmp(tok, "bg=", 3) == 0) {
				if (!parse_color(tok + 3, &st.bg))
					goto bad_value;
				st.set |= SET_BG;
			} else if (!parse_attr(tok, &st.attrs)) {
				goto bad_value;
			}
			continue;
bad_value:
			snprintf(err, errn, "%s:%d: bad value '%s'",
				 origin, lineno, tok);
			return false;
		}

		if (strcmp(slot, "state") == 0) {
			/* "state" alone is meaningless; needs a suffix. */
			if (state == SS_NORMAL) {
				snprintf(err, errn,
					 "%s:%d: state needs .focus, .active "
					 "or .disabled", origin, lineno);
				return false;
			}
			merge(&g_xform[state], &st);
			continue;
		}
		if (strcmp(slot, "key") == 0) {
			for (StyleId k = ST_KEY_DIGIT; k <= ST_KEY_PAREN; k++) {
				if (state == SS_NORMAL) {
					merge(&g_base[k], &st);
				} else {
					merge(&g_variant[k][state], &st);
					g_have_variant[k][state] = true;
				}
			}
			continue;
		}
		if (!lookup_slot(slot, &id)) {
			snprintf(err, errn, "%s:%d: unknown slot '%s'",
				 origin, lineno, slot);
			return false;
		}
		if (state == SS_NORMAL) {
			merge(&g_base[id], &st);
		} else {
			merge(&g_variant[id][state], &st);
			g_have_variant[id][state] = true;
		}
		(void)is_key_slot;
	}
	return true;
}

/* ----------------------------------------------------- SGR generation */

static Style resolve(StyleId id, StyleState st)
{
	Style out = g_base[id];

	if (st == SS_NORMAL)
		return out;
	if (g_have_variant[id][st]) {
		merge(&out, &g_variant[id][st]);
		return out;
	}

	/* Derive the state from the base style using the global transform. */
	if (g_xform[st].attrs & ATTR_SWAP) {
		if ((out.set & SET_FG) && (out.set & SET_BG)) {
			Color t = out.fg;

			out.fg = out.bg;
			out.bg = t;
			out.attrs |= ATTR_SWAPPED;
		} else {
			out.attrs |= ATTR_REVERSE;
		}
	}
	merge(&out, &g_xform[st]);
	out.attrs &= ~(ATTR_SWAP | ATTR_CLEAR);
	return out;
}

static void append(char *buf, size_t n, size_t *o, const char *s)
{
	size_t l = strlen(s);

	if (*o + l + 1 > n)
		return;
	memcpy(buf + *o, s, l);
	*o += l;
	buf[*o] = '\0';
}

static void append_color(char *buf, size_t n, size_t *o, const Color *c,
			 bool is_bg)
{
	char tmp[32];
	int base = is_bg ? 40 : 30;
	int ext = is_bg ? 48 : 38;
	int idx;

	if (c->kind == CK_NONE)
		return;

	if (c->kind == CK_RGB) {
		int r = clamp255(c->r), g = clamp255(c->g), b = clamp255(c->b);

		if (g_effective == CM_TRUECOLOR) {
			snprintf(tmp, sizeof tmp, ";%d;2;%d;%d;%d", ext, r, g, b);
			append(buf, n, o, tmp);
			return;
		}
		idx = (g_effective == CM_256) ? quantize256(r, g, b)
					      : quantize16(r, g, b);
	} else {
		idx = c->idx;
		if (g_effective == CM_16 && idx > 15)
			idx = quantize16(0, 0, 0) == idx ? idx : (idx % 16);
	}

	if (idx < 8)
		snprintf(tmp, sizeof tmp, ";%d", base + idx);
	else if (idx < 16)
		snprintf(tmp, sizeof tmp, ";%d", base + 60 + idx - 8);
	else
		snprintf(tmp, sizeof tmp, ";%d;5;%d", ext, idx);
	append(buf, n, o, tmp);
}

static void build_sgr(StyleId id, StyleState st)
{
	Style s = resolve(id, st);
	char *buf = g_sgr[id][st];
	size_t o = 0;

	buf[0] = '\0';
	append(buf, sizeof g_sgr[id][st], &o, "\x1b[0");
	if (s.attrs & ATTR_BOLD)
		append(buf, sizeof g_sgr[id][st], &o, ";1");
	if (s.attrs & ATTR_DIM)
		append(buf, sizeof g_sgr[id][st], &o, ";2");
	if (s.attrs & ATTR_ITALIC)
		append(buf, sizeof g_sgr[id][st], &o, ";3");
	if (s.attrs & ATTR_UNDER)
		append(buf, sizeof g_sgr[id][st], &o, ";4");
	/* Without colour, a swap can only be expressed as reverse video. */
	if ((s.attrs & ATTR_REVERSE) ||
	    (g_effective == CM_NONE && (s.attrs & ATTR_SWAPPED)))
		append(buf, sizeof g_sgr[id][st], &o, ";7");
	if (g_effective != CM_NONE) {
		append_color(buf, sizeof g_sgr[id][st], &o, &s.fg, false);
		append_color(buf, sizeof g_sgr[id][st], &o, &s.bg, true);
	}
	append(buf, sizeof g_sgr[id][st], &o, "m");
}

static void rebuild(void)
{
	g_effective = (g_mode == CM_AUTO) ? theme_detect_color_mode() : g_mode;
	for (int id = 0; id < ST_COUNT; id++)
		for (int st = 0; st < SS_COUNT; st++)
			build_sgr((StyleId)id, (StyleState)st);
}

/* --------------------------------------------------------------- api */

void theme_reset(void)
{
	char err[256];

	memset(g_base, 0, sizeof g_base);
	memset(g_variant, 0, sizeof g_variant);
	memset(g_have_variant, 0, sizeof g_have_variant);
	memset(g_xform, 0, sizeof g_xform);
	memset(&g_chrome, 0, sizeof g_chrome);
	set_border("rounded");
	snprintf(g_chrome.key_left, sizeof g_chrome.key_left, "[");
	snprintf(g_chrome.key_right, sizeof g_chrome.key_right, "]");
	snprintf(g_chrome.title, sizeof g_chrome.title, "gbcalc");
	snprintf(g_name, sizeof g_name, "builtin");

	if (!parse_buffer(DEFAULT_THEME, "<builtin>", err, sizeof err))
		fprintf(stderr, "gbcalc: built-in theme: %s\n", err);
	rebuild();
}

static bool read_whole_file(const char *path, char **out)
{
	FILE *f = fopen(path, "rb");
	size_t cap = 8192, len = 0;
	char *buf;

	if (!f)
		return false;
	buf = malloc(cap);
	if (!buf) {
		fclose(f);
		return false;
	}
	for (;;) {
		size_t n = fread(buf + len, 1, cap - len - 1, f);

		len += n;
		if (len + 1 < cap)
			break;
		cap *= 2;
		char *p = realloc(buf, cap);
		if (!p) {
			free(buf);
			fclose(f);
			return false;
		}
		buf = p;
	}
	buf[len] = '\0';
	fclose(f);
	*out = buf;
	return true;
}

bool theme_load_file(const char *path, char *err, size_t errn)
{
	char *text;
	bool ok;

	if (!read_whole_file(path, &text)) {
		snprintf(err, errn, "cannot read '%s'", path);
		return false;
	}
	ok = parse_buffer(text, path, err, errn);
	free(text);
	if (ok)
		rebuild();
	return ok;
}

/* Directories searched for named themes, most specific first. Room for the
 * GBCALC_THEME_PATH entries plus the three built-in locations. */
#define THEME_DIR_MAX  12
#define THEME_DIR_LEN  512
#define THEME_ENV_MAX  (THEME_DIR_MAX - 3)

static int theme_dirs(char dirs[THEME_DIR_MAX][THEME_DIR_LEN])
{
	const char *env = getenv("GBCALC_THEME_PATH");
	const char *xdg = getenv("XDG_CONFIG_HOME");
	const char *home = getenv("HOME");
	int n = 0;

	if (env && *env) {
		const char *s = env;

		while (*s && n < THEME_ENV_MAX) {
			const char *colon = strchr(s, ':');
			size_t len = colon ? (size_t)(colon - s) : strlen(s);

			if (len > 0 && len < sizeof dirs[0]) {
				memcpy(dirs[n], s, len);
				dirs[n][len] = '\0';
				n++;
			}
			if (!colon)
				break;
			s = colon + 1;
		}
	}
	if (xdg && *xdg)
		snprintf(dirs[n++], sizeof dirs[0], "%s/gbcalc/themes", xdg);
	else if (home && *home)
		snprintf(dirs[n++], sizeof dirs[0],
			 "%s/.config/gbcalc/themes", home);
	snprintf(dirs[n++], sizeof dirs[0], "themes");
	snprintf(dirs[n++], sizeof dirs[0], "%s", GBCALC_THEMEDIR);
	return n;
}

bool theme_load_named(const char *name, char *err, size_t errn)
{
	char dirs[THEME_DIR_MAX][THEME_DIR_LEN];
	size_t len = strlen(name);
	int n;

	if (strchr(name, '/') != NULL ||
	    (len > 5 && strcmp(name + len - 5, ".conf") == 0))
		return theme_load_file(name, err, errn);

	if (len > 128) {
		snprintf(err, errn, "theme name is too long");
		return false;
	}

	n = theme_dirs(dirs);
	for (int i = 0; i < n; i++) {
		char path[700];
		FILE *f;

		snprintf(path, sizeof path, "%.*s/%.128s.conf",
			 THEME_DIR_LEN - 1, dirs[i], name);
		f = fopen(path, "rb");
		if (f) {
			fclose(f);
			return theme_load_file(path, err, errn);
		}
	}
	snprintf(err, errn, "no theme named '%s' on the theme path "
			    "(try --list-themes)", name);
	return false;
}

void theme_load_user_default(void)
{
	const char *xdg = getenv("XDG_CONFIG_HOME");
	const char *home = getenv("HOME");
	char path[600];
	char err[256];
	FILE *f;

	if (xdg && *xdg)
		snprintf(path, sizeof path, "%s/gbcalc/theme.conf", xdg);
	else if (home && *home)
		snprintf(path, sizeof path, "%s/.config/gbcalc/theme.conf", home);
	else
		return;

	f = fopen(path, "rb");
	if (!f)
		return;
	fclose(f);
	if (!theme_load_file(path, err, sizeof err))
		fprintf(stderr, "gbcalc: %s\n", err);
}

void theme_set_color_mode(ColorMode m)
{
	g_mode = m;
	rebuild();
}

ColorMode theme_color_mode(void)
{
	return g_effective;
}

ColorMode theme_detect_color_mode(void)
{
	const char *ct = getenv("COLORTERM");
	const char *term = getenv("TERM");

	if (getenv("NO_COLOR") != NULL)
		return CM_NONE;
	if (ct && (strstr(ct, "truecolor") || strstr(ct, "24bit")))
		return CM_TRUECOLOR;
	if (!term || *term == '\0' || strcmp(term, "dumb") == 0)
		return CM_NONE;
	if (strstr(term, "direct"))
		return CM_TRUECOLOR;
	if (strstr(term, "256color"))
		return CM_256;
	return CM_16;
}

bool theme_parse_color_mode(const char *s, ColorMode *out)
{
	if (strcmp(s, "auto") == 0)                                *out = CM_AUTO;
	else if (strcmp(s, "truecolor") == 0 || strcmp(s, "24bit") == 0)
		*out = CM_TRUECOLOR;
	else if (strcmp(s, "256") == 0)                            *out = CM_256;
	else if (strcmp(s, "16") == 0 || strcmp(s, "ansi") == 0)   *out = CM_16;
	else if (strcmp(s, "none") == 0 || strcmp(s, "off") == 0)  *out = CM_NONE;
	else return false;
	return true;
}

const char *theme_name(void)
{
	return g_name;
}

const ThemeChrome *theme_chrome(void)
{
	return &g_chrome;
}

const char *theme_sgr(StyleId id, StyleState st)
{
	if (id < 0 || id >= ST_COUNT || st < 0 || st >= SS_COUNT)
		return "";
	return g_sgr[id][st];
}

const char *theme_sgr_reset(void)
{
	return "\x1b[0m";
}

void theme_dump(void)
{
	fputs(DEFAULT_THEME, stdout);
}

void theme_list(void)
{
	char dirs[THEME_DIR_MAX][THEME_DIR_LEN];
	int n = theme_dirs(dirs);

	printf("theme search path:\n");
	for (int i = 0; i < n; i++) {
		DIR *d = opendir(dirs[i]);
		struct dirent *e;
		int found = 0;

		printf("  %s\n", dirs[i]);
		if (!d)
			continue;
		while ((e = readdir(d)) != NULL) {
			size_t len = strlen(e->d_name);

			if (len > 5 && strcmp(e->d_name + len - 5, ".conf") == 0) {
				printf("      %.*s\n", (int)(len - 5), e->d_name);
				found++;
			}
		}
		closedir(d);
		if (!found)
			printf("      (none)\n");
	}
}
