//! The configuration screen's state: filtering the effective settings,
//! editing a value, and the overrides a restart applies.
//!
//! A restart builds the world again from the same folder with the
//! arguments the `avida` binary takes (`-s 7`, `-set WORLD_X 30`), the
//! mechanism the desktop viewer uses. An override replaces any `-set` of
//! the same name already on the command line and leaves every other
//! argument alone.

use avida_session::{ExperimentSetup, Setting};

/// A value being typed for one setting.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Edit {
    pub name: &'static str,
    pub buffer: String,
    /// Why the buffer was refused.
    pub error: Option<String>,
}

#[derive(Clone, Debug, Default)]
pub struct ConfigState {
    pub query: String,
    /// Keys go to the query.
    pub filtering: bool,
    pub changed_only: bool,
    /// Selected row of the filtered list.
    pub sel: usize,
    /// First row shown (kept between draws so scrolling is steady).
    pub offset: usize,
    /// Overrides in the order they were made: `(name, value)`.
    pub overrides: Vec<(String, String)>,
    pub edit: Option<Edit>,
}

impl ConfigState {
    /// Indices into `setup.settings` that pass the filter.
    pub fn visible(&self, setup: &ExperimentSetup) -> Vec<usize> {
        filter_settings(&setup.settings, &self.query, self.changed_only)
    }

    pub fn override_of(&self, name: &str) -> Option<&str> {
        self.overrides
            .iter()
            .find(|(n, _)| n == name)
            .map(|(_, v)| v.as_str())
    }

    /// Records `value` for `name`; a value equal to the running world's
    /// drops the override instead.
    pub fn set_override(&mut self, setting: &Setting, value: String) {
        self.overrides.retain(|(n, _)| n != setting.name);
        if value != setting.value {
            self.overrides.push((setting.name.to_string(), value));
        }
    }

    pub fn clear_override(&mut self, name: &str) -> bool {
        let n = self.overrides.len();
        self.overrides.retain(|(k, _)| k != name);
        self.overrides.len() != n
    }

    pub fn move_sel(&mut self, d: i64, len: usize) {
        if len == 0 {
            self.sel = 0;
        } else {
            self.sel = (self.sel as i64 + d).clamp(0, len as i64 - 1) as usize;
        }
    }
}

/// Checks `text` against the setting's C++ type and returns the form the
/// command line takes.
pub fn validate(kind: &str, text: &str) -> Result<String, String> {
    let t = text.trim();
    match kind {
        "int" => t
            .replace(['_', ','], "")
            .parse::<i32>()
            .map(|n| n.to_string())
            .map_err(|_| format!("\"{t}\" is not a whole number.")),
        "double" => match t.parse::<f64>() {
            Ok(x) if x.is_finite() => Ok(t.to_string()),
            _ => Err(format!("\"{t}\" is not a number.")),
        },
        "bool" => match t.to_ascii_lowercase().as_str() {
            "1" | "true" | "yes" | "on" => Ok("1".to_string()),
            "0" | "false" | "no" | "off" => Ok("0".to_string()),
            _ => Err(format!("\"{t}\" is not 0 or 1.")),
        },
        _ => {
            if t.is_empty() {
                Err("A text setting cannot be empty here.".to_string())
            } else {
                Ok(t.to_string())
            }
        }
    }
}

/// `base` with the overrides applied: a seed replaces every `-s`/`-seed`, a
/// setting replaces every `-set` of the same name; the new arguments come
/// last. Other arguments are kept in order.
pub fn merge_args(base: &[String], overrides: &[(String, String)]) -> Vec<String> {
    let seed = overrides.iter().find(|(n, _)| n == "RANDOM_SEED");
    let replaced: Vec<&str> = overrides.iter().map(|(n, _)| n.as_str()).collect();
    let mut out = Vec::with_capacity(base.len() + 3 * overrides.len());
    let mut i = 0;
    while i < base.len() {
        let a = base[i].as_str();
        if (a == "-s" || a == "-seed") && seed.is_some() {
            i += 2;
            continue;
        }
        if a == "-set" {
            let name = base.get(i + 1).map(String::as_str).unwrap_or("");
            let end = (i + 3).min(base.len());
            if !replaced.contains(&name) {
                out.extend(base[i..end].iter().cloned());
            }
            i += 3;
            continue;
        }
        out.push(base[i].clone());
        i += 1;
    }
    for (n, v) in overrides {
        if n == "RANDOM_SEED" {
            out.extend(["-s".to_string(), v.clone()]);
        } else {
            out.extend(["-set".to_string(), n.clone(), v.clone()]);
        }
    }
    out
}

