//! The plottable columns of an [`UpdateRow`] and a cache of their
//! decimated series.

use std::collections::HashMap;

use avida_session::UpdateRow;
use avida_viewmodel::{decimate_min_max, series_of};

use crate::model::history::RowHistory;

/// A statistic recorded once per update.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub enum Metric {
    Organisms,
    Genotypes,
    ThresholdGenotypes,
    AveFitness,
    MaxFitness,
    AveMerit,
    MaxMerit,
    AveGestation,
    AveGeneration,
    AveAge,
    AveGenomeSize,
    Births,
    Deaths,
    BreedTrue,
    Executed,
    Threads,
    TotalOrganisms,
}

impl Metric {
    pub const ALL: [Metric; 17] = [
        Metric::Organisms,
        Metric::Genotypes,
        Metric::ThresholdGenotypes,
        Metric::AveFitness,
        Metric::MaxFitness,
        Metric::AveMerit,
        Metric::MaxMerit,
        Metric::AveGestation,
        Metric::AveGeneration,
        Metric::AveAge,
        Metric::AveGenomeSize,
        Metric::Births,
        Metric::Deaths,
        Metric::BreedTrue,
        Metric::Executed,
        Metric::Threads,
        Metric::TotalOrganisms,
    ];

    /// The charts a new user sees.
    pub const DEFAULT_CHARTS: [Metric; 3] =
        [Metric::AveFitness, Metric::Organisms, Metric::Genotypes];

    pub fn label(self) -> &'static str {
        match self {
            Metric::Organisms => "Organisms",
            Metric::Genotypes => "Genotypes",
            Metric::ThresholdGenotypes => "Threshold genotypes",
            Metric::AveFitness => "Average fitness",
            Metric::MaxFitness => "Maximum fitness",
            Metric::AveMerit => "Average merit",
            Metric::MaxMerit => "Maximum merit",
            Metric::AveGestation => "Average gestation",
            Metric::AveGeneration => "Average generation",
            Metric::AveAge => "Average age",
            Metric::AveGenomeSize => "Average genome size",
            Metric::Births => "Births",
            Metric::Deaths => "Deaths",
            Metric::BreedTrue => "Breed-true births",
            Metric::Executed => "Instructions executed",
            Metric::Threads => "Threads",
            Metric::TotalOrganisms => "Organisms ever born",
        }
    }

    /// A short label for chips and cards.
    pub fn short(self) -> &'static str {
        match self {
            Metric::Organisms => "Organisms",
            Metric::Genotypes => "Genotypes",
            Metric::ThresholdGenotypes => "Established",
            Metric::AveFitness => "Avg fitness",
            Metric::MaxFitness => "Max fitness",
            Metric::AveMerit => "Avg merit",
            Metric::MaxMerit => "Max merit",
            Metric::AveGestation => "Avg gestation",
            Metric::AveGeneration => "Avg generation",
            Metric::AveAge => "Avg age",
            Metric::AveGenomeSize => "Avg genome",
            Metric::Births => "Births",
            Metric::Deaths => "Deaths",
            Metric::BreedTrue => "Breed true",
            Metric::Executed => "Executed",
            Metric::Threads => "Threads",
            Metric::TotalOrganisms => "Ever born",
        }
    }

    /// What the number means (tooltips).
    pub fn description(self) -> &'static str {
        match self {
            Metric::Organisms => "Living organisms at the end of the update.",
            Metric::Genotypes => "Distinct genotypes alive.",
            Metric::ThresholdGenotypes => {
                "Genotypes that have reached the THRESHOLD abundance and count as established."
            }
            Metric::AveFitness => "Mean fitness (merit / gestation time) of living organisms.",
            Metric::MaxFitness => "Highest fitness of any living organism.",
            Metric::AveMerit => {
                "Mean merit: the share of CPU time an organism gets, raised by rewarded tasks."
            }
            Metric::MaxMerit => "Highest merit of any living organism.",
            Metric::AveGestation => "Mean CPU cycles an organism needs to produce an offspring.",
            Metric::AveGeneration => {
                "Mean number of ancestors between the founder and the living organisms."
            }
            Metric::AveAge => "Mean age of living organisms, in updates.",
            Metric::AveGenomeSize => "Mean memory size (genome length while not copying).",
            Metric::Births => "Offspring placed during the update.",
            Metric::Deaths => "Organisms that died during the update.",
            Metric::BreedTrue => "Offspring identical to their parent during the update.",
            Metric::Executed => "Instructions executed by all organisms during the update.",
            Metric::Threads => "CPU threads summed over all organisms.",
            Metric::TotalOrganisms => "Organisms placed since the run began.",
        }
    }

    /// Whether the value is a whole count (formatted with separators).
    pub fn is_count(self) -> bool {
        matches!(
            self,
            Metric::Organisms
                | Metric::Genotypes
                | Metric::ThresholdGenotypes
                | Metric::Births
                | Metric::Deaths
                | Metric::BreedTrue
                | Metric::Executed
                | Metric::Threads
                | Metric::TotalOrganisms
        )
    }

    pub fn value(self, r: &UpdateRow) -> f64 {
        match self {
            Metric::Organisms => f64::from(r.orgs),
            Metric::Genotypes => f64::from(r.genotypes),
            Metric::ThresholdGenotypes => f64::from(r.threshold_genotypes),
            Metric::AveFitness => r.ave_fitness,
            Metric::MaxFitness => r.max_fitness,
            Metric::AveMerit => r.ave_merit,
            Metric::MaxMerit => r.max_merit,
            Metric::AveGestation => r.ave_gestation,
            Metric::AveGeneration => r.ave_generation,
            Metric::AveAge => r.ave_age,
            Metric::AveGenomeSize => r.ave_mem_size,
            Metric::Births => f64::from(r.births),
            Metric::Deaths => f64::from(r.deaths),
            Metric::BreedTrue => f64::from(r.breed_true),
            Metric::Executed => f64::from(r.executed),
            Metric::Threads => f64::from(r.threads),
            Metric::TotalOrganisms => f64::from(r.tot_organisms),
        }
    }

    /// A stable identifier (CSV column names).
    pub fn key(self) -> &'static str {
        match self {
            Metric::Organisms => "orgs",
            Metric::Genotypes => "genotypes",
            Metric::ThresholdGenotypes => "threshold_genotypes",
            Metric::AveFitness => "ave_fitness",
            Metric::MaxFitness => "max_fitness",
            Metric::AveMerit => "ave_merit",
            Metric::MaxMerit => "max_merit",
            Metric::AveGestation => "ave_gestation",
            Metric::AveGeneration => "ave_generation",
            Metric::AveAge => "ave_age",
            Metric::AveGenomeSize => "ave_mem_size",
            Metric::Births => "births",
            Metric::Deaths => "deaths",
            Metric::BreedTrue => "breed_true",
            Metric::Executed => "executed",
            Metric::Threads => "threads",
            Metric::TotalOrganisms => "tot_organisms",
        }
    }
}

