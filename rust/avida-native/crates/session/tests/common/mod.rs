//! Test directories, the reference run, and a harness that waits on a
//! session's events through its wake callback (with timeouts, no sleeps).

#![allow(dead_code)]

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, Receiver};
use std::time::{Duration, Instant};

use avida_config::cmdline::process_cmdline;
use avida_session::{Session, SessionConfig, SessionEvent, UpdateRow};
use avida_world::world::World;

pub const TEST: &str = "heads_default_100u";
pub const LAST_UPDATE: i32 = 100;

/// Generous: debug builds run the 100 updates in tens of seconds.
pub const TIMEOUT: Duration = Duration::from_secs(600);

/// [`TIMEOUT`], or `SESSION_TEST_TIMEOUT_SECS` when chasing a hang.
pub fn timeout() -> Duration {
    std::env::var("SESSION_TEST_TIMEOUT_SECS")
        .ok()
        .and_then(|s| s.parse().ok())
        .map_or(TIMEOUT, Duration::from_secs)
}

/// `avida-core/tests`
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

/// A fresh copy of `heads_default_100u/config` under the cargo test scratch
/// directory (runs write `data/` into it).
pub fn fresh_config(name: &str) -> PathBuf {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR"))
        .join("session")
        .join(name);
    let _ = fs::remove_dir_all(&dir);
    copy_dir(&tests_root().join(TEST).join("config"), &dir);
    dir
}

/// Rewrites the copy's `events.cfg` line by line.
pub fn edit_events(dir: &Path, edit: impl Fn(&str) -> Option<String>) {
    let path = dir.join("events.cfg");
    let text = fs::read_to_string(&path).unwrap();
    let mut out = String::new();
    for line in text.lines() {
        if let Some(l) = edit(line) {
            out.push_str(&l);
            out.push('\n');
        }
    }
    fs::write(&path, out).unwrap();
}

/// A copy whose run does not stop at update 100.
pub fn endless_config(name: &str) -> PathBuf {
    let dir = fresh_config(name);
    edit_events(&dir, |l| {
        (!l.trim_start().starts_with("u 100 ")).then(|| l.to_string())
    });
    dir
}

/// What the `avida` binary does with no arguments: build the world and
/// `World::run` it straight through. Returns the status lines.
pub fn plain_run(dir: &Path) -> String {
    let cwd = dir.to_string_lossy().into_owned();
    let cmd = process_cmdline(&["avida".to_string()], &cwd);
    assert!(cmd.exit.is_none(), "command line: {}", cmd.stderr);
    let (mut world, _report) =
        World::new(cmd.config, &cwd, &cmd.defs).unwrap_or_else(|e| panic!("World::new: {e}"));
    let mut out = Vec::new();
    world.run(&mut out).unwrap_or_else(|e| panic!("run: {e}"));
    world.out.flush_all();
    String::from_utf8(out).unwrap()
}

/// `# Tue Nov 10 18:24:44 2026`
pub fn is_timestamp(line: &[u8]) -> bool {
    line.len() > 10
        && line.starts_with(b"# ")
        && line[2].is_ascii_uppercase()
        && line[5] == b' '
        && line[6].is_ascii_uppercase()
        && line[9] == b' '
}

/// The file's lines without timestamp comments.
pub fn normalized(path: &Path) -> Vec<Vec<u8>> {
    fs::read(path)
        .unwrap_or_else(|e| panic!("{}: {e}", path.display()))
        .split(|&b| b == b'\n')
        .filter(|l| !is_timestamp(l))
        .map(<[u8]>::to_vec)
        .collect()
}

/// Every file under `dir`, relative to `base`, sorted.
pub fn files_under(dir: &Path, base: &Path, out: &mut Vec<PathBuf>) {
    for entry in fs::read_dir(dir).unwrap() {
        let entry = entry.unwrap();
        if entry.file_type().unwrap().is_dir() {
            files_under(&entry.path(), base, out);
        } else {
            out.push(entry.path().strip_prefix(base).unwrap().to_path_buf());
        }
    }
    out.sort();
}

