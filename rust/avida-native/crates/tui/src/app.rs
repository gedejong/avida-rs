//! Application state and the logic that changes it, free of terminal I/O.
//!
//! [`App`] holds what the screens show (the latest [`Frame`], the row
//! history, the log, the map settings, the cursor and selection) and what
//! the user asked for. It never talks to the session: input handlers
//! (`input.rs`) queue [`Command`]s ([`App::take_commands`]) and [`Request`]s
//! for the shell ([`App::take_requests`]), and the runner feeds it what the
//! session published through the `apply_*` methods.

use std::collections::{HashMap, VecDeque};
use std::path::PathBuf;
use std::sync::Arc;

use avida_session::{
    Command, ExperimentSetup, Frame, LayerId, RunState, SessionEvent, Speed, Status, Subscription,
    UntilOutcome, UpdateRow,
};
use avida_viewmodel::{EnvView, Legend, Palette, Scale};
use ratatui::layout::Rect;

use crate::abundance::AbundanceHistory;
use crate::actions::PaletteState;
use crate::color::ColorMode;
use crate::config_view::ConfigState;
use crate::debugger::Debugger;
use crate::history::RowHistory;
use crate::log::{EventLog, LogKind};
use crate::metrics::Metric;

/// Genotypes listed in each frame.
pub const GENOTYPE_LIST_LEN: usize = 100;
/// Toasts kept.
pub const MAX_TOASTS: usize = 3;

/// Updates-per-second steps between "slow" and "max".
pub const SPEED_LADDER: [f64; 10] = [1.0, 2.0, 5.0, 10.0, 20.0, 50.0, 100.0, 200.0, 500.0, 1000.0];

/// What the user is looking at. The names follow the ncurses viewer's
/// screens where there was one.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Screen {
    Map,
    Stats,
    Charts,
    Genotypes,
    Zoom,
    /// The organism debugger: registers, heads, stacks, a memory listing
    /// with breakpoints, single-stepping.
    Debug,
    Environment,
    Histogram,
    Log,
    /// The effective configuration, with overrides and restart.
    Config,
    Options,
    /// The viewer's blank screen: nothing drawn but the status line.
    Blank,
}

impl Screen {
    /// The screens of the tab strip, in `Tab` order.
    pub const TABS: [Screen; 11] = [
        Screen::Map,
        Screen::Stats,
        Screen::Charts,
        Screen::Genotypes,
        Screen::Zoom,
        Screen::Debug,
        Screen::Environment,
        Screen::Histogram,
        Screen::Log,
        Screen::Config,
        Screen::Options,
    ];

    pub fn title(self) -> &'static str {
        match self {
            Screen::Map => "Map",
            Screen::Stats => "Stats",
            Screen::Charts => "Charts",
            Screen::Genotypes => "Geno",
            Screen::Zoom => "Zoom",
            Screen::Debug => "Debug",
            Screen::Environment => "Env",
            Screen::Histogram => "Hist",
            Screen::Log => "Log",
            Screen::Config => "Cfg",
            Screen::Options => "Opts",
            Screen::Blank => "Blank",
        }
    }

    /// The key that opens the screen (shown in the tab strip).
    pub fn key(self) -> char {
        match self {
            Screen::Map => 'M',
            Screen::Stats => 'S',
            Screen::Charts => 'G',
            Screen::Genotypes => 'T',
            Screen::Zoom => 'Z',
            Screen::Debug => 'U',
            Screen::Environment => 'E',
            Screen::Histogram => 'H',
            Screen::Log => 'D',
            Screen::Config => 'F',
            Screen::Options => 'O',
            Screen::Blank => 'B',
        }
    }
}

/// Sub-views of the environment screen.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EnvTab {
    Resources,
    Reactions,
    Tasks,
}

impl EnvTab {
    pub const ALL: [EnvTab; 3] = [EnvTab::Resources, EnvTab::Reactions, EnvTab::Tasks];

