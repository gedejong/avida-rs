//! The simulation thread: builds the world, then carries out commands.

use std::any::Any;
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::path::PathBuf;
use std::sync::atomic::Ordering;
use std::sync::mpsc::{Receiver, RecvTimeoutError, SyncSender, TryRecvError};
use std::sync::Arc;
use std::time::{Duration, Instant};

use avida_apto::diag;
use avida_config::cmdline::{
    prints_data_directory, process_cmdline, random_seed_line, CmdLineOutcome,
};
use avida_config::config::{find_entry, Config};
use avida_config::init_file::InitFile;
use avida_viewmodel::{
    capture_with_progress, find_organism, ip_positions, ip_sites, probe, update_row, OrgProbe,
    Subscription, UpdateProgress,
};
use avida_world::births::Birth;
use avida_world::run::{Stepper, WatchedStep};
use avida_world::world::World;

use crate::{
    lock, Command, EventLine, ExperimentSetup, RunState, RunUntil, SessionConfig, SessionEvent,
    Setting, Shared, Speed, Status, StopWhen, UntilOutcome, WakeFn, MIN_FRAME_INTERVAL,
    SLICE_BATCH,
};

/// The text of a panic payload.
pub(crate) fn panic_message(payload: &(dyn Any + Send)) -> String {
    if let Some(s) = payload.downcast_ref::<&str>() {
        (*s).to_string()
    } else if let Some(s) = payload.downcast_ref::<String>() {
        s.clone()
    } else {
        "panic with a non-text payload".to_string()
    }
}

/// One event per line of `text`.
fn push_lines(events: &mut Vec<SessionEvent>, text: &str, make: fn(String) -> SessionEvent) {
    events.extend(text.lines().map(|l| make(l.to_string())));
}

/// The settings named on the command line: `-set NAME value` (any of a
/// setting's names), `-s` (`RANDOM_SEED`) and `-vN` (`VERBOSITY`).
fn command_line_settings(args: &[String]) -> Vec<&'static str> {
    let mut names = Vec::new();
    let mut i = 0;
    while i < args.len() {
        let a = args[i].as_str();
        if a == "-set" && i + 2 < args.len() {
            let name = args[i + 1].as_str();
            if let Some(e) = avida_config::config::all_entries().find(|e| e.names.contains(&name)) {
                names.push(e.names[0]);
            }
            i += 3;
            continue;
        }
        if a == "-def" || a == "-c" || a == "-config" {
            i += if a == "-def" { 3 } else { 2 };
            continue;
        }
        if a == "-s" || a == "-seed" {
            names.push("RANDOM_SEED");
            i += 2;
            continue;
        }
        if a.starts_with("-v") && a != "-v" && a != "-version" {
            names.push("VERBOSITY");
        }
        i += 1;
    }
    names
}

/// The effective configuration, setting by setting.
fn settings_of(cmd: &CmdLineOutcome, args: &[String]) -> Vec<Setting> {
    let defaults = Config::new();
    let from_cmdline = command_line_settings(args);
    cmd.config
        .iter()
        .map(|e| {
            let name = e.def.names[0];
            Setting {
                group: e.group.name,
                group_desc: e.group.desc,
                name,
                kind: e.def.ty.type_name(),
                default: find_entry(name)
                    .and_then(|_| defaults.get(name))
                    .unwrap_or_else(|| e.def.default.to_string()),
                value: e.value,
                desc: e.def.desc,
                from_command_line: from_cmdline.contains(&name),
            }
        })
        .collect()
}

/// The events file's lines, split into trigger, timing, action and
/// arguments the way `cEventList::AddEventFileFormat` reads them.
fn events_of(event_file: &str, defs: &[(String, String)], cwd: &str) -> Vec<EventLine> {
    let file = InitFile::with_mappings(event_file, defs, cwd);
    (0..file.num_lines())
        .map(|i| {
            let text = file.get_line(i as i32);
            let mut words = text.split_whitespace();
            let trigger = words.next().unwrap_or("").to_string();
            let immediate = matches!(trigger.as_str(), "i" | "immediate");
            let timing = if immediate {
                String::new()
            } else {
                words.next().unwrap_or("").to_string()
            };
            let action = words.next().unwrap_or("").to_string();
            let args = words.collect::<Vec<_>>().join(" ");
            EventLine {
                text: text.trim().to_string(),
                trigger,
                timing,
                action,
                args,
            }
        })
        .collect()
}

