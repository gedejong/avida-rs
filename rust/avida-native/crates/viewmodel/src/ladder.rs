//! The ladder view: the environment's reactions in file order as rungs, and
//! for each rung how many organisms held it over time.
//!
//! An environment file lists its reactions in order; in a ladder experiment
//! that order is a series of increasing difficulty (NOT, NAND, XOR, ...).
//! [`LadderHistory`] folds the per-update statistics rows
//! ([`UpdateRow::task_last_count`]) into a bounded series, and remembers for
//! every task the first update at which any organism performed it and the
//! first at which at least 1 % of the organisms did, exactly: those two
//! numbers are taken from every row pushed, so thinning the series later
//! cannot lose them. [`build`] reads the history and the environment into a
//! [`LadderView`], which both front ends draw.
//!
//! The counts are per task, not per reaction: the statistics rows (and so a
//! recording) carry `GetTaskLastCount(k)` and the map carries one flag per
//! task. A rung is a reaction, named by the reaction, and takes the count of
//! the task that triggers it; two reactions on one task are two rungs with
//! the same series (each lists the other in [`Rung::shares_task_with`]). A
//! reaction without a task has no count and is not a rung.
//!
//! The history is fed by the front ends from the rows they already receive,
//! so a playback (one row per recorded frame) fills it the same way a live
//! run does, with the recorded interval as its resolution.

use crate::model::{EnvView, UpdateRow};

/// Points kept before the series is thinned.
pub const HISTORY_CAP: usize = 2_000;

/// A rung is "held" by this share of the organisms (percent) or more.
pub const HELD_PERCENT: i64 = 1;

/// One update of the series: the population and, per task, the organisms
/// that performed it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LadderPoint {
    pub update: i32,
    pub orgs: i32,
    pub counts: Vec<i32>,
}

/// Whether `count` of `orgs` organisms hold a rung: at least one organism
/// and at least 1 % of the population.
pub fn is_held(count: i32, orgs: i32) -> bool {
    count > 0 && i64::from(count) * 100 >= HELD_PERCENT * i64::from(orgs.max(0))
}

/// The per-task counts over time, bounded, with the exact first update of
/// each task being performed and being held.
#[derive(Clone, Debug)]
pub struct LadderHistory {
    points: Vec<LadderPoint>,
    /// The newest point when it is not one the thinning keeps for good: the
    /// next point replaces it.
    tail: Option<LadderPoint>,
    capacity: usize,
    stride: usize,
    since_kept: usize,
    first_reached: Vec<Option<i32>>,
    first_held: Vec<Option<i32>>,
    version: u64,
}

impl Default for LadderHistory {
    fn default() -> Self {
        LadderHistory::with_capacity(HISTORY_CAP)
    }
}

impl LadderHistory {
    pub fn with_capacity(capacity: usize) -> LadderHistory {
        LadderHistory {
            points: Vec::new(),
            tail: None,
            capacity: capacity.max(4),
            stride: 1,
            since_kept: 0,
            first_reached: Vec::new(),
            first_held: Vec::new(),
            version: 0,
        }
    }

    /// Forgets everything (a restart, or a playback that went back).
    pub fn clear(&mut self) {
        let version = self.version + 1;
        *self = LadderHistory::with_capacity(self.capacity);
        self.version = version;
    }

    /// Folds in rows in update order.
    pub fn extend<'a>(&mut self, rows: impl IntoIterator<Item = &'a UpdateRow>) {
        for row in rows {
            self.push(row);
        }
    }

    pub fn push(&mut self, row: &UpdateRow) {
        self.version += 1;
        let n = row.task_last_count.len();
        if self.first_reached.len() < n {
            self.first_reached.resize(n, None);
            self.first_held.resize(n, None);
        }
        for (k, &c) in row.task_last_count.iter().enumerate() {
            if c > 0 && self.first_reached[k].is_none() {
                self.first_reached[k] = Some(row.update);
            }
            if is_held(c, row.orgs) && self.first_held[k].is_none() {
                self.first_held[k] = Some(row.update);
            }
        }
        let point = LadderPoint {
            update: row.update,
            orgs: row.orgs,
            counts: row.task_last_count.clone(),
        };
        self.tail = None;
        self.since_kept += 1;
        if self.since_kept >= self.stride {
            self.since_kept = 0;
            self.points.push(point);
            if self.points.len() > self.capacity {
                self.thin();
            }
        } else {
            self.tail = Some(point);
        }
    }

    /// Keeps the even positions (and the last) and doubles the stride.
    fn thin(&mut self) {
        let last = self.points.len() - 1;
        let mut i = 0;
        self.points.retain(|_| {
            let keep = i % 2 == 0 || i == last;
            i += 1;
            keep
        });
        self.stride *= 2;
    }

    /// The points, oldest first; the last is the newest row seen.
    pub fn points(&self) -> impl Iterator<Item = &LadderPoint> {
        self.points.iter().chain(self.tail.iter())
    }

    pub fn len(&self) -> usize {
        self.points.len() + usize::from(self.tail.is_some())
    }

    pub fn is_empty(&self) -> bool {
        self.points.is_empty() && self.tail.is_none()
    }

    /// One row in this many is kept (1 until the first thinning).
    pub fn stride(&self) -> usize {
        self.stride
    }

    /// The first update at which any organism performed task `k`.
    pub fn first_reached(&self, k: usize) -> Option<i32> {
        self.first_reached.get(k).copied().flatten()
    }

    /// The first update at which at least 1 % of the organisms did.
    pub fn first_held(&self, k: usize) -> Option<i32> {
        self.first_held.get(k).copied().flatten()
    }

    /// Changes with every row pushed (for caches).
    pub fn version(&self) -> u64 {
        self.version
    }
}

