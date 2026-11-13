//! Application state and the logic that changes it, free of widgets and
//! windows.
//!
//! [`AppState`] holds what the UI shows (the latest [`Frame`], the row
//! history, the log, the map settings and selection) and what the user
//! asked for. Widgets never talk to the session: they call methods here,
//! which queue [`Command`]s for the session ([`AppState::take_commands`])
//! or [`Request`]s for the application shell ([`AppState::take_requests`]:
//! file dialogs, exports, opening an experiment).

pub mod abundance;
pub mod commands;
pub mod config;
pub mod debugger;
pub mod export;
pub mod format;
pub mod genotypes;
pub mod history;
pub mod layers;
pub mod log;
pub mod metrics;
pub mod new_experiment;
pub mod viewport;
pub mod vision;

#[cfg(test)]
pub(crate) mod fixtures;

use std::collections::VecDeque;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use avida_session::{
    Command, ExperimentSetup, Frame, LayerId, PlaybackInfo, RecordingStatus, RunState,
    SessionEvent, Speed, Status, Subscription, UntilOutcome, UpdateRow,
};

use self::abundance::AbundanceHistory;
use self::commands::PaletteState;
use self::config::ConfigView;
use self::debugger::Debugger;
use self::genotypes::GenoSort;
use self::history::RowHistory;
use self::layers::MapSettings;
use self::log::{EventLog, LogFilter, LogKind, Toast, ToastKind};
use self::metrics::Metric;
use self::viewport::Viewport;
use self::vision::ColorVision;

/// Genotypes listed in each frame (the genotype table).
pub const GENOTYPE_LIST_LEN: usize = 100;
/// Toasts shown at once.
pub const MAX_TOASTS: usize = 4;

/// An experiment folder and the arguments its world is built with.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Experiment {
    pub dir: PathBuf,
    pub args: Vec<String>,
    /// Run pauses once this update has ended (a front-end limit: the
    /// world's own events still decide when the run is over).
    pub update_limit: Option<i32>,
}

impl Experiment {
    /// A short name: the folder name, with its parent when the folder has
    /// a generic name such as `config` (`heads_default_100u / config`).
    pub fn name(&self) -> String {
        display_name(&self.dir)
    }
}

/// See [`Experiment::name`].
pub fn display_name(dir: &Path) -> String {
    let name = dir
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| dir.display().to_string());
    let generic = matches!(name.as_str(), "config" | "configs" | "cfg" | "run" | "data");
    match dir.parent().and_then(Path::file_name) {
        Some(parent) if generic => format!("{} / {name}", parent.to_string_lossy()),
        _ => name,
    }
}

/// The experiment folder a dropped path stands for: a folder itself, or
/// the folder of a dropped file (`avida.cfg`, say). `None` for a path that
/// does not exist.
pub fn dropped_folder(path: &Path) -> Option<PathBuf> {
    if path.is_dir() {
        Some(path.to_path_buf())
    } else if path.is_file() {
        path.parent().map(Path::to_path_buf)
    } else {
        None
    }
}

/// Lines of what avida printed shown with the error that stopped a run.
pub const FATAL_CONTEXT_LINES: usize = 8;

/// A line avida printed on stdout that reports a problem (`error: ...`,
/// as `Feedback().Error` prints it).
fn is_error_line(line: &str) -> bool {
    let l = line.trim_start().to_ascii_lowercase();
    l.starts_with("error") || l.starts_with("warning")
}

/// Why a folder could not be opened, in words for the start screen.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OpenProblem {
    /// One sentence: what went wrong.
    pub title: String,
    /// What to do about it.
    pub hint: String,
    /// What avida said, when it says more than the title (shown in a
    /// fixed-width font).
    pub detail: Option<String>,
    /// The folder a new experiment could be created in, when the folder
    /// is missing or holds no configuration.
    pub create_here: Option<PathBuf>,
}

impl OpenProblem {
    /// Reads what avida (or the session) said about `dir`.
    pub fn explain(dir: Option<&Path>, raw: &str) -> OpenProblem {
        let raw = raw.trim();
        let missing_dir = dir.is_some_and(|d| !d.exists());
        let no_config = raw.contains("configuration file 'avida.cfg' not found")
            || dir.is_some_and(|d| d.is_dir() && !d.join("avida.cfg").exists());
        if missing_dir {
            OpenProblem {
                title: "This folder does not exist.".to_string(),
                hint: "It may have been moved or deleted. Choose another folder, or create a new experiment there.".to_string(),
                detail: dir.map(|d| d.display().to_string()),
                create_here: dir.map(Path::to_path_buf),
            }
        } else if no_config {
            OpenProblem {
                title: "There is no avida.cfg in this folder.".to_string(),
                hint: "An experiment folder holds avida.cfg, events.cfg and environment.cfg. Choose the folder that has them, or create a new experiment from the default settings.".to_string(),
                detail: dir.map(|d| d.display().to_string()),
                create_here: dir.map(Path::to_path_buf),
            }
        } else {
            OpenProblem {
                title: "Avida could not build the world from this folder.".to_string(),
                hint: "This is what avida reported. Fix the file it names, then open the folder again.".to_string(),
                detail: Some(raw.to_string()),
                create_here: None,
            }
        }
    }
}

/// How a run that is over ended, for the banner over the map.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RunEnd {
    pub failed: bool,
    /// One sentence: `The run finished at update 100.`
    pub title: String,
    /// Why, when known: `The Exit event in events.cfg ended it.`
    pub reason: String,
    /// The status the `avida` binary would exit with.
    pub exit_code: Option<i32>,
}

/// Where the application is.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Phase {
    /// No experiment: the start screen.
    Start,
    /// The world is being built on the simulation thread.
    Opening,
    /// A session exists (it may have finished or failed).
    Ready,
}

/// The tabs of the side panel.
#[derive(
    Clone, Copy, Debug, Default, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize,
)]
pub enum SideTab {
    #[default]
    Stats,
    Charts,
    Genotypes,
    Organism,
    Environment,
    Config,
}

impl SideTab {
    pub const ALL: [SideTab; 6] = [
        SideTab::Stats,
        SideTab::Charts,
        SideTab::Genotypes,
        SideTab::Organism,
        SideTab::Environment,
        SideTab::Config,
    ];

    pub fn title(self) -> &'static str {
        match self {
            SideTab::Stats => "Stats",
            SideTab::Charts => "Charts",
            SideTab::Genotypes => "Genotypes",
            SideTab::Organism => "Organism",
            SideTab::Environment => "Environment",
            SideTab::Config => "Config",
        }
    }
}

/// The views of the charts tab.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum ChartTab {
    #[default]
    TimeSeries,
    Tasks,
    Histogram,
}

impl ChartTab {
    pub const ALL: [ChartTab; 3] = [ChartTab::TimeSeries, ChartTab::Tasks, ChartTab::Histogram];

