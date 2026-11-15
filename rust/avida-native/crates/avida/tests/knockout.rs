//! The knockout map and the reuse measures (`PrintKnockoutMap`,
//! `PrintReuseData`, `World::knockout_map_of`).

use std::fs;
use std::path::{Path, PathBuf};

use avida_config::cmdline::process_cmdline;
use avida_world::knockout::{KnockoutMap, SiteClass};
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

fn config(name: &str, events: &str) -> PathBuf {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR"))
        .join("knockout")
        .join(name);
    let _ = fs::remove_dir_all(&dir);
    let src = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../../avida-core/support/config");
    copy_dir(&src, &dir);
    fs::write(dir.join("events.cfg"), events).unwrap();
    dir
}

fn build(dir: &Path, modular: bool, sets: &[(&str, &str)]) -> World {
    let cwd = dir.to_string_lossy().into_owned();
    let mut args: Vec<String> = vec!["avida".into()];
    if modular {
        args.extend(["-def", "INST_SET", "instset-heads-modular.cfg"].map(String::from));
    }
    for (n, v) in [
        ("WORLD_X", "10"),
        ("WORLD_Y", "10"),
        ("RANDOM_SEED", "42"),
        ("COPY_MUT_PROB", "0"),
        ("DIVIDE_INS_PROB", "0"),
        ("DIVIDE_DEL_PROB", "0"),
    ]
    .iter()
    .chain(sets)
    {
        args.extend(["-set".into(), n.to_string(), v.to_string()]);
    }
    let cmd = process_cmdline(&args, &cwd);
    assert!(cmd.exit.is_none(), "command line: {}", cmd.stderr);
    World::new(cmd.config, &cwd, &cmd.defs)
        .expect("World::new")
        .0
}

/// Run `updates` updates.
fn run(w: &mut World, updates: usize) {
    let mut stepper = Stepper::new(w);
    let mut out = Vec::new();
    for _ in 0..updates {
        if !stepper.begin_update(w, &mut out).unwrap() {
            break;
        }
        while stepper.step_slice(w).unwrap() {}
        stepper.end_update(w, &mut out).unwrap();
    }
    w.out.flush_all();
}

fn map_of_cell0(dir: &Path, modular: bool) -> KnockoutMap {
    let mut w = build(dir, modular, &[]);
    run(&mut w, 1);
    let (genome, id) = w.organism_genome(0).expect("an organism in cell 0");
    w.knockout_map_of(&genome, id).unwrap()
}

/// Task indices of the stock environment.
const NOT: usize = 0;
const NAND: usize = 1;
const AND: usize = 2;

/// The site of the n-th (0-based) instruction named `name`.
fn nth_site(m: &KnockoutMap, name: &str, n: usize) -> usize {
    m.inst_names
        .iter()
        .enumerate()
        .filter(|(_, i)| i.as_str() == name)
        .nth(n)
        .unwrap_or_else(|| panic!("no {name} number {n}"))
        .0
}

fn lost(m: &KnockoutMap, site: usize) -> Vec<usize> {
    m.sites[site].lost.clone()
}

