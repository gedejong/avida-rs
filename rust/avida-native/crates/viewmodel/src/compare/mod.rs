//! The arm comparison: a batch of runs, `<batch>/<arm>/seed-<N>/`, loaded
//! from the data files the runs wrote, and what the front ends show of it.
//!
//! An experiment compares arms (instruction sets, sexual and asexual
//! reproduction, population sizes, ...) over several seeds on a ladder of
//! rewarded tasks. Nothing here knows the arms' or the tasks' names: an arm
//! is a sub-directory of the batch that holds `seed-*` directories, a rung
//! is a column of `tasks.dat` in file order (the environment's reactions,
//! the lowest first). The module is pure (files in, plain data out), has no
//! dependency on the world, and is used by both front ends.
//!
//! # Files read
//!
//! Per run (a missing optional file leaves its panels at "not recorded";
//! a file that cannot be read adds one line to [`Run::problems`] and the
//! rest of the run still loads):
//!
//! * `tasks.dat` (required for the ladder): update, then one column per
//!   task, the organisms that performed it. `count.dat`, matched on the
//!   update, gives the population ("number of organisms"). A rung is held
//!   where [`crate::ladder::is_held`] says so (at least one organism and at
//!   least 1 % of the population), the one definition of the ladder view.
//! * `reuse.dat` (`PrintReuseData`): reuse index, shared sites and tasks
//!   performed of the dominant genotype; `-1` is "undefined" and is not
//!   plotted. `dominant.dat`: genome length of the dominant genotype.
//! * `modes*.dat` (`PrintMODESData`, the first such file by name):
//!   complexity.
//! * `.done`: the run has finished. Without it a run is shown as running,
//!   with the update of its last row.
//!
//! Columns are found by their titles in the `#` header, never by position.
//! `phylogeny.dat` and `knockout_map.dat` are not read.
//!
//! The final value of a measure is the last row of its file; see [`Reading`].
//!
//! # Rung names
//!
//! `tasks.dat` names the tasks, not the reactions (four field-arithmetic
//! rungs are all "FieldArith"). The rung names are the `REACTION` names, in
//! order, of the first `environment.cfg` found at `<batch>/`,
//! `<batch>/<arm>/`, `<run>/` or `<batch>/../config-<arm>/` whose reaction
//! count equals the task columns; otherwise the column titles, with
//! `#<rung number>` added to a title that occurs more than once.

mod dat;
pub mod stats;
mod view;

pub use view::CompareState;

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use crate::ladder::{is_held, LadderPoint};
pub use stats::{
    fisher_exact, mann_whitney, mann_whitney_normal, spread, MannWhitney, Method, Spread,
    EXACT_MAX_N,
};

/// The scalar measures compared across seeds.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Measure {
    /// Highest rung held at the end, as a number: 0 none, 1 the lowest rung.
    RungEnd,
    /// Highest rung held at any recorded update, numbered the same way.
    RungEver,
    /// MODES complexity: instructions of the longest skeleton.
    Complexity,
    /// Mean pairwise overlap of the site sets of the dominant's tasks.
    ReuseIndex,
    /// Sites whose knockout loses two or more tasks.
    SharedSites,
    /// Genome length of the dominant genotype.
    GenomeLength,
    /// Tasks the dominant genotype performs.
    TasksPerformed,
}

impl Measure {
    pub const ALL: [Measure; 7] = [
        Measure::RungEnd,
        Measure::RungEver,
        Measure::Complexity,
        Measure::ReuseIndex,
        Measure::SharedSites,
        Measure::GenomeLength,
        Measure::TasksPerformed,
    ];

    /// The measures drawn as strips (the two rung measures have the ladder
    /// panels).
    pub const STRIPS: [Measure; 5] = [
        Measure::Complexity,
        Measure::ReuseIndex,
        Measure::SharedSites,
        Measure::GenomeLength,
        Measure::TasksPerformed,
    ];

    pub fn index(self) -> usize {
        Measure::ALL.iter().position(|&m| m == self).unwrap_or(0)
    }

