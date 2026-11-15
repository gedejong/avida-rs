//! What the front ends keep of an open comparison: a loaded batch, which
//! runs the summaries take into account, the pair of arms the tests
//! compare, and the numbers derived from them (recomputed when a choice
//! changes, not on every frame).

use std::path::{Path, PathBuf};

use super::{load_batch, ArmSummary, Batch, Comparison, MedianPoint, Options, Run};

pub struct CompareState {
    pub dir: PathBuf,
    pub batch: Batch,
    /// Leave out runs without a `.done` file.
    pub finished_only: bool,
    /// The two arms the p-values compare (indices into the batch's arms).
    pub pair: (usize, usize),
    /// The run whose folder is shown under the table: arm, run.
    pub selected: Option<(usize, usize)>,
    pub summary: Vec<ArmSummary>,
    pub comparison: Option<Comparison>,
    pub medians: Vec<Vec<MedianPoint>>,
    /// The largest update of any run.
    pub max_update: i32,
}

impl CompareState {
    pub fn load(dir: &Path) -> Result<CompareState, String> {
        let batch = load_batch(dir)?;
        let mut s = CompareState {
            dir: dir.to_path_buf(),
            batch,
            finished_only: false,
            pair: (0, 1),
            selected: None,
            summary: Vec::new(),
            comparison: None,
            medians: Vec::new(),
            max_update: 0,
        };
        s.refresh();
        Ok(s)
    }

    /// Reads the folder again (runs in progress have written more),
    /// keeping the choices that still make sense.
    pub fn reload(&mut self) -> Result<(), String> {
        let batch = load_batch(&self.dir)?;
        self.batch = batch;
        let n = self.batch.arms.len();
        if self.pair.0 >= n || self.pair.1 >= n {
            self.pair = (0, 1);
        }
        if let Some((a, r)) = self.selected {
            if self.batch.arms.get(a).is_none_or(|arm| r >= arm.runs.len()) {
                self.selected = None;
            }
        }
        self.refresh();
        Ok(())
    }

    pub fn options(&self) -> Options {
        Options {
            finished_only: self.finished_only,
        }
    }

    pub fn set_finished_only(&mut self, on: bool) {
        if self.finished_only != on {
            self.finished_only = on;
            self.refresh();
        }
    }

    /// Chooses the arms the tests compare; two different arms.
    pub fn set_pair(&mut self, a: usize, b: usize) {
        if a != b && a < self.batch.arms.len() && b < self.batch.arms.len() && self.pair != (a, b) {
            self.pair = (a, b);
            self.refresh();
        }
    }

    /// Whether p-values can be shown: at least two arms.
    pub fn has_pair(&self) -> bool {
        self.batch.arms.len() >= 2
    }

    fn refresh(&mut self) {
        let opts = self.options();
        self.summary = self.batch.summarize(opts);
        self.comparison = if self.has_pair() {
            self.batch.compare(self.pair.0, self.pair.1, opts)
        } else {
            None
        };
        self.medians = (0..self.batch.arms.len())
            .map(|a| self.batch.median_series(a, opts))
            .collect();
        self.max_update = self
            .batch
            .runs()
            .filter_map(|r| r.last_update)
            .max()
            .unwrap_or(0);
    }

    pub fn takes(&self, run: &Run) -> bool {
        !self.finished_only || run.finished
    }
}
