//! `DROP_EXTINCT_GENOTYPES 1` (a port-only setting): a genotype leaves the
//! systematics when its last organism dies, instead of staying as a
//! historic genotype while some descendant names it as a parent. Nothing an
//! organism computes may change, so the population data files are
//! byte-identical to the run without the setting; the historic list stays
//! small; actions that need the ancestry stop with `unsupported:`; and a
//! saved population loads again.

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

/// A sexual 20x20 world with mutations, so that genotypes die out.
const SETS: &[(&str, &str)] = &[
    ("world.x", "20"),
    ("world.y", "20"),
    ("mutation.copy_prob", "0.0075"),
    ("mutation.divide_ins_prob", "0.05"),
    ("mutation.divide_del_prob", "0.05"),
];

const UPDATES: i32 = 300;

fn events(extra: &str) -> String {
    format!(
        "u begin Inject organism.50.sex\n\
         u 0:10:end PrintAverageData\n\
         u 0:10:end PrintDominantData\n\
         u 0:10:end PrintCountData\n\
         u 0:10:end PrintTasksData\n\
         u 0:10:end PrintTimeData\n\
         u 0:10:end PrintResourceData\n\
         {extra}\
         u {UPDATES} SavePopulation\n\
         u {UPDATES} Exit\n"
    )
}

fn setup(name: &str, events: &str) -> PathBuf {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR"))
        .join("drop_extinct")
        .join(name);
    let _ = fs::remove_dir_all(&dir);
    let src = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../../avida-core/tests/sex/config");
    copy_dir(&src, &dir);
    avida_config::convert::replace_section(&dir, "avida.yaml", "events", events).unwrap();
    dir
}

fn make_world(dir: &Path, sets: &[(&str, &str)]) -> Result<World, String> {
    let cwd = dir.to_string_lossy().into_owned();
    let mut args = vec!["avida".to_string()];
    for (n, v) in SETS.iter().chain(sets) {
        args.extend(["-set".to_string(), n.to_string(), v.to_string()]);
    }
    let cmd = process_cmdline(&args, &cwd);
    assert!(cmd.exit.is_none(), "command line: {}", cmd.stderr);
    World::new(cmd.config, &cwd).map(|(w, _)| w)
}

/// Runs to the end; the world and the largest historic list seen.
fn run_world(dir: &Path, sets: &[(&str, &str)]) -> (World, usize) {
    let mut w = make_world(dir, sets).unwrap();
    let mut stepper = Stepper::new(&w);
    let mut out = Vec::new();
    let mut max_historic = 0;
    while stepper.begin_update(&mut w, &mut out).unwrap() {
        while stepper.step_slice(&mut w).unwrap() {}
        stepper.end_update(&mut w, &mut out).unwrap();
        max_historic = max_historic.max(w.systematics.genotypes().unwrap().historic().len());
    }
    w.out.flush_all();
    (w, max_historic)
}

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

const FILES: &[&str] = &[
    "average.dat",
    "count.dat",
    "tasks.dat",
    "dominant.dat",
    "time.dat",
    "resource.dat",
];

/// The population data of the two runs agree on every file.
fn assert_same_population(off: &Path, on: &Path) {
    for f in FILES {
        let a = normalized(&off.join("data").join(f));
        let b = normalized(&on.join("data").join(f));
        assert!(a.len() > 10, "{f} is empty");
        assert_eq!(a, b, "{f} differs");
    }
}

fn check_identity(name: &str, extra_sets: &[(&str, &str)]) {
    let off = setup(&format!("{name}_off"), &events(""));
    let on = setup(&format!("{name}_on"), &events(""));
    let (w_off, historic_off) = run_world(&off, extra_sets);
    let mut sets_on = extra_sets.to_vec();
    sets_on.push(("genealogy.drop_extinct_genotypes", "1"));
    let (w_on, historic_on) = run_world(&on, &sets_on);
    assert_same_population(&off, &on);
    // With sexual reproduction the default keeps many extinct ancestors.
    assert!(
        historic_off > 20,
        "{name}: the default kept only {historic_off} historic genotypes"
    );
    assert_eq!(
        historic_on, 0,
        "{name}: historic genotypes with the setting"
    );
    let living = |w: &World| w.systematics.genotypes().unwrap().num_genotypes_alive();
    assert!(living(&w_on) < living(&w_off));
}

