//! avida-session: the simulation thread behind the desktop (egui) and
//! terminal (ratatui) front ends.
//!
//! A [`Session`] owns one thread that builds a [`World`] from a working
//! directory and an `avida` command line, then drives it with a
//! [`Stepper`] under the front end's [`Command`]s. The world is `!Send`, so
//! it is built inside that thread and never leaves it; the front end sees
//! only owned copies:
//!
//! * frames ([`Frame`]), latest-wins: one slot that the thread overwrites,
//!   numbered by [`Session::frame_generation`];
//! * rows ([`UpdateRow`]), one per finished update, kept until
//!   [`Session::drain_rows`] takes them, so charts lose nothing;
//! * events ([`SessionEvent`]): state changes, the text the `avida` binary
//!   prints (also what the simulation prints to stderr while running),
//!   command errors, how a [`Command::RunUntil`] ended, births from the
//!   inspected organism, the watched cell moving to an offspring, a
//!   restart;
//! * the [`ExperimentSetup`] the world was built from (the data directory,
//!   the effective configuration and the events file), replaced by
//!   [`Command::Restart`].
//!
//! # Watching one organism
//!
//! [`Command::RunUntil`] runs until something happens to one organism (it
//! has run a number of instructions, or an instruction pointer reaches a
//! memory position), until that organism dies or leaves its cell, or until
//! a slice budget is spent, whichever comes first, and pauses there (often
//! inside an update). It checks after every instruction of the organism:
//! with `SPECULATIVE` on, a time slice runs an instruction and then up to
//! 32 more speculatively, and the session drives the watched organism's
//! slices one instruction at a time through
//! [`Stepper::step_watched`](avida_world::run::Stepper::step_watched), so
//! a step is one instruction and a breakpoint is never stepped over. A
//! pause can then fall between two instructions of one slice
//! ([`Status::mid_slice`]); anything that runs a whole slice finishes that
//! one first.
//!
//! [`SessionEvent::Born`] reports each offspring of the inspected organism
//! as it is placed, and [`Command::FollowOffspring`] moves the inspected
//! cell to it. All of this reads the world between instructions
//! ([`avida_viewmodel::probe`], the world's birth log), so it changes
//! nothing the run computes.
//!
//! # Determinism
//!
//! A session computes exactly what `avida` computes for the same working
//! directory and arguments, however it is paused, stepped, throttled or
//! inspected: the thread calls the world only through [`Stepper`] (whose
//! cut points are the C++ loop's own, and between the instructions of a
//! speculative slice the order of the calls it makes), through the
//! read-only [`avida_viewmodel::capture()`], [`avida_viewmodel::update_row`]
//! and the organism probes, through the birth log (which nothing in the run
//! reads), and, on [`Command::SavePopulation`] only, through
//! [`World::save_population`], which writes a file but changes no state.
//!
//! # Recording and playback
//!
//! [`Command::Record`] makes the simulation thread append a [`Frame`] to a
//! file whenever an update ends at a multiple of the chosen interval (and
//! when the run finishes), captured between updates with a subscription of
//! its own (every layer unless the [`RecordSpec`] lists some). Capturing is
//! read-only, so a recorded run writes the data files an unrecorded one
//! does (`tests/recording.rs` pins it). The file format, its size and its
//! behaviour on truncated and corrupt files are in [`recording`]; the
//! headless recorder is the `avida-rec` binary of this crate.
//!
//! [`Session::play`] opens such a file and returns a `Session` whose frames
//! come from the file instead of a world, so a front end uses the handle it
//! already has: `Run`, `Pause`, `StepUpdate` (next frame), `StepSlice`
//! (previous frame), `RunToUpdate` and [`Command::SeekFrame`] (seek),
//! `SetSpeed` (updates of the recorded run per second), frames, rows (those
//! of the records up to the frame shown; [`Session::drain_rows_reset`] says
//! when a seek backwards makes them start again), [`Session::playback_info`]
//! and an [`ExperimentSetup`] with the recorded arguments and no settings.
//! Commands that need a world are answered with an error event.
//!
//! # Diagnostics
//!
//! What the simulation crates print on their own (`error:` and `warning:`
//! lines of actions, an instruction's `printf`) goes through
//! `avida_apto::diag`. The simulation thread captures it and publishes it
//! as [`SessionEvent::Stderr`] and [`SessionEvent::Stdout`] lines, so a
//! front end's terminal stays clean; the `avida` binary, which captures
//! nothing, prints it as before.
//!
//! # Pacing
//!
//! While running, the thread checks for commands every [`SLICE_BATCH`]
//! time slices, so `Pause` takes effect within a fraction of a millisecond
//! even inside an update of a full population. While paused it blocks on
//! the command channel. Frames are captured at the end of an update at most
//! once per [`MIN_FRAME_INTERVAL`] while running, and always after a step,
//! a pause, the end of the run, or a change of subscription.
//!
//! [`World`]: avida_world::world::World
//! [`World::save_population`]: avida_world::world::World::save_population
//! [`Stepper`]: avida_world::run::Stepper

