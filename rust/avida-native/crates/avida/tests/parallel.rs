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
fn transsmt_with_parasites_runs_bursts_and_does_not_depend_on_the_thread_count() {
    // Transsmt hosts and their parasite threads run their organism-local
    // instructions in bursts (`SMT_PARALLEL_METHODS`); IO, divides and
    // injections stay in the sequential phase. Still a function of the
    // seed only.
    let test = "parasites_inject_stride";
    // (This test's configuration says VERBOSITY 3, which keeps the whole
    // run sequential: slip mutations would print from the workers.)
    let quiet = ("VERBOSITY", "0");
    let (d1, o1, s1, n1) = run(test, "t1", &[PARALLEL, ("EXECUTION_THREADS", "1"), quiet]);
    let s1 = s1.unwrap();
    let executed = s1.parallel_insts + s1.sequential_insts;
    assert_eq!(executed as i64, n1 as i64 + s1.carry_over);
    assert!(
        s1.parallel_insts > 3 * s1.sequential_insts,
        "{}",
        s1.summary()
    );
    for threads in ["2", "4"] {
        let (d, o, s, _) = run(
            test,
            &format!("t{threads}"),
            &[PARALLEL, ("EXECUTION_THREADS", threads), quiet],
        );
        let s = s.unwrap();
        assert_eq!(
            (s.parallel_insts, s.sequential_insts, s.rounds, s.bursts),
            (s1.parallel_insts, s1.sequential_insts, s1.rounds, s1.bursts)
        );
        if let Err(e) = compare_runs((&d1, &o1), (&d, &o)) {
            panic!("EXECUTION_THREADS 1 and {threads}: {e}");
        }
    }
}

#[test]
fn transsmt_without_bursts_when_a_setting_reaches_the_world() {
    // THREAD_SLICING_METHOD 1 (every thread runs per step) and the energy
    // model keep transsmt organisms out of the parallel phase; the run
    // still completes through the exact path.
    let test = "parasites_inject_stride";
    let (_, _, s, n) = run(
        test,
        "slicing1",
        &[PARALLEL, ("THREAD_SLICING_METHOD", "1"), ("VERBOSITY", "0")],
    );
    let s = s.unwrap();
    assert_eq!(s.parallel_insts, 0, "{}", s.summary());
    assert_eq!(s.sequential_insts as i64, n as i64 + s.carry_over);
}

/// The mean of column `col` (0-based) over the last `n` data rows of
/// `file` under `dir/data`.
fn tail_mean(dir: &Path, file: &str, col: usize, n: usize) -> f64 {
    let text = fs::read_to_string(dir.join("data").join(file)).unwrap();
    let rows: Vec<f64> = text
        .lines()
        .filter(|l| !l.starts_with('#') && !l.trim().is_empty())
        .map(|l| l.split_whitespace().nth(col).unwrap().parse().unwrap())
        .collect();
    let tail = &rows[rows.len() - n..];
    tail.iter().sum::<f64>() / n as f64
}

/// Welch's z for the difference of the means of `a` and `b`.
fn welch_z(a: &[f64], b: &[f64]) -> f64 {
    let stats = |v: &[f64]| {
        let n = v.len() as f64;
        let m = v.iter().sum::<f64>() / n;
        let var = v.iter().map(|x| (x - m).powi(2)).sum::<f64>() / (n - 1.0);
        (m, var / n)
    };
    let ((ma, va), (mb, vb)) = (stats(a), stats(b));
    (ma - mb) / (va + vb).sqrt().max(1e-12)
}

