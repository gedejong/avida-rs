//! The organism tab: a debugger for the organism in the selected cell.
//!
//! A header names the organism (and what became of it), a toolbar steps
//! it one instruction or runs it to a breakpoint, and four views show its
//! CPU (registers, heads, stacks and a memory listing with breakpoints),
//! phenotype, tasks and I/O buffers. What the last step changed is
//! highlighted. The memory listing fills the height left, scrolls itself,
//! and keeps the instruction pointer in view.

use avida_session::RunState;
use avida_viewmodel::{mem_flags, MemCell, OrganismView, Palette, ThreadView};
use egui::{
    pos2, vec2, Align, Align2, Color32, CornerRadius, FontId, Layout, Rect, RichText, Role, Sense,
    Stroke, Ui,
};
use egui_material_icons::icons;

use crate::model::debugger::{scroll_target, Fate, OrgTab};
use crate::model::{format, AppState};
use crate::ui::genotypes::inst_color;
use crate::ui::theme::{semibold, Tokens};
use crate::ui::widgets::{self, name_widget, rgba};

/// The name of head `i` ([`avida_viewmodel::CpuView::head_names`]).
fn head_label(names: &[String], i: usize) -> &str {
    names.get(i).map_or("Head", String::as_str)
}

/// The name of register `i`: `AX`, `BX`, ... `LX`.
fn reg_name(i: usize) -> String {
    format!("{}X", char::from(b'A' + (i % 26) as u8))
}
/// Height of one memory row.
const ROW: f32 = 20.0;

pub fn show(ui: &mut Ui, state: &mut AppState) {
    let t = Tokens::of(ui);
    let Some(sel) = state.selection else {
        widgets::empty_state(
            ui,
            icons::ICON_BIOTECH,
            "No organism selected",
            "Click a cell on the map to watch the organism living there: step it one instruction at a time, set breakpoints, follow its offspring.",
        );
        if state.dominant_id().is_some() {
            ui.add_space(8.0);
            ui.vertical_centered(|ui| {
                if widgets::icon_text_button(
                    ui,
                    icons::ICON_MY_LOCATION,
                    "Inspect the dominant genotype",
                    "Select an organism of the most abundant genotype",
                )
                .clicked()
                {
                    state.inspect_dominant();
                }
            });
        }
        return;
    };
    let Some(org) = state.debugger.current.clone() else {
        let (x, y) = state.cell_xy(sel);
        match state.debugger.org_id {
            Some(id) => widgets::empty_state(
                ui,
                icons::ICON_CHILD_CARE,
                &format!("Following organism #{id}"),
                &format!("Waiting for the next frame of cell ({x}, {y})…"),
            ),
            None => widgets::empty_state(
                ui,
                icons::ICON_TRIP_ORIGIN,
                &format!("Cell ({x}, {y}) is empty"),
                if state.run_end().is_some() {
                    "No organism lived here when the run ended."
                } else {
                    "Its next occupant will appear here as soon as one is born into it."
                },
            ),
        }
        ui.add_space(8.0);
        ui.vertical_centered(|ui| {
            if ui.button("Clear selection").clicked() {
                state.select_cell(None);
            }
        });
        return;
    };

    header(ui, state, &org, &t);
    ui.add_space(6.0);
    toolbar(ui, state, &t);
    status_line(ui, state, &t);
    mid_slice_note(ui, state, &t);
    ui.add_space(4.0);
    let mut tab = state.debugger.tab;
    if widgets::segmented(
        ui,
        "org-tab",
        &mut tab,
        &[
            (OrgTab::Cpu, "CPU", "Registers, heads, stacks and memory"),
            (
                OrgTab::Phenotype,
                "Phenotype",
                "Merit, fitness, age, gestation",
            ),
            (
                OrgTab::Tasks,
                "Tasks",
                "Tasks performed in this and the last gestation",
            ),
            (OrgTab::Io, "I/O", "Input and output buffers"),
        ],
    ) {
        state.debugger.tab = tab;
    }
    ui.add_space(6.0);
    match state.debugger.tab {
        OrgTab::Cpu => cpu(ui, state, &org, &t),
        tab => {
            egui::ScrollArea::vertical()
                .id_salt(("org-view", tab))
                .auto_shrink([false, false])
                .show(ui, |ui| {
                    ui.spacing_mut().interact_size.y = 18.0;
                    match tab {
                        OrgTab::Phenotype => phenotype(ui, &org),
                        OrgTab::Tasks => tasks(ui, state, &org, &t),
                        OrgTab::Io => buffers(ui, &org, &t),
                        OrgTab::Cpu => {}
                    }
                });
        }
    }
}