    pub fn title(self) -> &'static str {
        match self {
            EnvTab::Resources => "Resources",
            EnvTab::Reactions => "Reactions",
            EnvTab::Tasks => "Tasks",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ToastKind {
    Info,
    Success,
    Error,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Toast {
    pub kind: ToastKind,
    pub text: String,
    pub expires: f64,
}

/// The sub-views of the genotypes screen.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GenoView {
    Table,
    Abundance,
    Lineage,
}

impl GenoView {
    pub fn next(self) -> GenoView {
        match self {
            GenoView::Table => GenoView::Abundance,
            GenoView::Abundance => GenoView::Lineage,
            GenoView::Lineage => GenoView::Table,
        }
    }

    pub fn title(self) -> &'static str {
        match self {
            GenoView::Table => "Table",
            GenoView::Abundance => "Abundance",
            GenoView::Lineage => "Lineage",
        }
    }
}

/// Something only the shell can do.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Request {
    /// Write a text file (the extracted genome).
    WriteFile { path: PathBuf, contents: String },
    /// Redraw from scratch (Ctrl-L).
    Redraw,
    /// Build the world again from the same folder with these `avida`
    /// arguments.
    Restart(Vec<String>),
}

/// Where the map was drawn, for hit testing and scrolling.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct MapGeom {
    /// Terminal area the cells fill (not the surrounding block).
    pub area: Rect,
    /// Terminal columns per cell (1 for half-blocks, 2 for symbols).
    pub cell_w: u16,
    /// Cells per terminal row (2 for half-blocks, 1 for symbols).
    pub cell_rows: u16,
    /// World cell at the top left of `area`.
    pub ox: usize,
    pub oy: usize,
    /// Cells shown.
    pub vis_w: usize,
    pub vis_h: usize,
}

impl MapGeom {
    /// The world cell under a terminal position.
    pub fn cell_at(&self, col: u16, row: u16) -> Option<(usize, usize)> {
        let a = self.area;
        if self.cell_w == 0
            || col < a.x
            || row < a.y
            || col >= a.x + a.width
            || row >= a.y + a.height
        {
            return None;
        }
        let cx = usize::from((col - a.x) / self.cell_w);
        let cy = usize::from(row - a.y) * usize::from(self.cell_rows);
        // For half-blocks both cells of the character row are candidates;
        // the pointer cannot tell which, so take the upper one.
        if cx >= self.vis_w || cy >= self.vis_h {
            return None;
        }
        Some((self.ox + cx, self.oy + cy))
    }
}

/// Cache of the rendered map colours.
#[derive(Clone, Debug)]
pub struct MapCache {
    pub key: (u64, LayerId, Palette, Scale),
    pub rgba: Vec<u8>,
    pub legend: Option<Legend>,
}

/// Updates per second, from when rows arrive.
#[derive(Clone, Debug, Default)]
pub struct RateMeter {
    samples: VecDeque<(f64, i32)>,
}

impl RateMeter {
    pub const WINDOW: f64 = 2.0;

    pub fn record(&mut self, now: f64, update: i32) {
        self.samples.push_back((now, update));
        while self
            .samples
            .front()
            .is_some_and(|&(t, _)| now - t > Self::WINDOW)
            && self.samples.len() > 2
        {
            self.samples.pop_front();
        }
    }

    pub fn rate(&self, now: f64) -> Option<f64> {
        let &(t1, u1) = self.samples.back()?;
        let &(t0, u0) = self.samples.front()?;
        if now - t1 > 1.0 || t1 <= t0 {
            return None;
        }
        Some(f64::from(u1 - u0) / (t1 - t0))
    }
}

/// The application state.
#[derive(Debug)]
pub struct App {
    pub name: String,
    pub work_dir: PathBuf,
    pub args: Vec<String>,
    pub color_mode: ColorMode,

    pub screen: Screen,
    pub status: Option<Status>,
    pub frame: Option<Arc<Frame>>,
    /// Frames applied so far.
    pub frames_applied: u64,
    pub history: RowHistory,
    pub log: EventLog,
    pub toasts: Vec<Toast>,
    /// The error that stopped the run.
    pub fatal: Option<String>,
    /// The arguments of a `Restart` sent and not yet answered. Until the
    /// session answers, commands are not sent and rows and frames (which
    /// may belong to either run) are not shown.
    pub restarting: Option<Vec<String>>,
    pub rate: RateMeter,
    pub now: f64,

    // Map.
    pub layer: LayerId,
    pub palette: Palette,
    pub scales: HashMap<LayerId, Scale>,
    /// Draw each cell two columns wide (symbols for genotypes).
    pub big_cells: bool,
    /// Cursor cell (x, y).
    pub cursor: (usize, usize),
    cursor_placed: bool,
    /// The inspected cell.
    pub selection: Option<i32>,
    pub geom: Option<MapGeom>,
    pub map_cache: Option<MapCache>,
    /// Screen positions of the tab-strip entries.
    pub tab_hits: Vec<(Rect, Screen)>,

