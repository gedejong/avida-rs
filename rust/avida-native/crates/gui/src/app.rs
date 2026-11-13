//! The application shell: owns the session, moves what it publishes into
//! the [`AppState`], lays out the window, and carries out the requests the
//! widgets leave (file dialogs, exports, opening and closing).
//!
//! The UI thread never blocks on the simulation: the world is built on a
//! helper thread (`Session::spawn_with_wake` waits for it), and while it
//! runs, the session's wake callback only asks egui for a repaint; each
//! repaint drains the frame slot, the rows and the events without
//! waiting.

use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, Receiver, TryRecvError};
use std::sync::{Arc, Mutex};

use avida_session::{Command, RecordSpec, Session, SessionConfig};
use egui::{
    ColorImage, Frame, Key, Margin, Modifiers, Panel, TextureHandle, TextureOptions,
    ViewportCommand,
};

use crate::cli::StartupArgs;
use crate::model::export::{
    charts_file_name, crop_rgba, csv_file_name, export_scale, genotypes_csv, genotypes_file_name,
    map_file_name, png_bytes, rows_csv,
};
use crate::model::layers::{render_map, MapImage};
use crate::model::log::{LogKind, ToastKind};
use crate::model::metrics::SeriesCache;
use crate::model::new_experiment::{create_experiment, target_folder};
use crate::model::{dropped_folder, AppState, Experiment, Phase, Request, SideTab};
use crate::settings::{Settings, ThemeChoice, STORAGE_KEY};
use crate::ui::theme::{install_fonts, install_styles, Tokens};
use crate::ui::{command_palette, log_panel, map, overlays, side, start, top_bar};

pub struct AvidaApp {
    state: AppState,
    settings: Settings,
    applied_theme: Option<ThemeChoice>,
    session: Option<Session>,
    opening: Option<Receiver<Result<Session, String>>>,
    /// Arguments from the command line, passed to every world opened.
    avida_args: Vec<String>,
    /// `--record FILE`: recording to start in the next session opened.
    pending_record: Option<RecordSpec>,
    /// The start screen's path field.
    path_input: String,
    map_texture: Option<TextureHandle>,
    map_image: Option<MapImage>,
    series: SeriesCache,
    last_generation: u64,
    ctx: egui::Context,
    /// Native file dialogs (off in headless tests: exports then go to the
    /// experiment's data folder without asking).
    native_dialogs: bool,
    /// The window title last sent.
    title: String,
    /// The first frames' window requests (focus, size).
    window_startup: crate::launch::WindowStartup,
    /// Without native dialogs (tests): the folder every folder dialog
    /// answers with.
    dialog_answer: Option<PathBuf>,
    /// Exports being written on helper threads.
    exports: Vec<ExportJob>,
    /// A PNG of the charts tab on its way (see [`ChartShot`]).
    chart_shot: Option<ChartShot>,
}

/// An export written on a helper thread.
struct ExportJob {
    what: &'static str,
    done: Receiver<Result<PathBuf, String>>,
}

/// Capturing the charts tab: wait for it to be drawn (the command palette
/// or a menu may still cover it), ask the window for a screenshot, crop
/// it to the charts.
enum ChartShot {
    Waiting {
        frames: u8,
        path: PathBuf,
    },
    Requested {
        path: PathBuf,
        image: Arc<Mutex<Option<Arc<ColorImage>>>>,
        /// Frames waited for the screenshot.
        waited: u32,
    },
}

/// Frames to wait for a requested screenshot before giving up.
const SCREENSHOT_PATIENCE: u32 = 120;

impl AvidaApp {
    pub fn new(cc: &eframe::CreationContext<'_>, startup: StartupArgs) -> AvidaApp {
        let ctx = cc.egui_ctx.clone();
        install_fonts(&ctx);
        install_styles(&ctx);
        let settings: Settings = cc
            .storage
            .and_then(|s| eframe::get_value(s, STORAGE_KEY))
            .unwrap_or_default();
        let mut state = AppState::default();
        settings.apply_to(&mut state);
        ctx.set_theme(settings.theme.preference());
        let mut app = AvidaApp {
            state,
            applied_theme: Some(settings.theme),
            settings,
            session: None,
            opening: None,
            avida_args: startup.avida_args,
            pending_record: startup.record,
            path_input: String::new(),
            map_texture: None,
            map_image: None,
            series: SeriesCache::default(),
            last_generation: 0,
            ctx,
            native_dialogs: true,
            title: String::new(),
            window_startup: crate::launch::WindowStartup::default(),
            dialog_answer: None,
            exports: Vec::new(),
            chart_shot: None,
        };
        if let Some(file) = startup.play {
            app.pending_record = None;
            app.open_playback(file);
        } else if let Some(dir) = startup.dir {
            app.open(dir);
        }
        app
    }

