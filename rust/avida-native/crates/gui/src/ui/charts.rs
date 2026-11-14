//! The charts tab: the filter bar and five views: time series of the
//! accumulated rows, task shares and counts, a histogram of the map layer,
//! a scatter plot of two properties, and the open-endedness measures.
//!
//! The time series and the task chart follow the whole run until the user
//! zooms (pinch, or Cmd/Ctrl and scroll) or drags along the update axis;
//! the time series share that axis. Double-clicking a chart or "Show the
//! whole run" goes back. The value axis always fits what is visible.
//! Hovering a chart reads the values at that update into the titles and
//! the legend instead of a floating label. Explanations and interaction
//! hints are tooltips of the titles, never lines of the page.
//!
//! The organism filter (`model::filtering`) is drawn here in every view:
//! the matching subset is the one filter colour (`Tokens::filter`).

use avida_session::UpdateRow;
use avida_viewmodel::{task_counts, Palette, SubsetRow};
use egui::{vec2, Color32, Id, RichText, Sense, Ui, Vec2b};
use egui_material_icons::icons;
use egui_plot::{GridInput, GridMark, Line, Plot, PlotUi};

use crate::model::filtering::{has_subset_value, subset_value, task_name, Smoothing};
use crate::model::history::RowHistory;
use crate::model::metrics::{
    log_marks, nice_step, shape_points, y_fit, Metric, SeriesCache, SeriesKey,
};
use crate::model::{format, AppState, ChartTab, Request};
use crate::ui::filter_bar;
use crate::ui::theme::{semibold, Tokens};
use crate::ui::widgets::{self, rgba};

/// Metrics plotted at once.
const MAX_SERIES: usize = 4;

pub fn show(ui: &mut Ui, state: &mut AppState, cache: &mut SeriesCache) {
    // The two export buttons sit right of the picker when there is room, and
    // under it, right-aligned, when the picker (four views) fills the row.
    let mut export_below = false;
    ui.horizontal(|ui| {
        let mut tab = state.chart_tab;
        let opts: Vec<(ChartTab, &str, &str)> = ChartTab::ALL
            .iter()
            .map(|&c| {
                let tip = match c {
                    ChartTab::TimeSeries => "Population statistics over time",
                    ChartTab::Tasks => "Organisms performing each task over time",
                    ChartTab::Histogram => "Distribution of the map layer's values now",
                    ChartTab::Scatter => "Two properties of every organism plotted against each other",
                    ChartTab::OpenEnded => {
                        "Open-endedness measures (MODES and the genealogy of the genotypes) over time"
                    }
                };
                (c, c.title(), tip)
            })
            .collect();
        // Five views side by side: tighter buttons so they fit the panel.
        let changed = ui
            .scope(|ui| {
                ui.spacing_mut().button_padding.x = 6.0;
                widgets::segmented(ui, "chart-tab", &mut tab, &opts)
            })
            .inner;
        if changed {
            state.chart_tab = tab;
        }
        if ui.available_width() < EXPORT_BUTTONS_WIDTH {
            export_below = true;
        } else {
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                export_buttons(ui, state);
            });
        }
    });
    if export_below {
        ui.horizontal(|ui| {
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                export_buttons(ui, state);
            });
        });
    }
    ui.add_space(4.0);
    if state.chart_tab == ChartTab::OpenEnded {
        // The measures come from the data files of the run, not from
        // organisms: nothing to filter.
        state.brush.add_open = false;
    } else {
        filter_bar::show(ui, state);
        ui.add_space(4.0);
    }
    match state.chart_tab {
        ChartTab::TimeSeries => time_series(ui, state, cache),
        ChartTab::Tasks => tasks(ui, state, cache),
        ChartTab::Histogram => super::histogram::show(ui, state),
        ChartTab::Scatter => super::scatter::show(ui, state),
        ChartTab::OpenEnded => super::open_ended::show(ui, state),
    }
}

