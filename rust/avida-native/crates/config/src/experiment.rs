//! The experiment file (`avida.yaml`): every setting of an experiment, its
//! instruction sets, its environment and its events in one YAML file.
//!
//! This departs from the C++ input format by the maintainer's decision of
//! 2026-11-18 (`PORTING.md`). The settings are grouped as the table in
//! [`crate::naming`] says; every value is still parsed by the C++ text
//! parsers (`cString::AsInt`, `AsDouble`), so a value means what it meant in
//! `avida.cfg`. The three sections are rendered into the line format the
//! C++ read from its own files (`INSTSET`/`INST` lines, environment lines,
//! event lines), which the environment, the instruction-set registry and
//! the event list then load exactly as before. Those parsers keep their C++
//! error messages; what is wrong with the YAML itself (an unknown key, a
//! list where a value belongs) is reported here with its file and line.
//!
//! A section may also be the name of a YAML file that holds just that
//! section under the same key (`events: events-long.yaml`); an instruction
//! set may be a stock set named by its name (`- heads_default`) or a file
//! of sets (`- my-sets.yaml`). There is no general include and no `$NAME`
//! substitution.

use crate::config::EntryType;
use crate::feedback::Feedback;
use crate::init_file::get_absolute_path;
use crate::naming::{self, SettingDef, SettingInfo};
use crate::string_util::as_double;
use crate::yaml::{self, Node, NodeKind};
use crate::Config;

/// The experiment file read when `-c` names no other.
pub const DEFAULT_FILE: &str = "avida.yaml";

/// The page that describes the file.
pub const REFERENCE_PAGE: &str = "docs/reference/config-yaml.md";

/// The stock experiment file (`avida-core/support/config/avida.yaml`),
/// which `--generate-config` writes.
pub const DEFAULT_EXPERIMENT: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../../../avida-core/support/config/avida.yaml"
));

/// What the experiment file holds besides the settings, in the line format
/// of the C++ files. (The instruction sets go to the `INSTSETS` list of the
/// configuration, where `avida.cfg` put them too.)
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Experiment {
    /// The experiment file read, as named on the command line; empty when
    /// the configuration did not come from one.
    pub file: String,
    /// The environment, one C++ environment-file line per entry.
    pub environment: Vec<String>,
    /// The file the environment came from (the experiment file or a
    /// section file).
    pub environment_file: String,
    /// The events, one C++ event-file line per entry.
    pub events: Vec<String>,
    /// The file the events came from.
    pub events_file: String,
    /// The settings the file gave (`group.key`), in file order.
    pub given: Vec<String>,
}

impl Experiment {
    /// Whether the configuration was read from an experiment file.
    pub fn is_loaded(&self) -> bool {
        !self.file.is_empty()
    }
}

/// `x` as text that `cString::AsDouble` reads back to the same bits.
pub fn exact_double(x: f64) -> String {
    let s = format!("{x:?}");
    if as_double(&s).to_bits() == x.to_bits() {
        return s;
    }
    format!("{x:.17e}")
}

// ---------------------------------------------------------------------------
// Known keys of the sections (the keys the C++ parsers accept)
// ---------------------------------------------------------------------------

/// Keys of `environment`.
pub const ENVIRONMENT_KEYS: &[&str] = &["resources", "grids", "reactions", "set_active"];
/// Kinds of resource (`kind:`); a plain resource has none.
pub const RESOURCE_KINDS: &[&str] = &["cell", "gradient"];
/// Keys of a plain resource (`RESOURCE`).
pub const RESOURCE_KEYS: &[&str] = &[
    "inflow",
    "outflow",
    "initial",
    "geometry",
    "cells",
    "inflowx1",
    "inflowx",
    "inflowx2",
    "inflowy1",
    "inflowy",
    "inflowy2",
    "outflowx1",
    "outflowx",
    "outflowx2",
    "outflowy1",
    "outflowy",
    "outflowy2",
    "xdiffuse",
    "xgravity",
    "ydiffuse",
    "ygravity",
    "deme",
    "collectable",
    "energy",
    "hgt",
];
/// Keys of a cell resource (`CELL`) besides `cells`.
pub const CELL_KEYS: &[&str] = &["initial", "inflow", "outflow"];
/// Keys of a gradient resource (`GRADIENT_RESOURCE`).
pub const GRADIENT_KEYS: &[&str] = &[
    "peakx",
    "peaky",
    "height",
    "spread",
    "plateau",
    "decay",
    "max_x",
    "max_y",
    "min_x",
    "min_y",
    "move_a_scaler",
    "updatestep",
    "halo",
    "halo_inner_radius",
    "halo_anchor_x",
    "halo_anchor_y",
    "move_speed",
    "move_resistance",
    "halo_width",
    "plateau_inflow",
    "plateau_outflow",
    "cone_inflow",
    "cone_outflow",
    "gradient_inflow",
    "initial",
    "common",
    "floor",
    "habitat",
    "min_size",
    "max_size",
    "config",
    "count",
    "resistance",
    "damage",
    "deadly",
    "path",
    "hammer",
    "threshold",
    "refuge",
];
/// Keys of a state grid (`GRID`) besides `name`.
pub const GRID_KEYS: &[&str] = &[
    "width",
    "height",
    "initx",
    "inity",
    "initfacing",
    "states",
    "grid",
];
/// Keys of a reaction.
pub const REACTION_KEYS: &[&str] = &[
    "name",
    "task",
    "args",
    "process",
    "requisite",
    "context_requisite",
];
/// Keys of a reaction process.
pub const PROCESS_KEYS: &[&str] = &[
    "resource",
    "value",
    "type",
    "max",
    "min",
    "frac",
    "ksubm",
    "product",
    "conversion",
    "inst",
    "random",
    "lethal",
    "sterilize",
    "deme",
    "germline",
    "detect",
    "threshold",
    "detectionerror",
    "string",
    "depletable",
    "phenplastbonus",
    "internal",
];
/// Keys of a reaction requisite (`cellbox` only outside a context requisite).
pub const REQUISITE_KEYS: &[&str] = &[
    "reaction",
    "noreaction",
    "min_count",
    "max_count",
    "reaction_min_count",
    "reaction_max_count",
    "divide_only",
    "min_tot_count",
    "max_tot_count",
    "parasite_only",
    "cellbox",
];
/// Keys of a `set_active` entry.
pub const SET_ACTIVE_KEYS: &[&str] = &["type", "name", "active"];
/// Keys of an event.
pub const EVENT_KEYS: &[&str] = &["trigger", "at", "every", "until", "action", "args"];
/// Event triggers with the letter of the C++ event-file format.
pub const TRIGGERS: &[(&str, &str)] = &[
    ("update", "u"),
    ("generation", "g"),
    ("births", "b"),
    ("org_id", "o"),
    ("immediate", "i"),
];
/// Keys of an instruction set.
pub const INST_SET_KEYS: &[&str] = &[
    "name",
    "hardware_type",
    "stack_size",
    "uops_per_cycle",
    "instructions",
];
/// Keys of an instruction (besides `name`) with their defaults.
pub const INST_KEYS: &[(&str, &str)] = &[
    ("redundancy", "1.0"),
    ("cost", "0"),
    ("initial_cost", "0"),
    ("energy_cost", "0"),
    ("addl_time_cost", "0"),
    ("female_cost", "0"),
    ("choosy_female_cost", "0"),
    ("post_cost", "0"),
    ("prob_fail", "0.0"),
    ("res_cost", "0.0"),
    ("fem_res_cost", "0.0"),
    ("bonus_cost", "0.0"),
    ("inst_code", ""),
];