pub mod cli;
mod playback;
pub mod recording;
mod sim;

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender};
use std::sync::{Arc, Mutex, MutexGuard};
use std::thread::JoinHandle;
use std::time::Duration;

pub use avida_viewmodel::{Frame, LayerId, Subscription, UpdateProgress, UpdateRow};
pub use recording::{MapFrameCompare, Recording, RecordingError, RecordingWriter};

/// Time slices run between two looks at the command channel.
pub const SLICE_BATCH: usize = 1024;

/// Shortest wall-clock gap between two frames captured while running
/// (about 30 per second).
pub const MIN_FRAME_INTERVAL: Duration = Duration::from_millis(33);

/// Stack of the simulation thread: as large as a typical main thread, which
/// is where the `avida` binary runs the world.
const SIM_STACK_SIZE: usize = 16 * 1024 * 1024;

/// What to run.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SessionConfig {
    /// The run's working directory (the `avida` binary's current
    /// directory): `avida.cfg`, the event and environment files and the
    /// `data/` output are resolved against it.
    pub work_dir: PathBuf,
    /// Command-line arguments as the `avida` binary takes them, without the
    /// program name: `["-s", "7", "-set", "WORLD_X", "30", "-c", "x.cfg"]`.
    pub args: Vec<String>,
}

/// How fast a running session advances.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub enum Speed {
    /// As fast as the thread can go.
    #[default]
    Max,
    /// At most this many updates per wall-clock second. A rate that is not
    /// a positive finite number means [`Speed::Max`].
    UpdatesPerSecond(f64),
}

impl Speed {
    /// Wall-clock time per update, `None` for [`Speed::Max`].
    fn period(self) -> Option<Duration> {
        match self {
            Speed::UpdatesPerSecond(r) if r.is_finite() && r > 0.0 => {
                Some(Duration::from_secs_f64((1.0 / r).min(3600.0)))
            }
            _ => None,
        }
    }
}

