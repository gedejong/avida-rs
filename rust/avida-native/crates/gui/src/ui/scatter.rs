//! The scatter view of the charts tab: any two numeric properties of the
//! organisms plotted against each other, one dot per organism, coloured as
//! the map colours it. The organisms the filter selects are drawn larger
//! and outlined in the filter colour, the others fainter.
//!
//! Dragging a rectangle makes its two ranges conditions of the filter;
//! clicking a dot inspects that organism. The dots are painted as one mesh
//! over a plot that only draws the axes, so 25,600 organisms cost a few
//! milliseconds (`ARCHITECTURE.md`, "Chart filtering and brushing").

use avida_viewmodel::{scatter_points, thin_points, Condition, Prop, ScatterPoint, Selection};
use egui::{pos2, vec2, Color32, Id, Mesh, PointerButton, Rect, RichText, Shape, Ui};
use egui_material_icons::icons;
use egui_plot::{Plot, PlotBounds};

use crate::model::filtering::prop_title;
use crate::model::{format, AppState};
use crate::ui::charts::{log_spacer, round_marks};
use crate::ui::ease;
use crate::ui::theme::{semibold, Tokens};
use crate::ui::widgets;

/// Dots drawn at most: beyond this the unselected ones are thinned, every
/// k-th by cell, and the selected ones are kept first.
pub const MAX_DOTS: usize = 60_000;

/// An axis value as plotted (log10 on a log axis).
fn plotted(v: f32, log: bool) -> f64 {
    if log {
        f64::from(v).log10()
    } else {
        f64::from(v)
    }
}

/// The property value a plotted coordinate stands for.
fn unplotted(v: f64, log: bool) -> f64 {
    if log {
        10f64.powf(v)
    } else {
        v
    }
}

/// How far, in units of the property, a dot of a whole-number property is
/// drawn from its value at most.
const JITTER: f64 = 0.36;

