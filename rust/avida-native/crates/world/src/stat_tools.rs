//! Ports of the small statistics helpers in `avida-core/source/tools`:
//! `cDoubleSum`, `cRunningAverage` and `cRunningStats`.
//!
//! In the reference binary these three classes are thin C++ wrappers around
//! Rust handles (`avd_ds_*`, `avd_ra_*`, `avd_rs_*` in
//! `rust/avida-rust/src/{double_sum,running_average,running_stats}.rs`), so
//! their arithmetic is Rust's: no multiply is fused into an add (`otool -tv`
//! shows `fmul`/`fadd`/`fsub` and no `fmadd`/`fmsub` in `_avd_ds_add`,
//! `_avd_ra_add` and `_avd_rs_push`), unlike the pre-Rust C++ headers, which
//! clang contracted. The expressions below follow those Rust bodies operation
//! for operation.

/// C++: `cDoubleSum`
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct DoubleSum {
    s1: f64,
    s2: f64,
    n: f64,
    max: f64,
}

impl Default for DoubleSum {
    fn default() -> Self {
        Self::new()
    }
}

impl DoubleSum {
    pub fn new() -> Self {
        DoubleSum {
            s1: 0.0,
            s2: 0.0,
            n: 0.0,
            // QUIRK: `numeric_limits<double>::min()` is the smallest positive
            // normal, so `max()` never reports a non-positive maximum.
            max: f64::MIN_POSITIVE,
        }
    }

    pub fn clear(&mut self) {
        *self = Self::new();
    }

    pub fn count(&self) -> f64 {
        self.n
    }

    pub fn sum(&self) -> f64 {
        self.s1
    }

    pub fn max(&self) -> f64 {
        self.max
    }

    pub fn average(&self) -> f64 {
        if self.n > 0.0 {
            self.s1 / self.n
        } else {
            0.0
        }
    }

    pub fn variance(&self) -> f64 {
        if self.n > 1.0 {
            (self.s2 - self.s1 * self.s1 / self.n) / (self.n - 1.0)
        } else {
            0.0
        }
    }

    pub fn std_deviation(&self) -> f64 {
        self.variance().sqrt()
    }

    pub fn std_error(&self) -> f64 {
        if self.n > 1.0 {
            (self.variance() / self.n).sqrt()
        } else {
            0.0
        }
    }

    /// `Add(value)` with the default weight of 1.
    pub fn add(&mut self, value: f64) {
        self.add_weighted(value, 1.0);
    }

    /// `Add(value, weight)`
    pub fn add_weighted(&mut self, value: f64, weight: f64) {
        let w_val = value * weight;
        self.n += weight;
        self.s1 += w_val;
        self.s2 += w_val * w_val;
        if value > self.max {
            self.max = value;
        }
    }

    /// `Subtract(value, weight)`
    pub fn subtract(&mut self, value: f64, weight: f64) {
        let w_val = value * weight;
        self.n -= weight;
        self.s1 -= w_val;
        self.s2 -= w_val * w_val;
    }
}

/// C++: `cRunningAverage`
#[derive(Debug, Clone, PartialEq)]
pub struct RunningAverage {
    values: Vec<f64>,
    s1: f64,
    s2: f64,
    window_size: usize,
    pointer: usize,
    n: usize,
}

impl RunningAverage {
    pub fn new(window_size: usize) -> Self {
        RunningAverage {
            values: vec![0.0; window_size],
            s1: 0.0,
            s2: 0.0,
            window_size,
            pointer: 0,
            n: 0,
        }
    }

    pub fn add(&mut self, value: f64) {
        self.s1 += value;
        self.s2 += value * value;
        if self.n < self.window_size {
            self.values[self.n] = value;
            self.n += 1;
        } else {
            let out_v = self.values[self.pointer];
            self.s1 -= out_v;
            self.s2 -= out_v * out_v;
            self.values[self.pointer] = value;
            self.pointer += 1;
            if self.pointer == self.window_size {
                self.pointer = 0;
            }
        }
    }

    pub fn clear(&mut self) {
        self.s1 = 0.0;
        self.s2 = 0.0;
        self.pointer = 0;
        self.n = 0;
    }

    pub fn sum(&self) -> f64 {
        self.s1
    }

    pub fn sum_of_squares(&self) -> f64 {
        self.s2
    }

