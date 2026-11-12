//! Port of `Avida2Driver` (avida-core/source/targets/avida/Avida2Driver.cc).
//!
//! Ported so far: the per-update status line. The run loop follows with the
//! population's organism handling.

use avida_apto::fmt::{fmt_g_showpoint, pad_left_justified, status_double, status_int};

/// `eVerbosity` (avida/core/Definitions.h).
pub mod verbosity {
    pub const SILENT: i32 = 0;
    pub const NORMAL: i32 = 1;
    pub const ON: i32 = 2;
    pub const DETAILS: i32 = 3;
    pub const DEBUG: i32 = 4;
}

/// The values the status line reads from `cStats`, `cPopulation` and `cWorld`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct StatusLine {
    /// `stats.GetUpdate()`
    pub update: i32,
    /// `stats.SumGeneration().Average()`
    pub ave_generation: f64,
    /// `stats.GetAveFitness()`
    pub ave_fitness: f64,
    /// `population.GetNumOrganisms()`
    pub num_organisms: i32,
    /// `population.GetNumDemes()`
    pub num_demes: i32,
    /// `stats.GetNumOccupiedDemes()`
    pub num_occupied_demes: i32,
    /// `stats.GetAveMerit()`
    pub ave_merit: f64,
    /// `stats.GetNumThreads()`
    pub num_threads: i32,
    /// `stats.GetNumParasites()`
    pub num_parasites: i32,
    /// `stats.GetAveSpeculative()`
    pub ave_speculative: f64,
    /// `stats.GetSpeculativeWaste()`
    pub speculative_waste: i32,
    /// `m_world->CalculateUpdateSize()`
    pub update_size: i32,
}

impl StatusLine {
    /// The line `Avida2Driver::Run` prints after each update (without the
    /// trailing newline), or `None` at `VERBOSE_SILENT`.
    ///
    /// The stream is left-adjusted with `showpoint`; `setprecision` is sticky,
    /// which does not matter here because every double sets its own.
    pub fn render(&self, verbosity: i32) -> Option<String> {
        if verbosity <= verbosity::SILENT {
            return None;
        }
        let mut s = String::new();
        s.push_str(&format!("UD: {}  ", status_int(i64::from(self.update), 6)));
        s.push_str(&format!("Gen: {}  ", status_double(self.ave_generation)));
        s.push_str(&format!("Fit: {}  ", status_double(self.ave_fitness)));
        s.push_str(&format!(
            "Orgs: {}  ",
            status_int(i64::from(self.num_organisms), 6)
        ));
        if self.num_demes > 1 {
            s.push_str(&format!(
                "Demes: {} ",
                status_int(i64::from(self.num_occupied_demes), 4)
            ));
        }
        if verbosity == verbosity::ON || verbosity == verbosity::DETAILS {
            s.push_str(&format!("Merit: {}  ", status_double(self.ave_merit)));
            s.push_str(&format!(
                "Thrd: {}  ",
                status_int(i64::from(self.num_threads), 6)
            ));
            // No setw: the width set for `Thrd:` was consumed by it.
            s.push_str(&format!("Para: {}  ", self.num_parasites));
        }
        if verbosity >= verbosity::DEBUG {
            let waste = (f64::from(self.speculative_waste) / f64::from(self.update_size)) * 100.0;
            s.push_str(&format!(
                "Spec: {}  ",
                pad_left_justified(&fmt_g_showpoint(self.ave_speculative, 4), 6)
            ));
            s.push_str(&format!(
                "SWst: {}%  ",
                pad_left_justified(&fmt_g_showpoint(waste, 4), 6)
            ));
        }
        Some(s)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn line() -> StatusLine {
        StatusLine {
            update: 99,
            ave_generation: 6.981132075471698,
            ave_fitness: 0.23768711,
            num_organisms: 53,
            num_demes: 1,
            num_occupied_demes: 0,
            ave_merit: 97.0,
            num_threads: 53,
            num_parasites: 0,
            ave_speculative: 0.0,
            speculative_waste: 0,
            update_size: 1590,
        }
    }

    #[test]
    fn matches_reference_stdout_of_heads_default_100u() {
        // Captured from the C++ binary (seed 101, update 99).
        assert_eq!(
            line().render(verbosity::NORMAL).unwrap(),
            "UD: 99      Gen: 6.981132   Fit: 0.2376871  Orgs: 53      "
        );
        assert_eq!(line().render(verbosity::SILENT), None);
    }

    #[test]
    fn verbose_levels_add_columns() {
        let on = line().render(verbosity::ON).unwrap();
        assert!(on.ends_with("Merit: 97.00000   Thrd: 53      Para: 0  "));
        let dbg = line().render(verbosity::DEBUG).unwrap();
        assert!(dbg.ends_with("Spec: 0.000   SWst: 0.000 %  "), "{dbg:?}");
        assert!(!dbg.contains("Merit"));
    }
}