/// The offsets of the dot of `cell` on the two axes, each within
/// `-JITTER..=JITTER`: a hash of the cell, the same in every frame.
pub fn jitter(cell: u32) -> (f64, f64) {
    let mut h = u64::from(cell).wrapping_add(0x9E37_79B9_7F4A_7C15);
    h = (h ^ (h >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    h = (h ^ (h >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    h ^= h >> 31;
    let unit = |bits: u64| (bits & 0xFFFF) as f64 / 65535.0 * 2.0 - 1.0;
    (unit(h) * JITTER, unit(h >> 32) * JITTER)
}

/// The range a drag between plotted coordinates `a` and `b` selects:
/// whole numbers for whole properties.
pub fn dragged_range(prop: Prop, log: bool, a: f64, b: f64) -> (f64, f64) {
    let (lo, hi) = (unplotted(a.min(b), log), unplotted(a.max(b), log));
    if prop.is_whole() {
        let (l, u) = (lo.ceil(), hi.floor());
        if l > u {
            let mid = ((lo + hi) / 2.0).round();
            (mid, mid)
        } else {
            (l, u)
        }
    } else {
        (lo, hi)
    }
}

/// The dots to draw for the axes: finite on both, and above zero on a log
/// axis. The second number counts those a log axis left out.
pub fn visible_points(
    points: &[ScatterPoint],
    log_x: bool,
    log_y: bool,
) -> (Vec<ScatterPoint>, usize) {
    let keep = |p: &&ScatterPoint| (!log_x || p.x > 0.0) && (!log_y || p.y > 0.0);
    let kept: Vec<ScatterPoint> = points.iter().filter(keep).copied().collect();
    let left_out = points.len() - kept.len();
    (kept, left_out)
}

/// The "Log" toggle next to an axis picker.
fn log_toggle(ui: &mut Ui, t: &Tokens, on: bool, name: &str) -> egui::Response {
    let text = if on {
        RichText::new("Log").color(t.accent).family(semibold())
    } else {
        RichText::new("Log").color(t.text_weak)
    };
    let mut b = egui::Button::selectable(on, text)
        .corner_radius(8.0)
        .stroke(egui::Stroke::new(1.0, t.border_strong));
    if on {
        b = b
            .fill(t.accent_soft)
            .stroke(egui::Stroke::new(1.0, t.accent.gamma_multiply(0.6)));
    }
    let r = ui.add(b);
    r.widget_info(|| egui::WidgetInfo::selected(egui::Role::CheckBox, true, on, name));
    r
}

fn drag_id() -> Id {
    Id::unique("scatter-brush")
}

fn axis_picker(
    ui: &mut Ui,
    id: &str,
    name: &str,
    current: &mut Prop,
    frame: Option<&avida_session::Frame>,
) {
    let r = egui::ComboBox::from_id_salt(id)
        .selected_text(prop_title(frame, *current))
        .width(104.0)
        .show_ui(ui, |ui| {
            for p in Prop::ALL {
                ui.selectable_value(current, p, prop_title(frame, p))
                    .on_hover_text(p.description());
            }
        })
        .response;
    widgets::name_widget(&r, egui::Role::ComboBox, name);
}

pub fn show(ui: &mut Ui, state: &mut AppState) {
    let t = Tokens::of(ui);
    let Some(frame) = state.frame.clone() else {
        widgets::empty_state(
            ui,
            icons::ICON_SCATTER_PLOT,
            "No frame yet",
            "The world is being built.",
        );
        return;
    };
    let mut axes = state.brush.scatter;
    // One row per axis: side by side the second toggle wrapped onto a line
    // of its own in the narrow panel.
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing = vec2(6.0, 4.0);
        ui.label(RichText::new("X").color(t.text_weak));
        axis_picker(ui, "scatter-x", "Horizontal axis", &mut axes.x, Some(&frame));
        if log_toggle(ui, &t, axes.log_x, "Logarithmic horizontal axis")
            .on_hover_text("A logarithmic horizontal axis (organisms with a value of zero or less are left out)")
            .clicked()
        {
            axes.log_x = !axes.log_x;
        }
    });
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing = vec2(6.0, 4.0);
        ui.label(RichText::new("Y").color(t.text_weak));
        axis_picker(ui, "scatter-y", "Vertical axis", &mut axes.y, Some(&frame));
        if log_toggle(ui, &t, axes.log_y, "Logarithmic vertical axis")
            .on_hover_text(
                "A logarithmic vertical axis (organisms with a value of zero or less are left out)",
            )
            .clicked()
        {
            axes.log_y = !axes.log_y;
        }
    });
    state.brush.scatter = axes;
    let Some(map) = frame.map.as_ref() else {
        ui.label(RichText::new("No map yet.").color(t.text_weak));
        return;
    };
    let all = scatter_points(map, axes.x, axes.y);
    let sel: Option<Selection> = state
        .brush
        .selection()
        .filter(|s| s.active)
        .map(|s| (**s).clone());
    let (visible, left_out) = visible_points(&all, axes.log_x, axes.log_y);
    let shown = thin_points(&visible, sel.as_ref(), MAX_DOTS);
    let x_title = prop_title(Some(&frame), axes.x);
    let y_title = prop_title(Some(&frame), axes.y);

    ui.add_space(2.0);
    let mut hint = String::from(
        "Each dot is one organism, coloured as on the map. Drag a rectangle to keep only organisms inside it: its two ranges become conditions of the filter. Click a dot to inspect that organism.",
    );
    if left_out > 0 {
        hint.push_str(&format!(
            " {} organisms with a value of zero or less are not shown on a logarithmic axis.",
            format::count(left_out as i64)
        ));
    }
    if shown.len() < visible.len() {
        hint.push_str(&format!(
            " The plot shows {} of the organisms: every one that matches the filter, and an even sample of the rest.",
            format::count(shown.len() as i64)
        ));
    }
    ui.horizontal(|ui| {
        ui.label(
            RichText::new(format!(
                "{y_title}{} against {x_title}",
                if axes.log_y { " (log scale)" } else { "" }
            ))
            .family(semibold())
            .color(t.text),
        )
        .on_hover_text(&hint);
        ui.label(icons::ICON_INFO.rich_text().size(13.0).color(t.text_faint))
            .on_hover_text(&hint);
        if sel.is_none() {
            ui.label(
                RichText::new(format!("{} organisms", format::count(all.len() as i64)))
                    .color(t.text_weak),
            );
        }
    });
    if all.is_empty() {
        widgets::empty_state(
            ui,
            icons::ICON_SCATTER_PLOT,
            "No organisms to plot",
            "Waiting for the values.",
        );
        return;
    }

    // Axis ranges with a margin.
    let (mut x0, mut x1, mut y0, mut y1) = (f64::MAX, f64::MIN, f64::MAX, f64::MIN);
    for p in &visible {
        let (x, y) = (plotted(p.x, axes.log_x), plotted(p.y, axes.log_y));
        x0 = x0.min(x);
        x1 = x1.max(x);
        y0 = y0.min(y);
        y1 = y1.max(y);
    }
    if x0 > x1 {
        (x0, x1) = (0.0, 1.0);
    }
    if y0 > y1 {
        (y0, y1) = (0.0, 1.0);
    }
    let pad = |lo: f64, hi: f64, log: bool| {
        let span = hi - lo;
        if span < 1e-9 {
            let w = if log { 0.5 } else { (hi.abs() * 0.1).max(0.5) };
            (lo - w, hi + w)
        } else {
            (lo - span * 0.05, hi + span * 0.05)
        }
    };
    let (x0, x1) = pad(x0, x1, axes.log_x);
    let (y0, y1) = pad(y0, y1, axes.log_y);
    // Room for the jitter of a whole-number property.
    let room = |lo: f64, hi: f64, prop: Prop, log: bool| {
        if prop.is_whole() && !log {
            (lo - JITTER, hi + JITTER)
        } else {
            (lo, hi)
        }
    };
    let (x0, x1) = room(x0, x1, axes.x, axes.log_x);
    let (y0, y1) = room(y0, y1, axes.y, axes.log_y);
    // Widen at once, narrow slowly: a stray organism does not make the
    // plot jump back and forth.
    let (x0, x1) = ease::eased_range(ui.ctx(), ("scatter-x", axes.x, axes.log_x), (x0, x1), false);
    let (y0, y1) = ease::eased_range(ui.ctx(), ("scatter-y", axes.y, axes.log_y), (y0, y1), false);

    let readout_h = 18.0;
    let height = (ui.available_height() - readout_h - 40.0).max(200.0);
    let mut plot = Plot::new("scatter")
        .show_background(false)
        .allow_scroll(false)
        .allow_zoom(false)
        .allow_drag(false)
        .allow_boxed_zoom(false)
        .allow_double_click_reset(false)
        .set_margin_fraction(vec2(0.0, 0.0))
        .grid_spacing(24.0..=160.0)
        .grid_color(t.text.gamma_multiply(if t.dark { 0.28 } else { 0.2 }))
        .height(height)
        .y_axis_min_width(44.0)
        .x_axis_label(if axes.log_x {
            format!("{x_title} (log scale)")
        } else {
            x_title.clone()
        })
        .cursor_color(t.text_faint);
    let (log_x, log_y) = (axes.log_x, axes.log_y);
    let (whole_x, whole_y) = (axes.x.is_whole(), axes.y.is_whole());
    // A narrow whole-number axis labels every value, when the values are
    // far enough apart on screen (30 points); the plot paints a label only
    // under a mark whose step is wide (over 60 points), so these marks
    // claim a huge step (a lower one flickers as the plot's scale settles).
    // A wide axis thins out to round values.
    let thinned = round_marks(whole_x, 2.0);
    plot = if log_x {
        plot.x_grid_spacer(log_spacer())
    } else if whole_x {
        plot.x_grid_spacer(move |input: egui_plot::GridInput| {
            // `base_step_size` is the value span of the plot's minimum grid
            // spacing, 24 points.
            let points_per_value = 24.0 / input.base_step_size;
            if !points_per_value.is_finite() || points_per_value < 30.0 {
                return thinned(input);
            }
            let (lo, hi) = input.bounds;
            ((lo.ceil() as i64)..=(hi.floor() as i64))
                .map(|v| egui_plot::GridMark {
                    value: v as f64,
                    step_size: 1.0e3,
                })
                .collect()
        })
    } else {
        plot.x_grid_spacer(round_marks(false, 2.0))
    };
    plot = if log_y {
        plot.y_grid_spacer(log_spacer())
    } else {
        plot.y_grid_spacer(round_marks(whole_y, 1.0))
    };
    let label = |v: f64, log: bool, whole: bool, span: f64| {
        if log {
            if span > 1.5 && (v - v.round()).abs() > 1e-6 {
                String::new()
            } else {
                format::real(10f64.powf(v))
            }
        } else {
            format::metric(v, whole)
        }
    };
    let (sx, sy) = (x1 - x0, y1 - y0);
    plot = plot
        .x_axis_formatter(move |m, _| label(m.value, log_x, whole_x, sx))
        .y_axis_formatter(move |m, _| label(m.value, log_y, whole_y, sy));
    let r = plot.show(ui, |p| {
        p.set_plot_bounds(PlotBounds::from_min_max([x0, y0], [x1, y1]));
    });
    let tr = r.transform;
    let frame_rect = *tr.frame();
    let painter = ui.painter().with_clip_rect(frame_rect);

    // The dots.
    let colors = state.map_rgba.clone();
    let color_of = |cell: u32| -> Color32 {
        colors
            .as_ref()
            .and_then(|c| c.get(cell as usize * 4..cell as usize * 4 + 3))
            .map_or(t.text_weak, |c| Color32::from_rgb(c[0], c[1], c[2]))
    };
    // A whole-number property on a linear axis puts every organism on a few
    // grid lines, one dot over the other. Each dot is moved off its line by
    // up to `JITTER` of a unit, by an amount that depends on its cell only,
    // so that the cloud shows how many there are and does not flicker.
    let (jitter_x, jitter_y) = (whole_x && !log_x, whole_y && !log_y);
    let screen = |p: &ScatterPoint| {
        let (jx, jy) = jitter(p.cell);
        pos2(
            tr.position_from_point_x(plotted(p.x, log_x) + if jitter_x { jx } else { 0.0 }),
            tr.position_from_point_y(plotted(p.y, log_y) + if jitter_y { jy } else { 0.0 }),
        )
    };
    let active = sel.is_some();
    let matches = |p: &ScatterPoint| sel.as_ref().is_some_and(|s| s.contains(p.cell as usize));
    let mut mesh = Mesh::default();
    let (half_plain, alpha) = if active { (1.1, 0.30) } else { (1.4, 0.80) };
    for p in shown.iter().filter(|p| !matches(p)) {
        let c = screen(p);
        if frame_rect.contains(c) {
            mesh.add_colored_rect(
                Rect::from_center_size(c, vec2(half_plain * 2.0, half_plain * 2.0)),
                color_of(p.cell).gamma_multiply(alpha),
            );
        }
    }
    let mut ring = Mesh::default();
    let mut core = Mesh::default();
    if active {
        for p in shown.iter().filter(|p| matches(p)) {
            let c = screen(p);
            if frame_rect.contains(c) {
                ring.add_colored_rect(Rect::from_center_size(c, vec2(4.8, 4.8)), t.filter);
                core.add_colored_rect(Rect::from_center_size(c, vec2(3.2, 3.2)), color_of(p.cell));
            }
        }
    }
    painter.add(Shape::mesh(mesh));
    painter.add(Shape::mesh(ring));
    painter.add(Shape::mesh(core));

    // The ranges the filter holds on these two properties.
    let range_x = state.brush.filter.range_of(axes.x);
    let range_y = state.brush.filter.range_of(axes.y);
    if range_x.is_some() || range_y.is_some() {
        let edge = |v: f64, lo_side: bool, horizontal: bool| -> f32 {
            let (log, lo, hi) = if horizontal {
                (log_x, frame_rect.left(), frame_rect.right())
            } else {
                (log_y, frame_rect.bottom(), frame_rect.top())
            };
            if !v.is_finite() || (log && v <= 0.0) {
                return if lo_side { lo } else { hi };
            }
            let pv = if log { v.log10() } else { v };
            if horizontal {
                tr.position_from_point_x(pv)
            } else {
                tr.position_from_point_y(pv)
            }
        };
        let (xl, xh) = range_x.map_or((frame_rect.left(), frame_rect.right()), |(a, b)| {
            (edge(a, true, true), edge(b, false, true))
        });
        let (yl, yh) = range_y.map_or((frame_rect.bottom(), frame_rect.top()), |(a, b)| {
            (edge(a, true, false), edge(b, false, false))
        });
        let rect = Rect::from_x_y_ranges(xl.min(xh)..=xl.max(xh), yl.min(yh)..=yl.max(yh));
        painter.rect_filled(rect, 0.0, t.filter.gamma_multiply(0.10));
        painter.rect_stroke(
            rect,
            0.0,
            egui::Stroke::new(1.5, t.filter),
            egui::StrokeKind::Inside,
        );
    }

    // The nearest dot to the pointer, and its read-out.
    let mut readout = String::new();
    let mut nearest: Option<ScatterPoint> = None;
    if let Some(pos) = r.response.hover_pos().filter(|p| frame_rect.contains(*p)) {
        let mut best = 9.0f32 * 9.0;
        for p in &shown {
            let d = screen(p).distance_sq(pos);
            if d < best {
                best = d;
                nearest = Some(*p);
            }
        }
    }
    if let Some(p) = nearest {
        let c = screen(&p);
        painter.circle_stroke(c, 5.5, egui::Stroke::new(1.5, t.text));
        let w = usize::try_from(map.w).unwrap_or(1).max(1);
        let cell = p.cell as usize;
        readout = format!(
            "Organism at column {}, row {}: {} {}, {} {}{}",
            cell % w,
            cell / w,
            x_title.to_lowercase(),
            format::real(f64::from(p.x)),
            y_title.to_lowercase(),
            format::real(f64::from(p.y)),
            match (&sel, matches(&p)) {
                (Some(_), true) => ", matches the filter",
                (Some(_), false) => ", outside the filter",
                _ => "",
            }
        );
    } else if let Some(pos) = r.response.hover_pos().filter(|p| frame_rect.contains(*p)) {
        let v = tr.value_from_position(pos);
        readout = format!(
            "{} {}, {} {}",
            x_title.to_lowercase(),
            format::real(unplotted(v.x, log_x)),
            y_title.to_lowercase(),
            format::real(unplotted(v.y, log_y))
        );
    }
    ui.allocate_ui(vec2(ui.available_width(), readout_h), |ui| {
        ui.label(RichText::new(readout).small().color(t.text_weak));
    });

    // Brushing a rectangle, or clicking a dot.
    let id = drag_id();
    let clamp = |p: egui::Pos2| frame_rect.clamp(p);
    if r.response.drag_started_by(PointerButton::Primary) {
        if let Some(p) = r.response.interact_pointer_pos() {
            ui.data_mut(|d| d.insert_temp(id, (clamp(p), clamp(p))));
        }
    }
    if r.response.dragged_by(PointerButton::Primary) {
        if let (Some(p), Some((start, _))) = (
            r.response.interact_pointer_pos(),
            ui.data(|d| d.get_temp::<(egui::Pos2, egui::Pos2)>(id)),
        ) {
            let cur = clamp(p);
            ui.data_mut(|d| d.insert_temp(id, (start, cur)));
            let rect = Rect::from_two_pos(start, cur);
            painter.rect_filled(rect, 0.0, t.filter.gamma_multiply(0.22));
            painter.rect_stroke(
                rect,
                0.0,
                egui::Stroke::new(1.5, t.filter),
                egui::StrokeKind::Inside,
            );
        }
    }
    if r.response.drag_stopped() {
        if let Some((start, cur)) = ui.data(|d| d.get_temp::<(egui::Pos2, egui::Pos2)>(id)) {
            ui.data_mut(|d| d.remove::<(egui::Pos2, egui::Pos2)>(id));
            let (a, b) = (tr.value_from_position(start), tr.value_from_position(cur));
            let wide = (start.x - cur.x).abs() >= 3.0;
            let tall = (start.y - cur.y).abs() >= 3.0;
            if wide || tall {
                let (px, py) = (axes.x, axes.y);
                let rx = dragged_range(px, log_x, a.x, b.x);
                let ry = dragged_range(py, log_y, a.y, b.y);
                state.edit_filter(|f| {
                    if wide {
                        f.set(Condition::range(px, rx.0, rx.1));
                    }
                    if tall {
                        f.set(Condition::range(py, ry.0, ry.1));
                    }
                });
            }
        }
    } else if r.response.clicked() {
        if let Some(p) = nearest {
            state.select_cell(Some(p.cell as i32));
        }
    }

    // Only what is on screen (the tab scrolls).
    let area = r.response.rect.union(ui.min_rect());
    state.chart_area = Some(area.intersect(ui.clip_rect()));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn jitter_stays_within_half_a_unit_and_spreads() {
        let v: Vec<(f64, f64)> = (0..2000).map(jitter).collect();
        assert!(v
            .iter()
            .all(|(x, y)| x.abs() <= JITTER && y.abs() <= JITTER));
        assert_eq!(jitter(17), jitter(17));
        let mean = v.iter().map(|p| p.0).sum::<f64>() / v.len() as f64;
        assert!(mean.abs() < 0.03, "{mean}");
        let spread = v.iter().map(|p| p.0.abs()).sum::<f64>() / v.len() as f64;
        assert!((spread - JITTER / 2.0).abs() < 0.03, "{spread}");
    }

    #[test]
    fn a_drag_becomes_ranges_in_the_property_units() {
        // Linear, whole numbers: 10.2 .. 20.8 keeps 11 to 20.
        assert_eq!(
            dragged_range(Prop::GenomeLen, false, 20.8, 10.2),
            (11.0, 20.0)
        );
        // Log: plotted 0 to 2 is 1 to 100.
        let (lo, hi) = dragged_range(Prop::Fitness, true, 0.0, 2.0);
        assert!((lo - 1.0).abs() < 1e-9 && (hi - 100.0).abs() < 1e-9);
        // A sliver of a whole-number axis still keeps one value.
        let (lo, hi) = dragged_range(Prop::Age, false, 5.1, 5.3);
        assert_eq!(lo, hi);
    }

    #[test]
    fn a_log_axis_leaves_out_what_has_no_logarithm() {
        let pts = [
            ScatterPoint {
                cell: 0,
                x: 1.0,
                y: 0.0,
            },
            ScatterPoint {
                cell: 1,
                x: 2.0,
                y: 3.0,
            },
        ];
        assert_eq!(visible_points(&pts, false, false).1, 0);
        let (kept, out) = visible_points(&pts, false, true);
        assert_eq!((kept.len(), out), (1, 1));
        assert_eq!(kept[0].cell, 1);
    }
}
