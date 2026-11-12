//! The population map: toolbar, zoomable canvas, hover tooltip, cell
//! selection, legend overlay and run-state banners.

use avida_session::{LayerId, RunState};
use avida_viewmodel::{Palette, Scale};
use egui::{
    pos2, vec2, Align, Color32, CornerRadius, Frame, Key, Layout, Margin, Modifiers, Painter, Rect,
    RichText, Role, Sense, Stroke, StrokeKind, TextureHandle, Ui, UiBuilder, Vec2,
};
use egui_material_icons::icons;

use crate::model::commands::palette_name;
use crate::model::layers::{
    cell_info, layer_description, layer_group, layer_title, MapImage, MapSettings,
};
use crate::model::vision::ColorVision;
use crate::model::{format, AppState};
use crate::ui::legend;
use crate::ui::theme::{semibold, Tokens};
use crate::ui::widgets::{self, name_widget, rgba};

/// Grid lines are drawn from this zoom (points per cell) on.
const GRID_FROM_ZOOM: f32 = 14.0;
/// One click of the zoom buttons or `+`/`-`.
const ZOOM_STEP: f32 = 1.5;
/// The longer side of the overview map.
const MINIMAP_SIDE: f32 = 150.0;

/// Fine diagonal lines over `area`: how empty cells look, so they stay
/// distinct from every palette colour (both ends of viridis and cividis
/// are close to the plain backdrop in one theme or the other). The lines
/// are anchored to the screen, so they do not swim while panning.
pub fn hatch(painter: &Painter, area: Rect, t: &Tokens) {
    let color = if t.dark {
        Color32::from_white_alpha(20)
    } else {
        Color32::from_black_alpha(26)
    };
    let area = area.intersect(painter.clip_rect());
    if area.width() <= 0.0 || area.height() <= 0.0 {
        return;
    }
    let p = painter.with_clip_rect(area);
    let step = 5.0;
    let h = area.height();
    let mut x = ((area.left() - h) / step).floor() * step;
    while x < area.right() {
        p.line_segment(
            [pos2(x, area.bottom()), pos2(x + h, area.top())],
            Stroke::new(1.0, color),
        );
        x += step;
    }
}

pub fn show(
    ui: &mut Ui,
    state: &mut AppState,
    texture: Option<&TextureHandle>,
    image: Option<&MapImage>,
) {
    let t = Tokens::of(ui);
    banners(ui, state, &t);
    toolbar(ui, state, image, &t);
    ui.add_space(6.0);
    // The legend sits under the canvas, so it never covers cells.
    egui::Panel::bottom("map-legend")
        .resizable(false)
        .frame(Frame::new().inner_margin(Margin {
            left: 2,
            right: 2,
            top: 8,
            bottom: 0,
        }))
        .show(ui, |ui| legend::show(ui, state, image));
    canvas(ui, state, texture, image, &t);
}

