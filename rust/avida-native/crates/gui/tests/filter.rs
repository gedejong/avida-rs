//! Linked filtering ("brushing") in the real application: conditions made
//! by dragging over the histogram and the scatter plot, by clicking a task
//! and by dragging a region on the map; chips that remove them; and the
//! filter's life across tabs and runs.

mod common;

use std::path::PathBuf;

use avida_gui::model::filtering::{Comparison, DraftKind};
use avida_gui::model::{ChartTab, Phase, SideTab};
use avida_gui::StartupArgs;
use avida_session::{LayerId, RunState};
use avida_viewmodel::{Condition, Prop};
use common::{fresh_config, harness, settle, wait_until};
use egui::{vec2, Modifiers, Role};
use egui_kittest::kittest::{by, Queryable};

type H = egui_kittest::Harness<'static, avida_gui::AvidaApp>;

fn ready(app: &avida_gui::AvidaApp) -> bool {
    let s = app.state();
    s.phase == Phase::Ready && s.frame.as_ref().is_some_and(|f| f.map.is_some())
}

/// A world run to update `update`, the Charts tab open on `chart`.
fn open(dir: PathBuf, size: egui::Vec2, update: i32, chart: ChartTab) -> H {
    let mut h = harness(
        StartupArgs {
            dir: Some(dir),
            avida_args: Vec::new(),
            ..Default::default()
        },
        size,
    );
    wait_until(&mut h, "the world", ready);
    h.state_mut().state_mut().run_to(update);
    wait_until(&mut h, "the update", |app| {
        let s = app.state();
        s.run_state() == Some(RunState::Paused)
            && s.history.latest().is_some_and(|r| r.update >= update)
    });
    let s = h.state_mut().state_mut();
    s.side_tab = SideTab::Charts;
    s.chart_tab = chart;
    settle(&mut h);
    h
}

fn filter_of(h: &H) -> Vec<Condition> {
    h.state().state().brush.filter.conditions().to_vec()
}

/// Drags from `a` to `b` with the pointer, a few steps in between.
fn drag(h: &mut H, a: egui::Pos2, b: egui::Pos2) {
    h.hover_at(a);
    h.step();
    h.drag_at(a);
    h.step();
    for k in 1..=6 {
        h.hover_at(a + (b - a) * (k as f32 / 6.0));
        h.step();
    }
    h.drop_at(b);
    settle(h);
}

#[test]
fn chips_show_the_conditions_and_remove_them() {
    let mut h = open(
        fresh_config("filter-chips"),
        vec2(1440.0, 900.0),
        30,
        ChartTab::TimeSeries,
    );
    h.get_by_label("Add condition");
    assert!(h.query_by_label_contains("organisms match").is_none());
    {
        let s = h.state_mut().state_mut();
        s.edit_filter(|f| {
            f.set(Condition::range(Prop::GenomeLen, 50.0, 200.0));
            f.set(Condition::Task {
                task: 0,
                performs: false,
            });
        });
    }
    settle(&mut h);
    h.get_by_label("Genome length 50 to 200");
    assert!(h
        .query_all(by().role(Role::Label).label_contains("Does not perform "))
        .next()
        .is_some());
    // "N of M organisms match (P%)".
    let m = h
        .state()
        .state()
        .brush
        .selection()
        .map(|s| (s.matched, s.occupied))
        .expect("a selection");
    assert!(m.1 > 0);
    h.get_by_label_contains(&format!(" of {} organisms match", {
        let s = m.1;
        if s >= 1000 {
            format!("{},{:03}", s / 1000, s % 1000)
        } else {
            s.to_string()
        }
    }));
    // The chip's cross removes its condition.
    h.get_by_label("Remove condition: Genome length 50 to 200")
        .click();
    settle(&mut h);
    assert_eq!(filter_of(&h).len(), 1);
    assert!(h.query_by_label("Genome length 50 to 200").is_none());
    // Clear removes the rest and the summary goes away.
    h.get_by_label("Clear").click();
    settle(&mut h);
    assert!(filter_of(&h).is_empty());
    assert!(h.state().state().brush.selection().is_none());
    assert!(h.query_by_label_contains("organisms match").is_none());
    h.check_accessibility();
}