fn header(ui: &mut Ui, state: &mut AppState, org: &OrganismView, t: &Tokens) {
    widgets::card(ui, |ui| {
        ui.set_width(ui.available_width());
        ui.horizontal(|ui| {
            let slot = state
                .frame
                .as_ref()
                .and_then(|f| f.map.as_ref())
                .and_then(|m| m.genotype_slot(org.genotype_id));
            let color = state
                .map
                .vision
                .simulate_rgba(Palette::OkabeIto.category(slot));
            widgets::swatch(ui, rgba(color), 14.0);
            ui.label(
                RichText::new(format!("Organism #{}", org.org_id))
                    .family(semibold())
                    .size(15.0)
                    .color(t.text),
            );
            if org.phenotype.to_die {
                widgets::badge(ui, "Dying", t.danger);
            }
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                if widgets::icon_button_sized(
                    ui,
                    icons::ICON_CLOSE,
                    "Clear selection",
                    "Stop watching this cell (Esc)",
                    16.0,
                    false,
                )
                .clicked()
                {
                    state.select_cell(None);
                }
            });
        });
        let genotype = if org.genotype_name.is_empty() {
            "unclassified".to_string()
        } else {
            org.genotype_name.clone()
        };
        ui.horizontal_wrapped(|ui| {
            ui.spacing_mut().item_spacing.x = 6.0;
            ui.label(
                RichText::new(format!(
                    "Cell ({}, {}) · {} · {} · {} offspring",
                    org.x,
                    org.y,
                    genotype,
                    format::born(org.phenotype.birth_update),
                    format::count(i64::from(org.phenotype.num_divides)),
                ))
                .color(t.text_weak),
            );
            if org.genotype_id >= 0 {
                let on = state.highlight == Some(org.genotype_id);
                if ui
                    .small_button(if on {
                        "Clear highlight"
                    } else {
                        "Highlight genotype"
                    })
                    .on_hover_text(
                        "Fade every other genotype on the map and trace this one's lineage",
                    )
                    .clicked()
                {
                    state.toggle_highlight(org.genotype_id);
                }
            }
        });
        fate_banner(ui, state, t);
    });
}

/// Says that the watched organism is gone, and offers to move on.
fn fate_banner(ui: &mut Ui, state: &mut AppState, t: &Tokens) {
    let fate = state.debugger.fate.clone();
    let (text, occupant) = match fate {
        Fate::Alive => return,
        Fate::Died { update } => (
            format!(
                "Died at {}. This is its last state.",
                format::update_phrase(update)
            ),
            None,
        ),
        Fate::Replaced { by, update } => (
            format!(
                "Organism #{by} took over the cell at {}. This is the last state of the one you watched.",
                format::update_phrase(update)
            ),
            Some(by),
        ),
    };
    ui.add_space(6.0);
    egui::Frame::new()
        .fill(t.warning.gamma_multiply(0.12))
        .stroke(Stroke::new(1.0, t.warning.gamma_multiply(0.45)))
        .corner_radius(CornerRadius::same(8))
        .inner_margin(egui::Margin::symmetric(10, 6))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal_wrapped(|ui| {
                ui.label(icons::ICON_SKULL.rich_text().size(16.0).color(t.warning));
                ui.label(RichText::new(text).color(t.text));
                if let Some(by) = occupant {
                    if ui
                        .small_button(format!("Watch #{by}"))
                        .on_hover_text("Debug the organism that lives in the cell now")
                        .clicked()
                    {
                        state.watch_occupant();
                    }
                }
                if ui.small_button("Clear").clicked() {
                    state.select_cell(None);
                }
            });
        });
}

fn toolbar(ui: &mut Ui, state: &mut AppState, t: &Tokens) {
    let alive = state.debugger.is_alive();
    let can = state.can_advance();
    let until = state.status.as_ref().and_then(|s| s.run_until).is_some();
    let running = state.run_state() == Some(RunState::Running);
    let micro_ops = steps_run_micro_ops(state);
    ui.horizontal_wrapped(|ui| {
        ui.spacing_mut().item_spacing.x = 6.0;
        ui.add_enabled_ui(alive && can && !until, |ui| {
            if widgets::icon_text_button(
                ui,
                icons::ICON_STEP_INTO,
                "Step",
                step_tooltip(micro_ops),
            )
            .clicked()
            {
                state.debug_step();
            }
            let has_bp = !state.debugger.breakpoints.is_empty();
            let r = ui
                .add_enabled_ui(has_bp, |ui| {
                    widgets::icon_text_button(
                        ui,
                        icons::ICON_PLAY_CIRCLE,
                        "Run to breakpoint",
                        "Run until this organism's instruction pointer stands on a breakpoint (B)",
                    )
                })
                .inner;
            let r = r.on_disabled_hover_text(
                "Click a position in the memory listing to set a breakpoint first.",
            );
            if r.clicked() {
                state.debug_run_to_breakpoint();
            }
        });
        if (until || running)
            && widgets::icon_text_button(ui, icons::ICON_PAUSE, "Pause", "Pause the run (Space)")
                .clicked()
        {
            state.toggle_run();
        }
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            let on = state.debugger.follow_offspring;
            if widgets::icon_toggle(
                ui,
                icons::ICON_CHILD_CARE,
                "Follow offspring",
                if on {
                    "Following offspring: when this organism divides, the watch moves to its child. Click to stay with the parent."
                } else {
                    "Stay with this organism when it divides. Click to move the watch to each new child instead."
                },
                on,
            )
            .clicked()
            {
                state.set_follow_offspring(!on);
            }
            let ip = state.debugger.follow_ip;
            if widgets::icon_toggle(
                ui,
                icons::ICON_MY_LOCATION,
                "Follow instruction pointer",
                if ip {
                    "The memory listing scrolls to keep the instruction pointer in view. Click to scroll freely."
                } else {
                    "Click to keep the instruction pointer in view."
                },
                ip,
            )
            .clicked()
            {
                if ip {
                    // Switching it off keeps the space listed now.
                    let now = state.debugger.listed_space();
                    state.debugger.pin_space(Some(now));
                } else {
                    state.debugger.pin_space(None);
                }
                state.debugger.follow_ip = !ip;
                state.debugger.scrolled_to = None;
            }
            let _ = t;
        });
    });
}

