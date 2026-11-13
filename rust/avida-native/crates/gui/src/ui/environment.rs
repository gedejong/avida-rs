//! The environment tab: resources, reactions and tasks.

use avida_session::LayerId;
use egui::{Layout, RichText, Ui};
use egui_extras::{Column, TableBuilder};
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

/// A column of a table here: its title, whether it holds numbers (which
/// sit at the right edge), and its width.
struct Col {
    title: &'static str,
    numeric: bool,
    width: Width,
}

enum Width {
    /// Shares what the fixed columns leave, equally with the other
    /// flexible ones.
    Flexible,
    Fixed(f32),
}

/// A table that takes the whole width of the panel, with stripes that reach
/// both edges; text keeps 6 points from the right edge and `left_inset` from
/// the left one. `cell` draws one cell of one row.
fn table(
    ui: &mut Ui,
    id: &str,
    cols: &[Col],
    rows: usize,
    t: &Tokens,
    left_inset: f32,
    mut cell: impl FnMut(&mut Ui, usize, usize),
) {
    let mut builder = TableBuilder::new(ui)
        .id_salt(id)
        .striped(true)
        .vscroll(false)
        .cell_layout(Layout::left_to_right(egui::Align::Center));
    for c in cols {
        builder = builder.column(match c.width {
            Width::Flexible => Column::remainder().at_least(56.0).clip(true),
            Width::Fixed(w) => Column::exact(w),
        });
    }
    builder
        .header(22.0, |mut header| {
            for (i, c) in cols.iter().enumerate() {
                header.col(|ui| {
                    let text = RichText::new(c.title).small().color(t.text_faint);
                    if c.numeric {
                        ui.with_layout(Layout::right_to_left(egui::Align::Center), |ui| {
                            ui.add_space(if i + 1 == cols.len() { 6.0 } else { 0.0 });
                            ui.label(text);
                        });
                    } else {
                        ui.add_space(if i == 0 { left_inset } else { 0.0 });
                        ui.label(text);
                    }
                });
            }
        })
        .body(|body| {
            body.rows(22.0, rows, |mut row| {
                let r = row.index();
                for (c, col) in cols.iter().enumerate() {
                    row.col(|ui| {
                        if col.numeric {
                            ui.with_layout(Layout::right_to_left(egui::Align::Center), |ui| {
                                ui.add_space(if c + 1 == cols.len() { 6.0 } else { 0.0 });
                                cell(ui, r, c);
                            });
                        } else {
                            ui.add_space(if c == 0 { left_inset } else { 0.0 });
                            cell(ui, r, c);
                        }
                    });
                }
            });
        });
}

pub fn show(ui: &mut Ui, state: &mut AppState) {
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
        let cols = [
            Col {
                title: "Name",
                numeric: false,
                width: Width::Flexible,
            },
            Col {
                title: "Kind",
                numeric: false,
                width: Width::Fixed(56.0),
            },
            Col {
                title: "Amount",
                numeric: true,
                width: Width::Fixed(64.0),
            },
            Col {
                title: "Inflow",
                numeric: true,
                width: Width::Fixed(56.0),
            },
            Col {
                title: "Outflow",
                numeric: true,
                width: Width::Fixed(60.0),
            },
        ];
        table(
            ui,
            "env-resources",
            &cols,
            env.resources.len(),
            &t,
            0.0,
            |ui, r, c| {
                let res = &env.resources[r];
                match c {
                    0 => {
                        let layer = LayerId::Resource(r);
                        let shown = state.map.layer == layer;
                        let name = ui
                            .selectable_label(shown, res.name.as_str())
                            .on_hover_text("Show this resource on the map");
                        if name.clicked() {
                            state.set_layer(layer);
                        }
                    }
                    1 => {
                        ui.label(
                            RichText::new(geometry(res.geometry, res.spatial)).color(t.text_weak),
                        );
                    }
                    2 => {
                        ui.label(format::real(res.quantity));
                    }
                    3 => {
                        ui.label(res.inflow.map_or("–".to_string(), format::real));
                    }
                    _ => {
                        ui.label(res.outflow.map_or("–".to_string(), format::real));
                    }
                }
            },
        );
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
        let cols = [
            Col {
                title: "Reaction",
                numeric: false,
                width: Width::Flexible,
            },
            Col {
                title: "Task",
                numeric: false,
                width: Width::Flexible,
            },
            Col {
                title: "Value",
                numeric: true,
                width: Width::Fixed(52.0),
            },
            Col {
                title: "Organisms",
                numeric: true,
                width: Width::Fixed(72.0),
            },
        ];
        table(
            ui,
            "env-reactions",
            &cols,
            env.reactions.len(),
            &t,
            6.0,
            |ui, r, c| {
                let re = &env.reactions[r];
                match c {
                    0 => {
                        ui.label(re.name.as_str());
                    }
                    1 => {
                        let task = re
                            .task
                            .and_then(|k| env.tasks.get(k))
                            .map_or("–".to_string(), |t| t.name.clone());
                        ui.label(RichText::new(task).color(t.text_weak));
                    }
                    2 => {
                        ui.label(format::real(re.value));
                    }
                    _ => {
                        ui.label(format::count(i64::from(re.last_count)));
                    }
                }
            },
        );
    }
    ui.add_space(12.0);

    widgets::caption(ui, &format!("Tasks · {}", env.tasks.len()));
    if env.tasks.is_empty() {
        ui.label(RichText::new("No tasks.").color(t.text_weak));
        return;
    }
    let cols = [
        Col {
            title: "Task",
            numeric: false,
            width: Width::Flexible,
        },
        Col {
            title: "Keyword",
            numeric: false,
            width: Width::Fixed(72.0),
        },
        Col {
            title: "Organisms",
            numeric: true,
            width: Width::Fixed(72.0),
        },
    ];
    table(
        ui,
        "env-tasks",
        &cols,
        env.tasks.len(),
        &t,
        0.0,
        |ui, k, c| {
            let task = &env.tasks[k];
            match c {
                0 => {
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
                }
                1 => {
                    ui.label(
                        RichText::new(task.name.as_str())
                            .monospace()
                            .color(t.text_weak),
                    );
                }
                _ => {
                    ui.label(format::count(i64::from(task.last_count)));
                }
            }
        },
    );
}
