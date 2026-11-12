//! Port of `cAvidaConfig` (avida-core/source/main/cAvidaConfig.{h,cc}).
//!
//! The settings table (`config_table.rs`) is generated from the header's
//! `CONFIG_ADD_GROUP` / `CONFIG_ADD_VAR` / `CONFIG_ADD_ALIAS` /
//! `CONFIG_ADD_CUSTOM_FORMAT` / `CONFIG_ADD_FORMAT_VAR` invocations by
//! `tools/gen_config_table.py` (run it again if the header changes). It also
//! contains the `Config` struct: one typed public field per setting (the
//! lower-cased setting name), plus the value lists of the custom formats
//! (`INSTSETS`, retrievable with [`Config::instsets`] or
//! [`Config::format_lines`]).

use crate::feedback::Feedback;
use crate::init_file::InitFile;
use crate::string_util::{as_double, as_int, format_ostream_double, pop, pop_word, split_list};

pub use crate::config_table::{Config, FORMATS, GROUPS, NUM_ENTRIES};

/// `Avida::Version::String()` (avida-core/include/public/avida/core/Version.h).
pub const VERSION_STRING: &str = "2.14.0";
/// `Avida::Version::Major()`.
pub const VERSION_MAJOR: i32 = 2;
/// `Avida::Version::Minor()`.
pub const VERSION_MINOR: i32 = 14;

/// Type of a setting (`CONFIG_ADD_VAR`'s `TYPE`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EntryType {
    /// `int`
    Int,
    /// `double`
    Double,
    /// `bool`
    Bool,
    /// `cString`
    Str,
}

impl EntryType {
    /// `cBaseConfigEntry::GetType()`: the C++ type name.
    pub fn type_name(self) -> &'static str {
        match self {
            EntryType::Int => "int",
            EntryType::Double => "double",
            EntryType::Bool => "bool",
            EntryType::Str => "cString",
        }
    }
}

/// `cBaseConfigEntry`: static description of one setting.
#[derive(Debug, Clone, Copy)]
pub struct EntryDef {
    /// Index in declaration order over all groups.
    pub id: usize,
    /// Primary name first, then `CONFIG_ADD_ALIAS` aliases.
    pub names: &'static [&'static str],
    /// Declared type.
    pub ty: EntryType,
    /// Default value text (`#DEFAULT` with string quotes removed).
    pub default: &'static str,
    /// Description (lines separated by '\n').
    pub desc: &'static str,
}

/// `cBaseConfigGroup`.
#[derive(Debug, Clone, Copy)]
pub struct GroupDef {
    /// Group name (e.g. `GENERAL_GROUP`).
    pub name: &'static str,
    /// Group description.
    pub desc: &'static str,
    /// `id` of the first entry in the group.
    pub first_id: usize,
    /// Entries in declaration order.
    pub entries: &'static [EntryDef],
}

/// `cBaseConfigFormatEntry`.
#[derive(Debug, Clone, Copy)]
pub struct FormatEntryDef {
    /// Keyword (e.g. `INSTSET`, `INST`).
    pub name: &'static str,
    /// Description.
    pub desc: &'static str,
}

/// `cBaseConfigCustomFormat`.
#[derive(Debug, Clone, Copy)]
pub struct FormatDef {
    /// Format name (e.g. `INSTSETS`).
    pub name: &'static str,
    /// Description.
    pub desc: &'static str,
    /// Keywords belonging to the format.
    pub entries: &'static [FormatEntryDef],
}

/// One setting together with its current value, as yielded by [`Config::iter`].
#[derive(Debug, Clone)]
pub struct ConfigEntry {
    /// Group the setting belongs to.
    pub group: &'static GroupDef,
    /// Static definition.
    pub def: &'static EntryDef,
    /// Current value as `cEntry::AsString()` gives it.
    pub value: String,
}

/// `Avida::Version::CheckCompatibility`: major and minor numbers must match.
pub fn check_version_compatibility(version: &str) -> bool {
    let mut lv = version.to_string();
    let major = pop(&mut lv, '.');
    let minor = pop(&mut lv, '.');
    if major.is_empty() || VERSION_MAJOR != as_int(&major) {
        return false;
    }
    if minor.is_empty() || VERSION_MINOR != as_int(&minor) {
        return false;
    }
    true
}