/// How the run ended, over the map, with what to do next.
fn banners(ui: &mut Ui, state: &mut AppState, t: &Tokens) {
    let Some(end) = state.run_end() else {
        return;
    };
    let (color, icon) = if end.failed {
        (t.danger, icons::ICON_ERROR)
    } else {
        (t.success, icons::ICON_CHECK_CIRCLE)
    };
    let fatal = state.fatal.clone().unwrap_or_default();
    let context = state.fatal_context.clone();
    Frame::new()
        .fill(color.gamma_multiply(0.10))
        .stroke(Stroke::new(1.0, color.gamma_multiply(0.45)))
        .corner_radius(CornerRadius::same(8))
        .inner_margin(Margin::symmetric(12, 10))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal_top(|ui| {
                ui.label(icon.rich_text().size(18.0).color(color));
                ui.vertical(|ui| {
                    ui.horizontal_wrapped(|ui| {
                        ui.label(RichText::new(&end.title).family(semibold()).color(t.text));
                        if let Some(code) = end.exit_code {
                            exit_chip(ui, code, t);
                        }
                    });
                    if !end.reason.is_empty() {
                        ui.add(
                            egui::Label::new(RichText::new(&end.reason).color(t.text_weak)).wrap(),
                        );
                    }
                    if end.failed {
                        let text: String = fatal.lines().take(6).collect::<Vec<_>>().join("\n");
                        ui.add(
                            egui::Label::new(
                                RichText::new(text).monospace().size(12.5).color(t.text),
                            )
                            .selectable(true)
                            .wrap(),
                        );
                        let extra: Vec<&String> = context
                            .iter()
                            .filter(|l| !fatal.contains(l.as_str()))
                            .collect();
                        if !extra.is_empty() {
                            ui.add_space(2.0);
                            ui.label(
                                RichText::new("Avida also printed this during the update:")
                                    .small()
                                    .color(t.text_weak),
                            );
                            let lines: Vec<&str> = extra.iter().map(|l| l.as_str()).collect();
                            ui.add(
                                egui::Label::new(
                                    RichText::new(lines.join("\n"))
                                        .monospace()
                                        .size(12.0)
                                        .color(t.text_weak),
                                )
                                .selectable(true)
                                .wrap(),
                            );
                        }
                    }
                    ui.add_space(4.0);
                    ui.horizontal_wrapped(|ui| {
                        if end.failed {
                            if ui.button("Copy the error").clicked() {
                                let mut text = format!("{}\n{fatal}", end.title);
                                for l in &context {
                                    text.push('\n');
                                    text.push_str(l);
                                }
                                state.request(crate::model::Request::Copy(text));
                            }
                            if ui.button("Show the log").clicked() {
                                state.show_log = true;
                            }
                        } else if ui.button("Export statistics…").clicked() {
                            state.request(crate::model::Request::ExportCsv);
                        }
                        if ui
                            .button("Restart")
                            .on_hover_text(
                                "Build the world again from the same folder and settings",
                            )
                            .clicked()
                        {
                            state.restart(None);
                        }
                    });
                });
            });
        });
    ui.add_space(6.0);
}

/// The exit status the `avida` program would end with, as a small chip.
fn exit_chip(ui: &mut Ui, code: i32, t: &Tokens) {
    let color = if code == 0 { t.success } else { t.danger };
    let r = Frame::new()
        .fill(color.gamma_multiply(0.14))
        .corner_radius(CornerRadius::same(6))
        .inner_margin(Margin::symmetric(6, 1))
        .show(ui, |ui| {
            ui.label(
                RichText::new(format!("Exit status {code}"))
                    .size(12.0)
                    .family(semibold())
                    .color(color),
            );
        })
        .response;
    r.on_hover_text(
        "The status the avida command-line program would exit with after this run: 0 when it ended normally.",
    );
}

fn toolbar(ui: &mut Ui, state: &mut AppState, image: Option<&MapImage>, t: &Tokens) {
    let layer = state.map.layer;
    // Below this width the pickers shrink and the world size is left out.
    let narrow = ui.available_width() < 660.0;
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = if narrow { 6.0 } else { 8.0 };
        layer_picker(ui, state, t, if narrow { 150.0 } else { 210.0 });
        let continuous = layer != LayerId::Genotype;
        ui.add_enabled_ui(continuous, |ui| palette_picker(ui, state, t, if narrow { 100.0 } else { 130.0 }))
            .response
            .on_disabled_hover_text("The genotype layer always uses the colour-blind-safe Okabe–Ito palette.");
        let applies = MapSettings::scale_applies(layer);
        ui.add_enabled_ui(applies, |ui| {
            let mut scale = state.map.scale(layer);
            if widgets::segmented(
                ui,
                "scale",
                &mut scale,
                &[
                    (Scale::Linear, "Linear", "Evenly spaced values get evenly spaced colours"),
                    (Scale::Log, "Log", "Evenly spaced orders of magnitude get evenly spaced colours (for values above zero)"),
                ],
            ) {
                state.map.set_scale(layer, scale);
                state.map_dirty = true;
            }
        });
        let legend = image.filter(|i| i.layer == layer).and_then(|i| i.legend.clone());
        let locked = state.map.is_locked(layer);
        ui.add_enabled_ui(continuous && (legend.is_some() || locked), |ui| {
            let (icon, tip) = if locked {
                (icons::ICON_LOCK, "Colour range locked: colours mean the same values in every frame. Click to follow the data again.")
            } else {
                (icons::ICON_LOCK_OPEN, "Lock the colour range at its current values, so colours stay comparable across frames.")
            };
            if widgets::icon_toggle(ui, icon, "Lock legend range", tip, locked).clicked() {
                if locked {
                    state.map.unlock(layer);
                } else if let Some(l) = &legend {
                    state.map.lock(layer, l);
                }
                state.map_dirty = true;
            }
        });

        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            if widgets::icon_button(ui, icons::ICON_FIT_SCREEN, "Fit map", "Fit the world to the view (F)").clicked() {
                state.viewport.auto_fit = true;
            }
            vision_menu(ui, state, t);
            if let Some((w, h)) = state.world_size().filter(|_| !narrow) {
                ui.label(
                    RichText::new(format!("{w} × {h}"))
                        .color(t.text_weak),
                )
                .on_hover_text("World size in cells");
            }
        });
    });
}