#[test]
fn the_shared_subroutine_is_found_by_its_knockout() {
    let dir = config("modular", "u begin Inject modular-subroutine.org\n");
    let m = map_of_cell0(&dir, true);
    assert!(m.base_viable);
    assert_eq!(m.symbols.chars().count(), 57);
    assert_eq!(m.performed, vec![NOT, NAND, AND]);
    // The same inputs for the base and every knockout: the test CPU's fixed
    // inputs, not drawn from any random stream.
    assert_eq!(m.inputs, vec![0x0f13149f, 0x3308e53e, 0x556241eb]);

    // The subroutine's one `nand` serves all three tasks.
    let nand = nth_site(&m, "nand", 0);
    assert_eq!(m.sites[nand].class, SiteClass::Shared);
    assert_eq!(lost(&m, nand), vec![NOT, NAND, AND]);
    assert_eq!(m.lost_bits(nand), "111000000");
    // Each call site removes a known subset: the first use serves NOT, the
    // second NAND and AND (through the intermediate it makes), the third AND.
    let calls: Vec<usize> = (0..3).map(|n| nth_site(&m, "call", n)).collect();
    assert_eq!(lost(&m, calls[0]), vec![NOT]);
    assert_eq!(m.sites[calls[0]].class, SiteClass::Task);
    assert_eq!(lost(&m, calls[1]), vec![NAND, AND]);
    assert_eq!(m.sites[calls[1]].class, SiteClass::Shared);
    assert_eq!(lost(&m, calls[2]), vec![AND]);
    assert_eq!(m.sites[calls[2]].class, SiteClass::Task);
    // The replication machinery is lethal; `return` too (the stack).
    for name in ["h-alloc", "h-divide", "h-copy", "return"] {
        assert_eq!(
            m.sites[nth_site(&m, name, 0)].class,
            SiteClass::Lethal,
            "{name}"
        );
    }
    // The per-task IOs: NAND's own output and AND's own output.
    let ios: Vec<usize> = (0..4).map(|n| nth_site(&m, "IO", n)).collect();
    assert_eq!(lost(&m, ios[2]), vec![NAND]);
    assert_eq!(lost(&m, ios[3]), vec![AND]);
    // A knockout that loses no task but changes the fitness is `speed`.
    assert_eq!(m.sites[nth_site(&m, "if-label", 0)].class, SiteClass::Speed);
    assert!(m.sites[nth_site(&m, "if-label", 0)].rel_fitness < 1.0);

    let r = m.measures();
    assert_eq!(r.tasks_performed, 3);
    assert_eq!(r.shared_sites, Some(14));
    assert_eq!(r.class_counts, [15, 14, 16, 12, 0]);
    assert_eq!(r.sites_per_task, vec![(NOT, 11), (NAND, 15), (AND, 25)]);
    let idx = r.reuse_index.unwrap();
    assert!((idx - 0.382_753_967).abs() < 1e-6, "{idx}");
    // NOT and AND overlap least (the NOT task has little code of its own to
    // share), NAND and AND most.
    let j = |a, b| {
        r.pairs
            .iter()
            .find(|p| p.0 == a && p.1 == b)
            .unwrap()
            .2
            .unwrap()
    };
    assert!(j(NAND, AND) > j(NOT, NAND) && j(NOT, NAND) > j(NOT, AND));
    // The call indicators: three call sites, one target (the subroutine),
    // all three matter.
    let c = r.calls.unwrap();
    assert_eq!((c.call_sites, c.distinct_targets, c.non_neutral), (3, 1, 3));
    // Every lethal site is outside every task's site set.
    for t in [NOT, NAND, AND] {
        for s in m.task_sites(t) {
            assert_ne!(m.sites[s].class, SiteClass::Lethal);
        }
    }
}

#[test]
fn the_map_does_not_depend_on_the_state_of_the_run() {
    // Same genome, a run that has progressed (mutating, random draws made):
    // the same map.
    let dir = config("state", "u begin Inject modular-subroutine.org\n");
    let a = map_of_cell0(&dir, true);
    let mut w = build(&dir, true, &[]);
    run(&mut w, 1);
    let (genome, id) = w.organism_genome(0).unwrap();
    for _ in 0..3 {
        run(&mut w, 5);
    }
    let b = w.knockout_map_of(&genome, id).unwrap();
    assert_eq!(
        (a.sites, a.inputs, a.base_fitness),
        (b.sites, b.inputs, b.base_fitness)
    );
}

