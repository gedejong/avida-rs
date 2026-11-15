//! The arm comparison's loader and summaries on synthetic batches written
//! under `CARGO_TARGET_TMPDIR` (no window, no world).

use std::fs;
use std::path::{Path, PathBuf};

use avida_viewmodel::compare::{load_batch, Measure, Options, Reading, RungNames};

fn root(name: &str) -> PathBuf {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR"))
        .join("compare")
        .join(name);
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    dir
}

/// A run with four task columns named like the real ones (the last two
/// share a title). `rows` are (update, organisms, counts).
fn write_run(dir: &Path, rows: &[(i32, i32, [i32; 4])], done: bool) {
    fs::create_dir_all(dir).unwrap();
    let mut tasks = String::from(
        "# Avida tasks data\n#  1: Update\n#  2: Not\n#  3: Nand\n#  4: FieldArith\n#  5: FieldArith\n\n",
    );
    let mut count = String::from(
        "# Avida count data\n#  1: update\n#  2: number of insts executed this update\n#  3: number of organisms\n\n",
    );
    for (u, orgs, c) in rows {
        tasks.push_str(&format!("{u} {} {} {} {} \n", c[0], c[1], c[2], c[3]));
        count.push_str(&format!("{u} 30 {orgs}\n"));
    }
    fs::write(dir.join("tasks.dat"), tasks).unwrap();
    fs::write(dir.join("count.dat"), count).unwrap();
    if done {
        fs::write(dir.join(".done"), "").unwrap();
    }
}

fn write_reuse(dir: &Path, rows: &[(i32, i32, i32, f64, i32)]) {
    let mut s = String::from(
        "# reuse\n# -1 means not defined\n#  1: Update\n#  2: Genotype id\n#  3: Genome length\n#  4: Tasks performed\n#  5: Shared sites (knockout loses two or more tasks)\n#  6: Reuse index (mean pairwise Jaccard overlap)\n\n",
    );
    for (u, len, tasks, index, shared) in rows {
        s.push_str(&format!("{u} 1 {len} {tasks} {shared} {index}\n"));
    }
    fs::write(dir.join("reuse.dat"), s).unwrap();
}

fn write_modes(dir: &Path, name: &str, rows: &[(i32, i32)]) {
    let mut s =
        String::from("# MODES\n#  1: Update\n#  2: Average generation\n#  3: Complexity\n\n");
    for (u, c) in rows {
        s.push_str(&format!("{u} 1.5 {c}\n"));
    }
    fs::write(dir.join(name), s).unwrap();
}

const E: [i32; 4] = [0, 0, 0, 0];

/// alpha: seeds 1, 2, 10 (10 sorts after 2); beta: seeds 1 and 2.
fn batch(name: &str) -> PathBuf {
    let r = root(name);
    let b = r.join("batch");
    // alpha 1: finished, climbs to rung 3 (index 2) and stays.
    write_run(
        &b.join("alpha/seed-1"),
        &[
            (0, 100, E),
            (100, 100, [50, 0, 0, 0]),
            (200, 100, [60, 40, 0, 0]),
            (300, 100, [60, 40, 5, 0]),
        ],
        true,
    );
    // alpha 2: finished, reaches index 1 with 0.5 % of 200 only (not held),
    // then 2 of 200 (held) at update 200.
    write_run(
        &b.join("alpha/seed-2"),
        &[
            (0, 200, E),
            (100, 200, [0, 1, 0, 0]),
            (200, 200, [0, 2, 0, 0]),
            (300, 200, [0, 0, 0, 0]),
        ],
        true,
    );
    // alpha 10: finished, longer run (more rows), top rung held.
    write_run(
        &b.join("alpha/seed-10"),
        &[
            (0, 100, E),
            (100, 100, [10, 10, 10, 0]),
            (200, 100, [10, 10, 10, 10]),
            (300, 100, [10, 10, 10, 10]),
            (400, 100, [10, 10, 10, 10]),
        ],
        true,
    );
    // beta 1: finished, only rung 0.
    write_run(
        &b.join("beta/seed-1"),
        &[
            (0, 100, E),
            (100, 100, [30, 0, 0, 0]),
            (300, 100, [30, 0, 0, 0]),
        ],
        true,
    );
    // beta 2: still running, a cut last line.
    write_run(
        &b.join("beta/seed-2"),
        &[(0, 100, E), (100, 100, [20, 3, 0, 0])],
        false,
    );
    let mut t = fs::read_to_string(b.join("beta/seed-2/tasks.dat")).unwrap();
    t.push_str("200 2");
    fs::write(b.join("beta/seed-2/tasks.dat"), t).unwrap();
    // The lab's convention: a config-<arm> folder next to the batch names the rungs.
    for arm in ["alpha", "beta"] {
        let c = r.join(format!("config-{arm}"));
        fs::create_dir_all(&c).unwrap();
        fs::write(
            c.join("environment.cfg"),
            "# rungs\nREACTION NOT not process:x\nREACTION NAND nand process:y\nREACTION HALF field_arith:fn=halfadd process:z\nREACTION ADD field_arith:fn=add process:w\n",
        )
        .unwrap();
    }
    b
}

