//! Bit-exact Rust port of the Avida Knuth subtractive RNG (`Apto::RNG::AvidaRNG`,
//! libs/apto/include/apto/rng/AvidaRNG.h, libs/apto/src/rng/AvidaRNG.cc).
//!
//! The generator is a lag-55 subtractive PRNG as described in Knuth TAOCP Vol 2,
//! with constants `UPPER_BOUND = 1_000_000_000` and `MAX_SEED = 161_803_398`.
//! All derived methods (`GetDouble`, `GetRandNormal`, `GetRandPoisson`, `GetRandBinomial`,
//! `Choose`, ...) are ported from the C++ `Apto::Random` base class
//! (libs/apto/include/apto/core/Random.h, libs/apto/src/core/Random.cc), with the
//! same draw order.
//!
//! Floating-point contraction: the reference binary is built by Apple clang for
//! arm64 with the default `-ffp-contract=on`, which fuses `a * b + c` written as
//! one C++ expression into a single `fmadd`. `GetDouble(min, max)` and
//! `GetRandNormal(mean, variance)` are such expressions and use `mul_add` here.
//!
//! Verified against `tools/ref_harness.cc rng` (`testdata/rng_ref.txt`).

// ---------------------------------------------------------------------------
// Constants (matching C++ statics)
// ---------------------------------------------------------------------------

const UPPER_BOUND: u32 = 1_000_000_000;
const MAX_SEED: i32 = 161_803_398;
const FACTOR: f64 = 1.0 / UPPER_BOUND as f64;

/// Statistical-approximation thresholds matching `Random.cc`.
const BINOMIAL_TO_NORMAL: u32 = 50;
const BINOMIAL_TO_POISSON: u32 = 1000;

// ---------------------------------------------------------------------------
// Core struct
// ---------------------------------------------------------------------------

/// Rust-native Avida RNG matching the `Apto::RNG::AvidaRNG` algorithm bit-for-bit.
#[derive(Debug, Clone)]
pub struct AvidaRng {
    m_inext: i32,
    m_inextp: i32,
    m_ma: [i32; 56],
    m_orig_seed: i32,
    m_seed: i32,
    m_rand_norm_exprv: f64,
}

impl AvidaRng {
    /// Create a new RNG seeded with `seed`.
    /// If `seed < 0`, a time-and-pid-based seed is chosen automatically.
    pub fn new(seed: i32) -> Self {
        let mut rng = Self {
            m_inext: 0,
            m_inextp: 0,
            m_ma: [0; 56],
            m_orig_seed: 0,
            m_seed: 0,
            m_rand_norm_exprv: 0.0,
        };
        rng.reset_seed(seed);
        rng
    }

    // -- seed accessors -----------------------------------------------------

    #[inline]
    pub fn seed(&self) -> i32 {
        self.m_seed
    }

    #[inline]
    pub fn original_seed(&self) -> i32 {
        self.m_orig_seed
    }

    // -- ResetSeed ----------------------------------------------------------

    /// Corresponds to `Random::ResetSeed`.
    pub fn reset_seed(&mut self, new_seed: i32) {
        self.m_orig_seed = new_seed;
        let mut effective = new_seed;
        if effective < 0 {
            effective = Self::random_seed();
        }
        self.m_seed = effective;
        if self.m_seed > MAX_SEED {
            self.m_seed %= MAX_SEED;
        }
        self.reset();
        self.m_rand_norm_exprv = -self.get_double().ln();
    }

    // -- Core PRNG ----------------------------------------------------------

    /// Knuth subtractive `reset()`.
    fn reset(&mut self) {
        self.m_inext = 0;
        self.m_inextp = 0;
        self.m_ma = [0; 56];

        let mut mj = MAX_SEED - self.m_seed;
        mj %= UPPER_BOUND as i32;
        self.m_ma[55] = mj;
        let mut mk: i32 = 1;

        for i in 1..55 {
            let j = (21 * i) % 55;
            self.m_ma[j] = mk;
            mk = mj - mk;
            if mk < 0 {
                mk += UPPER_BOUND as i32;
            }
            mj = self.m_ma[j];
        }

        for _ in 0..4 {
            for j in 1..55 {
                self.m_ma[j] -= self.m_ma[1 + (j + 30) % 55];
                if self.m_ma[j] < 0 {
                    self.m_ma[j] += UPPER_BOUND as i32;
                }
            }
        }

        self.m_inext = 0;
        self.m_inextp = 31;
    }