    pub fn title(self) -> &'static str {
        match self {
            ChartTab::TimeSeries => "Time series",
            ChartTab::Tasks => "Tasks",
            ChartTab::Histogram => "Histogram",
        }
    }
}

/// The pace the user picked.
#[derive(Clone, Copy, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct SpeedSetting {
    /// `false`: as fast as possible.
    pub limited: bool,
    /// Updates per second when limited.
    pub ups: f64,
}

impl Default for SpeedSetting {
    fn default() -> Self {
        SpeedSetting {
            limited: false,
            ups: 10.0,
        }
    }
}

impl SpeedSetting {
    pub const MIN_UPS: f64 = 0.5;
    pub const MAX_UPS: f64 = 2000.0;

    pub fn speed(self) -> Speed {
        if self.limited {
            Speed::UpdatesPerSecond(self.ups.clamp(Self::MIN_UPS, Self::MAX_UPS))
        } else {
            Speed::Max
        }
    }

    pub fn label(self) -> String {
        if self.limited {
            format::rate(self.ups)
        } else {
            "Max speed".to_string()
        }
    }
}

/// Something only the application shell can do.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Request {
    /// Show the native folder picker, then open the folder.
    PickFolder,
    /// Ask where to put a new experiment, write Avida's default
    /// configuration there and open it.
    NewExperiment,
    /// Write Avida's default configuration into this folder (created if
    /// missing; nothing is overwritten) and open it.
    CreateExperimentIn(PathBuf),
    /// Open this experiment folder.
    Open(PathBuf),
    /// Close the experiment and return to the start screen.
    Close,
    /// Save the current map as PNG (asks where).
    ExportMap,
    /// Save the accumulated rows as CSV (asks where).
    ExportCsv,
    /// Save the charts tab as it looks now as PNG (asks where).
    ExportCharts,
    /// Save the genotype table as CSV (asks where).
    ExportGenotypes,
    /// Put the log on the clipboard.
    CopyLog,
    /// Build the world of this experiment again (new arguments, new
    /// update limit), replacing the current session.
    Restart(Experiment),
    /// Put this text on the clipboard.
    Copy(String),
}

/// Where inside an update a frame was captured.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FrameProgress {
    /// The update in progress.
    pub update: i32,
    pub slices_done: i32,
    pub slices_total: i32,
    /// The update the frame's statistics belong to (the last finished one).
    pub stats_update: i32,
}

/// Updates per second, from when rows arrive.
#[derive(Clone, Debug, Default)]
pub struct RateMeter {
    samples: VecDeque<(f64, i32)>,
}

impl RateMeter {
    /// Window over which the rate is averaged, in seconds.
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

    /// The rate over the window, `None` when fewer than two samples are
    /// recent (paused, or just started).
    pub fn rate(&self, now: f64) -> Option<f64> {
        let &(t1, u1) = self.samples.back()?;
        let &(t0, u0) = self.samples.front()?;
        if now - t1 > 1.0 || t1 <= t0 {
            return None;
        }
        Some(f64::from(u1 - u0) / (t1 - t0))
    }

    pub fn clear(&mut self) {
        self.samples.clear();
    }
}

/// The application state.
#[derive(Debug)]
pub struct AppState {
    pub phase: Phase,
    pub experiment: Option<Experiment>,
    /// Why the last attempt to open an experiment failed (what avida
    /// said).
    pub open_error: Option<String>,
    /// The folder that could not be opened.
    pub open_error_dir: Option<PathBuf>,

    pub status: Option<Status>,
    pub frame: Option<Arc<Frame>>,
    /// Frames applied so far (for change detection).
    pub frames_applied: u64,
    pub history: RowHistory,
    pub log: EventLog,
    pub toasts: Vec<Toast>,
    /// The error that stopped the run.
    pub fatal: Option<String>,
    /// What avida printed on stderr (and `error:` lines on stdout) during
    /// the update that failed, shown with the error.
    pub fatal_context: Vec<String>,
    /// The status the `avida` binary would exit with, once the run is
    /// over (see `Session::exit_code`).
    pub exit_code: Option<i32>,
    /// Problem lines printed since the last finished update (the context
    /// of a failure).
    recent_problems: Vec<String>,
    /// The effective configuration and events of the open experiment.
    pub setup: Option<Arc<ExperimentSetup>>,
    pub rate: RateMeter,
    /// Cells per top genotype, frame by frame.
    pub abundance: AbundanceHistory,

    pub map: MapSettings,
    pub viewport: Viewport,
    /// The rendered map is out of date.
    pub map_dirty: bool,
    /// The inspected cell.
    pub selection: Option<i32>,
    /// The genotype whose cells the map emphasises.
    pub highlight: Option<i32>,

    pub speed: SpeedSetting,
    /// Text of the run-to-update field.
    pub run_to_input: String,
    pub side_tab: SideTab,
    pub chart_tab: ChartTab,
    pub chart_metrics: Vec<Metric>,
    pub geno_sort: GenoSort,
    pub show_shortcuts: bool,
    pub show_log: bool,
    pub log_filter: LogFilter,
    /// The organism debugger (the organism tab).
    pub debugger: Debugger,
    /// The command palette (Cmd/Ctrl+K).
    pub command_palette: PaletteState,
    /// The config tab: search and the restart form.
    pub config_view: ConfigView,
    /// Move the keyboard focus to the run-to field (next frame).
    pub focus_run_to: bool,
    /// Where the charts were drawn in the last frame (screen points), for
    /// saving them as an image; `None` when they were not on screen.
    pub chart_area: Option<egui::Rect>,
    /// Set while the session plays a recording (`--play FILE`).
    pub playback: Option<PlaybackInfo>,
    /// The record a seek asked for that the session has not shown yet.
    pub seek_target: Option<usize>,
    /// How the recording of the live run is going (`--record FILE`).
    pub recording: Option<RecordingStatus>,
    /// The experiment of a `Restart` sent and not yet answered. Until the
    /// session answers, commands are not sent and rows and frames (which
    /// may belong to either run) are not shown.
    pub restarting: Option<Experiment>,

    /// `egui` input time of the current frame.
    pub now: f64,
    commands: Vec<Command>,
    requests: Vec<Request>,
    /// The run was replaced in the same session since the last
    /// [`AppState::take_run_reset`].
    run_reset: bool,
}

