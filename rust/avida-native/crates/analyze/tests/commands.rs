//! Regression tests for the analyze commands that run genomes in the test
//! CPU or read the world RNG (MAP_TASKS, MAP_MUTATIONS, ANALYZE_KNOCKOUTS,
//! CALC_FUNCTIONAL_MODULARITY, ANALYZE_POP_COMPLEXITY, SAMPLE_OFFSPRING,
//! ANALYZE_COMPLEXITY, COMPETE, ANALYZE_FITNESS_TWO_SITES,
//! ANALYZE_LINEAGE_COMPLEXITY, ANALYZE_MUTS, MUTATION_REVERT,
//! ANALYZE_NEWINFO, ANALYZE_REDUNDANCY_BY_INST_FAILURE and the tree
//! statistics) and the population statistics
//! and writers, run through [`run_analyze`] with the real world.
//!
//! Each fixture under `tests/fixtures/` is an `analyze.cfg` run with
//! `-s 100 -a` in a copy of avida-core/tests/analyze_truncate_lineage/config
//! (plus the fixture's `input/` files, if any); `stdout.txt`, `stderr.txt`,
//! every file under `expected/` (the data directory) and under
//! `expected_cwd/` (files written to the current directory) were captured
//! from the C++ reference binary with the same script. Files are compared
//! byte for byte except for `cDataFile` timestamp lines.

use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, MutexGuard};

use avida_analyze::run_analyze;

/// Serialises the runs: each one changes the current directory.
static CWD: Mutex<()> = Mutex::new(());

fn lock_cwd() -> MutexGuard<'static, ()> {
    CWD.lock().unwrap_or_else(|e| e.into_inner())
}

fn base_config() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../../../avida-core/tests/analyze_truncate_lineage/config")
}

fn fixture(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures")
        .join(name)
}

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

fn files_under(dir: &Path, prefix: &Path, out: &mut Vec<PathBuf>) {
    for entry in std::fs::read_dir(dir).unwrap() {
        let entry = entry.unwrap();
        let rel = prefix.join(entry.file_name());
        if entry.file_type().unwrap().is_dir() {
            files_under(&entry.path(), &rel, out);
        } else {
            out.push(rel);
        }
    }
}

fn assert_same(what: &str, got: &str, want: &str) {
    let (g, w) = (without_timestamps(got), without_timestamps(want));
    if g != w {
        let at = g.iter().zip(&w).position(|(a, b)| a != b);
        let at = at.unwrap_or(g.len().min(w.len()));
        panic!(
            "{what} differs at line {}:\n  expected: {:?}\n  actual:   {:?}",
            at + 1,
            w.get(at),
            g.get(at)
        );
    }
}

/// Run fixture `name` and compare stdout, stderr and the data directory
/// with the reference's.
fn check(name: &str) {
    let fx = fixture(name);
    let work = Path::new(env!("CARGO_TARGET_TMPDIR"))
        .join("commands")
        .join(name);
    if work.exists() {
        std::fs::remove_dir_all(&work).unwrap();
    }
    copy_dir(&base_config(), &work);
    // Extra input files (detail files, instruction sets) of the fixture.
    if fx.join("input").is_dir() {
        copy_dir(&fx.join("input"), &work);
    }
    std::fs::copy(fx.join("analyze.cfg"), work.join("analyze.cfg")).unwrap();
    let args: Vec<String> = ["avida", "-s", "100", "-a"]
        .iter()
        .map(|s| s.to_string())
        .collect();
    let (out, err) = (Captured::default(), Captured::default());
    let result = {
        let _guard = lock_cwd();
        let cwd = std::env::current_dir().unwrap();
        std::env::set_current_dir(&work).unwrap();
        let r = run_analyze(
            work.to_str().unwrap(),
            &args,
            Box::new(out.clone()),
            Box::new(err.clone()),
        );
        std::env::set_current_dir(cwd).unwrap();
        r
    };
    match &result {
        Ok(0) => {}
        other => panic!(
            "{name}: {other:?}\n--- stdout\n{}\n--- stderr\n{}",
            out.text(),
            err.text()
        ),
    }
    assert_same(
        &format!("{name}: stdout"),
        &out.text(),
        &std::fs::read_to_string(fx.join("stdout.txt")).unwrap(),
    );
    assert_same(
        &format!("{name}: stderr"),
        &err.text(),
        &std::fs::read_to_string(fx.join("stderr.txt")).unwrap(),
    );
    let expected = fx.join("expected");
    let mut want = Vec::new();
    files_under(&expected, Path::new(""), &mut want);
    want.sort();
    let mut got = Vec::new();
    files_under(&work.join("data"), Path::new(""), &mut got);
    got.sort();
    assert_eq!(got, want, "{name}: the data directory holds other files");
    for rel in &want {
        assert_same(
            &format!("{name}: {}", rel.display()),
            &std::fs::read_to_string(work.join("data").join(rel)).unwrap(),
            &std::fs::read_to_string(expected.join(rel)).unwrap(),
        );
    }
    // Files the commands write to the current directory with a plain
    // `ofstream` (not through the output manager).
    let expected_cwd = fx.join("expected_cwd");
    if expected_cwd.is_dir() {
        let mut want = Vec::new();
        files_under(&expected_cwd, Path::new(""), &mut want);
        for rel in &want {
            assert_same(
                &format!("{name}: ./{}", rel.display()),
                &std::fs::read_to_string(work.join(rel)).unwrap_or_default(),
                &std::fs::read_to_string(expected_cwd.join(rel)).unwrap(),
            );
        }
    }
}

