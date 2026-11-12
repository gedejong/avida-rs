//! A scripted event source that drives the real `run_loop` against a
//! `TestBackend`, plus test-directory helpers.

#![allow(dead_code)]

use std::collections::VecDeque;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use avida_tui::app::App;
use avida_tui::color::ColorMode;
use avida_tui::runner::{run_loop, Driver, EventSource};
use ratatui::backend::TestBackend;
use ratatui::buffer::Buffer;
use ratatui::crossterm::event::{
    Event, KeyCode, KeyEvent, KeyEventKind, KeyEventState, KeyModifiers, MouseButton, MouseEvent,
    MouseEventKind,
};
use ratatui::Terminal;

pub const TEST: &str = "heads_default_100u";
pub const TIMEOUT: Duration = Duration::from_secs(240);

/// `TIMEOUT`, or `AVIDA_TUI_TEST_TIMEOUT` seconds (for debugging a hang).
pub fn test_timeout() -> Duration {
    std::env::var("AVIDA_TUI_TEST_TIMEOUT")
        .ok()
        .and_then(|v| v.parse().ok())
        .map_or(TIMEOUT, Duration::from_secs)
}

pub fn tests_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../../avida-core/tests")
}

pub fn copy_dir(from: &Path, to: &Path) {
    fs::create_dir_all(to).unwrap();
    for entry in fs::read_dir(from).unwrap() {
        let entry = entry.unwrap();
        let dest = to.join(entry.file_name());
        if entry.file_type().unwrap().is_dir() {
            copy_dir(&entry.path(), &dest);
        } else {
            fs::copy(entry.path(), dest).unwrap();
        }
    }
}

/// A fresh copy of `heads_default_100u/config` (runs write `data/` into it).
pub fn fresh_config(name: &str) -> PathBuf {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR"))
        .join("tui")
        .join(name);
    let _ = fs::remove_dir_all(&dir);
    copy_dir(&tests_root().join(TEST).join("config"), &dir);
    dir
}

/// A fresh copy of the default configuration shipped with Avida
/// (`avida-core/support/config`; runs write `data/` into it).
pub fn fresh_support_config(name: &str) -> PathBuf {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR"))
        .join("tui")
        .join(name);
    let _ = fs::remove_dir_all(&dir);
    copy_dir(
        &Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../../avida-core/support/config"),
        &dir,
    );
    dir
}

pub fn snap_dir(size: &str) -> PathBuf {
    let d = Path::new(env!("CARGO_TARGET_TMPDIR"))
        .join("tui_snaps")
        .join(size);
    fs::create_dir_all(&d).unwrap();
    d
}

pub fn text_of(buf: &Buffer) -> String {
    let a = buf.area;
    let mut out = String::new();
    for y in 0..a.height {
        let mut line = String::new();
        for x in 0..a.width {
            line.push_str(buf[(x, y)].symbol());
        }
        out.push_str(line.trim_end());
        out.push('\n');
    }
    out
}

pub fn key(code: KeyCode) -> Event {
    Event::Key(KeyEvent {
        code,
        modifiers: KeyModifiers::NONE,
        kind: KeyEventKind::Press,
        state: KeyEventState::NONE,
    })
}

/// Ctrl-C quits from any state (a text prompt would swallow a plain `Q`).
pub fn ctrl_c() -> Event {
    Event::Key(KeyEvent {
        code: KeyCode::Char('c'),
        modifiers: KeyModifiers::CONTROL,
        kind: KeyEventKind::Press,
        state: KeyEventState::NONE,
    })
}

pub fn shift_key(code: KeyCode) -> Event {
    Event::Key(KeyEvent {
        code,
        modifiers: KeyModifiers::SHIFT,
        kind: KeyEventKind::Press,
        state: KeyEventState::NONE,
    })
}

pub fn click(col: u16, row: u16) -> Event {
    Event::Mouse(MouseEvent {
        kind: MouseEventKind::Down(MouseButton::Left),
        column: col,
        row,
        modifiers: KeyModifiers::NONE,
    })
}

type Check = Box<dyn Fn(&App, &str)>;

