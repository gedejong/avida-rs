//! Key and mouse handling.
//!
//! Keys are resolved through the binding table in [`crate::actions`], the
//! same table the command palette and the help overlay print, so what a
//! key does and what the help says it does cannot differ. A capital letter
//! opens a screen or does an action, as in the ncurses viewer (which
//! ignored case); where a lowercase letter is also a movement or tweak
//! (`h j k l p`) the lowercase one does that and the capital keeps the
//! viewer's meaning (`H` histogram, `P` pause). Rows scoped to a screen
//! (the debugger's `i b g f`, the configuration's `e /`) win over global
//! rows on that screen.

use ratatui::crossterm::event::{
    KeyCode, KeyEvent, KeyEventKind, KeyModifiers, MouseButton, MouseEvent, MouseEventKind,
};

use crate::actions::{lookup, Action, Key, PaletteState};
use crate::app::{App, EnvTab, GenoView, Request, Screen, ToastKind};
use crate::metrics::Metric;

/// Records the playback keys `{` and `}` move by.
pub const SEEK_JUMP: i64 = 10;

/// Why an action cannot work on a recording, if it cannot.
fn playback_refusal(action: Action) -> Option<&'static str> {
    Some(match action {
        Action::SavePopulation => "there is no population to save",
        Action::ExtractGenome => "organisms are not recorded",
        Action::Restart => "there is no simulation to restart",
        Action::DebugStep | Action::DebugRun | Action::DebugFollowOffspring => {
            "there is no simulation to debug"
        }
        Action::GenoLineage => "lineages are not recorded",
        Action::ConfigEdit | Action::ConfigRevert => "settings cannot change",
        _ => return None,
    })
}

impl App {
    pub fn on_key(&mut self, key: KeyEvent) {
        if key.kind == KeyEventKind::Release {
            return;
        }
        let Some(k) = Key::from_event(&key) else {
            return;
        };
        if k == Key::Ctrl('c') {
            self.quit = true;
            return;
        }
        if self.confirm_quit {
            self.confirm_quit = false;
            if matches!(
                key.code,
                KeyCode::Char('y' | 'Y' | 'q' | 'Q') | KeyCode::Enter
            ) {
                self.quit = true;
            }
            return;
        }
        if self.confirm_restart {
            self.confirm_restart = false;
            if matches!(key.code, KeyCode::Char('y' | 'Y') | KeyCode::Enter) {
                self.confirm_restart_now();
            }
            return;
        }
        if self.cmd_palette.is_some() {
            self.on_palette_key(key, k);
            return;
        }
        if self.config.edit.is_some() || self.config.filtering {
            self.on_text_key(key, k);
            return;
        }
        if self.show_help {
            self.on_help_key(k);
            return;
        }
        let shift = key.modifiers.contains(KeyModifiers::SHIFT);
        if let Some(action) = lookup(self.screen, k) {
            self.perform(action, shift);
        }
    }

