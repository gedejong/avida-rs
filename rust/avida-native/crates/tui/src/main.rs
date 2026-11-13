use std::io::{IsTerminal, Write};
use std::process::ExitCode;
use std::time::Duration;

use avida_tui::cli::{self, Parsed};
use avida_tui::color::{Background, ColorMode};
use avida_tui::runner::{
    enter_terminal, forget_terminal, is_terminal_gone, run_loop, settle_output, terminal_hung_up,
    terminal_problem, write_terminal_nonblocking, CrosstermEvents, Driver, TerminalGuard,
    OUTPUT_SETTLE,
};

/// How long the exit waits for the simulation thread to stop. It stops
/// within a slice batch unless it is inside one long step (an update's
/// events, a restart building a world).
const SIM_STOP_WAIT: Duration = Duration::from_secs(3);

fn main() -> ExitCode {
    let cli = match cli::parse(std::env::args().skip(1)) {
        Parsed::Help => {
            print!("{}", cli::HELP);
            return ExitCode::SUCCESS;
        }
        Parsed::Version => {
            println!("avida-tui {}", env!("CARGO_PKG_VERSION"));
            return ExitCode::SUCCESS;
        }
        Parsed::Error(e) => {
            eprintln!("avida-tui: {e}\nTry avida-tui --help.");
            return ExitCode::from(2);
        }
        Parsed::Run(c) => c,
    };
    let term = std::env::var("TERM").ok();
    if let Some(problem) = terminal_problem(
        std::io::stdin().is_terminal(),
        std::io::stdout().is_terminal(),
        term.as_deref(),
    ) {
        eprintln!("{problem}");
        return ExitCode::from(1);
    }
    let colors = cli.colors.unwrap_or_else(ColorMode::detect);
    avida_tui::color::set_theme(cli.theme.unwrap_or_else(Background::detect), colors);

    // Build the world before taking over the terminal, so a configuration
    // error prints as ordinary text.
    let started = match &cli.play {
        Some(path) => Driver::start_playback(path, colors),
        None => Driver::start(cli.work_dir, cli.args, colors),
    };
    let mut driver = match started {
        Ok(d) => d,
        Err(e) => {
            eprintln!("{e}");
            return ExitCode::from(1);
        }
    };
    if let Some(spec) = cli.record {
        driver.record(spec);
    }
    if !driver.catch_signals() {
        eprintln!("avida-tui: cannot install the signal handlers");
        return ExitCode::from(1);
    }

    let mut terminal = match enter_terminal() {
        Ok(t) => t,
        Err(e) => {
            eprintln!("avida-tui: cannot start the terminal: {e}");
            return ExitCode::from(1);
        }
    };
    let guard = TerminalGuard;
    let mut events = match CrosstermEvents::spawn() {
        Ok(e) => e,
        Err(e) => {
            drop(guard);
            eprintln!("avida-tui: cannot start the input thread: {e}");
            return ExitCode::from(1);
        }
    };
    // Test hook for the panic path: AVIDA_TUI_TEST_PANIC=1 panics after the
    // first screen has been drawn.
    if std::env::var_os("AVIDA_TUI_TEST_PANIC").is_some() {
        driver.app.test_panic = true;
    }
    let result = run_loop(&mut terminal, &mut driver, &mut events);
    // A hang-up ends the loop with an error, or as a SIGHUP; either way no
    // more terminal I/O: nothing could be restored, and a write to a
    // terminal nobody reads can block.
    let gone = terminal_hung_up() || result.as_ref().is_err_and(is_terminal_gone);
    if gone {
        forget_terminal();
    }
    drop(guard);
    let signal = driver.signal();
    if result.is_err() || signal.is_some() {
        // The terminal may be gone (a hang-up): ratatui's `Drop` for it
        // prints to stderr when it cannot show the cursor, and a print to
        // a dead stderr panics. `leave_terminal` has done the restoring.
        std::mem::forget(terminal);
    } else {
        drop(terminal);
    }
    // Past the terminal: from here the exit is bounded on its own (the
    // wait for the simulation thread), so a signal's watchdog stands down.
    driver.exit_path_clear();
    let stopped = driver.shutdown(SIM_STOP_WAIT);
    let mut code: u8 = match (&result, signal) {
        (Err(_), _) => 1,
        // The shell's convention for death by a signal.
        (Ok(()), Some(s)) => u8::try_from(128 + s).unwrap_or(1),
        (Ok(()), None) => 0,
    };
    if !stopped && code == 0 {
        code = 1;
    }
    // Not eprintln!: with the terminal gone it would panic. To a terminal
    // only without blocking (one nobody reads would hold the exit), and
    // nothing at all when it went away.
    let say = |text: String| {
        let line = format!("avida-tui: {text}\n");
        let mut err = std::io::stderr();
        if !err.is_terminal() {
            let _ = err.write_all(line.as_bytes());
        } else if !gone {
            write_terminal_nonblocking(line.as_bytes());
        }
    };
    if let Err(e) = &result {
        say(e.to_string());
    }
    if !stopped {
        say(format!(
            "the simulation thread did not stop within {} seconds; \
             the run's output files may be incomplete.",
            SIM_STOP_WAIT.as_secs()
        ));
    }
    // The exit of the process that leads the terminal's session waits for
    // the terminal to take its output: never for long.
    if !gone {
        settle_output(OUTPUT_SETTLE);
    }
    if !stopped {
        // The thread is still running: end the process, and it, here.
        std::process::exit(i32::from(code));
    }
    ExitCode::from(code)
}
