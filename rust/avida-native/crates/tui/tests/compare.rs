//! The comparison screen (`avida-tui --compare DIR`) drawn into test
//! buffers (not a pty) at 80 x 24 and 100 x 30, on synthetic batches
//! written under the target's temporary directory. The screens are also
//! written to `<target>/tmp/tui_snaps/compare/`.

mod common;

use std::fs;
use std::path::{Path, PathBuf};

use avida_tui::compare::{draw, CompareApp, Page};
use avida_viewmodel::compare::CompareState;
use common::{snap_dir, text_of};
use ratatui::backend::TestBackend;
use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::Terminal;

const RUNGS: [&str; 8] = [
    "NOT",
    "NAND",
    "XOR",
    "AND",
    "HALFADD_PLANES",
    "HALFADD",
    "ADD2_PLANES",
    "ADD2",
];

fn root(name: &str) -> PathBuf {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR"))
        .join("compare-tui")
        .join(name);
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    dir
}

fn write_run(dir: &Path, top: usize, pace: i32, end: i32, done: bool, extras: bool) {
    fs::create_dir_all(dir).unwrap();
    let mut tasks = String::from("#  1: Update\n");
    for (k, n) in [
        "Not",
        "Nand",
        "Xor",
        "And",
        "FieldArith",
        "FieldArith",
        "FieldArith",
        "FieldArith",
    ]
    .iter()
    .enumerate()
    {
        tasks.push_str(&format!("#  {}: {n}\n", k + 2));
    }
    let mut count = String::from("#  1: update\n#  2: insts\n#  3: number of organisms\n");
    let mut u = 0;
    while u <= end {
        let reached = ((u / pace.max(1)) as usize).min(top);
        let row: Vec<String> = (0..8)
            .map(|k| if k < reached { "40" } else { "0" }.to_string())
            .collect();
        tasks.push_str(&format!("{u} {}\n", row.join(" ")));
        count.push_str(&format!("{u} 30 100\n"));
        u += 500;
    }
    fs::write(dir.join("tasks.dat"), tasks).unwrap();
    fs::write(dir.join("count.dat"), count).unwrap();
    if done {
        fs::write(dir.join(".done"), "").unwrap();
    }
    if extras {
        let t = top as i32;
        let reuse = if top >= 2 {
            0.1 + 0.05 * f64::from(t)
        } else {
            -1.0
        };
        fs::write(
            dir.join("reuse.dat"),
            format!(
                "#  1: Update\n#  2: Genotype id\n#  3: Genome length\n#  4: Tasks performed\n#  5: Shared sites\n#  6: Reuse index\n{end} 1 {} {t} {} {reuse}\n",
                60 + 8 * t,
                if top >= 2 { t * 2 } else { -1 }
            ),
        )
        .unwrap();
        fs::write(
            dir.join("modes-t500.dat"),
            format!(
                "#  1: Update\n#  2: Average generation\n#  3: Complexity\n{end} 10 {}\n",
                25 + 6 * t
            ),
        )
        .unwrap();
    }
}

fn batch(name: &str, extras: bool) -> PathBuf {
    let r = root(name);
    let b = r.join("batch");
    for (i, &top) in [6, 5, 7, 4, 6, 8].iter().enumerate() {
        write_run(
            &b.join(format!("alpha/seed-{}", i + 1)),
            top,
            1_000 + 120 * i as i32,
            if i == 4 { 9_000 } else { 12_000 },
            i != 4,
            extras,
        );
    }
    for (i, &top) in [3, 4, 2, 5, 3, 1].iter().enumerate() {
        write_run(
            &b.join(format!("beta/seed-{}", i + 1)),
            top,
            1_300 + 90 * i as i32,
            if i >= 4 { 8_000 } else { 12_000 },
            i < 4,
            extras,
        );
    }
    for arm in ["alpha", "beta"] {
        let c = r.join(format!("config-{arm}"));
        fs::create_dir_all(&c).unwrap();
        let text: String = RUNGS
            .iter()
            .map(|n| format!("REACTION {n} t process:x\n"))
            .collect();
        fs::write(c.join("environment.cfg"), text).unwrap();
    }
    b
}

fn app(dir: &Path) -> CompareApp {
    CompareApp::new(CompareState::load(dir).unwrap())
}

fn render(app: &CompareApp, w: u16, h: u16, name: &str) -> String {
    let mut t = Terminal::new(TestBackend::new(w, h)).unwrap();
    let done = t.draw(|f| draw(f, app)).unwrap();
    let text = text_of(done.buffer);
    let dir = snap_dir("compare");
    fs::write(dir.join(format!("{name}-{w}x{h}.txt")), &text).unwrap();
    text
}

fn press(app: &mut CompareApp, c: KeyCode) {
    app.on_key(KeyEvent::new(c, KeyModifiers::NONE));
}

fn has(text: &str, needle: &str) {
    assert!(
        text.contains(needle),
        "expected {needle:?} on screen:\n{text}"
    );
}