/// What a rung is like now.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RungState {
    /// No organism has performed it in the history.
    NeverReached,
    /// Performed before, by no organism now.
    Lost,
    /// Performed by at least one organism now.
    Present,
}

/// One rung: a reaction and its series.
#[derive(Clone, Debug, PartialEq)]
pub struct Rung {
    /// The reaction's name.
    pub name: String,
    /// The task that triggers it.
    pub task: usize,
    /// The other reactions on the same task (their counts are this rung's).
    pub shares_task_with: Vec<String>,
    /// Organisms that performed it, one per entry of [`LadderView::updates`].
    pub counts: Vec<i32>,
    /// The newest count and the population it is a share of.
    pub now: i32,
    pub state: RungState,
    pub first_reached: Option<i32>,
    pub first_held: Option<i32>,
    /// Among the organisms the filter selects, now: how many perform it.
    pub subset: Option<u32>,
}

impl Rung {
    /// Share of the population now, in percent.
    pub fn percent(&self, orgs: i32) -> f64 {
        if orgs > 0 {
            100.0 * f64::from(self.now) / f64::from(orgs)
        } else {
            0.0
        }
    }

    /// Whether at least 1 % of the organisms hold it now.
    pub fn held_now(&self, orgs: i32) -> bool {
        is_held(self.now, orgs)
    }
}

/// What a front end draws.
#[derive(Clone, Debug, PartialEq)]
pub struct LadderView {
    /// Rungs in the environment file's order: index 0 is the bottom rung.
    pub rungs: Vec<Rung>,
    /// The updates of the series, oldest first.
    pub updates: Vec<i32>,
    /// The population at each of them.
    pub orgs: Vec<i32>,
    /// The population now.
    pub orgs_now: i32,
    /// The highest rung (index into `rungs`) held by at least 1 % now.
    pub highest_held: Option<usize>,
    /// The highest rung any organism has performed.
    pub highest_reached: Option<usize>,
    /// Organisms the filter selects, when one is set.
    pub subset_orgs: Option<u32>,
    /// The most organisms at any point of the series (the scale of the
    /// strips is the population, so this is the largest population).
    pub max_orgs: i32,
}

impl LadderView {
    pub fn is_empty(&self) -> bool {
        self.rungs.is_empty()
    }

    /// The index of the point of the series nearest to `update`.
    pub fn nearest(&self, update: f64) -> Option<usize> {
        if self.updates.is_empty() {
            return None;
        }
        let i = self.updates.partition_point(|&u| f64::from(u) < update);
        let before = i.saturating_sub(1);
        let after = i.min(self.updates.len() - 1);
        let d = |j: usize| (f64::from(self.updates[j]) - update).abs();
        Some(if d(before) <= d(after) { before } else { after })
    }
}

/// The subset's task counts for the newest frame: per task, and the number
/// of organisms the filter selects.
pub struct SubsetCounts<'a> {
    pub per_task: &'a [u32],
    pub matched: u32,
}