/// Hosts (and, with `parasites`, their parasites) on a 30 x 30 world over
/// several seeds in the exact mode and in the parallel mode (whose
/// TransSMT organisms run in bursts): the outcomes at the end should not
/// differ by more than the seeds' own scatter. The runs are functions of
/// their seeds, so the verdict does not vary between executions; the bound
/// (|z| < 3.5 per outcome) is the same loose one the heads CPU's
/// 16-seed comparison of ARCHITECTURE.md meets with room to spare.
fn transsmt_equivalence(parasites: bool) {
    let test = "parasites_inject_stride";
    let events = format!(
        "u begin Inject evolved-not.org\n\
         {}\
         u 1000:10:1100 PrintCountData\n\
         u 1000:10:1100 PrintAverageData\n\
         u 1000:10:1100 PrintParasiteTasksData\n\
         u 1100 Exit\n",
        if parasites {
            "u 800 InjectParasite parasite-smt.org ABB 0 900\n"
        } else {
            ""
        }
    );
    let world = ("WORLD_X", "30");
    let world_y = ("WORLD_Y", "30");
    // (organisms, average gestation time, parasites) per seed and mode
    let mut outcomes: [Vec<[f64; 3]>; 2] = [Vec::new(), Vec::new()];
    for seed in 1..=8 {
        let seed_text = seed.to_string();
        for (mode, extra) in [("0", None), ("1", Some(PARALLEL))] {
            let mut sets = vec![
                world,
                world_y,
                ("VERBOSITY", "0"),
                ("RANDOM_SEED", seed_text.as_str()),
            ];
            sets.extend(extra);
            let name = format!("equiv{}_{mode}_{seed}", u8::from(parasites));
            let (dir, _, stats, _) = run_with_events(test, &name, &sets, Some(&events));
            if mode == "1" {
                let stats = stats.unwrap();
                assert!(
                    stats.parallel_insts > stats.sequential_insts,
                    "{}",
                    stats.summary()
                );
            }
            let parasites_now = if parasites {
                tail_mean(&dir, "parasite_tasks.dat", 1, 5)
            } else {
                0.0
            };
            outcomes[usize::from(mode == "1")].push([
                tail_mean(&dir, "count.dat", 2, 5),
                tail_mean(&dir, "average.dat", 2, 5),
                parasites_now,
            ]);
        }
    }
    let n_outcomes = if parasites { 3 } else { 2 };
    // The comparison means something only when there is a population to
    // compare (and parasites in it).
    assert!(outcomes[1].iter().all(|o| o[0] > 100.0), "{outcomes:?}");
    if parasites {
        assert!(outcomes[1].iter().all(|o| o[2] > 2.0), "{outcomes:?}");
    }
    for k in 0..n_outcomes {
        let a: Vec<f64> = outcomes[0].iter().map(|o| o[k]).collect();
        let b: Vec<f64> = outcomes[1].iter().map(|o| o[k]).collect();
        let z = welch_z(&a, &b);
        assert!(
            z.abs() < 3.5,
            "outcome {k}: exact {a:?} against parallel {b:?} (z = {z:.2})"
        );
    }
}

#[test]
fn transsmt_hosts_match_the_exact_mode_statistically() {
    transsmt_equivalence(false);
}

