//! The comparison screen: a batch of runs (`<arm>/seed-<N>/`) side by
//! side. Four panels on one page.
//!
//! 1. **Rungs reached**: per rung (the top rung first) and per arm, one dot
//!    for each seed, so that n is visible. A filled dot holds the rung at
//!    the end of the run, a ring held it earlier and lost it, a faint dot
//!    never held it. A line under a dot marks a run that is still going.
//! 2. **Highest rung over time**: one thin step line per seed in the colour
//!    of its arm and the arm's median thick.
//! 3. **Measures**: one strip per scalar measure, a dot per seed in each
//!    arm's column, the median as a bar and the quartiles as a line.
//! 4. **Runs**: a table; selecting a run shows its folder and a button that
//!    copies the path.
//!
//! The numbers come from `avida_viewmodel::compare`; this module draws
//! them. Text is kept to names and numbers, what they mean is in the
//! hovers. A p-value is always shown with the sample sizes and the test's
//! name, and nothing here calls a difference significant.

use avida_viewmodel::compare::{Measure, MeasureSummary, Reading, Run};
use egui::{
    pos2, vec2, Align, Align2, Color32, FontId, Layout, Rect, RichText, Role, Sense, Shape, Stroke,
    Ui, WidgetInfo,
};
use egui_material_icons::icons;

use crate::model::compare::{arm_rgb, CompareState};
use crate::model::format;
use crate::ui::theme::{semibold, Tokens};
use crate::ui::widgets;

/// What the screen asks the application to do.
#[derive(Default)]
pub struct Outcome {
    pub close: bool,
    pub reload: bool,
    pub open_other: bool,
    pub copy: Option<String>,
}

const ROW_H: f32 = 22.0;
/// The most room a seed's dot takes in the rung panel.
const DOT_STEP_MAX: f32 = 18.0;
const LABEL_MAX_W: f32 = 132.0;

fn arm_color(i: usize, t: &Tokens) -> Color32 {
    let c = arm_rgb(i, t.dark);
    Color32::from_rgb(c[0], c[1], c[2])
}

fn small() -> FontId {
    FontId::proportional(11.0)
}

/// The text cut to one line of at most `width` points.
fn elide(
    ui: &Ui,
    text: &str,
    font: FontId,
    color: Color32,
    width: f32,
) -> std::sync::Arc<egui::Galley> {
    let mut job = egui::text::LayoutJob::simple(text.to_string(), font, color, width.max(8.0));
    job.wrap.max_rows = 1;
    job.wrap.break_anywhere = true;
    ui.painter().layout_job(job)
}

pub use avida_viewmodel::compare::p_text;

pub fn show(ui: &mut Ui, st: &mut CompareState) -> Outcome {
    let t = Tokens::of(ui);
    let mut out = Outcome::default();
    egui::Panel::top("compare-bar")
        .exact_size(48.0)
        .frame(
            egui::Frame::new()
                .fill(t.surface)
                .inner_margin(egui::Margin::symmetric(14, 0)),
        )
        .show(ui, |ui| bar(ui, st, &mut out));
    egui::CentralPanel::default()
        .frame(
            egui::Frame::new()
                .fill(t.bg)
                .inner_margin(egui::Margin::symmetric(16, 10)),
        )
        .show(ui, |ui| {
            egui::ScrollArea::vertical()
                .id_salt("compare-scroll")
                .auto_shrink([false, false])
                .show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    page(ui, st, &mut out);
                });
        });
    out
}

fn bar(ui: &mut Ui, st: &CompareState, out: &mut Outcome) {
    let t = Tokens::of(ui);
    ui.horizontal_centered(|ui| {
        ui.spacing_mut().item_spacing.x = 8.0;
        ui.label(
            icons::ICON_COMPARE_ARROWS
                .rich_text()
                .size(20.0)
                .color(t.accent),
        );
        ui.label(
            RichText::new("Compare runs")
                .family(semibold())
                .size(16.0)
                .color(t.text),
        );
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            if widgets::icon_text_button(
                ui,
                icons::ICON_CLOSE,
                "Close",
                "Leave the comparison and return to the experiment (Esc)",
            )
            .clicked()
            {
                out.close = true;
            }
            if widgets::icon_text_button(
                ui,
                icons::ICON_FOLDER_OPEN,
                "Compare runs…",
                "Choose another folder of runs",
            )
            .clicked()
            {
                out.open_other = true;
            }
            if widgets::icon_text_button(
                ui,
                icons::ICON_REFRESH,
                "Reload",
                "Read the folder again, for runs that are still writing their files",
            )
            .clicked()
            {
                out.reload = true;
            }
            let path = st.dir.display().to_string();
            let room = (ui.available_width() - 190.0).max(60.0);
            let shown = crate::ui::start::middle_truncate(&path, ((room / 7.0) as usize).max(12));
            ui.add(egui::Label::new(
                RichText::new(shown)
                    .monospace()
                    .size(12.0)
                    .color(t.text_weak),
            ))
            .on_hover_text(path);
        });
    });
}

