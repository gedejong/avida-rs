//! Port of `cMigrationMatrix`
//! (avida-core/source/main/cMigrationMatrix.{h,cc} at 47f13dadb): the
//! deme-to-deme connection weights `DEMES_MIGRATION_METHOD 4` draws
//! migration targets from, with per-pair migration counters.

use avida_apto::AvidaRng;
use avida_config::feedback::{Feedback, FeedbackKind};
use avida_config::init_file::InitFile;
use avida_config::string_util::{as_double, pop};

/// C++: `cMigrationMatrix`
#[derive(Debug, Clone, Default)]
pub struct MigrationMatrix {
    /// `m_migration_matrix`
    matrix: Vec<Vec<f64>>,
    /// `m_row_connectivity_sums`
    row_sums: Vec<f64>,
    /// `m_parasite_migration_counts`
    parasite_counts: Vec<Vec<i32>>,
    /// `m_offspring_migration_counts`
    offspring_counts: Vec<Vec<i32>>,
}

impl MigrationMatrix {
    pub fn new() -> Self {
        Self::default()
    }

    /// `Load(num_demes, filename, working_dir, count_parasites,
    /// count_offspring, is_reload, feedback)`. Messages go to `feedback` as
    /// in C++; returns whether the matrix is valid.
    #[allow(clippy::too_many_arguments)]
    pub fn load(
        &mut self,
        num_demes: i32,
        filename: &str,
        working_dir: &str,
        count_parasites: bool,
        count_offspring: bool,
        is_reload: bool,
        feedback: &mut Feedback,
    ) -> bool {
        self.matrix.clear();
        self.row_sums.clear();
        let infile = InitFile::new(filename, working_dir, None, None);
        if !infile.was_opened() {
            for (kind, msg) in infile.feedback().messages() {
                match kind {
                    FeedbackKind::Error => feedback.error(msg.clone()),
                    FeedbackKind::Warning => feedback.warning(msg.clone()),
                    FeedbackKind::Notification => feedback.notify(msg.clone()),
                }
            }
            feedback.error(format!("failed to load migration matrix '{filename}'"));
            return false;
        }

        for line_id in 0..infile.num_lines() {
            // Load the next line from the file.
            let mut line = infile.get_line(line_id as i32);
            let mut row = Vec::new();
            let mut row_sum = 0.0;
            while !line.is_empty() {
                let val = as_double(&pop(&mut line, ','));
                if val < 0.0 {
                    feedback.error("Cannot have a negative connection in connection matrix");
                    return false;
                }
                row_sum += val;
                row.push(val);
            }
            if row_sum == 0.0 {
                feedback.error("Cannot have a row sum of 0.0 in connection matrix");
                return false;
            }
            self.row_sums.push(row_sum);
            self.matrix.push(row);
        }

        let n = self.matrix.len();
        let mismatch = |feedback: &mut Feedback| {
            feedback.error(format!(
                "The number of demes in the migration matrix ({n}) did not match the NUM_DEMES \
                 ({num_demes}) parameter in avida.cfg."
            ));
        };
        if num_demes as usize != n || num_demes < 0 {
            mismatch(feedback);
            return false;
        }
        for (r, row) in self.matrix.iter().enumerate() {
            if row.len() != n {
                feedback.error(format!(
                    "The number of columns in row {r} did not match total number of demes"
                ));
                return false;
            }
        }

        let blank = vec![0; n];
        if count_parasites && !is_reload {
            self.parasite_counts = vec![blank.clone(); n];
        }
        if count_offspring && !is_reload {
            self.offspring_counts = vec![blank; n];
        }
        true
    }

