//! The genotypes tab: the dominant genotype with its genome, a stacked
//! chart of how many cells the most abundant genotypes held over time,
//! the lineage of the highlighted genotype, and a sortable table of the
//! most abundant genotypes. Clicking a row highlights that genotype's
//! cells on the map and traces its lineage.

use avida_session::Frame;
use avida_viewmodel::{GenotypeRow, Palette, OTHER_GREY};
use egui::{
    pos2, vec2, Align2, Color32, CornerRadius, FontId, Mesh, RichText, Role, Sense, Stroke, Ui,
};
use egui_extras::{Column, TableBuilder};
use egui_material_icons::icons;

use crate::model::genotypes::{
    genome_listing, lineage_fitness, lineage_summary, sorted_indices, GenoColumn,
};
use crate::model::vision::ColorVision;
use crate::model::{format, AppState};
use crate::ui::theme::{semibold, Tokens};
use crate::ui::widgets::{self, name_widget, rgba};

pub fn show(ui: &mut Ui, state: &mut AppState) {
    let t = Tokens::of(ui);
    let Some(frame) = state.frame.clone() else {
        widgets::empty_state(
            ui,
            icons::ICON_GENETICS,
            "No genotypes yet",
            "Open an experiment to list its genotypes.",
        );
        return;
    };
    let vision = state.map.vision;
    if let Some(dom) = &frame.dominant {
        let slot = frame.map.as_ref().and_then(|m| m.genotype_slot(dom.row.id));
        widgets::card(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal(|ui| {
                widgets::swatch(ui, slot_color(vision, slot), 14.0);
                ui.label(
                    RichText::new(crate::model::layers::shown_name(&dom.row.name, dom.row.id))
                        .family(semibold())
                        .size(15.0)
                        .color(t.text),
                );
                widgets::badge(ui, "Dominant", t.accent);
            });
            ui.add_space(4.0);
            let rows = [
                ("Organisms", format::count(i64::from(dom.row.abundance))),
                ("Fitness", format::real(dom.row.ave_fitness)),
                ("Merit", format::real(dom.row.ave_merit)),
                ("Gestation", format::real(dom.row.ave_gestation)),
                ("Genome length", dom.row.genome_len.to_string()),
                (
                    "Copied / executed",
                    format!(
                        "{} / {}",
                        format::real(dom.ave_copy_size),
                        format::real(dom.ave_exe_size)
                    ),
                ),
                ("Reproduction rate", format::real(dom.ave_repro_rate)),
                ("Depth", dom.row.depth.to_string()),
                ("Born", format::update_phrase(dom.row.update_born)),
                (
                    "Parents",
                    if dom.parents.is_empty() {
                        "–".to_string()
                    } else {
                        dom.parents.clone()
                    },
                ),
            ];
            widgets::kv_grid(ui, "dominant-kv", &rows);
            ui.add_space(6.0);
            ui.horizontal(|ui| {
                let on = state.highlight == Some(dom.row.id);
                let label = if on {
                    "Remove highlight"
                } else {
                    "Highlight on map"
                };
                if widgets::icon_text_button(
                    ui,
                    icons::ICON_FILTER_CENTER_FOCUS,
                    label,
                    "Fade every other genotype on the map",
                )
                .clicked()
                {
                    state.toggle_highlight(dom.row.id);
                }
                let first = frame.map.as_ref().and_then(|m| m.first_cell_of(dom.row.id));
                if ui
                    .add_enabled_ui(first.is_some(), |ui| {
                        widgets::icon_text_button(
                            ui,
                            icons::ICON_BIOTECH,
                            "Inspect one",
                            "Select an organism of this genotype and open the inspector",
                        )
                    })
                    .inner
                    .clicked()
                {
                    state.select_cell(first);
                }
            });
            ui.add_space(4.0);
            egui::CollapsingHeader::new(
                RichText::new(format!("Genome · {} instructions", dom.row.genome_len))
                    .color(t.text),
            )
            .id_salt("dominant-genome")
            .default_open(false)
            .show(ui, |ui| genome(ui, &dom.genome, state.inst_names(), &t));
        });
        ui.add_space(10.0);
    }

    abundance_chart(ui, state, &frame, &t);
    ui.add_space(10.0);
    lineage(ui, state, &frame, &t);
    ui.add_space(10.0);

    ui.horizontal(|ui| {
        widgets::caption(
            ui,
            &format!("Most abundant genotypes · {}", frame.genotypes.len()),
        );
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if widgets::icon_button(
                ui,
                icons::ICON_DOWNLOAD,
                "Export genotype table",
                "Save this table as a CSV file",
            )
            .clicked()
            {
                state.request(crate::model::Request::ExportGenotypes);
            }
        });
    });
    table(ui, state, &frame, &t);
}

