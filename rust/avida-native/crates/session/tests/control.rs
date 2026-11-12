//! Command semantics: rows, stepping, pausing, throttling, set-up errors,
//! shutdown, saving, and a run that fails or finishes staying inspectable.

mod common;

use std::time::{Duration, Instant};

use avida_config::cmdline::process_cmdline;
use avida_session::{
    Command, LayerId, RunState, Session, SessionConfig, SessionEvent, Speed, Subscription,
};
use avida_viewmodel::update_row;
use avida_world::run::Stepper;
use avida_world::world::World;

use common::{endless_config, fresh_config, Harness, LAST_UPDATE};

/// The rows a stepper-driven run produces, one after each `end_update`.
fn reference_rows(dir: &std::path::Path) -> Vec<String> {
    let cwd = dir.to_string_lossy().into_owned();
    let cmd = process_cmdline(&["avida".to_string()], &cwd);
    let (mut w, _) = World::new(cmd.config, &cwd, &cmd.defs).unwrap();
    let mut stepper = Stepper::new(&w);
    let mut out = Vec::new();
    let mut rows = Vec::new();
    while stepper.step_update(&mut w, &mut out).unwrap() {
        rows.push(format!("{:?}", update_row(&w)));
    }
    rows
}

#[test]
fn rows_are_lossless_and_in_order_and_a_finished_run_stays_inspectable() {
    let reference = reference_rows(&fresh_config("rows_reference"));

    let mut h = Harness::spawn(&fresh_config("rows"), &[]);
    h.send(Command::Run);
    assert_eq!(h.next_state_event(), SessionEvent::Running);
    assert_eq!(
        h.next_state_event(),
        SessionEvent::Finished {
            update: LAST_UPDATE
        }
    );
    h.pump();
    assert_eq!(h.row_updates(), (0..=LAST_UPDATE).collect::<Vec<_>>());
    let rows: Vec<String> = h.rows.iter().map(|r| format!("{r:?}")).collect();
    assert_eq!(rows, reference);
    assert!(h.session.drain_rows().is_empty());

    // Finished: the last frame stays, subscriptions still capture, nothing
    // advances. The binary would exit with status 0.
    assert_eq!(h.session.state(), RunState::Finished);
    assert_eq!(h.session.exit_code(), Some(0));
    let frame = h.session.latest_frame().unwrap();
    assert_eq!(frame.update, LAST_UPDATE);
    assert!(frame.map.is_none());
    let generation = h.session.frame_generation();
    h.send(Command::Subscribe(Subscription {
        layers: vec![LayerId::Genotype],
        inspect_cell: None,
        genotypes_top_n: 3,
        lineage_of: None,
    }));
    h.send(Command::Run);
    let SessionEvent::Error(why) = h.next_state_event() else {
        panic!("expected an error");
    };
    assert!(why.contains("finished at update 100"), "{why}");
    assert!(h.session.frame_generation() > generation);
    let frame = h.session.latest_frame().unwrap();
    assert!(frame.map.is_some());
    assert!(frame.genotypes.len() <= 3);
    h.send(Command::StepSlice);
    assert!(matches!(h.next_state_event(), SessionEvent::Error(_)));
    assert_eq!(h.session.state(), RunState::Finished);
}

