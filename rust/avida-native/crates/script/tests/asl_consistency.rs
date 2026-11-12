//! Runs every `avida-core/tests/_asl_*` consistency test the way
//! `testrunner.py` does: copy `config/` to a scratch directory, run the
//! binary with the `args` of `test_list`, check the exit code against
//! `nonzeroexit` and compare the produced files with `expected/` (lines
//! starting with `#` and blank lines ignored). The repository's ASL tests have
//! no `expected/` directory, so the exit code is the check; when the
//! environment variable `AVIDA_S_REF` names a reference `avida-s`, stdout,
//! stderr and exit code are compared with it as well (banner excluded).

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

fn tests_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../../avida-core/tests")
}

fn copy_dir(src: &Path, dst: &Path) {
    fs::create_dir_all(dst).unwrap();
    for e in fs::read_dir(src).unwrap() {
        let e = e.unwrap();
        let t = dst.join(e.file_name());
        if e.path().is_dir() {
            copy_dir(&e.path(), &t);
        } else {
            fs::copy(e.path(), t).unwrap();
        }
    }
}

fn stripped(p: &Path) -> Vec<String> {
    fs::read_to_string(p)
        .unwrap_or_default()
        .lines()
        .map(|l| l.trim_start().to_string())
        .filter(|l| !l.is_empty() && !l.starts_with('#'))
        .collect()
}

fn ini(list: &str, key: &str) -> String {
    for l in list.lines() {
        if let Some((k, v)) = l.split_once('=') {
            if k.trim() == key {
                return v.split(';').next().unwrap_or("").trim().to_string();
            }
        }
    }
    String::new()
}

fn strip_banner(out: &[u8]) -> Vec<u8> {
    let b = avida_script::driver::banner_output().into_bytes();
    out.strip_prefix(&b[..]).unwrap_or(out).to_vec()
}

fn run_in(exe: &str, dir: &Path, args: &[&str]) -> (Vec<u8>, Vec<u8>, i32) {
    let o = Command::new(exe)
        .args(args)
        .current_dir(dir)
        .output()
        .unwrap();
    (o.stdout, o.stderr, o.status.code().unwrap_or(-1))
}

#[test]
fn asl_consistency_tests() {
    let exe = env!("CARGO_BIN_EXE_avida-s");
    let scratch = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join("asl_runs");
    let reference = std::env::var("AVIDA_S_REF").ok();
    let mut names: Vec<_> = fs::read_dir(tests_dir())
        .unwrap()
        .filter_map(|e| e.ok())
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .filter(|n| n.starts_with("_asl_"))
        .collect();
    names.sort();
    assert_eq!(names.len(), 28);
    for n in &names {
        let t = tests_dir().join(n);
        let list = fs::read_to_string(t.join("test_list")).unwrap();
        let args_s = ini(&list, "args");
        let args: Vec<&str> = args_s.split_whitespace().collect();
        let nz = ini(&list, "nonzeroexit");
        let run = scratch.join(n);
        let _ = fs::remove_dir_all(&run);
        copy_dir(&t.join("config"), &run);
        let (so, se, code) = run_in(exe, &run, &args);
        match nz.as_str() {
            "require" => assert_ne!(code, 0, "{n}: expected a failure exit"),
            "disallow" => assert_eq!(code, 0, "{n}: {}", String::from_utf8_lossy(&se)),
            _ => {}
        }
        let exp = t.join("expected");
        if exp.is_dir() {
            for e in fs::read_dir(&exp).unwrap() {
                let e = e.unwrap();
                assert_eq!(
                    stripped(&run.join(e.file_name())),
                    stripped(&e.path()),
                    "{n}"
                );
            }
        }
        if let Some(r) = &reference {
            let rdir = scratch.join(format!("{n}_ref"));
            let _ = fs::remove_dir_all(&rdir);
            copy_dir(&t.join("config"), &rdir);
            let (rso, rse, rc) = run_in(r, &rdir, &args);
            assert_eq!(strip_banner(&so), rso, "{n}: stdout");
            assert_eq!(se, rse, "{n}: stderr");
            assert_eq!(code, rc, "{n}: exit code");
        }
    }
}

#[test]
fn language_features_run() {
    use avida_script::driver::{default_library, run};
    let _ = (default_library, run);
}
