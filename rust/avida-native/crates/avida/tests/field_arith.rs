//! The adder ladder in whole runs: replicators that carry the hand-written
//! `field_arith` task blocks (`ladder-*planes*.org`) do the planes rungs,
//! the output tests run in the parallel mode's bursts, and a parallel run
//! depends on the seed only, not on `EXECUTION_THREADS`.

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

/// The instruction names of an `.org` file of the stock configuration.
fn names(dir: &Path, file: &str) -> Vec<String> {
    fs::read_to_string(dir.join(file))
        .unwrap()
        .lines()
        .map(|l| l.split('#').next().unwrap().trim().to_string())
        .filter(|l| !l.is_empty())
        .collect()
}

/// A replicator: the set-up of the stock ancestor, the task block of a
/// ladder organism (its idle trap and subroutine taken off), the copy loop,
/// and the subroutine after it (call searches forward).
fn replicator(dir: &Path, file: &str, modular: bool) -> String {
    let org = names(dir, file);
    let trap = org.iter().position(|n| n == "get-head").unwrap();
    let block = &org[..trap];
    let after_trap = &org[trap + 4..];
    let (set, hs, label) = if modular {
        ("heads_modular", "nop-F", "nop-F")
    } else {
        ("heads_default", "nop-C", "nop-C")
    };
    let mut lines: Vec<&str> = vec!["h-alloc", "h-search", hs, "nop-A", "mov-head", "nop-C"];
    lines.extend(block.iter().map(String::as_str));
    lines.extend(["h-search", "h-copy", "if-label", label, "nop-A", "h-divide"]);
    lines.extend(["mov-head", "nop-A"]);
    if after_trap.is_empty() {
        // The one nop-A is the modifier of mov-head and the end marker's
        // first nop at once, as in the stock ancestor.
        lines.push("nop-B");
    } else {
        lines.extend(after_trap.iter().map(String::as_str));
        lines.extend(["nop-A", "nop-B"]);
    }
    format!("#inst_set {set}\n#hw_type 0\n{}\n", lines.join("\n"))
}

/// The stock configuration with the ladder as its environment, the given
/// instruction set, a replicator injected into every cell, and `events`.
fn config(name: &str, modular: bool, org: &str) -> PathBuf {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR"))
        .join("field_arith")
        .join(name);
    let _ = fs::remove_dir_all(&dir);
    let src = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../../avida-core/support/config");
    copy_dir(&src, &dir);
    let cfg = fs::read_to_string(dir.join("avida.yaml")).unwrap();
    let start = cfg.find("\nenvironment:\n").unwrap();
    let end = cfg[start + 1..].find("\nevents:").unwrap() + start + 1;
    fs::write(
        dir.join("avida.yaml"),
        format!(
            "{}\nenvironment: environment-adder-ladder.yaml\n{}",
            &cfg[..start],
            &cfg[end..]
        ),
    )
    .unwrap();
    fs::write(dir.join("replicator.org"), replicator(&dir, org, modular)).unwrap();
    avida_config::convert::replace_section(
        &dir,
        "avida.yaml",
        "events",
        "u begin InjectAll filename=replicator.org\n\
         u 0:10:end PrintCountData\n\
         u 0:10:end PrintTasksData\n\
         u 50 Exit\n",
    )
    .unwrap();
    dir
}

fn run(dir: &Path, modular: bool, sets: &[(&str, &str)]) -> (Vec<u8>, Option<ParallelStats>) {
    let cwd = dir.to_string_lossy().into_owned();
    let mut args: Vec<String> = vec!["avida".into()];
    if modular {
        args.extend(["-set", "instruction_sets", "heads_modular"].map(String::from));
    }
    for (n, v) in [
        ("world.x", "30"),
        ("world.y", "30"),
        ("general.random_seed", "42"),
    ]
    .iter()
    .chain(sets)
    {
        args.extend(["-set".into(), n.to_string(), v.to_string()]);
    }
    let cmd = process_cmdline(&args, &cwd);
    assert!(cmd.exit.is_none(), "command line: {}", cmd.stderr);
    let (mut w, _): (World, _) = World::new(cmd.config, &cwd).expect("World::new");
    let mut stepper = Stepper::new(&w);
    let mut out = Vec::new();
    while stepper.begin_update(&mut w, &mut out).unwrap() {
        while stepper.step_slice(&mut w).unwrap() {}
        stepper.end_update(&mut w, &mut out).unwrap();
    }
    w.out.flush_all();
    (out, stepper.parallel_stats())
}