fn layer_picker(ui: &mut Ui, state: &mut AppState, t: &Tokens, width: f32) {
    let Some(env) = state.frame.as_ref().map(|f| f.env.clone()) else {
        return;
    };
    let layers = state.layers();
    let current = state.map.layer;
    let mut chosen = None;
    let response = egui::ComboBox::from_id_salt("layer-picker")
        .selected_text((
            icons::ICON_LAYERS.rich_text().size(16.0).color(t.text_weak),
            RichText::new(layer_title(&env, current)),
        ))
        .width(width)
        .height(520.0)
        .show_ui(ui, |ui| {
            ui.set_min_width(240.0);
            let mut group = "";
            for (i, &layer) in layers.iter().enumerate() {
                let g = layer_group(layer);
                if g != group {
                    if !group.is_empty() {
                        ui.add_space(4.0);
                    }
                    widgets::caption(ui, g);
                    group = g;
                }
                let mut b = egui::Button::selectable(layer == current, layer_title(&env, layer));
                if i < 9 {
                    b = b.shortcut_text(RichText::new(format!("{}", i + 1)).color(t.text_faint));
                }
                let r = ui.add(b).on_hover_text(layer_description(layer));
                if r.clicked() {
                    chosen = Some(layer);
                }
            }
        })
        .response;
    name_widget(&response, Role::ComboBox, "Layer");
    response.on_hover_text("Map layer (keys 1–9)");
    if let Some(layer) = chosen {
        state.set_layer(layer);
    }
}

/// The colour-vision preview: the map, legend and genotype colours as a
/// deficiency shows them.
fn vision_menu(ui: &mut Ui, state: &mut AppState, t: &Tokens) {
    let current = state.map.vision;
    let on = current != ColorVision::Normal;
    let icon = icons::ICON_VISIBILITY.rich_text().size(18.0).color(if on {
        t.accent
    } else {
        t.text_weak
    });
    let mut button = egui::Button::new(icon)
        .min_size(egui::Vec2::splat(ui.spacing().interact_size.y))
        .frame_when_inactive(on);
    if on {
        button = button.fill(t.accent_soft);
    }
    let (response, _) = egui::containers::menu::MenuButton::from_button(button).ui(ui, |ui| {
        ui.set_min_width(250.0);
        widgets::caption(ui, "Preview colour vision");
        for v in ColorVision::ALL {
            ui.horizontal(|ui| {
                palette_preview(ui, state.map.palette_for(state.map.layer), v, 40.0);
                let r = ui
                    .selectable_label(current == v, v.label())
                    .on_hover_text(v.description());
                if r.clicked() {
                    state.set_vision(v);
                    ui.close();
                }
            });
        }
        ui.add_space(2.0);
        ui.label(
            RichText::new("A preview only: exports keep the true colours.")
                .small()
                .color(t.text_faint),
        );
    });
    name_widget(&response, Role::Button, "Colour vision");
    response.on_hover_text(if on {
        format!(
            "Previewing {} (click to change)",
            current.label().to_lowercase()
        )
    } else {
        "Preview the map as viewers with a colour-vision deficiency see it".to_string()
    });
}

fn palette_preview(ui: &mut Ui, p: Palette, vision: ColorVision, width: f32) {
    let (rect, _) = ui.allocate_exact_size(vec2(width, 10.0), Sense::hover());
    let steps = if p.is_categorical() { 7 } else { 24 };
    for i in 0..steps {
        let a = i as f32 / steps as f32;
        let b = (i + 1) as f32 / steps as f32;
        let r = Rect::from_min_max(
            pos2(rect.left() + a * width, rect.top()),
            pos2(rect.left() + b * width, rect.bottom()),
        );
        ui.painter().rect_filled(
            r,
            CornerRadius::ZERO,
            rgba(vision.simulate_rgba(p.sample(f64::from((a + b) / 2.0)))),
        );
    }
}