    /// Zero until the window is full.
    pub fn average(&self) -> f64 {
        if self.n == self.window_size {
            self.s1 / self.n as f64
        } else {
            0.0
        }
    }

    pub fn variance(&self) -> f64 {
        if self.n == self.window_size {
            let n = self.n as f64;
            (self.s2 - self.s1 * self.s1 / n) / (n - 1.0)
        } else {
            0.0
        }
    }

    pub fn std_deviation(&self) -> f64 {
        self.variance().sqrt()
    }

    /// QUIRK: the C++ takes the root of `s2 - s1*s1/n/(n*(n-1))`.
    pub fn std_error(&self) -> f64 {
        if self.n == self.window_size {
            let n = self.n as f64;
            (self.s2 - self.s1 * self.s1 / n / (n * (n - 1.0))).sqrt()
        } else {
            0.0
        }
    }
}

/// C++: `cRunningStats` (`avd_rs_*` in the reference binary).
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct RunningStats {
    n: f64,
    m1: f64,
    m2: f64,
    m3: f64,
    m4: f64,
}

impl RunningStats {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn clear(&mut self) {
        *self = Self::default();
    }

    pub fn push(&mut self, x: f64) {
        self.n += 1.0;
        let n = self.n;
        let d = x - self.m1;
        let d_n = d / n;
        let d_n2 = d_n * d_n;
        self.m4 += d * d_n2 * d_n * ((n - 1.0) * ((n * n) - 3.0 * n + 3.0)) + 6.0 * d_n2 * self.m2
            - 4.0 * d_n * self.m3;
        self.m3 += d * d_n2 * ((n - 1.0) * (n - 2.0)) - 3.0 * d_n * self.m2;
        self.m2 += d * d_n * (n - 1.0);
        self.m1 += d_n;
    }

    pub fn n(&self) -> f64 {
        self.n
    }

    pub fn mean(&self) -> f64 {
        self.m1
    }

    pub fn variance(&self) -> f64 {
        if self.n > 1.0 {
            self.m2 / (self.n - 1.0)
        } else {
            0.0
        }
    }

    pub fn std_deviation(&self) -> f64 {
        self.variance().sqrt()
    }

    pub fn std_error(&self) -> f64 {
        if self.n > 1.0 {
            (self.variance() / self.n).sqrt()
        } else {
            0.0
        }
    }

    pub fn skewness(&self) -> f64 {
        self.n.sqrt() * self.m3 / self.m2.powf(1.5)
    }

    pub fn kurtosis(&self) -> f64 {
        self.n * self.m4 / (self.m2 * self.m2)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn double_sum_basics() {
        let mut s = DoubleSum::new();
        assert_eq!(s.average(), 0.0);
        for v in [1.0, 2.0, 3.0, 4.0] {
            s.add(v);
        }
        assert_eq!(s.count(), 4.0);
        assert_eq!(s.average(), 2.5);
        assert!((s.variance() - 5.0 / 3.0).abs() < 1e-15);
        assert_eq!(s.max(), 4.0);
        s.subtract(4.0, 1.0);
        assert_eq!(s.average(), 2.0);
        // QUIRK: the maximum of non-positive values is DBL_MIN.
        let mut neg = DoubleSum::new();
        neg.add(-1.0);
        assert_eq!(neg.max(), f64::MIN_POSITIVE);
    }

    #[test]
    fn running_average_needs_a_full_window() {
        let mut r = RunningAverage::new(3);
        r.add(1.0);
        r.add(2.0);
        assert_eq!(r.average(), 0.0);
        r.add(3.0);
        assert_eq!(r.average(), 2.0);
        r.add(6.0);
        assert!((r.average() - 11.0 / 3.0).abs() < 1e-15);
        assert!((r.variance() - 13.0 / 3.0).abs() < 1e-12);
    }

    #[test]
    fn running_stats_moments() {
        let mut r = RunningStats::new();
        for v in [2.0, 4.0, 4.0, 4.0, 5.0, 5.0, 7.0, 9.0] {
            r.push(v);
        }
        assert_eq!(r.n(), 8.0);
        assert!((r.mean() - 5.0).abs() < 1e-14);
        assert!((r.variance() - 32.0 / 7.0).abs() < 1e-13);
        assert!((r.skewness() - 0.65625).abs() < 1e-12);
        assert!((r.kurtosis() - 2.78125).abs() < 1e-12);
    }
}
