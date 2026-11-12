//! `EXECUTION_MODE 1`, the parallel execution mode: a run depends on the
//! seed only (not on `EXECUTION_THREADS`), spends exactly each update's
//! instructions, falls back to the exact code path where it must, and
//! rejects at start-up what it does not support. The exact mode is covered
//! by the consistency tests.

use std::fs;
use std::path::{Path, PathBuf};

use avida_config::cmdline::process_cmdline;
use avida_world::parallel::ParallelStats;
use avida_world::run::{Stepper, WatchedStep};
use avida_world::world::World;

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

fn fresh_config(test: &str, name: &str) -> PathBuf {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR"))
        .join("parallel")
        .join(name);
    let _ = fs::remove_dir_all(&dir);
    let src = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../../../avida-core/tests")
        .join(test)
        .join("config");
    copy_dir(&src, &dir);
    dir
}

/// `World::new` in `dir` with `-set NAME VALUE` for each of `sets`.
fn try_world(dir: &Path, sets: &[(&str, &str)]) -> Result<World, String> {
    let cwd = dir.to_string_lossy().into_owned();
    let mut args = vec!["avida".to_string()];
    for (name, value) in sets {
        args.extend(["-set".to_string(), name.to_string(), value.to_string()]);
    }
    let cmd = process_cmdline(&args, &cwd);
    assert!(cmd.exit.is_none(), "command line: {}", cmd.stderr);
    World::new(cmd.config, &cwd, &cmd.defs).map(|(w, _)| w)
}

fn is_timestamp(line: &[u8]) -> bool {
    line.len() > 10
        && line.starts_with(b"# ")
        && line[2].is_ascii_uppercase()
        && line[5] == b' '
        && line[6].is_ascii_uppercase()
        && line[9] == b' '
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
    out.sort();
}

fn normalized(path: &Path) -> Vec<Vec<u8>> {
    fs::read(path)
        .unwrap()
        .split(|&b| b == b'\n')
        .filter(|l| !is_timestamp(l))
        .map(<[u8]>::to_vec)
        .collect()
}

/// Whether two runs printed the same status lines and wrote the same files
/// (time stamps aside); the first difference otherwise.
fn compare_runs(
    (dir_a, out_a): (&Path, &[u8]),
    (dir_b, out_b): (&Path, &[u8]),
) -> Result<(), String> {
    if out_a != out_b {
        return Err("status lines".to_string());
    }
    let (mut fa, mut fb) = (Vec::new(), Vec::new());
    files_under(&dir_a.join("data"), dir_a, &mut fa);
    files_under(&dir_b.join("data"), dir_b, &mut fb);
    if fa != fb {
        return Err(format!("file lists {fa:?} / {fb:?}"));
    }
    assert!(!fa.is_empty());
    for f in &fa {
        if normalized(&dir_a.join(f)) != normalized(&dir_b.join(f)) {
            return Err(format!("{} differs", f.display()));
        }
    }
    Ok(())
}

/// A run of `test` driven update by update through a `Stepper`, with
/// `sets` on the command line: its directory, its status lines, the
/// parallel mode's counters, and the sum of the updates' sizes.
fn run(
    test: &str,
    name: &str,
    sets: &[(&str, &str)],
) -> (PathBuf, Vec<u8>, Option<ParallelStats>, u64) {
    run_with_events(test, name, sets, None)
}

/// [`run`] with `events` in place of the test's `events.cfg`.
fn run_with_events(
    test: &str,
    name: &str,
    sets: &[(&str, &str)],
    events: Option<&str>,
) -> (PathBuf, Vec<u8>, Option<ParallelStats>, u64) {
    let dir = fresh_config(test, &format!("{test}_{name}"));
    if let Some(events) = events {
        fs::write(dir.join("events.cfg"), events).unwrap();
    }
    let mut w = try_world(&dir, sets).unwrap_or_else(|e| panic!("World::new: {e}"));
    let mut stepper = Stepper::new(&w);
    let mut out = Vec::new();
    let mut instructions = 0u64;
    while stepper.begin_update(&mut w, &mut out).unwrap() {
        let (_, ud_size) = stepper.slice_progress();
        if w.pop.num_organisms > 0 {
            instructions += ud_size as u64;
        }
        while stepper.step_slice(&mut w).unwrap() {}
        stepper.end_update(&mut w, &mut out).unwrap();
    }
    w.out.flush_all();
    (dir, out, stepper.parallel_stats(), instructions)
}

const PARALLEL: (&str, &str) = ("EXECUTION_MODE", "1");