    /// `GetProbabilisticDemeID(from_deme_id, rng, is_parasite_migration)`:
    /// one `GetDouble(row_sum)` draw walked down the row. `Err` where the
    /// C++ indexes an unsized counter array (the matter was loaded without
    /// counting this kind of migration) or falls off the row.
    pub fn probabilistic_deme_id(
        &mut self,
        from_deme_id: i32,
        rng: &mut AvidaRng,
        is_parasite_migration: bool,
    ) -> Result<i32, String> {
        let from = usize::try_from(from_deme_id)
            .ok()
            .filter(|&f| f < self.matrix.len())
            .ok_or("migration matrix row out of range (C++: assertion)")?;
        let mut rand = rng.get_double_max(self.row_sums[from]);
        for col in 0..self.matrix[from].len() {
            rand -= self.matrix[from][col];
            if rand <= 0.0 {
                let counts = if is_parasite_migration {
                    &mut self.parasite_counts
                } else {
                    &mut self.offspring_counts
                };
                let slot = counts
                    .get_mut(from)
                    .and_then(|r| r.get_mut(col))
                    .ok_or("migration counts not allocated (C++: out-of-bounds write)")?;
                *slot += 1;
                return Ok(col as i32);
            }
        }
        // Should never get to this point
        Err("migration matrix row exhausted (C++: assertion)".to_string())
    }

    /// The number of rows of the loaded matrix (0 before `Load`).
    pub fn num_rows(&self) -> usize {
        self.matrix.len()
    }

    /// `AlterConnectionWeight(from, to, amount)`
    pub fn alter_connection_weight(&mut self, from: usize, to: usize, amount: f64) -> bool {
        self.matrix[from][to] += amount;
        self.row_sums[from] += amount;
        let row_sum: f64 = self.matrix[from].iter().sum();
        !(self.matrix[from][to] < 0.0 || row_sum <= 0.0)
    }

    /// `GetOffspringCountAt(from, to)`
    pub fn offspring_count_at(&self, from: usize, to: usize) -> Option<i32> {
        self.offspring_counts.get(from)?.get(to).copied()
    }

    /// `GetParasiteCountAt(from, to)`
    pub fn parasite_count_at(&self, from: usize, to: usize) -> Option<i32> {
        self.parasite_counts.get(from)?.get(to).copied()
    }

    /// `Print(os)`: rows of `os << double`, comma separated.
    pub fn print(&self) -> String {
        let mut out = String::new();
        for row in &self.matrix {
            let cells: Vec<String> = row
                .iter()
                .map(|&v| avida_config::string_util::format_ostream_double(v))
                .collect();
            out.push_str(&cells.join(","));
            out.push('\n');
        }
        out
    }

    /// `ResetParasiteCounts()`
    pub fn reset_parasite_counts(&mut self) {
        for row in &mut self.parasite_counts {
            row.fill(0);
        }
    }

    /// `ResetOffspringCounts()`
    pub fn reset_offspring_counts(&mut self) {
        for row in &mut self.offspring_counts {
            row.fill(0);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn load_text(text: &str, n: i32) -> (MigrationMatrix, bool, String) {
        let dir =
            std::env::temp_dir().join(format!("avida_mm_{}_{}", std::process::id(), text.len()));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("m.mat"), text).unwrap();
        let mut m = MigrationMatrix::new();
        let mut fb = Feedback::new();
        let ok = m.load(
            n,
            "m.mat",
            dir.to_str().unwrap(),
            false,
            true,
            false,
            &mut fb,
        );
        (m, ok, fb.render())
    }

    #[test]
    fn loads_prints_and_draws() {
        let (mut m, ok, fb) = load_text("1,0\n0.5,0.25\n", 2);
        assert!(ok, "{fb}");
        assert_eq!(m.print(), "1,0\n0.5,0.25\n");
        let mut rng = AvidaRng::new(1);
        assert_eq!(m.probabilistic_deme_id(0, &mut rng, false), Ok(0));
        assert_eq!(m.offspring_count_at(0, 0), Some(1));
        // Parasite counts were not allocated.
        assert!(m.probabilistic_deme_id(0, &mut rng, true).is_err());
    }

    #[test]
    fn rejects_bad_matrices() {
        let (_, ok, fb) = load_text("1,0\n0,0\n", 2);
        assert!(!ok);
        assert!(fb.contains("row sum of 0.0"));
        let (_, ok, fb) = load_text("1,0\n", 2);
        assert!(!ok);
        assert!(fb.contains("did not match the NUM_DEMES (2)"));
        let (_, ok, fb) = load_text("1,-1\n1,1\n", 2);
        assert!(!ok);
        assert!(fb.contains("negative connection"));
    }
}
