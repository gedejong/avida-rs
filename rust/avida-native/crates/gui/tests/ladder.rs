//! The ladder view of the charts tab in the real application: a short run
//! of the stock logic-9 environment (the rungs are its reactions, NOT at the
//! bottom), clicks that make filter conditions, and a synthetic recording
//! played back. The screenshots go to `<target>/tmp/gui-screenshots/ladder/`.

mod common;

use std::path::PathBuf;

use avida_gui::model::{ChartTab, Phase, SideTab};
use avida_gui::settings::ThemeChoice;
use avida_gui::StartupArgs;
use avida_session::RunState;
use avida_viewmodel::Condition;
use common::{fresh_config, harness, screenshot_dir, settle, wait_until};
use egui::{vec2, Role, Vec2};
use egui_kittest::kittest::{by, NodeT, Queryable};

type H = egui_kittest::Harness<'static, avida_gui::AvidaApp>;

const RUNGS: [&str; 9] = [
    "NOT", "NAND", "AND", "ORN", "OR", "ANDN", "NOR", "XOR", "EQU",
];

const SIZES: [(&str, Vec2); 2] = [
    ("1280x800", vec2(1280.0, 800.0)),
    ("narrow", vec2(760.0, 640.0)),
];

fn shots() -> PathBuf {
    let dir = screenshot_dir().join("ladder");
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

/// Saves the window in both themes at both sizes, then restores both.
fn shoot(h: &mut H, name: &str) {
    let before = h.state().settings().theme;
    for (size_name, size) in SIZES {
        h.set_size(size);
        for (theme_name, theme) in [("dark", ThemeChoice::Dark), ("light", ThemeChoice::Light)] {
            h.state_mut().settings_mut().theme = theme;
            settle(h);
            let path = shots().join(format!("{name}-{size_name}-{theme_name}.png"));
            match h.render() {
                Ok(img) => img.save(&path).unwrap(),
                Err(e) => eprintln!("{name}: not rendered: {e}"),
            }
        }
    }
    h.set_size(SIZES[0].1);
    h.state_mut().settings_mut().theme = before;
    settle(h);
}

fn live(name: &str, update: i32) -> H {
    let mut h = harness(
        StartupArgs {
            dir: Some(fresh_config(name)),
            avida_args: Vec::new(),
            ..Default::default()
        },
        SIZES[0].1,
    );
    wait_until(&mut h, "the world", |app| {
        let s = app.state();
        s.phase == Phase::Ready && s.frame.as_ref().is_some_and(|f| f.map.is_some())
    });
    h.state_mut().state_mut().run_to(update);
    wait_until(&mut h, "the update", |app| {
        let s = app.state();
        s.run_state() == Some(RunState::Paused)
            && s.history.latest().is_some_and(|r| r.update >= update)
    });
    let s = h.state_mut().state_mut();
    s.side_tab = SideTab::Charts;
    s.chart_tab = ChartTab::Ladder;
    settle(&mut h);
    h
}

fn played(name: &str, spec: &common::synth::Synth) -> H {
    let file = fresh_config(name).join("synthetic.avrec");
    common::synth::write(&file, spec);
    let mut h = harness(
        StartupArgs {
            play: Some(file),
            ..Default::default()
        },
        SIZES[0].1,
    );
    wait_until(&mut h, "the recording", |app| {
        let s = app.state();
        s.phase == Phase::Ready
            && s.playback.is_some()
            && s.frame.as_ref().is_some_and(|f| f.map.is_some())
    });
    let s = h.state_mut().state_mut();
    s.side_tab = SideTab::Charts;
    s.chart_tab = ChartTab::Ladder;
    settle(&mut h);
    h
}

fn seek(h: &mut H, index: usize) {
    h.state_mut().state_mut().seek_frame(index);
    wait_until(h, "the frame", move |app| {
        app.state()
            .playback
            .as_ref()
            .is_some_and(|p| p.index == index)
    });
    wait_until(h, "the rows", move |app| {
        app.state().history.len() == index + 1
    });
    settle(h);
}

fn rung_button<'a>(h: &'a H, name: &'a str) -> egui_kittest::Node<'a> {
    h.get_by_label(name)
}

#[test]
fn the_rungs_are_the_reactions_in_file_order_and_a_click_filters_on_one() {
    let mut h = live("ladder-live", 100);
    // Every reaction of the stock environment is a rung, listed top rung
    // first (the nodes come in the order the rows were laid out).
    let labels: Vec<String> = h
        .query_all(by().role(Role::Button).label_contains("Filter on rung "))
        .map(|n| n.accesskit_node().label().unwrap_or_default().to_string())
        .collect();
    let want: Vec<String> = RUNGS
        .iter()
        .rev()
        .map(|n| format!("Filter on rung {n}"))
        .collect();
    assert_eq!(labels, want);
    // The headline is there before any rung is held.
    h.get_by_label_contains("Held now:");
    h.get_by_label_contains("Reached:");
    let top = rung_button(&h, "Filter on rung NOT").rect();
    let bottom = rung_button(&h, "Filter on rung EQU").rect();
    assert!(bottom.top() < top.top(), "the top rung is drawn first");

    assert!(h.state().state().brush.filter.is_empty());
    rung_button(&h, "Filter on rung NAND").click();
    settle(&mut h);
    assert_eq!(
        h.state().state().brush.filter.conditions().to_vec(),
        vec![Condition::Task {
            task: 1,
            performs: true
        }]
    );
    // The filter bar shows the condition, and the rung says it is in use.
    h.get_by_label("Remove condition: Performs task Nand");
    // A second click removes it.
    rung_button(&h, "Filter on rung NAND").click();
    settle(&mut h);
    assert!(h.state().state().brush.filter.is_empty());
    h.check_accessibility();
}

#[test]
fn the_ladder_has_a_state_before_the_first_update() {
    let mut h = harness(
        StartupArgs {
            dir: Some(fresh_config("ladder-empty")),
            avida_args: Vec::new(),
            ..Default::default()
        },
        SIZES[0].1,
    );
    wait_until(&mut h, "the world", |app| {
        app.state().frame.as_ref().is_some_and(|f| f.map.is_some())
    });
    let s = h.state_mut().state_mut();
    s.side_tab = SideTab::Charts;
    s.chart_tab = ChartTab::Ladder;
    settle(&mut h);
    h.get_by_label("No finished updates yet");
}

#[test]
fn a_recording_fills_the_ladder_from_its_frames_up_to_the_frame_shown() {
    let spec = common::synth::Synth::default();
    let mut h = played("ladder-play", &spec);
    // The first frame alone: one point.
    seek(&mut h, 0);
    assert_eq!(h.state().state().ladder.len(), 1);
    let last = spec.frames - 1;
    seek(&mut h, last);
    let st = h.state().state();
    assert_eq!(st.ladder.len(), spec.frames);
    // The recorded rows give the first update of each rung at the
    // recording's resolution (a multiple of the interval).
    let reached: Vec<i32> = (0..9).filter_map(|k| st.ladder.first_reached(k)).collect();
    assert!(!reached.is_empty(), "the synthetic run performs tasks");
    assert!(reached.iter().all(|u| u % spec.every == 0), "{reached:?}");
    // Going back forgets what lies ahead of the frame shown.
    seek(&mut h, 3);
    assert_eq!(h.state().state().ladder.len(), 4);
    let ahead = h.state().state().ladder.first_reached(8);
    assert!(ahead.is_none_or(|u| u <= 3 * spec.every));
    // The rungs and the filter work on a recording too.
    seek(&mut h, last);
    rung_button(&h, "Filter on rung XOR").click();
    settle(&mut h);
    assert_eq!(
        h.state().state().brush.filter.conditions().to_vec(),
        vec![Condition::Task {
            task: 7,
            performs: true
        }]
    );
}

#[test]
fn screenshots_of_the_ladder_in_both_themes_and_sizes() {
    let spec = common::synth::Synth::default();
    let mut h = played("ladder-shots", &spec);
    // Early: a few rungs reached.
    seek(&mut h, 12);
    shoot(&mut h, "1-early");
    seek(&mut h, spec.frames - 1);
    shoot(&mut h, "2-late");
    rung_button(&h, "Filter on rung AND").click();
    settle(&mut h);
    wait_until(&mut h, "the subset", |app| {
        app.state().brush.selection().is_some()
    });
    settle(&mut h);
    shoot(&mut h, "3-filtered");
    // A live run in the early game: most rungs not reached.
    let mut h = live("ladder-shots-live", 100);
    shoot(&mut h, "4-live");
}