pub enum Step {
    /// One key press per character.
    Keys(&'static str),
    Event(Event),
    /// An event computed from the app (for mouse positions).
    EventFrom(Box<dyn Fn(&App) -> Event>),
    /// Waits (the loop keeps pumping and drawing) until the condition holds.
    Until(&'static str, Box<dyn Fn(&App) -> bool>),
    /// Runs assertions on the app and the text of the last drawn buffer.
    Check(Check),
    /// Saves the last drawn buffer as text under `snap_dir/<name>.txt`.
    Snap(String),
}

pub struct Script {
    steps: VecDeque<Step>,
    pending_keys: VecDeque<char>,
    last: String,
    snap_dir: PathBuf,
    deadline: Option<(Instant, &'static str)>,
    /// An event was handed out and the screen has not been redrawn since.
    stale: bool,
}

impl Script {
    pub fn new(steps: Vec<Step>, snap_dir: PathBuf) -> Script {
        Script {
            steps: steps.into(),
            pending_keys: VecDeque::new(),
            last: String::new(),
            snap_dir,
            deadline: None,
            stale: false,
        }
    }
}

impl EventSource for Script {
    fn after_draw(&mut self, _app: &App, buffer: &Buffer) {
        self.last = text_of(buffer);
        self.stale = false;
    }

    fn poll(&mut self, app: &App, timeout: Duration) -> io::Result<Option<Event>> {
        loop {
            if let Some(c) = self.pending_keys.pop_front() {
                self.stale = true;
                return Ok(Some(key(KeyCode::Char(c))));
            }
            let Some(step) = self.steps.pop_front() else {
                return Ok(Some(ctrl_c()));
            };
            // Assertions and snapshots need the screen to reflect the
            // events delivered so far: let the loop draw first.
            if self.stale && matches!(step, Step::Check(_) | Step::Snap(_)) {
                self.steps.push_front(step);
                return Ok(None);
            }
            match step {
                Step::Keys(s) => {
                    self.pending_keys = s.chars().collect();
                }
                Step::Event(e) => {
                    self.stale = true;
                    return Ok(Some(e));
                }
                Step::EventFrom(f) => {
                    self.stale = true;
                    return Ok(Some(f(app)));
                }
                Step::Check(f) => f(app, &self.last),
                Step::Snap(name) => {
                    fs::write(self.snap_dir.join(format!("{name}.txt")), &self.last).unwrap();
                }
                Step::Until(what, cond) => {
                    if cond(app) {
                        self.deadline = None;
                        continue;
                    }
                    let (limit, _) = *self
                        .deadline
                        .get_or_insert_with(|| (Instant::now() + test_timeout(), what));
                    assert!(
                        Instant::now() < limit,
                        "timed out waiting for: {what}\nstatus {:?}\nlast screen:\n{}",
                        app.status,
                        self.last
                    );
                    self.steps.push_front(Step::Until(what, cond));
                    std::thread::sleep(timeout.min(Duration::from_millis(10)));
                    return Ok(None);
                }
            }
        }
    }
}

pub struct Run {
    pub driver: Driver,
    pub script: Script,
}

/// Runs `steps` through the real loop on a `w` x `h` test terminal. Ends
/// with a `Q` press if the script does not quit itself.
pub fn run_script(dir: &Path, args: &[&str], w: u16, h: u16, steps: Vec<Step>, snaps: &str) -> Run {
    let mut driver = Driver::start(
        dir.to_path_buf(),
        args.iter().map(|s| s.to_string()).collect(),
        ColorMode::TrueColor,
    )
    .unwrap_or_else(|e| panic!("start: {e}"));
    // Toasts must not expire while a loaded machine runs the script.
    driver.freeze_clock(0.0);
    let mut terminal = Terminal::new(TestBackend::new(w, h)).unwrap();
    let mut script = Script::new(steps, snap_dir(snaps));
    run_loop(&mut terminal, &mut driver, &mut script).unwrap();
    assert!(driver.app.quit);
    Run { driver, script }
}

pub fn has(text: &str, needle: &str) {
    assert!(
        text.contains(needle),
        "expected {needle:?} on screen:\n{text}"
    );
}

/// Names of the instructions of the gene-expression test organisms.
const GENE_BCR_INSTS: &[&str] = &[
    "nop-A",
    "nop-B",
    "nop-C",
    "nop-D",
    "promoter",
    "terminator",
    "inc",
    "dec",
    "one",
    "push",
    "pop",
    "swap-stk",
    "add",
    "set-memory",
    "h-write",
    "wait-cond-gtr",
    "yield",
];
const GENE_GP8_INSTS: &[&str] = &[
    "nop-A",
    "nop-B",
    "nop-C",
    "nop-D",
    "promoter",
    "terminator",
    "inc",
    "dec",
    "push",
    "pop",
    "swap-stk",
    "add",
    "set-memory",
    "h-write",
    "wait-cond-gtr",
    "yield",
];

/// A copy of the `avatars` test configuration run on BCR (`hw_type` 5) or
/// GP8 (4) hardware (the `pp5_bcr_u2` / `pp5_gp8_u0` action cases supply the
/// configuration), with one organism of two genes in cell 12: gene 0 (label
/// `AB`) pushes, swaps stacks and starts a second memory space; gene 1
/// (label `BB`) waits for a register to exceed another thread's.
pub fn gene_config(hw_type: i32, name: &str) -> PathBuf {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR"))
        .join("tui_gene_hardware")
        .join(name);
    let _ = fs::remove_dir_all(&dir);
    copy_dir(&tests_root().join("avatars").join("config"), &dir);
    let case = if hw_type == 5 {
        "pp5_bcr_u2"
    } else {
        "pp5_gp8_u0"
    };
    let extra = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../avida/tests/actions")
        .join(case)
        .join("extra");
    fs::copy(extra.join("environment.cfg"), dir.join("environment.cfg")).unwrap();
    // The recorded configuration is older than the code: no version warning.
    let cfg = fs::read_to_string(extra.join("avida.cfg")).unwrap();
    fs::write(
        dir.join("avida.cfg"),
        cfg.replace("VERSION_ID 2.11.0", "VERSION_ID 2.14.0"),
    )
    .unwrap();
    let insts = if hw_type == 5 {
        GENE_BCR_INSTS
    } else {
        GENE_GP8_INSTS
    };
    let mut set = format!("INSTSET instset:hw_type={hw_type}\n");
    for i in insts {
        set.push_str(&format!("INST {i}\n"));
    }
    fs::write(dir.join("instset.cfg"), set).unwrap();
    let one = if hw_type == 5 { "one" } else { "inc" };
    let genome = [
        "promoter",
        "nop-A",
        "nop-B",
        one,
        "push",
        "inc",
        "swap-stk",
        "set-memory",
        "nop-C",
        one,
        "dec",
        "inc",
        "pop",
        "terminator",
        "promoter",
        "nop-B",
        "nop-B",
        one,
        "inc",
        "wait-cond-gtr",
        "yield",
        "inc",
        "terminator",
    ];
    fs::write(dir.join("genes.org"), genome.join("\n") + "\n").unwrap();
    fs::write(
        dir.join("events.cfg"),
        "u begin Inject genes.org 12\nu 100000 Exit\n",
    )
    .unwrap();
    dir
}
