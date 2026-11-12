//! The loop that joins the session, the application state and a terminal.
//!
//! [`Driver`] owns the [`Session`] and moves what it published into the
//! [`App`] and the app's queued commands into the session. [`run_loop`]
//! draws, waits for an event from an [`EventSource`] and repeats; tests
//! pass a scripted source and a `TestBackend` terminal, the binary passes
//! [`CrosstermEvents`], which reads the terminal on a thread of its own.
//! The UI thread never waits on the simulation or on terminal input: it
//! polls the session's published state every few milliseconds, and when
//! it stops it waits for the simulation thread only for a bounded time
//! ([`Driver::shutdown`]).

use std::io;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender};
use std::sync::Arc;
use std::time::{Duration, Instant};

use avida_session::{Command, Session, SessionConfig};
use ratatui::backend::Backend;
use ratatui::buffer::Buffer;
use ratatui::crossterm::event::{self, Event};
use ratatui::Terminal;

use crate::app::{App, Request, ToastKind};
use crate::color::ColorMode;
use crate::ui;

/// How long the loop waits for input before looking at the session again.
pub const TICK: Duration = Duration::from_millis(30);

/// What a [`Driver::pump`] changed.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Pumped {
    /// The screen should be redrawn.
    pub changed: bool,
    /// The terminal should be cleared first (Ctrl-L).
    pub redraw: bool,
}

pub struct Driver {
    pub app: App,
    session: Session,
    wake: Arc<AtomicBool>,
    last_generation: u64,
    started: Instant,
    /// The time the app sees, when fixed ([`Driver::freeze_clock`]).
    frozen: Option<f64>,
    /// Set to a signal number by [`Driver::catch_signals`].
    signal: Arc<AtomicUsize>,
    /// Tells the exit watchdog that the exit has got past the terminal
    /// ([`Driver::exit_path_clear`]).
    exit_clear: Option<Sender<()>>,
}

impl Driver {
    /// Builds the world (in the session thread) and the application state.
    /// Fails with the text `avida` would print.
    pub fn start(
        work_dir: std::path::PathBuf,
        args: Vec<String>,
        color_mode: ColorMode,
    ) -> Result<Driver, String> {
        let wake = Arc::new(AtomicBool::new(true));
        let w = Arc::clone(&wake);
        let session = Session::spawn_with_wake(
            SessionConfig {
                work_dir: work_dir.clone(),
                args: args.clone(),
            },
            Box::new(move || w.store(true, Ordering::Release)),
        )?;
        let mut app = App::new(work_dir, args, color_mode);
        app.set_setup(session.setup());
        let mut d = Driver {
            app,
            session,
            wake,
            last_generation: 0,
            started: Instant::now(),
            frozen: None,
            signal: Arc::new(AtomicUsize::new(0)),
            exit_clear: None,
        };
        d.pump();
        Ok(d)
    }

    /// Makes SIGTERM, SIGHUP and SIGINT end the loop like a quit, so the
    /// terminal is given back (a signal's default action would leave it in
    /// raw mode on the alternate screen), and starts the exit watchdog: if
    /// the exit has not got past the terminal ([`Driver::exit_path_clear`])
    /// within [`EXIT_GRACE`] of the first signal, because the loop or the
    /// restoring is stuck (a write to a terminal nobody reads blocks for
    /// ever), the watchdog ends the process with the status the signal
    /// asks for. Returns false if a handler could not be installed.
    pub fn catch_signals(&mut self) -> bool {
        use signal_hook::consts::{SIGHUP, SIGINT, SIGTERM};
        let signals = [SIGTERM, SIGHUP, SIGINT];
        let flags = signals.into_iter().all(|s| {
            signal_hook::flag::register_usize(s, Arc::clone(&self.signal), s as usize).is_ok()
        });
        flags && self.start_exit_watchdog(&signals)
    }

