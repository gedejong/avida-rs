//! The config tab: searching the effective configuration, and the form
//! that restarts the experiment with changed settings.
//!
//! A restart builds the world again from the same folder with the
//! arguments the `avida` binary takes (`-s 7`, `-set WORLD_X 30`); the
//! form only adds or replaces the arguments of the fields that differ from
//! the running world, so arguments it does not know survive.

use std::path::Path;

use avida_session::{ExperimentSetup, Setting};

use crate::model::Experiment;

/// The settings the restart form offers, besides the seed and the update
/// limit: `(config name, label, help)`.
pub const FORM_SETTINGS: [(&str, &str, &str); 5] = [
    ("WORLD_X", "World width", "Cells across (WORLD_X)."),
    ("WORLD_Y", "World height", "Cells down (WORLD_Y)."),
    (
        "COPY_MUT_PROB",
        "Copy mutation rate",
        "Chance that copying one instruction copies a random one instead (COPY_MUT_PROB).",
    ),
    (
        "DIVIDE_INS_PROB",
        "Insertion rate",
        "Chance of inserting a random instruction at division (DIVIDE_INS_PROB).",
    ),
    (
        "DIVIDE_DEL_PROB",
        "Deletion rate",
        "Chance of deleting an instruction at division (DIVIDE_DEL_PROB).",
    ),
];

/// The restart form's fields, as typed.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Overrides {
    pub seed: String,
    /// Values of [`FORM_SETTINGS`], same order.
    pub values: [String; 5],
    /// Empty: no limit.
    pub update_limit: String,
}

#[derive(Clone, Debug, Default)]
pub struct ConfigView {
    pub query: String,
    pub changed_only: bool,
    pub overrides: Overrides,
    /// The form as filled from the running world (to tell edits apart).
    pub baseline: Overrides,
}

impl ConfigView {
    /// Fills the form from a newly opened world.
    pub fn reset(&mut self, setup: &ExperimentSetup, limit: Option<i32>) {
        self.baseline = Overrides::from_setup(setup, limit);
        self.overrides = self.baseline.clone();
    }

    /// Whether the form differs from the running world.
    pub fn is_edited(&self) -> bool {
        self.overrides != self.baseline
    }
}

impl Overrides {
    pub fn from_setup(setup: &ExperimentSetup, limit: Option<i32>) -> Overrides {
        let value = |name: &str| {
            setup
                .setting(name)
                .map(|s| s.value.clone())
                .unwrap_or_default()
        };
        Overrides {
            seed: value("RANDOM_SEED"),
            values: FORM_SETTINGS.map(|(name, _, _)| value(name)),
            update_limit: limit.map(|l| l.to_string()).unwrap_or_default(),
        }
    }

    /// The arguments and update limit for a restart, against the running
    /// world's form (`baseline`): only changed fields become arguments.
    pub fn arguments(&self, baseline: &Overrides) -> Result<(Vec<Arg>, Option<i32>), String> {
        let mut args = Vec::new();
        let seed = self.seed.trim();
        if seed != baseline.seed.trim() {
            let n: i64 = seed
                .parse()
                .map_err(|_| format!("The random seed \"{seed}\" is not a whole number."))?;
            if !(i64::from(i32::MIN)..=i64::from(i32::MAX)).contains(&n) {
                return Err(format!("The random seed {n} is out of range."));
            }
            args.push(Arg::Seed(n.to_string()));
        }
        for (i, (name, label, _)) in FORM_SETTINGS.iter().enumerate() {
            let v = self.values[i].trim();
            if v == baseline.values[i].trim() {
                continue;
            }
            let ok = if name.starts_with("WORLD_") {
                v.parse::<i32>().is_ok_and(|n| n > 0)
            } else {
                v.parse::<f64>().is_ok_and(|p| (0.0..=1.0).contains(&p))
            };
            if !ok {
                return Err(if name.starts_with("WORLD_") {
                    format!("{label} must be a whole number above zero, not \"{v}\".")
                } else {
                    format!("{label} must be a probability from 0 to 1, not \"{v}\".")
                });
            }
            args.push(Arg::Set((*name).to_string(), v.to_string()));
        }
        let limit = match self.update_limit.trim().replace([',', '_'], "") {
            s if s.is_empty() => None,
            s => match s.parse::<i32>() {
                Ok(n) if n >= 0 => Some(n),
                _ => {
                    return Err(format!(
                        "The update limit \"{}\" is not an update number.",
                        self.update_limit.trim()
                    ))
                }
            },
        };
        Ok((args, limit))
    }
}