/// What the flag column of the memory listing means.
const MEMORY_LEGEND: &str = "✓ executed  © copied  * mutated";

/// Whether one step (one CPU cycle) of the watched organism can run several
/// micro-operations: BCR and GP8 hardware.
fn steps_run_micro_ops(state: &AppState) -> bool {
    state
        .debugger
        .current
        .as_ref()
        .is_some_and(|o| o.cpu.cycle_runs_micro_ops())
}

/// The hint under the toolbar before anything has been stepped (one line;
/// the CPU view explains micro-operations, [`MICRO_OPS_NOTE`]).
pub fn step_hint(micro_ops: bool) -> &'static str {
    if micro_ops {
        "Step runs one CPU cycle. Click a position in memory to set a breakpoint."
    } else {
        "Step runs one instruction. Click a position in memory to set a breakpoint."
    }
}

/// Under the CPU view's header on BCR and GP8 hardware.
pub const MICRO_OPS_NOTE: &str = "On this hardware a CPU cycle runs several micro-operations, \
     so one step can move the instruction pointer more than one position.";

/// What the Step button's tooltip says.
pub fn step_tooltip(micro_ops: bool) -> &'static str {
    if micro_ops {
        "Run this organism for one CPU cycle (I). On this hardware a cycle runs several \
         micro-operations, so the instruction pointer can move more than one position. \
         The rest of the world runs alongside."
    } else {
        "Run this organism for one instruction (I). The rest of the world runs alongside."
    }
}

fn status_line(ui: &mut Ui, state: &AppState, t: &Tokens) {
    let run_until = state.status.as_ref().and_then(|s| s.run_until);
    // A fixed height, so the views below do not jump as the line changes.
    let (rect, _) = ui.allocate_exact_size(vec2(ui.available_width(), 20.0), Sense::hover());
    let mut child = ui.new_child(
        egui::UiBuilder::new()
            .max_rect(rect)
            .layout(Layout::left_to_right(Align::Center)),
    );
    child.spacing_mut().item_spacing.x = 6.0;
    if let Some((slices, _)) = run_until {
        child.add(egui::Spinner::new().size(12.0));
        child.label(
            RichText::new(format!("Running… {}", format::plural(slices, "time slice")))
                .small()
                .color(t.text_weak),
        );
        return;
    }
    match &state.debugger.last_stop {
        Some(note) => {
            let (icon, color) = if note.is_problem() {
                (icons::ICON_INFO, t.warning)
            } else {
                (icons::ICON_CHECK_CIRCLE, t.success)
            };
            child.label(icon.rich_text().size(14.0).color(color));
            child.add(
                egui::Label::new(RichText::new(note.text()).small().color(t.text_weak)).truncate(),
            );
        }
        None => {
            child.add(
                egui::Label::new(
                    RichText::new(step_hint(steps_run_micro_ops(state)))
                        .small()
                        .color(t.text_faint),
                )
                .truncate(),
            );
        }
    }
}

/// With speculative execution one time slice of an organism runs up to 33
/// instructions; the debugger stops between any two of them. Said when a
/// pause falls inside such a slice, since the slice counter does not move
/// while its instructions are stepped.
fn mid_slice_note(ui: &mut Ui, state: &AppState, t: &Tokens) {
    if !state.status.as_ref().is_some_and(|s| s.mid_slice) {
        return;
    }
    ui.horizontal_wrapped(|ui| {
        ui.spacing_mut().item_spacing.x = 6.0;
        ui.label(icons::ICON_INFO.rich_text().size(14.0).color(t.info));
        ui.label(
            RichText::new(
                "Paused inside a time slice: the organism runs ahead speculatively here, one instruction per step.",
            )
            .small()
            .color(t.text_weak),
        );
    });
}

