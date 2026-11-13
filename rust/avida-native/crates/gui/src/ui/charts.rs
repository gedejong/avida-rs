//! The charts tab: time series of the accumulated rows, task counts, and
//! a histogram of the map layer.
//!
//! The time series and the task chart follow the whole run until the user
//! zooms (pinch, or Cmd/Ctrl and scroll) or drags along the update axis;
//! the time series share that axis. Double-clicking a chart or "Show the
//! whole run" goes back. The value axis always fits what is visible.
//! Hovering a chart reads the values at that update into the titles and
//! the legend instead of a floating label.

use avida_session::{LayerId, UpdateRow};
use avida_viewmodel::{Histogram, Palette};
use egui::{vec2, Color32, Id, RichText, Sense, Ui, Vec2b};
use egui_material_icons::icons;
use egui_plot::{Bar, BarChart, GridInput, GridMark, Line, Plot, PlotUi};

use crate::model::history::RowHistory;
use crate::model::layers::{genotype_name, layer_title};
use crate::model::metrics::{nice_step, y_fit, Metric, SeriesCache, SeriesKey};
use crate::model::{format, AppState, ChartTab, Request};
use crate::ui::theme::{semibold, Tokens};
use crate::ui::widgets::{self, rgba};

/// Metrics plotted at once.
const MAX_SERIES: usize = 4;
/// Histogram bins.
const BINS: usize = 24;

pub fn show(ui: &mut Ui, state: &mut AppState, cache: &mut SeriesCache) {
    ui.horizontal(|ui| {
        let mut tab = state.chart_tab;
        let opts: Vec<(ChartTab, &str, &str)> = ChartTab::ALL
            .iter()
            .map(|&c| {
                let tip = match c {
                    ChartTab::TimeSeries => "Population statistics over time",
                    ChartTab::Tasks => "Organisms performing each task over time",
                    ChartTab::Histogram => "Distribution of the map layer's values now",
                };
                (c, c.title(), tip)
            })
            .collect();
        if widgets::segmented(ui, "chart-tab", &mut tab, &opts) {
            state.chart_tab = tab;
        }
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if widgets::icon_button(
                ui,
                icons::ICON_DOWNLOAD,
                "Export statistics",
                "Save every recorded update as a CSV file",
            )
            .clicked()
            {
                state.request(Request::ExportCsv);
            }
            if widgets::icon_button(
                ui,
                icons::ICON_IMAGE,
                "Save charts as image",
                "Save the charts as they are shown as a PNG image",
            )
            .clicked()
            {
                state.request(Request::ExportCharts);
            }
        });
    });
    ui.add_space(6.0);
    match state.chart_tab {
        ChartTab::TimeSeries => time_series(ui, state, cache),
        ChartTab::Tasks => tasks(ui, state, cache),
        ChartTab::Histogram => histogram(ui, state),
    }
}

/// Grid marks at round values (1, 2 or 5 times a power of ten), at least
/// `widen` times the plot's minimum grid spacing apart; whole numbers
/// only for counts and updates.
fn round_marks(whole: bool, widen: f64) -> impl Fn(GridInput) -> Vec<GridMark> {
    move |input: GridInput| {
        let (lo, hi) = input.bounds;
        if !(input.base_step_size > 0.0 && lo.is_finite() && hi.is_finite()) {
            return Vec::new();
        }
        let step = nice_step(input.base_step_size * widen, whole);
        if (hi - lo) / step > 500.0 {
            return Vec::new();
        }
        let mut marks: Vec<GridMark> = Vec::new();
        let mut v = (lo / step).ceil() * step;
        while v <= hi {
            // Every tenth mark is a stronger line.
            let k = (v / step).round();
            let step_size = if (k / 10.0).fract().abs() < 1e-9 {
                step * 10.0
            } else {
                step
            };
            marks.push(GridMark {
                value: if v.abs() < step * 1e-9 { 0.0 } else { v },
                step_size,
            });
            v += step;
        }
        marks
    }
}

