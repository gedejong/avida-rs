//! The command palette (Cmd/Ctrl+K): a search field over every action,
//! matched letters in bold, shortcuts on the right; arrows move, Enter
//! runs, Esc closes.

use egui::text::LayoutJob;
use egui::{vec2, Align2, CornerRadius, FontId, Id, Key, Margin, RichText, Role, Sense, Stroke};
use egui_material_icons::icons;

use crate::model::commands::{catalog, perform, search, Hit, Item};
use crate::model::AppState;
use crate::settings::Settings;
use crate::ui::theme::{semibold, Tokens};
use crate::ui::widgets::{self, name_widget};

/// Rows shown at once.
const VISIBLE: usize = 9;
const ROW_H: f32 = 34.0;

pub fn show(
    ctx: &egui::Context,
    state: &mut AppState,
    settings: &mut Settings,
    legend: Option<&avida_viewmodel::Legend>,
) {
    if !state.command_palette.open {
        return;
    }
    let t = Tokens::of_ctx(ctx);
    let items = catalog(state, settings);
    let hits = search(&items, &state.command_palette.query);
    let n = hits.len();
    // Keys first, so the field below does not see them.
    let (up, down, enter, escape) = ctx.input_mut(|i| {
        (
            i.consume_key(egui::Modifiers::NONE, Key::ArrowUp),
            i.consume_key(egui::Modifiers::NONE, Key::ArrowDown),
            i.consume_key(egui::Modifiers::NONE, Key::Enter),
            i.consume_key(egui::Modifiers::NONE, Key::Escape),
        )
    });
    let p = &mut state.command_palette;
    if up {
        p.move_selection(-1, n);
    }
    if down {
        p.move_selection(1, n);
    }
    p.selected = p.selected.min(n.saturating_sub(1));
    let mut chosen: Option<usize> = enter.then_some(p.selected).filter(|&s| s < n);
    let mut close = escape;

    let screen = ctx.content_rect();
    let width = (screen.width() - 48.0).clamp(320.0, 620.0);
    let modal = egui::Modal::new(Id::unique("command-palette"))
        .area(
            egui::Modal::default_area(Id::unique("command-palette-area"))
                .anchor(Align2::CENTER_TOP, vec2(0.0, screen.height() * 0.14)),
        )
        .frame(
            egui::Frame::new()
                .fill(t.surface)
                .stroke(Stroke::new(1.0, t.border_strong))
                .corner_radius(CornerRadius::same(12))
                .inner_margin(Margin::same(10))
                .shadow(egui::Shadow {
                    offset: [0, 10],
                    blur: 32,
                    spread: 0,
                    color: t.shadow,
                }),
        )
        .show(ctx, |ui| {
            ui.set_width(width);
            ui.horizontal(|ui| {
                ui.label(icons::ICON_SEARCH.rich_text().size(20.0).color(t.text_weak));
                let field = egui::TextEdit::singleline(&mut state.command_palette.query)
                    .hint_text("Type a command…")
                    .frame(egui::Frame::NONE)
                    .font(FontId::proportional(16.0))
                    .desired_width(ui.available_width());
                let r = ui.add(field);
                name_widget(&r, Role::SearchInput, "Command");
                if r.changed() {
                    state.command_palette.selected = 0;
                }
                r.request_focus();
            });
            ui.add_space(4.0);
            ui.painter().hline(
                ui.max_rect().x_range(),
                ui.cursor().top(),
                Stroke::new(1.0, t.border),
            );
            ui.add_space(6.0);
            if hits.is_empty() {
                ui.add_space(8.0);
                ui.vertical_centered(|ui| {
                    ui.label(RichText::new("No command matches.").color(t.text_weak));
                });
                ui.add_space(8.0);
                return;
            }
            egui::ScrollArea::vertical()
                .id_salt("palette-hits")
                .max_height(ROW_H * VISIBLE as f32)
                .auto_shrink([false, true])
                .show(ui, |ui| {
                    let selected = state.command_palette.selected;
                    for (k, hit) in hits.iter().enumerate() {
                        let r = row(ui, &t, &items[hit.index], hit, k == selected);
                        if (up || down) && k == selected {
                            r.scroll_to_me(None);
                        }
                        if r.clicked() {
                            chosen = Some(k);
                        }
                        if r.hovered() && ui.input(|i| i.pointer.delta() != egui::Vec2::ZERO) {
                            state.command_palette.selected = k;
                        }
                    }
                });
            ui.add_space(4.0);
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 4.0;
                let hint = |ui: &mut egui::Ui, key: &str, what: &str| {
                    widgets::key_cap(ui, key);
                    ui.label(RichText::new(what).small().color(t.text_faint));
                    ui.add_space(8.0);
                };
                hint(ui, "↑↓", "move");
                hint(ui, "Enter", "run");
                hint(ui, "Esc", "close");
            });
        });
    if modal.should_close() {
        close = true;
    }
    if let Some(k) = chosen {
        if let Some(hit) = hits.get(k) {
            let action = items[hit.index].action.clone();
            state.command_palette.close();
            perform(state, settings, action, legend);
            return;
        }
    }
    if close {
        state.command_palette.close();
    }
}