/// One argument the form produces.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Arg {
    /// `-s <seed>`
    Seed(String),
    /// `-set <NAME> <value>`
    Set(String, String),
}

/// `base` with `args` applied: a seed replaces every `-s`/`-seed`, a
/// setting replaces every `-set` of the same name; the new arguments come
/// last. Other arguments are kept in order.
pub fn merge_args(base: &[String], args: &[Arg]) -> Vec<String> {
    let new_seed = args.iter().any(|a| matches!(a, Arg::Seed(_)));
    let replaced: Vec<&str> = args
        .iter()
        .filter_map(|a| match a {
            Arg::Set(n, _) => Some(n.as_str()),
            Arg::Seed(_) => None,
        })
        .collect();
    let mut out = Vec::with_capacity(base.len() + 3 * args.len());
    let mut i = 0;
    while i < base.len() {
        let a = base[i].as_str();
        if (a == "-s" || a == "-seed") && new_seed {
            i += 2;
            continue;
        }
        if a == "-set" {
            let name = base.get(i + 1).map(String::as_str).unwrap_or("");
            if replaced.contains(&name) {
                i += 3;
                continue;
            }
            out.extend(base[i..(i + 3).min(base.len())].iter().cloned());
            i += 3;
            continue;
        }
        out.push(base[i].clone());
        i += 1;
    }
    for a in args {
        match a {
            Arg::Seed(s) => out.extend(["-s".to_string(), s.clone()]),
            Arg::Set(n, v) => out.extend(["-set".to_string(), n.clone(), v.clone()]),
        }
    }
    out
}

/// The experiment a restart opens.
pub fn restarted(current: &Experiment, args: &[Arg], limit: Option<i32>) -> Experiment {
    Experiment {
        dir: current.dir.clone(),
        args: merge_args(&current.args, args),
        update_limit: limit,
    }
}

/// Quotes a word for a POSIX shell when it needs it.
pub fn shell_quote(word: &str) -> String {
    let safe = !word.is_empty()
        && word
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || "-_./:=+,@%".contains(c));
    if safe {
        word.to_string()
    } else {
        format!("'{}'", word.replace('\'', r"'\''"))
    }
}