fn page(ui: &mut Ui, st: &mut CompareState, out: &mut Outcome) {
    let t = Tokens::of(ui);
    summary_lines(ui, st, &t);
    ui.add_space(6.0);
    controls(ui, st, &t);
    ui.add_space(8.0);
    widgets::card(ui, |ui| {
        ui.set_width(ui.available_width());
        rung_panel(ui, st, &t);
    });
    ui.add_space(10.0);
    widgets::card(ui, |ui| {
        ui.set_width(ui.available_width());
        over_time_panel(ui, st, &t);
    });
    ui.add_space(10.0);
    widgets::card(ui, |ui| {
        ui.set_width(ui.available_width());
        measures_panel(ui, st, &t);
    });
    ui.add_space(10.0);
    widgets::card(ui, |ui| {
        ui.set_width(ui.available_width());
        table_panel(ui, st, &t, out);
    });
    ui.add_space(12.0);
}

fn summary_lines(ui: &mut Ui, st: &CompareState, t: &Tokens) {
    let b = &st.batch;
    let total = b.run_count();
    let done = b.finished_count();
    let running = total - done;
    let arms = b.arms.len();
    let mut line = format!(
        "{} in {}: {} finished, {} still running.",
        format::plural(total as u64, "run"),
        format::plural(arms as u64, "arm"),
        done,
        running
    );
    if running > 0 {
        line.push_str(" A running run is shown as of its last update.");
    }
    ui.label(RichText::new(line).color(t.text_weak));
    for n in &b.notes {
        ui.label(RichText::new(n).small().color(t.text_faint));
    }
    let bad = b.runs().filter(|r| !r.problems.is_empty()).count();
    if bad > 0 {
        ui.label(
            RichText::new(format!(
                "{} {} a file that is missing or could not be read. The table says which.",
                bad,
                if bad == 1 { "run has" } else { "runs have" }
            ))
            .small()
            .color(t.warning),
        );
    }
}

fn controls(ui: &mut Ui, st: &mut CompareState, t: &Tokens) {
    ui.horizontal_wrapped(|ui| {
        ui.spacing_mut().item_spacing = vec2(14.0, 6.0);
        // The legend: the colour of each arm and its seeds.
        for (i, arm) in st.batch.arms.iter().enumerate() {
            let shown = arm.runs.iter().filter(|r| st.takes(r)).count();
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 5.0;
                let (rect, _) = ui.allocate_exact_size(vec2(12.0, 14.0), Sense::hover());
                ui.painter().circle_filled(rect.center(), 5.0, arm_color(i, t));
                let r = ui.label(RichText::new(&arm.name).family(semibold()).color(t.text));
                r.on_hover_text(format!(
                    "Arm {} (the folder name). {} of its {} in the panels.",
                    arm.name,
                    format::plural(shown as u64, "run"),
                    arm.runs.len()
                ));
                ui.label(RichText::new(format!("n = {shown}")).small().color(t.text_weak));
            });
        }
        let mut only = st.finished_only;
        let r = ui
            .checkbox(&mut only, "Finished runs only")
            .on_hover_text("Leave out runs that have no .done file. Otherwise a running run counts with its values so far.");
        if r.changed() {
            st.set_finished_only(only);
        }
        if st.batch.arms.len() > 2 {
            let names: Vec<String> = st.batch.arms.iter().map(|a| a.name.clone()).collect();
            let (mut a, mut b) = st.pair;
            ui.label(RichText::new("p-values compare").color(t.text_weak))
                .on_hover_text("With more than two arms the tests compare the two you pick.");
            for (id, name, which) in [
                ("pair-a", "First arm to compare", &mut a),
                ("pair-b", "Second arm to compare", &mut b),
            ] {
                let r = egui::ComboBox::from_id_salt(id)
                    .selected_text(&names[*which])
                    .width(110.0)
                    .show_ui(ui, |ui| {
                        for (k, n) in names.iter().enumerate() {
                            ui.selectable_value(which, k, n);
                        }
                    })
                    .response;
                widgets::name_widget(&r, Role::ComboBox, name);
            }
            st.set_pair(a, b);
        }
    });
}

fn title(ui: &mut Ui, text: &str, tip: &str, t: &Tokens) {
    ui.label(
        RichText::new(text)
            .family(semibold())
            .size(14.5)
            .color(t.text),
    )
    .on_hover_text(tip);
    ui.add_space(4.0);
}

/// "NAME held at the end" and the like, for a dot's hover and accessible name.
fn dot_text(st: &CompareState, run: &Run, rung: usize) -> String {
    let name = &st.batch.rungs[rung];
    let state = if !run.has_ladder() {
        "no data yet".to_string()
    } else {
        let at_end = run
            .points
            .last()
            .is_some_and(|p| avida_viewmodel::ladder::is_held(p.counts[rung], p.orgs));
        match (at_end, run.first_held[rung]) {
            (true, _) => "held at the end".to_string(),
            (false, Some(_)) => "held earlier, not at the end".to_string(),
            (false, None) => "never held".to_string(),
        }
    };
    let mut s = format!("{}, seed {}: {name} {state}", run.arm, run.seed);
    if run.has_ladder() {
        if let Some(u) = run.first_held[rung] {
            s.push_str(&format!(
                "\nFirst held at update {}",
                format::count(i64::from(u))
            ));
        }
        if let Some(u) = run.first_reached[rung] {
            if run.first_held[rung].is_none() {
                s.push_str(&format!(
                    "\nPerformed by a few organisms (under 1%) from update {}",
                    format::count(i64::from(u))
                ));
            }
        }
    }
    s.push_str(&format!("\n{}", run.status()));
    s
}

