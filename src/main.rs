//! gbcalc command line entry point.

mod calc;
mod theme;
mod ui;

use theme::ColorMode;

pub const GBCALC_VERSION: &str = "1.0.0";

fn usage(to_stderr: bool) {
    const TEXT: &str = "\
usage: gbcalc [options]

A TUI scientific calculator: display on top, functions in the
middle, number entry at the bottom. Decimal, hexadecimal and
binary modes.

options:
  --theme NAME|PATH   use a theme from the theme path, or a file
  --list-themes       show the theme path and available themes
  --dump-theme        print the built-in theme (a starting point
                      for your own; also documents the format)
  --color MODE        auto | truecolor | 256 | 16 | none
  --no-color          same as --color none (also honours NO_COLOR)
  -h, --help          show this message and exit
  -V, --version       show the version and exit

Themes are read from $GBCALC_THEME, then
$XDG_CONFIG_HOME/gbcalc/theme.conf. Press ? inside gbcalc for
the key bindings.
";
    if to_stderr {
        eprint!("{TEXT}");
    } else {
        print!("{TEXT}");
    }
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let mut theme_arg = std::env::var("GBCALC_THEME").ok();
    let mut mode = ColorMode::Auto;
    let mut mode_set = false;

    let mut i = 1;
    while i < args.len() {
        let a = args[i].as_str();

        if a == "--theme" {
            i += 1;
            if i >= args.len() {
                eprintln!("gbcalc: --theme needs a name");
                std::process::exit(2);
            }
            theme_arg = Some(args[i].clone());
        } else if let Some(v) = a.strip_prefix("--theme=") {
            theme_arg = Some(v.to_string());
        } else if a == "--color" || a == "--colour" {
            i += 1;
            let bad = i >= args.len() || theme::parse_color_mode(&args[i]).is_none();
            if bad {
                eprintln!("gbcalc: --color wants one of auto truecolor 256 16 none");
                std::process::exit(2);
            }
            mode = theme::parse_color_mode(&args[i]).unwrap();
            mode_set = true;
        } else if let Some(v) = a.strip_prefix("--color=") {
            match theme::parse_color_mode(v) {
                Some(m) => mode = m,
                None => {
                    eprintln!("gbcalc: --color wants one of auto truecolor 256 16 none");
                    std::process::exit(2);
                }
            }
            mode_set = true;
        } else if a == "--no-color" || a == "--no-colour" {
            mode = ColorMode::None;
            mode_set = true;
        } else if a == "--dump-theme" {
            print!("{}", theme::dump());
            return;
        } else if a == "--list-themes" {
            theme::list();
            return;
        } else if a == "-h" || a == "--help" {
            usage(false);
            return;
        } else if a == "-V" || a == "--version" {
            println!("gbcalc {GBCALC_VERSION}");
            return;
        } else {
            eprintln!("gbcalc: unknown option '{a}'");
            usage(true);
            std::process::exit(2);
        }
        i += 1;
    }

    let mut t = theme::Theme::new();
    match theme_arg {
        Some(name) if !name.is_empty() => {
            // An explicit request that cannot be honoured is an error.
            if let Err(e) = t.load_named(&name) {
                eprintln!("gbcalc: {e}");
                std::process::exit(1);
            }
        }
        _ => t.load_user_default(),
    }
    if mode_set {
        t.set_color_mode(mode);
    }

    std::process::exit(ui::run(&t));
}