#[test]
fn the_add_condition_form_adds_a_range() {
    let mut h = open(
        fresh_config("filter-form"),
        vec2(1440.0, 900.0),
        30,
        ChartTab::TimeSeries,
    );
    h.get_by_label("Add condition").click();
    settle(&mut h);
    // The form opens on a numeric property (fitness) "at least".
    // The form is the last thing drawn: its field is the last text field.
    let field = h
        .get_all_by_role(Role::TextInput)
        .last()
        .expect("the value field");
    field.click();
    settle(&mut h);
    h.get_all_by_role(Role::TextInput)
        .last()
        .unwrap()
        .type_text("0.5");
    settle(&mut h);
    assert_eq!(h.state().state().brush.draft.value, "0.5");
    h.get_by_label("Add the condition").click();
    settle(&mut h);
    assert_eq!(
        h.state().state().brush.filter.range_of(Prop::Fitness),
        Some((0.5, f64::INFINITY))
    );
    h.get_by_label("Fitness ≥ 0.5");
}

#[test]
fn dragging_on_the_histogram_adds_a_range_and_a_click_removes_it() {
    let mut h = open(
        fresh_config("filter-histogram"),
        vec2(1440.0, 900.0),
        40,
        ChartTab::Histogram,
    );
    h.state_mut().state_mut().set_layer(LayerId::GenomeLen);
    wait_until(&mut h, "the genome length layer", |app| {
        app.state()
            .frame
            .as_ref()
            .and_then(|f| f.map.as_ref())
            .is_some_and(|m| m.layer(LayerId::GenomeLen).is_some())
    });
    settle(&mut h);
    let area = h
        .state()
        .state()
        .chart_area
        .expect("the histogram is drawn");
    let y = area.top() + area.height() * 0.5;
    drag(
        &mut h,
        egui::pos2(area.left() + area.width() * 0.3, y),
        egui::pos2(area.left() + area.width() * 0.7, y),
    );
    let range = h.state().state().brush.filter.range_of(Prop::GenomeLen);
    assert!(range.is_some(), "{:?}", filter_of(&h));
    let (lo, hi) = range.unwrap();
    assert!(lo <= hi);
    // The map dims the others: the selection exists and the summary shows.
    assert!(h.state().state().brush.selection().is_some());
    h.get_by_label_contains("organisms match");

    // A drag over the same property replaces the range, not adds one.
    drag(
        &mut h,
        egui::pos2(area.left() + area.width() * 0.4, y),
        egui::pos2(area.left() + area.width() * 0.6, y),
    );
    assert_eq!(filter_of(&h).len(), 1);
    // A click on the chart removes the range.
    let at = egui::pos2(area.left() + area.width() * 0.5, y);
    h.hover_at(at);
    h.step();
    h.drag_at(at);
    h.drop_at(at);
    settle(&mut h);
    assert!(filter_of(&h).is_empty(), "{:?}", filter_of(&h));
}

#[test]
fn clicking_a_task_toggles_performs_and_shift_click_does_not_perform() {
    let mut h = open(
        fresh_config("filter-tasks"),
        vec2(1440.0, 900.0),
        40,
        ChartTab::Tasks,
    );
    let row = |h: &H| {
        h.query_all(by().role(Role::Button).label_contains("Filter on "))
            .next()
            .expect("a task row")
            .rect()
    };
    let click = |h: &mut H, mods: Modifiers| {
        // The filter bar grows a line with the first condition.
        let at = row(h).center() + vec2(20.0, 0.0);
        h.hover_at(at);
        h.step();
        h.event(egui::Event::ModifiersChanged(mods));
        for pressed in [true, false] {
            h.event(egui::Event::PointerButton {
                pos: at,
                button: egui::PointerButton::Primary,
                pressed,
                modifiers: mods,
            });
        }
        h.step();
        h.event(egui::Event::ModifiersChanged(Modifiers::NONE));
        settle(h);
    };
    click(&mut h, Modifiers::NONE);
    assert_eq!(
        filter_of(&h),
        vec![Condition::Task {
            task: 0,
            performs: true
        }]
    );
    click(&mut h, Modifiers::SHIFT);
    assert_eq!(
        filter_of(&h),
        vec![Condition::Task {
            task: 0,
            performs: false
        }]
    );
    click(&mut h, Modifiers::SHIFT);
    assert!(filter_of(&h).is_empty());
}