#[test]
fn a_batch_loads_into_arms_runs_and_ladders() {
    let b = load_batch(&batch("a_batch_loads_into_arms_runs_and_ladders")).unwrap();
    assert_eq!(
        b.arms.iter().map(|a| a.name.as_str()).collect::<Vec<_>>(),
        ["alpha", "beta"]
    );
    // Seeds sort as numbers.
    assert_eq!(
        b.arms[0]
            .runs
            .iter()
            .map(|r| r.seed.as_str())
            .collect::<Vec<_>>(),
        ["1", "2", "10"]
    );
    assert_eq!(b.rungs, ["NOT", "NAND", "HALF", "ADD"]);
    assert!(matches!(b.rung_names, RungNames::Environment(_)));
    assert_eq!((b.run_count(), b.finished_count()), (5, 4));

    let a1 = &b.arms[0].runs[0];
    assert_eq!((a1.end_rung, a1.ever_rung), (Some(2), Some(2)));
    assert_eq!(a1.first_held, [Some(100), Some(200), Some(300), None]);
    assert_eq!(a1.first_reached, [Some(100), Some(200), Some(300), None]);
    assert!(a1.finished);
    assert_eq!(a1.last_update, Some(300));

    // Half a percent is reached but not held; one percent is held; the rung
    // is then lost again at the end.
    let a2 = &b.arms[0].runs[1];
    assert_eq!(a2.first_reached[1], Some(100));
    assert_eq!(a2.first_held[1], Some(200));
    assert_eq!((a2.end_rung, a2.ever_rung), (None, Some(1)));

    // A longer run keeps its own length.
    let a10 = &b.arms[0].runs[2];
    assert_eq!(a10.last_update, Some(400));
    assert_eq!(a10.end_rung, Some(3));

    // A run in progress: not finished, the cut line is left out.
    let b2 = &b.arms[1].runs[1];
    assert!(!b2.finished);
    assert_eq!(b2.last_update, Some(100));
    assert_eq!(b2.status(), "running, last update 100");
    assert!(b2.problems.is_empty(), "{:?}", b2.problems);
    assert_eq!(b2.end_rung, Some(1));
}

#[test]
fn summaries_count_seeds_per_rung_and_per_measure() {
    let b = load_batch(&batch("summaries_count_seeds_per_rung_and_per_measure")).unwrap();
    let s = b.summarize(Options::default());
    assert_eq!((s[0].runs, s[0].finished, s[0].with_ladder), (3, 3, 3));
    // alpha: rung 0 held ever by seeds 1 and 10; rung 1 by all three;
    // rung 2 by 1 and 10; rung 3 by 10.
    let ever: Vec<u32> = s[0].rungs.iter().map(|c| c.ever).collect();
    let end: Vec<u32> = s[0].rungs.iter().map(|c| c.at_end).collect();
    assert_eq!(ever, [2, 3, 2, 1]);
    assert_eq!(end, [2, 2, 2, 1]);
    // beta.
    let ever: Vec<u32> = s[1].rungs.iter().map(|c| c.ever).collect();
    assert_eq!(ever, [2, 1, 0, 0]);
    // Highest rung at the end, numbered from 1: alpha 3, 0, 4.
    let m = &s[0].measures[Measure::RungEnd.index()];
    assert_eq!(m.n, 3);
    assert_eq!(m.spread.unwrap().median, 3.0);
    // Finished runs only drops beta's running seed.
    let only = b.summarize(Options {
        finished_only: true,
    });
    assert_eq!((only[1].runs, only[1].with_ladder), (1, 1));
}

