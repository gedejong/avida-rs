//! Expected quality per genotype (`field_arith:...:expect=1`, the port-only
//! settings `EXPECTED_QUALITY_DRAWS` and `EXPECTED_QUALITY_MIN`; PORTING.md,
//! "Expected quality per genotype"): a task entry that asks for it is paid
//! at the genotype's average quality over several test-CPU runs with random
//! inputs, in place of the quality of the organism's own draw.
//!
//! The organisms are hand-written replicators (the set-up of the stock
//! ancestor, a task block of the ladder, the copy loop), injected into every
//! cell of a small world.

use std::fs;
use std::path::{Path, PathBuf};

use avida_config::cmdline::process_cmdline;
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

fn names(dir: &Path, file: &str) -> Vec<String> {
    fs::read_to_string(dir.join(file))
        .unwrap()
        .lines()
        .map(|l| l.split('#').next().unwrap().trim().to_string())
        .filter(|l| !l.is_empty())
        .collect()
}

/// The set-up of the stock ancestor, `block`, the copy loop.
fn replicator(block: &[String]) -> String {
    let mut lines: Vec<&str> = vec!["h-alloc", "h-search", "nop-C", "nop-A", "mov-head", "nop-C"];
    lines.extend(block.iter().map(String::as_str));
    lines.extend([
        "h-search", "h-copy", "if-label", "nop-C", "nop-A", "h-divide",
    ]);
    lines.extend(["mov-head", "nop-A", "nop-B"]);
    format!(
        "#inst_set heads_default\n#hw_type 0\n{}\n",
        lines.join("\n")
    )
}

/// The task block of `ladder-halfadd-planes-heads.org` up to the idle trap:
/// the half adder, two outputs.
fn planes_block(dir: &Path) -> Vec<String> {
    let org = names(dir, "ladder-halfadd-planes-heads.org");
    let trap = org.iter().position(|n| n == "get-head").unwrap();
    org[..trap].to_vec()
}

/// The same block cut after its first output, which is `x ^ y` of the two
/// newest inputs: the output IO tests the output before it reads the next
/// input, so the two newest are the second and the third; the block's
/// `push` is moved behind the second IO to keep the second input (and not
/// the first) for the `nand` steps. A two-bit `add` gets that partly right.
fn xor_block(dir: &Path) -> Vec<String> {
    let org = names(dir, "ladder-halfadd-planes-heads.org");
    let fourth_io = org
        .iter()
        .enumerate()
        .filter(|(_, n)| *n == "IO")
        .map(|(i, _)| i)
        .nth(3)
        .unwrap();
    let mut block = org[..=fourth_io].to_vec();
    assert_eq!((block[1].as_str(), block[2].as_str()), ("push", "IO"));
    block.swap(1, 2);
    block
}

struct Setup {
    dir: PathBuf,
}

/// The stock configuration with `reactions` as its environment (no
/// resources), `environment_extra` after them, and `block` as the injected
/// replicator.
fn setup(name: &str, block: impl Fn(&Path) -> Vec<String>, reactions: &str, extra: &str) -> Setup {
    setup_until(name, block, reactions, extra, 40)
}

fn setup_until(
    name: &str,
    block: impl Fn(&Path) -> Vec<String>,
    reactions: &str,
    extra: &str,
    exit: u32,
) -> Setup {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR"))
        .join("expected_quality")
        .join(name);
    let _ = fs::remove_dir_all(&dir);
    let src = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../../avida-core/support/config");
    copy_dir(&src, &dir);
    let cfg = fs::read_to_string(dir.join("avida.cfg")).unwrap();
    fs::write(
        dir.join("avida.cfg"),
        cfg.replace(
            "ENVIRONMENT_FILE environment.cfg",
            "ENVIRONMENT_FILE environment-expect.cfg",
        ),
    )
    .unwrap();
    fs::write(
        dir.join("environment-expect.cfg"),
        format!("{reactions}\n{extra}\n"),
    )
    .unwrap();
    let org = replicator(&block(&dir));
    fs::write(dir.join("replicator.org"), org).unwrap();
    fs::write(
        dir.join("events.cfg"),
        format!(
            "u begin InjectAll filename=replicator.org\n\
             u 0:1:end PrintCountData\n\
             u 0:1:end PrintTasksData\n\
             u 0:1:end PrintAverageData\n\
             u {exit} Exit\n"
        ),
    )
    .unwrap();
    Setup { dir }
}

