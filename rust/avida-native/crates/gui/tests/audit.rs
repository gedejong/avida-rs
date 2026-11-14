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
use egui_kittest::kittest::Queryable;

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
            ..Default::default()
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

/// The open-endedness view: an events file without either action, then one
/// with both, before the first MODES row and with rows.
#[test]
#[ignore = "a screenshot sweep for design review; run it explicitly"]
fn open_endedness_in_both_themes_and_sizes() {
    let start = |name: &str, extra: &str| {
        let dir = fresh_config(name);
        let text = std::fs::read_to_string(dir.join("events.cfg")).unwrap();
        // Before the `Exit` line, so that the actions fire.
        std::fs::write(dir.join("events.cfg"), format!("{extra}{text}")).unwrap();
        let mut h = harness(
            StartupArgs {
                dir: Some(dir),
                avida_args: Vec::new(),
                ..Default::default()
            },
            SIZES[0].1,
        );
        wait_until(&mut h, "the world", ready);
        h.state_mut().state_mut().side_tab = SideTab::Charts;
        h.state_mut().state_mut().chart_tab = ChartTab::OpenEnded;
        h
    };
    let reach = |h: &mut H, update: i32| {
        h.state_mut().state_mut().run_to(update);
        wait_until(h, "the update", |app| {
            let s = app.state();
            s.run_state() == Some(RunState::Paused)
                && s.history.latest().is_some_and(|r| r.update >= update)
        });
        settle(h);
    };

    let mut h = start("audit-oe-none", "");
    reach(&mut h, 10);
    shoot(&mut h, "22-open-ended-none");
    drop(h);

    let events =
        "u 0:1:end PrintMODESData modes.dat 1 2\nu 0:5:end PrintPhylogenyMetrics phylogeny.dat\n";
    let mut h = start("audit-oe", events);
    reach(&mut h, 3);
    shoot(&mut h, "23-open-ended-waiting");
    reach(&mut h, 100);
    shoot(&mut h, "24-open-ended-rows");
    // Tall enough to see the phylogeny charts below the MODES ones.
    h.set_size(vec2(1440.0, 2400.0));
    for (theme_name, theme) in [("dark", ThemeChoice::Dark), ("light", ThemeChoice::Light)] {
        h.state_mut().settings_mut().theme = theme;
        settle(&mut h);
        if let Ok(img) = h.render() {
            img.save(audit_dir().join(format!("25-open-ended-tall-{theme_name}.png")))
                .unwrap();
        }
    }
}

