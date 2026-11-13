//! A recording played back in the real loop: the recorded frames, rows and
//! layers drive the app, the playback keys move through it, and what needs
//! a simulation says so.

mod common;

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use avida_session::{Command, RecordSpec, Session, SessionConfig, SessionEvent};
use avida_tui::app::Screen;
use avida_tui::color::ColorMode;
use avida_tui::runner::Driver;
use common::*;

/// Records the first 30 updates of the default configuration on a 20x20
/// world, every 10 updates: frames at 0, 10, 20 and 30.
fn record(name: &str) -> PathBuf {
    let dir = fresh_support_config(name);
    let file = dir.join("play.avrec");
    let (tx, rx) = std::sync::mpsc::channel();
    let session = Session::spawn_with_wake(
        SessionConfig {
            work_dir: dir.clone(),
            args: ["-s", "5", "-set", "WORLD_X", "20", "-set", "WORLD_Y", "20"]
                .map(String::from)
                .to_vec(),
        },
        Box::new(move || {
            let _ = tx.send(());
        }),
    )
    .unwrap();
    let mut spec = RecordSpec::new(&file);
    spec.every = 10;
    session.send(Command::Record(spec));
    session.send(Command::RunToUpdate(30));
    wait_for(&session, &rx, |e| *e == SessionEvent::Paused { update: 30 });
    session.send(Command::StopRecording);
    wait_for(&session, &rx, |e| {
        matches!(e, SessionEvent::RecordingStopped { .. })
    });
    assert_eq!(session.recording_status().unwrap().frames, 4);
    file
}

fn wait_for(
    session: &Session,
    wake: &std::sync::mpsc::Receiver<()>,
    pred: impl Fn(&SessionEvent) -> bool,
) {
    let deadline = Instant::now() + test_timeout();
    loop {
        if session.drain_events().iter().any(&pred) {
            return;
        }
        assert!(Instant::now() < deadline, "the recording did not finish");
        let _ = wake.recv_timeout(Duration::from_millis(50));
    }
}

fn play(file: &Path, steps: Vec<Step>, snaps: &str) -> Run {
    let driver = Driver::start_playback(file, ColorMode::TrueColor)
        .unwrap_or_else(|e| panic!("start_playback: {e}"));
    run_driver(driver, 110, 32, steps, snaps)
}

fn index(a: &avida_tui::app::App) -> Option<usize> {
    a.playback.as_ref().map(|p| p.index)
}

#[test]
fn a_recording_drives_the_app_and_the_playback_keys_move_through_it() {
    let file = record("play_keys");
    let steps = vec![
        Step::Until(
            "the first frame",
            Box::new(|a| a.frame.as_ref().is_some_and(|f| f.map.is_some()) && a.playback.is_some()),
        ),
        Step::Check(Box::new(|a, t| {
            let p = a.playback.as_ref().unwrap();
            assert_eq!((p.frames, p.index, p.last_update), (4, 0, 30));
            has(t, "PLAYBACK");
            has(t, "update 0/30");
            has(t, "frame 1/4");
            // Every recorded layer is on offer.
            assert!(a.layers().contains(&avida_session::LayerId::Ancestry));
            assert!(a.layers().len() > 13);
        })),
        Step::Snap("playback_first".into()),
        // One frame forward, then to the end.
        Step::Keys("."),
        Step::Until("record 1", Box::new(|a| index(a) == Some(1))),
        Step::Keys(">"),
        Step::Until("the last record", Box::new(|a| index(a) == Some(3))),
        Step::Check(Box::new(|a, t| {
            has(t, "update 30/30");
            has(t, "frame 4/4");
            // The chart rows are the recorded ones up to this frame.
            let updates: Vec<i32> = a.history.rows().iter().map(|r| r.update).collect();
            assert_eq!(updates, [0, 10, 20, 30]);
        })),
        Step::Snap("playback_last".into()),
        // Back: the history is rebuilt up to the frame shown.
        Step::Keys(","),
        Step::Until("record 2", Box::new(|a| index(a) == Some(2))),
        Step::Until(
            "the rows up to it",
            Box::new(|a| a.history.rows().last().map(|r| r.update) == Some(20)),
        ),
        Step::Keys("{"),
        Step::Until("record 0", Box::new(|a| index(a) == Some(0))),
        Step::Until("one row", Box::new(|a| a.history.len() == 1)),
        // Ten forward from the first frame clamps to the last.
        Step::Keys("}"),
        Step::Until("record 3 by clamping", Box::new(|a| index(a) == Some(3))),
        // The map layers work on the recorded frames.
        Step::Keys("M]]]"),
        Step::Check(Box::new(|a, _| {
            assert_eq!(a.screen, Screen::Map);
            assert!(a.frame.as_ref().unwrap().map.is_some());
        })),
        // What needs a simulation says so.
        Step::Keys("U"),
        Step::Check(Box::new(|_, t| has(t, "debugger needs a simulation"))),
        Step::Keys("W"),
        Step::Check(Box::new(|_, t| has(t, "Not in a playback"))),
        Step::Keys("F"),
        Step::Check(Box::new(|_, t| has(t, "A playback has no settings"))),
        Step::Keys("Z"),
        Step::Check(Box::new(|_, t| has(t, "Organisms are not recorded"))),
        Step::Keys("T"),
        Step::Check(Box::new(|a, t| {
            assert_eq!(a.screen, Screen::Genotypes);
            assert!(!a.frame.as_ref().unwrap().genotypes.is_empty());
            has(t, "Genotypes");
        })),
        Step::Keys("?"),
        Step::Check(Box::new(|_, t| {
            has(t, "Playback");
            has(t, "Back ten frames");
        })),
        Step::Keys("?"),
        // Play from the start to the end.
        Step::Keys("<"),
        Step::Until("record 0 again", Box::new(|a| index(a) == Some(0))),
        Step::Keys(" "),
        Step::Until(
            "the end",
            Box::new(|a| {
                index(a) == Some(3)
                    && a.run_state() == avida_session::RunState::Paused
                    && a.history.len() == 4
            }),
        ),
    ];
    let run = play(&file, steps, "play");
    assert!(run.driver.session().playback_info().is_some());
}

#[test]
fn a_missing_or_foreign_file_is_reported_not_played() {
    let e = Driver::start_playback(Path::new("/no/such/file.avrec"), ColorMode::Mono)
        .err()
        .unwrap();
    assert!(e.contains("/no/such/file.avrec"), "{e}");
}
