//! The start screen: open an experiment folder or a recent one.

use std::path::PathBuf;

use egui::{vec2, Align, CornerRadius, Frame, Layout, Margin, RichText, Role, Sense, Stroke, Ui};
use egui_material_icons::icons;

use crate::model::{display_name, format, AppState, OpenProblem, Phase, Request};
use crate::settings::Settings;
use crate::ui::theme::{semibold, Tokens};
use crate::ui::top_bar::logo;
use crate::ui::widgets::{self, name_widget};

pub fn show(ui: &mut Ui, state: &mut AppState, settings: &mut Settings, path_input: &mut String) {
    let t = Tokens::of(ui);
    egui::CentralPanel::default()
        .frame(Frame::new().fill(t.bg))
        .show(ui, |ui| {
            egui::ScrollArea::vertical()
                .auto_shrink([false, false])
                .show(ui, |ui| {
                    let width = (ui.available_width() - 32.0).min(680.0);
                    let top = ((ui.available_height() - 560.0) / 2.0).max(24.0);
                    ui.add_space(top);
                    ui.vertical_centered(|ui| {
                        // Centred column, left-aligned content.
                        ui.allocate_ui_with_layout(
                            vec2(width, 0.0),
                            Layout::top_down(Align::Min),
                            |ui| {
                                ui.set_width(width);
                                content(ui, state, settings, path_input, &t);
                            },
                        );
                    });
                });
        });
}

fn content(
    ui: &mut Ui,
    state: &mut AppState,
    settings: &mut Settings,
    path_input: &mut String,
    t: &Tokens,
) {
    ui.horizontal(|ui| {
        logo(ui, 30.0);
        ui.add_space(4.0);
        ui.vertical(|ui| {
            ui.label(
                RichText::new("Avida")
                    .family(semibold())
                    .size(24.0)
                    .color(t.text),
            );
            ui.label(
                RichText::new(
                    "Digital evolution: self-replicating programs compete for CPU time on a grid.",
                )
                .color(t.text_weak),
            );
        });
    });
    ui.add_space(18.0);

    if state.phase == Phase::Opening {
        widgets::card(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal(|ui| {
                ui.add(egui::Spinner::new().size(18.0));
                let name = state
                    .experiment
                    .as_ref()
                    .map_or_else(String::new, |e| e.name());
                ui.label(RichText::new(format!("Building the world for {name}…")).color(t.text));
            });
        });
        return;
    }

    if let Some(problem) = state.open_problem() {
        problem_card(ui, state, &problem, t);
        ui.add_space(14.0);
    }

    // Two ways in, side by side when there is room.
    let gap = 14.0;
    let wide = ui.available_width() >= 520.0;
    let tile_w = if wide {
        (ui.available_width() - gap) / 2.0
    } else {
        ui.available_width()
    };
    let new_tile = |ui: &mut Ui, state: &mut AppState| {
        tile(
            ui,
            tile_w,
            icons::ICON_CREATE_NEW_FOLDER,
            "New experiment",
            "Start from Avida's default settings: a 60 by 60 world seeded with one self-copying ancestor. The files go into a folder you choose, where you can edit them later.",
            t,
            |ui| {
                let r = widgets::primary_button(
                    ui,
                    Some(icons::ICON_ADD),
                    "New experiment…",
                    0.0,
                )
                .on_hover_text(format!(
                    "Choose a folder for the new experiment ({})",
                    format::shortcut("Cmd N")
                ));
                if r.clicked() {
                    state.request(Request::NewExperiment);
                }
            },
        );
    };
    let open_tile = |ui: &mut Ui, state: &mut AppState, path_input: &mut String| {
        tile(
            ui,
            tile_w,
            icons::ICON_FOLDER_OPEN,
            "Open an experiment",
            "Open a folder that holds avida.cfg, events.cfg and environment.cfg. The run writes its data folder there.",
            t,
            |ui| {
                let r = ui
                    .add(
                        egui::Button::new(RichText::new("Open folder…").color(t.text))
                            .min_size(vec2(0.0, 30.0)),
                    )
                    .on_hover_text(format!("Pick a folder ({})", format::shortcut("Cmd O")));
                if r.clicked() {
                    state.request(Request::PickFolder);
                }
                ui.add_space(6.0);
                path_field(ui, state, path_input);
            },
        );
    };
    if wide {
        ui.horizontal_top(|ui| {
            ui.spacing_mut().item_spacing.x = gap;
            new_tile(ui, state);
            open_tile(ui, state, path_input);
        });
    } else {
        new_tile(ui, state);
        ui.add_space(gap);
        open_tile(ui, state, path_input);
    }

    if !settings.recent.is_empty() {
        ui.add_space(14.0);
        widgets::card(ui, |ui| {
            ui.set_width(ui.available_width());
            widgets::caption(ui, "Recent");
            let mut forget = None;
            for dir in settings.recent.clone() {
                recent_row(ui, t, &dir, state, &mut forget);
            }
            if let Some(dir) = forget {
                settings.forget(&dir);
            }
        });
    }

    ui.add_space(14.0);
    // One galley for the whole paragraph, so that every line has the same
    // height whatever mixes of text and commands it holds (separate labels
    // in a wrapping row get taller rows where the font changes).
    let line_height = 20.0;
    let format = |font_id: egui::FontId, color: egui::Color32| egui::TextFormat {
        font_id,
        color,
        line_height: Some(line_height),
        valign: Align::Center,
        ..Default::default()
    };
    let plain = format(egui::TextStyle::Body.resolve(ui.style()), t.text_weak);
    let code = format(egui::FontId::monospace(12.0), t.text);
    let mut job = egui::text::LayoutJob::default();
    // Non-breaking spaces keep each command on one line.
    let command = |s: &str| s.replace(' ', "\u{a0}");
    job.append("From a terminal, ", 0.0, plain.clone());
    job.append(
        &command("avida-gui <folder> [avida options]"),
        0.0,
        code.clone(),
    );
    job.append(
        " opens a folder directly. Options such as ",
        0.0,
        plain.clone(),
    );
    job.append(&command("-s 7"), 0.0, code.clone());
    job.append(" or ", 0.0, plain.clone());
    job.append(&command("-set WORLD_X 30"), 0.0, code);
    job.append(" are passed on to Avida.", 0.0, plain);
    ui.add(egui::Label::new(job));
}