fn genome(ui: &mut Ui, genome: &str, names: Option<&[String]>, t: &Tokens) {
    let sites = genome_listing(genome, names);
    let cols = if names.is_some() { 3 } else { 10 };
    egui::Grid::new("genome-grid")
        .num_columns(cols * 2)
        .spacing(vec2(6.0, 2.0))
        .show(ui, |ui| {
            for (i, site) in sites.iter().enumerate() {
                ui.label(
                    RichText::new(format!("{i:>3}"))
                        .monospace()
                        .color(t.text_faint),
                );
                let text = match &site.name {
                    Some(n) => n.clone(),
                    None => site.symbol.clone(),
                };
                ui.label(
                    RichText::new(text)
                        .monospace()
                        .color(inst_color(t, site.name.as_deref())),
                );
                if (i + 1) % cols == 0 {
                    ui.end_row();
                }
            }
        });
}

/// Colours instruction families in listings: nops faint, copying and
/// division accented, everything else plain.
pub fn inst_color(t: &Tokens, name: Option<&str>) -> Color32 {
    match name {
        Some(n) if n.starts_with("nop") => t.text_faint,
        Some(n)
            if n.starts_with("h-")
                || n.contains("divide")
                || n.contains("alloc")
                || n.contains("copy") =>
        {
            t.accent
        }
        Some(n) if n.starts_with("IO") || n == "io" => t.success,
        _ => t.text,
    }
}

/// The colour of a genotype slot (other: grey), as the colour-vision
/// preview shows it.
pub fn slot_color(vision: ColorVision, slot: Option<usize>) -> Color32 {
    rgba(vision.simulate_rgba(Palette::OkabeIto.category(slot)))
}

fn other_color(vision: ColorVision) -> Color32 {
    let [r, g, b] = vision.simulate(OTHER_GREY);
    Color32::from_rgb(r, g, b)
}

