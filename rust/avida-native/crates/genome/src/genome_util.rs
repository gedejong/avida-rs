//! Port of `cGenomeUtil` (avida-core/source/main/cGenomeUtil.{h,cc}):
//! substring matching and the random genome split / shuffle helpers used by
//! horizontal gene transfer.

use crate::sequence::InstructionSequence;
use avida_apto::AvidaRng;
use std::collections::VecDeque;

/// `cGenomeUtil::substring_match`: `[begin, end)` of a match in the base
/// string (circular when `begin > end`), its edit-distance `cost`, and the
/// base string `size`.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct SubstringMatch {
    /// `begin`
    pub begin: i32,
    /// `end`
    pub end: i32,
    /// `cost`
    pub cost: i32,
    /// `size` (`std::size_t` in C++)
    pub size: usize,
}

impl SubstringMatch {
    /// `substring_match(int b, int e, int c = 0, std::size_t s = 0)`.
    pub fn new(begin: i32, end: i32, cost: i32, size: usize) -> Self {
        SubstringMatch {
            begin,
            end,
            cost,
            size,
        }
    }

    /// `set(b, e, c, s)`.
    pub fn set(&mut self, begin: i32, end: i32, cost: i32, size: usize) {
        *self = SubstringMatch::new(begin, end, cost, size);
    }

    /// `distance()`: distance between begin and end (circular).
    pub fn distance(&self) -> usize {
        if self.begin <= self.end {
            (self.end - self.begin) as usize
        } else {
            // size - begin + end in size_t arithmetic
            self.size
                .wrapping_sub(self.begin as usize)
                .wrapping_add(self.end as i64 as usize)
        }
    }

    /// `resize(n)`: normalise begin/end into `[0, size)` and re-base onto a
    /// string of size `n`, preserving `distance()` from `begin`.
    pub fn resize(&mut self, n: usize) {
        let size = self.size;
        // fixup begin and end if they're negative or point beyond the current size
        if self.begin < 0 {
            self.begin = size.wrapping_sub((-(self.begin as i64)) as usize % size) as i32;
        }
        if self.end < 0 {
            self.end = size.wrapping_sub((-(self.end as i64)) as usize % size) as i32;
        }
        self.begin = ((self.begin as usize) % size) as i32;
        self.end = ((self.end as usize) % size) as i32;

        if size == n {
            return;
        }

        let d = self.distance();
        self.size = n;
        self.end = ((self.begin as usize).wrapping_add(d) % self.size) as i32;
    }

    /// `rotate(r, n)`: shift by `r` around a string of size `n`.
    pub fn rotate(&mut self, r: i32, n: usize) {
        let r = if r < 0 {
            // -((-r) % n) evaluated in size_t, then truncated back to int.
            (((-(r as i64)) as usize % n).wrapping_neg()) as i32
        } else {
            ((r as usize) % n) as i32
        };
        self.begin = self.begin.wrapping_add(r);
        self.end = self.end.wrapping_add(r);
        self.resize(n);
    }
}

/// `std::min_element` over `substring_match` (`operator<` compares cost):
/// the first element with the minimal cost.
fn min_by_cost(items: &[SubstringMatch]) -> usize {
    let mut best = 0;
    for (i, m) in items.iter().enumerate().skip(1) {
        if m.cost < items[best].cost {
            best = i;
        }
    }
    best
}

/// `cGenomeUtil::FindSubstringMatch(base, substring)`: dynamic-programming
/// best (lowest edit cost) match of `substring` inside `base`.
pub fn find_substring_match(
    base: &InstructionSequence,
    substring: &InstructionSequence,
) -> SubstringMatch {
    let rows = substring.size() as usize + 1;
    let cols = base.size() as usize + 1;
    let mut c = vec![SubstringMatch::default(); cols];
    let mut p = vec![SubstringMatch::default(); cols];

    for (j, pj) in p.iter_mut().enumerate().skip(1) {
        pj.begin = j as i32;
    }

    for i in 1..rows {
        c[0].cost = i as i32;
        for j in 1..cols {
            let l = [p[j - 1], p[j], c[j - 1]];
            let mut s = 0usize; // default match is to the upper left.
            if substring.get(i as i32 - 1) == base.get(j as i32 - 1) {
                c[j].cost = l[s].cost;
            } else {
                s = min_by_cost(&l);
                c[j].cost = l[s].cost + 1;
            }
            c[j].begin = l[s].begin;
            c[j].end = j as i32;
        }
        std::mem::swap(&mut c, &mut p);
    }

    let min = min_by_cost(&p);
    p[min].size = base.size() as usize;
    p[min]
}

/// `cGenomeUtil::FindUnbiasedCircularMatch(ctx, base, substring)`: rotate
/// `base` by a random amount (one `GetInt(size)` draw), extend it by its
/// first `substring.size()` sites, match, then undo the rotation.
pub fn find_unbiased_circular_match(
    rng: &mut AvidaRng,
    base: &InstructionSequence,
    substring: &InstructionSequence,
) -> SubstringMatch {
    let mut circ = base.clone();
    let rotate = rng.get_int(circ.size());
    circ.rotate(rotate);

    let head = circ.crop(0, substring.size());
    circ.append_seq(&head);

    let mut location = find_substring_match(&circ, substring);

    location.resize(base.size() as usize);
    location.rotate(-rotate, base.size() as usize);
    location
}

/// `cGenomeUtil::fragment_list_type`.
pub type FragmentList = VecDeque<InstructionSequence>;