#[test]
fn same_population_exact_mode() {
    check_identity("exact", &[]);
}

#[test]
fn same_population_parallel_mode() {
    check_identity(
        "parallel",
        &[("execution.mode", "1"), ("execution.threads", "2")],
    );
}

#[test]
fn ancestry_actions_stop_at_start_up() {
    for (action, args) in [
        ("PrintMODESData", "modes.dat 10 50"),
        ("PrintPhylogenyMetrics", ""),
        ("SavePhylogeny", ""),
    ] {
        let dir = setup(
            &format!("blocked_{action}"),
            &events(&format!("u 0:10:end {action} {args}\n")),
        );
        // Allowed without the setting.
        assert!(
            make_world(&dir, &[]).is_ok(),
            "{action} without the setting"
        );
        let Err(msg) = make_world(&dir, &[("genealogy.drop_extinct_genotypes", "1")]) else {
            panic!("{action} was accepted with the setting");
        };
        assert!(
            msg.contains(&format!(
                "unsupported: {action} with DROP_EXTINCT_GENOTYPES=1"
            )),
            "{action}: {msg}"
        );
    }
}

#[test]
fn save_and_load() {
    let dir = setup("save", &events(""));
    let sets = [("genealogy.drop_extinct_genotypes", "1")];
    let (w, _) = run_world(&dir, &sets);
    let alive = w.pop.num_organisms;
    assert!(alive > 20);
    let spop = dir.join("data").join(format!("detail-{UPDATES}.spop"));
    let text = fs::read_to_string(&spop).unwrap();
    // One row per living genotype: no historic rows; parents are kept as ids.
    let rows: Vec<&str> = text
        .lines()
        .filter(|l| !l.starts_with('#') && !l.is_empty())
        .collect();
    assert_eq!(
        rows.len(),
        w.systematics.genotypes().unwrap().num_genotypes_alive()
    );
    assert!(rows
        .iter()
        .any(|r| r.split(' ').nth(3).is_some_and(|p| p.contains(','))));

    // Load it with the setting on, and (a file without ancestors) the same file's
    // organisms come back.
    let dir2 = setup(
        "load",
        &format!(
            "u begin LoadPopulation {}\nu 0 PrintCountData\nu 2 Exit\n",
            spop.display()
        ),
    );
    let mut w2 = make_world(&dir2, &sets).unwrap();
    let mut stepper = Stepper::new(&w2);
    let mut out = Vec::new();
    assert!(stepper.begin_update(&mut w2, &mut out).unwrap());
    assert_eq!(w2.pop.num_organisms, alive);
    assert_eq!(
        w2.systematics.genotypes().unwrap().num_genotypes_alive(),
        rows.len()
    );
    loop {
        while stepper.step_slice(&mut w2).unwrap() {}
        stepper.end_update(&mut w2, &mut out).unwrap();
        if !stepper.begin_update(&mut w2, &mut out).unwrap() {
            break;
        }
    }
    assert!(w2.pop.num_organisms > 0);
}

#[test]
fn load_a_full_save_with_the_setting_on() {
    // A save with historic rows (written by the default) loads with the
    // setting on, and the ancestors nothing refers to are dropped.
    let dir = setup("full_save", &events(""));
    run_world(&dir, &[]);
    let spop = dir.join("data").join(format!("detail-{UPDATES}.spop"));
    let dir2 = setup(
        "full_load",
        &format!("u begin LoadPopulation {}\nu 2 Exit\n", spop.display()),
    );
    let mut w = make_world(&dir2, &[("genealogy.drop_extinct_genotypes", "1")]).unwrap();
    let mut stepper = Stepper::new(&w);
    let mut out = Vec::new();
    while stepper.begin_update(&mut w, &mut out).unwrap() {
        while stepper.step_slice(&mut w).unwrap() {}
        stepper.end_update(&mut w, &mut out).unwrap();
    }
    assert!(w.pop.num_organisms > 20);
    assert_eq!(w.systematics.genotypes().unwrap().historic().len(), 0);
}