impl Default for AppState {
    fn default() -> Self {
        AppState {
            phase: Phase::Start,
            experiment: None,
            open_error: None,
            open_error_dir: None,
            status: None,
            frame: None,
            frames_applied: 0,
            history: RowHistory::default(),
            log: EventLog::default(),
            toasts: Vec::new(),
            fatal: None,
            fatal_context: Vec::new(),
            exit_code: None,
            recent_problems: Vec::new(),
            setup: None,
            rate: RateMeter::default(),
            abundance: AbundanceHistory::default(),
            map: MapSettings::default(),
            viewport: Viewport::default(),
            map_dirty: true,
            selection: None,
            highlight: None,
            speed: SpeedSetting::default(),
            run_to_input: String::new(),
            side_tab: SideTab::default(),
            chart_tab: ChartTab::default(),
            chart_metrics: Metric::DEFAULT_CHARTS.to_vec(),
            geno_sort: GenoSort::default(),
            show_shortcuts: false,
            show_log: false,
            log_filter: LogFilter::default(),
            debugger: Debugger::default(),
            command_palette: PaletteState::default(),
            config_view: ConfigView::default(),
            focus_run_to: false,
            chart_area: None,
            playback: None,
            seek_target: None,
            recording: None,
            restarting: None,
            now: 0.0,
            commands: Vec::new(),
            requests: Vec::new(),
            run_reset: false,
        }
    }
}

impl AppState {
    // --- lifecycle --------------------------------------------------------

    /// An experiment is being opened: forget the previous run.
    pub fn begin_opening(&mut self, experiment: Experiment) {
        self.reset_run();
        self.log.clear();
        self.log.push(
            LogKind::Info,
            format!("Opening {}", experiment.dir.display()),
        );
        self.experiment = Some(experiment);
        self.open_error = None;
        self.open_error_dir = None;
        self.phase = Phase::Opening;
    }

    /// The session exists: ask for the first frame of the chosen layer.
    pub fn opened(&mut self) {
        self.phase = Phase::Ready;
        self.commands.push(Command::SetSpeed(self.speed.speed()));
        self.subscribe();
    }

    /// The effective configuration of the session just opened.
    pub fn set_setup(&mut self, setup: Arc<ExperimentSetup>) {
        let limit = self.experiment.as_ref().and_then(|e| e.update_limit);
        self.config_view.reset(&setup, limit);
        self.setup = Some(setup);
    }

    /// Building the world failed; back to the start screen.
    pub fn open_failed(&mut self, error: String) {
        self.log.push(LogKind::Fatal, error.clone());
        self.phase = Phase::Start;
        self.open_error_dir = self.experiment.take().map(|e| e.dir);
        self.open_error = Some(error);
    }

    /// Why the last folder could not be opened, in words for the start
    /// screen.
    pub fn open_problem(&self) -> Option<OpenProblem> {
        let raw = self.open_error.as_deref()?;
        Some(OpenProblem::explain(self.open_error_dir.as_deref(), raw))
    }

    /// Forgets the last opening error (the user dismissed it).
    pub fn dismiss_open_error(&mut self) {
        self.open_error = None;
        self.open_error_dir = None;
    }

    /// Back to the start screen.
    pub fn closed(&mut self) {
        self.reset_run();
        self.phase = Phase::Start;
        self.experiment = None;
    }

    /// The session was asked to restart with `experiment` (same folder,
    /// other arguments).
    pub fn begin_restart(&mut self, experiment: Experiment) {
        self.commands.clear();
        self.restarting = Some(experiment);
    }

    /// Whether the run was replaced in the same session since the last
    /// call (caches built from the old run's rows and frames are stale).
    pub fn take_run_reset(&mut self) -> bool {
        std::mem::take(&mut self.run_reset)
    }

    fn reset_run(&mut self) {
        self.restarting = None;
        self.status = None;
        self.frame = None;
        self.history.clear();
        self.toasts.clear();
        self.fatal = None;
        self.fatal_context.clear();
        self.exit_code = None;
        self.recent_problems.clear();
        self.setup = None;
        self.rate.clear();
        self.abundance.clear();
        self.viewport = Viewport::default();
        self.map_dirty = true;
        self.selection = None;
        self.highlight = None;
        // A new session follows nobody; display choices stay.
        self.debugger = self.debugger.for_new_session();
        self.command_palette.close();
        self.commands.clear();
        // The layer may not exist in the next world.
        if matches!(self.map.layer, LayerId::Task(_) | LayerId::Resource(_)) {
            self.map.layer = LayerId::Genotype;
        }
    }

    pub fn is_ready(&self) -> bool {
        self.phase == Phase::Ready
    }

    pub fn set_time(&mut self, now: f64) {
        self.now = now;
        self.toasts.retain(|t| t.expires > now);
    }

    // --- what the session published --------------------------------------

    pub fn apply_status(&mut self, status: Status) {
        self.status = Some(status);
    }

    pub fn apply_frame(&mut self, frame: Arc<Frame>) {
        if self.restarting.is_some() {
            return;
        }
        if let Some(cell) = self.selection {
            self.debugger.observe(cell, &frame);
        }
        self.abundance.record(&frame);
        self.frame = Some(frame);
        self.frames_applied += 1;
        self.map_dirty = true;
    }

    /// Where the playback stands; `None` for a simulation.
    pub fn apply_playback(&mut self, info: Option<PlaybackInfo>) {
        if let (Some(target), Some(i)) = (self.seek_target, &info) {
            if i.index == target {
                self.seek_target = None;
            }
        }
        self.playback = info;
    }

    /// How the recording of the live run is going.
    pub fn apply_recording(&mut self, status: Option<RecordingStatus>) {
        self.recording = status;
    }

    /// A playback went back in time: the rows that follow are the whole
    /// history up to the frame now shown, so what was built from the rows
    /// before is stale.
    pub fn rows_reset(&mut self) {
        self.history.clear();
        self.abundance.clear();
        self.rate.clear();
        self.run_reset = true;
    }

    pub fn is_playback(&self) -> bool {
        self.playback.is_some()
    }

    /// Why `what` does nothing on a recording: a toast, and true.
    fn refuse_in_playback(&mut self, what: &str) -> bool {
        if self.playback.is_none() {
            return false;
        }
        self.toast(
            ToastKind::Info,
            format!("{what} needs a simulation; a playback only shows what was recorded."),
        );
        true
    }

    /// Shows record `index` of a playback (clamped by the session).
    pub fn seek_frame(&mut self, index: usize) {
        if self.playback.is_none() {
            return;
        }
        if self.seek_target == Some(index) {
            return;
        }
        self.seek_target = Some(index);
        self.send(Command::SeekFrame(index));
    }

    /// Moves a playback by `delta` records from where the last seek (or the
    /// frame on show) is.
    pub fn seek_frames_by(&mut self, delta: i64) {
        let Some(info) = &self.playback else {
            return;
        };
        let here = self.seek_target.unwrap_or(info.index) as i64;
        let last = info.frames.saturating_sub(1) as i64;
        self.seek_frame((here + delta).clamp(0, last) as usize);
    }

    pub fn apply_rows(&mut self, rows: Vec<UpdateRow>) {
        if self.restarting.is_some() {
            return;
        }
        if let Some(last) = rows.last() {
            self.rate.record(self.now, last.update);
            self.recent_problems.clear();
        }
        self.history.extend(rows);
    }