    /// The watchdog: a thread that sleeps on a socket the signal handlers
    /// write to (after setting the flag), so it does not depend on the loop
    /// at all.
    fn start_exit_watchdog(&mut self, signals: &[i32]) -> bool {
        let Ok((wake, poke)) = std::os::unix::net::UnixStream::pair() else {
            return false;
        };
        for &s in signals {
            let registered = poke
                .try_clone()
                .is_ok_and(|p| signal_hook::low_level::pipe::register(s, p).is_ok());
            if !registered {
                return false;
            }
        }
        let (clear_tx, clear_rx) = mpsc::channel();
        let signal = Arc::clone(&self.signal);
        let started = std::thread::Builder::new()
            .name("avida-tui-exit-watchdog".to_string())
            .spawn(move || exit_watchdog(wake, &signal, &clear_rx));
        self.exit_clear = Some(clear_tx);
        started.is_ok()
    }

    /// The exit has got past the terminal (given back, or forgotten when it
    /// is gone); what is left is bounded on its own ([`Driver::shutdown`]).
    /// The exit watchdog stands down.
    pub fn exit_path_clear(&self) {
        if let Some(tx) = &self.exit_clear {
            let _ = tx.send(());
        }
    }

    /// Fixes the time the app sees at `secs` seconds after the start, so
    /// that nothing it shows depends on how fast the machine is: toasts
    /// stay until replaced, and no update rate is shown. For scripted
    /// tests.
    pub fn freeze_clock(&mut self, secs: f64) {
        self.frozen = Some(secs);
    }

    /// The signal that asked the loop to end, if one did.
    pub fn signal(&self) -> Option<i32> {
        match self.signal.load(Ordering::Acquire) {
            0 => None,
            n => i32::try_from(n).ok(),
        }
    }

    /// Moves published state into the app and queued commands into the
    /// session.
    pub fn pump(&mut self) -> Pumped {
        let mut out = Pumped::default();
        if self.signal().is_some() {
            self.app.quit = true;
        }
        let toasts_before = self.app.toasts.len();
        let now = self
            .frozen
            .unwrap_or_else(|| self.started.elapsed().as_secs_f64());
        self.app.set_time(now);
        if self.app.toasts.len() != toasts_before {
            out.changed = true;
        }
        self.wake.store(false, Ordering::Release);

        let events = self.session.drain_events();
        if !events.is_empty() {
            out.changed = true;
            self.app.apply_events(events);
        }
        let rows = self.session.drain_rows();
        if !rows.is_empty() {
            out.changed = true;
            self.app.apply_rows(rows);
        }
        let status = self.session.status();
        if self.app.status.as_ref() != Some(&status) {
            out.changed = true;
            self.app.apply_status(status);
        }
        let generation = self.session.frame_generation();
        if generation != self.last_generation {
            if let Some(frame) = self.session.latest_frame() {
                self.last_generation = generation;
                out.changed = true;
                self.app.apply_frame(frame);
            }
        }

        self.flush_commands(&mut out);
        let requests = self.app.take_requests();
        let restarted = requests.iter().any(|r| matches!(r, Request::Restart(_)));
        for r in requests {
            out.changed = true;
            match r {
                Request::Redraw => out.redraw = true,
                Request::Restart(args) => self.restart(args),
                Request::WriteFile { path, contents } => {
                    let result = path
                        .parent()
                        .map_or(Ok(()), std::fs::create_dir_all)
                        .and_then(|()| std::fs::write(&path, contents));
                    match result {
                        Ok(()) => self
                            .app
                            .notify(ToastKind::Success, format!("Wrote {}", path.display())),
                        Err(e) => self.app.notify(
                            ToastKind::Error,
                            format!("Cannot write {}: {e}", path.display()),
                        ),
                    }
                }
            }
        }
        if restarted {
            // The commands the new run's first frame needs.
            self.flush_commands(&mut out);
        }
        out
    }

    fn flush_commands(&mut self, out: &mut Pumped) {
        for c in self.app.take_commands() {
            if !self.session.send(c) {
                self.app
                    .notify(ToastKind::Error, "The simulation thread is gone.".into());
                out.changed = true;
            }
        }
    }

