//! The open-endedness view of the charts tab: the MODES measures (one group
//! of small charts per `PrintMODESData` instance) and the measures of the
//! genealogy of the genotypes (`PrintPhylogenyMetrics`), one small chart per
//! measure. The charts of a group share their x axis; each has its own value
//! axis. The series come from `avida_viewmodel::open_ended`, shared with the
//! terminal interface.
//!
//! Zooming, dragging and the hover read-out are those of the time series
//! (`charts::run_plot`): hovering a chart reads the values of every chart of
//! the group at that point into their titles. The page holds titles and
//! values only; what a group or a measure means, and how to explore it, is a
//! tooltip.

use std::sync::Arc;

use avida_viewmodel::open_ended::build;
use avida_viewmodel::{OeChart, OeGroup, OeSources, OeView, PLAYBACK_NOTE};
use egui::{Id, RichText, Ui};
use egui_material_icons::icons;
use egui_plot::Line;

use super::charts::{
    draw_run_series, load_memory, run_hint, run_plot, store_memory, zoom_reset, ChartMemory,
};
use crate::model::metrics::y_fit;
use crate::model::{format, AppState};
use crate::ui::theme::{semibold, Tokens};
use crate::ui::widgets;

/// Height of one small chart's plot.
const PLOT_HEIGHT: f32 = 100.0;

/// What the cached view was built from: the frames applied so far and the
/// address of the run's set-up (a restart replaces it).
type Stamp = (u64, usize);

/// The view for the frame and events of the run, rebuilt only when a new
/// frame has arrived.
fn view_of(ui: &Ui, state: &AppState) -> Option<Arc<OeView>> {
    let frame = state.frame.as_ref()?;
    let key = Id::unique("open-ended-view");
    let setup = state.setup.as_ref();
    let stamp: Stamp = (
        state.frames_applied,
        setup.map_or(0, |s| Arc::as_ptr(s) as usize),
    );
    let cached: Option<(Stamp, Arc<OeView>)> = ui.data(|d| d.get_temp(key));
    if let Some((s, v)) = cached {
        if s == stamp {
            return Some(v);
        }
    }
    let sources = setup
        .map(|s| {
            OeSources::from_lines(
                s.events
                    .iter()
                    .map(|e| (e.action.as_str(), e.args.as_str())),
            )
        })
        .unwrap_or_default();
    let v = Arc::new(build(&frame.open_ended, &sources));
    ui.data_mut(|d| d.insert_temp(key, (stamp, Arc::clone(&v))));
    Some(v)
}

pub fn show(ui: &mut Ui, state: &mut AppState) {
    if state.is_playback() {
        widgets::empty_state(
            ui,
            icons::ICON_SHOW_CHART,
            "Not in a recording",
            PLAYBACK_NOTE,
        );
        return;
    }
    let Some(view) = view_of(ui, state) else {
        widgets::empty_state(
            ui,
            icons::ICON_SHOW_CHART,
            "No frame yet",
            "The world is being built.",
        );
        return;
    };
    let area = ui
        .scope(|ui| {
            if view.is_unconfigured() {
                unconfigured(ui, &view);
                return;
            }
            for (i, g) in view.groups.iter().enumerate() {
                let intro = view
                    .intro
                    .as_deref()
                    .filter(|_| g.title.starts_with("MODES"));
                group(ui, i, g, intro);
            }
            if let Some(n) = &view.notice {
                ui.add_space(6.0);
                notice(ui, n, &view.example_lines);
            }
        })
        .response
        .rect;
    state.chart_area = Some(area.intersect(ui.clip_rect()));
}

fn unconfigured(ui: &mut Ui, view: &OeView) {
    let t = Tokens::of(ui);
    ui.add_space(16.0);
    ui.vertical_centered(|ui| {
        ui.label(
            icons::ICON_SHOW_CHART
                .rich_text()
                .size(34.0)
                .color(t.text_faint),
        );
        ui.add_space(6.0);
        ui.label(
            RichText::new("Nothing to chart yet")
                .family(semibold())
                .size(15.0)
                .color(t.text),
        );
    });
    ui.add_space(4.0);
    if let Some(n) = &view.notice {
        notice(ui, n, &view.example_lines);
    }
}

/// The first sentence of a longer text (the rest goes in a tooltip).
fn first_sentence(text: &str) -> &str {
    match text.find(". ") {
        Some(i) => &text[..=i],
        None => text,
    }
}

