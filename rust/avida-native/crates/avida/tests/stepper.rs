//! A run driven through `Stepper` with pauses at arbitrary slice
//! boundaries, or between the instructions of speculative slices, writes
//! the same files and status lines as `World::run`.

use std::fs;
use std::path::{Path, PathBuf};

use avida_config::cmdline::process_cmdline;
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
        .join("stepper")
        .join(name);
    let _ = fs::remove_dir_all(&dir);
    let src = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../../../avida-core/tests")
        .join(test)
        .join("config");
    copy_dir(&src, &dir);
    dir
}

fn build_world(dir: &Path) -> World {
    let cwd = dir.to_string_lossy().into_owned();
    let cmd = process_cmdline(&["avida".to_string()], &cwd);
    assert!(cmd.exit.is_none(), "command line: {}", cmd.stderr);
    World::new(cmd.config, &cwd, &cmd.defs)
        .unwrap_or_else(|e| panic!("World::new: {e}"))
        .0
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

/// `World::run` in a fresh copy of the test's configuration: the directory
/// and the status lines.
fn plain_run(test: &str, name: &str) -> (PathBuf, Vec<u8>) {
    let dir = fresh_config(test, &format!("{test}_{name}"));
    let mut w = build_world(&dir);
    let mut out = Vec::new();
    w.run(&mut out).unwrap();
    w.out.flush_all();
    (dir, out)
}

/// The two runs printed the same status lines and wrote the same files
/// (time stamps aside).
fn assert_same_run(test: &str, (dir_a, out_a): (&Path, &[u8]), (dir_b, out_b): (&Path, &[u8])) {
    assert_eq!(
        String::from_utf8_lossy(out_a),
        String::from_utf8_lossy(out_b),
        "{test}: status lines"
    );
    let (mut fa, mut fb) = (Vec::new(), Vec::new());
    files_under(&dir_a.join("data"), dir_a, &mut fa);
    files_under(&dir_b.join("data"), dir_b, &mut fb);
    assert_eq!(fa, fb, "{test}: file lists");
    assert!(!fa.is_empty());
    for f in &fa {
        assert!(
            normalized(&dir_a.join(f)) == normalized(&dir_b.join(f)),
            "{test}: {} differs",
            f.display()
        );
    }
}

/// `(id, lifetime CPU cycles)` of the organism in `cell`.
fn org_cycles(w: &World, cell: i32) -> Option<(i32, u64)> {
    let org = w.pop.orgs.get(usize::try_from(cell).ok()?)?.as_deref()?;
    Some((org.state.id, org.state.phenotype.lifetime_cpu_cycles))
}

/// A run driven through `Stepper::step_watched` with a watch that moves
/// between organisms and pauses inside their speculative time slices (and
/// breaks off there for `step_slice` and `end_update`) writes the same
/// files and status lines as `World::run`; each cut instruction runs
/// exactly one instruction of the watched organism.
fn check_watched(test: &str) {
    let (dir_a, out_a) = plain_run(test, "run_w");

    let dir_b = fresh_config(test, &format!("{test}_watched"));
    let mut b = build_world(&dir_b);
    let mut out_b = Vec::new();
    let mut stepper = Stepper::new(&b);
    assert!(stepper.speculative(), "{test}: needs SPECULATIVE on");
    let mut seed: u32 = 0x85eb_ca6b;
    let mut next = |n: u32| {
        seed = seed.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
        (seed >> 16) % n
    };
    let (mut cut, mut closed_cut, mut broken_off) = (0u64, 0u64, 0u64);
    let mut watch = -1;
    while stepper.begin_update(&mut b, &mut out_b).unwrap() {
        loop {
            if watch < 0 || next(64) == 0 {
                // An occupied cell, or none at all now and then.
                let live: Vec<i32> = (0..b.pop.orgs.len() as i32)
                    .filter(|&c| b.pop.orgs[c as usize].is_some())
                    .collect();
                watch = if live.is_empty() || next(8) == 0 {
                    -1
                } else {
                    live[next(live.len() as u32) as usize]
                };
            }
            let before = org_cycles(&b, watch);
            let progress = stepper.slice_progress().0;
            match stepper.step_watched(&mut b, watch).unwrap() {
                WatchedStep::UpdateDone => break,
                WatchedStep::Slice => assert!(stepper.open_slice().is_none()),
                WatchedStep::Instruction { closed } => {
                    let after = org_cycles(&b, watch);
                    if let (Some((id0, c0)), Some((id1, c1))) = (before, after) {
                        // A speculative instruction that stalls runs nothing
                        // and closes the slice.
                        if id0 == id1 && !closed {
                            assert_eq!(c1, c0 + 1, "{test}: one instruction");
                        } else if id0 == id1 {
                            assert!(c1 == c0 || c1 == c0 + 1, "{test}: closing instruction");
                        }
                    }
                    if closed {
                        assert!(stepper.open_slice().is_none());
                    } else {
                        cut += 1;
                        let open = stepper.open_slice().expect("an open slice");
                        assert_eq!(open.cell(), watch, "{test}: the slice of the watch");
                        // Between two instructions the organism is in its cell.
                        assert_eq!(
                            org_cycles(&b, open.cell()).map(|o| o.0),
                            Some(open.org_id())
                        );
                        assert!((1..=32).contains(&open.instructions()));
                        // Only the first instruction of a slice counts a slice.
                        let first = open.instructions() == 1;
                        assert_eq!(stepper.slice_progress().0, progress + i32::from(first));
                    }
                    closed_cut += u64::from(closed);
                }
            }
            match next(400) {
                // `step_slice` finishes an open slice as one slice.
                0 => {
                    let open = stepper.open_slice().is_some();
                    broken_off += u64::from(open);
                    let slices = stepper.slice_progress().0;
                    if !stepper.step_slice(&mut b).unwrap() {
                        break;
                    }
                    assert!(stepper.open_slice().is_none());
                    let finished = if open { slices } else { slices + 1 };
                    assert_eq!(stepper.slice_progress().0, finished);
                }
                // `end_update` finishes the update, an open slice first.
                1 => {
                    broken_off += u64::from(stepper.open_slice().is_some());
                    break;
                }
                _ => {}
            }
        }
        stepper.end_update(&mut b, &mut out_b).unwrap();
        assert!(stepper.open_slice().is_none());
    }
    b.out.flush_all();
    assert_same_run(test, (&dir_a, &out_a), (&dir_b, &out_b));
    assert!(
        cut > 1000 && closed_cut > 10 && broken_off > 0,
        "{test}: the pacing should cut slices ({cut} cut, {closed_cut} closed, {broken_off} broken off)"
    );
}

fn check(test: &str) {
    let dir_a = fresh_config(test, &format!("{test}_run"));
    let mut a = build_world(&dir_a);
    let mut out_a = Vec::new();
    a.run(&mut out_a).unwrap();
    a.out.flush_all();

    let dir_b = fresh_config(test, &format!("{test}_stepped"));
    let mut b = build_world(&dir_b);
    let mut out_b = Vec::new();
    let mut stepper = Stepper::new(&b);
    // A fixed pseudo-random pacing: single slices, partial updates left for
    // `end_update` to finish, and whole updates.
    let mut seed: u32 = 0x9e37_79b9;
    let mut next = |n: u32| {
        seed = seed.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
        (seed >> 16) % n
    };
    loop {
        match next(3) {
            0 => {
                if !stepper.step_update(&mut b, &mut out_b).unwrap() {
                    break;
                }
            }
            1 => {
                if !stepper.begin_update(&mut b, &mut out_b).unwrap() {
                    break;
                }
                assert!(stepper.in_update());
                for _ in 0..next(500) {
                    let before = stepper.slice_progress().0;
                    if !stepper.step_slice(&mut b).unwrap() {
                        break;
                    }
                    assert_eq!(stepper.slice_progress().0, before + 1);
                }
                stepper.end_update(&mut b, &mut out_b).unwrap();
            }
            _ => {
                if !stepper.begin_update(&mut b, &mut out_b).unwrap() {
                    break;
                }
                while stepper.step_slice(&mut b).unwrap() {}
                // Finishing a part-run update through `step_update`.
                assert!(stepper.step_update(&mut b, &mut out_b).unwrap());
            }
        }
        assert!(!stepper.in_update());
    }
    // Once done, it stays done.
    assert!(!stepper.step_update(&mut b, &mut out_b).unwrap());
    b.out.flush_all();

    assert_eq!(
        String::from_utf8_lossy(&out_a),
        String::from_utf8_lossy(&out_b),
        "{test}: status lines"
    );
    let (mut fa, mut fb) = (Vec::new(), Vec::new());
    files_under(&dir_a.join("data"), &dir_a, &mut fa);
    files_under(&dir_b.join("data"), &dir_b, &mut fb);
    assert_eq!(fa, fb, "{test}: file lists");
    assert!(!fa.is_empty());
    for f in &fa {
        assert!(
            normalized(&dir_a.join(f)) == normalized(&dir_b.join(f)),
            "{test}: {} differs",
            f.display()
        );
    }
}

#[test]
fn stepped_run_matches_plain_run() {
    check("heads_default_100u");
}

#[test]
fn stepped_sexual_run_matches_plain_run() {
    check("sex");
}

#[test]
fn instruction_stepped_run_matches_plain_run() {
    check_watched("heads_default_100u");
}

#[test]
fn instruction_stepped_sexual_run_matches_plain_run() {
    check_watched("sex");
}

#[test]
fn stepper_rejects_calls_out_of_order() {
    let dir = fresh_config("heads_default_100u", "order");
    let mut w = build_world(&dir);
    let mut out = Vec::new();
    let mut stepper = Stepper::new(&w);
    assert!(stepper.step_slice(&mut w).is_err());
    assert!(stepper.begin_update(&mut w, &mut out).unwrap());
    assert!(stepper.begin_update(&mut w, &mut out).is_err());
}
