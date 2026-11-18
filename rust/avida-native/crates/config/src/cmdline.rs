//! Port of `Avida::Util::ProcessCmdLineArgs` (avida-core/source/util/CmdLine.cc)
//! and the start-up sequence of avida-core/source/targets/avida/primitive.cc
//! that surrounds it.
//!
//! The C++ prints, loads the configuration and calls `exit()` from inside
//! the parser. Here the parser returns a [`CmdLineOutcome`] instead: the text it
//! would have written to stdout/stderr (in order within each stream), the exit
//! code it would have used (if any), and the resulting configuration.

use crate::config::{Config, VERSION_STRING};
use crate::experiment::DEFAULT_EXPERIMENT;
use crate::feedback::{Feedback, FeedbackKind};
use crate::string_util::{as_int, substring};

/// `Avida::Version::Banner()` for a release build, as printed by `primitive.cc`
/// before argument processing (`cout << Banner() << endl`, so a blank line
/// follows).
pub fn banner() -> String {
    let mut b = String::from("Avida ");
    b.push_str(VERSION_STRING);
    b.push('\n');
    b.push_str("release build\n");
    b.push_str(
        "--------------------------------------------------------------------------------\n",
    );
    b.push_str("by Charles Ofria\n");
    b.push_str("Lead Developer: David M. Bryson\n\n");
    b.push_str("Active developers include:\n");
    b.push_str("Aaron P. Wagner and Anya E. Johnson\n");
    b.push_str("For a more complete list of contributors, see the AUTHORS file.\n\n");
    b.push_str("Copyright 1999-2014 Michigan State University.\n");
    b.push_str("Copyright 1993-2003 California Institute of Technology.\n\n");
    b.push_str("Avida comes with ABSOLUTELY NO WARRANTY.\n");
    b.push_str("This is free software, and you are welcome to redistribute it under certain\n");
    b.push_str("conditions. See file COPYING for details.\n\n");
    b.push_str("For more information, see: http://avida.devosoft.org/\n");
    b.push_str(
        "--------------------------------------------------------------------------------\n",
    );
    b
}

/// The stdout text of `primitive.cc`'s `cout << Banner() << endl`.
pub fn banner_output() -> String {
    format!("{}\n", banner())
}

/// The `--help` text (`CmdLine.cc`).
pub fn help_text() -> String {
    let mut s = String::new();
    s.push_str("Options:\n");
    s.push_str("  -a[nalyze]            Process analyze.cfg instead of normal run.\n");
    s.push_str(
        "  -c[onfig] <filename>  Read the experiment file <filename> (default avida.yaml)\n",
    );
    s.push_str("  -e; -actions          Print a list of all known actions\n");
    s.push_str("  -h[elp]               Help on options (this listing)\n");
    s.push_str("  -i[nteractive]        Run analyze mode interactively\n");
    s.push_str("  -l[oad] <filename>    Load a clone file\n");
    s.push_str("  -r[eview]             Review the settings that differ from their defaults.\n");
    s.push_str("  -s[eed] <value>       Set random seed to <value>\n");
    s.push_str("  -set <path> <value>   Override a setting of avida.yaml (group.key), or a\n");
    s.push_str(
        "                        section (events, environment, instruction_sets) from a file\n",
    );
    s.push_str("  -v[ersion]            Prints the version number\n");
    s.push_str("  -v0 -v1 -v2 -v3 -v4   Set output verbosity to 0..4\n");
    s.push_str("  -w[arn]               Warn when default config settings are used.\n");
    s.push_str("  --generate-config     Write the default experiment file (avida.yaml)\n");
    s.push_str("  convert [DIR]         Convert a folder of the old format (avida convert -h)\n");
    s.push('\n');
    s
}