#[test]
fn the_summary_is_a_table_of_rungs_and_measures_at_both_sizes() {
    let a = app(&batch("summary", true));
    for (w, h) in [(80, 24), (100, 30)] {
        let t = render(&a, w, h, "summary");
        has(&t, "Compare runs:");
        has(&t, "12 runs in 2 arms: 9 finished, 3 running.");
        has(&t, "alpha n=6");
        has(&t, "beta n=6");
        has(&t, "Rung: ever (end)");
        // Every rung is a row, the top first; alpha held HALFADD in 4 of 6 seeds.
        let add2 = t
            .lines()
            .position(|l| l.starts_with("│ADD2 ") || l.contains(" ADD2 "))
            .unwrap();
        let not = t.lines().position(|l| l.contains("│NOT ")).unwrap();
        assert!(add2 < not, "{t}");
        let half = t.lines().find(|l| l.contains("│HALFADD ")).unwrap();
        assert!(half.contains("4 (4)"), "{half}");
        has(&t, "Median (quartiles)");
        has(&t, "MODES complexity");
        has(&t, "Reuse index");
        has(&t, "Tasks performed");
        // p-values come with the sample sizes.
        assert!(t.contains("0.0") && t.contains(" n 6,6"), "{t}");
        // Nothing is called significant.
        assert!(!t.contains("ignificant"), "{t}");
    }
}

#[test]
fn the_eighty_by_twenty_four_screen_shows_all_of_the_summary() {
    let a = app(&batch("fits", true));
    let t = render(&a, 80, 24, "fits");
    // Last measure row and the key line are both on screen.
    has(&t, "Tasks performed");
    has(&t, "Tab runs");
    assert_eq!(t.lines().count(), 24);
}

#[test]
fn the_runs_page_lists_every_seed_and_the_folder_of_the_chosen_one() {
    let dir = batch("runs", true);
    let mut a = app(&dir);
    press(&mut a, KeyCode::Tab);
    assert_eq!(a.page, Page::Runs);
    let t = render(&a, 100, 30, "runs");
    has(&t, "Highest at end");
    let listed = t
        .lines()
        .filter(|l| l.contains("finished") || l.contains("running"))
        .count();
    assert!(listed >= 12, "{t}");
    has(&t, "running");
    has(&t, "9,000");
    // The first run is chosen; its folder is written under the list.
    has(&t, "alpha/seed-1");
    press(&mut a, KeyCode::Down);
    press(&mut a, KeyCode::Down);
    let t = render(&a, 80, 24, "runs");
    has(&t, "alpha/seed-3");
    assert!(t.lines().any(|l| l.contains("> alpha")), "{t}");
}

#[test]
fn f_keeps_finished_runs_and_p_cycles_the_pair() {
    let mut a = app(&batch("keys", true));
    press(&mut a, KeyCode::Char('f'));
    let t = render(&a, 100, 30, "finished");
    has(&t, "Finished runs only.");
    has(&t, "alpha n=5");
    has(&t, "beta n=4");
    press(&mut a, KeyCode::Char('f'));
    has(&render(&a, 100, 30, "all"), "alpha n=6");
    // Two arms: p says so and changes nothing.
    press(&mut a, KeyCode::Char('p'));
    assert_eq!(a.state.pair, (0, 1));
    assert!(a.message.as_deref().unwrap().contains("Only two arms"));
}

#[test]
fn with_three_arms_p_walks_through_the_pairs() {
    let r = root("three");
    let b = r.join("batch");
    for (arm, top) in [("a", 3), ("b", 5), ("c", 7)] {
        for seed in 1..=3 {
            write_run(
                &b.join(format!("{arm}/seed-{seed}")),
                top,
                1_000,
                6_000,
                true,
                true,
            );
        }
    }
    let mut a = app(&b);
    assert_eq!(a.state.pair, (0, 1));
    let t = render(&a, 80, 24, "three");
    has(&t, "p for a against b");
    press(&mut a, KeyCode::Char('p'));
    assert_eq!(a.state.pair, (0, 2));
    press(&mut a, KeyCode::Char('p'));
    assert_eq!(a.state.pair, (1, 2));
    press(&mut a, KeyCode::Char('p'));
    assert_eq!(a.state.pair, (0, 1));
}

#[test]
fn missing_measure_files_are_not_recorded_and_the_unfinished_run_is_listed() {
    let a = app(&batch("plain", false));
    let t = render(&a, 100, 30, "plain");
    has(&t, "not recorded: needs");
    has(&t, "Rung: ever (end)");
}

#[test]
fn a_run_with_a_bad_file_shows_the_reason_and_r_reads_the_folder_again() {
    let dir = batch("bad", true);
    fs::write(
        dir.join("alpha/seed-2/reuse.dat"),
        "#  1: Update\n#  2: Reuse index\n1 oops\n2 3\n",
    )
    .unwrap();
    let mut a = app(&dir);
    press(&mut a, KeyCode::Tab);
    let t = render(&a, 120, 30, "bad");
    has(&t, "! reuse.dat");
    // The running run finishes; r picks it up.
    fs::write(dir.join("alpha/seed-5/.done"), "").unwrap();
    press(&mut a, KeyCode::Char('r'));
    assert!(a.state.batch.arms[0].runs[4].finished);
    assert_eq!(a.message.as_deref(), Some("Read the folder again."));
}

#[test]
fn q_quits_and_the_keys_of_the_other_screens_do_nothing() {
    let mut a = app(&batch("quit", true));
    for c in ['J', 'V', 'M', 'S'] {
        press(&mut a, KeyCode::Char(c));
    }
    assert!(!a.quit);
    assert_eq!(a.page, Page::Summary);
    press(&mut a, KeyCode::Char('q'));
    assert!(a.quit);
    let mut b = app(&batch("quit2", true));
    b.on_key(KeyEvent::new(KeyCode::Char('c'), KeyModifiers::CONTROL));
    assert!(b.quit);
}

#[test]
fn a_tiny_terminal_says_so_instead_of_drawing_garbage() {
    let a = app(&batch("tiny", true));
    let t = render(&a, 38, 10, "tiny");
    has(&t, "too small");
}
