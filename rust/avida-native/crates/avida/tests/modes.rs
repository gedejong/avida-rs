//! `PrintMODESData` (world/src/modes.rs): the skeleton of an organism that
//! does a task paid from a spatial resource, observation only (a run with the
//! action writes the same other files as one without, in the exact mode and
//! in the parallel mode on 1 and 4 threads), determinism, and the file
//! format.

use std::fs;
use std::path::{Path, PathBuf};

use avida_config::cmdline::process_cmdline;
use avida_genome::Genome;
use avida_world::modes::TestLevels;
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

/// Two spatial resources over a whole 20 x 20 world, each paying one task
/// from a fresh cell at 0.05 in and 0.05 out per cell and update (an unused
/// cell settles at 1 unit), as the large experiments do.
const ENVIRONMENT: &str = "\
RESOURCE resNOT:geometry=grid:initial=0:inflow=20:outflow=0.05:inflowx1=0:inflowx2=19:inflowy1=0:inflowy2=19:outflowx1=0:outflowx2=19:outflowy1=0:outflowy2=19:xdiffuse=0.2:ydiffuse=0.2:xgravity=0:ygravity=0
RESOURCE resNAND:geometry=grid:initial=0:inflow=20:outflow=0.05:inflowx1=0:inflowx2=19:inflowy1=0:inflowy2=19:outflowx1=0:outflowx2=19:outflowy1=0:outflowy2=19:xdiffuse=0.2:ydiffuse=0.2:xgravity=0:ygravity=0
REACTION NOT  not   process:resource=resNOT:value=1.0:type=pow:frac=0.5:max=1.0  requisite:max_count=1
REACTION NAND nand  process:resource=resNAND:value=1.0:type=pow:frac=0.5:max=1.0  requisite:max_count=1
";

fn fresh_dir(name: &str, events: &str) -> PathBuf {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR"))
        .join("modes")
        .join(name);
    let _ = fs::remove_dir_all(&dir);
    let src = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../../avida-core/support/config");
    copy_dir(&src, &dir);
    avida_config::convert::replace_section(&dir, "avida.yaml", "environment", ENVIRONMENT).unwrap();
    avida_config::convert::replace_section(&dir, "avida.yaml", "events", events).unwrap();
    dir
}

fn build_world(dir: &Path, sets: &[(&str, &str)]) -> World {
    let cwd = dir.to_string_lossy().into_owned();
    let mut args = vec!["avida".to_string()];
    for (n, v) in sets {
        args.extend(["-set".to_string(), n.to_string(), v.to_string()]);
    }
    let cmd = process_cmdline(&args, &cwd);
    assert!(cmd.exit.is_none(), "command line: {}", cmd.stderr);
    World::new(cmd.config, &cwd)
        .unwrap_or_else(|e| panic!("World::new: {e}"))
        .0
}

const SMALL: &[(&str, &str)] = &[
    ("world.x", "20"),
    ("world.y", "20"),
    ("general.random_seed", "7"),
];

/// The instructions of an `.org` file of the stock configuration as a genome
/// of `w`'s default instruction set, with the symbols `insert` (an
/// instruction name each) put after the first instruction.
fn org_genome(w: &World, org: &str, insert: &[&str]) -> Genome {
    let set = w.hw.registry().default_inst_set().unwrap();
    let text = fs::read_to_string(
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../../../avida-core/support/config")
            .join(org),
    )
    .unwrap();
    let mut names: Vec<String> = text
        .lines()
        .map(|l| l.split('#').next().unwrap().trim())
        .filter(|l| !l.is_empty())
        .map(str::to_string)
        .collect();
    for (i, n) in insert.iter().enumerate() {
        names.insert(1 + i, (*n).to_string());
    }
    let symbols: String = names
        .iter()
        .map(|n| set.get_inst(n).unwrap().symbol())
        .collect();
    Genome::from_string(&format!("0,{},{symbols}", set.inst_set_name()))
}

