//! The knockout map in the terminal (`v` on the debugger screen), drawn into
//! test buffers (not a pty): at 100x30 and at 80x24 the measures, the class
//! letters and the listing with a column per task show the hand-written
//! `modular-subroutine.org`, the selection moves down the sites, `v` returns
//! to the memory, and a playback says why it cannot map knockouts.

mod common;

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use avida_session::{Command, RecordSpec, Session, SessionConfig, SessionEvent};
use avida_tui::app::App;
use avida_tui::color::ColorMode;
use avida_tui::runner::Driver;
use common::*;
use ratatui::crossterm::event::KeyCode;

const ARGS: &[&str] = &[
    "-s",
    "7",
    "-set",
    "instruction_sets",
    "heads_modular",
    "-set",
    "world.x",
    "5",
    "-set",
    "world.y",
    "5",
    "-set",
    "mutation.copy_prob",
    "0",
];

fn modular_config(name: &str) -> PathBuf {
    let dir = fresh_support_config(name);
    avida_config::convert::replace_section(
        &dir,
        "avida.yaml",
        "events",
        "u begin Inject modular-subroutine.org 12\nu 100000 Exit\n",
    )
    .unwrap();
    dir
}

fn script(snap: &'static str) -> Vec<Step> {
    vec![
        Step::Until(
            "first frame",
            Box::new(|a| a.frame.as_ref().is_some_and(|f| f.map.is_some())),
        ),
        Step::Keys("."),
        Step::Until("one update", Box::new(|a| !a.history.is_empty())),
        Step::Keys("CU"),
        Step::Until(
            "watched organism",
            Box::new(|a| a.debugger.current.is_some() && a.debugger.ip().is_some()),
        ),
        // The memory listing first; nothing is computed until `v`.
        Step::Check(Box::new(|a, t| {
            assert!(a.debugger.reuse.reply.is_none());
            assert!(!t.contains("Reuse of organism"));
        })),
        Step::Keys("v"),
        Step::Until(
            "the knockout map",
            Box::new(|a| a.debugger.reuse.reply.is_some()),
        ),
        Step::Check(Box::new(|a, t| {
            let org = a.debugger.org_id.unwrap();
            let r = a.debugger.reuse.answer_for(org).expect("the map");
            let m = r.result.as_ref().expect("a map");
            assert_eq!(m.measures().shared_sites, Some(14));
            has(t, "Reuse of organism");
            has(t, "3 tasks, 14 shared sites, reuse index 0.383");
            has(t, "58 test runs");
            has(t, "sites per task: not 11, nand 15, and 25");
            has(t, "3 call sites, 1 target, 3 non-neutral");
            has(t, "lethal 15");
            has(t, "shared 14");
            // The class letters of the genome and the task columns.
            has(t, "LLLLLL");
            has(t, "NOT");
            has(t, "NAND");
            has(t, "AND");
            has(t, "Site 0, h-alloc: lethal");
        })),
        Step::Snap(format!("{snap}_top")),
        // To the subroutine's `nand` (site 53): Page Down five times, then
        // three lines down.
        Step::Event(key(KeyCode::PageDown)),
        Step::Event(key(KeyCode::PageDown)),
        Step::Event(key(KeyCode::PageDown)),
        Step::Event(key(KeyCode::PageDown)),
        Step::Event(key(KeyCode::PageDown)),
        Step::Keys("jjj"),
        Step::Until("the selection", Box::new(|a| a.debugger.reuse.sel == 53)),
        Step::Check(Box::new(|_, t| {
            has(t, "Site 53, nand: shared, loses not, nand, and");
            // The row of the `nand` is in the listing, marked and with a
            // `#` under each of the three tasks.
            let row = t
                .lines()
                .find(|l| l.contains(">  53"))
                .unwrap_or_else(|| panic!("no selected row:\n{t}"));
            assert!(row.contains("nand") && row.contains("S"), "{row}");
            assert_eq!(row.matches('#').count(), 3, "{row}");
        })),
        Step::Snap(format!("{snap}_nand")),
        Step::Event(key(KeyCode::End)),
        Step::Until("the last site", Box::new(|a| a.debugger.reuse.sel == 56)),
        Step::Snap(format!("{snap}_end")),
        // `v` again: back to the memory listing, the map stays for later.
        Step::Keys("v"),
        Step::Check(Box::new(|a, t| {
            assert!(!a.debugger.reuse.show);
            assert!(a.debugger.reuse.reply.is_some());
            assert!(!t.contains("Reuse of organism"));
            has(t, "AX");
        })),
    ]
}

#[test]
fn the_reuse_view_at_100x30() {
    let dir = modular_config("reuse_100x30");
    run_script(&dir, ARGS, 100, 30, script("reuse"), "100x30");
}

#[test]
fn the_reuse_view_at_80x24() {
    let dir = modular_config("reuse_80x24");
    run_script(&dir, ARGS, 80, 24, script("reuse"), "80x24");
}

fn record(name: &str) -> PathBuf {
    let dir = modular_config(name);
    let file = dir.join("play.avrec");
    let (tx, _rx) = std::sync::mpsc::channel();
    let session = Session::spawn_with_wake(
        SessionConfig {
            work_dir: dir.clone(),
            args: ARGS.iter().map(|s| s.to_string()).collect(),
        },
        Box::new(move || {
            let _ = tx.send(());
        }),
    )
    .unwrap();
    let mut spec = RecordSpec::new(&file);
    spec.every = 5;
    session.send(Command::Record(spec));
    session.send(Command::RunToUpdate(10));
    let deadline = Instant::now() + test_timeout();
    let mut paused = false;
    while !paused {
        paused = session
            .drain_events()
            .contains(&SessionEvent::Paused { update: 10 });
        assert!(Instant::now() < deadline, "the recording did not finish");
        std::thread::sleep(Duration::from_millis(10));
    }
    session.send(Command::StopRecording);
    while session.recording_status().is_none_or(|s| s.active) {
        assert!(Instant::now() < deadline);
        std::thread::sleep(Duration::from_millis(10));
    }
    file
}

#[test]
fn a_playback_says_why_it_cannot_map_knockouts() {
    let file: &Path = &record("reuse_playback");
    let driver = Driver::start_playback(file, ColorMode::TrueColor).unwrap();
    let steps = vec![
        Step::Until(
            "the first frame",
            Box::new(|a: &App| a.frame.as_ref().is_some_and(|f| f.map.is_some())),
        ),
        Step::Keys("U"),
        Step::Check(Box::new(|_, t| {
            has(t, "does not carry the instruction set");
        })),
        Step::Keys("v"),
        Step::Check(Box::new(|a, t| {
            assert!(!a.debugger.reuse.show && a.debugger.reuse.reply.is_none());
            has(t, "Not in a playback");
        })),
        Step::Snap("reuse_playback".into()),
    ];
    run_driver(driver, 100, 30, steps, "100x30");
}
