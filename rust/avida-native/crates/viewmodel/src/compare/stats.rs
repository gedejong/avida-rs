//! The statistics of the arm comparison, written by hand (no statistics
//! crate is in the workspace).
//!
//! * [`spread`]: the median and the quartiles of a sample. A quantile is the
//!   linear interpolation between the two nearest order statistics at
//!   position `(n - 1) p` (the default of NumPy and R's type 7).
//! * [`mann_whitney`]: the two-sided Mann-Whitney U test. U of a sample is
//!   its rank sum (ties get the mean of the ranks they span) minus
//!   `n (n + 1) / 2`. When `n_a + n_b` is at most [`EXACT_MAX_N`] the p-value
//!   is exact: the share of all ways to give `n_a` of the pooled midranks to
//!   the first sample whose U lies at least as far from its mean `n_a n_b / 2`
//!   as the observed one (a permutation test, so ties are handled exactly).
//!   Above that it is the normal approximation with the tie correction
//!   `var = n_a n_b / 12 * ((N + 1) - sum(t^3 - t) / (N (N - 1)))` and a
//!   continuity correction of 0.5: `p = erfc(z / sqrt 2)` with
//!   `z = (|U - mean| - 0.5) / sd`.
//! * [`fisher_exact`]: Fisher's exact test on a 2 x 2 table, two-sided: the
//!   sum of the hypergeometric probabilities of every table with the same
//!   margins that is no more probable than the observed one (a relative
//!   tolerance of 1e-7 on the probabilities, so that tables of equal
//!   probability are all counted).
//!
//! The p-values are descriptive: the view shows them with the sample sizes
//! and does not call anything significant.

/// Up to this many pooled values the Mann-Whitney p-value is exact.
pub const EXACT_MAX_N: usize = 40;

/// Median and quartiles.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Spread {
    pub n: usize,
    pub min: f64,
    pub q1: f64,
    pub median: f64,
    pub q3: f64,
    pub max: f64,
}

/// The quantile `p` (0 to 1) of an ascending sample, by linear
/// interpolation at `(n - 1) p`.
pub fn quantile(sorted: &[f64], p: f64) -> Option<f64> {
    if sorted.is_empty() {
        return None;
    }
    let pos = (sorted.len() - 1) as f64 * p.clamp(0.0, 1.0);
    let lo = pos.floor() as usize;
    let hi = pos.ceil() as usize;
    let frac = pos - lo as f64;
    Some(sorted[lo] + (sorted[hi] - sorted[lo]) * frac)
}

/// The spread of a sample (NaN values are ignored); `None` when empty.
pub fn spread(values: &[f64]) -> Option<Spread> {
    let mut v: Vec<f64> = values.iter().copied().filter(|x| !x.is_nan()).collect();
    if v.is_empty() {
        return None;
    }
    v.sort_by(f64::total_cmp);
    Some(Spread {
        n: v.len(),
        min: v[0],
        q1: quantile(&v, 0.25)?,
        median: quantile(&v, 0.5)?,
        q3: quantile(&v, 0.75)?,
        max: v[v.len() - 1],
    })
}

/// How a p-value was obtained.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Method {
    /// The permutation distribution of the midranks, enumerated.
    Exact,
    /// The normal approximation with tie and continuity corrections.
    Normal,
}

impl Method {
    pub fn label(self) -> &'static str {
        match self {
            Method::Exact => "exact",
            Method::Normal => "normal approximation",
        }
    }
}

/// The outcome of a Mann-Whitney U test.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct MannWhitney {
    pub n_a: usize,
    pub n_b: usize,
    /// U of the first sample (the number of pairs in which it is larger,
    /// ties counting one half).
    pub u_a: f64,
    pub u_b: f64,
    pub p: f64,
    pub method: Method,
}

/// Doubled midranks of the pooled sample (an integer, so that comparisons
/// are exact), in the order of `a` then `b`, and the sizes of the tie
/// groups.
fn doubled_midranks(a: &[f64], b: &[f64]) -> (Vec<u32>, Vec<usize>) {
    let mut all: Vec<(f64, usize)> = a.iter().chain(b).copied().zip(0..).collect();
    all.sort_by(|x, y| x.0.total_cmp(&y.0));
    let mut ranks = vec![0_u32; all.len()];
    let mut ties = Vec::new();
    let mut i = 0;
    while i < all.len() {
        let mut j = i;
        while j + 1 < all.len() && all[j + 1].0 == all[i].0 {
            j += 1;
        }
        // Ranks i + 1 ..= j + 1: their mean doubled is (i + 1) + (j + 1).
        let doubled = (i + j + 2) as u32;
        for item in &all[i..=j] {
            ranks[item.1] = doubled;
        }
        ties.push(j - i + 1);
        i = j + 1;
    }
    (ranks, ties)
}

