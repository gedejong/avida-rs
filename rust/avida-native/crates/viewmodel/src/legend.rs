//! Colour legends: the value range a palette spans, the mapping of values
//! onto it, and tick marks with labels.

/// How values map onto the palette.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum Scale {
    /// Evenly spaced values get evenly spaced colours.
    #[default]
    Linear,
    /// Evenly spaced logarithms get evenly spaced colours. Values `<= 0`
    /// map to the bottom of the range.
    Log,
}

/// A tick mark of a [`Legend`].
#[derive(Clone, Debug, PartialEq)]
pub struct Tick {
    /// The value at the tick.
    pub value: f64,
    /// Its position along the legend, in `[0, 1]`.
    pub pos: f64,
    /// The value as text.
    pub label: String,
}

/// A value range, its scale and its tick marks.
#[derive(Clone, Debug, PartialEq)]
pub struct Legend {
    /// Bottom of the range (position 0).
    pub min: f64,
    /// Top of the range (position 1).
    pub max: f64,
    /// The scale actually used: [`Scale::Log`] degrades to
    /// [`Scale::Linear`] when the range has no positive lower bound.
    pub scale: Scale,
    /// Tick marks, ascending.
    pub ticks: Vec<Tick>,
}

/// About this many ticks are placed on a linear legend.
const TARGET_TICKS: f64 = 5.0;

impl Legend {
    /// A legend over `[min, max]` (swapped if reversed). A log legend needs
    /// `min > 0`; otherwise it is linear.
    pub fn new(min: f64, max: f64, scale: Scale) -> Legend {
        let (min, max) = if min <= max { (min, max) } else { (max, min) };
        let scale = if scale == Scale::Log && !(min > 0.0 && max.is_finite()) {
            Scale::Linear
        } else {
            scale
        };
        let mut legend = Legend {
            min,
            max,
            scale,
            ticks: Vec::new(),
        };
        legend.ticks = match scale {
            Scale::Linear => legend.linear_ticks(),
            Scale::Log => legend.log_ticks(),
        };
        legend
    }

    /// A legend spanning the finite values of a layer (the positive ones
    /// for a log scale; if there are none the legend is linear). `None`
    /// when there is no finite value at all.
    pub fn from_values(values: &[f32], scale: Scale) -> Option<Legend> {
        let range = |keep: &dyn Fn(f32) -> bool| {
            values
                .iter()
                .copied()
                .filter(|v| v.is_finite() && keep(*v))
                .fold(None, |acc: Option<(f64, f64)>, v| {
                    let v = f64::from(v);
                    Some(match acc {
                        None => (v, v),
                        Some((lo, hi)) => (lo.min(v), hi.max(v)),
                    })
                })
        };
        if scale == Scale::Log {
            if let Some((lo, hi)) = range(&|v| v > 0.0) {
                return Some(Legend::new(lo, hi, Scale::Log));
            }
        }
        range(&|_| true).map(|(lo, hi)| Legend::new(lo, hi, Scale::Linear))
    }

    /// True when the range is a single value.
    pub fn is_degenerate(&self) -> bool {
        self.min == self.max
    }

    /// The position of `v` along the legend, clamped to `[0, 1]`. NaN stays
    /// NaN; a degenerate range puts everything at 0.5; on a log scale
    /// values `<= 0` are at 0.
    pub fn normalize(&self, v: f64) -> f64 {
        if v.is_nan() {
            return f64::NAN;
        }
        if self.is_degenerate() {
            return 0.5;
        }
        let t = match self.scale {
            Scale::Linear => (v - self.min) / (self.max - self.min),
            Scale::Log => {
                if v <= 0.0 {
                    return 0.0;
                }
                (v.ln() - self.min.ln()) / (self.max.ln() - self.min.ln())
            }
        };
        t.clamp(0.0, 1.0)
    }

    /// The value at position `pos` (inverse of [`Legend::normalize`] inside
    /// the range).
    pub fn value_at(&self, pos: f64) -> f64 {
        let pos = pos.clamp(0.0, 1.0);
        match self.scale {
            Scale::Linear => self.min + pos * (self.max - self.min),
            Scale::Log => (self.min.ln() + pos * (self.max.ln() - self.min.ln())).exp(),
        }
    }

