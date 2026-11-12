//! Port of `Apto::Map<K, V>` with its default storage policy
//! `DefaultHashBTree` = `HashBTree<K, V, 23>`
//! (libs/apto/include/apto/core/Map.h, MapStorage.h; string hashing and
//! comparison from String.h).
//!
//! Iteration visits the 23 hash buckets in order and, inside a bucket, the
//! entries in array order (insertion order, rearranged by removals). Avida
//! output depends on it: `cPopulation::SavePopulation` writes the current
//! genotypes in the order of an `Apto::Map<int, sGroupInfo*>` keyed by
//! genotype id, and `CladeArbiter` iterates its `Apto::Map<String, CladePtr>`.
//!
//! `HashBTree::Remove` has two pointer-update slips (the "tossup" case
//! updates the successor's parent's right link instead of its left one, and
//! the remove-last-entry cases can leave a stale left link); they are kept.
//! Where they would make a C++ tree search loop forever, the searches here
//! stop after visiting as many nodes as the bucket holds and report "not
//! found" (QUIRK).

/// Number of buckets of `DefaultHashBTree`.
pub const HASH_FACTOR: usize = 23;

/// Key behaviour of `HashKey<K, 23>` and the `<` / `>` operators.
pub trait AptoKey: Clone {
    /// `HashKey<K, 23>::Hash(key)`.
    fn apto_hash(&self) -> usize;
    /// `key < other`.
    fn apto_lt(&self, other: &Self) -> bool;
    /// `key > other`.
    fn apto_gt(&self, other: &Self) -> bool;
}

/// `HashKey<int, HashFactor>`: `abs(key % HashFactor)`.
impl AptoKey for i32 {
    fn apto_hash(&self) -> usize {
        (self % HASH_FACTOR as i32).unsigned_abs() as usize
    }
    fn apto_lt(&self, other: &Self) -> bool {
        self < other
    }
    fn apto_gt(&self, other: &Self) -> bool {
        self > other
    }
}

/// `BasicString::Compare(str)` with `char` signed (arm64 Apple ABI).
fn apto_string_compare(this: &str, other: &str) -> i32 {
    let a = this.as_bytes();
    let b = other.as_bytes();
    let at = |s: &[u8], i: usize| -> i8 { s.get(i).map(|&c| c as i8).unwrap_or(0) };
    let mut i = 0;
    while i < a.len() && at(b, i) != 0 && at(a, i) == at(b, i) {
        i += 1;
    }
    if i == a.len() && at(b, i) == 0 {
        return 0;
    }
    if i < a.len() && at(b, i) < at(a, i) {
        return 1;
    }
    -1
}

/// `HashKey<BasicString, HashFactor>`: sum of the characters as `unsigned
/// int` modulo the factor; ordering by `BasicString::Compare`.
impl AptoKey for String {
    fn apto_hash(&self) -> usize {
        let mut h: u32 = 0;
        for &c in self.as_bytes() {
            h = h.wrapping_add(c as i8 as i32 as u32);
        }
        (h % HASH_FACTOR as u32) as usize
    }
    fn apto_lt(&self, other: &Self) -> bool {
        apto_string_compare(self, other) < 0
    }
    fn apto_gt(&self, other: &Self) -> bool {
        apto_string_compare(self, other) > 0
    }
}

/// `HashBTree::Entry`.
#[derive(Clone, Debug)]
struct Entry<K, V> {
    key: K,
    value: V,
    left: i32,
    right: i32,
    parent: i32,
}

/// `Apto::Map<K, V>` (default storage).
#[derive(Clone, Debug)]
pub struct AptoMap<K: AptoKey, V> {
    table: Vec<Vec<Entry<K, V>>>,
    size: i32,
}

impl<K: AptoKey, V: Clone + Default> Default for AptoMap<K, V> {
    fn default() -> Self {
        Self::new()
    }
}

