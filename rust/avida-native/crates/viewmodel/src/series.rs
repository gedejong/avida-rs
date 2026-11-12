//! Histograms of layer values and min/max decimation of time series.

use crate::legend::Scale;
use crate::model::UpdateRow;

/// Counts of values in equal-width bins (equal width in log space for
/// [`Scale::Log`]).
#[derive(Clone, Debug, PartialEq)]
pub struct Histogram {
    /// Lower edge of the first bin.
    pub min: f64,
    /// Upper edge of the last bin (inclusive).
    pub max: f64,
    /// Linear or logarithmic bins.
    pub scale: Scale,
    /// One count per bin.
    pub counts: Vec<u32>,
    /// Values left out: non-finite (empty cells), outside `[min, max]`, or
    /// `<= 0` on a log scale.
    pub skipped: usize,
}

impl Histogram {
    /// Bins spanning the finite values (positive ones for a log scale). A
    /// single distinct value is centred in a range of width 1 (a factor of
    /// 10 for a log scale). With no usable value the range is `[0, 1]` and
    /// every count is 0. A log histogram without positive values is linear.
    pub fn new(values: &[f32], bins: usize, scale: Scale) -> Histogram {
        let usable = |v: f32| v.is_finite() && (scale == Scale::Linear || v > 0.0);
        let mut lo = f64::INFINITY;
        let mut hi = f64::NEG_INFINITY;
        for &v in values.iter().filter(|v| usable(**v)) {
            lo = lo.min(f64::from(v));
            hi = hi.max(f64::from(v));
        }
        if lo > hi {
            if scale == Scale::Log && values.iter().any(|v| v.is_finite()) {
                return Histogram::new(values, bins, Scale::Linear);
            }
            return Histogram::with_range(values, bins, 0.0, 1.0, Scale::Linear);
        }
        Histogram::with_range(values, bins, lo, hi, scale)
    }

    /// Bins spanning `[min, max]` (swapped if reversed; widened as in
    /// [`Histogram::new`] when equal). `bins` is at least 1. A log
    /// histogram needs `min > 0`, otherwise it is linear.
    pub fn with_range(values: &[f32], bins: usize, min: f64, max: f64, scale: Scale) -> Histogram {
        let bins = bins.max(1);
        let (mut min, mut max) = if min <= max { (min, max) } else { (max, min) };
        let scale = if scale == Scale::Log && min <= 0.0 {
            Scale::Linear
        } else {
            scale
        };
        if min == max {
            match scale {
                Scale::Linear => {
                    min -= 0.5;
                    max += 0.5;
                }
                Scale::Log => {
                    let f = 10f64.sqrt();
                    min /= f;
                    max *= f;
                }
            }
        }
        let mut h = Histogram {
            min,
            max,
            scale,
            counts: vec![0; bins],
            skipped: 0,
        };
        for &v in values {
            match h.bin_of(f64::from(v)) {
                Some(b) => h.counts[b] += 1,
                None => h.skipped += 1,
            }
        }
        h
    }

    /// The bin a value falls in; the top edge belongs to the last bin.
    pub fn bin_of(&self, v: f64) -> Option<usize> {
        if !v.is_finite() || v < self.min || v > self.max {
            return None;
        }
        let t = match self.scale {
            Scale::Linear => (v - self.min) / (self.max - self.min),
            Scale::Log => {
                if v <= 0.0 {
                    return None;
                }
                (v.ln() - self.min.ln()) / (self.max.ln() - self.min.ln())
            }
        };
        let n = self.counts.len();
        Some(((t * n as f64) as usize).min(n - 1))
    }

    /// The `bins + 1` bin edges.
    pub fn bin_edges(&self) -> Vec<f64> {
        let n = self.counts.len();
        (0..=n)
            .map(|i| {
                let t = i as f64 / n as f64;
                match self.scale {
                    Scale::Linear => self.min + t * (self.max - self.min),
                    Scale::Log => (self.min.ln() + t * (self.max.ln() - self.min.ln())).exp(),
                }
            })
            .collect()
    }

    /// Values counted.
    pub fn total(&self) -> u64 {
        self.counts.iter().map(|&c| u64::from(c)).sum()
    }

    /// The largest bin count (0 for an empty histogram).
    pub fn max_count(&self) -> u32 {
        self.counts.iter().copied().max().unwrap_or(0)
    }
}

/// Reduces a series of points to at most `max_points` for a line plot of
/// limited width, keeping the shape: the first and last points always,
/// and from each of `(max_points - 2) / 2` equal runs of the points in
/// between the lowest and the highest one, in their original order. Points
/// with a non-finite `y` are dropped. A series that already fits is
/// returned whole; `max_points < 4` keeps only the end points (or the last
/// point, or nothing). `xs` and `ys` are read up to the shorter length.
pub fn decimate_min_max(xs: &[f64], ys: &[f64], max_points: usize) -> Vec<(f64, f64)> {
    let pts: Vec<(f64, f64)> = xs
        .iter()
        .zip(ys)
        .map(|(&x, &y)| (x, y))
        .filter(|p| p.1.is_finite())
        .collect();
    let n = pts.len();
    if n <= max_points {
        return pts;
    }
    match max_points {
        0 => return Vec::new(),
        1 => return vec![pts[n - 1]],
        2 | 3 => return vec![pts[0], pts[n - 1]],
        _ => {}
    }
    let buckets = (max_points - 2) / 2;
    let inner = &pts[1..n - 1];
    let mut out = Vec::with_capacity(2 + 2 * buckets);
    out.push(pts[0]);
    for b in 0..buckets {
        let start = b * inner.len() / buckets;
        let end = (b + 1) * inner.len() / buckets;
        let run = &inner[start..end];
        if run.is_empty() {
            continue;
        }
        let mut lo = 0;
        let mut hi = 0;
        for (i, p) in run.iter().enumerate() {
            if p.1 < run[lo].1 {
                lo = i;
            }
            if p.1 > run[hi].1 {
                hi = i;
            }
        }
        let (a, b) = if lo <= hi { (lo, hi) } else { (hi, lo) };
        out.push(run[a]);
        if b != a {
            out.push(run[b]);
        }
    }
    out.push(pts[n - 1]);
    out
}

/// One column of a history of [`UpdateRow`]s as `(updates, values)`, ready
/// for [`decimate_min_max`].
pub fn series_of(rows: &[UpdateRow], f: impl Fn(&UpdateRow) -> f64) -> (Vec<f64>, Vec<f64>) {
    rows.iter().map(|r| (f64::from(r.update), f(r))).unzip()
}