/// The two-sided Mann-Whitney U test of two samples (NaN values are
/// ignored); `None` when either is empty. See the module comment for the
/// definitions.
pub fn mann_whitney(a: &[f64], b: &[f64]) -> Option<MannWhitney> {
    let a: Vec<f64> = a.iter().copied().filter(|x| !x.is_nan()).collect();
    let b: Vec<f64> = b.iter().copied().filter(|x| !x.is_nan()).collect();
    if a.is_empty() || b.is_empty() {
        return None;
    }
    let (n_a, n_b) = (a.len(), b.len());
    let (ranks, ties) = doubled_midranks(&a, &b);
    let rank_sum2: u64 = ranks[..n_a].iter().map(|&r| u64::from(r)).sum();
    // 2 U_a = 2 R_a - n_a (n_a + 1).
    let u2_a = rank_sum2 as i64 - (n_a * (n_a + 1)) as i64;
    let dev2 = (u2_a - (n_a * n_b) as i64).unsigned_abs();
    let u_a = u2_a as f64 / 2.0;
    let u_b = (n_a * n_b) as f64 - u_a;
    let (p, method) = if n_a + n_b <= EXACT_MAX_N {
        (exact_p(&ranks, n_a, n_b, dev2), Method::Exact)
    } else {
        (normal_p(n_a, n_b, u_a, &ties), Method::Normal)
    };
    Some(MannWhitney {
        n_a,
        n_b,
        u_a,
        u_b,
        p,
        method,
    })
}

/// The normal-approximation p-value of the Mann-Whitney test, whatever the
/// size (for tests and for callers that want it).
pub fn mann_whitney_normal(a: &[f64], b: &[f64]) -> Option<f64> {
    let a: Vec<f64> = a.iter().copied().filter(|x| !x.is_nan()).collect();
    let b: Vec<f64> = b.iter().copied().filter(|x| !x.is_nan()).collect();
    if a.is_empty() || b.is_empty() {
        return None;
    }
    let (ranks, ties) = doubled_midranks(&a, &b);
    let rank_sum2: u64 = ranks[..a.len()].iter().map(|&r| u64::from(r)).sum();
    let u2_a = rank_sum2 as i64 - (a.len() * (a.len() + 1)) as i64;
    Some(normal_p(a.len(), b.len(), u2_a as f64 / 2.0, &ties))
}

fn normal_p(n_a: usize, n_b: usize, u_a: f64, ties: &[usize]) -> f64 {
    let (na, nb) = (n_a as f64, n_b as f64);
    let n = na + nb;
    let tie_term: f64 = ties
        .iter()
        .map(|&t| {
            let t = t as f64;
            t * t * t - t
        })
        .sum();
    let var = na * nb / 12.0 * ((n + 1.0) - tie_term / (n * (n - 1.0)));
    if var <= 0.0 {
        return 1.0;
    }
    let dev = ((u_a - na * nb / 2.0).abs() - 0.5).max(0.0);
    (erfc(dev / var.sqrt() / std::f64::consts::SQRT_2)).clamp(0.0, 1.0)
}

/// The share of the ways to choose `n_a` of the pooled doubled midranks
/// whose U is at least `dev2 / 2` from its mean.
fn exact_p(ranks: &[u32], n_a: usize, n_b: usize, dev2: u64) -> f64 {
    let total: usize = ranks.iter().map(|&r| r as usize).sum();
    // ways[j][s]: choices of j ranks with doubled sum s.
    let mut ways = vec![vec![0.0_f64; total + 1]; n_a + 1];
    ways[0][0] = 1.0;
    for (seen, &r) in ranks.iter().enumerate() {
        let r = r as usize;
        for j in (1..=(seen + 1).min(n_a)).rev() {
            for s in (r..=total).rev() {
                let add = ways[j - 1][s - r];
                ways[j][s] += add;
            }
        }
    }
    let (mut hit, mut all) = (0.0_f64, 0.0_f64);
    for (s, &w) in ways[n_a].iter().enumerate() {
        if w == 0.0 {
            continue;
        }
        let u2 = s as i64 - (n_a * (n_a + 1)) as i64;
        let dev = (u2 - (n_a * n_b) as i64).unsigned_abs();
        all += w;
        if dev >= dev2 {
            hit += w;
        }
    }
    (hit / all).min(1.0)
}

