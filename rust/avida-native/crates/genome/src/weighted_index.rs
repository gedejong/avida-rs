//! Port of `cOrderedWeightedIndex`
//! (avida-core/source/tools/cOrderedWeightedIndex.{h,cc} at the pre-Rust
//! commit ef5c6d948; the current C++ forwards to
//! rust/avida-rust/src/ordered_weighted_index.rs, which has the same body).
//!
//! A list of `(value, weight)` items with cumulative weights, searched by
//! binary search for the item whose weight interval contains a position.
//! Cumulative weights are plain additions (`cum[i-1] + w`), and the interval
//! test is `cum[mid] - weight[mid] <= pos < cum[mid]`, as in the C++.
//!
//! Differences from ef5c6d948, all on paths that are undefined behaviour
//! there and unreachable from `cInstSet::GetRandomInst` (which draws
//! `pos` in `[0, total)`):
//! - `GetTotalWeight()` on an empty index reads `cum_weight[-1]`; 0.0 here.
//! - `Lookup` has no "not found" base case: a position below 0 recurses on
//!   `(0, -1)` forever (stack overflow), one at or above the total reads
//!   `cum_weight[size]`. Both return -1 here.
//! - `GetWeight` / `GetValue` out of range read past the array; 0.0 / -1 here.

/// `cOrderedWeightedIndex`.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct OrderedWeightedIndex {
    item_weight: Vec<f64>,
    cum_weight: Vec<f64>,
    item_value: Vec<i32>,
}

impl OrderedWeightedIndex {
    /// `cOrderedWeightedIndex()`.
    pub fn new() -> Self {
        Self::default()
    }

    /// `SetWeight(value, weight)`.
    ///
    /// QUIRK: this appends a new item; it never updates an existing value's
    /// weight (so `cInstSet::SetRedundancy` adds a second item).
    pub fn set_weight(&mut self, value: i32, in_weight: f64) {
        self.item_value.push(value);
        self.item_weight.push(in_weight);
        let next = match self.cum_weight.last() {
            Some(prev) => *prev + in_weight,
            None => in_weight,
        };
        self.cum_weight.push(next);
    }

    /// `GetWeight(id)` (0.0 when out of range).
    pub fn weight(&self, id: i32) -> f64 {
        if id < 0 {
            return 0.0;
        }
        self.item_weight.get(id as usize).copied().unwrap_or(0.0)
    }

    /// `GetValue(id)` (-1 when out of range).
    pub fn value(&self, id: i32) -> i32 {
        if id < 0 {
            return -1;
        }
        self.item_value.get(id as usize).copied().unwrap_or(-1)
    }

    /// `GetTotalWeight()` (0.0 when empty).
    pub fn total_weight(&self) -> f64 {
        self.cum_weight.last().copied().unwrap_or(0.0)
    }

    /// `GetSize()`.
    pub fn size(&self) -> i32 {
        self.item_weight.len() as i32
    }

    /// `FindPosition(position)`: the value whose interval
    /// `[cum - weight, cum)` contains `position`, or -1.
    pub fn find_position(&self, position: f64) -> i32 {
        if self.item_weight.is_empty() {
            return -1;
        }
        self.lookup(position, 0, self.item_weight.len() - 1)
    }

    /// `Lookup(weight, ndxA, ndxE)`.
    fn lookup(&self, weight: f64, ndx_a: usize, ndx_e: usize) -> i32 {
        if ndx_a > ndx_e {
            return -1;
        }
        let mid = ndx_a + (ndx_e - ndx_a) / 2;
        if self.cum_weight[mid] - self.item_weight[mid] <= weight && self.cum_weight[mid] > weight {
            return self.item_value[mid];
        }
        if self.cum_weight[mid] > weight {
            if mid == 0 {
                return -1;
            }
            self.lookup(weight, ndx_a, mid - 1)
        } else {
            self.lookup(weight, mid + 1, ndx_e)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lookup_values() {
        let mut h = OrderedWeightedIndex::new();
        assert_eq!(h.size(), 0);
        assert_eq!(h.find_position(0.0), -1);
        assert_eq!(h.total_weight(), 0.0);
        h.set_weight(10, 1.0);
        h.set_weight(20, 2.0);
        h.set_weight(30, 3.0);
        assert_eq!(h.size(), 3);
        assert_eq!(h.total_weight(), 6.0);
        assert_eq!(h.value(0), 10);
        assert_eq!(h.weight(2), 3.0);
        assert_eq!(h.find_position(0.2), 10);
        assert_eq!(h.find_position(1.0), 20);
        assert_eq!(h.find_position(1.4), 20);
        assert_eq!(h.find_position(3.5), 30);
        assert_eq!(h.find_position(7.0), -1);
        assert_eq!(h.find_position(-1.0), -1);
        assert_eq!(h.value(-1), -1);
        assert_eq!(h.weight(9), 0.0);
        let mut c = h.clone();
        c.set_weight(40, 4.0);
        assert_eq!((c.size(), c.total_weight(), h.size()), (4, 10.0, 3));
    }
}
