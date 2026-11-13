//! The playback thread: a [`Session`](crate::Session) whose frames come
//! from a recording instead of a world.
//!
//! It publishes what a simulation does (frames, rows, status, events), so
//! a front end drives it with the same calls. Frames are the recorded
//! ones; rows are the statistics rows of the records up to the one on show
//! (`Session::drain_rows_reset` tells a chart to start again after a seek
//! backwards). Commands that need a world are answered with an error event.

use std::path::Path;
use std::sync::atomic::Ordering;
use std::sync::mpsc::{Receiver, RecvTimeoutError, SyncSender, TryRecvError};
use std::sync::Arc;
use std::time::{Duration, Instant};

use avida_viewmodel::UpdateRow;

use crate::recording::{Recording, Tail};
use crate::{
    lock, Command, ExperimentSetup, PlaybackInfo, RunState, SessionEvent, Shared, Speed, Status,
    WakeFn, MIN_FRAME_INTERVAL,
};

/// Longest a frame stays up while playing, whatever the speed asks.
const MAX_FRAME_PERIOD: Duration = Duration::from_secs(10);

pub(crate) fn thread_main(
    rec: Recording,
    rx: Receiver<Command>,
    shared: Arc<Shared>,
    wake: Option<WakeFn>,
    ready: SyncSender<Result<(), String>>,
) {
    let mut p = Player {
        rec,
        shared,
        wake,
        index: 0,
        mode: RunState::Paused,
        speed: Speed::Max,
        next_due: None,
        rows: Vec::new(),
        rows_sent: 0,
    };
    let path = std::path::absolute(p.rec.path()).unwrap_or_else(|_| p.rec.path().to_path_buf());
    let dir = path
        .parent()
        .map_or_else(|| path.clone(), Path::to_path_buf);
    *lock(&p.shared.setup) = Some(Arc::new(ExperimentSetup {
        work_dir: dir.clone(),
        args: p.rec.meta().args.clone(),
        data_dir: dir,
        config_file: path.clone(),
        settings: Vec::new(),
        event_file: path,
        events: Vec::new(),
    }));
    if let Err(e) = p.show() {
        let _ = ready.send(Err(e));
        return;
    }
    p.push_event(SessionEvent::Paused {
        update: p.rec.update_of(0).unwrap_or(-1),
    });
    if ready.send(Ok(())).is_ok() {
        p.serve(&rx);
    }
}

struct Player {
    rec: Recording,
    shared: Arc<Shared>,
    wake: Option<WakeFn>,
    index: usize,
    mode: RunState,
    speed: Speed,
    next_due: Option<Instant>,
    /// The rows of records `0..rows.len()` (read as they are needed).
    rows: Vec<UpdateRow>,
    /// How many of them the rows queue of the session has had since its
    /// last reset.
    rows_sent: usize,
}

impl Player {
    fn serve(&mut self, rx: &Receiver<Command>) {
        loop {
            if self.shared.stop.load(Ordering::Acquire) {
                return;
            }
            let first = if self.mode == RunState::Running {
                let wait = self.next_due.map_or(Duration::ZERO, |d| {
                    d.saturating_duration_since(Instant::now())
                });
                match rx.recv_timeout(wait) {
                    Ok(c) => Some(c),
                    Err(RecvTimeoutError::Timeout) => {
                        self.tick();
                        continue;
                    }
                    Err(RecvTimeoutError::Disconnected) => return,
                }
            } else {
                match rx.recv() {
                    Ok(c) => Some(c),
                    Err(_) => return,
                }
            };
            // Everything queued is applied before the frame is decoded, so
            // a scrub that sends many seeks costs one frame.
            let mut next = first;
            let mut moved = false;
            while let Some(cmd) = next {
                match self.handle(cmd) {
                    Handled::Exit => return,
                    Handled::Moved => moved = true,
                    Handled::Other => {}
                }
                next = match rx.try_recv() {
                    Ok(c) => Some(c),
                    Err(TryRecvError::Empty) => None,
                    Err(TryRecvError::Disconnected) => return,
                };
            }
            if moved {
                if let Err(e) = self.show() {
                    self.fail(e);
                }
            }
            self.publish_status();
        }
    }

    /// One frame of playing.
    fn tick(&mut self) {
        if self.index + 1 >= self.rec.len() {
            self.refresh();
        }
        if self.index + 1 >= self.rec.len() {
            self.pause();
            return;
        }
        self.index += 1;
        if let Err(e) = self.show() {
            self.fail(e);
            return;
        }
        self.next_due = Some(Instant::now() + self.period());
        self.publish_status();
    }

    /// How long the frame on show stays up: its distance to the next frame
    /// in updates, at the chosen updates per second.
    fn period(&self) -> Duration {
        let gap = match (
            self.rec.update_of(self.index),
            self.rec.update_of(self.index + 1),
        ) {
            (Some(a), Some(b)) if b > a => f64::from(b - a),
            _ => f64::from(self.rec.meta().every.max(1)),
        };
        match self.speed {
            Speed::UpdatesPerSecond(r) if r.is_finite() && r > 0.0 => {
                Duration::from_secs_f64((gap / r).min(MAX_FRAME_PERIOD.as_secs_f64()))
            }
            _ => MIN_FRAME_INTERVAL,
        }
    }