/// The command line that opens `dir` with `args` in this program.
pub fn cli_line(dir: &Path, args: &[String]) -> String {
    let mut words = vec![
        "avida-gui".to_string(),
        shell_quote(&dir.display().to_string()),
    ];
    words.extend(args.iter().map(|a| shell_quote(a)));
    words.join(" ")
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
mod tests {
    use std::path::PathBuf;

    use super::*;

    fn setting(name: &'static str, value: &str, default: &str) -> Setting {
        Setting {
            group: "GENERAL_GROUP",
            group_desc: "General settings",
            name,
            kind: "int",
            value: value.into(),
            default: default.into(),
            desc: "a setting",
            from_command_line: false,
        }
    }

    fn setup() -> ExperimentSetup {
        ExperimentSetup {
            work_dir: PathBuf::from("/x"),
            args: Vec::new(),
            data_dir: PathBuf::from("/x/data"),
            config_file: PathBuf::from("/x/avida.cfg"),
            settings: vec![
                setting("RANDOM_SEED", "101", "0"),
                setting("WORLD_X", "60", "60"),
                setting("WORLD_Y", "60", "60"),
                setting("COPY_MUT_PROB", "0.0075", "0.0075"),
                setting("DIVIDE_INS_PROB", "0.05", "0.05"),
                setting("DIVIDE_DEL_PROB", "0.05", "0.05"),
            ],
            event_file: PathBuf::from("/x/events.cfg"),
            events: Vec::new(),
        }
    }

    #[test]
    fn only_edited_fields_become_arguments() {
        let mut view = ConfigView::default();
        view.reset(&setup(), None);
        assert!(!view.is_edited());
        assert_eq!(
            view.overrides.arguments(&view.baseline),
            Ok((Vec::new(), None))
        );
        view.overrides.seed = "7".into();
        view.overrides.values[0] = " 30 ".into();
        view.overrides.update_limit = "1,000".into();
        assert!(view.is_edited());
        let (args, limit) = view.overrides.arguments(&view.baseline).unwrap();
        assert_eq!(
            args,
            vec![
                Arg::Seed("7".into()),
                Arg::Set("WORLD_X".into(), "30".into())
            ]
        );
        assert_eq!(limit, Some(1000));
    }

    #[test]
    fn bad_values_are_explained() {
        let mut view = ConfigView::default();
        view.reset(&setup(), Some(50));
        assert_eq!(view.overrides.update_limit, "50");
        view.overrides.values[2] = "1.5".into();
        let e = view.overrides.arguments(&view.baseline).unwrap_err();
        assert!(e.contains("Copy mutation rate"), "{e}");
        view.overrides.values[2] = "0.01".into();
        view.overrides.values[1] = "0".into();
        assert!(view.overrides.arguments(&view.baseline).is_err());
        view.overrides.values[1] = "60".into();
        view.overrides.seed = "seven".into();
        assert!(view.overrides.arguments(&view.baseline).is_err());
        view.overrides.seed = "101".into();
        view.overrides.update_limit = "-3".into();
        assert!(view.overrides.arguments(&view.baseline).is_err());
    }

    #[test]
    fn merging_replaces_matching_arguments_and_keeps_the_rest() {
        let base: Vec<String> = [
            "-v2", "-s", "3", "-set", "WORLD_X", "10", "-set", "WORLD_Y", "9",
        ]
        .map(String::from)
        .to_vec();
        let out = merge_args(
            &base,
            &[
                Arg::Seed("7".into()),
                Arg::Set("WORLD_X".into(), "30".into()),
            ],
        );
        assert_eq!(
            out,
            ["-v2", "-set", "WORLD_Y", "9", "-s", "7", "-set", "WORLD_X", "30"]
                .map(String::from)
                .to_vec()
        );
        assert_eq!(merge_args(&base, &[]), base);
        let exp = Experiment {
            dir: PathBuf::from("/e x/config"),
            args: base,
            update_limit: None,
        };
        let next = restarted(&exp, &[Arg::Seed("1".into())], Some(20));
        assert_eq!(next.update_limit, Some(20));
        assert_eq!(next.dir, exp.dir);
        assert_eq!(
            cli_line(&next.dir, &next.args[..2]),
            "avida-gui '/e x/config' -v2 -set"
        );
        assert_eq!(shell_quote("it's"), r"'it'\''s'");
        assert_eq!(shell_quote("0.0075"), "0.0075");
    }

    #[test]
    fn settings_are_searched_by_every_word() {
        let s = setup().settings;
        assert_eq!(filter_settings(&s, "", false).len(), 6);
        assert_eq!(filter_settings(&s, "world x", false), vec![1]);
        assert_eq!(filter_settings(&s, "WORLD", false), vec![1, 2]);
        assert_eq!(filter_settings(&s, "", true), vec![0], "seed differs");
        assert_eq!(filter_settings(&s, "101", false), vec![0]);
        assert!(filter_settings(&s, "nothing-like-this", false).is_empty());
    }
}