    /// Sets `shown` on the first frame (the binary's watchdog waits for it)
    /// and brings the window forward on the first few.
    pub fn with_window_startup(
        mut self,
        shown: std::sync::Arc<std::sync::atomic::AtomicBool>,
    ) -> AvidaApp {
        self.window_startup = crate::launch::WindowStartup::new(Some(shown));
        self
    }

    /// Turns native file dialogs off (headless tests).
    pub fn without_native_dialogs(mut self) -> AvidaApp {
        self.native_dialogs = false;
        self
    }

    /// Without native dialogs: answer every folder dialog with `dir`.
    pub fn answer_folder_dialogs_with(&mut self, dir: PathBuf) {
        self.dialog_answer = Some(dir);
    }

    /// Exports still being written.
    pub fn exports_pending(&self) -> usize {
        self.exports.len() + usize::from(self.chart_shot.is_some())
    }

    pub fn state(&self) -> &AppState {
        &self.state
    }

    pub fn state_mut(&mut self) -> &mut AppState {
        &mut self.state
    }

    pub fn settings(&self) -> &Settings {
        &self.settings
    }

    pub fn settings_mut(&mut self) -> &mut Settings {
        &mut self.settings
    }

    /// The image the map shows now.
    pub fn map_image(&self) -> Option<&MapImage> {
        self.map_image.as_ref()
    }

    /// The window title the application wants now.
    pub fn title(&self) -> String {
        self.state.window_title()
    }

    /// Opens an experiment folder with the command-line arguments: forgets
    /// the current session and builds the new world on a helper thread.
    pub fn open(&mut self, dir: PathBuf) {
        let experiment = Experiment {
            dir,
            args: self.avida_args.clone(),
            update_limit: None,
        };
        self.open_experiment(experiment);
    }

    /// [`AvidaApp::open`] with its own arguments and update limit (a
    /// restart).
    pub fn open_experiment(&mut self, experiment: Experiment) {
        self.session = None;
        self.opening = None;
        let dir = std::path::absolute(&experiment.dir).unwrap_or(experiment.dir.clone());
        let args = experiment.args.clone();
        self.state.begin_opening(Experiment {
            dir: dir.clone(),
            ..experiment
        });
        self.map_texture = None;
        self.map_image = None;
        self.series.clear();
        self.last_generation = 0;

        let config = SessionConfig {
            work_dir: dir,
            args,
        };
        self.spawn_opening(move |wake| Session::spawn_with_wake(config, wake));
    }

    /// Opens a recording (`--play FILE`, or a `.avrec` file dropped on the
    /// window) instead of a world.
    pub fn open_playback(&mut self, file: PathBuf) {
        self.session = None;
        self.opening = None;
        let file = std::path::absolute(&file).unwrap_or(file);
        self.state.begin_opening(Experiment {
            dir: file.clone(),
            args: Vec::new(),
            update_limit: None,
        });
        self.map_texture = None;
        self.map_image = None;
        self.series.clear();
        self.last_generation = 0;
        self.spawn_opening(move |wake| Session::play_with_wake(&file, wake));
    }

    /// Builds a session on a helper thread (the UI thread never waits for
    /// it) and waits for it in [`AvidaApp::pump`].
    fn spawn_opening(
        &mut self,
        open: impl FnOnce(avida_session::WakeFn) -> Result<Session, String> + Send + 'static,
    ) {
        let (tx, rx) = mpsc::channel();
        let ctx = self.ctx.clone();
        let spawned = std::thread::Builder::new()
            .name("avida-open".into())
            .spawn(move || {
                let wake_ctx = ctx.clone();
                let result = open(Box::new(move || wake_ctx.request_repaint()));
                let _ = tx.send(result);
                ctx.request_repaint();
            });
        match spawned {
            Ok(_) => self.opening = Some(rx),
            Err(e) => self
                .state
                .open_failed(format!("cannot start a thread to build the world: {e}")),
        }
    }

