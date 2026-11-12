//! What the debugger, genotype, configuration and palette actions do to
//! the [`App`].

use avida_session::{Command, LayerId};

use crate::actions::{palette_items, search, PaletteItem, PaletteState, DEBUG_STEP_MICRO_OPS};
use crate::app::{layer_title, App, GenoView, Request, Screen, ToastKind};
use crate::config_view::{merge_args, validate, Edit};

/// A memory site as shown to the user: `12`, or `2:12` outside space 0.
fn site_name(space: i32, pos: usize) -> String {
    if space == 0 {
        pos.to_string()
    } else {
        format!("{space}:{pos}")
    }
}

impl App {
    // --- debugger ---------------------------------------------------------

    /// Runs the watched organism for one instruction.
    pub fn debug_step(&mut self) {
        if self.refuse_if_over() {
            return;
        }
        let Some(cell) = self.selection else {
            self.toast(
                ToastKind::Info,
                "No organism selected: press Enter on an occupied cell of the map.".to_string(),
            );
            return;
        };
        match self.debugger.step_command(cell, 1) {
            Some(cmd) => self.send(Command::RunUntil(cmd)),
            None => self.toast(
                ToastKind::Info,
                "The watched organism is gone or not shown yet; select another one.".to_string(),
            ),
        }
    }

    /// Runs until the watched organism's IP stands on a breakpoint.
    pub fn debug_run_to_breakpoint(&mut self) {
        if self.refuse_if_over() {
            return;
        }
        let Some(cell) = self.selection else {
            self.toast(
                ToastKind::Info,
                "No organism selected: press Enter on an occupied cell of the map.".to_string(),
            );
            return;
        };
        if self.debugger.breakpoints.is_empty() {
            self.toast(
                ToastKind::Info,
                "Set a breakpoint first: select a line and press b.".to_string(),
            );
            return;
        }
        match self.debugger.breakpoint_command(cell) {
            Some(cmd) => self.send(Command::RunUntil(cmd)),
            None => self.toast(
                ToastKind::Info,
                "The watched organism is gone or not shown yet; select another one.".to_string(),
            ),
        }
    }

    pub fn debug_toggle_breakpoint(&mut self) {
        match self.debugger.toggle_breakpoint() {
            Some(true) => {
                let (s, pos) = (self.debugger.space, self.debugger.sel);
                self.toast(
                    ToastKind::Info,
                    format!("Breakpoint set at {}.", site_name(s, pos)),
                );
            }
            Some(false) => {
                let (s, pos) = (self.debugger.space, self.debugger.sel);
                self.toast(
                    ToastKind::Info,
                    format!("Breakpoint at {} cleared.", site_name(s, pos)),
                );
            }
            None => self.toast(ToastKind::Info, "No memory to set a breakpoint in.".into()),
        }
    }

    pub fn debug_cycle_space(&mut self) {
        match self.debugger.cycle_space() {
            Some(_) => {
                let text = self
                    .debugger
                    .space_view()
                    .map(|s| format!("Listing {}.", s.title().to_lowercase()))
                    .unwrap_or_default();
                self.toast(ToastKind::Info, text);
            }
            None => self.toast(
                ToastKind::Info,
                "This organism has only one memory space.".to_string(),
            ),
        }
    }

    pub fn debug_clear_breakpoints(&mut self) {
        let n = self.debugger.breakpoints.len();
        self.debugger.breakpoints.clear();
        self.toast(
            ToastKind::Info,
            if n == 0 {
                "There are no breakpoints.".to_string()
            } else {
                format!("Cleared {n} breakpoint{}.", if n == 1 { "" } else { "s" })
            },
        );
    }

    pub fn debug_toggle_follow_offspring(&mut self) {
        let on = !self.debugger.follow_offspring;
        self.debugger.follow_offspring = on;
        self.send(Command::FollowOffspring(on));
        self.toast(
            ToastKind::Info,
            if on {
                "Following offspring: the watch moves to each child.".to_string()
            } else {
                "Not following offspring.".to_string()
            },
        );
    }

    pub fn cycle_thread(&mut self) {
        if self.screen == Screen::Debug {
            let n = self
                .debugger
                .current
                .as_ref()
                .map_or(0, |o| o.cpu.threads.len());
            if n > 0 {
                self.debugger.thread = (self.debugger.thread + 1) % n;
                self.debugger.thread_changed();
            }
            return;
        }
        let n = self
            .frame
            .as_ref()
            .and_then(|f| f.inspect.as_ref())
            .map_or(0, |o| o.cpu.threads.len());
        if n > 0 {
            self.zoom_thread = (self.zoom_thread + 1) % n;
        }
    }

    // --- genotypes --------------------------------------------------------