    /// Asks the session to build the world again from the same folder with
    /// `args` (`Command::Restart`, on the simulation thread, which finishes
    /// the old run's files first). The app waits for `Restarted` or
    /// `RestartFailed`; if the new world cannot be built the old run
    /// carries on.
    fn restart(&mut self, args: Vec<String>) {
        if self.session.send(Command::Restart { args: args.clone() }) {
            self.app.begin_restart(args);
        } else {
            self.app
                .notify(ToastKind::Error, "The simulation thread is gone.".into());
        }
    }

    pub fn session(&self) -> &Session {
        &self.session
    }

    /// Stops the simulation thread, waiting at most `timeout` for it
    /// ([`Session::shutdown`]). False when it did not stop in time: it is
    /// left running, and the process should exit.
    pub fn shutdown(&mut self, timeout: Duration) -> bool {
        self.session.shutdown(timeout)
    }

    /// True once the session has asked to be woken since the last pump.
    pub fn woken(&self) -> bool {
        self.wake.load(Ordering::Acquire)
    }
}

/// Where input comes from.
pub trait EventSource {
    /// The next event, or `None` when `timeout` passes without one.
    fn poll(&mut self, app: &App, timeout: Duration) -> io::Result<Option<Event>>;

    /// Called with the buffer that was just drawn.
    fn after_draw(&mut self, _app: &App, _buffer: &Buffer) {}
}

/// Events from the real terminal, read by crossterm on a thread of their
/// own.
///
/// crossterm cannot be read from the thread that has to notice the
/// terminal going away. Once its poll has reported the terminal readable,
/// its read loop (`UnixInternalEventSource::try_read` in crossterm 0.29)
/// returns only with an event, on `WouldBlock` or on `Interrupted`; a
/// hung-up terminal answers every read with end of file (or `EIO`), so the
/// loop spins at full speed for ever, deaf to signals. A check before each
/// call cannot rule that out, because the terminal can hang up between
/// the check and crossterm's poll. So the input thread does the reading,
/// and [`EventSource::poll`] waits on a channel and asks the terminal
/// itself ([`terminal_hung_up`]) on every call: if the input thread is
/// caught in that loop, the interface still ends within a tick and the
/// process exits around it.
pub struct CrosstermEvents {
    /// `None` under the test hook [`unguarded_input`].
    events: Option<Receiver<io::Result<Event>>>,
}

impl CrosstermEvents {
    /// Starts the input thread. It sends every event, then the error that
    /// ended it (the terminal going away included), and stops when the
    /// receiver is dropped; the channel is unbounded, so it never blocks
    /// on a send.
    pub fn spawn() -> io::Result<CrosstermEvents> {
        if unguarded_input() {
            return Ok(CrosstermEvents { events: None });
        }
        let (tx, rx) = mpsc::channel();
        std::thread::Builder::new()
            .name("avida-tui-input".to_string())
            .spawn(move || read_input(&tx))?;
        Ok(CrosstermEvents { events: Some(rx) })
    }
}

/// How long after a signal the exit may take to get past the terminal (the
/// loop ending and the terminal given back, which take milliseconds when
/// nothing is stuck) before the exit watchdog ends the process.
pub const EXIT_GRACE: Duration = Duration::from_secs(1);

/// The exit watchdog's thread: waits for the first signal, then for the
/// exit to get past the terminal; if that takes longer than
/// [`EXIT_GRACE`], gives the terminal back as far as it can without
/// blocking and exits with `128 + signal`.
fn exit_watchdog(
    mut wake: std::os::unix::net::UnixStream,
    signal: &AtomicUsize,
    clear: &Receiver<()>,
) {
    use std::io::Read;
    let mut byte = [0u8; 1];
    loop {
        match wake.read(&mut byte) {
            Ok(0) => return,
            Ok(_) => break,
            Err(e) if e.kind() == io::ErrorKind::Interrupted => {}
            Err(_) => return,
        }
    }
    if let Err(RecvTimeoutError::Timeout) = clear.recv_timeout(EXIT_GRACE) {
        let code = match signal.load(Ordering::Acquire) {
            0 => 1,
            s => 128 + i32::try_from(s).unwrap_or(0),
        };
        // The terminal has not taken the output for a second: what is
        // queued for it goes, so that the sequences fit.
        let _ = rustix::termios::tcflush(io::stdout(), rustix::termios::QueueSelector::OFlush);
        give_back_without_blocking();
        settle_output(OUTPUT_SETTLE);
        // A bare `_exit`. `std::process::exit` first flushes the standard
        // output buffer when no thread holds it, and the interface thread
        // leaves it unheld, with part of a frame in it, between two
        // writes: written to a terminal nobody reads, that flush would
        // block the exit itself. (Pointing standard output elsewhere
        // first does not help: replacing a descriptor waits for a write
        // in progress on it.)
        signal_hook::low_level::exit(code);
    }
}