#[test]
fn an_organism_without_tasks_has_undefined_measures() {
    let dir = config("stock", "u begin Inject default-heads.org\n");
    let m = map_of_cell0(&dir, false);
    assert!(m.base_viable);
    assert!(m.performed.is_empty());
    assert!(m.calls.is_none(), "the stock set has no call");
    let r = m.measures();
    assert_eq!((r.shared_sites, r.reuse_index), (None, None));
    assert!(r.pairs.is_empty());
    // Only lethal, speed and neutral sites exist.
    assert_eq!(r.class_counts[1] + r.class_counts[2], 0);
    assert!(r.class_counts[0] > 0 && r.class_counts[4] > 0);
    // The files use -1 for what is not defined.
    assert_eq!(avida_world::knockout::opt_f(r.reuse_index), "-1");
}

#[test]
fn an_organism_that_does_not_replicate_has_no_map() {
    let dir = config("dead", "u begin Inject default-heads.org\n");
    let mut w = build(&dir, false, &[]);
    run(&mut w, 1);
    let (genome, _) = w.organism_genome(0).unwrap();
    // Remove h-divide by truncating the genome to its first 20 sites.
    let mut seq = genome.representation().clone();
    seq.resize(20);
    let g = avida_genome::Genome::with_inst_set(genome.hardware_type(), genome.inst_set(), seq);
    let m = w.knockout_map_of(&g, -1).unwrap();
    assert!(!m.base_viable && m.sites.is_empty());
    assert_eq!(m.measures().shared_sites, None);
    assert!(m
        .file_lines()
        .iter()
        .any(|l| l.contains("does not replicate")));
}

#[test]
fn the_shared_subroutine_is_counted_as_subroutine_reuse() {
    // The body `nand; return` is entered from three distinct call sites
    // (sites 15, 23, 39), so both of its executed sites count, and both are
    // non-neutral (the nand loses all three tasks, a knocked-out return
    // does not replicate). The label nops before the body are skipped by the
    // jump, the call sites and everything else run outside any call.
    let dir = config("sub_modular", "u begin Inject modular-subroutine.org\n");
    let mut w = build(&dir, true, &[]);
    run(&mut w, 1);
    let (genome, id) = w.organism_genome(0).unwrap();
    let r = w.subroutine_reuse_of(&genome, id).unwrap().unwrap();
    assert_eq!((r.sites, r.non_neutral), (2, 2));

    // A straight-line organism that has `call` in its set but never calls.
    let dir = config(
        "sub_modular_plain",
        "u begin Inject default-heads-modular.org\n",
    );
    let mut w = build(&dir, true, &[]);
    run(&mut w, 1);
    let (genome, id) = w.organism_genome(0).unwrap();
    let r = w.subroutine_reuse_of(&genome, id).unwrap().unwrap();
    assert_eq!((r.sites, r.non_neutral), (0, 0));

    // The stock set has no call: not defined.
    let dir = config("sub_stock", "u begin Inject default-heads.org\n");
    let mut w = build(&dir, false, &[]);
    run(&mut w, 1);
    let (genome, id) = w.organism_genome(0).unwrap();
    assert_eq!(w.subroutine_reuse_of(&genome, id).unwrap(), None);
}

// ---------------------------------------------------------------------
// The actions
// ---------------------------------------------------------------------

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
        .unwrap()
        .split(|&b| b == b'\n')
        .filter(|l| !is_timestamp(l))
        .map(<[u8]>::to_vec)
        .collect()
}

fn data_files(dir: &Path) -> Vec<String> {
    let mut v: Vec<String> = fs::read_dir(dir.join("data"))
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    v.sort();
    v
}

const BASE_EVENTS: &str = "u begin Inject modular-subroutine.org 210\n\
     u 0:25:end PrintAverageData\n\
     u 0:25:end PrintDominantData\n\
     u 0:25:end PrintCountData\n\
     u 0:25:end PrintTasksData\n\
     u 0:50:end PrintInstructionData\n\
     u 300 SavePopulation\n\
     u 300 Exit\n";

const KNOCKOUT_LINES: &str =
    "u 50:100:end PrintKnockoutMap\nu 0:75:end PrintReuseData\nu 100 PrintKnockoutMap second.dat\n";