#[test]
fn missing_optional_files_say_not_recorded() {
    let b = load_batch(&batch("missing_optional_files_say_not_recorded")).unwrap();
    for m in [
        Measure::Complexity,
        Measure::ReuseIndex,
        Measure::SharedSites,
        Measure::TasksPerformed,
        Measure::GenomeLength,
    ] {
        assert!(!b.recorded(m), "{m:?}");
    }
    assert!(b.recorded(Measure::RungEnd));
    let s = b.summarize(Options::default());
    let c = &s[0].measures[Measure::Complexity.index()];
    assert_eq!((c.n, c.not_recorded), (0, 3));
    assert!(c.spread.is_none());
}

#[test]
fn measures_are_read_from_the_last_row_by_column_title() {
    let b = batch("measures_are_read_from_the_last_row_by_column_title");
    let a1 = b.join("alpha/seed-1");
    write_reuse(&a1, &[(100, 90, 1, -1.0, -1), (300, 120, 3, 0.25, 7)]);
    write_modes(&a1, "modes-t500.dat", &[(200, 40), (300, 55)]);
    fs::write(
        a1.join("dominant.dat"),
        "#  1: Update\n#  2: Merit\n#  3: Size of Dominant Genotype\n#  4: Name\n300 1 130 100-aaa\n",
    )
    .unwrap();
    // seed 2: the dominant is undefined (-1) at the last row.
    write_reuse(&b.join("alpha/seed-2"), &[(300, 80, 1, -1.0, -1)]);
    let batch = load_batch(&b).unwrap();
    let r = &batch.arms[0].runs[0];
    assert_eq!(r.reading(Measure::ReuseIndex), Reading::Value(0.25, 300));
    assert_eq!(r.reading(Measure::SharedSites), Reading::Value(7.0, 300));
    assert_eq!(r.reading(Measure::TasksPerformed), Reading::Value(3.0, 300));
    assert_eq!(r.reading(Measure::Complexity), Reading::Value(55.0, 300));
    // dominant.dat wins for the genome length.
    assert_eq!(r.reading(Measure::GenomeLength), Reading::Value(130.0, 300));
    let r2 = &batch.arms[0].runs[1];
    assert_eq!(r2.reading(Measure::ReuseIndex), Reading::Undefined(300));
    assert_eq!(r2.reading(Measure::GenomeLength), Reading::Value(80.0, 300));
    let s = batch.summarize(Options::default());
    let m = &s[0].measures[Measure::ReuseIndex.index()];
    assert_eq!((m.n, m.undefined, m.not_recorded), (1, 1, 1));
    assert!(batch.recorded(Measure::ReuseIndex));
    assert!(batch.recorded(Measure::Complexity));
}

#[test]
fn a_malformed_file_gives_one_reason_and_the_rest_of_the_run_loads() {
    let b = batch("a_malformed_file_gives_one_reason_and_the_rest_of_the_run_loads");
    let a1 = b.join("alpha/seed-1");
    fs::write(
        a1.join("reuse.dat"),
        "#  1: Update\n#  2: Reuse index\n1 oops\n2 3\n",
    )
    .unwrap();
    write_modes(&a1, "modes-x.dat", &[(100, 33)]);
    let batch = load_batch(&b).unwrap();
    let r = &batch.arms[0].runs[0];
    assert!(
        r.problems.iter().any(|p| p.starts_with("reuse.dat")),
        "{:?}",
        r.problems
    );
    // The ladder and the other files are still there.
    assert_eq!(r.end_rung, Some(2));
    assert_eq!(r.reading(Measure::Complexity), Reading::Value(33.0, 100));
    // Other runs are untouched.
    assert!(batch.arms[0].runs[1].problems.is_empty());
}