/// All groups in declaration order.
pub fn groups() -> &'static [GroupDef] {
    GROUPS
}

/// All entries in declaration order.
pub fn all_entries() -> impl Iterator<Item = &'static EntryDef> {
    GROUPS.iter().flat_map(|g| g.entries.iter())
}

/// Entry whose primary name is `name` (`cAvidaConfig::Get/Set` only match the
/// primary name, not aliases).
pub fn find_entry(name: &str) -> Option<&'static EntryDef> {
    all_entries().find(|e| e.names[0] == name)
}

impl Default for Config {
    fn default() -> Self {
        Config::new()
    }
}

impl Config {
    /// `cAvidaConfig::Get(entry, ret)` / `Get()` of a setting by primary name
    /// (the value as the C++ `AsString()` renders it: doubles as `%f`).
    pub fn get(&self, name: &str) -> Option<String> {
        find_entry(name).map(|e| self.as_string_by_id(e.id))
    }

    /// `cAvidaConfig::HasEntry(entry)`.
    pub fn has_entry(&self, name: &str) -> bool {
        find_entry(name).is_some()
    }

    /// `cAvidaConfig::Set(entry, val)`: load `val` into the setting named
    /// `name` (primary names; custom-format keywords append `"NAME val"` to
    /// that format's list). Returns false for an unknown name.
    pub fn set(&mut self, name: &str, value: &str) -> bool {
        if let Some(e) = find_entry(name) {
            self.load_str_by_id(e.id, value);
            return true;
        }
        for (fi, f) in FORMATS.iter().enumerate() {
            for fe in f.entries {
                if fe.name == name {
                    self.format_values[fi].push(format!("{} {}", fe.name, value));
                    return true;
                }
            }
        }
        false
    }

    /// `cAvidaConfig::Set(Apto::Map& sets)`: apply every pending `(name, value)`
    /// whose name matches a setting under any of its names (aliases too),
    /// removing applied ones from `sets`. Returns the lines the C++ prints to
    /// stdout (`CmdLine Set: NAME value` when VERBOSITY > 1).
    pub fn set_from_map(&mut self, sets: &mut Vec<(String, String)>) -> Vec<String> {
        let mut out = Vec::new();
        for g in GROUPS {
            for e in g.entries {
                for n in e.names {
                    if let Some(pos) = sets.iter().position(|(k, _)| k == n) {
                        let (_, val) = sets.remove(pos);
                        self.load_str_by_id(e.id, &val);
                        if self.verbosity > 1 {
                            out.push(format!("CmdLine Set: {} {}", n, val));
                        }
                        break;
                    }
                }
            }
        }
        out
    }