/// Mutations on (the stock rates): the run draws from its random stream
/// constantly and the dominant genotype changes.
const SETS: &[(&str, &str)] = &[("WORLD_X", "20"), ("WORLD_Y", "20"), ("RANDOM_SEED", "7")];

fn run_events(name: &str, events: &str, sets: &[(&str, &str)]) -> (PathBuf, String) {
    let dir = config(name, events);
    let cwd = dir.to_string_lossy().into_owned();
    let mut args: Vec<String> = ["avida", "-def", "INST_SET", "instset-heads-modular.cfg"]
        .map(String::from)
        .to_vec();
    for (n, v) in SETS.iter().chain(sets) {
        args.extend(["-set".into(), n.to_string(), v.to_string()]);
    }
    let cmd = process_cmdline(&args, &cwd);
    assert!(cmd.exit.is_none(), "command line: {}", cmd.stderr);
    let (mut w, _): (World, _) = World::new(cmd.config, &cwd, &cmd.defs).expect("World::new");
    let mut out = Vec::new();
    w.run(&mut out).unwrap();
    w.out.flush_all();
    (dir, String::from_utf8_lossy(&out).into_owned())
}

/// Everything but the knockout files is byte-identical (time stamps aside).
fn check_observer(tag: &str, sets: &[(&str, &str)]) -> PathBuf {
    let plain = run_events(&format!("{tag}_plain"), BASE_EVENTS, sets);
    let with = run_events(
        &format!("{tag}_with"),
        &BASE_EVENTS.replace("u 300 Exit", &format!("{KNOCKOUT_LINES}u 300 Exit")),
        sets,
    );
    assert_eq!(plain.1, with.1, "{tag}: status lines");
    let (fa, mut fb) = (data_files(&plain.0), data_files(&with.0));
    fb.retain(|f| !matches!(f.as_str(), "knockout_map.dat" | "reuse.dat" | "second.dat"));
    assert_eq!(fa, fb, "{tag}: file lists");
    assert!(fa.len() >= 6, "{tag}: {fa:?}");
    for f in &fa {
        assert!(
            normalized(&plain.0.join("data").join(f)) == normalized(&with.0.join("data").join(f)),
            "{tag}: {f} differs"
        );
    }
    for f in ["knockout_map.dat", "reuse.dat", "second.dat"] {
        assert!(with.0.join("data").join(f).exists(), "{tag}: {f} missing");
    }
    with.0
}

fn reuse_rows(dir: &Path) -> Vec<Vec<f64>> {
    fs::read_to_string(dir.join("data/reuse.dat"))
        .unwrap()
        .lines()
        .filter(|l| !l.starts_with('#') && !l.trim().is_empty())
        .map(|l| l.split_whitespace().map(|w| w.parse().unwrap()).collect())
        .collect()
}

#[test]
fn observation_only_in_the_exact_mode_and_the_files_are_right() {
    let dir = check_observer("exact", &[]);
    let rows = reuse_rows(&dir);
    // Updates 0, 75, 150, 225, 300.
    assert_eq!(rows.len(), 5, "{rows:?}");
    for (r, u) in rows.iter().zip([0.0, 75.0, 150.0, 225.0, 300.0]) {
        assert_eq!(r.len(), 9);
        assert_eq!(r[0], u);
        assert!(r[2] >= 1.0, "genome length");
    }
    // The header explains the columns and the sentinel.
    let text = fs::read_to_string(dir.join("data/reuse.dat")).unwrap();
    for needle in [
        "#  5: Shared sites",
        "#  6: Reuse index",
        "-1 means not defined",
        "#  8: Non-neutral call sites",
    ] {
        assert!(text.contains(needle), "{needle}: {text}");
    }
    // The map file: one block per firing (updates 50, 150 and 250 here; the
    // second file has update 100), a line per site.
    let map = fs::read_to_string(dir.join("data/knockout_map.dat")).unwrap();
    assert_eq!(map.matches("# Knockout map: update").count(), 3, "{map}");
    assert!(map.contains("# Tasks (the bit string has one character per task"));
    let lines: Vec<&str> = map
        .lines()
        .filter(|l| !l.starts_with('#') && !l.is_empty())
        .collect();
    let first_len: usize = map
        .lines()
        .find(|l| l.starts_with("# Knockout map"))
        .unwrap()
        .split("length ")
        .nth(1)
        .unwrap()
        .split(',')
        .next()
        .unwrap()
        .parse()
        .unwrap();
    assert!(lines.len() >= first_len);
    for l in &lines {
        let w: Vec<&str> = l.split_whitespace().collect();
        assert_eq!(w.len(), 5, "{l}");
        assert!(["lethal", "neutral", "task", "shared", "speed"].contains(&w[2]));
        assert_eq!(w[4].len(), 9, "{l}");
    }
    let second = fs::read_to_string(dir.join("data/second.dat")).unwrap();
    assert!(second.contains("# Knockout map: update 100,"));
}

