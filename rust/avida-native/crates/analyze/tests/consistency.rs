//! The analyze-mode consistency tests of avida-core/tests, run through
//! [`run_analyze`] with the real world and test CPU, and compared with their
//! `expected/` files.
//!
//! The comparison is first the one `testrunner.py` makes (leading
//! whitespace stripped, blank and `#` lines skipped), then a byte-for-byte
//! one in which only the timestamp line `cDataFile` writes (`# Fri Mar 20
//! 08:37:22 2026`) may differ.
//!
//! Each run copies the test's `config/` to a scratch directory and makes
//! that the process's current directory, as the testrunner does; the runs
//! are serialised because the current directory is process-wide.

use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, MutexGuard};

use avida_analyze::{run_analyze, AnalyzeError};

/// Serialises the runs: each one changes the current directory.
static CWD: Mutex<()> = Mutex::new(());

fn lock_cwd() -> MutexGuard<'static, ()> {
    CWD.lock().unwrap_or_else(|e| e.into_inner())
}

fn repo_tests() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../../avida-core/tests")
}

/// A `Write` whose bytes the test reads back afterwards.
#[derive(Clone, Default)]
struct Captured(Arc<Mutex<Vec<u8>>>);

impl Captured {
    fn text(&self) -> String {
        let buf = self.0.lock().unwrap_or_else(|e| e.into_inner());
        String::from_utf8_lossy(&buf).into_owned()
    }
}