/// `primitive.cc` up to `Avida2Driver::Run`: the command line, the
/// configuration, `cWorld::Initialize`, and the lines printed on the way
/// (all but the banner, which a front end shows its own way).
fn build_world(
    config: &SessionConfig,
) -> Result<(World, Vec<SessionEvent>, ExperimentSetup), String> {
    let work_dir = std::path::absolute(&config.work_dir)
        .map_err(|e| format!("working directory {}: {e}", config.work_dir.display()))?;
    if !work_dir.is_dir() {
        return Err(format!(
            "working directory {} does not exist",
            work_dir.display()
        ));
    }
    let cwd = work_dir
        .to_str()
        .ok_or_else(|| format!("working directory {} is not UTF-8", work_dir.display()))?
        .to_string();

    let mut argv = Vec::with_capacity(config.args.len() + 1);
    argv.push("avida".to_string());
    argv.extend(config.args.iter().cloned());
    let cmd = process_cmdline(&argv, &cwd);
    if let Some(code) = cmd.exit {
        let text = format!("{}{}", cmd.stdout, cmd.stderr);
        let text = text.trim_end();
        return Err(if text.is_empty() {
            format!("the command line makes avida exit with status {code}")
        } else {
            text.to_string()
        });
    }
    if cmd.config.analyze_mode > 0 {
        return Err(
            "ANALYZE_MODE > 0 runs an analyze script, not a population; a session cannot drive it"
                .to_string(),
        );
    }

    let mut events = Vec::new();
    push_lines(&mut events, &cmd.stdout, SessionEvent::Stdout);
    push_lines(&mut events, &cmd.stderr, SessionEvent::Stderr);
    let settings = settings_of(&cmd, &config.args);
    let event_file = cmd.config.event_file.clone();
    let event_lines = events_of(&event_file, &cmd.defs, &cwd);
    let config_file = work_dir.join(&cmd.config_filename);
    // What set-up prints through `avida_apto::diag` (the thread captures
    // it), in the order the binary prints it: before the report.
    let built = World::new(cmd.config, &cwd, &cmd.defs);
    let printed = diag::take_captured();
    let (world, report) = built.map_err(|e| {
        format!("{}{}{}{e}", cmd.stderr, printed.stdout, printed.stderr)
            .trim_end()
            .to_string()
    })?;
    push_lines(&mut events, &printed.stdout, SessionEvent::Stdout);
    push_lines(&mut events, &printed.stderr, SessionEvent::Stderr);
    push_lines(&mut events, &report.stdout, SessionEvent::Stdout);
    push_lines(&mut events, &report.stderr, SessionEvent::Stderr);
    push_lines(
        &mut events,
        &random_seed_line(world.cfg.random_seed, world.rng.seed()),
        SessionEvent::Stdout,
    );
    if prints_data_directory(&world.cfg) {
        events.push(SessionEvent::Stdout(format!(
            "Data Directory: {}",
            world.out.output_path()
        )));
    }
    events.push(SessionEvent::Stdout(String::new()));
    let setup = ExperimentSetup {
        work_dir: work_dir.clone(),
        args: config.args.clone(),
        data_dir: PathBuf::from(world.out.output_path()),
        config_file,
        settings,
        event_file: work_dir.join(&event_file),
        events: event_lines,
    };
    Ok((world, events, setup))
}

/// [`build_world`], a panic included.
fn build_world_guarded(
    config: &SessionConfig,
) -> Result<(World, Vec<SessionEvent>, ExperimentSetup), String> {
    let built = catch_unwind(AssertUnwindSafe(|| build_world(config))).unwrap_or_else(|p| {
        Err(format!(
            "panic while building the world: {}",
            panic_message(p.as_ref())
        ))
    });
    // Nothing printed while failing is left for the next run's lines.
    diag::take_captured();
    built
}