/// A request to the simulation thread. Commands are carried out in the
/// order sent.
#[derive(Clone, Debug, PartialEq)]
pub enum Command {
    /// Run until paused, finished or failed.
    Run,
    /// Stop at the next slice boundary (possibly inside an update). Answered
    /// with `Paused` also when already paused; ignored once the run is over.
    Pause,
    /// Finish the update in progress, or run the next one whole, then
    /// pause.
    StepUpdate,
    /// Run one time slice (beginning the next update first if none is in
    /// progress), then pause. When the update in progress has no slices
    /// left, end it instead.
    StepSlice,
    /// Run until the given update has ended (`status().update == n` between
    /// updates), then pause. Pauses at once if that point is already
    /// passed.
    RunToUpdate(i32),
    /// Change the pace of running.
    SetSpeed(Speed),
    /// Replace the subscription (layers, inspected cell, genotype list) and
    /// capture a frame for it now.
    Subscribe(Subscription),
    /// Change only the inspected cell and capture a frame now.
    WatchCell(Option<i32>),
    /// Do what the `SavePopulation` event action does with its default
    /// arguments: write `detail-<update>.spop` to the data directory.
    SavePopulation,
    /// Run time slices until something happens to one organism, then
    /// pause (see [`RunUntil`]). Answered with
    /// [`SessionEvent::RunUntilEnded`], then `Paused` (or the event of the
    /// run ending).
    RunUntil(RunUntil),
    /// Whether the inspected cell moves to the offspring of the inspected
    /// organism whenever it divides ([`SessionEvent::WatchMoved`]). Off in
    /// a new session.
    FollowOffspring(bool),
    /// Discard the run and build a new one in the same working directory
    /// from these command-line arguments (as [`SessionConfig::args`]).
    /// Answered with [`SessionEvent::Restarted`], the new run's set-up
    /// lines and `Paused { update: -1 }`; or, when the new world cannot be
    /// built, with [`SessionEvent::RestartFailed`], the old run going on
    /// (paused if it was running). The subscription's layers and genotype
    /// list carry over; the inspected cell, the lineage, following and any
    /// [`Command::RunUntil`] do not.
    Restart { args: Vec<String> },
    /// Start recording the run to a file (see [`RecordSpec`] and the
    /// [`recording`] module). Answered with [`SessionEvent::RecordingStarted`]
    /// or an `Error`; a recording already under way is replaced.
    Record(RecordSpec),
    /// Stop recording; the file stays as written. Answered with
    /// [`SessionEvent::RecordingStopped`].
    StopRecording,
    /// Append a frame to the recording now, if the run stands between
    /// updates and the current update is not recorded yet. (Frames are
    /// otherwise appended when an update ends at a multiple of the
    /// interval, and when the run finishes.)
    RecordNow,
    /// Playback only: show record number `n` (0 is the first). Out of
    /// range numbers are clamped. A simulation answers with an `Error`.
    SeekFrame(usize),
    /// Stop the thread. [`Session`]'s `Drop` sends it.
    Shutdown,
}

/// What to record and how often ([`Command::Record`]).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RecordSpec {
    /// The file to create (an existing one is replaced).
    pub path: PathBuf,
    /// Capture a frame whenever an update ends at a multiple of this many
    /// updates (at least 1; [`DEFAULT_RECORD_EVERY`] by default).
    pub every: u32,
    /// The map layers to record; `None` for every layer the world has.
    /// Layers the world does not have (a task or resource out of range)
    /// are dropped; [`LayerId::Genotype`] is always recorded.
    pub layers: Option<Vec<LayerId>>,
    /// Genotypes in the table of each frame, most abundant first.
    pub genotypes_top_n: usize,
}

/// The default capture interval of a recording, in updates.
pub const DEFAULT_RECORD_EVERY: u32 = 100;

/// The default length of the genotype table of a recorded frame (what the
/// desktop front end lists).
pub const DEFAULT_RECORD_TOP_N: usize = 100;

impl RecordSpec {
    /// Every layer, every [`DEFAULT_RECORD_EVERY`] updates.
    pub fn new(path: impl Into<PathBuf>) -> RecordSpec {
        RecordSpec {
            path: path.into(),
            every: DEFAULT_RECORD_EVERY,
            layers: None,
            genotypes_top_n: DEFAULT_RECORD_TOP_N,
        }
    }
}

/// How a recording is going ([`Session::recording_status`]).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RecordingStatus {
    pub path: PathBuf,
    pub every: u32,
    /// Frames written.
    pub frames: u64,
    /// The file's size.
    pub bytes: u64,
    /// The update of the last frame written.
    pub last_update: Option<i32>,
    /// Wall-clock time the simulation thread has spent capturing,
    /// compressing and writing frames: what recording costs the run.
    pub time_spent: Duration,
    /// Why the recording stopped on its own (a write failed).
    pub error: Option<String>,
    /// Whether frames are still being appended.
    pub active: bool,
}

/// Where a playback session stands ([`Session::playback_info`]).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PlaybackInfo {
    /// The recording being played.
    pub path: PathBuf,
    /// Complete records in it so far.
    pub frames: usize,
    /// The record on show (0-based).
    pub index: usize,
    /// The update of the record on show.
    pub update: i32,
    /// The updates of the first and the last record.
    pub first_update: i32,
    pub last_update: i32,
    /// Bytes at the end of the file that are not a complete record (the
    /// run was killed, or is still writing).
    pub cut_tail_bytes: u64,
    /// The recorded layers, for front ends that want to say what is there.
    pub layers: Vec<LayerId>,
}