    fn tick(&self, value: f64, label: String) -> Tick {
        Tick {
            value,
            pos: self.normalize(value),
            label,
        }
    }

    /// Multiples of a 1-2-5 step inside the range.
    fn linear_ticks(&self) -> Vec<Tick> {
        if !(self.min.is_finite() && self.max.is_finite()) {
            return Vec::new();
        }
        if self.is_degenerate() {
            return vec![self.tick(self.min, format_number(self.min))];
        }
        let step = nice_step((self.max - self.min) / TARGET_TICKS);
        let decimals = decimals_for(step);
        let first = (self.min / step).ceil();
        let last = (self.max / step).floor();
        let mut ticks = Vec::new();
        let mut k = first;
        while k <= last {
            let mut v = k * step;
            if v.abs() < step * 1e-9 {
                v = 0.0;
            }
            let label = match decimals {
                Some(d) => format!("{v:.d$}"),
                None => format_number(v),
            };
            ticks.push(self.tick(v, label));
            k += 1.0;
        }
        ticks
    }

    /// Powers of ten, or 1-2-5 multiples when the range spans less than two
    /// decades, or linear ticks when even those are too sparse.
    fn log_ticks(&self) -> Vec<Tick> {
        if self.is_degenerate() {
            return vec![self.tick(self.min, format_number(self.min))];
        }
        let lo = self.min.log10();
        let hi = self.max.log10();
        // A small tolerance keeps exact powers of ten at the ends.
        let first = (lo - 1e-9).ceil() as i32;
        let last = (hi + 1e-9).floor() as i32;
        let decades: Vec<i32> = (first..=last).collect();
        let in_range = |v: f64| v >= self.min * (1.0 - 1e-9) && v <= self.max * (1.0 + 1e-9);
        let values: Vec<f64> = if decades.len() >= 2 {
            let stride = decades.len().div_ceil(8);
            decades
                .iter()
                .step_by(stride)
                .map(|&e| 10f64.powi(e))
                .collect()
        } else {
            let mut vs = Vec::new();
            for e in (lo.floor() as i32)..=(hi.ceil() as i32) {
                for m in [1.0, 2.0, 5.0] {
                    let v = m * 10f64.powi(e);
                    if in_range(v) {
                        vs.push(v);
                    }
                }
            }
            vs
        };
        if values.len() < 2 {
            return self.linear_ticks();
        }
        values
            .into_iter()
            .map(|v| self.tick(v, format_number(v)))
            .collect()
    }
}

/// The 1-2-5 number nearest above `raw`.
fn nice_step(raw: f64) -> f64 {
    let mag = 10f64.powf(raw.log10().floor());
    let norm = raw / mag;
    let nice = if norm < 1.5 {
        1.0
    } else if norm < 3.0 {
        2.0
    } else if norm < 7.0 {
        5.0
    } else {
        10.0
    };
    nice * mag
}

/// Decimals needed to show multiples of `step`, or `None` when the step is
/// so small or large that [`format_number`] reads better.
fn decimals_for(step: f64) -> Option<usize> {
    if !(1e-4..1e6).contains(&step) {
        return None;
    }
    let d = -step.log10().floor();
    Some(if d > 0.0 { d as usize } else { 0 })
}

/// A compact number: integers as integers below a million, other values
/// with up to four significant digits, exponents outside `[1e-4, 1e6)`.
pub fn format_number(v: f64) -> String {
    if !v.is_finite() {
        return format!("{v}");
    }
    let a = v.abs();
    if a == 0.0 {
        return "0".into();
    }
    if v.fract() == 0.0 && a < 1e6 {
        return format!("{}", v as i64);
    }
    if (1e-4..1e6).contains(&a) {
        let digits = 4 - 1 - a.log10().floor() as i32;
        let d = digits.max(0) as usize;
        let s = format!("{v:.d$}");
        if s.contains('.') {
            return s.trim_end_matches('0').trim_end_matches('.').to_string();
        }
        return s;
    }
    let s = format!("{v:.3e}");
    // `1.500e7` -> `1.5e7`
    match s.split_once('e') {
        Some((m, e)) if m.contains('.') => {
            format!("{}e{e}", m.trim_end_matches('0').trim_end_matches('.'))
        }
        _ => s,
    }
}