/// A plot of the run: zooms and pans along the update axis only (the
/// panel itself scrolls), double-click shows the whole run again.
fn run_plot<'a>(ui: &Ui, id: impl std::hash::Hash + std::fmt::Debug, whole: bool) -> Plot<'a> {
    let t = Tokens::of(ui);
    Plot::new(id)
        .show_background(false)
        .allow_scroll(false)
        .allow_zoom(Vec2b::new(true, false))
        .allow_drag(Vec2b::new(true, false))
        .allow_boxed_zoom(false)
        .allow_double_click_reset(true)
        .set_margin_fraction(vec2(0.0, 0.0))
        .grid_spacing(24.0..=160.0)
        .grid_color(t.text.gamma_multiply(if t.dark { 0.28 } else { 0.2 }))
        .x_grid_spacer(round_marks(true, 2.5))
        .y_grid_spacer(round_marks(whole, 1.0))
        .x_axis_formatter(|mark, range| {
            // A label at the right edge would be cut in half, and one at the
            // left edge sits under the value axis's bottom label (two
            // zeros on top of each other).
            let span = range.end() - range.start();
            if mark.value > range.end() - span * 0.025 || mark.value < range.start() + span * 0.025
            {
                String::new()
            } else {
                format::compact(mark.value)
            }
        })
        .show_y(false)
        .cursor_color(t.text_faint)
}

/// A static plot (the histogram follows the run; it is not explored by
/// dragging) with a calm grid.
fn static_plot<'a>(ui: &Ui, id: impl std::hash::Hash + std::fmt::Debug) -> Plot<'a> {
    let t = Tokens::of(ui);
    Plot::new(id)
        .show_background(false)
        .allow_scroll(false)
        .allow_zoom(false)
        .allow_drag(false)
        .allow_boxed_zoom(false)
        .allow_double_click_reset(false)
        .grid_spacing(24.0..=160.0)
        .grid_color(t.text.gamma_multiply(if t.dark { 0.28 } else { 0.2 }))
        .y_grid_spacer(round_marks(true, 1.0))
}

/// What part of the run a chart shows, as of its last frame.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
struct Shown {
    /// `None`: the whole run (following it).
    range: Option<(f64, f64)>,
}

impl Shown {
    fn of(p: &PlotUi<'_>) -> Shown {
        let b = p.plot_bounds();
        Shown {
            range: (!p.auto_bounds().x).then(|| (b.min()[0], b.max()[0])),
        }
    }
}

/// The update under the pointer in the last frame (any run chart), and
/// whether the charts were zoomed in.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
struct ChartMemory {
    hover: Option<f64>,
    zoomed: bool,
}

fn memory_id(chart: &str) -> Id {
    Id::unique(("chart-memory", chart))
}

fn load_memory(ui: &Ui, chart: &str) -> ChartMemory {
    ui.data(|d| d.get_temp(memory_id(chart)).unwrap_or_default())
}

/// Stores this frame's memory; a change shows in the titles only next
/// frame, so ask for one.
fn store_memory(ui: &Ui, chart: &str, old: ChartMemory, new: ChartMemory) {
    if old != new {
        ui.data_mut(|d| d.insert_temp(memory_id(chart), new));
        ui.ctx().request_repaint();
    }
}

/// The row nearest to update `x`.
fn row_near(history: &RowHistory, x: f64) -> Option<&UpdateRow> {
    let rows = history.rows();
    let i = rows.partition_point(|r| f64::from(r.update) < x);
    let after = rows.get(i);
    let before = i.checked_sub(1).and_then(|j| rows.get(j));
    match (before, after) {
        (Some(b), Some(a)) => {
            if x - f64::from(b.update) <= f64::from(a.update) - x {
                Some(b)
            } else {
                Some(a)
            }
        }
        (b, a) => b.or(a),
    }
}