fn row(ui: &mut egui::Ui, t: &Tokens, item: &Item, hit: &Hit, selected: bool) -> egui::Response {
    let (rect, response) =
        ui.allocate_exact_size(vec2(ui.available_width(), ROW_H), Sense::click());
    name_widget(&response, Role::Button, &item.title);
    if selected {
        ui.painter()
            .rect_filled(rect, CornerRadius::same(7), t.accent_soft);
        ui.painter().rect_filled(
            egui::Rect::from_min_size(rect.min + vec2(0.0, 8.0), vec2(3.0, ROW_H - 16.0)),
            CornerRadius::same(2),
            t.accent,
        );
    } else if response.hovered() {
        ui.painter()
            .rect_filled(rect, CornerRadius::same(7), t.control);
    }
    let painter = ui.painter_at(rect);
    // The title, matched characters in the semibold face and accent colour.
    let mut job = LayoutJob::default();
    let plain = egui::TextFormat::simple(FontId::proportional(14.0), t.text);
    let strong = egui::TextFormat::simple(FontId::new(14.0, semibold()), t.accent);
    for (off, ch) in item.title.char_indices() {
        let fmt = if hit.matched.positions.contains(&off) {
            strong.clone()
        } else {
            plain.clone()
        };
        job.append(ch.encode_utf8(&mut [0; 4]), 0.0, fmt);
    }
    let galley = ui.painter().layout_job(job);
    let text_y = rect.center().y - galley.size().y / 2.0;
    painter.galley(
        egui::pos2(rect.left() + 12.0, text_y),
        galley.clone(),
        t.text,
    );
    // The group, faint, after the title (not when it only repeats it).
    let group = if item.group == item.title {
        ""
    } else {
        item.group
    };
    painter.text(
        egui::pos2(rect.left() + 12.0 + galley.size().x + 10.0, rect.center().y),
        Align2::LEFT_CENTER,
        group,
        FontId::proportional(12.0),
        t.text_faint,
    );
    if !item.shortcut.is_empty() {
        let font = FontId::new(11.5, semibold());
        let g = painter.layout_no_wrap(item.shortcut.clone(), font, t.text_weak);
        let cap = egui::Rect::from_min_size(
            egui::pos2(rect.right() - g.size().x - 22.0, rect.center().y - 10.0),
            vec2(g.size().x + 12.0, 20.0),
        );
        painter.rect_filled(cap, CornerRadius::same(5), t.control);
        painter.rect_stroke(
            cap,
            CornerRadius::same(5),
            Stroke::new(1.0, t.border_strong),
            egui::StrokeKind::Inside,
        );
        painter.galley(
            Align2::CENTER_CENTER
                .align_size_within_rect(g.size(), cap)
                .min,
            g,
            t.text_weak,
        );
    }
    response
}
