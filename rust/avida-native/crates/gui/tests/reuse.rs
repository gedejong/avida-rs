//! The Reuse tab of the organism view (headless): the knockout map of the
//! hand-written `modular-subroutine.org`, asked for with a click, comes back
//! from the session, shows the measures, the strip with a mark row per task
//! and a hover per site; a playback says why it cannot. Screenshots go to
//! `<target>/tmp/gui-screenshots/reuse-*.png` in both themes at 1280 x 800
//! and in a narrow window.

mod common;

use std::fs;
use std::path::{Path, PathBuf};

use avida_gui::model::Phase;
use avida_gui::settings::ThemeChoice;
use avida_gui::ui::reuse::site_position;
use avida_gui::StartupArgs;
use avida_session::{RunState, SiteClass};
use common::{copy_dir, harness, screenshot, settle, wait_until};
use egui::{vec2, Modifiers, Role, Vec2};
use egui_kittest::kittest::Queryable;

type H = egui_kittest::Harness<'static, avida_gui::AvidaApp>;

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

/// The app with the organism in cell 12 watched.
fn watching(name: &str) -> H {
    let dir = modular_config(name);
    let mut h = harness(args(&dir), vec2(1280.0, 800.0));
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
    h
}

const MAP_LABEL: &str = "Knockout map of 57 sites, 3 tasks";

fn open_map(h: &mut H) {
    h.get_by_label("Reuse").click();
    settle(h);
    // Nothing is computed until it is asked for.
    assert!(h.state().state().debugger.reuse.reply.is_none());
    assert!(h.query_by_label(MAP_LABEL).is_none());
    h.get_by_label("Map knockouts").click();
    wait_until(h, "the knockout map", |app| {
        app.state().debugger.reuse.reply.is_some()
    });
    settle(h);
}

fn shots(h: &mut H, name: &str) -> Vec<PathBuf> {
    let before = h.state().settings().theme;
    let mut out = Vec::new();
    for (size_name, size) in [
        ("1280", vec2(1280.0, 800.0)),
        ("narrow", vec2(1000.0, 650.0)),
    ] {
        h.set_size(size);
        for (theme_name, theme) in [("dark", ThemeChoice::Dark), ("light", ThemeChoice::Light)] {
            h.state_mut().settings_mut().theme = theme;
            out.push(screenshot(
                h,
                &format!("reuse-{name}-{size_name}-{theme_name}"),
            ));
        }
    }
    h.set_size(vec2(1280.0, 800.0) as Vec2);
    h.state_mut().settings_mut().theme = before;
    settle(h);
    out
}

#[test]
fn the_reuse_tab_maps_the_shared_subroutine() {
    let mut h = watching("reuse_tab");
    open_map(&mut h);
    let reply = h.state().state().debugger.reuse.reply.clone().unwrap();
    let map = reply.result.as_ref().expect("a map");
    let site = |name: &str, n: usize| {
        map.inst_names
            .iter()
            .enumerate()
            .filter(|(_, i)| i.as_str() == name)
            .nth(n)
            .unwrap()
            .0
    };
    // The classes of named sites: the shared `nand`, each `call`, `h-copy`.
    assert_eq!(map.sites[site("nand", 0)].class, SiteClass::Shared);
    assert_eq!(map.sites[site("nand", 0)].lost, vec![0, 1, 2]);
    assert_eq!(map.sites[site("call", 0)].class, SiteClass::Task);
    assert_eq!(map.sites[site("call", 0)].lost, vec![0]);
    assert_eq!(map.sites[site("call", 1)].class, SiteClass::Shared);
    assert_eq!(map.sites[site("call", 1)].lost, vec![1, 2]);
    assert_eq!(map.sites[site("call", 2)].lost, vec![2]);
    assert_eq!(map.sites[site("h-copy", 0)].class, SiteClass::Lethal);

    // The header: measures as text, the drawing named.
    h.get_by_label("Shared sites");
    h.get_by_label("14");
    h.get_by_label("0.38");
    h.get_by_label_contains("3 sites, 1 target, 3 not neutral");
    h.get_by_label("NOT 11 · NAND 15 · AND 25");
    h.get_by_label(MAP_LABEL);
    // The legend counts the classes.
    h.get_by_label("Lethal 15");
    h.get_by_label("Shared 14");

    // Hovering the subroutine's `nand` says what it is.
    let block = h.get_by_label(MAP_LABEL).rect();
    let nand = site("nand", 0);
    let p = site_position(block, 57, nand).unwrap();
    h.hover_at(p);
    settle(&mut h);
    h.get_by_label(&format!("Site {nand}: nand"));
    h.get_by_label("Loses: NOT, NAND, AND");
    h.hover_at(egui::pos2(5.0, 5.0));
    settle(&mut h);
    shots(&mut h, "organism");
}

#[test]
fn a_gone_organism_gets_a_reason_not_a_map() {
    let mut h = watching("reuse_gone");
    h.get_by_label("Reuse").click();
    settle(&mut h);
    // Ask for another organism's map by hand: the session answers with a reason.
    h.state_mut()
        .state_mut()
        .debugger
        .current
        .as_mut()
        .unwrap()
        .org_id += 1000;
    h.get_by_label("Map knockouts").click();
    wait_until(&mut h, "the answer", |app| {
        app.state().debugger.reuse.reply.is_some()
    });
    settle(&mut h);
    h.get_by_label_contains("no longer in cell 12");
}

#[test]
fn a_playback_says_why_it_cannot_map_knockouts() {
    use avida_session::{Command, RecordSpec, Session, SessionConfig, SessionEvent};
    let dir = modular_config("reuse_playback");
    let file = dir.join("play.avrec");
    {
        let (tx, rx) = std::sync::mpsc::channel();
        let session = Session::spawn_with_wake(
            SessionConfig {
                work_dir: dir.clone(),
                args: args(&dir).avida_args,
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
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(120);
        loop {
            if session
                .drain_events()
                .contains(&SessionEvent::Paused { update: 10 })
            {
                break;
            }
            assert!(
                std::time::Instant::now() < deadline,
                "the recording did not finish"
            );
            let _ = rx.recv_timeout(std::time::Duration::from_millis(50));
        }
        session.send(Command::StopRecording);
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(60);
        while session.recording_status().is_none_or(|s| s.active) {
            assert!(std::time::Instant::now() < deadline);
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
    }
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
    h.get_by_role_and_label(Role::Tab, "Organism").click();
    settle(&mut h);
    assert_eq!(
        h.state().state().side_tab,
        avida_gui::model::SideTab::Organism
    );
    h.get_by_label("Organisms are not recorded");
    h.get_by_label_contains("does not carry the instruction set");
    shots(&mut h, "playback");
}