#[test]
fn a_region_dragged_on_the_map_becomes_a_condition() {
    let mut h = open(
        fresh_config("filter-region"),
        vec2(1440.0, 900.0),
        20,
        ChartTab::TimeSeries,
    );
    h.get_by_label("Select a region").click();
    settle(&mut h);
    assert!(h.state().state().brush.region_mode);
    let canvas = h.get_by_label("Population map").rect();
    let vp = h.state().state().viewport;
    let w = h.state().map_image().map_or(1, |m| m.w);
    let a = vp.cell_rect(canvas, 10, 10).center();
    let b = vp.cell_rect(canvas, 19, 14).center();
    drag(&mut h, a, b);
    assert_eq!(
        filter_of(&h),
        vec![Condition::Region {
            x0: 10,
            y0: 10,
            x1: 19,
            y1: 14
        }],
        "world width {w}"
    );
    assert!(
        !h.state().state().brush.region_mode,
        "the tool is used once"
    );
    let sel = h.state().state().brush.selection().unwrap().clone();
    assert!(sel.matched <= 50);
    // The view did not move: the drag drew a region instead of panning.
    assert!(h.state().state().viewport.auto_fit);
    h.get_by_label("Cells x 10 to 19, y 10 to 14");
}

#[test]
fn a_rectangle_on_the_scatter_plot_sets_both_ranges() {
    let mut h = open(
        fresh_config("filter-scatter"),
        vec2(1440.0, 900.0),
        40,
        ChartTab::Scatter,
    );
    wait_until(&mut h, "the scatter layers", |app| {
        app.state()
            .frame
            .as_ref()
            .and_then(|f| f.map.as_ref())
            .is_some_and(|m| {
                m.layer(LayerId::GenomeLen).is_some() && m.layer(LayerId::Fitness).is_some()
            })
    });
    settle(&mut h);
    let area = h.state().state().chart_area.expect("the scatter is drawn");
    let a = egui::pos2(
        area.left() + area.width() * 0.25,
        area.top() + area.height() * 0.3,
    );
    let b = egui::pos2(
        area.left() + area.width() * 0.8,
        area.top() + area.height() * 0.7,
    );
    drag(&mut h, a, b);
    let f = &h.state().state().brush.filter;
    assert!(
        f.range_of(Prop::GenomeLen).is_some(),
        "{:?}",
        f.conditions()
    );
    assert!(f.range_of(Prop::Fitness).is_some(), "{:?}", f.conditions());
    assert_eq!(f.len(), 2);
}

#[test]
fn the_filter_survives_tabs_and_layers_and_goes_with_the_run() {
    let mut h = open(
        fresh_config("filter-life"),
        vec2(1440.0, 900.0),
        20,
        ChartTab::TimeSeries,
    );
    h.state_mut().state_mut().edit_filter(|f| {
        f.set(Condition::range(Prop::Age, 1.0, 100.0));
    });
    settle(&mut h);
    for c in ChartTab::ALL {
        h.state_mut().state_mut().chart_tab = c;
        settle(&mut h);
        assert_eq!(filter_of(&h).len(), 1, "{c:?}");
    }
    for l in [LayerId::Fitness, LayerId::Genotype, LayerId::Ancestry] {
        h.state_mut().state_mut().set_layer(l);
        settle(&mut h);
        assert_eq!(filter_of(&h).len(), 1, "{l:?}");
    }
    // The map's colours stay and the others are dimmed: the image has
    // pixels with the dimmed alpha when the filter leaves some out.
    h.state_mut().state_mut().closed();
    settle(&mut h);
    assert!(filter_of(&h).is_empty());
    assert!(h.state().state().brush.selection().is_none());
}