// ---------------------------------------------------------------------------
// Stock instruction sets
// ---------------------------------------------------------------------------

macro_rules! stock {
    ($name:literal, $file:literal) => {
        StockSet {
            name: $name,
            file: $file,
            text: include_str!(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/../../../../avida-core/support/config/",
                $file
            )),
        }
    };
}

/// An instruction set that ships with Avida and can be named in an
/// experiment file without a copy of it.
#[derive(Debug, Clone, Copy)]
pub struct StockSet {
    /// The set's name.
    pub name: &'static str,
    /// Its file in `avida-core/support/config`.
    pub file: &'static str,
    /// The file's text (an `instruction_sets:` section with one set).
    pub text: &'static str,
}

/// The stock instruction sets.
pub static STOCK_SETS: &[StockSet] = &[
    stock!("heads_default", "instset-heads.yaml"),
    stock!("heads_sex", "instset-heads-sex.yaml"),
    stock!("heads_modular", "instset-heads-modular.yaml"),
    stock!("smt", "instset-smt.yaml"),
    stock!("transsmt", "instset-transsmt.yaml"),
    stock!("experimental", "instset-experimental.yaml"),
];

/// The stock set called `name`.
pub fn stock_set(name: &str) -> Option<&'static StockSet> {
    STOCK_SETS.iter().find(|s| s.name == name)
}

// ---------------------------------------------------------------------------
// Messages
// ---------------------------------------------------------------------------

/// Levenshtein distance.
fn distance(a: &str, b: &str) -> usize {
    let b: Vec<char> = b.chars().collect();
    let mut prev: Vec<usize> = (0..=b.len()).collect();
    for (i, ca) in a.chars().enumerate() {
        let mut cur = vec![i + 1; b.len() + 1];
        for (j, cb) in b.iter().enumerate() {
            let sub = prev[j] + usize::from(ca != *cb);
            cur[j + 1] = sub.min(prev[j + 1] + 1).min(cur[j] + 1);
        }
        prev = cur;
    }
    prev[b.len()]
}

/// The candidate closest to `word`, when it is close enough to be a
/// likely typo.
pub fn nearest<'a>(word: &str, candidates: impl IntoIterator<Item = &'a str>) -> Option<&'a str> {
    let lower = word.to_ascii_lowercase();
    let mut best: Option<(usize, &str)> = None;
    for c in candidates {
        let d = distance(&lower, c);
        if best.is_none_or(|(bd, _)| d < bd) {
            best = Some((d, c));
        }
    }
    let (d, c) = best?;
    let limit = (word.chars().count() / 3).max(2);
    (d <= limit).then_some(c)
}

fn did_you_mean(s: Option<&str>) -> String {
    match s {
        Some(s) => format!("; did you mean '{s}'?"),
        None => String::new(),
    }
}

/// The message for a file of the old format.
pub fn old_format_message(file: &str) -> String {
    format!(
        "'{file}' is in the old configuration format; Avida now reads one YAML file per \
         experiment (avida.yaml). Convert the folder with 'avida convert DIR' (see {REFERENCE_PAGE})."
    )
}

/// The message for an unknown `group.key` (or a bare name) with the most
/// likely intended path.
pub fn unknown_setting_message(path: &str) -> String {
    let guess = guess_path(path);
    format!("unknown setting '{path}'{}", did_you_mean(guess.as_deref()))
}

fn guess_path(path: &str) -> Option<String> {
    // An old setting name.
    if let Some(new) = naming::new_name_of(&path.to_ascii_uppercase()) {
        return Some(new);
    }
    let (group, key) = path.split_once('.').unwrap_or(("", path));
    if let Some(new) = naming::new_name_of(&key.to_ascii_uppercase()) {
        return Some(new);
    }
    if naming::is_group(group) {
        let keys: Vec<&str> = naming::settings_of_group(group)
            .iter()
            .map(|s| s.key)
            .collect();
        if let Some(k) = nearest(key, keys) {
            return Some(format!("{group}.{k}"));
        }
    }
    // The same key in another group.
    if let Some(s) = naming::settings().iter().find(|s| s.key == key) {
        return Some(s.path());
    }
    let paths = naming::all_paths();
    nearest(path, paths.iter().map(String::as_str)).map(str::to_string)
}

// ---------------------------------------------------------------------------
// Loading
// ---------------------------------------------------------------------------

/// Where the node being read came from, for messages.
struct Ctx<'a> {
    file: String,
    working_dir: &'a str,
}

impl Ctx<'_> {
    fn at(&self, line: usize, msg: impl std::fmt::Display) -> String {
        format!("{}:{}: {}", self.file, line, msg)
    }
}

/// Whether `text` looks like a file of the old line format (`NAME value`,
/// `REACTION ...`, `u begin Inject ...`) rather than YAML.
pub fn looks_like_old_format(text: &str) -> bool {
    for line in text.lines() {
        let t = line.trim();
        if t.is_empty() || t.starts_with('#') {
            continue;
        }
        let first = t.split_whitespace().next().unwrap_or("");
        if first.ends_with(':') || first.starts_with('-') || first.starts_with('{') {
            return false;
        }
        return t.contains(char::is_whitespace)
            || first.bytes().all(|c| c.is_ascii_uppercase() || c == b'_');
    }
    false
}

