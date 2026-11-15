//! The comparison screen in the real application, on synthetic batches
//! written under the target's temporary directory: opening a batch from the
//! command line and from the start screen, the panels' text, the pair
//! selector, finished runs only, the folder of a run, and screenshots in
//! both themes at 1000 x 650 and 1280 x 800 (`<target>/tmp/gui-screenshots/compare/`).

mod common;

use std::fs;
use std::path::{Path, PathBuf};

use avida_gui::settings::ThemeChoice;
use avida_gui::StartupArgs;
use common::{harness, screenshot_dir, settle};
use egui::{vec2, Role, Vec2};
use egui_kittest::kittest::{by, Queryable};

type H = egui_kittest::Harness<'static, avida_gui::AvidaApp>;

const SIZES: [(&str, Vec2); 2] = [
    ("1000x650", vec2(1000.0, 650.0)),
    ("1280x800", vec2(1280.0, 800.0)),
];
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
        .join("compare-gui")
        .join(name);
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    dir
}

/// One run that reaches rung `top` (0 to 8 rungs) after `pace` updates per
/// rung and runs to `end`.
fn write_run(dir: &Path, top: usize, pace: i32, end: i32, done: bool, extras: bool) {
    fs::create_dir_all(dir).unwrap();
    let mut tasks = String::from("# Avida tasks data\n#  1: Update\n");
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
    tasks.push('\n');
    let mut count =
        String::from("# count\n#  1: update\n#  2: insts\n#  3: number of organisms\n\n");
    let mut u = 0;
    while u <= end {
        let reached = ((u / pace.max(1)) as usize).min(top);
        let row: Vec<String> = (0..8)
            .map(|k| {
                if k < reached {
                    "40".to_string()
                } else {
                    "0".to_string()
                }
            })
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
        let tasks = top as i32;
        let reuse = if top >= 2 {
            0.1 + 0.05 * f64::from(tasks)
        } else {
            -1.0
        };
        fs::write(
            dir.join("reuse.dat"),
            format!(
                "#  1: Update\n#  2: Genotype id\n#  3: Genome length\n#  4: Tasks performed\n#  5: Shared sites\n#  6: Reuse index\n{end} 1 {} {tasks} {} {reuse}\n",
                60 + 8 * tasks,
                if top >= 2 { top as i32 * 2 } else { -1 }
            ),
        )
        .unwrap();
        fs::write(
            dir.join("modes-t500.dat"),
            format!(
                "#  1: Update\n#  2: Average generation\n#  3: Complexity\n{end} 10 {}\n",
                25 + 6 * tasks
            ),
        )
        .unwrap();
    }
}

fn environment(batch_parent: &Path, arms: &[&str]) {
    for arm in arms {
        let dir = batch_parent.join(format!("config-{arm}"));
        fs::create_dir_all(&dir).unwrap();
        let mut text = String::new();
        for r in RUNGS {
            text.push_str(&format!("REACTION {r} task process:x\n"));
        }
        fs::write(dir.join("environment.cfg"), text).unwrap();
    }
}

/// Two arms, six seeds each; the second climbs less; some runs still going.
fn batch(name: &str, extras: bool) -> PathBuf {
    let r = root(name);
    let b = r.join("batch");
    let alpha = [6, 5, 7, 4, 6, 8];
    let beta = [3, 4, 2, 5, 3, 1];
    for (i, &top) in alpha.iter().enumerate() {
        let end = if i == 4 { 9_000 } else { 12_000 };
        write_run(
            &b.join(format!("alpha/seed-{}", i + 1)),
            top,
            1_000 + 120 * i as i32,
            end,
            i != 4,
            extras,
        );
    }
    for (i, &top) in beta.iter().enumerate() {
        let end = if i >= 4 { 8_000 } else { 12_000 };
        write_run(
            &b.join(format!("beta/seed-{}", i + 1)),
            top,
            1_300 + 90 * i as i32,
            end,
            i < 4,
            extras,
        );
    }
    environment(&r, &["alpha", "beta"]);
    b
}

fn open(dir: PathBuf, size: Vec2) -> H {
    let mut h = harness(
        StartupArgs {
            compare: Some(dir),
            ..Default::default()
        },
        size,
    );
    settle(&mut h);
    h
}

/// Scrolls the page to a widget (the table is below the fold) and clicks it.
fn pick(h: &mut H, label: &str) {
    h.get_by_label(label).scroll_to_me();
    settle(h);
    h.get_by_label(label).click();
    settle(h);
}

fn shoot(h: &mut H, name: &str) {
    let dir = screenshot_dir().join("compare");
    fs::create_dir_all(&dir).unwrap();
    let before = h.state().settings().theme;
    for (size_name, size) in SIZES {
        h.set_size(size);
        for (theme_name, theme) in [("dark", ThemeChoice::Dark), ("light", ThemeChoice::Light)] {
            h.state_mut().settings_mut().theme = theme;
            settle(h);
            let path = dir.join(format!("{name}-{size_name}-{theme_name}.png"));
            match h.render() {
                Ok(img) => img.save(&path).unwrap(),
                Err(e) => eprintln!("{name}: not rendered: {e}"),
            }
        }
    }
    h.set_size(SIZES[1].1);
    h.state_mut().settings_mut().theme = before;
    settle(h);
}