#[test]
fn transsmt_hosts_and_parasites_match_the_exact_mode_statistically() {
    transsmt_equivalence(true);
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

/// The deme replications `PrintDemeReplicationData` recorded in `dir`.
fn deme_replications(dir: &Path) -> u64 {
    let text = fs::read_to_string(dir.join("data/deme_repl.dat")).unwrap();
    text.lines()
        .filter(|l| !l.starts_with('#') && !l.trim().is_empty())
        .map(|l| l.split(' ').nth(1).unwrap().parse::<u64>().unwrap())
        .sum()
}

#[test]
fn demes_replicate_on_the_time_their_bursts_used() {
    // The test's configuration seeds from the clock.
    const SEED: (&str, &str) = ("RANDOM_SEED", "7");
    // A hundred demes of one organism each, replicating (through the
    // germline) on the instructions or the normalized time their organisms
    // used: what the bursts of the parallel phases add up, checked at the
    // start of each sequential phase. The run ends between two waves of
    // replications (the demes start in step), so that the lag of about one
    // update per deme generation the parallel mode has (ARCHITECTURE.md)
    // does not move a wave across the end.
    let events = "i InjectDemes default-classic.org\n\
                  u 0:1:end PrintDemeReplicationData\n\
                  u 0:10:end PrintAverageData\n\
                  u 0:10:end PrintCountData\n\
                  u 100 SavePopulation\n\
                  u 100 Exit\n";
    let test = "demes_germline";
    for (trigger, value) in [
        ("DEMES_REPLICATE_CPU_CYCLES", "3000"),
        ("DEMES_REPLICATE_TIME", "20"),
    ] {
        let exact = &[SEED, (trigger, value)];
        let (de, _, se, _) =
            run_with_events(test, &format!("{trigger}_exact"), exact, Some(events));
        assert!(se.is_none());
        let one = &[SEED, PARALLEL, ("EXECUTION_THREADS", "1"), (trigger, value)];
        let (d1, o1, s1, n1) = run_with_events(test, &format!("{trigger}_t1"), one, Some(events));
        let s1 = s1.expect("NUM_DEMES 100 runs in the parallel mode");
        assert!(s1.sequential_insts * 5 < n1, "{}", s1.summary());
        let four = &[SEED, PARALLEL, ("EXECUTION_THREADS", "4"), (trigger, value)];
        let (d4, o4, s4, _) = run_with_events(test, &format!("{trigger}_t4"), four, Some(events));
        let s4 = s4.unwrap();
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
        // As many replications as the exact mode, up to the difference
        // between two schedules.
        let (re, rp) = (deme_replications(&de), deme_replications(&d1));
        assert!(re > 50, "{trigger}: {re} replications in the exact mode");
        assert!(
            rp * 5 > re * 4 && rp * 4 < re * 5,
            "{trigger}: {rp} replications against {re} in the exact mode"
        );
    }
}

#[test]
fn unsupported_settings_are_rejected_at_start_up() {
    let dir = fresh_config("heads_default_100u", "mode2");
    let err = try_world(&dir, &[("EXECUTION_MODE", "2")])
        .err()
        .expect("EXECUTION_MODE 2 is rejected");
    assert!(err.contains("unsupported: EXECUTION_MODE 2"), "{err}");
}

#[test]
fn divides_and_output_tests_run_in_the_bursts() {
    let test = "heads_default_100u";
    let one = ("EXECUTION_THREADS", "1");
    let (_, _, local, n) = run(test, "local", &[PARALLEL, one]);
    let local = local.unwrap();
    // Every divide of this run ran its organism-local half in a burst and
    // its world half as a commit, and the output tests never reached the
    // sequential phase: what is left there is the rare organism that
    // stops before an instruction the burst does not run.
    assert!(local.commits > 10, "{}", local.summary());
    assert!(local.sequential_insts * 200 < n, "{}", local.summary());
    // A setting a divide in a burst cannot honour keeps every divide in the
    // sequential phase: the same run without commits.
    let (_, _, held, _) = run(test, "held", &[PARALLEL, one, ("EPIGENETIC_METHOD", "1")]);
    let held = held.unwrap();
    assert_eq!(held.commits, 0, "{}", held.summary());
    assert!(held.sequential_insts > local.sequential_insts);
}

/// The 30 x 30 stock world over several seeds in the exact mode and in the
/// parallel mode, whose divides and output tests run in the bursts: the
/// outcomes at the end should not differ by more than the seeds' own
/// scatter (the loose |z| < 3.5 bound of `transsmt_equivalence`; the
/// 16-seed comparison of ARCHITECTURE.md is the strict one).
#[test]
fn heads_cpu_with_bursts_for_divides_matches_the_exact_mode_statistically() {
    let test = "heads_default_100u";
    let events = "u begin Inject default-classic.org\n\
                  u 1500:10:2000 PrintCountData\n\
                  u 1500:10:2000 PrintAverageData\n\
                  u 1500:10:2000 PrintTasksData\n\
                  u 2000 Exit\n";
    // (organisms, average gestation time, average merit, organisms with
    // NOT) per seed and mode
    let mut outcomes: [Vec<[f64; 4]>; 2] = [Vec::new(), Vec::new()];
    for seed in 1..=8 {
        let seed_text = seed.to_string();
        for (mode, extra) in [("0", None), ("1", Some(PARALLEL))] {
            let mut sets = vec![
                ("WORLD_X", "30"),
                ("WORLD_Y", "30"),
                ("VERBOSITY", "0"),
                ("RANDOM_SEED", seed_text.as_str()),
            ];
            sets.extend(extra);
            let name = format!("heads_equiv_{mode}_{seed}");
            let (dir, _, stats, _) = run_with_events(test, &name, &sets, Some(events));
            if mode == "1" {
                let stats = stats.unwrap();
                assert!(stats.commits > 100, "{}", stats.summary());
            }
            outcomes[usize::from(mode == "1")].push([
                tail_mean(&dir, "count.dat", 2, 5),
                tail_mean(&dir, "average.dat", 2, 5),
                tail_mean(&dir, "average.dat", 1, 5),
                tail_mean(&dir, "tasks.dat", 1, 5),
            ]);
        }
    }
    assert!(outcomes[1].iter().all(|o| o[0] > 100.0), "{outcomes:?}");
    for k in 0..4 {
        let a: Vec<f64> = outcomes[0].iter().map(|o| o[k]).collect();
        let b: Vec<f64> = outcomes[1].iter().map(|o| o[k]).collect();
        let z = welch_z(&a, &b);
        assert!(
            z.abs() < 3.5,
            "outcome {k}: exact {a:?} against parallel {b:?} (z = {z:.2})"
        );
    }
}

/// The output test an organism runs in a burst
/// (`avida_hardware::local_output`) does what `cOrganism::doOutput` does
/// through the world: the same phenotype afterwards, and the statistics
/// changes it recorded, replayed, are those the world made itself.
#[test]
fn local_output_test_matches_the_world_path() {
    use avida_hardware::local_output::{output_is_local, OutEvent};
    use avida_hardware::organism::do_output;
    use avida_hardware::parallel::{CellInputs, LocalInterface, LocalWorld, PendingBirthSlot};
    use avida_world::pop_interface::PopulationInterface;

    struct Inputs(Vec<Vec<i32>>);
    impl CellInputs for Inputs {
        fn inputs(&self, cell: i32) -> &[i32] {
            &self.0[cell as usize]
        }
    }

    let dir = fresh_config("heads_default_100u", "local_output");
    fs::write(
        dir.join("events.cfg"),
        "u begin Inject default-classic.org\nu 30 Exit\n",
    )
    .unwrap();
    let mut w = try_world(&dir, &[("VERBOSITY", "0"), ("RANDOM_SEED", "7")]).unwrap();
    let mut stepper = Stepper::new(&w);
    let mut out = Vec::new();
    while stepper.begin_update(&mut w, &mut out).unwrap() {
        while stepper.step_slice(&mut w).unwrap() {}
        stepper.end_update(&mut w, &mut out).unwrap();
    }
    assert!(output_is_local(&w.cfg, &w.env, false));
    let cell = w.pop.live_org_list[0];
    let mut org = w.pop.orgs[cell as usize].take().unwrap();
    let inputs = w.pop.cells[cell as usize].inputs.clone();
    // Output tests that complete NOT, then NOT and NAND-free outputs.
    let start = org.state.clone();
    let is = org.hardware.base().inst_set.clone();
    for value in [!inputs[0], !inputs[1], inputs[0] ^ 0x55, !inputs[0]] {
        // Through the world.
        let before = (
            w.stats.new_task_count.clone(),
            w.stats.new_reaction_count.clone(),
        );
        org.state = start.clone();
        {
            let mut iface = PopulationInterface {
                w: &mut w,
                cell_id: cell,
            };
            do_output(
                &mut org.state,
                &[],
                &is,
                &mut iface,
                Some(value),
                false,
                false,
                None,
            )
            .unwrap();
        }
        let exact = format!("{:?}", org.state.phenotype);
        let after = (
            w.stats.new_task_count.clone(),
            w.stats.new_reaction_count.clone(),
        );
        // In a burst.
        org.state = start.clone();
        let mut events: Vec<OutEvent> = Vec::new();
        let mut pending = PendingBirthSlot::default();
        let all_inputs = Inputs(
            w.pop
                .cells
                .iter()
                .map(|c| c.inputs.clone())
                .collect::<Vec<_>>(),
        );
        let counts = std::collections::BTreeMap::new();
        {
            let lw = LocalWorld {
                config: &w.cfg,
                environment: &w.env,
                hardware_manager: &w.hw,
                update: w.stats.update(),
                tot_creatures: w.stats.tot_organisms,
                sense_size: 0,
                io_local: true,
                divide_local: false,
                cell_resources: None,
                cell_inputs: &all_inputs,
                group_attack_counts: &counts,
            };
            let mut rng = avida_hardware::parallel::org_stream(7, org.state.uid);
            let mut iface = LocalInterface::new(&lw, &mut rng, cell, &mut events, &mut pending);
            do_output(
                &mut org.state,
                &[],
                &is,
                &mut iface,
                Some(value),
                false,
                false,
                None,
            )
            .unwrap();
            assert_eq!(iface.denied_call(), None);
        }
        assert_eq!(format!("{:?}", org.state.phenotype), exact);
        // Replayed on the world's statistics, they add what it added.
        let delta =
            |a: &[i32], b: &[i32]| -> Vec<i32> { a.iter().zip(b).map(|(x, y)| y - x).collect() };
        let want = (delta(&before.0, &after.0), delta(&before.1, &after.1));
        let (mut tasks, mut reactions) = (vec![0; want.0.len()], vec![0; want.1.len()]);
        for e in &events {
            match e {
                OutEvent::NewTask(t) => tasks[*t] += 1,
                OutEvent::NewReaction(r) => reactions[*r] += 1,
                _ => {}
            }
        }
        assert_eq!((tasks, reactions), want, "value {value:#x}");
    }
}

/// An environment of two per-cell grid resources on the 30 x 30 world: NOT
/// feeds on the first and gives off the second as a byproduct (and is
/// rewarded each time, with no limit on the count), NAND feeds on the second.
const CELL_RESOURCE_ENVIRONMENT: &str = "\
RESOURCE resA:geometry=grid:initial=8:inflow=2:outflow=0.1:inflowx1=0:inflowx2=29:inflowy1=0:inflowy2=29:outflowx1=0:outflowx2=29:outflowy1=0:outflowy2=29:xdiffuse=0.2:ydiffuse=0.2
RESOURCE resB:geometry=grid:initial=3:inflow=1:outflow=0.1:inflowx1=0:inflowx2=14:inflowy1=0:inflowy2=29:outflowx1=0:outflowx2=29:outflowy1=0:outflowy2=29:xdiffuse=0.1:ydiffuse=0.1
REACTION NOT not process:resource=resA:value=1.0:type=pow:frac=0.5:max=2.0:product=resB:conversion=0.5
REACTION NAND nand process:resource=resB:value=1.0:type=pow:frac=0.25:max=2.0
REACTION AND and process:value=2.0:type=pow  requisite:max_count=1
";

/// `fresh_config` of the stock 30 x 30 test with [`CELL_RESOURCE_ENVIRONMENT`].
fn cell_resource_config(name: &str) -> PathBuf {
    let dir = fresh_config("heads_default_100u", name);
    fs::write(dir.join("environment.cfg"), CELL_RESOURCE_ENVIRONMENT).unwrap();
    dir
}

/// The output tests of a burst in an environment of per-cell resources do
/// what the world path does: the same phenotype after each, and the changes
/// they recorded, applied to the grids, leave every amount of the cell
/// the world path left, bit for bit, including the second and third test of
/// one burst, which must see what the first took.
#[test]
fn cell_resource_output_tests_match_the_world_path() {
    use avida_hardware::local_output::{output_is_local, CellResources, OutEvent};
    use avida_hardware::organism::do_output;
    use avida_hardware::parallel::{CellInputs, LocalInterface, LocalWorld, PendingBirthSlot};
    use avida_world::pop_interface::PopulationInterface;

    struct Inputs(Vec<Vec<i32>>);
    impl CellInputs for Inputs {
        fn inputs(&self, cell: i32) -> &[i32] {
            &self.0[cell as usize]
        }
    }
    struct View<'a>(&'a (dyn Fn(i32, &mut Vec<f64>) + Sync));
    impl CellResources for View<'_> {
        fn cell_levels(&self, cell: i32, out: &mut Vec<f64>) {
            (self.0)(cell, out);
        }
    }

    let dir = cell_resource_config("cell_resource_output");
    fs::write(
        dir.join("events.cfg"),
        "u begin Inject default-classic.org\nu 30 Exit\n",
    )
    .unwrap();
    let mut w = try_world(&dir, &[("VERBOSITY", "0"), ("RANDOM_SEED", "7")]).unwrap();
    let mut stepper = Stepper::new(&w);
    let mut out = Vec::new();
    while stepper.begin_update(&mut w, &mut out).unwrap() {
        while stepper.step_slice(&mut w).unwrap() {}
        stepper.end_update(&mut w, &mut out).unwrap();
    }
    assert!(w.pop.resource_count.are_plain_cell_grids(w.pop.cells.len()));
    assert!(output_is_local(&w.cfg, &w.env, true));
    assert!(!output_is_local(&w.cfg, &w.env, false));
    let cell = w.pop.live_org_list[0];
    let mut org = w.pop.orgs[cell as usize].take().unwrap();
    let inputs = w.pop.cells[cell as usize].inputs.clone();
    let is = org.hardware.base().inst_set.clone();
    for &v in &inputs {
        org.state.do_input(v);
    }
    let start = org.state.clone();
    let values = [
        !inputs[0],
        !inputs[1],
        inputs[0] ^ 0x55,
        !(inputs[0] & inputs[1]),
        !inputs[0],
        !(inputs[1] & inputs[2]),
    ];

    // Through the world, bringing the grids up to date first.
    org.state = start.clone();
    let mut exact_phenotypes = Vec::new();
    let (before, exact_amounts) = {
        let mut iface = PopulationInterface {
            w: &mut w,
            cell_id: cell,
        };
        let before = {
            use avida_hardware::org_interface::OrgInterface;
            iface.get_resources(&mut org.state)
        };
        for &value in &values {
            do_output(
                &mut org.state,
                &[],
                &is,
                &mut iface,
                Some(value),
                false,
                false,
                None,
            )
            .unwrap();
            exact_phenotypes.push(format!("{:?}", org.state.phenotype));
        }
        (before, w_amounts(&mut iface))
    };
    fn w_amounts(iface: &mut PopulationInterface<'_>) -> Vec<f64> {
        iface
            .w
            .pop
            .resource_count
            .get_frozen_resources(iface.cell_id)
            .to_vec()
    }
    assert!(
        before.iter().zip(&exact_amounts).any(|(a, b)| a != b),
        "the world path changed nothing: {before:?}"
    );

    // Put the cell back, then through one burst's interface.
    for (i, &amount) in before.iter().enumerate() {
        w.pop
            .resource_count
            .get_spatial_resource_mut(i)
            .base_mut()
            .set_cell_amount(cell, amount);
    }
    org.state = start.clone();
    let all_inputs = Inputs(w.pop.cells.iter().map(|c| c.inputs.clone()).collect());
    let counts = std::collections::BTreeMap::new();
    let mut events: Vec<OutEvent> = Vec::new();
    {
        let rc = &w.pop.resource_count;
        let read = |cell: i32, out: &mut Vec<f64>| rc.frozen_cell_amounts(cell, out);
        let view = View(&read);
        let lw = LocalWorld {
            config: &w.cfg,
            environment: &w.env,
            hardware_manager: &w.hw,
            update: w.stats.update(),
            tot_creatures: w.stats.tot_organisms,
            sense_size: 0,
            io_local: true,
            divide_local: false,
            cell_resources: Some(&view),
            cell_inputs: &all_inputs,
            group_attack_counts: &counts,
        };
        let mut pending = PendingBirthSlot::default();
        let mut rng = avida_hardware::parallel::org_stream(7, org.state.uid);
        let mut iface = LocalInterface::new(&lw, &mut rng, cell, &mut events, &mut pending);
        for (k, &value) in values.iter().enumerate() {
            do_output(
                &mut org.state,
                &[],
                &is,
                &mut iface,
                Some(value),
                false,
                false,
                None,
            )
            .unwrap();
            assert_eq!(iface.denied_call(), None);
            assert_eq!(
                format!("{:?}", org.state.phenotype),
                exact_phenotypes[k],
                "after test {k}"
            );
        }
    }
    // Nothing reached the grids during the burst.
    assert_eq!(w.pop.resource_count.get_frozen_resources(cell), &before[..]);
    let mut changes = 0;
    for e in &events {
        if let OutEvent::ResourceChange(c) = e {
            w.pop.resource_count.modify_cell_updated(c, cell);
            changes += 1;
        }
    }
    assert!(changes >= 3, "{events:?}");
    let local_amounts = w.pop.resource_count.get_frozen_resources(cell).to_vec();
    let bits = |v: &[f64]| v.iter().map(|x| x.to_bits()).collect::<Vec<_>>();
    assert_eq!(bits(&local_amounts), bits(&exact_amounts));
}