    /// Builds the world again with the experiment's arguments: in the
    /// running session (`Command::Restart`) when the folder is the same,
    /// otherwise by opening it anew. If the session cannot build the new
    /// world, the old run carries on.
    fn restart(&mut self, exp: Experiment) {
        if self.state.restarting.is_some() {
            self.state.toast(
                ToastKind::Info,
                "A restart is already under way.".to_string(),
            );
            return;
        }
        let note = if exp.args.is_empty() {
            "Restarting the experiment.".to_string()
        } else {
            format!("Restarting with {}", exp.args.join(" "))
        };
        let same_dir = self.state.experiment.as_ref().is_some_and(|e| {
            std::path::absolute(&e.dir).ok() == std::path::absolute(&exp.dir).ok()
        });
        match &self.session {
            Some(session) if same_dir => {
                if session.send(Command::Restart {
                    args: exp.args.clone(),
                }) {
                    self.state.begin_restart(exp);
                    self.state.log.push(LogKind::Info, note);
                } else {
                    self.state.notify(
                        ToastKind::Error,
                        "The simulation thread has stopped.".to_string(),
                    );
                }
            }
            _ => {
                self.open_experiment(exp);
                self.state.notify(ToastKind::Info, note);
            }
        }
    }

    /// Stops the session and returns to the start screen.
    pub fn close(&mut self) {
        self.session = None;
        self.opening = None;
        self.map_texture = None;
        self.map_image = None;
        self.series.clear();
        self.state.closed();
    }

    /// Moves what the session published into the state, without waiting.
    pub fn pump(&mut self) {
        if let Some(rx) = &self.opening {
            match rx.try_recv() {
                Ok(Ok(session)) => {
                    let setup = session.setup();
                    if let Some(spec) = self.pending_record.take() {
                        session.send(Command::Record(spec));
                    }
                    self.session = Some(session);
                    self.opening = None;
                    // The recent list holds experiment folders, not recordings.
                    if let (Some(e), None) = (
                        &self.state.experiment,
                        self.session.as_ref().and_then(Session::playback_info),
                    ) {
                        let dir = e.dir.clone();
                        self.settings.remember(&dir);
                    }
                    self.state.opened();
                    self.state.set_setup(setup);
                }
                Ok(Err(e)) => {
                    self.opening = None;
                    self.state.open_failed(e);
                }
                Err(TryRecvError::Empty) => {}
                Err(TryRecvError::Disconnected) => {
                    self.opening = None;
                    self.state
                        .open_failed("the thread building the world stopped".to_string());
                }
            }
        }
        if let Some(session) = &self.session {
            let events = session.drain_events();
            let (rows_reset, mut rows) = session.drain_rows_reset();
            // The rows of the updates before a failure come first, so the
            // failure's context starts after the last finished update.
            if events
                .iter()
                .any(|e| matches!(e, avida_session::SessionEvent::Fatal(_)))
            {
                self.state.apply_rows(std::mem::take(&mut rows));
            }
            self.state.apply_events(events);
            if rows_reset {
                self.state.rows_reset();
            }
            if self.state.take_run_reset() {
                // The session restarted: charts and the map belong to the
                // old run.
                self.map_texture = None;
                self.map_image = None;
                self.series.clear();
            }
            self.state.apply_rows(rows);
            let generation = session.frame_generation();
            if generation != self.last_generation {
                if let Some(frame) = session.latest_frame() {
                    self.state.apply_frame(frame);
                }
                self.last_generation = generation;
            }
            self.state.apply_status(session.status());
            self.state.apply_playback(session.playback_info());
            self.state.apply_recording(session.recording_status());
            self.state.exit_code = session.exit_code();
        }
        self.exports.retain(|job| match job.done.try_recv() {
            Ok(result) => {
                report_export(&mut self.state, result, job.what);
                false
            }
            Err(TryRecvError::Empty) => true,
            Err(TryRecvError::Disconnected) => {
                report_export(
                    &mut self.state,
                    Err("the thread writing it stopped".to_string()),
                    job.what,
                );
                false
            }
        });
    }

