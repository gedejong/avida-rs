//! The ladder view of the charts tab: the environment's reactions in file
//! order as rungs (the top rung at the top), one strip per rung over the
//! updates, all on one x axis. A strip is the share of the organisms that
//! performed the rung's task in their last gestation, 0 to the whole
//! population. The current count and share are at the right, the name at the
//! left. A ring marks the update at which any organism first performed the
//! rung, a dot the update at which at least 1 % of the organisms first did.
//!
//! A rung nobody has performed is an empty strip with a dotted baseline and
//! the word "never"; one that was performed and now is not keeps its past
//! and says "lost". The series come from `avida_viewmodel::ladder`, shared
//! with the terminal interface; the page holds names, numbers and marks, and
//! what they mean is in the hovers.
//!
//! A click on a rung makes "performs this task" a condition of the filter,
//! Shift-click "does not perform it". With a filter set, the right column
//! adds the matching organisms' count in the filter colour.

use avida_viewmodel::ladder::{build, SubsetCounts};
use avida_viewmodel::{task_counts, LadderView, Rung, RungState};
use egui::{pos2, vec2, Color32, Rect, Sense, Stroke, Ui};
use egui_material_icons::icons;

use super::charts::{click_modifiers, percent_of, task_color};
use crate::model::{format, AppState};
use crate::ui::theme::{semibold, Tokens};
use crate::ui::widgets;

/// Height of one rung's row.
const ROW_H: f32 = 34.0;
/// Width of the column at the right (count, share, the filter's mark).
const RIGHT_W: f32 = 112.0;
/// Room for the check mark of a rung the filter uses.
const MARK_W: f32 = 16.0;
/// The least a performed rung's strip rises, so a rung held by a handful of
/// organisms among thousands is still seen.
const MIN_RISE: f32 = 1.5;

pub fn show(ui: &mut Ui, state: &mut AppState) {
    let Some(frame) = state.frame.clone() else {
        widgets::empty_state(
            ui,
            icons::ICON_SHOW_CHART,
            "No frame yet",
            "The world is being built.",
        );
        return;
    };
    if state.ladder.is_empty() {
        widgets::empty_state(
            ui,
            icons::ICON_SHOW_CHART,
            "No finished updates yet",
            "Step or run the simulation.",
        );
        return;
    }
    let sub = state
        .brush
        .selection()
        .filter(|s| s.active)
        .zip(frame.map.as_ref())
        .map(|(s, map)| (task_counts(map, Some(&s.mask)), s.matched as u32));
    let view = build(
        &state.ladder,
        &frame.env,
        sub.as_ref().map(|(c, m)| SubsetCounts {
            per_task: c,
            matched: *m,
        }),
    );
    if view.is_empty() {
        widgets::empty_state(
            ui,
            icons::ICON_SCIENCE,
            "No rungs",
            "This environment has no reaction with a task.",
        );
        return;
    }
    let filter = state.brush.filter.clone();
    let n_tasks = frame.env.tasks.len();
    let area = ui
        .scope(|ui| {
            headline(ui, &view);
            ui.add_space(6.0);
            if let Some((task, performs)) = rungs(ui, &view, n_tasks, &|k| filter.task_state(k)) {
                state.edit_filter(|f| f.toggle_task(task, performs));
            }
        })
        .response
        .rect;
    state.chart_area = Some(area.intersect(ui.clip_rect()));
}

/// The two summary lines: the highest rung held by at least 1 % of the
/// organisms now, and the highest any organism has performed.
fn headline(ui: &mut Ui, view: &LadderView) {
    let t = Tokens::of(ui);
    let name_of =
        |i: Option<usize>| i.map_or("none yet".to_string(), |i| view.rungs[i].name.clone());
    let held = format!("Held now: {}", name_of(view.highest_held));
    let reached = format!("Reached: {}", name_of(view.highest_reached));
    ui.horizontal_wrapped(|ui| {
        ui.spacing_mut().item_spacing.x = 16.0;
        for (text, tip, set) in [
            (
                held,
                "The highest rung that at least 1% of the organisms performed in their last gestation, now.",
                view.highest_held.is_some(),
            ),
            (
                reached,
                "The highest rung any organism has performed since the history began, whether or not any does now.",
                view.highest_reached.is_some(),
            ),
        ] {
            let (label, value) = text.split_once(": ").unwrap_or((&text, ""));
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 4.0;
                ui.label(egui::RichText::new(format!("{label}:")).color(t.text_weak))
                    .on_hover_text(tip);
                ui.label(
                    egui::RichText::new(value)
                        .family(semibold())
                        .color(if set { t.text } else { t.text_faint }),
                )
                .on_hover_text(tip);
            });
        }
    });
}