/// One of the two ways in: an icon, a title, what it does, its actions.
fn tile(
    ui: &mut Ui,
    width: f32,
    icon: egui_material_icons::MaterialIcon,
    title: &str,
    body: &str,
    t: &Tokens,
    actions: impl FnOnce(&mut Ui),
) {
    ui.allocate_ui_with_layout(vec2(width, 0.0), Layout::top_down(Align::Min), |ui| {
        widgets::card_frame(ui).show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.set_min_height(170.0);
            ui.horizontal(|ui| {
                ui.label(icon.rich_text().size(22.0).color(t.accent));
                ui.label(
                    RichText::new(title)
                        .family(semibold())
                        .size(16.0)
                        .color(t.text),
                );
            });
            ui.add_space(4.0);
            ui.add(egui::Label::new(RichText::new(body).color(t.text_weak)).wrap());
            ui.add_space(10.0);
            actions(ui);
        });
    });
}

fn path_field(ui: &mut Ui, state: &mut AppState, path_input: &mut String) {
    let row = vec2(ui.available_width(), ui.spacing().interact_size.y + 6.0);
    ui.allocate_ui_with_layout(row, Layout::right_to_left(Align::Center), |ui| {
        let has_path = !path_input.trim().is_empty();
        let open = ui.add_enabled(
            has_path,
            egui::Button::new("Open").min_size(vec2(56.0, 0.0)),
        );
        let field = egui::TextEdit::singleline(path_input)
            .hint_text("Or paste a folder path")
            .margin(vec2(8.0, 5.0))
            .desired_width(ui.available_width());
        let r = ui.add(field);
        let enter = r.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
        if (enter || open.clicked()) && has_path {
            state.request(Request::Open(PathBuf::from(path_input.trim())));
        }
    });
}

