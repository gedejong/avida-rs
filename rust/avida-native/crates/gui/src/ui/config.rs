//! The config tab: restart the experiment with changed settings, and the
//! effective configuration (searchable, changed settings marked) and the
//! events of the running world.

use egui::{vec2, Align2, CornerRadius, FontId, RichText, Role, Sense, Stroke, Ui};
use egui_material_icons::icons;

use crate::model::config::{cli_line, filter_settings, restarted, FORM_SETTINGS};
use crate::model::{format, AppState, Request};
use crate::ui::theme::{semibold, Tokens};
use crate::ui::widgets::{self, name_widget};

/// Which list the tab shows under the form.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
enum List {
    #[default]
    Settings,
    Events,
}

const SETTING_ROW: f32 = 42.0;
const EVENT_ROW: f32 = 24.0;

pub fn show(ui: &mut Ui, state: &mut AppState) {
    let t = Tokens::of(ui);
    let Some(setup) = state.setup.clone() else {
        widgets::empty_state(
            ui,
            icons::ICON_TUNE,
            "No configuration yet",
            "The settings of the experiment appear here once its world is built.",
        );
        return;
    };
    restart_form(ui, state, &t);
    ui.add_space(8.0);

    let id = ui.scope_id().with("config-list");
    let mut list: List = ui.data(|d| d.get_temp(id)).unwrap_or_default();
    ui.horizontal(|ui| {
        let changed = setup.settings.iter().filter(|s| s.is_changed()).count();
        let settings_label = format!("Settings · {}", format::count(setup.settings.len() as i64));
        let events_label = format!("Events · {}", format::count(setup.events.len() as i64));
        let mut l = list;
        if widgets::segmented(
            ui,
            "config-list",
            &mut l,
            &[
                (
                    List::Settings,
                    settings_label.as_str(),
                    "The effective value of every setting",
                ),
                (
                    List::Events,
                    events_label.as_str(),
                    "The events of events.cfg, as the world read them",
                ),
            ],
        ) {
            list = l;
            ui.data_mut(|d| d.insert_temp(id, list));
        }
        if list == List::Settings {
            ui.label(
                RichText::new(format!("{changed} changed"))
                    .small()
                    .color(t.text_faint),
            )
            .on_hover_text("Settings whose value differs from Avida's default");
        }
    });
    ui.add_space(4.0);
    match list {
        List::Settings => settings_list(ui, state, &setup, &t),
        List::Events => events_list(ui, &setup, &t),
    }
}

fn restart_form(ui: &mut Ui, state: &mut AppState, t: &Tokens) {
    let Some(exp) = state.experiment.clone() else {
        return;
    };
    egui::CollapsingHeader::new(
        RichText::new("Change settings and restart")
            .family(semibold())
            .color(t.text),
    )
    .id_salt("restart-form")
    .default_open(true)
    .show(ui, |ui| {
        ui.spacing_mut().interact_size.y = 22.0;
        let view = &mut state.config_view;
        let field_w = 120.0;
        egui::Grid::new("restart-grid")
            .num_columns(2)
            .spacing(vec2(12.0, 6.0))
            .show(ui, |ui| {
                field(
                    ui,
                    t,
                    "Random seed",
                    "RANDOM_SEED: 0 picks a seed from the clock; any other value repeats the run exactly.",
                    &mut view.overrides.seed,
                    &view.baseline.seed,
                    field_w,
                    "",
                );
                for (i, (_, label, help)) in FORM_SETTINGS.iter().enumerate() {
                    field(
                        ui,
                        t,
                        label,
                        help,
                        &mut view.overrides.values[i],
                        &view.baseline.values[i],
                        field_w,
                        "",
                    );
                }
                field(
                    ui,
                    t,
                    "Update limit",
                    "Run pauses once this update has ended (Run again goes on). Empty: run until the world's Exit event.",
                    &mut view.overrides.update_limit,
                    &view.baseline.update_limit,
                    field_w,
                    "none",
                );
            });
        ui.add_space(6.0);
        let parsed = view.overrides.arguments(&view.baseline);
        let edited = view.is_edited();
        let next = parsed
            .as_ref()
            .ok()
            .map(|(args, limit)| restarted(&exp, args, *limit));
        match (&parsed, &next) {
            (Err(e), _) => {
                ui.label(RichText::new(e.as_str()).small().color(t.danger));
            }
            (Ok(_), Some(next)) => {
                let line = cli_line(&next.dir, &next.args);
                ui.horizontal(|ui| {
                    let copy = widgets::icon_button_sized(
                        ui,
                        icons::ICON_CONTENT_COPY,
                        "Copy command line",
                        "Copy the command that opens this experiment with these settings",
                        15.0,
                        false,
                    );
                    if copy.clicked() {
                        state.request(Request::Copy(line.clone()));
                    }
                    ui.add(
                        egui::Label::new(
                            RichText::new(line.as_str())
                                .monospace()
                                .size(11.5)
                                .color(t.text_weak),
                        )
                        .selectable(true)
                        .wrap(),
                    );
                });
                if let Some(l) = next.update_limit {
                    ui.label(
                        RichText::new(format!(
                            "Run pauses after {} (the limit is the viewer's, not a world argument).",
                            format::update_phrase(l)
                        ))
                        .small()
                        .color(t.text_faint),
                    );
                }
            }
            _ => {}
        }
        ui.add_space(4.0);
        ui.horizontal(|ui| {
            let label = if edited {
                "Restart with changes"
            } else {
                "Restart"
            };
            let r = ui
                .add_enabled_ui(next.is_some(), |ui| {
                    widgets::primary_button(ui, Some(icons::ICON_RESTART_ALT), label, 0.0)
                })
                .inner
                .on_hover_text("Build the world again from the folder: the current run is discarded");
            if r.clicked() {
                if let Some(next) = next.clone() {
                    state.restart(Some(next));
                }
            }
            if edited
                && ui
                    .button("Reset")
                    .on_hover_text("Back to the running world's values")
                    .clicked()
            {
                let base = state.config_view.baseline.clone();
                state.config_view.overrides = base;
            }
        });
    });
}

