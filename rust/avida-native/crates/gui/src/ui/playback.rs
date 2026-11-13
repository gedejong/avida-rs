//! The playback bar: what a recording is, where the frame on show sits in
//! it, and a slider to move it.

use egui::{vec2, Align, Layout, RichText, Ui};

use crate::model::{format, AppState};
use crate::ui::theme::Tokens;
use crate::ui::widgets;

/// Height of the bar.
pub const HEIGHT: f32 = 36.0;

pub fn show(ui: &mut Ui, state: &mut AppState) {
    let Some(info) = state.playback.clone() else {
        return;
    };
    let t = Tokens::of(ui);
    ui.horizontal_centered(|ui| {
        ui.spacing_mut().item_spacing.x = 8.0;
        widgets::badge(ui, "Playback", t.info).on_hover_text(
            "The frames come from a recording, not from a running simulation. \
             Organisms cannot be inspected and settings cannot change.",
        );
        ui.label(
            RichText::new(format!(
                "update {} / {}",
                format::count(i64::from(info.update)),
                format::count(i64::from(info.last_update))
            ))
            .color(t.text),
        );
        let frames = info.frames.max(1);
        let mut index = state.seek_target.unwrap_or(info.index).min(frames - 1);
        let right = ui.available_width();
        ui.allocate_ui_with_layout(
            vec2(right, ui.available_height()),
            Layout::right_to_left(Align::Center),
            |ui| {
                ui.label(
                    RichText::new(format!(
                        "frame {} of {}",
                        format::count(index as i64 + 1),
                        format::count(frames as i64)
                    ))
                    .color(t.text_weak),
                );
                if info.cut_tail_bytes > 0 {
                    ui.label(RichText::new("cut at the end").color(t.warning))
                        .on_hover_text(
                        "The file ends inside a record: the run was killed or is still writing. \
                         The complete frames before it are shown.",
                    );
                }
                ui.spacing_mut().slider_width = (ui.available_width() - 8.0).max(80.0);
                let slider = ui.add(
                    egui::Slider::new(&mut index, 0..=frames - 1)
                        .show_value(false)
                        .smart_aim(false)
                        .trailing_fill(true),
                );
                slider.widget_info(|| {
                    egui::WidgetInfo::labeled(egui::Role::Slider, true, "Playback position")
                });
                if slider.changed() {
                    state.seek_frame(index);
                }
                slider.on_hover_text("Drag to move through the recording (. and , step one frame)");
            },
        );
    });
}
