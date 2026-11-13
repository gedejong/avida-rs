//! The behaviours of the third design pass: starting a new experiment
//! from the default settings, zooming the map, exploring the charts, and
//! a top bar that fits the smallest window.

mod common;

use std::path::PathBuf;

use avida_gui::model::{Phase, SideTab};
use avida_gui::StartupArgs;
use avida_session::RunState;
use common::{fresh_config, harness, screenshot, settle, wait_until};
use egui::{vec2, Key, Role};
use egui_kittest::kittest::{by, NodeT, Queryable};

type H = egui_kittest::Harness<'static, avida_gui::AvidaApp>;

fn ready(app: &avida_gui::AvidaApp) -> bool {
    let s = app.state();
    s.phase == Phase::Ready && s.frame.as_ref().is_some_and(|f| f.map.is_some())
}

fn open(dir: PathBuf, size: egui::Vec2) -> H {
    let mut h = harness(
        StartupArgs {
            dir: Some(dir),
            avida_args: Vec::new(),
            ..Default::default()
        },
        size,
    );
    wait_until(&mut h, "the world", ready);
    h
}

fn scratch(name: &str) -> PathBuf {
    let dir = PathBuf::from(env!("CARGO_TARGET_TMPDIR"))
        .join("gui")
        .join(name);
    let _ = std::fs::remove_dir_all(&dir);
    dir
}

#[test]
fn a_new_experiment_starts_from_the_default_settings() {
    let mut h = harness(StartupArgs::default(), vec2(1280.0, 800.0));
    h.state_mut().settings_mut().recent.clear();
    // A folder that already holds something: the experiment goes into a
    // new folder inside it, so nothing is overwritten.
    let chosen = scratch("new-experiment");
    std::fs::create_dir_all(&chosen).unwrap();
    std::fs::write(chosen.join("notes.txt"), "mine").unwrap();
    h.state_mut().answer_folder_dialogs_with(chosen.clone());
    settle(&mut h);
    h.get_by_label("New experiment…").click();
    wait_until(&mut h, "the new world", ready);
    let made = chosen.join("avida-experiment");
    for f in [
        "avida.cfg",
        "events.cfg",
        "environment.cfg",
        "default-heads.org",
    ] {
        assert!(made.join(f).is_file(), "{f}");
    }
    assert_eq!(
        std::fs::read_to_string(chosen.join("notes.txt")).unwrap(),
        "mine"
    );
    assert_eq!(
        h.state().settings().recent.first(),
        Some(&std::path::absolute(&made).unwrap())
    );
    let (w, h_cells) = h.state().state().world_size().unwrap();
    assert_eq!((w, h_cells), (60, 60), "the default world");
    assert!(h
        .state()
        .state()
        .log
        .entries()
        .any(|e| e.text.contains("Created a new experiment")));
    h.check_accessibility();

    // Again: a second folder next to the first.
    h.state_mut()
        .state_mut()
        .request(avida_gui::model::Request::NewExperiment);
    wait_until(&mut h, "the second world", |app| {
        ready(app)
            && app
                .state()
                .experiment
                .as_ref()
                .is_some_and(|e| e.dir.ends_with("avida-experiment-2"))
    });
}

#[test]
fn a_missing_folder_offers_a_new_experiment() {
    let missing = scratch("no-such-folder");
    let mut h = harness(
        StartupArgs {
            dir: Some(missing.clone()),
            avida_args: Vec::new(),
            ..Default::default()
        },
        vec2(1000.0, 650.0),
    );
    wait_until(&mut h, "the error", |app| app.state().open_error.is_some());
    h.get_by_label("This folder does not exist.");
    // Creating the missing folder is one way forward (no dialog).
    h.get_by_label("Create a new experiment here").click();
    wait_until(&mut h, "the new world", ready);
    assert!(missing.join("avida.cfg").is_file());
    assert!(h.state().state().open_error.is_none());
}