impl<K: AptoKey, V: Clone + Default> AptoMap<K, V> {
    /// An empty map.
    pub fn new() -> Self {
        AptoMap {
            table: (0..HASH_FACTOR).map(|_| Vec::new()).collect(),
            size: 0,
        }
    }

    /// `GetSize()`.
    pub fn len(&self) -> i32 {
        self.size
    }

    /// `GetSize() == 0`.
    pub fn is_empty(&self) -> bool {
        self.size == 0
    }

    /// `Clear()`.
    pub fn clear(&mut self) {
        for t in &mut self.table {
            t.clear();
        }
        self.size = 0;
    }

    /// `HashBTree::Find(key)`: index of the entry in its bucket.
    fn find_idx(&self, key: &K) -> Option<(usize, usize)> {
        let hash = key.apto_hash();
        let tbl = &self.table[hash];
        if tbl.is_empty() {
            return None;
        }
        let mut cur: i32 = 0;
        let mut steps = 0;
        while cur >= 0 && (cur as usize) < tbl.len() && steps <= tbl.len() {
            steps += 1;
            let e = &tbl[cur as usize];
            if key.apto_lt(&e.key) {
                cur = e.left;
            } else if key.apto_gt(&e.key) {
                cur = e.right;
            } else {
                return Some((hash, cur as usize));
            }
        }
        None
    }

    /// `Has(key)`.
    pub fn has(&self, key: &K) -> bool {
        self.find_idx(key).is_some()
    }

    /// Const `Get(key, out)` / `Find(key)`: no insertion.
    pub fn find(&self, key: &K) -> Option<&V> {
        self.find_idx(key).map(|(h, i)| &self.table[h][i].value)
    }

    /// Mutable `Find(key)`.
    pub fn find_mut(&mut self, key: &K) -> Option<&mut V> {
        let (h, i) = self.find_idx(key)?;
        Some(&mut self.table[h][i].value)
    }

    /// Non-const `Get(key)` / `operator[]`: inserts a default value when
    /// the key is missing.
    pub fn get_or_insert(&mut self, key: &K) -> &mut V {
        let hash = key.apto_hash();
        if self.table[hash].is_empty() {
            self.table[hash].push(Entry {
                key: key.clone(),
                value: V::default(),
                left: -1,
                right: -1,
                parent: -1,
            });
            self.size += 1;
            return &mut self.table[hash][0].value;
        }
        let len = self.table[hash].len();
        let mut cur: i32 = 0;
        let mut past: i32 = 0;
        let mut steps = 0;
        loop {
            steps += 1;
            if cur < 0 || cur as usize >= len || steps > len {
                break; // QUIRK guard, see module docs
            }
            let e = &self.table[hash][cur as usize];
            if key.apto_lt(&e.key) {
                past = cur;
                cur = e.left;
                if cur >= 0 {
                    continue;
                }
            } else if key.apto_gt(&e.key) {
                past = cur;
                cur = e.right;
                if cur >= 0 {
                    continue;
                }
            } else {
                return &mut self.table[hash][cur as usize].value;
            }
            break;
        }
        // Add the new entry to the tree.
        let tbl = &mut self.table[hash];
        let new_idx = tbl.len();
        tbl.push(Entry {
            key: key.clone(),
            value: V::default(),
            left: -1,
            right: -1,
            parent: past,
        });
        self.size += 1;
        let p = past as usize;
        if key.apto_lt(&tbl[p].key) {
            tbl[p].left = new_idx as i32;
        } else {
            tbl[p].right = new_idx as i32;
        }
        &mut tbl[new_idx].value
    }

    /// `Set(key, value)`: `Get(key) = value`.
    pub fn set(&mut self, key: &K, value: V) {
        *self.get_or_insert(key) = value;
    }

    /// Non-const `GetWithDefault(key, default)`: inserts `default` when the
    /// key is missing (QUIRK of the C++ API) and returns the stored value.
    pub fn get_with_default(&mut self, key: &K, default: V) -> &mut V {
        if self.has(key) {
            return self.get_or_insert(key);
        }
        let v = self.get_or_insert(key);
        *v = default;
        v
    }