fn phenotype(ui: &mut Ui, org: &OrganismView) {
    let p = &org.phenotype;
    let rows = [
        ("Merit", format::real(p.merit)),
        ("Fitness", format::real(p.fitness)),
        ("Current bonus", format::real(p.cur_bonus)),
        ("Gestation time", format::count(i64::from(p.gestation_time))),
        ("Age", format::plural(p.age.max(0) as u64, "update")),
        ("Generation", format::count(i64::from(p.generation))),
        ("Genome length", format::count(i64::from(p.genome_length))),
        (
            "Copied / executed",
            format!(
                "{} / {}",
                format::count(i64::from(p.copied_size)),
                format::count(i64::from(p.executed_size))
            ),
        ),
        (
            "CPU cycles used",
            format::count(i64::from(p.cpu_cycles_used)),
        ),
        (
            "CPU cycles in all",
            format::count(i64::try_from(p.lifetime_cpu_cycles).unwrap_or(i64::MAX)),
        ),
        ("Offspring", format::count(i64::from(p.num_divides))),
        ("Born at", format::update_phrase(p.birth_update)),
        (
            "Flags",
            [
                (p.is_modified, "modified"),
                (p.is_multi_thread, "multi-threaded"),
                (p.parent_true, "breeds true"),
            ]
            .iter()
            .filter(|(on, _)| *on)
            .map(|(_, s)| *s)
            .collect::<Vec<_>>()
            .join(", "),
        ),
    ];
    widgets::kv_grid(ui, "phenotype", &rows);
}

fn tasks(ui: &mut Ui, state: &AppState, org: &OrganismView, t: &Tokens) {
    let Some(frame) = state.frame.as_deref() else {
        return;
    };
    if frame.env.tasks.is_empty() {
        widgets::empty_state(
            ui,
            icons::ICON_CHECKLIST,
            "No tasks",
            "This environment rewards no tasks.",
        );
        return;
    }
    egui::Grid::new("org-tasks")
        .num_columns(3)
        .spacing(vec2(14.0, 3.0))
        .show(ui, |ui| {
            ui.label(RichText::new("Task").small().color(t.text_faint));
            ui.label(RichText::new("This gestation").small().color(t.text_faint));
            ui.label(RichText::new("Last gestation").small().color(t.text_faint));
            ui.end_row();
            for (k, task) in frame.env.tasks.iter().enumerate() {
                let cur = org.phenotype.cur_task_count.get(k).copied().unwrap_or(0);
                let last = org.phenotype.last_task_count.get(k).copied().unwrap_or(0);
                let name = if task.desc.is_empty() {
                    &task.name
                } else {
                    &task.desc
                };
                let active = cur > 0 || last > 0;
                ui.label(RichText::new(name.as_str()).color(if active {
                    t.text
                } else {
                    t.text_faint
                }));
                ui.label(count_text(cur, t));
                ui.label(count_text(last, t));
                ui.end_row();
            }
        });
}

fn count_text(n: i32, t: &Tokens) -> RichText {
    if n > 0 {
        RichText::new(format::count(i64::from(n)))
            .color(t.success)
            .family(semibold())
    } else {
        RichText::new("–").color(t.text_faint)
    }
}

fn buffers(ui: &mut Ui, org: &OrganismView, t: &Tokens) {
    let rows: [(&str, &Vec<i32>); 3] = [
        ("Cell inputs", &org.cell_inputs),
        ("Input buffer", &org.input_buf),
        ("Output buffer", &org.output_buf),
    ];
    egui::Grid::new("io-buffers")
        .num_columns(2)
        .spacing(vec2(14.0, 3.0))
        .show(ui, |ui| {
            for (name, values) in rows {
                ui.label(RichText::new(name).color(t.text_weak));
                if values.is_empty() {
                    ui.label(RichText::new("empty").color(t.text_faint));
                } else {
                    ui.vertical(|ui| {
                        for v in values {
                            ui.label(
                                RichText::new(format!("{}  {}", format::hex(*v), v))
                                    .monospace()
                                    .size(12.0),
                            );
                        }
                    });
                }
                ui.end_row();
            }
        });
    ui.label(
        RichText::new("Buffers list the newest value first.")
            .small()
            .color(t.text_faint),
    );
}

// --- the CPU view -----------------------------------------------------------