/// The row the titles describe: the hovered one, else the newest.
fn described_row(history: &RowHistory, mem: ChartMemory) -> Option<(&UpdateRow, bool)> {
    match mem.hover.and_then(|x| row_near(history, x)) {
        Some(r) => Some((r, true)),
        None => history.latest().map(|r| (r, false)),
    }
}

/// The toolbar under the chart picker: how to explore, and the way back.
/// Returns whether "Show the whole run" was clicked.
fn zoom_bar(ui: &mut Ui, mem: ChartMemory) -> bool {
    let t = Tokens::of(ui);
    let mut reset = false;
    ui.horizontal(|ui| {
        ui.set_min_height(24.0);
        if mem.zoomed {
            reset = widgets::icon_text_button(
                ui,
                icons::ICON_ZOOM_OUT_MAP,
                "Show the whole run",
                "Zoom out to every recorded update (or double-click a chart)",
            )
            .clicked();
        } else {
            ui.label(
                RichText::new(format!(
                    "Drag sideways to move along the run. Pinch, or hold {} and scroll, to zoom.",
                    format::command_key()
                ))
                .small()
                .color(t.text_weak),
            );
        }
    });
    reset
}

/// Draws the data of one run chart: picks the points for the part of the
/// run shown, fits the value axis to them, and records the hovered
/// update.
fn draw_run_series(
    p: &mut PlotUi<'_>,
    reset: bool,
    fit: impl FnOnce(&mut PlotUi<'_>, Option<(f64, f64)>) -> (f64, f64),
    hover: &mut Option<f64>,
) -> Shown {
    if reset {
        p.set_auto_bounds(Vec2b::new(true, false));
    }
    let shown = if reset {
        Shown::default()
    } else {
        Shown::of(p)
    };
    let (lo, hi) = fit(p, shown.range);
    p.set_plot_bounds_y(lo..=hi);
    if p.response().contains_pointer() {
        if let Some(c) = p.pointer_coordinate() {
            *hover = Some(c.x);
        }
    }
    shown
}

fn no_rows(ui: &mut Ui) {
    widgets::empty_state(
        ui,
        icons::ICON_SHOW_CHART,
        "No finished updates yet",
        "Step or run the simulation. Every finished update adds a point.",
    );
}

fn metric_chips(ui: &mut Ui, state: &mut AppState) {
    let t = Tokens::of(ui);
    ui.horizontal_wrapped(|ui| {
        ui.spacing_mut().item_spacing = vec2(4.0, 4.0);
        for m in Metric::ALL {
            let on = state.chart_metrics.contains(&m);
            let text = if on {
                RichText::new(m.short()).color(t.accent).family(semibold())
            } else {
                RichText::new(m.short()).color(t.text_weak)
            };
            let mut b = egui::Button::selectable(on, text.size(12.0)).corner_radius(10.0);
            if on {
                b = b
                    .fill(t.accent_soft)
                    .stroke(egui::Stroke::new(1.0, t.accent.gamma_multiply(0.6)));
            }
            let r = ui.add(b).on_hover_text(m.description());
            if r.clicked() {
                if on {
                    if state.chart_metrics.len() > 1 {
                        state.chart_metrics.retain(|&x| x != m);
                    }
                } else {
                    state.chart_metrics.push(m);
                    if state.chart_metrics.len() > MAX_SERIES {
                        state.chart_metrics.remove(0);
                    }
                }
            }
        }
    });
}

fn time_series(ui: &mut Ui, state: &mut AppState, cache: &mut SeriesCache) {
    let t = Tokens::of(ui);
    metric_chips(ui, state);
    ui.add_space(4.0);
    if state.history.is_empty() {
        no_rows(ui);
        return;
    }
    let mem = load_memory(ui, "time-series");
    let reset = zoom_bar(ui, mem);
    let metrics = state.chart_metrics.clone();
    let n = metrics.len().max(1) as f32;
    // Per chart: its title row, two item gaps and the space after it; the
    // x axis (ticks and title) is drawn only under the last chart, outside
    // the plot height.
    let title_h = ui.text_style_height(&egui::TextStyle::Body);
    let overhead = title_h + 2.0 * ui.spacing().item_spacing.y + 4.0;
    let x_axis = 40.0;
    let each = ((ui.available_height() - x_axis - 4.0) / n - overhead).max(90.0);
    let width = ui.available_width();
    let max_points = (width * 1.5) as usize;
    let group = Id::unique("time-series-link");
    let described = described_row(&state.history, mem).map(|(r, h)| (r.clone(), h));
    let mut hover = None;
    let mut zoomed = mem.zoomed;
    let history = &state.history;
    let area = ui
        .scope(|ui| {
            for (i, m) in metrics.iter().enumerate() {
                ui.horizontal(|ui| {
                    ui.label(RichText::new(m.label()).family(semibold()).color(t.text));
                    if let Some((row, hovered)) = &described {
                        ui.label(
                            RichText::new(format::metric(m.value(row), m.is_count()))
                                .color(t.text_weak),
                        );
                        if *hovered {
                            ui.label(
                                RichText::new(format!(
                                    "at update {}",
                                    format::count(i64::from(row.update))
                                ))
                                .small()
                                .color(t.text_faint),
                            );
                        }
                    }
                });
                let is_count = m.is_count();
                let mut plot = run_plot(ui, ("ts", *m), is_count)
                    .height(each)
                    .width(width)
                    .link_axis(group, [true, false])
                    .link_cursor(group, [true, false])
                    .y_axis_min_width(44.0)
                    .y_axis_formatter(move |mark, _| format::metric(mark.value, is_count));
                if i + 1 == metrics.len() {
                    plot = plot.x_axis_label("update");
                } else {
                    plot = plot.show_axes([false, true]);
                }
                let key = SeriesKey::Metric(*m);
                let shown = plot
                    .show(ui, |p| {
                        draw_run_series(
                            p,
                            reset,
                            |p, range| {
                                let pts = cache.get_range(history, key, max_points, range).to_vec();
                                let fit = y_fit(&pts, range, is_count);
                                p.line(
                                    Line::new(m.label(), pts)
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
        })
        .response
        .rect;
    // Only what is on screen (the tab scrolls).
    state.chart_area = Some(area.intersect(ui.clip_rect()));
    store_memory(ui, "time-series", mem, ChartMemory { hover, zoomed });
}

/// Colours for the tasks, in their order (NOT .. EQU, by logical
/// complexity), so an ordered palette reads naturally. The end that
/// disappears into the background is cut: the dark end in dark mode, the
/// light end in light mode.
pub fn task_color(k: usize, n: usize, dark: bool) -> Color32 {
    let (from, to) = if dark { (0.22, 1.0) } else { (0.0, 0.82) };
    let pos = if n <= 1 {
        0.5
    } else {
        from + k as f64 / (n - 1) as f64 * (to - from)
    };
    rgba(Palette::Viridis.sample(pos))
}

fn hidden_tasks_id() -> Id {
    Id::unique("hidden-tasks")
}

fn task_names(state: &AppState) -> Vec<String> {
    state
        .frame
        .as_ref()
        .map(|f| {
            f.env
                .tasks
                .iter()
                .map(|t| {
                    if t.desc.is_empty() {
                        t.name.clone()
                    } else {
                        t.desc.clone()
                    }
                })
                .collect()
        })
        .unwrap_or_default()
}

/// The legend of the task chart: one chip per task, in task order, with
/// its count at the described update. Clicking a chip hides or shows its
/// line; hovering one brings its line forward. Returns the hovered task.
fn task_legend(
    ui: &mut Ui,
    names: &[String],
    counts: Option<&[i32]>,
    hidden: &mut Vec<bool>,
) -> Option<usize> {
    let t = Tokens::of(ui);
    let mut hovered = None;
    ui.horizontal_wrapped(|ui| {
        ui.spacing_mut().item_spacing = vec2(10.0, 4.0);
        for (k, name) in names.iter().enumerate() {
            let off = hidden.get(k).copied().unwrap_or(false);
            let color = task_color(k, names.len(), t.dark);
            let count = counts
                .and_then(|c| c.get(k))
                .map(|&c| format::count(i64::from(c)))
                .unwrap_or_default();
            // Painted as one widget, so a click anywhere on it toggles.
            let font = egui::FontId::proportional(12.5);
            let painter = ui.painter();
            let name_color = if off { t.text_faint } else { t.text };
            let name_g = painter.layout_no_wrap(name.clone(), font.clone(), name_color);
            let count_g = painter.layout_no_wrap(count.clone(), font, t.text_weak);
            let gap = if count.is_empty() { 0.0 } else { 5.0 };
            let size = vec2(
                10.0 + 5.0 + name_g.size().x + gap + count_g.size().x + 4.0,
                name_g.size().y.max(16.0) + 4.0,
            );
            let (rect, r) = ui.allocate_exact_size(size, Sense::click());
            let painter = ui.painter();
            if r.hovered() {
                painter.rect_filled(rect.expand(2.0), 4.0, t.control);
            }
            let cy = rect.center().y;
            let swatch =
                egui::Rect::from_center_size(egui::pos2(rect.left() + 5.0, cy), vec2(10.0, 10.0));
            if off {
                painter.rect_stroke(
                    swatch,
                    3.0,
                    egui::Stroke::new(1.2, t.text_faint),
                    egui::StrokeKind::Inside,
                );
            } else {
                painter.rect_filled(swatch, 3.0, color);
            }
            let x = swatch.right() + 5.0;
            let nw = name_g.size().x;
            painter.galley(
                egui::pos2(x, cy - name_g.size().y / 2.0),
                name_g,
                name_color,
            );
            painter.galley(
                egui::pos2(x + nw + gap, cy - count_g.size().y / 2.0),
                count_g,
                t.text_weak,
            );
            let label = format!("Show {name} in the task chart");
            r.widget_info(|| egui::WidgetInfo::selected(egui::Role::CheckBox, true, !off, &label));
            let r = r.on_hover_text(if off {
                "Click to show this task"
            } else {
                "Click to hide this task"
            });
            if r.hovered() {
                hovered = Some(k);
                ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
            }
            if r.has_focus() {
                ui.painter().rect_stroke(
                    r.rect.expand(2.0),
                    4.0,
                    egui::Stroke::new(1.5, t.accent),
                    egui::StrokeKind::Outside,
                );
            }
            if r.clicked() {
                if hidden.len() < names.len() {
                    hidden.resize(names.len(), false);
                }
                hidden[k] = !off;
            }
        }
    });
    hovered
}

fn tasks(ui: &mut Ui, state: &mut AppState, cache: &mut SeriesCache) {
    let t = Tokens::of(ui);
    if state.history.is_empty() {
        no_rows(ui);
        return;
    }
    let names = task_names(state);
    if names.is_empty() {
        widgets::empty_state(
            ui,
            icons::ICON_SCIENCE,
            "No tasks",
            "This environment rewards no tasks.",
        );
        return;
    }
    let mem = load_memory(ui, "tasks");
    let reset = zoom_bar(ui, mem);
    let described = described_row(&state.history, mem).map(|(r, h)| (r.clone(), h));
    let area = ui
        .scope(|ui| {
            // Newest first: stops at the first row with a task.
            let none_yet = !state
                .history
                .rows()
                .iter()
                .rev()
                .any(|r| r.task_last_count.iter().any(|&c| c > 0));
            let caption = match &described {
                _ if none_yet => {
                    "No organism has performed a task yet. Each line rises as organisms learn its task."
                        .to_string()
                }
                Some((row, true)) => format!(
                    "Organisms that performed each task in their last gestation, at update {}",
                    format::count(i64::from(row.update))
                ),
                _ => "Organisms that performed each task in their last gestation".to_string(),
            };
            ui.label(RichText::new(caption).color(t.text_weak));
            let mut hidden: Vec<bool> = ui.data(|d| d.get_temp(hidden_tasks_id()).unwrap_or_default());
            let before = hidden.clone();
            let focus = task_legend(
                ui,
                &names,
                described.as_ref().map(|(r, _)| r.task_last_count.as_slice()),
                &mut hidden,
            );
            if hidden != before {
                ui.data_mut(|d| d.insert_temp(hidden_tasks_id(), hidden.clone()));
            }
            ui.add_space(4.0);
            let width = ui.available_width();
            let max_points = (width * 1.5) as usize;
            let n = names.len();
            let history = &state.history;
            let mut hover = None;
            let shown = run_plot(ui, "task-plot", true)
                .height((ui.available_height() - 8.0).max(200.0))
                .x_axis_label("update")
                .y_axis_min_width(40.0)
                .y_axis_formatter(|mark, _| format::metric(mark.value, true))
                .show(ui, |p| {
                    draw_run_series(
                        p,
                        reset,
                        |p, range| {
                            let mut all: Vec<[f64; 2]> = Vec::new();
                            let mut lines = Vec::new();
                            for k in 0..n {
                                if hidden.get(k).copied().unwrap_or(false) {
                                    continue;
                                }
                                let pts = cache
                                    .get_range(history, SeriesKey::Task(k), max_points, range)
                                    .to_vec();
                                all.extend_from_slice(&pts);
                                lines.push((k, pts));
                            }
                            // The focused line last, so it is drawn on top.
                            lines.sort_by_key(|(k, _)| Some(*k) == focus);
                            for (k, pts) in lines {
                                let mut color = task_color(k, n, t.dark);
                                let mut width = 1.6;
                                if let Some(f) = focus {
                                    if f == k {
                                        width = 2.6;
                                    } else {
                                        color = color.gamma_multiply(0.3);
                                    }
                                }
                                p.line(Line::new(names[k].clone(), pts).color(color).width(width));
                            }
                            y_fit(&all, range, true)
                        },
                        &mut hover,
                    )
                })
                .inner;
            store_memory(
                ui,
                "tasks",
                mem,
                ChartMemory {
                    hover,
                    zoomed: shown.range.is_some(),
                },
            );
        })
        .response
        .rect;
    // Only what is on screen (the tab scrolls).
    state.chart_area = Some(area.intersect(ui.clip_rect()));
}

fn histogram(ui: &mut Ui, state: &mut AppState) {
    let t = Tokens::of(ui);
    let Some(frame) = state.frame.clone() else {
        no_rows(ui);
        return;
    };
    let layer = state.map.layer;
    let title = layer_title(&frame.env, layer);
    let area = ui
        .scope(|ui| {
            ui.label(
                RichText::new(title.clone())
                    .family(semibold())
                    .color(t.text),
            );
            let Some(map) = frame.map.as_ref() else {
                ui.label(RichText::new("No map has been captured yet.").color(t.text_weak));
                return;
            };
            let height = (ui.available_height() - 40.0).max(200.0);
            if layer == LayerId::Genotype {
                ui.label(
                    RichText::new("Organisms of the most abundant genotypes").color(t.text_weak),
                );
                let rows: Vec<_> = frame.genotypes.iter().take(20).collect();
                let bars: Vec<Bar> = rows
                    .iter()
                    .enumerate()
                    .map(|(i, g)| {
                        Bar::new(i as f64, f64::from(g.abundance))
                            .width(0.8)
                            .name(genotype_name(&frame, g.id))
                            .fill(rgba(Palette::OkabeIto.category(map.genotype_slot(g.id))))
                    })
                    .collect();
                let names: Vec<String> = rows.iter().map(|g| genotype_name(&frame, g.id)).collect();
                static_plot(ui, "genotype-bars")
                    .height(height)
                    .show_grid([false, true])
                    .y_axis_min_width(40.0)
                    .x_axis_formatter(move |mark, _| {
                        let i = mark.value.round();
                        if (mark.value - i).abs() < 1e-6 && i >= 0.0 {
                            names.get(i as usize).cloned().unwrap_or_default()
                        } else {
                            String::new()
                        }
                    })
                    .show(ui, |p| p.bar_chart(BarChart::new("abundance", bars)));
                return;
            }
            let Some(values) = map.layer(layer) else {
                ui.label(RichText::new("Waiting for this layer's values.").color(t.text_weak));
                return;
            };
            let scale = state.map.scale(layer);
            let hist = Histogram::new(values, BINS, scale);
            let edges = hist.bin_edges();
            let bars: Vec<Bar> = hist
                .counts
                .iter()
                .enumerate()
                .map(|(i, &c)| {
                    let lo = edges[i];
                    let hi = edges[i + 1];
                    Bar::new(i as f64 + 0.5, f64::from(c))
                        .width(0.92)
                        .name(format!("{} to {}", format::real(lo), format::real(hi)))
                        .fill(t.accent.gamma_multiply(0.85))
                })
                .collect();
            let edge_labels = edges.clone();
            static_plot(ui, "layer-histogram")
                .height(height)
                .show_grid([false, true])
                .y_axis_min_width(40.0)
                .x_axis_label(title.clone())
                .x_axis_formatter(move |mark, _| {
                    let i = mark.value.round();
                    if (mark.value - i).abs() < 1e-6 && i >= 0.0 && (i as usize) < edge_labels.len()
                    {
                        format::real(edge_labels[i as usize])
                    } else {
                        String::new()
                    }
                })
                .show(ui, |p| p.bar_chart(BarChart::new("cells", bars)));
            let empty = format::count(hist.skipped as i64);
            ui.label(
                RichText::new(format!(
                    "{} cells counted, {empty} empty or out of range.{}",
                    format::count(hist.total() as i64),
                    if hist.scale == avida_viewmodel::Scale::Log {
                        " The bins are logarithmic."
                    } else {
                        ""
                    }
                ))
                .small()
                .color(t.text_weak),
            );
        })
        .response
        .rect;
    // Only what is on screen (the tab scrolls).
    state.chart_area = Some(area.intersect(ui.clip_rect()));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_marks_are_round_and_whole_for_counts() {
        let marks = round_marks(true, 1.0)(GridInput {
            bounds: (0.0, 7.4),
            base_step_size: 1.18,
        });
        let values: Vec<f64> = marks.iter().map(|m| m.value).collect();
        assert_eq!(values, vec![0.0, 2.0, 4.0, 6.0]);
        let marks = round_marks(true, 1.0)(GridInput {
            bounds: (0.0, 3.0),
            base_step_size: 0.2,
        });
        assert!(marks.iter().all(|m| m.value.fract() == 0.0), "{marks:?}");
        assert!(round_marks(false, 1.0)(GridInput {
            bounds: (0.0, 1.0),
            base_step_size: 0.0,
        })
        .is_empty());
    }

    #[test]
    fn task_colours_stay_off_the_background_end() {
        // The darkest task colour in dark mode is not viridis' near-black
        // purple, the lightest in light mode not its pale yellow.
        let dark_first = task_color(0, 9, true);
        let light_last = task_color(8, 9, false);
        assert_ne!(dark_first, rgba(Palette::Viridis.sample(0.0)));
        assert_ne!(light_last, rgba(Palette::Viridis.sample(1.0)));
    }
}