fn last_row(dir: &Path, file: &str) -> Vec<f64> {
    let text = fs::read_to_string(dir.join("data").join(file)).unwrap();
    text.lines()
        .rfind(|l| !l.starts_with('#') && !l.trim().is_empty())
        .unwrap()
        .split_whitespace()
        .map(|v| v.parse().unwrap())
        .collect()
}

const ZERO: [(&str, &str); 3] = [
    ("mutation.copy_prob", "0"),
    ("mutation.divide_ins_prob", "0"),
    ("mutation.divide_del_prob", "0"),
];

/// tasks.dat: update, then the eight reactions of the ladder in file order
/// (NOT, NAND, XOR, AND, HALFADD_PLANES, HALFADD, ADD2_PLANES, ADD2).
#[test]
fn replicators_with_planes_blocks_do_their_rung_and_breed_true() {
    for (modular, file, rung) in [
        (false, "ladder-halfadd-planes-heads.org", 5),
        (false, "ladder-add2-planes-heads.org", 7),
        (true, "ladder-halfadd-planes-modular.org", 5),
        (true, "ladder-add2-planes-modular.org", 7),
    ] {
        let dir = config(&format!("exact_{file}"), modular, file);
        let (_, stats) = run(&dir, modular, &ZERO);
        assert!(stats.is_none(), "exact mode");
        let c = last_row(&dir, "count.dat");
        assert_eq!(c[3], 1.0, "{file}: one genotype: {c:?}");
        assert_eq!(c[2], 900.0, "{file}: organisms: {c:?}");
        let t = last_row(&dir, "tasks.dat");
        assert!(t[rung] > 0.0, "{file}: rung {rung} done by nobody: {t:?}");
    }
}

#[test]
fn parallel_run_with_field_arith_does_not_depend_on_the_thread_count() {
    for (modular, file) in [
        (false, "ladder-add2-planes-heads.org"),
        (true, "ladder-halfadd-planes-modular.org"),
    ] {
        let mut first: Option<(Vec<u8>, String, ParallelStats)> = None;
        for threads in ["1", "2", "4"] {
            let dir = config(&format!("par_{file}_{threads}"), modular, file);
            let (out, stats) = run(
                &dir,
                modular,
                &[("execution.mode", "1"), ("execution.threads", threads)],
            );
            let s = stats.expect("EXECUTION_MODE 1 runs the parallel mode");
            // The IO instructions, which carry the output tests, run in the
            // bursts: the sequential phases are the divides' world halves.
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
            let t = last_row(&dir, "tasks.dat");
            assert!(
                t[5] > 0.0 || t[7] > 0.0,
                "{file}: a planes rung is done: {t:?}"
            );
            match &first {
                None => first = Some((out, data, s)),
                Some((o1, d1, s1)) => {
                    let (a, b) = (String::from_utf8_lossy(&out), String::from_utf8_lossy(o1));
                    let diff = a.lines().zip(b.lines()).find(|(x, y)| x != y);
                    assert!(diff.is_none(), "status lines, {threads} threads: {diff:?}");
                    assert_eq!(&data, d1, "data files, {file}, {threads} threads");
                    assert_eq!(
                        (s.parallel_insts, s.sequential_insts, s.rounds, s.bursts),
                        (s1.parallel_insts, s1.sequential_insts, s1.rounds, s1.bursts)
                    );
                }
            }
        }
    }
}