    /// Carries out an action, whether it came from a key, the palette or a
    /// test.
    pub fn perform(&mut self, action: Action, shift: bool) {
        if self.playback.is_some() {
            if let Some(why) = playback_refusal(action) {
                self.toast(ToastKind::Info, format!("Not in a playback: {why}."));
                return;
            }
        }
        match action {
            Action::RunPause => self.toggle_run(),
            Action::StepUpdate => self.step_update(),
            Action::StepSlice => self.step_slice(),
            Action::Faster => self.faster(),
            Action::Slower => self.slower(),
            Action::SavePopulation => self.save_population(),
            Action::ExtractGenome => self.extract_organism(),
            Action::Restart => self.ask_restart(),
            Action::Goto(s) => self.goto(s),
            Action::NextScreen => self.next_screen(1),
            Action::PrevScreen => self.next_screen(-1),
            Action::Back => {
                if self.screen != Screen::Map {
                    self.goto(Screen::Map);
                } else {
                    self.select(None);
                }
            }
            Action::ChooseNext => self.choose_next_organism(),
            Action::CyclePalette => self.cycle_palette(),
            Action::ToggleScale => self.toggle_scale(),
            Action::BigCells => self.big_cells = !self.big_cells,
            Action::PrevLayer => self.cycle_layer(-1),
            Action::NextLayer => self.cycle_layer(1),
            Action::Layer(l) => self.set_layer(l),
            Action::Dir(dx, dy) => self.on_dir(dx, dy, shift),
            Action::PageUp => self.on_page(-1),
            Action::PageDown => self.on_page(1),
            Action::Home => self.on_home_end(true),
            Action::End => self.on_home_end(false),
            Action::Enter => self.on_enter(),
            Action::CycleThread => self.cycle_thread(),
            Action::DebugStep => self.debug_step(),
            Action::DebugRun => self.debug_run_to_breakpoint(),
            Action::DebugBreakpoint => self.debug_toggle_breakpoint(),
            Action::DebugClearBreakpoints => self.debug_clear_breakpoints(),
            Action::DebugFollowOffspring => self.debug_toggle_follow_offspring(),
            Action::DebugFollowIp => self.debugger.follow(),
            Action::DebugCycleSpace => self.debug_cycle_space(),
            Action::ConfigFilter => self.config_filter(),
            Action::ConfigEdit => self.config_edit(),
            Action::ConfigChangedOnly => self.config_toggle_changed_only(),
            Action::ConfigRevert => self.config_revert(),
            Action::LogProblemsOnly => {
                self.log_problems_only = !self.log_problems_only;
                self.log_follow = true;
            }
            Action::GenoView => self.geno_cycle_view(),
            Action::GenoLineage => self.geno_trace_lineage(),
            Action::Analyze => self.toast(
                ToastKind::Info,
                "Analyze mode is not part of the live viewer; run avida with an analyze file."
                    .to_string(),
            ),
            Action::OpenPalette => self.cmd_palette = Some(PaletteState::default()),
            Action::Help => {
                self.show_help = true;
                self.help_scroll = 0;
            }
            Action::Quit => self.confirm_quit = true,
            Action::QuitNow => self.quit = true,
            Action::Redraw => self.request(Request::Redraw),
            Action::SeekBack => self.seek_by(Some(-SEEK_JUMP), false),
            Action::SeekForward => self.seek_by(Some(SEEK_JUMP), false),
            Action::SeekFirst => self.seek_by(None, false),
            Action::SeekLast => self.seek_by(None, true),
        }
    }

    fn on_help_key(&mut self, k: Key) {
        match k {
            Key::Code(KeyCode::Up) | Key::Char('k') => {
                self.help_scroll = self.help_scroll.saturating_sub(1);
            }
            Key::Code(KeyCode::Down) | Key::Char('j') => self.help_scroll += 1,
            Key::Code(KeyCode::PageUp) => self.help_scroll = self.help_scroll.saturating_sub(10),
            Key::Code(KeyCode::PageDown) => self.help_scroll += 10,
            Key::Code(KeyCode::Home) => self.help_scroll = 0,
            Key::Code(KeyCode::End) => self.help_scroll = usize::MAX / 2,
            Key::Code(KeyCode::Esc | KeyCode::Enter | KeyCode::F(1))
            | Key::Char('?' | 'q' | 'Q' | ' ') => self.show_help = false,
            _ => {}
        }
    }

    /// Keys while the command palette is open.
    fn on_palette_key(&mut self, key: KeyEvent, k: Key) {
        let Some(mut p) = self.cmd_palette.take() else {
            return;
        };
        let hits = self.palette_hits(&p);
        let n = hits.len();
        match k {
            Key::Code(KeyCode::Esc) | Key::Ctrl('p') => return,
            Key::Code(KeyCode::Enter) => {
                if let Some(item) = hits.get(p.selected.min(n.saturating_sub(1))) {
                    let action = item.action;
                    self.perform(action, false);
                }
                return;
            }
            Key::Code(KeyCode::Up) | Key::Ctrl('k') => p.selected = p.selected.saturating_sub(1),
            Key::Code(KeyCode::Down) | Key::Ctrl('j') | Key::Code(KeyCode::Tab) => {
                p.selected = (p.selected + 1).min(n.saturating_sub(1));
            }
            Key::Code(KeyCode::BackTab) => p.selected = p.selected.saturating_sub(1),
            Key::Code(KeyCode::PageUp) => p.selected = p.selected.saturating_sub(8),
            Key::Code(KeyCode::PageDown) => p.selected = (p.selected + 8).min(n.saturating_sub(1)),
            Key::Code(KeyCode::Backspace) => {
                p.query.pop();
                p.selected = 0;
            }
            Key::Ctrl('u') => {
                p.query.clear();
                p.selected = 0;
            }
            Key::Char(c) if !key.modifiers.contains(KeyModifiers::CONTROL) => {
                p.query.push(c);
                p.selected = 0;
            }
            _ => {}
        }
        self.cmd_palette = Some(p);
    }

