//! The command line: `avida-gui [config-dir] [avida args...]`.

use std::path::PathBuf;

/// What the program was started with.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct StartupArgs {
    /// The experiment folder to open at once (the `avida` working
    /// directory), if one was given.
    pub dir: Option<PathBuf>,
    /// Arguments passed on to the world as the `avida` binary takes them
    /// (`-s 7`, `-set WORLD_X 30`, ...). They apply to every experiment
    /// opened in this run of the program.
    pub avida_args: Vec<String>,
    /// `--play FILE`: a recording to play back instead of an experiment.
    pub play: Option<PathBuf>,
    /// `--record FILE [--record-every N]`: record the run that is opened.
    pub record: Option<avida_session::RecordSpec>,
}

/// The outcome of reading the command line.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Parsed {
    Run(StartupArgs),
    /// `-h` / `--help`: print [`USAGE`] and exit.
    Help,
    /// A bad option: print the reason and exit with status 2.
    Error(String),
}

pub const USAGE: &str = "\
usage: avida-gui [CONFIG_DIR] [AVIDA_ARGS...]
       avida-gui --play FILE

Opens the Avida experiment in CONFIG_DIR (the folder holding avida.cfg,
events.cfg and environment.cfg) in the desktop viewer. Without a folder
the start screen offers a folder picker and the recently opened ones.

--play FILE opens a recording made with --record or avida-rec and plays it
back: the map with every recorded layer, the charts and the genotype table,
with a slider to move through it. A .avrec file dropped on the window opens
too. --record FILE [--record-every N] [--record-layers LIST] [--record-top N]
records the experiment as it runs.

AVIDA_ARGS are passed to the world as the avida binary takes them, for
example `-s 7 -set WORLD_X 30`.

Logs go to stderr: warnings by default, more with RUST_LOG=info or debug.

On macOS a program started inside tmux or over ssh runs outside the
desktop session and cannot take the focus: its window opens behind the
others. avida-gui warns about it (AVIDA_GUI_SKIP_SESSION_CHECK=1 skips the
check) and floats the window above the others until it is clicked. An app
bundle works from any shell:

    crates/gui/scripts/make-app-bundle.sh OUT_DIR
    open OUT_DIR/Avida.app --args /absolute/path/to/config [AVIDA_ARGS...]";

/// Reads the arguments after the program name. The first argument is the
/// experiment folder unless it starts with `-`; everything else goes to
/// the world untouched.
pub fn parse<I: IntoIterator<Item = String>>(args: I) -> Parsed {
    let mut args: Vec<String> = args.into_iter().collect();
    if args.iter().any(|a| a == "-h" || a == "--help") {
        return Parsed::Help;
    }
    let (play, record) = match (
        avida_session::cli::take_option(&mut args, "--play"),
        avida_session::cli::take_record_options(&mut args),
    ) {
        (Ok(p), Ok(r)) => (p.map(PathBuf::from), r),
        (Err(e), _) | (_, Err(e)) => return Parsed::Error(e),
    };
    if play.is_some() && record.is_some() {
        return Parsed::Error("--play and --record cannot be combined".to_string());
    }
    let dir = match args.first() {
        Some(first) if !first.starts_with('-') => Some(PathBuf::from(args.remove(0))),
        _ => None,
    };
    Parsed::Run(StartupArgs {
        dir,
        avida_args: args,
        play,
        record,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn strings(v: &[&str]) -> Vec<String> {
        v.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn a_leading_folder_is_the_experiment_and_the_rest_goes_to_avida() {
        assert_eq!(
            parse(strings(&["cfg", "-s", "7"])),
            Parsed::Run(StartupArgs {
                dir: Some(PathBuf::from("cfg")),
                avida_args: strings(&["-s", "7"]),
                ..Default::default()
            })
        );
    }

    #[test]
    fn without_a_folder_every_argument_goes_to_avida() {
        assert_eq!(
            parse(strings(&["-set", "WORLD_X", "30"])),
            Parsed::Run(StartupArgs {
                dir: None,
                avida_args: strings(&["-set", "WORLD_X", "30"]),
                ..Default::default()
            })
        );
        assert_eq!(parse(Vec::new()), Parsed::Run(StartupArgs::default()));
    }

    #[test]
    fn play_and_record_options_are_taken_out_of_the_avida_arguments() {
        let Parsed::Run(a) = parse(strings(&["--play", "r.avrec"])) else {
            panic!()
        };
        assert_eq!(a.play, Some(PathBuf::from("r.avrec")));
        assert_eq!(a.dir, None);
        let Parsed::Run(a) = parse(strings(&[
            "cfg",
            "--record",
            "x",
            "--record-every",
            "10",
            "-s",
            "3",
        ])) else {
            panic!()
        };
        assert_eq!(a.dir, Some(PathBuf::from("cfg")));
        assert_eq!(a.avida_args, strings(&["-s", "3"]));
        assert_eq!(a.record.map(|r| r.every), Some(10));
        assert!(matches!(
            parse(strings(&["--play", "a", "--record", "b"])),
            Parsed::Error(_)
        ));
    }

    #[test]
    fn help_wins() {
        assert_eq!(parse(strings(&["cfg", "--help"])), Parsed::Help);
        assert_eq!(parse(strings(&["-h"])), Parsed::Help);
    }
}