/// A missing action: one sentence, with the line to add; the rest is a
/// tooltip.
fn notice(ui: &mut Ui, text: &str, lines: &[String]) {
    let t = Tokens::of(ui);
    ui.label(
        RichText::new(first_sentence(text))
            .small()
            .color(t.text_weak),
    )
    .on_hover_text(text);
    example_lines(ui, lines);
}

/// The event lines to copy, as written in the events file.
fn example_lines(ui: &mut Ui, lines: &[String]) {
    let t = Tokens::of(ui);
    ui.add_space(4.0);
    for l in lines {
        egui::Frame::new()
            .fill(t.control)
            .corner_radius(4.0)
            .inner_margin(egui::Margin::symmetric(8, 5))
            .show(ui, |ui| {
                ui.add(
                    egui::Label::new(RichText::new(l).monospace().color(t.text))
                        .selectable(true)
                        .wrap_mode(egui::TextWrapMode::Wrap),
                );
            });
        ui.add_space(3.0);
    }
}

fn group(ui: &mut Ui, index: usize, g: &OeGroup, intro: Option<&str>) {
    let t = Tokens::of(ui);
    let mut about = String::new();
    if let Some(i) = intro {
        about.push_str(i);
        about.push_str("\n\n");
    }
    about.push_str(&g.caption);
    let memory_name = format!("open-ended-{index}");
    let mem = load_memory(ui, &memory_name);
    let mut reset = false;
    ui.horizontal(|ui| {
        let title = ui.label(
            RichText::new(&g.title)
                .family(semibold())
                .size(14.0)
                .color(t.text),
        );
        if !about.trim().is_empty() {
            title.on_hover_text(about.trim());
        }
        if g.note.is_none() {
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                reset = zoom_reset(ui, mem);
            });
        }
    });
    if let Some(note) = &g.note {
        ui.label(
            RichText::new(first_sentence(note))
                .small()
                .color(t.text_weak),
        )
        .on_hover_text(note);
        ui.add_space(10.0);
        return;
    }
    let link = Id::unique(("open-ended-link", index));
    let width = ui.available_width();
    let mut hover = None;
    let mut zoomed = mem.zoomed;
    let n = g.charts.len();
    for (i, c) in g.charts.iter().enumerate() {
        let (value, hovered) = match mem.hover.and_then(|x| c.nearest(x)) {
            Some(p) => (Some(p), true),
            None => (c.latest(), false),
        };
        chart_title(ui, c, value, hovered);
        let whole = c.whole;
        let mut plot = run_plot(ui, ("oe", index, c.id), whole)
            .height(PLOT_HEIGHT)
            .width(width)
            .link_axis(link, [true, false])
            .link_cursor(link, [true, false])
            .y_axis_min_width(44.0)
            .y_axis_formatter(move |mark, _| format::metric(mark.value, whole));
        if i + 1 == n {
            plot = plot.x_axis_label(c.x_label);
        } else {
            plot = plot.show_axes([false, true]);
        }
        let pts: Vec<[f64; 2]> = c.points.iter().map(|&(x, y)| [x, y]).collect();
        let shown = plot
            .show(ui, |p| {
                draw_run_series(
                    p,
                    reset,
                    |p, range| {
                        let fit = y_fit(&pts, range, whole);
                        p.line(
                            Line::new(c.title.clone(), pts.clone())
                                .color(t.accent)
                                .width(1.8)
                                .fill(fit.0 as f32)
                                .fill_alpha(0.08),
                        );
                        fit
                    },
                    &mut hover,
                )
            })
            .inner;
        if i == 0 {
            zoomed = shown.range.is_some();
        }
        ui.add_space(4.0);
    }
    store_memory(ui, &memory_name, mem, ChartMemory { hover, zoomed });
    ui.add_space(10.0);
}

/// The title row of a chart: its name and an info mark that explain the
/// measure on hover, and the value at the pointer (or the newest value).
fn chart_title(ui: &mut Ui, c: &OeChart, value: Option<(f64, f64)>, hovered: bool) {
    let t = Tokens::of(ui);
    ui.horizontal(|ui| {
        let tip = format!("{} {}", c.explanation, run_hint());
        ui.label(RichText::new(&c.title).family(semibold()).color(t.text))
            .on_hover_text(&tip);
        ui.label(icons::ICON_INFO.rich_text().size(13.0).color(t.text_faint))
            .on_hover_text(&tip);
        if let Some((x, y)) = value {
            ui.label(RichText::new(format::metric(y, c.whole)).color(t.text_weak));
            if hovered {
                ui.label(
                    RichText::new(format!("at {} {}", c.x_label, format::compact(x)))
                        .small()
                        .color(t.text_faint),
                );
            }
        }
    });
}