    // Speed.
    /// `None` is as fast as possible.
    pub speed_idx: Option<usize>,

    // Per-screen state.
    pub stats_task_offset: usize,
    pub chart_metrics: Vec<Metric>,
    pub chart_focus: usize,
    pub chart_log: bool,
    pub geno_sel: usize,
    pub geno_view: GenoView,
    /// The genotype whose lineage the frames carry.
    pub highlight: Option<i32>,
    pub lineage_sel: usize,
    pub abundance: AbundanceHistory,
    pub debugger: Debugger,
    /// What the world was built from (set by the shell).
    pub setup: Option<Arc<ExperimentSetup>>,
    pub config: ConfigState,
    /// The command palette, while open.
    pub cmd_palette: Option<PaletteState>,
    pub confirm_restart: bool,
    pub help_scroll: usize,
    pub env_tab: EnvTab,
    pub env_scroll: usize,
    pub log_scroll: usize,
    /// The log follows its newest line.
    pub log_follow: bool,
    /// Show only warnings and errors on the log screen.
    pub log_problems_only: bool,
    /// Rows the log screen showed and lines it could show, as drawn last
    /// (scrolling by keys needs them).
    pub log_view_h: usize,
    pub log_shown: usize,
    pub zoom_thread: usize,
    pub zoom_scroll: usize,
    pub zoom_follow_ip: bool,

    // Overlays and exit.
    pub show_help: bool,
    pub confirm_quit: bool,
    pub quit: bool,
    /// Makes the loop panic after the next draw (the `AVIDA_TUI_TEST_PANIC`
    /// hook that lets the real-terminal tests check the panic path).
    pub test_panic: bool,

    commands: Vec<Command>,
    requests: Vec<Request>,
}

/// A line like `update 100` for messages.
pub fn update_phrase(update: i32) -> String {
    if update < 0 {
        "the start".to_string()
    } else {
        format!("update {update}")
    }
}

impl App {
    /// A new application for a run in `work_dir`. The first subscription is
    /// queued.
    pub fn new(work_dir: PathBuf, args: Vec<String>, color_mode: ColorMode) -> App {
        let name = display_name(&work_dir);
        let mut app = App {
            name,
            work_dir,
            args,
            color_mode,
            screen: Screen::Map,
            status: None,
            frame: None,
            frames_applied: 0,
            history: RowHistory::default(),
            log: EventLog::default(),
            toasts: Vec::new(),
            fatal: None,
            restarting: None,
            rate: RateMeter::default(),
            now: 0.0,
            layer: LayerId::Genotype,
            palette: Palette::Viridis,
            scales: HashMap::new(),
            big_cells: false,
            cursor: (0, 0),
            cursor_placed: false,
            selection: None,
            geom: None,
            map_cache: None,
            tab_hits: Vec::new(),
            speed_idx: None,
            stats_task_offset: 0,
            chart_metrics: vec![Metric::AveFitness],
            chart_focus: 0,
            chart_log: false,
            geno_sel: 0,
            geno_view: GenoView::Table,
            highlight: None,
            lineage_sel: 0,
            abundance: AbundanceHistory::default(),
            debugger: Debugger::default(),
            setup: None,
            config: ConfigState::default(),
            cmd_palette: None,
            confirm_restart: false,
            help_scroll: 0,
            env_tab: EnvTab::Resources,
            env_scroll: 0,
            log_scroll: 0,
            log_follow: true,
            log_problems_only: false,
            log_view_h: 20,
            log_shown: 0,
            zoom_thread: 0,
            zoom_scroll: 0,
            zoom_follow_ip: true,
            show_help: false,
            confirm_quit: false,
            quit: false,
            test_panic: false,
            commands: Vec::new(),
            requests: Vec::new(),
        };
        app.subscribe();
        app
    }

    // --- what the session published ---------------------------------------

    pub fn set_time(&mut self, now: f64) {
        self.now = now;
        self.toasts.retain(|t| t.expires > now);
    }

    pub fn apply_status(&mut self, status: Status) {
        self.status = Some(status);
    }

