//! Getting the window onto the screen: a stderr logger, a check of the
//! login session, a watchdog for a window that never appears, and the
//! first frames' requests that bring the window forward.
//!
//! On macOS only a program in the logged-in desktop ("Aqua") session may
//! take the focus. A shell inside tmux (whose server leaves that session
//! when it daemonises) or an ssh login is in the "Background" session
//! instead (`launchctl managername` says which): a window opened from
//! there is created, but behind the frontmost app's windows, without a
//! Dock icon, and macOS ignores its requests for the focus. `avida-gui`
//! warns about it on stderr ([`session_problem`], [`HINT`]) and floats
//! the window above the others until it gets the focus, so it is at
//! least seen ([`WindowStartup`]). The app bundle that
//! `crates/gui/scripts/make-app-bundle.sh` builds is started by
//! LaunchServices in the desktop session, so
//! `open Avida.app --args /abs/path/to/config` works from any shell.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

use egui::{ViewportCommand, ViewportInfo};

/// How to get a focused window from a shell outside the desktop session.
pub const HINT: &str = "\
Ways round it:
  * start avida-gui from a Terminal or iTerm window that is not inside tmux;
  * wrap it in an app bundle, which macOS starts in the desktop session
    from any shell, tmux included:
        crates/gui/scripts/make-app-bundle.sh /tmp/avida-app
        open /tmp/avida-app/Avida.app --args /absolute/path/to/config";

/// Environment variable that skips [`display_problem`] (and its warning).
pub const SKIP_SESSION_CHECK: &str = "AVIDA_GUI_SKIP_SESSION_CHECK";

/// How long the watchdog waits for the first frame.
pub const FIRST_FRAME_TIMEOUT: Duration = Duration::from_secs(10);

// --- logging ----------------------------------------------------------------

/// Prints `log` records (eframe's, wgpu's, winit's and ours) to stderr.
struct StderrLogger {
    level: log::LevelFilter,
}

impl log::Log for StderrLogger {
    fn enabled(&self, metadata: &log::Metadata<'_>) -> bool {
        metadata.level() <= self.level
    }

    fn log(&self, record: &log::Record<'_>) {
        if self.enabled(record.metadata()) {
            eprintln!(
                "avida-gui: {} {}: {}",
                record.level().as_str().to_ascii_lowercase(),
                record.target(),
                record.args()
            );
        }
    }

    fn flush(&self) {}
}

/// The level `RUST_LOG` names (`error`, `warn`, `info`, `debug`, `trace`,
/// `off`); warnings and errors otherwise.
pub fn log_level(rust_log: Option<&str>) -> log::LevelFilter {
    rust_log
        .and_then(|s| s.trim().parse::<log::LevelFilter>().ok())
        .unwrap_or(log::LevelFilter::Warn)
}

/// Sends `log` output to stderr, at the level [`log_level`] reads from
/// `RUST_LOG`. Does nothing when a logger is already installed.
pub fn install_logger() {
    let level = log_level(std::env::var("RUST_LOG").ok().as_deref());
    let logger: &'static StderrLogger = Box::leak(Box::new(StderrLogger { level }));
    if log::set_logger(logger).is_ok() {
        log::set_max_level(level);
    }
}

// --- can this process show a window at all? ---------------------------------

/// Why a window opened from this process may not reach the user, when
/// that can be told before trying: on macOS a login session other than
/// the desktop one, elsewhere no display variable at all.
pub fn display_problem() -> Option<String> {
    if std::env::var_os(SKIP_SESSION_CHECK).is_some_and(|v| !v.is_empty() && v != "0") {
        return None;
    }
    platform_display_problem()
}

#[cfg(target_os = "macos")]
fn platform_display_problem() -> Option<String> {
    let out = std::process::Command::new("/bin/launchctl")
        .arg("managername")
        .output()
        .ok()?;
    if !out.status.success() {
        return None;
    }
    session_problem(String::from_utf8_lossy(&out.stdout).trim())
}

#[cfg(all(unix, not(target_os = "macos")))]
fn platform_display_problem() -> Option<String> {
    let set = |k: &str| std::env::var_os(k).is_some_and(|v| !v.is_empty());
    if set("DISPLAY") || set("WAYLAND_DISPLAY") {
        None
    } else {
        Some(
            "neither DISPLAY nor WAYLAND_DISPLAY is set, so there is no display to open \
             a window on (an ssh session without -X, or a console)."
                .to_string(),
        )
    }
}

#[cfg(not(unix))]
fn platform_display_problem() -> Option<String> {
    None
}

/// The problem with a macOS session named `manager` (what `launchctl
/// managername` prints), if it is not the desktop session ("Aqua").
pub fn session_problem(manager: &str) -> Option<String> {
    if manager.is_empty() || manager == "Aqua" {
        return None;
    }
    Some(format!(
        "this shell runs in the macOS \"{manager}\" session (launchctl managername), \
         not the desktop one, as shells inside tmux and ssh logins do. macOS does not let a program started from here take the focus: its \
         window opens behind the others, without a Dock icon, and may not get the \
         keyboard. avida-gui floats it above the others until it gets the focus."
    ))
}