/// Room the two export buttons need.
const EXPORT_BUTTONS_WIDTH: f32 = 72.0;

fn export_buttons(ui: &mut Ui, state: &mut AppState) {
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
}

/// Grid marks at round values (1, 2 or 5 times a power of ten), at least
/// `widen` times the plot's minimum grid spacing apart; whole numbers
/// only for counts and updates.
pub(super) fn round_marks(whole: bool, widen: f64) -> impl Fn(GridInput) -> Vec<GridMark> {
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
pub(super) fn run_plot<'a>(
    ui: &Ui,
    id: impl std::hash::Hash + std::fmt::Debug,
    whole: bool,
) -> Plot<'a> {
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
pub(super) fn static_plot<'a>(ui: &Ui, id: impl std::hash::Hash + std::fmt::Debug) -> Plot<'a> {
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
pub(super) struct Shown {
    /// `None`: the whole run (following it).
    pub(super) range: Option<(f64, f64)>,
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
pub(super) struct ChartMemory {
    pub(super) hover: Option<f64>,
    pub(super) zoomed: bool,
}

fn memory_id(chart: &str) -> Id {
    Id::unique(("chart-memory", chart))
}

pub(super) fn load_memory(ui: &Ui, chart: &str) -> ChartMemory {
    ui.data(|d| d.get_temp(memory_id(chart)).unwrap_or_default())
}

/// Stores this frame's memory; a change shows in the titles only next
/// frame, so ask for one.
pub(super) fn store_memory(ui: &Ui, chart: &str, old: ChartMemory, new: ChartMemory) {
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

/// What hovering a run chart's title says about exploring it.
pub(super) fn run_hint() -> String {
    format!(
        "Drag sideways to move along the run. Pinch, or hold {} and scroll, to zoom. Double-click to show the whole run.",
        format::command_key()
    )
}

/// The button that goes back to the whole run, shown at the right of a
/// title row while the chart is zoomed in. Returns whether it was clicked.
pub(super) fn zoom_reset(ui: &mut Ui, mem: ChartMemory) -> bool {
    mem.zoomed
        && widgets::icon_text_button(
            ui,
            icons::ICON_ZOOM_OUT_MAP,
            "Show the whole run",
            "Zoom out to every recorded update (or double-click a chart)",
        )
        .clicked()
}

/// Draws the data of one run chart: picks the points for the part of the
/// run shown, fits the value axis to them, and records the hovered
/// update.
pub(super) fn draw_run_series(
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
        "Step or run the simulation.",
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
            let tip = if has_subset_value(m) {
                format!(
                    "{} Chart it. With a filter set, a second line shows the matching organisms.",
                    m.description()
                )
            } else {
                m.description().to_string()
            };
            let r = ui.add(b).on_hover_text(tip);
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

/// The value-axis scale and the moving average of the time series.
fn series_controls(ui: &mut Ui, state: &mut AppState) {
    let t = Tokens::of(ui);
    ui.horizontal(|ui| {
        let mut log = state.brush.series.log_y;
        if widgets::segmented(
            ui,
            "series-scale",
            &mut log,
            &[
                (false, "Linear", "Evenly spaced values on the value axis"),
                (
                    true,
                    "Log",
                    "Evenly spaced orders of magnitude on the value axis (values above zero only)",
                ),
            ],
        ) {
            state.brush.series.log_y = log;
        }
        ui.label(RichText::new("Average").color(t.text_weak))
            .on_hover_text(
                "Draw each line as a moving average over this many points, to see the trend through the noise.",
            );
        let current = state.brush.series.smoothing;
        let r = egui::ComboBox::from_id_salt("series-average")
            .selected_text(current.label())
            .width(56.0)
            .show_ui(ui, |ui| {
                for c in Smoothing::CHOICES {
                    if ui.selectable_label(current == c, c.label()).clicked() {
                        state.brush.series.smoothing = c;
                    }
                }
            })
            .response;
        widgets::name_widget(&r, egui::Role::ComboBox, "Moving average");
    });
}

/// A small filled disc: the key of a line in a title row.
fn key_dot(ui: &mut Ui, color: Color32) {
    let (rect, _) = ui.allocate_exact_size(vec2(10.0, 14.0), Sense::hover());
    ui.painter().circle_filled(rect.center(), 4.0, color);
}

/// The y marks of a log10 axis.
pub(super) fn log_spacer() -> impl Fn(GridInput) -> Vec<GridMark> {
    |input: GridInput| {
        log_marks(input.bounds.0, input.bounds.1)
            .into_iter()
            .map(|(value, step_size)| GridMark { value, step_size })
            .collect()
    }
}

/// The nearest subset row to update `x`.
fn subset_near(rows: &[SubsetRow], x: f64) -> Option<&SubsetRow> {
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

const SUBSET_HINT: &str = "The line for the matching organisms is the mean over the organisms that match the filter, taken from the moment the filter was set: a live run does not keep the past of each organism.";
const SUBSET_HINT_PLAYBACK: &str = "The line for the matching organisms is the mean over the organisms that match the filter in each recorded frame. The recording is read in the background, so the line fills in over a moment.";

fn time_series(ui: &mut Ui, state: &mut AppState, cache: &mut SeriesCache) {
    let t = Tokens::of(ui);
    metric_chips(ui, state);
    series_controls(ui, state);
    ui.add_space(4.0);
    if state.history.is_empty() {
        no_rows(ui);
        return;
    }
    let mem = load_memory(ui, "time-series");
    let mut reset = false;
    let opts = state.brush.series;
    let log = opts.log_y;
    let window = opts.smoothing.window();
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
    let playback = state.is_playback();
    let mut hover = None;
    let mut zoomed = mem.zoomed;
    let history = &state.history;
    // A playback holds the whole run's subset rows; the charts show the run
    // up to the frame on screen.
    let upto = state.history.latest().map_or(i32::MAX, |r| r.update);
    let all_rows = state.brush.subset.rows();
    let sub_rows: &[SubsetRow] = if playback {
        &all_rows[..all_rows.partition_point(|r| r.update <= upto)]
    } else {
        all_rows
    };
    let subset_on = state.brush.is_active() && !sub_rows.is_empty();
    let hint = run_hint();
    let area = ui
        .scope(|ui| {
            for (i, m) in metrics.iter().enumerate() {
                let has_subset = subset_on && has_subset_value(*m);
                let sub_value = has_subset
                    .then(|| match mem.hover {
                        Some(x) => subset_near(sub_rows, x),
                        None => sub_rows.last(),
                    })
                    .flatten()
                    .and_then(|r| subset_value(*m, r));
                ui.horizontal(|ui| {
                    let tip = format!("{} {hint}", m.description());
                    ui.label(RichText::new(m.label()).family(semibold()).color(t.text))
                        .on_hover_text(tip);
                    if has_subset {
                        key_dot(ui, t.accent);
                    }
                    if let Some((row, hovered)) = &described {
                        ui.label(
                            RichText::new(format::metric(m.value(row), m.is_count()))
                                .color(t.text_weak),
                        );
                        if has_subset {
                            if let Some(v) = sub_value {
                                key_dot(ui, t.filter);
                                ui.label(
                                    RichText::new(if v.is_finite() {
                                        format::metric(v, m.is_count())
                                    } else {
                                        "none".to_string()
                                    })
                                    .color(t.text_weak),
                                )
                                .on_hover_text(if playback {
                                    SUBSET_HINT_PLAYBACK
                                } else {
                                    SUBSET_HINT
                                });
                            }
                        }
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
                    if i == 0 {
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            reset = zoom_reset(ui, mem);
                        });
                    }
                });
                let is_count = m.is_count();
                let mut plot = run_plot(ui, ("ts", *m), is_count && !log)
                    .height(each)
                    .width(width)
                    .link_axis(group, [true, false])
                    .link_cursor(group, [true, false])
                    .y_axis_min_width(44.0)
                    .y_axis_formatter(move |mark, range| {
                        if log {
                            if mark.step_size < 1.0 && range.end() - range.start() > 1.5 {
                                String::new()
                            } else {
                                format::metric(10f64.powf(mark.value), is_count)
                            }
                        } else {
                            format::metric(mark.value, is_count)
                        }
                    });
                if log {
                    plot = plot.y_grid_spacer(log_spacer());
                }
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
                                let pts = if window <= 1 && !log {
                                    cache.get_range(history, key, max_points, range).to_vec()
                                } else {
                                    let rows = history.rows();
                                    let xs: Vec<f64> =
                                        rows.iter().map(|r| f64::from(r.update)).collect();
                                    let ys: Vec<f64> = rows.iter().map(|r| m.value(r)).collect();
                                    shape_points(&xs, &ys, window, log, range, max_points)
                                };
                                let mut fit_points = pts.clone();
                                let sub_pts: Vec<[f64; 2]> = if has_subset {
                                    let xs: Vec<f64> =
                                        sub_rows.iter().map(|r| f64::from(r.update)).collect();
                                    let ys: Vec<f64> = sub_rows
                                        .iter()
                                        .map(|r| subset_value(*m, r).unwrap_or(f64::NAN))
                                        .collect();
                                    shape_points(&xs, &ys, window, log, range, max_points)
                                } else {
                                    Vec::new()
                                };
                                fit_points.extend_from_slice(&sub_pts);
                                let fit = y_fit(&fit_points, range, is_count && !log);
                                p.line(
                                    Line::new(m.label(), pts)
                                        .color(t.accent)
                                        .width(1.8)
                                        .fill(fit.0 as f32)
                                        .fill_alpha(if has_subset { 0.04 } else { 0.08 }),
                                );
                                if has_subset {
                                    p.line(
                                        Line::new("Matching organisms", sub_pts)
                                            .color(t.filter)
                                            .width(1.8),
                                    );
                                }
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
    let frame = state.frame.as_deref();
    (0..frame.map_or(0, |f| f.env.tasks.len()))
        .map(|k| task_name(frame, k))
        .collect()
}

/// A percentage for a share of 0 to 1.
pub(super) fn percent_of(share: f64) -> String {
    let p = share * 100.0;
    if p >= 10.0 || p == 0.0 {
        format!("{p:.0}%")
    } else {
        format!("{p:.1}%")
    }
}

/// What the task table shows for one task.
struct TaskRow {
    count: u32,
    /// The matching organisms that perform it, with how many match.
    subset: Option<(u32, u32)>,
}

/// What a click in the task table asked for.
#[derive(Default)]
struct TaskClicks {
    hovered: Option<usize>,
    /// Task, and whether the condition is "performs".
    condition: Option<(usize, bool)>,
}

const TASK_ROW_H: f32 = 22.0;

/// The modifier keys held when the pointer was released (the click), else
/// those held now.
fn click_modifiers(ui: &Ui) -> egui::Modifiers {
    ui.input(|i| {
        i.events
            .iter()
            .rev()
            .find_map(|e| match e {
                egui::Event::PointerButton {
                    pressed: false,
                    modifiers,
                    ..
                } => Some(*modifiers),
                _ => None,
            })
            .unwrap_or(i.modifiers)
    })
}

/// One row per task, in task order: its colour (click to hide or show its
/// line), its name, two thin bars (everyone, and the matching organisms
/// while a filter is set) and the shares. A click anywhere else on the row
/// makes "performs this task" a condition of the filter, Shift-click "does
/// not perform it".
fn task_table(
    ui: &mut Ui,
    names: &[String],
    rows: &[TaskRow],
    total: u32,
    state_of: &dyn Fn(usize) -> Option<bool>,
    hidden: &mut Vec<bool>,
    what: &str,
) -> TaskClicks {
    let t = Tokens::of(ui);
    let mut out = TaskClicks::default();
    let with_subset = rows.iter().any(|r| r.subset.is_some());
    // The scroll area lays its content out a scroll bar's width wider than
    // the space it is given.
    let full = ui.available_width();
    egui::ScrollArea::vertical()
        .id_salt("task-table")
        .max_height(TASK_ROW_H * 10.0)
        .auto_shrink([false, true])
        .show(ui, |ui| {
            ui.spacing_mut().item_spacing.y = 0.0;
            ui.set_max_width(full);
            for (k, name) in names.iter().enumerate() {
                let off = hidden.get(k).copied().unwrap_or(false);
                let row = &rows[k];
                let (rect, r) = ui.allocate_exact_size(
                    vec2(full, TASK_ROW_H),
                    Sense::click(),
                );
                let want = state_of(k);
                let sw_rect = egui::Rect::from_center_size(
                    egui::pos2(rect.left() + 12.0, rect.center().y),
                    vec2(18.0, TASK_ROW_H),
                );
                let sw = ui.interact(sw_rect, ui.scope_id().with(("task-line", k)), Sense::click());
                let painter = ui.painter();
                if want.is_some() {
                    painter.rect_filled(rect.shrink2(vec2(0.0, 1.0)), 4.0, t.filter_soft);
                } else if r.hovered() {
                    painter.rect_filled(rect.shrink2(vec2(0.0, 1.0)), 4.0, t.control);
                }
                let color = task_color(k, names.len(), t.dark);
                let swatch = egui::Rect::from_center_size(sw_rect.center(), vec2(10.0, 10.0));
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
                if sw.has_focus() {
                    painter.rect_stroke(
                        sw_rect,
                        4.0,
                        egui::Stroke::new(1.5, t.accent),
                        egui::StrokeKind::Outside,
                    );
                }
                // Right side: the shares and the state of the condition.
                let font = egui::FontId::proportional(12.5);
                let icon_w = 16.0;
                let col_w = 38.0;
                let pct_w = if with_subset { 2.0 * col_w } else { col_w };
                let right = rect.right() - 4.0;
                let share = |c: u32, of: u32| {
                    if of == 0 {
                        0.0
                    } else {
                        f64::from(c) / f64::from(of)
                    }
                };
                let pop_share = share(row.count, total);
                let mut x = right - icon_w;
                if let Some(performs) = want {
                    let icon = if performs {
                        icons::ICON_CHECK
                    } else {
                        icons::ICON_BLOCK
                    };
                    painter.text(
                        egui::pos2(x + icon_w / 2.0, rect.center().y),
                        egui::Align2::CENTER_CENTER,
                        icon.codepoint,
                        egui::FontId::proportional(14.0),
                        t.filter,
                    );
                }
                if let Some((c, matched)) = row.subset {
                    painter.text(
                        egui::pos2(x, rect.center().y),
                        egui::Align2::RIGHT_CENTER,
                        percent_of(share(c, matched)),
                        font.clone(),
                        t.text,
                    );
                    x -= col_w;
                }
                painter.text(
                    egui::pos2(x, rect.center().y),
                    egui::Align2::RIGHT_CENTER,
                    percent_of(pop_share),
                    font.clone(),
                    t.text_weak,
                );
                let pct_left = right - icon_w - pct_w;
                // Name and bars share the rest.
                let inner_left = rect.left() + 26.0;
                let inner_w = (pct_left - 6.0 - inner_left).max(40.0);
                let name_w = (inner_w * 0.5).clamp(40.0, 190.0);
                let mut job = egui::text::LayoutJob::simple(
                    name.clone(),
                    font.clone(),
                    if off { t.text_faint } else { t.text },
                    name_w,
                );
                job.wrap.max_rows = 1;
                job.wrap.break_anywhere = true;
                let galley = painter.layout_job(job);
                painter.galley(
                    egui::pos2(inner_left, rect.center().y - galley.size().y / 2.0),
                    galley,
                    t.text,
                );
                let bar_left = inner_left + name_w + 8.0;
                let bar_w = (pct_left - 6.0 - bar_left).max(0.0);
                if bar_w > 4.0 {
                    let (h, y1, y2) = if with_subset {
                        (5.0, rect.center().y - 5.0, rect.center().y + 1.0)
                    } else {
                        (7.0, rect.center().y - 3.5, 0.0)
                    };
                    let track = |y: f32| {
                        egui::Rect::from_min_size(egui::pos2(bar_left, y), vec2(bar_w, h))
                    };
                    painter.rect_filled(track(y1), 2.0, t.control_active.gamma_multiply(0.6));
                    painter.rect_filled(
                        egui::Rect::from_min_size(
                            egui::pos2(bar_left, y1),
                            vec2((bar_w * pop_share as f32).max(0.0), h),
                        ),
                        2.0,
                        if off { color.gamma_multiply(0.35) } else { color },
                    );
                    if let Some((c, matched)) = row.subset {
                        painter.rect_filled(track(y2), 2.0, t.control_active.gamma_multiply(0.6));
                        painter.rect_filled(
                            egui::Rect::from_min_size(
                                egui::pos2(bar_left, y2),
                                vec2((bar_w * share(c, matched) as f32).max(0.0), h),
                            ),
                            2.0,
                            t.filter,
                        );
                    }
                }
                // Accessibility and tooltips.
                let label = format!("Show {name} in the task chart");
                sw.widget_info(|| {
                    egui::WidgetInfo::selected(egui::Role::CheckBox, true, !off, &label)
                });
                let sw = sw.on_hover_text(if off {
                    "Click to show this task's line"
                } else {
                    "Click to hide this task's line"
                });
                let mut tip = format!(
                    "{name}: {} of {} organisms ({}) {what}.",
                    format::count(i64::from(row.count)),
                    format::count(i64::from(total)),
                    percent_of(pop_share)
                );
                if let Some((c, matched)) = row.subset {
                    tip.push_str(&format!(
                        " Among the {} matching organisms: {} ({}).",
                        format::count(i64::from(matched)),
                        format::count(i64::from(c)),
                        percent_of(share(c, matched))
                    ));
                }
                tip.push_str(match want {
                    Some(true) => " The filter keeps organisms that perform it. Click to remove that condition, Shift-click to keep those that do not.",
                    Some(false) => " The filter keeps organisms that do not perform it. Click to keep those that do, Shift-click to remove that condition.",
                    None => " Click to keep only organisms that perform it, Shift-click only those that do not.",
                });
                let rname = format!("Filter on {name}");
                r.widget_info(|| {
                    egui::WidgetInfo::selected(
                        egui::Role::Button,
                        true,
                        want.is_some(),
                        &rname,
                    )
                });
                let r = r.on_hover_text(tip);
                if r.hovered() || sw.hovered() {
                    out.hovered = Some(k);
                    ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
                }
                if r.has_focus() {
                    ui.painter().rect_stroke(
                        r.rect,
                        4.0,
                        egui::Stroke::new(1.5, t.accent),
                        egui::StrokeKind::Inside,
                    );
                }
                if sw.clicked() {
                    if hidden.len() < names.len() {
                        hidden.resize(names.len(), false);
                    }
                    hidden[k] = !off;
                } else if r.clicked() {
                    let shift = click_modifiers(ui).shift;
                    out.condition = Some((k, !shift));
                }
            }
        });
    out
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
    let described = described_row(&state.history, mem).map(|(r, h)| (r.clone(), h));
    let hovered_row = described.as_ref().filter(|(_, h)| *h).map(|(r, _)| r);
    // The table reads the newest frame (where the filter lives); while a
    // chart is hovered it reads the row at that update instead.
    let from_frame = state
        .frame
        .as_ref()
        .and_then(|f| f.map.as_ref())
        .filter(|_| hovered_row.is_none());
    let (rows, total, what): (Vec<TaskRow>, u32, &str) = match (from_frame, &described) {
        (Some(map), _) => {
            let all = task_counts(map, None);
            let sub = state
                .brush
                .selection()
                .filter(|s| s.active)
                .map(|s| (task_counts(map, Some(&s.mask)), s.matched as u32));
            let rows = all
                .iter()
                .enumerate()
                .map(|(k, &count)| TaskRow {
                    count,
                    subset: sub.as_ref().map(|(c, m)| (c[k], *m)),
                })
                .collect();
            (
                rows,
                map.occupied() as u32,
                "perform it in their last gestation",
            )
        }
        (None, Some((row, _))) => (
            names
                .iter()
                .enumerate()
                .map(|(k, _)| TaskRow {
                    count: row.task_last_count.get(k).map_or(0, |&c| c.max(0) as u32),
                    subset: None,
                })
                .collect(),
            row.orgs.max(0) as u32,
            "performed it in their last gestation, at that update",
        ),
        (None, None) => (Vec::new(), 0, ""),
    };
    if rows.is_empty() {
        no_rows(ui);
        return;
    }
    let filter_active = rows.iter().any(|r| r.subset.is_some());
    let mut hidden: Vec<bool> = ui.data(|d| d.get_temp(hidden_tasks_id()).unwrap_or_default());
    let before = hidden.clone();
    let mut reset = false;
    let area = ui
        .scope(|ui| {
            ui.horizontal(|ui| {
                ui.label(
                    RichText::new("Share of organisms")
                        .family(semibold())
                        .color(t.text),
                )
                .on_hover_text(
                    "How many organisms performed each task in their last gestation. With a filter set, a second bar compares the matching organisms. Click a task to keep only organisms that perform it, Shift-click only those that do not.",
                );
                if filter_active {
                    ui.add_space(6.0);
                    ui.spacing_mut().item_spacing.x = 4.0;
                    key_dot(ui, t.text_faint);
                    ui.label(RichText::new("Everyone").small().color(t.text_weak));
                    key_dot(ui, t.filter);
                    ui.label(RichText::new("Matching").small().color(t.text_weak));
                }
            });
            let filter = state.brush.filter.clone();
            let clicks = task_table(
                ui,
                &names,
                &rows,
                total,
                &|k| filter.task_state(k),
                &mut hidden,
                what,
            );
            if let Some((k, performs)) = clicks.condition {
                state.edit_filter(|f| f.toggle_task(k, performs));
            }
            if hidden != before {
                ui.data_mut(|d| d.insert_temp(hidden_tasks_id(), hidden.clone()));
            }
            ui.add_space(6.0);
            ui.horizontal(|ui| {
                let none_yet = !state
                    .history
                    .rows()
                    .iter()
                    .rev()
                    .any(|r| r.task_last_count.iter().any(|&c| c > 0));
                ui.label(
                    RichText::new("Organisms over time")
                        .family(semibold())
                        .color(t.text),
                )
                .on_hover_text(format!(
                    "Organisms that performed each task in their last gestation, update by update. {}",
                    run_hint()
                ));
                if none_yet {
                    ui.label(RichText::new("No task performed yet").color(t.text_weak));
                } else if let Some((row, true)) = &described {
                    ui.label(
                        RichText::new(format!(
                            "at update {}",
                            format::count(i64::from(row.update))
                        ))
                        .small()
                        .color(t.text_faint),
                    );
                }
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    reset = zoom_reset(ui, mem);
                });
            });
            let focus = clicks.hovered;
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