/// A synthetic run played back: a rich world (families that grow, tasks
/// that spread) without simulating one.
fn synthetic(name: &str, spec: &common::synth::Synth) -> H {
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

/// The linked filtering: the chart views without and with a filter, the
/// dimmed map, the scatter plot, the form and the chips.
#[test]
#[ignore = "a screenshot sweep for design review; run it explicitly"]
fn chart_filtering_in_both_themes_and_sizes() {
    use avida_gui::model::filtering::{Comparison, DraftKind};
    use avida_session::LayerId;
    use avida_viewmodel::{Condition, Prop};
    let spec = common::synth::Synth::default();
    let mut h = synthetic("audit-filter", &spec);
    let last = spec.frames - 1;
    seek(&mut h, last);
    tab(&mut h, SideTab::Charts);

    // Without a filter.
    for (c, name) in [
        (ChartTab::TimeSeries, "time-series"),
        (ChartTab::Tasks, "tasks"),
        (ChartTab::Histogram, "histogram"),
        (ChartTab::Scatter, "scatter"),
    ] {
        h.state_mut().state_mut().chart_tab = c;
        if c == ChartTab::Histogram {
            h.state_mut().state_mut().set_layer(LayerId::Fitness);
        }
        settle(&mut h);
        shoot(&mut h, &format!("30-filter-none-{name}"));
    }

    // Whole-number axes: the dots spread around their values.
    {
        let st = h.state_mut().state_mut();
        st.chart_tab = ChartTab::Scatter;
        st.brush.scatter.x = Prop::TaskCount;
        st.brush.scatter.log_y = false;
        st.brush.scatter.y = Prop::Generation;
        st.sync_subscription();
    }
    wait_until(&mut h, "the task count layer", |app| {
        app.state()
            .frame
            .as_ref()
            .and_then(|f| f.map.as_ref())
            .is_some_and(|m| m.layer(Prop::TaskCount.layer()).is_some())
    });
    settle(&mut h);
    shoot(&mut h, "30-filter-none-scatter-whole");
    {
        let st = h.state_mut().state_mut();
        st.brush.scatter.x = Prop::GenomeLen;
        st.brush.scatter.y = Prop::Fitness;
        st.brush.scatter.log_y = true;
    }

    // With a filter: a genome length range.
    h.state_mut().state_mut().edit_filter(|f| {
        f.set(Condition::range(Prop::GenomeLen, 80.0, 105.0));
    });
    for (c, name) in [
        (ChartTab::TimeSeries, "time-series"),
        (ChartTab::Tasks, "tasks"),
        (ChartTab::Histogram, "histogram"),
        (ChartTab::Scatter, "scatter"),
    ] {
        h.state_mut().state_mut().chart_tab = c;
        settle(&mut h);
        wait_until(&mut h, "the scan", |app| !app.state().brush.scanning());
        shoot(&mut h, &format!("31-filter-length-{name}"));
    }

    // Three conditions: a task, a region, a length; the layer fitness.
    h.state_mut().state_mut().edit_filter(|f| {
        f.set(Condition::Task {
            task: 0,
            performs: true,
        });
        f.set(Condition::region((5, 5), (45, 45)));
    });
    h.state_mut().state_mut().chart_tab = ChartTab::Histogram;
    settle(&mut h);
    shoot(&mut h, "32-filter-three-histogram");
    h.state_mut().state_mut().chart_tab = ChartTab::Tasks;
    settle(&mut h);
    shoot(&mut h, "32-filter-three-tasks");
    h.state_mut().state_mut().chart_tab = ChartTab::Scatter;
    settle(&mut h);
    shoot(&mut h, "32-filter-three-scatter");

    // The genotype and family bar charts under a filter.
    h.state_mut().state_mut().chart_tab = ChartTab::Histogram;
    for (l, name) in [
        (LayerId::Genotype, "genotype"),
        (LayerId::Ancestry, "ancestry"),
    ] {
        h.state_mut().state_mut().set_layer(l);
        settle(&mut h);
        shoot(&mut h, &format!("32-filter-three-bars-{name}"));
    }

    // The "Add condition" form.
    h.state_mut().state_mut().brush.draft.kind = DraftKind::Prop(Prop::Merit);
    h.state_mut().state_mut().brush.draft.comparison = Comparison::Between;
    h.get_by_label("Add condition").click();
    settle(&mut h);
    shoot_open(&mut h, "33-filter-form");
    h.key_press(Key::Escape);
    settle(&mut h);

    // Region tool on, and the genotype map under the filter.
    h.state_mut().state_mut().brush.region_mode = true;
    h.state_mut().state_mut().set_layer(LayerId::Genotype);
    settle(&mut h);
    shoot(&mut h, "34-filter-region-tool");
    h.state_mut().state_mut().brush.region_mode = false;
}

/// Like [`shoot`] but keeps popups open (a theme change would close them).
fn shoot_open(h: &mut H, name: &str) {
    for (size_name, size) in SIZES {
        h.set_size(size);
        settle(h);
        let path = audit_dir().join(format!("{name}-{size_name}-dark.png"));
        if let Ok(img) = h.render() {
            img.save(&path).unwrap();
        }
    }
    h.set_size(SIZES[0].1);
}

/// The filtering states the first sweep does not reach: the form for each
/// kind of condition, a long row of chips, a histogram with a range, the
/// time series zoomed in, and a filter that matches nothing.
#[test]
#[ignore = "a screenshot sweep for design review; run it explicitly"]
fn chart_filtering_edge_states() {
    use avida_gui::model::filtering::{Comparison, DraftKind};
    use avida_session::LayerId;
    use avida_viewmodel::{Condition, Prop};
    let spec = common::synth::Synth::default();
    let mut h = synthetic("audit-filter-edges", &spec);
    seek(&mut h, spec.frames - 1);
    tab(&mut h, SideTab::Charts);
    h.state_mut().state_mut().chart_tab = ChartTab::Histogram;
    h.state_mut().state_mut().set_layer(LayerId::Ancestry);
    settle(&mut h);

    // The form for each kind.
    let kinds = [
        ("task", DraftKind::Performs),
        ("not-task", DraftKind::NotPerforms),
        ("at-least", DraftKind::Prop(Prop::Merit)),
        ("genotype-none", DraftKind::Genotype),
        ("family", DraftKind::Family),
    ];
    h.get_by_label("Add condition").click();
    settle(&mut h);
    for (name, kind) in kinds {
        let st = h.state_mut().state_mut();
        st.brush.draft.kind = kind;
        st.brush.draft.comparison = Comparison::AtLeast;
        settle(&mut h);
        shoot_open(&mut h, &format!("35-form-{name}"));
    }
    h.key_press(Key::Escape);
    settle(&mut h);
    assert!(!h.state().state().brush.form_open, "Escape closes the form");

    // The bar charts, labelled.
    shoot(&mut h, "35-bars-ancestry");
    h.state_mut().state_mut().set_layer(LayerId::Genotype);
    settle(&mut h);
    shoot(&mut h, "35-bars-genotype");

    // A histogram with a range.
    h.state_mut().state_mut().set_layer(LayerId::GenomeLen);
    h.state_mut().state_mut().edit_filter(|f| {
        f.set(Condition::range(Prop::GenomeLen, 70.0, 100.0));
    });
    wait_until(&mut h, "the scan", |app| !app.state().brush.scanning());
    shoot(&mut h, "36-histogram-range");

    // The time series, zoomed in.
    h.state_mut().state_mut().chart_tab = ChartTab::TimeSeries;
    settle(&mut h);
    h.hover_at(egui::pos2(1200.0, 520.0));
    h.step();
    for _ in 0..3 {
        h.event(egui::Event::MouseWheel {
            unit: egui::MouseWheelUnit::Point,
            delta: vec2(0.0, 60.0),
            phase: egui::TouchPhase::Move,
            source: egui::MouseWheelSource::Wheel,
            modifiers: Modifiers::COMMAND,
        });
        h.step();
    }
    settle(&mut h);
    shoot(&mut h, "37-time-series-zoomed");

    // Six chips.
    h.state_mut().state_mut().edit_filter(|f| {
        f.set(Condition::Task {
            task: 0,
            performs: true,
        });
        f.set(Condition::Task {
            task: 3,
            performs: false,
        });
        f.set(Condition::range(Prop::Fitness, 0.5, f64::INFINITY));
        f.set(Condition::range(Prop::Age, f64::NEG_INFINITY, 40.0));
        f.set(Condition::region((5, 5), (45, 45)));
    });
    settle(&mut h);
    shoot(&mut h, "38-six-chips-time-series");
    h.state_mut().state_mut().chart_tab = ChartTab::Tasks;
    settle(&mut h);
    shoot(&mut h, "38-six-chips-tasks");

    // A filter that matches nothing.
    h.state_mut().state_mut().clear_filter();
    h.state_mut().state_mut().edit_filter(|f| {
        f.set(Condition::range(Prop::GenomeLen, 1000.0, 2000.0));
    });
    wait_until(&mut h, "the scan", |app| !app.state().brush.scanning());
    for (c, name) in [
        (ChartTab::TimeSeries, "time-series"),
        (ChartTab::Tasks, "tasks"),
        (ChartTab::Histogram, "histogram"),
        (ChartTab::Scatter, "scatter"),
    ] {
        h.state_mut().state_mut().chart_tab = c;
        settle(&mut h);
        shoot(&mut h, &format!("39-empty-{name}"));
    }
    h.state_mut().state_mut().set_layer(LayerId::Genotype);
    h.state_mut().state_mut().chart_tab = ChartTab::Histogram;
    settle(&mut h);
    shoot(&mut h, "39-empty-genotype-bars");
}
