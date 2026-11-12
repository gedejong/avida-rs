//! Small reusable widgets: icon buttons with accessible names, badges,
//! cards, section captions, segmented controls, sparklines.

use egui::{
    vec2, Align2, Color32, CornerRadius, Frame, Margin, Response, RichText, Role, Sense, Stroke,
    StrokeKind, Ui, Vec2, WidgetInfo,
};
use egui_material_icons::MaterialIcon;

use crate::ui::theme::{caption_style, semibold, Tokens};

/// Gives a widget an accessible name (icon-only buttons would otherwise be
/// named by their private-use icon character).
pub fn name_widget(response: &Response, role: Role, name: &str) {
    let enabled = response.enabled();
    let name = name.to_string();
    response.widget_info(|| WidgetInfo::labeled(role, enabled, &name));
}

/// A square button showing only an icon. `name` is its accessible name,
/// `tooltip` what hovering it says.
pub fn icon_button(ui: &mut Ui, icon: MaterialIcon, name: &str, tooltip: &str) -> Response {
    icon_button_sized(ui, icon, name, tooltip, 18.0, false)
}

/// [`icon_button`] that can show a selected (toggled on) state.
pub fn icon_toggle(
    ui: &mut Ui,
    icon: MaterialIcon,
    name: &str,
    tooltip: &str,
    on: bool,
) -> Response {
    icon_button_sized(ui, icon, name, tooltip, 18.0, on)
}

pub fn icon_button_sized(
    ui: &mut Ui,
    icon: MaterialIcon,
    name: &str,
    tooltip: &str,
    size: f32,
    selected: bool,
) -> Response {
    let t = Tokens::of(ui);
    let side = ui.spacing().interact_size.y;
    let color = if selected { t.accent } else { t.text_weak };
    let mut button = egui::Button::new(icon.rich_text().size(size).color(color))
        .min_size(Vec2::splat(side))
        .frame_when_inactive(selected);
    if selected {
        button = button.fill(t.accent_soft);
    }
    let response = ui.add(button);
    name_widget(&response, Role::Button, name);
    response.on_hover_text(tooltip)
}

/// A button with an icon and a label.
pub fn icon_text_button(ui: &mut Ui, icon: MaterialIcon, text: &str, tooltip: &str) -> Response {
    let t = Tokens::of(ui);
    let response = ui.add(egui::Button::new((
        icon.rich_text().size(17.0).color(t.text_weak),
        RichText::new(text),
    )));
    name_widget(&response, Role::Button, text);
    response.on_hover_text(tooltip)
}

/// The prominent button of a view (accent fill).
pub fn primary_button(
    ui: &mut Ui,
    icon: Option<MaterialIcon>,
    text: &str,
    min_width: f32,
) -> Response {
    let t = Tokens::of(ui);
    let label = RichText::new(text).color(t.on_accent).family(semibold());
    let button = match icon {
        Some(icon) => egui::Button::new((icon.rich_text().size(18.0).color(t.on_accent), label)),
        None => egui::Button::new(label),
    }
    .fill(t.accent)
    .stroke(Stroke::NONE)
    .min_size(vec2(min_width, ui.spacing().interact_size.y));
    let response = ui.add(button);
    name_widget(&response, Role::Button, text);
    response
}

/// A pill with a coloured dot: run state, counts.
pub fn badge(ui: &mut Ui, text: &str, color: Color32) -> Response {
    let font = egui::FontId::new(12.0, semibold());
    let galley = ui.painter().layout_no_wrap(text.to_string(), font, color);
    let size = vec2(galley.size().x + 26.0, 22.0);
    let (rect, response) = ui.allocate_exact_size(size, Sense::hover());
    if ui.is_rect_visible(rect) {
        let p = ui.painter();
        p.rect_filled(rect, CornerRadius::same(11), color.gamma_multiply(0.14));
        p.rect_stroke(
            rect,
            CornerRadius::same(11),
            Stroke::new(1.0, color.gamma_multiply(0.35)),
            StrokeKind::Inside,
        );
        let dot = egui::pos2(rect.left() + 11.0, rect.center().y);
        p.circle_filled(dot, 3.5, color);
        p.galley(
            egui::pos2(rect.left() + 19.0, rect.center().y - galley.size().y / 2.0),
            galley,
            color,
        );
    }
    response.widget_info(|| WidgetInfo::labeled(Role::Label, true, text));
    response
}

