//! The top bar: experiment menu, transport controls, speed, run state and
//! theme.

use avida_session::RunState;
use egui::{
    vec2, Align, Color32, CornerRadius, Layout, PopupCloseBehavior, RichText, Role, Sense, Stroke,
    Ui,
};
use egui_material_icons::icons;

use crate::model::{format, AppState, Request, SpeedSetting};
use crate::settings::{Settings, ThemeChoice};
use crate::ui::theme::{semibold, Tokens};
use crate::ui::widgets::{self, name_widget};

/// The Avida mark: a 3x3 patch of the categorical genotype colours.
pub fn logo(ui: &mut Ui, side: f32) {
    let (rect, _) = ui.allocate_exact_size(vec2(side, side), Sense::hover());
    let cell = side / 3.0;
    let colors = avida_viewmodel::OKABE_ITO;
    let pattern = [0usize, 1, 2, 4, 0, 6, 2, 5, 0];
    for (i, &c) in pattern.iter().enumerate() {
        let (x, y) = ((i % 3) as f32, (i / 3) as f32);
        let r = egui::Rect::from_min_size(rect.min + vec2(x * cell, y * cell), vec2(cell, cell))
            .shrink(0.9);
        let [red, green, blue] = colors[c];
        ui.painter().rect_filled(
            r,
            CornerRadius::same(2),
            Color32::from_rgb(red, green, blue),
        );
    }
}

pub fn run_state_color(t: &Tokens, state: Option<RunState>) -> Color32 {
    match state {
        Some(RunState::Running) => t.success,
        Some(RunState::Paused) => t.info,
        Some(RunState::Finished) => t.text_weak,
        Some(RunState::Failed) => t.danger,
        None => t.text_faint,
    }
}

/// How much room the top bar has: below these widths it drops the
/// wordmark and shortens the speed button, then narrows its fields.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Fit {
    Tight,
    Compact,
    Roomy,
}

impl Fit {
    pub fn for_width(width: f32) -> Fit {
        if width < 1120.0 {
            Fit::Tight
        } else if width < 1300.0 {
            Fit::Compact
        } else {
            Fit::Roomy
        }
    }
}

pub fn show(ui: &mut Ui, state: &mut AppState, settings: &mut Settings) {
    let t = Tokens::of(ui);
    let fit = Fit::for_width(ui.available_width());
    ui.horizontal_centered(|ui| {
        ui.spacing_mut().item_spacing.x = 6.0;
        ui.add_space(4.0);
        logo(ui, 18.0);
        if fit == Fit::Roomy {
            ui.label(RichText::new("Avida").family(semibold()).size(15.0));
        }
        experiment_menu(ui, state, settings, if fit == Fit::Tight { 130.0 } else { 220.0 });
        divider(ui, &t);
        transport(ui, state);
        divider(ui, &t);
        run_to(ui, state, if fit == Fit::Tight { 70.0 } else { 86.0 });
        speed(ui, state, fit);

        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            ui.add_space(2.0);
            let r = widgets::icon_button(
                ui,
                icons::ICON_KEYBOARD,
                "Keyboard shortcuts",
                "Keyboard shortcuts (?)",
            );
            if r.clicked() {
                state.show_shortcuts = !state.show_shortcuts;
            }
            let r = widgets::icon_button(
                ui,
                icons::ICON_SEARCH,
                "Command palette",
                &format!("Find any command ({})", format::shortcut("Cmd K")),
            );
            if r.clicked() {
                state.command_palette.open();
            }
            let icon = match settings.theme {
                ThemeChoice::System => icons::ICON_BRIGHTNESS_AUTO,
                ThemeChoice::Light => icons::ICON_LIGHT_MODE,
                ThemeChoice::Dark => icons::ICON_DARK_MODE,
            };
            let tip = format!("{} (click for {})", settings.theme.label(), settings.theme.next().label().to_lowercase());
            if widgets::icon_button(ui, icon, "Theme", &tip).clicked() {
                settings.theme = settings.theme.next();
            }
            divider(ui, &t);
            let run_state = state.run_state();
            let label = match (run_state, state.is_playback()) {
                (None, _) => "No run",
                (Some(RunState::Running), true) => "Playing",
                (Some(RunState::Paused), true) => "Paused",
                (Some(s), _) => format::run_state(s),
            };
            widgets::badge(ui, label, run_state_color(&t, run_state)).on_hover_text(
                match run_state {
                    Some(RunState::Running) => "The simulation is advancing.",
                    Some(RunState::Paused) => "Waiting for a command.",
                    Some(RunState::Finished) => "The run reached its Exit event or died out.",
                    Some(RunState::Failed) => "The run stopped on an error. The banner over the map and the log say more.",
                    None => "No experiment is open.",
                },
            );
            ui.add_space(4.0);
            readout(
                ui,
                &t,
                "Organisms",
                &state
                    .organisms()
                    .map_or("–".to_string(), |n| format::count(i64::from(n))),
                "",
                "Living organisms after the last finished update.",
                52.0,
            );
            ui.add_space(6.0);
            let update = state.current_update();
            // The slice count only while paused: at full speed it would
            // change the width of the bar every frame.
            let paused = state.run_state() == Some(RunState::Paused);
            let value = match (update, state.in_progress().filter(|_| paused)) {
                (_, Some((u, done, total))) => format!(
                    "{} · {}/{}",
                    format::count(i64::from(u)),
                    format::count(i64::from(done)),
                    format::count(i64::from(total))
                ),
                (Some(u), _) if u >= 0 => format::count(i64::from(u)),
                _ => "–".to_string(),
            };
            let limit = state.update_limit();
            let mut tip = "The update in progress (with its time slices run so far, when paused inside it), or the last finished one.".to_string();
            if let Some(l) = limit {
                tip.push_str(&format!(" Run pauses after {}.", format::update_phrase(l)));
            }
            let suffix = limit.map_or(String::new(), |l| format!(" / {}", format::count(i64::from(l))));
            readout(ui, &t, "Update", &value, &suffix, &tip, 44.0);
        });
    });
}