#[test]
fn the_map_zooms_with_buttons_and_keys_and_shows_an_overview() {
    let mut h = open(fresh_config("map-zoom"), vec2(1440.0, 900.0));
    assert!(h.query_by_label("Overview of the whole world").is_none());
    h.get_by_label_contains("the whole world fits");
    let fit = {
        let s = h.state().state();
        s.viewport.zoom
    };
    h.get_by_label("Zoom in").click();
    settle(&mut h);
    let vp = h.state().state().viewport;
    assert!(!vp.auto_fit);
    assert!((vp.zoom / fit - 1.5).abs() < 0.01, "{} vs {fit}", vp.zoom);
    h.get_by_label_contains("Zoom 150%");
    h.get_by_label("Zoom in").click();
    h.get_by_label("Zoom in").click();
    settle(&mut h);
    // Most of the world is off screen now: the overview shows where the
    // view is, and a click moves it.
    let overview = h.get_by_label("Overview of the whole world").rect();
    let before = h.state().state().viewport.center;
    h.hover_at(overview.left_top() + vec2(4.0, 4.0));
    h.drag_at(overview.left_top() + vec2(4.0, 4.0));
    h.step();
    h.drop_at(overview.left_top() + vec2(4.0, 4.0));
    settle(&mut h);
    let after = h.state().state().viewport.center;
    assert!(
        after.x < before.x && after.y < before.y,
        "{before:?} -> {after:?}"
    );
    screenshot(&mut h, "map-zoomed-overview-dark");

    // Keys: minus zooms out, the arrows move, F fits.
    let z = h.state().state().viewport.zoom;
    h.key_press(Key::Minus);
    settle(&mut h);
    assert!(h.state().state().viewport.zoom < z);
    let c = h.state().state().viewport.center;
    h.key_press(Key::ArrowRight);
    settle(&mut h);
    assert!(h.state().state().viewport.center.x > c.x);
    h.key_press(Key::F);
    settle(&mut h);
    assert!(h.state().state().viewport.auto_fit);
    assert!(h.query_by_label("Overview of the whole world").is_none());
    h.check_accessibility();
}

#[test]
fn charts_zoom_along_the_run_and_come_back() {
    let mut h = open(fresh_config("chart-zoom"), vec2(1440.0, 900.0));
    h.state_mut().state_mut().run_to(60);
    wait_until(&mut h, "update 60", |app| {
        let s = app.state();
        s.run_state() == Some(RunState::Paused)
            && s.history.latest().is_some_and(|r| r.update >= 60)
    });
    h.get_by_role_and_label(Role::Tab, "Charts").click();
    settle(&mut h);
    h.get_by_label_contains("Drag sideways to move along the run.");
    let area = h.state().state().chart_area.expect("the charts are drawn");
    // Drag the first chart to the right: earlier updates come into view
    // and the charts no longer follow the whole run.
    let from = area.left_top() + vec2(area.width() * 0.5, 90.0);
    h.hover_at(from);
    h.step();
    h.drag_at(from);
    h.step();
    for k in 1..=6 {
        h.hover_at(from + vec2(20.0 * k as f32, 0.0));
        h.step();
    }
    h.drop_at(from + vec2(120.0, 0.0));
    settle(&mut h);
    h.get_by_label("Show the whole run").click();
    settle(&mut h);
    h.get_by_label_contains("Drag sideways to move along the run.");

    // Hovering a chart reads the values at that update into the titles.
    h.hover_at(from);
    settle(&mut h);
    assert!(h.query_all_by_label_contains("at update").next().is_some());
    screenshot(&mut h, "charts-hover-dark");
    h.remove_cursor();

    // The task legend hides and shows a task.
    h.state_mut().state_mut().chart_tab = avida_gui::model::ChartTab::Tasks;
    settle(&mut h);
    h.get_all_by_role(Role::CheckBox)
        .next()
        .expect("a task chip")
        .click();
    settle(&mut h);
    assert!(h
        .query_all_by_role(Role::CheckBox)
        .any(|n| n.accesskit_node().toggled() == Some(egui::accesskit::Toggled::False)));
    assert_eq!(h.state().state().side_tab, SideTab::Charts);
    h.check_accessibility();
}

#[test]
fn the_top_bar_fits_the_smallest_window() {
    let mut h = open(fresh_config("top-bar"), vec2(1000.0, 650.0));
    h.state_mut().state_mut().run_to(12);
    wait_until(&mut h, "update 12", |app| {
        let s = app.state();
        s.run_state() == Some(RunState::Paused)
            && s.history.latest().is_some_and(|r| r.update >= 12)
    });
    let speed = h.get_by_label("Speed").rect();
    // The readouts of the top bar (other views have labels like these).
    let in_bar = |label: &str| {
        h.query_all(by().label_contains(label).role(Role::Label))
            .map(|n| n.rect())
            .find(|r| r.top() < 52.0)
            .unwrap_or_else(|| panic!("no {label} readout in the top bar"))
    };
    let update = in_bar("Update ");
    let organisms = in_bar("Organisms ");
    assert!(
        speed.right() + 8.0 <= update.left(),
        "speed {speed:?} runs into the update {update:?}"
    );
    assert!(
        update.right() <= organisms.left(),
        "{update:?} {organisms:?}"
    );
    // Every side tab is reachable by its full name; none is abbreviated.
    for tab in SideTab::ALL {
        h.get_by_role_and_label(Role::Tab, tab.title());
    }
    assert!(h.query_by_label_contains("Genot.").is_none());
    screenshot(&mut h, "top-bar-1000-dark");
}
