//! The analyze job queue on worker threads gives the output of running the
//! jobs one at a time, for any worker count.
//!
//! Each test runs one analyze script with `MAX_CONCURRENCY 1` (the
//! single-threaded queue, the oracle) and then several times each with 2
//! and 8 workers, and compares everything the run produced byte for byte:
//! stdout, stderr, what the test CPU printed through `avida_apto::diag`,
//! the result, and every file in the data directory (only `cDataFile`
//! timestamp lines may differ). It also checks, through
//! [`Analyze::worker_stats`], that the work really ran on the workers.
//!
//! The scripts run in a copy of avida-core/tests/analyze_truncate_lineage/
//! config with `-s 100`; paths in them resolve against that directory, so
//! the process's current directory is never changed.

use std::collections::BTreeMap;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use avida_analyze::{Analyze, AnalyzeWorld, WorkerStats};
use avida_apto::diag;

fn base_config() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../../../avida-core/tests/analyze_truncate_lineage/config")
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

fn without_timestamps(text: &str) -> String {
    text.split_inclusive('\n')
        .filter(|l| !is_timestamp(l.trim_end_matches('\n')))
        .collect()
}

fn files_under(dir: &Path, prefix: &Path, out: &mut BTreeMap<PathBuf, String>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries {
        let entry = entry.unwrap();
        let rel = prefix.join(entry.file_name());
        if entry.file_type().unwrap().is_dir() {
            files_under(&entry.path(), &rel, out);
        } else {
            let bytes = std::fs::read(entry.path()).unwrap();
            out.insert(rel, without_timestamps(&String::from_utf8_lossy(&bytes)));
        }
    }
}

/// Everything one run produced.
struct Run {
    result: String,
    stdout: String,
    stderr: String,
    diag: diag::Captured,
    files: BTreeMap<PathBuf, String>,
    workers: usize,
    stats: WorkerStats,
}

/// Run `script` as `analyze.cfg` with `MAX_CONCURRENCY max_concurrency`
/// in a fresh copy of the base configuration plus `extra` files.
fn run(name: &str, script: &str, extra: &[(&str, &str)], max_concurrency: i32, rep: usize) -> Run {
    let work = Path::new(env!("CARGO_TARGET_TMPDIR"))
        .join("parallel")
        .join(format!("{name}-mc{max_concurrency}-{rep}"));
    if work.exists() {
        std::fs::remove_dir_all(&work).unwrap();
    }
    copy_dir(&base_config(), &work);
    for (file, text) in extra {
        std::fs::write(work.join(file), text).unwrap();
    }
    std::fs::write(work.join("analyze.cfg"), script).unwrap();
    let dir = work.to_str().unwrap();
    let args: Vec<String> = [
        "avida",
        "-s",
        "100",
        "-set",
        "MAX_CONCURRENCY",
        &max_concurrency.to_string(),
        "-a",
    ]
    .iter()
    .map(|s| s.to_string())
    .collect();
    let outcome = avida_config::cmdline::process_cmdline(&args, dir);
    assert!(outcome.exit.is_none(), "{name}: {}", outcome.stderr);
    let init = AnalyzeWorld::initialize(outcome.config, dir, &outcome.defs).unwrap();
    let mut world = init
        .world
        .unwrap_or_else(|| panic!("{name}: {}", init.stderr));
    let (out, err) = (Captured::default(), Captured::default());
    diag::start_capture();
    let mut analyze =
        Analyze::with_streams(&mut world, Box::new(out.clone()), Box::new(err.clone()));
    let result = analyze.run_file(&mut world, "analyze.cfg");
    let _ = analyze.flush();
    let printed = diag::stop_capture();
    let mut files = BTreeMap::new();
    files_under(&work.join("data"), Path::new(""), &mut files);
    Run {
        result: format!("{result:?}"),
        stdout: out.text(),
        stderr: err.text(),
        diag: printed,
        files,
        workers: analyze.job_workers(),
        stats: analyze.worker_stats(),
    }
}