/// How long the exit lets a terminal take what was written to it.
pub const OUTPUT_SETTLE: Duration = Duration::from_millis(300);

/// Lets the terminal take what was written to it, for at most `wait`, then
/// makes it discard what it holds and everything written to it from then
/// on. Only for the session leader, just before it exits: the exit of the
/// process that leads its terminal's session (as under `ssh -t`, or a
/// terminal emulator running the program directly) waits until the
/// terminal's output has drained, with no time limit, even after
/// `SIGKILL`, and then takes the terminal away from the session. Any other
/// process exits without waiting, and what it wrote is shown once the
/// terminal reads again, so this does nothing then; nor when the terminal
/// has hung up (the kernel has discarded the output already).
///
/// A flush alone is not enough. It wakes a thread blocked writing a frame,
/// which fills the queue again before the exit stops it, so the terminal
/// is first told to discard output (`FLUSHO`). That lasts until the mode is
/// set again, and [`leave_terminal`] sets it after its last write.
pub fn settle_output(wait: Duration) {
    if !leads_terminal_session() || terminal_hung_up() {
        return;
    }
    let (tx, rx) = mpsc::channel();
    let drained = std::thread::Builder::new()
        .name("avida-tui-drain".to_string())
        .spawn(move || {
            let _ = rustix::termios::tcdrain(io::stdout());
            let _ = tx.send(());
        })
        .is_ok_and(|_| rx.recv_timeout(wait).is_ok());
    if !drained {
        let out = io::stdout();
        if let Ok(mut mode) = rustix::termios::tcgetattr(&out) {
            mode.local_modes |= rustix::termios::LocalModes::FLUSHO;
            let _ = rustix::termios::tcsetattr(&out, rustix::termios::OptionalActions::Now, &mode);
        }
        // Also wakes the thread waiting in `tcdrain`.
        let _ = rustix::termios::tcflush(&out, rustix::termios::QueueSelector::OFlush);
    }
}

/// Whether this process leads the session whose controlling terminal is
/// standard output.
fn leads_terminal_session() -> bool {
    rustix::termios::tcgetsid(io::stdout()).is_ok_and(|sid| {
        u32::try_from(sid.as_raw_nonzero().get()).is_ok_and(|sid| sid == std::process::id())
    })
}

/// [`leave_terminal`] for a thread that must not block (the interface
/// thread may be stuck in a write, holding standard output): the mouse,
/// screen and cursor sequences go through a descriptor of its own, opened
/// non-blocking, so they are dropped when the terminal does not take them
/// at once, and then raw mode is switched off (`tcsetattr` does not wait).
/// Nothing at all when the terminal has hung up.
fn give_back_without_blocking() {
    if !TERMINAL_TAKEN.swap(false, Ordering::AcqRel) || terminal_hung_up() {
        return;
    }
    let mut sequences: Vec<u8> = Vec::new();
    let _ = ratatui::crossterm::queue!(
        sequences,
        event::DisableMouseCapture,
        ratatui::crossterm::terminal::LeaveAlternateScreen,
        ratatui::crossterm::cursor::Show
    );
    write_terminal_nonblocking(&sequences);
    let _ = ratatui::crossterm::terminal::disable_raw_mode();
}