/// What a [`Command::RunUntil`] waits for.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum StopWhen {
    /// The organism has executed this many more instructions (its
    /// lifetime CPU cycles, [`avida_viewmodel::OrgProbe::cpu_cycles`], have
    /// grown by that much; 0 counts as 1). `Cycles(1)` is "step one
    /// instruction of this organism".
    Cycles(u32),
    /// After an instruction of the organism, an instruction pointer of one
    /// of its threads (in its own memory) stands at one of these memory
    /// positions: a breakpoint.
    IpAt(Vec<i32>),
    /// Like [`StopWhen::IpAt`] for hardware with several memory spaces:
    /// the breakpoints are `(memory space, position)` pairs, and an
    /// instruction pointer counts in whichever space it stands.
    IpIn(Vec<(i32, i32)>),
}

/// [`Command::RunUntil`]: run time slices until something happens to one
/// organism.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RunUntil {
    /// The cell the organism lives in.
    pub cell: i32,
    /// Its id (`cOrganism::GetID()`): the run stops when the cell no longer
    /// holds it (unless it moved, which the watch follows).
    pub org_id: i32,
    pub stop: StopWhen,
    /// Most time slices to run, across update boundaries, before giving
    /// up.
    pub max_slices: u64,
}

/// How a [`Command::RunUntil`] ended.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UntilOutcome {
    /// The condition held.
    Reached,
    /// The organism is gone: it died, or another organism took its cell.
    /// `occupant` is the id of the organism in the cell now, if any.
    OrganismGone { occupant: Option<i32> },
    /// `max_slices` slices ran without the condition holding.
    BudgetExhausted,
    /// Another command (pause, run, a step, a new `RunUntil`) took over.
    Interrupted,
    /// The run finished or failed first.
    RunOver,
}

/// Where the run stands.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RunState {
    /// Waiting for a command (also the state a new session starts in).
    Paused,
    /// Advancing on its own.
    Running,
    /// The run reached `Exit` or died out. The last frame stays readable;
    /// nothing can advance it.
    Finished,
    /// The run stopped on an error (an unported path, an action that
    /// failed, a panic). The last frame stays readable; nothing can
    /// advance it.
    Failed,
}

/// The thread's position, as of the last slice batch, update boundary or
/// command.
#[derive(Clone, Debug, PartialEq)]
pub struct Status {
    pub state: RunState,
    /// `cStats::GetUpdate()`: the update in progress, or the last one
    /// finished between updates (`-1` before the first).
    pub update: i32,
    /// `Some((slices run, UD_size))` while inside an update
    /// ([`Stepper::slice_progress`](avida_world::run::Stepper::slice_progress)).
    pub slice: Option<(i32, i32)>,
    pub speed: Speed,
    /// The target of a [`Command::RunToUpdate`] in force.
    pub run_to: Option<i32>,
    /// `Some((slices run, max_slices))` while a [`Command::RunUntil`] is in
    /// force.
    pub run_until: Option<(u64, u64)>,
    /// Whether [`Command::FollowOffspring`] is on.
    pub follow_offspring: bool,
    /// Paused between two instructions of a speculative time slice of the
    /// watched organism (counted in `slice` already).
    pub mid_slice: bool,
}