    /// Keys while the configuration screen is taking text (a filter query
    /// or a value).
    fn on_text_key(&mut self, key: KeyEvent, k: Key) {
        if let Some(edit) = self.config.edit.as_mut() {
            match k {
                Key::Code(KeyCode::Esc) => self.config.edit = None,
                Key::Code(KeyCode::Enter) => self.config_commit_edit(),
                Key::Code(KeyCode::Backspace) => {
                    edit.buffer.pop();
                    edit.error = None;
                }
                Key::Ctrl('u') => {
                    edit.buffer.clear();
                    edit.error = None;
                }
                Key::Char(c) if !key.modifiers.contains(KeyModifiers::CONTROL) => {
                    edit.buffer.push(c);
                    edit.error = None;
                }
                _ => {}
            }
            return;
        }
        match k {
            Key::Code(KeyCode::Esc) => {
                self.config.query.clear();
                self.config.filtering = false;
                self.config.sel = 0;
            }
            Key::Code(KeyCode::Enter) => self.config.filtering = false,
            Key::Code(KeyCode::Backspace) => {
                self.config.query.pop();
                self.config.sel = 0;
            }
            Key::Ctrl('u') => {
                self.config.query.clear();
                self.config.sel = 0;
            }
            Key::Code(KeyCode::Up) => self.config_move(-1),
            Key::Code(KeyCode::Down) => self.config_move(1),
            Key::Char(c) if !key.modifiers.contains(KeyModifiers::CONTROL) => {
                self.config.query.push(c);
                self.config.sel = 0;
            }
            _ => {}
        }
    }

    fn config_move(&mut self, d: i64) {
        let n = self
            .setup
            .as_ref()
            .map_or(0, |s| self.config.visible(s).len());
        self.config.move_sel(d, n);
    }

    fn on_enter(&mut self) {
        match self.screen {
            Screen::Map | Screen::Histogram | Screen::Stats | Screen::Options => {
                self.select_cursor();
                self.goto(Screen::Zoom);
            }
            Screen::Charts => self.toggle_chart_metric(),
            Screen::Genotypes => {
                let id = if self.geno_view == GenoView::Lineage {
                    let rows = self.lineage_rows();
                    let i = self.lineage_sel.min(rows.len().saturating_sub(1));
                    rows.get(i).map(|g| g.id)
                } else {
                    self.frame
                        .as_ref()
                        .and_then(|f| f.genotypes.get(self.geno_sel))
                        .map(|g| g.id)
                };
                if let Some(id) = id {
                    if self.jump_to_genotype(id) {
                        self.goto(Screen::Map);
                    } else {
                        self.toast(
                            ToastKind::Info,
                            "No living cell holds that genotype.".into(),
                        );
                    }
                }
            }
            _ => {}
        }
    }

    fn toggle_chart_metric(&mut self) {
        let m = Metric::ALL[self.chart_focus.min(Metric::ALL.len() - 1)];
        if let Some(i) = self.chart_metrics.iter().position(|&x| x == m) {
            if self.chart_metrics.len() > 1 {
                self.chart_metrics.remove(i);
            }
        } else {
            self.chart_metrics.push(m);
        }
    }

    fn on_dir(&mut self, dx: i32, dy: i32, big: bool) {
        let k = if big { 5 } else { 1 };
        match self.screen {
            Screen::Map | Screen::Zoom | Screen::Histogram | Screen::Options | Screen::Blank => {
                self.move_cursor(dx * k, dy * k);
            }
            Screen::Debug => self
                .debugger
                .move_sel(i64::from(dy * if big { 8 } else { 1 })),
            Screen::Config => self.config_move(i64::from(dy)),
            Screen::Stats => {
                if dx != 0 {
                    let n = self.env().map_or(0, |e| e.tasks.len());
                    let off = self.stats_task_offset as i64 + i64::from(dx) * 5;
                    self.stats_task_offset = off.clamp(0, n.saturating_sub(1) as i64) as usize;
                }
            }
            Screen::Charts => {
                if dy != 0 {
                    let n = Metric::ALL.len() as i64;
                    self.chart_focus =
                        (self.chart_focus as i64 + i64::from(dy)).clamp(0, n - 1) as usize;
                }
            }
            Screen::Genotypes if self.geno_view == GenoView::Lineage => {
                let n = self.lineage_rows().len();
                self.lineage_sel = step(self.lineage_sel.min(n.saturating_sub(1)), dy, n);
            }
            Screen::Genotypes => {
                let n = self.frame.as_ref().map_or(0, |f| f.genotypes.len());
                self.geno_sel = step(self.geno_sel, dy, n);
            }
            Screen::Environment => {
                if dx != 0 {
                    let tabs = EnvTab::ALL;
                    let i = tabs.iter().position(|&t| t == self.env_tab).unwrap_or(0) as i32;
                    self.env_tab = tabs[(i + dx).rem_euclid(3) as usize];
                    self.env_scroll = 0;
                }
                self.env_scroll = step_unbounded(self.env_scroll, dy);
            }
            Screen::Log => self.log_scroll_by(dy),
        }
    }