    pub fn apply_frame(&mut self, frame: Arc<Frame>) {
        if self.restarting.is_some() {
            return;
        }
        if !self.cursor_placed {
            if let (Ok(w), Ok(h)) = (
                usize::try_from(frame.header.world_x),
                usize::try_from(frame.header.world_y),
            ) {
                if w > 0 && h > 0 {
                    self.cursor = (w / 2, h / 2);
                    self.cursor_placed = true;
                }
            }
        }
        if let Some(org) = &frame.inspect {
            if self.zoom_thread >= org.cpu.threads.len() {
                self.zoom_thread = org.cpu.cur_thread;
            }
        }
        self.abundance.record(&frame);
        if let Some(cell) = self.selection {
            self.debugger.observe(cell, &frame);
        }
        self.frame = Some(frame);
        self.frames_applied += 1;
    }

    /// What the session was built from, for the configuration screen.
    pub fn set_setup(&mut self, setup: Arc<ExperimentSetup>) {
        self.setup = Some(setup);
    }

    pub fn apply_rows(&mut self, rows: Vec<UpdateRow>) {
        if self.restarting.is_some() {
            return;
        }
        if let Some(last) = rows.last() {
            self.rate.record(self.now, last.update);
        }
        self.history.extend(rows);
    }

    pub fn apply_events(&mut self, events: Vec<SessionEvent>) {
        for e in events {
            self.apply_event(e);
        }
    }

    fn apply_event(&mut self, event: SessionEvent) {
        match event {
            SessionEvent::Running | SessionEvent::Paused { .. } => {}
            SessionEvent::RunUntilEnded {
                outcome,
                slices,
                update,
            } => {
                let note = self.debugger.until_ended(outcome, slices, update).clone();
                if note.is_problem() {
                    self.toast(ToastKind::Info, note.text());
                }
                if outcome != UntilOutcome::Interrupted {
                    self.log.push(LogKind::Info, note.text());
                }
            }
            SessionEvent::WatchMoved {
                from,
                to,
                org_id,
                offspring,
            } => {
                // The session moved the watch already: no `WatchCell`.
                if self.selection == Some(from) || self.selection.is_none() {
                    self.selection = Some(to);
                    self.debugger.moved(org_id);
                    if let Some((w, _)) = self.world_size() {
                        let to = usize::try_from(to).unwrap_or(0);
                        self.cursor = (to % w, to / w);
                    }
                }
                let text = if offspring {
                    format!("Following offspring #{org_id} into cell {to}.")
                } else {
                    format!("Organism #{org_id} moved to cell {to}.")
                };
                self.log.push(LogKind::Info, text);
            }
            SessionEvent::Finished { update } => {
                let text = format!("The run finished at {}.", update_phrase(update));
                self.log.push(LogKind::Info, text.clone());
                self.toast(ToastKind::Success, text);
            }
            SessionEvent::Fatal(e) => {
                self.log.push_fatal(e.clone());
                let first = e.lines().next().unwrap_or("").to_string();
                self.toast(ToastKind::Error, format!("The run stopped: {first}"));
                self.fatal = Some(e);
            }
            SessionEvent::Error(e) => {
                self.log.push(LogKind::Error, e.clone());
                self.toast(ToastKind::Error, e);
            }
            SessionEvent::PopulationSaved(path) => {
                let text = format!("Population saved to {}", path.display());
                self.log.push(LogKind::Info, text.clone());
                self.toast(ToastKind::Success, text);
            }
            SessionEvent::Stdout(line) => self.log.push(LogKind::Output, line),
            SessionEvent::Stderr(line) => self.log.push_stderr(line),
            SessionEvent::Born {
                parent_id,
                org_id,
                cell,
                update,
                ..
            } => {
                let at = self.world_size().map_or(String::new(), |(w, _)| {
                    let c = usize::try_from(cell).unwrap_or(0);
                    format!(" ({},{})", c % w, c / w)
                });
                self.log.push(
                    LogKind::Info,
                    format!(
                        "Organism #{parent_id} gave birth to #{org_id} in cell {cell}{at} at {}.",
                        update_phrase(update)
                    ),
                );
            }
            SessionEvent::Restarted(setup) => {
                let args = self.restarting.take().unwrap_or_else(|| self.args.clone());
                let note = if args.is_empty() {
                    "Restarted with the original arguments.".to_string()
                } else {
                    format!("Restarted with {}", args.join(" "))
                };
                self.reset_run(args, setup);
                self.notify(ToastKind::Info, note);
            }
            SessionEvent::RestartFailed(e) => {
                self.restarting = None;
                self.notify(ToastKind::Error, format!("Restart failed: {e}"));
            }
        }
    }

