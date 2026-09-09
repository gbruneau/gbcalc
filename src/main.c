/* main.c -- gbcalc command line entry point. */

#include "theme.h"
#include "ui.h"

#include <stdio.h>
#include <stdlib.h>
#include <string.h>

#define GBCALC_VERSION "1.0.0"

static void usage(FILE *out)
{
	fputs("usage: gbcalc [options]\n"
	      "\n"
	      "A TUI scientific calculator: display on top, functions in the\n"
	      "middle, number entry at the bottom. Decimal, hexadecimal and\n"
	      "binary modes.\n"
	      "\n"
	      "options:\n"
	      "  --theme NAME|PATH   use a theme from the theme path, or a file\n"
	      "  --list-themes       show the theme path and available themes\n"
	      "  --dump-theme        print the built-in theme (a starting point\n"
	      "                      for your own; also documents the format)\n"
	      "  --color MODE        auto | truecolor | 256 | 16 | none\n"
	      "  --no-color          same as --color none (also honours NO_COLOR)\n"
	      "  -h, --help          show this message and exit\n"
	      "  -V, --version       show the version and exit\n"
	      "\n"
	      "Themes are read from $GBCALC_THEME, then\n"
	      "$XDG_CONFIG_HOME/gbcalc/theme.conf. Press ? inside gbcalc for\n"
	      "the key bindings.\n", out);
}

int main(int argc, char **argv)
{
	const char *theme_arg = getenv("GBCALC_THEME");
	ColorMode mode = CM_AUTO;
	bool mode_set = false;
	char err[256];

	for (int i = 1; i < argc; i++) {
		const char *a = argv[i];

		if (strcmp(a, "--theme") == 0) {
			if (++i >= argc) {
				fprintf(stderr, "gbcalc: --theme needs a name\n");
				return 2;
			}
			theme_arg = argv[i];
		} else if (strncmp(a, "--theme=", 8) == 0) {
			theme_arg = a + 8;
		} else if (strcmp(a, "--color") == 0 ||
			   strcmp(a, "--colour") == 0) {
			if (++i >= argc || !theme_parse_color_mode(argv[i], &mode)) {
				fprintf(stderr, "gbcalc: --color wants one of "
						"auto truecolor 256 16 none\n");
				return 2;
			}
			mode_set = true;
		} else if (strncmp(a, "--color=", 8) == 0) {
			if (!theme_parse_color_mode(a + 8, &mode)) {
				fprintf(stderr, "gbcalc: --color wants one of "
						"auto truecolor 256 16 none\n");
				return 2;
			}
			mode_set = true;
		} else if (strcmp(a, "--no-color") == 0 ||
			   strcmp(a, "--no-colour") == 0) {
			mode = CM_NONE;
			mode_set = true;
		} else if (strcmp(a, "--dump-theme") == 0) {
			theme_dump();
			return 0;
		} else if (strcmp(a, "--list-themes") == 0) {
			theme_list();
			return 0;
		} else if (strcmp(a, "-h") == 0 || strcmp(a, "--help") == 0) {
			usage(stdout);
			return 0;
		} else if (strcmp(a, "-V") == 0 || strcmp(a, "--version") == 0) {
			printf("gbcalc %s\n", GBCALC_VERSION);
			return 0;
		} else {
			fprintf(stderr, "gbcalc: unknown option '%s'\n", a);
			usage(stderr);
			return 2;
		}
	}

	theme_reset();
	if (theme_arg && *theme_arg) {
		/* An explicit request that cannot be honoured is an error. */
		if (!theme_load_named(theme_arg, err, sizeof err)) {
			fprintf(stderr, "gbcalc: %s\n", err);
			return 1;
		}
	} else {
		theme_load_user_default();
	}
	if (mode_set)
		theme_set_color_mode(mode);

	return ui_run();
}
