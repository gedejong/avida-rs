//! Port of `Apto::Stat::Accumulator` with its default statistics
//! (libs/apto/include/apto/stat/Accumulator.h, AccumulatorStats.h): Count,
//! Min, Max, Mean, Variance, StdError, and the Sum and second Moment they
//! depend on. `apto/stat/Functions.h` only declares `FishersExact`, which
//! avida-core does not use; nothing from it is ported.
//!
//! Arithmetic is the C++'s exactly: the sum and the sum of squares are kept
//! in the value type (so `Accumulator<i32>`'s second moment wraps on
//! overflow, as the C++ `int` does in practice), and the variance is the
//! population variance `E[x^2] - mean^2` despite the "sample variance"
//! comment in the header. Verified against `tools/ref_harness.cc acc`
//! (`testdata/acc_ref.txt`).

/// The value types avida-core instantiates (`int`, `double`).
pub trait AccValue: Copy + PartialOrd {
    /// `0` of the type.
    const ZERO: Self;
    /// `std::numeric_limits<T>::max()`: the initial minimum.
    const LIMIT_MAX: Self;
    /// `std::numeric_limits<T>::min()`: the initial maximum. QUIRK: for
    /// `double` this is the smallest positive normal (`DBL_MIN`), so a
    /// `double` accumulator of negative values reports `Max() == DBL_MIN`.
    const LIMIT_MIN: Self;
    /// `+` in the value type (wrapping for integers).
    fn acc_add(self, other: Self) -> Self;
    /// `*` in the value type (wrapping for integers).
    fn acc_mul(self, other: Self) -> Self;
    /// `static_cast<double>`.
    fn to_f64(self) -> f64;
}

impl AccValue for i32 {
    const ZERO: Self = 0;
    const LIMIT_MAX: Self = i32::MAX;
    const LIMIT_MIN: Self = i32::MIN;
    fn acc_add(self, other: Self) -> Self {
        self.wrapping_add(other)
    }
    fn acc_mul(self, other: Self) -> Self {
        self.wrapping_mul(other)
    }
    fn to_f64(self) -> f64 {
        f64::from(self)
    }
}

impl AccValue for f64 {
    const ZERO: Self = 0.0;
    const LIMIT_MAX: Self = f64::MAX;
    const LIMIT_MIN: Self = f64::MIN_POSITIVE;
    fn acc_add(self, other: Self) -> Self {
        self + other
    }
    fn acc_mul(self, other: Self) -> Self {
        self * other
    }
    fn to_f64(self) -> f64 {
        self
    }
}

/// `Apto::Stat::Accumulator<T>` with the default statistics list.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Accumulator<T: AccValue> {
    /// `StatImpl<Count>::m_n` (`std::size_t`).
    n: usize,
    /// `StatImpl<Sum>::m_s`.
    sum: T,
    /// `StatImpl<Moment<2>>::m_s`.
    sum_sq: T,
    min: T,
    max: T,
}

impl<T: AccValue> Default for Accumulator<T> {
    fn default() -> Self {
        Self::new()
    }
}

impl<T: AccValue> Accumulator<T> {
    /// `Accumulator()`: starts cleared.
    pub fn new() -> Self {
        Accumulator {
            n: 0,
            sum: T::ZERO,
            sum_sq: T::ZERO,
            min: T::LIMIT_MAX,
            max: T::LIMIT_MIN,
        }
    }

    /// `Clear()`.
    pub fn clear(&mut self) {
        *self = Self::new();
    }

    /// `Add(value)`.
    pub fn add(&mut self, value: T) {
        self.n += 1;
        if value > self.max {
            self.max = value;
        }
        if value < self.min {
            self.min = value;
        }
        self.sum = self.sum.acc_add(value);
        // Moment<2>: pow(x, Int<2>) = x * x in the value type.
        self.sum_sq = self.sum_sq.acc_add(value.acc_mul(value));
    }

    /// `Count()`.
    pub fn count(&self) -> usize {
        self.n
    }

    /// `Sum()`.
    pub fn sum(&self) -> T {
        self.sum
    }