#[test]
fn observation_only_in_the_parallel_mode_and_thread_count_does_not_matter() {
    let one = check_observer(
        "par1",
        &[("EXECUTION_MODE", "1"), ("EXECUTION_THREADS", "1")],
    );
    let four = check_observer(
        "par4",
        &[("EXECUTION_MODE", "1"), ("EXECUTION_THREADS", "4")],
    );
    for f in ["knockout_map.dat", "reuse.dat", "second.dat"] {
        assert!(
            normalized(&one.join("data").join(f)) == normalized(&four.join("data").join(f)),
            "{f} differs between 1 and 4 threads"
        );
    }
}

#[test]
fn bad_arguments_stop_the_run_at_start_up() {
    let dir = config("badargs", "u 0:1:end PrintKnockoutMap a.dat b.dat\n");
    let cwd = dir.to_string_lossy().into_owned();
    let cmd = process_cmdline(&["avida".to_string()], &cwd);
    let e = World::new(cmd.config, &cwd, &cmd.defs)
        .err()
        .expect("must fail");
    assert!(e.contains("PrintKnockoutMap"), "{e}");
}

/// What one call costs: the hand-written organism padded to about 100, 200
/// and 300 sites with `push`, `pop` pairs (which change nothing) before its
/// first `IO`, so that the pad is executed (a divide needs half the genome
/// executed) and the gestation grows with the length as a real genome's does.
/// `cargo test --release -p avida --test knockout cost -- --ignored --nocapture`
#[test]
#[ignore = "a timing, run explicitly in the release profile"]
fn cost_of_one_call_by_genome_length() {
    let dir = config("cost", "u begin Inject modular-subroutine.org\n");
    let mut w = build(&dir, true, &[]);
    run(&mut w, 1);
    let (genome, id) = w.organism_genome(0).unwrap();
    let set = w.hw.registry().default_inst_set().unwrap();
    let (push, pop) = (set.get_inst("push").unwrap(), set.get_inst("pop").unwrap());
    for target in [57usize, 101, 201, 301] {
        let mut seq = genome.representation().clone();
        let pad = target - 57;
        let at = 6;
        let old = seq.clone();
        seq.resize(target as i32);
        for i in 0..target as i32 {
            seq[i] = if (i as usize) < at {
                old[i]
            } else if (i as usize) < at + pad {
                if (i as usize - at).is_multiple_of(2) {
                    push
                } else {
                    pop
                }
            } else {
                old[i - pad as i32]
            };
        }
        let g = avida_genome::Genome::with_inst_set(genome.hardware_type(), genome.inst_set(), seq);
        let t = std::time::Instant::now();
        let m = w.knockout_map_of(&g, id).unwrap();
        let ms = t.elapsed().as_secs_f64() * 1000.0;
        println!(
            "viable {} fitness {} performed {:?}\nlength {target}: {ms:.1} ms for {} test runs ({:.2} ms each), {} tasks, {}",
            m.base_viable,
            m.base_fitness,
            m.performed,
            target + 1,
            ms / (target + 1) as f64,
            m.performed.len(),
            avida_world::knockout::summary(&m.measures())
        );
    }
}