/// The complementary error function, to about 1e-14 (the Taylor series of
/// erf below 2.5, a continued fraction above).
pub fn erfc(x: f64) -> f64 {
    if x < 0.0 {
        return 2.0 - erfc(-x);
    }
    if x < 2.5 {
        // erf x = 2 / sqrt(pi) * sum (-1)^n x^(2n+1) / (n! (2n + 1)).
        let mut term = x;
        let mut sum = x;
        for n in 1..200 {
            term *= -x * x / n as f64;
            let add = term / (2 * n + 1) as f64;
            sum += add;
            if add.abs() < 1e-17 * sum.abs() {
                break;
            }
        }
        1.0 - 2.0 / std::f64::consts::PI.sqrt() * sum
    } else {
        // erfc x = exp(-x^2) / sqrt(pi) * 1 / (x + (1/2) / (x + 1 / (x + (3/2) / (x + ...)))).
        let mut f = x;
        for k in (1..=80).rev() {
            f = x + (k as f64 / 2.0) / f;
        }
        (-x * x).exp() / (std::f64::consts::PI.sqrt() * f)
    }
}

/// `ln n!` for `n` up to `max`, as a table.
fn ln_factorials(max: usize) -> Vec<f64> {
    let mut t = vec![0.0; max + 1];
    for i in 2..=max {
        t[i] = t[i - 1] + (i as f64).ln();
    }
    t
}