fn palette_picker(ui: &mut Ui, state: &mut AppState, t: &Tokens, width: f32) {
    let shown = state.map.palette_for(state.map.layer);
    let response = egui::ComboBox::from_id_salt("palette-picker")
        .selected_text((
            icons::ICON_PALETTE.rich_text().size(16.0).color(t.text_weak),
            RichText::new(palette_name(shown)),
        ))
        .width(width)
        .show_ui(ui, |ui| {
            ui.set_min_width(200.0);
            for (p, tip) in [
                (Palette::Viridis, "Perceptually uniform, dark purple to yellow; readable with colour-vision deficiency"),
                (Palette::Cividis, "Perceptually uniform for red-green colour-vision deficiency, blue to yellow"),
                (Palette::OkabeIto, "Seven colour-blind-safe colours: quantises the range into bands"),
            ] {
                ui.horizontal(|ui| {
                    palette_preview(ui, p, state.map.vision, 56.0);
                    let r = ui
                        .selectable_label(state.map.palette == p, palette_name(p))
                        .on_hover_text(tip);
                    if r.clicked() {
                        state.map.palette = p;
                        state.map_dirty = true;
                    }
                });
            }
        })
        .response;
    name_widget(&response, Role::ComboBox, "Palette");
}

fn canvas(
    ui: &mut Ui,
    state: &mut AppState,
    texture: Option<&TextureHandle>,
    image: Option<&MapImage>,
    t: &Tokens,
) {
    let size = ui.available_size().max(vec2(64.0, 64.0));
    let (rect, response) = ui.allocate_exact_size(size, Sense::click_and_drag());
    name_widget(&response, Role::Image, "Population map");
    let painter = ui.painter_at(rect);
    painter.rect_filled(rect, CornerRadius::same(10), t.canvas);
    painter.rect_stroke(
        rect,
        CornerRadius::same(10),
        Stroke::new(1.0, t.border),
        StrokeKind::Inside,
    );

    let (Some(texture), Some(image)) = (texture, image) else {
        let msg = if state.frame.is_some() {
            "Rendering the map…"
        } else {
            "Waiting for the first frame…"
        };
        painter.text(
            rect.center(),
            egui::Align2::CENTER_CENTER,
            msg,
            egui::FontId::proportional(14.0),
            t.text_weak,
        );
        return;
    };
    let (w, h) = (image.w, image.h);
    keyboard(ui, state, rect, w, h);
    let vp = &mut state.viewport;
    vp.refit_if_auto(rect, w, h);

    // Zoom (scroll wheel, pinch, ctrl-scroll) and pan (drag).
    if response.hovered() {
        let (scroll, pinch, pointer) = ui.input(|i| {
            (
                i.smooth_scroll_delta.y,
                i.zoom_delta(),
                i.pointer.hover_pos(),
            )
        });
        let factor = pinch * (scroll * 0.002).exp();
        if let Some(p) = pointer {
            if (factor - 1.0).abs() > 1e-4 {
                vp.zoom_at(rect, w, h, p, factor);
            }
        }
    }
    if response.dragged() {
        vp.pan(rect, w, h, response.drag_delta());
    }
    let vp = *vp;

    let world = vp.world_rect(rect, w, h);
    painter.rect_filled(world, CornerRadius::ZERO, t.map_backdrop);
    hatch(&painter, world, t);
    painter.image(
        texture.id(),
        world,
        Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0)),
        Color32::WHITE,
    );
    if vp.zoom >= GRID_FROM_ZOOM {
        grid_lines(&painter, rect, world, &vp, w, h, t);
    }
    painter.rect_stroke(
        world,
        CornerRadius::ZERO,
        Stroke::new(1.0, t.border_strong),
        StrokeKind::Outside,
    );

    // Hover.
    let hovered_cell = response
        .hover_pos()
        .and_then(|p| vp.cell_at(rect, w, h, p))
        .filter(|_| !response.dragged());
    if let Some(cell) = hovered_cell {
        let r = vp.cell_rect(rect, cell % w, cell / w);
        painter.rect_stroke(
            r.expand(0.5),
            CornerRadius::ZERO,
            Stroke::new(1.0, Color32::from_white_alpha(200)),
            StrokeKind::Outside,
        );
    }

    // Selection.
    let mut selected_rect = None;
    if let Some(sel) = state
        .selection
        .and_then(|c| usize::try_from(c).ok())
        .filter(|&c| c < w * h)
    {
        let r = vp.cell_rect(rect, sel % w, sel / w);
        let r = if r.width() < 8.0 {
            Rect::from_center_size(r.center(), egui::Vec2::splat(8.0))
        } else {
            r
        };
        selected_rect = Some(r);
        painter.rect_stroke(
            r,
            CornerRadius::same(2),
            Stroke::new(3.0, Color32::from_black_alpha(200)),
            StrokeKind::Outside,
        );
        painter.rect_stroke(
            r,
            CornerRadius::same(2),
            Stroke::new(1.6, Color32::WHITE),
            StrokeKind::Outside,
        );
    }

    let mut response = response;
    if let Some(cell) = hovered_cell {
        if let Some(frame) = state.frame.as_deref() {
            if let Some(info) = cell_info(frame, state.map.layer, cell) {
                let layer_name = layer_title(&frame.env, state.map.layer);
                response = response.on_hover_ui_at_pointer(|ui| {
                    tooltip(ui, &info, state.map.layer, &layer_name);
                });
            }
        }
    }
    if response.clicked() {
        let cell = response
            .interact_pointer_pos()
            .and_then(|p| vp.cell_at(rect, w, h, p));
        state.select_cell(cell.map(|c| c as i32));
    }
    if response.double_clicked() {
        state.viewport.auto_fit = true;
    }

    overlays(ui, state, rect, selected_rect, t);
    minimap(ui, state, rect, texture, w, h, t);
    zoom_controls(ui, state, rect, w, h, t);
}