/// The body of the simulation thread. Reports the outcome of building the
/// world through `ready`, then serves commands until `Shutdown` or until the
/// handle is gone.
pub(crate) fn thread_main(
    config: SessionConfig,
    rx: Receiver<Command>,
    shared: Arc<Shared>,
    wake: Option<WakeFn>,
    ready: SyncSender<Result<(), String>>,
) {
    // Everything the simulation prints on this thread becomes events.
    diag::start_capture();
    let (world, setup_events, setup) = match build_world_guarded(&config) {
        Ok(built) => built,
        Err(e) => {
            let _ = ready.send(Err(e));
            return;
        }
    };
    let mut sim = Sim::new(world, config, shared, wake);
    *lock(&sim.shared.setup) = Some(Arc::new(setup));
    lock(&sim.shared.events).extend(setup_events);
    sim.publish_status();
    sim.publish_frame();
    sim.push_event(SessionEvent::Paused {
        update: sim.world.stats.update(),
    });
    if ready.send(Ok(())).is_ok() {
        sim.serve(&rx);
    }
    sim.world.out.flush_all();
}

enum Flow {
    Continue,
    Exit,
}

struct Sim {
    world: World,
    stepper: Stepper,
    /// What the world was built from (a restart keeps the working
    /// directory).
    config: SessionConfig,
    shared: Arc<Shared>,
    wake: Option<WakeFn>,
    sub: Subscription,
    mode: RunState,
    speed: Speed,
    /// Target of a `RunToUpdate` in force.
    run_to: Option<i32>,
    /// When the update in progress began (while running).
    update_began: Instant,
    /// Earliest start of the next update under a throttle.
    next_due: Option<Instant>,
    last_frame: Option<Instant>,
    /// What the stepper and the simulation printed to stdout and has not
    /// been forwarded yet.
    stdout: Vec<u8>,
    /// What the simulation printed to stderr and has not been forwarded
    /// yet (a last unterminated line).
    stderr: String,
    /// A panic left the world in an unknown state: no more captures or
    /// saves.
    poisoned: bool,
    /// The `RunUntil` in force.
    until: Option<Until>,
    /// How the `RunUntil` in force ended, set between two slices and acted
    /// on after the batch.
    until_ended: Option<UntilOutcome>,
    /// `FollowOffspring` is on.
    follow: bool,
}

/// A `RunUntil` in progress.
struct Until {
    spec: RunUntil,
    /// The organism's lifetime CPU cycles when the watch began (or moved to
    /// an offspring) and after its last instruction.
    start_cycles: u64,
    last_cycles: u64,
    slices: u64,
}

impl Sim {
    fn new(world: World, config: SessionConfig, shared: Arc<Shared>, wake: Option<WakeFn>) -> Sim {
        let mut world = world;
        world.record_births(true);
        let stepper = Stepper::new(&world);
        Sim {
            world,
            stepper,
            config,
            shared,
            wake,
            sub: Subscription::default(),
            mode: RunState::Paused,
            speed: Speed::Max,
            run_to: None,
            update_began: Instant::now(),
            next_due: None,
            last_frame: None,
            stdout: Vec::new(),
            stderr: String::new(),
            poisoned: false,
            until: None,
            until_ended: None,
            follow: false,
        }
    }

    /// The command loop: blocks while paused; while running, alternates
    /// slice batches with draining the channel.
    fn serve(&mut self, rx: &Receiver<Command>) {
        loop {
            if self.mode != RunState::Running {
                match rx.recv() {
                    Ok(cmd) => {
                        if let Flow::Exit = self.handle(cmd) {
                            return;
                        }
                    }
                    Err(_) => return,
                }
                continue;
            }
            loop {
                match rx.try_recv() {
                    Ok(cmd) => {
                        if let Flow::Exit = self.handle(cmd) {
                            return;
                        }
                    }
                    Err(TryRecvError::Empty) => break,
                    Err(TryRecvError::Disconnected) => return,
                }
            }
            if self.mode != RunState::Running {
                continue;
            }
            if let Some(wait) = self.advance() {
                match rx.recv_timeout(wait) {
                    Ok(cmd) => {
                        if let Flow::Exit = self.handle(cmd) {
                            return;
                        }
                    }
                    Err(RecvTimeoutError::Timeout) => {}
                    Err(RecvTimeoutError::Disconnected) => return,
                }
            }
        }
    }

