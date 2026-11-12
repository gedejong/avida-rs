//! [`run_analyze`]: the `avida` front end (avida-core/source/targets/avida/
//! primitive.cc and `Avida2Driver::Run`, 47f13dadb) for a run that ends in
//! analyze mode.

use std::io::{BufRead, Write};

use avida_config::cmdline;

use crate::analyze::Analyze;
use crate::error::{AnalyzeError, Result};
use crate::host::AnalyzeHost;
use crate::world::{AnalyzeWorld, NoSetupHooks, SetupHooks};

/// Run `avida` with command line `args` (`args[0]` is the program name) in
/// the directory `working_dir`, in analyze mode:
///
/// 1. banner; `ProcessCmdLineArgs` (config file, `-s`, `-set`, `-a`, ...),
///    with its messages;
/// 2. `cWorld::Initialize` as [`AnalyzeWorld::initialize`] (environment,
///    instruction sets, world RNG seeded and drawn as population set-up
///    draws it, event file check), then the feedback, the `Random Seed:`
///    line, `Data Directory:` at `VERBOSITY > 1`, a blank line;
/// 3. `Avida2Driver::Run`: `In analyze mode!!`, `cAnalyze` construction
///    (its job queue draws one value from the world RNG), `RunFile` of
///    `ANALYZE_FILE`; data files go to `DATA_DIR` under `working_dir`;
///    then, when `ANALYZE_MODE` is 2 (`-i`), `RunInteractive`: commands
///    read from standard input until `END`, `QUIT` or `EXIT` (the C++
///    prompts `>> ` forever once the input has ended; that stops here).
///
/// `out` and `err` receive what the C++ writes to stdout and stderr.
///
/// `Ok(code)` is what the C++ passes to `exit()` or returns from `main`
/// (0 on success, -1 when the world cannot be set up, 1 for an analyze
/// error, ...); the process exit status is `code & 0xFF`. `Err` is a loud
/// failure: a command, action, setting or test-CPU path this port does not
/// support ([`AnalyzeError::Unsupported`] / [`AnalyzeError::World`]), or C++
/// undefined behaviour ([`AnalyzeError::Fatal`]). A normal (non-analyze)
/// run (`ANALYZE_MODE 0`) belongs to the population layer and is refused.
/// Without [`SetupHooks`] the population set-up is the analyze world's own
/// (see [`AnalyzeWorld`]) and `-e` is refused; [`run_analyze_with`] takes
/// hooks.
///
/// Paths in analyze commands that start with `.` or `/` resolve against the
/// process's current directory, as `Avida::Output::Manager` does; the
/// reference runs with the current directory equal to `working_dir`.
pub fn run_analyze(
    working_dir: &str,
    args: &[String],
    out: Box<dyn Write>,
    err: Box<dyn Write>,
) -> Result<i32> {
    run_analyze_with(working_dir, args, out, err, &mut NoSetupHooks, None)
}

/// [`run_analyze`] with the parts of the world set-up and of the front end
/// that live above this crate supplied by `hooks` (`-e`, the migration
/// matrix, the population), and interactive mode reading `input` instead
/// of the process's standard input when it is given.
pub fn run_analyze_with(
    working_dir: &str,
    args: &[String],
    mut out: Box<dyn Write>,
    mut err: Box<dyn Write>,
    hooks: &mut dyn SetupHooks,
    input: Option<Box<dyn BufRead>>,
) -> Result<i32> {
    out.write_all(cmdline::banner_output().as_bytes())?;

    let outcome = cmdline::process_cmdline(args, working_dir);
    out.write_all(outcome.stdout.as_bytes())?;
    err.write_all(outcome.stderr.as_bytes())?;
    if outcome.list_actions {
        // `cout << cActionLibrary::GetInstance().DescribeAll() << endl;
        // exit(0);`
        let Some(text) = hooks.describe_all_actions() else {
            flush(&mut out, &mut err);
            return Err(AnalyzeError::unsupported(
                "-e",
                "cActionLibrary::DescribeAll (CmdLine.cc)",
            ));
        };
        writeln!(out, "{text}")?;
        flush(&mut out, &mut err);
        return Ok(0);
    }
    if let Some(code) = outcome.exit {
        flush(&mut out, &mut err);
        return Ok(code);
    }

    let init =
        match AnalyzeWorld::initialize_with(outcome.config, working_dir, &outcome.defs, hooks) {
            Ok(init) => init,
            Err(e) => {
                flush(&mut out, &mut err);
                return Err(e);
            }
        };
    out.write_all(init.stdout.as_bytes())?;
    err.write_all(init.stderr.as_bytes())?;
    let Some(mut world) = init.world else {
        flush(&mut out, &mut err);
        return Ok(init.exit_code);
    };

    {
        let cfg = world.config();
        out.write_all(cmdline::random_seed_line(cfg.random_seed, world.random_seed()).as_bytes())?;
        if cmdline::prints_data_directory(cfg) {
            writeln!(out, "Data Directory: {}", world.output_path())?;
        }
        writeln!(out)?;
    }

    // Avida2Driver::Run
    let (analyze_mode, analyze_file) = {
        let cfg = world.config();
        (cfg.analyze_mode, cfg.analyze_file.clone())
    };
    if analyze_mode <= 0 {
        flush(&mut out, &mut err);
        return Err(AnalyzeError::unsupported(
            "normal run (ANALYZE_MODE 0)",
            "Avida2Driver::Run (population update loop)",
        ));
    }
    writeln!(out, "In analyze mode!!")?;
    let mut analyze = Analyze::with_streams(&mut world, out, err);
    if let Some(input) = input {
        analyze.set_input(input);
    }
    let mut r = analyze.run_file(&mut world, &analyze_file);
    // The setting is read again: the analyze file may have changed it
    // (`CONFIG_SET`).
    if r.is_ok() && world.config().analyze_mode == 2 {
        r = analyze.run_interactive(&mut world);
    }
    let _ = analyze.flush();
    match r {
        Ok(()) => Ok(0),
        Err(AnalyzeError::Exit(code)) => Ok(code),
        Err(e) => Err(e),
    }
}

fn flush(out: &mut Box<dyn Write>, err: &mut Box<dyn Write>) {
    let _ = out.flush();
    let _ = err.flush();
}
