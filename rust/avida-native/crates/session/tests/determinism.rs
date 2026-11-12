//! A session computes what `avida` computes: `heads_default_100u` driven
//! through a scripted mix of every command must write byte-identical data
//! files (timestamp lines aside), print the same status lines and deliver
//! one row per update, as a plain `World::run` of the same directory.

mod common;

use avida_session::{
    Command, LayerId, RunUntil, SessionEvent, Speed, StopWhen, Subscription, UntilOutcome,
};

use common::{assert_same_data, fresh_config, plain_run, Harness, LAST_UPDATE};

fn is_paused(e: &SessionEvent) -> bool {
    matches!(e, SessionEvent::Paused { .. })
}

fn paused_at(update: i32) -> impl Fn(&SessionEvent) -> bool {
    move |e| *e == SessionEvent::Paused { update }
}

#[test]
fn a_session_driven_by_a_command_mix_writes_what_a_plain_run_writes() {
    let plain = fresh_config("determinism_plain");
    let plain_stdout = plain_run(&plain);

    let dir = fresh_config("determinism_session");
    let mut h = Harness::spawn(&dir, &[]);

    h.send(Command::Subscribe(Subscription {
        layers: vec![LayerId::Genotype, LayerId::Fitness, LayerId::TaskCount],
        inspect_cell: None,
        genotypes_top_n: 10,
        lineage_of: None,
    }));
    for _ in 0..3 {
        h.send(Command::StepSlice);
        h.wait_event(is_paused);
    }
    h.send(Command::StepUpdate);
    h.wait_event(is_paused);
    h.send(Command::RunToUpdate(10));
    h.wait_event(paused_at(10));

    h.send(Command::WatchCell(Some(0)));
    for _ in 0..5 {
        h.send(Command::StepSlice);
        h.wait_event(is_paused);
    }
    // Inside update 11: saving reads the world without changing it.
    h.send(Command::SavePopulation);
    let SessionEvent::PopulationSaved(saved) =
        h.wait_event(|e| matches!(e, SessionEvent::PopulationSaved(_)))
    else {
        unreachable!()
    };
    assert!(saved.ends_with("detail-11.spop"), "{}", saved.display());
    assert_eq!(h.session.status().slice.map(|s| s.0), Some(5));
    h.send(Command::SetSpeed(Speed::UpdatesPerSecond(500.0)));
    h.send(Command::RunToUpdate(12));
    h.wait_event(paused_at(12));

    // The debugger: step the watched organism, run out of budget, run to a
    // breakpoint, follow offspring; all read-only between slices.
    h.send(Command::WatchCell(Some(0)));
    h.wait_until(|s| {
        s.latest_frame()
            .is_some_and(|f| f.inspect.as_ref().is_some_and(|o| o.cell == 0))
    });
    let org = h.session.latest_frame().unwrap().inspect.clone().unwrap();
    let until = |stop: StopWhen, max_slices: u64| {
        Command::RunUntil(RunUntil {
            cell: 0,
            org_id: org.org_id,
            stop,
            max_slices,
        })
    };
    let ended = |h: &mut Harness| {
        let SessionEvent::RunUntilEnded { outcome, .. } =
            h.wait_event(|e| matches!(e, SessionEvent::RunUntilEnded { .. }))
        else {
            unreachable!()
        };
        h.wait_event(is_paused);
        outcome
    };
    h.send(until(StopWhen::Cycles(1), 100_000));
    assert_eq!(ended(&mut h), UntilOutcome::Reached);
    h.send(until(StopWhen::IpAt(vec![1_000_000]), 300));
    assert_eq!(ended(&mut h), UntilOutcome::BudgetExhausted);
    let divide = (0..org.cpu.memory.len())
        .find(|&p| org.cpu.inst_name(p) == "h-divide")
        .expect("the ancestor divides") as i32;
    h.send(until(StopWhen::IpAt(vec![divide]), 5_000_000));
    assert_eq!(ended(&mut h), UntilOutcome::Reached);
    // Executing `h-divide` moves the watch to the offspring. (Following
    // only now: during the budget run above it would have moved the watch
    // away from cell 0 at the organism's first division.)
    h.send(Command::FollowOffspring(true));
    h.send(until(StopWhen::Cycles(1), 100_000));
    let moved = h.wait_event(|e| matches!(e, SessionEvent::WatchMoved { .. }));
    let SessionEvent::WatchMoved {
        from: 0,
        offspring: true,
        ..
    } = moved
    else {
        panic!("{moved:?}");
    };
    assert_eq!(ended(&mut h), UntilOutcome::Reached);

    // The organism divides some ten updates on; carry on from there.
    let next = h.session.status().update.max(20) + 2;
    h.send(Command::RunToUpdate(next));
    h.wait_event(paused_at(next));

    // A pause from outside, wherever it lands (possibly inside an update,
    // possibly after the run already stopped at 45).
    h.send(Command::SetSpeed(Speed::Max));
    h.send(Command::RunToUpdate(45));
    h.wait_until(|s| s.status().update >= 25);
    h.send(Command::Pause);
    h.wait_event(is_paused);
    for _ in 0..3 {
        h.send(Command::StepSlice);
        h.wait_event(is_paused);
    }
    h.send(Command::StepUpdate);
    h.wait_event(is_paused);

    h.send(Command::Subscribe(Subscription {
        layers: vec![
            LayerId::Genotype,
            LayerId::Merit,
            LayerId::Task(0),
            LayerId::Resource(0),
        ],
        inspect_cell: Some(1830),
        genotypes_top_n: usize::MAX,
        lineage_of: Some(1),
    }));
    h.send(Command::RunToUpdate(60));
    h.wait_event(paused_at(60));
    h.send(Command::WatchCell(None));
    h.send(Command::Run);
    let last = h.wait_event(|e| {
        matches!(
            e,
            SessionEvent::Finished { .. } | SessionEvent::Fatal(_) | SessionEvent::Error(_)
        )
    });
    assert_eq!(
        last,
        SessionEvent::Finished {
            update: LAST_UPDATE
        }
    );

    let frame = h.session.latest_frame().expect("a frame");
    assert_eq!(frame.update, LAST_UPDATE);
    assert!(frame.header.done);
    h.pump();
    let rows = h.row_updates();
    let stdout = h.stdout_lines();
    let events = h.events.clone();
    drop(h);

    assert_eq!(rows, (0..=LAST_UPDATE).collect::<Vec<_>>(), "rows");
    assert!(
        !events
            .iter()
            .any(|e| matches!(e, SessionEvent::Fatal(_) | SessionEvent::Error(_))),
        "{events:?}"
    );

    // The saved population sits beside the run's own files; the run's
    // files must match the plain run's.
    assert_eq!(saved, dir.join("data").join("detail-11.spop"));
    assert!(std::fs::metadata(&saved).unwrap().len() > 0);
    std::fs::remove_file(&saved).unwrap();
    assert_same_data(&plain.join("data"), &dir.join("data"));

    let plain_lines: Vec<&str> = plain_stdout.lines().collect();
    assert_eq!(plain_lines.len(), (LAST_UPDATE + 1) as usize);
    assert!(stdout.len() >= plain_lines.len(), "stdout: {stdout:?}");
    assert_eq!(
        &stdout[stdout.len() - plain_lines.len()..],
        &plain_lines[..],
        "status lines differ"
    );
}
