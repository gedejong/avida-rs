//! A screenshot sweep for design review: every screen and state of the
//! application, in both themes, at the default window size (1440 x 900)
//! and the minimum one (1000 x 650). It asserts little; it writes PNG
//! files under `<target>/tmp/gui-screenshots/audit/` to look at.
//!
//! It runs a short experiment, so it is left out of the default run:
//! `cargo test -p avida-gui --test audit -- --ignored --nocapture`.

mod common;

use std::path::PathBuf;

use avida_gui::model::{ChartTab, Phase, SideTab};
use avida_gui::settings::ThemeChoice;
use avida_gui::StartupArgs;
use avida_session::RunState;
use common::{fresh_config, harness, screenshot_dir, settle, wait_until};
use egui::{vec2, Key, Modifiers, Vec2};

type H = egui_kittest::Harness<'static, avida_gui::AvidaApp>;

const SIZES: [(&str, Vec2); 2] = [("wide", vec2(1440.0, 900.0)), ("min", vec2(1000.0, 650.0))];

fn audit_dir() -> PathBuf {
    let dir = screenshot_dir().join("audit");
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

/// Saves the window as it looks now, in both themes and at both sizes.
fn shoot(h: &mut H, name: &str) {
    let before = h.state().settings().theme;
    for (size_name, size) in SIZES {
        h.set_size(size);
        for (theme_name, theme) in [("dark", ThemeChoice::Dark), ("light", ThemeChoice::Light)] {
            h.state_mut().settings_mut().theme = theme;
            settle(h);
            let path = audit_dir().join(format!("{name}-{size_name}-{theme_name}.png"));
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

fn ready(app: &avida_gui::AvidaApp) -> bool {
    let s = app.state();
    s.phase == Phase::Ready && s.frame.as_ref().is_some_and(|f| f.map.is_some())
}

fn tab(h: &mut H, t: SideTab) {
    h.state_mut().state_mut().side_tab = t;
    settle(h);
}

#[test]
#[ignore = "a screenshot sweep for design review; run it explicitly"]
fn every_screen_in_both_themes_and_sizes() {
    let dir = fresh_config("audit");
    let mut h = harness(StartupArgs::default(), SIZES[0].1);
    h.state_mut().settings_mut().recent.clear();
    settle(&mut h);
    shoot(&mut h, "01-start-empty");

    // Open; the world before its first update.
    h.state_mut()
        .state_mut()
        .request(avida_gui::model::Request::Open(dir.clone()));
    h.step();
    shoot(&mut h, "02-opening");
    wait_until(&mut h, "the world", ready);
    tab(&mut h, SideTab::Stats);
    shoot(&mut h, "03-ready-before-first-update");
    tab(&mut h, SideTab::Charts);
    shoot(&mut h, "04-charts-empty");

    // Forty updates.
    h.state_mut().state_mut().run_to(40);
    wait_until(&mut h, "update 40", |app| {
        let s = app.state();
        s.run_state() == Some(RunState::Paused)
            && s.history.latest().is_some_and(|r| r.update >= 40)
    });
    tab(&mut h, SideTab::Stats);
    shoot(&mut h, "05-stats");
    tab(&mut h, SideTab::Charts);
    for c in ChartTab::ALL {
        h.state_mut().state_mut().chart_tab = c;
        settle(&mut h);
        shoot(
            &mut h,
            &format!("06-charts-{}", c.title().replace(' ', "-")),
        );
    }
    h.state_mut().state_mut().chart_tab = ChartTab::TimeSeries;
    tab(&mut h, SideTab::Genotypes);
    shoot(&mut h, "07-genotypes");
    let dom = h.state().state().dominant_id();
    h.state_mut().state_mut().set_highlight(dom);
    wait_until(&mut h, "the lineage", |app| {
        let s = app.state();
        s.frame.as_ref().is_some_and(|f| !f.lineage.is_empty())
    });
    shoot(&mut h, "08-genotypes-highlight");
    h.state_mut().state_mut().clear_highlight();
    tab(&mut h, SideTab::Organism);
    shoot(&mut h, "09-organism-none");
    h.state_mut().state_mut().inspect_dominant();
    wait_until(&mut h, "the inspected organism", |app| {
        app.state().debugger.current.is_some()
    });
    shoot(&mut h, "10-organism");
    tab(&mut h, SideTab::Environment);
    shoot(&mut h, "11-environment");
    tab(&mut h, SideTab::Config);
    shoot(&mut h, "12-config");
    tab(&mut h, SideTab::Stats);
    h.state_mut().state_mut().show_log = true;
    settle(&mut h);
    shoot(&mut h, "13-log");
    h.state_mut().state_mut().show_log = false;
    h.key_press_modifiers(Modifiers::COMMAND, Key::K);
    settle(&mut h);
    shoot(&mut h, "14-palette");
    h.key_press(Key::Escape);
    h.state_mut().state_mut().show_shortcuts = true;
    settle(&mut h);
    shoot(&mut h, "15-shortcuts");
    h.state_mut().state_mut().show_shortcuts = false;
    // Zoomed in on the map.
    h.state_mut().state_mut().viewport.zoom = 40.0;
    h.state_mut().state_mut().viewport.auto_fit = false;
    settle(&mut h);
    shoot(&mut h, "16-map-zoomed");
    h.state_mut().state_mut().viewport.auto_fit = true;

    // Running.
    h.state_mut()
        .state_mut()
        .set_speed(avida_gui::model::SpeedSetting {
            limited: true,
            ups: 4.0,
        });
    h.state_mut().state_mut().toggle_run();
    wait_until(&mut h, "running", |app| {
        app.state().run_state() == Some(RunState::Running)
    });
    shoot(&mut h, "17-running");
    h.state_mut()
        .state_mut()
        .set_speed(avida_gui::model::SpeedSetting::default());
    wait_until(&mut h, "the end", |app| {
        app.state().run_state() == Some(RunState::Finished)
    });
    shoot(&mut h, "18-finished");

    // Back to the start screen: recent experiments, then an error.
    h.state_mut()
        .state_mut()
        .request(avida_gui::model::Request::Close);
    settle(&mut h);
    shoot(&mut h, "19-start-recent");
    let missing = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join("gui/audit-missing");
    h.state_mut()
        .state_mut()
        .request(avida_gui::model::Request::Open(missing));
    wait_until(&mut h, "the error", |app| app.state().open_error.is_some());
    shoot(&mut h, "20-start-error");
}

#[test]
#[ignore = "a screenshot sweep for design review; run it explicitly"]
fn a_failed_run_in_both_themes_and_sizes() {
    let dir = fresh_config("audit-fatal");
    let text = std::fs::read_to_string(dir.join("events.cfg")).unwrap();
    std::fs::write(
        dir.join("events.cfg"),
        format!("{text}u 3 Inject missing.org\n"),
    )
    .unwrap();
    let mut h = harness(
        StartupArgs {
            dir: Some(dir),
            avida_args: Vec::new(),
        },
        SIZES[0].1,
    );
    wait_until(&mut h, "the world", ready);
    h.key_press(Key::Space);
    wait_until(&mut h, "the failure", |app| {
        app.state().run_state() == Some(RunState::Failed)
    });
    shoot(&mut h, "21-failed");
}
