//! Watching one organism: `RunUntil` (step its instructions, run to a
//! breakpoint, give up after a slice budget, notice when it is gone or a
//! command takes over) and `FollowOffspring`.

mod common;

use avida_session::{
    Command, RunState, RunUntil, SessionEvent, StopWhen, UntilOutcome, UpdateProgress,
};
use avida_viewmodel::{LayerId, OrganismView, Subscription};

use common::{endless_config, fresh_config, gene_config, Harness};

fn is_paused(e: &SessionEvent) -> bool {
    matches!(e, SessionEvent::Paused { .. })
}

/// The next `RunUntilEnded`, then the `Paused` that follows it.
fn until_ended(h: &mut Harness) -> (UntilOutcome, u64) {
    let SessionEvent::RunUntilEnded {
        outcome, slices, ..
    } = h.wait_event(|e| matches!(e, SessionEvent::RunUntilEnded { .. }))
    else {
        unreachable!()
    };
    h.wait_event(is_paused);
    (outcome, slices)
}

/// The organism the latest frame shows in `cell`.
fn watched(h: &mut Harness, cell: i32) -> OrganismView {
    h.wait_until(|s| {
        s.latest_frame()
            .is_some_and(|f| f.inspect.as_ref().is_some_and(|o| o.cell == cell))
    });
    h.session.latest_frame().unwrap().inspect.clone().unwrap()
}

fn until(org: &OrganismView, stop: StopWhen, max_slices: u64) -> Command {
    Command::RunUntil(RunUntil {
        cell: org.cell,
        org_id: org.org_id,
        stop,
        max_slices,
    })
}

#[test]
fn run_until_steps_instructions_stops_at_breakpoints_and_respects_its_budget() {
    // `SPECULATIVE` is on (the default): a time slice of the organism runs
    // up to 33 instructions, and the watch still stops after each one.
    let mut h = Harness::spawn(&fresh_config("watch_steps"), &[]);
    h.send(Command::StepUpdate);
    h.wait_event(is_paused);
    h.send(Command::WatchCell(Some(0)));
    let before = watched(&mut h, 0);
    let cycles = before.phenotype.lifetime_cpu_cycles;

    // One instruction of the watched organism: paused inside update 1.
    h.send(until(&before, StopWhen::Cycles(1), 100_000));
    assert_eq!(h.next_state_event(), SessionEvent::Running);
    let (outcome, slices) = until_ended(&mut h);
    assert_eq!(outcome, UntilOutcome::Reached);
    assert!(slices >= 1);
    let after = watched(&mut h, 0);
    assert_eq!(after.org_id, before.org_id);
    assert_eq!(after.phenotype.lifetime_cpu_cycles, cycles + 1);

    // Every further step is exactly one instruction, also between the
    // instructions of one slice; the IP moves through the genome.
    let mut prev = after.clone();
    let mut ips = vec![prev.cpu.threads[0].heads[0].pos];
    let mut mid_slice = 0;
    for k in 0..60 {
        h.send(until(&prev, StopWhen::Cycles(1), 100_000));
        assert_eq!(until_ended(&mut h).0, UntilOutcome::Reached, "step {k}");
        mid_slice += usize::from(h.session.status().mid_slice);
        let now = watched(&mut h, 0);
        assert_eq!(
            now.phenotype.lifetime_cpu_cycles,
            prev.phenotype.lifetime_cpu_cycles + 1,
            "step {k} ran one instruction"
        );
        ips.push(now.cpu.threads[0].heads[0].pos);
        prev = now;
    }
    assert!(mid_slice > 0, "no step paused inside a speculative slice");
    let distinct: std::collections::BTreeSet<_> = ips.iter().collect();
    assert!(distinct.len() > 5, "the IP moved: {ips:?}");
    let after = prev;
    let frame = h.session.latest_frame().unwrap();
    assert!(frame.header.progress.in_update);
    assert_eq!(
        frame.header.last_finished_update,
        h.session.status().update - 1
    );
    assert_eq!(
        frame.header.progress,
        UpdateProgress::inside(
            h.session.status().slice.unwrap().0,
            h.session.status().slice.unwrap().1
        )
    );
    assert_eq!(h.session.status().run_until, None);

    // A breakpoint on the copy loop: the IP stands on it. `h-copy` runs
    // speculatively in the middle of slices, so a slice-level check would
    // usually step over it.
    let target = (0..after.cpu.memory.len())
        .find(|&p| after.cpu.inst_name(p) == "h-copy")
        .expect("the ancestor copies itself") as i32;
    h.send(until(&after, StopWhen::IpAt(vec![target]), 1_000_000));
    assert_eq!(until_ended(&mut h).0, UntilOutcome::Reached);
    let mut at = watched(&mut h, 0);
    assert_eq!(at.cpu.threads[0].heads[0].pos, target);
    // The positions the IP stands on in the copy loop, stepped one by one;
    // then a breakpoint on each of them in turn is hit exactly (the loop
    // runs once per instruction copied).
    let mut loop_ips = Vec::new();
    for _ in 0..8 {
        h.send(until(&at, StopWhen::Cycles(1), 100_000));
        assert_eq!(until_ended(&mut h).0, UntilOutcome::Reached);
        at = watched(&mut h, 0);
        loop_ips.push(at.cpu.threads[0].heads[0].pos);
    }
    loop_ips.sort_unstable();
    loop_ips.dedup();
    assert!(loop_ips.len() >= 3, "{loop_ips:?}");
    for &pos in loop_ips.iter().rev().chain(&loop_ips) {
        h.send(until(&at, StopWhen::IpAt(vec![pos]), 100_000));
        assert_eq!(until_ended(&mut h).0, UntilOutcome::Reached, "IP at {pos}");
        at = watched(&mut h, 0);
        assert_eq!(at.cpu.threads[0].heads[0].pos, pos);
    }

    // An unreachable position: the budget runs out, to the slice.
    h.send(until(&at, StopWhen::IpAt(vec![1_000_000]), 777));
    assert_eq!(until_ended(&mut h), (UntilOutcome::BudgetExhausted, 777));
    // Breakpoints per (space, position): the host's space 0 is hit, a
    // site of a space the organism does not have is not.
    h.send(until(&at, StopWhen::IpIn(vec![(0, loop_ips[0])]), 100_000));
    assert_eq!(until_ended(&mut h).0, UntilOutcome::Reached);
    at = watched(&mut h, 0);
    assert_eq!(at.cpu.threads[0].heads[0].pos, loop_ips[0]);
    h.send(until(&at, StopWhen::IpIn(vec![(2, loop_ips[0])]), 777));
    assert_eq!(until_ended(&mut h), (UntilOutcome::BudgetExhausted, 777));

    // The wrong organism: gone at once, naming the occupant.
    let wrong = OrganismView {
        org_id: at.org_id + 1000,
        ..at.clone()
    };
    h.send(until(&wrong, StopWhen::Cycles(1), 100));
    assert_eq!(
        until_ended(&mut h),
        (
            UntilOutcome::OrganismGone {
                occupant: Some(at.org_id)
            },
            0
        )
    );
    assert_eq!(h.session.state(), RunState::Paused);
}

