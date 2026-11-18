//! The `heads_modular` set in the terminal interface: the organism views
//! show all six registers with their names and the stacks, and stepping the
//! hand-written organism through a `call` and its `return` shows the return
//! address appear on the stack and leave it again.

mod common;

use std::cell::RefCell;
use std::path::PathBuf;
use std::rc::Rc;

use avida_tui::app::App;
use common::*;

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
    // One cycle per update and organism, so the organism has only just
    // started when it is first watched.
    "-set",
    "time.slicing_method",
    "0",
    "-set",
    "time.ave_time_slice",
    "1",
];

/// The stock configuration with the modular set and the hand-written
/// organism alone in the centre cell of a 5 x 5 world.
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

/// The state after one step: (ip, local stack, registers).
type Snapshot = (i32, Vec<i32>, Vec<i32>);

fn watched(a: &App) -> &avida_viewmodel::OrganismView {
    a.debugger.current.as_ref().unwrap()
}

/// The position of the first memory cell named `name`.
fn position_of(a: &App, name: &str) -> i32 {
    let o = watched(a);
    (0..o.cpu.memory.len())
        .find(|&p| o.cpu.cell_name(o.cpu.memory.get(p)) == name)
        .unwrap_or_else(|| panic!("no {name} in the genome")) as i32
}

#[test]
fn the_debugger_shows_six_registers_and_the_return_address_on_the_stack() {
    let dir = modular_config("modular_debugger");
    // (ip, local stack, registers) after every step.
    let log: Rc<RefCell<Vec<Snapshot>>> = Rc::default();
    let mut steps = vec![
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
        Step::Check(Box::new(|a, t| {
            let o = watched(a);
            assert_eq!(o.cpu.threads[0].regs.len(), 6);
            for r in ["AX", "BX", "CX", "DX", "EX", "FX"] {
                has(t, r);
            }
            has(t, "stk A*");
        })),
    ];
    // 60 single steps: past the first call, its return and the next call.
    for _ in 0..60 {
        steps.push(Step::Keys("i"));
        steps.push(Step::Until(
            "one instruction",
            Box::new(|a| a.status.as_ref().is_some_and(|s| s.run_until.is_none())),
        ));
        let log = log.clone();
        steps.push(Step::Check(Box::new(move |a, t| {
            let th = &watched(a).cpu.threads[0];
            let (call, sub) = (position_of(a, "call"), position_of(a, "return"));
            assert_eq!(call, 15);
            let ip = a.debugger.ip().unwrap();
            let entry = (ip, th.stacks[0].clone(), th.regs.clone());
            // While the subroutine runs the screen shows the return address.
            if ip == sub - 1 && !log.borrow().iter().any(|e| e.0 == ip) {
                assert_eq!(entry.1[0], call, "return address on the stack");
                has(t, &format!("stk A* {call}"));
                has(t, "FX");
            }
            log.borrow_mut().push(entry);
        })));
    }
    steps.push(Step::Snap("modular_debugger".into()));
    run_script(&dir, ARGS, 100, 40, steps, "100x40");

    let log = log.borrow();
    let call = 15; // the first `call` of modular-subroutine.org
    let enter = log
        .iter()
        .position(|e| e.1[0] == call)
        .expect("the return address appears on the stack");
    // It stays while the subroutine runs (nand, return) and is gone after
    // the `return`, which resumes behind the call's label.
    let leave = enter
        + log[enter..]
            .iter()
            .position(|e| e.1[0] == 0)
            .expect("the return address leaves the stack");
    assert_eq!(log[leave].0, call + 1, "execution resumes behind the call");
    // The intermediate lives in a register beyond CX.
    assert!(
        log.iter().any(|e| e.2[3] != 0 || e.2[4] != 0),
        "DX or EX holds a value"
    );
}

#[test]
fn the_organism_view_shows_six_registers() {
    let dir = modular_config("modular_zoom");
    let steps = vec![
        Step::Until(
            "first frame",
            Box::new(|a| a.frame.as_ref().is_some_and(|f| f.map.is_some())),
        ),
        Step::Keys("."),
        Step::Until("one update", Box::new(|a| !a.history.is_empty())),
        Step::Until(
            "the organism on the map",
            Box::new(|a| {
                a.frame
                    .as_ref()
                    .and_then(|f| f.map.as_ref())
                    .is_some_and(|m| m.occupied() >= 1)
            }),
        ),
        Step::Keys("CZ"),
        Step::Until(
            "the organism's view",
            Box::new(|a| a.frame.as_ref().is_some_and(|f| f.inspect.is_some())),
        ),
        Step::Check(Box::new(|_, t| {
            for r in ["AX", "BX", "CX", "DX", "EX", "FX"] {
                has(t, r);
            }
        })),
        Step::Snap("modular_zoom".into()),
    ];
    run_script(&dir, ARGS, 100, 40, steps, "100x40");
}
