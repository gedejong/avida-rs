//! A recording opened in the real application: the recorded frames, rows
//! and layers drive the state, the slider and the keys move through it, and
//! what needs a simulation is refused.

mod common;

use std::path::PathBuf;
use std::time::{Duration, Instant};

use avida_gui::model::Phase;
use avida_gui::StartupArgs;
use avida_session::{Command, LayerId, RecordSpec, Session, SessionConfig, SessionEvent};
use common::{fresh_config, harness, screenshot, settle, timeout, wait_until};
use egui::{vec2, Key};
use egui_kittest::kittest::Queryable;

/// Records the first 30 updates of `heads_default_100u` every 10 updates:
/// frames at 0, 10, 20 and 30.
fn record(name: &str) -> PathBuf {
    let dir = fresh_config(name);
    let file = dir.join("play.avrec");
    let (tx, rx) = std::sync::mpsc::channel();
    let session = Session::spawn_with_wake(
        SessionConfig {
            work_dir: dir,
            args: vec!["-s".into(), "3".into()],
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
    let wait = |pred: &dyn Fn(&SessionEvent) -> bool| {
        let deadline = Instant::now() + timeout();
        loop {
            if session.drain_events().iter().any(pred) {
                return;
            }
            assert!(Instant::now() < deadline, "the recording did not finish");
            let _ = rx.recv_timeout(Duration::from_millis(50));
        }
    };
    wait(&|e| *e == SessionEvent::Paused { update: 30 });
    session.send(Command::StopRecording);
    wait(&|e| matches!(e, SessionEvent::RecordingStopped { .. }));
    assert_eq!(session.recording_status().unwrap().frames, 4);
    file
}

fn index(app: &avida_gui::AvidaApp) -> Option<usize> {
    app.state().playback.as_ref().map(|p| p.index)
}

#[test]
fn a_recording_drives_the_state_and_the_slider_moves_through_it() {
    let file = record("playback_app");
    let mut h = harness(
        StartupArgs {
            play: Some(file),
            ..Default::default()
        },
        vec2(1280.0, 800.0),
    );
    wait_until(&mut h, "the recording", |app| {
        let s = app.state();
        s.phase == Phase::Ready
            && s.playback.is_some()
            && s.frame.as_ref().is_some_and(|f| f.map.is_some())
    });
    {
        let s = h.state().state();
        let p = s.playback.as_ref().unwrap();
        assert_eq!((p.frames, p.index, p.last_update), (4, 0, 30));
        assert!(s.layers().contains(&LayerId::Ancestry));
        assert!(s.layers().len() > 13, "every layer was recorded");
        assert!(s.window_title().contains("play.avrec"));
    }
    // The bar names it and carries the slider.
    h.get_by_label("Playback");
    h.get_by_label("Playback position");
    screenshot(&mut h, "playback-first");

    // The keys of the live viewer move through frames: . next, , previous.
    h.key_press(Key::Period);
    wait_until(&mut h, "record 1", |app| index(app) == Some(1));
    h.key_press(Key::Comma);
    wait_until(&mut h, "record 0", |app| index(app) == Some(0));
    // A seek by the state (what the slider does).
    h.state_mut().state_mut().seek_frame(3);
    wait_until(&mut h, "record 3", |app| index(app) == Some(3));
    wait_until(&mut h, "all the rows", |app| app.state().history.len() == 4);
    assert_eq!(h.state().state().current_update(), Some(30));
    // Back in time: the charts' rows are rebuilt up to the frame.
    h.state_mut().state_mut().seek_frame(1);
    wait_until(&mut h, "record 1 again", |app| index(app) == Some(1));
    wait_until(&mut h, "two rows", |app| app.state().history.len() == 2);
    assert_eq!(
        h.state().state().history.latest().map(|r| r.update),
        Some(10)
    );
    screenshot(&mut h, "playback-middle");

    // What needs a simulation is refused, with a reason, and sends nothing.
    h.state_mut().state_mut().select_cell(Some(0));
    h.state_mut().state_mut().save_population();
    assert_eq!(h.state().state().selection, None);
    assert!(h.state().state().toasts.len() >= 2);
    settle(&mut h);
    h.state_mut().state_mut().side_tab = avida_gui::model::SideTab::Organism;
    settle(&mut h);
    h.get_by_label("Organisms are not recorded");
    h.state_mut().state_mut().side_tab = avida_gui::model::SideTab::Config;
    settle(&mut h);
    h.get_by_label("A playback has no settings");
    h.state_mut().state_mut().side_tab = avida_gui::model::SideTab::Charts;
    settle(&mut h);
    screenshot(&mut h, "playback-charts");

    // Play to the end.
    h.state_mut().state_mut().seek_frame(0);
    wait_until(&mut h, "record 0", |app| index(app) == Some(0));
    h.key_press(Key::Space);
    wait_until(&mut h, "the end", |app| {
        index(app) == Some(3) && app.state().run_state() == Some(avida_session::RunState::Paused)
    });
}