/// Why the last folder could not be opened, in words, with what avida
/// said underneath and the ways forward.
fn problem_card(ui: &mut Ui, state: &mut AppState, problem: &OpenProblem, t: &Tokens) {
    Frame::new()
        .fill(t.danger.gamma_multiply(0.10))
        .stroke(Stroke::new(1.0, t.danger.gamma_multiply(0.45)))
        .corner_radius(CornerRadius::same(10))
        .inner_margin(Margin::same(12))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal_top(|ui| {
                ui.label(icons::ICON_ERROR.rich_text().size(20.0).color(t.danger));
                ui.vertical(|ui| {
                    ui.label(
                        RichText::new(&problem.title)
                            .family(semibold())
                            .color(t.text),
                    );
                    ui.add(
                        egui::Label::new(RichText::new(&problem.hint).color(t.text_weak)).wrap(),
                    );
                    if let Some(detail) = &problem.detail {
                        ui.add_space(4.0);
                        ui.add(
                            egui::Label::new(
                                RichText::new(detail).monospace().size(12.0).color(t.text),
                            )
                            .selectable(true)
                            .wrap(),
                        );
                    }
                    ui.add_space(6.0);
                    ui.horizontal_wrapped(|ui| {
                        if let Some(dir) = problem.create_here.clone() {
                            if ui
                                .button("Create a new experiment here")
                                .on_hover_text(format!(
                                    "Write Avida's default settings into {} and open it. No file is overwritten.",
                                    dir.display()
                                ))
                                .clicked()
                            {
                                state.request(Request::CreateExperimentIn(dir));
                            }
                        }
                        if ui.button("Choose another folder…").clicked() {
                            state.request(Request::PickFolder);
                        }
                        if ui.button("Dismiss").clicked() {
                            state.dismiss_open_error();
                        }
                    });
                });
            });
        });
}

/// `text` with its middle replaced by an ellipsis when it is longer than
/// `max` characters, so both the start and the end of a path stay
/// visible.
pub fn middle_truncate(text: &str, max: usize) -> String {
    let n = text.chars().count();
    if n <= max || max < 5 {
        return text.to_string();
    }
    let keep = max - 1;
    let head = keep / 2;
    let tail = keep - head;
    let start: String = text.chars().take(head).collect();
    let end: String = text.chars().skip(n - tail).collect();
    format!("{start}…{end}")
}

fn recent_row(
    ui: &mut Ui,
    t: &Tokens,
    dir: &std::path::Path,
    state: &mut AppState,
    forget: &mut Option<PathBuf>,
) {
    let height = 42.0;
    let (rect, response) =
        ui.allocate_exact_size(vec2(ui.available_width(), height), Sense::click());
    let name = display_name(dir);
    name_widget(&response, Role::Button, &format!("Open {name}"));
    let exists = dir.is_dir();
    if response.hovered() {
        ui.painter()
            .rect_filled(rect, CornerRadius::same(8), t.control);
    }
    if response.has_focus() {
        ui.painter().rect_stroke(
            rect,
            CornerRadius::same(8),
            Stroke::new(1.5, t.accent),
            egui::StrokeKind::Inside,
        );
    }
    let mut child = ui.new_child(
        egui::UiBuilder::new()
            .max_rect(rect.shrink2(vec2(10.0, 4.0)))
            .layout(Layout::left_to_right(Align::Center)),
    );
    child.label(
        icons::ICON_FOLDER_OPEN
            .rich_text()
            .size(20.0)
            .color(if exists { t.accent } else { t.text_faint }),
    );
    // Leave room for the forget button on the right.
    let text_w = (child.available_width() - 34.0).max(40.0);
    child.vertical(|ui| {
        ui.set_max_width(text_w);
        ui.spacing_mut().item_spacing.y = 0.0;
        ui.label(
            RichText::new(name.as_str())
                .color(t.text)
                .family(semibold()),
        );
        let parent = dir
            .parent()
            .map(|p| p.display().to_string())
            .unwrap_or_default();
        // About 6.5 points per character of the small font.
        let fit = middle_truncate(&parent, ((text_w / 6.5) as usize).max(12));
        let sub = if exists {
            fit
        } else {
            format!("{fit} (missing)")
        };
        ui.label(RichText::new(sub).small().color(t.text_weak));
    });
    child.with_layout(Layout::right_to_left(Align::Center), |ui| {
        if widgets::icon_button_sized(
            ui,
            icons::ICON_CLOSE,
            &format!("Forget {name}"),
            "Remove from the list",
            16.0,
            false,
        )
        .clicked()
        {
            *forget = Some(dir.to_path_buf());
        }
    });
    if response.clicked() && forget.is_none() {
        state.request(Request::Open(dir.to_path_buf()));
    }
    response.on_hover_text(dir.display().to_string());
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn long_paths_keep_both_ends() {
        assert_eq!(middle_truncate("/short", 20), "/short");
        let t = middle_truncate("/Users/someone/projects/avida/experiments/2026", 20);
        assert_eq!(t.chars().count(), 20);
        assert!(t.starts_with("/Users/s"), "{t}");
        assert!(t.ends_with("ments/2026"), "{t}");
        assert!(t.contains('…'));
    }
}