fn divider(ui: &mut Ui, t: &Tokens) {
    let (rect, _) = ui.allocate_exact_size(vec2(9.0, 22.0), Sense::hover());
    ui.painter()
        .vline(rect.center().x, rect.y_range(), Stroke::new(1.0, t.border));
}

/// A small caption over a number (and a fainter suffix such as the update
/// limit), so each number sits under its own name. The block gets at
/// least `min_w` points (and keeps the widest it has needed), so it does
/// not push its neighbours about as the number grows and shrinks.
fn readout(
    ui: &mut Ui,
    t: &Tokens,
    caption: &str,
    value: &str,
    suffix: &str,
    tip: &str,
    min_w: f32,
) {
    let value_font = egui::FontId::new(14.5, semibold());
    let caption_font = egui::FontId::new(10.5, semibold());
    let suffix_font = egui::FontId::new(12.5, egui::FontFamily::Proportional);
    let painter = ui.painter();
    let v = painter.layout_no_wrap(value.to_string(), value_font, t.text);
    let s = painter.layout_no_wrap(suffix.to_string(), suffix_font, t.text_weak);
    let c = painter.layout_no_wrap(caption.to_uppercase(), caption_font, t.text_weak);
    let id = ui.scope_id().with(("readout", caption));
    let widest: f32 = ui.data(|d| d.get_temp(id)).unwrap_or(0.0);
    let w = (v.size().x + s.size().x)
        .max(c.size().x)
        .max(min_w)
        .max(widest);
    if w > widest {
        ui.data_mut(|d| d.insert_temp(id, w));
    }
    let (rect, response) = ui.allocate_exact_size(vec2(w, 34.0), Sense::hover());
    let painter = ui.painter();
    painter.galley(rect.left_top() + vec2(0.0, 1.0), c, t.text_weak);
    let baseline = rect.left_bottom() - vec2(0.0, v.size().y);
    let suffix_y = rect.bottom() - s.size().y - 1.0;
    let vw = v.size().x;
    painter.galley(baseline, v, t.text);
    painter.galley(egui::pos2(rect.left() + vw, suffix_y), s, t.text_weak);
    let label = format!("{caption} {value}{suffix}");
    response.widget_info(|| egui::WidgetInfo::labeled(Role::Label, true, &label));
    response.on_hover_text(tip);
}

