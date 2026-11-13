//! The status bar and the collapsible log panel.

use egui::{Align, Layout, RichText, Ui};
use egui_material_icons::icons;

use crate::model::log::{LogFilter, LogKind};
use crate::model::{format, AppState, Request};
use crate::ui::theme::Tokens;
use crate::ui::widgets;

fn kind_color(t: &Tokens, kind: LogKind) -> egui::Color32 {
    match kind {
        LogKind::Info => t.info,
        LogKind::Output => t.text_weak,
        LogKind::Stderr => t.warning,
        LogKind::Error | LogKind::Fatal => t.danger,
    }
}

/// The thin bar at the bottom: log toggle, last line, simulation rate.
pub fn status_bar(ui: &mut Ui, state: &mut AppState) {
    let t = Tokens::of(ui);
    ui.horizontal_centered(|ui| {
        ui.spacing_mut().item_spacing.x = 8.0;
        let (errors, warnings) = (state.log.error_count(), state.log.warning_count());
        let plural = |n: usize, what: &str| format!("{n} {what}{}", if n == 1 { "" } else { "s" });
        let mut label = "Log".to_string();
        if errors > 0 {
            label.push_str(&format!(" · {}", plural(errors, "error")));
        }
        if warnings > 0 {
            label.push_str(&format!(" · {}", plural(warnings, "warning")));
        }
        let icon = if state.show_log {
            icons::ICON_EXPAND_MORE
        } else {
            icons::ICON_TERMINAL
        };
        let color = if errors > 0 {
            t.danger
        } else if warnings > 0 {
            t.warning
        } else {
            t.text_weak
        };
        let b = egui::Button::new((
            icon.rich_text().size(15.0).color(color),
            RichText::new(label.as_str()).size(12.0).color(color),
        ))
        .frame_when_inactive(false)
        .small();
        let r = ui.add(b);
        widgets::name_widget(&r, egui::Role::Button, "Toggle log");
        if r.on_hover_text("Show or hide the run log").clicked() {
            state.show_log = !state.show_log;
        }
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            let rate = state.rate.rate(state.now);
            let text = match (state.run_state(), rate) {
                (Some(avida_session::RunState::Running), Some(r)) => format::rate(r),
                // "Starting" holds only until the first update has finished;
                // after that a missing rate (a slow update, or one sample
                // so far) shows nothing rather than a stale word.
                (Some(avida_session::RunState::Running), None) if state.history.is_empty() => {
                    "Starting…".to_string()
                }
                _ => String::new(),
            };
            if !text.is_empty() {
                ui.label(RichText::new(text).size(12.0).color(t.text_weak))
                    .on_hover_text("Simulation speed over the last two seconds");
            }
            if let Some(e) = &state.experiment {
                if !e.args.is_empty() {
                    ui.label(
                        RichText::new(e.args.join(" "))
                            .monospace()
                            .size(11.5)
                            .color(t.text_faint),
                    )
                    .on_hover_text("Arguments passed to the world");
                }
            }
            ui.with_layout(Layout::left_to_right(Align::Center), |ui| {
                if let Some(last) = state.log.last_note() {
                    ui.add(
                        egui::Label::new(
                            RichText::new(last.text.as_str())
                                .monospace()
                                .size(11.5)
                                .color(kind_color(&t, last.kind)),
                        )
                        .truncate(),
                    );
                }
            });
        });
    });
}

/// The log lines with a filter, copy and clear.
pub fn log_panel(ui: &mut Ui, state: &mut AppState) {
    let t = Tokens::of(ui);
    ui.horizontal(|ui| {
        widgets::caption(ui, "Run log");
        let mut filter = state.log_filter;
        if widgets::segmented(
            ui,
            "log-filter",
            &mut filter,
            &[
                (
                    LogFilter::All,
                    "All",
                    "Everything avida prints, plus front-end notes",
                ),
                (LogFilter::Problems, "Problems", "Errors and warnings only"),
            ],
        ) {
            state.log_filter = filter;
        }
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            if widgets::icon_button(ui, icons::ICON_CLOSE, "Close log", "Hide the log").clicked() {
                state.show_log = false;
            }
            if widgets::icon_button(ui, icons::ICON_DELETE, "Clear log", "Clear the log").clicked()
            {
                state.log.clear();
            }
            if widgets::icon_button(
                ui,
                icons::ICON_CONTENT_COPY,
                "Copy log",
                "Copy the shown lines",
            )
            .clicked()
            {
                state.request(Request::CopyLog);
            }
            if state.log.dropped() > 0 {
                ui.label(
                    RichText::new(format!(
                        "{} older lines dropped",
                        format::count(state.log.dropped() as i64)
                    ))
                    .small()
                    .color(t.text_faint),
                );
            }
        });
    });
    let entries = state.log.filtered(state.log_filter);
    let row_h = ui.text_style_height(&egui::TextStyle::Monospace);
    egui::Frame::new()
        .fill(t.canvas)
        .stroke(egui::Stroke::new(1.0, t.border))
        .corner_radius(8.0)
        .inner_margin(egui::Margin::symmetric(10, 6))
        .show(ui, |ui| {
            egui::ScrollArea::vertical()
                .id_salt("log-lines")
                .auto_shrink([false, false])
                .stick_to_bottom(true)
                .show_rows(ui, row_h, entries.len(), |ui, range| {
                    for e in &entries[range] {
                        let mut text = RichText::new(e.text.as_str())
                            .monospace()
                            .color(kind_color(&t, e.kind));
                        if matches!(e.kind, LogKind::Fatal) {
                            text = text.strong();
                        }
                        ui.add(egui::Label::new(text).extend());
                    }
                });
        });
}