#[test]
fn steps_advance_exactly_one_unit() {
    let mut h = Harness::spawn(&fresh_config("stepping"), &[]);
    assert_eq!(h.session.state(), RunState::Paused);
    assert_eq!(h.session.latest_frame().unwrap().update, -1);
    assert!(h.stdout_lines().iter().any(|l| l == "Random Seed: 101"));

    h.send(Command::StepUpdate);
    assert_eq!(h.next_state_event(), SessionEvent::Paused { update: 0 });
    h.send(Command::StepUpdate);
    assert_eq!(h.next_state_event(), SessionEvent::Paused { update: 1 });
    let status = h.session.status();
    assert_eq!((status.update, status.slice), (1, None));
    assert_eq!(h.row_updates(), vec![0, 1]);
    assert_eq!(h.session.latest_frame().unwrap().update, 1);

    // A slice begins update 2 and runs its first slice.
    let generation = h.session.frame_generation();
    h.send(Command::StepSlice);
    assert_eq!(h.next_state_event(), SessionEvent::Paused { update: 2 });
    let Some((1, ud_size)) = h.session.status().slice else {
        panic!("status {:?}", h.session.status());
    };
    assert!(ud_size > 2);
    h.send(Command::StepSlice);
    assert_eq!(h.next_state_event(), SessionEvent::Paused { update: 2 });
    assert_eq!(h.session.status().slice, Some((2, ud_size)));
    assert_eq!(h.session.frame_generation(), generation + 2);
    h.pump();
    assert_eq!(h.row_updates(), vec![0, 1]);

    // StepUpdate inside an update finishes it.
    h.send(Command::StepUpdate);
    assert_eq!(h.next_state_event(), SessionEvent::Paused { update: 2 });
    assert_eq!(h.session.status().slice, None);
    assert_eq!(h.row_updates(), vec![0, 1, 2]);

    h.send(Command::RunToUpdate(7));
    assert_eq!(h.next_state_event(), SessionEvent::Running);
    assert_eq!(h.next_state_event(), SessionEvent::Paused { update: 7 });
    assert_eq!(h.row_updates(), (0..=7).collect::<Vec<_>>());
    assert_eq!(h.session.latest_frame().unwrap().update, 7);
    let status = h.session.status();
    assert_eq!(
        (status.state, status.update, status.slice, status.run_to),
        (RunState::Paused, 7, None, None)
    );

    // A target already passed pauses at once; Pause while paused says so.
    h.send(Command::RunToUpdate(5));
    assert_eq!(h.next_state_event(), SessionEvent::Paused { update: 7 });
    h.send(Command::Pause);
    assert_eq!(h.next_state_event(), SessionEvent::Paused { update: 7 });
    assert_eq!(h.row_updates(), (0..=7).collect::<Vec<_>>());
}

/// Updates of 100000 slices per organism: an update of a few organisms
/// takes far longer than a slice batch, so a pause lands inside it.
const HUGE_UPDATES: [&str; 3] = ["-set", "AVE_TIME_SLICE", "100000"];

/// Waits until the session is running with at least `min_slices / 2`
/// slices of its update still to go (the status is polled, so the margin
/// has to cover the polling interval).
fn wait_inside_a_long_update(h: &mut Harness, min_slices: i32) {
    h.wait_until(|s| {
        let st = s.status();
        st.state == RunState::Running
            && st
                .slice
                .is_some_and(|(done, size)| size >= min_slices && size - done >= min_slices / 2)
    });
}

#[test]
fn pause_is_honoured_promptly_inside_an_update() {
    let mut h = Harness::spawn(&endless_config("pause"), &HUGE_UPDATES);
    h.send(Command::Run);
    assert_eq!(h.next_state_event(), SessionEvent::Running);
    wait_inside_a_long_update(&mut h, 2_000_000);
    let asked = Instant::now();
    h.send(Command::Pause);
    let SessionEvent::Paused { update } = h.next_state_event() else {
        panic!("expected Paused");
    };
    let latency = asked.elapsed();
    assert!(latency < Duration::from_secs(1), "pause took {latency:?}");
    let status = h.session.status();
    assert_eq!((status.state, status.update), (RunState::Paused, update));
    let (done, size) = status.slice.expect("paused inside the update");
    assert!(done < size);
    assert_eq!(h.session.latest_frame().unwrap().update, update);

    // Nothing advances while paused.
    h.pump();
    let rows = h.rows.len();
    let generation = h.session.frame_generation();
    std::thread::sleep(Duration::from_millis(200));
    h.pump();
    assert_eq!(h.rows.len(), rows);
    assert_eq!(h.session.status(), status);
    assert_eq!(h.session.frame_generation(), generation);

    // Stepping goes on from the slice the pause stopped at.
    h.send(Command::StepSlice);
    assert_eq!(h.next_state_event(), SessionEvent::Paused { update });
    assert_eq!(h.session.status().slice, Some((done + 1, size)));
    assert_eq!(h.row_updates(), (0..update).collect::<Vec<_>>());
}