/// Something the front end should know about.
#[derive(Clone, Debug, PartialEq)]
pub enum SessionEvent {
    /// The session has started running.
    Running,
    /// The session has paused (or was asked to pause while paused).
    Paused { update: i32 },
    /// The run is over: `Exit`, or the population died out.
    Finished { update: i32 },
    /// The run stopped on an error and cannot continue.
    Fatal(String),
    /// A command could not be carried out; the session carries on.
    Error(String),
    /// [`Command::SavePopulation`] wrote this file.
    PopulationSaved(PathBuf),
    /// A line the `avida` binary prints to stdout: set-up messages, the
    /// `Random Seed:` line, the per-update status lines, action output.
    Stdout(String),
    /// A line the `avida` binary prints to stderr: set-up warnings, and
    /// the errors and warnings of actions and instructions while running.
    Stderr(String),
    /// A [`Command::RunUntil`] ended after `slices` time slices, at
    /// `update` (the update in progress if it stopped inside one).
    RunUntilEnded {
        outcome: UntilOutcome,
        slices: u64,
        update: i32,
    },
    /// The inspected cell moved from `from` to `to`, which holds organism
    /// `org_id`: an offspring of the inspected organism
    /// ([`Command::FollowOffspring`]), or, with `offspring` false, the
    /// organism a [`Command::RunUntil`] watches, which moved.
    WatchMoved {
        from: i32,
        to: i32,
        org_id: i32,
        offspring: bool,
    },
    /// The inspected organism `parent_id`, living in `parent_cell`, divided
    /// and its offspring `org_id` was placed in `cell`, during `update`
    /// (`slice` slices into it). Offspring that die as they are placed
    /// (deadly boundaries, birth methods 12 and 13) are not reported.
    Born {
        parent_id: i32,
        parent_cell: i32,
        org_id: i32,
        cell: i32,
        update: i32,
        slice: i32,
    },
    /// [`Command::Restart`] built a new world from this set-up; the old run
    /// is gone, and rows, frames and events from here on are the new run's.
    Restarted(Arc<ExperimentSetup>),
    /// [`Command::Restart`] could not build a new world (the text `avida`
    /// would print); the old run goes on.
    RestartFailed(String),
    /// A recording to this file has begun ([`Command::Record`]).
    RecordingStarted(PathBuf),
    /// The recording ended (asked for, replaced, or by a restart) with
    /// this many frames and bytes; its file is complete.
    RecordingStopped { frames: u64, bytes: u64 },
}

/// One setting of the effective configuration.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Setting {
    /// `CONFIG_ADD_GROUP` name (`GENERAL_GROUP`).
    pub group: &'static str,
    /// The group's description (`General Settings`).
    pub group_desc: &'static str,
    /// The setting's primary name (`WORLD_X`).
    pub name: &'static str,
    /// The C++ type name (`int`, `double`, `bool`, `cString`).
    pub kind: &'static str,
    /// The value in force, as `cAvidaConfig` prints it.
    pub value: String,
    /// The built-in default, printed the same way.
    pub default: String,
    /// The setting's description (lines separated by `\n`).
    pub desc: &'static str,
    /// Set by `-set`, `-s` or `-v` on the command line.
    pub from_command_line: bool,
}

impl Setting {
    /// Whether the value differs from the built-in default.
    pub fn is_changed(&self) -> bool {
        self.value != self.default
    }
}

/// One line of the events file, split the way `cEventList` reads it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EventLine {
    /// The line as read (comments and blank lines removed).
    pub text: String,
    /// `u`, `g`, `b`, `o` or `i` (as written: `update`, `immediate`...).
    pub trigger: String,
    /// `begin`, `100`, `0:10:end`...; empty for an immediate event.
    pub timing: String,
    /// The action name (`PrintAverageData`).
    pub action: String,
    /// The rest of the line: the action's arguments.
    pub args: String,
}

/// What the world was built from, as the session found it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ExperimentSetup {
    /// The working directory (absolute).
    pub work_dir: PathBuf,
    /// The command-line arguments.
    pub args: Vec<String>,
    /// Where the run writes its output (`DATA_DIR`, absolute).
    pub data_dir: PathBuf,
    /// The configuration file (`-c`, default `avida.cfg`).
    pub config_file: PathBuf,
    /// Every setting in declaration order, with the command line applied.
    pub settings: Vec<Setting>,
    /// The events file (`EVENT_FILE`).
    pub event_file: PathBuf,
    /// Its lines, includes expanded.
    pub events: Vec<EventLine>,
}

impl ExperimentSetup {
    /// The setting with this primary name.
    pub fn setting(&self, name: &str) -> Option<&Setting> {
        self.settings.iter().find(|s| s.name == name)
    }
}