/// The skeleton of an organism that does NOT is longer than that of its
/// replication machinery alone when the test CPU runs at the levels of an
/// unexploited cell, and is not under the default test environment, where
/// `initial=0` pays nothing from the limited resource.
#[test]
fn a_rewarded_task_lengthens_the_skeleton_in_a_spatial_resource_world() {
    let dir = fresh_dir("skeleton", "u 10 Exit\n");
    let mut w = build_world(&dir, SMALL);
    let plain = org_genome(&w, "default-heads.org", &[]);
    // IO (input A), push, pop into CX, nand (A nand A), IO (output NOT A).
    let tasker = org_genome(
        &w,
        "default-heads.org",
        &["IO", "push", "pop", "nop-C", "nand", "IO"],
    );
    assert_eq!(plain.representation().size(), 100);
    assert_eq!(tasker.representation().size(), 106);

    let hw = w.modes_private_hw();
    let constant = w.modes_levels();
    let initial = TestLevels::Initial;
    let plain_c = w.modes_skeleton(&hw, &constant, &plain).unwrap();
    let tasker_c = w.modes_skeleton(&hw, &constant, &tasker).unwrap();
    let plain_i = w.modes_skeleton(&hw, &initial, &plain).unwrap();
    let tasker_i = w.modes_skeleton(&hw, &initial, &tasker).unwrap();
    eprintln!(
        "skeleton lengths: plain {} / {} (constant / initial), NOT organism {} / {}\n{}\n{}\n{}",
        plain_c.1, plain_i.1, tasker_c.1, tasker_i.1, plain_c.0, tasker_c.0, tasker_i.0
    );
    // Without a task the levels do not matter.
    assert_eq!(plain_c, plain_i);
    // The default test environment is blind to the task: the instructions
    // that only make it (IO, push, nand, IO) are not informative there. (The
    // pop and its nop-C overwrite CX, which the replication code reads, so
    // they stay.)
    assert!(
        !tasker_i.0.contains('y') && !tasker_i.0.contains('u') && !tasker_i.0.contains('o'),
        "{}",
        tasker_i.0
    );
    assert!(
        tasker_i.1 <= plain_i.1 + 2,
        "{} vs {}",
        tasker_i.1,
        plain_i.1
    );
    // At the unexploited levels the five instructions that make the task
    // (IO, push, pop with its nop-C, nand, IO) are informative.
    assert!(
        tasker_c.1 >= plain_c.1 + 5,
        "NOT organism {} vs plain {}",
        tasker_c.1,
        plain_c.1
    );
    assert!(tasker_c.0.contains('y') && tasker_c.0.contains('u') && tasker_c.0.contains('o'));
}

/// The run: a small world with two task resources.
fn events(extra: &str, updates: u32) -> String {
    format!(
        "u begin Inject default-heads.org 210\n\
         u 0:50:end PrintAverageData\n\
         u 0:50:end PrintDominantData\n\
         u 0:50:end PrintCountData\n\
         u 0:50:end PrintTasksData\n\
         u 0:50:end PrintResourceData\n\
         u 0:100:end PrintInstructionData\n\
         u {updates} SavePopulation\n\
         {extra}\
         u {updates} Exit\n"
    )
}

const MODES_LINES: &str =
    "u 0:1:end PrintMODESData modes.dat 3 6\nu 0:1:end PrintMODESData modes_any.dat 3 3 any\n";

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

/// A run of `events` in a fresh directory; the directory and the status
/// lines.
fn run(name: &str, events: &str, sets: &[(&str, &str)]) -> (PathBuf, String) {
    let dir = fresh_dir(name, events);
    let mut w = build_world(&dir, sets);
    let mut out = Vec::new();
    w.run(&mut out).unwrap();
    w.out.flush_all();
    (dir, String::from_utf8_lossy(&out).into_owned())
}

fn modes_files(dir: &Path) -> Vec<PathBuf> {
    let mut all = Vec::new();
    files_under(&dir.join("data"), dir, &mut all);
    all.into_iter()
        .filter(|f| {
            f.file_name()
                .unwrap()
                .to_string_lossy()
                .starts_with("modes")
        })
        .collect()
}

/// Everything but the MODES files is byte-identical (time stamps aside).
fn assert_unperturbed(plain: &(PathBuf, String), with: &(PathBuf, String), what: &str) {
    assert_eq!(plain.1, with.1, "{what}: status lines");
    let (mut fa, mut fb) = (Vec::new(), Vec::new());
    files_under(&plain.0.join("data"), &plain.0, &mut fa);
    files_under(&with.0.join("data"), &with.0, &mut fb);
    fb.retain(|f| {
        !f.file_name()
            .unwrap()
            .to_string_lossy()
            .starts_with("modes")
    });
    assert_eq!(fa, fb, "{what}: file lists");
    assert!(fa.len() >= 7, "{what}: {fa:?}");
    for f in &fa {
        assert!(
            normalized(&plain.0.join(f)) == normalized(&with.0.join(f)),
            "{what}: {} differs",
            f.display()
        );
    }
}