#[test]
fn a_throttle_holds_updates_back() {
    let mut h = Harness::spawn(&fresh_config("throttle"), &[]);
    h.send(Command::SetSpeed(Speed::UpdatesPerSecond(40.0)));
    let started = Instant::now();
    h.send(Command::RunToUpdate(9));
    assert_eq!(h.next_state_event(), SessionEvent::Running);
    assert_eq!(h.next_state_event(), SessionEvent::Paused { update: 9 });
    // Ten updates, each begun at least 1/40 s after the one before.
    let elapsed = started.elapsed();
    assert!(elapsed >= Duration::from_millis(225), "{elapsed:?}");
    assert_eq!(h.session.status().speed, Speed::UpdatesPerSecond(40.0));
    assert_eq!(h.row_updates(), (0..=9).collect::<Vec<_>>());
}

#[test]
fn set_up_errors_are_returned_from_spawn() {
    let spawn = |dir: &std::path::Path, args: &[&str]| {
        Session::spawn(SessionConfig {
            work_dir: dir.to_path_buf(),
            args: args.iter().map(|s| s.to_string()).collect(),
        })
    };
    let missing = std::env::temp_dir().join("avida-session-test-no-such-directory");
    let err = spawn(&missing, &[]).err().expect("missing directory");
    assert!(err.contains("does not exist"), "{err}");

    let empty = fresh_config("errors_empty");
    std::fs::remove_file(empty.join("avida.cfg")).unwrap();
    std::fs::remove_file(empty.join("events.cfg")).unwrap();
    assert!(spawn(&empty, &[]).is_err());

    let dir = fresh_config("errors");
    let err = spawn(&dir, &["-set", "NO_SUCH_SETTING", "1"])
        .err()
        .expect("unknown setting");
    assert!(err.contains("NO_SUCH_SETTING"), "{err}");
    let err = spawn(&dir, &["-bogus"]).err().expect("unknown option");
    assert!(err.contains("Unknown Option"), "{err}");
    let err = spawn(&dir, &["-a"]).err().expect("analyze mode");
    assert!(err.contains("ANALYZE_MODE"), "{err}");

    // Arguments reach the world, and the setup reports them.
    let session = spawn(&dir, &["-s", "7", "-set", "WORLD_X", "20"]).unwrap();
    let frame = session.latest_frame().unwrap();
    assert_eq!((frame.header.random_seed, frame.header.world_x), (7, 20));
    let setup = session.setup();
    assert_eq!(session.data_dir(), dir.join("data"));
    assert_eq!(setup.config_file, dir.join("avida.cfg"));
    let world_x = setup.setting("WORLD_X").unwrap();
    assert_eq!(
        (world_x.value.as_str(), world_x.from_command_line),
        ("20", true)
    );
    assert!(world_x.is_changed());
    let seed = setup.setting("RANDOM_SEED").unwrap();
    assert_eq!((seed.value.as_str(), seed.from_command_line), ("7", true));
    // `COPY_MUT_PROB` is a double printed as `%f` both ways.
    let copy = setup.setting("COPY_MUT_PROB").unwrap();
    assert_eq!(copy.value, copy.default, "{copy:?}");
    assert!(!copy.from_command_line);
    assert_eq!(
        setup.settings.len(),
        avida_config::config::all_entries().count()
    );
    assert_eq!(setup.event_file, dir.join("events.cfg"));
    let first = &setup.events[0];
    assert_eq!(
        (
            first.trigger.as_str(),
            first.timing.as_str(),
            first.action.as_str()
        ),
        ("u", "begin", "Inject")
    );
    assert_eq!(first.args, "default-classic.org");
    let exit = setup.events.last().unwrap();
    assert_eq!(
        (exit.timing.as_str(), exit.action.as_str()),
        ("100", "Exit")
    );
    assert!(exit.args.is_empty(), "the comment is not an argument");
    assert!(session
        .drain_events()
        .contains(&SessionEvent::Stdout("Random Seed: 7".to_string())));
}