/// Writes `bytes` to the controlling terminal through a descriptor of its
/// own, opened non-blocking (the flag belongs to that descriptor, not to
/// the standard streams the shell shares): what the terminal does not take
/// at once is dropped. For the exit path, which must not block.
pub fn write_terminal_nonblocking(bytes: &[u8]) {
    use rustix::fs::{open, Mode, OFlags};
    let flags = OFlags::WRONLY | OFlags::NONBLOCK | OFlags::NOCTTY | OFlags::CLOEXEC;
    if let Ok(tty) = open("/dev/tty", flags, Mode::empty()) {
        let _ = rustix::io::write(&tty, bytes);
    }
}

/// Test hook (`AVIDA_TUI_TEST_UNGUARDED_INPUT=1`): read crossterm on the
/// interface thread with no hang-up check, the way that gets stuck when the
/// terminal hangs up, so the pty tests can check that a signal still ends
/// the process.
fn unguarded_input() -> bool {
    std::env::var_os("AVIDA_TUI_TEST_UNGUARDED_INPUT").is_some()
}

/// The error for a terminal that has gone away.
pub fn terminal_gone() -> io::Error {
    io::Error::new(io::ErrorKind::BrokenPipe, "the terminal went away")
}

/// Whether `e` says the terminal has gone away.
pub fn is_terminal_gone(e: &io::Error) -> bool {
    e.kind() == io::ErrorKind::BrokenPipe
}

/// What waiting on standard input found.
enum Wait {
    Timeout,
    Readable,
    Gone,
}

fn timespec(d: Duration) -> rustix::event::Timespec {
    rustix::event::Timespec {
        tv_sec: i64::try_from(d.as_secs()).unwrap_or(i64::MAX),
        tv_nsec: i64::from(d.subsec_nanos()),
    }
}

/// Whether standard input has hung up: the other end of the terminal (the
/// emulator, the ssh session, the pty master) is gone. A hang-up is a
/// lasting state of the terminal, which no other reader can take away, so
/// this is safe to ask from any thread at any time. It does not block.
pub fn terminal_hung_up() -> bool {
    use rustix::event::{poll, PollFd, PollFlags};
    let stdin = io::stdin();
    let mut fds = [PollFd::new(&stdin, PollFlags::IN)];
    match poll(&mut fds, Some(&timespec(Duration::ZERO))) {
        Ok(0) | Err(rustix::io::Errno::INTR) => false,
        Ok(_) => fds[0]
            .revents()
            .intersects(PollFlags::HUP | PollFlags::ERR | PollFlags::NVAL),
        Err(_) => true,
    }
}

/// Waits up to `timeout` for standard input to become readable, noticing
/// a hang-up and end of file. Only for the input thread: "readable with
/// nothing to read" means end of file only when no other thread can have
/// taken the input in between.
fn wait_for_input(timeout: Duration) -> Wait {
    use rustix::event::{poll, PollFd, PollFlags};
    let stdin = io::stdin();
    let mut fds = [PollFd::new(&stdin, PollFlags::IN)];
    match poll(&mut fds, Some(&timespec(timeout))) {
        Ok(0) | Err(rustix::io::Errno::INTR) => Wait::Timeout,
        Ok(_) => {
            let r = fds[0].revents();
            if r.intersects(PollFlags::HUP | PollFlags::ERR | PollFlags::NVAL) {
                return Wait::Gone;
            }
            if r.contains(PollFlags::IN) && rustix::io::ioctl_fionread(&stdin) == Ok(0) {
                Wait::Gone
            } else {
                Wait::Readable
            }
        }
        Err(_) => Wait::Gone,
    }
}

/// The input thread: hands crossterm only input that is there (and, every
/// tick, the chance to report a resize, which it learns from a signal).
fn read_input(tx: &Sender<io::Result<Event>>) {
    loop {
        if matches!(wait_for_input(TICK), Wait::Gone) {
            let _ = tx.send(Err(terminal_gone()));
            return;
        }
        loop {
            let next = match event::poll(Duration::ZERO) {
                Ok(false) => break,
                Ok(true) => event::read(),
                Err(e) => Err(e),
            };
            let failed = next.is_err();
            if tx.send(next).is_err() || failed {
                return;
            }
        }
    }
}

