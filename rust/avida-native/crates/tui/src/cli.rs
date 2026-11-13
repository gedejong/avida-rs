//! Command line: `avida-tui [OPTIONS] [CONFIG_DIR] [AVIDA_ARGS...]`.

use std::path::PathBuf;

use crate::color::{Background, ColorMode};

pub const HELP: &str = "\
avida-tui: terminal front end for the native Avida port

USAGE:
    avida-tui [OPTIONS] [CONFIG_DIR] [AVIDA_ARGS...]

CONFIG_DIR is the run's working directory (avida.cfg, events.cfg, the
environment file); it defaults to the current directory. Everything after it
is passed to the avida command line (for example: -s 7 -set WORLD_X 30).

OPTIONS (before CONFIG_DIR):
    --colors MODE   auto (default), truecolor, 256, 16 or none (NO_COLOR also
                    turns colour off; this option wins over it)
    --theme NAME    auto (default), dark or light: the terminal background
                    the colours are chosen for. auto reads AVIDA_TUI_THEME,
                    then COLORFGBG, and assumes dark
    --help          show this text
    --version       show the version

Press ? inside for the key list and : (or Ctrl-P) for the command palette.
Space runs and pauses, . steps one update, M S G T Z U E H D F O switch
screens (U is the organism debugger, F the configuration), q quits.
";

/// What the command line asks for.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Cli {
    pub work_dir: PathBuf,
    pub args: Vec<String>,
    pub colors: Option<ColorMode>,
    pub theme: Option<Background>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Parsed {
    Run(Cli),
    Help,
    Version,
    Error(String),
}

/// Parses the arguments after the program name.
pub fn parse<I: IntoIterator<Item = String>>(args: I) -> Parsed {
    let mut it = args.into_iter().peekable();
    let mut colors = None;
    let mut theme = None;
    while let Some(a) = it.peek().cloned() {
        match a.as_str() {
            "--help" => return Parsed::Help,
            "--version" => return Parsed::Version,
            "--colors" => {
                it.next();
                let Some(v) = it.next() else {
                    return Parsed::Error("--colors needs auto, truecolor, 256, 16 or none".into());
                };
                match parse_colors(&v) {
                    Ok(c) => colors = c,
                    Err(e) => return Parsed::Error(e),
                }
            }
            "--theme" => {
                it.next();
                let Some(v) = it.next() else {
                    return Parsed::Error("--theme needs auto, dark or light".into());
                };
                match parse_theme(&v) {
                    Ok(t) => theme = t,
                    Err(e) => return Parsed::Error(e),
                }
            }
            s if s.starts_with("--theme=") => {
                it.next();
                match parse_theme(&s["--theme=".len()..]) {
                    Ok(t) => theme = t,
                    Err(e) => return Parsed::Error(e),
                }
            }
            s if s.starts_with("--colors=") => {
                it.next();
                match parse_colors(&s["--colors=".len()..]) {
                    Ok(c) => colors = c,
                    Err(e) => return Parsed::Error(e),
                }
            }
            _ => break,
        }
    }
    let work_dir = match it.peek() {
        Some(a) if !a.starts_with('-') => PathBuf::from(it.next().unwrap_or_default()),
        _ => PathBuf::from("."),
    };
    if !work_dir.is_dir() {
        return Parsed::Error(format!("{}: not a directory", work_dir.display()));
    }
    Parsed::Run(Cli {
        work_dir,
        args: it.collect(),
        colors,
        theme,
    })
}

fn parse_colors(v: &str) -> Result<Option<ColorMode>, String> {
    match v {
        "auto" => Ok(None),
        "truecolor" | "24bit" => Ok(Some(ColorMode::TrueColor)),
        "256" => Ok(Some(ColorMode::Indexed256)),
        "16" => Ok(Some(ColorMode::Ansi16)),
        "none" | "mono" => Ok(Some(ColorMode::Mono)),
        other => Err(format!(
            "--colors: unknown mode {other:?} (auto, truecolor, 256, 16, none)"
        )),
    }
}

fn parse_theme(v: &str) -> Result<Option<Background>, String> {
    match v {
        "auto" => Ok(None),
        "dark" => Ok(Some(Background::Dark)),
        "light" => Ok(Some(Background::Light)),
        other => Err(format!(
            "--theme: unknown theme {other:?} (auto, dark, light)"
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn p(a: &[&str]) -> Parsed {
        parse(a.iter().map(|s| s.to_string()))
    }

    #[test]
    fn help_version_and_defaults() {
        assert_eq!(p(&["--help"]), Parsed::Help);
        assert_eq!(p(&["--version"]), Parsed::Version);
        let Parsed::Run(c) = p(&[]) else { panic!() };
        assert_eq!(c.work_dir, PathBuf::from("."));
        assert!(c.args.is_empty());
    }

    #[test]
    fn dir_then_avida_args() {
        let Parsed::Run(c) = p(&["/", "-s", "7", "-set", "WORLD_X", "30"]) else {
            panic!()
        };
        assert_eq!(c.work_dir, PathBuf::from("/"));
        assert_eq!(c.args, ["-s", "7", "-set", "WORLD_X", "30"]);
    }

    #[test]
    fn leading_dash_means_avida_args_in_the_current_directory() {
        let Parsed::Run(c) = p(&["-s", "7"]) else {
            panic!()
        };
        assert_eq!(c.work_dir, PathBuf::from("."));
        assert_eq!(c.args, ["-s", "7"]);
    }

    #[test]
    fn colors_flag() {
        let Parsed::Run(c) = p(&["--colors", "256", "/"]) else {
            panic!()
        };
        assert_eq!(c.colors, Some(ColorMode::Indexed256));
        let Parsed::Run(c) = p(&["--colors=truecolor"]) else {
            panic!()
        };
        assert_eq!(c.colors, Some(ColorMode::TrueColor));
        let Parsed::Run(c) = p(&["--colors=none"]) else {
            panic!()
        };
        assert_eq!(c.colors, Some(ColorMode::Mono));
        assert!(matches!(p(&["--colors=7"]), Parsed::Error(_)));
        assert!(matches!(p(&["--colors"]), Parsed::Error(_)));
    }

    #[test]
    fn theme_flag() {
        let Parsed::Run(c) = p(&["--theme", "light", "/"]) else {
            panic!()
        };
        assert_eq!(c.theme, Some(Background::Light));
        let Parsed::Run(c) = p(&["--theme=dark", "--colors", "16"]) else {
            panic!()
        };
        assert_eq!(c.theme, Some(Background::Dark));
        assert_eq!(c.colors, Some(ColorMode::Ansi16));
        assert!(matches!(p(&["--theme=blue"]), Parsed::Error(_)));
    }

    #[test]
    fn missing_directory_is_an_error() {
        assert!(matches!(p(&["/no/such/dir/anywhere"]), Parsed::Error(_)));
    }
}