#[allow(clippy::too_many_arguments)]
fn field(
    ui: &mut Ui,
    t: &Tokens,
    label: &str,
    help: &str,
    value: &mut String,
    baseline: &str,
    width: f32,
    hint: &str,
) {
    let edited = value.trim() != baseline.trim();
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 4.0;
        ui.label(RichText::new(label).color(if edited { t.text } else { t.text_weak }))
            .on_hover_text(help);
        if edited {
            ui.label(RichText::new("●").small().color(t.accent))
                .on_hover_text(format!("Running world: {}", or_none(baseline)));
        }
    });
    let r = ui.add(
        egui::TextEdit::singleline(value)
            .desired_width(width)
            .hint_text(hint),
    );
    name_widget(&r, Role::TextInput, label);
    r.on_hover_text(help);
    ui.end_row();
}

fn or_none(s: &str) -> &str {
    if s.trim().is_empty() {
        "none"
    } else {
        s
    }
}

fn settings_list(
    ui: &mut Ui,
    state: &mut AppState,
    setup: &avida_session::ExperimentSetup,
    t: &Tokens,
) {
    ui.horizontal(|ui| {
        ui.label(icons::ICON_SEARCH.rich_text().size(17.0).color(t.text_weak));
        let r = ui.add(
            egui::TextEdit::singleline(&mut state.config_view.query)
                .hint_text("Search settings")
                .desired_width((ui.available_width() - 130.0).max(80.0)),
        );
        name_widget(&r, Role::SearchInput, "Search settings");
        ui.checkbox(&mut state.config_view.changed_only, "Changed only")
            .on_hover_text("Only settings whose value differs from Avida's default");
    });
    ui.add_space(4.0);
    let shown = filter_settings(
        &setup.settings,
        &state.config_view.query,
        state.config_view.changed_only,
    );
    if shown.is_empty() {
        widgets::empty_state(
            ui,
            icons::ICON_SAVED_SEARCH,
            "No setting matches",
            "Try another word, or show every setting.",
        );
        return;
    }
    list_frame(ui, t, |ui| {
        egui::ScrollArea::vertical()
            .id_salt("settings-rows")
            .auto_shrink([false, false])
            .show_rows(ui, SETTING_ROW, shown.len(), |ui, range| {
                for &i in &shown[range] {
                    setting_row(ui, &setup.settings[i], t);
                }
            });
    });
}

fn list_frame(ui: &mut Ui, t: &Tokens, add: impl FnOnce(&mut Ui)) {
    egui::Frame::new()
        .fill(t.canvas)
        .stroke(Stroke::new(1.0, t.border))
        .corner_radius(CornerRadius::same(8))
        .inner_margin(egui::Margin::symmetric(4, 4))
        .show(ui, |ui| {
            ui.spacing_mut().item_spacing.y = 0.0;
            add(ui);
        });
}