fn cpu(ui: &mut Ui, state: &mut AppState, org: &OrganismView, t: &Tokens) {
    let cpu = &org.cpu;
    ui.horizontal_wrapped(|ui| {
        ui.spacing_mut().item_spacing.x = 6.0;
        ui.label(
            RichText::new(format!(
                "{} · hardware type {} · {} ({} since the last divide)",
                cpu.inst_set,
                cpu.hw_type,
                format::plural(org.phenotype.lifetime_cpu_cycles, "cycle"),
                format::count(i64::from(org.phenotype.cpu_cycles_used))
            ))
            .small()
            .color(t.text_weak),
        );
    });
    if cpu.cycle_runs_micro_ops() {
        ui.label(RichText::new(MICRO_OPS_NOTE).small().color(t.text_weak));
    }
    if cpu.threads.len() > 1 {
        ui.horizontal_wrapped(|ui| {
            ui.spacing_mut().item_spacing.x = 4.0;
            for (i, th) in cpu.threads.iter().enumerate() {
                let label = if i == cpu.cur_thread {
                    format!("Thread {} ●", th.id)
                } else {
                    format!("Thread {}", th.id)
                };
                let r = ui
                    .selectable_label(state.debugger.thread_index() == i, label)
                    .on_hover_text(if i == cpu.cur_thread {
                        "The thread that runs next"
                    } else {
                        "Show this thread's registers, heads and stacks"
                    });
                if r.clicked() {
                    state.debugger.thread = i;
                    state.debugger.scrolled_to = None;
                    // A parasite's thread runs in its own space: list that.
                    state.debugger.pin_space(None);
                }
            }
        });
    }
    let (cur, _) = state.debugger.threads();
    if let Some(th) = cur.cloned() {
        ui.add_space(2.0);
        if !th.status.is_empty() {
            ui.label(
                RichText::new(format!("Thread {}: {}", th.id, th.status))
                    .small()
                    .color(t.text_weak),
            );
            ui.add_space(2.0);
        }
        registers(ui, state, &th, t);
        ui.add_space(4.0);
        heads(ui, state, &th, &cpu.head_names, cpu.memory.len(), t);
        ui.add_space(2.0);
        stacks(ui, state, &th, t);
    }
    ui.add_space(6.0);
    memory(ui, state, org, t);
}

fn registers(ui: &mut Ui, state: &AppState, th: &ThreadView, t: &Tokens) {
    if th.regs.is_empty() {
        ui.label(
            RichText::new("This hardware does not expose the registers of other threads.")
                .small()
                .color(t.text_weak),
        );
        return;
    }
    let changed = state.debugger.changed_registers();
    let n = th.regs.len();
    let gap = 6.0;
    // At most six to a row, so twelve registers (BCR) wrap to two rows.
    let per_row = n.min(6) as f32;
    let w = ((ui.available_width() - gap * (per_row - 1.0)) / per_row - 1.0).max(60.0);
    ui.horizontal_wrapped(|ui| {
        ui.spacing_mut().item_spacing = vec2(gap, gap);
        for (i, &v) in th.regs.iter().enumerate() {
            let name = reg_name(i);
            let hot = changed.get(i).copied().unwrap_or(false);
            let (rect, r) = ui.allocate_exact_size(vec2(w, 40.0), Sense::hover());
            let p = ui.painter();
            p.rect_filled(
                rect,
                CornerRadius::same(7),
                if hot { t.accent_soft } else { t.control },
            );
            if hot {
                p.rect_stroke(
                    rect,
                    CornerRadius::same(7),
                    Stroke::new(1.0, t.accent),
                    egui::StrokeKind::Inside,
                );
            }
            p.text(
                rect.left_top() + vec2(8.0, 5.0),
                Align2::LEFT_TOP,
                &name,
                FontId::new(10.5, semibold()),
                t.text_faint,
            );
            p.text(
                rect.left_bottom() + vec2(8.0, -5.0),
                Align2::LEFT_BOTTOM,
                v.to_string(),
                FontId::monospace(13.0),
                if hot { t.accent } else { t.text },
            );
            name_widget(&r, Role::Label, &format!("{name} {v}"));
            r.on_hover_text(format!(
                "{name} = {v} ({}){}",
                format::hex(v),
                if hot {
                    ", changed by the last step"
                } else {
                    ""
                }
            ));
        }
    });
}

fn heads(
    ui: &mut Ui,
    state: &AppState,
    th: &ThreadView,
    names: &[String],
    mem_len: usize,
    t: &Tokens,
) {
    let moved = state.debugger.moved_heads();
    ui.horizontal_wrapped(|ui| {
        ui.spacing_mut().item_spacing.x = 5.0;
        for (i, h) in th.heads.iter().enumerate() {
            let name = head_label(names, i);
            let text = if h.mem_space != 0 {
                format!("{name} {}:{}", h.mem_space, h.pos)
            } else {
                format!("{name} {}", h.pos)
            };
            let hot = moved.get(i).copied().unwrap_or(false);
            let r = chip(ui, &text, head_color(i), hot, t);
            name_widget(&r, Role::Label, &text);
            r.on_hover_text(format!(
                "{name} head at position {} of {}{}{}",
                h.pos,
                mem_len,
                if h.mem_space != 0 {
                    format!(" in memory space {}", h.mem_space)
                } else {
                    String::new()
                },
                if hot { "; moved by the last step" } else { "" }
            ));
        }
    });
}