const UPDATES: u32 = 700;

fn check_mode(tag: &str, sets: &[(&str, &str)]) -> PathBuf {
    let mut all = SMALL.to_vec();
    all.extend_from_slice(sets);
    let plain = run(&format!("{tag}_plain"), &events("", UPDATES), &all);
    let with = run(&format!("{tag}_with"), &events(MODES_LINES, UPDATES), &all);
    assert_unperturbed(&plain, &with, tag);
    let modes = modes_files(&with.0);
    assert_eq!(modes.len(), 2, "{tag}: {modes:?}");
    with.0
}

fn rows(path: &Path) -> Vec<Vec<f64>> {
    fs::read_to_string(path)
        .unwrap()
        .lines()
        .filter(|l| !l.starts_with('#') && !l.trim().is_empty())
        .map(|l| l.split_whitespace().map(|w| w.parse().unwrap()).collect())
        .collect()
}

#[test]
fn observation_only_and_deterministic_in_the_exact_mode() {
    let dir = check_mode("exact", &[]);
    let file = dir.join("data/modes.dat");
    let r = rows(&file);
    assert!(
        !r.is_empty(),
        "no rows: {}",
        fs::read_to_string(&file).unwrap()
    );
    for row in &r {
        assert_eq!(row.len(), 11);
    }
    // Updates strictly increase, the snapshot is older, counts are sane.
    for w in r.windows(2) {
        assert!(w[1][0] > w[0][0]);
    }
    for row in &r {
        assert!(row[2] < row[0]);
        assert!(row[7] >= 1.0 && row[8] >= 1.0 && row[8] <= row[7]);
        assert!(row[9] >= row[7] || row[9] >= 1.0);
        assert!(row[5] >= 1.0, "complexity of a replicator is positive");
    }
    // Cumulative novelty is the running sum of novelty.
    let mut sum = 0.0;
    for row in &r {
        sum += row[4];
        assert_eq!(row[10], sum);
    }
    // The first row has change = novelty = number of skeletons.
    assert_eq!(r[0][3], r[0][8]);
    assert_eq!(r[0][4], r[0][8]);
    // Header: parameters and conventions.
    let text = fs::read_to_string(&file).unwrap();
    for needle in [
        "MODES",
        "resolution 3 generations",
        "t = 6 generations",
        "positions not kept",
        "unexploited cell",
        "# 11: Cumulative novelty",
    ] {
        assert!(text.contains(needle), "header lacks {needle:?}:\n{text}");
    }
    // The file with the paper's filter has at least as many persistent
    // genotypes at the same samples (t = 3 is shorter, so only compare shape).
    assert!(!rows(&dir.join("data/modes_any.dat")).is_empty());

    // The same seed again gives the same file.
    let again = check_mode("exact2", &[]);
    for f in ["modes.dat", "modes_any.dat"] {
        assert!(
            normalized(&dir.join("data").join(f)) == normalized(&again.join("data").join(f)),
            "{f} differs between two runs"
        );
    }
}

#[test]
fn observation_only_in_the_parallel_mode_on_one_and_four_threads() {
    let one = check_mode(
        "par1",
        &[("execution.mode", "1"), ("execution.threads", "1")],
    );
    let four = check_mode(
        "par4",
        &[("execution.mode", "1"), ("execution.threads", "4")],
    );
    for f in ["modes.dat", "modes_any.dat"] {
        assert!(!rows(&one.join("data").join(f)).is_empty(), "{f}");
        assert!(
            normalized(&one.join("data").join(f)) == normalized(&four.join("data").join(f)),
            "{f} differs between 1 and 4 threads"
        );
    }
}

#[test]
fn bad_arguments_stop_the_run_at_start_up() {
    for bad in [
        "u 0:1:end PrintMODESData modes.dat 100 250\n",
        "u 0:1:end PrintMODESData modes.dat\n",
    ] {
        let dir = fresh_dir("badargs", bad);
        let cwd = dir.to_string_lossy().into_owned();
        let cmd = process_cmdline(&["avida".to_string()], &cwd);
        let e = World::new(cmd.config, &cwd).err().expect("must fail");
        assert!(e.contains("PrintMODESData"), "{e}");
    }
}