/// Reads a YAML file relative to the working directory. `Ok(None)` when it
/// does not exist.
fn read_yaml(name: &str, working_dir: &str) -> Result<Option<Node>, String> {
    let path = get_absolute_path(name, working_dir);
    let text = match std::fs::read_to_string(&path) {
        Ok(t) => t,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(e) => return Err(format!("unable to read '{name}': {e}")),
    };
    if name.ends_with(".cfg") || looks_like_old_format(&text) {
        return Err(old_format_message(name));
    }
    match yaml::parse(&text) {
        Ok(Some(n)) => Ok(Some(n)),
        Ok(None) => Ok(Some(Node {
            kind: NodeKind::Map(Vec::new()),
            line: 1,
        })),
        Err(e) => Err(format!("{name}:{e}")),
    }
}

impl Config {
    /// Reads the experiment file `filename` (relative to `working_dir`).
    /// Returns false, with the messages in `feedback`, when it cannot be
    /// read or holds anything that is not a setting or a valid section.
    pub fn load_experiment(
        &mut self,
        filename: &str,
        working_dir: &str,
        feedback: &mut Feedback,
    ) -> bool {
        let root = match read_yaml(filename, working_dir) {
            Ok(Some(root)) => root,
            Ok(None) => {
                feedback.error(format!("unable to open file '{filename}'."));
                feedback.error(format!("configuration file '{filename}' not found"));
                if std::path::Path::new(&get_absolute_path("avida.cfg", working_dir)).is_file() {
                    feedback.error(old_format_message("avida.cfg"));
                }
                return false;
            }
            Err(e) => {
                feedback.error(e);
                return false;
            }
        };
        let ctx = Ctx {
            file: filename.to_string(),
            working_dir,
        };
        let mut errors = Vec::new();
        self.experiment.file = filename.to_string();
        self.experiment.environment_file = filename.to_string();
        self.experiment.events_file = filename.to_string();
        let Some(map) = root.as_map() else {
            feedback.error(ctx.at(
                root.line,
                "an experiment file is a mapping of groups and sections",
            ));
            return false;
        };
        for (key, node) in map {
            let k = key.text.as_str();
            if naming::is_group(k) {
                self.load_group(&ctx, k, node, &mut errors);
            } else if k == naming::SECTION_INSTRUCTION_SETS {
                match section(&ctx, k, node).and_then(|(c, n)| render_instruction_sets(&c, &n, 0)) {
                    Ok(lines) => self.push_inst_set_lines(&lines),
                    Err(e) => errors.extend(e),
                }
            } else if k == naming::SECTION_ENVIRONMENT {
                match section(&ctx, k, node)
                    .and_then(|(c, n)| render_environment(&c, &n).map(|l| (c.file, l)))
                {
                    Ok((file, lines)) => {
                        self.experiment.environment = lines;
                        self.experiment.environment_file = file;
                    }
                    Err(e) => errors.extend(e),
                }
            } else if k == naming::SECTION_EVENTS {
                match section(&ctx, k, node)
                    .and_then(|(c, n)| render_events(&c, &n).map(|l| (c.file, l)))
                {
                    Ok((file, lines)) => {
                        self.experiment.events = lines;
                        self.experiment.events_file = file;
                    }
                    Err(e) => errors.extend(e),
                }
            } else {
                errors.push(ctx.at(key.line, unknown_top_level_message(k)));
            }
        }
        for e in &errors {
            feedback.error(e.clone());
        }
        errors.is_empty()
    }

    fn load_group(&mut self, ctx: &Ctx<'_>, group: &str, node: &Node, errors: &mut Vec<String>) {
        if node.is_null() {
            return;
        }
        let Some(map) = node.as_map() else {
            errors.push(ctx.at(
                node.line,
                format!("group '{group}' must be a mapping of settings"),
            ));
            return;
        };
        for (key, value) in map {
            let path = format!("{group}.{}", key.text);
            let Some(info) = naming::setting_in_group(group, &key.text) else {
                errors.push(ctx.at(key.line, unknown_setting_message(&path)));
                continue;
            };
            let text = match &value.kind {
                NodeKind::Scalar { text, .. } => text.clone(),
                NodeKind::Null => {
                    errors.push(ctx.at(
                        value.line,
                        format!(
                            "setting '{path}' has no value (leave it out to keep its default, {})",
                            info.default()
                        ),
                    ));
                    continue;
                }
                _ => {
                    errors.push(ctx.at(
                        value.line,
                        format!(
                            "setting '{path}' takes a single value, not {}",
                            value.kind_name()
                        ),
                    ));
                    continue;
                }
            };
            match setting_value(&info, &text) {
                Ok(v) => {
                    self.apply_setting(&info, &v);
                    self.experiment.given.push(path);
                }
                Err(e) => errors.push(ctx.at(value.line, e)),
            }
        }
    }

    /// Loads `text` into the setting `info` (with the C++ parser of its
    /// type).
    pub fn apply_setting(&mut self, info: &SettingInfo, text: &str) {
        match info.def {
            SettingDef::Cpp(e) => self.load_str_by_id(e.id, text),
            SettingDef::Port(p) => {
                self.port.load_str(p.name, text);
            }
        }
    }

    /// The value of a setting as text that loads back to the same value.
    pub fn setting_text(&self, info: &SettingInfo) -> String {
        match info.def {
            SettingDef::Cpp(e) => self.exact_string_by_id(e.id),
            SettingDef::Port(p) => self.port.as_string(p.name).unwrap_or_default(),
        }
    }

    fn push_inst_set_lines(&mut self, lines: &[String]) {
        for l in lines {
            let mut rest = l.clone();
            let kw = crate::string_util::pop_word(&mut rest);
            self.set(&kw, &rest);
        }
    }

