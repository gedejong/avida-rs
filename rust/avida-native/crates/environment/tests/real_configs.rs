//! Loads every environment file shipped with Avida (avida-core/support/config
//! and each consistency test's config directory) through the native parser,
//! with the avida.cfg settings and `-set` / `-def` arguments the test uses.

use avida_config::config::Config;
use avida_config::feedback::Feedback;
use avida_environment::{proc_type, Environment, SimpleTaskLib, TaskRegistry};
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

fn avida_core() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../../avida-core")
}

/// `-c`, `-set` and `-def` arguments from a test's `test_list`.
#[derive(Default)]
struct TestArgs {
    cfg: Option<String>,
    sets: Vec<(String, String)>,
    defs: Vec<(String, String)>,
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
    let words: Vec<&str> = args.split_whitespace().collect();
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
            "-def" if i + 2 < words.len() => {
                out.defs
                    .push((words[i + 1].to_string(), words[i + 2].to_string()));
                i += 3;
            }
            _ => i += 1,
        }
    }
    out
}

fn load_config(dir: &Path, args: &TestArgs) -> Config {
    let mut config = Config::new();
    let cfg = args.cfg.clone().unwrap_or_else(|| "avida.cfg".to_string());
    if dir.join(&cfg).exists() {
        let mut fb = Feedback::new();
        assert!(
            config.load(
                &cfg,
                dir.to_str().unwrap(),
                Some(&mut fb),
                Some(&args.defs),
                false
            ),
            "config {} in {}",
            cfg,
            dir.display()
        );
    }
    let mut sets = args.sets.clone();
    config.set_from_map(&mut sets);
    config
}

/// Environment files in `dir`: the configured ENVIRONMENT_FILE plus every
/// file whose name starts with "env" or ends in "env.cfg".
fn env_files(dir: &Path, config: &Config) -> BTreeSet<String> {
    let mut files = BTreeSet::new();
    if dir.join(&config.environment_file).is_file() {
        files.insert(config.environment_file.clone());
    }
    for e in std::fs::read_dir(dir).unwrap() {
        let name = e.unwrap().file_name().to_string_lossy().to_string();
        if name.starts_with("env") || name.ends_with("env.cfg") {
            files.insert(name);
        }
    }
    files
}

fn load_env(dir: &Path, file: &str, config: &Config, defs: &[(String, String)]) {
    let mut env = Environment::new(config, SimpleTaskLib::new());
    let mut fb = Feedback::new();
    let r = env.load(file, dir.to_str().unwrap(), &mut fb, Some(defs), config);
    assert!(
        r.is_ok(),
        "{}/{}: {:?}\n{}",
        dir.display(),
        file,
        r,
        fb.render()
    );
    assert_eq!(
        fb.num_errors(),
        0,
        "{}/{}: {}",
        dir.display(),
        file,
        fb.render()
    );
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
        let config = load_config(&dir, &args);
        for file in env_files(&dir, &config) {
            load_env(&dir, &file, &config, &args.defs);
            loaded += 1;
        }
    }
    // 89 environment*.cfg files plus the other ENVIRONMENT_FILE targets.
    assert!(loaded >= 95, "only {} environment files loaded", loaded);
}

#[test]
fn load_support_environments() {
    let support = avida_core().join("support/config");
    let config = load_config(&support, &TestArgs::default());
    load_env(&support, "environment.cfg", &config, &[]);
    let misc = support.join("misc");
    let mut n = 0;
    for e in std::fs::read_dir(&misc).unwrap() {
        let name = e.unwrap().file_name().to_string_lossy().to_string();
        if name.starts_with("environment") {
            load_env(&misc, &name, &config, &[]);
            n += 1;
        }
    }
    assert_eq!(n, 4);
}

#[test]
fn default_environment() {
    let support = avida_core().join("support/config");
    let config = load_config(&support, &TestArgs::default());
    let mut env = Environment::new(&config, SimpleTaskLib::new());
    let mut fb = Feedback::new();
    env.load(
        "environment.cfg",
        support.to_str().unwrap(),
        &mut fb,
        None,
        &config,
    )
    .unwrap();

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
    let config = Config::new();
    let mut env = Environment::new(&config, SimpleTaskLib::new());
    let mut fb = Feedback::new();
    env.load(
        "environment-9resource.cfg",
        misc.to_str().unwrap(),
        &mut fb,
        None,
        &config,
    )
    .unwrap();
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
