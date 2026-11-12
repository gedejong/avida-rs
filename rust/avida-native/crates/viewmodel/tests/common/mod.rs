//! Setting up and driving a world from a consistency-test directory, the way
//! the `avida` binary does (`crates/avida/src/main.rs`), without touching
//! the process's working directory.

#![allow(dead_code)]

use std::fs;
use std::path::{Path, PathBuf};

use avida_config::cmdline::process_cmdline;
use avida_world::world::World;

/// `avida-core/tests`
pub fn tests_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../../avida-core/tests")
}

pub fn copy_dir(from: &Path, to: &Path) {
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

/// A fresh copy of `<test>/config` under the cargo test scratch directory.
pub fn fresh_config(test: &str, name: &str) -> PathBuf {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR"))
        .join("viewmodel")
        .join(name);
    let _ = fs::remove_dir_all(&dir);
    copy_dir(&tests_root().join(test).join("config"), &dir);
    dir
}

/// Appends lines to the copy's `events.cfg`.
pub fn append_events(dir: &Path, lines: &str) {
    let path = dir.join("events.cfg");
    let mut text = fs::read_to_string(&path).unwrap();
    if !text.ends_with('\n') {
        text.push('\n');
    }
    text.push_str(lines);
    fs::write(&path, text).unwrap();
}

/// `primitive.cc` up to the run: command line (no arguments), config,
/// `cWorld::Initialize`.
pub fn build_world(dir: &Path) -> World {
    let cwd = dir.to_string_lossy().into_owned();
    let cmd = process_cmdline(&["avida".to_string()], &cwd);
    assert!(cmd.exit.is_none(), "command line: {}", cmd.stderr);
    let (world, _report) =
        World::new(cmd.config, &cwd, &cmd.defs).unwrap_or_else(|e| panic!("World::new: {e}"));
    world
}

/// Runs to `last_update`, calling `at_stop` each time the run loop returns
/// on an `Exit` event (and at the end).
///
/// `World::run` returns from the top of its loop, after the events of the
/// update just finished and before the next update starts, so `Exit`
/// events added to the copy's `events.cfg` give stops between updates.
/// Clearing `done` and calling `run` again resumes the loop: the events
/// that already fired at this update have moved on to their next trigger,
/// so the extra `process_events` call fires nothing (an event with interval
/// 0 would fire twice; the test configurations have none). A public
/// "step one update" on `World` would make this unnecessary.
pub fn run_with_stops(
    world: &mut World,
    last_update: i32,
    stdout: &mut Vec<u8>,
    mut at_stop: impl FnMut(&World),
) {
    loop {
        world.run(stdout).unwrap_or_else(|e| panic!("run: {e}"));
        at_stop(world);
        if world.stats.update() >= last_update || world.pop.num_organisms == 0 {
            break;
        }
        world.done = false;
    }
    world.out.flush_all();
}

/// `# Tue Nov 10 18:24:44 2026`
pub fn is_timestamp(line: &[u8]) -> bool {
    line.len() > 10
        && line.starts_with(b"# ")
        && line[2].is_ascii_uppercase()
        && line[5] == b' '
        && line[6].is_ascii_uppercase()
        && line[9] == b' '
}

/// The file's lines without timestamp comments.
pub fn normalized(path: &Path) -> Vec<Vec<u8>> {
    fs::read(path)
        .unwrap_or_else(|e| panic!("{}: {e}", path.display()))
        .split(|&b| b == b'\n')
        .filter(|l| !is_timestamp(l))
        .map(<[u8]>::to_vec)
        .collect()
}

/// Every file under `dir`, relative to `base`, sorted.
pub fn files_under(dir: &Path, base: &Path, out: &mut Vec<PathBuf>) {
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

/// Names of the instructions of the gene-expression test organisms.
const GENE_BCR_INSTS: &[&str] = &[
    "nop-A",
    "nop-B",
    "nop-C",
    "nop-D",
    "promoter",
    "terminator",
    "inc",
    "dec",
    "one",
    "push",
    "pop",
    "swap-stk",
    "add",
    "set-memory",
    "h-write",
    "wait-cond-gtr",
    "yield",
];
const GENE_GP8_INSTS: &[&str] = &[
    "nop-A",
    "nop-B",
    "nop-C",
    "nop-D",
    "promoter",
    "terminator",
    "inc",
    "dec",
    "push",
    "pop",
    "swap-stk",
    "add",
    "set-memory",
    "h-write",
    "wait-cond-gtr",
    "yield",
];

/// A copy of the `avatars` test configuration run on BCR (`hw_type` 5) or
/// GP8 (4) hardware (the `pp5_bcr_u2` / `pp5_gp8_u0` action cases supply the
/// configuration), with one organism of two genes in cell 12: gene 0 (label
/// `AB`) pushes, swaps stacks and starts a second memory space; gene 1
/// (label `BB`) waits for a register to exceed another thread's.
pub fn gene_config(hw_type: i32, name: &str) -> PathBuf {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR"))
        .join("gene_hardware")
        .join(name);
    let _ = fs::remove_dir_all(&dir);
    copy_dir(&tests_root().join("avatars").join("config"), &dir);
    let case = if hw_type == 5 {
        "pp5_bcr_u2"
    } else {
        "pp5_gp8_u0"
    };
    let extra = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../avida/tests/actions")
        .join(case)
        .join("extra");
    fs::copy(extra.join("environment.cfg"), dir.join("environment.cfg")).unwrap();
    // The recorded configuration is older than the code: no version warning.
    let cfg = fs::read_to_string(extra.join("avida.cfg")).unwrap();
    fs::write(
        dir.join("avida.cfg"),
        cfg.replace("VERSION_ID 2.11.0", "VERSION_ID 2.14.0"),
    )
    .unwrap();
    let insts = if hw_type == 5 {
        GENE_BCR_INSTS
    } else {
        GENE_GP8_INSTS
    };
    let mut set = format!("INSTSET instset:hw_type={hw_type}\n");
    for i in insts {
        set.push_str(&format!("INST {i}\n"));
    }
    fs::write(dir.join("instset.cfg"), set).unwrap();
    let one = if hw_type == 5 { "one" } else { "inc" };
    let genome = [
        "promoter",
        "nop-A",
        "nop-B",
        one,
        "push",
        "inc",
        "swap-stk",
        "set-memory",
        "nop-C",
        one,
        "dec",
        "inc",
        "pop",
        "terminator",
        "promoter",
        "nop-B",
        "nop-B",
        one,
        "inc",
        "wait-cond-gtr",
        "yield",
        "inc",
        "terminator",
    ];
    fs::write(dir.join("genes.org"), genome.join("\n") + "\n").unwrap();
    fs::write(
        dir.join("events.cfg"),
        "u begin Inject genes.org 12\nu 100000 Exit\n",
    )
    .unwrap();
    dir
}