/// `+`/`-` zoom around the centre, the arrow keys move the view, when no
/// other widget has the keyboard.
fn keyboard(ui: &Ui, state: &mut AppState, rect: Rect, w: usize, h: usize) {
    let busy = state.command_palette.open
        || state.show_shortcuts
        || ui.ctx().text_edit_focused()
        || ui.memory(|m| m.focused().is_some());
    if busy {
        return;
    }
    let (zoom, pan) = ui.input_mut(|i| {
        let mut zoom = 1.0;
        if i.consume_key(Modifiers::NONE, Key::Plus) || i.consume_key(Modifiers::NONE, Key::Equals)
        {
            zoom *= ZOOM_STEP;
        }
        if i.consume_key(Modifiers::NONE, Key::Minus) {
            zoom /= ZOOM_STEP;
        }
        let step = rect.size() * 0.2;
        let mut pan = Vec2::ZERO;
        for (key, d) in [
            (Key::ArrowLeft, vec2(step.x, 0.0)),
            (Key::ArrowRight, vec2(-step.x, 0.0)),
            (Key::ArrowUp, vec2(0.0, step.y)),
            (Key::ArrowDown, vec2(0.0, -step.y)),
        ] {
            if i.consume_key(Modifiers::NONE, key) {
                pan += d;
            }
        }
        (zoom, pan)
    });
    let vp = &mut state.viewport;
    if zoom != 1.0 {
        vp.refit_if_auto(rect, w, h);
        vp.zoom_at(rect, w, h, rect.center(), zoom);
    }
    if pan != Vec2::ZERO {
        vp.refit_if_auto(rect, w, h);
        vp.pan(rect, w, h, pan);
    }
}

