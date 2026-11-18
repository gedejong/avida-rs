//! Loads every environment shipped with Avida (the experiment files of
//! avida-core/support/config and of each consistency test's config
//! directory, and the environment section files) through the native
//! parser, with the settings and `-set` arguments the test uses.

use avida_config::config::Config;
use avida_config::feedback::Feedback;
use avida_environment::{proc_type, Environment, SimpleTaskLib, TaskRegistry};
use std::path::{Path, PathBuf};

fn avida_core() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../../avida-core")
}

/// `-c` and `-set` arguments from a test's `test_list`.
#[derive(Default)]
struct TestArgs {
    cfg: Option<String>,
    sets: Vec<(String, String)>,
}

fn test_args(test_dir: &Path) -> TestArgs {
    let mut out = TestArgs::default();
    let Ok(text) = std::fs::read_to_string(test_dir.join("test_list")) else {
        return out;
    };
    let Some(line) = text.lines().find(|l| l.trim_start().starts_with("args")) else {
        return out;
    };
    let Some((_, args)) = line.split_once('=') else {
        return out;
    };
    let words: Vec<&str> = args
        .split(';')
        .next()
        .unwrap_or("")
        .split_whitespace()
        .collect();
    let mut i = 0;
    while i < words.len() {
        match words[i] {
            "-c" if i + 1 < words.len() => {
                out.cfg = Some(words[i + 1].to_string());
                i += 2;
            }
            "-set" if i + 2 < words.len() => {
                out.sets
                    .push((words[i + 1].to_string(), words[i + 2].to_string()));
                i += 3;
            }
            _ => i += 1,
        }
    }
    out
}

/// The experiment file of `dir` with the test's `-set` arguments, or the
/// defaults when the folder has none.
fn load_config(dir: &Path, args: &TestArgs) -> Option<Config> {
    let mut config = Config::new();
    let cfg = args.cfg.clone().unwrap_or_else(|| "avida.yaml".to_string());
    if !dir.join(&cfg).exists() {
        return None;
    }
    let mut fb = Feedback::new();
    assert!(
        config.load_experiment(&cfg, dir.to_str().unwrap(), &mut fb),
        "config {} in {}: {}",
        cfg,
        dir.display(),
        fb.render()
    );
    for (path, value) in &args.sets {
        // A placeholder of the test runner (`%(cpus)s`) is not a value.
        if !value.starts_with('%') {
            config
                .set_path(path, value, dir.to_str().unwrap())
                .unwrap_or_else(|e| panic!("{}: -set {path}: {e}", dir.display()));
        }
    }
    Some(config)
}

fn load_env(what: &str, config: &Config) -> Environment<SimpleTaskLib> {
    let mut env = Environment::new(config, SimpleTaskLib::new());
    let mut fb = Feedback::new();
    let r = env.load_lines(&config.experiment.environment, &mut fb, config);
    assert!(r.is_ok(), "{what}: {:?}\n{}", r, fb.render());
    assert_eq!(fb.num_errors(), 0, "{what}: {}", fb.render());
    env
}

/// The environment section file `file` of `dir`, read with the defaults.
fn load_env_file(dir: &Path, file: &str) -> Environment<SimpleTaskLib> {
    let mut config = Config::new();
    config
        .set_path("environment", file, dir.to_str().unwrap())
        .unwrap_or_else(|e| panic!("{}/{file}: {e}", dir.display()));
    load_env(&format!("{}/{file}", dir.display()), &config)
}

#[test]
fn load_every_test_environment() {
    let tests = avida_core().join("tests");
    let mut loaded = 0;
    let mut dirs: Vec<PathBuf> = std::fs::read_dir(&tests)
        .unwrap()
        .map(|e| e.unwrap().path())
        .collect();
    dirs.sort();
    for test_dir in dirs {
        let dir = test_dir.join("config");
        if !dir.is_dir() {
            continue;
        }
        let args = test_args(&test_dir);
        let Some(config) = load_config(&dir, &args) else {
            continue;
        };
        load_env(&dir.display().to_string(), &config);
        loaded += 1;
    }
    // Every test folder but the 28 analyze-script (ASL) ones.
    assert!(loaded >= 90, "only {} environments loaded", loaded);
}

