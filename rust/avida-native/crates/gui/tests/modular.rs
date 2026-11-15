//! The `heads_modular` set in the desktop application (headless): the
//! organism view shows all six registers with their names and the stacks,
//! and stepping the hand-written organism through a `call` and its
//! `return` shows the return address appear on the stack and leave it.

mod common;

use std::fs;
use std::path::{Path, PathBuf};

use avida_gui::model::Phase;
use avida_gui::StartupArgs;
use avida_session::{RunState, UntilOutcome};
use common::{copy_dir, harness, screenshot, settle, wait_until};
use egui::{vec2, Modifiers, Role};
use egui_kittest::kittest::Queryable;

type H = egui_kittest::Harness<'static, avida_gui::AvidaApp>;

/// The stock configuration with the modular set and the hand-written
/// organism alone in the centre cell (12) of a 5 x 5 world.
fn modular_config(name: &str) -> PathBuf {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR"))
        .join("gui")
        .join(name)
        .join("config");
    let _ = fs::remove_dir_all(&dir);
    copy_dir(
        &Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../../avida-core/support/config"),
        &dir,
    );
    fs::write(
        dir.join("events.cfg"),
        "u begin Inject modular-subroutine.org 12\nu 100000 Exit\n",
    )
    .unwrap();
    dir
}

fn args(dir: &Path) -> StartupArgs {
    StartupArgs {
        dir: Some(dir.to_path_buf()),
        avida_args: [
            "-s",
            "7",
            "-def",
            "INST_SET",
            "instset-heads-modular.cfg",
            "-set",
            "WORLD_X",
            "5",
            "-set",
            "WORLD_Y",
            "5",
            "-set",
            "COPY_MUT_PROB",
            "0",
        ]
        .map(String::from)
        .to_vec(),
        ..Default::default()
    }
}

fn click_cell(h: &mut H, cell: usize) {
    let w = h.state().map_image().map_or(1, |m| m.w);
    let canvas = h.get_by_label("Population map").rect();
    let vp = h.state().state().viewport;
    let target = vp.cell_rect(canvas, cell % w, cell / w).center();
    h.hover_at(target);
    h.step();
    for pressed in [true, false] {
        h.event(egui::Event::PointerButton {
            pos: target,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: Modifiers::NONE,
        });
    }
}

fn stopped(h: &mut H) -> UntilOutcome {
    h.state_mut().state_mut().debugger.last_stop = None;
    wait_until(h, "the debugger to stop", |app| {
        let s = app.state();
        s.debugger.last_stop.is_some()
            && s.status.as_ref().is_some_and(|st| st.run_until.is_none())
            && s.run_state() != Some(RunState::Running)
    });
    h.state()
        .state()
        .debugger
        .last_stop
        .as_ref()
        .unwrap()
        .outcome
}

/// One instruction: the Step button, then the new state of the organism.
fn step_once(h: &mut H) {
    let before = h
        .state()
        .state()
        .debugger
        .current
        .as_ref()
        .unwrap()
        .phenotype
        .lifetime_cpu_cycles;
    h.get_by_label("Step").click();
    assert_eq!(stopped(h), UntilOutcome::Reached);
    wait_until(h, "the stepped organism", |app| {
        app.state()
            .debugger
            .current
            .as_ref()
            .is_some_and(|o| o.phenotype.lifetime_cpu_cycles > before)
    });
}

fn ip(h: &H) -> i32 {
    h.state().state().debugger.ip().unwrap()
}

/// The accessible name of a stack item: `<stack> level <depth>: <value>`.
fn stack_item(depth: usize, value: i32) -> String {
    format!("Local stack level {depth}: {value}")
}

#[test]
fn the_organism_view_shows_six_registers_and_the_return_address_of_a_call() {
    let dir = modular_config("modular");
    let mut h = harness(args(&dir), vec2(1440.0, 900.0));
    wait_until(&mut h, "the world", |app| {
        let s = app.state();
        s.phase == Phase::Ready && s.frame.as_ref().is_some_and(|f| f.map.is_some())
    });
    h.get_by_role_and_label(Role::Tab, "Organism").click();
    settle(&mut h);
    h.get_by_label("Step update").click();
    wait_until(&mut h, "the ancestor", |app| {
        app.state()
            .frame
            .as_ref()
            .and_then(|f| f.map.as_ref())
            .is_some_and(|m| m.occupied() >= 1)
            && app.state().run_state() == Some(RunState::Paused)
    });
    click_cell(&mut h, 12);
    wait_until(&mut h, "the watched organism", |app| {
        let s = app.state();
        s.selection == Some(12) && s.debugger.current.as_ref().is_some_and(|o| o.cell == 12)
    });
    let org = h.state().state().debugger.current.clone().unwrap();

    // All six registers, with their names (`AX` to `FX`), and the stacks.
    assert_eq!(org.cpu.threads[0].regs.len(), 6);
    for name in ["AX", "BX", "CX", "DX", "EX", "FX"] {
        let _ = h.get_all_by_label_contains(&format!("{name} "));
    }
    h.get_by_label("Local stack");
    h.get_by_label("Global stack");

    // Run to the first `call` and step through it.
    let call = (0..org.cpu.memory.len())
        .find(|&p| org.cpu.inst_name(p) == "call")
        .unwrap() as i32;
    let ret = (0..org.cpu.memory.len())
        .find(|&p| org.cpu.inst_name(p) == "return")
        .unwrap() as i32;
    h.state_mut()
        .state_mut()
        .debugger
        .toggle_breakpoint(0, call);
    h.get_by_label("Run to breakpoint").click();
    assert_eq!(stopped(&mut h), UntilOutcome::Reached);
    wait_until(&mut h, "the IP on the call", |app| {
        app.state().debugger.ip() == Some(call)
    });
    settle(&mut h);
    h.get_by_label("Local stack");
    assert!(
        h.query_by_label(&stack_item(0, call)).is_none(),
        "nothing on the stack before the call"
    );
    assert!(h.get_all_by_label("empty").count() >= 1);
    h.state_mut()
        .state_mut()
        .debugger
        .toggle_breakpoint(0, call);

    // `call C C`: the return address (the call's own position) is pushed
    // and the IP lands on the subroutine's first instruction, behind the
    // label D D.
    step_once(&mut h);
    settle(&mut h);
    assert_eq!(ip(&h), ret - 1, "in the subroutine, at `nand`");
    h.get_by_label(&stack_item(0, call));
    screenshot(&mut h, "modular-in-subroutine");

    // `nand`, then `return` pops the address and resumes behind the label.
    step_once(&mut h);
    assert_eq!(ip(&h), ret);
    h.get_by_label(&stack_item(0, call));
    step_once(&mut h);
    settle(&mut h);
    assert_eq!(ip(&h), call + 1, "back behind the call");
    assert!(h.query_by_label(&stack_item(0, call)).is_none());
    assert!(h.get_all_by_label("empty").count() >= 1);
    screenshot(&mut h, "modular-after-return");
}