    pub fn apply_events(&mut self, events: Vec<SessionEvent>) {
        for event in events {
            self.apply_event(event);
        }
    }

    fn apply_event(&mut self, event: SessionEvent) {
        match event {
            SessionEvent::Running | SessionEvent::Paused { .. } => {}
            // The banner over the map says so; no toast on top of it.
            SessionEvent::Finished { update } => {
                let text = format!("The run finished at {}.", format::update_phrase(update));
                self.log.push(LogKind::Info, text);
            }
            SessionEvent::Fatal(e) => {
                // Avida often prints the error itself first, labelled
                // "error:"; the log and the banner show it once.
                self.log.push_fatal(e.clone());
                self.fatal_context = std::mem::take(&mut self.recent_problems)
                    .into_iter()
                    .filter(|l| !avida_viewmodel::problem::repeats_fatal(l, &e))
                    .collect();
                let excess = self.fatal_context.len().saturating_sub(FATAL_CONTEXT_LINES);
                self.fatal_context.drain(..excess);
                self.fatal = Some(e);
            }
            SessionEvent::Error(e) => {
                self.log.push(LogKind::Error, e.clone());
                self.toast(ToastKind::Error, e);
            }
            SessionEvent::PopulationSaved(path) => {
                self.log.push(
                    LogKind::Info,
                    format!("Population saved to {}", path.display()),
                );
                let name = path
                    .file_name()
                    .map(|n| n.to_string_lossy().into_owned())
                    .unwrap_or_else(|| path.display().to_string());
                self.toast(ToastKind::Success, format!("Population saved as {name}"));
            }
            SessionEvent::Stdout(line) => {
                if line.trim_start().starts_with("UD:") {
                    // Avida's per-update line: what came before belongs
                    // to an update that finished.
                    self.recent_problems.clear();
                } else if is_error_line(&line) {
                    self.note_problem(&line);
                }
                self.log.push(LogKind::Output, line);
            }
            SessionEvent::Stderr(line) => {
                self.note_problem(&line);
                self.log.push_stderr(line);
            }
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
                // The session moved the watch already: no WatchCell.
                if self.selection == Some(from) || self.selection.is_none() {
                    self.selection = Some(to);
                    self.debugger.moved(org_id);
                }
                let (x, y) = self.cell_xy(to);
                let text = if offspring {
                    format!("Following offspring #{org_id} into cell ({x}, {y}).")
                } else {
                    format!("Organism #{org_id} moved to cell ({x}, {y}).")
                };
                self.log.push(LogKind::Info, text);
            }
            SessionEvent::Born {
                parent_id,
                org_id,
                cell,
                update,
                ..
            } => {
                let (x, y) = self.cell_xy(cell);
                self.log.push(
                    LogKind::Info,
                    format!(
                        "Organism #{parent_id} gave birth to #{org_id} in cell ({x}, {y}) at {}.",
                        format::update_phrase(update)
                    ),
                );
            }
            SessionEvent::Restarted(setup) => {
                let experiment = self.restarting.take().or_else(|| self.experiment.clone());
                let note = match &experiment {
                    Some(e) if !e.args.is_empty() => {
                        format!("Restarted with {}", e.args.join(" "))
                    }
                    _ => "Restarted the experiment.".to_string(),
                };
                self.reset_run();
                self.experiment = experiment;
                self.phase = Phase::Ready;
                self.set_setup(setup);
                self.opened();
                self.run_reset = true;
                self.notify(ToastKind::Info, note);
            }
            SessionEvent::RestartFailed(e) => {
                self.restarting = None;
                self.notify(ToastKind::Error, format!("Restart failed: {e}"));
            }
            SessionEvent::RecordingStarted(path) => {
                self.notify(ToastKind::Info, format!("Recording to {}.", path.display()));
            }
            SessionEvent::RecordingStopped { frames, bytes } => self.log.push(
                LogKind::Info,
                format!(
                    "The recording is complete: {} frames, {}.",
                    format::count(i64::try_from(frames).unwrap_or(i64::MAX)),
                    format::bytes(bytes)
                ),
            ),
        }
    }

    /// Remembers a problem line printed during the update in progress.
    fn note_problem(&mut self, line: &str) {
        if line.trim().is_empty() {
            return;
        }
        self.recent_problems.push(line.to_string());
        let excess = self
            .recent_problems
            .len()
            .saturating_sub(4 * FATAL_CONTEXT_LINES);
        self.recent_problems.drain(..excess);
    }

    // --- derived ----------------------------------------------------------

    /// How the run ended, once it is over.
    pub fn run_end(&self) -> Option<RunEnd> {
        match self.run_state()? {
            RunState::Failed => {
                let update = self
                    .frame
                    .as_ref()
                    .map_or(-1, |f| f.update.max(f.header.last_finished_update));
                Some(RunEnd {
                    failed: true,
                    title: format!(
                        "The run stopped on an error during {}.",
                        format::update_phrase(update.max(0))
                    ),
                    reason: String::new(),
                    exit_code: self.exit_code,
                })
            }
            RunState::Finished => {
                let update = self.current_update().unwrap_or(-1);
                let died_out = self.organisms() == Some(0);
                let exit = self.setup.as_ref().and_then(|s| {
                    s.events
                        .iter()
                        .find(|e| e.action.starts_with("Exit"))
                        .map(|e| e.action.clone())
                });
                let reason = if died_out {
                    "The population died out, so there is nothing left to run.".to_string()
                } else if let Some(action) = exit {
                    format!(
                        "The {action} event in the events file ended it. The last frame stays on screen to inspect and export."
                    )
                } else {
                    "The last frame stays on screen to inspect and export.".to_string()
                };
                Some(RunEnd {
                    failed: false,
                    title: format!("The run finished at {}.", format::update_phrase(update)),
                    reason,
                    exit_code: self.exit_code.or(Some(0)),
                })
            }
            _ => None,
        }
    }

    /// The run state, `None` without a session.
    pub fn run_state(&self) -> Option<RunState> {
        if self.phase != Phase::Ready {
            return None;
        }
        if self.fatal.is_some() {
            return Some(RunState::Failed);
        }
        self.status.as_ref().map(|s| s.state)
    }

    /// Whether stepping and running can do anything.
    pub fn can_advance(&self) -> bool {
        matches!(self.run_state(), Some(RunState::Paused | RunState::Running))
    }

    /// The update in progress, or the last finished one between updates.
    pub fn current_update(&self) -> Option<i32> {
        self.status
            .as_ref()
            .map(|s| s.update)
            .or_else(|| self.frame.as_ref().map(|f| f.update))
    }

    /// `Some((update, slices run, slices in the update))` while paused or
    /// running inside an update (the live session status).
    pub fn in_progress(&self) -> Option<(i32, i32, i32)> {
        let s = self.status.as_ref()?;
        s.slice.map(|(done, total)| (s.update, done, total))
    }

    /// Whether the frame on screen was captured inside an update, and
    /// where: what labels describing the shown content go by.
    pub fn frame_progress(&self) -> Option<FrameProgress> {
        let f = self.frame.as_ref()?;
        let p = f.header.progress;
        p.in_update.then_some(FrameProgress {
            update: f.update,
            slices_done: p.slices_done,
            slices_total: p.slices_total,
            stats_update: f.header.last_finished_update,
        })
    }

    /// Instruction names of the default instruction set, by opcode.
    pub fn inst_names(&self) -> Option<&[String]> {
        self.frame
            .as_ref()
            .map(|f| &f.inst_names[..])
            .filter(|n| !n.is_empty())
    }

    /// The id of the dominant genotype.
    pub fn dominant_id(&self) -> Option<i32> {
        self.frame.as_ref()?.dominant.as_ref().map(|d| d.row.id)
    }

    /// `(x, y)` of a cell id.
    pub fn cell_xy(&self, cell: i32) -> (i32, i32) {
        match self.world_size() {
            Some((w, _)) if w > 0 => (cell % w as i32, cell / w as i32),
            _ => (cell, 0),
        }
    }

    /// The window title: experiment and run state.
    pub fn window_title(&self) -> String {
        let Some(e) = &self.experiment else {
            return "Avida".to_string();
        };
        let state = match (self.phase.clone(), self.run_state()) {
            (Phase::Opening, _) => "opening".to_string(),
            (_, Some(RunState::Running)) => "running".to_string(),
            (_, Some(RunState::Paused)) => match self.current_update() {
                Some(u) if u >= 0 => format!("paused at update {}", format::count(i64::from(u))),
                _ => "paused".to_string(),
            },
            (_, Some(RunState::Finished)) => "finished".to_string(),
            (_, Some(RunState::Failed)) => "failed".to_string(),
            (_, None) => "opening".to_string(),
        };
        format!("{} · {state} · Avida", e.name())
    }

    /// The statistics of the last finished update: the newest row, or the
    /// frame's header before the first row arrives.
    pub fn latest_stats(&self) -> Option<&UpdateRow> {
        self.history
            .latest()
            .or_else(|| self.frame.as_ref().map(|f| &f.header.stats))
    }

    /// Living organisms, from the newest statistics.
    pub fn organisms(&self) -> Option<i32> {
        self.latest_stats().map(|r| r.orgs)
    }

    /// The layers of the current world.
    pub fn layers(&self) -> Vec<LayerId> {
        match &self.frame {
            Some(f) if !f.layers.is_empty() => f.layers.clone(),
            _ => LayerId::FIXED.to_vec(),
        }
    }

    pub fn subscription(&self) -> Subscription {
        Subscription {
            layers: vec![self.map.layer],
            inspect_cell: self.selection,
            genotypes_top_n: GENOTYPE_LIST_LEN,
            lineage_of: self.highlight,
        }
    }

    /// The world size, from the frame.
    pub fn world_size(&self) -> Option<(usize, usize)> {
        let h = &self.frame.as_ref()?.header;
        Some((
            usize::try_from(h.world_x).ok()?,
            usize::try_from(h.world_y).ok()?,
        ))
    }

    // --- user actions -----------------------------------------------------

    fn send(&mut self, cmd: Command) {
        if self.phase == Phase::Ready && self.restarting.is_none() {
            self.commands.push(cmd);
        }
    }

    fn refuse_if_over(&mut self) -> bool {
        match self.run_state() {
            Some(RunState::Finished) => {
                self.toast(
                    ToastKind::Info,
                    "The run has finished; nothing can advance it.".to_string(),
                );
                true
            }
            Some(RunState::Failed) => {
                self.toast(
                    ToastKind::Error,
                    "The run stopped on an error; nothing can advance it.".to_string(),
                );
                true
            }
            None => true,
            _ => false,
        }
    }

    /// Run when paused, pause when running. With an update limit not yet
    /// reached, Run stops after it.
    pub fn toggle_run(&mut self) {
        if self.refuse_if_over() {
            return;
        }
        if self.run_state() == Some(RunState::Running) {
            self.send(Command::Pause);
            return;
        }
        let finished = self
            .frame
            .as_ref()
            .map_or(-1, |f| f.header.last_finished_update);
        match self.update_limit() {
            Some(limit) if finished < limit => self.send(Command::RunToUpdate(limit)),
            _ => self.send(Command::Run),
        }
    }

    /// The update limit of the open experiment.
    pub fn update_limit(&self) -> Option<i32> {
        self.experiment.as_ref().and_then(|e| e.update_limit)
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

    pub fn run_to(&mut self, update: i32) {
        if !self.refuse_if_over() {
            self.send(Command::RunToUpdate(update));
        }
    }

    /// Reads the run-to field and runs to that update.
    pub fn commit_run_to(&mut self) -> bool {
        let text = self.run_to_input.trim().replace([',', '_', ' '], "");
        match text.parse::<i32>() {
            Ok(n) if n >= 0 => {
                self.run_to(n);
                true
            }
            _ => {
                self.toast(
                    ToastKind::Error,
                    format!("\"{}\" is not an update number.", self.run_to_input.trim()),
                );
                false
            }
        }
    }

    pub fn set_speed(&mut self, speed: SpeedSetting) {
        self.speed = speed;
        self.send(Command::SetSpeed(speed.speed()));
    }

    pub fn save_population(&mut self) {
        if self.refuse_in_playback("Saving the population") {
            return;
        }
        match self.run_state() {
            None => {}
            Some(RunState::Failed) => self.toast(
                ToastKind::Error,
                "A failed run's population cannot be saved.".to_string(),
            ),
            Some(_) => self.send(Command::SavePopulation),
        }
    }

    pub fn set_layer(&mut self, layer: LayerId) {
        if self.map.layer != layer {
            self.map.layer = layer;
            self.map_dirty = true;
            self.subscribe();
        }
    }

    /// Selects layer `n` (0-based) of the world's layers.
    pub fn set_layer_index(&mut self, n: usize) {
        if let Some(&layer) = self.layers().get(n) {
            self.set_layer(layer);
        }
    }

    /// Inspects a cell (or nothing) and shows the inspector. A new cell
    /// starts a new debugging session: breakpoints are cleared.
    pub fn select_cell(&mut self, cell: Option<i32>) {
        if self.selection == cell {
            return;
        }
        if cell.is_some() && self.refuse_in_playback("Inspecting an organism") {
            return;
        }
        self.selection = cell;
        self.debugger.reset(true);
        self.send(Command::WatchCell(cell));
        if cell.is_some() {
            self.side_tab = SideTab::Organism;
        }
    }

    /// Emphasises a genotype's cells on the map (and traces its lineage),
    /// or stops doing so.
    pub fn toggle_highlight(&mut self, genotype: i32) {
        let next = if self.highlight == Some(genotype) {
            None
        } else {
            Some(genotype)
        };
        self.set_highlight(next);
    }

    pub fn set_highlight(&mut self, genotype: Option<i32>) {
        if self.highlight != genotype {
            self.highlight = genotype;
            self.map_dirty = true;
            self.subscribe();
        }
    }

    pub fn clear_highlight(&mut self) {
        self.set_highlight(None);
    }

    /// Selects an organism of the dominant genotype.
    pub fn inspect_dominant(&mut self) {
        let cell = self.dominant_id().and_then(|id| {
            self.frame
                .as_ref()
                .and_then(|f| f.map.as_ref())
                .and_then(|m| m.first_cell_of(id))
        });
        match cell {
            Some(c) => {
                self.select_cell(Some(c));
                self.side_tab = SideTab::Organism;
            }
            None => self.toast(
                ToastKind::Info,
                "The dominant genotype has no organism on the map.".to_string(),
            ),
        }
    }

    // --- the debugger -----------------------------------------------------

    /// Runs the watched organism for one instruction.
    pub fn debug_step(&mut self) {
        if self.refuse_if_over() {
            return;
        }
        let Some(cell) = self.selection else {
            return;
        };
        match self.debugger.step_command(cell, 1) {
            Some(cmd) => self.send(Command::RunUntil(cmd)),
            None => self.toast(
                ToastKind::Info,
                "The watched organism is gone; select another one to step.".to_string(),
            ),
        }
    }

    /// Runs until the watched organism's IP stands on a breakpoint.
    pub fn debug_run_to_breakpoint(&mut self) {
        if self.refuse_if_over() {
            return;
        }
        let Some(cell) = self.selection else {
            return;
        };
        if self.debugger.breakpoints.is_empty() {
            self.toast(
                ToastKind::Info,
                "Set a breakpoint first: click a position in the memory listing.".to_string(),
            );
            return;
        }
        match self.debugger.breakpoint_command(cell) {
            Some(cmd) => self.send(Command::RunUntil(cmd)),
            None => self.toast(
                ToastKind::Info,
                "The watched organism is gone; select another one to debug.".to_string(),
            ),
        }
    }

    /// Whether the session moves the watch to each offspring.
    pub fn set_follow_offspring(&mut self, on: bool) {
        self.debugger.follow_offspring = on;
        self.send(Command::FollowOffspring(on));
    }

    /// Watches the organism that replaced the watched one.
    pub fn watch_occupant(&mut self) {
        self.debugger.watch_occupant();
    }

    // --- experiment -------------------------------------------------------

    /// Builds the world again: with the same arguments (`None`) or another
    /// set.
    pub fn restart(&mut self, with: Option<Experiment>) {
        if self.refuse_in_playback("Restarting") {
            return;
        }
        let Some(exp) = with.or_else(|| self.experiment.clone()) else {
            return;
        };
        self.request(Request::Restart(exp));
    }

    /// Previews the map as a colour-vision deficiency shows it.
    pub fn set_vision(&mut self, vision: ColorVision) {
        if self.map.vision != vision {
            self.map.vision = vision;
            self.map_dirty = true;
        }
    }

    /// Re-sends the subscription (layer or selection changed).
    pub fn subscribe(&mut self) {
        let sub = self.subscription();
        self.send(Command::Subscribe(sub));
    }

    pub fn request(&mut self, request: Request) {
        self.requests.push(request);
    }

    pub fn toast(&mut self, kind: ToastKind, text: String) {
        self.toasts.retain(|t| t.text != text);
        self.toasts.push(Toast {
            kind,
            text,
            expires: self.now + kind.duration(),
        });
        let excess = self.toasts.len().saturating_sub(MAX_TOASTS);
        self.toasts.drain(..excess);
    }

    /// Logs and announces something the shell did.
    pub fn notify(&mut self, kind: ToastKind, text: String) {
        let log_kind = match kind {
            ToastKind::Error => LogKind::Error,
            _ => LogKind::Info,
        };
        self.log.push(log_kind, text.clone());
        self.toast(kind, text);
    }

    pub fn take_commands(&mut self) -> Vec<Command> {
        std::mem::take(&mut self.commands)
    }

    pub fn take_requests(&mut self) -> Vec<Request> {
        std::mem::take(&mut self.requests)
    }

    /// Commands queued and not taken yet (tests).
    pub fn pending_commands(&self) -> &[Command] {
        &self.commands
    }
}