/// Elided to one line of at most `width`.
fn one_line(
    ui: &Ui,
    text: &str,
    font: egui::FontId,
    color: Color32,
    width: f32,
) -> std::sync::Arc<egui::Galley> {
    let mut job = egui::text::LayoutJob::simple(text.to_string(), font, color, width);
    job.wrap.max_rows = 1;
    job.wrap.break_anywhere = true;
    ui.painter().layout_job(job)
}

/// The x axis the strips share: where an update is drawn.
struct Axis {
    left: f32,
    width: f32,
    from: f64,
    to: f64,
}

impl Axis {
    fn x(&self, update: f64) -> f32 {
        self.left + ((update - self.from) / (self.to - self.from)) as f32 * self.width
    }

    fn update_at(&self, x: f32) -> f64 {
        self.from + f64::from((x - self.left) / self.width) * (self.to - self.from)
    }
}

/// Draws the rungs, top first, and the axis; returns the condition a click
/// asked for.
fn rungs(
    ui: &mut Ui,
    view: &LadderView,
    n_tasks: usize,
    state_of: &dyn Fn(usize) -> Option<bool>,
) -> Option<(usize, bool)> {
    let t = Tokens::of(ui);
    let full = ui.available_width();
    let font = egui::FontId::proportional(12.5);
    let small = egui::FontId::proportional(11.0);
    // The name column fits the longest name, within bounds.
    let label_w = view
        .rungs
        .iter()
        .map(|r| {
            ui.painter()
                .layout_no_wrap(r.name.clone(), font.clone(), t.text)
                .size()
                .x
        })
        .fold(0.0_f32, f32::max)
        .clamp(36.0, 96.0)
        + 6.0;
    let strip_left = label_w + 8.0;
    let strip_w = (full - strip_left - RIGHT_W - 8.0).max(40.0);
    let (from, mut to) = (
        f64::from(view.updates.first().copied().unwrap_or(0)),
        f64::from(view.updates.last().copied().unwrap_or(1)),
    );
    if to <= from {
        to = from + 1.0;
    }

    // Allocate every row first: the pointer's update is wanted by all.
    let top = ui.next_widget_position();
    let mut slots: Vec<(usize, Rect, egui::Response)> = Vec::new();
    ui.spacing_mut().item_spacing.y = 2.0;
    for (i, rung) in view.rungs.iter().enumerate().rev() {
        let (rect, r) = ui.allocate_exact_size(vec2(full, ROW_H), Sense::click());
        r.widget_info(|| {
            egui::WidgetInfo::selected(
                egui::Role::Button,
                true,
                state_of(rung.task).is_some(),
                format!("Filter on rung {}", rung.name),
            )
        });
        slots.push((i, rect, r));
    }
    let axis = Axis {
        left: top.x + strip_left,
        width: strip_w,
        from,
        to,
    };
    let hover_x = ui
        .input(|i| i.pointer.hover_pos())
        .filter(|p| {
            slots
                .iter()
                .any(|(_, rect, r)| r.hovered() && rect.contains(*p))
        })
        .map(|p| p.x)
        .filter(|x| (axis.left..=axis.left + axis.width).contains(x));

    let mut click = None;
    for (i, rect, r) in &slots {
        let rung = &view.rungs[*i];
        let want = state_of(rung.task);
        paint_row(
            ui, view, rung, *rect, r, &axis, hover_x, want, n_tasks, &font, &small, label_w,
        );
        if r.hovered() {
            ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
        }
        if r.has_focus() {
            ui.painter().rect_stroke(
                r.rect,
                4.0,
                Stroke::new(1.5, t.accent),
                egui::StrokeKind::Inside,
            );
        }
        let idx = hover_x
            .filter(|_| r.hovered())
            .and_then(|x| view.nearest(axis.update_at(x)));
        let tip = tooltip(view, rung, idx, want);
        let r = r.clone().on_hover_text(tip);
        if r.clicked() {
            let shift = click_modifiers(ui).shift;
            click = Some((rung.task, !shift));
        }
    }

    // The shared axis under the last strip.
    let (axis_rect, _) = ui.allocate_exact_size(vec2(full, 16.0), Sense::hover());
    let y = axis_rect.top() + 2.0;
    for (frac, align) in [
        (0.0_f64, egui::Align2::LEFT_TOP),
        (0.5, egui::Align2::CENTER_TOP),
        (1.0, egui::Align2::RIGHT_TOP),
    ] {
        let u = from + (to - from) * frac;
        ui.painter().text(
            pos2(axis.x(u), y),
            align,
            format::compact(u),
            small.clone(),
            t.text_faint,
        );
    }
    ui.painter().text(
        pos2(axis.left + axis.width + 6.0, y),
        egui::Align2::LEFT_TOP,
        "update",
        small.clone(),
        t.text_faint,
    );
    click
}