    fn handle(&mut self, cmd: Command) -> Flow {
        if matches!(
            cmd,
            Command::Run
                | Command::RunToUpdate(_)
                | Command::Pause
                | Command::StepUpdate
                | Command::StepSlice
                | Command::RunUntil(_)
                | Command::Restart { .. }
        ) {
            self.end_until(UntilOutcome::Interrupted);
        }
        match cmd {
            Command::Shutdown => return Flow::Exit,
            Command::Run => self.run(None),
            Command::RunToUpdate(n) => self.run(Some(n)),
            Command::Pause => {
                if matches!(self.mode, RunState::Running | RunState::Paused) {
                    self.enter_paused();
                }
            }
            Command::StepUpdate => self.step_update(),
            Command::StepSlice => self.step_slice(),
            Command::SetSpeed(speed) => {
                self.speed = speed;
                self.next_due = None;
                self.publish_status();
            }
            Command::Subscribe(sub) => {
                self.sub = sub;
                self.publish_frame();
            }
            Command::WatchCell(cell) => {
                self.sub.inspect_cell = cell;
                self.publish_frame();
            }
            Command::SavePopulation => self.save_population(),
            Command::RunUntil(spec) => self.run_until(spec),
            Command::FollowOffspring(on) => {
                self.follow = on;
                self.publish_status();
            }
            Command::Restart { args } => self.restart(args),
        }
        Flow::Continue
    }

    /// `Command::Restart`: a new world in the same working directory, or
    /// the old run going on (paused) when it cannot be built.
    fn restart(&mut self, args: Vec<String>) {
        // The old run's output, complete, before the new world opens the
        // same files.
        self.forward_stdout_tail();
        self.world.out.flush_all();
        let config = SessionConfig {
            work_dir: self.config.work_dir.clone(),
            args,
        };
        let (world, setup_events, setup) = match build_world_guarded(&config) {
            Ok(built) => built,
            Err(e) => {
                self.push_event(SessionEvent::RestartFailed(e));
                if self.mode == RunState::Running {
                    self.enter_paused();
                }
                return;
            }
        };
        let mut new = Sim::new(world, config, Arc::clone(&self.shared), self.wake.take());
        new.speed = self.speed;
        new.sub = Subscription {
            inspect_cell: None,
            lineage_of: None,
            ..std::mem::take(&mut self.sub)
        };
        // The old world goes here, its files flushed above.
        *self = new;
        let setup = Arc::new(setup);
        *lock(&self.shared.setup) = Some(Arc::clone(&setup));
        *lock(&self.shared.exit_code) = None;
        lock(&self.shared.rows).clear();
        {
            let mut events = lock(&self.shared.events);
            events.push(SessionEvent::Restarted(setup));
            events.extend(setup_events);
        }
        self.enter_paused();
    }

    // --- commands ---------------------------------------------------------

    fn run(&mut self, target: Option<i32>) {
        if !self.can_advance("run") {
            return;
        }
        self.run_to = target;
        if self.target_reached() {
            self.enter_paused();
            return;
        }
        self.mode = RunState::Running;
        self.next_due = None;
        self.update_began = Instant::now();
        self.publish_status();
        self.push_event(SessionEvent::Running);
    }

    fn step_update(&mut self) {
        if !self.can_advance("step") {
            return;
        }
        self.mode = RunState::Paused;
        self.run_to = None;
        if !self.stepper.in_update() && !self.begin() {
            return;
        }
        // The slices go through `one_slice` (rather than `end_update`'s own
        // loop) so following offspring sees each of them.
        let slices = self.guarded(|s| {
            while s.one_slice()? {}
            Ok(())
        });
        match slices {
            Ok(()) => {
                if self.end() {
                    self.enter_paused();
                }
            }
            Err(e) => self.enter_failed(e),
        }
    }