fn stacks(ui: &mut Ui, state: &AppState, th: &ThreadView, t: &Tokens) {
    let changed = state.debugger.changed_stacks();
    for (s, stack) in th.stacks.iter().enumerate() {
        let depth = stack.iter().rposition(|&v| v != 0).map_or(0, |d| d + 1);
        // The heads CPU has a local and a global stack; TransSMT three
        // local ones (A to C) and a global one (D).
        let n = th.stacks.len();
        let name = match s {
            _ if n <= 2 && s == 0 => "Local stack".to_string(),
            _ if n <= 2 => "Global stack".to_string(),
            _ => format!(
                "Stack {} ({})",
                char::from(b'A' + (s % 26) as u8),
                if s + 1 == n { "global" } else { "local" }
            ),
        };
        let active = th.active_stack == s as i32;
        ui.horizontal_wrapped(|ui| {
            ui.spacing_mut().item_spacing.x = 6.0;
            ui.label(
                RichText::new(name)
                    .small()
                    .color(if active { t.text } else { t.text_weak }),
            );
            if active {
                ui.label(RichText::new("active").small().color(t.accent));
            }
            if depth == 0 {
                ui.label(RichText::new("empty").small().color(t.text_faint));
            }
            for (d, v) in stack.iter().take(depth).enumerate() {
                let hot = changed
                    .get(s)
                    .and_then(|c| c.get(d))
                    .copied()
                    .unwrap_or(false);
                let text = RichText::new(v.to_string()).monospace().size(12.0);
                let text = if hot {
                    text.color(t.accent).background_color(t.accent_soft)
                } else {
                    text.color(t.text_weak)
                };
                ui.label(text);
            }
        });
    }
    if !th.read_label.is_empty() || !th.next_label.is_empty() {
        ui.horizontal(|ui| {
            ui.label(RichText::new("Labels").small().color(t.text_weak));
            ui.label(
                RichText::new(format!(
                    "read {} · next {}",
                    or_dash(&th.read_label),
                    or_dash(&th.next_label)
                ))
                .monospace()
                .size(12.0),
            );
        });
    }
}

fn or_dash(s: &str) -> &str {
    if s.is_empty() {
        "–"
    } else {
        s
    }
}

/// The colour of head `head` (Okabe–Ito: orange, sky blue, bluish green,
/// reddish purple).
pub fn head_color(head: usize) -> Color32 {
    let c = match head {
        0 => avida_viewmodel::OKABE_ITO[0],
        1 => avida_viewmodel::OKABE_ITO[1],
        2 => avida_viewmodel::OKABE_ITO[2],
        _ => avida_viewmodel::OKABE_ITO[6],
    };
    Color32::from_rgb(c[0], c[1], c[2])
}

fn chip(ui: &mut Ui, text: &str, color: Color32, hot: bool, t: &Tokens) -> egui::Response {
    let font = FontId::monospace(11.5);
    let galley = ui.painter().layout_no_wrap(text.to_string(), font, t.text);
    let (rect, r) = ui.allocate_exact_size(galley.size() + vec2(20.0, 8.0), Sense::hover());
    let p = ui.painter();
    p.rect_filled(rect, CornerRadius::same(6), color.gamma_multiply(0.16));
    if hot {
        p.rect_stroke(
            rect,
            CornerRadius::same(6),
            Stroke::new(1.2, color),
            egui::StrokeKind::Inside,
        );
    }
    p.circle_filled(pos2(rect.left() + 8.0, rect.center().y), 3.0, color);
    p.galley(pos2(rect.left() + 14.0, rect.top() + 4.0), galley, t.text);
    r
}

/// Flag glyphs of one memory site, each in its own colour.
fn flag_glyphs(m: &MemCell, t: &Tokens) -> Vec<(&'static str, Color32, &'static str)> {
    let mut v = Vec::new();
    if m.flags & mem_flags::EXECUTED != 0 {
        v.push(("✓", t.success, "executed"));
    }
    if m.flags & mem_flags::COPIED != 0 {
        v.push(("©", t.info, "copied"));
    }
    if m.flags & (mem_flags::MUTATED | mem_flags::POINT_MUT | mem_flags::COPY_MUT) != 0 {
        v.push(("*", t.danger, "mutated"));
    }
    v
}