/// Result of [`process_cmdline`].
#[derive(Debug, Clone)]
pub struct CmdLineOutcome {
    /// The configuration after loading the file and applying the flags
    /// (defaults only, if parsing ended before the load).
    pub config: Config,
    /// What `cWorld::Initialize` got as `-def` variables: always empty, since
    /// an experiment file has no `#include` or `$NAME` (kept for the callers
    /// of `World::new`).
    pub defs: Vec<(String, String)>,
    /// `Some(code)` when the C++ would call `exit(code)` (note `exit(-1)` is
    /// process status 255); `None` when the program proceeds to run.
    pub exit: Option<i32>,
    /// Text the C++ writes to stdout, in order.
    pub stdout: String,
    /// Text the C++ writes to stderr, in order.
    pub stderr: String,
    /// `-e` / `-actions`: stdout holds `"\nSupported Actions:\n"`; the caller
    /// must append `cActionLibrary::GetInstance().DescribeAll()` and a newline
    /// (then exit 0).
    pub list_actions: bool,
    /// Messages produced by loading the configuration file (also already
    /// rendered into `stderr` as `error: ` / `warning: ` lines).
    pub feedback: Feedback,
    /// Experiment file used (`-c`, default `avida.yaml`).
    pub config_filename: String,
    /// `-a`
    pub flag_analyze: bool,
    /// `-i`
    pub flag_interactive: bool,
    /// `-r`
    pub flag_review: bool,
    /// `-w`
    pub flag_warn_default: bool,
    /// `-s N`: the seed given on the command line.
    pub seed: Option<i32>,
    /// `-vN`: the verbosity given on the command line.
    pub verbosity: Option<i32>,
    /// Pending `-set` entries (empty after a successful run).
    pub unused_sets: Vec<(String, String)>,
}

fn map_set(m: &mut Vec<(String, String)>, k: &str, v: &str) {
    if let Some(e) = m.iter_mut().find(|e| e.0 == k) {
        e.1 = v.to_string();
    } else {
        m.push((k.to_string(), v.to_string()));
    }
}

/// `ProcessCmdLineArgs` using the process's current directory as the working
/// directory.
pub fn process_cmdline_args(args: &[String]) -> CmdLineOutcome {
    let cwd = std::env::current_dir()
        .map(|p| p.to_string_lossy().into_owned())
        .unwrap_or_else(|_| ".".to_string());
    process_cmdline(args, &cwd)
}