    fn flush_commands(&mut self) {
        let commands = self.state.take_commands();
        let Some(session) = &self.session else {
            return;
        };
        for cmd in commands {
            if !session.send(cmd) {
                self.state.notify(
                    ToastKind::Error,
                    "The simulation thread has stopped.".to_string(),
                );
                break;
            }
        }
    }

    fn refresh_map(&mut self, ctx: &egui::Context) {
        if !self.state.map_dirty {
            return;
        }
        self.state.map_dirty = false;
        let Some(frame) = &self.state.frame else {
            return;
        };
        let Some(img) = render_map(frame, &self.state.map, self.state.highlight) else {
            return;
        };
        if img.w == 0 || img.h == 0 {
            return;
        }
        let color = ColorImage::from_rgba_unmultiplied([img.w, img.h], &img.rgba);
        match &mut self.map_texture {
            Some(tex) => tex.set(color, TextureOptions::NEAREST),
            None => {
                self.map_texture =
                    Some(ctx.load_texture("population-map", color, TextureOptions::NEAREST));
            }
        }
        if self.map_image.as_ref().map(|m| (m.w, m.h)) != Some((img.w, img.h)) {
            self.state.viewport.auto_fit = true;
        }
        self.map_image = Some(img);
    }

    fn shortcuts(&mut self, ctx: &egui::Context) {
        let typing = ctx.text_edit_focused();
        let ready = self.state.phase == Phase::Ready;
        let palette_open = self.state.command_palette.open;
        let mut actions: Vec<Shortcut> = Vec::new();
        ctx.input_mut(|i| {
            if i.consume_key(Modifiers::COMMAND, Key::K) {
                actions.push(Shortcut::Palette);
            }
            // The palette has the keyboard while it is open.
            if palette_open {
                return;
            }
            if i.consume_key(Modifiers::COMMAND, Key::O) {
                actions.push(Shortcut::Open);
            }
            if i.consume_key(Modifiers::COMMAND, Key::N) {
                actions.push(Shortcut::New);
            }
            if i.consume_key(Modifiers::COMMAND, Key::S) {
                actions.push(Shortcut::Save);
            }
            if typing {
                return;
            }
            if i.consume_key(Modifiers::NONE, Key::Questionmark) {
                actions.push(Shortcut::Help);
            }
            if i.consume_key(Modifiers::NONE, Key::Escape) {
                actions.push(Shortcut::Escape);
            }
            if !ready {
                return;
            }
            if i.consume_key(Modifiers::NONE, Key::Space) {
                actions.push(Shortcut::RunPause);
            }
            if i.consume_key(Modifiers::NONE, Key::Period) {
                actions.push(Shortcut::StepUpdate);
            }
            if i.consume_key(Modifiers::NONE, Key::Comma) {
                actions.push(Shortcut::StepSlice);
            }
            if i.consume_key(Modifiers::NONE, Key::F) {
                actions.push(Shortcut::Fit);
            }
            if i.consume_key(Modifiers::NONE, Key::I) {
                actions.push(Shortcut::StepOrganism);
            }
            if i.consume_key(Modifiers::NONE, Key::B) {
                actions.push(Shortcut::RunToBreakpoint);
            }
            let digits = [
                Key::Num1,
                Key::Num2,
                Key::Num3,
                Key::Num4,
                Key::Num5,
                Key::Num6,
                Key::Num7,
                Key::Num8,
                Key::Num9,
            ];
            for (n, key) in digits.into_iter().enumerate() {
                if i.consume_key(Modifiers::NONE, key) {
                    actions.push(Shortcut::Layer(n));
                }
            }
        });
        let s = &mut self.state;
        for a in actions {
            match a {
                Shortcut::Palette => {
                    if s.command_palette.open {
                        s.command_palette.close();
                    } else {
                        s.command_palette.open();
                    }
                }
                Shortcut::StepOrganism => s.debug_step(),
                Shortcut::RunToBreakpoint => s.debug_run_to_breakpoint(),
                Shortcut::Open => s.request(Request::PickFolder),
                Shortcut::New => s.request(Request::NewExperiment),
                Shortcut::Save => s.save_population(),
                Shortcut::Help => s.show_shortcuts = !s.show_shortcuts,
                Shortcut::Escape => {
                    if s.show_shortcuts {
                        s.show_shortcuts = false;
                    } else {
                        s.select_cell(None);
                        s.clear_highlight();
                    }
                }
                Shortcut::RunPause => s.toggle_run(),
                Shortcut::StepUpdate => s.step_update(),
                Shortcut::StepSlice => s.step_slice(),
                Shortcut::Fit => s.viewport.auto_fit = true,
                Shortcut::Layer(n) => s.set_layer_index(n),
            }
        }
    }