fn first_difference(a: &str, b: &str) -> String {
    let at = a
        .lines()
        .zip(b.lines())
        .position(|(x, y)| x != y)
        .unwrap_or(a.lines().count().min(b.lines().count()));
    format!(
        "line {}:\n  serial:   {:?}\n  parallel: {:?}",
        at + 1,
        a.lines().nth(at),
        b.lines().nth(at)
    )
}

fn assert_same(what: &str, serial: &str, parallel: &str) {
    assert!(
        serial == parallel,
        "{what} differs at {}",
        first_difference(serial, parallel)
    );
}

fn assert_same_run(name: &str, serial: &Run, par: &Run) {
    let tag = format!("{name} with {} workers", par.workers);
    assert_eq!(serial.result, par.result, "{tag}: result");
    assert_same(&format!("{tag}: stdout"), &serial.stdout, &par.stdout);
    assert_same(&format!("{tag}: stderr"), &serial.stderr, &par.stderr);
    assert_same(
        &format!("{tag}: diag stdout"),
        &serial.diag.stdout,
        &par.diag.stdout,
    );
    assert_same(
        &format!("{tag}: diag stderr"),
        &serial.diag.stderr,
        &par.diag.stderr,
    );
    assert_eq!(
        serial.files.keys().collect::<Vec<_>>(),
        par.files.keys().collect::<Vec<_>>(),
        "{tag}: data files"
    );
    for (path, text) in &serial.files {
        assert_same(
            &format!("{tag}: {}", path.display()),
            text,
            &par.files[path],
        );
    }
}

/// Repetitions per worker count.
const REPS: usize = 2;

/// Run `script` serially, then `REPS` times with each of `counts`
/// workers (`MAX_CONCURRENCY`); every parallel run must match the serial
/// one, and `used` must accept its worker count and statistics.
fn check(
    name: &str,
    script: &str,
    extra: &[(&str, &str)],
    counts: &[i32],
    used: &dyn Fn(usize, &WorkerStats) -> bool,
) -> Run {
    let serial = run(name, script, extra, 1, 0);
    assert_eq!(serial.workers, 1, "{name}");
    assert_eq!(
        serial.stats,
        WorkerStats::default(),
        "{name}: serial run used workers"
    );
    assert!(
        serial.result.starts_with("Ok"),
        "{name}: {}\n--- stdout\n{}\n--- stderr\n{}",
        serial.result,
        serial.stdout,
        serial.stderr
    );
    for &mc in counts {
        for rep in 0..REPS {
            let par = run(name, script, extra, mc, rep);
            assert_same_run(name, &serial, &par);
            assert!(
                used(par.workers, &par.stats),
                "{name}, {} workers: {:?}",
                par.workers,
                par.stats
            );
            assert_eq!(par.stats.refusals, 0, "{name}: workers refused");
        }
    }
    serial
}

/// The landscape actions, each a job per genotype (per genotype and
/// distance for AnalyzeLandscape): the exhaustive ones on three genotypes,
/// those drawing from their job RNGs (Random/Sample/PairTest, the larger
/// distances of AnalyzeLandscape), where the seed order shows, on 31.
/// VERBOSE DETAILS prints the job-queue notices between the jobs' output.
#[test]
fn landscape_actions() {
    let script = "\
VERBOSE DETAILS
LOAD detail-50000.pop
FILTER num_cpus > 20
KEEP_TOP 3
FullLandscape land-full.dat 1 land-ent.dat land-sites.dat
DeletionLandscape land-del.dat 1 land-del-sites.dat
InsertionLandscape land-ins.dat 1 land-ins-sites.dat
AnalyzeLandscape land-analyze.dat 20 0 0 3
PrecalcLandscape
DETAIL precalc.dat id fitness frac_dead frac_neg frac_neut frac_pos land_fitness
VERBOSE NORMAL
SET_BATCH 1
LOAD detail-50000.pop
FILTER num_cpus > 10
RandomLandscape land-random.dat 2 20
SampleLandscape land-sample.dat 20
PairTestLandscape land-pairs.dat 20
";
    let serial = check("landscapes", script, &[], &[2, 8], &|w, s| {
        w < 2 || s.units == 3 * 7 + 3 * 31
    });
    assert!(serial.files.contains_key(Path::new("land-analyze.dat")));
}