    /// Knuth subtractive `getNext()` — returns value in `[0, UPPER_BOUND)`.
    #[inline]
    fn get_next(&mut self) -> u32 {
        self.m_inext += 1;
        if self.m_inext == 56 {
            self.m_inext = 0;
        }
        self.m_inextp += 1;
        if self.m_inextp == 56 {
            self.m_inextp = 0;
        }
        let mut mj = self.m_ma[self.m_inext as usize] - self.m_ma[self.m_inextp as usize];
        if mj < 0 {
            mj += UPPER_BOUND as i32;
        }
        self.m_ma[self.m_inext as usize] = mj;
        mj as u32
    }

    // -- Derived samplers (matching Random base class) ----------------------

    #[inline]
    pub fn get_double(&mut self) -> f64 {
        self.get_next() as f64 * FACTOR
    }

    #[inline]
    pub fn get_double_max(&mut self, max: f64) -> f64 {
        self.get_double() * max
    }

    /// `GetDouble(min, max)`: `GetDouble() * (max - min) + min`.
    ///
    /// The reference build (Apple clang, arm64, default `-ffp-contract=on`)
    /// compiles this inline expression to a single `fmadd`, so it is rounded
    /// once: `mul_add` reproduces it bit for bit. `get_uint_range`,
    /// `get_int_range` and `get_int_full` go through here.
    #[inline]
    pub fn get_double_range(&mut self, min: f64, max: f64) -> f64 {
        self.get_double().mul_add(max - min, min)
    }

    #[inline]
    pub fn get_uint(&mut self, max: u32) -> u32 {
        self.get_double_max(max as f64) as u32
    }

    #[inline]
    pub fn get_int(&mut self, max: i32) -> i32 {
        self.get_double_max(max as f64) as i32
    }

    #[inline]
    pub fn get_int_range(&mut self, min: i32, max: i32) -> i32 {
        self.get_double_range(min as f64, max as f64).floor() as i32
    }

    /// Bernoulli trial: returns `true` with probability `p`.
    #[inline]
    pub fn p(&mut self, prob: f64) -> bool {
        (self.get_next() as f64) < (prob * UPPER_BOUND as f64)
    }

    /// Draw from unit normal via rejection method with cached exponential.
    pub fn get_rand_normal(&mut self) -> f64 {
        let mut exp_rv2;
        loop {
            exp_rv2 = -self.get_double().ln();
            self.m_rand_norm_exprv -= (exp_rv2 - 1.0) * (exp_rv2 - 1.0) / 2.0;
            if self.m_rand_norm_exprv > 0.0 {
                break;
            }
            self.m_rand_norm_exprv = -self.get_double().ln();
        }
        if self.p(0.5) {
            exp_rv2
        } else {
            -exp_rv2
        }
    }

    /// `GetRandNormal(mean, variance)`: `mean + GetRandNormal() * sqrt(variance)`,
    /// fused into one `fmadd` by the reference build (see `get_double_range`).
    pub fn get_rand_normal_params(&mut self, mean: f64, variance: f64) -> f64 {
        self.get_rand_normal().mul_add(variance.sqrt(), mean)
    }

    /// `Random::GetRandPoisson(double mean)` (Random.cc): rejection method;
    /// `u32::MAX` when `exp(-mean)` underflows.
    pub fn get_rand_poisson(&mut self, mean: f64) -> u32 {
        let mut k: u32 = 0;
        let a = (-mean).exp();
        // The C++ draws `u` before the underflow check, so the early return
        // still consumes one number.
        let mut u = self.get_double();
        if a <= 0.0 {
            return u32::MAX;
        }
        while u >= a {
            u *= self.get_double();
            k += 1;
        }
        k
    }

    /// `Random::GetRandPoisson(double n, double p)` (Random.h): Poisson
    /// approximation of a binomial; for `p > 0.5` the complement is drawn.
    pub fn get_rand_poisson_np(&mut self, n: f64, prob: f64) -> u32 {
        if prob > 0.5 {
            return (n as u32).wrapping_sub(self.get_rand_poisson(n * (1.0 - prob)));
        }
        self.get_rand_poisson(n * prob)
    }