    // --- derived -----------------------------------------------------------

    /// The run state; a fatal error overrides it.
    pub fn run_state(&self) -> RunState {
        if self.fatal.is_some() {
            return RunState::Failed;
        }
        self.status.as_ref().map_or(RunState::Paused, |s| s.state)
    }

    pub fn can_advance(&self) -> bool {
        matches!(self.run_state(), RunState::Paused | RunState::Running)
    }

    /// Whether a debugger step (one CPU cycle) of the watched organism can
    /// run several micro-operations: BCR and GP8 hardware.
    pub fn steps_run_micro_ops(&self) -> bool {
        self.debugger
            .current
            .as_ref()
            .is_some_and(|o| o.cpu.cycle_runs_micro_ops())
    }

    pub fn current_update(&self) -> Option<i32> {
        self.status
            .as_ref()
            .map(|s| s.update)
            .or_else(|| self.frame.as_ref().map(|f| f.update))
    }

    /// The statistics of the last finished update.
    pub fn latest_stats(&self) -> Option<&UpdateRow> {
        self.history
            .latest()
            .or_else(|| self.frame.as_ref().map(|f| &f.header.stats))
    }

    pub fn env(&self) -> Option<&EnvView> {
        self.frame.as_ref().map(|f| &f.env)
    }

    /// The layers of the current world.
    pub fn layers(&self) -> Vec<LayerId> {
        match self.env() {
            Some(env) => available_layers(env),
            None => LayerId::FIXED.to_vec(),
        }
    }

    pub fn world_size(&self) -> Option<(usize, usize)> {
        let h = &self.frame.as_ref()?.header;
        let (w, hh) = (
            usize::try_from(h.world_x).ok()?,
            usize::try_from(h.world_y).ok()?,
        );
        (w > 0 && hh > 0).then_some((w, hh))
    }

    pub fn cursor_cell(&self) -> Option<i32> {
        let (w, _) = self.world_size()?;
        i32::try_from(self.cursor.1 * w + self.cursor.0).ok()
    }

    pub fn scale(&self) -> Scale {
        self.scale_of(self.layer)
    }

    pub fn scale_of(&self, layer: LayerId) -> Scale {
        self.scales
            .get(&layer)
            .copied()
            .unwrap_or_else(|| layer.default_scale())
    }

    /// The palette used for `layer` (the genotype layer is categorical).
    pub fn palette_for(&self, layer: LayerId) -> Palette {
        if layer == LayerId::Genotype {
            Palette::OkabeIto
        } else {
            self.palette
        }
    }

    /// Whether the map draws two-column cells: asked for with `i`, or
    /// forced when there is no colour and the layer is categorical (the
    /// half-block cells would show brightness only, so the genotypes would
    /// be indistinguishable; the two-column cells carry a letter each).
    pub fn wide_cells(&self) -> bool {
        self.big_cells || self.mono_letters()
    }

    /// No colour and a categorical layer: letters stand in for colours.
    pub fn mono_letters(&self) -> bool {
        self.color_mode.is_mono() && self.palette_for(self.layer).is_categorical()
    }

    /// Whether the linear/log choice means anything for the layer.
    pub fn scale_applies(layer: LayerId) -> bool {
        layer != LayerId::Genotype && !layer.is_boolean()
    }

    pub fn subscription(&self) -> Subscription {
        Subscription {
            layers: vec![self.layer],
            inspect_cell: self.selection,
            genotypes_top_n: GENOTYPE_LIST_LEN,
            lineage_of: self.highlight,
        }
    }

    pub fn speed(&self) -> Speed {
        match self.speed_idx {
            Some(i) => Speed::UpdatesPerSecond(SPEED_LADDER[i.min(SPEED_LADDER.len() - 1)]),
            None => Speed::Max,
        }
    }

    pub fn speed_label(&self) -> String {
        match self.speed_idx {
            Some(i) => format!("{}/s", SPEED_LADDER[i.min(SPEED_LADDER.len() - 1)]),
            None => "max".to_string(),
        }
    }