fn experiment_menu(ui: &mut Ui, state: &mut AppState, settings: &Settings, max_w: f32) {
    let t = Tokens::of(ui);
    let name = state
        .experiment
        .as_ref()
        .map_or_else(|| "No experiment".to_string(), |e| e.name());
    let button = egui::Button::new((
        RichText::new(name.clone()).color(t.text),
        icons::ICON_EXPAND_MORE
            .rich_text()
            .size(16.0)
            .color(t.text_weak),
    ))
    .frame_when_inactive(false)
    .truncate();
    let response = ui
        .scope(|ui| {
            ui.set_max_width(max_w);
            egui::menu::MenuButton::from_button(button).ui(ui, |ui| {
            ui.set_min_width(260.0);
            if menu_item(ui, icons::ICON_CREATE_NEW_FOLDER, "New experiment…", &format::shortcut("Cmd N"))
                .on_hover_text("Write Avida's default settings into a folder you choose and open it")
                .clicked()
            {
                state.request(Request::NewExperiment);
                ui.close();
            }
            if menu_item(ui, icons::ICON_FOLDER_OPEN, "Open experiment…", &format::shortcut("Cmd O")).clicked() {
                state.request(Request::PickFolder);
                ui.close();
            }
            ui.menu_button("Open recent", |ui| {
                if settings.recent.is_empty() {
                    ui.label(RichText::new("No recent experiments").color(t.text_weak));
                }
                for dir in &settings.recent {
                    let r = ui
                        .button(crate::model::display_name(dir))
                        .on_hover_text(dir.display().to_string());
                    if r.clicked() {
                        state.request(Request::Open(dir.clone()));
                        ui.close();
                    }
                }
            });
            ui.separator();
            let ready = state.is_ready();
            ui.add_enabled_ui(ready, |ui| {
                if menu_item(ui, icons::ICON_SAVE, "Save population", &format::shortcut("Cmd S"))
                    .on_hover_text("Write detail-<update>.spop to the data directory, as the SavePopulation event does.")
                    .clicked()
                {
                    state.save_population();
                    ui.close();
                }
                if menu_item(ui, icons::ICON_IMAGE, "Export map as PNG…", "").clicked() {
                    state.request(Request::ExportMap);
                    ui.close();
                }
                if menu_item(ui, icons::ICON_TABLE_CHART, "Export statistics as CSV…", "").clicked() {
                    state.request(Request::ExportCsv);
                    ui.close();
                }
                if menu_item(ui, icons::ICON_SHOW_CHART, "Save the charts as PNG…", "").clicked() {
                    state.request(Request::ExportCharts);
                    ui.close();
                }
                if menu_item(ui, icons::ICON_TABLE_VIEW, "Export the genotype table as CSV…", "").clicked() {
                    state.request(Request::ExportGenotypes);
                    ui.close();
                }
                ui.separator();
                if menu_item(ui, icons::ICON_RESTART_ALT, "Restart experiment", "")
                    .on_hover_text("Build the world again with the same arguments (the Config tab can change them)")
                    .clicked()
                {
                    state.restart(None);
                    ui.close();
                }
                if menu_item(ui, icons::ICON_CLOSE, "Close experiment", "").clicked() {
                    state.request(Request::Close);
                    ui.close();
                }
            });
        })
        .0
        })
        .inner;
    name_widget(&response, Role::Button, "Experiment menu");
    let path = state
        .experiment
        .as_ref()
        .map_or_else(String::new, |e| e.dir.display().to_string());
    if !path.is_empty() {
        response.on_hover_text(path);
    }
}

fn menu_item(
    ui: &mut Ui,
    icon: egui_material_icons::MaterialIcon,
    text: &str,
    shortcut: &str,
) -> egui::Response {
    let t = Tokens::of(ui);
    let mut b = egui::Button::new((icon.rich_text().size(17.0).color(t.text_weak), text));
    if !shortcut.is_empty() {
        b = b.shortcut_text(RichText::new(shortcut).color(t.text_faint));
    }
    let r = ui.add(b);
    name_widget(&r, Role::Button, text);
    r
}