    fn step_slice(&mut self) {
        if !self.can_advance("step") {
            return;
        }
        self.mode = RunState::Paused;
        self.run_to = None;
        if !self.stepper.in_update() && !self.begin() {
            return;
        }
        match self.guarded(|s| s.one_slice()) {
            Ok(true) => self.enter_paused(),
            Ok(false) => {
                if self.end() {
                    self.enter_paused();
                }
            }
            Err(e) => self.enter_failed(e),
        }
    }

    fn run_until(&mut self, spec: RunUntil) {
        if !self.can_advance("run") {
            return;
        }
        self.run_to = None;
        let start = probe(&self.world, spec.cell).filter(|p| p.org_id == spec.org_id);
        let Some(start) = start else {
            let occupant = probe(&self.world, spec.cell).map(|p| p.org_id);
            self.push_event(SessionEvent::RunUntilEnded {
                outcome: UntilOutcome::OrganismGone { occupant },
                slices: 0,
                update: self.world.stats.update(),
            });
            self.enter_paused();
            return;
        };
        self.until = Some(Until {
            spec,
            start_cycles: start.cpu_cycles,
            last_cycles: start.cpu_cycles,
            slices: 0,
        });
        self.until_ended = None;
        self.mode = RunState::Running;
        self.next_due = None;
        self.update_began = Instant::now();
        self.publish_status();
        self.push_event(SessionEvent::Running);
    }

    /// Ends the `RunUntil` in force, if any, with `outcome`.
    fn end_until(&mut self, outcome: UntilOutcome) {
        self.until_ended = None;
        if let Some(u) = self.until.take() {
            self.push_event(SessionEvent::RunUntilEnded {
                outcome,
                slices: u.slices,
                update: self.world.stats.update(),
            });
        }
    }

    /// `cActionSavePopulation::Process` with its default arguments. Reads
    /// the world and writes one file; a failure leaves the run as it was.
    fn save_population(&mut self) {
        if self.poisoned {
            self.push_event(SessionEvent::Error(
                "cannot save the population: a panic left the world in an unknown state"
                    .to_string(),
            ));
            return;
        }
        let name = format!("detail-{}.spop", self.world.stats.update());
        let world = &mut self.world;
        let result = catch_unwind(AssertUnwindSafe(|| {
            world.save_population(&name, true, false, false, false)
        }))
        .unwrap_or_else(|p| Err(format!("panic: {}", panic_message(p.as_ref()))));
        let event = match result {
            Ok(()) => SessionEvent::PopulationSaved(
                PathBuf::from(self.world.out.output_path()).join(name),
            ),
            Err(e) => SessionEvent::Error(format!("SavePopulation: {e}")),
        };
        self.push_event(event);
    }

    /// Refuses to advance a run that is over, with an `Error` event.
    fn can_advance(&mut self, what: &str) -> bool {
        let why = match self.mode {
            RunState::Paused | RunState::Running => return true,
            RunState::Finished => format!(
                "cannot {what}: the run finished at update {}",
                self.world.stats.update()
            ),
            RunState::Failed => format!("cannot {what}: the run stopped on a fatal error"),
        };
        self.push_event(SessionEvent::Error(why));
        false
    }

    // --- running ----------------------------------------------------------

    /// One unit of work while running: begin an update, or run a batch of
    /// slices, or end an update. Returns how long to wait for a command
    /// when a throttle holds the next update back.
    fn advance(&mut self) -> Option<Duration> {
        if !self.stepper.in_update() {
            if self.target_reached() {
                self.enter_paused();
                return None;
            }
            if self.until.is_none() {
                if let Some(wait) = self.pace_wait() {
                    return Some(wait);
                }
            }
            self.update_began = Instant::now();
            if !self.begin() {
                return None;
            }
        }
        let batch = self.guarded(|s| {
            for _ in 0..SLICE_BATCH {
                if !s.one_slice()? {
                    return Ok(false);
                }
                if s.until_ended.is_some() {
                    break;
                }
            }
            Ok(true)
        });
        if let Some(outcome) = self.until_ended.take() {
            if batch.is_ok() {
                self.end_until(outcome);
                self.enter_paused();
                return None;
            }
        }
        match batch {
            Ok(true) => self.publish_status(),
            Ok(false) => {
                if self.end() {
                    if let Some(period) = self.speed.period() {
                        self.next_due = Some(self.update_began + period);
                    }
                    if self.target_reached() {
                        self.enter_paused();
                    } else {
                        self.publish_status();
                        if self
                            .last_frame
                            .is_none_or(|t| t.elapsed() >= MIN_FRAME_INTERVAL)
                        {
                            self.publish_frame();
                        }
                    }
                }
            }
            Err(e) => self.enter_failed(e),
        }
        None
    }

