//! Small statistics holders used by genotypes and the genotype arbiter.

/// Port of `cDoubleSum` (avida-core/source/tools/cDoubleSum.h) as it was at
/// the pre-Rust reference commit 47f13dadb (inline C++; the current tree
/// forwards to the `avd_ds_*` FFI helpers in rust/avida-rust/src/double_sum.rs).
///
/// The reference binary is the current tree, so `s2 += w_val * w_val` (and
/// `s2 -= ...`) is rounded twice, as in the Rust helper (`otool -tv` of
/// `_avd_ds_add`/`_avd_ds_subtract` shows `fmul` then `fadd`/`fsub`), not
/// once as the contracted 47f13dadb header was.
#[derive(Clone, Copy, Debug, PartialEq)]
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
    /// `cDoubleSum()`: cleared.
    pub fn new() -> Self {
        DoubleSum {
            s1: 0.0,
            s2: 0.0,
            n: 0.0,
            max: f64::MIN_POSITIVE,
        }
    }

    /// `Clear()`. QUIRK: `max` starts at `numeric_limits<double>::min()`,
    /// the smallest positive normal.
    pub fn clear(&mut self) {
        *self = Self::new();
    }

    /// `Count()` / `N()`.
    pub fn count(&self) -> f64 {
        self.n
    }

    /// `Sum()`.
    pub fn sum(&self) -> f64 {
        self.s1
    }

    /// `Max()`.
    pub fn max(&self) -> f64 {
        self.max
    }

    /// `Average()`: 0 when empty.
    pub fn average(&self) -> f64 {
        if self.n > 0.0 {
            self.s1 / self.n
        } else {
            0.0
        }
    }

    /// `Variance()`: `(s2 - s1*s1/n) / (n - 1)`, 0 for `n <= 1`.
    pub fn variance(&self) -> f64 {
        if self.n > 1.0 {
            (self.s2 - self.s1 * self.s1 / self.n) / (self.n - 1.0)
        } else {
            0.0
        }
    }

    /// `StdDeviation()`.
    pub fn std_deviation(&self) -> f64 {
        self.variance().sqrt()
    }

    /// `StdError()`: `sqrt(Variance() / n)`, 0 for `n <= 1`.
    pub fn std_error(&self) -> f64 {
        if self.n > 1.0 {
            (self.variance() / self.n).sqrt()
        } else {
            0.0
        }
    }

    /// `Add(value, weight)`.
    pub fn add(&mut self, value: f64, weight: f64) {
        let w_val = value * weight;
        self.n += weight;
        self.s1 += w_val;
        self.s2 += w_val * w_val;
        if value > self.max {
            self.max = value;
        }
    }

    /// `Add(value)` with the default weight 1.
    pub fn add1(&mut self, value: f64) {
        self.add(value, 1.0);
    }

    /// `Subtract(value, weight)`.
    pub fn subtract(&mut self, value: f64, weight: f64) {
        let w_val = value * weight;
        self.n -= weight;
        self.s1 -= w_val;
        self.s2 -= w_val * w_val;
    }
}

/// Port of `cCountTracker` (avida-core/source/tools/cCountTracker.h).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct CountTracker {
    cur_count: i32,
    last_count: i32,
    total_count: i32,
}

impl CountTracker {
    /// `GetCur()`.
    pub fn cur(&self) -> i32 {
        self.cur_count
    }

    /// `GetLast()`.
    pub fn last(&self) -> i32 {
        self.last_count
    }

    /// `GetTotal()`.
    pub fn total(&self) -> i32 {
        self.total_count
    }

    /// `Inc()`.
    pub fn inc(&mut self) {
        self.cur_count = self.cur_count.wrapping_add(1);
        self.total_count = self.total_count.wrapping_add(1);
    }

    /// `Dec()`.
    pub fn dec(&mut self) {
        self.cur_count = self.cur_count.wrapping_sub(1);
    }

    /// `Next()`.
    pub fn next(&mut self) {
        self.last_count = self.cur_count;
        self.cur_count = 0;
    }

    /// `Clear()`.
    pub fn clear(&mut self) {
        *self = Self::default();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn double_sum_basics() {
        let mut d = DoubleSum::new();
        assert_eq!((d.average(), d.variance(), d.std_error()), (0.0, 0.0, 0.0));
        assert_eq!(d.max(), f64::MIN_POSITIVE);
        d.add1(2.0);
        d.add1(4.0);
        d.add(6.0, 2.0);
        // n = 4, s1 = 18, s2 = 4 + 16 + 144 = 164
        assert_eq!(d.count(), 4.0);
        assert_eq!(d.sum(), 18.0);
        assert_eq!(d.average(), 4.5);
        assert_eq!(d.variance(), (164.0 - 18.0 * 18.0 / 4.0) / 3.0);
        assert_eq!(d.std_error(), (d.variance() / 4.0).sqrt());
        assert_eq!(d.max(), 6.0);
        d.subtract(6.0, 2.0);
        assert_eq!((d.count(), d.sum()), (2.0, 6.0));
    }

    #[test]
    fn double_sum_square_accumulation_is_not_fused() {
        // (2^27 + 1)^2 = 2^54 + 2^28 + 1 needs 55 bits. With s2 = 2, the
        // exact sum ends in ...+3 and rounds up once; rounded twice, as the
        // reference binary's `avd_ds_add` does, it ties to even and rounds
        // down.
        let v = 134_217_729.0_f64;
        let mut d = DoubleSum::new();
        d.add1(1.0);
        d.add1(1.0);
        d.add1(v);
        let unfused = v * v + 2.0;
        assert_eq!(d.s2, unfused);
        assert_ne!(unfused, v.mul_add(v, 2.0));
    }

    #[test]
    fn count_tracker() {
        let mut c = CountTracker::default();
        c.inc();
        c.inc();
        c.dec();
        assert_eq!((c.cur(), c.last(), c.total()), (1, 0, 2));
        c.next();
        assert_eq!((c.cur(), c.last(), c.total()), (0, 1, 2));
    }
}