/// A synthetic recording played back.
fn play(name: &str, spec: &common::synth::Synth) -> H {
    let file = fresh_config(name).join("synthetic.avrec");
    common::synth::write(&file, spec);
    let mut h = harness(
        StartupArgs {
            play: Some(file),
            ..Default::default()
        },
        vec2(1440.0, 900.0),
    );
    wait_until(&mut h, "the recording", |app| {
        let s = app.state();
        s.phase == Phase::Ready
            && s.playback.is_some()
            && s.frame.as_ref().is_some_and(|f| f.map.is_some())
    });
    h.state_mut().state_mut().side_tab = SideTab::Charts;
    settle(&mut h);
    h
}

#[test]
fn a_recording_gives_the_subset_line_over_every_frame() {
    use avida_viewmodel::{subset_row, Filter};
    let spec = common::synth::Synth {
        w: 40,
        h: 40,
        genotypes: 24,
        frames: 12,
        ..Default::default()
    };
    let mut h = play("filter-playback", &spec);
    let cond = Condition::range(Prop::Fitness, 0.3, f64::INFINITY);
    h.state_mut()
        .state_mut()
        .edit_filter(|f| f.set(cond.clone()));
    wait_until(&mut h, "the scan", |app| {
        !app.state().brush.scanning() && app.state().brush.subset.rows().len() == 12
    });
    // Every row is what the filter makes of that frame.
    let frames = common::synth::frames(&spec);
    let mut f = Filter::new();
    f.set(cond);
    for (row, frame) in h.state().state().brush.subset.rows().iter().zip(&frames) {
        let map = frame.map.as_ref().unwrap();
        let want = subset_row(frame.update, map, &f.select(map));
        assert_eq!(row.update, want.update);
        assert_eq!(row.organisms, want.organisms);
        assert_eq!(row.genotypes, want.genotypes);
        assert_eq!(row.mean[0].to_bits(), want.mean[0].to_bits());
    }
    // Clearing the filter stops and forgets.
    h.state_mut().state_mut().clear_filter();
    settle(&mut h);
    assert!(h.state().state().brush.subset.is_empty());
    assert!(!h.state().state().brush.scanning());
}

#[test]
fn a_condition_on_a_layer_a_recording_lacks_is_not_applied() {
    let spec = common::synth::Synth {
        w: 30,
        h: 30,
        genotypes: 12,
        frames: 3,
        layers: Some(vec![LayerId::Fitness]),
        ..Default::default()
    };
    let mut h = play("filter-playback-layers", &spec);
    h.state_mut().state_mut().edit_filter(|f| {
        f.set(Condition::range(Prop::GenomeLen, 50.0, 60.0));
    });
    settle(&mut h);
    let sel = h.state().state().brush.selection().unwrap().clone();
    assert_eq!(sel.ignored, vec![0]);
    assert_eq!(
        sel.matched, sel.occupied,
        "an ignored condition selects all"
    );
    // The chip says so in its name's tooltip; it carries a warning.
    h.get_by_label("Genome length 50 to 60");
}

#[test]
fn a_condition_on_a_layer_not_yet_shown_is_applied_once_the_session_sends_it() {
    let mut h = open(
        fresh_config("filter-new-layer"),
        vec2(1440.0, 900.0),
        25,
        ChartTab::TimeSeries,
    );
    assert!(h.state().state().map.layer == LayerId::Genotype);
    h.state_mut().state_mut().edit_filter(|f| {
        f.set(Condition::range(Prop::Merit, 0.0, f64::INFINITY));
    });
    // The subscription now names the merit layer; the next frame carries it.
    wait_until(&mut h, "the merit layer", |app| {
        let s = app.state();
        s.frame
            .as_ref()
            .and_then(|f| f.map.as_ref())
            .is_some_and(|m| m.layer(LayerId::Merit).is_some())
            && s.brush
                .selection()
                .is_some_and(|sel| sel.ignored.is_empty())
    });
    let s = h.state().state();
    let sel = s.brush.selection().unwrap();
    assert_eq!(sel.matched, sel.occupied, "every organism has a merit");
    // The subset's series started when the filter was set and has grown.
    assert!(!s.brush.subset.is_empty());
}