    pub fn label(self) -> &'static str {
        match self {
            Measure::RungEnd => "Rung at the end (no.)",
            Measure::RungEver => "Rung ever (no.)",
            Measure::Complexity => "MODES complexity",
            Measure::ReuseIndex => "Reuse index",
            Measure::SharedSites => "Shared sites",
            Measure::GenomeLength => "Genome length",
            Measure::TasksPerformed => "Tasks performed",
        }
    }

    pub fn short(self) -> &'static str {
        match self {
            Measure::RungEnd => "rung end",
            Measure::RungEver => "rung ever",
            Measure::Complexity => "complexity",
            Measure::ReuseIndex => "reuse index",
            Measure::SharedSites => "shared sites",
            Measure::GenomeLength => "genome length",
            Measure::TasksPerformed => "tasks",
        }
    }

    /// What it is and where it comes from.
    pub fn description(self) -> &'static str {
        match self {
            Measure::RungEnd => "The highest rung that at least 1% of the organisms performed at the last update of the run, counted from 1 for the lowest rung; 0 means none.",
            Measure::RungEver => "The highest rung that at least 1% of the organisms performed at any recorded update, counted from 1 for the lowest rung; 0 means none.",
            Measure::Complexity => "The number of instructions in the longest skeleton of the MODES measure (the instructions whose replacement by a null instruction lowers the test fitness). The last row of the MODES file.",
            Measure::ReuseIndex => "The mean pairwise overlap (Jaccard) of the sites each task needs in the dominant genotype, from single-site knockouts. Higher means more shared code. Undefined with fewer than two tasks. The last row of reuse.dat.",
            Measure::SharedSites => "The sites of the dominant genotype whose knockout loses two or more tasks. The last row of reuse.dat; undefined with fewer than two tasks.",
            Measure::GenomeLength => "The length of the dominant genotype at the last recorded update (dominant.dat, or reuse.dat when that is missing).",
            Measure::TasksPerformed => "The number of tasks the dominant genotype performs in the test CPU. The last row of reuse.dat.",
        }
    }

    /// Whether the values are whole numbers.
    pub fn integer(self) -> bool {
        !matches!(self, Measure::ReuseIndex)
    }

    /// The file (or files) a front end names when the measure is not
    /// recorded.
    pub fn file_hint(self) -> &'static str {
        match self {
            Measure::RungEnd | Measure::RungEver => "tasks.dat and count.dat",
            Measure::Complexity => "a modes*.dat file (PrintMODESData)",
            Measure::ReuseIndex | Measure::SharedSites | Measure::TasksPerformed => {
                "reuse.dat (PrintReuseData)"
            }
            Measure::GenomeLength => "dominant.dat or reuse.dat",
        }
    }
}

/// What a run says about one measure.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Reading {
    /// The file or column is not there.
    NotRecorded,
    /// The file is there and has no row yet.
    NoRows,
    /// The last row says "not defined" (`-1`) at this update.
    Undefined(i32),
    /// The last row's value, and its update.
    Value(f64, i32),
}

impl Reading {
    pub fn value(self) -> Option<f64> {
        match self {
            Reading::Value(v, _) => Some(v),
            _ => None,
        }
    }

    pub fn update(self) -> Option<i32> {
        match self {
            Reading::Undefined(u) | Reading::Value(_, u) => Some(u),
            _ => None,
        }
    }
}

/// One run: `<batch>/<arm>/seed-<N>/`.
#[derive(Clone, Debug)]
pub struct Run {
    pub arm: String,
    /// The text after `seed-` in the directory name.
    pub seed: String,
    pub dir: PathBuf,
    /// A `.done` file is there.
    pub finished: bool,
    /// The update of the newest row of `tasks.dat` that could be used.
    pub last_update: Option<i32>,
    /// One line per file that could not be read or lacks what is needed.
    pub problems: Vec<String>,
    /// Task columns of `tasks.dat` (0 when there is no usable ladder).
    pub rung_count: usize,
    /// The ladder series: per row the population and the task counts.
    pub points: Vec<LadderPoint>,
    /// Per row, the highest rung held plus one (0: none).
    pub levels: Vec<(i32, u32)>,
    /// The first update at which any organism performed each rung.
    pub first_reached: Vec<Option<i32>>,
    /// The first update at which 1 % or more did.
    pub first_held: Vec<Option<i32>>,
    /// The highest rung held at the last row, as an index.
    pub end_rung: Option<usize>,
    /// The highest rung held at any row.
    pub ever_rung: Option<usize>,
    /// By [`Measure::index`].
    pub readings: Vec<Reading>,
}

