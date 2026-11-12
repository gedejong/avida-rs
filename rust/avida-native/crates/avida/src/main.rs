//! The `avida` command-line program: port of
//! avida-core/source/targets/avida/primitive.cc.

use std::io::{BufWriter, Write};
use std::process::ExitCode;

use avida_config::cmdline::{
    banner_output, prints_data_directory, process_cmdline_args, random_seed_line,
};
use avida_world::action_library;
use avida_world::world::World;

/// `exit(code)` as a process status (`exit(-1)` is 255).
fn status(code: i32) -> ExitCode {
    ExitCode::from((code & 0xff) as u8)
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().collect();
    let cwd = std::env::current_dir()
        .map(|p| p.to_string_lossy().into_owned())
        .unwrap_or_else(|_| ".".to_string());

    // Initialize the configuration data...
    let cmd = process_cmdline_args(&args);

    // `Avida2Driver::Run`: `ANALYZE_MODE > 0` runs the analyze script (and,
    // for 2, the interactive loop on standard input) instead of the
    // population. The analyze front end prints the banner and the
    // command-line output itself; the world it sets up builds its
    // population through `AnalyzeSetup`.
    if cmd.exit.is_none() && !cmd.list_actions && cmd.config.analyze_mode > 0 {
        return match avida_analyze::run_analyze_with(
            &cwd,
            &args,
            Box::new(std::io::stdout()),
            Box::new(std::io::stderr()),
            &mut avida_world::analyze_setup::AnalyzeSetup,
            None,
        ) {
            Ok(code) => status(code),
            Err(e) => {
                eprintln!("error: {e}");
                status(1)
            }
        };
    }

    let stdout = std::io::stdout();
    let mut out = BufWriter::new(stdout.lock());
    let _ = write!(out, "{}", banner_output());
    let _ = write!(out, "{}", cmd.stdout);
    let _ = out.flush();
    eprint!("{}", cmd.stderr);
    if cmd.list_actions {
        // `cout << cActionLibrary::GetInstance().DescribeAll() << endl;`
        let _ = writeln!(out, "{}", action_library::describe_all());
        let _ = out.flush();
        return status(0);
    }
    if let Some(code) = cmd.exit {
        return status(code);
    }

    let (mut world, report) = match World::new(cmd.config, &cwd, &cmd.defs) {
        Ok(w) => w,
        Err(text) => {
            let _ = out.flush();
            eprint!("{text}");
            if !text.ends_with('\n') {
                eprintln!();
            }
            return status(-1);
        }
    };
    let _ = write!(out, "{}", report.stdout);
    let _ = out.flush();
    eprint!("{}", report.stderr);

    let _ = write!(
        out,
        "{}",
        random_seed_line(world.cfg.random_seed, world.rng.seed())
    );
    if prints_data_directory(&world.cfg) {
        let _ = writeln!(out, "Data Directory: {}", world.out.output_path());
    }
    let _ = writeln!(out);

    let result = world.run(&mut out);
    if result.is_err() && world.action_state.exit_code.is_some() {
        // What an abort inside an update (an organism's step, the deme
        // update) printed just before `exit`.
        let _ = write!(out, "{}", std::mem::take(&mut world.action_state.stdout));
    }
    world.out.flush_all();
    let _ = out.flush();
    match result {
        Ok(()) => ExitCode::SUCCESS,
        // `Driver().Abort(condition)`: what it had to say is already out.
        Err(_) if world.action_state.exit_code.is_some() => {
            status(world.action_state.exit_code.unwrap_or(1))
        }
        Err(e) => {
            eprintln!("error: {e}");
            status(1)
        }
    }
}