    // --- actions -----------------------------------------------------------

    pub(crate) fn send(&mut self, cmd: Command) {
        // A command now would reach the old run or the new one depending on
        // timing; the new run gets the subscription and speed once it is
        // there (`reset_run`).
        if self.restarting.is_none() {
            self.commands.push(cmd);
        }
    }

    /// A `Restart` with `args` was sent to the session.
    pub fn begin_restart(&mut self, args: Vec<String>) {
        self.commands.clear();
        self.restarting = Some(args);
        self.log
            .push(LogKind::Info, "Restarting the run...".to_string());
    }

    pub fn subscribe(&mut self) {
        let sub = self.subscription();
        self.send(Command::Subscribe(sub));
    }

    pub(crate) fn refuse_if_over(&mut self) -> bool {
        match self.run_state() {
            RunState::Finished => {
                self.toast(
                    ToastKind::Info,
                    "The run has finished; nothing can advance it.".to_string(),
                );
                true
            }
            RunState::Failed => {
                self.toast(
                    ToastKind::Error,
                    "The run stopped on an error; nothing can advance it.".to_string(),
                );
                true
            }
            _ => false,
        }
    }

    /// Run when paused, pause when running.
    pub fn toggle_run(&mut self) {
        if self.refuse_if_over() {
            return;
        }
        if self.run_state() == RunState::Running {
            self.send(Command::Pause);
        } else {
            self.send(Command::Run);
        }
    }

    pub fn pause(&mut self) {
        if !self.refuse_if_over() {
            self.send(Command::Pause);
        }
    }

    pub fn step_update(&mut self) {
        if !self.refuse_if_over() {
            self.send(Command::StepUpdate);
        }
    }

    pub fn step_slice(&mut self) {
        if !self.refuse_if_over() {
            self.send(Command::StepSlice);
        }
    }

    pub fn faster(&mut self) {
        self.speed_idx = match self.speed_idx {
            None => None,
            Some(i) if i + 1 < SPEED_LADDER.len() => Some(i + 1),
            Some(_) => None,
        };
        self.send(Command::SetSpeed(self.speed()));
    }

    pub fn slower(&mut self) {
        self.speed_idx = match self.speed_idx {
            None => Some(SPEED_LADDER.len() - 1),
            Some(0) => Some(0),
            Some(i) => Some(i - 1),
        };
        self.send(Command::SetSpeed(self.speed()));
    }

    pub fn save_population(&mut self) {
        if self.run_state() == RunState::Failed {
            self.toast(
                ToastKind::Error,
                "A failed run's population cannot be saved.".to_string(),
            );
        } else {
            self.send(Command::SavePopulation);
        }
    }

    pub fn set_layer(&mut self, layer: LayerId) {
        if self.layer != layer {
            self.layer = layer;
            self.subscribe();
        }
    }

    /// Moves to the next (`delta > 0`) or previous layer, wrapping.
    pub fn cycle_layer(&mut self, delta: i32) {
        let layers = self.layers();
        if layers.is_empty() {
            return;
        }
        let i = layers.iter().position(|&l| l == self.layer).unwrap_or(0) as i32;
        let n = layers.len() as i32;
        let j = (i + delta).rem_euclid(n) as usize;
        self.set_layer(layers[j]);
    }

    pub fn toggle_scale(&mut self) {
        if !Self::scale_applies(self.layer) {
            self.toast(
                ToastKind::Info,
                "This layer has no linear/log choice.".to_string(),
            );
            return;
        }
        let next = match self.scale() {
            Scale::Linear => Scale::Log,
            Scale::Log => Scale::Linear,
        };
        self.scales.insert(self.layer, next);
    }

    pub fn cycle_palette(&mut self) {
        self.palette = match self.palette {
            Palette::Viridis => Palette::Cividis,
            _ => Palette::Viridis,
        };
    }

    /// Inspects a cell (or nothing).
    pub fn select(&mut self, cell: Option<i32>) {
        if self.selection == cell {
            return;
        }
        self.selection = cell;
        // A new organism is a new debugging session.
        self.debugger.reset();
        self.zoom_thread = 0;
        self.zoom_scroll = 0;
        self.zoom_follow_ip = true;
        self.send(Command::WatchCell(cell));
    }

