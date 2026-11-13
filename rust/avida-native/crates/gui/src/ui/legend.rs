//! The map legend, a strip under the canvas: a gradient with ticks for
//! continuous layers, clickable swatches for the top genotypes.

use avida_session::LayerId;
use avida_viewmodel::{Palette, Scale, CATEGORICAL_SLOTS, OTHER_GREY};
use egui::{
    pos2, vec2, Align2, Color32, CornerRadius, FontId, Mesh, Rect, RichText, Role, Sense, Stroke,
    Ui,
};
use egui_material_icons::icons;

use crate::model::layers::{genotype_name, layer_title, MapImage};
use crate::model::vision::ColorVision;
use crate::model::{format, AppState};
use crate::ui::genotypes::slot_color;
use crate::ui::theme::{semibold, Tokens};
use crate::ui::widgets::{name_widget, rgba};

/// Width of the gradient bar.
pub const BAR_WIDTH: f32 = 260.0;
/// Height of one legend row.
const ROW: f32 = 22.0;

pub fn show(ui: &mut Ui, state: &mut AppState, image: Option<&MapImage>) {
    let t = Tokens::of(ui);
    let Some(frame) = state.frame.clone() else {
        return;
    };
    let layer = state.map.layer;
    ui.horizontal_wrapped(|ui| {
        ui.spacing_mut().item_spacing = vec2(14.0, 6.0);
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 5.0;
            ui.label(
                RichText::new(layer_title(&frame.env, layer))
                    .family(semibold())
                    .color(t.text),
            );
            if !layer.is_categorical() && state.map.is_locked(layer) {
                ui.label(icons::ICON_LOCK.rich_text().size(14.0).color(t.text_weak))
                    .on_hover_text("The colour range is locked.");
            }
            let log =
                image.is_some_and(|img| img.legend.as_ref().is_some_and(|l| l.scale == Scale::Log));
            if log && !layer.is_categorical() {
                ui.label(RichText::new("log scale").small().color(t.text_weak));
            }
            if state.map.vision != ColorVision::Normal {
                ui.label(
                    RichText::new(format!("{} preview", state.map.vision.label()))
                        .small()
                        .color(t.warning),
                )
                .on_hover_text(state.map.vision.description());
            }
        });
        if layer == LayerId::Genotype {
            genotype_chips(ui, state, &frame, &t);
        } else if layer == LayerId::Ancestry {
            family_chips(ui, state, &frame, &t);
        } else {
            match image
                .filter(|i| i.layer == layer)
                .and_then(|i| i.legend.as_ref().map(|l| (i, l)))
            {
                Some((img, legend)) => gradient(ui, img.palette, img.vision, legend, &t),
                None => {
                    ui.label(RichText::new("No values yet").color(t.text_weak));
                }
            }
        }
        empty_key(ui, &t);
    });
}

fn empty_key(ui: &mut Ui, t: &Tokens) {
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 6.0;
        let (r, _) = ui.allocate_exact_size(vec2(12.0, 12.0), Sense::hover());
        ui.painter()
            .rect_filled(r, CornerRadius::same(3), t.map_backdrop);
        crate::ui::map::hatch(ui.painter(), r, t);
        ui.painter().rect_stroke(
            r,
            CornerRadius::same(3),
            Stroke::new(1.0, t.border_strong),
            egui::StrokeKind::Inside,
        );
        ui.label(RichText::new("Empty cell").small().color(t.text_weak));
    });
}

fn gradient(
    ui: &mut Ui,
    palette: Palette,
    vision: ColorVision,
    legend: &avida_viewmodel::Legend,
    t: &Tokens,
) {
    // The bar sits on the row's centre line, tick labels hang below it.
    let (rect, _) = ui.allocate_exact_size(vec2(BAR_WIDTH, 32.0), Sense::hover());
    let bar = Rect::from_min_size(pos2(rect.left(), rect.top() + 5.0), vec2(BAR_WIDTH, 10.0));
    let painter = ui.painter();
    let steps = if palette.is_categorical() {
        CATEGORICAL_SLOTS
    } else {
        64
    };
    let mut mesh = Mesh::default();
    for i in 0..steps {
        let (a, b) = (i as f32 / steps as f32, (i + 1) as f32 / steps as f32);
        let c = rgba(vision.simulate_rgba(palette.sample(f64::from((a + b) / 2.0))));
        let r = Rect::from_min_max(
            pos2(bar.left() + a * bar.width(), bar.top()),
            pos2(bar.left() + b * bar.width(), bar.bottom()),
        );
        mesh.add_colored_rect(r, c);
    }
    painter.add(egui::Shape::mesh(mesh));
    painter.rect_stroke(
        bar,
        CornerRadius::same(2),
        Stroke::new(1.0, t.border_strong),
        egui::StrokeKind::Outside,
    );
    let font = FontId::proportional(11.0);
    let mut last_right = f32::NEG_INFINITY;
    for tick in &legend.ticks {
        let x = bar.left() + tick.pos as f32 * bar.width();
        painter.vline(
            x,
            bar.bottom()..=bar.bottom() + 4.0,
            Stroke::new(1.0, t.text_weak),
        );
        let galley = painter.layout_no_wrap(tick.label.clone(), font.clone(), t.text_weak);
        let w = galley.size().x;
        let left = (x - w / 2.0).clamp(rect.left(), rect.right() - w);
        // A label that would touch the previous one is left out.
        if left < last_right + 6.0 {
            continue;
        }
        painter.galley(pos2(left, bar.bottom() + 5.0), galley, t.text_weak);
        last_right = left + w;
    }
    if legend.ticks.is_empty() {
        painter.text(
            pos2(bar.left(), bar.bottom() + 5.0),
            Align2::LEFT_TOP,
            format::real(legend.min),
            font.clone(),
            t.text_weak,
        );
        painter.text(
            pos2(bar.right(), bar.bottom() + 5.0),
            Align2::RIGHT_TOP,
            format::real(legend.max),
            font,
            t.text_weak,
        );
    }
}

