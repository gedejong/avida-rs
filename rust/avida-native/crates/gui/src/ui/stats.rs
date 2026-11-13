//! The stats tab: key numbers of the last finished update with
//! sparklines, task coverage, run facts.

use egui::{vec2, CornerRadius, RichText, Sense, Ui};
use egui_material_icons::icons;

use crate::model::metrics::{tail_series, Metric, SeriesKey};
use crate::model::{format, AppState};
use crate::ui::theme::{metric_style, semibold, Tokens};
use crate::ui::widgets::{self, card_frame};

/// Rows a sparkline covers.
const SPARK_ROWS: usize = 400;

const CARDS: [Metric; 8] = [
    Metric::Organisms,
    Metric::Genotypes,
    Metric::AveFitness,
    Metric::MaxFitness,
    Metric::AveMerit,
    Metric::AveGestation,
    Metric::AveGeneration,
    Metric::Births,
];

pub fn show(ui: &mut Ui, state: &AppState) {
    let t = Tokens::of(ui);
    let Some(latest) = state.latest_stats().cloned() else {
        widgets::empty_state(
            ui,
            icons::ICON_MONITORING,
            "No statistics yet",
            "Open an experiment to see its numbers.",
        );
        return;
    };
    // Which update the numbers belong to.
    ui.horizontal_wrapped(|ui| {
        let stamp = if latest.update < 0 {
            "Before the first update".to_string()
        } else {
            format!("Statistics of {}", format::update_phrase(latest.update))
        };
        ui.label(RichText::new(stamp).family(semibold()).color(t.text));
        // Only when paused: while running the line would flicker in and
        // out with every frame.
        let paused = state.run_state() == Some(avida_session::RunState::Paused);
        if let Some(fp) = state.frame_progress().filter(|_| paused) {
            ui.label(
                RichText::new(format!(
                    "· update {} in progress ({}/{} slices)",
                    format::count(i64::from(fp.update)),
                    format::count(i64::from(fp.slices_done)),
                    format::count(i64::from(fp.slices_total))
                ))
                .color(t.text_weak),
            );
        }
    });
    ui.add_space(6.0);

    let rows = state.history.rows();
    let gap = 8.0;
    let card_w = ((ui.available_width() - gap) / 2.0).floor();
    for pair in CARDS.chunks(2) {
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = gap;
            for &m in pair {
                stat_card(ui, &t, m, latest.update, m.value(&latest), rows, card_w);
            }
        });
        ui.add_space(gap - ui.spacing().item_spacing.y);
    }

    ui.add_space(6.0);
    tasks(ui, state, &latest, &t);
    ui.add_space(10.0);
    run_facts(ui, state, &latest, &t);
}

fn stat_card(
    ui: &mut Ui,
    t: &Tokens,
    m: Metric,
    latest_update: i32,
    value: f64,
    rows: &[avida_session::UpdateRow],
    width: f32,
) {
    // Margins (2 × 12) and stroke (2 × 1).
    let inner_w = (width - 26.0).max(40.0);
    card_frame(ui)
        .show(ui, |ui| {
            // The card sits in a horizontal row; its content stacks.
            ui.vertical(|ui| {
                ui.set_width(inner_w);
                ui.spacing_mut().item_spacing.y = 2.0;
                widgets::caption(ui, m.short());
                // Before the first update the averages are not zero, they do
                // not exist yet.
                let shown = if latest_update < 0 && !m.is_count() {
                    "–".to_string()
                } else {
                    format::metric(value, m.is_count())
                };
                ui.label(
                    RichText::new(shown)
                        .text_style(metric_style())
                        .color(t.text),
                );
                let pts = tail_series(
                    rows,
                    SeriesKey::Metric(m),
                    SPARK_ROWS,
                    (inner_w * 2.0) as usize,
                );
                widgets::sparkline(ui, &pts, vec2(inner_w, 30.0), t.accent);
            });
        })
        .response
        .on_hover_text(m.description());
}

