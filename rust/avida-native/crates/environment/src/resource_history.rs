//! Port of `cResourceHistory` (avida-core/source/main/cResourceHistory.{h,cc}).
//!
//! The current C++ calls `avd_rh_select_entry_index` /
//! `avd_rh_value_at_or_zero` (rust/avida-rust); this ports the original
//! pre-Rust bodies (`git show 47f13dadb:avida-core/source/main/cResourceHistory.cc`),
//! which differ for an empty history (see `entry_for_update`).

use avida_config::init_file::InitFile;
use avida_config::string_util::{as_double, as_int, split_list};

/// `cResourceHistory::sResourceHistoryEntry`.
#[derive(Debug, Clone, PartialEq)]
pub struct ResourceHistoryEntry {
    /// `update`
    pub update: i32,
    /// `values`
    pub values: Vec<f64>,
}

/// `cResourceHistory`: resource levels recorded at given updates.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ResourceHistory {
    entries: Vec<ResourceHistoryEntry>,
}

impl ResourceHistory {
    /// `cResourceHistory()`.
    pub fn new() -> Self {
        Self::default()
    }

    /// All entries in order.
    pub fn entries(&self) -> &[ResourceHistoryEntry] {
        &self.entries
    }

    /// `getEntryForUpdate(update, exact)`: with `exact`, the first entry
    /// recorded for `update` (-1 if none); otherwise the entry before the
    /// first one past `update` (0 if that is the first).
    ///
    /// QUIRK: for an empty history the inexact lookup gives 0, not -1 (the
    /// `avd_rh_select_entry_index` helper in the current C++ gives -1).
    fn entry_for_update(&self, update: i32, exact: bool) -> i32 {
        let size = self.entries.len() as i32;
        if exact {
            for i in 0..size {
                if update == self.entries[i as usize].update {
                    return i;
                }
            }
            -1
        } else {
            let mut entry = 0;
            while entry < size {
                if self.entries[entry as usize].update > update {
                    break;
                }
                entry += 1;
            }
            if entry > 0 {
                entry -= 1;
            }
            entry
        }
    }

    /// `GetResourceCountForUpdate(ctx, update, rc, exact)`: calls
    /// `set(i, value)` for each of the `rc_size` resources of the target
    /// resource count (`rc.Set(ctx, i, v)` in C++; values past the stored
    /// ones are 0). Returns false when no entry matches; an empty history
    /// matches inexactly and sets every level to 0.
    pub fn resource_count_for_update(
        &self,
        update: i32,
        rc_size: usize,
        exact: bool,
        mut set: impl FnMut(usize, f64),
    ) -> bool {
        let entry = self.entry_for_update(update, exact);
        if entry == -1 {
            return false;
        }
        let entry = entry as usize;
        for i in 0..rc_size {
            match self.entries.get(entry).and_then(|e| e.values.get(i)) {
                Some(v) => set(i, *v),
                None => set(i, 0.0),
            }
        }
        true
    }

    /// `GetResourceLevelsForUpdate(update, levels, exact)`. The C++ indexes
    /// past the end of an empty history (inexact lookup); this returns an
    /// empty level list there.
    pub fn resource_levels_for_update(&self, update: i32, exact: bool) -> Option<Vec<f64>> {
        let entry = self.entry_for_update(update, exact);
        if entry == -1 {
            return None;
        }
        Some(
            self.entries
                .get(entry as usize)
                .map(|e| e.values.clone())
                .unwrap_or_default(),
        )
    }

    /// `AddEntry(update, values)` (no check for duplicate updates).
    pub fn add_entry(&mut self, update: i32, values: &[f64]) {
        self.entries.push(ResourceHistoryEntry {
            update,
            values: values.to_vec(),
        });
    }

    /// `LoadFile(filename, working_dir)`: one entry per line, the update
    /// first and the levels after it. Returns false if the file cannot be
    /// opened. QUIRK: replaces all existing entries.
    pub fn load_file(&mut self, filename: &str, working_dir: &str) -> bool {
        let file = InitFile::new(filename, working_dir, None, None);
        if !file.was_opened() {
            return false;
        }
        self.entries.clear();
        for line in 0..file.num_lines() {
            // cStringList(line): split on ' ' and pop from the front.
            let mut words = split_list(&file.get_line(line as i32), ' ').into_iter();
            let update = words.next().map(|w| as_int(&w)).unwrap_or(0);
            let values = words.map(|w| as_double(&w)).collect();
            self.entries.push(ResourceHistoryEntry { update, values });
        }
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn select_entries() {
        let mut h = ResourceHistory::new();
        // QUIRK: an empty history matches inexactly with all-zero levels.
        assert_eq!(h.resource_levels_for_update(0, false), Some(vec![]));
        assert!(h.resource_levels_for_update(0, true).is_none());
        let mut zeros = Vec::new();
        assert!(h.resource_count_for_update(0, 2, false, |i, v| zeros.push((i, v))));
        assert_eq!(zeros, vec![(0, 0.0), (1, 0.0)]);
        h.add_entry(10, &[1.0, 2.0]);
        h.add_entry(20, &[3.0]);
        h.add_entry(20, &[4.0]);
        h.add_entry(30, &[5.0]);
        assert_eq!(h.resource_levels_for_update(20, true), Some(vec![3.0]));
        assert_eq!(h.resource_levels_for_update(25, true), None);
        assert_eq!(h.resource_levels_for_update(5, false), Some(vec![1.0, 2.0]));
        assert_eq!(h.resource_levels_for_update(20, false), Some(vec![4.0]));
        assert_eq!(h.resource_levels_for_update(99, false), Some(vec![5.0]));
        let mut got = Vec::new();
        assert!(h.resource_count_for_update(10, 3, true, |i, v| got.push((i, v))));
        assert_eq!(got, vec![(0, 1.0), (1, 2.0), (2, 0.0)]);
        assert!(!h.resource_count_for_update(11, 3, true, |_, _| {}));
    }

    #[test]
    fn load_file() {
        let dir = std::env::temp_dir().join(format!("avida_env_rh_{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("res.dat"), "# c\n0 1.5 2\n100 3 4e1\n").unwrap();
        let mut h = ResourceHistory::new();
        assert!(h.load_file("res.dat", dir.to_str().unwrap()));
        assert_eq!(h.entries().len(), 2);
        assert_eq!(h.entries()[1].update, 100);
        assert_eq!(h.entries()[1].values, vec![3.0, 40.0]);
        assert!(!h.load_file("missing.dat", dir.to_str().unwrap()));
    }
}