/// An uppercase caption above a group of controls or numbers.
pub fn caption(ui: &mut Ui, text: &str) -> Response {
    let r = ui.label(caption_text(ui, text));
    // Screen readers get the text as written, not shouted.
    r.widget_info(|| egui::WidgetInfo::labeled(Role::Label, true, text));
    r
}

fn caption_text(ui: &Ui, text: &str) -> RichText {
    RichText::new(text.to_uppercase())
        .text_style(caption_style())
        .color(Tokens::of(ui).text_faint)
        .extra_letter_spacing(0.6)
}

/// How wide [`caption`] draws `text` on one line.
pub fn caption_width(ui: &Ui, text: &str) -> f32 {
    text_width(ui, caption_text(ui, text))
}

/// How wide `text` is on one line (in the body font unless it names
/// another).
pub fn text_width(ui: &Ui, text: impl Into<egui::WidgetText>) -> f32 {
    text.into()
        .into_galley(
            ui,
            Some(egui::TextWrapMode::Extend),
            f32::INFINITY,
            egui::TextStyle::Body,
        )
        .size()
        .x
}

/// A raised container.
pub fn card_frame(ui: &Ui) -> Frame {
    let t = Tokens::of(ui);
    Frame::new()
        .fill(t.surface)
        .stroke(Stroke::new(1.0, t.border))
        .corner_radius(CornerRadius::same(10))
        .inner_margin(Margin::same(12))
}

pub fn card<R>(ui: &mut Ui, add: impl FnOnce(&mut Ui) -> R) -> R {
    card_frame(ui).show(ui, add).inner
}

/// A segmented control: one button per option, the chosen one raised.
/// Returns true when the value changed.
pub fn segmented<T: PartialEq + Copy>(
    ui: &mut Ui,
    id: &str,
    value: &mut T,
    options: &[(T, &str, &str)],
) -> bool {
    let t = Tokens::of(ui);
    let mut changed = false;
    Frame::new()
        .fill(t.control)
        .corner_radius(CornerRadius::same(8))
        .inner_margin(Margin::same(2))
        .show(ui, |ui| {
            ui.push_id(id, |ui| {
                ui.spacing_mut().item_spacing.x = 2.0;
                ui.horizontal(|ui| {
                    for &(option, label, tip) in options {
                        let selected = *value == option;
                        let text = if selected {
                            RichText::new(label).color(t.text).family(semibold())
                        } else {
                            RichText::new(label).color(t.text_weak)
                        };
                        let mut b = egui::Button::selectable(selected, text)
                            .corner_radius(CornerRadius::same(6))
                            .min_size(vec2(0.0, ui.spacing().interact_size.y - 4.0));
                        if selected {
                            b = b.fill(t.surface).stroke(Stroke::new(1.0, t.border_strong));
                        } else {
                            b = b.fill(Color32::TRANSPARENT);
                        }
                        let r = ui.add(b);
                        let r = if tip.is_empty() {
                            r
                        } else {
                            r.on_hover_text(tip)
                        };
                        if r.clicked() && !selected {
                            *value = option;
                            changed = true;
                        }
                    }
                });
            });
        });
    changed
}