    /// `-set PATH VALUE` on an experiment: a setting by its dotted path, or
    /// a section from a file (`environment`, `events`; `instruction_sets`
    /// also takes a stock set's name). Returns an error message for an
    /// unknown path or a value that cannot be used.
    pub fn set_path(&mut self, path: &str, value: &str, working_dir: &str) -> Result<(), String> {
        let ctx = Ctx {
            file: "-set".to_string(),
            working_dir,
        };
        let node = Node {
            kind: NodeKind::Scalar {
                text: value.to_string(),
                quoted: false,
            },
            line: 0,
        };
        let join = |e: Vec<String>| e.join("\n");
        match path {
            naming::SECTION_INSTRUCTION_SETS => {
                let item = Node {
                    kind: NodeKind::Seq(vec![node]),
                    line: 0,
                };
                let lines = render_instruction_sets(&ctx, &item, 0).map_err(join)?;
                self.clear_inst_sets();
                self.push_inst_set_lines(&lines);
                Ok(())
            }
            naming::SECTION_ENVIRONMENT => {
                let (c, n) = section(&ctx, path, &node).map_err(join)?;
                self.experiment.environment = render_environment(&c, &n).map_err(join)?;
                self.experiment.environment_file = c.file;
                Ok(())
            }
            naming::SECTION_EVENTS => {
                let (c, n) = section(&ctx, path, &node).map_err(join)?;
                self.experiment.events = render_events(&c, &n).map_err(join)?;
                self.experiment.events_file = c.file;
                Ok(())
            }
            _ => {
                let info =
                    naming::setting_by_path(path).ok_or_else(|| unknown_setting_message(path))?;
                let v = setting_value(&info, value)?;
                self.apply_setting(&info, &v);
                Ok(())
            }
        }
    }

    fn clear_inst_sets(&mut self) {
        for list in &mut self.format_values {
            list.clear();
        }
    }
}

fn unknown_top_level_message(key: &str) -> String {
    if let Some(new) = naming::new_name_of(&key.to_ascii_uppercase()) {
        if key.bytes().any(|c| c.is_ascii_uppercase()) {
            return format!(
                "'{key}' is a setting name of the old format; in this file it is '{new}'"
            );
        }
    }
    if let Some(n) = nearest(key, naming::top_level_keys()) {
        return format!("unknown group or section '{key}'; did you mean '{n}'?");
    }
    if let Some(s) = naming::settings().iter().find(|s| s.key == key) {
        return format!(
            "unknown group or section '{key}'; settings belong in their group: did you mean '{}'?",
            s.path()
        );
    }
    format!("unknown group or section '{key}'")
}

/// The text loaded into a setting: `true` and `false` stand for 1 and 0
/// where the setting is an integer or a flag; anything else must start the
/// way a number does where the setting is a number (the C++ parser read a
/// word as 0).
pub fn setting_value(info: &SettingInfo, text: &str) -> Result<String, String> {
    let ty = info.ty();
    if matches!(ty, EntryType::Int | EntryType::Bool) {
        match text {
            "true" | "True" | "TRUE" => return Ok("1".into()),
            "false" | "False" | "FALSE" => return Ok("0".into()),
            _ => {}
        }
    }
    if matches!(ty, EntryType::Int | EntryType::Double | EntryType::Bool)
        && !starts_like_number(text)
    {
        return Err(format!(
            "setting '{}' takes a number, not '{text}'",
            info.path()
        ));
    }
    Ok(text.to_string())
}

fn starts_like_number(text: &str) -> bool {
    let t = text.trim_start();
    let t = t.strip_prefix(['+', '-']).unwrap_or(t);
    let lower = t.to_ascii_lowercase();
    if lower.starts_with("inf") || lower.starts_with("nan") {
        return true;
    }
    let b = t.as_bytes();
    match b.first() {
        Some(c) if c.is_ascii_digit() => true,
        Some(b'.') => b.get(1).is_some_and(u8::is_ascii_digit),
        _ => false,
    }
}

/// A section's node: inline, or the same key of the YAML file it names.
fn section<'a>(ctx: &Ctx<'a>, name: &str, node: &Node) -> Result<(Ctx<'a>, Node), Vec<String>> {
    let Some(file) = node.as_str() else {
        return Ok((
            Ctx {
                file: ctx.file.clone(),
                working_dir: ctx.working_dir,
            },
            node.clone(),
        ));
    };
    let root = match read_yaml(file, ctx.working_dir) {
        Ok(Some(r)) => r,
        Ok(None) => {
            return Err(vec![
                ctx.at(node.line, format!("cannot open the {name} file '{file}'"))
            ])
        }
        Err(e) => return Err(vec![ctx.at(node.line, e)]),
    };
    let sub = Ctx {
        file: file.to_string(),
        working_dir: ctx.working_dir,
    };
    let Some(map) = root.as_map() else {
        return Err(vec![sub.at(
            root.line,
            format!("a {name} file is a mapping with the key '{name}'"),
        )]);
    };
    let mut found = None;
    let mut errors = Vec::new();
    for (k, v) in map {
        if k.text == name {
            found = Some(v.clone());
        } else {
            errors.push(sub.at(
                k.line,
                format!("a {name} file holds only '{name}', not '{}'", k.text),
            ));
        }
    }
    if !errors.is_empty() {
        return Err(errors);
    }
    match found {
        Some(n) if n.as_str().is_some() => Err(vec![
            sub.at(n.line, format!("the {name} file names another file"))
        ]),
        Some(n) => Ok((sub, n)),
        None => Ok((
            sub,
            Node {
                kind: NodeKind::Null,
                line: root.line,
            },
        )),
    }
}

// ---------------------------------------------------------------------------
// Rendering the sections into the C++ line formats
// ---------------------------------------------------------------------------

/// A value inside an environment or instruction line: one word without the
/// `:` that separates the entries.
fn word(ctx: &Ctx<'_>, node: &Node, what: &str) -> Result<String, String> {
    let text = match &node.kind {
        NodeKind::Scalar { text, .. } => text.clone(),
        NodeKind::Null => return Err(ctx.at(node.line, format!("{what} has no value"))),
        _ => {
            return Err(ctx.at(
                node.line,
                format!("{what} takes a single value, not {}", node.kind_name()),
            ))
        }
    };
    if text.is_empty() {
        return Err(ctx.at(node.line, format!("{what} has an empty value")));
    }
    if text.contains(':') || text.contains(char::is_whitespace) {
        return Err(ctx.at(
            node.line,
            format!("{what} cannot hold ':' or spaces: '{text}'"),
        ));
    }
    Ok(text)
}

