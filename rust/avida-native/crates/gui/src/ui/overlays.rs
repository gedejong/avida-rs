//! Toasts, the keyboard-shortcut overlay and the drop target shown while
//! a folder is dragged over the window.

use egui::{vec2, Align2, CornerRadius, Frame, Id, Margin, RichText, Stroke};
use egui_material_icons::icons;

use crate::model::log::ToastKind;
use crate::model::{format, AppState};
use crate::ui::theme::{semibold, Tokens};
use crate::ui::widgets;

/// The shortcuts and what they do, by subject (the source of the
/// overlay). `Cmd` is shown as this platform names it; `/` separates
/// alternatives.
pub const SHORTCUT_GROUPS: &[(&str, &[(&str, &str)])] = &[
    (
        "Run",
        &[
            ("Space", "Run or pause"),
            (".", "Step one update"),
            (",", "Step one time slice"),
        ],
    ),
    (
        "Map",
        &[
            ("+ / -", "Zoom in or out"),
            ("Arrows", "Move the view"),
            ("F", "Fit the world to the view"),
            ("1 – 9", "Show map layer 1 to 9"),
            ("Esc", "Clear the selection and highlight"),
            ("Scroll / Pinch", "Zoom at the pointer"),
            ("Drag", "Move the view"),
        ],
    ),
    (
        "Charts",
        &[
            ("Drag", "Move along the run"),
            ("Pinch / Cmd scroll", "Zoom into part of the run"),
            ("Double-click", "Show the whole run"),
        ],
    ),
    (
        "Organism",
        &[
            ("I", "Step the watched organism one CPU cycle"),
            ("B", "Run the watched organism to a breakpoint"),
        ],
    ),
    (
        "Experiment",
        &[
            ("Cmd K", "Find any command"),
            ("Cmd N", "Start a new experiment"),
            ("Cmd O", "Open an experiment folder"),
            ("Cmd S", "Save the population"),
            ("?", "Show or hide this overview"),
        ],
    ),
];

/// Covers the window while files are dragged over it.
pub fn drop_target(ctx: &egui::Context, name: Option<&str>) {
    let t = Tokens::of_ctx(ctx);
    let screen = ctx.content_rect();
    let layer = egui::LayerId::new(egui::Order::Foreground, Id::unique("drop-target"));
    let painter = ctx.layer_painter(layer);
    painter.rect_filled(screen, CornerRadius::ZERO, t.bg.gamma_multiply(0.86));
    let inner = screen.shrink(18.0);
    painter.rect_stroke(
        inner,
        CornerRadius::same(16),
        Stroke::new(2.0, t.accent),
        egui::StrokeKind::Inside,
    );
    painter.text(
        inner.center() - vec2(0.0, 14.0),
        Align2::CENTER_CENTER,
        "Drop to open the experiment",
        egui::FontId::new(20.0, semibold()),
        t.text,
    );
    painter.text(
        inner.center() + vec2(0.0, 16.0),
        Align2::CENTER_CENTER,
        name.unwrap_or("a folder with avida.cfg, or a file inside it"),
        egui::FontId::proportional(14.0),
        t.text_weak,
    );
}

pub fn toasts(ctx: &egui::Context, state: &AppState) {
    if state.toasts.is_empty() {
        return;
    }
    let t = Tokens::of_ctx(ctx);
    egui::Area::new(Id::unique("toasts"))
        .anchor(Align2::RIGHT_BOTTOM, vec2(-16.0, -40.0))
        .order(egui::Order::Foreground)
        .interactable(false)
        .show(ctx, |ui| {
            ui.set_max_width(380.0);
            for toast in &state.toasts {
                let (icon, color) = match toast.kind {
                    ToastKind::Info => (icons::ICON_INFO, t.info),
                    ToastKind::Success => (icons::ICON_CHECK_CIRCLE, t.success),
                    ToastKind::Error => (icons::ICON_ERROR, t.danger),
                };
                Frame::new()
                    .fill(t.surface)
                    .stroke(Stroke::new(1.0, t.border_strong))
                    .corner_radius(CornerRadius::same(10))
                    .inner_margin(Margin::symmetric(12, 9))
                    .shadow(egui::Shadow {
                        offset: [0, 4],
                        blur: 16,
                        spread: 0,
                        color: t.shadow,
                    })
                    .show(ui, |ui| {
                        ui.horizontal(|ui| {
                            ui.label(icon.rich_text().size(18.0).color(color));
                            ui.add(
                                egui::Label::new(RichText::new(toast.text.as_str()).color(t.text))
                                    .wrap(),
                            );
                        });
                    });
                ui.add_space(6.0);
            }
        });
    // Repaint when the next toast expires.
    if let Some(next) = state.toasts.iter().map(|t| t.expires).reduce(f64::min) {
        let wait = (next - state.now).max(0.05);
        ctx.request_repaint_after(std::time::Duration::from_secs_f64(wait));
    }
}

pub fn shortcuts(ctx: &egui::Context, state: &mut AppState) {
    if !state.show_shortcuts {
        return;
    }
    let t = Tokens::of_ctx(ctx);
    let modal = egui::Modal::new(Id::unique("shortcuts")).show(ctx, |ui| {
        let two_columns = ctx.content_rect().width() >= 900.0;
        ui.set_width(if two_columns { 760.0 } else { 440.0 });
        ui.horizontal(|ui| {
            ui.label(icons::ICON_KEYBOARD.rich_text().size(20.0).color(t.accent));
            ui.label(
                RichText::new("Keyboard shortcuts")
                    .family(semibold())
                    .size(16.0),
            );
        });
        ui.add_space(8.0);
        ui.spacing_mut().interact_size.y = 20.0;
        let group = |ui: &mut egui::Ui, title: &str, rows: &[(&str, &str)]| {
            widgets::caption(ui, title);
            egui::Grid::new(("shortcut-grid", title))
                .num_columns(2)
                .min_col_width(110.0)
                .spacing(vec2(14.0, 6.0))
                .show(ui, |ui| {
                    for (keys, what) in rows {
                        ui.horizontal(|ui| {
                            ui.spacing_mut().item_spacing.x = 4.0;
                            for k in keys.split(" / ") {
                                widgets::key_cap(ui, &format::shortcut(k));
                            }
                        });
                        ui.label(RichText::new(*what).color(t.text));
                        ui.end_row();
                    }
                });
            ui.add_space(10.0);
        };
        if two_columns {
            ui.columns(2, |cols| {
                for (i, (title, rows)) in SHORTCUT_GROUPS.iter().enumerate() {
                    // Run, Map | Charts, Organism, Experiment.
                    let col = usize::from(i >= 2);
                    group(&mut cols[col], title, rows);
                }
            });
        } else {
            egui::ScrollArea::vertical()
                .max_height(ctx.content_rect().height() - 160.0)
                .show(ui, |ui| {
                    for (title, rows) in SHORTCUT_GROUPS {
                        group(ui, title, rows);
                    }
                });
        }
        ui.label(
            RichText::new("Drop a folder on the window to open it as an experiment.")
                .color(t.text_weak),
        );
        ui.add_space(6.0);
        ui.vertical_centered(|ui| {
            if ui.button("Close").clicked() {
                state.show_shortcuts = false;
            }
        });
    });
    if modal.should_close() {
        state.show_shortcuts = false;
    }
}