    pub fn geno_cycle_view(&mut self) {
        self.geno_view = self.geno_view.next();
    }

    /// Traces the ancestry of the selected genotype and shows it.
    pub fn geno_trace_lineage(&mut self) {
        let id = self
            .frame
            .as_ref()
            .and_then(|f| f.genotypes.get(self.geno_sel))
            .map(|g| g.id);
        let Some(id) = id else {
            self.toast(ToastKind::Info, "No genotype selected.".to_string());
            return;
        };
        if self.highlight != Some(id) {
            self.highlight = Some(id);
            self.subscribe();
        }
        self.geno_view = GenoView::Lineage;
        // Clamped to the newest genotype when drawn.
        self.lineage_sel = usize::MAX / 2;
    }

    /// The genotypes of the traced lineage, founder first.
    pub fn lineage_rows(&self) -> Vec<&avida_viewmodel::GenotypeRow> {
        self.frame
            .as_ref()
            .map(|f| f.lineage.iter().rev().collect())
            .unwrap_or_default()
    }

    // --- configuration ----------------------------------------------------

    /// The setting under the config screen's selection.
    fn selected_setting(&self) -> Option<&avida_session::Setting> {
        let setup = self.setup.as_ref()?;
        let vis = self.config.visible(setup);
        let i = *vis.get(self.config.sel.min(vis.len().saturating_sub(1)))?;
        setup.settings.get(i)
    }

    pub fn config_filter(&mut self) {
        self.config.filtering = true;
    }

    pub fn config_toggle_changed_only(&mut self) {
        self.config.changed_only = !self.config.changed_only;
        self.config.sel = 0;
    }

    pub fn config_edit(&mut self) {
        let Some(s) = self.selected_setting() else {
            self.toast(ToastKind::Info, "No setting selected.".to_string());
            return;
        };
        let name = s.name;
        let buffer = self
            .config
            .override_of(name)
            .map_or_else(|| s.value.clone(), str::to_string);
        self.config.edit = Some(Edit {
            name,
            buffer,
            error: None,
        });
    }

    pub fn config_revert(&mut self) {
        let Some(name) = self.selected_setting().map(|s| s.name) else {
            return;
        };
        if self.config.clear_override(name) {
            self.toast(ToastKind::Info, format!("Override of {name} dropped."));
        } else {
            self.toast(ToastKind::Info, format!("{name} has no override."));
        }
    }

    /// Checks and keeps the value typed for the setting being edited.
    pub fn config_commit_edit(&mut self) {
        let Some(edit) = self.config.edit.clone() else {
            return;
        };
        let Some(setting) = self
            .setup
            .as_ref()
            .and_then(|s| s.setting(edit.name))
            .cloned()
        else {
            self.config.edit = None;
            return;
        };
        match validate(setting.kind, &edit.buffer) {
            Ok(v) => {
                self.config.set_override(&setting, v);
                self.config.edit = None;
            }
            Err(e) => {
                if let Some(ed) = self.config.edit.as_mut() {
                    ed.error = Some(e);
                }
            }
        }
    }

    /// The `avida` arguments a restart would use.
    pub fn restart_args(&self) -> Vec<String> {
        merge_args(&self.args, &self.config.overrides)
    }

    /// Asks for confirmation before discarding the run.
    pub fn ask_restart(&mut self) {
        self.confirm_restart = true;
    }

    /// Queues the restart for the shell.
    pub fn confirm_restart_now(&mut self) {
        self.confirm_restart = false;
        let args = self.restart_args();
        self.request(Request::Restart(args));
    }

    // --- palette ----------------------------------------------------------

    /// The palette's rows that make sense now.
    pub fn palette_entries(&self) -> Vec<PaletteItem> {
        let env = self.env();
        let layers: Vec<(LayerId, String)> = self
            .layers()
            .into_iter()
            .map(|l| (l, layer_title(env, l)))
            .collect();
        let alive = self.debugger.is_alive();
        let micro_ops = self.steps_run_micro_ops();
        palette_items(&layers)
            .into_iter()
            .map(|mut it| {
                if micro_ops && it.action == crate::actions::Action::DebugStep {
                    it.title = DEBUG_STEP_MICRO_OPS.to_string();
                }
                it
            })
            .filter(|it| {
                use crate::actions::Action::*;
                match it.action {
                    DebugStep | DebugRun => alive,
                    DebugClearBreakpoints => !self.debugger.breakpoints.is_empty(),
                    _ => true,
                }
            })
            .collect()
    }

    /// The palette's rows for its query, best first.
    pub fn palette_hits(&self, p: &PaletteState) -> Vec<PaletteItem> {
        let items = self.palette_entries();
        search(&items, &p.query)
            .into_iter()
            .map(|i| items[i].clone())
            .collect()
    }
}