#[cfg(test)]
mod tests {
    use avida_session::Speed;

    use super::fixtures::{frame_with_map, status};
    use super::*;

    fn ready() -> AppState {
        let mut s = AppState::default();
        s.begin_opening(Experiment {
            dir: PathBuf::from("/x/heads/config"),
            args: Vec::new(),
            update_limit: None,
        });
        s.opened();
        s
    }

    fn playing() -> AppState {
        let mut s = ready();
        s.apply_status(status(RunState::Paused, 100));
        s.apply_frame(Arc::new(frame_with_map()));
        s.apply_playback(Some(PlaybackInfo {
            path: PathBuf::from("/x/run.avrec"),
            frames: 6,
            index: 2,
            update: 100,
            first_update: 0,
            last_update: 500,
            cut_tail_bytes: 0,
            layers: vec![LayerId::Genotype, LayerId::Fitness],
        }));
        s.take_commands();
        s
    }

    #[test]
    fn a_playback_seeks_by_frames_and_clamps() {
        let mut s = playing();
        assert!(s.is_playback());
        s.seek_frames_by(2);
        // A second key before the session answers adds to the first.
        s.seek_frames_by(10);
        s.seek_frames_by(-100);
        s.seek_frames_by(3);
        assert_eq!(
            s.take_commands(),
            vec![
                Command::SeekFrame(4),
                Command::SeekFrame(5),
                Command::SeekFrame(0),
                Command::SeekFrame(3)
            ]
        );
        // The session shows the record asked for: the pending seek ends.
        let mut info = s.playback.clone().unwrap();
        info.index = 3;
        s.apply_playback(Some(info));
        assert_eq!(s.seek_target, None);
        // The same target twice is one command.
        s.seek_frame(1);
        s.seek_frame(1);
        assert_eq!(s.take_commands(), vec![Command::SeekFrame(1)]);
    }