    /// Puts the cursor on `(x, y)`, clamped to the world.
    pub fn set_cursor(&mut self, x: usize, y: usize) {
        if let Some((w, h)) = self.world_size() {
            self.cursor = (x.min(w - 1), y.min(h - 1));
        } else {
            self.cursor = (x, y);
        }
        if self.screen == Screen::Zoom {
            self.select(self.cursor_cell());
        }
    }

    pub fn move_cursor(&mut self, dx: i32, dy: i32) {
        let Some((w, h)) = self.world_size() else {
            return;
        };
        let nx = (self.cursor.0 as i64 + i64::from(dx)).clamp(0, w as i64 - 1) as usize;
        let ny = (self.cursor.1 as i64 + i64::from(dy)).clamp(0, h as i64 - 1) as usize;
        self.set_cursor(nx, ny);
    }

    /// Selects the organism under the cursor.
    pub fn select_cursor(&mut self) {
        let cell = self.cursor_cell();
        self.select(cell);
    }

    /// Moves the cursor to the next occupied cell after it (row-major,
    /// wrapping) and inspects it: the viewer's "choose new CPU".
    pub fn choose_next_organism(&mut self) {
        let Some((w, h)) = self.world_size() else {
            return;
        };
        let Some(map) = self.frame.as_ref().and_then(|f| f.map.as_ref()) else {
            return;
        };
        let n = w * h;
        let start = self.cursor.1 * w + self.cursor.0;
        let found = (1..=n)
            .map(|k| (start + k) % n)
            .find(|&c| map.genotype_id.get(c).is_some_and(|&g| g != -1));
        match found {
            Some(c) => {
                self.cursor = (c % w, c / w);
                self.select_cursor();
            }
            None => self.toast(ToastKind::Info, "No organism alive.".to_string()),
        }
    }

    /// Puts the cursor on the occupied cell holding `genotype` that is
    /// nearest to the current one.
    pub fn jump_to_genotype(&mut self, genotype: i32) -> bool {
        let Some((w, _)) = self.world_size() else {
            return false;
        };
        let Some(map) = self.frame.as_ref().and_then(|f| f.map.as_ref()) else {
            return false;
        };
        let (cx, cy) = (self.cursor.0 as i64, self.cursor.1 as i64);
        let best = map
            .genotype_id
            .iter()
            .enumerate()
            .filter(|&(_, &g)| g == genotype)
            .min_by_key(|&(c, _)| {
                let (x, y) = ((c % w) as i64, (c / w) as i64);
                (x - cx).abs() + (y - cy).abs()
            })
            .map(|(c, _)| c);
        match best {
            Some(c) => {
                self.cursor = (c % w, c / w);
                self.select_cursor();
                true
            }
            None => false,
        }
    }

    /// Queues a file with the inspected organism's genome, one instruction
    /// name per line (the `.org` format `LOAD_ORGANISM` reads).
    pub fn extract_organism(&mut self) {
        let Some(org) = self.frame.as_ref().and_then(|f| f.inspect.as_ref()) else {
            self.toast(
                ToastKind::Error,
                "No organism inspected: press Enter on the map first.".to_string(),
            );
            return;
        };
        let mut contents = String::new();
        // The genome, not the memory the organism allocated beyond it.
        let len = usize::try_from(org.phenotype.genome_length)
            .map_or(org.cpu.memory.len(), |n| n.min(org.cpu.memory.len()));
        for pos in 0..len {
            contents.push_str(org.cpu.inst_name(pos));
            contents.push('\n');
        }
        let update = self.frame.as_ref().map_or(0, |f| f.update);
        let path = self
            .work_dir
            .join("data")
            .join(format!("extracted-u{update}-cell{}.org", org.cell));
        self.requests.push(Request::WriteFile { path, contents });
    }

    pub fn goto(&mut self, screen: Screen) {
        self.screen = screen;
        if matches!(screen, Screen::Zoom | Screen::Debug) {
            // The inspector follows the cursor.
            if self.selection.is_none() {
                self.select_cursor();
            }
        }
    }

    pub fn next_screen(&mut self, delta: i32) {
        let tabs = Screen::TABS;
        let i = tabs.iter().position(|&s| s == self.screen);
        let next = match i {
            Some(i) => tabs[(i as i32 + delta).rem_euclid(tabs.len() as i32) as usize],
            None => Screen::Map,
        };
        self.goto(next);
    }