/// A series that can be plotted from the row history.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum SeriesKey {
    Metric(Metric),
    /// `task_last_count[k]`
    Task(usize),
}

impl SeriesKey {
    pub fn value(self, r: &UpdateRow) -> f64 {
        match self {
            SeriesKey::Metric(m) => m.value(r),
            SeriesKey::Task(k) => r.task_last_count.get(k).map_or(f64::NAN, |&c| f64::from(c)),
        }
    }
}

/// Decimated series for the charts.
///
/// The whole run is decimated incrementally: rows are grouped into
/// buckets of a power-of-two size, and each finished bucket keeps its
/// lowest and highest value (so spikes stay visible). New rows only fill
/// the buckets at the end; when there are too many buckets, neighbours
/// merge. A frame that brings a few rows therefore costs about the
/// number of points drawn, not the length of the run. Only thinning the
/// history (see [`RowHistory::epoch`]) starts a series over.
///
/// A zoomed-in chart asks for a window of the run, decimated from the
/// rows inside it.
#[derive(Debug, Default)]
pub struct SeriesCache {
    /// (series, resolution) -> the whole run.
    entries: HashMap<(SeriesKey, usize), Incremental>,
    /// (series, resolution) -> the last window asked for.
    windows: HashMap<(SeriesKey, usize), Window>,
}

/// A point of a series: `[update, value]`.
type Point = [f64; 2];

/// The lowest and highest point of a run of rows, in update order (the
/// same point twice when they coincide); `None` when no value is finite.
type Extremes = Option<(Point, Point)>;

#[derive(Debug, Default)]
struct Incremental {
    epoch: u64,
    version: u64,
    /// Rows per finished bucket (a power of two).
    bucket: usize,
    /// Rows folded into `buckets`.
    done: usize,
    buckets: Vec<Extremes>,
    points: Vec<Point>,
}

#[derive(Debug, Default)]
struct Window {
    version: u64,
    rows: std::ops::Range<usize>,
    points: Vec<Point>,
}

/// The resolution a plot asking for `max_points` gets: a power of two no
/// larger than it (at least 16), so resizing a panel does not create a new
/// cache entry per pixel.
pub fn resolution(max_points: usize) -> usize {
    let n = max_points.max(16);
    1 << (usize::BITS - 1 - n.leading_zeros())
}

