//! Experiment copies, a harness around the real app, waiting with
//! timeouts, and screenshots.

#![allow(dead_code)]

use std::fs;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use avida_gui::{AvidaApp, StartupArgs};
use egui::Vec2;
use egui_kittest::Harness;

/// Debug builds run the 100 updates of the test config in tens of seconds;
/// `GUI_TEST_TIMEOUT_SECS` shortens the wait when chasing a hang.
pub fn timeout() -> Duration {
    std::env::var("GUI_TEST_TIMEOUT_SECS")
        .ok()
        .and_then(|s| s.parse().ok())
        .map_or(Duration::from_secs(600), Duration::from_secs)
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

/// A fresh copy of `heads_default_100u/config` (runs write `data/` into
/// it), in a folder named `<name>/config` like the original.
pub fn fresh_config(name: &str) -> PathBuf {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR"))
        .join("gui")
        .join(name)
        .join("config");
    let _ = fs::remove_dir_all(&dir);
    copy_dir(
        &tests_root().join("heads_default_100u").join("config"),
        &dir,
    );
    dir
}

/// Where screenshots go: `<target>/tmp/gui-screenshots`.
pub fn screenshot_dir() -> PathBuf {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join("gui-screenshots");
    fs::create_dir_all(&dir).unwrap();
    dir
}

pub fn harness(startup: StartupArgs, size: Vec2) -> Harness<'static, AvidaApp> {
    Harness::builder()
        .with_size(size)
        .with_render_cursor(false)
        .with_max_steps(12)
        .build_eframe(move |cc| AvidaApp::new(cc, startup).without_native_dialogs())
}

/// Steps frames until `cond` holds for the app.
#[track_caller]
pub fn wait_until(
    h: &mut Harness<'static, AvidaApp>,
    what: &str,
    cond: impl Fn(&AvidaApp) -> bool,
) {
    let deadline = Instant::now() + timeout();
    loop {
        h.step();
        if cond(h.state()) {
            // One more frame so the UI shows what the state now holds.
            h.step();
            return;
        }
        assert!(
            Instant::now() < deadline,
            "timed out waiting for {what}; status {:?}, log tail {:?}",
            h.state().state().status,
            h.state()
                .state()
                .log
                .entries()
                .rev()
                .take(5)
                .map(|e| e.text.clone())
                .collect::<Vec<_>>()
        );
        std::thread::sleep(Duration::from_millis(4));
    }
}

/// Runs a few frames so layouts that take two passes settle.
pub fn settle(h: &mut Harness<'static, AvidaApp>) {
    for _ in 0..4 {
        h.step();
    }
}

/// Renders the window and saves it; returns the path.
pub fn screenshot(h: &mut Harness<'static, AvidaApp>, name: &str) -> PathBuf {
    settle(h);
    let path = screenshot_dir().join(format!("{name}.png"));
    match h.render() {
        Ok(img) => {
            img.save(&path).unwrap();
            eprintln!("screenshot: {}", path.display());
        }
        Err(e) => eprintln!("screenshot {name} not rendered: {e}"),
    }
    path
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
        .join("gui_gene_hardware")
        .join(name)
        .join("config");
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
