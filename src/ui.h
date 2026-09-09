/* ui.h -- gbcalc terminal UI entry point. */
#ifndef GBCALC_UI_H
#define GBCALC_UI_H

/* Runs the interactive calculator until the user quits. Returns an exit
 * code. Styling comes from theme.c, so load a theme before calling. */
int ui_run(void);

#endif /* GBCALC_UI_H */