#[test]
fn dropping_a_running_session_joins_promptly() {
    let mut h = Harness::spawn(&endless_config("drop_running"), &HUGE_UPDATES);
    h.send(Command::Run);
    wait_inside_a_long_update(&mut h, 2_000_000);
    let asked = Instant::now();
    drop(h);
    let took = asked.elapsed();
    assert!(took < Duration::from_secs(2), "drop took {took:?}");

    // Also while a throttle is holding the next update back.
    let mut h = Harness::spawn(&endless_config("drop_throttled"), &[]);
    h.send(Command::SetSpeed(Speed::UpdatesPerSecond(0.1)));
    h.send(Command::Run);
    h.wait_until(|s| s.status().update >= 0 && s.status().slice.is_none());
    let asked = Instant::now();
    drop(h);
    let took = asked.elapsed();
    assert!(took < Duration::from_secs(2), "drop took {took:?}");
}

#[test]
fn a_fatal_error_leaves_the_session_inspectable() {
    let dir = fresh_config("fatal");
    let text = std::fs::read_to_string(dir.join("events.cfg")).unwrap();
    std::fs::write(
        dir.join("events.cfg"),
        format!("{text}u 3 Inject missing.org\n"),
    )
    .unwrap();

    let mut h = Harness::spawn(&dir, &[]);
    assert_eq!(h.session.exit_code(), None, "the run can still advance");
    h.send(Command::Run);
    assert_eq!(h.next_state_event(), SessionEvent::Running);
    let SessionEvent::Fatal(why) = h.next_state_event() else {
        panic!("expected Fatal");
    };
    assert!(why.contains("missing.org"), "{why}");
    assert_eq!(h.session.state(), RunState::Failed);
    // `avida` prints `error: ...` and exits with status 1.
    assert_eq!(h.session.exit_code(), Some(1));
    assert_eq!(h.row_updates(), vec![0, 1, 2, 3]);
    assert_eq!(h.session.latest_frame().unwrap().update, 3);

    let generation = h.session.frame_generation();
    // The founder, injected at the start into cell 0.
    h.send(Command::WatchCell(Some(0)));
    h.send(Command::StepUpdate);
    let SessionEvent::Error(why) = h.next_state_event() else {
        panic!("expected Error");
    };
    assert!(why.contains("fatal"), "{why}");
    assert_eq!(h.session.frame_generation(), generation + 1);
    assert!(h.session.latest_frame().unwrap().inspect.is_some());

    h.send(Command::SavePopulation);
    let SessionEvent::PopulationSaved(path) = h.next_state_event() else {
        panic!("expected PopulationSaved");
    };
    assert!(path.is_file());
}