/// One swatch, name and count; clicking a genotype's chip highlights it.
fn chip(
    ui: &mut Ui,
    t: &Tokens,
    color: Color32,
    name: &str,
    count: usize,
    emphasis: bool,
    clickable: bool,
) -> egui::Response {
    let name_font = if emphasis {
        FontId::new(12.0, semibold())
    } else {
        FontId::proportional(12.0)
    };
    let name_color = if emphasis { t.accent } else { t.text };
    let painter = ui.painter();
    let name_g = painter.layout_no_wrap(name.to_string(), name_font, name_color);
    let count_g = painter.layout_no_wrap(
        format::count(count as i64),
        FontId::proportional(12.0),
        t.text_weak,
    );
    let width = 6.0 + 12.0 + 6.0 + name_g.size().x + 6.0 + count_g.size().x + 6.0;
    let sense = if clickable {
        Sense::click()
    } else {
        Sense::hover()
    };
    let (rect, response) = ui.allocate_exact_size(vec2(width, ROW), sense);
    if clickable && response.hovered() {
        ui.painter()
            .rect_filled(rect, CornerRadius::same(6), t.control);
    }
    if response.has_focus() {
        ui.painter().rect_stroke(
            rect,
            CornerRadius::same(6),
            Stroke::new(1.5, t.accent),
            egui::StrokeKind::Inside,
        );
    }
    let cy = rect.center().y;
    let sw = Rect::from_center_size(pos2(rect.left() + 12.0, cy), vec2(12.0, 12.0));
    let p = ui.painter();
    p.rect_filled(sw, CornerRadius::same(3), color);
    if emphasis {
        p.rect_stroke(
            sw.expand(1.5),
            CornerRadius::same(4),
            Stroke::new(1.5, t.accent),
            egui::StrokeKind::Outside,
        );
    }
    let x = sw.right() + 6.0;
    let nh = name_g.size().y;
    let nw = name_g.size().x;
    p.galley(pos2(x, cy - nh / 2.0), name_g, name_color);
    let ch = count_g.size().y;
    p.galley(pos2(x + nw + 6.0, cy - ch / 2.0), count_g, t.text_weak);
    response
}

fn genotype_chips(ui: &mut Ui, state: &mut AppState, frame: &avida_viewmodel::Frame, t: &Tokens) {
    let Some(map) = frame.map.as_ref() else {
        ui.label(RichText::new("No map yet").color(t.text_weak));
        return;
    };
    let mut toggle = None;
    let vision = state.map.vision;
    for (slot, &gid) in map.top_genotypes.iter().enumerate() {
        let name = genotype_name(frame, gid);
        let n = map.cells_of(gid) as usize;
        let on = state.highlight == Some(gid);
        let r = chip(ui, t, slot_color(vision, Some(slot)), &name, n, on, true);
        name_widget(&r, Role::Button, &format!("Highlight {name}"));
        let tip = if on {
            "Click to show all genotypes again"
        } else {
            "Click to highlight this genotype on the map and trace its lineage"
        };
        if r.on_hover_text(format!("{name}: {} cells\n{tip}", format::count(n as i64)))
            .clicked()
        {
            toggle = Some(gid);
        }
    }
    let top: usize = map
        .top_genotypes
        .iter()
        .map(|&g| map.cells_of(g) as usize)
        .sum();
    let others = map.occupied().saturating_sub(top);
    if others > 0 || map.top_genotypes.is_empty() {
        let [r, g, b] = vision.simulate(OTHER_GREY);
        chip(
            ui,
            t,
            Color32::from_rgb(r, g, b),
            "Other genotypes",
            others,
            false,
            false,
        );
    }
    if let Some(gid) = toggle {
        state.toggle_highlight(gid);
    }
}

/// One chip per family of the ancestry layer, largest first, each in the
/// colour it has on the map.
fn family_chips(ui: &mut Ui, state: &AppState, frame: &avida_viewmodel::Frame, t: &Tokens) {
    let Some(map) = frame.map.as_ref().filter(|m| !m.families.is_empty()) else {
        ui.label(RichText::new("No values yet").color(t.text_weak));
        return;
    };
    let vision = state.map.vision;
    let mut in_families = 0usize;
    for f in map.families.iter() {
        let name = format!("Family #{}", f.root);
        let n = f.organisms as usize;
        in_families += n;
        chip(
            ui,
            t,
            slot_color(vision, Some(f.slot)),
            &name,
            n,
            false,
            false,
        )
        .on_hover_text(format!(
            "The descendants of genotype #{} that are not in a family of their own: {} organisms",
            f.root,
            format::count(n as i64)
        ));
    }
    let others = map.occupied().saturating_sub(in_families);
    if others > 0 {
        let [r, g, b] = vision.simulate(OTHER_GREY);
        chip(
            ui,
            t,
            Color32::from_rgb(r, g, b),
            "Other founders",
            others,
            false,
            false,
        );
    }
}