    /// Scrolls the log by `delta` lines from where it was drawn. Reaching
    /// the bottom resumes following.
    pub(crate) fn log_scroll_by(&mut self, delta: i32) {
        let max = self.log_shown.saturating_sub(self.log_view_h);
        let from = if self.log_follow {
            max
        } else {
            self.log_scroll.min(max)
        };
        let to = (i64::try_from(from).unwrap_or(0) + i64::from(delta)).clamp(0, max as i64);
        self.log_scroll = to as usize;
        self.log_follow = self.log_scroll >= max;
    }

    fn on_page(&mut self, dir: i32) {
        const PAGE: i32 = 10;
        match self.screen {
            Screen::Debug => self.debugger.move_sel(i64::from(dir * PAGE)),
            Screen::Config => self.config_move(i64::from(dir * PAGE)),
            Screen::Genotypes if self.geno_view == GenoView::Lineage => {
                let n = self.lineage_rows().len();
                self.lineage_sel = step(self.lineage_sel.min(n.saturating_sub(1)), dir * PAGE, n);
            }
            Screen::Genotypes => {
                let n = self.frame.as_ref().map_or(0, |f| f.genotypes.len());
                self.geno_sel = step(self.geno_sel, dir * PAGE, n);
            }
            Screen::Log => {
                let page = i32::try_from(self.log_view_h.saturating_sub(1).max(1)).unwrap_or(PAGE);
                self.log_scroll_by(dir * page);
            }
            Screen::Environment => self.env_scroll = step_unbounded(self.env_scroll, dir * PAGE),
            Screen::Zoom => {
                self.zoom_follow_ip = false;
                self.zoom_scroll = step_unbounded(self.zoom_scroll, dir * PAGE);
            }
            _ => {}
        }
    }

    fn on_home_end(&mut self, home: bool) {
        match self.screen {
            Screen::Debug => {
                if home {
                    self.debugger.follow();
                } else {
                    self.debugger.set_sel(usize::MAX / 2);
                }
            }
            Screen::Config => {
                let n = self
                    .setup
                    .as_ref()
                    .map_or(0, |s| self.config.visible(s).len());
                self.config.sel = if home { 0 } else { n.saturating_sub(1) };
            }
            Screen::Genotypes if self.geno_view == GenoView::Lineage => {
                let n = self.lineage_rows().len();
                self.lineage_sel = if home { 0 } else { n.saturating_sub(1) };
            }
            Screen::Genotypes => {
                let n = self.frame.as_ref().map_or(0, |f| f.genotypes.len());
                self.geno_sel = if home { 0 } else { n.saturating_sub(1) };
            }
            Screen::Log => {
                if home {
                    self.log_scroll = 0;
                    self.log_follow = self.log_shown <= self.log_view_h;
                } else {
                    self.log_follow = true;
                }
            }
            Screen::Zoom => {
                self.zoom_follow_ip = home;
                if !home {
                    self.zoom_scroll = usize::MAX / 2;
                }
            }
            Screen::Environment => self.env_scroll = if home { 0 } else { usize::MAX / 2 },
            _ => {}
        }
    }

    pub fn on_mouse(&mut self, ev: MouseEvent) {
        if self.show_help
            || self.confirm_quit
            || self.confirm_restart
            || self.cmd_palette.is_some()
            || self.config.edit.is_some()
        {
            return;
        }
        match ev.kind {
            MouseEventKind::Down(MouseButton::Left) => {
                if let Some(&(_, s)) = self
                    .tab_hits
                    .iter()
                    .find(|(r, _)| ev.row == r.y && ev.column >= r.x && ev.column < r.x + r.width)
                {
                    self.goto(s);
                    return;
                }
                if self.screen != Screen::Map {
                    return;
                }
                let hit = self.geom.and_then(|g| g.cell_at(ev.column, ev.row));
                if let Some((x, y)) = hit {
                    self.cursor = (x, y);
                    self.select_cursor();
                }
            }
            MouseEventKind::ScrollUp => self.on_wheel(-3),
            MouseEventKind::ScrollDown => self.on_wheel(3),
            _ => {}
        }
    }