    /// `Random::GetFullRandBinomial(double n, double p)` (Random.h): exact
    /// binomial via Bernoulli trials.
    pub fn get_full_rand_binomial(&mut self, n: f64, prob: f64) -> u32 {
        let mut k: u32 = 0;
        // QUIRK: the C++ loop is `for (unsigned int i = 0; i < n; ++i)` with a
        // double `n`, so a fractional `n` runs ceil(n) trials, not floor(n).
        let mut i: u32 = 0;
        while (i as f64) < n {
            if self.p(prob) {
                k += 1;
            }
            i = i.wrapping_add(1);
        }
        k
    }

    /// Binomial with automatic approximation (normal / Poisson / exact).
    pub fn get_rand_binomial(&mut self, n: f64, prob: f64) -> u32 {
        if n * prob * (1.0 - prob) >= BINOMIAL_TO_NORMAL as f64 {
            return (self.get_rand_normal_params(n * prob, n * prob * (1.0 - prob)) + 0.5) as u32;
        }
        if n >= BINOMIAL_TO_POISSON as f64 {
            let k = self.get_rand_poisson_np(n, prob);
            if k < u32::MAX {
                return k;
            }
        }
        self.get_full_rand_binomial(n, prob)
    }

    // -- Remaining `Apto::Random` surface (Random.h) -------------------------

    /// `Random::UpperBound()`.
    #[inline]
    pub fn upper_bound(&self) -> u32 {
        UPPER_BOUND
    }

    /// `Random::MaxSeed()`.
    #[inline]
    pub fn max_seed(&self) -> i32 {
        MAX_SEED
    }

    /// `Random::GetUInt()` with no argument.
    ///
    /// QUIRK: the C++ returns the constant `INT_MAX` and draws nothing.
    #[inline]
    pub fn get_uint_full(&self) -> u32 {
        i32::MAX as u32
    }

    /// `Random::GetUInt(unsigned int min, unsigned int max)`.
    #[inline]
    pub fn get_uint_range(&mut self, min: u32, max: u32) -> u32 {
        self.get_double_range(min as f64, max as f64) as u32
    }

    /// `Random::GetInt()` with no argument: uniform over the whole `int` range.
    #[inline]
    pub fn get_int_full(&mut self) -> i32 {
        self.get_double_range(i32::MIN as f64, i32::MAX as f64) as i32
    }

    /// `Random::Choose(int num_in, A& out_array)`: fill `out` with distinct
    /// values drawn from `[0, num_in)`, in draw order, by rejection.
    ///
    /// When `num_in == out.len()` the identity permutation is written and no
    /// random numbers are drawn. As in the C++, `num_in < out.len()` never
    /// terminates; callers must not pass it (the C++ asserts).
    pub fn choose(&mut self, num_in: i32, out: &mut [i32]) {
        debug_assert!(num_in >= 0 && num_in as usize >= out.len());
        if num_in as usize == out.len() {
            for (i, slot) in out.iter_mut().enumerate() {
                *slot = i as i32;
            }
            return;
        }

        let mut choice_count = 0;
        while choice_count < out.len() {
            let next = self.get_int(num_in);
            if !out[..choice_count].contains(&next) {
                out[choice_count] = next;
                choice_count += 1;
            }
        }
    }

    // -- Auto-seed helper ---------------------------------------------------

    /// Matches `Random::getRandomSeed()` — `time ^ (pid << 8)`.
    fn random_seed() -> i32 {
        let seed_time = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs() as i32)
            .unwrap_or(0);
        let seed_pid = std::process::id() as i32;
        seed_time ^ (seed_pid << 8)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const REF: &str = include_str!("../testdata/rng_ref.txt");