    /// `HashBTree::Remove(key)`, transliterated with its slips.
    pub fn remove(&mut self, key: &K) -> bool {
        let hash = key.apto_hash();
        let tbl = &mut self.table[hash];
        if tbl.is_empty() {
            return false;
        }
        let n = tbl.len();
        let ok = |i: i32| i >= 0 && (i as usize) < n;
        let mut cur: i32 = 0;
        let mut past: i32 = 0;
        let mut steps = 0;
        loop {
            steps += 1;
            if !ok(cur) || steps > n {
                return false;
            }
            let c = cur as usize;
            if key.apto_lt(&tbl[c].key) {
                past = cur;
                cur = tbl[c].left;
                if cur >= 0 {
                    continue;
                }
            } else if key.apto_gt(&tbl[c].key) {
                past = cur;
                cur = tbl[c].right;
                if cur >= 0 {
                    continue;
                }
            } else {
                break;
            }
            return false;
        }
        if remove_found(tbl, cur, past).is_none() {
            // Only reachable on a tree the C++ slips have corrupted; the C++
            // would index out of bounds here.
            return false;
        }
        let last = tbl.len() - 1;
        tbl.truncate(last);
        self.size -= 1;
        true
    }

    /// Iteration in `Begin()` / `Keys()` / `Values()` order.
    pub fn iter(&self) -> impl Iterator<Item = (&K, &V)> {
        self.table
            .iter()
            .flat_map(|t| t.iter().map(|e| (&e.key, &e.value)))
    }

    /// Keys in iteration order.
    pub fn keys(&self) -> impl Iterator<Item = &K> {
        self.iter().map(|(k, _)| k)
    }

    /// Values in iteration order.
    pub fn values(&self) -> impl Iterator<Item = &V> {
        self.iter().map(|(_, v)| v)
    }

    /// Mutable values in iteration order.
    pub fn values_mut(&mut self) -> impl Iterator<Item = &mut V> {
        self.table
            .iter_mut()
            .flat_map(|t| t.iter_mut().map(|e| &mut e.value))
    }
}

