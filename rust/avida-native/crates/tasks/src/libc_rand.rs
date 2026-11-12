//! `rand()` of the C library the reference binary links against (macOS
//! `libSystem`), which two places in Avida call instead of the world RNG:
//! `cTaskLib::Task_ConsumePublicGood` (`std::rand() % prod_set.size()`) and
//! `cPopulationInterface::UpdateRandomResources` (`rand() % size`).
//!
//! The reference never calls `srand`, so every run draws the generator's
//! default sequence from its first value on, and the state is shared by the
//! whole process (population and test CPUs alike). macOS keeps the FreeBSD
//! generator without `USE_WEAK_SEEDING`: the Park-Miller "minimal standard"
//! multiplicative congruential generator (multiplier 16807, modulus
//! 2^31 - 1, Schrage's method), seeded with 1, a zero state replaced by
//! 123459876, and the result reduced modulo `RAND_MAX + 1` (2^31). Its first
//! values are 16807, 282475249, 1622650073.

/// The C library's `rand()` state (`static u_long next = 1`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LibcRand {
    next: u64,
}

impl Default for LibcRand {
    fn default() -> Self {
        LibcRand { next: 1 }
    }
}

impl LibcRand {
    /// The generator as a process starts (no `srand` call).
    pub fn new() -> Self {
        Self::default()
    }

    /// `rand()`: the next value, in `0..=RAND_MAX` (`0x7fffffff`).
    pub fn next_value(&mut self) -> i32 {
        // do_rand(&next), FreeBSD lib/libc/stdlib/rand.c
        if self.next == 0 {
            self.next = 123_459_876;
        }
        let ctx = self.next as i64;
        let hi = ctx / 127_773;
        let lo = ctx % 127_773;
        let mut x = 16_807 * lo - 2_836 * hi;
        if x < 0 {
            x += 0x7fff_ffff;
        }
        self.next = x as u64;
        (x % (0x7fff_ffff_i64 + 1)) as i32
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Values printed by a C program calling `rand()` on macOS.
    #[test]
    fn matches_the_macos_sequence() {
        let mut r = LibcRand::new();
        let first: Vec<i32> = (0..10).map(|_| r.next_value()).collect();
        assert_eq!(
            first,
            vec![
                16807, 282475249, 1622650073, 984943658, 1144108930, 470211272, 101027544,
                1457850878, 1458777923, 2007237709
            ]
        );
        let mut sum: i64 = 0;
        for _ in 0..10_000_000 {
            sum += i64::from(r.next_value());
        }
        assert_eq!(sum, 10_737_818_690_934_398);
        assert_eq!(r.next_value(), 1_978_698_165);
    }
}