#[test]
fn a_command_interrupts_run_until_and_a_finished_run_ends_it() {
    let mut h = Harness::spawn(&fresh_config("watch_interrupt"), &[]);
    h.send(Command::StepUpdate);
    h.wait_event(is_paused);
    h.send(Command::WatchCell(Some(0)));
    let org = watched(&mut h, 0);
    h.send(until(&org, StopWhen::IpAt(vec![1_000_000]), u64::MAX));
    assert_eq!(h.next_state_event(), SessionEvent::Running);
    h.wait_until(|s| s.status().run_until.is_some_and(|(n, _)| n > 0));
    h.send(Command::Pause);
    assert_eq!(until_ended(&mut h).0, UntilOutcome::Interrupted);
    assert_eq!(h.session.status().run_until, None);

    // The run reaches its Exit event first.
    h.send(until(&org, StopWhen::IpAt(vec![1_000_000]), u64::MAX));
    let SessionEvent::RunUntilEnded { outcome, .. } =
        h.wait_event(|e| matches!(e, SessionEvent::RunUntilEnded { .. }))
    else {
        unreachable!()
    };
    // The ancestor lives on in cell 0 until update 100 in this run.
    assert_eq!(outcome, UntilOutcome::RunOver);
    assert_eq!(h.next_state_event(), SessionEvent::Finished { update: 100 });
}

/// A 4 x 4 world fills within a few dozen updates; from then on every
/// birth replaces an organism.
const CROWDED: [&str; 6] = ["-set", "WORLD_X", "4", "-set", "WORLD_Y", "4"];