fn map_of<'n>(
    ctx: &Ctx<'_>,
    node: &'n Node,
    what: &str,
) -> Result<&'n [(yaml::Key, Node)], String> {
    match &node.kind {
        NodeKind::Map(m) => Ok(m),
        NodeKind::Null => Ok(&[]),
        _ => Err(ctx.at(
            node.line,
            format!("{what} must be a mapping, not {}", node.kind_name()),
        )),
    }
}

fn seq_of<'n>(ctx: &Ctx<'_>, node: &'n Node, what: &str) -> Result<&'n [Node], String> {
    match &node.kind {
        NodeKind::Seq(s) => Ok(s),
        NodeKind::Null => Ok(&[]),
        _ => Err(ctx.at(
            node.line,
            format!("{what} must be a list, not {}", node.kind_name()),
        )),
    }
}

fn check_key(ctx: &Ctx<'_>, key: &yaml::Key, known: &[&str], what: &str) -> Result<(), String> {
    if known.contains(&key.text.as_str()) {
        return Ok(());
    }
    Err(ctx.at(
        key.line,
        format!(
            "unknown key '{}' in {what}{}",
            key.text,
            did_you_mean(nearest(&key.text, known.iter().copied()))
        ),
    ))
}

/// `k=v` entries of a mapping (a list value repeats its key), skipping
/// `skip`, with every key checked against `known`.
fn entries(
    ctx: &Ctx<'_>,
    map: &[(yaml::Key, Node)],
    known: &[&str],
    skip: &[&str],
    what: &str,
) -> Result<Vec<String>, String> {
    let mut out = Vec::new();
    for (k, v) in map {
        if skip.contains(&k.text.as_str()) {
            continue;
        }
        check_key(ctx, k, known, what)?;
        let label = format!("'{}' of {what}", k.text);
        match &v.kind {
            NodeKind::Seq(items) => {
                for item in items {
                    out.push(format!("{}={}", k.text, word(ctx, item, &label)?));
                }
            }
            _ => out.push(format!("{}={}", k.text, word(ctx, v, &label)?)),
        }
    }
    Ok(out)
}

fn required_word(
    ctx: &Ctx<'_>,
    node: &Node,
    map: &[(yaml::Key, Node)],
    key: &str,
    what: &str,
) -> Result<String, String> {
    match map.iter().find(|(k, _)| k.text == key) {
        Some((_, v)) => word(ctx, v, &format!("the {key} of {what}")),
        None => Err(ctx.at(node.line, format!("{what} needs a '{key}'"))),
    }
}

fn field<'n>(map: &'n [(yaml::Key, Node)], key: &str) -> Option<&'n Node> {
    map.iter().find(|(k, _)| k.text == key).map(|(_, v)| v)
}

/// Renders an `environment` section into environment-file lines: the
/// resources (plain, cell and gradient in their order), the state grids,
/// the reactions, then `set_active`.
pub fn render_environment_node(
    node: &Node,
    file: &str,
    working_dir: &str,
) -> Result<Vec<String>, Vec<String>> {
    render_environment(
        &Ctx {
            file: file.to_string(),
            working_dir,
        },
        node,
    )
}

fn render_environment(ctx: &Ctx<'_>, node: &Node) -> Result<Vec<String>, Vec<String>> {
    let map = map_of(ctx, node, "the environment").map_err(|e| vec![e])?;
    let mut errors = Vec::new();
    for (k, _) in map {
        if let Err(e) = check_key(ctx, k, ENVIRONMENT_KEYS, "the environment") {
            errors.push(e);
        }
    }
    let mut lines = Vec::new();
    let mut each = |key: &str, f: &dyn Fn(&Node) -> Result<String, String>| {
        if let Some(list) = field(map, key) {
            match seq_of(ctx, list, &format!("'{key}'")) {
                Ok(items) => {
                    for item in items {
                        match f(item) {
                            Ok(l) => lines.push(l),
                            Err(e) => errors.push(e),
                        }
                    }
                }
                Err(e) => errors.push(e),
            }
        }
    };
    each("resources", &|n| render_resource(ctx, n));
    each("grids", &|n| render_grid(ctx, n));
    each("reactions", &|n| render_reaction(ctx, n));
    each("set_active", &|n| render_set_active(ctx, n));
    if errors.is_empty() {
        Ok(lines)
    } else {
        Err(errors)
    }
}

fn render_resource(ctx: &Ctx<'_>, node: &Node) -> Result<String, String> {
    let map = map_of(ctx, node, "a resource")?;
    let name = required_word(ctx, node, map, "name", "a resource")?;
    let what = format!("resource '{name}'");
    let kind = match field(map, "kind") {
        None => None,
        Some(v) => {
            let k = word(ctx, v, &format!("the kind of {what}"))?;
            if !RESOURCE_KINDS.contains(&k.as_str()) {
                return Err(ctx.at(
                    v.line,
                    format!(
                        "unknown kind '{k}' of {what} (a plain resource has no kind; the others are 'cell' and 'gradient'){}",
                        did_you_mean(nearest(&k, RESOURCE_KINDS.iter().copied()))
                    ),
                ));
            }
            Some(k)
        }
    };
    let mut parts = vec![name];
    let (keyword, known): (&str, &[&str]) = match kind.as_deref() {
        Some("cell") => {
            // The cell list comes first, without a key.
            let cells = match field(map, "cells") {
                Some(v) => match &v.kind {
                    NodeKind::Scalar { text, .. } if text.is_empty() => String::new(),
                    _ => word(ctx, v, &format!("the cells of {what}"))?,
                },
                None => String::new(),
            };
            parts.push(cells);
            ("CELL", CELL_KEYS)
        }
        Some(_) => ("GRADIENT_RESOURCE", GRADIENT_KEYS),
        None => ("RESOURCE", RESOURCE_KEYS),
    };
    let skip: &[&str] = if keyword == "CELL" {
        &["name", "kind", "cells"]
    } else {
        &["name", "kind"]
    };
    parts.extend(entries(ctx, map, known, skip, &what)?);
    if keyword == "CELL" && parts.len() == 2 && parts[1].is_empty() {
        parts.pop();
    }
    Ok(format!("{keyword} {}", parts.join(":")))
}

fn render_grid(ctx: &Ctx<'_>, node: &Node) -> Result<String, String> {
    let map = map_of(ctx, node, "a grid")?;
    let name = required_word(ctx, node, map, "name", "a grid")?;
    let what = format!("grid '{name}'");
    let mut parts = vec![name];
    parts.extend(entries(ctx, map, GRID_KEYS, &["name"], &what)?);
    Ok(format!("GRID {}", parts.join(":")))
}

