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
                global_pool: false,
                pools: None,
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

/// An environment of two per-cell grid resources on the stock world: NOT
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
            global_pool: false,
            pools: None,
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

/// A small global pool for each of two reactions (and one with no resource),
/// which the 900 organisms of a 30 x 30 world drain within a few tests: no
/// inflow, no outflow, no byproduct, so that what leaves the pools is exactly
/// what the organisms consume.
const SCARCE_POOL_ENVIRONMENT: &str = "\
RESOURCE resA:initial=6:inflow=0:outflow=0
RESOURCE resB:initial=2:inflow=0:outflow=0
REACTION NOT not process:resource=resA:value=1.0:type=pow:frac=0.3:max=2.0
REACTION NAND nand process:resource=resB:value=1.0:type=add:frac=0.5:max=1.0
REACTION ORN orn process:resource=resA:value=0.5:type=add:frac=0.1:max=1.0 process:resource=resB:value=0.5:type=add:frac=0.1:max=1.0
REACTION AND and process:value=2.0:type=pow  requisite:max_count=1
";

/// The classic limited-resource environment in small: two well-mixed pools
/// with inflow and outflow, NOT feeding on the first and giving off the
/// second as a byproduct (and rewarded each time, with no limit on the
/// count), NAND feeding on the second.
const GLOBAL_RESOURCE_ENVIRONMENT: &str = "\
RESOURCE resA:initial=100:inflow=20:outflow=0.01
RESOURCE resB:initial=50:inflow=10:outflow=0.01
REACTION NOT not process:resource=resA:value=1.0:type=pow:frac=0.01:max=2.0:product=resB:conversion=0.5
REACTION NAND nand process:resource=resB:value=1.0:type=pow:frac=0.01:max=2.0
REACTION AND and process:value=2.0:type=pow  requisite:max_count=1
";

/// `fresh_config` of the stock 30 x 30 test with `environment`, and an
/// ancestor that does NOT: the classic one with `IO, push, pop nop-C, nand,
/// IO` (the input into BX, a copy of it into CX, their NAND, which is the
/// complement, and the output) in the padding before its copy loop
/// (`not-ancestor.org`).
fn global_resource_config(name: &str, environment: &str) -> PathBuf {
    let dir = fresh_config("heads_default_100u", name);
    fs::write(dir.join("environment.cfg"), environment).unwrap();
    let classic = fs::read_to_string(dir.join("default-classic.org")).unwrap();
    let mut insts: Vec<&str> = classic
        .lines()
        .map(|l| l.split('#').next().unwrap().trim())
        .filter(|l| !l.is_empty())
        .collect();
    assert_eq!(&insts[5..9], ["nop-C"; 4]);
    insts.splice(8..14, ["IO", "push", "pop", "nop-C", "nand", "IO"]);
    fs::write(dir.join("not-ancestor.org"), insts.join("\n") + "\n").unwrap();
    dir
}