    fn reference(name: &str) -> Vec<&'static str> {
        REF.lines()
            .find(|l| l.split_whitespace().next() == Some(name))
            .unwrap_or_else(|| panic!("no reference line {name}"))
            .split_whitespace()
            .skip(1)
            .collect()
    }

    fn bits(v: f64) -> String {
        format!("{:016x}", v.to_bits())
    }

    fn check<T: ToString>(name: &str, got: Vec<T>) {
        let got: Vec<String> = got.iter().map(T::to_string).collect();
        assert_eq!(got, reference(name), "{name}");
    }

    /// Mirror of `Rng()` in tools/ref_harness.cc, in the same draw order.
    #[test]
    fn matches_cpp() {
        let seed_lines: Vec<&str> = REF.lines().filter(|l| l.starts_with("seed ")).collect();
        let seeds = [0, 1, 42, 161_803_398, 161_803_399, 2_147_483_647];
        assert_eq!(seed_lines.len(), seeds.len());
        for (seed, line) in seeds.iter().zip(seed_lines) {
            let mut r = AvidaRng::new(*seed);
            let mut got = format!("seed {} {} {}", seed, r.seed(), r.original_seed());
            for _ in 0..64 {
                got.push(' ');
                got.push_str(&bits(r.get_double()));
            }
            assert_eq!(got, line);
        }

        let mut r = AvidaRng::new(12345);
        check("uint", (0..200).map(|i| r.get_uint(1000 + i)).collect());
        check(
            "uint_range",
            (0..200).map(|i| r.get_uint_range(5, 5000 + i)).collect(),
        );
        check("int", (0..200).map(|i| r.get_int(i - 100)).collect());
        check(
            "int_range",
            (0..200).map(|i| r.get_int_range(-50 - i, 50 + i)).collect(),
        );
        check("int_full", (0..200).map(|_| r.get_int_full()).collect());
        check(
            "p",
            (0..400)
                .map(|i| i32::from(r.p(f64::from(i) / 400.0)))
                .collect(),
        );
        check(
            "normal",
            (0..200).map(|_| bits(r.get_rand_normal())).collect(),
        );
        check(
            "normal_mv",
            (0..200)
                .map(|i| bits(r.get_rand_normal_params(f64::from(i) * 0.5, 1.0 + f64::from(i))))
                .collect(),
        );
        check(
            "poisson",
            (0..200)
                .map(|i| r.get_rand_poisson(f64::from(i) * 0.37))
                .collect(),
        );
        let mut big = Vec::new();
        for i in 0..20 {
            big.push(r.get_rand_poisson(800.0 + f64::from(i)));
            big.push(r.get_uint(1_000_000));
        }
        check("poisson_big", big);
        check(
            "poisson_np",
            (0..200)
                .map(|i| {
                    r.get_rand_poisson_np(10.0 + f64::from(i), f64::from(i % 10) / 10.0 + 0.05)
                })
                .collect(),
        );
        check(
            "full_binomial",
            (0..200)
                .map(|i| r.get_full_rand_binomial(f64::from(i) * 0.75, f64::from(i % 7) / 7.0))
                .collect(),
        );
        let mut binom = Vec::new();
        for i in 0..300 {
            let fi = f64::from(i);
            let n = if i % 3 == 0 {
                5.0 + fi
            } else if i % 3 == 1 {
                37.0_f64.mul_add(fi, 1000.0)
            } else {
                3.0_f64.mul_add(fi, 20.0)
            };
            let p = if i % 3 == 1 {
                0.0005 * f64::from((i % 11) + 1)
            } else {
                f64::from((i % 13) + 1) / 14.0
            };
            binom.push(r.get_rand_binomial(n, p));
        }
        check("binomial", binom);
        let mut chosen = Vec::new();
        for i in 1..=40 {
            let mut out = vec![0; (i % 8 + 1) as usize];
            r.choose(i % 8 + 1 + (i % 5) * 3, &mut out);
            chosen.push("|".to_string());
            chosen.extend(out.iter().map(i32::to_string));
        }
        check("choose", chosen);
        check("tail", (0..20).map(|_| bits(r.get_double())).collect());
    }

    #[test]
    fn surface() {
        let mut r = AvidaRng::new(5);
        assert_eq!(
            (r.upper_bound(), r.max_seed()),
            (1_000_000_000, 161_803_398)
        );
        assert_eq!(r.get_uint_full(), i32::MAX as u32);
        let mut c = r.clone();
        assert_eq!(r.get_double(), c.get_double());
        r.reset_seed(9);
        assert_eq!((r.seed(), r.original_seed()), (9, 9));
        assert_eq!(r.get_double(), AvidaRng::new(9).get_double());
        let mut out = [0; 4];
        r.choose(4, &mut out); // identity, no draws
        assert_eq!(out, [0, 1, 2, 3]);
        // Negative seed: time/pid based, original seed kept.
        let t = AvidaRng::new(-1);
        assert_eq!(t.original_seed(), -1);
        assert!(t.seed() >= 0);
    }
}