/// What hovering a rung says: the numbers at the update under the pointer
/// (the newest when the pointer is not over the strip), when the rung was
/// first performed and first held, and what a click does.
fn tooltip(view: &LadderView, rung: &Rung, idx: Option<usize>, want: Option<bool>) -> String {
    let i = idx.unwrap_or(view.updates.len().saturating_sub(1));
    let at = view.updates.get(i).copied().unwrap_or(0);
    let count = rung.counts.get(i).copied().unwrap_or(0);
    let orgs = view.orgs.get(i).copied().unwrap_or(0);
    let share = if orgs > 0 {
        f64::from(count) / f64::from(orgs)
    } else {
        0.0
    };
    let mut tip = format!(
        "{}\nUpdate {}: {} of {} organisms ({})",
        rung.name,
        format::count(i64::from(at)),
        format::count(i64::from(count)),
        format::count(i64::from(orgs)),
        percent_of(share),
    );
    let when = |u: Option<i32>, what: &str| match u {
        Some(u) => format!("\n{what}: update {}", format::count(i64::from(u))),
        None => format!("\n{what}: not yet"),
    };
    tip.push_str(&when(rung.first_reached, "First performed (ring)"));
    tip.push_str(&when(rung.first_held, "First held by 1% or more (dot)"));
    if let (Some(s), Some(m)) = (rung.subset, view.subset_orgs) {
        tip.push_str(&format!(
            "\nMatching organisms now: {} of {}",
            format::count(i64::from(s)),
            format::count(i64::from(m))
        ));
    }
    if !rung.shares_task_with.is_empty() {
        tip.push_str(&format!(
            "\nThe same task as {}: their counts are the same.",
            rung.shares_task_with.join(", ")
        ));
    }
    tip.push_str(match want {
        Some(true) => "\nThe filter keeps organisms that perform it. Click to remove that condition, Shift-click to keep those that do not.",
        Some(false) => "\nThe filter keeps organisms that do not perform it. Click to keep those that do, Shift-click to remove that condition.",
        None => "\nClick to keep only organisms that perform it, Shift-click only those that do not.",
    });
    tip
}

