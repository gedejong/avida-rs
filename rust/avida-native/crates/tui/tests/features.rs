//! The debugger, command palette, configuration restart, help overlay and
//! genotype views against a real session on the default configuration
//! (`avida-core/support/config`, copied to a temporary directory first).

mod common;

use std::cell::Cell;
use std::rc::Rc;

use avida_session::{RunState, UntilOutcome};
use avida_tui::debugger::StopWhat;
use common::*;
use ratatui::crossterm::event::KeyCode;

const SEED: [&str; 2] = ["-s", "7"];

fn first_frame() -> Step {
    Step::Until(
        "first frame",
        Box::new(|a| a.frame.as_ref().is_some_and(|f| f.map.is_some())),
    )
}

/// One update finished and the organism of cell 0 on the debugger screen.
fn to_debugger() -> Vec<Step> {
    vec![
        first_frame(),
        Step::Keys("."),
        Step::Until("one update", Box::new(|a| !a.history.is_empty())),
        Step::Keys("CU"),
        Step::Until(
            "watched organism",
            Box::new(|a| a.debugger.current.is_some() && a.debugger.ip().is_some()),
        ),
    ]
}

#[test]
fn a_breakpoint_is_set_and_reached_in_a_real_session() {
    let dir = fresh_support_config("dbg_break");
    let base = Rc::new(Cell::new(0u64));
    let bp = Rc::new(Cell::new(-1));
    let mut steps = to_debugger();
    steps.push(Step::Check(Box::new(|a, t| {
        assert_eq!(a.screen, avida_tui::app::Screen::Debug);
        has(t, "AX");
        has(t, "BX");
        has(t, "IP ");
        has(t, "Memory");
        has(t, "nop-");
        has(t, "stk A");
        has(t, "i step");
        assert!(a.debugger.is_alive());
    })));
    // Single-step three instructions.
    for k in 1..=3 {
        let b = Rc::clone(&base);
        let b2 = Rc::clone(&base);
        steps.push(Step::Check(Box::new(move |a, _| {
            b.set(
                a.debugger
                    .current
                    .as_ref()
                    .unwrap()
                    .phenotype
                    .lifetime_cpu_cycles,
            );
        })));
        steps.push(Step::Keys("i"));
        steps.push(Step::Until(
            "one instruction",
            Box::new(move |a| {
                a.debugger
                    .current
                    .as_ref()
                    .is_some_and(|o| o.phenotype.lifetime_cpu_cycles > b2.get())
                    && a.status.as_ref().is_some_and(|s| s.run_until.is_none())
            }),
        ));
        steps.push(Step::Check(Box::new(move |a, t| {
            assert_eq!(
                a.debugger.cycles_since_previous(),
                Some(1),
                "step {k} ran one instruction: cur {:?} prev {:?}",
                a.debugger
                    .current
                    .as_ref()
                    .map(|o| o.phenotype.lifetime_cpu_cycles),
                a.debugger
                    .previous
                    .as_ref()
                    .map(|o| o.phenotype.lifetime_cpu_cycles)
            );
            assert_eq!(a.run_state(), RunState::Paused);
            has(t, "Stepped after");
        })));
    }
    // The selection follows the IP: `b` sets a breakpoint on the line the
    // IP stands on; the organism comes back to it on its next pass.
    let (bp1, bp2, bp3) = (Rc::clone(&bp), Rc::clone(&bp), Rc::clone(&bp));
    steps.push(Step::Keys("b"));
    steps.push(Step::Check(Box::new(move |a, t| {
        let ip = a.debugger.ip().unwrap();
        assert_eq!(
            a.debugger.breakpoints.iter().copied().collect::<Vec<_>>(),
            vec![(0, ip)]
        );
        bp1.set(ip);
        assert!(t.contains('●'), "no breakpoint marker:\n{t}");
        has(t, &format!("breakpoints: {ip}"));
    })));
    steps.push(Step::Keys("g"));
    steps.push(Step::Until(
        "breakpoint reached",
        Box::new(move |a| {
            // The event and the final frame are published separately.
            a.debugger
                .last_stop
                .as_ref()
                .is_some_and(|n| n.what == StopWhat::Breakpoint)
                && a.debugger.ip() == Some(bp3.get())
                && a.status.as_ref().is_some_and(|s| s.run_until.is_none())
        }),
    ));
    steps.push(Step::Check(Box::new(move |a, t| {
        let note = a.debugger.last_stop.as_ref().unwrap();
        assert_eq!(note.outcome, UntilOutcome::Reached, "{}", note.text());
        assert_eq!(
            a.debugger.ip(),
            Some(bp2.get()),
            "stopped on the breakpoint"
        );
        assert!(note.slices > 1, "the organism ran to get there");
        assert_eq!(a.run_state(), RunState::Paused);
        has(t, "Breakpoint reached after");
    })));
    steps.push(Step::Snap("dbg_breakpoint".into()));
    // `SPECULATIVE` is on (the default): steps are still single
    // instructions and the breakpoint is hit exactly.
    run_script(&dir, &SEED, 80, 24, steps, "80x24");
    assert!(bp.get() >= 0);
}