fn extremes(rows: &[UpdateRow], key: SeriesKey) -> Extremes {
    let mut lo: Option<Point> = None;
    let mut hi: Option<Point> = None;
    for r in rows {
        let y = key.value(r);
        if !y.is_finite() {
            continue;
        }
        let p = [f64::from(r.update), y];
        if lo.is_none_or(|l| y < l[1]) {
            lo = Some(p);
        }
        if hi.is_none_or(|h| y > h[1]) {
            hi = Some(p);
        }
    }
    let (lo, hi) = (lo?, hi?);
    Some(if lo[0] <= hi[0] { (lo, hi) } else { (hi, lo) })
}

fn merge(a: Extremes, b: Extremes) -> Extremes {
    let (Some(a), Some(b)) = (a, b) else {
        return a.or(b);
    };
    let pts = [a.0, a.1, b.0, b.1];
    let lo = pts
        .iter()
        .copied()
        .reduce(|m, p| if p[1] < m[1] { p } else { m })?;
    let hi = pts
        .iter()
        .copied()
        .reduce(|m, p| if p[1] > m[1] { p } else { m })?;
    Some(if lo[0] <= hi[0] { (lo, hi) } else { (hi, lo) })
}

impl Incremental {
    /// Brings the points up to date with `history`, with at most
    /// `resolution` points.
    fn update(&mut self, history: &RowHistory, key: SeriesKey, resolution: usize) {
        let limit = (resolution / 2).saturating_sub(2).max(2);
        let rows = history.rows();
        if self.epoch != history.epoch() || self.bucket == 0 {
            self.epoch = history.epoch();
            self.buckets.clear();
            self.done = 0;
            self.bucket = rows.len().div_ceil(limit).max(1).next_power_of_two();
        } else if self.version == history.version() {
            return;
        }
        self.version = history.version();
        // The newest row may still be replaced: never fold it in for good.
        let settled = rows.len().saturating_sub(1);
        while self.done + self.bucket <= settled {
            let run = &rows[self.done..self.done + self.bucket];
            self.buckets.push(extremes(run, key));
            self.done += self.bucket;
            if self.buckets.len() > limit {
                if self.buckets.len() % 2 == 1 {
                    // Its rows are folded again into a bucket twice the size.
                    self.buckets.pop();
                    self.done -= self.bucket;
                }
                self.buckets = self
                    .buckets
                    .chunks(2)
                    .map(|pair| merge(pair[0], pair[1]))
                    .collect();
                self.bucket *= 2;
            }
        }
        let mut points: Vec<Point> = Vec::with_capacity(2 * self.buckets.len() + 4);
        let mut push = |p: Point| {
            if points.last().is_none_or(|last: &Point| p[0] > last[0]) {
                points.push(p);
            }
        };
        let first = rows.first().map(|r| [f64::from(r.update), key.value(r)]);
        if let Some(p) = first.filter(|p| p[1].is_finite()) {
            push(p);
        }
        let tail = extremes(rows.get(self.done..settled).unwrap_or_default(), key);
        for (a, b) in self.buckets.iter().chain(std::iter::once(&tail)).flatten() {
            push(*a);
            push(*b);
        }
        let last = rows.last().map(|r| [f64::from(r.update), key.value(r)]);
        if let Some(p) = last.filter(|p| p[1].is_finite()) {
            push(p);
        }
        self.points = points;
    }
}

impl SeriesCache {
    /// The series of `key` over the whole history, at most `max_points`
    /// points (min/max decimation keeps spikes visible).
    pub fn get(&mut self, history: &RowHistory, key: SeriesKey, max_points: usize) -> &[Point] {
        let res = resolution(max_points);
        let entry = self.entries.entry((key, res)).or_default();
        entry.update(history, key, res);
        &entry.points
    }

    /// The series of `key` for the updates in `range` (and one row past
    /// each edge), at most `max_points` points; the whole run when
    /// `range` is `None` or covers it.
    pub fn get_range(
        &mut self,
        history: &RowHistory,
        key: SeriesKey,
        max_points: usize,
        range: Option<(f64, f64)>,
    ) -> &[Point] {
        let rows = match range {
            Some((lo, hi)) => history.index_range(lo, hi),
            None => 0..history.len(),
        };
        if rows.start == 0 && rows.end >= history.len() {
            return self.get(history, key, max_points);
        }
        let res = resolution(max_points);
        let window = self.windows.entry((key, res)).or_default();
        if window.version != history.version() || window.rows != rows || window.points.is_empty() {
            window.points = series(&history.rows()[rows.clone()], key, res);
            window.version = history.version();
            window.rows = rows;
        }
        &window.points
    }