/// `RECALCULATE` runs ahead of the world RNG, each genotype from the state
/// predicted for it. Every genotype draws (the divide's mutation checks),
/// a fixed number of times except for genotypes 9 and 20 of 30, which
/// execute `if-p-0.50` (an added 27th instruction, `A`) and draw a number
/// that depends on the values, so their successors' predicted states can
/// be wrong. With random inputs every genotype draws its inputs too.
/// SAMPLE_OFFSPRING draws from the world RNG afterwards, so a wrong RNG
/// state shows. Two workers run everything on the host.
#[test]
fn recalculate_speculation() {
    let instset = format!(
        "{}if-p-0.50  1   # A\n",
        std::fs::read_to_string(base_config().join("instset-heads.cfg")).unwrap()
    );
    let pop = std::fs::read_to_string(base_config().join("detail-50000.pop")).unwrap();
    let mut seqs: Vec<&str> = pop
        .lines()
        .filter(|l| !l.starts_with('#'))
        .filter_map(|l| l.split_whitespace().nth(12))
        .collect();
    seqs.dedup();
    let mut script = String::new();
    for (i, seq) in seqs.iter().take(30).enumerate() {
        let seq = if i == 9 || i == 20 {
            // An if-p-0.50 before the copy loop's if-label.
            seq.replacen("ycsva", "Aycsva", 1)
        } else {
            seq.to_string()
        };
        script.push_str(&format!("LOAD_SEQUENCE {seq}\n"));
    }
    script.push_str(
        "\
RECALCULATE
DETAIL recalc.dat id fitness merit gest_time length copy_length exe_length viable task.0 task.1 task.2 task.3 task.4 task.5 task.6 task.7 task.8 sequence
RECALC use_random_inputs
DETAIL recalc-random.dat id fitness merit gest_time task.0 task.1 task.2 task.3 task.4 task.5 task.6 task.7 task.8
RECALCULATE 0 -1 1
DETAIL recalc-random2.dat id fitness merit gest_time task.0 task.1 task.2 task.3 task.4 task.5 task.6 task.7 task.8
RECALCULATE
DETAIL recalc-again.dat id fitness merit gest_time
SAMPLE_OFFSPRING 3
DETAIL offspring.dat id num_cpus length sequence
",
    );
    let serial = check(
        "recalculate",
        &script,
        &[("instset-heads.cfg", &instset)],
        &[2, 3, 8],
        &|w, s| {
            if w < 3 {
                *s == WorkerStats::default()
            } else {
                s.speculative_rounds >= 4 && s.speculative_jobs >= 4 * 30 - 8
            }
        },
    );
    assert!(serial.files.contains_key(Path::new("offspring.dat")));
}

/// `MutationalNeighborhood` (one unit of 1 + length jobs per genotype, on
/// short genomes) and `CALC_FUNCTIONAL_MODULARITY` (the first viable
/// genotype runs on the host and activates the NULL instruction, the rest
/// on the workers).
#[test]
fn mutational_neighborhood_and_modularity() {
    let script = "\
VERBOSE DETAILS
LOAD_SEQUENCE rucav
LOAD_SEQUENCE qcepoc
LOAD_SEQUENCE ttysva
LOAD_SEQUENCE abcdef
MutationalNeighborhood mutn.dat
VERBOSE NORMAL
SET_BATCH 1
LOAD detail-50000.pop
FILTER num_cpus > 10
CALC_FUNCTIONAL_MODULARITY
DETAIL modularity.dat id tasks_done insts_tasks tasks_prop ave_tasks_per_site ave_sites_per_task sites_per_task.0 sites_per_task.1 sites_per_task.2 sites_per_task.3 sites_per_task.4
";
    check("mutn_modularity", script, &[], &[2, 8], &|w, s| {
        w < 2 || s.units >= 4 + 25
    });
}