    /// `Min()` (`numeric_limits<T>::max()` when empty).
    pub fn min(&self) -> T {
        self.min
    }

    /// `Max()` (`numeric_limits<T>::min()` when empty).
    pub fn max(&self) -> T {
        self.max
    }

    /// `Moment(Type::Int<2>())`: `double(sum of squares) / Count()`; NaN when
    /// empty (0.0 / 0).
    pub fn moment2(&self) -> f64 {
        self.sum_sq.to_f64() / self.n as f64
    }

    /// `Mean()`: NaN when empty.
    pub fn mean(&self) -> f64 {
        if self.n > 0 {
            self.sum.to_f64() / self.n as f64
        } else {
            f64::NAN
        }
    }

    /// `Variance()`: `Moment(2) - Mean()^2` (population variance).
    ///
    /// The reference build (Apple clang, arm64, default `-ffp-contract=on`)
    /// fuses `m2 - mean * mean` into one `fmsub`, so it is rounded once.
    pub fn variance(&self) -> f64 {
        let mean = self.mean();
        (-mean).mul_add(mean, self.moment2())
    }

    /// `StdError()`: `sqrt(Variance() / (Count() - 1))`. QUIRK: `Count() - 1`
    /// is unsigned, so it is NaN for both n = 0 and n = 1.
    pub fn std_error(&self) -> f64 {
        (self.variance() / self.n.wrapping_sub(1) as f64).sqrt()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rng::AvidaRng;

    const REF: &str = include_str!("../testdata/acc_ref.txt");

    fn row<T: AccValue>(name: &str, a: &Accumulator<T>) -> String {
        let bits = |v: f64| format!("{:016x}", v.to_bits());
        format!(
            "{name} {} | {} {} {} {} {} {}",
            a.count(),
            bits(a.sum().to_f64()),
            bits(a.min().to_f64()),
            bits(a.max().to_f64()),
            bits(a.mean()),
            bits(a.variance()),
            bits(a.std_error())
        )
    }

    /// Mirror of `Acc()` in tools/ref_harness.cc.
    #[test]
    fn matches_cpp() {
        let mut out = Vec::new();
        let mut a = Accumulator::<i32>::new();
        out.push(row("int_empty", &a));
        for v in [5, -3, 7, 7, 0, 100_000, -46_341, 2, 9] {
            a.add(v);
            out.push(row("int_seq", &a));
        }
        a.clear();
        out.push(row("int_cleared", &a));
        a.add(4);
        out.push(row("int_one", &a));

        let mut a = Accumulator::<i32>::new();
        for i in 0..5 {
            a.add(50_000 + i);
            out.push(row("int_overflow", &a));
        }

        let mut d = Accumulator::<f64>::new();
        out.push(row("dbl_empty", &d));
        for v in [-1.5, -2.25, -0.125, 3.75, 1e10, 1e-10, 0.1, 0.2, 0.3] {
            d.add(v);
            out.push(row("dbl_seq", &d));
        }

        let mut r = AvidaRng::new(77);
        let mut a = Accumulator::<i32>::new();
        let mut d = Accumulator::<f64>::new();
        for _ in 0..200 {
            a.add(r.get_int_range(-1000, 1000));
            d.add(r.get_double_range(-5.0, 5.0));
        }
        out.push(row("int_rand", &a));
        out.push(row("dbl_rand", &d));

        let expected: Vec<&str> = REF.lines().collect();
        assert_eq!(out.len(), expected.len());
        for (got, want) in out.iter().zip(expected) {
            assert_eq!(got, want);
        }
    }

    #[test]
    fn edge_cases() {
        let a = Accumulator::<i32>::default();
        assert_eq!((a.count(), a.min(), a.max()), (0, i32::MAX, i32::MIN));
        assert!(a.mean().is_nan() && a.variance().is_nan() && a.std_error().is_nan());
        let mut d = Accumulator::<f64>::new();
        d.add(-3.0);
        assert_eq!(d.max(), f64::MIN_POSITIVE); // QUIRK
        assert_eq!(d.variance(), 0.0);
        assert!(d.std_error().is_nan());
    }
}