impl Run {
    fn empty(arm: &str, seed: &str, dir: &Path) -> Run {
        Run {
            arm: arm.to_string(),
            seed: seed.to_string(),
            dir: dir.to_path_buf(),
            finished: dir.join(".done").exists(),
            last_update: None,
            problems: Vec::new(),
            rung_count: 0,
            points: Vec::new(),
            levels: Vec::new(),
            first_reached: Vec::new(),
            first_held: Vec::new(),
            end_rung: None,
            ever_rung: None,
            readings: vec![Reading::NotRecorded; Measure::ALL.len()],
        }
    }

    /// Whether the ladder could be read.
    pub fn has_ladder(&self) -> bool {
        !self.points.is_empty()
    }

    pub fn reading(&self, m: Measure) -> Reading {
        self.readings[m.index()]
    }

    /// The ladder level (highest rung held plus one) at `update`: the
    /// newest row at or before it, `None` after the run's last row or
    /// before its first.
    pub fn level_at(&self, update: i32) -> Option<u32> {
        let last = self.levels.last()?.0;
        if update > last {
            return None;
        }
        let i = self.levels.partition_point(|&(u, _)| u <= update);
        i.checked_sub(1).map(|i| self.levels[i].1)
    }

    /// A one-line state: "finished", "running, last update 3,900" or
    /// "no data yet".
    pub fn status(&self) -> String {
        match (self.finished, self.last_update) {
            (true, Some(u)) => format!("finished at update {}", grouped(u)),
            (true, None) => "finished".to_string(),
            (false, Some(u)) => format!("running, last update {}", grouped(u)),
            (false, None) => "no data yet".to_string(),
        }
    }
}

/// A p-value as the front ends write it: three decimals below 0.1, two
/// above, `< 0.001` for the smallest and `1.00` for one that rounds to 1.
pub fn p_text(p: f64) -> String {
    if p < 0.001 {
        "< 0.001".to_string()
    } else if p >= 0.995 {
        "1.00".to_string()
    } else if p >= 0.1 {
        format!("{p:.2}")
    } else {
        format!("{p:.3}")
    }
}

/// A number with thousands separators: `12,300`.
pub fn grouped(v: i32) -> String {
    let digits = v.unsigned_abs().to_string();
    let mut out = String::new();
    if v < 0 {
        out.push('-');
    }
    for (i, ch) in digits.chars().enumerate() {
        if i > 0 && (digits.len() - i).is_multiple_of(3) {
            out.push(',');
        }
        out.push(ch);
    }
    out
}

/// An arm: a directory of seeds.
#[derive(Clone, Debug)]
pub struct Arm {
    pub name: String,
    pub runs: Vec<Run>,
}

/// Where the rung names came from.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RungNames {
    /// The reactions of this environment file.
    Environment(PathBuf),
    /// The column titles of `tasks.dat`.
    Columns,
}

/// A loaded batch.
#[derive(Clone, Debug)]
pub struct Batch {
    pub dir: PathBuf,
    /// Sorted by name, which fixes the colour of each arm.
    pub arms: Vec<Arm>,
    /// Lowest rung first.
    pub rungs: Vec<String>,
    pub rung_names: RungNames,
    /// Things worth knowing about the batch as a whole (not per run).
    pub notes: Vec<String>,
}

/// Which runs the summaries use.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Options {
    /// Leave out runs that have no `.done` file.
    pub finished_only: bool,
}

impl Options {
    fn takes(&self, run: &Run) -> bool {
        !self.finished_only || run.finished
    }
}

/// Per rung: how many seeds held it.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct RungCount {
    /// Seeds that held the rung at some recorded update.
    pub ever: u32,
    /// Seeds that hold it at the last recorded update.
    pub at_end: u32,
}

/// The spread of one measure over an arm's seeds.
#[derive(Clone, Debug, PartialEq)]
pub struct MeasureSummary {
    pub measure: Measure,
    /// Seeds with a value.
    pub n: usize,
    /// Seeds whose last row says undefined.
    pub undefined: usize,
    /// Seeds without the file or column.
    pub not_recorded: usize,
    /// Seeds whose file has no row yet.
    pub no_rows: usize,
    pub spread: Option<Spread>,
}

/// What an arm's runs add up to.
#[derive(Clone, Debug, PartialEq)]
pub struct ArmSummary {
    pub name: String,
    /// Runs in the arm that the options take.
    pub runs: usize,
    pub finished: usize,
    /// Runs with a ladder (the `n` of the rung counts).
    pub with_ladder: usize,
    pub rungs: Vec<RungCount>,
    /// By [`Measure::index`].
    pub measures: Vec<MeasureSummary>,
}