/// Called by the simulation thread whenever a new frame or a state event is
/// available (egui's `request_repaint`). Rows and stdout lines alone do not
/// call it; they arrive with the next frame.
pub type WakeFn = Box<dyn Fn() + Send>;

/// What the thread publishes and the handle reads.
struct Shared {
    frame: Mutex<Option<Arc<Frame>>>,
    frame_generation: AtomicU64,
    rows: Mutex<Vec<UpdateRow>>,
    /// Playback: the rows delivered before are void and `rows` holds the
    /// whole history up to the frame shown. Set and cleared with the
    /// `rows` lock held, so a reader sees the flag and the rows together.
    rows_reset: AtomicBool,
    events: Mutex<Vec<SessionEvent>>,
    status: Mutex<Status>,
    /// Set once the world is built, replaced by a restart.
    setup: Mutex<Option<Arc<ExperimentSetup>>>,
    /// The status the `avida` binary would exit with, once the run is
    /// over ([`Session::exit_code`]).
    exit_code: Mutex<Option<i32>>,
    /// Set by the handle before it queues `Shutdown`: the thread then
    /// skips whatever commands are still queued ahead of it.
    stop: AtomicBool,
    /// How the recording of this session is going, once one was started.
    recording: Mutex<Option<RecordingStatus>>,
    /// Set for a playback session.
    playback: Mutex<Option<PlaybackInfo>>,
}

impl Shared {
    fn new() -> Shared {
        Shared {
            frame: Mutex::new(None),
            frame_generation: AtomicU64::new(0),
            rows: Mutex::new(Vec::new()),
            rows_reset: AtomicBool::new(false),
            events: Mutex::new(Vec::new()),
            status: Mutex::new(Status {
                state: RunState::Paused,
                update: -1,
                slice: None,
                speed: Speed::Max,
                run_to: None,
                run_until: None,
                follow_offspring: false,
                mid_slice: false,
            }),
            setup: Mutex::new(None),
            exit_code: Mutex::new(None),
            stop: AtomicBool::new(false),
            recording: Mutex::new(None),
            playback: Mutex::new(None),
        }
    }
}

/// A mutex guard, also after a panic elsewhere poisoned the mutex (the
/// protected values are plain data that a panic cannot leave half-built).
fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|e| e.into_inner())
}

/// The front end's handle on a running simulation thread.
pub struct Session {
    tx: Sender<Command>,
    shared: Arc<Shared>,
    thread: Option<JoinHandle<()>>,
    /// Disconnected once the thread's body has returned (or unwound): its
    /// sender is dropped there. Nothing is ever sent on it. (In a mutex
    /// only so that the handle stays `Sync`.)
    done: Mutex<Receiver<()>>,
}

impl Session {
    /// Starts the simulation thread and builds the world in it. Returns
    /// once the world exists (paused before the first update, with a first
    /// frame captured), or with the text `avida` would print when the
    /// command line, the configuration or the world set-up fails.
    pub fn spawn(config: SessionConfig) -> Result<Session, String> {
        Session::start(config, None)
    }

    /// [`Session::spawn`] with a [`WakeFn`].
    pub fn spawn_with_wake(config: SessionConfig, wake: WakeFn) -> Result<Session, String> {
        Session::start(config, Some(wake))
    }

    fn start(config: SessionConfig, wake: Option<WakeFn>) -> Result<Session, String> {
        let (tx, rx) = mpsc::channel();
        // Room for the one message the thread sends: it never blocks on it.
        let (ready_tx, ready_rx) = mpsc::sync_channel::<Result<(), String>>(1);
        let (done_tx, done) = mpsc::channel::<()>();
        let shared = Arc::new(Shared::new());
        let thread_shared = Arc::clone(&shared);
        let thread = std::thread::Builder::new()
            .name("avida-session".to_string())
            .stack_size(SIM_STACK_SIZE)
            .spawn(move || {
                let _done = done_tx;
                sim::thread_main(config, rx, thread_shared, wake, ready_tx);
            })
            .map_err(|e| format!("cannot start the simulation thread: {e}"))?;
        match ready_rx.recv() {
            Ok(Ok(())) => Ok(Session {
                tx,
                shared,
                thread: Some(thread),
                done: Mutex::new(done),
            }),
            Ok(Err(e)) => {
                let _ = thread.join();
                Err(e)
            }
            Err(_) => {
                let why = match thread.join() {
                    Err(payload) => sim::panic_message(payload.as_ref()),
                    Ok(()) => "no reason given".to_string(),
                };
                Err(format!(
                    "the simulation thread stopped while building the world: {why}"
                ))
            }
        }
    }

