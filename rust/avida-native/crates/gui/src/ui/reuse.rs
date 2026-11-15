//! The Reuse tab of the organism view: the knockout map of the genome.
//!
//! Replacing each instruction in turn by a no-op and running the genome in a
//! test CPU shows which instructions each task depends on. The genome is
//! drawn as a strip with one cell per site, coloured by class; under it one
//! row per task the organism performs marks the sites that task depends on,
//! so a block of sites that several tasks depend on shows as aligned marks.
//! A long genome wraps into blocks. The measures (shared sites, reuse index,
//! sites per task, call sites) sit above. Definitions are in
//! `avida_world::knockout`, and every explanation here is in a hover.

use std::sync::Arc;

use avida_session::{KnockoutMap, KnockoutReply, ReuseMeasures, SiteClass};
use avida_viewmodel::OrganismView;
use egui::{
    pos2, vec2, Align2, Color32, CornerRadius, FontId, Rect, RichText, Role, Sense, Stroke,
    StrokeKind, Ui,
};
use egui_material_icons::icons;

use crate::model::AppState;
use crate::ui::charts::task_color;
use crate::ui::theme::{semibold, Tokens};
use crate::ui::widgets::{self, name_widget};

/// The colour of a site class, from the theme's own tokens.
pub fn class_color(t: &Tokens, class: SiteClass) -> Color32 {
    match class {
        SiteClass::Lethal => t.danger,
        SiteClass::Shared => t.accent,
        SiteClass::Task => t.success,
        SiteClass::Speed => t.warning,
        SiteClass::Neutral => t.control_active,
    }
}

/// What a class means, for hovers.
pub fn class_help(class: SiteClass) -> &'static str {
    match class {
        SiteClass::Lethal => {
            "Lethal: without this instruction the organism no longer replicates. Lethal sites are left out of the task rows."
        }
        SiteClass::Shared => {
            "Shared: without this instruction the organism loses two or more of its tasks."
        }
        SiteClass::Task => "Task: without this instruction the organism loses exactly one task.",
        SiteClass::Speed => {
            "Replication speed: no task is lost, but the fitness changes (usually drops)."
        }
        SiteClass::Neutral => "Neutral: nothing measurable changes without this instruction.",
    }
}

fn class_title(class: SiteClass) -> &'static str {
    match class {
        SiteClass::Lethal => "Lethal",
        SiteClass::Shared => "Shared",
        SiteClass::Task => "One task",
        SiteClass::Speed => "Speed only",
        SiteClass::Neutral => "Neutral",
    }
}

const STRIP_H: f32 = 16.0;
const MARK_H: f32 = 12.0;
const GAP: f32 = 3.0;
const BLOCK_GAP: f32 = 9.0;
/// Width of the left gutter: the block's first site, and the task names.
const GUTTER: f32 = 46.0;