fn render_reaction(ctx: &Ctx<'_>, node: &Node) -> Result<String, String> {
    let map = map_of(ctx, node, "a reaction")?;
    for (k, _) in map {
        check_key(ctx, k, REACTION_KEYS, "a reaction")?;
    }
    let name = match field(map, "name") {
        Some(v) => {
            let n = word(ctx, v, "the name of a reaction")?;
            if n.contains(char::is_whitespace) {
                return Err(ctx.at(v.line, "a reaction name cannot hold spaces"));
            }
            n
        }
        None => return Err(ctx.at(node.line, "a reaction needs a 'name'")),
    };
    let what = format!("reaction '{name}'");
    let Some(task_node) = field(map, "task") else {
        if map.len() > 1 {
            return Err(ctx.at(node.line, format!("{what} needs a 'task'")));
        }
        // A name alone declares the reaction before its definition.
        return Ok(format!("REACTION {name}"));
    };
    let mut trigger = word(ctx, task_node, &format!("the task of {what}"))?;
    if let Some(args) = field(map, "args") {
        let text = match &args.kind {
            NodeKind::Map(m) => {
                let mut items = Vec::new();
                for (k, v) in m {
                    let v = word(ctx, v, &format!("task argument '{}' of {what}", k.text))?;
                    items.push(format!("{}={v}", k.text));
                }
                items.join(",")
            }
            NodeKind::Scalar { text, .. } => {
                if text.contains(char::is_whitespace) {
                    return Err(ctx.at(
                        args.line,
                        format!("the task arguments of {what} cannot hold spaces"),
                    ));
                }
                text.clone()
            }
            NodeKind::Null => String::new(),
            NodeKind::Seq(_) => {
                return Err(ctx.at(
                    args.line,
                    format!("the task arguments of {what} are a mapping or a single value"),
                ))
            }
        };
        trigger.push(':');
        trigger.push_str(&text);
    }
    let mut words = vec![name.clone(), trigger];
    for (part, known) in [
        ("process", PROCESS_KEYS),
        ("requisite", REQUISITE_KEYS),
        ("context_requisite", REQUISITE_KEYS),
    ] {
        let Some(v) = field(map, part) else {
            continue;
        };
        let pwhat = format!("a {} of {what}", part.replace('_', " "));
        let blocks: Vec<&Node> = match &v.kind {
            NodeKind::Seq(items) => items.iter().collect(),
            _ => vec![v],
        };
        for b in blocks {
            let m = map_of(ctx, b, &pwhat)?;
            let e = entries(ctx, m, known, &[], &pwhat)?;
            words.push(format!("{part}:{}", e.join(":")));
        }
    }
    Ok(format!("REACTION {}", words.join(" ")))
}

fn render_set_active(ctx: &Ctx<'_>, node: &Node) -> Result<String, String> {
    let map = map_of(ctx, node, "a set_active entry")?;
    for (k, _) in map {
        check_key(ctx, k, SET_ACTIVE_KEYS, "a set_active entry")?;
    }
    let ty = required_word(ctx, node, map, "type", "a set_active entry")?;
    let name = required_word(ctx, node, map, "name", "a set_active entry")?;
    let active = match field(map, "active") {
        Some(v) => word(ctx, v, "the active flag of a set_active entry")?,
        None => "1".into(),
    };
    Ok(format!("SET_ACTIVE {ty} {name} {active}"))
}

/// Renders an `events` section into event-file lines.
pub fn render_events_node(
    node: &Node,
    file: &str,
    working_dir: &str,
) -> Result<Vec<String>, Vec<String>> {
    render_events(
        &Ctx {
            file: file.to_string(),
            working_dir,
        },
        node,
    )
}

fn render_events(ctx: &Ctx<'_>, node: &Node) -> Result<Vec<String>, Vec<String>> {
    let items = seq_of(ctx, node, "the events").map_err(|e| vec![e])?;
    let mut lines = Vec::new();
    let mut errors = Vec::new();
    for item in items {
        match render_event(ctx, item) {
            Ok(l) => lines.push(l),
            Err(e) => errors.push(e),
        }
    }
    if errors.is_empty() {
        Ok(lines)
    } else {
        Err(errors)
    }
}

fn plain_word(ctx: &Ctx<'_>, node: &Node, what: &str) -> Result<String, String> {
    match &node.kind {
        NodeKind::Scalar { text, .. }
            if !text.is_empty() && !text.contains(char::is_whitespace) =>
        {
            Ok(text.clone())
        }
        _ => Err(ctx.at(node.line, format!("{what} is one word"))),
    }
}

fn render_event(ctx: &Ctx<'_>, node: &Node) -> Result<String, String> {
    let map = map_of(ctx, node, "an event")?;
    if map.is_empty() {
        return Err(ctx.at(node.line, "an event needs an 'action'"));
    }
    for (k, _) in map {
        check_key(ctx, k, EVENT_KEYS, "an event")?;
    }
    let action = match field(map, "action") {
        Some(v) => plain_word(ctx, v, "the action of an event")?,
        None => return Err(ctx.at(node.line, "an event needs an 'action'")),
    };
    let trigger = match field(map, "trigger") {
        None => "u",
        Some(v) => {
            let t = plain_word(ctx, v, "the trigger of an event")?;
            match TRIGGERS.iter().find(|(name, _)| *name == t) {
                Some((_, letter)) => letter,
                None => {
                    return Err(ctx.at(
                        v.line,
                        format!(
                            "unknown event trigger '{t}' (update, generation, births, org_id or immediate){}",
                            did_you_mean(nearest(&t, TRIGGERS.iter().map(|(n, _)| *n)))
                        ),
                    ))
                }
            }
        }
    };
    let args = match field(map, "args") {
        None => String::new(),
        Some(v) => match &v.kind {
            NodeKind::Scalar { text, .. } => text.clone(),
            NodeKind::Null => String::new(),
            NodeKind::Seq(items) => {
                let mut words = Vec::new();
                for i in items {
                    match i.as_str() {
                        Some(s) => words.push(s.to_string()),
                        None => return Err(ctx.at(i.line, "an event argument is a single value")),
                    }
                }
                words.join(" ")
            }
            NodeKind::Map(_) => {
                return Err(ctx.at(v.line, "the args of an event are text or a list"))
            }
        },
    };
    if args.contains(['\n', '\r']) {
        return Err(ctx.at(node.line, "the args of an event are one line"));
    }
    let mut line = String::from(trigger);
    if trigger == "i" {
        for key in ["at", "every", "until"] {
            if let Some(v) = field(map, key) {
                return Err(ctx.at(v.line, format!("an immediate event has no '{key}'")));
            }
        }
    } else {
        let mut timing = match field(map, "at") {
            Some(v) => timing_part(ctx, v, "at")?,
            None => "begin".to_string(),
        };
        match (field(map, "every"), field(map, "until")) {
            (None, Some(u)) => return Err(ctx.at(u.line, "'until' needs 'every'")),
            (Some(e), u) => {
                timing.push(':');
                timing.push_str(&timing_part(ctx, e, "every")?);
                if let Some(u) = u {
                    timing.push(':');
                    timing.push_str(&timing_part(ctx, u, "until")?);
                }
            }
            (None, None) => {}
        }
        line.push(' ');
        line.push_str(&timing);
    }
    line.push(' ');
    line.push_str(&action);
    if !args.is_empty() {
        line.push(' ');
        line.push_str(&args);
    }
    Ok(line)
}