impl EventSource for CrosstermEvents {
    fn poll(&mut self, _app: &App, timeout: Duration) -> io::Result<Option<Event>> {
        let Some(events) = &self.events else {
            return if event::poll(timeout)? {
                event::read().map(Some)
            } else {
                Ok(None)
            };
        };
        if terminal_hung_up() {
            return Err(terminal_gone());
        }
        match events.recv_timeout(timeout) {
            Ok(event) => event.map(Some),
            Err(RecvTimeoutError::Timeout) if terminal_hung_up() => Err(terminal_gone()),
            Err(RecvTimeoutError::Timeout) => Ok(None),
            // The thread sends its error before it stops.
            Err(RecvTimeoutError::Disconnected) => Err(terminal_gone()),
        }
    }
}

fn handle(app: &mut App, ev: Event) {
    match ev {
        Event::Key(k) => app.on_key(k),
        Event::Mouse(m) => app.on_mouse(m),
        _ => {}
    }
}

/// Draws, waits for input and pumps the session until the app quits. Draws
/// nothing once the app has quit (a signal may mean the terminal is gone).
pub fn run_loop<B: Backend>(
    terminal: &mut Terminal<B>,
    driver: &mut Driver,
    source: &mut dyn EventSource,
) -> io::Result<()> {
    let mut dirty = true;
    loop {
        let p = driver.pump();
        if driver.app.quit {
            return Ok(());
        }
        dirty |= p.changed;
        if p.redraw {
            // `Terminal::clear` asks the terminal where the cursor is and
            // fails when no answer comes in time (a slow link, or a
            // terminal that does not reply); resizing to the same area
            // clears the screen and forgets the old one without asking.
            let size = terminal
                .size()
                .map_err(|e| io::Error::other(e.to_string()))?;
            terminal
                .resize(ratatui::layout::Rect::from(size))
                .map_err(|e| io::Error::other(e.to_string()))?;
            dirty = true;
        }
        if dirty {
            let done = terminal
                .draw(|f| ui::draw(f, &mut driver.app))
                .map_err(|e| io::Error::other(e.to_string()))?;
            source.after_draw(&driver.app, done.buffer);
            dirty = false;
            assert!(!driver.app.test_panic, "test panic requested");
        }
        if let Some(ev) = source.poll(&driver.app, TICK)? {
            // Resizes and other events redraw; keys and clicks change state.
            dirty = true;
            handle(&mut driver.app, ev);
            // Take whatever else is already waiting before drawing again.
            for _ in 0..64 {
                match source.poll(&driver.app, Duration::ZERO)? {
                    Some(ev) => handle(&mut driver.app, ev),
                    None => break,
                }
            }
        }
    }
}

/// Why the interface cannot run here, if it cannot: it needs a terminal on
/// both input and output and one that can place the cursor.
pub fn terminal_problem(stdin_tty: bool, stdout_tty: bool, term: Option<&str>) -> Option<String> {
    if !stdin_tty || !stdout_tty {
        let which = match (stdin_tty, stdout_tty) {
            (false, false) => "Neither standard input nor standard output is",
            (false, true) => "Standard input is not",
            _ => "Standard output is not",
        };
        return Some(format!(
            "avida-tui: {which} a terminal. It is an interactive viewer; \
             run it from a terminal window, or use the avida binary for batch runs."
        ));
    }
    if term.is_some_and(|t| t.eq_ignore_ascii_case("dumb")) {
        return Some(
            "avida-tui: TERM is \"dumb\", which cannot place the cursor or clear the screen. \
             Use a terminal such as xterm-256color, or the avida binary for batch runs."
                .to_string(),
        );
    }
    None
}