/// The test of one measure between two arms.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct MeasureTest {
    pub measure: Measure,
    pub test: Option<MannWhitney>,
}

/// Fisher's exact test of one rung between two arms.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct RungTest {
    /// Fisher's exact test on the table `[[a_yes, a_no], [b_yes, b_no]]`
    /// (seeds of each arm), "yes" being: held at some recorded update.
    pub p_ever: f64,
    /// The same with "yes" being: held at the last recorded update.
    pub p_end: f64,
}

/// The tests between two arms.
#[derive(Clone, Debug, PartialEq)]
pub struct Comparison {
    pub a: usize,
    pub b: usize,
    pub rungs: Vec<RungTest>,
    /// By [`Measure::index`].
    pub measures: Vec<MeasureTest>,
}

/// The median of the arm's highest-rung level at one update.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct MedianPoint {
    pub update: i32,
    pub median: f64,
    /// Runs that have a row at or after this update.
    pub n: usize,
}

impl Batch {
    pub fn runs(&self) -> impl Iterator<Item = &Run> {
        self.arms.iter().flat_map(|a| a.runs.iter())
    }

    pub fn run_count(&self) -> usize {
        self.arms.iter().map(|a| a.runs.len()).sum()
    }

    pub fn finished_count(&self) -> usize {
        self.runs().filter(|r| r.finished).count()
    }

    /// Whether any run has what `m` is read from.
    pub fn recorded(&self, m: Measure) -> bool {
        self.runs()
            .any(|r| !matches!(r.reading(m), Reading::NotRecorded))
    }

    /// Per arm: the rung counts and the spread of every measure.
    pub fn summarize(&self, opts: Options) -> Vec<ArmSummary> {
        self.arms
            .iter()
            .map(|arm| {
                let runs: Vec<&Run> = arm.runs.iter().filter(|r| opts.takes(r)).collect();
                let mut rungs = vec![RungCount::default(); self.rungs.len()];
                let mut with_ladder = 0;
                for r in runs.iter().filter(|r| r.has_ladder()) {
                    with_ladder += 1;
                    for (k, c) in rungs.iter_mut().enumerate() {
                        if r.first_held.get(k).copied().flatten().is_some() {
                            c.ever += 1;
                        }
                        if r.points
                            .last()
                            .is_some_and(|p| is_held(p.counts.get(k).copied().unwrap_or(0), p.orgs))
                        {
                            c.at_end += 1;
                        }
                    }
                }
                let measures = Measure::ALL
                    .iter()
                    .map(|&m| {
                        let mut values = Vec::new();
                        let (mut undefined, mut not_recorded, mut no_rows) = (0, 0, 0);
                        for r in &runs {
                            match r.reading(m) {
                                Reading::Value(v, _) => values.push(v),
                                Reading::Undefined(_) => undefined += 1,
                                Reading::NotRecorded => not_recorded += 1,
                                Reading::NoRows => no_rows += 1,
                            }
                        }
                        MeasureSummary {
                            measure: m,
                            n: values.len(),
                            undefined,
                            not_recorded,
                            no_rows,
                            spread: spread(&values),
                        }
                    })
                    .collect();
                ArmSummary {
                    name: arm.name.clone(),
                    runs: runs.len(),
                    finished: runs.iter().filter(|r| r.finished).count(),
                    with_ladder,
                    rungs,
                    measures,
                }
            })
            .collect()
    }

    /// The tests between arms `a` and `b` (indices into [`Batch::arms`]).
    pub fn compare(&self, a: usize, b: usize, opts: Options) -> Option<Comparison> {
        let (sa, sb) = (self.arms.get(a)?, self.arms.get(b)?);
        let summary = self.summarize(opts);
        let rungs = (0..self.rungs.len())
            .map(|k| {
                let (x, y) = (summary[a].rungs[k], summary[b].rungs[k]);
                let (na, nb) = (summary[a].with_ladder as u32, summary[b].with_ladder as u32);
                RungTest {
                    p_ever: fisher_exact(x.ever, na - x.ever, y.ever, nb - y.ever),
                    p_end: fisher_exact(x.at_end, na - x.at_end, y.at_end, nb - y.at_end),
                }
            })
            .collect();
        let values = |arm: &Arm, m: Measure| -> Vec<f64> {
            arm.runs
                .iter()
                .filter(|r| opts.takes(r))
                .filter_map(|r| r.reading(m).value())
                .collect()
        };
        let measures = Measure::ALL
            .iter()
            .map(|&m| MeasureTest {
                measure: m,
                test: mann_whitney(&values(sa, m), &values(sb, m)),
            })
            .collect();
        Some(Comparison {
            a,
            b,
            rungs,
            measures,
        })
    }