impl Write for Captured {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        self.0
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .extend_from_slice(buf);
        Ok(buf.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

struct Outcome {
    work: PathBuf,
    result: Result<i32, AnalyzeError>,
    stdout: String,
    stderr: String,
}

fn copy_dir(from: &Path, to: &Path) {
    std::fs::create_dir_all(to).unwrap();
    for entry in std::fs::read_dir(from).unwrap() {
        let entry = entry.unwrap();
        let target = to.join(entry.file_name());
        if entry.file_type().unwrap().is_dir() {
            copy_dir(&entry.path(), &target);
        } else {
            std::fs::copy(entry.path(), &target).unwrap();
        }
    }
}

/// The `args = ...` line of the test's `test_list`, without its comment.
fn test_args(test: &Path) -> Vec<String> {
    let list = std::fs::read_to_string(test.join("test_list")).unwrap();
    let line = list
        .lines()
        .find_map(|l| l.trim_start().strip_prefix("args"))
        .and_then(|l| l.trim_start().strip_prefix('='))
        .expect("test_list has an args line");
    let line = line.split(';').next().unwrap_or("");
    line.split_whitespace().map(str::to_string).collect()
}

/// Copy the test's `config/` to a scratch directory and run `avida <args>`
/// there.
fn run(name: &str) -> Outcome {
    let test = repo_tests().join(name);
    let work = Path::new(env!("CARGO_TARGET_TMPDIR"))
        .join("consistency")
        .join(name);
    if work.exists() {
        std::fs::remove_dir_all(&work).unwrap();
    }
    copy_dir(&test.join("config"), &work);
    let mut args = vec!["avida".to_string()];
    args.extend(test_args(&test));

    let (out, err) = (Captured::default(), Captured::default());
    let _guard = lock_cwd();
    let cwd = std::env::current_dir().unwrap();
    std::env::set_current_dir(&work).unwrap();
    let result = run_analyze(
        work.to_str().unwrap(),
        &args,
        Box::new(out.clone()),
        Box::new(err.clone()),
    );
    std::env::set_current_dir(cwd).unwrap();
    // Kept beside the run directory for comparison with the reference's
    // console output.
    std::fs::write(work.with_extension("stdout"), out.text()).unwrap();
    std::fs::write(work.with_extension("stderr"), err.text()).unwrap();
    Outcome {
        work,
        result,
        stdout: out.text(),
        stderr: err.text(),
    }
}

/// `testrunner.py`'s view of a file: lines with leading whitespace
/// stripped, blank and comment lines dropped.
fn testrunner_lines(text: &str) -> Vec<&str> {
    text.lines()
        .map(str::trim_start)
        .filter(|l| !l.is_empty() && !l.starts_with('#'))
        .collect()
}

/// `cDataFile`'s timestamp comment (`# Fri Mar 20 08:37:22 2026`).
fn is_timestamp(line: &str) -> bool {
    const DAYS: [&str; 7] = ["Mon", "Tue", "Wed", "Thu", "Fri", "Sat", "Sun"];
    let Some(rest) = line.strip_prefix("# ") else {
        return false;
    };
    let words: Vec<&str> = rest.split_whitespace().collect();
    words.len() == 5
        && DAYS.contains(&words[0])
        && words[3].matches(':').count() == 2
        && words[4].len() == 4
        && words[4].bytes().all(|b| b.is_ascii_digit())
}

fn without_timestamps(text: &str) -> Vec<&str> {
    text.split_inclusive('\n')
        .filter(|l| !is_timestamp(l.trim_end_matches('\n')))
        .collect()
}

fn expected_files(dir: &Path, prefix: &Path, out: &mut Vec<PathBuf>) {
    for entry in std::fs::read_dir(dir).unwrap() {
        let entry = entry.unwrap();
        let rel = prefix.join(entry.file_name());
        if entry.file_type().unwrap().is_dir() {
            expected_files(&entry.path(), &rel, out);
        } else if entry.file_name() != ".gitignore" {
            out.push(rel);
        }
    }
}

/// Compare every file under the test's `expected/` with the run's copy;
/// returns the number of files compared.
fn assert_expected(name: &str, outcome: &Outcome) -> usize {
    let expected = repo_tests().join(name).join("expected");
    let mut files = Vec::new();
    expected_files(&expected, Path::new(""), &mut files);
    files.sort();
    for rel in &files {
        let want = std::fs::read(expected.join(rel)).unwrap();
        let got = std::fs::read(outcome.work.join(rel))
            .unwrap_or_else(|e| panic!("{name}: {} not written: {e}", rel.display()));
        let (want, got) = (
            String::from_utf8_lossy(&want).into_owned(),
            String::from_utf8_lossy(&got).into_owned(),
        );
        let (w, g) = (testrunner_lines(&want), testrunner_lines(&got));
        if w != g {
            let at = w.iter().zip(&g).position(|(a, b)| a != b);
            let at = at.unwrap_or(w.len().min(g.len()));
            panic!(
                "{name}: {} differs (testrunner comparison) at significant line {}:\n  \
                 expected: {:?}\n  actual:   {:?}",
                rel.display(),
                at + 1,
                w.get(at),
                g.get(at)
            );
        }
        assert!(
            without_timestamps(&want) == without_timestamps(&got),
            "{name}: {} matches for the testrunner but differs in comments or layout",
            rel.display()
        );
    }
    files.len()
}

fn assert_success(name: &str, outcome: &Outcome) {
    match &outcome.result {
        Ok(0) => {}
        other => panic!(
            "{name}: {other:?}\n--- stdout\n{}\n--- stderr\n{}",
            outcome.stdout, outcome.stderr
        ),
    }
}

/// A test that must exit 0 and reproduce its expected files.
fn consistent(name: &str) {
    let outcome = run(name);
    assert_success(name, &outcome);
    let n = assert_expected(name, &outcome);
    assert!(n > 0, "{name}: no expected files");
}

#[test]
fn analyze_dumplandscape() {
    consistent("analyze_dumplandscape");
}

#[test]
fn analyze_fulllandscape_1step() {
    consistent("analyze_fulllandscape_1step");
}

#[test]
fn analyze_print_withargs() {
    consistent("analyze_print_withargs");
}

#[test]
fn analyze_printphenotypes() {
    consistent("analyze_printphenotypes");
}

#[test]
fn analyze_printphenotypes_grouping() {
    consistent("analyze_printphenotypes_grouping");
}

#[test]
fn analyze_truncate_lineage() {
    consistent("analyze_truncate_lineage");
}

#[test]
fn task_sort_analyze() {
    consistent("task_sort_analyze");
}

/// Disabled upstream (leading `_`): its `avida.cfg` sets `INST_SET -` and
/// defines no instruction set, so the reference binary stops in
/// `cWorld::setup` with "error: no instruction sets defined" and exits 255
/// before analyze mode; this port does the same.
#[test]
fn analyze_detail_all() {
    let name = "_analyze_detail_all";
    let outcome = run(name);
    match outcome.result {
        Ok(code) => assert_eq!(code, -1, "{name}"),
        Err(e) => panic!("{name}: {e:?}"),
    }
    assert!(
        outcome
            .stderr
            .ends_with("error: no instruction sets defined\n"),
        "{name}: {}",
        outcome.stderr
    );
    assert!(!outcome.stdout.contains("In analyze mode!!"), "{name}");
}

/// `nonzeroexit = require`: `SET_BATCH 50000` is past `MAX_BATCHES`.
#[test]
fn analyze_resize_max_batches() {
    let name = "analyze_resize_max_batches";
    let outcome = run(name);
    match outcome.result {
        Ok(code) => assert_ne!(code & 0xFF, 0, "{name}: exit status must be nonzero"),
        Err(e) => panic!("{name}: {e:?}"),
    }
    assert_expected(name, &outcome);
}

/// `expected/` holds only `.gitignore`: the run must succeed.
#[test]
fn collect_specification_analyze() {
    let name = "collect_specification_analyze";
    let outcome = run(name);
    assert_success(name, &outcome);
    assert_expected(name, &outcome);
}

/// Marked long upstream (and disabled there by its leading `_`):
/// `MutationalNeighborhood` tests every one- and two-step mutant of a
/// 49-instruction genome, about 3.3 million test-CPU runs. The reference
/// binary cannot run it (it deadlocks with `MAX_CONCURRENCY 1` and crashes
/// with worker threads; see `Analyze::action_mutational_neighborhood`), so
/// the checked-in `expected/` file is the oracle.
#[test]
#[ignore = "long: MutationalNeighborhood, millions of test-CPU runs"]
fn mutn_rate() {
    consistent("_mutn_rate");
}

/// A test without `expected/`: the testrunner only requires exit 0.
fn succeeds(name: &str) {
    let outcome = run(name);
    assert_success(name, &outcome);
}

/// No `expected/`. Long: `FullLandscape` at distance 2 (25 s for the C++
/// release build).
#[test]
#[ignore = "long: FullLandscape at distance 2"]
fn analyze_fulllandscape_2step() {
    succeeds("analyze_fulllandscape_2step");
}

/// No `expected/`. Long in a debug build (`PrecalcLandscape` over the
/// lineage).
#[test]
#[ignore = "long: PrecalcLandscape over a lineage"]
fn analyze_truncate_lineage_fulllandscape() {
    succeeds("analyze_truncate_lineage_fulllandscape");
}

/// No `expected/`; marked long upstream.
#[test]
#[ignore = "long: RECALCULATE of a 100000-update population"]
fn hoarding_instn_analyze() {
    succeeds("hoarding_instn_analyze");
}