#[test]
fn follow_offspring_moves_the_watch_into_the_child() {
    let dir = fresh_support_config("dbg_follow");
    let mut steps = to_debugger();
    steps.push(Step::Keys("f"));
    steps.push(Step::Keys(" "));
    steps.push(Step::Until(
        "the watch moved to an offspring",
        Box::new(|a| a.selection != Some(0) && a.debugger.org_id.is_some_and(|id| id > 1)),
    ));
    steps.push(Step::Keys(" "));
    steps.push(Step::Until(
        "paused",
        Box::new(|a| a.run_state() == RunState::Paused && a.debugger.current.is_some()),
    ));
    steps.push(Step::Check(Box::new(|a, t| {
        assert!(a.debugger.follow_offspring);
        has(t, "follow offspring: on");
        assert!(
            a.log.iter().any(|l| l.text.contains("Following offspring")),
            "no log line"
        );
    })));
    run_script(&dir, &SEED, 80, 24, steps, "80x24");
}

#[test]
fn the_palette_and_help_work_in_the_real_loop() {
    let dir = fresh_support_config("palette");
    let steps = vec![
        first_frame(),
        Step::Keys(":"),
        Step::Check(Box::new(|_, t| {
            has(t, "Commands");
            has(t, "Step one time slice");
        })),
        Step::Snap("palette_open".into()),
        Step::Keys("debugger"),
        Step::Check(Box::new(|_, t| has(t, "Go to the organism debugger"))),
        Step::Event(key(KeyCode::Enter)),
        Step::Check(Box::new(|a, _| {
            assert_eq!(a.screen, avida_tui::app::Screen::Debug);
            assert!(a.cmd_palette.is_none());
        })),
        Step::Keys("?"),
        Step::Check(Box::new(|_, t| {
            has(t, "Keys ");
            has(t, "Run or pause");
        })),
        Step::Snap("help_open".into()),
        Step::Event(key(KeyCode::Esc)),
        Step::Check(Box::new(|a, _| assert!(!a.show_help))),
    ];
    run_script(&dir, &SEED, 80, 24, steps, "80x24");
}