    /// One step of the run, then the watches: births from the inspected
    /// organism (and following them), and the `RunUntil` in force. While a
    /// `RunUntil` is in force a step is one instruction of its organism
    /// (`Stepper::step_watched`), otherwise a time slice. Returns whether
    /// anything ran.
    fn one_slice(&mut self) -> Result<bool, String> {
        let parent = self
            .sub
            .inspect_cell
            .and_then(|c| probe(&self.world, c))
            .map(|p| p.org_id);
        let slices_before = self.stepper.slice_progress().0;
        let step = match self.until.as_ref().map(|u| u.spec.cell) {
            Some(cell) => self.stepper.step_watched(&mut self.world, cell)?,
            None if self.stepper.step_slice(&mut self.world)? => WatchedStep::Slice,
            None => WatchedStep::UpdateDone,
        };
        if step == WatchedStep::UpdateDone {
            return Ok(false);
        }
        let births = self.world.take_births();
        if let Some(parent) = parent {
            self.after_births(parent, &births);
        }
        if self.until.is_some() {
            // A step that only went on with an open slice is no new slice.
            let new_slice =
                step == WatchedStep::Slice || self.stepper.slice_progress().0 != slices_before;
            self.check_until(new_slice);
        }
        Ok(true)
    }

    /// `Born` for each offspring of the inspected organism `parent`; with
    /// `FollowOffspring`, the inspected cell moves to the first.
    fn after_births(&mut self, parent: i32, births: &[Birth]) {
        let slice = self.stepper.slice_progress().0;
        let mut follow = self.follow;
        for b in births.iter().filter(|b| b.parent_id == parent) {
            self.push_event(SessionEvent::Born {
                parent_id: b.parent_id,
                parent_cell: b.parent_cell,
                org_id: b.org_id,
                cell: b.cell,
                update: b.update,
                slice,
            });
            if !follow {
                continue;
            }
            follow = false;
            let (Some(from), Some(child)) = (self.sub.inspect_cell, probe(&self.world, b.cell))
            else {
                continue;
            };
            if child.org_id == b.org_id {
                self.move_watch(from, child, true);
            }
        }
    }

    /// Moves the inspected cell (and a `RunUntil` watching the same
    /// organism) to `to`.
    fn move_watch(&mut self, from: i32, to: OrgProbe, offspring: bool) {
        if self.sub.inspect_cell == Some(from) {
            self.sub.inspect_cell = Some(to.cell);
        }
        if let Some(u) = &mut self.until {
            if u.spec.cell == from {
                u.spec.cell = to.cell;
                if u.spec.org_id != to.org_id {
                    u.spec.org_id = to.org_id;
                    u.start_cycles = to.cpu_cycles;
                }
                u.last_cycles = to.cpu_cycles;
            }
        }
        self.push_event(SessionEvent::WatchMoved {
            from,
            to: to.cell,
            org_id: to.org_id,
            offspring,
        });
    }

