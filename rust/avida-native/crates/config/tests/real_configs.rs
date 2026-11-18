//! Loads every experiment file of the repository (avida-core/tests/*/config
//! and avida-core/support/config, with the section files there) and
//! spot-checks values.

use avida_config::cmdline::process_cmdline;
use avida_config::config::Config;
use avida_config::feedback::Feedback;
use std::path::{Path, PathBuf};

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../..")
}

fn config_dirs() -> Vec<PathBuf> {
    let core = repo_root().join("avida-core");
    let mut dirs = vec![
        core.join("support/config"),
        core.join("support/config/misc/Parasites"),
    ];
    let mut tests: Vec<PathBuf> = std::fs::read_dir(core.join("tests"))
        .unwrap()
        .filter_map(|e| e.ok())
        .map(|e| e.path().join("config"))
        .filter(|p| p.is_dir())
        .collect();
    tests.sort();
    dirs.extend(tests);
    dirs
}

fn load(dir: &Path, name: &str) -> (Config, Feedback, bool) {
    let mut c = Config::new();
    let mut fb = Feedback::new();
    let ok = c.load_experiment(name, dir.to_str().unwrap(), &mut fb);
    (c, fb, ok)
}

#[test]
fn every_repo_experiment_file_loads() {
    let mut n = 0;
    for d in config_dirs() {
        if !d.join("avida.yaml").exists() {
            continue;
        }
        let (_, fb, ok) = load(&d, "avida.yaml");
        assert!(ok, "{}: {}", d.display(), fb.render());
        assert_eq!(fb.num_errors(), 0, "{}: {}", d.display(), fb.render());
        n += 1;
    }
    assert!(n >= 90, "only {} experiment files found", n);
}

#[test]
fn every_repo_experiment_file_loads_through_cmdline() {
    for d in config_dirs() {
        if !d.join("avida.yaml").exists() {
            continue;
        }
        let o = process_cmdline(&["avida".to_string()], d.to_str().unwrap());
        assert!(o.exit.is_none(), "{}: {}", d.display(), o.stderr);
    }
}

#[test]
fn every_section_file_loads() {
    // The stock instruction sets and environments, and the examples in misc/.
    let mut n = 0;
    for dir in [
        repo_root().join("avida-core/support/config"),
        repo_root().join("avida-core/support/config/misc"),
    ] {
        let mut names: Vec<String> = std::fs::read_dir(&dir)
            .unwrap()
            .filter_map(|e| e.ok())
            .map(|e| e.file_name().to_string_lossy().into_owned())
            .filter(|n| n.ends_with(".yaml") && n != "avida.yaml")
            .collect();
        names.sort();
        for name in names {
            let section = if name.starts_with("instset-") {
                "instruction_sets"
            } else {
                "environment"
            };
            let mut c = Config::new();
            c.set_path(section, &name, dir.to_str().unwrap())
                .unwrap_or_else(|e| panic!("{name}: {e}"));
            n += 1;
        }
    }
    assert!(n >= 20, "only {n} section files found");
}

#[test]
fn support_config_values_and_stock_set() {
    let d = repo_root().join("avida-core/support/config");
    let (c, fb, ok) = load(&d, "avida.yaml");
    assert!(ok, "{}", fb.render());
    // `instruction_sets: [heads_default]` names the stock set.
    let lines = c.instsets();
    assert_eq!(lines[0], "INSTSET heads_default:hw_type=0");
    assert_eq!(lines.iter().filter(|l| l.starts_with("INST ")).count(), 26);
    assert!(lines.iter().any(|l| l == "INST nop-A"));
    assert!(lines.iter().any(|l| l == "INST h-copy"));
    // other spot values from the support file
    assert_eq!(c.world_x, 60);
    assert_eq!(c.copy_mut_prob, 0.0075);
    assert_eq!(c.data_dir, "data");
    assert_eq!(c.birth_method, 0);
    assert_eq!(c.experiment.environment.len(), 9);
    assert_eq!(c.experiment.events.len(), 9);
}

#[test]
fn test_configs_spot_values() {
    let core = repo_root().join("avida-core");
    // The consistency tests set their own topology and seed.
    for (name, x, seed) in [
        ("sex", 60, 7),
        ("large_pop", 200, 101),
        ("heads_default_100u", 60, 101),
    ] {
        let d = core.join("tests").join(name).join("config");
        let (c, fb, ok) = load(&d, "avida.yaml");
        assert!(ok, "{}: {}", name, fb.render());
        assert_eq!(c.world_x, x, "{name}");
        assert_eq!(c.random_seed, seed, "{name}");
    }
}

#[test]
fn set_and_seed_handling_on_real_file() {
    let d = repo_root().join("avida-core/support/config");
    let args: Vec<String> = [
        "avida",
        "-s",
        "123",
        "-set",
        "world.x",
        "17",
        "-set",
        "mutation.copy_prob",
        "0.1",
        "-v0",
    ]
    .iter()
    .map(|s| s.to_string())
    .collect();
    let o = process_cmdline(&args, d.to_str().unwrap());
    assert_eq!(o.exit, None, "{}", o.stderr);
    assert_eq!(o.config.random_seed, 123);
    assert_eq!(o.config.world_x, 17);
    assert_eq!(o.config.copy_mut_prob, 0.1);
    assert_eq!(o.config.verbosity, 0);
    assert!(!o.config.instsets().is_empty());
}
