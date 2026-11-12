//! Regression tests for event actions that no `avida-core/tests` directory
//! exercises. Each directory under `tests/actions/` is one small experiment
//! whose expected output was produced by the C++ reference binary:
//!
//! - `case.txt`: `base <test>` names the `avida-core/tests/<test>/config`
//!   directory the run starts from; `args <...>` gives the command line;
//!   `rc <n>` the expected exit status (default 0).
//! - `events.cfg` replaces the base configuration's event file.
//! - `extra/` (optional) holds files copied over the configuration.
//! - `stdin.txt` (optional) is fed to the run as standard input (the
//!   interactive analyze loop); without it standard input is empty.
//! - `expected/` holds every file the run must write (`_stdout` is the
//!   standard output; `_stderr`, when present, the standard error).
//!   Timestamp comment lines are ignored.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

fn tests_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../../avida-core/tests")
}

fn cases_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/actions")
}

fn copy_dir(from: &Path, to: &Path) {
    fs::create_dir_all(to).unwrap();
    for entry in fs::read_dir(from).unwrap() {
        let entry = entry.unwrap();
        let dest = to.join(entry.file_name());
        if entry.file_type().unwrap().is_dir() {
            copy_dir(&entry.path(), &dest);
        } else {
            fs::copy(entry.path(), dest).unwrap();
        }
    }
}

fn files_under(dir: &Path, base: &Path, out: &mut Vec<PathBuf>) {
    for entry in fs::read_dir(dir).unwrap() {
        let entry = entry.unwrap();
        if entry.file_type().unwrap().is_dir() {
            files_under(&entry.path(), base, out);
        } else {
            out.push(entry.path().strip_prefix(base).unwrap().to_path_buf());
        }
    }
}

/// `# Tue Nov 10 18:24:44 2026`
fn is_timestamp(line: &[u8]) -> bool {
    line.len() > 10
        && line.starts_with(b"# ")
        && line[2].is_ascii_uppercase()
        && line[5] == b' '
        && line[6].is_ascii_uppercase()
        && line[9] == b' '
}

fn normalized(bytes: &[u8]) -> Vec<Vec<u8>> {
    bytes
        .split(|&b| b == b'\n')
        .filter(|l| !is_timestamp(l))
        .map(|l| {
            if l.starts_with(b"Data Directory: ") {
                b"Data Directory: D".to_vec()
            } else {
                l.to_vec()
            }
        })
        .collect()
}

/// The event file the configuration names (`EVENT_FILE`, default
/// `events.cfg`).
fn event_file_of(config: &Path) -> String {
    let text = fs::read_to_string(config.join("avida.cfg")).unwrap_or_default();
    for line in text.lines() {
        let mut words = line.split_whitespace();
        if words.next() == Some("EVENT_FILE") {
            if let Some(f) = words.next() {
                return f.to_string();
            }
        }
    }
    "events.cfg".to_string()
}

fn run_case(name: &str) -> Result<(), String> {
    let case = cases_root().join(name);
    let spec = fs::read_to_string(case.join("case.txt")).map_err(|e| e.to_string())?;
    let mut base = String::new();
    let mut args: Vec<String> = Vec::new();
    let mut rc = 0;
    for line in spec.lines() {
        match line.split_once(' ') {
            Some(("base", v)) => base = v.trim().to_string(),
            Some(("args", v)) => args = v.split_whitespace().map(str::to_string).collect(),
            Some(("rc", v)) => rc = v.trim().parse().map_err(|_| "bad rc".to_string())?,
            _ => {}
        }
    }
    let work = Path::new(env!("CARGO_TARGET_TMPDIR"))
        .join("actions")
        .join(name);
    let _ = fs::remove_dir_all(&work);
    let config = tests_root().join(&base).join("config");
    copy_dir(&config, &work);
    if case.join("extra").is_dir() {
        copy_dir(&case.join("extra"), &work);
    }
    fs::copy(case.join("events.cfg"), work.join(event_file_of(&work)))
        .map_err(|e| e.to_string())?;

    let stdin = match fs::File::open(case.join("stdin.txt")) {
        Ok(f) => Stdio::from(f),
        Err(_) => Stdio::null(),
    };
    let output = Command::new(env!("CARGO_BIN_EXE_avida"))
        .args(&args)
        .current_dir(&work)
        .stdin(stdin)
        .output()
        .map_err(|e| e.to_string())?;
    if output.status.code() != Some(rc) {
        return Err(format!(
            "exit {:?} (expected {rc}): {}",
            output.status.code(),
            String::from_utf8_lossy(&output.stderr)
        ));
    }

    let expected = case.join("expected");
    let mut files = Vec::new();
    files_under(&expected, &expected, &mut files);
    files.sort();
    let mut diffs = Vec::new();
    for f in &files {
        let want = fs::read(expected.join(f)).unwrap();
        let got = if f.as_os_str() == "_stdout" {
            output.stdout.clone()
        } else if f.as_os_str() == "_stderr" {
            output.stderr.clone()
        } else {
            match fs::read(work.join(f)) {
                Ok(b) => b,
                Err(_) => {
                    diffs.push(format!("missing {}", f.display()));
                    continue;
                }
            }
        };
        if normalized(&want) != normalized(&got) {
            diffs.push(format!("differs {}", f.display()));
        }
    }
    if diffs.is_empty() {
        Ok(())
    } else {
        Err(diffs.join(", "))
    }
}

#[test]
fn action_cases_match_reference_output() {
    let mut names: Vec<String> = fs::read_dir(cases_root())
        .map(|rd| {
            rd.filter_map(Result::ok)
                .filter(|e| e.path().join("case.txt").exists())
                .map(|e| e.file_name().to_string_lossy().into_owned())
                .collect()
        })
        .unwrap_or_default();
    names.sort();
    assert!(!names.is_empty(), "no cases under tests/actions");

    let failures: Vec<String> = std::thread::scope(|scope| {
        let handles: Vec<_> = names
            .iter()
            .map(|name| scope.spawn(move || run_case(name).err().map(|e| format!("{name}: {e}"))))
            .collect();
        handles
            .into_iter()
            .filter_map(|h| h.join().unwrap())
            .collect()
    });
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}
