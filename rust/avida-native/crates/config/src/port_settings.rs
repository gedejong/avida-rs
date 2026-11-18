//! Settings of the native port that `cAvidaConfig.h` does not have.
//!
//! Nothing in the C++ corresponds to these, so they are not part of the
//! generated table ([`crate::config::GROUPS`], built from the kept header by
//! `tools/gen_config_table.py`) and survive its regeneration: the generator
//! only adds the [`PortSettings`] field to `Config`. They are read from
//! `avida.cfg` and set with `-set NAME VALUE` like any other setting (a
//! config file without them gets their defaults without a warning), but
//! `cAvidaConfig::Print`, `Status` and [`crate::config::Config::iter`] list
//! the C++ settings only, so nothing the C++ prints about its settings
//! changes. The review (`-r`) lists a port setting only when it differs
//! from its default, after the C++ settings.

use crate::string_util::{as_double, as_int, convert_double, convert_int};

/// One port-only setting (`int`, except `EXPECTED_QUALITY_MIN`, a `double`).
#[derive(Debug, Clone, Copy)]
pub struct PortEntryDef {
    /// The setting's name in `avida.cfg` and for `-set`.
    pub name: &'static str,
    /// Default value text.
    pub default: &'static str,
    /// Description (lines separated by '\n').
    pub desc: &'static str,
}

/// Every port-only setting, in the order the review lists them.
pub static PORT_ENTRIES: &[PortEntryDef] = &[
    PortEntryDef {
        name: "EXECUTION_MODE",
        default: "0",
        desc: "0 = exact: the C++ scheduler, one instruction per time slice (byte parity with the C++)\n\
               1 = parallel: per-update instruction budgets, organism-local instructions on worker threads \
               (deterministic for a seed, not byte-identical to the C++)",
    },
    PortEntryDef {
        name: "EXECUTION_THREADS",
        default: "0",
        desc: "Worker threads for EXECUTION_MODE 1 (0 or -1 = all available cores)",
    },
    PortEntryDef {
        name: "DROP_EXTINCT_GENOTYPES",
        default: "0",
        desc: "0 = keep a genotype without living organisms while another genotype names it as a parent (as the C++ does)\n\
               1 = drop it as soon as its last organism dies: memory stays flat with sexual reproduction, \
               ancestry (lineages, coalescence, kin distance) is not tracked and actions that need it stop",
    },
    PortEntryDef {
        name: "EXPECTED_QUALITY_DRAWS",
        default: "8",
        desc: "Test-CPU runs (random inputs) behind a genotype's expected quality, which a task with expect=1 \
               pays in place of the organism's own draw; 0 turns the feature off, and a task with expect=1 \
               then stops the run at load",
    },
    PortEntryDef {
        name: "EXPECTED_QUALITY_MIN",
        default: "0.25",
        desc: "An expected quality below this counts as 0 (the reaction does not fire)",
    },
];

/// The entry named `name`, if it is a port-only setting.
pub fn find(name: &str) -> Option<&'static PortEntryDef> {
    PORT_ENTRIES.iter().find(|e| e.name == name)
}

/// `EXECUTION_MODE 0`: the C++ scheduler (see [`PortSettings::execution_mode`]).
pub const EXECUTION_MODE_EXACT: i32 = 0;
/// `EXECUTION_MODE 1`: the parallel execution mode.
pub const EXECUTION_MODE_PARALLEL: i32 = 1;

/// Values of the port-only settings.
#[derive(Debug, Clone, PartialEq)]
pub struct PortSettings {
    /// `EXECUTION_MODE`: [`EXECUTION_MODE_EXACT`] or
    /// [`EXECUTION_MODE_PARALLEL`]. Read when the run starts.
    pub execution_mode: i32,
    /// `EXECUTION_THREADS`: worker threads of the parallel mode; 0 or a
    /// negative value means every available core.
    pub execution_threads: i32,
    /// `DROP_EXTINCT_GENOTYPES`: 0 keeps ancestry (the C++ behaviour), 1
    /// drops a genotype when its last organism dies.
    pub drop_extinct_genotypes: i32,
    /// `EXPECTED_QUALITY_DRAWS`: test runs behind a genotype's expected
    /// quality; 0 disables the feature.
    pub expected_quality_draws: i32,
    /// `EXPECTED_QUALITY_MIN`: an expected quality below it counts as 0.
    pub expected_quality_min: f64,
}