fn rung_panel(ui: &mut Ui, st: &CompareState, t: &Tokens) {
    title(
        ui,
        "Rungs reached",
        "For each rung, one dot per seed. A rung counts as held when at least 1% of the organisms perform its task (the same rule as the Ladder view). Filled: held at the end of the run. Ring: held at some update, not at the end. Faint: never held. A line under a dot: the run is still going.",
        t,
    );
    let b = &st.batch;
    if b.rungs.is_empty() {
        ui.label(RichText::new("No run has a readable tasks.dat with a population in count.dat, so there is no ladder to show.").color(t.text_weak));
        return;
    }
    let arms = b.arms.len().max(1);
    let full = ui.available_width();
    let font = FontId::proportional(12.5);
    let label_w = b
        .rungs
        .iter()
        .map(|n| {
            ui.painter()
                .layout_no_wrap(n.clone(), font.clone(), t.text)
                .size()
                .x
        })
        .fold(0.0_f32, f32::max)
        .clamp(40.0, LABEL_MAX_W)
        + 8.0;
    let p_w = if st.comparison.is_some() { 84.0 } else { 0.0 };
    let gap = 12.0;
    let count_w = 50.0;
    let most = b
        .arms
        .iter()
        .map(|a| a.runs.iter().filter(|r| st.takes(r)).count())
        .max()
        .unwrap_or(1)
        .max(1);
    // Wide enough for the dots at their largest, and no wider: the arms
    // stay next to each other.
    let cell_w = ((full - label_w - p_w - gap * (arms as f32)) / arms as f32)
        .min(most as f32 * DOT_STEP_MAX + count_w + 4.0)
        .max(60.0);

    // Header: the arms.
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 0.0;
        ui.add_space(label_w);
        for (i, arm) in b.arms.iter().enumerate() {
            let (rect, _) = ui.allocate_exact_size(vec2(cell_w + gap, 18.0), Sense::hover());
            ui.painter().circle_filled(pos2(rect.left() + 5.0, rect.center().y), 4.0, arm_color(i, t));
            let g = elide(ui, &arm.name, small(), t.text_weak, cell_w - 14.0);
            ui.painter().galley(pos2(rect.left() + 13.0, rect.center().y - g.size().y / 2.0), g, t.text_weak);
        }
        if let Some(c) = &st.comparison {
            let (rect, r) = ui.allocate_exact_size(vec2(p_w, 18.0), Sense::hover());
            ui.painter().text(
                pos2(rect.right(), rect.center().y),
                Align2::RIGHT_CENTER,
                "p (ever)",
                small(),
                t.text_faint,
            );
            r.on_hover_text(format!(
                "Fisher's exact test, two-sided, comparing {} with {}: seeds that held the rung at some update against seeds that did not. Shown with the counts; it is a description of these seeds, not a claim about significance.",
                b.arms[c.a].name, b.arms[c.b].name
            ));
        }
    });

    for rung in (0..b.rungs.len()).rev() {
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 0.0;
            let (rect, r) = ui.allocate_exact_size(vec2(label_w, ROW_H), Sense::hover());
            let g = elide(ui, &b.rungs[rung], font.clone(), t.text, label_w - 8.0);
            ui.painter().galley(pos2(rect.left(), rect.center().y - g.size().y / 2.0), g, t.text);
            r.widget_info(|| WidgetInfo::labeled(Role::Label, true, &b.rungs[rung]));
            r.on_hover_text(format!("Rung {} of {}: {}", rung + 1, b.rungs.len(), b.rungs[rung]));
            for (i, arm) in b.arms.iter().enumerate() {
                let runs: Vec<&Run> = arm.runs.iter().filter(|r| st.takes(r)).collect();
                let n = runs.len().max(1);
                let (rect, _) = ui.allocate_exact_size(vec2(cell_w + gap, ROW_H), Sense::hover());
                let step = ((cell_w - count_w) / n as f32).clamp(3.0, DOT_STEP_MAX);
                let radius = (step * 0.36).clamp(1.5, 5.0);
                let color = arm_color(i, t);
                for (k, run) in runs.iter().enumerate() {
                    let c = pos2(rect.left() + step * (k as f32 + 0.5), rect.center().y - 1.0);
                    let hit = Rect::from_center_size(c, vec2(step.max(8.0), ROW_H));
                    let resp = ui.interact(hit, ui.scope_id().with(("rung-dot", rung, i, k)), Sense::hover());
                    let text = dot_text(st, run, rung);
                    let p = ui.painter();
                    if !run.has_ladder() {
                        p.line_segment(
                            [c - vec2(radius, 0.0), c + vec2(radius, 0.0)],
                            Stroke::new(1.0, t.text_faint),
                        );
                    } else {
                        let at_end = run.points.last().is_some_and(|q| {
                            avida_viewmodel::ladder::is_held(q.counts[rung], q.orgs)
                        });
                        if at_end {
                            p.circle_filled(c, radius, color);
                        } else if run.first_held[rung].is_some() {
                            p.circle_stroke(c, radius - 0.6, Stroke::new(1.6, color));
                        } else {
                            p.circle_filled(c, radius.min(2.0), t.border_strong);
                        }
                        if !run.finished {
                            p.line_segment(
                                [c + vec2(-radius, radius + 2.5), c + vec2(radius, radius + 2.5)],
                                Stroke::new(1.2, t.text_weak),
                            );
                        }
                    }
                    resp.widget_info(|| WidgetInfo::labeled(Role::Image, true, &text));
                    resp.on_hover_text(text);
                }
                let s = &st.summary[i];
                let c = s.rungs[rung];
                let count_rect = Rect::from_min_size(
                    pos2(rect.left() + step * runs.len() as f32 + 6.0, rect.top()),
                    vec2(count_w, ROW_H),
                );
                let label = format!("{} of {}", c.ever, s.with_ladder);
                let r = ui.interact(count_rect, ui.scope_id().with(("rung-count", rung, i)), Sense::hover());
                ui.painter().text(
                    count_rect.left_center(),
                    Align2::LEFT_CENTER,
                    &label,
                    small(),
                    if c.ever == 0 { t.text_faint } else { t.text_weak },
                );
                let tip = format!(
                    "{}: {} of {} seeds held {} at some update; {} hold it at the end.",
                    arm.name, c.ever, s.with_ladder, b.rungs[rung], c.at_end
                );
                r.widget_info(|| WidgetInfo::labeled(Role::Label, true, &tip));
                r.on_hover_text(tip);
            }
            if let Some(c) = &st.comparison {
                let (rect, r) = ui.allocate_exact_size(vec2(p_w, ROW_H), Sense::hover());
                let (a, bb) = (c.a, c.b);
                let (x, y) = (st.summary[a].rungs[rung], st.summary[bb].rungs[rung]);
                let (na, nb) = (st.summary[a].with_ladder, st.summary[bb].with_ladder);
                let p = c.rungs[rung].p_ever;
                ui.painter().text(
                    pos2(rect.right(), rect.center().y),
                    Align2::RIGHT_CENTER,
                    p_text(p),
                    small(),
                    t.text_weak,
                );
                let tip = format!(
                    "Fisher's exact test, two-sided: {} held {} in {} of {} seeds, {} in {} of {}. p = {:.4}. At the end of the run: {} of {} against {} of {}, p = {:.4}.",
                    b.arms[a].name, b.rungs[rung], x.ever, na, b.arms[bb].name, y.ever, nb, p,
                    x.at_end, na, y.at_end, nb, c.rungs[rung].p_end
                );
                r.widget_info(|| WidgetInfo::labeled(Role::Label, true, format!("{} p = {}", b.rungs[rung], p_text(p))));
                r.on_hover_text(tip);
            }
        });
    }
    ui.add_space(4.0);
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 5.0;
        let (rect, _) = ui.allocate_exact_size(vec2(10.0, 12.0), Sense::hover());
        ui.painter().circle_filled(rect.center(), 4.0, t.text_weak);
        ui.label(RichText::new("held at the end").small().color(t.text_weak));
        ui.add_space(8.0);
        let (rect, _) = ui.allocate_exact_size(vec2(10.0, 12.0), Sense::hover());
        ui.painter()
            .circle_stroke(rect.center(), 3.5, Stroke::new(1.6, t.text_weak));
        ui.label(RichText::new("held earlier").small().color(t.text_weak));
        ui.add_space(8.0);
        let (rect, _) = ui.allocate_exact_size(vec2(10.0, 12.0), Sense::hover());
        ui.painter()
            .circle_filled(rect.center(), 2.0, t.border_strong);
        ui.label(RichText::new("never").small().color(t.text_weak));
        ui.add_space(8.0);
        let (rect, _) = ui.allocate_exact_size(vec2(10.0, 12.0), Sense::hover());
        ui.painter().line_segment(
            [
                pos2(rect.left(), rect.center().y),
                pos2(rect.right(), rect.center().y),
            ],
            Stroke::new(1.2, t.text_weak),
        );
        ui.label(
            RichText::new("under a dot: still running")
                .small()
                .color(t.text_weak),
        );
    });
}