    /// Whether the `RunUntil` in force has ended; records how in
    /// `until_ended`. `new_slice`: the step began a time slice (counted
    /// against the budget). Only reads the world.
    fn check_until(&mut self, new_slice: bool) {
        let Some(u) = &mut self.until else {
            return;
        };
        u.slices += u64::from(new_slice);
        let (cell, org_id) = (u.spec.cell, u.spec.org_id);
        let mut now = probe(&self.world, cell);
        if now.is_none_or(|p| p.org_id != org_id) {
            // Moved (`SwapCells`), or gone.
            match find_organism(&self.world, org_id).and_then(|c| probe(&self.world, c)) {
                Some(moved) => {
                    self.move_watch(cell, moved, false);
                    now = Some(moved);
                }
                None => {
                    self.until_ended = Some(UntilOutcome::OrganismGone {
                        occupant: now.map(|p| p.org_id),
                    });
                    return;
                }
            }
        }
        let Some(u) = &mut self.until else {
            return;
        };
        let Some(now) = now else {
            return;
        };
        let ran = now.cpu_cycles != u.last_cycles;
        u.last_cycles = now.cpu_cycles;
        let reached = ran
            && match &u.spec.stop {
                StopWhen::Cycles(n) => {
                    now.cpu_cycles.saturating_sub(u.start_cycles) >= u64::from((*n).max(1))
                }
                StopWhen::IpAt(positions) => ip_positions(&self.world, now.cell)
                    .iter()
                    .any(|ip| positions.contains(ip)),
                StopWhen::IpIn(sites) => ip_sites(&self.world, now.cell)
                    .iter()
                    .any(|ip| sites.contains(ip)),
            };
        if reached {
            self.until_ended = Some(UntilOutcome::Reached);
        } else if u.slices >= u.spec.max_slices {
            self.until_ended = Some(UntilOutcome::BudgetExhausted);
        }
    }

    /// Between updates, at or past the `RunToUpdate` target.
    fn target_reached(&self) -> bool {
        !self.stepper.in_update() && self.run_to.is_some_and(|t| self.world.stats.update() >= t)
    }

    /// Time left before a throttled run may begin the next update.
    fn pace_wait(&self) -> Option<Duration> {
        self.speed.period()?;
        let due = self.next_due?;
        due.checked_duration_since(Instant::now())
            .filter(|d| !d.is_zero())
    }

    // --- stepping the world -----------------------------------------------

    /// Runs `f` on the simulation, then forwards what it printed; a panic
    /// becomes an error and poisons the world.
    fn guarded<T>(&mut self, f: impl FnOnce(&mut Sim) -> Result<T, String>) -> Result<T, String> {
        let r = match catch_unwind(AssertUnwindSafe(|| f(self))) {
            Ok(r) => r,
            Err(p) => {
                self.poisoned = true;
                Err(format!("panic: {}", panic_message(p.as_ref())))
            }
        };
        self.forward_stdout();
        r
    }

    /// `Stepper::begin_update`. True when an update is now in progress;
    /// otherwise the session has finished or failed.
    fn begin(&mut self) -> bool {
        let r = self.guarded(|s| s.stepper.begin_update(&mut s.world, &mut s.stdout));
        self.forward_stdout();
        match r {
            Ok(true) => true,
            Ok(false) => {
                self.enter_finished();
                false
            }
            Err(e) => {
                self.enter_failed(e);
                false
            }
        }
    }

    /// `Stepper::end_update` and the update's row. True when the run can go
    /// on; otherwise the session has finished or failed.
    fn end(&mut self) -> bool {
        let r = self.guarded(|s| {
            s.stepper.end_update(&mut s.world, &mut s.stdout)?;
            Ok(update_row(&s.world))
        });
        self.forward_stdout();
        match r {
            Ok(row) => {
                lock(&self.shared.rows).push(row);
                // `Avida2Driver::Run` leaves its loop here when the
                // population died out; `begin_update` would only say so.
                if self.world.done {
                    self.enter_finished();
                    return false;
                }
                true
            }
            Err(e) => {
                self.enter_failed(e);
                false
            }
        }
    }

    // --- state changes ----------------------------------------------------

    fn enter_paused(&mut self) {
        self.mode = RunState::Paused;
        self.run_to = None;
        self.world.out.flush_all();
        self.publish_status();
        self.publish_frame();
        self.push_event(SessionEvent::Paused {
            update: self.world.stats.update(),
        });
    }

    fn enter_finished(&mut self) {
        self.end_until(UntilOutcome::RunOver);
        self.mode = RunState::Finished;
        self.run_to = None;
        *lock(&self.shared.exit_code) = Some(0);
        self.forward_stdout_tail();
        self.world.out.flush_all();
        self.publish_status();
        self.publish_frame();
        self.push_event(SessionEvent::Finished {
            update: self.world.stats.update(),
        });
    }