pub fn show(ui: &mut Ui, state: &mut AppState, org: &OrganismView, t: &Tokens) {
    let pending = state.debugger.reuse.pending == Some(org.org_id);
    let reply: Option<Arc<KnockoutReply>> = state
        .debugger
        .reuse
        .reply
        .clone()
        .filter(|r| r.request.org_id == org.org_id);

    ui.horizontal_wrapped(|ui| {
        ui.spacing_mut().item_spacing.x = 8.0;
        let label = if reply.is_some() {
            "Map again"
        } else {
            "Map knockouts"
        };
        ui.add_enabled_ui(!pending, |ui| {
            let r = widgets::icon_text_button(
                ui,
                icons::ICON_ACCOUNT_TREE,
                label,
                "Replace each instruction of the genome in turn by a no-op and run it in a test CPU, to see which instructions each task depends on. One test run per site; the simulation waits while it runs.",
            );
            if r.clicked() {
                state.request_knockouts();
            }
        });
        if pending {
            ui.spinner();
            ui.label(RichText::new("Testing the genome…").color(t.text_weak));
        } else if let Some(r) = &reply {
            if let Ok(m) = &r.result {
                ui.label(
                    RichText::new(format!(
                        "{} test runs, {} ms",
                        m.sites.len() + 1,
                        r.elapsed.as_millis()
                    ))
                    .small()
                    .color(t.text_faint),
                );
            }
        }
    });
    ui.add_space(6.0);

    let Some(reply) = reply else {
        if !pending {
            ui.label(
                RichText::new("No map yet for this organism.")
                    .color(t.text_faint),
            )
            .on_hover_text(
                "Which instructions does each task depend on, and do several tasks share code? Map knockouts answers both for this genome.",
            );
        }
        return;
    };
    let map = match &reply.result {
        Ok(m) => m,
        Err(e) => {
            ui.label(RichText::new(e.as_str()).color(t.warning));
            return;
        }
    };
    if !map.base_viable {
        ui.label(
            RichText::new("The genome does not replicate in the test CPU, so there is no map.")
                .color(t.warning),
        );
        return;
    }
    let m = map.measures();
    measures(ui, map, &m, t);
    ui.add_space(6.0);
    legend(ui, &m, t);
    ui.add_space(6.0);
    strip(ui, map, t);
}

fn task_label(map: &KnockoutMap, k: usize) -> String {
    map.tasks
        .get(k)
        .map_or_else(|| format!("task {k}"), |n| n.to_uppercase())
}

fn measures(ui: &mut Ui, map: &KnockoutMap, m: &ReuseMeasures, t: &Tokens) {
    widgets::card(ui, |ui| {
        ui.set_width(ui.available_width());
        ui.scope(|ui| {
            ui.spacing_mut().interact_size.y = 18.0;
            egui::Grid::new("reuse-measures")
                .num_columns(2)
                .spacing(vec2(16.0, 4.0))
                .show(ui, |ui| {
                    row(
                        ui,
                        t,
                        "Tasks performed",
                        "The tasks this genome performs in the test CPU.",
                        &m.tasks_performed.to_string(),
                    );
                    row(
                        ui,
                        t,
                        "Shared sites",
                        "Sites whose knockout removes two or more tasks (lethal sites excluded). Undefined below two tasks.",
                        &m.shared_sites.map_or_else(|| "–".to_string(), |n| n.to_string()),
                    );
                    let label = ui.label(RichText::new("Reuse index").color(t.text_weak));
                    label.on_hover_text(
                        "Mean pairwise overlap (Jaccard) of the sets of sites the tasks depend on: 0 means separate code for every task, 1 means every task uses the same sites. A knockout that breaks an early step removes all later tasks too, and redundant copies hide sites, so read it as a bound, not a count of subroutines.",
                    );
                    let idx = m
                        .reuse_index
                        .map_or_else(|| "–".to_string(), |v| format!("{v:.2}"));
                    let r = ui.label(RichText::new(idx).color(t.text).family(semibold()));
                    r.on_hover_ui(|ui| {
                        if m.pairs.is_empty() {
                            ui.label("Needs two performed tasks.");
                        }
                        for (a, b, j) in &m.pairs {
                            ui.label(format!(
                                "{} and {}: {}",
                                task_label(map, *a),
                                task_label(map, *b),
                                j.map_or_else(|| "undefined (no sites)".to_string(), |j| format!("{j:.2}"))
                            ));
                        }
                    });
                    ui.end_row();
                    let per_task = m
                        .sites_per_task
                        .iter()
                        .map(|(k, n)| format!("{} {n}", task_label(map, *k)))
                        .collect::<Vec<_>>()
                        .join(" · ");
                    row(
                        ui,
                        t,
                        "Sites per task",
                        "How many non-lethal sites each task depends on.",
                        if per_task.is_empty() { "–" } else { &per_task },
                    );
                    if let Some(c) = m.calls {
                        row(
                            ui,
                            t,
                            "Calls",
                            "Call sites of the genome, the distinct targets they reach (a call with no label jumps to a register and counts as one target), and how many of the call sites are not neutral.",
                            &format!(
                                "{} {}, {} {}, {} not neutral",
                                c.call_sites,
                                if c.call_sites == 1 { "site" } else { "sites" },
                                c.distinct_targets,
                                if c.distinct_targets == 1 { "target" } else { "targets" },
                                c.non_neutral
                            ),
                        );
                    }
                });
        });
    });
}

