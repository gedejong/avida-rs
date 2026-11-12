//! The hash map the script sources were written against.
//!
//! `cSymbolTable`, `cASLibrary` and `cLocalDict` use the pre-2012 `Apto::Map`
//! API (`Find`, `HasEntry`, `SetValue`, `Add`, `NearMatch`, `GetKeys`, ...).
//! That class no longer exists anywhere in the tree (the script sources do
//! not compile against the current Apto), so its iteration order is not
//! recoverable from the repository. This port models it as a 23 bucket
//! chained hash table: buckets in index order, insertion order inside a
//! bucket, order preserving removal. Iteration order is observable only
//! through `keys()`/`values()` of a dict and through ties of the
//! "possible match" hint of undefined-name errors.

pub const HASH_SIZE: i32 = 23;

pub trait MapKey: Clone {
    /// `HashKey<K>(key, 23)`.
    fn hash23(&self) -> i32;
    fn key_eq(&self, other: &Self) -> bool;
}

impl MapKey for Vec<u8> {
    /// `Apto::HashKey<String>`: byte sum (sign extended) modulo the table size.
    fn hash23(&self) -> i32 {
        let mut h: u32 = 0;
        for &c in self {
            h = h.wrapping_add(c as i8 as i32 as u32);
        }
        (h % HASH_SIZE as u32) as i32
    }
    fn key_eq(&self, other: &Self) -> bool {
        self == other
    }
}

pub struct OldMap<K: MapKey, V> {
    buckets: Vec<Vec<(K, V)>>,
    n: usize,
}

impl<K: MapKey, V> Default for OldMap<K, V> {
    fn default() -> Self {
        Self::new()
    }
}

impl<K: MapKey, V> OldMap<K, V> {
    pub fn new() -> Self {
        OldMap {
            buckets: (0..HASH_SIZE).map(|_| Vec::new()).collect(),
            n: 0,
        }
    }

    pub fn len(&self) -> usize {
        self.n
    }

    pub fn is_empty(&self) -> bool {
        self.n == 0
    }

    fn b(&self, k: &K) -> usize {
        k.hash23() as usize
    }

    pub fn find(&self, k: &K) -> Option<&V> {
        self.buckets[self.b(k)]
            .iter()
            .find(|e| e.0.key_eq(k))
            .map(|e| &e.1)
    }

    pub fn has(&self, k: &K) -> bool {
        self.find(k).is_some()
    }

    /// `Add`: appends (callers only add absent keys).
    pub fn add(&mut self, k: K, v: V) {
        let b = self.b(&k);
        self.buckets[b].push((k, v));
        self.n += 1;
    }

    /// `SetValue`: replaces the value of an existing key, else appends.
    pub fn set_value(&mut self, k: K, v: V) {
        let b = self.b(&k);
        for e in self.buckets[b].iter_mut() {
            if e.0.key_eq(&k) {
                e.1 = v;
                return;
            }
        }
        self.buckets[b].push((k, v));
        self.n += 1;
    }

    /// `Remove`: returns the removed value.
    pub fn remove(&mut self, k: &K) -> Option<V> {
        let b = self.b(k);
        let pos = self.buckets[b].iter().position(|e| e.0.key_eq(k))?;
        self.n -= 1;
        Some(self.buckets[b].remove(pos).1)
    }

    pub fn clear(&mut self) {
        for b in self.buckets.iter_mut() {
            b.clear();
        }
        self.n = 0;
    }

    pub fn iter(&self) -> impl Iterator<Item = &(K, V)> {
        self.buckets.iter().flat_map(|b| b.iter())
    }
}

/// `Apto::EditDistance` (including its inner-loop pointer swap, so the
/// result is not the Levenshtein distance). QUIRK: the C++ reads
/// uninitialised heap memory for longer second strings; zeros are assumed.
pub fn edit_distance(s1: &[u8], s2: &[u8]) -> i32 {
    let sz1 = s1.len();
    let sz2 = s2.len();
    if sz1 == 0 {
        return sz2 as i32;
    }
    if sz2 == 0 {
        return sz1 as i32;
    }
    let mut cur = vec![0i32; sz2 + 1];
    let mut prev = vec![0i32; sz2 + 1];
    for (i, p) in prev.iter_mut().enumerate() {
        *p = i as i32;
    }
    for (i, &c1) in s1.iter().enumerate() {
        cur[0] = i as i32 + 1;
        for j in 0..sz2 {
            let a = (1 + cur[j]).min(1 + prev[1 + j]);
            let b = prev[j] + if c1 == s2[j] { 0 } else { 1 };
            cur[j + 1] = a.min(b);
            std::mem::swap(&mut cur, &mut prev);
        }
    }
    prev[sz2]
}

/// `Apto::NearMatch` over the keys: the first key (in map order) with the
/// smallest edit distance below `v.len()`; empty when none.
pub fn near_match<V>(m: &OldMap<Vec<u8>, V>, v: &[u8]) -> Vec<u8> {
    let mut best: Vec<u8> = Vec::new();
    let mut best_dist = v.len() as i32;
    for (k, _) in m.iter() {
        let d = edit_distance(v, k);
        if d < best_dist {
            best_dist = d;
            best = k.clone();
        }
    }
    best
}