/// Builds the view of `history` for the environment's reactions. `subset` is
/// the filter's selection in the newest frame, if one is set.
pub fn build(
    history: &LadderHistory,
    env: &EnvView,
    subset: Option<SubsetCounts<'_>>,
) -> LadderView {
    let points: Vec<&LadderPoint> = history.points().collect();
    let updates: Vec<i32> = points.iter().map(|p| p.update).collect();
    let orgs: Vec<i32> = points.iter().map(|p| p.orgs).collect();
    let orgs_now = orgs.last().copied().unwrap_or(0);
    let mut rungs: Vec<Rung> = Vec::new();
    for (i, r) in env.reactions.iter().enumerate() {
        let Some(task) = r.task else { continue };
        let counts: Vec<i32> = points
            .iter()
            .map(|p| p.counts.get(task).copied().unwrap_or(0).max(0))
            .collect();
        let now = counts.last().copied().unwrap_or(0);
        let first_reached = history.first_reached(task);
        let state = if now > 0 {
            RungState::Present
        } else if first_reached.is_some() {
            RungState::Lost
        } else {
            RungState::NeverReached
        };
        let shares_task_with = env
            .reactions
            .iter()
            .enumerate()
            .filter(|&(j, o)| j != i && o.task == Some(task))
            .map(|(_, o)| o.name.clone())
            .collect();
        rungs.push(Rung {
            name: r.name.clone(),
            task,
            shares_task_with,
            counts,
            now,
            state,
            first_reached,
            first_held: history.first_held(task),
            subset: subset
                .as_ref()
                .map(|s| s.per_task.get(task).copied().unwrap_or(0)),
        });
    }
    let highest_held = rungs.iter().rposition(|r| r.held_now(orgs_now));
    let highest_reached = rungs.iter().rposition(|r| r.first_reached.is_some());
    LadderView {
        rungs,
        updates,
        orgs_now,
        max_orgs: orgs.iter().copied().max().unwrap_or(0),
        orgs,
        highest_held,
        highest_reached,
        subset_orgs: subset.map(|s| s.matched),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{ReactionView, TaskView};

    fn row(update: i32, orgs: i32, counts: &[i32]) -> UpdateRow {
        UpdateRow {
            update,
            orgs,
            task_last_count: counts.to_vec(),
            ..UpdateRow::default()
        }
    }

    fn env(reactions: &[(&str, Option<usize>)], tasks: usize) -> EnvView {
        EnvView {
            reactions: reactions
                .iter()
                .map(|&(n, t)| ReactionView {
                    name: n.to_string(),
                    task: t,
                    value: 1.0,
                    last_count: 0,
                })
                .collect(),
            tasks: (0..tasks)
                .map(|k| TaskView {
                    name: format!("t{k}"),
                    desc: String::new(),
                    last_count: 0,
                })
                .collect(),
            ..EnvView::default()
        }
    }

    #[test]
    fn first_reached_and_first_held_are_the_first_rows_that_qualify() {
        let mut h = LadderHistory::default();
        h.extend(&[
            row(0, 1000, &[0, 0]),
            row(1, 1000, &[3, 0]),
            // 5 of 1000 is a reached rung but not a held one.
            row(2, 1000, &[5, 1]),
            row(3, 1000, &[10, 0]),
            row(4, 1000, &[200, 0]),
        ]);
        assert_eq!(h.first_reached(0), Some(1));
        assert_eq!(h.first_held(0), Some(3), "10 of 1000 is exactly 1 %");
        assert_eq!(h.first_reached(1), Some(2));
        assert_eq!(h.first_held(1), None);
        assert_eq!(h.first_reached(7), None);
    }

    #[test]
    fn one_organism_of_a_tiny_population_holds_a_rung() {
        assert!(is_held(1, 50));
        assert!(!is_held(0, 50));
        assert!(!is_held(1, 101));
        assert!(is_held(1, 100));
    }

    #[test]
    fn thinning_bounds_the_series_but_not_the_first_updates() {
        let mut h = LadderHistory::with_capacity(16);
        for u in 0..1000 {
            // The task is first performed at update 3, held from 5.
            let c = match u {
                0..=2 => 0,
                3..=4 => 1,
                _ => 50,
            };
            h.push(&row(u, 1000, &[c]));
            assert!(h.len() <= 17, "len {} at {u}", h.len());
            assert_eq!(h.points().last().map(|p| p.update), Some(u));
        }
        let updates: Vec<i32> = h.points().map(|p| p.update).collect();
        assert_eq!(updates[0], 0);
        assert!(updates.windows(2).all(|w| w[0] < w[1]));
        assert!(h.stride() >= 64);
        let max_gap = updates.windows(2).map(|w| w[1] - w[0]).max().unwrap();
        assert!(max_gap as usize <= 2 * h.stride(), "gap {max_gap}");
        // The exact first updates survive although update 3 and 5 were
        // thinned out of the series long ago.
        assert_eq!(h.first_reached(0), Some(3));
        assert_eq!(h.first_held(0), Some(5));
        assert!(!updates.contains(&3));
    }

    #[test]
    fn clearing_forgets_the_rows_and_the_first_updates() {
        let mut h = LadderHistory::default();
        h.push(&row(5, 10, &[4]));
        let v = h.version();
        h.clear();
        assert!(h.is_empty());
        assert_eq!(h.first_reached(0), None);
        assert!(h.version() > v);
    }

    #[test]
    fn rungs_follow_the_reactions_and_skip_those_without_a_task() {
        let mut h = LadderHistory::default();
        h.extend(&[row(0, 100, &[0, 0, 0]), row(10, 100, &[40, 2, 0])]);
        let e = env(
            &[
                ("NOT", Some(0)),
                ("FREE", None),
                ("NAND", Some(1)),
                ("XOR", Some(2)),
            ],
            3,
        );
        let v = build(&h, &e, None);
        let names: Vec<&str> = v.rungs.iter().map(|r| r.name.as_str()).collect();
        assert_eq!(names, ["NOT", "NAND", "XOR"]);
        assert_eq!(v.rungs[0].now, 40);
        assert_eq!(v.rungs[0].state, RungState::Present);
        assert_eq!(v.rungs[2].state, RungState::NeverReached);
        assert_eq!(v.rungs[1].first_reached, Some(10));
        assert_eq!(v.rungs[1].first_held, Some(10), "2 of 100 is held");
        assert_eq!(v.highest_held, Some(1));
        assert_eq!(v.highest_reached, Some(1));
        assert_eq!(v.updates, [0, 10]);
        assert_eq!(v.orgs_now, 100);
        assert!((v.rungs[0].percent(v.orgs_now) - 40.0).abs() < 1e-9);
    }

    #[test]
    fn a_lost_rung_is_told_from_one_never_reached() {
        let mut h = LadderHistory::default();
        h.extend(&[
            row(0, 100, &[0, 0]),
            row(1, 100, &[9, 0]),
            row(2, 100, &[0, 0]),
        ]);
        let v = build(&h, &env(&[("A", Some(0)), ("B", Some(1))], 2), None);
        assert_eq!(v.rungs[0].state, RungState::Lost);
        assert_eq!(v.rungs[1].state, RungState::NeverReached);
        assert_eq!(v.highest_reached, Some(0));
        assert_eq!(v.highest_held, None);
    }

    #[test]
    fn reactions_on_one_task_share_the_series_and_name_each_other() {
        let mut h = LadderHistory::default();
        h.push(&row(0, 10, &[3]));
        let v = build(&h, &env(&[("A", Some(0)), ("B", Some(0))], 1), None);
        assert_eq!(v.rungs[0].counts, v.rungs[1].counts);
        assert_eq!(v.rungs[0].shares_task_with, ["B"]);
        assert_eq!(v.rungs[1].shares_task_with, ["A"]);
    }

    #[test]
    fn the_subset_is_carried_per_rung() {
        let mut h = LadderHistory::default();
        h.push(&row(0, 10, &[3, 1]));
        let sub = SubsetCounts {
            per_task: &[2, 0],
            matched: 4,
        };
        let v = build(&h, &env(&[("A", Some(0)), ("B", Some(1))], 2), Some(sub));
        assert_eq!(v.rungs[0].subset, Some(2));
        assert_eq!(v.rungs[1].subset, Some(0));
        assert_eq!(v.subset_orgs, Some(4));
    }

    #[test]
    fn a_playback_derives_the_same_view_from_rows_one_per_frame() {
        // What a recording holds: one row per frame, here every 50 updates.
        let rows: Vec<UpdateRow> = (0..20)
            .map(|i| {
                row(
                    i * 50,
                    100,
                    &[if i >= 4 { 30 } else { 0 }, i32::from(i >= 12) * 5],
                )
            })
            .collect();
        let mut h = LadderHistory::default();
        h.extend(&rows);
        let v = build(&h, &env(&[("A", Some(0)), ("B", Some(1))], 2), None);
        // The resolution is the frame interval: the first update seen with
        // the rung is the frame's, not the update in between.
        assert_eq!(v.rungs[0].first_reached, Some(200));
        assert_eq!(v.rungs[1].first_reached, Some(600));
        assert_eq!(v.updates.len(), 20);
    }

    #[test]
    fn nearest_picks_the_closest_update() {
        let mut h = LadderHistory::default();
        h.extend(&[row(0, 1, &[0]), row(10, 1, &[0]), row(20, 1, &[0])]);
        let v = build(&h, &env(&[("A", Some(0))], 1), None);
        assert_eq!(v.nearest(4.0), Some(0));
        assert_eq!(v.nearest(6.0), Some(1));
        assert_eq!(v.nearest(99.0), Some(2));
        assert_eq!(
            build(&LadderHistory::default(), &EnvView::default(), None).nearest(1.0),
            None
        );
    }
}
