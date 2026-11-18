//! Converting a folder of the old configuration format (`avida convert`)
//! and loading the experiment file gives the same settings, instruction
//! sets, environment and events as reading the old files. Checked for every
//! test of `avida-core/tests` (with the files its command line selects) and
//! for the stock configuration.

use std::path::{Path, PathBuf};

use avida_config::config::Config;
use avida_config::convert::{convert_folder, Options};
use avida_config::feedback::Feedback;
use avida_config::init_file::InitFile;
use avida_config::naming;
use avida_genome::inst_set_registry::convert_legacy_inst_set_file;
use avida_genome::{Feedback as GenomeFeedback, InstCodeConfig};
use avida_hardware::task_adapter::{Env, TaskLibRegistry};
use avida_hardware::HardwareManager;
use avida_world::event_list::{parse_event_line, EventSpec};

fn repo() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../..")
}

/// The settings that pick files: the converter takes them from the command
/// line of a test.
const FILE_SETTINGS: &[&str] = &[
    "EVENT_FILE",
    "ENVIRONMENT_FILE",
    "INST_SET",
    "INST_SET_LOAD_LEGACY",
    "HARDWARE_TYPE",
];

fn options_of(test: &Path) -> Options {
    let mut opts = Options::default();
    let Ok(text) = std::fs::read_to_string(test.join("test_list")) else {
        return opts;
    };
    let Some(line) = text
        .lines()
        .find_map(|l| l.trim_start().strip_prefix("args"))
    else {
        return opts;
    };
    let line = line.trim_start().trim_start_matches('=');
    let line = line.split(';').next().unwrap_or("");
    let line = line.replace("%(default_app)s", "").replace("%(cpus)s", "1");
    let args: Vec<&str> = line.split_whitespace().collect();
    let mut i = 0;
    while i < args.len() {
        match args[i] {
            "-c" if i + 1 < args.len() => {
                opts.config_file = args[i + 1].to_string();
                i += 2;
            }
            "-def" if i + 2 < args.len() => {
                opts.defs.push((args[i + 1].into(), args[i + 2].into()));
                i += 3;
            }
            "-set" if i + 2 < args.len() => {
                if FILE_SETTINGS.contains(&args[i + 1]) {
                    opts.sets.push((args[i + 1].into(), args[i + 2].into()));
                }
                i += 3;
            }
            _ => i += 1,
        }
    }
    opts
}

struct Loaded {
    settings: Vec<(String, String)>,
    inst_sets: String,
    environment: String,
    events: Vec<Result<EventSpec, String>>,
}

fn inst_sets_of(list: &[String], cfg: &Config) -> String {
    let mut hw = HardwareManager::new();
    let mut gfb = GenomeFeedback::new();
    let code_cfg = InstCodeConfig {
        inst_code_length: cfg.inst_code_length,
        inst_code_default_type: cfg.inst_code_default_type,
    };
    let r = hw.load_inst_sets(list, code_cfg, &mut gfb);
    format!("{r:?} {} {:?}", gfb.render(), hw.registry())
}

fn settings_of(cfg: &Config) -> Vec<(String, String)> {
    naming::settings()
        .iter()
        .map(|s| (s.path(), cfg.setting_text(s)))
        .collect()
}

fn environment_of(
    cfg: &Config,
    load: impl FnOnce(&mut Env, &mut Feedback) -> Result<(), String>,
) -> String {
    let mut env = Env::new(cfg, TaskLibRegistry::new());
    let mut fb = Feedback::new();
    let r = load(&mut env, &mut fb);
    format!("{r:?} {} {env:?}", fb.render())
}

fn events_of(lines: &[String]) -> Vec<Result<EventSpec, String>> {
    lines.iter().map(|l| parse_event_line(l)).collect()
}