#[test]
fn a_run_without_data_yet_is_listed_not_failed() {
    let b = batch("a_run_without_data_yet_is_listed_not_failed");
    fs::create_dir_all(b.join("beta/seed-3")).unwrap();
    let batch = load_batch(&b).unwrap();
    let r = &batch.arms[1].runs[2];
    assert_eq!(r.status(), "no data yet");
    assert!(!r.has_ladder());
    let s = batch.summarize(Options::default());
    assert_eq!((s[1].runs, s[1].with_ladder), (3, 2));
}

#[test]
fn rung_names_fall_back_to_the_column_titles() {
    let b = batch("rung_names_fall_back_to_the_column_titles");
    fs::remove_dir_all(b.parent().unwrap().join("config-alpha")).unwrap();
    fs::remove_dir_all(b.parent().unwrap().join("config-beta")).unwrap();
    let batch = load_batch(&b).unwrap();
    assert_eq!(batch.rung_names, RungNames::Columns);
    assert_eq!(
        batch.rungs,
        ["Not", "Nand", "FieldArith #3", "FieldArith #4"]
    );
}

#[test]
fn a_run_with_another_number_of_tasks_is_reported_and_left_out_of_the_ladder() {
    let b = batch("a_run_with_another_number_of_tasks_is_reported_and_left_out_of_the_ladder");
    let odd = b.join("beta/seed-3");
    fs::create_dir_all(&odd).unwrap();
    fs::write(odd.join("tasks.dat"), "#  1: Update\n#  2: Not\n0 0\n").unwrap();
    fs::write(
        odd.join("count.dat"),
        "#  1: update\n#  2: x\n#  3: number of organisms\n0 1 5\n",
    )
    .unwrap();
    let batch = load_batch(&b).unwrap();
    let r = &batch.arms[1].runs[2];
    assert!(!r.has_ladder());
    assert!(r.problems[0].contains("task columns"), "{:?}", r.problems);
}

#[test]
fn a_folder_without_arms_is_an_error_and_a_lone_arm_loads() {
    let r = root("empty");
    assert!(load_batch(&r).unwrap_err().contains("arm"));
    assert!(load_batch(&r.join("missing")).is_err());
    let b = batch("a_folder_without_arms_is_an_error_and_a_lone_arm_loads");
    let one = load_batch(&b.join("alpha")).unwrap();
    assert_eq!(one.arms.len(), 1);
    assert_eq!(one.arms[0].name, "alpha");
    assert!(!one.notes.is_empty());
}

#[test]
fn two_arms_are_compared_with_exact_tests() {
    let r = root("compare");
    let b = r.join("b");
    // Three seeds each; arm "hi" reaches rung 2 everywhere, "lo" never.
    for (arm, counts) in [("hi", [30, 30, 30, 0]), ("lo", [30, 0, 0, 0])] {
        for seed in 1..=3 {
            write_run(
                &b.join(format!("{arm}/seed-{seed}")),
                &[(0, 100, E), (100, 100, counts)],
                true,
            );
        }
    }
    let batch = load_batch(&b).unwrap();
    let c = batch.compare(0, 1, Options::default()).unwrap();
    // Rung 3 (index 1) was held by 3 of 3 against 0 of 3: Fisher 2 / 20.
    assert!((c.rungs[1].p_ever - 0.1).abs() < 1e-12);
    assert!((c.rungs[1].p_end - 0.1).abs() < 1e-12);
    // Rung 1 was held by everyone: p = 1.
    assert!((c.rungs[0].p_ever - 1.0).abs() < 1e-12);
    // Highest rung at the end: 3 3 3 against 1 1 1: Mann-Whitney 2 / 20.
    let t = c.measures[Measure::RungEnd.index()].test.unwrap();
    assert!((t.p - 0.1).abs() < 1e-12);
    assert_eq!((t.n_a, t.n_b), (3, 3));
    // A measure nobody recorded has no test.
    assert!(c.measures[Measure::Complexity.index()].test.is_none());
    assert!(batch.compare(0, 5, Options::default()).is_none());
}