fn memory(ui: &mut Ui, state: &mut AppState, org: &OrganismView, t: &Tokens) {
    let cpu = &org.cpu;
    let space_id = state.debugger.listed_space();
    let Some(space) = cpu.space(space_id).or(cpu.spaces.first()) else {
        return;
    };
    let len = space.memory.len();
    let (thread, _) = state.debugger.threads();
    let heads: Vec<(usize, i32)> = thread
        .map(|th| {
            th.heads
                .iter()
                .enumerate()
                .filter(|(_, h)| h.mem_space == space_id)
                .map(|(i, h)| (i, h.pos))
                .collect()
        })
        .unwrap_or_default();
    // Heads of this thread that stand in another space.
    let elsewhere: Vec<String> = thread
        .map(|th| {
            th.heads
                .iter()
                .enumerate()
                .filter(|(_, h)| h.mem_space != space_id)
                .map(|(i, h)| {
                    format!(
                        "{} in space {} at {}",
                        head_label(&cpu.head_names, i),
                        h.mem_space,
                        h.pos
                    )
                })
                .collect()
        })
        .unwrap_or_default();
    let ip = state
        .debugger
        .ip_site()
        .filter(|&(s, _)| s == space_id)
        .map(|(_, p)| p);

    if cpu.spaces.len() > 1 {
        ui.horizontal_wrapped(|ui| {
            ui.spacing_mut().item_spacing.x = 4.0;
            widgets::caption(ui, "Memory space");
            for s in &cpu.spaces {
                let r = ui
                    .selectable_label(s.id == space_id, format!("{} {}", s.id, s.label))
                    .on_hover_text(format!(
                        "List {}, {}",
                        s.title().to_lowercase(),
                        format::plural(s.memory.len() as u64, "site")
                    ));
                name_widget(&r, Role::Tab, &format!("Memory space {} {}", s.id, s.label));
                if r.clicked() {
                    state.debugger.pin_space(Some(s.id));
                }
            }
            if state.debugger.pinned_space.is_some()
                && ui
                    .small_button("Show the IP's space")
                    .on_hover_text("Go back to listing the space the instruction pointer is in")
                    .clicked()
            {
                state.debugger.pin_space(None);
            }
        });
    }
    let heading = if cpu.spaces.len() > 1 {
        format!("{} · {}", space.title(), format::plural(len as u64, "site"))
    } else {
        format!("Memory · {}", format::plural(len as u64, "site"))
    };
    let legend = RichText::new(MEMORY_LEGEND).small().color(t.text_faint);
    let n = state.debugger.breakpoints.len();
    let clear = (n > 0).then(|| format!("Clear {}", format::plural(n as u64, "breakpoint")));
    // The legend and the button sit right of the heading when they fit
    // there, and on a line of their own below it when they do not (a long
    // gene name, a narrow panel).
    const GAP: f32 = 8.0;
    let right_width = widgets::text_width(ui, legend.clone())
        + clear.as_ref().map_or(0.0, |c| {
            GAP + widgets::text_width(ui, RichText::new(c).small())
                + 2.0 * ui.spacing().button_padding.x
        });
    let one_line = widgets::caption_width(ui, &heading) + GAP + right_width <= ui.available_width();
    let right_side = |ui: &mut Ui| {
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            ui.spacing_mut().item_spacing.x = GAP;
            if let Some(c) = &clear {
                if ui.small_button(c).clicked() {
                    state.debugger.breakpoints.clear();
                }
            }
            ui.label(legend);
        });
    };
    if one_line {
        ui.horizontal(|ui| {
            widgets::caption(ui, &heading);
            right_side(ui);
        });
    } else {
        widgets::caption(ui, &heading);
        ui.horizontal(right_side);
    }
    if !elsewhere.is_empty() {
        ui.label(
            RichText::new(format!("Elsewhere: {}.", elsewhere.join(", ")))
                .small()
                .color(t.text_weak),
        );
    }

    let mut area = egui::ScrollArea::vertical()
        .id_salt(("memory-listing", space_id))
        .auto_shrink([false, false]);
    if state.debugger.follow_ip {
        if let Some(ip) = ip.filter(|&p| p >= 0 && (p as usize) < len) {
            if state.debugger.scrolled_to != Some((space_id, ip)) {
                let (offset, h) = state.debugger.listing;
                if let Some(y) = scroll_target(ip as usize, ROW, offset, h) {
                    area = area.vertical_scroll_offset(y);
                }
                state.debugger.scrolled_to = Some((space_id, ip));
            }
        }
    }
    let frame = egui::Frame::new()
        .fill(t.canvas)
        .stroke(Stroke::new(1.0, t.border))
        .corner_radius(CornerRadius::same(8))
        .inner_margin(egui::Margin::symmetric(4, 4));
    let mut toggle: Option<i32> = None;
    let out = frame
        .show(ui, |ui| {
            ui.spacing_mut().item_spacing.y = 0.0;
            // The listing takes the height left (at least a few rows).
            let area = area.max_height(ui.available_height().max(ROW * 6.0));
            area.show_rows(ui, ROW, len, |ui, range| {
                for i in range {
                    let m = space.memory[i];
                    let here: Vec<usize> = heads
                        .iter()
                        .filter(|(_, p)| *p == i as i32)
                        .map(|(h, _)| *h)
                        .collect();
                    let r = memory_row(
                        ui,
                        state,
                        (space_id, cpu.cell_name(Some(&m))),
                        &m,
                        i,
                        &here,
                        &cpu.head_names,
                        ip,
                        t,
                    );
                    if r {
                        toggle = Some(i as i32);
                    }
                }
            })
        })
        .inner;
    state.debugger.listing = (out.state.clamped_offset().y, out.inner_rect.height());
    if let Some(pos) = toggle {
        state.debugger.toggle_breakpoint(space_id, pos);
    }
}