#[test]
fn restart_replaces_the_run_in_the_same_thread() {
    let dir = fresh_config("restart");
    let mut h = Harness::spawn(&dir, &["-s", "7"]);
    h.send(Command::Subscribe(Subscription {
        layers: vec![LayerId::Genotype],
        inspect_cell: Some(0),
        genotypes_top_n: 5,
        lineage_of: Some(1),
    }));
    h.send(Command::FollowOffspring(true));
    h.send(Command::RunToUpdate(5));
    assert_eq!(h.next_state_event(), SessionEvent::Running);
    assert_eq!(h.next_state_event(), SessionEvent::Paused { update: 5 });

    // A command line the new world cannot be built from: the old run goes
    // on where it was.
    h.send(Command::Restart {
        args: vec!["-set".into(), "NO_SUCH_SETTING".into(), "1".into()],
    });
    let SessionEvent::RestartFailed(why) = h.next_state_event() else {
        panic!("expected RestartFailed");
    };
    assert!(why.contains("NO_SUCH_SETTING"), "{why}");
    assert_eq!(h.session.status().update, 5);
    assert_eq!(h.session.setup().setting("RANDOM_SEED").unwrap().value, "7");
    h.send(Command::StepUpdate);
    assert_eq!(h.next_state_event(), SessionEvent::Paused { update: 6 });

    // A new world: new set-up, paused before its first update, the old
    // run's rows gone, the subscription's layers kept, the watch dropped.
    h.pump();
    h.send(Command::Restart {
        args: ["-s", "9", "-set", "WORLD_X", "20", "-set", "WORLD_Y", "20"]
            .map(String::from)
            .to_vec(),
    });
    let SessionEvent::Restarted(setup) = h.next_state_event() else {
        panic!("expected Restarted");
    };
    assert_eq!(setup.setting("WORLD_X").unwrap().value, "20");
    assert_eq!(*setup, *h.session.setup());
    assert_eq!(h.session.data_dir(), dir.join("data"));
    assert_eq!(h.next_state_event(), SessionEvent::Paused { update: -1 });
    assert!(h
        .events
        .iter()
        .any(|e| *e == SessionEvent::Stdout("Random Seed: 9".to_string())));
    assert!(h.session.drain_rows().is_empty());
    let status = h.session.status();
    assert_eq!(
        (status.state, status.update, status.follow_offspring),
        (RunState::Paused, -1, false)
    );
    let frame = h.session.latest_frame().unwrap();
    assert_eq!((frame.header.world_x, frame.header.world_y), (20, 20));
    assert!(frame.map.is_some() && frame.inspect.is_none());
    assert_eq!(frame.update, -1);

    // The new run computes what a fresh session of it computes.
    h.send(Command::RunToUpdate(10));
    assert_eq!(h.next_state_event(), SessionEvent::Running);
    assert_eq!(h.next_state_event(), SessionEvent::Paused { update: 10 });
    h.pump();
    let restarted: Vec<String> = h
        .rows
        .iter()
        .rev()
        .take(11)
        .map(|r| format!("{r:?}"))
        .collect();
    drop(h);
    let mut fresh = Harness::spawn(
        &fresh_config("restart_fresh"),
        &["-s", "9", "-set", "WORLD_X", "20", "-set", "WORLD_Y", "20"],
    );
    fresh.send(Command::RunToUpdate(10));
    fresh.wait_event(|e| *e == SessionEvent::Paused { update: 10 });
    fresh.pump();
    let expected: Vec<String> = fresh.rows.iter().rev().map(|r| format!("{r:?}")).collect();
    assert_eq!(restarted, expected);
}

#[test]
fn what_the_simulation_prints_arrives_as_events() {
    let dir = fresh_config("diagnostics");
    let text = std::fs::read_to_string(dir.join("events.cfg")).unwrap();
    std::fs::write(
        dir.join("events.cfg"),
        format!("{text}u 2 PrintData odd.dat update,no_such_entry\n"),
    )
    .unwrap();
    let mut h = Harness::spawn(&dir, &[]);
    h.send(Command::RunToUpdate(3));
    h.wait_event(|e| *e == SessionEvent::Paused { update: 3 });
    h.pump();
    // After the set-up warnings (a version mismatch in this file), once.
    let missing = "Data manager unable to find entry 'no_such_entry'";
    let lines = h.stderr_lines();
    assert_eq!(
        lines.iter().filter(|l| *l == missing).count(),
        1,
        "{lines:?}"
    );
    // In order with the status lines: the event fires once update 2 has
    // ended, before update 3 begins.
    let pos = |pred: &dyn Fn(&SessionEvent) -> bool| h.events.iter().position(pred).unwrap();
    let err = pos(&|e| matches!(e, SessionEvent::Stderr(l) if l == missing));
    let ud2 = pos(&|e| matches!(e, SessionEvent::Stdout(l) if l.starts_with("UD: 2 ")));
    let ud3 = pos(&|e| matches!(e, SessionEvent::Stdout(l) if l.starts_with("UD: 3 ")));
    assert!(ud2 < err && err < ud3, "{:?}", h.events);
}
