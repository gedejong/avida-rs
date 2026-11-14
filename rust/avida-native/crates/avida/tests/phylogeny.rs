//! `PrintPhylogenyMetrics` and `SavePhylogeny` (extensions beyond the C++
//! program) observe the tracker and nothing else: with them in the event
//! list every other data file of the run is byte-identical to the run
//! without them (time stamps aside), in the exact mode and in the parallel
//! mode on one and four threads; the metrics file itself does not depend
//! on the thread count.

use std::collections::{HashMap, HashSet};
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

const BASE: &str = "u begin InjectAll filename=default-classic.org\n\
    u 0:5:end PrintAverageData\n\
    u 0:5:end PrintDominantData\n\
    u 0:5:end PrintCountData\n\
    u 0:5:end PrintTasksData\n\
    u 0:5:end PrintTimeData\n\
    u 0:10:end PrintDepthHistogram\n\
    u 0:10:end PrintGenotypeAbundanceHistogram\n\
    u 90 SavePopulation\n\
    u 90 Exit\n";
const EXTRA: &str = "u 0:10:end PrintPhylogenyMetrics\n\
    u 50 SavePhylogeny tree50.csv\n\
    u 90 SavePhylogeny tree90.csv\n";

/// Run the world for `events` with `sets`; the run directory.
fn run(name: &str, sets: &[(&str, &str)], events: &str) -> (PathBuf, Vec<u8>) {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR"))
        .join("phylogeny")
        .join(name);
    let _ = fs::remove_dir_all(&dir);
    let src = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../../../avida-core/tests/heads_default_100u/config");
    copy_dir(&src, &dir);
    fs::write(dir.join("events.cfg"), events).unwrap();
    let cwd = dir.to_string_lossy().into_owned();
    let mut args = vec!["avida".to_string()];
    for (n, v) in sets {
        args.extend(["-set".to_string(), n.to_string(), v.to_string()]);
    }
    let cmd = process_cmdline(&args, &cwd);
    assert!(cmd.exit.is_none(), "command line: {}", cmd.stderr);
    let (mut w, _) = World::new(cmd.config, &cwd, &cmd.defs).unwrap();
    let mut stepper = Stepper::new(&w);
    let mut out = Vec::new();
    while stepper.begin_update(&mut w, &mut out).unwrap() {
        while stepper.step_slice(&mut w).unwrap() {}
        stepper.end_update(&mut w, &mut out).unwrap();
    }
    w.out.flush_all();
    (dir, out)
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

fn data_files(dir: &Path) -> Vec<String> {
    let mut v: Vec<String> = fs::read_dir(dir.join("data"))
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    v.sort();
    v
}

const OURS: &[&str] = &["phylogeny.dat", "tree50.csv", "tree90.csv"];

/// The run without the actions and the run with them agree on everything
/// but the files the actions write.
fn assert_unperturbed(name: &str, sets: &[(&str, &str)]) -> PathBuf {
    let (da, oa) = run(&format!("{name}_plain"), sets, BASE);
    let events = BASE.replace(
        "u 90 SavePopulation",
        &format!("{EXTRA}u 90 SavePopulation"),
    );
    let (db, ob) = run(&format!("{name}_with"), sets, &events);
    assert_eq!(oa, ob, "{name}: status lines");
    let fa = data_files(&da);
    let fb: Vec<String> = data_files(&db)
        .into_iter()
        .filter(|f| !OURS.contains(&f.as_str()))
        .collect();
    assert_eq!(fa, fb, "{name}: file lists");
    assert!(fa.len() >= 8);
    for f in &fa {
        assert_eq!(
            normalized(&da.join("data").join(f)),
            normalized(&db.join("data").join(f)),
            "{name}: {f} differs"
        );
    }
    for f in OURS {
        assert!(db.join("data").join(f).exists(), "{name}: {f} missing");
    }
    db
}

fn rows(dir: &Path) -> Vec<Vec<String>> {
    fs::read_to_string(dir.join("data/phylogeny.dat"))
        .unwrap()
        .lines()
        .filter(|l| !l.starts_with('#') && !l.trim().is_empty())
        .map(|l| l.split_whitespace().map(String::from).collect())
        .collect()
}

#[test]
fn exact_mode_is_unperturbed_and_deterministic() {
    let d1 = assert_unperturbed("exact", &[]);
    let r = rows(&d1);
    assert_eq!(r.len(), 10, "updates 0, 10, ..., 90");
    assert!(r.iter().all(|row| row.len() == 19));
    // Update 0: one genotype per cell... InjectAll starts one genotype.
    assert_eq!(r[0][1], "1");
    // The first row counts no MRCA change; the count never decreases.
    assert_eq!(r[0][5], "0");
    let changes: Vec<u64> = r.iter().map(|row| row[5].parse().unwrap()).collect();
    assert!(changes.windows(2).all(|w| w[0] <= w[1]));
    // The genotype count grows by mutation.
    assert!(r.last().unwrap()[1].parse::<u32>().unwrap() > 5, "{r:?}");
    // The same seed gives the same file.
    let (d2, _) = run(
        "exact_again",
        &[],
        &BASE.replace(
            "u 90 SavePopulation",
            &format!("{EXTRA}u 90 SavePopulation"),
        ),
    );
    assert_eq!(
        normalized(&d1.join("data/phylogeny.dat")),
        normalized(&d2.join("data/phylogeny.dat"))
    );

    // The CSV is a tree: unique ids, every ancestor present, one root, and
    // its living taxa are the metrics row's.
    let csv = fs::read_to_string(d1.join("data/tree50.csv")).unwrap();
    let mut lines = csv.lines();
    assert_eq!(
        lines.next().unwrap(),
        "id,ancestor_list,origin_time,destruction_time,num_orgs,depth"
    );
    let mut ids = HashSet::new();
    let mut anc: HashMap<i32, String> = HashMap::new();
    let mut living = 0;
    for l in lines {
        let f: Vec<&str> = l.split(',').collect();
        assert_eq!(f.len(), 6, "{l}");
        let id: i32 = f[0].parse().unwrap();
        assert!(ids.insert(id));
        anc.insert(id, f[1].to_string());
        let orgs: i32 = f[4].parse().unwrap();
        if orgs > 0 {
            living += 1;
            assert_eq!(f[3], "inf");
        }
    }
    let roots = anc.values().filter(|a| *a == "[none]").count();
    assert_eq!(roots, 1);
    for a in anc.values().filter(|a| *a != "[none]") {
        let p: i32 = a.trim_matches(|c| c == '[' || c == ']').parse().unwrap();
        assert!(ids.contains(&p), "ancestor {p} missing");
    }
    assert_eq!(living.to_string(), r[5][1], "update 50 row");
    // The MRCA id of the update-50 row is in the tree.
    let mrca: i32 = r[5][3].parse().unwrap();
    assert!(ids.contains(&mrca));
}

#[test]
fn parallel_mode_is_unperturbed_and_independent_of_the_thread_count() {
    let one = assert_unperturbed(
        "par1",
        &[("EXECUTION_MODE", "1"), ("EXECUTION_THREADS", "1")],
    );
    let four = assert_unperturbed(
        "par4",
        &[("EXECUTION_MODE", "1"), ("EXECUTION_THREADS", "4")],
    );
    for f in OURS {
        assert_eq!(
            normalized(&one.join("data").join(f)),
            normalized(&four.join("data").join(f)),
            "{f} depends on the thread count"
        );
    }
    assert_eq!(rows(&one).len(), 10);
}