#[test]
fn load_support_environments() {
    let support = avida_core().join("support/config");
    let config = load_config(&support, &TestArgs::default()).unwrap();
    load_env("support/config/avida.yaml", &config);
    let mut n = 0;
    for dir in [support.clone(), support.join("misc")] {
        for e in std::fs::read_dir(&dir).unwrap() {
            let name = e.unwrap().file_name().to_string_lossy().to_string();
            if name.starts_with("environment") && name.ends_with(".yaml") {
                load_env_file(&dir, &name);
                n += 1;
            }
        }
    }
    assert_eq!(n, 6);
}

#[test]
fn default_environment() {
    let support = avida_core().join("support/config");
    let config = load_config(&support, &TestArgs::default()).unwrap();
    let env = load_env("support/config/avida.yaml", &config);

    let names = [
        "NOT", "NAND", "AND", "ORN", "OR", "ANDN", "NOR", "XOR", "EQU",
    ];
    let tasks = [
        "not", "nand", "and", "orn", "or", "andn", "nor", "xor", "equ",
    ];
    let descs = [
        "Not", "Nand", "And", "OrNot", "Or", "AndNot", "Nor", "Xor", "Equals",
    ];
    let values = [1.0, 1.0, 2.0, 2.0, 3.0, 3.0, 4.0, 4.0, 5.0];

    assert_eq!(env.num_reactions(), 9);
    assert_eq!(env.num_tasks(), 9);
    assert_eq!(env.resource_lib().size(), 0);
    for i in 0..9 {
        let r = env.reaction_lib().reaction(i);
        assert_eq!(r.id(), i as i32);
        assert_eq!(r.name(), names[i]);
        assert_eq!(r.task, Some(i as i32));
        assert_eq!(env.tasklib().task_name(i as i32), tasks[i]);
        assert_eq!(env.tasklib().task_desc(i as i32), descs[i]);
        assert!(r.active);
        assert_eq!(r.processes.len(), 1);
        let p = &r.processes[0];
        assert_eq!(p.value, values[i]);
        assert_eq!(p.proc_type, proc_type::POW);
        assert_eq!(p.resource, None);
        assert_eq!(p.max_number, 1.0);
        assert_eq!(p.min_number, 0.0);
        assert_eq!(p.max_fraction, 1.0);
        assert!(p.depletable);
        assert_eq!(r.requisites.len(), 1);
        let q = &r.requisites[0];
        // `max_count` limits the task count (SetMaxTaskCount).
        assert_eq!(q.max_task_count, 1);
        assert_eq!(q.min_task_count, 0);
        assert_eq!(q.max_reaction_count, i32::MAX);
        assert_eq!(q.min_reaction_count, 0);
        assert_eq!(q.divide_only, 0);
        assert!(q.prior_reactions.is_empty());
        assert!(r.context_requisites.is_empty());
        assert_eq!(env.reaction_value(i as i32), values[i]);
    }
    assert_eq!(env.reaction_value_by_name("XOR"), 4.0);
    assert_eq!(env.reaction_value_by_name("missing"), 0.0);
    // Each new task is an action trigger, ordered by task id.
    assert_eq!(env.action_triggers().len(), 9);
    assert_eq!(env.action_triggers()[4].id, "or");
    assert_eq!(env.action_triggers()[4].temp_ordering, 4);
}

#[test]
fn nine_resource_environment() {
    let misc = avida_core().join("support/config/misc");
    let env = load_env_file(&misc, "environment-9resource.yaml");
    assert_eq!(env.resource_lib().size(), 9);
    let res = env.resource_lib().resource(0);
    assert_eq!(res.name(), "resNOT");
    assert_eq!(res.inflow, 100.0);
    assert_eq!(res.outflow, 0.01);
    assert_eq!(res.initial, 0.0);
    let p = &env.reaction_lib().reaction(0).processes[0];
    assert_eq!(p.resource, Some(0));
    assert_eq!(p.value, 1.0);
    assert_eq!(p.max_fraction, 0.0025);
    assert_eq!(p.max_number, 25.0);
    assert_eq!(p.proc_type, proc_type::ADD);
    assert!(env.reaction_lib().reaction(0).requisites.is_empty());
    let p = &env.reaction_lib().reaction(8).processes[0];
    assert_eq!(p.resource, Some(8));
    assert_eq!(p.value, 16.0);
}