const NO_MUTATION: [(&str, &str); 3] = [
    ("COPY_MUT_PROB", "0"),
    ("DIVIDE_INS_PROB", "0"),
    ("DIVIDE_DEL_PROB", "0"),
];

fn make_world(dir: &Path, sets: &[(&str, &str)]) -> Result<World, String> {
    let cwd = dir.to_string_lossy().into_owned();
    let mut args: Vec<String> = vec!["avida".into()];
    for (n, v) in [("WORLD_X", "10"), ("WORLD_Y", "10"), ("RANDOM_SEED", "42")]
        .iter()
        .chain(sets)
    {
        args.extend(["-set".into(), n.to_string(), v.to_string()]);
    }
    let cmd = process_cmdline(&args, &cwd);
    assert!(cmd.exit.is_none(), "command line: {}", cmd.stderr);
    World::new(cmd.config, &cwd, &cmd.defs).map(|(w, _)| w)
}

/// Runs to the end; the world and the status lines.
fn run(dir: &Path, sets: &[(&str, &str)]) -> (World, Vec<u8>) {
    let mut w = make_world(dir, sets).expect("World::new");
    let mut stepper = Stepper::new(&w);
    let mut out = Vec::new();
    while stepper.begin_update(&mut w, &mut out).unwrap() {
        while stepper.step_slice(&mut w).unwrap() {}
        stepper.end_update(&mut w, &mut out).unwrap();
    }
    w.out.flush_all();
    (w, out)
}