/// Indices of the settings matching `query` (every word, ignoring case,
/// in the name, value, group or description), changed ones only when
/// asked.
pub fn filter_settings(settings: &[Setting], query: &str, changed_only: bool) -> Vec<usize> {
    let words: Vec<String> = query.split_whitespace().map(str::to_lowercase).collect();
    settings
        .iter()
        .enumerate()
        .filter(|(_, s)| !changed_only || s.is_changed())
        .filter(|(_, s)| {
            if words.is_empty() {
                return true;
            }
            let hay = format!("{} {} {} {}", s.name, s.value, s.group, s.desc).to_lowercase();
            words.iter().all(|w| hay.contains(w.as_str()))
        })
        .map(|(i, _)| i)
        .collect()
}

#[cfg(test)]
pub(crate) mod tests {
    use std::path::PathBuf;

    use super::*;

    pub fn setting(name: &'static str, kind: &'static str, value: &str, default: &str) -> Setting {
        Setting {
            group: "GENERAL_GROUP",
            group_desc: "General settings",
            name,
            kind,
            value: value.into(),
            default: default.into(),
            desc: "a setting",
            from_command_line: false,
        }
    }

    pub fn setup(settings: Vec<Setting>) -> ExperimentSetup {
        ExperimentSetup {
            work_dir: PathBuf::from("/x"),
            args: Vec::new(),
            data_dir: PathBuf::from("/x/data"),
            config_file: PathBuf::from("/x/avida.cfg"),
            settings,
            event_file: PathBuf::from("/x/events.cfg"),
            events: Vec::new(),
        }
    }

    fn strings(v: &[&str]) -> Vec<String> {
        v.iter().map(ToString::to_string).collect()
    }

    #[test]
    fn values_are_checked_by_type() {
        assert_eq!(validate("int", " 30 "), Ok("30".into()));
        assert!(validate("int", "3.5").is_err());
        assert_eq!(validate("double", "0.02"), Ok("0.02".into()));
        assert!(validate("double", "nan").is_err());
        assert_eq!(validate("bool", "true"), Ok("1".into()));
        assert!(validate("bool", "2").is_err());
        assert!(validate("cString", "").is_err());
        assert_eq!(validate("cString", "x.org"), Ok("x.org".into()));
    }

    #[test]
    fn overrides_replace_matching_arguments_only() {
        let base = strings(&[
            "-s", "7", "-set", "WORLD_X", "30", "-set", "WORLD_Y", "30", "-c", "x.cfg",
        ]);
        let ov = vec![
            ("WORLD_X".to_string(), "20".to_string()),
            ("RANDOM_SEED".to_string(), "9".to_string()),
        ];
        assert_eq!(
            merge_args(&base, &ov),
            strings(&["-set", "WORLD_Y", "30", "-c", "x.cfg", "-set", "WORLD_X", "20", "-s", "9"])
        );
        assert_eq!(merge_args(&base, &[]), base);
    }

    #[test]
    fn filtering_matches_every_word_and_changed_only() {
        let s = vec![
            setting("WORLD_X", "int", "60", "60"),
            setting("COPY_MUT_PROB", "double", "0.02", "0.0075"),
        ];
        assert_eq!(filter_settings(&s, "", false), vec![0, 1]);
        assert_eq!(filter_settings(&s, "copy MUT", false), vec![1]);
        assert_eq!(filter_settings(&s, "", true), vec![1]);
        assert_eq!(
            filter_settings(&s, "general nothing", false),
            Vec::<usize>::new()
        );
    }

    #[test]
    fn an_override_equal_to_the_running_value_disappears() {
        let s = setting("WORLD_X", "int", "60", "60");
        let mut c = ConfigState::default();
        c.set_override(&s, "20".into());
        assert_eq!(c.override_of("WORLD_X"), Some("20"));
        c.set_override(&s, "60".into());
        assert!(c.overrides.is_empty());
        c.set_override(&s, "20".into());
        assert!(c.clear_override("WORLD_X"));
        assert!(!c.clear_override("WORLD_X"));
    }
}