/// Asserts that two `data/` directories hold the same files with the same
/// contents, timestamp lines aside.
pub fn assert_same_data(a: &Path, b: &Path) {
    let mut fa = Vec::new();
    let mut fb = Vec::new();
    files_under(a, a, &mut fa);
    files_under(b, b, &mut fb);
    assert!(fa.len() >= 10, "expected the run's data files, got {fa:?}");
    assert_eq!(fa, fb, "different sets of data files");
    let differing: Vec<String> = fa
        .iter()
        .filter(|f| normalized(&a.join(f)) != normalized(&b.join(f)))
        .map(|f| f.display().to_string())
        .collect();
    assert!(differing.is_empty(), "files differ: {differing:?}");
    assert!(fs::read(a.join("average.dat")).unwrap().len() > 1000);
}

/// A session plus everything it has published so far.
pub struct Harness {
    pub session: Session,
    wake: Receiver<()>,
    /// Every event drained so far.
    pub events: Vec<SessionEvent>,
    /// Events before this index have been consumed by a wait.
    cursor: usize,
    /// Every row drained so far.
    pub rows: Vec<UpdateRow>,
}

impl Harness {
    pub fn spawn(dir: &Path, args: &[&str]) -> Harness {
        let (tx, rx) = mpsc::channel();
        let session = Session::spawn_with_wake(
            SessionConfig {
                work_dir: dir.to_path_buf(),
                args: args.iter().map(|s| s.to_string()).collect(),
            },
            Box::new(move || {
                let _ = tx.send(());
            }),
        )
        .unwrap_or_else(|e| panic!("spawn: {e}"));
        let mut h = Harness {
            session,
            wake: rx,
            events: Vec::new(),
            cursor: 0,
            rows: Vec::new(),
        };
        // A new session is paused before the first update.
        assert_eq!(h.next_state_event(), SessionEvent::Paused { update: -1 });
        h
    }

    pub fn send(&self, cmd: avida_session::Command) {
        assert!(self.session.send(cmd), "the simulation thread is gone");
    }

    /// Moves what the session has published into the harness.
    pub fn pump(&mut self) {
        self.events.extend(self.session.drain_events());
        self.rows.extend(self.session.drain_rows());
    }

    /// Waits for the next event after the cursor that `pred` accepts and
    /// moves the cursor past it.
    pub fn wait_event(&mut self, pred: impl Fn(&SessionEvent) -> bool) -> SessionEvent {
        let deadline = Instant::now() + timeout();
        loop {
            self.pump();
            if let Some(i) = self.events[self.cursor..].iter().position(&pred) {
                let i = self.cursor + i;
                self.cursor = i + 1;
                return self.events[i].clone();
            }
            let now = Instant::now();
            assert!(
                now < deadline,
                "timed out; last events: {:?}",
                &self.events[self.events.len().saturating_sub(10)..]
            );
            let _ = self
                .wake
                .recv_timeout((deadline - now).min(Duration::from_millis(100)));
        }
    }

    /// The next event that is not a line of text or a birth report.
    pub fn next_state_event(&mut self) -> SessionEvent {
        self.wait_event(|e| {
            !matches!(
                e,
                SessionEvent::Stdout(_) | SessionEvent::Stderr(_) | SessionEvent::Born { .. }
            )
        })
    }

    /// The `Stderr` lines drained so far.
    pub fn stderr_lines(&self) -> Vec<String> {
        self.events
            .iter()
            .filter_map(|e| match e {
                SessionEvent::Stderr(l) => Some(l.clone()),
                _ => None,
            })
            .collect()
    }

    /// Waits until `cond` holds for the session.
    pub fn wait_until(&mut self, cond: impl Fn(&Session) -> bool) {
        let deadline = Instant::now() + timeout();
        while !cond(&self.session) {
            let now = Instant::now();
            assert!(
                now < deadline,
                "timed out; status {:?}",
                self.session.status()
            );
            let _ = self
                .wake
                .recv_timeout((deadline - now).min(Duration::from_millis(20)));
        }
        self.pump();
    }

    /// The `Stdout` lines drained so far.
    pub fn stdout_lines(&self) -> Vec<String> {
        self.events
            .iter()
            .filter_map(|e| match e {
                SessionEvent::Stdout(l) => Some(l.clone()),
                _ => None,
            })
            .collect()
    }

    pub fn row_updates(&self) -> Vec<i32> {
        self.rows.iter().map(|r| r.update).collect()
    }
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
        .join("gene_hardware")
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