/// The output tests of a burst in an environment of global pools, finished by
/// the world, do what the exact path does when they come in the same order:
/// the same phenotype after each, the same amounts in the pools bit for bit
/// (the later organisms see what the earlier took, down to an empty pool),
/// no pool below zero, and what left the pools is what the organisms
/// consumed.
#[test]
fn global_pool_output_tests_settle_as_the_exact_path_does() {
    use avida_hardware::local_output::{finish_output_global, output_is_local_global};
    use avida_hardware::org_interface::OrgInterface;
    use avida_hardware::organism::do_output;
    use avida_hardware::parallel::{CellInputs, LocalInterface, LocalWorld, PendingBirthSlot};
    use avida_world::pop_interface::PopulationInterface;

    struct Inputs(Vec<Vec<i32>>);
    impl CellInputs for Inputs {
        fn inputs(&self, cell: i32) -> &[i32] {
            &self.0[cell as usize]
        }
    }

    let make = |name: &str| {
        let dir = global_resource_config(name, SCARCE_POOL_ENVIRONMENT);
        fs::write(
            dir.join("events.cfg"),
            "u begin InjectAll filename=default-classic.org\nu 3 Exit\n",
        )
        .unwrap();
        let mut w = try_world(&dir, &[("VERBOSITY", "0"), ("RANDOM_SEED", "7")]).unwrap();
        let mut stepper = Stepper::new(&w);
        let mut out = Vec::new();
        while stepper.begin_update(&mut w, &mut out).unwrap() {
            while stepper.step_slice(&mut w).unwrap() {}
            stepper.end_update(&mut w, &mut out).unwrap();
        }
        w
    };
    let mut wa = make("gpool_exact");
    let mut wb = make("gpool_burst");
    assert!(wb.pop.resource_count.are_global_pools());
    assert!(output_is_local_global(&wb.cfg, &wb.env));
    assert!(!avida_hardware::local_output::output_is_local(
        &wb.cfg, &wb.env, false
    ));
    let cells = wa.pop.live_org_list.clone();
    assert_eq!(cells, wb.pop.live_org_list);
    assert!(cells.len() > 500);
    let pool_start = wb.pop.resource_count.read_resources().to_vec();
    assert_eq!(pool_start, wa.pop.resource_count.read_resources());
    assert_eq!(pool_start, [6.0, 2.0]);

    let all_inputs = Inputs(wb.pop.cells.iter().map(|c| c.inputs.clone()).collect());
    let counts = std::collections::BTreeMap::new();
    let mut unused = avida_hardware::parallel::stream(1, 0);
    let (mut pending_tests, mut consumed, mut empty_after) = (0, 0.0, 0);
    for (k, &cell) in cells.iter().enumerate() {
        let mut oa = wa.pop.orgs[cell as usize].take().unwrap();
        let mut ob = wb.pop.orgs[cell as usize].take().unwrap();
        let inputs = wa.pop.cells[cell as usize].inputs.clone();
        let is = oa.hardware.base().inst_set.clone();
        for &v in &inputs {
            oa.state.do_input(v);
            ob.state.do_input(v);
        }
        let values = [
            !inputs[0],
            !(inputs[0] & inputs[1]),
            inputs[0] ^ 0x55,
            !inputs[0],
            !(inputs[1] & inputs[2]),
        ];
        for (j, &value) in values.iter().enumerate() {
            // The exact path.
            {
                let mut iface = PopulationInterface {
                    w: &mut wa,
                    cell_id: cell,
                };
                do_output(
                    &mut oa.state,
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
            // A burst and the world's half.
            let mut events = Vec::new();
            let mut birth = PendingBirthSlot::default();
            let pending = {
                let lw = LocalWorld {
                    config: &wb.cfg,
                    environment: &wb.env,
                    hardware_manager: &wb.hw,
                    update: wb.stats.update(),
                    tot_creatures: wb.stats.tot_organisms,
                    sense_size: 0,
                    io_local: true,
                    divide_local: false,
                    global_pool: true,
                    pools: None,
                    cell_resources: None,
                    cell_inputs: &all_inputs,
                    group_attack_counts: &counts,
                };
                let mut rng = avida_hardware::parallel::org_stream(7, ob.state.uid);
                let mut iface = LocalInterface::new(&lw, &mut rng, cell, &mut events, &mut birth);
                do_output(
                    &mut ob.state,
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
                iface.take_pending_output().0
            };
            let before = ob.state.phenotype.res_consumed;
            if let Some(pending) = pending {
                pending_tests += 1;
                let levels = PopulationInterface {
                    w: &mut wb,
                    cell_id: cell,
                }
                .get_resources(&mut ob.state);
                let update = wb.stats.update();
                let (change, _) = finish_output_global(
                    &mut ob.state,
                    *pending,
                    &wb.env,
                    &wb.cfg,
                    &levels,
                    update,
                    &mut unused,
                )
                .unwrap();
                PopulationInterface {
                    w: &mut wb,
                    cell_id: cell,
                }
                .update_resources(&mut ob.state, &change);
                consumed += ob.state.phenotype.res_consumed - before;
            }
            assert_eq!(
                format!("{:?}", ob.state.phenotype),
                format!("{:?}", oa.state.phenotype),
                "organism {k} test {j}"
            );
            let (pa, pb) = (
                wa.pop.resource_count.read_resources(),
                wb.pop.resource_count.read_resources(),
            );
            let bits = |v: &[f64]| v.iter().map(|x| x.to_bits()).collect::<Vec<_>>();
            assert_eq!(bits(pa), bits(pb), "organism {k} test {j}");
            assert!(pb.iter().all(|&x| x >= 0.0), "{pb:?}");
            empty_after += usize::from(pb.iter().any(|&x| x < 1e-9));
        }
        wa.pop.orgs[cell as usize] = Some(oa);
        wb.pop.orgs[cell as usize] = Some(ob);
    }
    // Tests settled, the pools drained to nothing, and what left them is
    // what the organisms consumed.
    let pool_end = wb.pop.resource_count.read_resources().to_vec();
    assert!(pending_tests > 100, "{pending_tests}");
    assert!(empty_after > 100, "{empty_after}");
    let taken: f64 = pool_start.iter().zip(&pool_end).map(|(a, b)| a - b).sum();
    assert!(
        consumed > 7.9 && (taken - consumed).abs() < 1e-9,
        "{taken} {consumed}"
    );
}

/// A full 30 x 30 world of global pools, with the output tests in the
/// bursts: the same output for any number of threads, the divides and tests
/// out of the sequential phase, and no pool below zero.
#[test]
fn global_pool_world_does_not_depend_on_the_thread_count() {
    let events = "u begin InjectAll filename=not-ancestor.org\n\
                  u 0:5:end PrintAverageData\n\
                  u 0:5:end PrintCountData\n\
                  u 0:5:end PrintTasksData\n\
                  u 0:5:end PrintResourceData\n\
                  u 80 SavePopulation\n\
                  u 80 Exit\n";
    let run_threads = |threads: &str| {
        let dir = global_resource_config(
            &format!("gpool_full_t{threads}"),
            GLOBAL_RESOURCE_ENVIRONMENT,
        );
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
        assert!(w
            .pop
            .resource_count
            .read_resources()
            .iter()
            .all(|&x| x >= 0.0));
        (dir, out, stepper.parallel_stats().unwrap())
    };
    let (d1, o1, s1) = run_threads("1");
    // The tests are settled in the sequential phase, the divides commit, and
    // few instructions are left to the exact path.
    assert!(s1.pool_tests > 50, "{}", s1.summary());
    assert!(s1.commits > 100, "{}", s1.summary());
    assert!(
        s1.sequential_insts * 50 < s1.parallel_insts,
        "{}",
        s1.summary()
    );
    for threads in ["2", "4"] {
        let (d, o, s) = run_threads(threads);
        assert_eq!(
            (s.pool_tests, s.rounds, s.commits),
            (s1.pool_tests, s1.rounds, s1.commits)
        );
        compare_runs((&d1, &o1), (&d, &o))
            .unwrap_or_else(|e| panic!("EXECUTION_THREADS 1 and {threads}: {e}"));
    }
}

/// Global pools over several seeds in the exact mode and in the parallel
/// mode, whose output tests run in the bursts and settle against the pools
/// in the sequential phase: the outcomes at the end, and the pools left in
/// the world, should not differ by more than the seeds' own scatter (the
/// loose |z| < 3.5 bound of the other equivalence tests).
#[test]
fn global_pools_in_bursts_match_the_exact_mode_statistically() {
    let events = "u begin Inject not-ancestor.org\n\
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
            let name = format!("gpool_equiv_{mode}_{seed}");
            let dir = global_resource_config(&name, GLOBAL_RESOURCE_ENVIRONMENT);
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
                assert!(stats.pool_tests > 10, "{}", stats.summary());
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

/// One per-cell grid and one global pool together on the stock world. NOT
/// has a reaction on each (so one test triggers both kinds), NAND feeds on
/// the pool, ORN on the grid, and AND uses neither. The pool is small and
/// closed, so that the organisms drain it within a few tests.
const MIXED_SCARCE_ENVIRONMENT: &str = "\
RESOURCE gridA:geometry=grid:initial=8:inflow=2:outflow=0.1:inflowx1=0:inflowx2=29:inflowy1=0:inflowy2=29:outflowx1=0:outflowx2=29:outflowy1=0:outflowy2=29:xdiffuse=0.2:ydiffuse=0.2
RESOURCE poolB:initial=6:inflow=0:outflow=0
REACTION NOT not process:resource=gridA:value=1.0:type=pow:frac=0.5:max=2.0
REACTION NOTP not process:resource=poolB:value=0.5:type=pow:frac=0.3:max=2.0
REACTION NAND nand process:resource=poolB:value=1.0:type=add:frac=0.5:max=1.0
REACTION ORN orn process:resource=gridA:value=0.5:type=add:frac=0.1:max=1.0
REACTION AND and process:value=2.0:type=pow  requisite:max_count=1
";

/// The grid and pool of [`MIXED_SCARCE_ENVIRONMENT`] with flows that keep a
/// full world going: a pool with inflow and outflow, a grid with inflow
/// everywhere, NOT on both.
const MIXED_RESOURCE_ENVIRONMENT: &str = "\
RESOURCE gridA:geometry=grid:initial=8:inflow=2:outflow=0.1:inflowx1=0:inflowx2=29:inflowy1=0:inflowy2=29:outflowx1=0:outflowx2=29:outflowy1=0:outflowy2=29:xdiffuse=0.2:ydiffuse=0.2
RESOURCE poolB:initial=100:inflow=20:outflow=0.01
REACTION NOT not process:resource=gridA:value=1.0:type=pow:frac=0.5:max=2.0
REACTION NOTP not process:resource=poolB:value=0.5:type=pow:frac=0.01:max=2.0:product=poolB:conversion=0.1
REACTION NAND nand process:resource=poolB:value=1.0:type=pow:frac=0.01:max=2.0
REACTION ORN orn process:resource=gridA:value=0.5:type=add:frac=0.1:max=1.0
REACTION AND and process:value=2.0:type=pow  requisite:max_count=1
";

/// The output tests of a burst in a world of a grid and a pool, finished by
/// the world, do what the exact path does when they come in the same order:
/// the same phenotype after each (a NOT test triggers a reaction of each
/// kind), the same amounts in the pool and in the organism's cell bit for
/// bit, the pool never below zero, and what left the pool and the grids is
/// what the organisms consumed.
#[test]
fn mixed_resource_output_tests_settle_as_the_exact_path_does() {
    use avida_hardware::local_output::{
        finish_output_global, output_is_local_mixed, CellResources, OutEvent,
    };
    use avida_hardware::org_interface::OrgInterface;
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

    let make = |name: &str| {
        let dir = global_resource_config(name, MIXED_SCARCE_ENVIRONMENT);
        fs::write(
            dir.join("events.cfg"),
            "u begin InjectAll filename=default-classic.org\nu 3 Exit\n",
        )
        .unwrap();
        let mut w = try_world(&dir, &[("VERBOSITY", "0"), ("RANDOM_SEED", "7")]).unwrap();
        let mut stepper = Stepper::new(&w);
        let mut out = Vec::new();
        while stepper.begin_update(&mut w, &mut out).unwrap() {
            while stepper.step_slice(&mut w).unwrap() {}
            stepper.end_update(&mut w, &mut out).unwrap();
        }
        w
    };
    let mut wa = make("mixed_exact");
    let mut wb = make("mixed_burst");
    let cells_n = wb.pop.cells.len();
    assert!(!wb.pop.resource_count.are_plain_cell_grids(cells_n));
    assert!(!wb.pop.resource_count.are_global_pools());
    let kinds = wb
        .pop
        .resource_count
        .grid_or_global_kinds(cells_n)
        .expect("a grid and a pool");
    assert_eq!(kinds, [false, true]);
    assert!(output_is_local_mixed(&wb.cfg, &wb.env, &kinds));
    let cells = wa.pop.live_org_list.clone();
    assert_eq!(cells, wb.pop.live_org_list);
    assert!(cells.len() > 500);
    let pool_start = wb.pop.resource_count.get_frozen_resources(0).to_vec()[1];
    assert_eq!(pool_start, 6.0);

    let all_inputs = Inputs(wb.pop.cells.iter().map(|c| c.inputs.clone()).collect());
    let counts = std::collections::BTreeMap::new();
    let mut unused = avida_hardware::parallel::stream(1, 0);
    let (mut pending_tests, mut both_kinds, mut empty_after) = (0, 0, 0);
    let (mut consumed, mut grid_changes) = (0.0, 0);
    let bits = |v: &[f64]| v.iter().map(|x| x.to_bits()).collect::<Vec<_>>();
    for (k, &cell) in cells.iter().enumerate() {
        let mut oa = wa.pop.orgs[cell as usize].take().unwrap();
        let mut ob = wb.pop.orgs[cell as usize].take().unwrap();
        let inputs = wa.pop.cells[cell as usize].inputs.clone();
        let is = oa.hardware.base().inst_set.clone();
        for &v in &inputs {
            oa.state.do_input(v);
            ob.state.do_input(v);
        }
        let values = [
            !inputs[0],
            !(inputs[0] & inputs[1]),
            inputs[0] ^ 0x55,
            !inputs[0],
            !(inputs[1] & inputs[2]),
            inputs[0] | !inputs[1],
        ];
        for (j, &value) in values.iter().enumerate() {
            {
                let mut iface = PopulationInterface {
                    w: &mut wa,
                    cell_id: cell,
                };
                do_output(
                    &mut oa.state,
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
            // The grids are as the phase began: bring them up to date as
            // the world does at the start of a parallel phase, then one burst.
            PopulationInterface {
                w: &mut wb,
                cell_id: cell,
            }
            .get_resources(&mut ob.state);
            let mut events: Vec<OutEvent> = Vec::new();
            let mut birth = PendingBirthSlot::default();
            let pending = {
                let rc = &wb.pop.resource_count;
                let read = |cell: i32, out: &mut Vec<f64>| rc.frozen_cell_amounts(cell, out);
                let view = View(&read);
                let lw = LocalWorld {
                    config: &wb.cfg,
                    environment: &wb.env,
                    hardware_manager: &wb.hw,
                    update: wb.stats.update(),
                    tot_creatures: wb.stats.tot_organisms,
                    sense_size: 0,
                    io_local: true,
                    divide_local: false,
                    global_pool: true,
                    pools: Some(&kinds),
                    cell_resources: Some(&view),
                    cell_inputs: &all_inputs,
                    group_attack_counts: &counts,
                };
                let mut rng = avida_hardware::parallel::org_stream(7, ob.state.uid);
                let mut iface = LocalInterface::new(&lw, &mut rng, cell, &mut events, &mut birth);
                do_output(
                    &mut ob.state,
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
                iface.take_pending_output().0
            };
            // What the world replays after the parallel phase.
            for e in &events {
                if let OutEvent::ResourceChange(c) = e {
                    wb.pop.resource_count.modify_cell_updated(c, cell);
                    grid_changes += 1;
                }
            }
            let before = ob.state.phenotype.res_consumed;
            if let Some(pending) = pending {
                pending_tests += 1;
                // The grid's reaction ran in the burst (nothing of it is in
                // the cell yet); the pool's waits for the world.
                both_kinds +=
                    usize::from(pending.result.consumed(0) > 0.0 && !pending.deferred.is_empty());
                let levels = PopulationInterface {
                    w: &mut wb,
                    cell_id: cell,
                }
                .get_resources(&mut ob.state);
                let update = wb.stats.update();
                let (change, _) = finish_output_global(
                    &mut ob.state,
                    *pending,
                    &wb.env,
                    &wb.cfg,
                    &levels,
                    update,
                    &mut unused,
                )
                .unwrap();
                PopulationInterface {
                    w: &mut wb,
                    cell_id: cell,
                }
                .update_resources(&mut ob.state, &change);
                consumed += ob.state.phenotype.res_consumed - before;
            }
            assert_eq!(
                format!("{:?}", ob.state.phenotype),
                format!("{:?}", oa.state.phenotype),
                "organism {k} test {j}"
            );
            let (xa, xb) = (
                wa.pop.resource_count.get_frozen_resources(cell).to_vec(),
                wb.pop.resource_count.get_frozen_resources(cell).to_vec(),
            );
            assert_eq!(bits(&xa), bits(&xb), "organism {k} test {j}");
            assert!(xb.iter().all(|&x| x >= 0.0), "{xb:?}");
            empty_after += usize::from(xb[1] < 1e-9);
        }
        wa.pop.orgs[cell as usize] = Some(oa);
        wb.pop.orgs[cell as usize] = Some(ob);
    }
    // Tests settled against the pool, grids changed in the bursts, tests
    // that triggered a reaction of each kind, and the pool drained.
    assert!(pending_tests > 100, "{pending_tests}");
    assert!(both_kinds > 50, "{both_kinds}");
    assert!(grid_changes > 100, "{grid_changes}");
    assert!(empty_after > 100, "{empty_after}");
    assert!(consumed > 0.0);
}

/// `fresh_config` of the stock 30 x 30 test with the grid and the pool of
/// [`MIXED_RESOURCE_ENVIRONMENT`], and the ancestor that does NOT.
fn mixed_resource_config(name: &str) -> PathBuf {
    global_resource_config(name, MIXED_RESOURCE_ENVIRONMENT)
}

/// A full 30 x 30 world of a grid and a pool, with the output tests in the
/// bursts: the same output for any number of threads, the divides and the
/// grid's tests out of the sequential phase, the pool's half of a test
/// settled by the world, and nothing below zero.
#[test]
fn mixed_resource_world_does_not_depend_on_the_thread_count() {
    let events = "u begin InjectAll filename=not-ancestor.org\n\
                  u 0:5:end PrintAverageData\n\
                  u 0:5:end PrintCountData\n\
                  u 0:5:end PrintTasksData\n\
                  u 0:5:end PrintResourceData\n\
                  u 80 SavePopulation\n\
                  u 80 Exit\n";
    let run_threads = |threads: &str| {
        let dir = mixed_resource_config(&format!("mixed_full_t{threads}"));
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
        let rc = &w.pop.resource_count;
        assert!(rc.read_resources().iter().all(|&x| x >= 0.0));
        let g = rc.get_spatial_resource(0).base();
        assert!(
            (0..g.size()).all(|c| g.get_amount(c) >= 0.0),
            "grid below zero"
        );
        (dir, out, stepper.parallel_stats().unwrap())
    };
    let (d1, o1, s1) = run_threads("1");
    // The pool's half of the tests is settled in the sequential phase, the
    // divides commit, and few instructions are left to the exact path.
    assert!(s1.pool_tests > 50, "{}", s1.summary());
    assert!(s1.commits > 100, "{}", s1.summary());
    assert!(
        s1.sequential_insts * 50 < s1.parallel_insts,
        "{}",
        s1.summary()
    );
    for threads in ["2", "4"] {
        let (d, o, s) = run_threads(threads);
        assert_eq!(
            (s.pool_tests, s.rounds, s.commits),
            (s1.pool_tests, s1.rounds, s1.commits)
        );
        compare_runs((&d1, &o1), (&d, &o))
            .unwrap_or_else(|e| panic!("EXECUTION_THREADS 1 and {threads}: {e}"));
    }
}

/// A grid and a pool over several seeds in the exact mode and in the
/// parallel mode, whose output tests run in the bursts (the pool's half
/// settled in the sequential phase): the outcomes at the end, and the
/// resources left in the world, should not differ by more than the seeds'
/// own scatter (the loose |z| < 3.5 bound of the other equivalence tests).
#[test]
fn mixed_resources_in_bursts_match_the_exact_mode_statistically() {
    let events = "u begin Inject not-ancestor.org\n\
                  u 300:10:600 PrintCountData\n\
                  u 300:10:600 PrintAverageData\n\
                  u 300:10:600 PrintTasksData\n\
                  u 300:10:600 PrintResourceData\n\
                  u 600 Exit\n";
    let mut outcomes: [Vec<[f64; 5]>; 2] = [Vec::new(), Vec::new()];
    for seed in 1..=6 {
        let seed_text = seed.to_string();
        for (mode, extra) in [("0", None), ("1", Some(PARALLEL))] {
            let mut sets = vec![("VERBOSITY", "0"), ("RANDOM_SEED", seed_text.as_str())];
            sets.extend(extra);
            let name = format!("mixed_equiv_{mode}_{seed}");
            let dir = mixed_resource_config(&name);
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
                assert!(stats.pool_tests > 10, "{}", stats.summary());
            }
            outcomes[usize::from(mode == "1")].push([
                tail_mean(&dir, "count.dat", 2, 5),
                tail_mean(&dir, "average.dat", 2, 5),
                tail_mean(&dir, "average.dat", 1, 5),
                tail_mean(&dir, "tasks.dat", 1, 5),
                tail_mean(&dir, "resource.dat", 2, 5),
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

// ---------------------------------------------------------------------
// Reactions that use grids and pools, PARTIAL resources, gradients
// ---------------------------------------------------------------------

/// Grid and pool as in [`MIXED_SCARCE_ENVIRONMENT`], with reactions that use
/// both: NOT takes from the grid and gives to the pool, NAND takes from the
/// pool and gives to the grid, ORN is the grid's alone (and first in the
/// library, so that it may share the grid with NOT).
const BOTH_KINDS_SCARCE_ENVIRONMENT: &str = "\
RESOURCE gridA:geometry=grid:initial=8:inflow=2:outflow=0.1:inflowx1=0:inflowx2=29:inflowy1=0:inflowy2=29:outflowx1=0:outflowx2=29:outflowy1=0:outflowy2=29:xdiffuse=0.2:ydiffuse=0.2
RESOURCE poolB:initial=6:inflow=0:outflow=0
REACTION ORN orn process:resource=gridA:value=0.5:type=add:frac=0.1:max=1.0
REACTION NOT not process:resource=gridA:value=1.0:type=pow:frac=0.5:max=2.0:product=poolB:conversion=0.001
REACTION NAND nand process:resource=poolB:value=1.0:type=pow:frac=0.3:max=2.0:product=gridA:conversion=0.5
REACTION AND and process:value=2.0:type=pow  requisite:max_count=1
";

/// [`BOTH_KINDS_SCARCE_ENVIRONMENT`] with flows that keep a full world going.
const BOTH_KINDS_ENVIRONMENT: &str = "\
RESOURCE gridA:geometry=grid:initial=8:inflow=2:outflow=0.1:inflowx1=0:inflowx2=29:inflowy1=0:inflowy2=29:outflowx1=0:outflowx2=29:outflowy1=0:outflowy2=29:xdiffuse=0.2:ydiffuse=0.2
RESOURCE poolB:initial=100:inflow=20:outflow=0.01
REACTION ORN orn process:resource=gridA:value=0.5:type=add:frac=0.1:max=1.0
REACTION NOT not process:resource=gridA:value=1.0:type=pow:frac=0.5:max=2.0:product=poolB:conversion=0.1
REACTION NAND nand process:resource=poolB:value=1.0:type=pow:frac=0.01:max=2.0:product=gridA:conversion=0.05
REACTION AND and process:value=2.0:type=pow  requisite:max_count=1
";

/// NOT first and ORN after it, both taking from the grid, with NOT also
/// giving to the pool: NOT waits for the world and would run after ORN.
const BOTH_KINDS_WRONG_ORDER_ENVIRONMENT: &str = "\
RESOURCE gridA:geometry=grid:initial=8:inflow=2:outflow=0.1:inflowx1=0:inflowx2=29:inflowy1=0:inflowy2=29:outflowx1=0:outflowx2=29:outflowy1=0:outflowy2=29:xdiffuse=0.2:ydiffuse=0.2
RESOURCE poolB:initial=6:inflow=0:outflow=0
REACTION NOT not process:resource=gridA:value=1.0:type=pow:frac=0.5:max=2.0:product=poolB:conversion=0.4
REACTION ORN orn process:resource=gridA:value=0.5:type=add:frac=0.1:max=1.0
REACTION AND and process:value=2.0:type=pow  requisite:max_count=1
";

/// Two `PARTIAL` pools, each on half of the 60 x 60 world's cells: NOT eats
/// the first (in the upper half) and gives to the second, NAND eats the
/// second (in the lower half).
const PARTIAL_SCARCE_ENVIRONMENT: &str = "\
RESOURCE resA:geometry=partial:cells=0..1799:initial=6:inflow=0:outflow=0
RESOURCE resB:geometry=partial:cells=1800..3599:initial=2:inflow=0:outflow=0
REACTION NOT not process:resource=resA:value=1.0:type=pow:frac=0.3:max=2.0:product=resB:conversion=0.5
REACTION NAND nand process:resource=resB:value=1.0:type=add:frac=0.5:max=1.0
REACTION AND and process:value=2.0:type=pow  requisite:max_count=1
";

const PARTIAL_ENVIRONMENT: &str = "\
RESOURCE resA:geometry=partial:cells=0..1799:initial=100:inflow=20:outflow=0.01
RESOURCE resB:geometry=partial:cells=1800..3599:initial=100:inflow=20:outflow=0.01
REACTION NOT not process:resource=resA:value=1.0:type=pow:frac=0.01:max=2.0:product=resB:conversion=0.5
REACTION NAND nand process:resource=resB:value=1.0:type=pow:frac=0.01:max=2.0
REACTION AND and process:value=2.0:type=pow  requisite:max_count=1
";

/// A `PARTIAL` pool next to a grid, NOT using both.
const PARTIAL_AND_GRID_SCARCE_ENVIRONMENT: &str = "\
RESOURCE gridA:geometry=grid:initial=8:inflow=2:outflow=0.1:inflowx1=0:inflowx2=29:inflowy1=0:inflowy2=29:outflowx1=0:outflowx2=29:outflowy1=0:outflowy2=29:xdiffuse=0.2:ydiffuse=0.2
RESOURCE poolP:geometry=partial:cells=0..1799:initial=6:inflow=0:outflow=0
REACTION ORN orn process:resource=gridA:value=0.5:type=add:frac=0.1:max=1.0
REACTION NOT not process:resource=gridA:value=1.0:type=pow:frac=0.5:max=2.0:product=poolP:conversion=0.4
REACTION NAND nand process:resource=poolP:value=1.0:type=pow:frac=0.3:max=2.0
REACTION AND and process:value=2.0:type=pow  requisite:max_count=1
";

const PARTIAL_AND_GRID_ENVIRONMENT: &str = "\
RESOURCE gridA:geometry=grid:initial=8:inflow=2:outflow=0.1:inflowx1=0:inflowx2=29:inflowy1=0:inflowy2=29:outflowx1=0:outflowx2=29:outflowy1=0:outflowy2=29:xdiffuse=0.2:ydiffuse=0.2
RESOURCE poolP:geometry=partial:cells=0..1799:initial=100:inflow=20:outflow=0.01
REACTION ORN orn process:resource=gridA:value=0.5:type=add:frac=0.1:max=1.0
REACTION NOT not process:resource=gridA:value=1.0:type=pow:frac=0.5:max=2.0:product=poolP:conversion=0.1
REACTION NAND nand process:resource=poolP:value=1.0:type=pow:frac=0.01:max=2.0
REACTION AND and process:value=2.0:type=pow  requisite:max_count=1
";

/// A gradient resource (a static peak with an inflow on its plateau, which
/// NOT and NAND eat) on the stock world.
const GRADIENT_ENVIRONMENT: &str = "\
GRADIENT_RESOURCE food:height=10:spread=9:plateau=50:decay=1:move_a_scaler=1:updatestep=1:peakx=15:peaky=15:min_x=0:max_x=29:min_y=0:max_y=29:plateau_inflow=0.5:threshold=1:initial=50
REACTION NOT not process:resource=food:value=0.01:type=pow:frac=0.3:max=100.0
REACTION NAND nand process:resource=food:value=0.01:type=add:frac=0.1:max=100.0
REACTION AND and process:value=2.0:type=pow  requisite:max_count=1
";

/// The gradient with a pool, NOT taking from the gradient and giving to it.
const GRADIENT_AND_POOL_ENVIRONMENT: &str = "\
GRADIENT_RESOURCE food:height=10:spread=9:plateau=50:decay=1:move_a_scaler=1:updatestep=1:peakx=15:peaky=15:min_x=0:max_x=29:min_y=0:max_y=29:plateau_inflow=0.5:threshold=1:initial=50
RESOURCE poolB:initial=6:inflow=0:outflow=0
REACTION NOT not process:resource=food:value=1.0:type=pow:frac=0.5:max=2.0:product=poolB:conversion=0.4
REACTION NAND nand process:resource=poolB:value=1.0:type=pow:frac=0.3:max=2.0:product=food:conversion=0.5
REACTION AND and process:value=2.0:type=pow  requisite:max_count=1
";

/// A world of `environment` on the stock test (60 x 60 cells), filled with
/// the classic ancestor by `InjectAll` and run `updates` updates in the
/// exact mode.
fn small_full_world(name: &str, environment: &str, updates: u32) -> World {
    let dir = global_resource_config(name, environment);
    fs::write(
        dir.join("events.cfg"),
        format!("u begin InjectAll filename=default-classic.org\nu {updates} Exit\n"),
    )
    .unwrap();
    let mut w = try_world(&dir, &[("VERBOSITY", "0"), ("RANDOM_SEED", "7")]).unwrap();
    let mut stepper = Stepper::new(&w);
    let mut out = Vec::new();
    while stepper.begin_update(&mut w, &mut out).unwrap() {
        while stepper.step_slice(&mut w).unwrap() {}
        stepper.end_update(&mut w, &mut out).unwrap();
    }
    w
}

/// What the first output test of an update does to the grids: their update.
fn bring_resources_up_to_date(w: &mut World) {
    use avida_hardware::org_interface::OrgInterface;
    use avida_world::pop_interface::PopulationInterface;
    let cell = w.pop.live_org_list[0];
    let mut org = w.pop.orgs[cell as usize].take().unwrap();
    PopulationInterface { w, cell_id: cell }.get_resources(&mut org.state);
    w.pop.orgs[cell as usize] = Some(org);
}

/// What [`output_tests_settle_as_the_exact_path_does`] counted.
struct SettleCounts {
    /// tests with a part left for the world
    pending: usize,
    /// resource changes the bursts handed to the world
    grid_changes: usize,
    /// tests after which some resource of the cell was below 1e-4
    empty_after: usize,
    /// total the organisms consumed in pending tests
    consumed: f64,
}

/// Output tests of bursts in the world of `environment` (every cell has an
/// organism; the tests are the NOT, NAND and ORN values of its inputs), each
/// finished by the world when it is pending as the settlement does, do what
/// the exact path does when the tests come in the same order: the same
/// phenotype after each, the same amounts of every resource in the cell bit
/// for bit, and no amount below zero.
fn output_tests_settle_as_the_exact_path_does(
    name: &str,
    environment: &str,
    warm_up: u32,
) -> SettleCounts {
    use avida_hardware::local_output::{
        finish_output_global, output_is_local, output_is_local_global, output_is_local_mixed,
        CellResources, OutEvent,
    };
    use avida_hardware::org_interface::OrgInterface;
    use avida_hardware::organism::do_output;
    use avida_hardware::parallel::{CellInputs, LocalInterface, LocalWorld, PendingBirthSlot};
    use avida_world::parallel::settle_levels;
    use avida_world::pop_interface::PopulationInterface;

    struct Inputs(Vec<Vec<i32>>);
    impl CellInputs for Inputs {
        fn inputs(&self, cell: i32) -> &[i32] {
            &self.0[cell as usize]
        }
    }
    type Read<'a> = &'a (dyn Fn(i32, &mut Vec<f64>) + Sync);
    struct View<'a>(Read<'a>, Read<'a>);
    impl CellResources for View<'_> {
        fn cell_levels(&self, cell: i32, out: &mut Vec<f64>) {
            (self.0)(cell, out);
        }
        fn cell_deltas(&self, cell: i32, out: &mut Vec<f64>) {
            (self.1)(cell, out);
        }
    }

    let mut wa = small_full_world(&format!("{name}_exact"), environment, warm_up);
    let mut wb = small_full_world(&format!("{name}_burst"), environment, warm_up);
    bring_resources_up_to_date(&mut wa);
    bring_resources_up_to_date(&mut wb);
    let cells_n = wb.pop.cells.len();
    // What the world's gate makes of the environment.
    let kinds: Vec<bool> = {
        let rc = &wb.pop.resource_count;
        if rc.are_cell_grids(cells_n) {
            assert!(output_is_local(&wb.cfg, &wb.env, true));
            Vec::new()
        } else if rc.are_global_pools() {
            assert!(output_is_local_global(&wb.cfg, &wb.env));
            vec![true; rc.size()]
        } else {
            let kinds = rc
                .grid_or_global_kinds(cells_n)
                .expect("grids and pools only");
            assert!(output_is_local_mixed(&wb.cfg, &wb.env, &kinds));
            kinds
        }
    };
    let (pools_only, mixed) = (
        kinds.iter().all(|&k| k) && !kinds.is_empty(),
        kinds.contains(&true) && kinds.contains(&false),
    );
    let pools = if mixed { Some(&kinds[..]) } else { None };
    let cells = wa.pop.live_org_list.clone();
    assert_eq!(cells, wb.pop.live_org_list);
    assert!(cells.len() > 500);

    let all_inputs = Inputs(wb.pop.cells.iter().map(|c| c.inputs.clone()).collect());
    let counts = std::collections::BTreeMap::new();
    let mut unused = avida_hardware::parallel::stream(1, 0);
    let mut c = SettleCounts {
        pending: 0,
        grid_changes: 0,
        empty_after: 0,
        consumed: 0.0,
    };
    let bits = |v: &[f64]| v.iter().map(|x| x.to_bits()).collect::<Vec<_>>();
    for (k, &cell) in cells.iter().enumerate() {
        let mut oa = wa.pop.orgs[cell as usize].take().unwrap();
        let mut ob = wb.pop.orgs[cell as usize].take().unwrap();
        let inputs = wa.pop.cells[cell as usize].inputs.clone();
        let is = oa.hardware.base().inst_set.clone();
        for &v in &inputs {
            oa.state.do_input(v);
            ob.state.do_input(v);
        }
        let values = [
            !inputs[0],
            !(inputs[0] & inputs[1]),
            inputs[0] ^ 0x55,
            !inputs[0],
            !(inputs[1] & inputs[2]),
            inputs[0] | !inputs[1],
        ];
        for (j, &value) in values.iter().enumerate() {
            {
                let mut iface = PopulationInterface {
                    w: &mut wa,
                    cell_id: cell,
                };
                do_output(
                    &mut oa.state,
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
            // The grids as the phase began (the world brings them up to
            // date at its start), then one burst.
            PopulationInterface {
                w: &mut wb,
                cell_id: cell,
            }
            .get_resources(&mut ob.state);
            let mut events: Vec<OutEvent> = Vec::new();
            let mut birth = PendingBirthSlot::default();
            let pending = {
                let rc = &wb.pop.resource_count;
                let (read, deltas) = (
                    |c: i32, out: &mut Vec<f64>| rc.frozen_cell_amounts(c, out),
                    |c: i32, out: &mut Vec<f64>| rc.frozen_cell_deltas(c, out),
                );
                let view = View(&read, &deltas);
                let lw = LocalWorld {
                    config: &wb.cfg,
                    environment: &wb.env,
                    hardware_manager: &wb.hw,
                    update: wb.stats.update(),
                    tot_creatures: wb.stats.tot_organisms,
                    sense_size: 0,
                    io_local: true,
                    divide_local: false,
                    global_pool: pools_only || mixed,
                    pools,
                    cell_resources: if pools_only { None } else { Some(&view) },
                    cell_inputs: &all_inputs,
                    group_attack_counts: &counts,
                };
                let mut rng = avida_hardware::parallel::org_stream(7, ob.state.uid);
                let mut iface = LocalInterface::new(&lw, &mut rng, cell, &mut events, &mut birth);
                do_output(
                    &mut ob.state,
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
                iface.take_pending_output().0
            };
            // What the world replays after the parallel phase.
            for e in &events {
                if let OutEvent::ResourceChange(ch) = e {
                    wb.pop.resource_count.modify_cell_updated(ch, cell);
                    c.grid_changes += 1;
                }
            }
            let before = ob.state.phenotype.res_consumed;
            if let Some(pending) = pending {
                c.pending += 1;
                let mut levels = PopulationInterface {
                    w: &mut wb,
                    cell_id: cell,
                }
                .get_resources(&mut ob.state);
                settle_levels(
                    &wb.pop.resource_count,
                    cell,
                    &mut levels,
                    &pending.grid_levels,
                    &kinds,
                );
                let update = wb.stats.update();
                let (change, _) = finish_output_global(
                    &mut ob.state,
                    *pending,
                    &wb.env,
                    &wb.cfg,
                    &levels,
                    update,
                    &mut unused,
                )
                .unwrap();
                PopulationInterface {
                    w: &mut wb,
                    cell_id: cell,
                }
                .update_resources(&mut ob.state, &change);
                c.consumed += ob.state.phenotype.res_consumed - before;
            }
            assert_eq!(
                format!("{:?}", ob.state.phenotype),
                format!("{:?}", oa.state.phenotype),
                "organism {k} test {j}"
            );
            let (xa, xb) = (
                wa.pop.resource_count.get_frozen_resources(cell).to_vec(),
                wb.pop.resource_count.get_frozen_resources(cell).to_vec(),
            );
            assert_eq!(bits(&xa), bits(&xb), "organism {k} test {j}");
            assert!(xb.iter().all(|&x| x >= 0.0), "{xb:?}");
            c.empty_after += usize::from(xb.iter().any(|&x| x < 1e-4));
        }
        wa.pop.orgs[cell as usize] = Some(oa);
        wb.pop.orgs[cell as usize] = Some(ob);
    }
    assert_eq!(
        bits(wa.pop.resource_count.read_resources()),
        bits(wb.pop.resource_count.read_resources())
    );
    c
}

/// Reactions that take from a grid and give to a pool and the other way
/// round wait for the world as wholes: the grid's amount they read is the
/// one the test saw, the pool's the one of the settlement, and what they
/// leave in both is what the exact path leaves.
#[test]
fn reactions_on_grids_and_pools_settle_as_the_exact_path_does() {
    let c =
        output_tests_settle_as_the_exact_path_does("both_kinds", BOTH_KINDS_SCARCE_ENVIRONMENT, 3);
    assert!(c.pending > 500, "{}", c.pending);
    assert!(c.grid_changes > 100, "{}", c.grid_changes);
    assert!(c.empty_after > 100, "{}", c.empty_after);
    assert!(c.consumed > 0.0);
}

/// `PARTIAL` resources: a cell off the list reads 0, one on it the pool.
#[test]
fn partial_resource_output_tests_settle_as_the_exact_path_does() {
    let c = output_tests_settle_as_the_exact_path_does("partial", PARTIAL_SCARCE_ENVIRONMENT, 3);
    assert!(c.pending > 300, "{}", c.pending);
    assert!(c.empty_after > 100, "{}", c.empty_after);
    assert!(c.consumed > 0.0);
    let c = output_tests_settle_as_the_exact_path_does(
        "partial_grid",
        PARTIAL_AND_GRID_SCARCE_ENVIRONMENT,
        3,
    );
    assert!(c.pending > 300, "{}", c.pending);
    assert!(c.grid_changes > 100, "{}", c.grid_changes);
    assert!(c.consumed > 0.0);
}

/// A gradient with a pool (NOT takes from the gradient and gives to the
/// pool, NAND the other way round).
#[test]
fn gradient_and_pool_output_tests_settle_as_the_exact_path_does() {
    let c = output_tests_settle_as_the_exact_path_does(
        "gradient_pool",
        GRADIENT_AND_POOL_ENVIRONMENT,
        80,
    );
    // Both reactions use the pool, so every change comes with a pending test.
    assert!(c.pending > 100, "{}", c.pending);
    assert!(c.consumed > 0.0);
}

/// The gate refuses a grid that a waiting reaction takes from and a later
/// reaction that does not wait takes from too (the waiting one would run
/// second), and takes the same reactions in the other order.
#[test]
fn mixed_gate_keeps_the_library_order_of_the_reactions_taking_from_a_grid() {
    use avida_hardware::local_output::output_is_local_mixed;
    for (name, environment, accepted) in [
        ("gate_ok", BOTH_KINDS_SCARCE_ENVIRONMENT, true),
        ("gate_wrong", BOTH_KINDS_WRONG_ORDER_ENVIRONMENT, false),
    ] {
        let w = small_full_world(name, environment, 3);
        let kinds = w
            .pop
            .resource_count
            .grid_or_global_kinds(w.pop.cells.len())
            .unwrap();
        assert_eq!(kinds, [false, true]);
        assert_eq!(
            output_is_local_mixed(&w.cfg, &w.env, &kinds),
            accepted,
            "{name}"
        );
    }
}

/// A gradient the organisms eat from, several tests in one burst: they see
/// what the first took, and the cell's pending change (what a gradient's
/// inflow and flow leave unfolded) goes into the amount with the first
/// change, as `ModifyCell` has it.
#[test]
fn gradient_output_tests_in_one_burst_match_the_world_path() {
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
    type Read<'a> = &'a (dyn Fn(i32, &mut Vec<f64>) + Sync);
    struct View<'a>(Read<'a>, Read<'a>);
    impl CellResources for View<'_> {
        fn cell_levels(&self, cell: i32, out: &mut Vec<f64>) {
            (self.0)(cell, out);
        }
        fn cell_deltas(&self, cell: i32, out: &mut Vec<f64>) {
            (self.1)(cell, out);
        }
    }

    let mut wa = small_full_world("gradient_one_exact", GRADIENT_ENVIRONMENT, 80);
    let mut wb = small_full_world("gradient_one_burst", GRADIENT_ENVIRONMENT, 80);
    bring_resources_up_to_date(&mut wa);
    bring_resources_up_to_date(&mut wb);
    let cells_n = wb.pop.cells.len();
    assert!(!wb.pop.resource_count.are_plain_cell_grids(cells_n));
    assert!(wb.pop.resource_count.are_cell_grids(cells_n));
    assert!(output_is_local(&wb.cfg, &wb.env, true));
    // The cell with the most food, and a pending change in it (and in a few
    // others), as a flow would leave.
    let food = |w: &World, c: i32| w.pop.resource_count.get_frozen_cell_res_val(c, 0);
    let cell = (0..cells_n as i32)
        .max_by(|&a, &b| food(&wa, a).total_cmp(&food(&wa, b)))
        .unwrap();
    assert!(food(&wa, cell) > 10.0, "{}", food(&wa, cell));
    for w in [&mut wa, &mut wb] {
        let g = w.pop.resource_count.get_spatial_resource_mut(0).base_mut();
        g.rate(cell, 0.375);
    }
    let mut oa = wa.pop.orgs[cell as usize].take().unwrap();
    let mut ob = wb.pop.orgs[cell as usize].take().unwrap();
    let inputs = wa.pop.cells[cell as usize].inputs.clone();
    let is = oa.hardware.base().inst_set.clone();
    for &v in &inputs {
        oa.state.do_input(v);
        ob.state.do_input(v);
    }
    let values = [
        !inputs[0],
        !(inputs[0] & inputs[1]),
        !inputs[0],
        !(inputs[1] & inputs[2]),
        !(inputs[0] & inputs[1]),
    ];
    let mut exact_phenotypes = Vec::new();
    {
        let mut iface = PopulationInterface {
            w: &mut wa,
            cell_id: cell,
        };
        for &value in &values {
            do_output(
                &mut oa.state,
                &[],
                &is,
                &mut iface,
                Some(value),
                false,
                false,
                None,
            )
            .unwrap();
            exact_phenotypes.push(format!("{:?}", oa.state.phenotype));
        }
    }
    // The grids up to date as the world does at the start of the phase.
    {
        use avida_hardware::org_interface::OrgInterface;
        PopulationInterface {
            w: &mut wb,
            cell_id: cell,
        }
        .get_resources(&mut ob.state);
    }
    let all_inputs = Inputs(wb.pop.cells.iter().map(|c| c.inputs.clone()).collect());
    let counts = std::collections::BTreeMap::new();
    let mut events: Vec<OutEvent> = Vec::new();
    {
        let rc = &wb.pop.resource_count;
        let (read, deltas) = (
            |c: i32, out: &mut Vec<f64>| rc.frozen_cell_amounts(c, out),
            |c: i32, out: &mut Vec<f64>| rc.frozen_cell_deltas(c, out),
        );
        let view = View(&read, &deltas);
        let lw = LocalWorld {
            config: &wb.cfg,
            environment: &wb.env,
            hardware_manager: &wb.hw,
            update: wb.stats.update(),
            tot_creatures: wb.stats.tot_organisms,
            sense_size: 0,
            io_local: true,
            divide_local: false,
            global_pool: false,
            pools: None,
            cell_resources: Some(&view),
            cell_inputs: &all_inputs,
            group_attack_counts: &counts,
        };
        let mut pending = PendingBirthSlot::default();
        let mut rng = avida_hardware::parallel::org_stream(7, ob.state.uid);
        let mut iface = LocalInterface::new(&lw, &mut rng, cell, &mut events, &mut pending);
        for (k, &value) in values.iter().enumerate() {
            do_output(
                &mut ob.state,
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
                format!("{:?}", ob.state.phenotype),
                exact_phenotypes[k],
                "after test {k}"
            );
        }
    }
    let mut changes = 0;
    for e in &events {
        if let OutEvent::ResourceChange(c) = e {
            wb.pop.resource_count.modify_cell_updated(c, cell);
            changes += 1;
        }
    }
    assert!(changes >= 2, "{events:?}");
    let bits = |v: &[f64]| v.iter().map(|x| x.to_bits()).collect::<Vec<_>>();
    assert_eq!(
        bits(wa.pop.resource_count.get_frozen_resources(cell)),
        bits(wb.pop.resource_count.get_frozen_resources(cell))
    );
    // Both worlds folded the pending change.
    let g = wb.pop.resource_count.get_spatial_resource(0).base();
    assert_eq!(g.element(cell).delta(), 0.0);
}

/// A full 30 x 30 world of `environment` with the output tests in the
/// bursts: the same output for 1, 2 and 4 threads, the tests out of the
/// sequential phase, and nothing below zero. Returns the one-thread run's
/// statistics.
fn thread_independent_world_of(prefix: &str, environment: &str, resources: usize) -> ParallelStats {
    let events = "u begin InjectAll filename=not-ancestor.org\n\
                  u 0:5:end PrintAverageData\n\
                  u 0:5:end PrintCountData\n\
                  u 0:5:end PrintTasksData\n\
                  u 0:5:end PrintResourceData\n\
                  u 80 SavePopulation\n\
                  u 80 Exit\n";
    let run_threads = |threads: &str| {
        let dir = global_resource_config(&format!("{prefix}_t{threads}"), environment);
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
        let rc = &w.pop.resource_count;
        assert!(rc.read_resources().iter().all(|&x| x >= 0.0));
        for i in 0..resources {
            if rc.is_spatial_resource(i) {
                let g = rc.get_spatial_resource(i).base();
                assert!(
                    (0..g.size()).all(|c| g.get_amount(c) >= 0.0),
                    "resource {i} below zero"
                );
            }
        }
        (dir, out, stepper.parallel_stats().unwrap())
    };
    let (d1, o1, s1) = run_threads("1");
    assert!(s1.commits > 100, "{}", s1.summary());
    assert!(
        s1.sequential_insts * 50 < s1.parallel_insts,
        "{}",
        s1.summary()
    );
    for threads in ["2", "4"] {
        let (d, o, s) = run_threads(threads);
        assert_eq!(
            (s.pool_tests, s.rounds, s.commits),
            (s1.pool_tests, s1.rounds, s1.commits)
        );
        compare_runs((&d1, &o1), (&d, &o))
            .unwrap_or_else(|e| panic!("EXECUTION_THREADS 1 and {threads}: {e}"));
    }
    s1
}

#[test]
fn reactions_on_grids_and_pools_world_does_not_depend_on_the_thread_count() {
    let s = thread_independent_world_of("both_full", BOTH_KINDS_ENVIRONMENT, 2);
    assert!(s.pool_tests > 50, "{}", s.summary());
}

#[test]
fn partial_resource_world_does_not_depend_on_the_thread_count() {
    let s = thread_independent_world_of("partial_full", PARTIAL_ENVIRONMENT, 2);
    assert!(s.pool_tests > 50, "{}", s.summary());
    let s = thread_independent_world_of("partial_grid_full", PARTIAL_AND_GRID_ENVIRONMENT, 2);
    assert!(s.pool_tests > 50, "{}", s.summary());
}

#[test]
fn gradient_world_does_not_depend_on_the_thread_count() {
    let s = thread_independent_world_of("gradient_full", GRADIENT_ENVIRONMENT, 1);
    // The gradient's tests ran in the bursts and never reached the world.
    assert_eq!(s.pool_tests, 0, "{}", s.summary());
}

/// `environment` over several seeds in the exact mode and in the parallel
/// mode, whose output tests run in the bursts: the outcomes at the end,
/// and the first resource left in the world, should not differ by more
/// than the seeds' own scatter (the loose |z| < 3.5 bound of the other
/// equivalence tests).
fn bursts_match_the_exact_mode_statistically(prefix: &str, environment: &str, min_pool_tests: u64) {
    let events = "u begin Inject not-ancestor.org\n\
                  u 300:10:600 PrintCountData\n\
                  u 300:10:600 PrintAverageData\n\
                  u 300:10:600 PrintTasksData\n\
                  u 300:10:600 PrintResourceData\n\
                  u 600 Exit\n";
    let mut outcomes: [Vec<[f64; 5]>; 2] = [Vec::new(), Vec::new()];
    for seed in 1..=6 {
        let seed_text = seed.to_string();
        for (mode, extra) in [("0", None), ("1", Some(PARALLEL))] {
            let mut sets = vec![("VERBOSITY", "0"), ("RANDOM_SEED", seed_text.as_str())];
            sets.extend(extra);
            let dir = global_resource_config(&format!("{prefix}_{mode}_{seed}"), environment);
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
                assert!(stats.pool_tests >= min_pool_tests, "{}", stats.summary());
                assert!(
                    stats.sequential_insts * 50 < stats.parallel_insts,
                    "{}",
                    stats.summary()
                );
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

#[test]
fn reactions_on_grids_and_pools_in_bursts_match_the_exact_mode_statistically() {
    bursts_match_the_exact_mode_statistically("both_equiv", BOTH_KINDS_ENVIRONMENT, 10);
}

#[test]
fn partial_resources_in_bursts_match_the_exact_mode_statistically() {
    bursts_match_the_exact_mode_statistically("partial_equiv", PARTIAL_ENVIRONMENT, 10);
}

#[test]
fn gradients_in_bursts_match_the_exact_mode_statistically() {
    bursts_match_the_exact_mode_statistically("gradient_equiv", GRADIENT_ENVIRONMENT, 0);
}