    /// The arm's median highest-rung level (0: none, 1: the lowest rung)
    /// over the updates at which any run of the batch has a row. A run
    /// counts at an update from its first row to its last, with the newest
    /// row at or before the update.
    pub fn median_series(&self, arm: usize, opts: Options) -> Vec<MedianPoint> {
        let Some(arm) = self.arms.get(arm) else {
            return Vec::new();
        };
        let runs: Vec<&Run> = arm
            .runs
            .iter()
            .filter(|r| opts.takes(r) && r.has_ladder())
            .collect();
        let mut updates: Vec<i32> = runs
            .iter()
            .flat_map(|r| r.levels.iter().map(|l| l.0))
            .collect();
        updates.sort_unstable();
        updates.dedup();
        updates
            .into_iter()
            .filter_map(|u| {
                let mut v: Vec<f64> = runs
                    .iter()
                    .filter_map(|r| r.level_at(u))
                    .map(f64::from)
                    .collect();
                let n = v.len();
                v.sort_by(f64::total_cmp);
                stats::quantile(&v, 0.5).map(|median| MedianPoint {
                    update: u,
                    median,
                    n,
                })
            })
            .collect()
    }
}

/// An arm folder found on disk: its name, its path and its seed folders
/// (seed label and path).
type ArmDir = (String, PathBuf, Vec<(String, PathBuf)>);

/// `seed-12` gives "12"; anything else is not a seed directory.
fn seed_of(name: &str) -> Option<&str> {
    let lower = name.to_ascii_lowercase();
    lower
        .starts_with("seed-")
        .then(|| &name["seed-".len()..])
        .filter(|s| !s.is_empty())
}

fn sorted_subdirs(dir: &Path) -> Result<Vec<(String, PathBuf)>, String> {
    let mut out = Vec::new();
    let entries =
        std::fs::read_dir(dir).map_err(|e| format!("{} cannot be read: {e}", dir.display()))?;
    for e in entries.flatten() {
        let name = e.file_name().to_string_lossy().to_string();
        if name.starts_with('.') {
            continue;
        }
        let path = e.path();
        if path.is_dir() {
            out.push((name, path));
        }
    }
    out.sort();
    Ok(out)
}

/// Seeds sort by number when they are numbers, by text otherwise.
fn seed_order(a: &str, b: &str) -> std::cmp::Ordering {
    match (a.parse::<u64>(), b.parse::<u64>()) {
        (Ok(x), Ok(y)) => x.cmp(&y),
        (Ok(_), Err(_)) => std::cmp::Ordering::Less,
        (Err(_), Ok(_)) => std::cmp::Ordering::Greater,
        _ => a.cmp(b),
    }
}

/// Loads the batch in `dir`. An error only when there is no arm to show:
/// a directory that cannot be read, or one without `<arm>/seed-<N>/`.
pub fn load_batch(dir: &Path) -> Result<Batch, String> {
    let mut arm_dirs: Vec<ArmDir> = Vec::new();
    for (name, path) in sorted_subdirs(dir)? {
        let mut seeds: Vec<(String, PathBuf)> = sorted_subdirs(&path)?
            .into_iter()
            .filter_map(|(n, p)| seed_of(&n).map(|s| (s.to_string(), p)))
            .collect();
        if !seeds.is_empty() {
            seeds.sort_by(|a, b| seed_order(&a.0, &b.0));
            arm_dirs.push((name, path, seeds));
        }
    }
    let mut notes = Vec::new();
    if arm_dirs.is_empty() {
        // The folder may be an arm itself: a single arm named after it.
        let mut seeds: Vec<(String, PathBuf)> = sorted_subdirs(dir)?
            .into_iter()
            .filter_map(|(n, p)| seed_of(&n).map(|s| (s.to_string(), p)))
            .collect();
        if seeds.is_empty() {
            return Err(format!(
                "{} has no arm folders: expected <arm>/seed-<N>/ inside it.",
                dir.display()
            ));
        }
        seeds.sort_by(|a, b| seed_order(&a.0, &b.0));
        let name = dir
            .file_name()
            .map_or_else(|| "arm".to_string(), |n| n.to_string_lossy().to_string());
        notes.push(format!(
            "{} holds seed folders directly, so it is shown as one arm named {name}.",
            dir.display()
        ));
        arm_dirs.push((name, dir.to_path_buf(), seeds));
    }

    let mut loaded: Vec<(String, Vec<LoadedRun>)> = Vec::new();
    for (name, _, seeds) in &arm_dirs {
        let runs = seeds
            .iter()
            .map(|(seed, path)| load_run(name, seed, path))
            .collect();
        loaded.push((name.clone(), runs));
    }

    // The ladder's size: the commonest task count (the larger on a tie).
    let mut sizes: HashMap<usize, usize> = HashMap::new();
    for r in loaded.iter().flat_map(|(_, r)| r.iter()) {
        if r.tasks.is_some() {
            *sizes.entry(r.rung_count).or_default() += 1;
        }
    }
    let n_rungs = sizes
        .iter()
        .max_by_key(|&(&size, &count)| (count, size))
        .map_or(0, |(&size, _)| size);

    let (rungs, rung_names, name_notes) = rung_names(dir, &arm_dirs, &loaded, n_rungs);
    notes.extend(name_notes);

    let arms = loaded
        .into_iter()
        .map(|(name, runs)| Arm {
            name,
            runs: runs.into_iter().map(|r| r.finish(n_rungs)).collect(),
        })
        .collect();
    Ok(Batch {
        dir: dir.to_path_buf(),
        arms,
        rungs,
        rung_names,
        notes,
    })
}