/// Fisher's exact test, two-sided, on the table `[[a, b], [c, d]]`. The
/// p-value is 1 for an empty table.
pub fn fisher_exact(a: u32, b: u32, c: u32, d: u32) -> f64 {
    let (a, b, c, d) = (a as usize, b as usize, c as usize, d as usize);
    let n = a + b + c + d;
    if n == 0 {
        return 1.0;
    }
    let (r1, r2, c1) = (a + b, c + d, a + c);
    let lf = ln_factorials(n);
    let ln_choose = |n: usize, k: usize| lf[n] - lf[k] - lf[n - k];
    let ln_p = |x: usize| ln_choose(r1, x) + ln_choose(r2, c1 - x) - ln_choose(n, c1);
    let lo = c1.saturating_sub(r2);
    let hi = r1.min(c1);
    let observed = ln_p(a);
    let limit = observed + 1e-7;
    let p: f64 = (lo..=hi)
        .map(ln_p)
        .filter(|&l| l <= limit)
        .map(f64::exp)
        .sum();
    p.min(1.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn close(a: f64, b: f64, tol: f64) {
        assert!((a - b).abs() <= tol, "{a} is not within {tol} of {b}");
    }

    #[test]
    fn quartiles_interpolate_linearly() {
        // Hand computed: n = 5, quartile positions 1, 2 and 3 are exact.
        let s = spread(&[5.0, 1.0, 3.0, 2.0, 4.0]).unwrap();
        assert_eq!(
            (s.q1, s.median, s.q3, s.min, s.max),
            (2.0, 3.0, 4.0, 1.0, 5.0)
        );
        // n = 4: positions 0.75, 1.5 and 2.25 between 1,2,3,4.
        let s = spread(&[1.0, 2.0, 3.0, 4.0]).unwrap();
        close(s.q1, 1.75, 1e-12);
        close(s.median, 2.5, 1e-12);
        close(s.q3, 3.25, 1e-12);
        assert_eq!(spread(&[f64::NAN]), None);
        let one = spread(&[7.0]).unwrap();
        assert_eq!((one.q1, one.median, one.q3), (7.0, 7.0, 7.0));
    }

    #[test]
    fn erfc_matches_known_values() {
        close(erfc(0.0), 1.0, 1e-15);
        // erfc(1) = 0.157299207050285, erfc(2) = 0.004677734981047,
        // erfc(3) = 2.209049699858544e-5 (tables).
        close(erfc(1.0), 0.157299207050285, 1e-13);
        close(erfc(2.0), 0.004677734981047, 1e-13);
        close(erfc(3.0), 2.209049699858544e-5, 1e-16);
        close(erfc(-1.0), 1.842700792949715, 1e-13);
        // The two branches agree where they meet.
        close(erfc(2.4999999), erfc(2.5000001), 1e-9);
    }

    #[test]
    fn mann_whitney_exact_without_ties() {
        // 1 2 3 against 4 5 6: only the two extreme splits of C(6,3) = 20
        // are as far from the mean, so p = 2 / 20.
        let r = mann_whitney(&[1.0, 2.0, 3.0], &[4.0, 5.0, 6.0]).unwrap();
        assert_eq!((r.u_a, r.u_b), (0.0, 9.0));
        assert_eq!(r.method, Method::Exact);
        close(r.p, 0.1, 1e-12);
        // 1 2 against 3 4 5: C(5,2) = 10 splits, two as extreme: 0.2.
        close(
            mann_whitney(&[1.0, 2.0], &[3.0, 4.0, 5.0]).unwrap().p,
            0.2,
            1e-12,
        );
        // The test is symmetric in its arguments.
        close(
            mann_whitney(&[3.0, 4.0, 5.0], &[1.0, 2.0]).unwrap().p,
            0.2,
            1e-12,
        );
    }

    #[test]
    fn mann_whitney_exact_interleaved_samples_are_not_unusual() {
        // 1 3 5 against 2 4 6: ranks of a are 1, 3, 5 so R = 9, U = 3, the
        // mean is 4.5, the deviation 1.5. The distribution of U for 3 and 3
        // (counts of U = 0..=9: 1 1 2 3 3 3 3 2 1 1) puts 1 + 1 + 2 + 3 =
        // 7 at U <= 3, and 7 at U >= 6, so p = 14 / 20 = 0.7.
        let r = mann_whitney(&[1.0, 3.0, 5.0], &[2.0, 4.0, 6.0]).unwrap();
        assert_eq!(r.u_a, 3.0);
        close(r.p, 0.7, 1e-12);
    }

    #[test]
    fn mann_whitney_exact_with_ties() {
        // a = 1 2 2, b = 2 3 4: pooled 1 2 2 2 3 4, midranks 1 3 3 3 5 6,
        // R_a = 7, U_a = 1, mean 4.5, deviation 3.5. Of the 20 choices of
        // three midranks for a, those with a sum of 7 (U = 1: the rank 1 and
        // two of the three 3s, 3 choices) and those with a sum of 14 (U = 8:
        // the 5, the 6 and one of the three 3s, 3 choices) are that far
        // out, so p = 6 / 20 = 0.3 (checked by enumeration).
        let r = mann_whitney(&[1.0, 2.0, 2.0], &[2.0, 3.0, 4.0]).unwrap();
        assert_eq!(r.u_a, 1.0);
        assert_eq!(r.method, Method::Exact);
        close(r.p, 0.3, 1e-12);
    }

    #[test]
    fn identical_samples_give_p_one() {
        close(
            mann_whitney(&[2.0, 2.0, 2.0], &[2.0, 2.0, 2.0]).unwrap().p,
            1.0,
            1e-12,
        );
        assert!(mann_whitney(&[], &[1.0]).is_none());
        assert!(mann_whitney(&[f64::NAN], &[1.0]).is_none());
    }

    #[test]
    fn normal_approximation_with_tie_correction() {
        // a = 1 2 3 4, b = 3 5 6 7: ranks 1 2 3.5 3.5 5 6 7 8, R_a = 11.5,
        // U_a = 1.5, mean 8, one tie group of 2, so
        // var = 16 / 12 * (9 - 6 / 56) = 11.857142857, sd = 3.443419,
        // z = (6.5 - 0.5) / sd = 1.742462, p = erfc(z / sqrt 2) = 0.08143
        // (computed with Python's math.erfc).
        let p = mann_whitney_normal(&[1.0, 2.0, 3.0, 4.0], &[3.0, 5.0, 6.0, 7.0]).unwrap();
        close(p, 0.08143, 5e-5);
        // Without the tie correction term the variance would be 12 and
        // p slightly larger; the correction is applied.
        let no_ties = mann_whitney_normal(&[1.0, 2.0, 3.0, 4.0], &[3.5, 5.0, 6.0, 7.0]).unwrap();
        assert!(no_ties < p + 0.01 && no_ties != p);
    }

    #[test]
    fn large_samples_use_the_normal_approximation() {
        let a: Vec<f64> = (0..25).map(f64::from).collect();
        let b: Vec<f64> = (0..25).map(|x| f64::from(x) + 10.5).collect();
        let r = mann_whitney(&a, &b).unwrap();
        assert_eq!(r.method, Method::Normal);
        assert!(r.p < 0.05);
        let same = mann_whitney(&a, &a).unwrap();
        close(same.p, 1.0, 1e-9);
    }

    #[test]
    fn fisher_matches_hand_computed_tables() {
        // [[3, 0], [0, 3]]: C(6,3) = 20 tables, the two extreme ones are as
        // improbable: 2 / 20.
        close(fisher_exact(3, 0, 0, 3), 0.1, 1e-12);
        // [[8, 2], [1, 5]]: margins 10 and 6 against 9 and 7, C(16,9) = 11440,
        // the tables with a = 3, 8, 9 have 120, 270 and 10 ways:
        // 400 / 11440 = 0.034965.
        close(fisher_exact(8, 2, 1, 5), 400.0 / 11440.0, 1e-9);
        // Symmetric under swapping the rows.
        close(fisher_exact(1, 5, 8, 2), 400.0 / 11440.0, 1e-9);
        // Equal groups: p = 1.
        close(fisher_exact(2, 3, 2, 3), 1.0, 1e-12);
        close(fisher_exact(0, 0, 0, 0), 1.0, 1e-12);
        // [[10, 0], [0, 10]]: 2 / C(20,10) = 2 / 184756.
        close(fisher_exact(10, 0, 0, 10), 2.0 / 184_756.0, 1e-12);
    }
}