#[allow(clippy::too_many_arguments)]
fn paint_row(
    ui: &Ui,
    view: &LadderView,
    rung: &Rung,
    rect: Rect,
    r: &egui::Response,
    axis: &Axis,
    hover_x: Option<f32>,
    want: Option<bool>,
    n_tasks: usize,
    font: &egui::FontId,
    small: &egui::FontId,
    label_w: f32,
) {
    let t = Tokens::of(ui);
    let p = ui.painter();
    if want.is_some() {
        p.rect_filled(rect, 4.0, t.filter_soft);
    } else if r.hovered() {
        p.rect_filled(rect, 4.0, t.control);
    }
    let never = rung.state == RungState::NeverReached;
    let cy = rect.center().y;

    // The name.
    let galley = one_line(
        ui,
        &rung.name,
        font.clone(),
        if never { t.text_faint } else { t.text },
        label_w,
    );
    p.galley(
        pos2(rect.left() + 4.0, cy - galley.size().y / 2.0),
        galley,
        t.text,
    );

    // The strip.
    let strip = Rect::from_min_max(
        pos2(axis.left, rect.top() + 4.0),
        pos2(axis.left + axis.width, rect.bottom() - 4.0),
    );
    p.rect_filled(strip, 3.0, t.control_active.gamma_multiply(0.5));
    let color = task_color(rung.task, n_tasks, t.dark);
    if never {
        let mut x = strip.left() + 2.0;
        while x < strip.right() - 2.0 {
            p.line_segment(
                [
                    pos2(x, strip.bottom() - 2.0),
                    pos2((x + 3.0).min(strip.right()), strip.bottom() - 2.0),
                ],
                Stroke::new(1.0, t.text_faint),
            );
            x += 6.0;
        }
    } else {
        paint_area(p, view, rung, axis, strip, color);
    }
    // First performed (ring) and first held (dot), on the baseline.
    let base = strip.bottom() - 4.0;
    let in_range = |u: i32| f64::from(u) >= axis.from && f64::from(u) <= axis.to;
    if let Some(u) = rung.first_reached.filter(|&u| in_range(u)) {
        p.circle_stroke(
            pos2(axis.x(f64::from(u)), base),
            3.0,
            Stroke::new(1.4, t.text),
        );
    }
    if let Some(u) = rung.first_held.filter(|&u| in_range(u)) {
        p.circle_filled(pos2(axis.x(f64::from(u)), base), 2.2, t.text);
    }
    if let Some(x) = hover_x {
        p.line_segment(
            [pos2(x, strip.top()), pos2(x, strip.bottom())],
            Stroke::new(1.0, t.text_weak.gamma_multiply(0.8)),
        );
    }

    // The right column: the count and share now, the filter's count under it.
    let right = rect.right() - 4.0;
    let text_right = right - MARK_W;
    let subset_line = rung.subset.zip(view.subset_orgs);
    let y1 = if subset_line.is_some() { cy - 7.0 } else { cy };
    match rung.state {
        RungState::NeverReached => {
            p.text(
                pos2(text_right, y1),
                egui::Align2::RIGHT_CENTER,
                "never",
                font.clone(),
                t.text_faint,
            );
        }
        RungState::Lost => {
            p.text(
                pos2(text_right, y1),
                egui::Align2::RIGHT_CENTER,
                "lost",
                font.clone(),
                t.text_weak,
            );
        }
        RungState::Present => {
            let share = if view.orgs_now > 0 {
                f64::from(rung.now) / f64::from(view.orgs_now)
            } else {
                0.0
            };
            let pct = p.layout_no_wrap(percent_of(share), font.clone(), t.text);
            let pct_w = pct.size().x.max(34.0);
            p.galley(
                pos2(text_right - pct.size().x, y1 - pct.size().y / 2.0),
                pct,
                t.text,
            );
            p.text(
                pos2(text_right - pct_w - 6.0, y1),
                egui::Align2::RIGHT_CENTER,
                format::count(i64::from(rung.now)),
                font.clone(),
                t.text_weak,
            );
        }
    }
    if let Some((s, m)) = subset_line {
        p.text(
            pos2(text_right, cy + 8.0),
            egui::Align2::RIGHT_CENTER,
            format!(
                "{} of {}",
                format::count(i64::from(s)),
                format::count(i64::from(m))
            ),
            small.clone(),
            t.filter,
        );
    }
    if let Some(performs) = want {
        let icon = if performs {
            icons::ICON_CHECK
        } else {
            icons::ICON_BLOCK
        };
        p.text(
            pos2(right - MARK_W / 2.0 + 1.0, cy),
            egui::Align2::CENTER_CENTER,
            icon.codepoint,
            egui::FontId::proportional(14.0),
            t.filter,
        );
    }
}