fn level_name(st: &CompareState, level: u32) -> String {
    match level {
        0 => "none".to_string(),
        l => st
            .batch
            .rungs
            .get(l as usize - 1)
            .cloned()
            .unwrap_or_default(),
    }
}

fn over_time_panel(ui: &mut Ui, st: &CompareState, t: &Tokens) {
    title(
        ui,
        "Highest rung over time",
        "Per seed, the highest rung that at least 1% of the organisms performed at each recorded update (a thin line in the arm's colour, offset a little so that seeds on the same path all show). The thick line is the arm's median over the seeds that have a row at that update. A dot is where a running run stands now.",
        t,
    );
    let b = &st.batch;
    let levels = b.rungs.len();
    if levels == 0 || st.max_update <= 0 {
        ui.label(RichText::new("No updates recorded yet.").color(t.text_weak));
        return;
    }
    let width = ui.available_width();
    let height = 210.0;
    let font = small();
    let left = (b
        .rungs
        .iter()
        .chain(std::iter::once(&"none".to_string()))
        .map(|n| {
            ui.painter()
                .layout_no_wrap(n.clone(), font.clone(), t.text_weak)
                .size()
                .x
        })
        .fold(0.0_f32, f32::max))
    .clamp(30.0, 100.0)
        + 10.0;
    let (outer, resp) = ui.allocate_exact_size(vec2(width, height + 34.0), Sense::hover());
    let plot = Rect::from_min_max(
        pos2(outer.left() + left, outer.top() + 4.0),
        pos2(outer.right() - 10.0, outer.top() + height),
    );
    let max_u = f64::from(st.max_update.max(1));
    let x_of = |u: f64| plot.left() + (u / max_u) as f32 * plot.width();
    let y_of = |l: f32| plot.bottom() - l / levels as f32 * plot.height();
    let p = ui.painter().with_clip_rect(outer.expand(2.0));
    // Grid and the rung names.
    for l in 0..=levels {
        let y = y_of(l as f32);
        p.line_segment(
            [pos2(plot.left(), y), pos2(plot.right(), y)],
            Stroke::new(1.0, if l == 0 { t.border_strong } else { t.border }),
        );
        let g = elide(
            ui,
            &level_name(st, l as u32),
            font.clone(),
            t.text_weak,
            left - 10.0,
        );
        p.galley(
            pos2(plot.left() - 6.0 - g.size().x, y - g.size().y / 2.0),
            g,
            t.text_weak,
        );
    }
    for (frac, align) in [
        (0.0, Align2::LEFT_TOP),
        (0.5, Align2::CENTER_TOP),
        (1.0, Align2::RIGHT_TOP),
    ] {
        let u = max_u * frac;
        p.text(
            pos2(x_of(u), plot.bottom() + 4.0),
            align,
            format::compact(u),
            font.clone(),
            t.text_faint,
        );
    }
    p.text(
        pos2(plot.right(), plot.bottom() + 4.0 + 12.0),
        Align2::RIGHT_TOP,
        "update",
        font.clone(),
        t.text_faint,
    );

    // The pointer.
    let hover = resp
        .hover_pos()
        .or_else(|| ui.input(|i| i.pointer.hover_pos()))
        .filter(|h| plot.expand2(vec2(0.0, 4.0)).contains(*h) && resp.hovered());
    let hover_u = hover.map(|h| f64::from((h.x - plot.left()) / plot.width()) * max_u);

    let mut nearest: Option<(usize, usize, f32)> = None;
    for (ai, arm) in b.arms.iter().enumerate() {
        let color = arm_color(ai, t);
        let runs: Vec<(usize, &Run)> = arm
            .runs
            .iter()
            .enumerate()
            .filter(|(_, r)| st.takes(r) && r.has_ladder())
            .collect();
        let n = runs.len().max(1);
        for (k, (ri, run)) in runs.iter().enumerate() {
            let off = ((k as f32 + 0.5) / n as f32 - 0.5) * 8.0;
            let mut pts: Vec<egui::Pos2> = Vec::new();
            let mut prev: Option<u32> = None;
            for &(u, l) in &run.levels {
                let x = x_of(f64::from(u));
                if let Some(pl) = prev {
                    pts.push(pos2(x, y_of(pl as f32) + off));
                }
                pts.push(pos2(x, y_of(l as f32) + off));
                prev = Some(l);
            }
            if let (Some(h), Some(u)) = (hover, hover_u) {
                if let Some(l) = run.level_at(u as i32) {
                    let d = (y_of(l as f32) + off - h.y).abs();
                    if d < 10.0 && nearest.is_none_or(|(_, _, best)| d < best) {
                        nearest = Some((ai, *ri, d));
                    }
                }
            }
            p.add(Shape::line(
                pts.clone(),
                Stroke::new(1.1, color.gamma_multiply(0.55)),
            ));
            if !run.finished {
                if let Some(last) = pts.last() {
                    p.circle_filled(*last, 3.0, color);
                    p.circle_stroke(*last, 3.0, Stroke::new(1.0, t.surface));
                }
            }
        }
    }
    if let Some((ai, ri, _)) = nearest {
        let run = &b.arms[ai].runs[ri];
        let n = b.arms[ai]
            .runs
            .iter()
            .filter(|r| st.takes(r) && r.has_ladder())
            .count()
            .max(1);
        let k = b.arms[ai].runs[..ri]
            .iter()
            .filter(|r| st.takes(r) && r.has_ladder())
            .count();
        let off = ((k as f32 + 0.5) / n as f32 - 0.5) * 8.0;
        let mut pts: Vec<egui::Pos2> = Vec::new();
        let mut prev: Option<u32> = None;
        for &(u, l) in &run.levels {
            let x = x_of(f64::from(u));
            if let Some(pl) = prev {
                pts.push(pos2(x, y_of(pl as f32) + off));
            }
            pts.push(pos2(x, y_of(l as f32) + off));
            prev = Some(l);
        }
        p.add(Shape::line(pts, Stroke::new(2.2, arm_color(ai, t))));
    }
    // The medians on top.
    for (ai, series) in st.medians.iter().enumerate() {
        let mut pts: Vec<egui::Pos2> = Vec::new();
        let mut prev: Option<f64> = None;
        for m in series {
            let x = x_of(f64::from(m.update));
            if let Some(pl) = prev {
                pts.push(pos2(x, y_of(pl as f32)));
            }
            pts.push(pos2(x, y_of(m.median as f32)));
            prev = Some(m.median);
        }
        // Dark underlay so the thick line holds against the thin ones.
        p.add(Shape::line(pts.clone(), Stroke::new(4.2, t.surface)));
        p.add(Shape::line(pts, Stroke::new(2.6, arm_color(ai, t))));
    }
    if let (Some(h), Some(u)) = (hover, hover_u) {
        p.line_segment(
            [pos2(h.x, plot.top()), pos2(h.x, plot.bottom())],
            Stroke::new(1.0, t.text_weak.gamma_multiply(0.8)),
        );
        let mut tip = format!("Update {}", format::count(u.round() as i64));
        for (ai, arm) in b.arms.iter().enumerate() {
            let m = st.medians[ai]
                .iter()
                .rev()
                .find(|m| f64::from(m.update) <= u);
            match m {
                Some(m) => tip.push_str(&format!(
                    "\n{}: median {} (of {})",
                    arm.name,
                    level_name(st, m.median.round() as u32),
                    format::plural(m.n as u64, "seed")
                )),
                None => tip.push_str(&format!("\n{}: no data", arm.name)),
            }
        }
        if let Some((ai, ri, _)) = nearest {
            let run = &b.arms[ai].runs[ri];
            let l = run.level_at(u as i32).unwrap_or(0);
            tip.push_str(&format!(
                "\nThe line near the pointer: {}, seed {}: {} ({})",
                run.arm,
                run.seed,
                level_name(st, l),
                run.status()
            ));
        }
        resp.clone()
            .widget_info(|| WidgetInfo::labeled(Role::Image, true, "Highest rung over time"));
        resp.on_hover_text(tip);
    } else {
        resp.widget_info(|| WidgetInfo::labeled(Role::Image, true, "Highest rung over time"));
    }
}