/// `cGenomeUtil::RandomSplit(ctx, mean, variance, genome, fragments)`:
/// rotate a copy of `genome` randomly and chop it into fragments whose sizes
/// are `floor(|N(mean, variance)|)` (redrawn while 0), appending them to
/// `fragments`.
///
/// QUIRK (deviation): for an empty genome the C++ loops forever; here
/// nothing is appended.
pub fn random_split(
    rng: &mut AvidaRng,
    mean: f64,
    variance: f64,
    genome: &InstructionSequence,
    fragments: &mut FragmentList,
) {
    let mut g = genome.clone();
    let r = rng.get_int(g.size());
    g.rotate(r);

    let mut remaining_size = g.size();
    if remaining_size <= 0 {
        return;
    }
    let mut i = 0;
    loop {
        let mut fsize = 0;
        while fsize == 0 {
            let draw = rng.get_rand_normal_params(mean, variance).abs().floor() as i32;
            fsize = remaining_size.min(draw);
        }
        fragments.push_back(g.crop(i, i + fsize));
        i += fsize;
        remaining_size -= fsize;
        if remaining_size <= 0 {
            break;
        }
    }
}

/// `cGenomeUtil::RandomShuffle(ctx, genome)`: permute the instructions with
/// libc++'s `std::random_shuffle(first, last, rng)` (`rng(d)` ==
/// `GetUInt(d)`), then assign the result back.
pub fn random_shuffle(rng: &mut AvidaRng, genome: &mut InstructionSequence) {
    let n = genome.size().max(0) as usize;
    let mut idx: Vec<i32> = (0..n as i32).collect();
    libcxx_random_shuffle(&mut idx, rng);
    let mut shuffled = InstructionSequence::with_size(n as i32);
    for (i, &k) in idx.iter().enumerate() {
        shuffled.set(i as i32, genome.get(k));
    }
    genome.assign(&shuffled);
}

/// libc++ `std::random_shuffle(first, last, rand)` with `rand(d)` =
/// `AvidaRng::get_uint(d)` (macOS reference build).
pub fn libcxx_random_shuffle<T>(v: &mut [T], rng: &mut AvidaRng) {
    let len = v.len();
    if len > 1 {
        let mut d = len;
        let last = len - 1;
        let mut first = 0;
        while first < last {
            let i = rng.get_uint(d as u32) as usize;
            if i != 0 {
                v.swap(first, first + i);
            }
            first += 1;
            d -= 1;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn s(x: &str) -> InstructionSequence {
        InstructionSequence::from_string(x)
    }

    #[test]
    fn substring_match_exact_and_fuzzy() {
        let m = find_substring_match(&s("xxabcdxx"), &s("abcd"));
        assert_eq!(m, SubstringMatch::new(2, 6, 0, 8));
        let m = find_substring_match(&s("xxabzdxx"), &s("abcd"));
        assert_eq!(m.cost, 1);
        assert_eq!((m.begin, m.end), (2, 6));
        let m = find_substring_match(&s("abc"), &s(""));
        assert_eq!(m, SubstringMatch::new(0, 0, 0, 3));
    }

    #[test]
    fn match_resize_rotate() {
        let mut m = SubstringMatch::new(-2, 3, 0, 10);
        m.resize(10);
        assert_eq!((m.begin, m.end), (8, 3));
        assert_eq!(m.distance(), 5);

        let mut m = SubstringMatch::new(5, 15, 0, 14);
        m.resize(12); // begin 5, end 15 % 14 = 1, d = 10 -> end 15 % 12
        assert_eq!((m.begin, m.end, m.size), (5, 3, 12));

        let mut m = SubstringMatch::new(1, 4, 0, 10);
        m.rotate(-3, 10);
        assert_eq!((m.begin, m.end), (8, 1));
        m.rotate(13, 10);
        assert_eq!((m.begin, m.end), (1, 4));
    }

    #[test]
    fn circular_match_finds_wrapped_fragment() {
        let base = s("cdefghab");
        let mut rng = AvidaRng::new(7);
        let m = find_unbiased_circular_match(&mut rng, &base, &s("abcd"));
        assert_eq!(m.cost, 0);
        assert_eq!((m.begin, m.end, m.size), (6, 2, 8));
    }

    #[test]
    fn split_and_shuffle_preserve_content() {
        let g = s("abcdefghijklmnopqrstuvwxyz");
        let mut rng = AvidaRng::new(11);
        let mut frags = FragmentList::new();
        random_split(&mut rng, 4.0, 2.0, &g, &mut frags);
        let total: i32 = frags.iter().map(|f| f.size()).sum();
        assert_eq!(total, 26);
        assert!(frags.iter().all(|f| f.size() > 0));

        let mut h = g.clone();
        random_shuffle(&mut rng, &mut h);
        let mut a: Vec<i32> = h.iter().map(|i| i.op()).collect();
        a.sort();
        assert_eq!(a, (0..26).collect::<Vec<_>>());
        assert_ne!(h, g);

        let mut empty = s("");
        random_shuffle(&mut rng, &mut empty);
        assert_eq!(empty.size(), 0);
        random_split(&mut rng, 4.0, 2.0, &empty, &mut frags);
    }

    #[test]
    fn libcxx_shuffle_order() {
        // Same draws as libc++: rand(d) for d = n, n-1, ..., 2.
        let mut r1 = AvidaRng::new(5);
        let mut v = vec![0, 1, 2, 3, 4];
        libcxx_random_shuffle(&mut v, &mut r1);
        let mut r2 = AvidaRng::new(5);
        let mut w = vec![0, 1, 2, 3, 4];
        for (first, d) in (0..4).zip((2..=5).rev()) {
            let i = r2.get_uint(d) as usize;
            if i != 0 {
                w.swap(first, first + i);
            }
        }
        assert_eq!(v, w);
    }
}