    #[test]
    fn what_needs_a_simulation_is_refused_in_a_playback() {
        let mut s = playing();
        s.save_population();
        s.select_cell(Some(0));
        s.restart(None);
        assert!(s.take_commands().is_empty());
        assert!(s.take_requests().is_empty());
        assert_eq!(s.selection, None);
        assert_eq!(s.toasts.len(), 3);
        // A simulation does all three.
        let mut live = ready();
        live.apply_status(status(RunState::Paused, 3));
        live.take_commands();
        live.save_population();
        assert_eq!(live.take_commands(), vec![Command::SavePopulation]);
    }

    #[test]
    fn going_back_in_a_playback_restarts_the_history() {
        let mut s = playing();
        s.apply_rows(vec![UpdateRow::default(), UpdateRow::default()]);
        assert_eq!(s.history.len(), 2);
        s.rows_reset();
        assert_eq!(s.history.len(), 0);
        assert!(s.take_run_reset());
        s.apply_rows(vec![UpdateRow::default()]);
        assert_eq!(s.history.len(), 1);
    }

    #[test]
    fn the_layers_of_a_playback_are_the_recorded_ones() {
        let mut f = frame_with_map();
        f.layers = vec![LayerId::Genotype, LayerId::Fitness];
        let mut s = playing();
        s.apply_frame(Arc::new(f));
        assert_eq!(s.layers(), vec![LayerId::Genotype, LayerId::Fitness]);
    }

