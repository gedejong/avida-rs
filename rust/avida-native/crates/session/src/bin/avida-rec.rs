//! `avida-rec`: run an Avida experiment without a window and record its
//! arena to a file that `avida-gui --play` and `avida-tui --play` can play
//! back later.
//!
//! It takes what the `avida` binary takes (a configuration folder, `-s`,
//! `-set`, `-def`, ...) and computes exactly what `avida` computes, with
//! frames appended to the recording as updates end. SIGINT and SIGTERM
//! stop it at the end of the update in progress with a complete file; the
//! exit status is the run's (128 plus the signal after one).

use std::io::Write;
use std::path::PathBuf;
use std::process::ExitCode;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::mpsc::{self, Receiver};
use std::sync::Arc;
use std::time::{Duration, Instant};

use avida_session::cli::{take_option, take_record_options, RECORD_USAGE};
use avida_session::{Command, RunState, Session, SessionConfig, SessionEvent, UpdateRow};

const HELP: &str = "\
avida-rec: run an Avida experiment without a window and record its arena

USAGE:
    avida-rec --record FILE [OPTIONS] [CONFIG_DIR] [AVIDA_ARGS...]

CONFIG_DIR is the run's working directory (avida.cfg, events.cfg, the
environment file); it defaults to the current directory. Everything after it
is passed to the avida command line (for example: -s 7 -set WORLD_X 30
-set EXECUTION_MODE 1 -set EXECUTION_THREADS 4).

OPTIONS (before CONFIG_DIR):
";

const HELP_TAIL: &str =
    "    --progress SECS       print a progress line to stderr every SECS seconds
                          (default 10, 0 for none)
    --verbose             also print the per-update `UD:` lines
    --help                show this text

SIGINT or SIGTERM finish the update in progress, write its frame and leave
a complete recording. The exit status is the run's, or 128 plus the signal.
";

/// How long a stop waits for the thread to settle before giving up.
const STOP_WAIT: Duration = Duration::from_secs(600);

fn main() -> ExitCode {
    match run() {
        Ok(code) => ExitCode::from(code),
        Err(e) => {
            eprintln!("avida-rec: {e}");
            ExitCode::from(2)
        }
    }
}

struct Rec {
    session: Session,
    wake: Receiver<()>,
    verbose: bool,
    last_row: Option<UpdateRow>,
    fatal: bool,
}

impl Rec {
    /// Prints what the run printed; returns the events (for the waits).
    fn pump(&mut self) -> Vec<SessionEvent> {
        if let Some(r) = self.session.drain_rows().pop() {
            self.last_row = Some(r);
        }
        let events = self.session.drain_events();
        let mut out = std::io::stdout().lock();
        for e in &events {
            match e {
                SessionEvent::Stdout(l) => {
                    if self.verbose || !l.trim_start().starts_with("UD:") {
                        let _ = writeln!(out, "{l}");
                    }
                }
                SessionEvent::Stderr(l) => eprintln!("{l}"),
                SessionEvent::Fatal(e) => {
                    self.fatal = true;
                    eprintln!("error: {e}");
                }
                SessionEvent::Error(e) => eprintln!("avida-rec: {e}"),
                _ => {}
            }
        }
        events
    }

    /// Waits up to `timeout` for an event `pred` accepts.
    fn wait(&mut self, timeout: Duration, pred: impl Fn(&SessionEvent) -> bool) -> bool {
        let deadline = Instant::now() + timeout;
        loop {
            if self.pump().iter().any(&pred) {
                return true;
            }
            let left = deadline.saturating_duration_since(Instant::now());
            if left.is_zero() {
                return false;
            }
            let _ = self.wake.recv_timeout(left.min(Duration::from_millis(200)));
        }
    }

    fn progress_line(&self) -> String {
        let status = self.session.status();
        let rec = self.session.recording_status();
        let orgs = self.last_row.as_ref().map_or(0, |r| r.orgs);
        let (frames, bytes, spent) = rec.map_or((0, 0, 0.0), |r| {
            (r.frames, r.bytes, r.time_spent.as_secs_f64())
        });
        format!(
            "update {}  organisms {}  frames {}  file {}  recording {:.1} s",
            status.update,
            orgs,
            frames,
            human_bytes(bytes),
            spent
        )
    }
}

fn human_bytes(b: u64) -> String {
    const UNITS: [&str; 4] = ["B", "KiB", "MiB", "GiB"];
    let mut v = b as f64;
    let mut u = 0;
    while v >= 1024.0 && u + 1 < UNITS.len() {
        v /= 1024.0;
        u += 1;
    }
    if u == 0 {
        format!("{b} B")
    } else {
        format!("{v:.1} {}", UNITS[u])
    }
}