    /// A folder from the native picker (or the test's answer); `None`
    /// when cancelled.
    fn pick_folder(&mut self, title: &str) -> Option<PathBuf> {
        if !self.native_dialogs {
            if self.dialog_answer.is_none() {
                self.state.toast(
                    ToastKind::Info,
                    "The folder picker is not available here. Paste a path instead.".to_string(),
                );
            }
            return self.dialog_answer.clone();
        }
        let mut dialog = rfd::FileDialog::new().set_title(title);
        if let Some(dir) = self.settings.recent.first().and_then(|d| d.parent()) {
            dialog = dialog.set_directory(dir);
        }
        dialog.pick_folder()
    }

    fn handle_requests(&mut self, ctx: &egui::Context) {
        for request in self.state.take_requests() {
            match request {
                Request::PickFolder => {
                    if let Some(dir) = self.pick_folder("Open an Avida experiment folder") {
                        self.open(dir);
                    }
                }
                Request::NewExperiment | Request::CreateExperimentIn(_) => {
                    let target = if let Request::CreateExperimentIn(dir) = request {
                        dir
                    } else if let Some(chosen) =
                        self.pick_folder("Choose where to create the new experiment")
                    {
                        target_folder(&chosen)
                    } else {
                        continue;
                    };
                    match create_experiment(&target) {
                        Ok(dir) => {
                            self.open(dir.clone());
                            self.state.notify(
                                ToastKind::Success,
                                format!(
                                    "Created a new experiment from the default settings in {}.",
                                    dir.display()
                                ),
                            );
                        }
                        Err(e) => self.state.notify(ToastKind::Error, e),
                    }
                }
                Request::Open(dir) => {
                    self.path_input.clear();
                    self.open(dir);
                }
                Request::Close => self.close(),
                Request::Restart(exp) => self.restart(exp),
                Request::Copy(text) => {
                    ctx.copy_text(text);
                    self.state
                        .toast(ToastKind::Success, "Copied to the clipboard.".to_string());
                }
                Request::ExportMap => self.export(ExportKind::Map),
                Request::ExportCsv => self.export(ExportKind::Statistics),
                Request::ExportGenotypes => self.export(ExportKind::Genotypes),
                Request::ExportCharts => self.export(ExportKind::Charts),
                Request::CopyLog => {
                    let text = self.state.log.to_text(self.state.log_filter);
                    let lines = text.lines().count();
                    ctx.copy_text(text);
                    self.state
                        .toast(ToastKind::Success, format!("Copied {lines} log lines."));
                }
            }
        }
    }

    /// Where an export goes: the world's data directory (`DATA_DIR`, as
    /// the session reports it; the experiment folder if it does not exist)
    /// under a descriptive name, confirmed in a save dialog when native
    /// dialogs are on. `Ok(None)` when the user cancelled.
    fn export_target(&self, kind: ExportKind) -> Result<Option<PathBuf>, String> {
        let exp = self
            .state
            .experiment
            .as_ref()
            .ok_or("no experiment is open")?;
        let dir = match &self.state.setup {
            Some(setup) if setup.data_dir.is_dir() => setup.data_dir.clone(),
            _ => exp.dir.clone(),
        };
        let update = self.state.current_update().unwrap_or(0).max(0);
        let (name, ext) = match kind {
            ExportKind::Map => (map_file_name(self.state.map.layer, update), "png"),
            ExportKind::Charts => (charts_file_name(update), "png"),
            ExportKind::Statistics => (csv_file_name(update), "csv"),
            ExportKind::Genotypes => (genotypes_file_name(update), "csv"),
        };
        if !self.native_dialogs {
            return Ok(Some(dir.join(name)));
        }
        Ok(rfd::FileDialog::new()
            .set_directory(&dir)
            .set_file_name(&name)
            .add_filter(ext.to_uppercase(), &[ext])
            .save_file())
    }