    #[test]
    fn opening_subscribes_to_the_chosen_layer() {
        let mut s = ready();
        assert_eq!(s.experiment.as_ref().unwrap().name(), "heads / config");
        let cmds = s.take_commands();
        assert_eq!(cmds[0], Command::SetSpeed(Speed::Max));
        match &cmds[1] {
            Command::Subscribe(sub) => {
                assert_eq!(sub.layers, vec![LayerId::Genotype]);
                assert_eq!(sub.inspect_cell, None);
                assert_eq!(sub.genotypes_top_n, GENOTYPE_LIST_LEN);
            }
            c => panic!("{c:?}"),
        }
    }

    #[test]
    fn a_restart_in_the_session_waits_for_its_answer() {
        let mut s = ready();
        s.take_commands();
        s.apply_frame(Arc::new(frame_with_map()));
        s.apply_events(vec![SessionEvent::Born {
            parent_id: 1,
            parent_cell: 0,
            org_id: 2,
            cell: 1,
            update: 3,
            slice: 40,
        }]);
        assert!(s
            .log
            .entries()
            .any(|e| e.text.starts_with("Organism #1 gave birth to #2")));

        let next = Experiment {
            dir: PathBuf::from("/x/heads/config"),
            args: vec!["-s".into(), "7".into()],
            update_limit: Some(5),
        };
        s.begin_restart(next.clone());
        s.toggle_run();
        assert!(s.take_commands().is_empty(), "nothing goes to either run");
        let frames = s.frames_applied;
        s.apply_frame(Arc::new(frame_with_map()));
        assert_eq!(s.frames_applied, frames);

        s.apply_events(vec![SessionEvent::RestartFailed("error: no".into())]);
        assert!(s.restarting.is_none());
        assert_eq!(s.experiment.as_ref().unwrap().args, Vec::<String>::new());
        assert!(s.toasts.iter().any(|t| t.text.contains("Restart failed")));

        s.begin_restart(next.clone());
        let setup = Arc::new(ExperimentSetup {
            work_dir: PathBuf::from("/x/heads/config"),
            args: next.args.clone(),
            data_dir: PathBuf::from("/x/heads/config/data"),
            config_file: PathBuf::from("/x/heads/config/avida.cfg"),
            settings: Vec::new(),
            event_file: PathBuf::from("/x/heads/config/events.cfg"),
            events: Vec::new(),
        });
        s.apply_events(vec![SessionEvent::Restarted(Arc::clone(&setup))]);
        assert!(s.take_run_reset() && !s.take_run_reset());
        assert!(s.restarting.is_none() && s.frame.is_none() && s.is_ready());
        assert_eq!(s.experiment, Some(next));
        assert_eq!(s.setup, Some(setup));
        let cmds = s.take_commands();
        assert!(cmds.iter().any(|c| matches!(c, Command::Subscribe(_))));
        assert!(cmds.iter().any(|c| matches!(c, Command::SetSpeed(_))));
    }

    #[test]
    fn nothing_is_sent_without_a_session() {
        let mut s = AppState::default();
        s.toggle_run();
        s.set_layer(LayerId::Fitness);
        s.select_cell(Some(3));
        assert!(s.take_commands().is_empty());
        assert_eq!(s.map.layer, LayerId::Fitness);
    }

    #[test]
    fn run_toggles_with_the_state_and_refuses_when_over() {
        let mut s = ready();
        s.take_commands();
        s.apply_status(status(RunState::Paused, 4));
        s.toggle_run();
        assert_eq!(s.take_commands(), vec![Command::Run]);
        s.apply_status(status(RunState::Running, 5));
        s.toggle_run();
        assert_eq!(s.take_commands(), vec![Command::Pause]);
        s.apply_status(status(RunState::Finished, 100));
        s.toggle_run();
        s.step_update();
        assert!(s.take_commands().is_empty());
        assert_eq!(s.toasts.len(), 1, "one toast, deduplicated");
    }

    #[test]
    fn a_fatal_event_fails_the_run_and_keeps_the_frame() {
        let mut s = ready();
        s.apply_status(status(RunState::Running, 7));
        s.apply_frame(Arc::new(frame_with_map()));
        s.apply_events(vec![
            SessionEvent::Stderr("warning: early".into()),
            SessionEvent::Stdout("UD: 7".into()),
        ]);
        s.apply_rows(vec![UpdateRow {
            update: 7,
            ..UpdateRow::default()
        }]);
        s.apply_events(vec![
            SessionEvent::Stdout("warning: from update 8".into()),
            SessionEvent::Stdout("UD: 8".into()),
            SessionEvent::Stderr("error: the cell is gone".into()),
            SessionEvent::Stdout("error: failed to load population".into()),
            SessionEvent::Stderr("unsupported: X".into()),
            SessionEvent::Stderr("error: unsupported: X".into()),
            SessionEvent::Fatal("unsupported: X\nmore".into()),
        ]);
        assert_eq!(s.run_state(), Some(RunState::Failed));
        assert!(s.frame.is_some());
        // An "error:" line on stderr is an error; the one that repeats the
        // fatal message is folded into it.
        assert_eq!(s.log.error_count(), 2);
        assert_eq!(
            s.log
                .entries()
                .filter(|e| e.text.contains("unsupported: X") && e.kind.is_error())
                .count(),
            1
        );
        // The banner says it; no toast repeats it.
        assert!(s.toasts.is_empty());
        // What was printed during the failing update, without the error
        // itself and without what came before.
        assert_eq!(
            s.fatal_context,
            vec![
                "error: the cell is gone".to_string(),
                "error: failed to load population".to_string()
            ]
        );
        s.exit_code = Some(2);
        let end = s.run_end().unwrap();
        assert!(end.failed);
        assert_eq!(end.exit_code, Some(2));
        s.step_slice();
        assert!(s
            .take_commands()
            .iter()
            .all(|c| !matches!(c, Command::StepSlice)));
    }