/// The zoom buttons and readout in the canvas' bottom right corner.
fn zoom_controls(ui: &mut Ui, state: &mut AppState, rect: Rect, w: usize, h: usize, t: &Tokens) {
    if rect.width() < 220.0 || rect.height() < 140.0 {
        return;
    }
    let area = Rect::from_min_max(
        pos2(rect.right() - 200.0, rect.bottom() - 52.0),
        pos2(rect.right() - 12.0, rect.bottom() - 12.0),
    );
    let mut child = ui.new_child(
        UiBuilder::new()
            .max_rect(area)
            .layout(Layout::right_to_left(Align::Max)),
    );
    let vp = state.viewport;
    let readout = if vp.auto_fit {
        "Fit".to_string()
    } else {
        format!("{:.0}%", vp.relative_zoom(rect, w, h) * 100.0)
    };
    let mut factor = 1.0;
    let mut fit = false;
    Frame::new()
        .fill(t.surface)
        .stroke(Stroke::new(1.0, t.border_strong))
        .corner_radius(CornerRadius::same(8))
        .inner_margin(Margin::same(2))
        .shadow(egui::Shadow {
            offset: [0, 2],
            blur: 8,
            spread: 0,
            color: t.shadow,
        })
        .show(&mut child, |ui| {
            // The area lays out right to left (so the frame hugs its
            // content in the corner): plus, readout, minus reads as
            // minus, readout, plus.
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 0.0;
                if widgets::icon_button(ui, icons::ICON_ADD, "Zoom in", "Zoom in (+)").clicked() {
                    factor = ZOOM_STEP;
                }
                let r = ui
                    .add(
                        egui::Button::new(
                            RichText::new(&readout)
                                .size(12.5)
                                .color(t.text)
                                .family(semibold()),
                        )
                        .frame(false)
                        .min_size(vec2(52.0, 28.0)),
                    )
                    .on_hover_text("Zoom relative to the whole world. Click to fit the world (F).");
                let name = if vp.auto_fit {
                    "Zoom: the whole world fits the view".to_string()
                } else {
                    format!("Zoom {readout}: fit the whole world")
                };
                name_widget(&r, Role::Button, &name);
                if r.clicked() {
                    fit = true;
                }
                if widgets::icon_button(ui, icons::ICON_REMOVE, "Zoom out", "Zoom out (-)")
                    .clicked()
                {
                    factor = 1.0 / ZOOM_STEP;
                }
            });
        });
    let vp = &mut state.viewport;
    if fit {
        vp.auto_fit = true;
    } else if factor != 1.0 {
        vp.refit_if_auto(rect, w, h);
        vp.zoom_at(rect, w, h, rect.center(), factor);
    }
}

/// An overview of the whole world in the canvas' top right corner while
/// the view shows only part of it: the visible part is outlined, and a
/// click or drag moves the view there.
fn minimap(
    ui: &mut Ui,
    state: &mut AppState,
    rect: Rect,
    texture: &TextureHandle,
    w: usize,
    h: usize,
    t: &Tokens,
) {
    let vp = state.viewport;
    let world = vp.world_rect(rect, w, h);
    if w == 0 || h == 0 || rect.expand(0.5).contains_rect(world) || rect.width() < 360.0 {
        return;
    }
    let scale = MINIMAP_SIDE / w.max(h) as f32;
    let size = vec2(w as f32 * scale, h as f32 * scale);
    let map = Rect::from_min_size(pos2(rect.right() - 16.0 - size.x, rect.top() + 16.0), size);
    let response = ui
        .interact(map, ui.scope_id().with("minimap"), Sense::click_and_drag())
        .on_hover_text("The whole world. Click or drag to move the view.");
    name_widget(&response, Role::Image, "Overview of the whole world");
    let painter = ui.painter_at(rect);
    painter.rect(
        map.expand(4.0),
        CornerRadius::same(6),
        t.surface,
        Stroke::new(1.0, t.border_strong),
        StrokeKind::Inside,
    );
    painter.rect_filled(map, CornerRadius::ZERO, t.map_backdrop);
    painter.image(
        texture.id(),
        map,
        Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0)),
        Color32::WHITE,
    );
    let seen = Rect::from_min_max(
        vp.to_cells(rect, rect.min).to_pos2(),
        vp.to_cells(rect, rect.max).to_pos2(),
    );
    let view = Rect::from_min_max(
        map.min + seen.min.to_vec2() * scale,
        map.min + seen.max.to_vec2() * scale,
    )
    .intersect(map);
    painter.rect_stroke(
        view,
        CornerRadius::ZERO,
        Stroke::new(2.5, Color32::from_black_alpha(180)),
        StrokeKind::Middle,
    );
    painter.rect_stroke(
        view,
        CornerRadius::ZERO,
        Stroke::new(1.2, Color32::WHITE),
        StrokeKind::Middle,
    );
    if response.is_pointer_button_down_on() || response.dragged() {
        if let Some(p) = response.interact_pointer_pos() {
            let vp = &mut state.viewport;
            vp.center = (p - map.min) / scale;
            vp.auto_fit = false;
            vp.clamp(rect, w, h);
        }
    }
}