    fn enter_failed(&mut self, error: String) {
        self.end_until(UntilOutcome::RunOver);
        self.mode = RunState::Failed;
        self.run_to = None;
        // `Driver().Abort(code)`: the binary exits with that code, after
        // printing what the aborting update had to say; any other error
        // is the binary's `error: ...` and status 1.
        let abort = self.world.action_state.exit_code;
        *lock(&self.shared.exit_code) = Some(abort.unwrap_or(1));
        if abort.is_some() {
            let said = std::mem::take(&mut self.world.action_state.stdout);
            self.stdout.extend_from_slice(said.as_bytes());
        }
        self.forward_stdout_tail();
        self.world.out.flush_all();
        self.publish_status();
        self.publish_frame();
        self.push_event(SessionEvent::Fatal(error));
    }

    // --- publishing -------------------------------------------------------

    fn wake(&self) {
        if let Some(wake) = &self.wake {
            wake();
        }
    }

    fn push_event(&self, event: SessionEvent) {
        lock(&self.shared.events).push(event);
        self.wake();
    }

    fn publish_status(&self) {
        let status = Status {
            state: self.mode,
            update: self.world.stats.update(),
            slice: self
                .stepper
                .in_update()
                .then(|| self.stepper.slice_progress()),
            speed: self.speed,
            run_to: self.run_to,
            run_until: self.until.as_ref().map(|u| (u.slices, u.spec.max_slices)),
            follow_offspring: self.follow,
            mid_slice: self.stepper.open_slice().is_some(),
        };
        *lock(&self.shared.status) = status;
    }

    /// Captures a frame for the current subscription. Capturing only reads
    /// the world, so a panic in it costs the subscription, not the run.
    fn publish_frame(&mut self) {
        if self.poisoned {
            return;
        }
        let (world, sub) = (&self.world, &self.sub);
        let progress = if self.stepper.in_update() {
            let (done, total) = self.stepper.slice_progress();
            UpdateProgress::inside(done, total)
        } else {
            UpdateProgress::default()
        };
        match catch_unwind(AssertUnwindSafe(|| {
            capture_with_progress(world, sub, progress)
        })) {
            Ok(frame) => {
                *lock(&self.shared.frame) = Some(Arc::new(frame));
                self.shared.frame_generation.fetch_add(1, Ordering::Release);
                self.last_frame = Some(Instant::now());
                self.wake();
            }
            Err(p) => {
                self.sub = Subscription::default();
                self.push_event(SessionEvent::Error(format!(
                    "capturing a frame panicked ({}); the subscription was reset",
                    panic_message(p.as_ref())
                )));
            }
        }
    }

    /// Forwards the complete lines the stepper and the simulation printed
    /// (the simulation's stderr lines first: they were printed during the
    /// step, the stepper's stdout mostly at its end).
    fn forward_stdout(&mut self) {
        let printed = diag::take_captured();
        self.stdout.extend_from_slice(printed.stdout.as_bytes());
        self.stderr.push_str(&printed.stderr);
        let mut events = lock(&self.shared.events);
        if let Some(last) = self.stderr.rfind('\n') {
            let text: String = self.stderr.drain(..=last).collect();
            push_lines(&mut events, &text, SessionEvent::Stderr);
        }
        let Some(last) = self.stdout.iter().rposition(|&b| b == b'\n') else {
            return;
        };
        let text: Vec<u8> = self.stdout.drain(..=last).collect();
        for line in text[..last].split(|&b| b == b'\n') {
            events.push(SessionEvent::Stdout(
                String::from_utf8_lossy(line).into_owned(),
            ));
        }
    }

    /// Forwards everything printed, last unterminated lines included.
    fn forward_stdout_tail(&mut self) {
        self.forward_stdout();
        let mut events = lock(&self.shared.events);
        if !self.stderr.is_empty() {
            events.push(SessionEvent::Stderr(std::mem::take(&mut self.stderr)));
        }
        if !self.stdout.is_empty() {
            let line = String::from_utf8_lossy(&self.stdout).into_owned();
            self.stdout.clear();
            events.push(SessionEvent::Stdout(line));
        }
    }
}