fn transport(ui: &mut Ui, state: &mut AppState) {
    let running = state.run_state() == Some(RunState::Running);
    let can = state.can_advance();
    let playback = state.is_playback();
    ui.add_enabled_ui(can, |ui| {
        let (icon, text) = match (running, playback) {
            (true, _) => (icons::ICON_PAUSE, "Pause"),
            (false, true) => (icons::ICON_PLAY_ARROW, "Play"),
            (false, false) => (icons::ICON_PLAY_ARROW, "Run"),
        };
        let r = widgets::primary_button(ui, Some(icon), text, 92.0).on_hover_text(
            match (running, playback) {
                (true, _) => "Pause (Space)",
                (false, true) => "Play the recording from here; at the end it starts again (Space)",
                (false, false) => "Run (Space)",
            },
        );
        if r.clicked() {
            state.toggle_run();
        }
        if widgets::icon_button(
            ui,
            icons::ICON_SKIP_NEXT,
            if playback {
                "Next frame"
            } else {
                "Step update"
            },
            if playback {
                "Next frame (.)"
            } else {
                "Step update (.): finish the update in progress, or run the next one whole"
            },
        )
        .clicked()
        {
            state.step_update();
        }
        if widgets::icon_button(
            ui,
            icons::ICON_STEP,
            if playback {
                "Previous frame"
            } else {
                "Step slice"
            },
            if playback {
                "Previous frame (,)"
            } else {
                "Step slice (,): run one time slice of the update"
            },
        )
        .clicked()
        {
            state.step_slice();
        }
    });
}

fn run_to(ui: &mut Ui, state: &mut AppState, width: f32) {
    let can = state.can_advance();
    ui.add_enabled_ui(can, |ui| {
        let target = state.status.as_ref().and_then(|s| s.run_to);
        let hint = match target {
            Some(n) => format!("→ {}", format::count(i64::from(n))),
            None if state.is_playback() => "Seek to…".to_string(),
            None => "Run to…".to_string(),
        };
        let edit = egui::TextEdit::singleline(&mut state.run_to_input)
            .hint_text(hint)
            .desired_width(width);
        let r = ui.add(edit);
        if state.focus_run_to {
            r.request_focus();
            state.focus_run_to = false;
        }
        let r = r.on_hover_text(if state.is_playback() {
            "Type an update number and press Enter: the playback shows the recorded frame at or before it."
        } else {
            "Type an update number and press Enter: the run goes on until that update has ended, then pauses."
        });
        let submitted = r.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
        let go = widgets::icon_button(
            ui,
            icons::ICON_FAST_FORWARD,
            "Run to update",
            "Run until the update typed on the left has ended",
        );
        if (submitted || go.clicked()) && state.commit_run_to() {
            state.run_to_input.clear();
        }
    });
}

fn speed(ui: &mut Ui, state: &mut AppState, fit: Fit) {
    let t = Tokens::of(ui);
    let current = state.speed;
    let label = match (fit, current.limited) {
        (Fit::Roomy, _) => current.label(),
        (_, false) => "Max".to_string(),
        (_, true) => format!("{}/s", format::rate_number(current.ups)),
    };
    let button = egui::Button::new((
        icons::ICON_SPEED.rich_text().size(17.0).color(t.text_weak),
        RichText::new(label),
    ))
    .min_size(vec2(
        if fit == Fit::Roomy { 108.0 } else { 64.0 },
        ui.spacing().interact_size.y,
    ));
    let response = ui.add(button);
    name_widget(&response, Role::Button, "Speed");
    let response = response.on_hover_text(
        "How fast the run advances: as fast as possible, or a fixed number of updates per second.",
    );
    egui::Popup::from_toggle_button_response(&response)
        .close_behavior(PopupCloseBehavior::CloseOnClickOutside)
        .show(|ui| {
            ui.set_min_width(240.0);
            widgets::caption(ui, "Speed");
            let mut s = state.speed;
            let mut limited = s.limited;
            let changed_mode = widgets::segmented(
                ui,
                "speed-mode",
                &mut limited,
                &[
                    (false, "Max", "As fast as the simulation thread can go"),
                    (true, "Limited", "At most the rate below"),
                ],
            );
            if changed_mode {
                s.limited = limited;
            }
            ui.add_space(4.0);
            let slider = ui.add_enabled(
                s.limited,
                egui::Slider::new(&mut s.ups, SpeedSetting::MIN_UPS..=SpeedSetting::MAX_UPS)
                    .logarithmic(true)
                    .max_decimals(1)
                    .text("updates/s"),
            );
            if changed_mode || slider.changed() {
                state.set_speed(s);
            }
        });
}
