//! The registers, labels and ids of every thread for TransSMT hardware,
//! whose `Hardware` trait shows only the current thread's registers (they
//! are the tops of the thread's stacks).
//!
//! The experimental hardware takes the same path through
//! `HardwareExperimental::threads()`; no configuration of the consistency
//! suite runs it (the support organism `experimental.org` needs `h-search`
//! and `if-label`, which this port's experimental instruction library
//! lacks), so it is not exercised here.

mod common;

use std::fs;
use std::path::Path;

use avida_config::cmdline::process_cmdline;
use avida_viewmodel::{capture, Subscription};
use avida_world::run::Stepper;
use avida_world::world::World;
use common::*;

fn world_with(dir: &Path, args: &[&str]) -> World {
    let cwd = dir.to_string_lossy().into_owned();
    let mut argv = vec!["avida".to_string()];
    argv.extend(args.iter().map(|a| a.to_string()));
    let cmd = process_cmdline(&argv, &cwd);
    assert!(cmd.exit.is_none(), "command line: {}", cmd.stderr);
    World::new(cmd.config, &cwd, &cmd.defs)
        .unwrap_or_else(|e| panic!("World::new: {e}"))
        .0
}

#[test]
fn transsmt_threads_show_their_registers_and_labels() {
    // Parasites infect hosts at update 30 (600 in the test), so hosts run
    // a second thread.
    let dir = fresh_config("parasites_limited_resources", "threads_transsmt");
    let events = fs::read_to_string(dir.join("events.cfg")).unwrap();
    fs::write(
        dir.join("events.cfg"),
        events.replace("u 600 InjectParasite", "u 30 InjectParasite"),
    )
    .unwrap();
    let mut w = world_with(&dir, &["-s", "1"]);
    let mut stepper = Stepper::new(&w);
    let mut out = Vec::new();
    for _ in 0..34 {
        assert!(stepper.step_update(&mut w, &mut out).unwrap());
    }

    let (mut orgs, mut multi) = (0, 0);
    for cell in 0..w.pop.orgs.len() as i32 {
        let Some(org) = w.pop.orgs[cell as usize].as_deref() else {
            continue;
        };
        let sub = Subscription {
            inspect_cell: Some(cell),
            ..Subscription::default()
        };
        let cpu = capture(&w, &sub).inspect.expect("an organism").cpu;
        assert_eq!(cpu.hw_type, 2);
        for (t, th) in cpu.threads.iter().enumerate() {
            assert_eq!(th.regs.len(), 4, "cell {cell} thread {t}");
            assert_eq!(th.stacks.len(), 4, "three local stacks and the global one");
            for (r, v) in th.regs.iter().enumerate() {
                assert_eq!(*v, th.stacks[r][0], "register {r} is the top of stack {r}");
            }
            assert!(th.read_label.chars().all(|c| c.is_ascii_uppercase()));
            assert!(th.next_label.chars().all(|c| c.is_ascii_uppercase()));
        }
        // The current thread's registers are what the trait reports.
        let hw = org.hardware.as_ref();
        let cur: Vec<i32> = (0..4).map(|r| hw.register(r)).collect();
        assert_eq!(cpu.threads[cpu.cur_thread].regs, cur, "cell {cell}");
        orgs += 1;
        multi += usize::from(cpu.threads.len() > 1);
    }
    assert!(orgs > 0);
    assert!(multi > 0, "no infected host among {orgs} organisms");
}
