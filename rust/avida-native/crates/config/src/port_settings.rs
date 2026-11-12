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

use crate::string_util::{as_int, convert_int};

/// One port-only setting (all are `int`).
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
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PortSettings {
    /// `EXECUTION_MODE`: [`EXECUTION_MODE_EXACT`] or
    /// [`EXECUTION_MODE_PARALLEL`]. Read when the run starts.
    pub execution_mode: i32,
    /// `EXECUTION_THREADS`: worker threads of the parallel mode; 0 or a
    /// negative value means every available core.
    pub execution_threads: i32,
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
            _ => return false,
        }
        true
    }

    /// The value of `name` as text, `None` when it is not a port setting.
    pub fn as_string(&self, name: &str) -> Option<String> {
        match name {
            "EXECUTION_MODE" => Some(convert_int(self.execution_mode)),
            "EXECUTION_THREADS" => Some(convert_int(self.execution_threads)),
            _ => None,
        }
    }

    /// The review lines (`-r`) of the settings that differ from their
    /// defaults, in the C++ `PrintReview` format.
    pub fn review_lines(&self) -> String {
        let mut o = String::new();
        for e in PORT_ENTRIES {
            let value = self.as_string(e.name).unwrap_or_default();
            if as_int(&value) != as_int(e.default) {
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
        assert!(!s.load_str("WORLD_X", "4"));
        assert_eq!(s.execution_mode, EXECUTION_MODE_PARALLEL);
        assert_eq!(s.execution_threads, 4);
        assert_eq!(
            s.review_lines(),
            " EXECUTION_MODE 1 (default=0)\n EXECUTION_THREADS 4 (default=0)\n"
        );
        assert!(find("EXECUTION_MODE").is_some());
        assert!(find("SPECULATIVE").is_none());
    }
}