fn run() -> Result<u8, String> {
    let mut args: Vec<String> = std::env::args().skip(1).collect();
    if args.iter().any(|a| a == "--help" || a == "-h") {
        print!("{HELP}{RECORD_USAGE}\n{HELP_TAIL}");
        return Ok(0);
    }
    let spec = take_record_options(&mut args)?
        .ok_or_else(|| "--record FILE is required (try --help)".to_string())?;
    let progress: f64 = match take_option(&mut args, "--progress")? {
        None => 10.0,
        Some(v) => v
            .parse()
            .ok()
            .filter(|s: &f64| s.is_finite() && *s >= 0.0)
            .ok_or_else(|| format!("--progress: {v:?} is not a number of seconds"))?,
    };
    let verbose = if let Some(i) = args.iter().position(|a| a == "--verbose") {
        args.remove(i);
        true
    } else {
        false
    };
    let work_dir = match args.first() {
        Some(a) if !a.starts_with('-') => PathBuf::from(args.remove(0)),
        _ => PathBuf::from("."),
    };

    let signal = Arc::new(AtomicUsize::new(0));
    for s in [signal_hook::consts::SIGINT, signal_hook::consts::SIGTERM] {
        signal_hook::flag::register_usize(s, Arc::clone(&signal), s as usize)
            .map_err(|e| format!("cannot install the signal handlers: {e}"))?;
    }

    let (tx, wake) = mpsc::channel();
    let session = Session::spawn_with_wake(
        SessionConfig { work_dir, args },
        Box::new(move || {
            let _ = tx.send(());
        }),
    )?;
    let mut rec = Rec {
        session,
        wake,
        verbose,
        last_row: None,
        fatal: false,
    };
    rec.pump();
    rec.session.send(Command::Record(spec.clone()));
    let started = rec.wait(Duration::from_secs(60), |e| {
        matches!(
            e,
            SessionEvent::RecordingStarted(_) | SessionEvent::Error(_)
        )
    });
    let Some(status) = rec.session.recording_status().filter(|_| started) else {
        return Err(format!("cannot record to {}", spec.path.display()));
    };
    eprintln!(
        "recording to {} every {} updates",
        status.path.display(),
        status.every
    );
    rec.session.send(Command::Run);

    let started_at = Instant::now();
    let mut next_progress = Instant::now() + Duration::from_secs_f64(progress.max(0.001));
    let mut stopped_by = 0usize;
    loop {
        let _ = rec.wake.recv_timeout(Duration::from_millis(200));
        rec.pump();
        let state = rec.session.state();
        if matches!(state, RunState::Finished | RunState::Failed) {
            break;
        }
        let sig = signal.load(Ordering::Relaxed);
        if sig != 0 {
            stopped_by = sig;
            eprintln!("avida-rec: stopping at the end of the update in progress");
            stop_cleanly(&mut rec);
            break;
        }
        if progress > 0.0 && Instant::now() >= next_progress {
            eprintln!("{}", rec.progress_line());
            next_progress = Instant::now() + Duration::from_secs_f64(progress);
        }
    }
    rec.pump();
    rec.session.send(Command::StopRecording);
    rec.wait(Duration::from_secs(30), |e| {
        matches!(e, SessionEvent::RecordingStopped { .. })
    });
    let summary = rec.session.recording_status();
    let code = rec.session.exit_code();
    let mut session = rec.session;
    let stopped = session.shutdown(Duration::from_secs(10));
    if let Some(s) = summary {
        eprintln!(
            "recorded {} frames ({}) to {} in {:.1} s, of which {:.1} s capturing and writing them",
            s.frames,
            human_bytes(s.bytes),
            s.path.display(),
            started_at.elapsed().as_secs_f64(),
            s.time_spent.as_secs_f64()
        );
        if let Some(e) = s.error {
            eprintln!("avida-rec: the recording stopped early: {e}");
        }
    }
    if !stopped {
        eprintln!("avida-rec: the simulation thread did not stop in time");
        std::process::exit(1);
    }
    Ok(match (stopped_by, code) {
        (0, Some(c)) => u8::try_from(c).unwrap_or(1),
        (0, None) => 1,
        (s, _) => u8::try_from(128 + s).unwrap_or(1),
    })
}

/// Pauses the run, finishes the update in progress, records it, so the
/// file ends on a whole update.
fn stop_cleanly(rec: &mut Rec) {
    rec.session.send(Command::Pause);
    let paused = |e: &SessionEvent| {
        matches!(
            e,
            SessionEvent::Paused { .. } | SessionEvent::Finished { .. } | SessionEvent::Fatal(_)
        )
    };
    if !rec.wait(STOP_WAIT, paused) {
        return;
    }
    if rec.session.status().slice.is_some() {
        rec.session.send(Command::StepUpdate);
        if !rec.wait(STOP_WAIT, paused) {
            return;
        }
    }
    if rec.session.state() == RunState::Paused {
        rec.session.send(Command::RecordNow);
        // The command is carried out before the stop that follows it.
    }
}