/// The full message for a problem [`display_problem`] found (the ways
/// round it are macOS's).
pub fn display_problem_message(problem: &str) -> String {
    let hint = if cfg!(target_os = "macos") {
        format!("\n\n{HINT}")
    } else {
        String::new()
    };
    format!("avida-gui: {problem}{hint}\n\n({SKIP_SESSION_CHECK}=1 skips this check.)")
}

/// What the watchdog prints when no frame came within `after`.
pub fn no_window_message(after: Duration) -> String {
    format!(
        "avida-gui: no window has been drawn after {} s. The display or the graphics \
         driver may be refusing it (RUST_LOG=info shows what eframe and wgpu tried); \
         stop with Ctrl-C.",
        after.as_secs()
    )
}

/// Starts a thread that prints [`no_window_message`] unless the returned
/// flag is set (by [`WindowStartup`], on the first frame) within `after`.
pub fn watch_for_first_frame(after: Duration) -> Arc<AtomicBool> {
    let shown = Arc::new(AtomicBool::new(false));
    let flag = shown.clone();
    let spawned = std::thread::Builder::new()
        .name("avida-gui-window-watchdog".into())
        .spawn(move || {
            std::thread::sleep(after);
            if !flag.load(Ordering::Acquire) {
                eprintln!("{}", no_window_message(after));
            }
        });
    if let Err(e) = spawned {
        log::warn!("no window watchdog: {e}");
    }
    shown
}

// --- the first frames -------------------------------------------------------

/// Frames (after the first) during which an unfocused window asks for the
/// focus again. eframe shows the window only after painting the first
/// frame, and macOS ignores a focus request for a window it hides.
const FOCUS_FRAMES: u32 = 3;
/// A window still unfocused then floats above the others until it gets
/// the focus (a click on it), for this long at most. macOS refuses the
/// focus to a program outside the desktop session, and its window would
/// otherwise stay behind the terminal: back at the normal level it drops
/// behind the active app's windows again, so a short float is no use.
const FLOAT_SECONDS: f64 = 30.0;

/// The window requests of the first frames: bring a bare binary started
/// from a terminal to the front, undo a restored minimised state, and
/// shrink a restored size that no longer fits the monitor.
#[derive(Default)]
pub struct WindowStartup {
    frame: u32,
    /// When the window was made floating.
    raised_at: Option<f64>,
    done: bool,
    shown: Option<Arc<AtomicBool>>,
}

impl WindowStartup {
    /// `shown` is set on the first frame (see [`watch_for_first_frame`]).
    pub fn new(shown: Option<Arc<AtomicBool>>) -> WindowStartup {
        WindowStartup {
            shown,
            ..WindowStartup::default()
        }
    }

    /// Call once per frame; sends what [`WindowStartup::step`] decides.
    pub fn on_frame(&mut self, ctx: &egui::Context) {
        if self.done {
            return;
        }
        if self.frame == 0 {
            if let Some(flag) = &self.shown {
                flag.store(true, Ordering::Release);
            }
        }
        let (info, now) = ctx.input(|i| (i.viewport().clone(), i.time));
        if self.frame == 1 {
            log::info!(
                "window up: focused {:?}, minimized {:?}, inner rect {:?}, monitor {:?}",
                info.focused,
                info.minimized,
                info.inner_rect,
                info.monitor_size
            );
        }
        for command in self.step(&info, now) {
            ctx.send_viewport_cmd(command);
        }
        // The steps need frames even when nothing else asks for one.
        if !self.done {
            let wait = if self.raised_at.is_some() { 250 } else { 50 };
            ctx.request_repaint_after(std::time::Duration::from_millis(wait));
        }
    }