/// The share of the organisms, as a filled area one column per pixel: where
/// several updates fall in a column the largest share is drawn, where none
/// does the share holds.
fn paint_area(
    p: &egui::Painter,
    view: &LadderView,
    rung: &Rung,
    axis: &Axis,
    strip: Rect,
    color: Color32,
) {
    let n = view.updates.len();
    if n == 0 {
        return;
    }
    let share = |i: usize| -> f32 {
        let o = view.orgs[i];
        if o > 0 {
            rung.counts[i] as f32 / o as f32
        } else {
            0.0
        }
    };
    let h = strip.height();
    let cols = axis.width.floor().max(1.0) as usize;
    let mut tops: Vec<f32> = Vec::with_capacity(cols + 1);
    let mut j = 0usize;
    let mut held = 0.0_f32;
    for c in 0..=cols {
        let edge = axis.update_at(axis.left + c as f32 * axis.width / cols as f32);
        let mut best: Option<f32> = None;
        while j < n && f64::from(view.updates[j]) <= edge {
            let s = share(j);
            best = Some(best.map_or(s, |b: f32| b.max(s)));
            j += 1;
        }
        if let Some(b) = best {
            held = b;
        }
        let mut rise = held * h;
        if held > 0.0 {
            rise = rise.max(MIN_RISE);
        }
        tops.push(strip.bottom() - rise.min(h));
    }
    let mut mesh = egui::Mesh::default();
    let fill = color.gamma_multiply(0.75);
    for (c, w) in tops.windows(2).enumerate() {
        let x0 = axis.left + c as f32 * axis.width / cols as f32;
        let x1 = axis.left + (c + 1) as f32 * axis.width / cols as f32;
        let base = mesh.vertices.len() as u32;
        for (x, y) in [
            (x0, w[0]),
            (x1, w[1]),
            (x1, strip.bottom()),
            (x0, strip.bottom()),
        ] {
            mesh.colored_vertex(pos2(x, y), fill);
        }
        mesh.add_triangle(base, base + 1, base + 2);
        mesh.add_triangle(base, base + 2, base + 3);
    }
    p.add(egui::Shape::mesh(mesh));
    let line: Vec<egui::Pos2> = tops
        .iter()
        .enumerate()
        .map(|(c, &y)| pos2(axis.left + c as f32 * axis.width / cols as f32, y))
        .collect();
    p.add(egui::Shape::line(line, Stroke::new(1.2, color)));
}

#[cfg(test)]
mod tests {
    use super::*;
    use avida_viewmodel::{EnvView, LadderHistory, ReactionView, UpdateRow};

    fn view() -> LadderView {
        let mut h = LadderHistory::default();
        for (u, c) in [(0, 0), (10, 4), (20, 0)] {
            h.push(&UpdateRow {
                update: u,
                orgs: 100,
                task_last_count: vec![c],
                ..UpdateRow::default()
            });
        }
        let env = EnvView {
            reactions: vec![ReactionView {
                name: "NOT".into(),
                task: Some(0),
                value: 1.0,
                last_count: 0,
            }],
            ..EnvView::default()
        };
        build(&h, &env, None)
    }

    #[test]
    fn the_hover_reads_the_update_under_the_pointer() {
        let v = view();
        let tip = tooltip(&v, &v.rungs[0], Some(1), None);
        assert!(
            tip.contains("Update 10: 4 of 100 organisms (4.0%)"),
            "{tip}"
        );
        assert!(tip.contains("First performed (ring): update 10"), "{tip}");
        assert!(
            tip.contains("First held by 1% or more (dot): update 10"),
            "{tip}"
        );
        // Off the strip it reads the newest point, and a lost rung says 0.
        let now = tooltip(&v, &v.rungs[0], None, None);
        assert!(now.contains("Update 20: 0 of 100 organisms (0%)"), "{now}");
    }
}