/// One line of the listing; returns whether its gutter was clicked.
#[allow(clippy::too_many_arguments)]
fn memory_row(
    ui: &mut Ui,
    state: &AppState,
    (space, name): (i32, &str),
    m: &MemCell,
    i: usize,
    heads_here: &[usize],
    head_names: &[String],
    ip: Option<i32>,
    t: &Tokens,
) -> bool {
    let (rect, row_r) = ui.allocate_exact_size(vec2(ui.available_width(), ROW), Sense::hover());
    let is_ip = ip == Some(i as i32);
    let bp = state.debugger.breakpoints.contains(&(space, i as i32));
    let changed = state.debugger.memory_changed(space, i);
    let p = ui.painter_at(rect);
    if is_ip {
        p.rect_filled(rect, CornerRadius::same(4), t.accent_soft);
    } else if changed {
        p.rect_filled(rect, CornerRadius::same(4), t.warning.gamma_multiply(0.14));
    }
    // Gutter: breakpoint toggle.
    let gutter = Rect::from_min_size(rect.min, vec2(46.0, ROW));
    let gutter_id = ui.scope_id().with(("bp", i));
    let g = ui.interact(gutter, gutter_id, Sense::click());
    name_widget(
        &g,
        Role::Button,
        &format!(
            "{} breakpoint at {}",
            if bp { "Remove" } else { "Set" },
            if space == 0 {
                i.to_string()
            } else {
                format!("{i} in space {space}")
            }
        ),
    );
    let dot = pos2(rect.left() + 8.0, rect.center().y);
    if bp {
        p.circle_filled(dot, 4.5, t.danger);
    } else if g.hovered() {
        p.circle_stroke(dot, 4.0, Stroke::new(1.2, t.danger.gamma_multiply(0.7)));
    }
    p.text(
        pos2(rect.left() + 40.0, rect.center().y),
        Align2::RIGHT_CENTER,
        i.to_string(),
        FontId::monospace(11.5),
        if is_ip { t.accent } else { t.text_faint },
    );
    // Head markers.
    let mut x = rect.left() + 50.0;
    if is_ip {
        p.text(
            pos2(x, rect.center().y),
            Align2::LEFT_CENTER,
            "▶",
            FontId::proportional(11.0),
            head_color(0),
        );
    }
    x += 12.0;
    // One dot per colour: the flow heads beyond the first share one (BCR
    // and GP8 have up to nine), and would run into the name.
    let mut drawn: Vec<Color32> = Vec::new();
    for &h in heads_here.iter().filter(|&&h| h != 0) {
        let c = head_color(h);
        if drawn.contains(&c) {
            continue;
        }
        drawn.push(c);
        p.circle_filled(pos2(x + 3.0, rect.center().y), 3.5, c);
        x += 9.0;
    }
    let name_x = rect.left() + 92.0;
    p.text(
        pos2(name_x, rect.center().y),
        Align2::LEFT_CENTER,
        name,
        FontId::monospace(12.0),
        inst_color(t, Some(name)),
    );
    let glyphs = flag_glyphs(m, t);
    let mut fx = rect.right() - 8.0;
    for (glyph, color, _) in glyphs.iter().rev() {
        let galley = p.layout_no_wrap((*glyph).to_string(), FontId::proportional(11.5), *color);
        fx -= galley.size().x + 3.0;
        p.galley(
            pos2(fx, rect.center().y - galley.size().y / 2.0),
            galley,
            *color,
        );
    }
    let mut tip = format!("Position {i}: {name} ({})", m.symbol());
    let flags: Vec<&str> = glyphs.iter().map(|(_, _, w)| *w).collect();
    if !flags.is_empty() {
        tip.push_str(&format!(" · {}", flags.join(", ")));
    }
    if !heads_here.is_empty() {
        let names: Vec<&str> = heads_here
            .iter()
            .map(|&h| head_label(head_names, h))
            .collect();
        tip.push_str(&format!(" · {} head", names.join(", ")));
    }
    if changed {
        tip.push_str(" · changed by the last step");
    }
    tip.push_str(if bp {
        "\nClick the number to remove the breakpoint."
    } else {
        "\nClick the number to stop here (breakpoint)."
    });
    row_r.on_hover_text(tip);
    g.clicked()
}