    /// Checks that there is something to export, asks where, and writes it
    /// on a helper thread (a long run's statistics take a while).
    fn export(&mut self, kind: ExportKind) {
        let what = kind.what();
        let ready = match kind {
            ExportKind::Map => self
                .map_image
                .as_ref()
                .map(|_| ())
                .ok_or("there is no map to export yet"),
            ExportKind::Statistics | ExportKind::Charts => {
                if self.state.history.is_empty() {
                    Err("no update has finished yet")
                } else {
                    Ok(())
                }
            }
            ExportKind::Genotypes => match &self.state.frame {
                Some(f) if !f.genotypes.is_empty() => Ok(()),
                _ => Err("no genotype is alive"),
            },
        };
        if let Err(e) = ready {
            report_export(&mut self.state, Err(e.to_string()), what);
            return;
        }
        let path = match self.export_target(kind) {
            Ok(Some(p)) => p,
            Ok(None) => return,
            Err(e) => {
                report_export(&mut self.state, Err(e), what);
                return;
            }
        };
        match kind {
            ExportKind::Map => {
                let Some(img) = self.map_image.clone() else {
                    return;
                };
                self.spawn_export(what, path, move |p| {
                    let bytes = png_bytes(&img.rgba, img.w, img.h, export_scale(img.w, img.h))?;
                    write_file(p, &bytes)
                });
            }
            ExportKind::Statistics => {
                let rows = self.state.history.snapshot();
                let env = self.state.frame.as_ref().map(|f| f.env.clone());
                self.spawn_export(what, path, move |p| {
                    write_file(p, rows_csv(&rows, env.as_ref()).as_bytes())
                });
            }
            ExportKind::Genotypes => {
                let rows = self
                    .state
                    .frame
                    .as_ref()
                    .map(|f| f.genotypes.clone())
                    .unwrap_or_default();
                self.spawn_export(what, path, move |p| {
                    write_file(p, genotypes_csv(&rows).as_bytes())
                });
            }
            ExportKind::Charts => {
                // Show the charts and capture them once they are drawn.
                self.state.side_tab = SideTab::Charts;
                self.chart_shot = Some(ChartShot::Waiting { frames: 2, path });
            }
        }
    }

    fn spawn_export(
        &mut self,
        what: &'static str,
        path: PathBuf,
        work: impl FnOnce(&Path) -> Result<(), String> + Send + 'static,
    ) {
        let (tx, rx) = mpsc::channel();
        let ctx = self.ctx.clone();
        let spawned = std::thread::Builder::new()
            .name("avida-export".into())
            .spawn(move || {
                let _ = tx.send(work(&path).map(|()| path));
                ctx.request_repaint();
            });
        match spawned {
            Ok(_) => self.exports.push(ExportJob { what, done: rx }),
            Err(e) => report_export(
                &mut self.state,
                Err(format!("cannot start a thread to write it: {e}")),
                what,
            ),
        }
    }