#[test]
fn a_replaced_organism_ends_the_watch_and_offspring_can_be_followed() {
    let mut h = Harness::spawn(&endless_config("watch_crowded"), &CROWDED);
    // The map, to find occupied cells later.
    h.send(Command::Subscribe(Subscription {
        layers: vec![LayerId::Genotype],
        inspect_cell: None,
        genotypes_top_n: 10,
        lineage_of: None,
    }));

    // Follow the founder's line from cell 0.
    h.send(Command::WatchCell(Some(0)));
    h.send(Command::FollowOffspring(true));
    h.send(Command::RunToUpdate(30));
    let moved = h.wait_event(|e| matches!(e, SessionEvent::WatchMoved { .. }));
    let SessionEvent::WatchMoved {
        from: 0,
        to,
        org_id,
        offspring: true,
    } = moved
    else {
        panic!("{moved:?}");
    };
    // The birth that moved the watch, reported just before.
    let born = h
        .events
        .iter()
        .rev()
        .find_map(|e| match e {
            SessionEvent::Born {
                parent_cell,
                org_id: child,
                cell,
                ..
            } if *child == org_id => Some((*parent_cell, *cell)),
            _ => None,
        })
        .expect("a Born event for the offspring followed");
    assert_eq!(born, (0, to));
    h.wait_event(|e| *e == SessionEvent::Paused { update: 30 });
    assert!(h.session.status().follow_offspring);
    // While following, each birth from the inspected organism moves the
    // watch to that offspring at once.
    let mut pending = None;
    let mut parent = None;
    for e in &h.events {
        match e {
            SessionEvent::Born {
                parent_id,
                org_id,
                cell,
                ..
            } => {
                assert_eq!(pending, None, "a birth that was not followed");
                if let Some(p) = parent {
                    assert_eq!(*parent_id, p, "born from the organism followed");
                }
                pending = Some((*cell, *org_id));
            }
            SessionEvent::WatchMoved {
                to,
                org_id,
                offspring: true,
                ..
            } => {
                assert_eq!(pending.take(), Some((*to, *org_id)));
                parent = Some(*org_id);
            }
            _ => {}
        }
    }
    // The frame shows the cell the watch moved to last.
    let last_move = h
        .events
        .iter()
        .rev()
        .find_map(|e| match e {
            SessionEvent::WatchMoved { to, org_id, .. } => Some((*to, *org_id)),
            _ => None,
        })
        .unwrap();
    assert!(to != 0 && org_id > 0);
    let frame = h.session.latest_frame().unwrap();
    if let Some(o) = &frame.inspect {
        assert_eq!(o.cell, last_move.0);
    }

    // Stop following; watch some other occupied cell until its organism
    // is replaced (which cells are full at update 30 depends on the run).
    let map = frame.map.as_ref().expect("subscribed to the map");
    let cell = (0..map.genotype_id.len())
        .find(|&c| c as i32 != last_move.0 && map.genotype_id[c] != -1)
        .expect("a crowded world has another organism") as i32;
    h.send(Command::FollowOffspring(false));
    h.send(Command::WatchCell(Some(cell)));
    let org = watched(&mut h, cell);
    h.send(until(&org, StopWhen::IpAt(vec![1_000_000]), 50_000_000));
    let (outcome, slices) = until_ended(&mut h);
    let UntilOutcome::OrganismGone { occupant } = outcome else {
        panic!("{outcome:?} after {slices} slices");
    };
    assert!(slices > 0);
    let frame = h.session.latest_frame().unwrap();
    match (occupant, &frame.inspect) {
        (Some(id), Some(o)) => {
            assert_ne!(id, org.org_id);
            assert_eq!(o.org_id, id);
        }
        (None, None) => {}
        other => panic!("occupant and frame disagree: {other:?}"),
    }
}

/// BCR and GP8 threads run in gene memories: a step moves the IP of the
/// thread that ran, within the gene's space, and a breakpoint names that
/// space (a site in the genome space is never reached).
fn gene_hardware_steps_and_breaks(hw_type: i32, name: &str) {
    let mut h = Harness::spawn(&gene_config(hw_type, name), &[]);
    h.send(Command::StepUpdate);
    h.wait_event(is_paused);
    h.send(Command::WatchCell(Some(12)));
    let mut prev = watched(&mut h, 12);
    assert_eq!(prev.cpu.threads.len(), 2);
    let mut seen = std::collections::BTreeSet::new();
    for k in 0..30 {
        h.send(until(&prev, StopWhen::Cycles(1), 100_000));
        assert_eq!(until_ended(&mut h).0, UntilOutcome::Reached, "step {k}");
        let now = watched(&mut h, 12);
        assert_eq!(
            now.phenotype.lifetime_cpu_cycles,
            prev.phenotype.lifetime_cpu_cycles + 1,
            "step {k} ran one instruction"
        );
        let ip = now.cpu.threads[0].heads[0];
        assert_eq!(ip.mem_space, 16, "thread 0 runs in gene 0");
        seen.insert(ip.pos);
        prev = now;
    }
    assert!(seen.len() >= 5, "the IP moved through the gene: {seen:?}");

    // Run to a site of gene 0 that the IP is not on: the IP stops there.
    let target = *seen
        .iter()
        .find(|&&p| p != prev.cpu.threads[0].heads[0].pos)
        .unwrap();
    h.send(until(&prev, StopWhen::IpIn(vec![(16, target)]), 100_000));
    assert_eq!(until_ended(&mut h).0, UntilOutcome::Reached);
    let at = watched(&mut h, 12);
    assert!(
        at.cpu
            .threads
            .iter()
            .any(|t| t.heads[0].mem_space == 16 && t.heads[0].pos == target),
        "a thread's IP stands on the breakpoint"
    );
    // The same position in the genome space is never an IP.
    h.send(until(&at, StopWhen::IpIn(vec![(0, target)]), 500));
    assert_eq!(until_ended(&mut h), (UntilOutcome::BudgetExhausted, 500));
}

#[test]
fn bcr_steps_and_breakpoints_work_in_gene_spaces() {
    gene_hardware_steps_and_breaks(5, "watch_bcr");
}

#[test]
fn gp8_steps_and_breakpoints_work_in_gene_spaces() {
    gene_hardware_steps_and_breaks(4, "watch_gp8");
}