/// `Avida::Util::ProcessCmdLineArgs(argc, argv, cfg, defs)`.
///
/// `args[0]` is the program name (as `argv[0]`); `cwd` is
/// `Apto::FileSystem::GetCWD()`, used to resolve the configuration file and
/// the `--generate-config` output.
pub fn process_cmdline(args: &[String], cwd: &str) -> CmdLineOutcome {
    let mut out = CmdLineOutcome {
        config: Config::new(),
        defs: Vec::new(),
        exit: None,
        stdout: String::new(),
        stderr: String::new(),
        list_actions: false,
        feedback: Feedback::new(),
        config_filename: crate::experiment::DEFAULT_FILE.to_string(),
        flag_analyze: false,
        flag_interactive: false,
        flag_review: false,
        flag_warn_default: false,
        seed: None,
        verbosity: None,
        unused_sets: Vec::new(),
    };
    let argc = args.len();
    let mut arg_num = 1usize;
    let mut sets: Vec<(String, String)> = Vec::new();

    while arg_num < argc {
        let cur_arg = args[arg_num].as_str();
        if cur_arg == "-e" || cur_arg == "-events" || cur_arg == "-actions" {
            out.stdout.push_str("\nSupported Actions:\n");
            out.list_actions = true;
            out.exit = Some(0);
            return out;
        } else if cur_arg == "-review" || cur_arg == "-r" {
            out.flag_review = true;
        } else if cur_arg == "--help" || cur_arg == "-help" || cur_arg == "-h" {
            out.stdout.push_str(&help_text());
            out.exit = Some(0);
            return out;
        } else if cur_arg == "-seed" || cur_arg == "-s" {
            if arg_num + 1 == argc {
                out.stderr
                    .push_str("Error: Must include a number as the random seed!\n");
                out.exit = Some(0);
                return out;
            }
            arg_num += 1;
            out.seed = Some(as_int(&args[arg_num]));
        } else if cur_arg == "-analyze" || cur_arg == "-a" {
            out.flag_analyze = true;
        } else if cur_arg == "-interactive" || cur_arg == "-i" {
            out.flag_interactive = true;
        } else if cur_arg == "-warn" || cur_arg == "-w" {
            out.flag_warn_default = true;
        } else if cur_arg == "-version" || cur_arg == "-v" {
            // The banner (with the version) has already been shown.
            out.exit = Some(0);
            return out;
        } else if cur_arg.len() >= 2 && substring(cur_arg, 0, 2) == "-v" {
            // QUIRK: any "-v..." not matched above, e.g. "-verbose", sets the
            // verbosity to AsInt of the rest (0 when not numeric).
            out.verbosity = Some(as_int(&substring(cur_arg, 2, cur_arg.len() - 2)));
        } else if cur_arg == "-def" {
            out.stderr.push_str(
                "error: '-def' is gone: an experiment file has no '#include' or '$NAME'. \
                 Name a section file with -set (-set instruction_sets NAME, -set events FILE), \
                 or convert old files with 'avida convert DIR -def NAME VALUE'.\n",
            );
            out.exit = Some(1);
            return out;
        } else if cur_arg == "-set" {
            if arg_num + 1 == argc || arg_num + 2 == argc {
                out.stderr.push_str(&format!(
                    "'{}' option must be followed by name and value\n",
                    cur_arg
                ));
                out.exit = Some(0);
                return out;
            }
            let name = args[arg_num + 1].clone();
            let value = args[arg_num + 2].clone();
            arg_num += 2;
            map_set(&mut sets, &name, &value);
        } else if cur_arg == "-c" || cur_arg == "-config" {
            if arg_num + 1 == argc || args[arg_num + 1].as_bytes().first() == Some(&b'-') {
                out.stderr
                    .push_str("Error: Filename for configuration must be specified.\n");
                out.exit = Some(0);
                return out;
            }
            arg_num += 1;
            out.config_filename = args[arg_num].clone();
        } else if cur_arg == "--generate-config" {
            out.stderr.push_str("Generating default avida.yaml\n");
            // Write errors are silent, as for the C++ ofstream.
            let _ = std::fs::write(format!("{}/avida.yaml", cwd), DEFAULT_EXPERIMENT);
            out.exit = Some(0);
            return out;
        } else {
            out.stderr.push_str(&format!(
                "Error: Unknown Option '{}'\nType: \"{} -h\" for a full option list.\n",
                args[arg_num], args[0]
            ));
            out.exit = Some(0);
            return out;
        }
        arg_num += 1;
    }

    // Load the experiment file.
    let mut feedback = Feedback::new();
    if out
        .config
        .load_experiment(&out.config_filename, cwd, &mut feedback)
        && out.flag_warn_default
    {
        for s in crate::naming::settings() {
            if !out.config.experiment.given.contains(&s.path()) {
                feedback.warning(format!(
                    "{} not in '{}', defaulting to: {}",
                    s.path(),
                    out.config_filename,
                    s.default()
                ));
            }
        }
    }
    for i in 0..feedback.num_messages() {
        let line = match feedback.message_type(i) {
            FeedbackKind::Error => format!("error: {}\n", feedback.message(i)),
            FeedbackKind::Warning => format!("warning: {}\n", feedback.message(i)),
            FeedbackKind::Notification => format!("{}\n", feedback.message(i)),
        };
        out.stderr.push_str(&line);
    }
    let errors = feedback.num_errors();
    out.feedback = feedback;
    if errors != 0 {
        out.exit = Some(-1);
        return out;
    }

    // Command line flags override the file.
    if out.flag_analyze && out.config.analyze_mode < 1 {
        out.config.analyze_mode = 1;
    }
    if out.flag_interactive && out.config.analyze_mode < 2 {
        out.config.analyze_mode = 2;
    }
    if let Some(s) = out.seed {
        out.config.random_seed = s;
    }
    if let Some(v) = out.verbosity {
        out.config.verbosity = v;
    }

    // `-set` (a repeated path keeps its last value). The settings are set
    // and echoed in the order of the C++ table, as `cAvidaConfig::Set` went
    // through them; a section comes after them, in the order given.
    let order =
        |path: &str| crate::naming::setting_by_path(path).map_or(usize::MAX, |s| s.table_order());
    sets.sort_by_key(|(path, _)| order(path));
    let mut failed = Vec::new();
    for (path, value) in sets {
        match out.config.set_path(&path, &value, cwd) {
            Ok(()) => {
                if out.config.verbosity > 1 {
                    out.stdout
                        .push_str(&format!("CmdLine Set: {path} {value}\n"));
                }
            }
            Err(e) => {
                let known = crate::naming::setting_by_path(&path).is_some()
                    || crate::naming::SECTIONS.contains(&path.as_str());
                if known {
                    out.stderr.push_str(&format!("error: -set {path}: {e}\n"));
                } else {
                    // The C++ wording, with the likely intended path.
                    let hint = e
                        .strip_prefix(&format!("unknown setting '{path}'"))
                        .unwrap_or("");
                    let end = if hint.is_empty() { "." } else { hint };
                    out.stderr.push_str(&format!(
                        "error: unrecognized command line configuration setting '{path}'{end}\n"
                    ));
                }
                failed.push((path, value));
            }
        }
    }
    if !failed.is_empty() {
        out.unused_sets = failed;
        out.exit = Some(1);
        return out;
    }

    if out.flag_review {
        out.stdout.push_str(&out.config.print_review());
        out.exit = Some(0);
        return out;
    }
    out
}