// ---------------------------------------------------------------------
// PrintReuseSurvey
// ---------------------------------------------------------------------

const SURVEY_LINES: &str = "u 0:100:end PrintReuseSurvey\n";

fn survey_rows(dir: &Path) -> Vec<Vec<f64>> {
    fs::read_to_string(dir.join("data/reuse_survey.dat"))
        .unwrap()
        .lines()
        .filter(|l| !l.starts_with('#') && !l.trim().is_empty())
        .map(|l| l.split_whitespace().map(|w| w.parse().unwrap()).collect())
        .collect()
}

#[test]
fn the_survey_is_an_observer_and_deterministic() {
    let plain = run_events("survey_plain", BASE_EVENTS, &[]);
    let events = BASE_EVENTS.replace("u 300 Exit", &format!("{SURVEY_LINES}u 300 Exit"));
    let a = run_events("survey_a", &events, &[]);
    let b = run_events("survey_b", &events, &[]);
    assert_eq!(plain.1, a.1, "status lines");
    for f in data_files(&plain.0) {
        assert!(
            normalized(&plain.0.join("data").join(&f)) == normalized(&a.0.join("data").join(&f)),
            "{f} differs"
        );
    }
    assert!(
        normalized(&a.0.join("data/reuse_survey.dat"))
            == normalized(&b.0.join("data/reuse_survey.dat")),
        "two runs give different survey files"
    );
    let rows = survey_rows(&a.0);
    // Updates 0, 100, 200, 300, at least one row each, 12 columns.
    assert!(rows.len() >= 4, "{rows:?}");
    for r in &rows {
        assert_eq!(r.len(), 12);
    }
    // At update 0 the one organism's genotype qualifies (3 tasks) and has
    // the subroutine's two sites.
    let first = &rows[0];
    assert_eq!(
        (first[0], first[2], first[3], first[4]),
        (0.0, 1.0, 57.0, 3.0)
    );
    assert_eq!((first[10], first[11]), (2.0, 2.0), "{first:?}");
    let text = fs::read_to_string(a.0.join("data/reuse_survey.dat")).unwrap();
    assert!(text.contains("50 most abundant"), "header names the bound");
}

#[test]
fn the_survey_writes_a_row_of_minus_ones_when_nothing_qualifies() {
    let dir = config(
        "survey_none",
        "u begin Inject default-heads-modular.org\nu 0 PrintReuseSurvey\nu 1 Exit\n",
    );
    let cwd = dir.to_string_lossy().into_owned();
    let args: Vec<String> = ["avida", "-def", "INST_SET", "instset-heads-modular.cfg"]
        .map(String::from)
        .to_vec();
    let cmd = process_cmdline(&args, &cwd);
    let (mut w, _): (World, _) = World::new(cmd.config, &cwd, &cmd.defs).unwrap();
    w.run(&mut Vec::new()).unwrap();
    w.out.flush_all();
    let rows = survey_rows(&dir);
    assert_eq!(
        rows,
        vec![vec![
            0.0, -1.0, -1.0, -1.0, -1.0, -1.0, -1.0, -1.0, -1.0, -1.0, -1.0, -1.0
        ]]
    );
}

#[test]
fn survey_arguments() {
    for (args, ok) in [
        ("", true),
        ("a.dat", true),
        ("a.dat 3", true),
        ("filename=a.dat top=2", true),
        ("a.dat 0", false),
        ("a.dat x", false),
        ("a b c", false),
    ] {
        let dir = config("survey_args", &format!("u 0 PrintReuseSurvey {args}\n"));
        let cwd = dir.to_string_lossy().into_owned();
        let cmd = process_cmdline(&["avida".to_string()], &cwd);
        assert_eq!(
            World::new(cmd.config, &cwd, &cmd.defs).is_ok(),
            ok,
            "{args:?}"
        );
    }
}