fn grid_lines(
    painter: &egui::Painter,
    clip: Rect,
    world: Rect,
    vp: &crate::model::viewport::Viewport,
    w: usize,
    h: usize,
    t: &Tokens,
) {
    let color = if t.dark {
        Color32::from_black_alpha(90)
    } else {
        Color32::from_white_alpha(110)
    };
    let stroke = Stroke::new(1.0, color);
    let x0 = ((clip.left() - world.left()) / vp.zoom).floor().max(0.0) as usize;
    let x1 = (((clip.right() - world.left()) / vp.zoom).ceil() as usize).min(w);
    let y0 = ((clip.top() - world.top()) / vp.zoom).floor().max(0.0) as usize;
    let y1 = (((clip.bottom() - world.top()) / vp.zoom).ceil() as usize).min(h);
    for x in x0..=x1 {
        let sx = world.left() + x as f32 * vp.zoom;
        painter.vline(
            sx,
            world.top().max(clip.top())..=world.bottom().min(clip.bottom()),
            stroke,
        );
    }
    for y in y0..=y1 {
        let sy = world.top() + y as f32 * vp.zoom;
        painter.hline(
            world.left().max(clip.left())..=world.right().min(clip.right()),
            sy,
            stroke,
        );
    }
}

fn tooltip(ui: &mut Ui, info: &crate::model::layers::CellInfo, layer: LayerId, layer_name: &str) {
    let t = Tokens::of(ui);
    ui.label(
        RichText::new(format!("Cell ({}, {})", info.x, info.y))
            .family(semibold())
            .color(t.text),
    );
    match &info.genotype {
        Some(g) => {
            ui.horizontal(|ui| {
                let color = rgba(Palette::OkabeIto.category(g.slot));
                widgets::swatch(ui, color, 10.0);
                ui.label(RichText::new(g.name.as_str()).color(t.text));
            });
        }
        None => {
            ui.label(RichText::new("Empty").color(t.text_weak));
        }
    }
    if layer != LayerId::Genotype {
        let value = info.value.map_or("–".to_string(), format::real);
        ui.label(RichText::new(format!("{layer_name}: {value}")).color(t.text_weak));
    }
    ui.label(
        RichText::new("Click to inspect")
            .small()
            .color(t.text_faint),
    );
}

fn overlays(ui: &mut Ui, state: &mut AppState, rect: Rect, selected: Option<Rect>, t: &Tokens) {
    // Mid-update note: what the frame on screen shows (not the live
    // status, which may be a step ahead). Top left, unless the selected
    // cell is there; then bottom left.
    if let Some(fp) = state.frame_progress() {
        if state.run_state() != Some(RunState::Running) {
            let (u, done, total) = (fp.update, fp.slices_done, fp.slices_total);
            let size = vec2((rect.width() - 24.0).min(520.0), 60.0);
            let top = Rect::from_min_size(pos2(rect.left() + 12.0, rect.top() + 12.0), size);
            let covers_selection = selected.is_some_and(|s| s.expand(6.0).intersects(top));
            let (note_rect, layout) = if covers_selection {
                (
                    Rect::from_min_size(
                        pos2(rect.left() + 12.0, rect.bottom() - 12.0 - size.y),
                        size,
                    ),
                    Layout::bottom_up(Align::Min),
                )
            } else {
                (top, Layout::top_down(Align::Min))
            };
            let mut child = ui.new_child(UiBuilder::new().max_rect(note_rect).layout(layout));
            Frame::new()
                .fill(t.surface.gamma_multiply(0.94))
                .stroke(Stroke::new(1.0, t.border))
                .corner_radius(CornerRadius::same(8))
                .inner_margin(Margin::symmetric(10, 6))
                .show(&mut child, |ui| {
                    ui.label(
                        RichText::new(format!(
                            "Paused inside update {} (slice {} of {}): cells show the current state, statistics are from {}.",
                            format::count(i64::from(u)),
                            format::count(i64::from(done)),
                            format::count(i64::from(total)),
                            format::update_phrase(fp.stats_update),
                        ))
                        .small()
                        .color(t.text_weak),
                    );
                });
        }
    }
}