/// A full 30 x 30 world of per-cell resources, with the output tests in the
/// bursts: the same output for any number of threads, the output tests
/// out of the sequential phase, and no cell below zero.
#[test]
fn cell_resource_world_does_not_depend_on_the_thread_count() {
    let events = "u begin InjectAll filename=default-classic.org\n\
                  u 0:5:end PrintAverageData\n\
                  u 0:5:end PrintCountData\n\
                  u 0:5:end PrintTasksData\n\
                  u 0:5:end PrintResourceData\n\
                  u 60 SavePopulation\n\
                  u 60 Exit\n";
    let run_threads = |threads: &str| {
        let dir = cell_resource_config(&format!("cell_res_full_t{threads}"));
        fs::write(dir.join("events.cfg"), events).unwrap();
        let sets = [PARALLEL, ("EXECUTION_THREADS", threads), ("VERBOSITY", "0")];
        let mut w = try_world(&dir, &sets).unwrap();
        let mut stepper = Stepper::new(&w);
        let mut out = Vec::new();
        while stepper.begin_update(&mut w, &mut out).unwrap() {
            while stepper.step_slice(&mut w).unwrap() {}
            stepper.end_update(&mut w, &mut out).unwrap();
        }
        w.out.flush_all();
        for i in 0..2 {
            let g = w.pop.resource_count.get_spatial_resource(i).base();
            assert!(
                (0..g.size()).all(|c| g.get_amount(c) >= 0.0),
                "resource {i} below zero"
            );
        }
        (dir, out, stepper.parallel_stats().unwrap())
    };
    let (d1, o1, s1) = run_threads("1");
    let (d4, o4, s4) = run_threads("4");
    // The output tests ran in the bursts (and the divides committed).
    assert!(s1.commits > 100, "{}", s1.summary());
    assert!(
        s1.sequential_insts * 100 < s1.parallel_insts,
        "{}",
        s1.summary()
    );
    assert!(s4.inline_bursts * 2 < s4.bursts, "{}", s4.summary());
    compare_runs((&d1, &o1), (&d4, &o4)).unwrap();
}

