//! Loads every real `avida*.cfg` in the repository (avida-core/tests/*/config
//! and avida-core/support/config) and spot-checks values.

use avida_config::cmdline::process_cmdline;
use avida_config::config::Config;
use avida_config::feedback::Feedback;
use std::path::{Path, PathBuf};

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../..")
}

fn config_dirs() -> Vec<PathBuf> {
    let core = repo_root().join("avida-core");
    let mut dirs = vec![core.join("support/config")];
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

fn cfg_files(dir: &Path) -> Vec<String> {
    let mut v: Vec<String> = std::fs::read_dir(dir)
        .unwrap()
        .filter_map(|e| e.ok())
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .filter(|n| n.starts_with("avida") && n.ends_with(".cfg"))
        .collect();
    v.sort();
    v
}

fn load(dir: &Path, name: &str) -> (Config, Feedback, bool) {
    let mut c = Config::new();
    let mut fb = Feedback::new();
    let ok = c.load(name, dir.to_str().unwrap(), Some(&mut fb), None, false);
    (c, fb, ok)
}

#[test]
fn every_repo_config_loads() {
    let mut n = 0;
    for d in config_dirs() {
        for f in cfg_files(&d) {
            let (_, fb, ok) = load(&d, &f);
            assert!(ok, "{}/{}: {}", d.display(), f, fb.render());
            assert_eq!(fb.num_errors(), 0, "{}/{}: {}", d.display(), f, fb.render());
            n += 1;
        }
    }
    assert!(n >= 90, "only {} configs found", n);
}

#[test]
fn every_repo_config_loads_through_cmdline() {
    for d in config_dirs() {
        if !d.join("avida.cfg").exists() {
            continue;
        }
        let o = process_cmdline(&["avida".to_string()], d.to_str().unwrap());
        // Every unknown line in these files would be listed as a warning; the
        // run must not stop.
        assert!(o.exit.is_none(), "{}: {}", d.display(), o.stderr);
    }
}

#[test]
fn support_config_instset_via_include() {
    let d = repo_root().join("avida-core/support/config");
    let (c, fb, ok) = load(&d, "avida.cfg");
    assert!(ok, "{}", fb.render());
    // `#include INST_SET=instset-heads.cfg` pulls in the INSTSET block.
    let inc = std::fs::read_to_string(d.join("instset-heads.cfg")).unwrap();
    let n_inst = inc
        .lines()
        .filter(|l| l.trim_start().starts_with("INST "))
        .count();
    let lines = c.instsets();
    assert!(!lines.is_empty());
    assert!(lines[0].starts_with("INSTSET "), "{:?}", lines[0]);
    assert_eq!(
        lines.iter().filter(|l| l.starts_with("INST ")).count(),
        n_inst
    );
    assert!(lines.iter().any(|l| l == "INST nop-A"));
    assert!(lines.iter().any(|l| l == "INST h-copy"));
    // other spot values from the support file
    assert_eq!(c.world_x, 60);
    assert_eq!(c.copy_mut_prob, 0.0075);
    assert_eq!(c.data_dir, "data");
    assert_eq!(c.inst_set, "-");
    assert_eq!(c.birth_method, 0);
}

#[test]
fn test_configs_spot_values() {
    let core = repo_root().join("avida-core");
    // The consistency tests set their own topology and seed.
    for name in ["sex", "large_pop", "heads_default_100u"] {
        let d = core.join("tests").join(name).join("config");
        let (c, fb, ok) = load(&d, "avida.cfg");
        assert!(ok, "{}: {}", name, fb.render());
        assert!(c.world_x > 0 && c.world_y > 0);
        let text = std::fs::read_to_string(d.join("avida.cfg")).unwrap();
        for l in text.lines() {
            let l = l.split('#').next().unwrap().trim();
            if let Some(v) = l.strip_prefix("WORLD_X ") {
                assert_eq!(c.world_x, v.trim().parse::<i32>().unwrap());
            }
            if let Some(v) = l.strip_prefix("RANDOM_SEED ") {
                assert_eq!(c.random_seed, v.trim().parse::<i32>().unwrap());
            }
            if let Some(v) = l.strip_prefix("COPY_MUT_PROB ") {
                assert_eq!(c.copy_mut_prob, v.trim().parse::<f64>().unwrap());
            }
        }
    }
}

#[test]
fn instset_blocks_captured_in_tests() {
    // Any test config with INSTSET lines must have them captured verbatim.
    let mut seen = 0;
    for d in config_dirs() {
        for f in cfg_files(&d) {
            let text = std::fs::read_to_string(d.join(&f)).unwrap();
            let expect = text
                .lines()
                .map(|l| {
                    l.split('#')
                        .next()
                        .unwrap()
                        .split_whitespace()
                        .collect::<Vec<_>>()
                        .join(" ")
                })
                .filter(|l| l.starts_with("INSTSET ") || l.starts_with("INST "))
                .count();
            if expect == 0 {
                continue;
            }
            let (c, _, ok) = load(&d, &f);
            assert!(ok);
            assert!(c.instsets().len() >= expect, "{}/{}", d.display(), f);
            seen += 1;
        }
    }
    assert!(seen >= 1);
}

#[test]
fn set_and_seed_handling_on_real_file() {
    let d = repo_root().join("avida-core/support/config");
    let args: Vec<String> = [
        "avida",
        "-s",
        "123",
        "-set",
        "WORLD_X",
        "17",
        "-set",
        "COPY_MUT_PROB",
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