fn nice_step(raw: f64) -> f64 {
    let mag = 10f64.powf(raw.max(1e-12).log10().floor());
    let f = raw / mag;
    let nice = if f <= 1.0 {
        1.0
    } else if f <= 2.0 {
        2.0
    } else if f <= 5.0 {
        5.0
    } else {
        10.0
    };
    nice * mag
}

fn measures_panel(ui: &mut Ui, st: &CompareState, t: &Tokens) {
    title(
        ui,
        "Measures",
        "One dot per seed in each arm's column, at the value of the last row of that run's file. A bar marks the median and a thin line the quartiles (interpolated). A ring is a run that is still going. A measure can be undefined for a seed (for example the reuse index with fewer than two tasks); that seed has no dot and is counted under the arm.",
        t,
    );
    let avail = ui.available_width();
    let gap = 12.0;
    let cols = ((avail / 215.0) as usize).clamp(1, 5);
    let cell_w = (avail - gap * (cols as f32 - 1.0)) / cols as f32;
    let measures = Measure::STRIPS;
    for chunk in measures.chunks(cols) {
        ui.horizontal_top(|ui| {
            ui.spacing_mut().item_spacing.x = gap;
            for &m in chunk {
                ui.allocate_ui_with_layout(vec2(cell_w, 0.0), Layout::top_down(Align::Min), |ui| {
                    ui.set_width(cell_w);
                    strip(ui, st, t, m, cell_w);
                });
            }
        });
        ui.add_space(8.0);
    }
}

