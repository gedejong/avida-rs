//! The environment tab: resources, reactions and tasks.

use avida_session::LayerId;
use egui::{vec2, RichText, Ui};
use egui_material_icons::icons;

use crate::model::{format, AppState};
use crate::ui::theme::Tokens;
use crate::ui::widgets;

/// `nGeometry` names.
fn geometry(g: i32, spatial: bool) -> &'static str {
    match g {
        0 => "global",
        1 => "grid",
        2 => "torus",
        3 => "clique",
        4 => "hex",
        5 => "partial",
        6 => "lattice",
        _ if spatial => "spatial",
        _ => "other",
    }
}

pub fn show(ui: &mut Ui, state: &mut AppState) {
    // Dense tables: grid rows need not be as tall as buttons.
    ui.spacing_mut().interact_size.y = 18.0;
    let t = Tokens::of(ui);
    let Some(frame) = state.frame.clone() else {
        widgets::empty_state(
            ui,
            icons::ICON_ECO,
            "No environment",
            "Open an experiment to see its resources and reactions.",
        );
        return;
    };
    let env = &frame.env;

    widgets::caption(ui, &format!("Resources · {}", env.resources.len()));
    if env.resources.is_empty() {
        ui.label(
            RichText::new("No resources: tasks are rewarded without limit.").color(t.text_weak),
        );
    } else {
        egui::Grid::new("env-resources")
            .num_columns(5)
            .striped(true)
            .spacing(vec2(12.0, 4.0))
            .show(ui, |ui| {
                for h in ["Name", "Kind", "Amount", "Inflow", "Outflow"] {
                    ui.label(RichText::new(h).small().color(t.text_faint));
                }
                ui.end_row();
                for (r, res) in env.resources.iter().enumerate() {
                    let layer = LayerId::Resource(r);
                    let shown = state.map.layer == layer;
                    let name = ui
                        .selectable_label(shown, res.name.as_str())
                        .on_hover_text("Show this resource on the map");
                    if name.clicked() {
                        state.set_layer(layer);
                    }
                    ui.label(RichText::new(geometry(res.geometry, res.spatial)).color(t.text_weak));
                    ui.label(format::real(res.quantity));
                    ui.label(res.inflow.map_or("–".to_string(), format::real));
                    ui.label(res.outflow.map_or("–".to_string(), format::real));
                    ui.end_row();
                }
            });
        ui.label(
            RichText::new("Amounts are as of the last resource update the run itself made.")
                .small()
                .color(t.text_faint),
        );
    }
    ui.add_space(12.0);

    widgets::caption(ui, &format!("Reactions · {}", env.reactions.len()));
    if env.reactions.is_empty() {
        ui.label(RichText::new("No reactions.").color(t.text_weak));
    } else {
        egui::Grid::new("env-reactions")
            .num_columns(4)
            .striped(true)
            .spacing(vec2(12.0, 4.0))
            .show(ui, |ui| {
                for h in ["Reaction", "Task", "Value", "Organisms"] {
                    ui.label(RichText::new(h).small().color(t.text_faint));
                }
                ui.end_row();
                for r in &env.reactions {
                    ui.label(r.name.as_str());
                    let task = r
                        .task
                        .and_then(|k| env.tasks.get(k))
                        .map_or("–".to_string(), |t| t.name.clone());
                    ui.label(RichText::new(task).color(t.text_weak));
                    ui.label(format::real(r.value));
                    ui.label(format::count(i64::from(r.last_count)));
                    ui.end_row();
                }
            });
    }
    ui.add_space(12.0);

    widgets::caption(ui, &format!("Tasks · {}", env.tasks.len()));
    if env.tasks.is_empty() {
        ui.label(RichText::new("No tasks.").color(t.text_weak));
        return;
    }
    egui::Grid::new("env-tasks")
        .num_columns(3)
        .striped(true)
        .spacing(vec2(12.0, 4.0))
        .show(ui, |ui| {
            for h in ["Task", "Keyword", "Organisms"] {
                ui.label(RichText::new(h).small().color(t.text_faint));
            }
            ui.end_row();
            for (k, task) in env.tasks.iter().enumerate() {
                let layer = LayerId::Task(k);
                let shown = state.map.layer == layer;
                let label = if task.desc.is_empty() {
                    task.name.as_str()
                } else {
                    task.desc.as_str()
                };
                if ui
                    .selectable_label(shown, label)
                    .on_hover_text("Show this task on the map")
                    .clicked()
                {
                    state.set_layer(layer);
                }
                ui.label(
                    RichText::new(task.name.as_str())
                        .monospace()
                        .color(t.text_weak),
                );
                ui.label(format::count(i64::from(task.last_count)));
                ui.end_row();
            }
        });
}