    /// Iterate over all settings in declaration order with their current values.
    pub fn iter(&self) -> impl Iterator<Item = ConfigEntry> + '_ {
        GROUPS.iter().flat_map(move |g| {
            g.entries.iter().map(move |e| ConfigEntry {
                group: g,
                def: e,
                value: self.as_string_by_id(e.id),
            })
        })
    }

    /// Lines accumulated for the custom format `name` (`cBaseConfigCustomFormat::Get()`),
    /// each of the form `"KEYWORD rest of line"`, in file order.
    pub fn format_lines(&self, name: &str) -> Option<&[String]> {
        FORMATS
            .iter()
            .position(|f| f.name == name)
            .map(|i| self.format_values[i].as_slice())
    }

    /// `cAvidaConfig::Load`. Returns false when the file cannot be opened.
    ///
    /// Messages go to `feedback` when given, with `cUserFeedback::Append`
    /// semantics (see [`Feedback::append`]: the error/warning counters do not
    /// include appended messages). `mappings` are the `-def` variables.
    pub fn load(
        &mut self,
        filename: &str,
        working_dir: &str,
        feedback: Option<&mut Feedback>,
        mappings: Option<&[(String, String)]>,
        warn_default: bool,
    ) -> bool {
        let empty: [(String, String); 0] = [];
        let maps: &[(String, String)] = mappings.unwrap_or(&empty);
        let mut init_file = InitFile::with_mappings(filename, maps, working_dir);

        if !init_file.was_opened() {
            if let Some(fb) = feedback {
                fb.append(init_file.feedback());
                if init_file.was_found() {
                    fb.error(format!("unable to open configuration file '{}'", filename));
                } else {
                    fb.error(format!("configuration file '{}' not found", filename));
                }
            }
            return false;
        }

        let version_id = init_file.read_string("VERSION_ID", "Unknown", warn_default);
        let mut fb_opt = feedback;
        if !check_version_compatibility(&version_id) {
            if let Some(fb) = fb_opt.as_deref_mut() {
                fb.warning(format!(
                    "config file version number mismatch -- Avida: '{}'  File: '{}'",
                    VERSION_STRING, version_id
                ));
            }
        }

        // All groups, then all entries: load each from the file or its default.
        for g in GROUPS {
            for e in g.entries {
                let names: Vec<String> = e.names.iter().map(|s| s.to_string()).collect();
                let v = init_file.read_string_names(&names, e.default, warn_default);
                self.load_str_by_id(e.id, &v);
            }
        }

        // Second pass over the file for custom-format keywords.
        for line_id in 0..init_file.num_lines() {
            let mut cur_line = init_file.get_line(line_id as i32);
            let keyword = pop_word(&mut cur_line);
            // entry_dict.Set(name, entry): a later format overrides an earlier one.
            let mut hit = None;
            for (fi, f) in FORMATS.iter().enumerate() {
                for fe in f.entries {
                    if fe.name == keyword {
                        hit = Some((fi, fe.name));
                    }
                }
            }
            if let Some((fi, name)) = hit {
                self.format_values[fi].push(format!("{} {}", name, cur_line));
                init_file.mark_line_used(line_id);
            }
        }

        if let Some(fb) = fb_opt {
            init_file.warn_unused();
            fb.append(init_file.feedback());
        }
        true
    }

    /// The text `cAvidaConfig::Print` writes: a complete `avida.cfg` listing
    /// every setting with its *default* value (not the current one) and its
    /// description.
    pub fn print_to_string(&self) -> String {
        let mut o = String::new();
        o.push_str(
            "#############################################################################\n",
        );
        o.push_str("# This file includes all the basic run-time definitions for Avida.\n");
        o.push_str("# For more information, see documentation/config.html\n");
        o.push_str(
            "#############################################################################\n",
        );
        o.push('\n');
        o.push_str(&format!(
            "VERSION_ID {}   # Do not change this value.\n",
            VERSION_STRING
        ));

        for g in GROUPS {
            o.push('\n');
            o.push_str(&format!("### {} ###\n", g.name));
            for d in split_list(g.desc, '\n') {
                o.push_str(&format!("# {}\n", d));
            }
            let mut max_width = 0usize;
            for e in g.entries {
                let w = e.names[0].len() + e.default.len() + 1;
                if w > max_width {
                    max_width = w;
                }
            }
            for e in g.entries {
                let cur_width = e.names[0].len() + e.default.len() + 1;
                o.push_str(e.names[0]);
                o.push(' ');
                o.push_str(e.default);
                for _ in cur_width..max_width {
                    o.push(' ');
                }
                if e.desc.is_empty() {
                    o.push_str("  # \n");
                } else {
                    let mut lines = split_list(e.desc, '\n').into_iter();
                    if let Some(first) = lines.next() {
                        o.push_str(&format!("  # {}\n", first));
                    }
                    for l in lines {
                        for _ in 0..max_width {
                            o.push(' ');
                        }
                        o.push_str(&format!("  # {}\n", l));
                    }
                }
            }
        }
        o
    }

    /// `cAvidaConfig::Print(filename)`.
    pub fn print(&self, filename: &str) -> std::io::Result<()> {
        std::fs::write(filename, self.print_to_string())
    }

    /// The text `cAvidaConfig::Status` prints to stdout.
    pub fn status(&self) -> String {
        let mut o = format!("Config contains {} groups.\n", GROUPS.len());
        for g in GROUPS {
            o.push_str(&format!(
                "Group \"{}\" has {} settings.\n",
                g.name,
                g.entries.len()
            ));
        }
        o
    }

    /// The text `cAvidaConfig::PrintReview` prints to stdout: every setting
    /// that differs from its default.
    pub fn print_review(&self) -> String {
        let mut o = String::from("\nNon-Default Settings: \n\n");
        for g in GROUPS {
            for e in g.entries {
                if !self.equals_string_by_id(e.id, e.default) {
                    o.push_str(&format!(" {} ", e.names[0]));
                    let s = self.as_string_by_id(e.id);
                    match e.ty {
                        EntryType::Double => {
                            o.push_str(&format_ostream_double(as_double(&s)));
                            o.push(' ');
                        }
                        EntryType::Int => {
                            o.push_str(&as_int(&s).to_string());
                            o.push(' ');
                        }
                        _ => {
                            o.push_str(&s);
                            o.push(' ');
                        }
                    }
                    o.push_str(&format!("(default={})\n", e.default));
                }
            }
        }
        o.push('\n');
        o
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const HEADER: &str = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/testdata/reference/cAvidaConfig.h"
    );

    fn count_live(header: &str, mac: &str) -> usize {
        header
            .lines()
            .map(|l| l.trim_start())
            .filter(|l| !l.starts_with("//") && !l.starts_with("#define"))
            .map(|l| l.matches(mac).count())
            .sum()
    }

    #[test]
    fn table_is_complete() {
        let h = std::fs::read_to_string(HEADER).expect("header");
        assert_eq!(count_live(&h, "CONFIG_ADD_VAR("), NUM_ENTRIES);
        assert_eq!(count_live(&h, "CONFIG_ADD_VAR("), all_entries().count());
        assert_eq!(count_live(&h, "CONFIG_ADD_GROUP("), GROUPS.len());
        assert_eq!(count_live(&h, "CONFIG_ADD_CUSTOM_FORMAT("), FORMATS.len());
        assert_eq!(
            count_live(&h, "CONFIG_ADD_FORMAT_VAR("),
            FORMATS.iter().map(|f| f.entries.len()).sum::<usize>()
        );
        let aliases: usize = all_entries().map(|e| e.names.len() - 1).sum();
        assert_eq!(count_live(&h, "CONFIG_ADD_ALIAS("), aliases);
        // Every name in the header appears once, in order.
        let mut names = Vec::new();
        for l in h.lines().map(|l| l.trim_start()) {
            if l.starts_with("//") || l.starts_with("#define") {
                continue;
            }
            for m in ["CONFIG_ADD_VAR(", "CONFIG_ADD_ALIAS("] {
                if let Some(p) = l.find(m) {
                    let rest = &l[p + m.len()..];
                    let end = rest.find([',', ')']).unwrap();
                    names.push(rest[..end].trim().to_string());
                }
            }
        }
        let table: Vec<String> = all_entries()
            .flat_map(|e| e.names.iter().map(|n| n.to_string()))
            .collect();
        assert_eq!(names, table);
        // ids are dense and ordered
        for (i, e) in all_entries().enumerate() {
            assert_eq!(e.id, i);
        }
    }

    #[test]
    fn defaults_match_header() {
        let c = Config::new();
        assert_eq!(c.verbosity, 1);
        assert_eq!(c.random_seed, -1);
        assert!(c.speculative);
        assert_eq!(c.world_x, 60);
        assert_eq!(c.world_y, 60);
        assert_eq!(c.world_geometry, 2);
        assert_eq!(c.scale_free_alpha, 1.0);
        assert_eq!(c.data_dir, "data");
        assert_eq!(c.event_file, "events.cfg");
        assert_eq!(c.environment_file, "environment.cfg");
        assert_eq!(c.migration_file, "-");
        assert_eq!(c.copy_mut_prob, 0.0075);
        assert_eq!(c.inst_set, "-");
        assert!(c.instsets().is_empty());
        assert_eq!(c.get("COPY_MUT_PROB").as_deref(), Some("0.007500"));
        assert_eq!(c.get("WORLD_X").as_deref(), Some("60"));
        assert_eq!(c.get("SPECULATIVE").as_deref(), Some("1"));
        assert_eq!(c.get("NOPE"), None);
    }

    #[test]
    fn set_and_get() {
        let mut c = Config::new();
        assert!(c.set("WORLD_X", "30abc"));
        assert_eq!(c.world_x, 30);
        assert!(c.set("COPY_MUT_PROB", "0.5"));
        assert_eq!(c.copy_mut_prob, 0.5);
        assert!(c.set("DATA_DIR", "x y"));
        assert_eq!(c.get("DATA_DIR").as_deref(), Some("x y"));
        assert!(c.set("SPECULATIVE", "0"));
        assert!(!c.speculative);
        assert!(!c.set("NOT_A_SETTING", "1"));
        // aliases are not matched by Set(name, value)
        assert!(!c.set("CHILD_SIZE_RANGE", "1.5"));
        assert!(c.set("INST", "Nop-A"));
        assert_eq!(c.instsets(), &["INST Nop-A".to_string()]);
        assert_eq!(c.format_lines("INSTSETS").unwrap().len(), 1);
    }

    #[test]
    fn alias_map_set() {
        let mut c = Config::new();
        let mut sets = vec![
            ("CHILD_SIZE_RANGE".to_string(), "1.5".to_string()),
            ("WORLD_Y".to_string(), "7".to_string()),
            ("BOGUS".to_string(), "1".to_string()),
        ];
        let out = c.set_from_map(&mut sets);
        assert!(out.is_empty());
        assert_eq!(sets, vec![("BOGUS".to_string(), "1".to_string())]);
        assert_eq!(c.world_y, 7);
        // CHILD_SIZE_RANGE is an alias of OFFSPRING_SIZE_RANGE
        let e = all_entries()
            .find(|e| e.names.contains(&"CHILD_SIZE_RANGE"))
            .unwrap();
        assert_eq!(c.get(e.names[0]).as_deref(), Some("1.500000"));
        c.verbosity = 2;
        let mut sets = vec![("WORLD_X".to_string(), "9".to_string())];
        assert_eq!(
            c.set_from_map(&mut sets),
            vec!["CmdLine Set: WORLD_X 9".to_string()]
        );
    }

    #[test]
    fn version_check() {
        assert!(check_version_compatibility("2.14.0"));
        assert!(check_version_compatibility("2.14"));
        assert!(!check_version_compatibility("2.12.1"));
        assert!(!check_version_compatibility("Unknown"));
        assert!(!check_version_compatibility("2"));
        assert!(!check_version_compatibility(""));
    }

    #[test]
    fn print_roundtrip() {
        let c = Config::new();
        let text = c.print_to_string();
        assert!(text.starts_with("####"));
        assert!(text.contains("VERSION_ID 2.14.0   # Do not change this value.\n"));
        assert!(text.contains("### GENERAL_GROUP ###\n# General Settings\n"));
        assert!(text.contains("\nVERBOSITY 1"));
        // multi-line description continues aligned
        assert!(text.contains("  # 0 = No output at all\n"));
        // Loading what we printed gives back the defaults, without warnings
        let mut dir = std::env::temp_dir();
        dir.push(format!("avida_config_print_{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        c.print(dir.join("avida.cfg").to_str().unwrap()).unwrap();
        let mut d = Config::new();
        let mut fb = Feedback::new();
        assert!(d.load(
            "avida.cfg",
            dir.to_str().unwrap(),
            Some(&mut fb),
            None,
            true
        ));
        assert_eq!(d, c);
        assert_eq!(fb.num_messages(), 0, "{}", fb.render());
        assert_eq!(
            c.status().lines().next(),
            Some("Config contains 40 groups.")
        );
    }

    #[test]
    fn review_output() {
        let mut c = Config::new();
        assert_eq!(c.print_review(), "\nNon-Default Settings: \n\n\n");
        c.set("COPY_MUT_PROB", "0.02");
        c.set("WORLD_X", "20");
        c.set("DATA_DIR", "out");
        let r = c.print_review();
        assert!(r.contains(" WORLD_X 20 (default=60)\n"));
        assert!(r.contains(" COPY_MUT_PROB 0.02 (default=0.0075)\n"));
        assert!(r.contains(" DATA_DIR out (default=data)\n"));
    }
}