    /// Opens a recording for playback ([`Session::play_with_wake`]).
    pub fn play(path: &std::path::Path) -> Result<Session, String> {
        Session::start_player(path, None)
    }

    /// A session that plays back a recording instead of running a world.
    /// It has the same handle: frames, rows, status and events, and the
    /// commands `Run`, `Pause`, `StepUpdate` (next frame), `StepSlice`
    /// (previous frame), `RunToUpdate` (seek), `SeekFrame` and `SetSpeed`;
    /// the others are answered with an `Error` or, for a subscription,
    /// ignored (a frame has the layers that were recorded). See
    /// [`Session::playback_info`]. Returns once the file is open and its
    /// first frame is published, or with the reason it cannot be played.
    pub fn play_with_wake(path: &std::path::Path, wake: WakeFn) -> Result<Session, String> {
        Session::start_player(path, Some(wake))
    }

    fn start_player(path: &std::path::Path, wake: Option<WakeFn>) -> Result<Session, String> {
        let recording = Recording::open(path).map_err(|e| format!("{}: {e}", path.display()))?;
        if recording.is_empty() {
            return Err(format!(
                "{}: the recording holds no complete frame yet",
                path.display()
            ));
        }
        let (tx, rx) = mpsc::channel();
        let (done_tx, done) = mpsc::channel::<()>();
        let shared = Arc::new(Shared::new());
        let thread_shared = Arc::clone(&shared);
        let (ready_tx, ready_rx) = mpsc::sync_channel::<Result<(), String>>(1);
        let thread = std::thread::Builder::new()
            .name("avida-playback".to_string())
            .spawn(move || {
                let _done = done_tx;
                playback::thread_main(recording, rx, thread_shared, wake, ready_tx);
            })
            .map_err(|e| format!("cannot start the playback thread: {e}"))?;
        match ready_rx.recv() {
            Ok(Ok(())) => Ok(Session {
                tx,
                shared,
                thread: Some(thread),
                done: Mutex::new(done),
            }),
            Ok(Err(e)) => {
                let _ = thread.join();
                Err(e)
            }
            Err(_) => {
                let _ = thread.join();
                Err("the playback thread stopped while opening the recording".to_string())
            }
        }
    }

    /// `Some` for a session that plays a recording.
    pub fn playback_info(&self) -> Option<PlaybackInfo> {
        lock(&self.shared.playback).clone()
    }

    /// The recording of a simulation, once [`Command::Record`] has been
    /// carried out.
    pub fn recording_status(&self) -> Option<RecordingStatus> {
        lock(&self.shared.recording).clone()
    }

    /// Queues a command. Returns `false` if the simulation thread is gone.
    pub fn send(&self, command: Command) -> bool {
        self.tx.send(command).is_ok()
    }

    /// The newest frame.
    pub fn latest_frame(&self) -> Option<Arc<Frame>> {
        lock(&self.shared.frame).clone()
    }

    /// How many frames have been published; changes exactly when
    /// [`Session::latest_frame`] does.
    pub fn frame_generation(&self) -> u64 {
        self.shared.frame_generation.load(Ordering::Acquire)
    }

    /// The rows of the updates finished since the last call, in order.
    pub fn drain_rows(&self) -> Vec<UpdateRow> {
        std::mem::take(&mut *lock(&self.shared.rows))
    }