/// A run read from disk, before it is fitted to the batch's ladder.
struct LoadedRun {
    run: Run,
    /// The titles of the task columns.
    task_titles: Vec<String>,
    /// Update, population and task counts of the rows `tasks.dat` and
    /// `count.dat` share.
    tasks: Option<Vec<LadderPoint>>,
    rung_count: usize,
}

impl LoadedRun {
    fn finish(self, n_rungs: usize) -> Run {
        let mut run = self.run;
        let Some(points) = self.tasks else {
            return run;
        };
        if self.rung_count != n_rungs {
            run.problems.push(format!(
                "tasks.dat has {} task columns where the batch's ladder has {n_rungs}; the ladder of this run is not used",
                self.rung_count
            ));
            run.rung_count = self.rung_count;
            return run;
        }
        run.rung_count = n_rungs;
        run.first_reached = vec![None; n_rungs];
        run.first_held = vec![None; n_rungs];
        for p in &points {
            let mut level = 0_u32;
            for k in 0..n_rungs {
                let c = p.counts.get(k).copied().unwrap_or(0);
                if c > 0 && run.first_reached[k].is_none() {
                    run.first_reached[k] = Some(p.update);
                }
                if is_held(c, p.orgs) {
                    if run.first_held[k].is_none() {
                        run.first_held[k] = Some(p.update);
                    }
                    level = k as u32 + 1;
                }
            }
            run.levels.push((p.update, level));
        }
        run.last_update = points.last().map(|p| p.update);
        run.end_rung = run
            .levels
            .last()
            .and_then(|l| (l.1 as usize).checked_sub(1));
        run.ever_rung = run
            .levels
            .iter()
            .map(|l| l.1)
            .max()
            .and_then(|l| (l as usize).checked_sub(1));
        let end = run.levels.last().map_or(0, |l| l.1);
        let ever = run.levels.iter().map(|l| l.1).max().unwrap_or(0);
        let last = run.last_update.unwrap_or(0);
        run.readings[Measure::RungEnd.index()] = Reading::Value(f64::from(end), last);
        run.readings[Measure::RungEver.index()] = Reading::Value(f64::from(ever), last);
        run.points = points;
        run
    }
}

fn read_text(path: &Path) -> Option<Result<String, String>> {
    match std::fs::read(path) {
        Ok(bytes) => Some(Ok(String::from_utf8_lossy(&bytes).into_owned())),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => None,
        Err(e) => Some(Err(e.to_string())),
    }
}

/// A data file of the run: `None` when it is not there, `Some(Err)` with
/// the one-line reason when it cannot be read.
fn read_dat(dir: &Path, name: &str) -> Option<Result<dat::Dat, String>> {
    let text = read_text(&dir.join(name))?;
    Some(
        text.map_err(|e| format!("{name} cannot be read: {e}"))
            .and_then(|t| dat::parse(&t).map_err(|e| format!("{name}: {e}"))),
    )
}