impl Default for PortSettings {
    fn default() -> Self {
        PortSettings::new()
    }
}

impl PortSettings {
    /// Every setting at its default.
    pub fn new() -> Self {
        let mut s = PortSettings {
            execution_mode: 0,
            execution_threads: 0,
            drop_extinct_genotypes: 0,
            expected_quality_draws: 8,
            expected_quality_min: 0.25,
        };
        for e in PORT_ENTRIES {
            s.load_str(e.name, e.default);
        }
        s
    }

    /// Loads `value` into the setting `name` (C++ `LoadStr` semantics:
    /// `atoi`-like parsing). Returns false when `name` is not a port
    /// setting.
    pub fn load_str(&mut self, name: &str, value: &str) -> bool {
        match name {
            "EXECUTION_MODE" => self.execution_mode = as_int(value),
            "EXECUTION_THREADS" => self.execution_threads = as_int(value),
            "DROP_EXTINCT_GENOTYPES" => self.drop_extinct_genotypes = as_int(value),
            "EXPECTED_QUALITY_DRAWS" => self.expected_quality_draws = as_int(value),
            "EXPECTED_QUALITY_MIN" => self.expected_quality_min = as_double(value),
            _ => return false,
        }
        true
    }

    /// The value of `name` as text, `None` when it is not a port setting.
    pub fn as_string(&self, name: &str) -> Option<String> {
        match name {
            "EXECUTION_MODE" => Some(convert_int(self.execution_mode)),
            "EXECUTION_THREADS" => Some(convert_int(self.execution_threads)),
            "DROP_EXTINCT_GENOTYPES" => Some(convert_int(self.drop_extinct_genotypes)),
            "EXPECTED_QUALITY_DRAWS" => Some(convert_int(self.expected_quality_draws)),
            "EXPECTED_QUALITY_MIN" => Some(convert_double(self.expected_quality_min)),
            _ => None,
        }
    }

    /// The review lines (`-r`) of the settings that differ from their
    /// defaults, in the C++ `PrintReview` format.
    pub fn review_lines(&self) -> String {
        let mut o = String::new();
        for e in PORT_ENTRIES {
            let value = self.as_string(e.name).unwrap_or_default();
            if as_double(&value) != as_double(e.default) {
                o.push_str(&format!(" {} {} (default={})\n", e.name, value, e.default));
            }
        }
        o
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_and_parsing() {
        let mut s = PortSettings::new();
        assert_eq!(s.execution_mode, EXECUTION_MODE_EXACT);
        assert_eq!(s.execution_threads, 0);
        assert_eq!(s.review_lines(), "");
        assert!(s.load_str("EXECUTION_MODE", "1"));
        assert!(s.load_str("EXECUTION_THREADS", "4 # comment"));
        assert_eq!(s.drop_extinct_genotypes, 0);
        assert!(s.load_str("DROP_EXTINCT_GENOTYPES", "1"));
        assert_eq!(s.as_string("DROP_EXTINCT_GENOTYPES").as_deref(), Some("1"));
        assert!(!s.load_str("WORLD_X", "4"));
        assert_eq!(s.execution_mode, EXECUTION_MODE_PARALLEL);
        assert_eq!(s.execution_threads, 4);
        assert_eq!(
            s.review_lines(),
            " EXECUTION_MODE 1 (default=0)\n EXECUTION_THREADS 4 (default=0)\n\
             \x20DROP_EXTINCT_GENOTYPES 1 (default=0)\n"
        );
        assert_eq!(s.expected_quality_draws, 8);
        assert_eq!(s.expected_quality_min, 0.25);
        assert!(s.load_str("EXPECTED_QUALITY_MIN", "0.5"));
        assert_eq!(
            s.as_string("EXPECTED_QUALITY_MIN").as_deref(),
            Some("0.500000")
        );
        assert!(s
            .review_lines()
            .ends_with(" EXPECTED_QUALITY_MIN 0.500000 (default=0.25)\n"));
        assert!(s.load_str("EXPECTED_QUALITY_MIN", "0.25"));
        assert!(find("EXECUTION_MODE").is_some());
        assert!(find("SPECULATIVE").is_none());
    }
}