fn timing_part(ctx: &Ctx<'_>, node: &Node, key: &str) -> Result<String, String> {
    let w = plain_word(ctx, node, &format!("'{key}' of an event"))?;
    if w.contains(':') {
        return Err(ctx.at(node.line, format!("'{key}' of an event cannot hold ':'")));
    }
    Ok(w)
}

/// Renders an `instruction_sets` section into `INSTSET` and `INST` lines.
pub fn render_instruction_sets_node(
    node: &Node,
    file: &str,
    working_dir: &str,
) -> Result<Vec<String>, Vec<String>> {
    render_instruction_sets(
        &Ctx {
            file: file.to_string(),
            working_dir,
        },
        node,
        0,
    )
}

fn render_instruction_sets(
    ctx: &Ctx<'_>,
    node: &Node,
    depth: usize,
) -> Result<Vec<String>, Vec<String>> {
    let items = seq_of(ctx, node, "the instruction sets").map_err(|e| vec![e])?;
    let mut lines = Vec::new();
    let mut errors = Vec::new();
    for item in items {
        let r = match &item.kind {
            NodeKind::Scalar { text, .. } => inst_set_reference(ctx, item, text, depth),
            _ => render_inst_set(ctx, item).map_err(|e| vec![e]),
        };
        match r {
            Ok(l) => lines.extend(l),
            Err(e) => errors.extend(e),
        }
    }
    if errors.is_empty() {
        Ok(lines)
    } else {
        Err(errors)
    }
}

fn inst_set_reference(
    ctx: &Ctx<'_>,
    item: &Node,
    text: &str,
    depth: usize,
) -> Result<Vec<String>, Vec<String>> {
    if depth > 0 {
        return Err(vec![ctx.at(
            item.line,
            "a file of instruction sets cannot name another file or a stock set",
        )]);
    }
    if text.ends_with(".yaml") || text.ends_with(".yml") {
        let file_node = Node {
            kind: NodeKind::Scalar {
                text: text.to_string(),
                quoted: false,
            },
            line: item.line,
        };
        let (c, n) = section(ctx, naming::SECTION_INSTRUCTION_SETS, &file_node)?;
        return render_instruction_sets(&c, &n, depth + 1);
    }
    if text.ends_with(".cfg") {
        return Err(vec![ctx.at(item.line, old_format_message(text))]);
    }
    let Some(stock) = stock_set(text) else {
        let names: Vec<&str> = STOCK_SETS.iter().map(|s| s.name).collect();
        return Err(vec![ctx.at(
            item.line,
            format!(
                "unknown instruction set '{text}' (the stock sets are {}; a file of sets ends in .yaml){}",
                names.join(", "),
                did_you_mean(nearest(text, names.iter().copied()))
            ),
        )]);
    };
    let root = yaml::parse(stock.text)
        .map_err(|e| vec![format!("{}:{e}", stock.file)])?
        .unwrap_or(Node {
            kind: NodeKind::Null,
            line: 1,
        });
    let c = Ctx {
        file: stock.file.to_string(),
        working_dir: ctx.working_dir,
    };
    let n = root
        .get(naming::SECTION_INSTRUCTION_SETS)
        .cloned()
        .unwrap_or(Node {
            kind: NodeKind::Null,
            line: 1,
        });
    render_instruction_sets(&c, &n, depth + 1)
}