fn tasks(ui: &mut Ui, state: &AppState, latest: &avida_session::UpdateRow, t: &Tokens) {
    let Some(frame) = state.frame.as_deref() else {
        return;
    };
    if frame.env.tasks.is_empty() {
        return;
    }
    widgets::caption(ui, "Tasks performed (last gestation)");
    ui.add_space(2.0);
    let orgs = latest.orgs.max(0) as f32;
    let font = egui::FontId::proportional(13.0);
    let small = egui::FontId::proportional(12.0);
    let names: Vec<&str> = frame
        .env
        .tasks
        .iter()
        .map(|task| {
            if task.desc.is_empty() {
                task.name.as_str()
            } else {
                task.desc.as_str()
            }
        })
        .collect();
    let name_w = names
        .iter()
        .map(|n| {
            ui.painter()
                .layout_no_wrap((*n).to_string(), font.clone(), t.text)
                .size()
                .x
        })
        .fold(0.0_f32, f32::max);
    let count_w = 92.0;
    for (k, name) in names.iter().enumerate() {
        let count = latest.task_last_count.get(k).copied().unwrap_or(0);
        let frac = if orgs > 0.0 {
            (count as f32 / orgs).clamp(0.0, 1.0)
        } else {
            0.0
        };
        let (row, resp) = ui.allocate_exact_size(vec2(ui.available_width(), 22.0), Sense::hover());
        let p = ui.painter();
        let cy = row.center().y;
        p.text(
            egui::pos2(row.left(), cy),
            egui::Align2::LEFT_CENTER,
            *name,
            font.clone(),
            t.text,
        );
        let bar = egui::Rect::from_min_max(
            egui::pos2(row.left() + name_w + 12.0, cy - 3.5),
            egui::pos2(
                (row.right() - count_w).max(row.left() + name_w + 40.0),
                cy + 3.5,
            ),
        );
        p.rect_filled(bar, CornerRadius::same(4), t.control);
        if frac > 0.0 {
            let mut fill = bar;
            fill.set_width((bar.width() * frac).max(3.0));
            p.rect_filled(fill, CornerRadius::same(4), t.accent);
        }
        let text = if count > 0 {
            format!("{}  {:.0}%", format::count(i64::from(count)), frac * 100.0)
        } else {
            "0".to_string()
        };
        let color = if count > 0 { t.text } else { t.text_faint };
        p.text(
            egui::pos2(row.right(), cy),
            egui::Align2::RIGHT_CENTER,
            text,
            small.clone(),
            color,
        );
        widgets::name_widget(
            &resp,
            egui::Role::Row,
            &format!("{name}: {count} organisms"),
        );
        resp.on_hover_text(format!(
            "{} of {} organisms ({:.1}%) performed {name} in their last gestation",
            format::count(i64::from(count)),
            format::count(latest.orgs.into()),
            frac * 100.0
        ));
    }
}

fn run_facts(ui: &mut Ui, state: &AppState, latest: &avida_session::UpdateRow, t: &Tokens) {
    let Some(frame) = state.frame.as_deref() else {
        return;
    };
    widgets::caption(ui, "Run");
    let h = &frame.header;
    let rate = state
        .rate
        .rate(state.now)
        .map_or("–".to_string(), format::rate);
    let mut rows = vec![
        ("World", format!("{} × {} cells", h.world_x, h.world_y)),
        ("Random seed", h.random_seed.to_string()),
        ("Instruction set", h.inst_set.clone()),
        (
            "Organisms ever born",
            format::count(i64::from(latest.tot_organisms)),
        ),
        (
            "Executed last update",
            format::count(i64::from(latest.executed)),
        ),
        ("Simulation speed", rate),
    ];
    if let Some(l) = state.update_limit() {
        rows.push(("Update limit", format::update_phrase(l)));
    }
    if state.history.stride() > 1 {
        rows.push((
            "History",
            format!("every {}th update kept", state.history.stride()),
        ));
    }
    let _ = t;
    widgets::kv_grid(ui, "run-facts", &rows);
}