/// `primitive.cc`: the `Random Seed:` line printed once the world exists.
/// `config_seed` is `RANDOM_SEED`; `world_seed` is the RNG's actual seed
/// (differs when `RANDOM_SEED < 0` asked for a time-based seed).
pub fn random_seed_line(config_seed: i32, world_seed: i32) -> String {
    let mut s = format!("Random Seed: {}", config_seed);
    if config_seed != world_seed {
        s.push_str(&format!(" -> {}", world_seed));
    }
    s.push('\n');
    s
}

/// `primitive.cc`: whether the `Data Directory:` line is printed
/// (`VERBOSITY > VERBOSE_NORMAL`, where `VERBOSE_NORMAL == 1`).
pub fn prints_data_directory(config: &Config) -> bool {
    config.verbosity > 1
}

/// `primitive.cc` / `CmdLine.cc`: render feedback the way the front end does.
pub fn render_feedback(fb: &Feedback) -> String {
    fb.render()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn scratch(name: &str) -> PathBuf {
        let mut p = std::env::temp_dir();
        p.push(format!(
            "avida_cmdline_test_{}_{}",
            std::process::id(),
            name
        ));
        let _ = std::fs::remove_dir_all(&p);
        std::fs::create_dir_all(&p).unwrap();
        p
    }

    fn run(dir: &std::path::Path, a: &[&str]) -> CmdLineOutcome {
        let mut v = vec!["avida".to_string()];
        v.extend(a.iter().map(|s| s.to_string()));
        process_cmdline(&v, dir.to_str().unwrap())
    }

    fn cfg_dir(name: &str, body: &str) -> PathBuf {
        let d = scratch(name);
        std::fs::write(d.join("avida.yaml"), body).unwrap();
        d
    }

    #[test]
    fn plain_run_loads_file() {
        let d = cfg_dir(
            "plain",
            "world:\n  x: 10\n  y: 20\nmutation:\n  copy_prob: 0.01\n",
        );
        let o = run(&d, &[]);
        assert_eq!(o.exit, None, "{}", o.stderr);
        assert_eq!(o.config.world_x, 10);
        assert_eq!(o.config.world_y, 20);
        assert_eq!(o.config.copy_mut_prob, 0.01);
        assert_eq!(o.stderr, "");
    }

    #[test]
    fn seed_verbosity_analyze() {
        let d = cfg_dir("flags", "general:\n  random_seed: 5\nanalyze:\n  mode: 0\n");
        let o = run(&d, &["-s", "42", "-v3", "-a"]);
        assert_eq!(o.exit, None);
        assert_eq!(o.config.random_seed, 42);
        assert_eq!(o.config.verbosity, 3);
        assert_eq!(o.config.analyze_mode, 1);
        let o = run(&d, &["-seed", "0x10", "-i", "-a"]);
        assert_eq!(o.config.random_seed, 16);
        assert_eq!(o.config.analyze_mode, 2);
        let o = run(&d, &["-verbose"]); // QUIRK: -v prefix
        assert_eq!(o.config.verbosity, 0);
        let o = run(&d, &["-s"]);
        assert_eq!(o.exit, Some(0));
        assert_eq!(
            o.stderr,
            "Error: Must include a number as the random seed!\n"
        );
    }

    #[test]
    fn set_takes_dotted_paths() {
        let d = cfg_dir("set", "world:\n  x: 10\n");
        let o = run(
            &d,
            &[
                "-set", "world.x", "33", "-set", "world.x", "34", "-set", "world.y", "5",
            ],
        );
        assert_eq!(o.exit, None, "{}", o.stderr);
        assert_eq!(o.config.world_x, 34);
        assert_eq!(o.config.world_y, 5);
        let o = run(&d, &["-set", "BOGUS", "1", "-set", "world.x", "1"]);
        assert_eq!(o.exit, Some(1));
        assert_eq!(
            o.stderr,
            "error: unrecognized command line configuration setting 'BOGUS'.\n"
        );
        let o = run(&d, &["-set", "WORLD_X", "1"]);
        assert_eq!(
            o.stderr,
            "error: unrecognized command line configuration setting 'WORLD_X'; did you mean 'world.x'?\n"
        );
        let o = run(&d, &["-set", "world.x", "wide"]);
        assert_eq!(o.exit, Some(1));
        assert_eq!(
            o.stderr,
            "error: -set world.x: setting 'world.x' takes a number, not 'wide'\n"
        );
        let o = run(&d, &["-set", "world.x"]);
        assert_eq!(o.exit, Some(0));
        assert_eq!(
            o.stderr,
            "'-set' option must be followed by name and value\n"
        );
        let o = run(&d, &["-v2", "-set", "world.x", "3"]);
        assert_eq!(o.stdout, "CmdLine Set: world.x 3\n");
    }

    #[test]
    fn config_file_option_and_missing() {
        let d = cfg_dir("cfgopt", "world:\n  x: 10\n");
        std::fs::write(d.join("other.yaml"), "world:\n  x: 77\n").unwrap();
        let o = run(&d, &["-c", "other.yaml"]);
        assert_eq!(o.config.world_x, 77);
        let o = run(&d, &["-config", "nothere.yaml"]);
        assert_eq!(o.exit, Some(-1));
        assert_eq!(
            o.stderr,
            "error: unable to open file 'nothere.yaml'.\nerror: configuration file 'nothere.yaml' not found\n"
        );
        let o = run(&d, &["-c"]);
        assert_eq!(
            o.stderr,
            "Error: Filename for configuration must be specified.\n"
        );
        let o = run(&d, &["-c", "-a"]);
        assert_eq!(o.exit, Some(0));
    }

    #[test]
    fn an_old_folder_is_refused_with_the_converter() {
        let d = scratch("old");
        std::fs::write(d.join("avida.cfg"), "VERSION_ID 2.14.0\nWORLD_X 10\n").unwrap();
        let o = run(&d, &[]);
        assert_eq!(o.exit, Some(-1));
        assert!(
            o.stderr
                .contains("configuration file 'avida.yaml' not found"),
            "{}",
            o.stderr
        );
        assert!(o.stderr.contains("avida convert DIR"), "{}", o.stderr);
        let o = run(&d, &["-c", "avida.cfg"]);
        assert_eq!(o.exit, Some(-1));
        assert!(
            o.stderr.contains("old configuration format"),
            "{}",
            o.stderr
        );
    }

    #[test]
    fn version_and_help_and_unknown() {
        let d = cfg_dir("misc", "");
        assert_eq!(run(&d, &["-v"]).exit, Some(0));
        assert_eq!(run(&d, &["-version"]).exit, Some(0));
        let o = run(&d, &["-h"]);
        assert_eq!(o.exit, Some(0));
        assert!(o.stdout.starts_with("Options:\n  -a[nalyze]"));
        let o = run(&d, &["-bogus"]);
        assert_eq!(o.exit, Some(0));
        assert_eq!(
            o.stderr,
            "Error: Unknown Option '-bogus'\nType: \"avida -h\" for a full option list.\n"
        );
        let o = run(&d, &["-e"]);
        assert!(o.list_actions);
        assert_eq!(o.stdout, "\nSupported Actions:\n");
    }

    #[test]
    fn def_is_gone_and_sections_come_from_set() {
        let d = scratch("def");
        std::fs::write(d.join("avida.yaml"), "instruction_sets: [heads_default]\n").unwrap();
        std::fs::write(
            d.join("sets.yaml"),
            "instruction_sets:\n  - {name: B, hardware_type: 0, instructions: [nop-B]}\n",
        )
        .unwrap();
        let o = run(&d, &[]);
        assert_eq!(o.config.instsets()[0], "INSTSET heads_default:hw_type=0");
        let o = run(&d, &["-set", "instruction_sets", "sets.yaml"]);
        assert_eq!(
            o.config.instsets(),
            &["INSTSET B:hw_type=0".to_string(), "INST nop-B".to_string()]
        );
        let o = run(&d, &["-set", "instruction_sets", "heads_modular"]);
        assert_eq!(o.config.instsets()[0], "INSTSET heads_modular:hw_type=0");
        let o = run(&d, &["-def", "INST_SET", "instset-b.cfg"]);
        assert_eq!(o.exit, Some(1));
        assert!(o.stderr.contains("'-def' is gone"), "{}", o.stderr);
        assert!(o.defs.is_empty());
    }

    #[test]
    fn warn_default() {
        let d = cfg_dir("warn", "world:\n  x: 10\n");
        let o = run(&d, &[]);
        assert_eq!(o.exit, None);
        assert_eq!(o.stderr, "");
        let o = run(&d, &["-w"]);
        assert_eq!(o.exit, None);
        assert!(!o.stderr.contains("world.x not in"));
        assert!(o
            .stderr
            .contains("warning: world.y not in 'avida.yaml', defaulting to: 60\n"));
    }

    #[test]
    fn review_and_generate() {
        let d = cfg_dir("review", "world:\n  x: 10\nmutation:\n  copy_prob: 0.02\n");
        let o = run(&d, &["-r"]);
        assert_eq!(o.exit, Some(0));
        assert_eq!(
            o.stdout,
            "\nNon-Default Settings: \n\n world.x 10 (default=60)\n mutation.copy_prob 0.02 (default=0.0075)\n\n"
        );
        let o = run(&d, &["--generate-config"]);
        assert_eq!(o.exit, Some(0));
        assert_eq!(o.stderr, "Generating default avida.yaml\n");
        let printed = std::fs::read_to_string(d.join("avida.yaml")).unwrap();
        assert_eq!(printed, DEFAULT_EXPERIMENT);
        let o = run(&d, &[]);
        assert_eq!(o.exit, None, "{}", o.stderr);
        assert_eq!(o.config.instsets()[0], "INSTSET heads_default:hw_type=0");
        assert_eq!(o.config.experiment.events.len(), 9);
    }

    #[test]
    fn startup_lines() {
        assert_eq!(random_seed_line(5, 5), "Random Seed: 5\n");
        assert_eq!(random_seed_line(-1, 1234), "Random Seed: -1 -> 1234\n");
        assert!(banner().starts_with("Avida 2.14.0\nrelease build\n"));
    }
}
