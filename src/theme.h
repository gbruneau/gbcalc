/* theme.h -- externalised gbcalc styling.
 *
 * Every colour, attribute and border glyph the UI draws comes from here.
 * The built-in default theme is itself written in the theme file format and
 * parsed at startup, so a user theme file is just a partial override of it.
 */
#ifndef GBCALC_THEME_H
#define GBCALC_THEME_H

#include <stdbool.h>
#include <stddef.h>

/* One style slot per thing the UI can paint. Key slots are the calculator
 * function categories -- that is what a theme colours by. */
typedef enum {
	/* frame and display */
	ST_FRAME,
	ST_TITLE,
	ST_STATUS,		/* pending operator, paren depth */
	ST_STATUS_MODE,		/* angle unit and number base */
	ST_STATUS_INV,		/* the INV latch indicator */
	ST_STATUS_MEM,		/* the M indicator */
	ST_DISPLAY,
	ST_DISPLAY_ERROR,
	ST_AUX,			/* the alternate-base line */
	ST_HINT,
	ST_HELP_TITLE,
	ST_HELP_TEXT,
	/* key categories */
	ST_KEY_DIGIT,		/* 0-9 . +/- */
	ST_KEY_HEXDIGIT,	/* A-F */
	ST_KEY_OPERATOR,	/* + - * / x^y */
	ST_KEY_SCI,		/* trig, logs, roots, constants */
	ST_KEY_BITWISE,		/* AND OR XOR NOT << >> MOD */
	ST_KEY_MODE,		/* DRG INV DEC HEX BIN */
	ST_KEY_MEMORY,		/* STO RCL M+ MX MC ANS */
	ST_KEY_EDIT,		/* C AC DEL */
	ST_KEY_EQUALS,
	ST_KEY_PAREN,
	ST_COUNT
} StyleId;

/* Visual state a slot can be drawn in. */
typedef enum { SS_NORMAL, SS_FOCUS, SS_ACTIVE, SS_DISABLED, SS_COUNT } StyleState;

typedef enum { CM_AUTO, CM_TRUECOLOR, CM_256, CM_16, CM_NONE } ColorMode;

/* Border and key glyphs, also theme-controlled. */
typedef struct {
	const char *tl, *tr, *bl, *br, *h, *v;
	char key_left[8], key_right[8];
	char title[32];
} ThemeChrome;

/* Install the built-in default theme. Call before anything else. */
void theme_reset(void);

/* Merge a theme file over the current theme. Returns false and fills `err`
 * on an unreadable file or a syntax error. */
bool theme_load_file(const char *path, char *err, size_t errn);

/* Resolve `name` against the theme search path, then load it. A name that
 * looks like a path (contains '/' or ends in .conf) is used directly. */
bool theme_load_named(const char *name, char *err, size_t errn);

/* Load the user's default theme if one exists. Missing is not an error. */
void theme_load_user_default(void);

void theme_set_color_mode(ColorMode m);
ColorMode theme_color_mode(void);
ColorMode theme_detect_color_mode(void);
bool theme_parse_color_mode(const char *s, ColorMode *out);

const char *theme_name(void);
const ThemeChrome *theme_chrome(void);

/* SGR escape sequence for a slot in a state, always non-NULL. */
const char *theme_sgr(StyleId id, StyleState st);

/* Sequence that returns the terminal to its default appearance. */
const char *theme_sgr_reset(void);

/* Print the built-in theme, which doubles as the format's documentation. */
void theme_dump(void);

/* Print the theme search path and the files found in it. */
void theme_list(void);

#endif /* GBCALC_THEME_H */