#[test]
fn editing_the_config_restarts_the_run_with_the_overrides() {
    let dir = fresh_support_config("cfg_restart");
    let steps = vec![
        first_frame(),
        Step::Keys(".."),
        Step::Until("two updates", Box::new(|a| a.history.len() >= 2)),
        Step::Keys("F"),
        Step::Check(Box::new(|_, t| {
            has(t, "WORLD_X");
            has(t, "A restart would run with: -s 7");
        })),
        Step::Snap("config".into()),
        Step::Keys("/world_x"),
        Step::Event(key(KeyCode::Enter)),
        Step::Keys("e"),
        Step::Event(key(KeyCode::Backspace)),
        Step::Event(key(KeyCode::Backspace)),
        Step::Keys("20"),
        Step::Event(key(KeyCode::Enter)),
        Step::Keys("/"),
        Step::Event(key(KeyCode::Esc)),
        Step::Keys("/world_y"),
        Step::Event(key(KeyCode::Enter)),
        Step::Keys("e"),
        Step::Event(key(KeyCode::Backspace)),
        Step::Event(key(KeyCode::Backspace)),
        Step::Keys("20"),
        Step::Event(key(KeyCode::Enter)),
        Step::Check(Box::new(|a, t| {
            assert_eq!(a.config.overrides.len(), 2);
            has(
                t,
                "A restart would run with: -s 7 -set WORLD_X 20 -set WORLD_Y 20",
            );
        })),
        Step::Keys("R"),
        Step::Check(Box::new(|_, t| {
            has(
                t,
                "Restart the run, discarding this one, with 2 changed settings",
            )
        })),
        Step::Keys("y"),
        Step::Until(
            "the world was rebuilt",
            Box::new(|a| {
                a.frame
                    .as_ref()
                    .is_some_and(|f| f.header.world_x == 20 && f.header.world_y == 20)
            }),
        ),
        Step::Check(Box::new(|a, t| {
            assert!(a.history.is_empty(), "history belongs to the old run");
            assert_eq!(
                a.args,
                ["-s", "7", "-set", "WORLD_X", "20", "-set", "WORLD_Y", "20"]
            );
            assert!(a.config.overrides.is_empty());
            let setup = a.setup.as_ref().unwrap();
            assert_eq!(setup.setting("WORLD_X").unwrap().value, "20");
            assert_eq!(a.run_state(), RunState::Paused);
            has(t, "Restarted with");
        })),
        // The new run advances.
        Step::Keys("."),
        Step::Until("a row of the new run", Box::new(|a| !a.history.is_empty())),
    ];
    run_script(&dir, &SEED, 80, 24, steps, "80x24");
}

#[test]
fn a_rejected_value_never_reaches_the_session() {
    let dir = fresh_support_config("cfg_bad");
    let steps = vec![
        first_frame(),
        Step::Keys("F/world_x"),
        Step::Event(key(KeyCode::Enter)),
        Step::Keys("ex"),
        Step::Event(key(KeyCode::Enter)),
        Step::Check(Box::new(|a, t| {
            assert!(a.config.overrides.is_empty());
            has(t, "is not a whole number");
        })),
    ];
    run_script(&dir, &SEED, 80, 24, steps, "80x24");
}

#[test]
fn the_abundance_chart_and_lineage_follow_a_real_population() {
    let dir = fresh_support_config("geno_views");
    let steps = vec![
        first_frame(),
        Step::Keys(" "),
        Step::Until("40 updates", Box::new(|a| a.history.len() >= 40)),
        Step::Keys(" "),
        Step::Until("paused", Box::new(|a| a.run_state() == RunState::Paused)),
        Step::Keys("Tv"),
        Step::Check(Box::new(|a, t| {
            assert!(a.abundance.len() >= 2);
            has(t, "Abundance over time");
            assert!(t.contains('█'), "no bands:\n{t}");
            has(t, "other");
        })),
        Step::Snap("abundance".into()),
        Step::Keys("vv"),
        Step::Keys("l"),
        Step::Until(
            "lineage frame",
            Box::new(|a| a.frame.as_ref().is_some_and(|f| !f.lineage.is_empty())),
        ),
        Step::Check(Box::new(|a, t| {
            assert_eq!(a.geno_view, avida_tui::app::GenoView::Lineage);
            has(t, "back to the founder");
            has(t, "founder first");
            let founder = a.lineage_rows()[0];
            has(t, &founder.name);
        })),
        Step::Snap("lineage".into()),
    ];
    run_script(&dir, &SEED, 80, 24, steps, "80x24");
}
