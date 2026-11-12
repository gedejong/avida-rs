//! The per-update statistics rows behind the charts and sparklines.
//!
//! A run can last millions of updates, so the history is bounded: past its
//! capacity it keeps every other row and from then on accepts only every
//! `stride`-th row. The newest row is always the last one.

use avida_session::UpdateRow;

/// Rows kept before thinning.
pub const DEFAULT_CAPACITY: usize = 100_000;

#[derive(Clone, Debug)]
pub struct RowHistory {
    rows: Vec<UpdateRow>,
    capacity: usize,
    stride: u64,
    seen: u64,
    /// The last row is the newest but not kept for good.
    provisional: bool,
    version: u64,
}

impl Default for RowHistory {
    fn default() -> Self {
        RowHistory::with_capacity(DEFAULT_CAPACITY)
    }
}

impl RowHistory {
    pub fn with_capacity(capacity: usize) -> RowHistory {
        RowHistory {
            rows: Vec::new(),
            capacity: capacity.max(4),
            stride: 1,
            seen: 0,
            provisional: false,
            version: 0,
        }
    }

    pub fn extend(&mut self, rows: impl IntoIterator<Item = UpdateRow>) {
        for row in rows {
            self.push(row);
            self.version += 1;
        }
    }

    fn push(&mut self, row: UpdateRow) {
        if self.provisional {
            self.rows.pop();
        }
        let keep = self.seen.is_multiple_of(self.stride);
        self.seen += 1;
        self.rows.push(row);
        self.provisional = !keep;
        if !self.provisional && self.rows.len() > self.capacity {
            let mut i = 0;
            self.rows.retain(|_| {
                i += 1;
                (i - 1) % 2 == 0
            });
            self.stride *= 2;
        }
    }

    pub fn rows(&self) -> &[UpdateRow] {
        &self.rows
    }

    pub fn latest(&self) -> Option<&UpdateRow> {
        self.rows.last()
    }

    pub fn len(&self) -> usize {
        self.rows.len()
    }

    pub fn is_empty(&self) -> bool {
        self.rows.is_empty()
    }

    pub fn clear(&mut self) {
        *self = RowHistory::with_capacity(self.capacity);
    }

    /// Changes whenever the rows do.
    pub fn version(&self) -> u64 {
        self.version
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn row(u: i32) -> UpdateRow {
        UpdateRow {
            update: u,
            ..UpdateRow::default()
        }
    }

    #[test]
    fn keeps_everything_below_capacity() {
        let mut h = RowHistory::with_capacity(10);
        h.extend((0..10).map(row));
        assert_eq!(h.len(), 10);
        assert_eq!(h.latest().map(|r| r.update), Some(9));
    }

    #[test]
    fn thins_evenly_and_ends_at_the_present() {
        let mut h = RowHistory::with_capacity(8);
        h.extend((0..1000).map(row));
        assert!(h.len() <= 9, "{}", h.len());
        assert_eq!(h.latest().map(|r| r.update), Some(999));
        let ups: Vec<i32> = h.rows().iter().map(|r| r.update).collect();
        let body = &ups[..ups.len() - 1];
        let gaps: Vec<i32> = body.windows(2).map(|w| w[1] - w[0]).collect();
        assert!(gaps.windows(2).all(|g| g[0] == g[1]), "{gaps:?}");
        assert_eq!(ups[0], 0);
    }

    #[test]
    fn clear_forgets() {
        let mut h = RowHistory::default();
        h.extend((0..5).map(row));
        h.clear();
        assert!(h.is_empty());
    }
}