/// Cells held by the genotypes of the map's colour slots, stacked, with
/// all others on top: the top edge is the population.
fn abundance_chart(ui: &mut Ui, state: &AppState, frame: &Frame, t: &Tokens) {
    let slots: Vec<i32> = frame
        .map
        .as_ref()
        .map(|m| m.top_genotypes.to_vec())
        .unwrap_or_default();
    let history = &state.abundance;
    widgets::caption(ui, "Abundance of the top genotypes");
    if history.len() < 2 {
        ui.label(
            RichText::new("The chart fills in as the run advances.")
                .small()
                .color(t.text_weak),
        );
        return;
    }
    let bands = history.bands(&slots);
    let xs = history.updates();
    let ymax = f64::from(history.max_total().max(1));
    let (x0, x1) = (xs[0], *xs.last().unwrap_or(&xs[0]));
    let dx = (x1 - x0).max(1.0);
    let (rect, response) =
        ui.allocate_exact_size(vec2(ui.available_width(), 118.0), Sense::hover());
    name_widget(&response, Role::Image, "Abundance chart");
    let plot = rect.shrink2(vec2(0.0, 0.0));
    let plot = egui::Rect::from_min_max(plot.min + vec2(0.0, 14.0), plot.max - vec2(0.0, 16.0));
    let painter = ui.painter_at(rect);
    painter.rect_filled(plot, CornerRadius::same(4), t.control);
    let to_screen = |x: f64, y: f64| {
        pos2(
            plot.left() + ((x - x0) / dx) as f32 * plot.width(),
            plot.bottom() - (y / ymax) as f32 * plot.height(),
        )
    };
    let vision = state.map.vision;
    let mut below = vec![0.0; xs.len()];
    for band in &bands {
        let color = match band.slot {
            Some(slot) => slot_color(vision, Some(slot)),
            None => other_color(vision),
        };
        let mut mesh = Mesh::default();
        for (i, (&x, &top)) in xs.iter().zip(&band.top).enumerate() {
            mesh.colored_vertex(to_screen(x, top), color);
            mesh.colored_vertex(to_screen(x, below[i]), color);
            if i > 0 {
                let k = (2 * i) as u32;
                mesh.add_triangle(k - 2, k - 1, k);
                mesh.add_triangle(k - 1, k + 1, k);
            }
        }
        painter.add(egui::Shape::mesh(mesh));
        below.clone_from(&band.top);
    }
    let font = FontId::proportional(10.5);
    painter.text(
        rect.left_top(),
        Align2::LEFT_TOP,
        format!("{} organisms", format::compact(ymax)),
        font.clone(),
        t.text_faint,
    );
    painter.text(
        rect.left_bottom(),
        Align2::LEFT_BOTTOM,
        format::update_phrase((x0 as i32).max(0)),
        font.clone(),
        t.text_faint,
    );
    painter.text(
        rect.right_bottom(),
        Align2::RIGHT_BOTTOM,
        format::update_phrase(x1 as i32),
        font,
        t.text_faint,
    );
    // Hover: the sample under the pointer.
    if let Some(p) = response.hover_pos().filter(|p| plot.contains(*p)) {
        let x = x0 + f64::from((p.x - plot.left()) / plot.width()) * dx;
        let i = xs
            .iter()
            .enumerate()
            .min_by(|a, b| (a.1 - x).abs().total_cmp(&(b.1 - x).abs()))
            .map_or(0, |(i, _)| i);
        let sx = to_screen(xs[i], 0.0).x;
        painter.vline(sx, plot.y_range(), Stroke::new(1.0, t.text));
        let sample = &history.samples()[i];
        response.on_hover_ui_at_pointer(|ui| {
            ui.label(
                RichText::new(format!(
                    "{} · {} organisms",
                    format::update_phrase(sample.update),
                    format::count(i64::from(sample.total))
                ))
                .family(semibold()),
            );
            let mut known = 0;
            for (slot, &g) in slots.iter().enumerate() {
                let n = sample.count_of(g);
                known += n;
                ui.horizontal(|ui| {
                    widgets::swatch(ui, slot_color(vision, Some(slot)), 10.0);
                    ui.label(format!(
                        "{}: {}",
                        crate::model::layers::genotype_name(frame, g),
                        format::count(i64::from(n))
                    ));
                });
            }
            ui.horizontal(|ui| {
                widgets::swatch(ui, other_color(vision), 10.0);
                ui.label(format!(
                    "Other genotypes: {}",
                    format::count(i64::from(sample.total.saturating_sub(known)))
                ));
            });
        });
    }
}