fn reading_text(r: Reading) -> String {
    match r {
        Reading::Value(v, u) => format!(
            "{} (update {})",
            format::real(v),
            format::count(i64::from(u))
        ),
        Reading::Undefined(u) => format!("undefined at update {}", format::count(i64::from(u))),
        Reading::NoRows => "no row yet".to_string(),
        Reading::NotRecorded => "not recorded".to_string(),
    }
}

fn strip(ui: &mut Ui, st: &CompareState, t: &Tokens, m: Measure, width: f32) {
    let b = &st.batch;
    ui.label(RichText::new(m.label()).family(semibold()).color(t.text))
        .on_hover_text(m.description());
    if !b.recorded(m) {
        ui.label(RichText::new("Not recorded").color(t.text_faint))
            .on_hover_text(format!("No run has {}.", m.file_hint()));
        return;
    }
    let arms = b.arms.len().max(1);
    let plot_h = 128.0;
    let name_h = 30.0;
    let left = 34.0;
    let (outer, _) = ui.allocate_exact_size(vec2(width, plot_h + name_h), Sense::hover());
    let plot = Rect::from_min_max(
        pos2(outer.left() + left, outer.top() + 6.0),
        pos2(outer.right() - 4.0, outer.top() + plot_h),
    );
    let values: Vec<f64> = b
        .runs()
        .filter(|r| st.takes(r))
        .filter_map(|r| r.reading(m).value())
        .collect();
    let hi = values.iter().copied().fold(0.0_f64, f64::max);
    let step = nice_step((hi.max(if m.integer() { 3.0 } else { 0.1 })) / 3.0);
    let top = (hi / step).ceil().max(1.0) * step;
    let y_of = |v: f64| plot.bottom() - (v / top) as f32 * plot.height();
    let p = ui.painter().with_clip_rect(outer.expand(2.0));
    let mut tick = 0.0;
    while tick <= top + step * 0.01 {
        let y = y_of(tick);
        p.line_segment(
            [pos2(plot.left(), y), pos2(plot.right(), y)],
            Stroke::new(
                1.0,
                if tick == 0.0 {
                    t.border_strong
                } else {
                    t.border
                },
            ),
        );
        p.text(
            pos2(plot.left() - 5.0, y),
            Align2::RIGHT_CENTER,
            format::real((tick * 1e6).round() / 1e6),
            small(),
            t.text_faint,
        );
        tick += step;
    }
    let col_w = plot.width() / arms as f32;
    for (ai, arm) in b.arms.iter().enumerate() {
        let color = arm_color(ai, t);
        let cx = plot.left() + col_w * (ai as f32 + 0.5);
        // Name and the counts that are not dots, under the column.
        let g = elide(ui, &arm.name, small(), t.text_weak, col_w - 2.0);
        p.galley(
            pos2(cx - g.size().x / 2.0, plot.bottom() + 5.0),
            g,
            t.text_weak,
        );
        let s: &MeasureSummary = &st.summary[ai].measures[m.index()];
        let mut notes: Vec<String> = Vec::new();
        if s.undefined > 0 {
            notes.push(format!("{} undefined", s.undefined));
        }
        if s.no_rows + s.not_recorded > 0 {
            notes.push(format!("{} without", s.no_rows + s.not_recorded));
        }
        if !notes.is_empty() {
            let g = elide(
                ui,
                &notes.join(", "),
                FontId::proportional(10.0),
                t.text_faint,
                col_w - 2.0,
            );
            p.galley(
                pos2(cx - g.size().x / 2.0, plot.bottom() + 17.0),
                g,
                t.text_faint,
            );
        }
        // The dots, spread sideways where they would overlap.
        let radius = 4.0;
        let mut items: Vec<(f32, &Run)> = arm
            .runs
            .iter()
            .filter(|r| st.takes(r))
            .filter_map(|r| r.reading(m).value().map(|v| (y_of(v), r)))
            .collect();
        items.sort_by(|a, b| a.0.total_cmp(&b.0));
        let mut placed: Vec<(f32, f32)> = Vec::new();
        for (y, run) in &items {
            let mut k = 0;
            let off = loop {
                let off = if k == 0 {
                    0.0
                } else {
                    ((k + 1) / 2) as f32 * radius * 2.2 * if k % 2 == 1 { 1.0 } else { -1.0 }
                };
                if placed.iter().all(|&(py, px)| {
                    (py - y).abs() > radius * 2.0 || (px - off).abs() > radius * 2.0
                }) {
                    break off;
                }
                k += 1;
                if k > 40 {
                    break off;
                }
            };
            placed.push((*y, off));
            let c = pos2(
                cx + off.clamp(-col_w / 2.0 + radius, col_w / 2.0 - radius),
                *y,
            );
            let resp = ui.interact(
                Rect::from_center_size(c, vec2(11.0, 11.0)),
                ui.scope_id()
                    .with(("dot", m.index(), ai, run.seed.as_str())),
                Sense::hover(),
            );
            if run.finished {
                p.circle_filled(c, radius, color);
                p.circle_stroke(c, radius, Stroke::new(1.0, t.surface));
            } else {
                p.circle_stroke(c, radius - 0.5, Stroke::new(1.6, color));
            }
            let text = format!(
                "{}, seed {}: {}: {}\n{}",
                run.arm,
                run.seed,
                m.label(),
                reading_text(run.reading(m)),
                run.status()
            );
            resp.widget_info(|| WidgetInfo::labeled(Role::Image, true, &text));
            resp.on_hover_text(text);
        }
        if let Some(sp) = s.spread {
            let (ym, y1, y3) = (y_of(sp.median), y_of(sp.q1), y_of(sp.q3));
            if sp.n >= 2 && (y1 - y3).abs() > 0.5 {
                p.line_segment(
                    [pos2(cx + 15.0, y1), pos2(cx + 15.0, y3)],
                    Stroke::new(1.6, t.text_weak),
                );
                for y in [y1, y3] {
                    p.line_segment(
                        [pos2(cx + 12.5, y), pos2(cx + 17.5, y)],
                        Stroke::new(1.6, t.text_weak),
                    );
                }
            }
            p.line_segment(
                [pos2(cx - 14.0, ym), pos2(cx + 14.0, ym)],
                Stroke::new(2.4, t.text),
            );
            let r = ui.interact(
                Rect::from_center_size(pos2(cx, ym), vec2(34.0, 8.0)),
                ui.scope_id().with(("median", m.index(), ai)),
                Sense::hover(),
            );
            let tip = format!(
                "{}, {}: median {}, quartiles {} to {}, range {} to {}, n = {}.",
                arm.name,
                m.label(),
                format::real(sp.median),
                format::real(sp.q1),
                format::real(sp.q3),
                format::real(sp.min),
                format::real(sp.max),
                sp.n
            );
            r.widget_info(|| WidgetInfo::labeled(Role::Image, true, &tip));
            r.on_hover_text(tip);
        } else {
            p.text(
                pos2(cx, plot.center().y),
                Align2::CENTER_CENTER,
                "none",
                small(),
                t.text_faint,
            );
        }
    }
    if let Some(c) = &st.comparison {
        let test = c.measures[m.index()].test;
        let text = match test {
            Some(r) => format!("p = {}, n = {} and {}", p_text(r.p), r.n_a, r.n_b),
            None => "no test: an arm has no value".to_string(),
        };
        let resp = ui.label(RichText::new(text).small().color(t.text_weak));
        if let Some(r) = test {
            resp.on_hover_text(format!(
                "Mann-Whitney U test, two-sided, {} with {} ({}). U = {} and {}. A description of these seeds, not a claim about significance. Values of undefined seeds are left out.",
                b.arms[c.a].name,
                b.arms[c.b].name,
                r.method.label(),
                format::real(r.u_a),
                format::real(r.u_b)
            ));
        }
    }
}