    /// Moves a capture of the charts tab along, one step per frame.
    fn advance_chart_shot(&mut self, ctx: &egui::Context) {
        let Some(shot) = self.chart_shot.take() else {
            return;
        };
        ctx.request_repaint();
        match shot {
            ChartShot::Waiting { frames, path } if frames > 0 => {
                self.chart_shot = Some(ChartShot::Waiting {
                    frames: frames - 1,
                    path,
                });
            }
            ChartShot::Waiting { path, .. } => {
                let image: Arc<Mutex<Option<Arc<ColorImage>>>> = Arc::default();
                let slot = Arc::clone(&image);
                ctx.send_viewport_cmd(ViewportCommand::Screenshot(egui::ScreenshotCallback::new(
                    move |img| {
                        *slot.lock().unwrap_or_else(|e| e.into_inner()) = Some(img);
                    },
                )));
                self.chart_shot = Some(ChartShot::Requested {
                    path,
                    image,
                    waited: 0,
                });
            }
            ChartShot::Requested {
                path,
                image,
                waited,
            } => {
                let taken = image.lock().unwrap_or_else(|e| e.into_inner()).take();
                let Some(img) = taken else {
                    if waited < SCREENSHOT_PATIENCE {
                        self.chart_shot = Some(ChartShot::Requested {
                            path,
                            image,
                            waited: waited + 1,
                        });
                    } else {
                        report_export(
                            &mut self.state,
                            Err("the window did not hand over a screenshot".to_string()),
                            ExportKind::Charts.what(),
                        );
                    }
                    return;
                };
                let Some(area) = self.state.chart_area else {
                    report_export(
                        &mut self.state,
                        Err("the charts are not on screen".to_string()),
                        ExportKind::Charts.what(),
                    );
                    return;
                };
                let ppp = img.size[0] as f32 / ctx.content_rect().width().max(1.0);
                let rgba: Vec<u8> = img.pixels.iter().flat_map(|c| c.to_array()).collect();
                let rect = [area.min.x, area.min.y, area.max.x, area.max.y];
                let Some((px, w, h)) = crop_rgba(&rgba, img.size[0], img.size[1], rect, ppp) else {
                    report_export(
                        &mut self.state,
                        Err("the charts are outside the window".to_string()),
                        ExportKind::Charts.what(),
                    );
                    return;
                };
                self.spawn_export(ExportKind::Charts.what(), path, move |p| {
                    write_file(p, &png_bytes(&px, w, h, 1)?)
                });
            }
        }
    }

    /// Writes the current map image as PNG (empty cells transparent), on
    /// this thread.
    pub fn export_map_png(&self, path: &Path) -> Result<(), String> {
        let img = self
            .map_image
            .as_ref()
            .ok_or("there is no map to export yet")?;
        let bytes = png_bytes(&img.rgba, img.w, img.h, export_scale(img.w, img.h))?;
        write_file(path, &bytes)
    }

    /// A folder (or a file in one) dropped on the window opens it; while
    /// files hover, the window says so.
    fn dropped_files(&mut self, ctx: &egui::Context) {
        let (hovered, dropped) = ctx.input(|i| {
            (
                i.raw
                    .hovered_files
                    .iter()
                    .map(|f| f.path.clone())
                    .collect::<Vec<_>>(),
                i.raw
                    .dropped_files
                    .iter()
                    .map(|f| f.path().to_path_buf())
                    .collect::<Vec<_>>(),
            )
        });
        if let Some(path) = dropped
            .first()
            .filter(|p| p.is_file() && p.extension().is_some_and(|e| e == "avrec"))
        {
            self.open_playback(path.clone());
        } else if let Some(path) = dropped.first() {
            match dropped_folder(path) {
                Some(dir) => self.state.request(Request::Open(dir)),
                None => self.state.toast(
                    ToastKind::Error,
                    format!("{} cannot be opened.", path.display()),
                ),
            }
        }
        if !hovered.is_empty() {
            let name = hovered
                .first()
                .and_then(|p| p.as_deref())
                .and_then(dropped_folder)
                .map(|d| crate::model::display_name(&d));
            overlays::drop_target(ctx, name.as_deref());
        }
    }

    fn apply_theme(&mut self, ctx: &egui::Context) {
        if self.applied_theme != Some(self.settings.theme) {
            ctx.set_theme(self.settings.theme.preference());
            self.applied_theme = Some(self.settings.theme);
        }
    }