/// The ancestry of the highlighted genotype, newest first.
fn lineage(ui: &mut Ui, state: &mut AppState, frame: &Frame, t: &Tokens) {
    let Some(traced) = state.highlight else {
        ui.horizontal_wrapped(|ui| {
            widgets::caption(ui, "Lineage");
            ui.label(
                RichText::new("Highlight a genotype to trace its ancestry back to the founder.")
                    .small()
                    .color(t.text_weak),
            );
            if let Some(dom) = state.dominant_id() {
                if ui
                    .small_button("Trace the dominant genotype")
                    .on_hover_text(
                        "Highlight the dominant genotype on the map and list its ancestors",
                    )
                    .clicked()
                {
                    state.set_highlight(Some(dom));
                }
            }
        });
        return;
    };
    let chain = &frame.lineage;
    let name = crate::model::layers::genotype_name(frame, traced);
    ui.horizontal(|ui| {
        widgets::caption(ui, &format!("Lineage of {name}"));
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if ui
                .small_button("Show all genotypes")
                .on_hover_text("Show every genotype on the map again (Esc)")
                .clicked()
            {
                state.clear_highlight();
            }
        });
    });
    if chain.first().map(|g| g.id) != Some(traced) {
        ui.label(
            RichText::new("Tracing the ancestors…")
                .small()
                .color(t.text_weak),
        );
        return;
    }
    if let Some(sum) = lineage_summary(chain) {
        let mut text = if sum.genotypes <= 1 && !sum.truncated {
            "This genotype is the founder: it has no ancestors.".to_string()
        } else {
            format!(
                "{} back to the founder, over {}.",
                format::plural(sum.genotypes as u64 - 1, "ancestor"),
                format::plural(sum.span.max(0) as u64, "update"),
            )
        };
        if sum.extinct > 0 {
            let verb = if sum.extinct == 1 { "is" } else { "are" };
            text.push_str(&format!(
                " {} of them {verb} extinct.",
                format::count(sum.extinct as i64)
            ));
        }
        if let Some(g) = sum.fitness_gain.filter(|g| (g - 1.0).abs() > 1e-9) {
            text.push_str(&format!(
                " Average fitness changed by a factor of {} along the way.",
                format::real(g)
            ));
        }
        if sum.truncated {
            text.push_str(" Older ancestors were not recorded.");
        }
        ui.label(RichText::new(text).small().color(t.text_weak));
        let pts = lineage_fitness(chain);
        if pts.len() >= 2 {
            widgets::sparkline(ui, &pts, vec2(ui.available_width(), 30.0), t.accent)
                .on_hover_text("Average fitness along the lineage, founder on the left");
        }
    }
    let id = ui.scope_id().with("lineage-all");
    let mut all: bool = ui.data(|d| d.get_temp(id)).unwrap_or(false);
    let limit = if all { chain.len() } else { chain.len().min(8) };
    let slot_of = |id: i32| frame.map.as_ref().and_then(|m| m.genotype_slot(id));
    let vision = state.map.vision;
    let mut inspect = None;
    for (k, g) in chain.iter().take(limit).enumerate() {
        let last = k + 1 == chain.len();
        if let Some(cell) = lineage_row(
            ui,
            g,
            k == 0,
            last,
            slot_color(vision, slot_of(g.id)),
            frame,
            t,
        ) {
            inspect = Some(cell);
        }
    }
    if chain.len() > 8 {
        let label = if all {
            "Show fewer".to_string()
        } else {
            format!("Show all {}", format::count(chain.len() as i64))
        };
        if ui.small_button(label).clicked() {
            all = !all;
            ui.data_mut(|d| d.insert_temp(id, all));
        }
    }
    if let Some(cell) = inspect {
        state.select_cell(Some(cell));
    }
}

