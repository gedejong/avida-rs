//! The opt-in `heads_modular` instruction set in whole runs: the
//! hand-written subroutine organism (`modular-subroutine.org`) keeps
//! replicating exactly when mutations are off and performs its three tasks,
//! and a parallel run with the set depends on the seed only, not on
//! `EXECUTION_THREADS`.

use std::fs;
use std::path::{Path, PathBuf};

use avida_config::cmdline::process_cmdline;
use avida_world::parallel::ParallelStats;
use avida_world::run::Stepper;
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

/// The stock configuration with the modular set, `modular-subroutine.org`
/// injected into every cell (so that the rounds are large enough for the
/// worker pool) and `events` as its event list.
fn config(name: &str, events: &str) -> PathBuf {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR"))
        .join("modular")
        .join(name);
    let _ = fs::remove_dir_all(&dir);
    let src = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../../avida-core/support/config");
    copy_dir(&src, &dir);
    fs::write(dir.join("events.cfg"), events).unwrap();
    dir
}

const EVENTS: &str = "u begin InjectAll filename=modular-subroutine.org\n\
                      u 0:10:end PrintCountData\n\
                      u 0:10:end PrintTasksData\n\
                      u 60 Exit\n";

fn run(dir: &Path, sets: &[(&str, &str)]) -> (Vec<u8>, Option<ParallelStats>) {
    let cwd = dir.to_string_lossy().into_owned();
    let mut args: Vec<String> = ["avida", "-def", "INST_SET", "instset-heads-modular.cfg"]
        .iter()
        .map(|s| s.to_string())
        .collect();
    for (n, v) in [("WORLD_X", "30"), ("WORLD_Y", "30"), ("RANDOM_SEED", "42")]
        .iter()
        .chain(sets)
    {
        args.extend(["-set".into(), n.to_string(), v.to_string()]);
    }
    let cmd = process_cmdline(&args, &cwd);
    assert!(cmd.exit.is_none(), "command line: {}", cmd.stderr);
    let (mut w, _): (World, _) = World::new(cmd.config, &cwd, &cmd.defs).expect("World::new");
    let mut stepper = Stepper::new(&w);
    let mut out = Vec::new();
    while stepper.begin_update(&mut w, &mut out).unwrap() {
        while stepper.step_slice(&mut w).unwrap() {}
        stepper.end_update(&mut w, &mut out).unwrap();
    }
    w.out.flush_all();
    (out, stepper.parallel_stats())
}

/// The last data row of `data/file`, as numbers.
fn last_row(dir: &Path, file: &str) -> Vec<f64> {
    let text = fs::read_to_string(dir.join("data").join(file)).unwrap();
    text.lines()
        .rfind(|l| !l.starts_with('#') && !l.trim().is_empty())
        .unwrap()
        .split_whitespace()
        .map(|v| v.parse().unwrap())
        .collect()
}

#[test]
fn without_mutations_the_organism_stays_one_genotype_and_does_its_tasks() {
    let dir = config("exact", EVENTS);
    let zero = [
        ("COPY_MUT_PROB", "0"),
        ("DIVIDE_INS_PROB", "0"),
        ("DIVIDE_DEL_PROB", "0"),
    ];
    let (_, stats) = run(&dir, &zero);
    assert!(stats.is_none(), "exact mode");
    // count.dat: update, instructions, organisms, genotypes, ... births,
    // deaths, breed-true births (see the file header).
    let row = last_row(&dir, "count.dat");
    assert_eq!(row[3], 1.0, "one genotype: {row:?}");
    assert_eq!(row[2], 900.0, "organisms: {row:?}");
    assert!(row[8] > 0.0 && row[8] == row[10], "every birth breeds true");
    // tasks.dat: update, then the nine tasks: not, nand and all done, the
    // other six never.
    let t = last_row(&dir, "tasks.dat");
    assert!(t[1] > 0.0 && t[2] > 0.0 && t[3] > 0.0, "{t:?}");
    assert!(t[4..10].iter().all(|&c| c == 0.0), "{t:?}");
}

#[test]
fn parallel_run_with_the_modular_set_does_not_depend_on_the_thread_count() {
    let mut first: Option<(Vec<u8>, String, ParallelStats)> = None;
    for threads in ["1", "2", "4"] {
        let dir = config(&format!("t{threads}"), EVENTS);
        let (out, stats) = run(
            &dir,
            &[("EXECUTION_MODE", "1"), ("EXECUTION_THREADS", threads)],
        );
        let s = stats.expect("EXECUTION_MODE 1 runs the parallel mode");
        // The new instructions run in the bursts.
        assert!(s.parallel_insts > 3 * s.sequential_insts, "{}", s.summary());
        let data = ["count.dat", "tasks.dat"]
            .iter()
            .map(|f| fs::read_to_string(dir.join("data").join(f)).unwrap())
            .map(|t| {
                t.lines()
                    .filter(|l| !l.starts_with('#'))
                    .collect::<Vec<_>>()
                    .join("\n")
            })
            .collect::<Vec<_>>()
            .join("\n");
        match &first {
            None => first = Some((out, data, s)),
            Some((o1, d1, s1)) => {
                let (a, b) = (String::from_utf8_lossy(&out), String::from_utf8_lossy(o1));
                let diff = a.lines().zip(b.lines()).find(|(x, y)| x != y);
                assert!(diff.is_none(), "status lines, {threads} threads: {diff:?}");
                assert_eq!(&data, d1, "data files, {threads} threads");
                assert_eq!(
                    (s.parallel_insts, s.sequential_insts, s.rounds, s.bursts),
                    (s1.parallel_insts, s1.sequential_insts, s1.rounds, s1.bursts)
                );
            }
        }
    }
}
