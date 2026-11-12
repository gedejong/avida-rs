//! The accumulated per-update statistics rows behind the charts, the
//! sparklines and the CSV export.
//!
//! A run can last millions of updates, so the history is bounded: when it
//! exceeds its capacity it keeps every other row and from then on accepts
//! only every `stride`-th row, so the kept rows stay evenly spaced. The
//! newest row is always the last one, also when it would not be kept for
//! good, so plots and stat cards end at the present.

use std::sync::Arc;

use avida_session::UpdateRow;

/// Rows kept before thinning (about 50 MB of rows with a few tasks).
pub const DEFAULT_CAPACITY: usize = 200_000;

#[derive(Clone, Debug)]
pub struct RowHistory {
    /// Shared so an export can take a snapshot without copying the rows
    /// on the UI thread; a write while a snapshot is out copies once.
    rows: Arc<Vec<UpdateRow>>,
    capacity: usize,
    /// Keep one row in `stride`.
    stride: usize,
    /// Rows seen since the last row kept for good.
    since_kept: usize,
    /// The last row is the newest one but not kept for good: the next row
    /// replaces it unless it is kept itself.
    provisional_tail: bool,
    /// Changes whenever `rows` does (for caches).
    version: u64,
    /// Changes whenever rows already kept change or go away (thinning,
    /// clearing), not when rows are only appended or the provisional
    /// newest row is replaced.
    epoch: u64,
    seen: u64,
}

impl Default for RowHistory {
    fn default() -> Self {
        RowHistory::with_capacity(DEFAULT_CAPACITY)
    }
}

impl RowHistory {
    pub fn with_capacity(capacity: usize) -> RowHistory {
        RowHistory {
            rows: Arc::new(Vec::new()),
            capacity: capacity.max(4),
            stride: 1,
            since_kept: 0,
            provisional_tail: false,
            version: 0,
            epoch: 0,
            seen: 0,
        }
    }

    /// Appends rows in update order.
    pub fn extend(&mut self, rows: impl IntoIterator<Item = UpdateRow>) {
        let mut changed = false;
        for row in rows {
            self.push(row);
            changed = true;
        }
        if changed {
            self.version += 1;
        }
    }

    fn push(&mut self, row: UpdateRow) {
        self.seen += 1;
        let rows = Arc::make_mut(&mut self.rows);
        if self.provisional_tail {
            rows.pop();
        }
        self.since_kept += 1;
        rows.push(row);
        if self.since_kept >= self.stride {
            self.since_kept = 0;
            self.provisional_tail = false;
        } else {
            self.provisional_tail = true;
        }
        if self.permanent_len() > self.capacity {
            self.thin();
        }
    }

    fn permanent_len(&self) -> usize {
        self.rows.len() - usize::from(self.provisional_tail)
    }

    /// Keeps the rows at even positions (and the last permanent one), and
    /// doubles the stride.
    fn thin(&mut self) {
        let rows = Arc::make_mut(&mut self.rows);
        let tail = if self.provisional_tail {
            rows.pop()
        } else {
            None
        };
        let last = rows.len().saturating_sub(1);
        let mut i = 0;
        rows.retain(|_| {
            let keep = i % 2 == 0 || i == last;
            i += 1;
            keep
        });
        rows.extend(tail);
        self.stride *= 2;
        self.epoch += 1;
    }

    /// The kept rows, oldest first; the last one is the newest row seen.
    pub fn rows(&self) -> &[UpdateRow] {
        &self.rows
    }

    /// The kept rows as a shared snapshot: cheap, and safe to hand to a
    /// thread.
    pub fn snapshot(&self) -> Arc<Vec<UpdateRow>> {
        Arc::clone(&self.rows)
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

    /// One row in this many is kept (1 until the first thinning).
    pub fn stride(&self) -> usize {
        self.stride
    }

    /// Rows received in total.
    pub fn seen(&self) -> u64 {
        self.seen
    }

    pub fn version(&self) -> u64 {
        self.version
    }

    /// See the field: caches that only fold in appended rows start over
    /// when this changes. Only the newest row can change without it.
    pub fn epoch(&self) -> u64 {
        self.epoch
    }

    /// The rows whose update lies in `lo..=hi`, with one more row on each
    /// side so a line drawn through them reaches the edges; as an index
    /// range into [`RowHistory::rows`].
    pub fn index_range(&self, lo: f64, hi: f64) -> std::ops::Range<usize> {
        let start = self
            .rows
            .partition_point(|r| f64::from(r.update) < lo)
            .saturating_sub(1);
        let end =
            (self.rows.partition_point(|r| f64::from(r.update) <= hi) + 1).min(self.rows.len());
        start..end.max(start)
    }

    pub fn clear(&mut self) {
        let version = self.version + 1;
        let epoch = self.epoch + 1;
        *self = RowHistory::with_capacity(self.capacity);
        self.version = version;
        self.epoch = epoch;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn row(update: i32) -> UpdateRow {
        UpdateRow {
            update,
            orgs: update * 2,
            ..UpdateRow::default()
        }
    }

    #[test]
    fn keeps_everything_below_capacity() {
        let mut h = RowHistory::with_capacity(100);
        h.extend((0..50).map(row));
        assert_eq!(h.len(), 50);
        assert_eq!(h.stride(), 1);
        assert_eq!(h.latest().map(|r| r.update), Some(49));
        assert_eq!(h.version(), 1);
    }

    #[test]
    fn thinning_stays_bounded_ordered_and_ends_at_the_newest_row() {
        let mut h = RowHistory::with_capacity(16);
        for u in 0..1000 {
            h.extend([row(u)]);
            assert!(h.len() <= 17, "len {} at {u}", h.len());
            assert_eq!(h.latest().map(|r| r.update), Some(u));
        }
        let updates: Vec<i32> = h.rows().iter().map(|r| r.update).collect();
        assert_eq!(updates[0], 0, "the first update survives thinning");
        assert!(updates.windows(2).all(|w| w[0] < w[1]), "{updates:?}");
        assert!(h.stride() >= 64);
        assert_eq!(h.seen(), 1000);
        // Evenly spaced: no gap more than twice the stride.
        let max_gap = updates.windows(2).map(|w| w[1] - w[0]).max().unwrap();
        assert!(max_gap as usize <= 2 * h.stride(), "gap {max_gap}");
    }

    #[test]
    fn the_epoch_changes_only_when_kept_rows_change() {
        let mut h = RowHistory::with_capacity(8);
        h.extend((0..8).map(row));
        assert_eq!(h.epoch(), 0, "appending keeps the epoch");
        h.extend((8..9).map(row));
        assert_eq!(h.epoch(), 1, "thinning changes it");
        h.clear();
        assert_eq!(h.epoch(), 2, "clearing changes it");
    }

    #[test]
    fn index_ranges_reach_one_row_past_each_edge() {
        let mut h = RowHistory::with_capacity(100);
        h.extend((0..10).map(|u| row(u * 10)));
        assert_eq!(h.index_range(25.0, 55.0), 2..7);
        assert_eq!(h.index_range(30.0, 50.0), 2..7);
        assert_eq!(h.index_range(-5.0, 1000.0), 0..10);
        assert_eq!(h.index_range(200.0, 300.0), 9..10);
    }

    #[test]
    fn clear_forgets_rows_but_bumps_the_version() {
        let mut h = RowHistory::with_capacity(8);
        h.extend((0..20).map(row));
        let v = h.version();
        h.clear();
        assert!(h.is_empty());
        assert_eq!(h.stride(), 1);
        assert!(h.version() > v);
    }
}