    fn on_wheel(&mut self, d: i32) {
        match self.screen {
            Screen::Log => self.on_dir(0, d, false),
            Screen::Genotypes | Screen::Environment | Screen::Debug | Screen::Config => {
                self.on_dir(0, d, false)
            }
            Screen::Zoom => {
                self.zoom_follow_ip = false;
                self.zoom_scroll = step_unbounded(self.zoom_scroll, d);
            }
            _ => {}
        }
    }
}

fn step(i: usize, d: i32, n: usize) -> usize {
    if n == 0 {
        return 0;
    }
    (i as i64 + i64::from(d)).clamp(0, n as i64 - 1) as usize
}

fn step_unbounded(i: usize, d: i32) -> usize {
    (i as i64 + i64::from(d)).clamp(0, i64::MAX / 4) as usize
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::color::ColorMode;
    use avida_session::{Command, LayerId};
    use ratatui::crossterm::event::{KeyEventState, MouseEvent};

    fn app() -> App {
        let mut a = App::new(".".into(), vec![], ColorMode::TrueColor);
        a.take_commands();
        a
    }

    fn key(c: KeyCode) -> KeyEvent {
        KeyEvent {
            code: c,
            modifiers: KeyModifiers::NONE,
            kind: KeyEventKind::Press,
            state: KeyEventState::NONE,
        }
    }

    fn press(a: &mut App, s: &str) {
        for c in s.chars() {
            a.on_key(key(KeyCode::Char(c)));
        }
    }

    #[test]
    fn run_control_keys_queue_commands() {
        let mut a = app();
        press(&mut a, " ");
        assert_eq!(a.take_commands(), vec![Command::Run]);
        press(&mut a, ".,");
        assert_eq!(
            a.take_commands(),
            vec![Command::StepUpdate, Command::StepSlice]
        );
        press(&mut a, "N");
        assert_eq!(a.take_commands(), vec![Command::StepUpdate]);
        press(&mut a, "w");
        assert_eq!(a.take_commands(), vec![Command::SavePopulation]);
    }

    #[test]
    fn speed_ladder_walks_both_ways() {
        let mut a = app();
        press(&mut a, "-");
        assert_eq!(a.speed_label(), "1000/s");
        press(&mut a, "----------");
        assert_eq!(a.speed_label(), "1/s");
        press(&mut a, "+");
        assert_eq!(a.speed_label(), "2/s");
        for _ in 0..20 {
            press(&mut a, "+");
        }
        assert_eq!(a.speed_label(), "max");
        let cmds = a.take_commands();
        assert!(cmds.iter().all(|c| matches!(c, Command::SetSpeed(_))));
        assert_eq!(
            cmds.last(),
            Some(&Command::SetSpeed(avida_session::Speed::Max))
        );
    }

    #[test]
    fn screens_and_case_rules() {
        let mut a = app();
        for (k, s) in [
            ('S', Screen::Stats),
            ('G', Screen::Charts),
            ('T', Screen::Genotypes),
            ('Z', Screen::Zoom),
            ('E', Screen::Environment),
            ('H', Screen::Histogram),
            ('D', Screen::Log),
            ('O', Screen::Options),
            ('B', Screen::Blank),
            ('M', Screen::Map),
            ('s', Screen::Stats),
            ('m', Screen::Map),
        ] {
            press(&mut a, &k.to_string());
            assert_eq!(a.screen, s, "key {k}");
        }
        // lowercase h is a cursor move, not the histogram.
        press(&mut a, "h");
        assert_eq!(a.screen, Screen::Map);
        a.take_commands();
    }

    #[test]
    fn tab_cycles_and_escape_returns() {
        let mut a = app();
        a.on_key(key(KeyCode::Tab));
        assert_eq!(a.screen, Screen::Stats);
        a.on_key(key(KeyCode::BackTab));
        a.on_key(key(KeyCode::BackTab));
        assert_eq!(a.screen, Screen::Options);
        a.on_key(key(KeyCode::Esc));
        assert_eq!(a.screen, Screen::Map);
    }

    #[test]
    fn quit_needs_confirmation_but_capital_does_not() {
        let mut a = app();
        press(&mut a, "q");
        assert!(a.confirm_quit && !a.quit);
        press(&mut a, "n");
        assert!(!a.confirm_quit && !a.quit);
        press(&mut a, "qy");
        assert!(a.quit);
        let mut b = app();
        press(&mut b, "Q");
        assert!(b.quit);
        let mut c = app();
        c.on_key(KeyEvent {
            modifiers: KeyModifiers::CONTROL,
            ..key(KeyCode::Char('c'))
        });
        assert!(c.quit);
    }

    #[test]
    fn help_overlay_swallows_keys() {
        let mut a = app();
        press(&mut a, "?");
        assert!(a.show_help);
        press(&mut a, "S");
        assert_eq!(a.screen, Screen::Map);
        press(&mut a, "?");
        assert!(!a.show_help);
    }

    #[test]
    fn layer_and_scale_keys() {
        let mut a = app();
        press(&mut a, "]");
        assert_eq!(a.layer, LayerId::Fitness);
        assert!(a
            .take_commands()
            .iter()
            .any(|c| matches!(c, Command::Subscribe(s) if s.layers == vec![LayerId::Fitness])));
        assert_eq!(a.scale(), avida_viewmodel::Scale::Log);
        press(&mut a, "L");
        assert_eq!(a.scale(), avida_viewmodel::Scale::Linear);
        press(&mut a, "[[");
        assert_eq!(a.layer, *LayerId::FIXED.last().unwrap());
        press(&mut a, "p");
        assert_eq!(a.palette, avida_viewmodel::Palette::Cividis);
        // Genotype layer has no scale.
        a.set_layer(LayerId::Genotype);
        press(&mut a, "L");
        assert_eq!(a.toasts.len(), 1);
    }

    #[test]
    fn mouse_wheel_and_click_outside_map_do_nothing_harmful() {
        let mut a = app();
        a.on_mouse(MouseEvent {
            kind: MouseEventKind::Down(MouseButton::Left),
            column: 3,
            row: 3,
            modifiers: KeyModifiers::NONE,
        });
        assert_eq!(a.selection, None);
    }

    // --- debugger, palette, help, config, genotypes ----------------------

    use crate::app::{GenoView, Request};
    use crate::config_view::tests::{setting, setup};
    use crate::fixtures::{
        app_with, frame_with_map, frame_with_org, genotype, organism, status as fx_status,
    };
    use avida_session::{RunState, RunUntil, StopWhen};
    use std::sync::Arc;

    fn debugging() -> App {
        let mut a = app_with(frame_with_org(organism(5, 1, 2, 8)));
        a.goto(Screen::Debug);
        a.take_commands();
        a
    }

    #[test]
    fn the_debugger_steps_sets_breakpoints_and_runs_to_them() {
        let mut a = debugging();
        assert_eq!(a.debugger.sel, 2, "the selection starts on the IP");
        press(&mut a, "i");
        assert_eq!(
            a.take_commands(),
            vec![Command::RunUntil(RunUntil {
                cell: 3,
                org_id: 5,
                stop: StopWhen::Cycles(1),
                max_slices: crate::debugger::STEP_BUDGET,
            })]
        );
        // `g` without a breakpoint says so instead of sending anything.
        press(&mut a, "g");
        assert!(a.take_commands().is_empty());
        assert!(a.toasts.last().unwrap().text.contains("breakpoint"));
        // Move down two lines, toggle a breakpoint there, run to it.
        a.on_key(key(KeyCode::Down));
        a.on_key(key(KeyCode::Down));
        press(&mut a, "b");
        assert_eq!(
            a.debugger.breakpoints.iter().copied().collect::<Vec<_>>(),
            vec![(0, 4)]
        );
        press(&mut a, "g");
        match a.take_commands().as_slice() {
            [Command::RunUntil(r)] => assert_eq!(r.stop, StopWhen::IpIn(vec![(0, 4)])),
            other => panic!("{other:?}"),
        }
        // Enter toggles it off again; Delete clears them all.
        a.on_key(key(KeyCode::Enter));
        assert!(a.debugger.breakpoints.is_empty());
        a.on_key(key(KeyCode::Enter));
        a.on_key(key(KeyCode::Delete));
        assert!(a.debugger.breakpoints.is_empty());
    }

    #[test]
    fn follow_offspring_and_the_watch_moving() {
        let mut a = debugging();
        press(&mut a, "f");
        assert_eq!(a.take_commands(), vec![Command::FollowOffspring(true)]);
        assert!(a.debugger.follow_offspring);
        a.apply_events(vec![avida_session::SessionEvent::WatchMoved {
            from: 3,
            to: 0,
            org_id: 9,
            offspring: true,
        }]);
        assert_eq!(a.selection, Some(0));
        assert_eq!(a.debugger.org_id, Some(9));
        assert!(a.debugger.follow_offspring, "the setting survives the move");
        assert!(a.take_commands().is_empty(), "the session moved it already");
        // Selecting another cell by hand starts a new debugging session
        // but the session still follows offspring.
        a.select(Some(1));
        assert!(a.debugger.follow_offspring);
        assert!(a.debugger.breakpoints.is_empty());
    }

    #[test]
    fn stepping_a_finished_run_is_refused() {
        let mut a = debugging();
        a.apply_status(fx_status(RunState::Finished, 12));
        press(&mut a, "i");
        assert!(a.take_commands().is_empty());
        assert!(a.toasts.last().unwrap().text.contains("finished"));
    }

    #[test]
    fn debugger_keys_do_not_leak_to_other_screens() {
        let mut a = app();
        press(&mut a, "i");
        assert!(a.big_cells, "i is the wide-cell toggle on the map");
        a.goto(Screen::Genotypes);
        press(&mut a, "l");
        assert!(a.highlight.is_none() && a.geno_view == GenoView::Table);
    }

    #[test]
    fn the_palette_opens_filters_and_performs() {
        let mut a = app();
        press(&mut a, ":");
        assert!(a.cmd_palette.is_some());
        // Letters go to the query, not to the screens they would open.
        press(&mut a, "slice");
        assert_eq!(a.screen, Screen::Map);
        let hits = a.palette_hits(a.cmd_palette.as_ref().unwrap());
        assert_eq!(hits[0].action, crate::actions::Action::StepSlice);
        assert_eq!(hits[0].keys, ",");
        a.on_key(key(KeyCode::Enter));
        assert!(a.cmd_palette.is_none());
        assert_eq!(a.take_commands(), vec![Command::StepSlice]);
        // Ctrl-P toggles; Esc closes.
        a.on_key(KeyEvent {
            modifiers: KeyModifiers::CONTROL,
            ..key(KeyCode::Char('p'))
        });
        assert!(a.cmd_palette.is_some());
        press(&mut a, "debug");
        a.on_key(key(KeyCode::Enter));
        assert_eq!(a.screen, Screen::Debug);
        a.on_key(KeyEvent {
            modifiers: KeyModifiers::CONTROL,
            ..key(KeyCode::Char('p'))
        });
        a.on_key(key(KeyCode::Esc));
        assert!(a.cmd_palette.is_none());
    }

    #[test]
    fn the_palette_lists_only_available_debugger_actions() {
        let a = app();
        assert!(a
            .palette_entries()
            .iter()
            .all(|i| i.action != crate::actions::Action::DebugStep));
        let a = debugging();
        assert!(a
            .palette_entries()
            .iter()
            .any(|i| i.action == crate::actions::Action::DebugStep));
        assert!(a
            .palette_entries()
            .iter()
            .any(|i| i.action == crate::actions::Action::Layer(LayerId::Fitness)));
    }

    #[test]
    fn help_scrolls_and_swallows_keys() {
        let mut a = app();
        press(&mut a, "?");
        assert!(a.show_help);
        press(&mut a, "jj");
        a.on_key(key(KeyCode::PageDown));
        assert_eq!(a.help_scroll, 12);
        a.on_key(key(KeyCode::Home));
        assert_eq!(a.help_scroll, 0);
        press(&mut a, "S");
        assert_eq!(a.screen, Screen::Map);
        a.on_key(key(KeyCode::Esc));
        assert!(!a.show_help);
    }

    fn configuring() -> App {
        let mut a = app();
        a.set_setup(Arc::new(setup(vec![
            setting("WORLD_X", "int", "60", "60"),
            setting("WORLD_Y", "int", "60", "60"),
            setting("COPY_MUT_PROB", "double", "0.0075", "0.0075"),
            setting("RANDOM_SEED", "int", "-1", "-1"),
        ])));
        a.goto(Screen::Config);
        a
    }

    #[test]
    fn config_filter_edit_and_restart() {
        let mut a = configuring();
        press(&mut a, "/");
        assert!(a.config.filtering);
        press(&mut a, "world_y");
        a.on_key(key(KeyCode::Enter));
        assert!(!a.config.filtering);
        let setup = a.setup.clone().unwrap();
        assert_eq!(a.config.visible(&setup), vec![1]);
        // Edit: a bad value is refused and stays in the prompt.
        press(&mut a, "e");
        assert_eq!(a.config.edit.as_ref().unwrap().buffer, "60");
        for _ in 0..2 {
            a.on_key(key(KeyCode::Backspace));
        }
        press(&mut a, "x1");
        a.on_key(key(KeyCode::Enter));
        assert!(a.config.edit.as_ref().unwrap().error.is_some());
        a.on_key(key(KeyCode::Backspace));
        a.on_key(key(KeyCode::Backspace));
        press(&mut a, "25");
        a.on_key(key(KeyCode::Enter));
        assert!(a.config.edit.is_none());
        assert_eq!(a.config.override_of("WORLD_Y"), Some("25"));
        assert_eq!(a.restart_args(), vec!["-set", "WORLD_Y", "25"]);
        // R asks first; n declines, y queues the request.
        press(&mut a, "R");
        assert!(a.confirm_restart);
        press(&mut a, "n");
        assert!(a.take_requests().is_empty());
        press(&mut a, "Ry");
        assert_eq!(
            a.take_requests(),
            vec![Request::Restart(vec![
                "-set".into(),
                "WORLD_Y".into(),
                "25".into()
            ])]
        );
        // Delete drops the override.
        a.on_key(key(KeyCode::Delete));
        assert!(a.config.overrides.is_empty());
    }

    #[test]
    fn config_edit_keys_do_not_open_screens_and_escape_cancels() {
        let mut a = configuring();
        press(&mut a, "e");
        press(&mut a, "S");
        assert_eq!(a.screen, Screen::Config);
        a.on_key(key(KeyCode::Esc));
        assert!(a.config.edit.is_none());
        press(&mut a, "c");
        assert!(a.config.changed_only);
    }

    #[test]
    fn restarting_resets_the_run_state() {
        let mut a = debugging();
        a.config.overrides.push(("WORLD_X".into(), "9".into()));
        a.debugger.breakpoints.insert((0, 3));
        a.reset_run(
            vec!["-set".into(), "WORLD_X".into(), "9".into()],
            Arc::new(setup(vec![])),
        );
        assert!(a.frame.is_none() && a.selection.is_none());
        assert!(a.debugger.breakpoints.is_empty() && a.config.overrides.is_empty());
        let cmds = a.take_commands();
        assert!(cmds.iter().any(|c| matches!(c, Command::Subscribe(_))));
        assert!(cmds.iter().any(|c| matches!(c, Command::SetSpeed(_))));
        assert_eq!(a.args, ["-set", "WORLD_X", "9"]);
    }

    #[test]
    fn genotype_views_and_lineage_tracing() {
        let mut frame = frame_with_map();
        frame.lineage = vec![
            genotype(0, 8, 7, "051-bbbbb", 1),
            genotype(1, 7, 3, "050-aaaaa", 2),
            genotype(2, 3, -1, "050-ccccc", 0),
        ];
        let mut a = app_with(frame);
        a.goto(Screen::Genotypes);
        press(&mut a, "v");
        assert_eq!(a.geno_view, GenoView::Abundance);
        press(&mut a, "v");
        assert_eq!(a.geno_view, GenoView::Lineage);
        press(&mut a, "vl");
        assert_eq!(a.geno_view, GenoView::Lineage);
        assert_eq!(a.highlight, Some(7));
        assert!(a
            .take_commands()
            .iter()
            .any(|c| matches!(c, Command::Subscribe(s) if s.lineage_of == Some(7))));
        // Founder first; the traced genotype is selected after a move.
        let rows: Vec<i32> = a.lineage_rows().iter().map(|g| g.id).collect();
        assert_eq!(rows, vec![3, 7, 8]);
        a.on_key(key(KeyCode::Up));
        assert_eq!(a.lineage_sel, 1);
        a.on_key(key(KeyCode::Home));
        assert_eq!(a.lineage_sel, 0);
        a.on_key(key(KeyCode::End));
        assert_eq!(a.lineage_sel, 2);
        // Enter on a living genotype jumps to its cell on the map.
        a.on_key(key(KeyCode::Enter));
        assert_eq!(a.screen, Screen::Map);
        assert_eq!(a.selection, Some(1));
    }
}