fn row(ui: &mut Ui, t: &Tokens, key: &str, help: &str, value: &str) {
    ui.label(RichText::new(key).color(t.text_weak))
        .on_hover_text(help);
    ui.add(egui::Label::new(RichText::new(value).color(t.text).family(semibold())).wrap());
    ui.end_row();
}

fn legend(ui: &mut Ui, m: &ReuseMeasures, t: &Tokens) {
    ui.horizontal_wrapped(|ui| {
        ui.spacing_mut().item_spacing = vec2(14.0, 4.0);
        for (i, class) in SiteClass::ALL.iter().enumerate() {
            let text = format!("{} {}", class_title(*class), m.class_counts[i]);
            let galley =
                ui.painter()
                    .layout_no_wrap(text.clone(), FontId::proportional(11.5), t.text_weak);
            let size = vec2(15.0 + galley.size().x, 14.0);
            if ui.available_width() < size.x {
                ui.end_row();
            }
            let (rect, resp) = ui.allocate_exact_size(size, Sense::hover());
            let p = ui.painter();
            p.rect_filled(
                Rect::from_min_size(rect.left_top() + vec2(0.0, 1.5), vec2(11.0, 11.0)),
                CornerRadius::same(3),
                class_color(t, *class),
            );
            p.galley(
                pos2(rect.left() + 15.0, rect.center().y - galley.size().y / 2.0),
                galley,
                t.text_weak,
            );
            name_widget(&resp, Role::Label, &text);
            resp.on_hover_text(class_help(*class));
        }
    });
}

/// How many sites a block holds for a drawing `width` wide.
fn sites_per_block(width: f32, len: usize) -> usize {
    let fit = ((width / 6.0).floor() as usize / 10 * 10).max(10);
    fit.min(len.max(1))
}

/// Where site `site` of a `len`-site genome is drawn (the centre of its
/// strip cell), given the rectangle of the first block, which is as wide as
/// the drawing.
pub fn site_position(first_block: Rect, len: usize, site: usize) -> Option<egui::Pos2> {
    if site >= len {
        return None;
    }
    let draw_w = (first_block.width() - GUTTER).max(60.0);
    let per_block = sites_per_block(draw_w, len);
    let cell_w = (draw_w / per_block as f32).min(14.0);
    let (block, col) = (site / per_block, site % per_block);
    Some(pos2(
        first_block.left() + GUTTER + (col as f32 + 0.5) * cell_w,
        first_block.top() + block as f32 * (first_block.height() + BLOCK_GAP) + STRIP_H / 2.0,
    ))
}