    #[test]
    fn layer_and_selection_changes_resubscribe() {
        let mut s = ready();
        s.take_commands();
        s.set_layer(LayerId::Merit);
        s.select_cell(Some(42));
        let cmds = s.take_commands();
        assert!(matches!(&cmds[0], Command::Subscribe(sub) if sub.layers == vec![LayerId::Merit]));
        assert_eq!(cmds[1], Command::WatchCell(Some(42)));
        assert_eq!(s.side_tab, SideTab::Organism);
        assert_eq!(s.subscription().inspect_cell, Some(42));
        s.select_cell(Some(42));
        assert!(s.take_commands().is_empty(), "same cell, no command");
    }

    #[test]
    fn layer_shortcuts_index_the_world_layers() {
        let mut s = ready();
        s.apply_frame(Arc::new(frame_with_map()));
        s.set_layer_index(1);
        assert_eq!(s.map.layer, LayerId::Fitness);
        s.set_layer_index(12);
        assert_eq!(s.map.layer, LayerId::Ancestry);
        s.set_layer_index(13);
        assert_eq!(s.map.layer, LayerId::Task(0));
        s.set_layer_index(99);
        assert_eq!(s.map.layer, LayerId::Task(0));
    }

    #[test]
    fn run_to_parses_the_field() {
        let mut s = ready();
        s.apply_status(status(RunState::Paused, 4));
        s.take_commands();
        s.run_to_input = " 1,000 ".into();
        assert!(s.commit_run_to());
        assert_eq!(s.take_commands(), vec![Command::RunToUpdate(1000)]);
        s.run_to_input = "soon".into();
        assert!(!s.commit_run_to());
        assert!(s.take_commands().is_empty());
        assert_eq!(s.toasts.last().unwrap().kind, ToastKind::Error);
    }

    #[test]
    fn stats_come_from_rows_and_the_rate_from_their_arrival() {
        let mut s = ready();
        s.apply_frame(Arc::new(frame_with_map()));
        assert_eq!(s.latest_stats().map(|r| r.update), Some(0));
        s.set_time(10.0);
        s.apply_rows(vec![UpdateRow {
            update: 3,
            orgs: 9,
            ..UpdateRow::default()
        }]);
        s.set_time(11.0);
        s.apply_rows(vec![UpdateRow {
            update: 53,
            orgs: 11,
            ..UpdateRow::default()
        }]);
        assert_eq!(s.organisms(), Some(11));
        assert_eq!(s.rate.rate(11.2), Some(50.0));
        assert_eq!(s.rate.rate(13.0), None, "stale");
    }

    #[test]
    fn in_progress_reads_the_slice() {
        let mut s = ready();
        let mut st = status(RunState::Paused, 8);
        st.slice = Some((3, 30));
        s.apply_status(st);
        assert_eq!(s.in_progress(), Some((8, 3, 30)));
        assert_eq!(s.current_update(), Some(8));
    }

    #[test]
    fn saving_and_population_events_are_announced() {
        let mut s = ready();
        s.apply_status(status(RunState::Paused, 4));
        s.take_commands();
        s.save_population();
        assert_eq!(s.take_commands(), vec![Command::SavePopulation]);
        s.apply_events(vec![SessionEvent::PopulationSaved(PathBuf::from(
            "/tmp/data/detail-4.spop",
        ))]);
        assert_eq!(s.toasts[0].text, "Population saved as detail-4.spop");
    }

    #[test]
    fn toasts_expire_and_are_capped() {
        let mut s = AppState::default();
        for i in 0..10 {
            s.toast(ToastKind::Info, format!("t{i}"));
        }
        assert_eq!(s.toasts.len(), MAX_TOASTS);
        assert_eq!(s.toasts[0].text, "t6");
        s.set_time(100.0);
        assert!(s.toasts.is_empty());
    }

    #[test]
    fn highlight_toggles_and_marks_the_map_dirty() {
        let mut s = AppState {
            map_dirty: false,
            ..AppState::default()
        };
        s.toggle_highlight(5);
        assert_eq!(s.highlight, Some(5));
        assert!(s.map_dirty);
        s.toggle_highlight(5);
        assert_eq!(s.highlight, None);
    }

    #[test]
    fn a_finished_run_says_why_it_ended() {
        let mut s = ready();
        s.apply_status(status(RunState::Finished, 100));
        s.apply_rows(vec![UpdateRow {
            update: 100,
            orgs: 50,
            ..UpdateRow::default()
        }]);
        let end = s.run_end().unwrap();
        assert!(!end.failed);
        assert_eq!(end.title, "The run finished at update 100.");
        assert_eq!(end.exit_code, Some(0));
        s.apply_rows(vec![UpdateRow {
            update: 101,
            orgs: 0,
            ..UpdateRow::default()
        }]);
        assert!(s.run_end().unwrap().reason.contains("died out"));
        s.apply_status(status(RunState::Paused, 100));
        assert!(s.run_end().is_none());
    }

    #[test]
    fn opening_problems_are_explained() {
        let p = OpenProblem::explain(
            Some(Path::new("/no/such/folder")),
            "working directory /no/such/folder does not exist",
        );
        assert_eq!(p.title, "This folder does not exist.");
        assert_eq!(p.create_here.as_deref(), Some(Path::new("/no/such/folder")));
        let p = OpenProblem::explain(None, "error: unable to open file 'avida.cfg'.\nerror: configuration file 'avida.cfg' not found");
        assert_eq!(p.title, "There is no avida.cfg in this folder.");
        let p = OpenProblem::explain(None, "error: unknown/unprocessed task entry 'bogus'");
        assert!(p.title.starts_with("Avida could not build the world"));
        assert_eq!(
            p.detail.as_deref(),
            Some("error: unknown/unprocessed task entry 'bogus'")
        );
        assert!(
            p.create_here.is_none(),
            "a broken configuration is fixed, not replaced"
        );
    }

    #[test]
    fn opening_failures_return_to_the_start_screen() {
        let mut s = AppState::default();
        s.begin_opening(Experiment {
            dir: PathBuf::from("/nowhere"),
            args: Vec::new(),
            update_limit: None,
        });
        assert_eq!(s.phase, Phase::Opening);
        s.open_failed("no avida.cfg".into());
        assert_eq!(s.phase, Phase::Start);
        assert_eq!(s.open_error.as_deref(), Some("no avida.cfg"));
        assert_eq!(s.open_error_dir.as_deref(), Some(Path::new("/nowhere")));
        assert_eq!(
            s.open_problem().unwrap().title,
            "This folder does not exist."
        );
        s.dismiss_open_error();
        assert!(s.open_problem().is_none());
        assert_eq!(display_name(Path::new("/a/b/exp1")), "exp1");
    }
}