/// MAP_TASKS (text; HTML with links, `:blank` and the boolean column),
/// MAP_MUTATIONS (text, HTML), ANALYZE_KNOCKOUTS, CALC_FUNCTIONAL_MODULARITY
/// with the modularity data entries, and ANALYZE_POP_COMPLEXITY over
/// genomes of different lengths after the NULL instruction was activated.
#[test]
fn knockout_maps_and_modularity() {
    check("cpu_maps");
}

/// SAMPLE_OFFSPRING, ANALYZE_COMPLEXITY, COMPETE (scheduler and mutations
/// from the world RNG), ANALYZE_FITNESS_TWO_SITES,
/// ANALYZE_LINEAGE_COMPLEXITY (two sites) and ANALYZE_MUTS (fewer than 79
/// combinations: one progress dot).
#[test]
fn mutants_offspring_and_complexity() {
    check("mutants");
}

/// MUTATION_REVERT (50 random-input trials per reversion, look-ahead
/// wrapping past the end of the lineage), PRINT_TREE_STATS,
/// PRINT_CUMULATIVE_STEMMINESS, PRINT_GAMMA with the lineage-through-time
/// file, and ANALYZE_NEWINFO.
#[test]
fn lineage_reversion_tree_stats_and_new_info() {
    check("lineage");
}

/// Commands without the test CPU: HAMMING, LEVENSTEIN, SPECIES,
/// PRINT_DISTANCES, PRINT_DIVERSITY, RECOMBINE, RECOMBINE_SAMPLE and
/// MUTAGENIZE (world RNG), WRITE_CLONE, WRITE_INJECT_EVENTS,
/// WRITE_INJECT_INITIAL, WRITE_COMPETITION, ANALYZE_INSTRUCTIONS (HTML),
/// ANALYZE_INST_POP, MAP_DEPTH and ANALYZE_MUTATION_TRACEBACK.
#[test]
fn population_statistics_and_writers() {
    check("population");
}

/// The skeleton commands: COUNT_NEW_SIG_LINEAGES, COUNT_NOVEL_SKELETONS,
/// CALC_SKELETON_DIVERSITY, GET_LARGEST_SKELETON (with a non-viable
/// genome collapsing to one NULL), CHANGE_DEBUG, COMPLEXITY_DEBUG,
/// ALL_COMPLEXITY_BARRIERS (once reusing the first file's skeletons as the
/// coalescence file) and GET_SKELETONS (with repeated NULLs removed and the
/// stale sites past the end read).
#[test]
fn skeletons() {
    check("skeletons");
}

/// ANALYZE_MATE_SELECTION (no samples: NaN fractions; a minimum swap
/// fraction out of range), ANALYZE_COMPLEXITY_DELTA (knockout charts of
/// task-doing genotypes, dead mutants, insertions and deletions, all rates
/// zero) and COMMUNITY_COMPLEXITY (closest given genotypes; a one-genotype
/// community, whose genome length is taken as 0) and AVERAGE_MODULARITY
/// (header only, all task columns, the boolean flag, an unknown column, a
/// batch where task columns are read by column index).
#[test]
fn mate_selection_complexity_delta_and_community() {
    check("complexity");
}

/// ANALYZE_COMPLEXITY_TWO_SITES on a nine-instruction set (so the
/// vectorised `w_avg` sum has a fused remainder), with and without
/// mutation, resources and batch skipping, and a zero-fitness site left
/// mutated.
#[test]
fn complexity_two_sites() {
    check("complexity_two_sites");
}

/// PRINT_RESOURCE_FITNESS_MAP on a two-function `optimize` environment
/// with range and threshold reactions (merit capped at the maximum and
/// zeroed below the minimum), before any resource history is loaded, for
/// an update not in the history, and twice into one static file.
#[test]
fn resource_fitness_map() {
    check("resource_fitness_map");
}

/// The landscape actions PredictWLandscape and PredictNuLandscape (the
/// fitness chart, then 2 to 10 mutations sampled from it with the analyze
/// RNG), PairTestLandscape sampled (one genotype per batch: with several,
/// the C++ worker threads draw the job seeds in a racy order) and
/// exhaustive, AnalyzePopulation's analyze-mode warning, the action names
/// in any case, and every action on an empty batch (HillClimb included).
#[test]
fn landscape_predictions_and_pairs() {
    check("landscape_predict_pairs");
}

/// ANALYZE_REDUNDANCY_BY_INST_FAILURE with `nand`, `h-copy` and `h-search`
/// able to fail: on the nop-less copy of the instruction set the
/// nop-dependent genomes do not reproduce (and are left out), the nop-free
/// replicators are measured at five failure chances; the genotypes keep
/// the copy, so a second run copies the copy (the end above 0 is warned
/// about but kept) and RECALCULATE runs on it.
#[test]
fn redundancy_by_inst_failure() {
    check("redundancy_inst_failure");
}