fn strip(ui: &mut Ui, map: &KnockoutMap, t: &Tokens) {
    let len = map.sites.len();
    if len == 0 {
        return;
    }
    // Blocks follow each other `BLOCK_GAP` apart (`site_position` relies on it).
    ui.spacing_mut().item_spacing.y = BLOCK_GAP;
    let tasks: Vec<usize> = map.performed.clone();
    let total_w = ui.available_width();
    let draw_w = (total_w - GUTTER).max(60.0);
    let per_block = sites_per_block(draw_w, len);
    let cell_w = (draw_w / per_block as f32).min(14.0);
    let block_h = STRIP_H + GAP + tasks.len() as f32 * MARK_H;
    let font = FontId::monospace(10.0);
    let task_font = FontId::proportional(10.0);
    let n_tasks_all = map.tasks.len().max(1);
    let sets: Vec<Vec<usize>> = tasks.iter().map(|&k| map.task_sites(k)).collect();
    let mut start = 0;
    while start < len {
        let end = (start + per_block).min(len);
        let (rect, resp) = ui.allocate_exact_size(vec2(total_w, block_h), Sense::hover());
        if start == 0 {
            // The whole drawing, named for screen readers and tests.
            name_widget(
                &resp,
                Role::Image,
                &format!("Knockout map of {len} sites, {} tasks", map.performed.len()),
            );
        }
        let x0 = rect.left() + GUTTER;
        let painter = ui.painter_at(rect);
        painter.text(
            pos2(x0 - 6.0, rect.top() + STRIP_H / 2.0),
            Align2::RIGHT_CENTER,
            start.to_string(),
            font.clone(),
            t.text_faint,
        );
        let cell_rect = |i: usize, y: f32, h: f32| {
            Rect::from_min_size(
                pos2(x0 + (i - start) as f32 * cell_w, y),
                vec2((cell_w - 1.0).max(1.0), h),
            )
        };
        for i in start..end {
            painter.rect_filled(
                cell_rect(i, rect.top(), STRIP_H),
                CornerRadius::same(1),
                class_color(t, map.sites[i].class),
            );
        }
        for (row, &k) in tasks.iter().enumerate() {
            let y = rect.top() + STRIP_H + GAP + row as f32 * MARK_H;
            // The task's own colour (as in the charts), lifted a little
            // towards the text colour so it stands off the panel.
            let color = task_color(k, n_tasks_all, t.dark).lerp_to_gamma(t.text, 0.3);
            painter.rect_filled(
                Rect::from_min_size(
                    pos2(x0, y + (MARK_H - 3.0) / 2.0),
                    vec2((end - start) as f32 * cell_w - 1.0, 1.0),
                ),
                CornerRadius::ZERO,
                t.border,
            );
            for &i in sets[row].iter().filter(|&&i| i >= start && i < end) {
                painter.rect_filled(cell_rect(i, y, MARK_H - 3.0), CornerRadius::same(1), color);
            }
            painter.text(
                pos2(x0 - 6.0, y + (MARK_H - 3.0) / 2.0),
                Align2::RIGHT_CENTER,
                task_label(map, k),
                task_font.clone(),
                t.text_weak,
            );
        }
        if let Some(p) = resp.hover_pos() {
            if p.x >= x0 && p.x < x0 + (end - start) as f32 * cell_w {
                let i = start + ((p.x - x0) / cell_w) as usize;
                if i < end {
                    painter.rect_stroke(
                        cell_rect(i, rect.top() - 1.0, block_h + 1.0).expand2(vec2(0.5, 0.0)),
                        CornerRadius::same(1),
                        Stroke::new(1.0, t.text),
                        StrokeKind::Outside,
                    );
                    site_tooltip(&resp, map, i, t);
                }
            }
        }
        start = end;
    }
}

fn site_tooltip(resp: &egui::Response, map: &KnockoutMap, i: usize, t: &Tokens) {
    let s = &map.sites[i];
    let name = &map.inst_names[i];
    let t = *t;
    resp.clone().on_hover_ui_at_pointer(|ui| {
        ui.set_max_width(280.0);
        ui.label(
            RichText::new(format!("Site {i}: {name}"))
                .family(semibold())
                .color(t.text),
        );
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 4.0;
            widgets::swatch(ui, class_color(&t, s.class), 10.0);
            ui.label(class_title(s.class));
        });
        if s.class != SiteClass::Lethal {
            let lost = if s.lost.is_empty() {
                "no task".to_string()
            } else {
                s.lost
                    .iter()
                    .map(|&k| task_label(map, k))
                    .collect::<Vec<_>>()
                    .join(", ")
            };
            ui.label(format!("Loses: {lost}"));
            ui.label(format!(
                "Fitness without it: {:.0}% of the base",
                s.rel_fitness * 100.0
            ));
        } else {
            ui.label("Does not replicate without it.");
        }
    });
}
