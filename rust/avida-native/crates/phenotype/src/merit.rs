//! Port of `cMerit` (avida-core/source/main/cMerit.h; the C++ type is now a
//! thin wrapper over `rust/avida-rust/src/merit.rs`, which this follows).

/// Merit value with the binary decomposition the integrated scheduler reads.
/// Negative values clamp to 0.
#[derive(Debug, Clone, Copy, Default)]
pub struct Merit {
    bits: i32,
    base: u32,
    offset: i32,
    value: f64,
}

/// C `frexp`: returns (mantissa in [0.5, 1), exponent) with `x = m * 2^e`.
/// Zero, infinities and NaN return themselves with exponent 0.
fn frexp(x: f64) -> (f64, i32) {
    if x == 0.0 || x.is_nan() || x.is_infinite() {
        return (x, 0);
    }
    let bits = x.to_bits();
    let exp_field = ((bits >> 52) & 0x7ff) as i32;
    if exp_field == 0 {
        // Subnormal: scale up by 2^54 first.
        let (m, e) = frexp(x * f64::from_bits(0x4350_0000_0000_0000));
        return (m, e - 54);
    }
    let e = exp_field - 1022;
    let m = f64::from_bits((bits & !(0x7ff_u64 << 52)) | (1022_u64 << 52));
    (m, e)
}

impl Merit {
    const MAX_BITS: i32 = 32;

    fn exp_mult(i: i32) -> f64 {
        2.0_f64.powi(i - 1)
    }

    /// C++: `cMerit::cMerit(double)`
    pub fn new(in_value: f64) -> Self {
        let mut m = Merit::default();
        m.set(in_value);
        m
    }

    /// C++: `cMerit::operator=(double)` / `UpdateValue`
    pub fn set(&mut self, in_value: f64) {
        self.value = if in_value < 0.0 { 0.0 } else { in_value };
        let (mant, bits) = frexp(self.value);
        self.bits = bits;
        self.offset = if self.bits > Self::MAX_BITS {
            self.bits - Self::MAX_BITS
        } else {
            0
        };
        self.base = (mant * Self::exp_mult(self.bits - self.offset) * 2.0) as u32;
    }

    pub fn get_double(&self) -> f64 {
        self.value
    }

    pub fn get_uint(&self) -> u32 {
        self.value as u32
    }

    pub fn get_bit(&self, bit_num: i32) -> i32 {
        if bit_num >= self.offset && bit_num < self.bits {
            ((self.base >> (bit_num - self.offset)) & 1) as i32
        } else {
            0
        }
    }

    pub fn get_num_bits(&self) -> i32 {
        self.bits
    }

    /// C++: `cMerit::CalcFitness`
    pub fn calc_fitness(&self, gestation_time: i32) -> f64 {
        if gestation_time != 0 {
            self.value / f64::from(gestation_time)
        } else {
            0.0
        }
    }

    pub fn clear(&mut self) {
        *self = Merit::default();
    }
}

impl PartialEq for Merit {
    fn eq(&self, other: &Self) -> bool {
        self.value == other.value
    }
}

impl PartialOrd for Merit {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        self.value.partial_cmp(&other.value)
    }
}

impl From<f64> for Merit {
    fn from(v: f64) -> Self {
        Merit::new(v)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn frexp_matches_c() {
        assert_eq!(frexp(0.0), (0.0, 0));
        assert_eq!(frexp(1.0), (0.5, 1));
        assert_eq!(frexp(8.0), (0.5, 4));
        assert_eq!(frexp(0.75), (0.75, 0));
        assert_eq!(frexp(97.0), (97.0 / 128.0, 7));
        let (m, e) = frexp(5e-324);
        assert_eq!((m, e), (0.5, -1073));
    }

    #[test]
    fn bits_of_small_merit() {
        let m = Merit::new(5.0);
        assert_eq!(m.get_num_bits(), 3);
        assert_eq!(
            (m.get_bit(0), m.get_bit(1), m.get_bit(2), m.get_bit(3)),
            (1, 0, 1, 0)
        );
        assert_eq!(m.get_uint(), 5);
    }

    #[test]
    fn large_merit_uses_offset() {
        let m = Merit::new(2.0_f64.powi(40));
        assert_eq!(m.get_num_bits(), 41);
        assert_eq!(m.get_bit(40), 1);
        assert_eq!(m.get_bit(39), 0);
        assert_eq!(m.get_bit(0), 0);
    }

    #[test]
    fn negative_clamps_and_fitness() {
        let m = Merit::new(-3.0);
        assert_eq!(m.get_double(), 0.0);
        assert_eq!(Merit::new(10.0).calc_fitness(4), 2.5);
        assert_eq!(Merit::new(10.0).calc_fitness(0), 0.0);
    }
}