fn table_panel(ui: &mut Ui, st: &mut CompareState, t: &Tokens, out: &mut Outcome) {
    title(
        ui,
        "Runs",
        "One row per seed folder. Select a row to see its folder. A run without a .done file is shown as running, with the update of its last row.",
        t,
    );
    let rung_name = |i: Option<usize>, run: &Run| -> String {
        if !run.has_ladder() {
            "-".to_string()
        } else {
            i.map_or("none".to_string(), |i| st.batch.rungs[i].clone())
        }
    };
    let mut pick: Option<(usize, usize)> = None;
    egui::Grid::new("compare-runs")
        .num_columns(7)
        .spacing(vec2(18.0, 4.0))
        .min_row_height(ROW_H)
        .show(ui, |ui| {
            for h in [
                "Arm",
                "Seed",
                "State",
                "Last update",
                "Highest at end",
                "Highest ever",
                "Files",
            ] {
                ui.label(RichText::new(h).small().color(t.text_faint));
            }
            ui.end_row();
            for (ai, arm) in st.batch.arms.iter().enumerate() {
                for (ri, run) in arm.runs.iter().enumerate() {
                    if !st.takes(run) {
                        continue;
                    }
                    ui.horizontal(|ui| {
                        ui.spacing_mut().item_spacing.x = 5.0;
                        let (rect, _) = ui.allocate_exact_size(vec2(10.0, 14.0), Sense::hover());
                        ui.painter()
                            .circle_filled(rect.center(), 4.0, arm_color(ai, t));
                        ui.label(RichText::new(&arm.name).color(t.text));
                    });
                    let chosen = st.selected == Some((ai, ri));
                    let r = ui.selectable_label(chosen, RichText::new(&run.seed).color(t.text));
                    r.widget_info(|| {
                        WidgetInfo::selected(
                            Role::Button,
                            true,
                            chosen,
                            format!("Run {} seed {}", run.arm, run.seed),
                        )
                    });
                    if r.clicked() {
                        pick = Some((ai, ri));
                    }
                    r.on_hover_text(format!("{}\nClick to show the folder.", run.dir.display()));
                    let state_color = if run.finished { t.success } else { t.warning };
                    ui.label(
                        RichText::new(if run.finished { "finished" } else { "running" })
                            .color(state_color),
                    );
                    ui.label(
                        RichText::new(
                            run.last_update
                                .map_or("-".to_string(), |u| format::count(i64::from(u))),
                        )
                        .color(t.text_weak),
                    );
                    ui.label(RichText::new(rung_name(run.end_rung, run)).color(t.text));
                    ui.label(RichText::new(rung_name(run.ever_rung, run)).color(t.text));
                    if run.problems.is_empty() {
                        ui.label(RichText::new("ok").color(t.text_faint));
                    } else {
                        let first = run.problems[0].clone();
                        let more = run.problems.len() - 1;
                        let shown = if more > 0 {
                            format!("{first} (+{more})")
                        } else {
                            first
                        };
                        let short = crate::ui::start::middle_truncate(&shown, 48);
                        ui.label(RichText::new(short).small().color(t.warning))
                            .on_hover_text(run.problems.join("\n"));
                    }
                    ui.end_row();
                }
            }
        });
    if let Some(p) = pick {
        st.selected = if st.selected == Some(p) {
            None
        } else {
            Some(p)
        };
    }
    ui.add_space(8.0);
    match st
        .selected
        .and_then(|(a, r)| st.batch.arms.get(a).and_then(|arm| arm.runs.get(r)))
    {
        Some(run) => {
            let path = run.dir.display().to_string();
            ui.horizontal_wrapped(|ui| {
                ui.label(RichText::new("Folder").color(t.text_weak));
                ui.add(
                    egui::Label::new(RichText::new(&path).monospace().size(12.0).color(t.text))
                        .selectable(true),
                );
                if ui
                    .button("Copy path")
                    .on_hover_text("Put the folder's path on the clipboard")
                    .clicked()
                {
                    out.copy = Some(path.clone());
                }
            });
            ui.label(RichText::new(run.status()).small().color(t.text_weak));
        }
        None => {
            ui.label(
                RichText::new("Select a run to see its folder.")
                    .small()
                    .color(t.text_faint),
            );
        }
    }
}