/// One genotype of the lineage; returns a cell to inspect when clicked.
fn lineage_row(
    ui: &mut Ui,
    g: &GenotypeRow,
    first: bool,
    last: bool,
    color: Color32,
    frame: &Frame,
    t: &Tokens,
) -> Option<i32> {
    let h = 26.0;
    let (rect, r) = ui.allocate_exact_size(vec2(ui.available_width(), h), Sense::click());
    let alive = g.is_alive();
    let cell = frame.map.as_ref().and_then(|m| m.first_cell_of(g.id));
    let p = ui.painter_at(rect);
    if r.hovered() && cell.is_some() {
        p.rect_filled(rect, CornerRadius::same(6), t.control);
    }
    if r.has_focus() {
        p.rect_stroke(
            rect,
            CornerRadius::same(6),
            Stroke::new(1.5, t.accent),
            egui::StrokeKind::Inside,
        );
    }
    let cx = rect.left() + 10.0;
    let cy = rect.center().y;
    // The chain: a line through the dots.
    let line = Stroke::new(1.5, t.border_strong);
    if !first {
        p.vline(cx, rect.top()..=cy, line);
    }
    if !last {
        p.vline(cx, cy..=rect.bottom(), line);
    }
    if alive {
        p.circle_filled(pos2(cx, cy), 5.0, color);
    } else {
        p.circle_filled(pos2(cx, cy), 5.0, t.surface);
        p.circle_stroke(pos2(cx, cy), 4.5, Stroke::new(1.5, t.text_faint));
    }
    let name_color = if first {
        t.accent
    } else if alive {
        t.text
    } else {
        t.text_weak
    };
    // Genotypes below the threshold share a name (`100-no_name`); the id
    // tells them apart.
    let name_rect = p.text(
        pos2(rect.left() + 24.0, cy),
        Align2::LEFT_CENTER,
        &g.name,
        FontId::monospace(12.0),
        name_color,
    );
    p.text(
        pos2(name_rect.right() + 6.0, cy),
        Align2::LEFT_CENTER,
        format!("#{}", g.id),
        FontId::proportional(11.0),
        t.text_faint,
    );
    let right = if alive {
        format!(
            "{} alive · fitness {}",
            format::count(i64::from(g.abundance)),
            format::real(g.ave_fitness),
        )
    } else {
        "extinct".to_string()
    };
    p.text(
        pos2(rect.right() - 6.0, cy),
        Align2::RIGHT_CENTER,
        right,
        FontId::proportional(11.5),
        t.text_weak,
    );
    name_widget(&r, Role::Row, &g.name);
    let r = r.on_hover_text(format!(
        "{}{}: {}, fitness {}, depth {}, {}{}",
        g.name,
        if g.parent_id >= 0 {
            format!(" (parent #{})", g.parent_id)
        } else {
            " (injected)".to_string()
        },
        if alive {
            format!("{} organisms", format::count(i64::from(g.abundance)))
        } else {
            "extinct".to_string()
        },
        format::real(g.ave_fitness),
        g.depth,
        format::born(g.update_born),
        if cell.is_some() {
            ". Click to inspect one of its organisms."
        } else {
            ""
        }
    ));
    r.clicked().then_some(cell).flatten()
}