fn load_old(dir: &str, opts: &Options) -> Result<Loaded, String> {
    let mut cfg = Config::new();
    let mut fb = Feedback::new();
    if !cfg.load(
        &opts.config_file,
        dir,
        Some(&mut fb),
        Some(&opts.defs),
        false,
    ) {
        return Err(fb.render());
    }
    let mut sets = opts.sets.clone();
    cfg.set_from_map(&mut sets);
    let mut list = cfg.instsets().to_vec();
    if cfg.inst_set_load_legacy != 0 {
        let mut gfb = GenomeFeedback::new();
        let c = convert_legacy_inst_set_file(
            &cfg.inst_set,
            cfg.hardware_type,
            dir,
            &mut list,
            &mut gfb,
        );
        if !c.success {
            return Err(gfb.render());
        }
    }
    let inst_sets = inst_sets_of(&list, &cfg);
    let env_file = cfg.environment_file.clone();
    let environment = environment_of(&cfg, |env, fb| {
        env.load(&env_file, dir, fb, Some(&opts.defs), &cfg)
    });
    let ev = InitFile::with_mappings(&cfg.event_file, &opts.defs, dir);
    let lines: Vec<String> = (0..ev.num_lines()).map(|i| ev.get_line(i as i32)).collect();
    Ok(Loaded {
        settings: settings_of(&cfg),
        inst_sets,
        environment,
        events: events_of(&lines),
    })
}

fn load_new(yaml: &str, scratch: &Path) -> Result<Loaded, String> {
    std::fs::create_dir_all(scratch).unwrap();
    std::fs::write(scratch.join("avida.yaml"), yaml).unwrap();
    let dir = scratch.to_str().unwrap();
    let mut cfg = Config::new();
    let mut fb = Feedback::new();
    if !cfg.load_experiment("avida.yaml", dir, &mut fb) {
        return Err(fb.render());
    }
    let inst_sets = inst_sets_of(&cfg.instsets().to_vec(), &cfg);
    let lines = cfg.experiment.environment.clone();
    let environment = environment_of(&cfg, |env, fb| env.load_lines(&lines, fb, &cfg));
    Ok(Loaded {
        settings: settings_of(&cfg),
        inst_sets,
        environment,
        events: events_of(&cfg.experiment.events),
    })
}

fn compare(name: &str, dir: &Path, opts: &Options, scratch: &Path, problems: &mut Vec<String>) {
    let d = dir.to_str().unwrap();
    let old = match load_old(d, opts) {
        Ok(o) => o,
        Err(e) => {
            problems.push(format!("{name}: the old reader fails: {}", e.trim_end()));
            return;
        }
    };
    let conv = match convert_folder(d, opts) {
        Ok(c) => c,
        Err(e) => {
            problems.push(format!("{name}: conversion fails: {e}"));
            return;
        }
    };
    let new = match load_new(&conv.yaml, &scratch.join(name)) {
        Ok(n) => n,
        Err(e) => {
            problems.push(format!(
                "{name}: the experiment file does not load: {e}\n{}",
                conv.yaml
            ));
            return;
        }
    };
    for (a, b) in old.settings.iter().zip(&new.settings) {
        if a != b {
            problems.push(format!(
                "{name}: setting {} is {} (old) but {} (new)",
                a.0, a.1, b.1
            ));
        }
    }
    if old.inst_sets != new.inst_sets {
        problems.push(format!("{name}: the instruction sets differ"));
    }
    if old.environment != new.environment {
        problems.push(format!("{name}: the environment differs"));
    }
    if old.events != new.events {
        problems.push(format!("{name}: the events differ"));
    }
}

#[test]
fn every_test_folder_converts_to_the_same_configuration() {
    let tests = repo().join("avida-core/tests");
    let scratch =
        std::env::temp_dir().join(format!("avida-yaml-equivalence-{}", std::process::id()));
    let mut problems = Vec::new();
    let mut count = 0;
    let mut names: Vec<_> = std::fs::read_dir(&tests)
        .unwrap()
        .filter_map(|e| e.ok())
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .filter(|n| tests.join(n).join("config").is_dir())
        .collect();
    names.sort();
    for name in &names {
        let t = tests.join(name);
        let config = t.join("config");
        let opts = options_of(&t);
        if !config.join(&opts.config_file).is_file() {
            continue;
        }
        count += 1;
        compare(name, &config, &opts, &scratch, &mut problems);
    }
    count += 1;
    compare(
        "support/config",
        &repo().join("avida-core/support/config"),
        &Options::default(),
        &scratch,
        &mut problems,
    );
    let _ = std::fs::remove_dir_all(&scratch);
    assert!(
        problems.is_empty(),
        "{count} folders, problems:\n{}",
        problems.join("\n")
    );
    assert!(count > 90, "only {count} folders compared");
}