#[test]
fn the_median_series_follows_runs_over_their_own_length() {
    let b = load_batch(&batch(
        "the_median_series_follows_runs_over_their_own_length",
    ))
    .unwrap();
    let m = b.median_series(0, Options::default());
    let at = |u: i32| m.iter().find(|p| p.update == u).copied().unwrap();
    // Update 100: levels of alpha 1, 2, 10 are 1, 0 (0.5 %), 3: median 1.
    assert_eq!((at(100).median, at(100).n), (1.0, 3));
    // Update 400: only seed 10 is still recorded.
    assert_eq!((at(400).median, at(400).n), (4.0, 1));
    // Past the end of every run there is no point.
    assert!(m.iter().all(|p| p.update <= 400));
    // beta's median stops where its runs stop; the unfinished one counts
    // from its first to its last row.
    let m1 = b.median_series(1, Options::default());
    assert_eq!(m1.last().unwrap().update, 300);
    assert_eq!(m1.iter().find(|p| p.update == 300).unwrap().n, 1);
    assert!(b.median_series(9, Options::default()).is_empty());
}

/// Reads a real batch when `AVIDA_COMPARE_BATCH` names one (`--ignored`):
/// prints what the view would show.
#[test]
#[ignore]
fn print_a_real_batch() {
    let Ok(dir) = std::env::var("AVIDA_COMPARE_BATCH") else {
        return;
    };
    let b = load_batch(Path::new(&dir)).unwrap();
    println!("rungs {:?} from {:?}", b.rungs, b.rung_names);
    for n in &b.notes {
        println!("note: {n}");
    }
    for arm in &b.arms {
        for r in &arm.runs {
            println!(
                "{} seed {}: {} end {:?} ever {:?} problems {:?} readings {:?}",
                arm.name,
                r.seed,
                r.status(),
                r.end_rung,
                r.ever_rung,
                r.problems,
                r.readings
            );
        }
    }
    for s in b.summarize(Options::default()) {
        println!("{} runs {} rungs {:?}", s.name, s.runs, s.rungs);
        for m in &s.measures {
            println!(
                "   {:?} n {} undef {} nr {} {:?}",
                m.measure, m.n, m.undefined, m.not_recorded, m.spread
            );
        }
    }
    if b.arms.len() >= 2 {
        let c = b.compare(0, 1, Options::default()).unwrap();
        println!("rung tests {:?}", c.rungs);
        for m in &c.measures {
            println!("   {:?} {:?}", m.measure, m.test);
        }
    }
}

#[test]
fn the_state_keeps_choices_over_a_reload_and_derives_the_numbers() {
    use avida_viewmodel::compare::CompareState;
    let dir = batch("state");
    let mut s = CompareState::load(&dir).unwrap();
    assert!(s.has_pair());
    assert_eq!(s.pair, (0, 1));
    assert_eq!(s.summary.len(), 2);
    assert!(s.comparison.is_some());
    assert_eq!(s.max_update, 400);
    s.set_finished_only(true);
    assert_eq!(s.summary[1].runs, 1);
    // A pair of the same arm, or out of range, is refused.
    s.set_pair(0, 0);
    s.set_pair(0, 7);
    assert_eq!(s.pair, (0, 1));
    s.selected = Some((1, 1));
    // The running run finishes and writes more.
    write_run(
        &dir.join("beta/seed-2"),
        &[
            (0, 100, E),
            (100, 100, [20, 3, 0, 0]),
            (200, 100, [20, 3, 9, 0]),
            (500, 100, [20, 3, 9, 9]),
        ],
        true,
    );
    s.reload().unwrap();
    assert!(s.finished_only);
    assert_eq!(s.summary[1].runs, 2);
    assert_eq!(s.max_update, 500);
    assert_eq!(s.selected, Some((1, 1)));
    // A single arm has no pair to test.
    let one = CompareState::load(&dir.join("alpha")).unwrap();
    assert!(!one.has_pair());
    assert!(one.comparison.is_none());
    assert!(CompareState::load(&root("state-empty")).is_err());
}

#[test]
fn p_values_are_written_with_two_or_three_decimals() {
    use avida_viewmodel::compare::p_text;
    assert_eq!(p_text(0.0004), "< 0.001");
    assert_eq!(p_text(0.0123), "0.012");
    assert_eq!(p_text(0.1), "0.10");
    assert_eq!(p_text(0.4567), "0.46");
    assert_eq!(p_text(0.9999999999999994), "1.00");
}