/// The first `modes*.dat` by name.
fn modes_file(dir: &Path) -> Option<String> {
    let mut names: Vec<String> = std::fs::read_dir(dir)
        .ok()?
        .flatten()
        .map(|e| e.file_name().to_string_lossy().to_string())
        .filter(|n| n.starts_with("modes") && n.ends_with(".dat"))
        .collect();
    names.sort();
    names.into_iter().next()
}

/// The last row of column `col` as a reading: negative is "undefined".
fn last_reading(
    d: &dat::Dat,
    file: &str,
    col: usize,
    update_col: Option<usize>,
) -> Result<Reading, String> {
    let Some((line, cells)) = d.rows.last() else {
        return Ok(Reading::NoRows);
    };
    let value = cells.get(col).copied().flatten().ok_or_else(|| {
        format!(
            "{file} line {line}: the value in column {} is not a number",
            col + 1
        )
    })?;
    let update = update_col
        .and_then(|c| cells.get(c).copied().flatten())
        .map_or(0, |u| u as i32);
    Ok(if value.is_nan() || value < 0.0 {
        Reading::Undefined(update)
    } else {
        Reading::Value(value, update)
    })
}

fn load_run(arm: &str, seed: &str, dir: &Path) -> LoadedRun {
    let mut run = Run::empty(arm, seed, dir);
    let mut out = LoadedRun {
        run: Run::empty(arm, seed, dir),
        task_titles: Vec::new(),
        tasks: None,
        rung_count: 0,
    };

    // The ladder: tasks.dat and count.dat.
    match (read_dat(dir, "tasks.dat"), read_dat(dir, "count.dat")) {
        (None, _) => run.problems.push("tasks.dat is not there yet".to_string()),
        (Some(Err(e)), _) => run.problems.push(e),
        (Some(Ok(_)), None) => run.problems.push(
            "count.dat is not there, so the population is unknown and no rung can be judged held"
                .to_string(),
        ),
        (Some(Ok(_)), Some(Err(e))) => run.problems.push(e),
        (Some(Ok(tasks)), Some(Ok(count))) => match ladder_points(&tasks, &count) {
            Ok((titles, points)) => {
                out.rung_count = titles.len();
                out.task_titles = titles;
                if points.is_empty() {
                    run.problems
                        .push("tasks.dat and count.dat have no update in common yet".to_string());
                } else {
                    out.tasks = Some(points);
                }
            }
            Err(e) => run.problems.push(e),
        },
    }

    // Reuse measures.
    let mut genome_from_reuse = Reading::NotRecorded;
    match read_dat(dir, "reuse.dat") {
        None => {}
        Some(Err(e)) => run.problems.push(e),
        Some(Ok(d)) => {
            let upd = d.find("update");
            let mut set = |m: Measure, prefix: &str, run: &mut Run| match d
                .need("reuse.dat", prefix)
                .and_then(|c| last_reading(&d, "reuse.dat", c, upd))
            {
                Ok(r) => {
                    if m == Measure::GenomeLength {
                        genome_from_reuse = r;
                    } else {
                        run.readings[m.index()] = r;
                    }
                }
                Err(e) => run.problems.push(e),
            };
            set(Measure::ReuseIndex, "reuse index", &mut run);
            set(Measure::SharedSites, "shared sites", &mut run);
            set(Measure::TasksPerformed, "tasks performed", &mut run);
            set(Measure::GenomeLength, "genome length", &mut run);
        }
    }
    // Genome length: dominant.dat, else reuse.dat.
    match read_dat(dir, "dominant.dat") {
        Some(Ok(d)) => match d
            .need("dominant.dat", "size of dominant")
            .and_then(|c| last_reading(&d, "dominant.dat", c, d.find("update")))
        {
            Ok(r) => run.readings[Measure::GenomeLength.index()] = r,
            Err(e) => {
                run.problems.push(e);
                run.readings[Measure::GenomeLength.index()] = genome_from_reuse;
            }
        },
        Some(Err(e)) => {
            run.problems.push(e);
            run.readings[Measure::GenomeLength.index()] = genome_from_reuse;
        }
        None => run.readings[Measure::GenomeLength.index()] = genome_from_reuse,
    }

    // MODES complexity.
    if let Some(name) = modes_file(dir) {
        match read_dat(dir, &name) {
            Some(Ok(d)) => match d
                .need(&name, "complexity")
                .and_then(|c| last_reading(&d, &name, c, d.find("update")))
            {
                Ok(r) => run.readings[Measure::Complexity.index()] = r,
                Err(e) => run.problems.push(e),
            },
            Some(Err(e)) => run.problems.push(e),
            None => {}
        }
    }
    out.run = run;
    out
}