    pub fn toast(&mut self, kind: ToastKind, text: String) {
        self.toasts.retain(|t| t.text != text);
        let secs = if kind == ToastKind::Error { 8.0 } else { 4.0 };
        self.toasts.push(Toast {
            kind,
            text,
            expires: self.now + secs,
        });
        let excess = self.toasts.len().saturating_sub(MAX_TOASTS);
        self.toasts.drain(..excess);
    }

    /// Logs and announces something the shell did.
    pub fn notify(&mut self, kind: ToastKind, text: String) {
        let k = if kind == ToastKind::Error {
            LogKind::Error
        } else {
            LogKind::Info
        };
        self.log.push(k, text.clone());
        self.toast(kind, text);
    }

    /// The session restarted (with `args`): forget everything the old run
    /// showed and ask the new one for a frame.
    pub fn reset_run(&mut self, args: Vec<String>, setup: Arc<ExperimentSetup>) {
        self.restarting = None;
        self.args = args;
        self.setup = Some(setup);
        self.status = None;
        self.frame = None;
        self.frames_applied = 0;
        self.history = RowHistory::default();
        self.rate = RateMeter::default();
        self.fatal = None;
        self.selection = None;
        self.map_cache = None;
        self.cursor_placed = false;
        self.abundance.clear();
        self.debugger = Debugger::default();
        self.highlight = None;
        self.geno_sel = 0;
        self.lineage_sel = 0;
        self.config = ConfigState::default();
        self.zoom_thread = 0;
        self.zoom_scroll = 0;
        self.zoom_follow_ip = true;
        self.commands.clear();
        self.send(Command::SetSpeed(self.speed()));
        self.subscribe();
    }

    pub fn take_commands(&mut self) -> Vec<Command> {
        std::mem::take(&mut self.commands)
    }

    pub fn take_requests(&mut self) -> Vec<Request> {
        std::mem::take(&mut self.requests)
    }

    pub fn pending_commands(&self) -> &[Command] {
        &self.commands
    }

    pub fn request(&mut self, r: Request) {
        self.requests.push(r);
    }
}

/// The layers a world has: the fixed per-organism ones, one per task, one
/// per resource.
pub fn available_layers(env: &EnvView) -> Vec<LayerId> {
    LayerId::FIXED
        .iter()
        .copied()
        .chain((0..env.tasks.len()).map(LayerId::Task))
        .chain((0..env.resources.len()).map(LayerId::Resource))
        .collect()
}

/// A display name: "Fitness", "Task: NAND", "Resource: glucose".
pub fn layer_title(env: Option<&EnvView>, layer: LayerId) -> String {
    let fixed = match layer {
        LayerId::Genotype => "Genotype",
        LayerId::Fitness => "Fitness",
        LayerId::Merit => "Merit",
        LayerId::Gestation => "Gestation time",
        LayerId::Age => "Age",
        LayerId::Generation => "Generation",
        LayerId::GenomeLen => "Genome length",
        LayerId::Lineage => "Lineage label",
        LayerId::BreedTrue => "Breeds true",
        LayerId::Modified => "Modified",
        LayerId::Threads => "Threads",
        LayerId::TaskCount => "Tasks performed",
        LayerId::Task(k) => {
            return match env.and_then(|e| e.tasks.get(k)) {
                Some(t) if !t.desc.is_empty() => format!("Task: {}", t.desc),
                Some(t) => format!("Task: {}", t.name),
                None => layer.key(),
            }
        }
        LayerId::Resource(r) => {
            return match env.and_then(|e| e.resources.get(r)) {
                Some(res) => format!("Resource: {}", res.name),
                None => layer.key(),
            }
        }
    };
    fixed.to_string()
}

/// `heads_default_100u / config` for a folder called `config`.
pub fn display_name(dir: &std::path::Path) -> String {
    let dir = std::fs::canonicalize(dir).unwrap_or_else(|_| dir.to_path_buf());
    let name = dir
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| dir.display().to_string());
    let generic = matches!(name.as_str(), "config" | "configs" | "cfg" | "run" | "data");
    match dir.parent().and_then(std::path::Path::file_name) {
        Some(parent) if generic => format!("{} / {name}", parent.to_string_lossy()),
        _ => name,
    }
}