    /// The requests for the next startup frame, given what eframe reports
    /// about the window and the time (seconds, `egui::InputState::time`).
    pub fn step(&mut self, info: &ViewportInfo, now: f64) -> Vec<ViewportCommand> {
        if self.done {
            return Vec::new();
        }
        let frame = self.frame;
        self.frame += 1;
        let unfocused = info.focused != Some(true);
        let mut out = Vec::new();
        if frame == 0 && info.minimized == Some(true) {
            out.push(ViewportCommand::Minimized(false));
        }
        if frame == 1 {
            if let (Some(inner), Some(monitor)) = (info.inner_rect, info.monitor_size) {
                let size = inner.size();
                // Restored from a larger monitor: fit 90 % of this one.
                if monitor.x > 1.0 && monitor.y > 1.0 && (size.x > monitor.x || size.y > monitor.y)
                {
                    out.push(ViewportCommand::InnerSize(size.min(monitor * 0.9)));
                }
            }
        }
        if frame == 0 {
            return out;
        }
        if frame <= FOCUS_FRAMES {
            if unfocused {
                out.push(ViewportCommand::Focus);
            } else {
                self.done = true;
            }
            return out;
        }
        match self.raised_at {
            None if unfocused => {
                log::info!("the window did not get the focus; floating it until it does");
                self.raised_at = Some(now);
                out.push(ViewportCommand::WindowLevel(egui::WindowLevel::AlwaysOnTop));
            }
            None => self.done = true,
            Some(t) if !unfocused || now - t >= FLOAT_SECONDS => {
                out.push(ViewportCommand::WindowLevel(egui::WindowLevel::Normal));
                self.done = true;
            }
            Some(_) => {}
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use egui::{pos2, vec2, Rect};

    #[test]
    fn rust_log_levels_are_read_and_default_to_warnings() {
        assert_eq!(log_level(None), log::LevelFilter::Warn);
        assert_eq!(log_level(Some("info")), log::LevelFilter::Info);
        assert_eq!(log_level(Some(" DEBUG ")), log::LevelFilter::Debug);
        assert_eq!(log_level(Some("off")), log::LevelFilter::Off);
        assert_eq!(log_level(Some("wgpu=info")), log::LevelFilter::Warn);
    }

    #[test]
    fn only_the_desktop_session_can_show_windows() {
        assert_eq!(session_problem("Aqua"), None);
        assert_eq!(session_problem(""), None);
        let p = session_problem("Background").unwrap();
        assert!(p.contains("\"Background\" session"), "{p}");
        assert!(p.contains("tmux") && p.contains("take the focus"), "{p}");
        let m = display_problem_message(&p);
        if cfg!(target_os = "macos") {
            assert!(m.contains("open /tmp/avida-app/Avida.app --args"), "{m}");
        }
        assert!(m.contains(SKIP_SESSION_CHECK), "{m}");
        assert!(no_window_message(Duration::from_secs(10)).contains("after 10 s"));
    }

    /// Runs a fresh startup through `frames` frames 1/8 s apart, with the
    /// same info.
    fn run(frames: u32, info: &ViewportInfo) -> Vec<Vec<ViewportCommand>> {
        let mut s = WindowStartup::default();
        (0..frames)
            .map(|f| s.step(info, f64::from(f) * 0.125))
            .collect()
    }

    #[test]
    fn the_first_frames_bring_the_window_forward_once_it_is_shown() {
        let focused = ViewportInfo {
            focused: Some(true),
            ..Default::default()
        };
        assert!(run(20, &focused).iter().all(Vec::is_empty));
        // Frame 0 is painted before eframe shows the window; then three
        // focus requests; still unfocused, the window floats until it gets
        // the focus, or for 30 s.
        let unfocused = ViewportInfo {
            focused: Some(false),
            ..Default::default()
        };
        let float = vec![ViewportCommand::WindowLevel(egui::WindowLevel::AlwaysOnTop)];
        let normal = vec![ViewportCommand::WindowLevel(egui::WindowLevel::Normal)];
        let focus = vec![ViewportCommand::Focus];
        let mut s = WindowStartup::default();
        let steps: Vec<_> = (0..6).map(|f| s.step(&unfocused, f64::from(f))).collect();
        assert_eq!(
            steps,
            vec![
                vec![],
                focus.clone(),
                focus.clone(),
                focus,
                float.clone(),
                vec![]
            ]
        );
        assert_eq!(s.step(&focused, 6.0), normal, "clicked: back to normal");
        assert!(s.step(&unfocused, 7.0).is_empty(), "and done");
        let mut s = WindowStartup::default();
        for f in 0..5 {
            s.step(&unfocused, f64::from(f));
        }
        assert!(s.step(&unfocused, 33.0).is_empty());
        assert_eq!(s.step(&unfocused, 34.0), normal, "30 s at most");
        let minimised = ViewportInfo {
            minimized: Some(true),
            focused: Some(true),
            ..Default::default()
        };
        assert_eq!(
            run(1, &minimised),
            vec![vec![ViewportCommand::Minimized(false)]]
        );
    }

    #[test]
    fn a_window_larger_than_its_monitor_is_shrunk() {
        let info = ViewportInfo {
            focused: Some(true),
            inner_rect: Some(Rect::from_min_size(pos2(0.0, 0.0), vec2(2400.0, 1400.0))),
            monitor_size: Some(vec2(1440.0, 900.0)),
            ..Default::default()
        };
        assert_eq!(
            run(2, &info)[1],
            vec![ViewportCommand::InnerSize(vec2(1296.0, 810.0))]
        );
        let fits = ViewportInfo {
            inner_rect: Some(Rect::from_min_size(pos2(0.0, 0.0), vec2(1200.0, 800.0))),
            ..info
        };
        assert!(run(10, &fits).iter().all(Vec::is_empty));
    }
}