    /// [`Session::drain_rows`], and whether the rows delivered before are
    /// void: in a playback, a seek backwards (or a replay) restarts the
    /// history, and the rows returned are all of it up to the frame now
    /// shown. A simulation never says so.
    pub fn drain_rows_reset(&self) -> (bool, Vec<UpdateRow>) {
        let mut rows = lock(&self.shared.rows);
        let reset = self.shared.rows_reset.swap(false, Ordering::AcqRel);
        (reset, std::mem::take(&mut *rows))
    }

    /// The events since the last call, in order.
    pub fn drain_events(&self) -> Vec<SessionEvent> {
        std::mem::take(&mut *lock(&self.shared.events))
    }

    pub fn state(&self) -> RunState {
        lock(&self.shared.status).state
    }

    pub fn status(&self) -> Status {
        lock(&self.shared.status).clone()
    }

    /// What the world of the current run was built from: data directory,
    /// effective configuration, events. A [`Command::Restart`] replaces it
    /// (when [`SessionEvent::Restarted`] is published).
    pub fn setup(&self) -> Arc<ExperimentSetup> {
        let setup = lock(&self.shared.setup).clone();
        // `start` returns only once the thread has stored a set-up.
        setup.expect("a session has a set-up once spawned")
    }

    /// Where the current run writes its output (`DATA_DIR`).
    pub fn data_dir(&self) -> PathBuf {
        self.setup().data_dir.clone()
    }

    /// The status the `avida` binary would exit with for this run, once it
    /// is over: 0 when it [finished](RunState::Finished); when it
    /// [failed](RunState::Failed), the code a `Driver().Abort(...)` asked
    /// for, or 1 (the binary's `error:` exit). `None` while the run can
    /// still advance; a [`Command::Restart`] clears it.
    pub fn exit_code(&self) -> Option<i32> {
        *lock(&self.shared.exit_code)
    }

    /// Stops the thread as dropping the handle does, but waits for it at
    /// most `timeout`. The thread stops within one slice batch, also while
    /// running or waiting for a throttled update, unless it is inside one
    /// long step of the simulation (an update's events, a restart building
    /// a world). Returns whether it stopped; if it did not, it is left to
    /// run on its own (dropping the handle no longer waits for it), and a
    /// front end that is exiting should end the process.
    pub fn shutdown(&mut self, timeout: Duration) -> bool {
        self.shared.stop.store(true, Ordering::Release);
        let _ = self.tx.send(Command::Shutdown);
        let Some(thread) = self.thread.take() else {
            return true;
        };
        let done = self.done.get_mut().unwrap_or_else(|e| e.into_inner());
        match done.recv_timeout(timeout) {
            Err(RecvTimeoutError::Timeout) => false,
            Ok(()) | Err(RecvTimeoutError::Disconnected) => {
                let _ = thread.join();
                true
            }
        }
    }
}

/// How long dropping a [`Session`] waits for the thread before detaching
/// it.
pub const DROP_WAIT: Duration = Duration::from_secs(2);

impl Drop for Session {
    /// Stops the thread (within one slice batch, also while running or
    /// waiting for a throttled update) and waits for it at most
    /// [`DROP_WAIT`]; a thread still inside one long step is detached. It
    /// owns its world and an `Arc` of the shared state, so it can finish
    /// (and flush its output files) without the handle. A no-op after
    /// [`Session::shutdown`].
    fn drop(&mut self) {
        if self.thread.is_some() {
            self.shutdown(DROP_WAIT);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_handle_and_its_outputs_cross_threads() {
        fn send_sync<T: Send + Sync>() {}
        send_sync::<Session>();
        send_sync::<Arc<Frame>>();
        send_sync::<UpdateRow>();
        send_sync::<SessionEvent>();
        send_sync::<Command>();
        send_sync::<Arc<ExperimentSetup>>();
    }

    #[test]
    fn only_a_positive_finite_rate_throttles() {
        assert_eq!(Speed::Max.period(), None);
        assert_eq!(Speed::UpdatesPerSecond(0.0).period(), None);
        assert_eq!(Speed::UpdatesPerSecond(f64::NAN).period(), None);
        assert_eq!(
            Speed::UpdatesPerSecond(4.0).period(),
            Some(Duration::from_millis(250))
        );
    }
}