    fn handle(&mut self, cmd: Command) -> Handled {
        if self.shared.stop.load(Ordering::Acquire) {
            return Handled::Exit;
        }
        match cmd {
            Command::Shutdown => return Handled::Exit,
            Command::Run => {
                self.refresh();
                if self.index + 1 >= self.rec.len() {
                    // At the end: play it again from the start.
                    self.index = 0;
                    self.mode = RunState::Running;
                    self.next_due = None;
                    self.push_event(SessionEvent::Running);
                    return Handled::Moved;
                }
                self.mode = RunState::Running;
                self.next_due = None;
                self.push_event(SessionEvent::Running);
            }
            Command::Pause => self.pause(),
            Command::StepUpdate => {
                self.pause();
                self.refresh();
                if self.index + 1 < self.rec.len() {
                    self.index += 1;
                    return Handled::Moved;
                }
            }
            Command::StepSlice => {
                self.pause();
                if self.index > 0 {
                    self.index -= 1;
                    return Handled::Moved;
                }
            }
            Command::RunToUpdate(n) => {
                self.pause();
                self.refresh();
                if let Some(i) = self.rec.position_at_or_before(n) {
                    self.index = i;
                    return Handled::Moved;
                }
            }
            Command::SeekFrame(i) => {
                self.refresh();
                self.index = i.min(self.rec.len().saturating_sub(1));
                self.next_due = None;
                return Handled::Moved;
            }
            Command::SetSpeed(speed) => {
                self.speed = speed;
                self.next_due = None;
            }
            // A frame has the layers that were recorded; there is no
            // organism to watch.
            Command::Subscribe(_) | Command::WatchCell(_) | Command::FollowOffspring(_) => {}
            Command::SavePopulation
            | Command::RunUntil(_)
            | Command::Restart { .. }
            | Command::Record(_)
            | Command::StopRecording
            | Command::RecordNow => self.push_event(SessionEvent::Error(
                "not available in a playback: there is no simulation behind the frames".to_string(),
            )),
        }
        Handled::Other
    }

    fn pause(&mut self) {
        self.mode = RunState::Paused;
        self.next_due = None;
        self.publish_status();
        self.push_event(SessionEvent::Paused {
            update: self.rec.update_of(self.index).unwrap_or(-1),
        });
    }

    fn fail(&mut self, error: String) {
        self.mode = RunState::Paused;
        self.next_due = None;
        self.push_event(SessionEvent::Error(error));
        self.publish_status();
    }

    /// Picks up records appended since the last look (a recording still
    /// being written).
    fn refresh(&mut self) {
        if let Err(e) = self.rec.refresh() {
            self.push_event(SessionEvent::Error(e.to_string()));
        }
    }

    /// Publishes the record at `index`: its frame, the rows up to it and
    /// the playback info.
    fn show(&mut self) -> Result<(), String> {
        let frame = self.rec.frame(self.index).map_err(|e| e.to_string())?;
        while self.rows.len() <= self.index {
            let row = self.rec.row(self.rows.len()).map_err(|e| e.to_string())?;
            self.rows.push(row);
        }
        {
            let mut queue = lock(&self.shared.rows);
            if self.index + 1 < self.rows_sent {
                queue.clear();
                self.shared.rows_reset.store(true, Ordering::Release);
                self.rows_sent = 0;
            }
            queue.extend(self.rows[self.rows_sent..=self.index].iter().cloned());
            self.rows_sent = self.index + 1;
        }
        let update = frame.update;
        *lock(&self.shared.frame) = Some(Arc::new(frame));
        self.shared.frame_generation.fetch_add(1, Ordering::Release);
        *lock(&self.shared.playback) = Some(PlaybackInfo {
            path: self.rec.path().to_path_buf(),
            frames: self.rec.len(),
            index: self.index,
            update,
            first_update: self.rec.update_of(0).unwrap_or(update),
            last_update: self
                .rec
                .update_of(self.rec.len().saturating_sub(1))
                .unwrap_or(update),
            cut_tail_bytes: match self.rec.tail() {
                Tail::Clean => 0,
                Tail::Cut { bytes } | Tail::Damaged { bytes } => bytes,
            },
            layers: self.rec.meta().layers.clone(),
        });
        self.publish_status();
        self.wake();
        Ok(())
    }

    fn publish_status(&self) {
        *lock(&self.shared.status) = Status {
            state: self.mode,
            update: self.rec.update_of(self.index).unwrap_or(-1),
            slice: None,
            speed: self.speed,
            run_to: None,
            run_until: None,
            follow_offspring: false,
            mid_slice: false,
        };
        self.wake();
    }

    fn wake(&self) {
        if let Some(wake) = &self.wake {
            wake();
        }
    }

    fn push_event(&self, event: SessionEvent) {
        lock(&self.shared.events).push(event);
        self.wake();
    }
}

enum Handled {
    Exit,
    Moved,
    Other,
}