fn table(ui: &mut Ui, state: &mut AppState, frame: &avida_session::Frame, t: &Tokens) {
    let rows = &frame.genotypes;
    if rows.is_empty() {
        widgets::empty_state(
            ui,
            icons::ICON_GENETICS,
            "No living genotypes",
            "The population has died out.",
        );
        return;
    }
    let order = sorted_indices(rows, state.geno_sort);
    let slot_of = |id: i32| frame.map.as_ref().and_then(|m| m.genotype_slot(id));
    let vision = state.map.vision;
    let row_h = 24.0;
    let mut clicked = None;
    let mut sort_click = None;
    // Number columns in order of importance; as many as fit.
    const NUMBERS: [GenoColumn; 6] = [
        GenoColumn::Abundance,
        GenoColumn::Fitness,
        GenoColumn::Merit,
        GenoColumn::Gestation,
        GenoColumn::Length,
        GenoColumn::Depth,
    ];
    const NUMBER_W: f32 = 78.0;
    const NAME_MIN: f32 = 104.0;
    let gap = ui.spacing().item_spacing.x;
    let room = ui.available_width() - 18.0 - NAME_MIN - 2.0 * gap;
    let fit = ((room / (NUMBER_W + gap)).floor().max(1.0) as usize).min(NUMBERS.len());
    let mut columns = vec![GenoColumn::Name];
    columns.extend_from_slice(&NUMBERS[..fit]);
    let table = TableBuilder::new(ui)
        .id_salt("genotype-table")
        .striped(false)
        .vscroll(false)
        .sense(Sense::click())
        .cell_layout(egui::Layout::left_to_right(egui::Align::Center))
        .column(Column::exact(18.0))
        .column(Column::remainder().at_least(NAME_MIN).clip(true))
        .columns(Column::exact(NUMBER_W), fit);
    table
        .header(26.0, |mut header| {
            header.col(|_| {});
            for &c in &columns {
                header.col(|ui| {
                    if c != GenoColumn::Name {
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            sort_button(ui, state, c, t, &mut sort_click);
                        });
                        return;
                    }
                    sort_button(ui, state, c, t, &mut sort_click);
                });
            }
        })
        .body(|body| {
            body.rows(row_h, order.len(), |mut row| {
                let g = &rows[order[row.index()]];
                let highlighted = state.highlight == Some(g.id);
                row.set_selected(highlighted);
                row.col(|ui| {
                    let (rect, _) = ui.allocate_exact_size(vec2(12.0, 12.0), Sense::hover());
                    ui.painter().rect_filled(rect, CornerRadius::same(3), slot_color(vision, slot_of(g.id)));
                    if g.threshold {
                        ui.painter().rect_stroke(rect.expand(1.5), CornerRadius::same(4), Stroke::new(1.0, t.border_strong), egui::StrokeKind::Outside);
                    }
                });
                row.col(|ui| {
                    let text = RichText::new(crate::model::layers::shown_name(&g.name, g.id)).monospace().size(12.0);
                    ui.add(egui::Label::new(if highlighted { text.color(t.accent) } else { text.color(t.text) }).truncate());
                });
                for &c in &columns[1..] {
                    let v = match c {
                        GenoColumn::Abundance => format::count(i64::from(g.abundance)),
                        GenoColumn::Fitness => format::real(g.ave_fitness),
                        GenoColumn::Merit => format::real(g.ave_merit),
                        GenoColumn::Gestation => format::real(g.ave_gestation),
                        GenoColumn::Length => g.genome_len.to_string(),
                        GenoColumn::Depth => g.depth.to_string(),
                        GenoColumn::Name | GenoColumn::Born => String::new(),
                    };
                    row.col(|ui| {
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            ui.label(RichText::new(v).size(12.5).color(t.text_weak));
                        });
                    });
                }
                let response = row.response();
                if response.clicked() {
                    clicked = Some(g.id);
                }
                response.on_hover_text(format!(
                    "{}: {} organisms, fitness {}, merit {}, gestation {}, length {}, depth {}, {}{}. Click to highlight its cells on the map and trace its lineage.",
                    crate::model::layers::shown_name(&g.name, g.id),
                    g.abundance,
                    format::real(g.ave_fitness),
                    format::real(g.ave_merit),
                    format::real(g.ave_gestation),
                    g.genome_len,
                    g.depth,
                    format::born(g.update_born),
                    if g.threshold { ", established" } else { "" }
                ));
            });
        });
    if let Some(c) = sort_click {
        state.geno_sort.click(c);
    }
    if let Some(id) = clicked {
        state.toggle_highlight(id);
    }
}

fn sort_button(
    ui: &mut Ui,
    state: &AppState,
    c: GenoColumn,
    t: &Tokens,
    sort_click: &mut Option<GenoColumn>,
) {
    let sorted = state.geno_sort.column == c;
    let arrow = if state.geno_sort.descending {
        icons::ICON_ARROW_DOWNWARD
    } else {
        icons::ICON_ARROW_UPWARD
    };
    let text = RichText::new(c.title())
        .size(12.0)
        .color(if sorted { t.text } else { t.text_weak });
    let b = if sorted {
        egui::Button::new((
            text.family(semibold()),
            arrow.rich_text().size(13.0).color(t.accent),
        ))
    } else {
        egui::Button::new(text)
    }
    .frame_when_inactive(false)
    .small();
    let r = ui.add(b);
    name_widget(
        &r,
        Role::Button,
        &format!("Sort by {}", c.title().to_lowercase()),
    );
    if r.on_hover_text(c.tooltip()).clicked() {
        *sort_click = Some(c);
    }
}