/// Every file of `data/`, by name, without the comment lines (the header
/// has the time of the run).
fn data_files(dir: &Path) -> Vec<(String, Vec<u8>)> {
    let mut files: Vec<(String, Vec<u8>)> = fs::read_dir(dir.join("data"))
        .unwrap()
        .map(|e| e.unwrap())
        .map(|e| {
            (
                e.file_name().to_string_lossy().into_owned(),
                fs::read_to_string(e.path())
                    .unwrap()
                    .lines()
                    .filter(|l| !l.starts_with('#'))
                    .collect::<Vec<_>>()
                    .join("\n")
                    .into_bytes(),
            )
        })
        .collect();
    files.sort();
    files
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

fn expected_task(w: &World) -> usize {
    w.env
        .tasklib()
        .0
        .tasks()
        .iter()
        .position(|t| t.is_expected())
        .expect("a task with expect=1")
}

const HALFADD: &str = "field_arith:fn=halfadd:bits=1:mode=planes:thresh=0.75";
const PARTIAL: &str = "field_arith:fn=add:bits=2:width=24:thresh=0";

fn reaction(task: &str, expect: bool) -> String {
    let flag = if expect { ":expect=1" } else { "" };
    format!("REACTION PAID {task}{flag} process:value=1.0:type=pow requisite:max_count=1")
}

#[test]
fn without_an_expected_task_the_setting_changes_nothing() {
    let mut runs = Vec::new();
    for draws in ["8", "0"] {
        let s = setup(
            &format!("none_{draws}"),
            planes_block,
            &reaction(HALFADD, false),
            "",
        );
        let mut sets = NO_MUTATION.to_vec();
        sets.push(("EXPECTED_QUALITY_DRAWS", draws));
        let (w, out) = run(&s.dir, &sets);
        assert_eq!(w.host.expected_computed, 0);
        assert!(w.host.expected.is_empty());
        let t = last_row(&s.dir, "tasks.dat");
        assert!(t[1] > 0.0, "the rung is done: {t:?}");
        runs.push((out, data_files(&s.dir)));
    }
    assert!(runs[0].0 == runs[1].0, "status lines differ");
    assert!(runs[0].1 == runs[1].1, "data files differ");
}

#[test]
fn an_exact_solver_is_paid_as_without_the_feature() {
    let mut runs = Vec::new();
    for expect in [false, true] {
        // Through the first gestation: later organisms read their inputs in
        // a rotated order, so their own draws are no longer exact, and the
        // genotype's table (exact) is rightly worth more than they earned.
        let s = setup_until(
            &format!("exact_{expect}"),
            planes_block,
            &reaction(HALFADD, expect),
            "",
            12,
        );
        let (w, out) = run(&s.dir, &NO_MUTATION);
        if expect {
            assert_eq!(w.host.expected_computed, 1, "one genotype, one table");
            let task = expected_task(&w);
            let tables: Vec<_> = w.host.expected.values().collect();
            assert_eq!(tables.len(), 1);
            assert_eq!(tables[0][task], 1.0, "exact on every draw");
        } else {
            assert_eq!(w.host.expected_computed, 0);
        }
        let t = last_row(&s.dir, "tasks.dat");
        assert!(t[1] > 0.0, "the rung is done: {t:?}");
        runs.push((out, data_files(&s.dir)));
    }
    assert!(runs[0].0 == runs[1].0, "status lines differ");
    assert!(runs[0].1 == runs[1].1, "data files differ");
}

#[test]
fn later_gestations_are_paid_the_genotypes_exact_table() {
    // After the first gestation the organisms' own draws are below 1 (their
    // inputs are read in a rotated order), and with the feature the genotype
    // is paid its table of 1: the mean merit is higher and one table was made.
    let mut merit = Vec::new();
    for expect in [false, true] {
        let s = setup(
            &format!("later_{expect}"),
            planes_block,
            &reaction(HALFADD, expect),
            "",
        );
        let (w, _) = run(&s.dir, &NO_MUTATION);
        assert_eq!(w.host.expected_computed, u64::from(expect));
        merit.push(last_row(&s.dir, "average.dat")[1]);
    }
    assert!(merit[1] > merit[0], "{merit:?}");
}

/// The quality of `a ^ b` against `add` with two-bit operands in three-bit
/// fields below bit 24 (`fn=add:bits=2:width=24:thresh=0`), on one random
/// draw of the two inputs, written out here and not taken from the tasks
/// crate: eight fields, three result bits each, `q = max(0, 2f - 1)` for the
/// fraction `f` of the 24 bits that are right.
fn xor_quality(rng: &mut u64) -> f64 {
    let mut next = || {
        // SplitMix64: independent of the port's generators.
        *rng = rng.wrapping_add(0x9e37_79b9_7f4a_7c15);
        let mut z = *rng;
        z = (z ^ (z >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
        ((z ^ (z >> 31)) >> 8) as u32 & 0x00ff_ffff
    };
    let (a, b) = (next(), next());
    let mut right = 0;
    for field in 0..8 {
        let shift = 3 * field;
        let (x, y) = ((a >> shift) & 3, (b >> shift) & 3);
        let sum = x + y;
        let out = x ^ y;
        right += (!(sum ^ out) & 7).count_ones();
    }
    let f = f64::from(right) / 24.0;
    (2.0 * f - 1.0).max(0.0)
}

#[test]
fn a_partial_solver_gets_one_table_between_zero_and_one() {
    let draws = 8;
    let s = setup("partial", xor_block, &reaction(PARTIAL, true), "");
    let mut sets = NO_MUTATION.to_vec();
    let n = draws.to_string();
    sets.push(("EXPECTED_QUALITY_DRAWS", &n));
    // No minimum: another seed's estimate may fall below the stock one.
    sets.push(("EXPECTED_QUALITY_MIN", "0"));
    let (w, _) = run(&s.dir, &sets);
    let task = expected_task(&w);
    assert_eq!(w.host.expected_computed, 1, "one genotype, one table");
    let table = *w.host.expected.values().next().unwrap().get(task).unwrap();
    assert!(table > 0.0 && table < 1.0, "table {table}");

    // The mean of the same quantity over many independent draws; the table
    // is the mean of `draws` of them, so it may differ by a few standard
    // errors of that.
    let mut rng = 0x1234_5678_9abc_def0u64;
    let samples: Vec<f64> = (0..200_000).map(|_| xor_quality(&mut rng)).collect();
    let mean = samples.iter().sum::<f64>() / samples.len() as f64;
    let var = samples.iter().map(|q| (q - mean).powi(2)).sum::<f64>() / samples.len() as f64;
    let tolerance = 5.0 * (var / f64::from(draws)).sqrt() + 0.005;
    assert!(
        (table - mean).abs() < tolerance,
        "table {table}, mean {mean} (+- {tolerance})"
    );

    // Every organism is paid that one value: the organisms of the genotype
    // that did the task carry it as their task quality.
    let task_quality: Vec<f64> = w
        .pop
        .orgs
        .iter()
        .flatten()
        .map(|o| o.state.phenotype.cur_task_quality[task])
        .collect();
    assert!(task_quality.len() > 50);
    assert!(
        task_quality.iter().all(|&q| q == 0.0 || q == table),
        "own draws leaked into the pay: {:?}",
        task_quality
            .iter()
            .copied()
            .filter(|&q| q != 0.0 && q != table)
            .collect::<Vec<_>>()
    );

    // The same genome gets the same table again (a private stream).
    let s2 = setup("partial_again", xor_block, &reaction(PARTIAL, true), "");
    let (w2, _) = run(&s2.dir, &sets);
    assert_eq!(w2.host.expected, w.host.expected);
    // And a world with another seed gets another estimate of the mean.
    let mut sets3 = sets.clone();
    sets3.push(("RANDOM_SEED", "43"));
    let (w3, _) = run(&s2.dir, &sets3);
    let table3 = w3.host.expected.values().next().unwrap()[task];
    assert!(table3 > 0.0 && table3 < 1.0 && table3 != table, "{table3}");
}

#[test]
fn a_table_below_the_minimum_counts_as_zero_and_the_reaction_does_not_fire() {
    let mut fired = Vec::new();
    for min in ["0.05", "0.9"] {
        let s = setup(
            &format!("min_{min}"),
            xor_block,
            &reaction(PARTIAL, true),
            "",
        );
        let mut sets = NO_MUTATION.to_vec();
        sets.push(("EXPECTED_QUALITY_MIN", min));
        let (w, _) = run(&s.dir, &sets);
        let task = expected_task(&w);
        let table = w.host.expected.values().next().unwrap()[task];
        let t = last_row(&s.dir, "tasks.dat");
        fired.push((table, t[1]));
    }
    assert!(fired[0].0 > 0.05 && fired[0].1 > 0.0, "{fired:?}");
    assert_eq!(fired[1], (0.0, 0.0), "{fired:?}");
}

#[test]
fn expected_tasks_without_draws_stop_the_run_at_load() {
    let s = setup("no_draws", planes_block, &reaction(HALFADD, true), "");
    let Err(err) = make_world(&s.dir, &[("EXPECTED_QUALITY_DRAWS", "0")]) else {
        panic!("expect=1 with EXPECTED_QUALITY_DRAWS 0 must not start");
    };
    assert!(
        err.contains("expect=1") && err.contains("EXPECTED_QUALITY_DRAWS"),
        "{err}"
    );
    // The same environment without the flag starts.
    let ok = setup("no_draws_ok", planes_block, &reaction(HALFADD, false), "");
    assert!(make_world(&ok.dir, &[("EXPECTED_QUALITY_DRAWS", "0")]).is_ok());
    // The parallel mode refuses the flag, loudly.
    let Err(err) = make_world(&s.dir, &[("EXECUTION_MODE", "1")]) else {
        panic!("the parallel mode must stop");
    };
    assert!(err.starts_with("unsupported: EXECUTION_MODE 1"), "{err}");
}

#[test]
fn the_test_runs_see_the_input_mask() {
    // With every input masked to 0 the XOR organism is exact on every run:
    // a table of exactly 1 shows the runs drew masked inputs. Without the
    // mask the same organism averages about 7/12.
    let mut tables = Vec::new();
    for mask in [Some("INPUT_AND_MASK 0"), None] {
        let s = setup(
            &format!("mask_{}", mask.is_some()),
            xor_block,
            &reaction(PARTIAL, true),
            mask.unwrap_or(""),
        );
        let (w, _) = run(&s.dir, &NO_MUTATION);
        let task = expected_task(&w);
        tables.push(w.host.expected.values().next().unwrap()[task]);
    }
    assert_eq!(tables[0], 1.0);
    assert!(tables[1] < 0.9 && tables[1] > 0.05, "{tables:?}");
}

#[test]
fn the_table_goes_with_the_genotype() {
    let s = setup("extinct", xor_block, &reaction(PARTIAL, true), "");
    let sets = [
        ("COPY_MUT_PROB", "0.02"),
        ("DIVIDE_INS_PROB", "0.1"),
        ("DIVIDE_DEL_PROB", "0.1"),
        ("DROP_EXTINCT_GENOTYPES", "1"),
    ];
    let (mut w, _) = run(&s.dir, &sets);
    assert!(w.host.expected_computed > 5, "{}", w.host.expected_computed);
    let before = w.host.expected.len();
    w.prune_expected_tables();
    let after = w.host.expected.len();
    let arb = w.systematics.genotypes().unwrap();
    let alive: std::collections::HashSet<i32> = arb
        .iter_groups()
        .into_iter()
        .map(|h| arb.genotype(h).unwrap().id())
        .collect();
    assert!(after < before, "{before} tables before, {after} after");
    assert!(
        w.host.expected.keys().all(|id| alive.contains(id)),
        "a table outlived its genotype"
    );
    // Every genotype that lives and did the task keeps its table.
    assert!(after > 0 && after <= alive.len());
}