    fn workspace(&mut self, ui: &mut egui::Ui) {
        let t = Tokens::of(ui);
        Panel::top("top-bar")
            .exact_size(52.0)
            .frame(
                Frame::new()
                    .fill(t.surface)
                    .inner_margin(Margin::symmetric(10, 0)),
            )
            .show(ui, |ui| {
                top_bar::show(ui, &mut self.state, &mut self.settings)
            });
        if self.state.is_playback() {
            Panel::top("playback-bar")
                .exact_size(crate::ui::playback::HEIGHT)
                .frame(
                    Frame::new()
                        .fill(t.surface)
                        .inner_margin(Margin::symmetric(14, 0)),
                )
                .show(ui, |ui| crate::ui::playback::show(ui, &mut self.state));
        }
        Panel::bottom("status-bar")
            .exact_size(28.0)
            .frame(
                Frame::new()
                    .fill(t.surface)
                    .inner_margin(Margin::symmetric(8, 0)),
            )
            .show(ui, |ui| log_panel::status_bar(ui, &mut self.state));
        let mut log_open = self.state.show_log;
        Panel::bottom("log-panel")
            .resizable(true)
            .default_size(210.0)
            .size_range(110.0..=640.0)
            .frame(
                Frame::new()
                    .fill(t.bg)
                    .inner_margin(Margin::symmetric(12, 8)),
            )
            .show_collapsible(ui, &mut log_open, |ui| {
                side::contained(ui, |ui| log_panel::log_panel(ui, &mut self.state));
            });
        if !log_open {
            self.state.show_log = false;
        }
        // The side panel may take at most 40 % of the window (but 330 points
        // at least), so the map keeps most of a small window.
        let side_max = (ui.max_rect().width() * 0.4).max(330.0);
        Panel::right("side-panel")
            .resizable(true)
            .default_size(430.0_f32.min(side_max))
            .size_range(330.0..=side_max)
            .frame(Frame::new().fill(t.bg).inner_margin(Margin {
                left: 14,
                right: 14,
                top: 8,
                bottom: 8,
            }))
            .show(ui, |ui| side::show(ui, &mut self.state, &mut self.series));
        egui::CentralPanel::default()
            .frame(Frame::new().fill(t.bg).inner_margin(Margin::same(12)))
            .show(ui, |ui| {
                side::contained(ui, |ui| {
                    map::show(
                        ui,
                        &mut self.state,
                        self.map_texture.as_ref(),
                        self.map_image.as_ref(),
                    );
                });
            });
    }
}

/// What an export writes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ExportKind {
    Map,
    Charts,
    Statistics,
    Genotypes,
}

impl ExportKind {
    /// How the notification names it.
    fn what(self) -> &'static str {
        match self {
            ExportKind::Map => "The map",
            ExportKind::Charts => "The charts",
            ExportKind::Statistics => "The statistics",
            ExportKind::Genotypes => "The genotype table",
        }
    }
}

fn write_file(path: &Path, bytes: &[u8]) -> Result<(), String> {
    std::fs::write(path, bytes).map_err(|e| format!("{}: {e}", path.display()))
}

fn report_export(state: &mut AppState, result: Result<PathBuf, String>, what: &str) {
    match result {
        Ok(p) => state.notify(
            ToastKind::Success,
            format!("{what} was saved to {}.", p.display()),
        ),
        Err(e) => state.notify(ToastKind::Error, format!("{what} could not be saved: {e}.")),
    }
}

enum Shortcut {
    New,
    Palette,
    StepOrganism,
    RunToBreakpoint,
    Open,
    Save,
    Help,
    Escape,
    RunPause,
    StepUpdate,
    StepSlice,
    Fit,
    Layer(usize),
}

impl eframe::App for AvidaApp {
    fn logic(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.window_startup.on_frame(ctx);
        self.pump();
        self.state.set_time(ctx.input(|i| i.time));
    }

    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        self.apply_theme(&ctx);
        self.dropped_files(&ctx);
        self.shortcuts(&ctx);
        self.refresh_map(&ctx);
        self.state.chart_area = None;
        match self.state.phase {
            Phase::Start | Phase::Opening => start::show(
                ui,
                &mut self.state,
                &mut self.settings,
                &mut self.path_input,
            ),
            Phase::Ready => self.workspace(ui),
        }
        overlays::toasts(&ctx, &self.state);
        overlays::shortcuts(&ctx, &mut self.state);
        let legend = self.map_image.as_ref().and_then(|m| m.legend.clone());
        command_palette::show(&ctx, &mut self.state, &mut self.settings, legend.as_ref());
        self.handle_requests(&ctx);
        self.advance_chart_shot(&ctx);
        self.flush_commands();
        let title = self.title();
        if title != self.title {
            ctx.send_viewport_cmd(ViewportCommand::Title(title.clone()));
            self.title = title;
        }
        // Changes made this frame (a layer switch, a highlight) are drawn
        // with the next one.
        if self.state.map_dirty {
            ctx.request_repaint();
        }
    }

    fn save(&mut self, storage: &mut dyn eframe::Storage) {
        self.settings.capture(&self.state);
        eframe::set_value(storage, STORAGE_KEY, &self.settings);
    }
}