#[test]
fn a_batch_opens_from_the_command_line_with_every_panel() {
    let h = open(batch("panels", true), SIZES[1].1);
    h.get_by_label("Compare runs");
    h.get_by_label_contains("12 runs in 2 arms: 9 finished, 3 still running");
    // A dot for every seed on every rung: 12 seeds x 8 rungs.
    let dots = h
        .query_all(by().role(Role::Image).label_contains(", seed "))
        .count();
    // Rung dots plus the strips' dots (5 measures x up to 12 seeds) plus medians.
    assert!(dots >= 96, "{dots}");
    // The counts per arm and rung, in words for the hover and the reader.
    h.get_by_label("alpha: 6 of 6 seeds held NOT at some update; 6 hold it at the end.");
    h.get_by_label_contains("alpha, seed 6: ADD2_PLANES held at the end");
    h.get_by_label_contains("alpha, seed 1: ADD2_PLANES never held");
    h.get_by_label_contains("beta, seed 6: NOT held at the end");
    // The first alpha seed's folder is in its row's hover; the table lists runs.
    h.get_by_label("Run alpha seed 1");
    h.get_by_label("Run beta seed 6");
    // The tests: Fisher per rung, Mann-Whitney per measure, both with n.
    h.get_by_label_contains("ADD2_PLANES p = ");
    assert_eq!(h.query_all_by_label_contains("n = 6 and 6").count(), 3);
    // No claim of significance anywhere on the page.
    assert!(h
        .query_all(by().label_contains("ignificant"))
        .next()
        .is_none());
    h.check_accessibility();
}

#[test]
fn running_runs_are_shown_as_running_with_their_last_update() {
    let h = open(batch("running", true), SIZES[1].1);
    // alpha seed 5 stopped at update 9,000 without a .done file.
    assert_eq!(h.query_all_by_label("running").count(), 3);
    assert!(h.query_all_by_label_contains("last update 9,000").count() > 0);
    let c = h.state().compare().unwrap();
    let run = &c.batch.arms[0].runs[4];
    assert!(!run.finished);
    assert_eq!(run.last_update, Some(9_000));
    assert_eq!(run.status(), "running, last update 9,000");
}

#[test]
fn finished_runs_only_drops_the_running_ones() {
    let mut h = open(batch("finished-only", true), SIZES[1].1);
    h.get_by_label("Finished runs only").click();
    settle(&mut h);
    let c = h.state().compare().unwrap();
    assert!(c.finished_only);
    assert_eq!(c.summary[0].runs, 5);
    assert_eq!(c.summary[1].runs, 4);
    assert!(h.query_all_by_label_contains("n = 5").count() > 0);
    assert!(h.query_by_label("Run alpha seed 5").is_none());
    h.get_by_label("Run alpha seed 4");
}

#[test]
fn without_reuse_or_modes_files_the_panels_say_not_recorded() {
    let h = open(batch("plain", false), SIZES[1].1);
    // Five strips, none recorded.
    assert_eq!(h.query_all_by_label("Not recorded").count(), 5);
    // The ladder is there all the same.
    h.get_by_label("alpha: 6 of 6 seeds held NOT at some update; 6 hold it at the end.");
}

#[test]
fn selecting_a_run_shows_its_folder_and_copies_the_path() {
    let dir = batch("folder", true);
    let mut h = open(dir.clone(), SIZES[1].1);
    h.get_by_label("Select a run to see its folder.");
    pick(&mut h, "Run beta seed 2");
    let want = std::path::absolute(&dir)
        .unwrap()
        .join("beta/seed-2")
        .display()
        .to_string();
    assert!(h.query_all_by_label_contains(&want).count() > 0, "{want}");
    pick(&mut h, "Copy path");
    h.get_by_label_contains("Copied to the clipboard");
}

#[test]
fn more_than_two_arms_compare_the_pair_that_is_picked() {
    let r = root("three");
    let b = r.join("batch");
    for (arm, top) in [("a", 3), ("b", 5), ("c", 7)] {
        for seed in 1..=4 {
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
    environment(&r, &["a", "b", "c"]);
    let mut h = open(b, SIZES[1].1);
    h.get_by_label_contains("p-values compare");
    assert_eq!(h.state().compare().unwrap().pair, (0, 1));
    h.state_mut().compare_mut().unwrap().set_pair(0, 2);
    settle(&mut h);
    let c = h.state().compare().unwrap().comparison.as_ref().unwrap();
    assert_eq!((c.a, c.b), (0, 2));
    // Three arms: three legend entries, three dot rows per rung.
    h.get_by_label_contains("12 runs in 3 arms");
    shoot(&mut h, "3-three-arms");
}

#[test]
fn a_folder_without_arms_leaves_the_start_screen_and_says_why() {
    let dir = root("none");
    let mut h = harness(StartupArgs::default(), SIZES[1].1);
    h.state_mut().answer_folder_dialogs_with(dir);
    settle(&mut h);
    h.get_by_label("Compare runs…").click();
    settle(&mut h);
    assert!(h.state().compare().is_none());
    h.get_by_label_contains("has no arm folders");
}

#[test]
fn the_start_screen_opens_a_batch_and_close_returns_to_it() {
    let mut h = harness(StartupArgs::default(), SIZES[1].1);
    h.state_mut()
        .answer_folder_dialogs_with(batch("from-start", true));
    settle(&mut h);
    h.get_by_label("Compare runs…").click();
    settle(&mut h);
    assert!(h.state().compare().is_some());
    h.get_by_label("Close").click();
    settle(&mut h);
    assert!(h.state().compare().is_none());
    h.get_by_label("Open folder…");
}

#[test]
fn screenshots_of_the_comparison_in_both_themes_and_sizes() {
    let mut h = open(batch("shots", true), SIZES[1].1);
    shoot(&mut h, "1-two-arms");
    h.get_by_label("MODES complexity").scroll_to_me();
    settle(&mut h);
    shoot(&mut h, "1b-measures");
    pick(&mut h, "Run alpha seed 2");
    h.get_by_label("Copy path").scroll_to_me();
    settle(&mut h);
    shoot(&mut h, "2-selected");
}