fn render_inst_set(ctx: &Ctx<'_>, node: &Node) -> Result<Vec<String>, String> {
    let map = map_of(ctx, node, "an instruction set")?;
    for (k, _) in map {
        check_key(ctx, k, INST_SET_KEYS, "an instruction set")?;
    }
    let name = required_word(ctx, node, map, "name", "an instruction set")?;
    let what = format!("instruction set '{name}'");
    let hw = required_word(ctx, node, map, "hardware_type", &what)?;
    let mut head = format!("INSTSET {name}:hw_type={hw}");
    for key in ["stack_size", "uops_per_cycle"] {
        if let Some(v) = field(map, key) {
            head.push_str(&format!(
                ":{key}={}",
                word(ctx, v, &format!("'{key}' of {what}"))?
            ));
        }
    }
    let mut lines = vec![head];
    let known: Vec<&str> = INST_KEYS.iter().map(|(k, _)| *k).collect();
    if let Some(list) = field(map, "instructions") {
        for inst in seq_of(ctx, list, &format!("the instructions of {what}"))? {
            match &inst.kind {
                NodeKind::Map(m) => {
                    let iname =
                        required_word(ctx, inst, m, "name", &format!("an instruction of {what}"))?;
                    let iwhat = format!("instruction '{iname}' of {what}");
                    let mut parts = vec![iname];
                    parts.extend(entries(ctx, m, &known, &["name"], &iwhat)?);
                    lines.push(format!("INST {}", parts.join(":")));
                }
                _ => lines.push(format!(
                    "INST {}",
                    word(ctx, inst, &format!("an instruction of {what}"))?
                )),
            }
        }
    }
    Ok(lines)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch(name: &str) -> std::path::PathBuf {
        static N: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
        let n = N.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let d = std::env::temp_dir().join(format!(
            "avida-experiment-{}-{name}-{n}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    fn load(text: &str) -> (Config, Feedback, bool) {
        let d = scratch("load");
        std::fs::write(d.join("avida.yaml"), text).unwrap();
        let mut cfg = Config::new();
        let mut fb = Feedback::new();
        let ok = cfg.load_experiment("avida.yaml", d.to_str().unwrap(), &mut fb);
        (cfg, fb, ok)
    }

    #[test]
    fn settings_and_defaults() {
        let (cfg, fb, ok) =
            load("world:\n  x: 30\nmutation:\n  copy_prob: 0.0025\nexecution:\n  mode: 1\n");
        assert!(ok, "{}", fb.render());
        assert_eq!(cfg.world_x, 30);
        assert_eq!(cfg.world_y, 60);
        assert_eq!(cfg.copy_mut_prob, 0.0025);
        assert_eq!(cfg.port.execution_mode, 1);
        let (cfg, _, ok) = load("energy:\n  enabled: true\n");
        assert!(ok);
        assert!(cfg.energy_enabled);
    }

    #[test]
    fn unknown_keys_get_a_suggestion() {
        let (_, fb, ok) = load("mutation:\n  copy_prop: 0.1\n");
        assert!(!ok);
        assert!(fb.render().contains("avida.yaml:2: unknown setting 'mutation.copy_prop'; did you mean 'mutation.copy_prob'?"), "{}", fb.render());
        let (_, fb, _) = load("COPY_MUT_PROB: 0.1\n");
        assert!(
            fb.render()
                .contains("in this file it is 'mutation.copy_prob'"),
            "{}",
            fb.render()
        );
        let (_, fb, _) = load("mutaton:\n  copy_prob: 0.1\n");
        assert!(
            fb.render().contains("did you mean 'mutation'?"),
            "{}",
            fb.render()
        );
        let (_, fb, _) = load("world:\n  copy_prob: 0.1\n");
        assert!(
            fb.render().contains("did you mean 'mutation.copy_prob'?"),
            "{}",
            fb.render()
        );
        let (_, fb, _) = load("world:\n  x: sixty\n");
        assert!(fb.render().contains("takes a number"), "{}", fb.render());
    }

    #[test]
    fn old_files_are_refused() {
        let (_, fb, ok) = load("VERSION_ID 2.14.0\nWORLD_X 10\n");
        assert!(!ok);
        assert!(
            fb.render().contains("old configuration format"),
            "{}",
            fb.render()
        );
        assert!(fb.render().contains("avida convert"), "{}", fb.render());
    }

    #[test]
    fn sections_render_to_cpp_lines() {
        let text = "\
instruction_sets:
  - name: mine
    hardware_type: 0
    instructions:
      - nop-A
      - {name: inc, redundancy: 2}
environment:
  resources:
    - {name: glucose, inflow: 100, outflow: 0.01}
    - {name: spot, kind: cell, cells: '0..3', initial: 5}
  reactions:
    - name: NOT
      task: not
      process: {value: 1.0, type: pow}
      requisite: {max_count: 1, reaction: [A, B]}
    - {name: OPT, task: optimize, args: {function: 9, binary: 1}}
events:
  - {at: begin, action: Inject, args: default-heads.org}
  - {at: 0, every: 100, action: PrintAverageData}
  - {trigger: generation, at: 0, every: 10, until: 50, action: Foo, args: [a, b]}
  - {trigger: immediate, action: Exit}
";
        let (cfg, fb, ok) = load(text);
        assert!(ok, "{}", fb.render());
        assert_eq!(
            cfg.instsets(),
            [
                "INSTSET mine:hw_type=0",
                "INST nop-A",
                "INST inc:redundancy=2"
            ]
        );
        assert_eq!(
            cfg.experiment.environment,
            [
                "RESOURCE glucose:inflow=100:outflow=0.01",
                "CELL spot:0..3:initial=5",
                "REACTION NOT not process:value=1.0:type=pow requisite:max_count=1:reaction=A:reaction=B",
                "REACTION OPT optimize:function=9,binary=1",
            ]
        );
        assert_eq!(
            cfg.experiment.events,
            [
                "u begin Inject default-heads.org",
                "u 0:100 PrintAverageData",
                "g 0:10:50 Foo a b",
                "i Exit",
            ]
        );
    }

    #[test]
    fn stock_sets_and_section_files() {
        let d = scratch("sections");
        std::fs::write(d.join("ev.yaml"), "events:\n  - {action: Exit, at: 10}\n").unwrap();
        std::fs::write(
            d.join("avida.yaml"),
            "instruction_sets: [heads_default]\nevents: ev.yaml\n",
        )
        .unwrap();
        let mut cfg = Config::new();
        let mut fb = Feedback::new();
        assert!(
            cfg.load_experiment("avida.yaml", d.to_str().unwrap(), &mut fb),
            "{}",
            fb.render()
        );
        assert_eq!(cfg.instsets()[0], "INSTSET heads_default:hw_type=0");
        assert_eq!(cfg.instsets()[1], "INST nop-A");
        assert_eq!(cfg.experiment.events, ["u 10 Exit"]);
        assert_eq!(cfg.experiment.events_file, "ev.yaml");
        cfg.set_path("instruction_sets", "heads_sex", d.to_str().unwrap())
            .unwrap();
        assert_eq!(cfg.instsets()[0], "INSTSET heads_sex:hw_type=0");
        cfg.set_path("mutation.copy_prob", "0.5", "").unwrap();
        assert_eq!(cfg.copy_mut_prob, 0.5);
        let e = cfg
            .set_path("mutation.copy_mut_prob", "0.5", "")
            .unwrap_err();
        assert!(e.contains("did you mean 'mutation.copy_prob'"), "{e}");
        let e = cfg
            .set_path("instruction_sets", "heads_defualt", "")
            .unwrap_err();
        assert!(e.contains("did you mean 'heads_default'"), "{e}");
    }

    #[test]
    fn exact_doubles_round_trip() {
        for x in [0.0075, 0.1, 1.0, 1e-7, 123456.789, 0.0, -0.5, f64::MAX] {
            assert_eq!(as_double(&exact_double(x)).to_bits(), x.to_bits());
        }
        assert_eq!(exact_double(0.0075), "0.0075");
    }
}