/// The rows `tasks.dat` and `count.dat` share, matched on the update.
fn ladder_points(
    tasks: &dat::Dat,
    count: &dat::Dat,
) -> Result<(Vec<String>, Vec<LadderPoint>), String> {
    let t_update = tasks.need("tasks.dat", "update")?;
    let c_update = count.need("count.dat", "update")?;
    let c_orgs = count.need("count.dat", "number of organisms")?;
    let titles: Vec<(usize, String)> = tasks
        .columns
        .iter()
        .enumerate()
        .filter(|&(i, _)| i != t_update)
        .map(|(i, t)| (i, t.clone()))
        .collect();
    if titles.is_empty() {
        return Err("tasks.dat has no task columns".to_string());
    }
    let updates = tasks.numbers("tasks.dat", t_update)?;
    let cols: Vec<Vec<f64>> = titles
        .iter()
        .map(|(i, _)| tasks.numbers("tasks.dat", *i))
        .collect::<Result<_, _>>()?;
    let c_updates = count.numbers("count.dat", c_update)?;
    let c_orgs = count.numbers("count.dat", c_orgs)?;
    let orgs: HashMap<i64, f64> = c_updates
        .iter()
        .zip(&c_orgs)
        .map(|(&u, &o)| (u as i64, o))
        .collect();
    let mut points = Vec::new();
    for (row, &u) in updates.iter().enumerate() {
        let Some(&o) = orgs.get(&(u as i64)) else {
            continue;
        };
        points.push(LadderPoint {
            update: u as i32,
            orgs: o as i32,
            counts: cols.iter().map(|c| c[row] as i32).collect(),
        });
    }
    Ok((titles.into_iter().map(|(_, t)| t).collect(), points))
}

/// `REACTION` names, in order, of environment-file lines (the environment
/// of an experiment file as [`avida_config::experiment::environment_of`]
/// gives it, or an environment file of the old format).
pub fn reaction_names(text: &str) -> Vec<String> {
    text.lines()
        .filter_map(|l| {
            let l = l.trim();
            let mut it = l.split_whitespace();
            (it.next() == Some("REACTION")).then(|| it.next().map(str::to_string))?
        })
        .collect()
}

fn rung_names(
    batch: &Path,
    arms: &[ArmDir],
    loaded: &[(String, Vec<LoadedRun>)],
    n: usize,
) -> (Vec<String>, RungNames, Vec<String>) {
    let mut notes = Vec::new();
    let mut dirs: Vec<PathBuf> = vec![batch.to_path_buf()];
    for (name, path, seeds) in arms {
        dirs.push(path.clone());
        for (_, s) in seeds {
            dirs.push(s.clone());
        }
        if let Some(parent) = batch.parent() {
            dirs.push(parent.join(format!("config-{name}")));
        }
    }
    // The experiment file of a folder, or the environment file of a batch
    // run before the experiment file replaced it (its names are only read).
    let candidates = dirs.iter().flat_map(|d| {
        [
            d.join(avida_config::experiment::DEFAULT_FILE),
            d.join("environment.cfg"),
        ]
    });
    for path in candidates {
        let text = if path.extension().is_some_and(|e| e == "yaml") {
            if !path.is_file() {
                continue;
            }
            match avida_config::experiment::environment_of(&path) {
                Ok(lines) => lines.join("\n"),
                Err(_) => continue,
            }
        } else {
            let Some(Ok(text)) = read_text(&path) else {
                continue;
            };
            text
        };
        let names = reaction_names(&text);
        if names.len() == n {
            return (names, RungNames::Environment(path), notes);
        }
        notes.push(format!(
            "{} lists {} reactions but the runs have {n} task columns; its names are not used.",
            path.display(),
            names.len()
        ));
    }
    // The column titles of the first run that has them.
    let titles = loaded
        .iter()
        .flat_map(|(_, runs)| runs.iter())
        .find(|r| r.rung_count == n && n > 0)
        .map(|r| r.task_titles.clone())
        .unwrap_or_default();
    let names = titles
        .iter()
        .enumerate()
        .map(|(i, t)| {
            if titles.iter().filter(|x| *x == t).count() > 1 {
                format!("{t} #{}", i + 1)
            } else {
                t.clone()
            }
        })
        .collect();
    (names, RungNames::Columns, notes)
}
