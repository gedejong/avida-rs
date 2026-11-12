//! Runs the `avida` binary on the consistency tests of `avida-core/tests`
//! and compares every file under `expected/` with what the run wrote, the
//! way `run_tests` does. Timestamp comment lines are ignored.
//!
//! `tests/consistency_passing.txt` lists the test directories this port
//! reproduces byte for byte; a test leaves the list only with a reason
//! recorded in `docs/test_matrix.json`.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

fn tests_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../../avida-core/tests")
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
        } else if !entry.file_name().to_string_lossy().starts_with('.') {
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

fn normalized(path: &Path) -> Vec<Vec<u8>> {
    fs::read(path)
        .unwrap_or_else(|e| panic!("{}: {e}", path.display()))
        .split(|&b| b == b'\n')
        .filter(|l| !is_timestamp(l))
        .map(<[u8]>::to_vec)
        .collect()
}

/// The `args = ...` line of the test's `test_list`.
fn args_of(test: &Path) -> Vec<String> {
    let text = fs::read_to_string(test.join("test_list")).unwrap();
    for line in text.lines() {
        let Some((key, value)) = line.split_once('=') else {
            continue;
        };
        if key.trim() == "args" {
            let value = value.split(';').next().unwrap_or("");
            return value.split_whitespace().map(str::to_string).collect();
        }
    }
    Vec::new()
}

fn run_test(name: &str) -> Result<(), String> {
    let test = tests_root().join(name);
    let work = Path::new(env!("CARGO_TARGET_TMPDIR")).join(name);
    let _ = fs::remove_dir_all(&work);
    copy_dir(&test.join("config"), &work);

    let output = Command::new(env!("CARGO_BIN_EXE_avida"))
        .args(args_of(&test))
        .current_dir(&work)
        .output()
        .map_err(|e| e.to_string())?;
    if !output.status.success() {
        return Err(format!(
            "exit {:?}: {}",
            output.status.code(),
            String::from_utf8_lossy(&output.stderr)
        ));
    }

    let expected = test.join("expected");
    let mut files = Vec::new();
    files_under(&expected, &expected, &mut files);
    files.sort();
    let mut diffs = Vec::new();
    for f in &files {
        let got = work.join(f);
        if !got.exists() {
            diffs.push(format!("missing {}", f.display()));
        } else if normalized(&expected.join(f)) != normalized(&got) {
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
fn consistency_tests_match_expected_output() {
    let list = include_str!("consistency_passing.txt");
    let names: Vec<&str> = list
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty() && !l.starts_with('#'))
        .collect();
    assert!(!names.is_empty());

    let failures: Vec<String> = std::thread::scope(|scope| {
        let handles: Vec<_> = names
            .iter()
            .map(|name| scope.spawn(move || run_test(name).err().map(|e| format!("{name}: {e}"))))
            .collect();
        handles
            .into_iter()
            .filter_map(|h| h.join().unwrap())
            .collect()
    });
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}