/// A small line chart without axes. `points` are `[x, y]`.
pub fn sparkline(ui: &mut Ui, points: &[[f64; 2]], size: Vec2, color: Color32) -> Response {
    let (rect, response) = ui.allocate_exact_size(size, Sense::hover());
    if !ui.is_rect_visible(rect) || points.len() < 2 {
        return response;
    }
    let (mut x0, mut x1, mut y0, mut y1) = (f64::MAX, f64::MIN, f64::MAX, f64::MIN);
    for p in points {
        x0 = x0.min(p[0]);
        x1 = x1.max(p[0]);
        y0 = y0.min(p[1]);
        y1 = y1.max(p[1]);
    }
    if y1 - y0 < 1e-12 {
        y0 -= 1.0;
        y1 += 1.0;
    }
    let dx = (x1 - x0).max(1e-12);
    let inner = rect.shrink2(vec2(1.0, 2.0));
    let pts: Vec<egui::Pos2> = points
        .iter()
        .map(|p| {
            egui::pos2(
                inner.left() + ((p[0] - x0) / dx) as f32 * inner.width(),
                inner.bottom() - ((p[1] - y0) / (y1 - y0)) as f32 * inner.height(),
            )
        })
        .collect();
    let painter = ui.painter_at(rect);
    // A soft area under the line.
    let mut mesh = egui::Mesh::default();
    let fill = color.gamma_multiply(0.16);
    for (i, p) in pts.iter().enumerate() {
        mesh.colored_vertex(*p, fill);
        mesh.colored_vertex(egui::pos2(p.x, inner.bottom()), color.gamma_multiply(0.0));
        if i > 0 {
            let k = (2 * i) as u32;
            mesh.add_triangle(k - 2, k - 1, k);
            mesh.add_triangle(k - 1, k + 1, k);
        }
    }
    painter.add(egui::Shape::mesh(mesh));
    painter.line(pts.clone(), Stroke::new(1.5, color));
    if let Some(last) = pts.last() {
        painter.circle_filled(*last, 2.5, color);
    }
    response
}

/// A filled square in a genotype or palette colour.
pub fn swatch(ui: &mut Ui, color: Color32, side: f32) -> Response {
    let (rect, response) = ui.allocate_exact_size(Vec2::splat(side), Sense::hover());
    ui.painter().rect_filled(rect, CornerRadius::same(3), color);
    response
}

/// A centred message for an empty view.
pub fn empty_state(ui: &mut Ui, icon: MaterialIcon, title: &str, body: &str) {
    let t = Tokens::of(ui);
    ui.add_space(24.0);
    ui.vertical_centered(|ui| {
        ui.label(icon.rich_text().size(34.0).color(t.text_faint));
        ui.add_space(6.0);
        ui.label(
            RichText::new(title)
                .family(semibold())
                .size(15.0)
                .color(t.text),
        );
        ui.add_space(2.0);
        ui.label(RichText::new(body).color(t.text_weak));
    });
}

/// A two-column grid of labels and values.
pub fn kv_grid(ui: &mut Ui, id: &str, rows: &[(&str, String)]) {
    let t = Tokens::of(ui);
    ui.scope(|ui| {
        // Grid rows are at least `interact_size.y` high; text rows need less.
        ui.spacing_mut().interact_size.y = 18.0;
        egui::Grid::new(id)
            .num_columns(2)
            .spacing(vec2(16.0, 5.0))
            .show(ui, |ui| {
                for (k, v) in rows {
                    ui.label(RichText::new(*k).color(t.text_weak));
                    ui.label(RichText::new(v.as_str()).color(t.text));
                    ui.end_row();
                }
            });
    });
}

/// Draws a key cap (`Space`, `Cmd O`).
pub fn key_cap(ui: &mut Ui, key: &str) -> Response {
    let t = Tokens::of(ui);
    let font = egui::FontId::new(11.5, semibold());
    let galley = ui.painter().layout_no_wrap(key.to_string(), font, t.text);
    let size = vec2(galley.size().x.max(10.0) + 12.0, 20.0);
    let (rect, response) = ui.allocate_exact_size(size, Sense::hover());
    let p = ui.painter();
    p.rect_filled(rect, CornerRadius::same(5), t.control);
    p.rect_stroke(
        rect,
        CornerRadius::same(5),
        Stroke::new(1.0, t.border_strong),
        StrokeKind::Inside,
    );
    p.galley(
        Align2::CENTER_CENTER
            .align_size_within_rect(galley.size(), rect)
            .min,
        galley,
        t.text,
    );
    response
}

/// Converts a viewmodel colour.
pub fn rgba(c: [u8; 4]) -> Color32 {
    Color32::from_rgba_unmultiplied(c[0], c[1], c[2], c[3])
}