/// Puts the terminal into the viewer's mode: raw, alternate screen, mouse.
pub fn enter_terminal() -> io::Result<ratatui::DefaultTerminal> {
    let terminal = ratatui::try_init()?;
    TERMINAL_TAKEN.store(true, Ordering::Release);
    ratatui::crossterm::execute!(io::stdout(), event::EnableMouseCapture)?;
    install_panic_hook();
    Ok(terminal)
}

/// Whether the terminal is in the viewer's mode and still to be given back.
static TERMINAL_TAKEN: AtomicBool = AtomicBool::new(false);

/// The terminal has gone away: there is nothing to give back, so the
/// later [`leave_terminal`] (and the [`TerminalGuard`]) must not write to
/// it or change its mode. On a hung-up terminal those calls fail at once,
/// but a terminal that is not read any more can also block a write for
/// ever.
pub fn forget_terminal() {
    TERMINAL_TAKEN.store(false, Ordering::Release);
}

/// Gives the terminal back. Safe to call twice: only the first call after
/// [`enter_terminal`] writes anything (a second `?1049l` would restore a
/// stale cursor position over text printed since, such as a panic message).
pub fn leave_terminal() {
    if !TERMINAL_TAKEN.swap(false, Ordering::AcqRel) {
        return;
    }
    // Dropping the `Terminal` shows the cursor too, but `main` skips that
    // drop when the terminal may be gone.
    let _ = ratatui::crossterm::execute!(
        io::stdout(),
        event::DisableMouseCapture,
        ratatui::crossterm::terminal::LeaveAlternateScreen,
        ratatui::crossterm::cursor::Show
    );
    // The mode goes back after the last write: restoring it also clears
    // the discarding that `settle_output` may have switched on from the
    // exit watchdog, and nothing written after that may reach the queue.
    let _ = ratatui::crossterm::terminal::disable_raw_mode();
}

/// Whether a panic on this thread is the interface's own, so the terminal
/// has to be given back before the message is printed. A panic on another
/// thread (the simulation thread's are caught by the session and reported
/// as an error in the log) must leave the screen alone: printing there
/// would corrupt the display, and restoring the terminal would leave the
/// running interface drawing into a cooked terminal.
pub fn panic_is_ours(current: std::thread::ThreadId, interface: std::thread::ThreadId) -> bool {
    current == interface
}

fn install_panic_hook() {
    let interface = std::thread::current().id();
    let previous = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        if panic_is_ours(std::thread::current().id(), interface) {
            leave_terminal();
            previous(info);
        }
    }));
}

/// Restores the terminal when dropped, so an early return or an unwinding
/// panic cannot leave it raw.
pub struct TerminalGuard;

impl Drop for TerminalGuard {
    fn drop(&mut self) {
        leave_terminal();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_missing_or_dumb_terminal_is_explained() {
        assert!(terminal_problem(true, true, Some("xterm-256color")).is_none());
        assert!(terminal_problem(true, true, None).is_none());
        let both = terminal_problem(false, false, Some("xterm")).unwrap();
        assert!(both.contains("Neither standard input nor standard output"));
        assert!(terminal_problem(true, false, None)
            .unwrap()
            .contains("Standard output is not a terminal"));
        assert!(terminal_problem(false, true, None)
            .unwrap()
            .contains("Standard input is not a terminal"));
        let dumb = terminal_problem(true, true, Some("dumb")).unwrap();
        assert!(dumb.contains("dumb"), "{dumb}");
        assert!(terminal_problem(true, true, Some("DUMB")).is_some());
    }

    #[test]
    fn only_the_interface_thread_restores_the_terminal_on_a_panic() {
        let main = std::thread::current().id();
        assert!(panic_is_ours(main, main));
        let other = std::thread::spawn(|| std::thread::current().id())
            .join()
            .unwrap();
        assert!(!panic_is_ours(other, main));
    }

    #[test]
    fn leaving_a_terminal_never_entered_writes_nothing() {
        // Must not touch the test runner's stdout; the flag is clear here.
        assert!(!TERMINAL_TAKEN.load(Ordering::Acquire));
        leave_terminal();
        assert!(!TERMINAL_TAKEN.load(Ordering::Acquire));
    }
}