    /// Forgets everything (a new experiment).
    pub fn clear(&mut self) {
        self.entries.clear();
        self.windows.clear();
    }
}

/// The value range to show for `points` whose update lies in `x` (all of
/// them when `None`), with a small margin. Counts start at zero. A flat
/// series gets a band around its value.
pub fn y_fit(points: &[Point], x: Option<(f64, f64)>, from_zero: bool) -> (f64, f64) {
    let mut lo = f64::INFINITY;
    let mut hi = f64::NEG_INFINITY;
    for p in points {
        if x.is_some_and(|(a, b)| p[0] < a || p[0] > b) || !p[1].is_finite() {
            continue;
        }
        lo = lo.min(p[1]);
        hi = hi.max(p[1]);
    }
    if lo > hi {
        return (0.0, 1.0);
    }
    if from_zero {
        lo = lo.min(0.0);
        hi = hi.max(0.0);
    }
    if hi - lo < 1e-12 {
        let pad = if from_zero {
            1.0
        } else {
            (hi.abs() * 0.1).max(1e-3)
        };
        return if from_zero && lo >= 0.0 {
            (0.0, hi + pad)
        } else {
            (lo - pad, hi + pad)
        };
    }
    let margin = (hi - lo) * 0.06;
    let bottom = if from_zero && lo >= 0.0 {
        lo
    } else {
        lo - margin
    };
    (bottom, hi + margin)
}

/// A grid step of 1, 2 or 5 times a power of ten, at least `min` (and at
/// least 1 for whole numbers).
pub fn nice_step(min: f64, whole: bool) -> f64 {
    let min = if whole { min.max(1.0) } else { min };
    if !(min.is_finite() && min > 0.0) {
        return 1.0;
    }
    let power = 10f64.powf(min.log10().floor());
    [1.0, 2.0, 5.0, 10.0]
        .into_iter()
        .map(|m| m * power)
        .find(|&s| s >= min * (1.0 - 1e-9))
        .unwrap_or(10.0 * power)
}

/// `series_of` + `decimate_min_max` for one key.
pub fn series(rows: &[UpdateRow], key: SeriesKey, max_points: usize) -> Vec<[f64; 2]> {
    let (xs, ys) = series_of(rows, |r| key.value(r));
    decimate_min_max(&xs, &ys, max_points)
        .into_iter()
        .map(|(x, y)| [x, y])
        .collect()
}