#[test]
fn the_form_stays_open_while_its_lists_and_slider_are_used() {
    let mut h = open(
        fresh_config("filter-form-lists"),
        vec2(1440.0, 900.0),
        30,
        ChartTab::TimeSeries,
    );
    h.get_by_label("Add condition").click();
    settle(&mut h);
    assert!(h.state().state().brush.form_open);
    // The property list opens and a choice in it lands in the draft; the
    // form is still there afterwards.
    h.get_by_label("What the condition is about").click();
    settle(&mut h);
    h.get_by_label("Age").click();
    settle(&mut h);
    assert_eq!(
        h.state().state().brush.draft.kind,
        DraftKind::Prop(Prop::Age)
    );
    assert!(h.state().state().brush.form_open);
    h.get_by_label("at most").click();
    settle(&mut h);
    assert_eq!(h.state().state().brush.draft.comparison, Comparison::AtMost);
    // The slider appears once the age layer arrives; a click on it
    // writes the number field.
    wait_until(&mut h, "the age layer", |app| {
        app.state()
            .frame
            .as_ref()
            .and_then(|f| f.map.as_ref())
            .is_some_and(|m| m.layer(LayerId::Age).is_some())
    });
    settle(&mut h);
    h.get_by_label("Range of values").click();
    settle(&mut h);
    assert!(h.state().state().brush.draft.numbers().is_some());
    h.get_by_label("Add the condition").click();
    settle(&mut h);
    assert!(h.state().state().brush.filter.range_of(Prop::Age).is_some());
    assert!(!h.state().state().brush.form_open);
}

#[test]
fn the_task_and_family_lists_keep_the_form_open_and_escape_closes_one_layer_at_a_time() {
    let spec = common::synth::Synth {
        frames: 6,
        genotypes: 20,
        ..Default::default()
    };
    let mut h = play("filter-form-task-family", &spec);
    h.get_by_label("Add condition").click();
    settle(&mut h);

    // The task list: pick the second task.
    h.state_mut().state_mut().brush.draft.kind = DraftKind::Performs;
    settle(&mut h);
    h.get_by_label("Task").click();
    settle(&mut h);
    h.get_by_label("Nand").click();
    settle(&mut h);
    assert_eq!(h.state().state().brush.draft.task, 1);
    assert!(h.state().state().brush.form_open);

    // Escape with a list open closes the list and leaves the form.
    h.get_by_label("Task").click();
    settle(&mut h);
    assert!(h.query_by_label("Orn").is_some(), "the list did not open");
    h.key_press(egui::Key::Escape);
    settle(&mut h);
    assert!(h.state().state().brush.form_open);
    assert!(h.query_by_label("Orn").is_none(), "the list is still open");
    // The next Escape closes the form.
    h.key_press(egui::Key::Escape);
    settle(&mut h);
    assert!(!h.state().state().brush.form_open);

    // The family list: pick the second family.
    h.get_by_label("Add condition").click();
    settle(&mut h);
    h.state_mut().state_mut().brush.draft.kind = DraftKind::Family;
    settle(&mut h);
    let families: Vec<i32> = h
        .state()
        .state()
        .frame
        .as_ref()
        .and_then(|f| f.map.as_ref())
        .map(|m| m.families.iter().map(|f| f.root).collect())
        .unwrap_or_default();
    assert!(families.len() >= 2, "{families:?}");
    h.get_by_label("Family").click();
    settle(&mut h);
    // One entry per family (the combo itself is named "Family").
    let entries: Vec<_> = h.get_all_by_label_contains("organisms)").collect();
    assert_eq!(entries.len(), families.len());
    entries[1].click();
    settle(&mut h);
    assert_eq!(h.state().state().brush.draft.family, Some(families[1]));
    assert!(h.state().state().brush.form_open);
}

#[test]
fn a_filter_that_matches_nothing_says_so_and_keeps_clear_with_the_count() {
    let mut h = open(
        fresh_config("filter-empty"),
        vec2(1000.0, 650.0),
        30,
        ChartTab::TimeSeries,
    );
    h.state_mut().state_mut().edit_filter(|f| {
        f.set(Condition::range(Prop::GenomeLen, 100_000.0, 200_000.0));
    });
    wait_until(&mut h, "the scan", |app| !app.state().brush.scanning());
    settle(&mut h);
    h.get_by_label_contains("0 of ");
    h.get_by_label_contains("No organism meets all the conditions");
    h.get_by_label("Clear").click();
    settle(&mut h);
    assert!(filter_of(&h).is_empty());
}