/// Per-cell resources over several seeds in the exact mode and in the
/// parallel mode, whose output tests run in the bursts: the outcomes at the
/// end, and the resources left in the world, should not differ by more than
/// the seeds' own scatter (the loose |z| < 3.5 bound of the other
/// equivalence tests).
#[test]
fn cell_resources_in_bursts_match_the_exact_mode_statistically() {
    let events = "u begin Inject default-classic.org\n\
                  u 300:10:600 PrintCountData\n\
                  u 300:10:600 PrintAverageData\n\
                  u 300:10:600 PrintTasksData\n\
                  u 300:10:600 PrintResourceData\n\
                  u 600 Exit\n";
    // (organisms, average gestation time, average merit, organisms with
    // NOT, resource A in the world) per seed and mode
    let mut outcomes: [Vec<[f64; 5]>; 2] = [Vec::new(), Vec::new()];
    for seed in 1..=6 {
        let seed_text = seed.to_string();
        for (mode, extra) in [("0", None), ("1", Some(PARALLEL))] {
            let mut sets = vec![("VERBOSITY", "0"), ("RANDOM_SEED", seed_text.as_str())];
            sets.extend(extra);
            let name = format!("cell_res_equiv_{mode}_{seed}");
            let dir = cell_resource_config(&name);
            fs::write(dir.join("events.cfg"), events).unwrap();
            let mut w = try_world(&dir, &sets).unwrap();
            let mut stepper = Stepper::new(&w);
            let mut out = Vec::new();
            while stepper.begin_update(&mut w, &mut out).unwrap() {
                while stepper.step_slice(&mut w).unwrap() {}
                stepper.end_update(&mut w, &mut out).unwrap();
            }
            w.out.flush_all();
            if mode == "1" {
                let stats = stepper.parallel_stats().unwrap();
                assert!(stats.commits > 100, "{}", stats.summary());
            }
            outcomes[usize::from(mode == "1")].push([
                tail_mean(&dir, "count.dat", 2, 5),
                tail_mean(&dir, "average.dat", 2, 5),
                tail_mean(&dir, "average.dat", 1, 5),
                tail_mean(&dir, "tasks.dat", 1, 5),
                tail_mean(&dir, "resource.dat", 1, 5),
            ]);
        }
    }
    assert!(outcomes[1].iter().all(|o| o[0] > 100.0), "{outcomes:?}");
    for k in 0..5 {
        let a: Vec<f64> = outcomes[0].iter().map(|o| o[k]).collect();
        let b: Vec<f64> = outcomes[1].iter().map(|o| o[k]).collect();
        let z = welch_z(&a, &b);
        assert!(
            z.abs() < 3.5,
            "outcome {k}: exact {a:?} against parallel {b:?} (z = {z:.2})"
        );
    }
}