/// The node rewiring of `HashBTree::Remove` once the key was found at `cur`
/// (reached from `past`). Does not shrink the array. `None` when a link
/// points outside the bucket.
fn remove_found<K: Clone, V: Clone>(tbl: &mut [Entry<K, V>], cur: i32, past: i32) -> Option<()> {
    let n = tbl.len();
    let ix = |i: i32| -> Option<usize> {
        if i >= 0 && (i as usize) < n {
            Some(i as usize)
        } else {
            None
        }
    };
    let last_idx = n as i32 - 1;
    let c = ix(cur)?;
    let last = ix(last_idx)?;

    // "Move last node to the position of `to`" block shared by three cases.
    fn move_last<K: Clone, V: Clone>(
        tbl: &mut [Entry<K, V>],
        to: usize,
        last: usize,
    ) -> Option<()> {
        let n = tbl.len();
        let ix = |i: i32| -> Option<usize> {
            if i >= 0 && (i as usize) < n {
                Some(i as usize)
            } else {
                None
            }
        };
        tbl[to] = tbl[last].clone();
        let parent = ix(tbl[last].parent)?;
        if tbl[parent].left == last as i32 {
            tbl[parent].left = to as i32;
        } else {
            tbl[parent].right = to as i32;
        }
        if let Some(l) = ix(tbl[to].left) {
            tbl[l].parent = to as i32;
        }
        if let Some(r) = ix(tbl[to].right) {
            tbl[r].parent = to as i32;
        }
        Some(())
    }

    if cur != last_idx {
        if tbl[c].left < 0 && tbl[c].right < 0 {
            // Leaf node.
            let p = ix(past)?;
            if tbl[p].left == cur {
                tbl[p].left = -1;
            } else {
                tbl[p].right = -1;
            }
            move_last(tbl, c, last)?;
        } else if tbl[c].left < 0 {
            // All child nodes greater than current: find next right node.
            let mut sub = tbl[c].right;
            let mut steps = 0;
            while tbl[ix(sub)?].left > 0 && steps <= n {
                sub = tbl[ix(sub)?].left;
                steps += 1;
            }
            let s = ix(sub)?;
            let right_idx = tbl[c].right;
            tbl[c].key = tbl[s].key.clone();
            tbl[c].value = tbl[s].value.clone();
            let sp = ix(tbl[s].parent)?;
            tbl[sp].left = tbl[s].right;
            if let Some(sr) = ix(tbl[s].right) {
                tbl[sr].parent = tbl[s].parent;
            }
            tbl[c].right = if sub != right_idx { right_idx } else { -1 };
            if sub != last_idx {
                move_last(tbl, s, last)?;
            }
        } else if tbl[c].right < 0 {
            // All child nodes less than current: find next left node.
            let mut sub = tbl[c].left;
            let mut steps = 0;
            while tbl[ix(sub)?].right > 0 && steps <= n {
                sub = tbl[ix(sub)?].right;
                steps += 1;
            }
            let s = ix(sub)?;
            let left_idx = tbl[c].left;
            tbl[c].key = tbl[s].key.clone();
            tbl[c].value = tbl[s].value.clone();
            let sp = ix(tbl[s].parent)?;
            tbl[sp].right = tbl[s].left;
            if let Some(sl) = ix(tbl[s].left) {
                tbl[sl].parent = tbl[s].parent;
            }
            tbl[c].left = if sub != left_idx { left_idx } else { -1 };
            if sub != last_idx {
                move_last(tbl, s, last)?;
            }
        } else {
            // Tossup.
            let mut sub = tbl[c].right;
            let mut steps = 0;
            while tbl[ix(sub)?].left > 0 && steps <= n {
                sub = tbl[ix(sub)?].left;
                steps += 1;
            }
            let s = ix(sub)?;
            tbl[c].key = tbl[s].key.clone();
            tbl[c].value = tbl[s].value.clone();
            // QUIRK: updates the parent's right link (the C++ does so too).
            let sp = ix(tbl[s].parent)?;
            tbl[sp].right = tbl[s].right;
            if let Some(sr) = ix(tbl[s].right) {
                tbl[sr].parent = tbl[s].parent;
            }
            if tbl[c].right == sub {
                tbl[c].right = -1;
            }
            if tbl[c].left == sub {
                tbl[c].left = -1;
            }
            if sub != last_idx {
                move_last(tbl, s, last)?;
            }
        }
    } else {
        let parent_idx = tbl[last].parent;
        if let Some(p) = ix(parent_idx) {
            if tbl[last].left < 0 && tbl[last].right < 0 {
                if tbl[p].left == last_idx {
                    tbl[p].left = -1;
                } else {
                    tbl[p].right = -1;
                }
            } else if tbl[last].left < 0 {
                let mut sub = tbl[last].right;
                let mut steps = 0;
                while tbl[ix(sub)?].left > 0 && steps <= n {
                    sub = tbl[ix(sub)?].left;
                    steps += 1;
                }
                let s = ix(sub)?;
                if tbl[p].left == last_idx {
                    tbl[p].left = sub;
                } else {
                    tbl[p].right = sub;
                }
                if sub != tbl[last].right {
                    if let Some(sr) = ix(tbl[s].right) {
                        tbl[sr].parent = tbl[s].parent;
                        let sp = ix(tbl[s].parent)?;
                        tbl[sp].left = tbl[s].right;
                    }
                }
                tbl[s].parent = parent_idx;
                if tbl[last].right != sub {
                    tbl[s].right = tbl[last].right;
                    if let Some(sr) = ix(tbl[s].right) {
                        tbl[sr].parent = sub;
                    }
                }
                tbl[s].left = tbl[last].left;
                if let Some(sl) = ix(tbl[s].left) {
                    tbl[sl].parent = sub;
                }
            } else {
                // `right < 0` and the tossup case run the same code.
                let mut sub = tbl[last].left;
                let mut steps = 0;
                while tbl[ix(sub)?].right > 0 && steps <= n {
                    sub = tbl[ix(sub)?].right;
                    steps += 1;
                }
                let s = ix(sub)?;
                if tbl[p].left == last_idx {
                    tbl[p].left = sub;
                } else {
                    tbl[p].right = sub;
                }
                if sub != tbl[last].left {
                    if let Some(sl) = ix(tbl[s].left) {
                        tbl[sl].parent = tbl[s].parent;
                        let sp = ix(tbl[s].parent)?;
                        tbl[sp].right = tbl[s].left;
                    }
                }
                tbl[s].parent = parent_idx;
                if tbl[last].left != sub {
                    tbl[s].left = tbl[last].left;
                    if let Some(sl) = ix(tbl[s].left) {
                        tbl[sl].parent = sub;
                    }
                }
                tbl[s].right = tbl[last].right;
                if let Some(sr) = ix(tbl[s].right) {
                    tbl[sr].parent = sub;
                }
            }
        }
    }
    Some(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn int_keys_bucket_then_insertion_order() {
        let mut m: AptoMap<i32, i32> = AptoMap::new();
        for k in [30, 7, 23, 1, 46, 24, -5] {
            m.set(&k, k * 10);
        }
        let keys: Vec<i32> = m.keys().copied().collect();
        // buckets: 0:{23,46} 1:{1,24} 5:{-5} 7:{30,7}
        assert_eq!(keys, vec![23, 46, 1, 24, -5, 30, 7]);
        assert_eq!(m.find(&24), Some(&240));
        assert_eq!(m.find(&2), None);
        assert_eq!(m.len(), 7);
    }

    #[test]
    fn string_hash_and_compare() {
        assert_eq!("genotype".to_string().apto_hash(), 1);
        assert_eq!("clade".to_string().apto_hash(), 22);
        assert!("abc".to_string().apto_lt(&"abd".to_string()));
        assert!("ab".to_string().apto_lt(&"abc".to_string()));
        assert!("abc".to_string().apto_gt(&"ab".to_string()));
        assert!(!"x".to_string().apto_lt(&"x".to_string()));
    }

    /// Replays tools/apto_map_ref.cc (random Set / Remove on the real Apto
    /// header) against testdata/apto_map_ref.txt.
    #[test]
    fn matches_cpp_random_operations() {
        const REF: &str = include_str!("../testdata/apto_map_ref.txt");
        let mut s: u32 = 4242;
        let mut rnd = |m: u32| {
            s = s.wrapping_mul(1_103_515_245).wrapping_add(12345);
            (s >> 8) % m
        };
        let mut mi: AptoMap<i32, i32> = AptoMap::new();
        let mut ms: AptoMap<String, i32> = AptoMap::new();
        // The C++ run segfaults a few steps after the recorded ones (the
        // Remove slips corrupt a bucket), so only the complete lines it
        // printed are compared.
        let total = REF.lines().count() as i32;
        assert!(total > 300);
        let mut lines = REF.lines();
        for step in 0..total {
            let op = rnd(3);
            let k = rnd(120) as i32 - 20;
            let sk = format!("c{}", rnd(90));
            let (ri, rs) = if op < 2 {
                mi.set(&k, step);
                ms.set(&sk, step);
                (1, 1)
            } else {
                (i32::from(mi.remove(&k)), i32::from(ms.remove(&sk)))
            };
            let mut got = format!("{step} {op} {k} {sk} {ri} {rs} {} {}:", mi.len(), ms.len());
            for (k, v) in mi.iter() {
                got.push_str(&format!(" {k}={v}"));
            }
            got.push_str(" |");
            for (k, v) in ms.iter() {
                got.push_str(&format!(" {k}={v}"));
            }
            assert_eq!(Some(got.as_str()), lines.next(), "step {step}");
        }
    }
}