/// The values of the last `n` rows (sparklines), decimated to `max_points`.
pub fn tail_series(
    rows: &[UpdateRow],
    key: SeriesKey,
    n: usize,
    max_points: usize,
) -> Vec<[f64; 2]> {
    let start = rows.len().saturating_sub(n);
    series(&rows[start..], key, max_points)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rows(n: i32) -> Vec<UpdateRow> {
        (0..n)
            .map(|u| UpdateRow {
                update: u,
                orgs: u * 3,
                ave_fitness: f64::from(u) / 10.0,
                task_last_count: vec![u, 2 * u],
                ..UpdateRow::default()
            })
            .collect()
    }

    #[test]
    fn every_metric_has_distinct_labels_and_keys() {
        let mut keys: Vec<_> = Metric::ALL.iter().map(|m| m.key()).collect();
        keys.sort_unstable();
        keys.dedup();
        assert_eq!(keys.len(), Metric::ALL.len());
        let mut labels: Vec<_> = Metric::ALL.iter().map(|m| m.label()).collect();
        labels.sort_unstable();
        labels.dedup();
        assert_eq!(labels.len(), Metric::ALL.len());
    }

    #[test]
    fn values_read_the_right_columns() {
        let r = &rows(5)[4];
        assert_eq!(Metric::Organisms.value(r), 12.0);
        assert_eq!(Metric::AveFitness.value(r), 0.4);
        assert_eq!(SeriesKey::Task(1).value(r), 8.0);
        assert!(SeriesKey::Task(9).value(r).is_nan());
    }

    #[test]
    fn cache_recomputes_only_when_the_history_changes() {
        let mut h = RowHistory::default();
        h.extend(rows(1000));
        let mut cache = SeriesCache::default();
        let key = SeriesKey::Metric(Metric::Organisms);
        let a = cache.get(&h, key, 100).to_vec();
        assert!(a.len() <= 100);
        assert_eq!(a.first(), Some(&[0.0, 0.0]));
        assert_eq!(a.last(), Some(&[999.0, 2997.0]));
        assert_eq!(cache.get(&h, key, 100), &a[..]);
        h.extend(rows(1001).into_iter().skip(1000));
        assert_eq!(cache.get(&h, key, 100).last(), Some(&[1000.0, 3000.0]));
    }

    fn check_decimated(points: &[[f64; 2]], all: &[[f64; 2]], max_points: usize) {
        assert!(
            points.len() <= max_points,
            "{} > {max_points}",
            points.len()
        );
        assert!(points.windows(2).all(|w| w[0][0] < w[1][0]), "ordered");
        assert_eq!(points.first(), all.first());
        assert_eq!(points.last(), all.last());
        let ext = |v: &[[f64; 2]]| {
            v.iter()
                .fold((f64::INFINITY, f64::NEG_INFINITY), |(lo, hi), p| {
                    (lo.min(p[1]), hi.max(p[1]))
                })
        };
        assert_eq!(ext(points), ext(all), "the extremes survive");
    }

    #[test]
    fn short_series_keep_every_row() {
        let mut h = RowHistory::default();
        h.extend(rows(40));
        let mut cache = SeriesCache::default();
        let s = cache.get(&h, SeriesKey::Metric(Metric::Organisms), 400);
        assert_eq!(s.len(), 40);
    }

    #[test]
    fn growing_series_stay_bounded_and_keep_spikes() {
        let mut h = RowHistory::with_capacity(5000);
        let mut cache = SeriesCache::default();
        let key = SeriesKey::Metric(Metric::Organisms);
        let spiky = |u: i32| UpdateRow {
            update: u,
            orgs: if u % 977 == 0 { 100_000 - u } else { u % 50 },
            ..UpdateRow::default()
        };
        for chunk in 0..120 {
            h.extend((chunk * 97..(chunk + 1) * 97).map(spiky));
            let all: Vec<[f64; 2]> = h
                .rows()
                .iter()
                .map(|r| [f64::from(r.update), key.value(r)])
                .collect();
            let pts = cache.get(&h, key, 300).to_vec();
            check_decimated(&pts, &all, 300);
        }
        assert!(h.stride() > 1, "the history was thinned along the way");
    }

    #[test]
    fn windows_decimate_only_the_rows_inside() {
        let mut h = RowHistory::default();
        h.extend(rows(1000));
        let mut cache = SeriesCache::default();
        let key = SeriesKey::Metric(Metric::Organisms);
        let w = cache.get_range(&h, key, 64, Some((100.0, 199.0))).to_vec();
        assert!(w.len() <= 64);
        assert_eq!(w.first(), Some(&[99.0, 297.0]));
        assert_eq!(w.last(), Some(&[200.0, 600.0]));
        let whole = cache.get_range(&h, key, 64, Some((-10.0, 5000.0))).to_vec();
        assert_eq!(whole, cache.get(&h, key, 64));
    }

    #[test]
    fn y_ranges_fit_the_visible_points() {
        let pts = [[0.0, 5.0], [1.0, 9.0], [2.0, 7.0], [3.0, 100.0]];
        let (lo, hi) = y_fit(&pts, Some((0.0, 2.0)), false);
        assert!(lo < 5.0 && lo > 4.0 && hi > 9.0 && hi < 10.0, "{lo} {hi}");
        assert_eq!(
            y_fit(&pts, Some((0.0, 2.0)), true).0,
            0.0,
            "counts from zero"
        );
        assert_eq!(y_fit(&[[0.0, 0.0], [1.0, 0.0]], None, true), (0.0, 1.0));
        let (lo, hi) = y_fit(&[[0.0, 2.0]], None, false);
        assert!(lo < 2.0 && hi > 2.0);
        assert_eq!(y_fit(&[], None, false), (0.0, 1.0));
    }

    #[test]
    fn grid_steps_are_round() {
        assert_eq!(nice_step(1.18, true), 2.0);
        assert_eq!(nice_step(0.3, true), 1.0, "whole numbers");
        assert!((nice_step(0.03, false) - 0.05).abs() < 1e-12);
        assert_eq!(nice_step(7.0, false), 10.0);
        assert_eq!(nice_step(500.0, false), 500.0);
        assert_eq!(resolution(495), 256);
        assert_eq!(resolution(512), 512);
        assert_eq!(resolution(3), 16);
    }

    #[test]
    fn tail_series_covers_the_last_rows() {
        let r = rows(50);
        let s = tail_series(&r, SeriesKey::Metric(Metric::Organisms), 10, 100);
        assert_eq!(s.len(), 10);
        assert_eq!(s[0], [40.0, 120.0]);
    }
}