fn setting_row(ui: &mut Ui, s: &avida_session::Setting, t: &Tokens) {
    let (rect, r) = ui.allocate_exact_size(vec2(ui.available_width(), SETTING_ROW), Sense::hover());
    let changed = s.is_changed();
    let p = ui.painter_at(rect);
    if r.hovered() {
        p.rect_filled(rect, CornerRadius::same(5), t.control);
    }
    if changed {
        p.rect_filled(
            egui::Rect::from_min_size(rect.min + vec2(1.0, 7.0), vec2(3.0, SETTING_ROW - 14.0)),
            CornerRadius::same(2),
            t.accent,
        );
    }
    let left = rect.left() + 10.0;
    let top = rect.top() + 12.0;
    let value_g = p.layout_no_wrap(
        s.value.clone(),
        FontId::monospace(12.0),
        if changed { t.accent } else { t.text },
    );
    let value_w = value_g.size().x.min(rect.width() * 0.45);
    let value_left = rect.right() - 8.0 - value_w;
    p.text(
        egui::pos2(left, top),
        Align2::LEFT_CENTER,
        s.name,
        FontId::new(12.5, semibold()),
        t.text,
    );
    p.with_clip_rect(egui::Rect::from_min_max(
        egui::pos2(value_left, rect.top()),
        rect.max,
    ))
    .galley(
        egui::pos2(value_left, top - value_g.size().y / 2.0),
        value_g,
        t.text,
    );
    // Second line: the description, or the default when changed; one
    // line, cut short with an ellipsis (the tooltip has all of it).
    let desc = one_line(s.desc);
    let second = if changed {
        format!("default {} · {desc}", or_none(&s.default))
    } else {
        desc
    };
    let second = if s.from_command_line {
        format!("command line · {second}")
    } else {
        second
    };
    let mut job =
        egui::text::LayoutJob::simple_singleline(second, FontId::proportional(11.5), t.text_faint);
    job.wrap = egui::text::TextWrapping::truncate_at_width(rect.right() - 8.0 - left);
    let g = p.layout_job(job);
    p.galley(egui::pos2(left, rect.top() + 23.0), g, t.text_faint);
    name_widget(&r, Role::Row, &format!("{} = {}", s.name, s.value));
    r.on_hover_ui(|ui| {
        ui.set_max_width(360.0);
        ui.label(RichText::new(s.name).family(semibold()));
        ui.label(format!("{} ({})", s.value, s.kind));
        if changed {
            ui.label(
                RichText::new(format!("Avida's default: {}", or_none(&s.default))).color(t.accent),
            );
        }
        if s.from_command_line {
            ui.label(RichText::new("Set on the command line").color(t.text_weak));
        }
        ui.label(RichText::new(s.desc).color(t.text_weak));
        ui.label(
            RichText::new(format!("{} · {}", s.group, s.group_desc))
                .small()
                .color(t.text_faint),
        );
    });
}

/// Avida's descriptions span lines (`1 = ...` / `2 = ...`); joined with
/// semicolons they fit a row.
fn one_line(text: &str) -> String {
    text.lines()
        .map(str::trim)
        .filter(|l| !l.is_empty())
        .collect::<Vec<_>>()
        .join("; ")
}

fn events_list(ui: &mut Ui, setup: &avida_session::ExperimentSetup, t: &Tokens) {
    let path = setup.event_file.display().to_string();
    ui.add(egui::Label::new(RichText::new(path.as_str()).small().color(t.text_faint)).truncate())
        .on_hover_text(path.as_str());
    if setup.events.is_empty() {
        widgets::empty_state(
            ui,
            icons::ICON_EVENT_NOTE,
            "No events",
            "The event file schedules nothing: the run goes on until you stop it.",
        );
        return;
    }
    list_frame(ui, t, |ui| {
        egui::ScrollArea::vertical()
            .id_salt("event-rows")
            .auto_shrink([false, false])
            .show_rows(ui, EVENT_ROW, setup.events.len(), |ui, range| {
                for e in &setup.events[range] {
                    let (rect, r) = ui
                        .allocate_exact_size(vec2(ui.available_width(), EVENT_ROW), Sense::hover());
                    let p = ui.painter_at(rect);
                    if r.hovered() {
                        p.rect_filled(rect, CornerRadius::same(5), t.control);
                    }
                    let when = format!("{} {}", e.trigger, e.timing);
                    let cy = rect.center().y;
                    p.text(
                        egui::pos2(rect.left() + 8.0, cy),
                        Align2::LEFT_CENTER,
                        &when,
                        FontId::monospace(11.5),
                        t.text_weak,
                    );
                    let x = rect.left() + 8.0 + 110.0;
                    let g =
                        p.layout_no_wrap(e.action.clone(), FontId::new(12.0, semibold()), t.text);
                    let aw = g.size().x;
                    p.galley(egui::pos2(x, cy - g.size().y / 2.0), g, t.text);
                    p.text(
                        egui::pos2(x + aw + 8.0, cy),
                        Align2::LEFT_CENTER,
                        &e.args,
                        FontId::monospace(11.5),
                        t.text_faint,
                    );
                    name_widget(&r, Role::Row, &e.text);
                    r.on_hover_text(e.text.as_str());
                }
            });
    });
}