#[test]
fn parallel_run_does_not_depend_on_the_thread_count() {
    let test = "heads_default_100u";
    let (d1, o1, s1, n1) = run(test, "t1", &[PARALLEL, ("EXECUTION_THREADS", "1")]);
    let s1 = s1.expect("EXECUTION_MODE 1 runs the parallel mode");
    // The run executed the instructions of all its updates (up to the
    // carry-over into the next one), most of them in bursts. The carry-over
    // is what the last update's newborn grants and deaths left unbalanced
    // (here two births and a death in update 100, about 2% of that
    // update), not a drift that grows over the run.
    let executed = s1.parallel_insts + s1.sequential_insts;
    assert_eq!(executed as i64, n1 as i64 + s1.carry_over);
    assert!(s1.carry_over.unsigned_abs() * 100 < n1, "{}", s1.summary());
    assert!(s1.sequential_insts * 20 < n1, "{}", s1.summary());
    for threads in ["2", "4"] {
        let (d, o, s, n) = run(
            test,
            &format!("t{threads}"),
            &[PARALLEL, ("EXECUTION_THREADS", threads)],
        );
        let s = s.unwrap();
        assert_eq!(n, n1);
        assert_eq!(
            (
                s.parallel_insts,
                s.sequential_insts,
                s.rounds,
                s.bursts,
                s.carry_over
            ),
            (
                s1.parallel_insts,
                s1.sequential_insts,
                s1.rounds,
                s1.bursts,
                s1.carry_over
            )
        );
        if let Err(e) = compare_runs((&d1, &o1), (&d, &o)) {
            panic!("EXECUTION_THREADS 1 and {threads}: {e}");
        }
    }
    // It is a different schedule from the exact mode's.
    let (de, oe, se, _) = run(test, "exact", &[]);
    assert!(se.is_none());
    assert!(compare_runs((&d1, &o1), (&de, &oe)).is_err());
}

#[test]
fn full_world_does_not_depend_on_the_thread_count() {
    // Every cell occupied from the start, so that the rounds are large
    // enough for the worker pool.
    let events = "u begin InjectAll filename=default-classic.org\n\
                  u 0:5:end PrintAverageData\n\
                  u 0:5:end PrintCountData\n\
                  u 0:5:end PrintTasksData\n\
                  u 40 SavePopulation\n\
                  u 40 Exit\n";
    let test = "heads_default_100u";
    let one = &[PARALLEL, ("EXECUTION_THREADS", "1")];
    let (d1, o1, s1, _) = run_with_events(test, "full_t1", one, Some(events));
    let s1 = s1.unwrap();
    assert_eq!(s1.inline_bursts, s1.bursts);
    let four = &[PARALLEL, ("EXECUTION_THREADS", "4")];
    let (d4, o4, s4, _) = run_with_events(test, "full_t4", four, Some(events));
    let s4 = s4.unwrap();
    // The worker pool ran most of them.
    assert!(s4.inline_bursts * 2 < s4.bursts, "{}", s4.summary());
    assert_eq!(
        (
            s4.parallel_insts,
            s4.sequential_insts,
            s4.rounds,
            s4.carry_over
        ),
        (
            s1.parallel_insts,
            s1.sequential_insts,
            s1.rounds,
            s1.carry_over
        )
    );
    compare_runs((&d1, &o1), (&d4, &o4)).unwrap();
}

#[test]
fn execution_threads_does_not_change_the_exact_mode() {
    let test = "heads_default_100u";
    let (da, oa, _, _) = run(test, "exact_a", &[]);
    let (db, ob, sb, _) = run(
        test,
        "exact_b",
        &[("EXECUTION_MODE", "0"), ("EXECUTION_THREADS", "4")],
    );
    assert!(sb.is_none());
    compare_runs((&da, &oa), (&db, &ob)).unwrap();
}

#[test]
fn hardware_without_bursts_runs_through_the_exact_path() {
    // Transsmt (parasites) has no parallel allow-list: every instruction
    // runs in the sequential phase, still independent of the thread count.
    let test = "parasites_inject_stride";
    let (d1, o1, s1, n1) = run(test, "t1", &[PARALLEL, ("EXECUTION_THREADS", "1")]);
    let (d4, o4, s4, _) = run(test, "t4", &[PARALLEL, ("EXECUTION_THREADS", "4")]);
    let (s1, s4) = (s1.unwrap(), s4.unwrap());
    assert_eq!(s1.parallel_insts, 0);
    assert_eq!(s1.sequential_insts as i64, n1 as i64 + s1.carry_over);
    assert_eq!(s4.sequential_insts, s1.sequential_insts);
    compare_runs((&d1, &o1), (&d4, &o4)).unwrap();
}

#[test]
fn parallel_update_is_one_slice() {
    let dir = fresh_config("heads_default_100u", "one_slice");
    let mut w = try_world(&dir, &[PARALLEL, ("EXECUTION_THREADS", "2")]).unwrap();
    let mut stepper = Stepper::new(&w);
    assert!(stepper.parallel());
    assert!(!stepper.speculative());
    let mut out = Vec::new();
    for _ in 0..30 {
        assert!(stepper.begin_update(&mut w, &mut out).unwrap());
        let (_, ud_size) = stepper.slice_progress();
        assert_eq!(stepper.step_watched(&mut w, 0).unwrap(), WatchedStep::Slice);
        assert_eq!(stepper.slice_progress(), (ud_size, ud_size));
        assert_eq!(
            stepper.step_watched(&mut w, 0).unwrap(),
            WatchedStep::UpdateDone
        );
        assert!(!stepper.step_slice(&mut w).unwrap());
        stepper.end_update(&mut w, &mut out).unwrap();
    }
}

#[test]
fn unsupported_settings_are_rejected_at_start_up() {
    let dir = fresh_config("demes_germline", "demes");
    let err = try_world(&dir, &[PARALLEL])
        .err()
        .expect("NUM_DEMES 100 is rejected");
    assert!(
        err.contains("unsupported in EXECUTION_MODE 1: NUM_DEMES 100"),
        "{err}"
    );
    let dir = fresh_config("heads_default_100u", "mode2");
    let err = try_world(&dir, &[("EXECUTION_MODE", "2")])
        .err()
        .expect("EXECUTION_MODE 2 is rejected");
    assert!(err.contains("unsupported: EXECUTION_MODE 2"), "{err}");
}
