//! `avida convert`: writes one experiment file (`avida.yaml`) from a folder
//! of the old format (`avida.cfg` with its includes, the environment file,
//! the events file and the instruction-set files).
//!
//! The old files are read with the old readers (`cInitFile` with `#include`,
//! `-def` mappings and `$NAME` substitution, `cAvidaConfig::Load`, the
//! legacy instruction-set reader), so the experiment file holds what the
//! C++ would have run. Settings are written only where they differ from
//! their defaults (all of them, with their descriptions, with `full`). The
//! environment, events and instruction sets are split into their parts
//! without interpreting a value: every value is written as the text the
//! old file had, so the parsers read the same thing from the new file.

use crate::config::Config;
use crate::experiment::{self, INST_KEYS, STOCK_SETS};
use crate::feedback::{Feedback, FeedbackKind};
use crate::init_file::InitFile;
use crate::naming::{self, GROUPS};
use crate::string_util::{as_double, as_int, format_f, pop, pop_word, trimmed};
use crate::yaml::{self, quote, quote_key};

/// What `avida convert` was asked for.
#[derive(Debug, Clone)]
pub struct Options {
    /// The old configuration file in the folder (`-c`, default `avida.cfg`).
    pub config_file: String,
    /// `-def NAME VALUE` mappings for `#include NAME=...` and `$NAME`.
    pub defs: Vec<(String, String)>,
    /// `-set NAME VALUE` with the old names, applied before writing.
    pub sets: Vec<(String, String)>,
    /// Every setting with its description, not only the changed ones.
    pub full: bool,
}

impl Default for Options {
    fn default() -> Self {
        Options {
            config_file: "avida.cfg".into(),
            defs: Vec::new(),
            sets: Vec::new(),
            full: false,
        }
    }
}

/// The converted experiment.
#[derive(Debug, Clone)]
pub struct Conversion {
    /// The experiment file's text.
    pub yaml: String,
    /// The old files read, as named (relative to the folder unless
    /// absolute), without duplicates.
    pub files_read: Vec<String>,
    /// What the conversion left out (lines the C++ ignored with a warning).
    pub notes: Vec<String>,
}

fn push_unique(v: &mut Vec<String>, s: &str) {
    if !v.iter().any(|x| x == s) {
        v.push(s.to_string());
    }
}

/// Converts the folder `dir`.
pub fn convert_folder(dir: &str, opts: &Options) -> Result<Conversion, String> {
    let mut cfg = Config::new();
    let mut fb = Feedback::new();
    if !cfg.load(
        &opts.config_file,
        dir,
        Some(&mut fb),
        Some(&opts.defs),
        false,
    ) {
        return Err(fb.render());
    }
    let mut notes: Vec<String> = Vec::new();
    for (kind, msg) in fb.messages() {
        if *kind != FeedbackKind::Error && !msg.contains("version number mismatch") {
            notes.push(msg.clone());
        }
    }
    let mut files_read = Vec::new();
    for f in InitFile::with_mappings(&opts.config_file, &opts.defs, dir).files_read() {
        push_unique(&mut files_read, f);
    }
    let mut sets = opts.sets.clone();
    cfg.set_from_map(&mut sets);
    if let Some((name, _)) = sets.first() {
        return Err(format!("unknown setting '{name}' in -set"));
    }

    // Instruction sets: the INSTSET lines of the configuration, then a
    // legacy file when the configuration asks for one.
    let mut inst_lines: Vec<String> = cfg.instsets().to_vec();
    if cfg.inst_set_load_legacy != 0 {
        let file = legacy_inst_set_lines(&cfg.inst_set, cfg.hardware_type, dir, &mut inst_lines)?;
        push_unique(&mut files_read, &file);
    }
    let inst_sets = parse_inst_sets(&inst_lines)?;

    let env = read_lines(&cfg.environment_file, dir, &opts.defs, "environment")?;
    for f in &env.1 {
        push_unique(&mut files_read, f);
    }
    let environment = parse_environment(&env.0)?;
    let ev = read_lines(&cfg.event_file, dir, &opts.defs, "events")?;
    for f in &ev.1 {
        push_unique(&mut files_read, f);
    }
    let events = parse_events(&ev.0)?;

    let mut out = String::new();
    if opts.full {
        out.push_str(FULL_HEADER);
    } else {
        out.push_str(&format!(
            "# Avida experiment file, converted with 'avida convert' from\n# {}.\n# A setting left out keeps its default; {} lists them all.\n",
            files_read.join(", "),
            experiment::REFERENCE_PAGE
        ));
    }
    out.push_str(&settings_yaml(&cfg, opts.full));
    out.push('\n');
    out.push_str(&inst_sets_yaml(&inst_sets, opts.full));
    out.push('\n');
    out.push_str(&environment_yaml(&environment, opts.full));
    out.push('\n');
    out.push_str(&events_yaml(&events, opts.full));
    Ok(Conversion {
        yaml: out,
        files_read,
        notes,
    })
}

const FULL_HEADER: &str = "\
# The Avida experiment file: every setting of an experiment, its instruction
# sets, its environment and its events. A setting left out keeps its
# default. The settings are grouped; on the command line a setting is named
# by its group and key, as in '-set mutation.copy_prob 0.0025'.
# docs/reference/config-yaml.md lists every setting with its old name.
";

/// Converts one old file into a section file (`environment:`, `events:`
/// or `instruction_sets:` with the file's content).
pub fn convert_section_file(
    section: &str,
    file: &str,
    dir: &str,
    defs: &[(String, String)],
) -> Result<String, String> {
    let (lines, _) = read_lines(file, dir, defs, section)?;
    let header = format!(
        "# Converted with 'avida convert' from {file}. Name it in an experiment\n# file as \"{section}: <this file>\".\n\n"
    );
    let body = match section {
        naming::SECTION_ENVIRONMENT => environment_yaml(&parse_environment(&lines)?, false),
        naming::SECTION_EVENTS => events_yaml(&parse_events(&lines)?, false),
        naming::SECTION_INSTRUCTION_SETS => {
            let mut list = Vec::new();
            let keyword = |l: &String| {
                let mut rest = l.clone();
                pop_word(&mut rest)
            };
            if lines.iter().any(|l| keyword(l) == "INSTSET") {
                for l in &lines {
                    let kw = keyword(l);
                    if kw == "INSTSET" || kw == "INST" {
                        list.push(l.clone());
                    } else {
                        return Err(format!("{file}: '{l}' is not an INSTSET or INST line"));
                    }
                }
            } else {
                // A legacy file (one instruction a line with its redundancy
                // and costs): read as INST_SET_LOAD_LEGACY read it for the
                // heads CPU, so the set is named after the file.
                legacy_inst_set_lines(file, 0, dir, &mut list)?;
            }
            inst_sets_yaml(&parse_inst_sets(&list)?, false)
        }
        _ => {
            return Err(format!(
                "unknown section '{section}' (environment, events or instruction_sets)"
            ))
        }
    };
    Ok(header + &body)
}

/// The lines of an old file after `cInitFile` processing, with the files
/// read.
fn read_lines(
    file: &str,
    dir: &str,
    defs: &[(String, String)],
    what: &str,
) -> Result<(Vec<String>, Vec<String>), String> {
    let f = InitFile::with_mappings(file, defs, dir);
    if !f.was_opened() {
        return Err(format!(
            "cannot read the {what} file '{file}': {}",
            f.feedback().render().trim_end()
        ));
    }
    let lines = (0..f.num_lines()).map(|i| f.get_line(i as i32)).collect();
    Ok((lines, f.files_read().to_vec()))
}

// ---------------------------------------------------------------------------
// Settings
// ---------------------------------------------------------------------------

fn comment(out: &mut String, indent: usize, text: &str) {
    for l in text.split('\n') {
        let l = l.trim_end().replace('\t', " ");
        out.push_str(&" ".repeat(indent));
        if l.is_empty() {
            out.push_str("#\n");
        } else {
            out.push_str("# ");
            out.push_str(&l);
            out.push('\n');
        }
    }
}

fn settings_yaml(cfg: &Config, full: bool) -> String {
    let defaults = Config::new();
    let mut out = String::new();
    for g in GROUPS {
        let mut body = String::new();
        for s in naming::settings_of_group(g.key) {
            let v = cfg.setting_text(&s);
            if !full && v == defaults.setting_text(&s) {
                continue;
            }
            if full {
                comment(&mut body, 2, s.desc());
            }
            body.push_str(&format!("  {}: {}\n", quote_key(s.key), quote(&v, false)));
        }
        if body.is_empty() {
            continue;
        }
        if full {
            out.push('\n');
            comment(&mut out, 0, g.title);
        }
        out.push_str(&format!("{}:\n", g.key));
        out.push_str(&body);
    }
    out
}

// ---------------------------------------------------------------------------
// Instruction sets
// ---------------------------------------------------------------------------

/// `cHardwareManager::ConvertLegacyInstSetFile`: the INSTSET and INST lines
/// of a legacy instruction-set file. Returns the file read.
fn legacy_inst_set_lines(
    file: &str,
    hardware_type: i32,
    dir: &str,
    out: &mut Vec<String>,
) -> Result<String, String> {
    let default = match hardware_type {
        0 => "instset-heads.cfg",
        2 => "instset-transsmt.cfg",
        3 => "instset-experimental.cfg",
        _ => return Err("unknown/unsupported HARDWARE_TYPE specified".into()),
    };
    let filename = if file.is_empty() || file == "-" {
        default
    } else {
        file
    };
    let f = InitFile::new(filename, dir, None, None);
    if !f.was_opened() {
        return Err(format!("cannot read the instruction set file '{filename}'"));
    }
    out.push(format!("INSTSET {filename}:hw_type={hardware_type}"));
    for i in 0..f.num_lines() {
        let mut cur = f.get_line(i as i32);
        let name = pop_word(&mut cur);
        let redundancy = as_int(&pop_word(&mut cur));
        let cost = as_int(&pop_word(&mut cur));
        let ft_cost = as_int(&pop_word(&mut cur));
        let energy_cost = as_int(&pop_word(&mut cur));
        let prob_fail = as_double(&pop_word(&mut cur));
        let addl_time_cost = as_int(&pop_word(&mut cur));
        let res_cost = as_double(&pop_word(&mut cur));
        let fem_res_cost = as_double(&pop_word(&mut cur));
        let post_cost = as_double(&pop_word(&mut cur)) as i32;
        out.push(format!(
            "INST {name}:redundancy={redundancy}:cost={cost}:initial_cost={ft_cost}:energy_cost={energy_cost}:prob_fail={}:addl_time_cost={addl_time_cost}:res_cost={}:fem_res_cost={}:post_cost={post_cost}",
            format_f(prob_fail),
            format_f(res_cost),
            format_f(fem_res_cost),
        ));
    }
    Ok(filename.to_string())
}

#[derive(Debug)]
struct InstSetDef {
    name: String,
    args: Vec<(String, String)>,
    insts: Vec<(String, Vec<(String, String)>)>,
}

/// The `:`-separated entries the C++ `while (s.GetSize()) s.Pop(':')` loop
/// reads: a trailing `:` ends the list without an empty entry.
fn entries_of(text: &str) -> Vec<String> {
    let mut s = text.to_string();
    let mut out = Vec::new();
    while !s.is_empty() {
        out.push(pop(&mut s, ':'));
    }
    out
}

fn split_args(text: &str, what: &str) -> Result<Vec<(String, String)>, String> {
    let mut out = Vec::new();
    for entry in entries_of(text) {
        match entry.split_once('=') {
            Some((k, v)) if !k.is_empty() => out.push((k.to_string(), v.to_string())),
            _ => return Err(format!("cannot convert {what}: '{entry}' is not KEY=VALUE")),
        }
    }
    Ok(out)
}

fn parse_inst_sets(lines: &[String]) -> Result<Vec<InstSetDef>, String> {
    let mut sets: Vec<InstSetDef> = Vec::new();
    for line in lines {
        let mut rest = line.clone();
        let kw = pop_word(&mut rest);
        if kw == "INSTSET" {
            let name = trimmed(&pop(&mut rest, ':'));
            let args = split_args(&rest, &format!("instruction set '{name}'"))?;
            if name.is_empty() || !args.iter().any(|(k, _)| k == "hw_type") {
                return Err(format!(
                    "cannot convert '{line}': an instruction set needs a name and hw_type"
                ));
            }
            sets.push(InstSetDef {
                name,
                args,
                insts: Vec::new(),
            });
        } else if kw == "INST" {
            let Some(set) = sets.last_mut() else {
                return Err(format!(
                    "cannot convert '{line}': an INST line before any INSTSET line"
                ));
            };
            let name = pop(&mut rest, ':');
            let args = split_args(&rest, &format!("instruction '{name}'"))?;
            // Arguments with their default value say nothing.
            let args = args
                .into_iter()
                .filter(|(k, v)| match INST_KEYS.iter().find(|(key, _)| key == k) {
                    Some((key, d)) => !is_default_inst_arg(key, v, d),
                    None => true,
                })
                .collect();
            set.insts.push((name, args));
        }
    }
    Ok(sets)
}

#[allow(clippy::float_cmp)]
fn is_default_inst_arg(key: &str, value: &str, default: &str) -> bool {
    match key {
        "inst_code" => value == default,
        "prob_fail" | "res_cost" | "redundancy" | "fem_res_cost" | "bonus_cost" => {
            crate::string_util::is_number(value) && as_double(value) == as_double(default)
        }
        _ => {
            crate::string_util::is_number(value)
                && as_int(value) == as_int(default)
                && !value.contains(['.', 'e', 'E'])
        }
    }
}

fn inst_set_body(set: &InstSetDef) -> String {
    let mut o = String::new();
    o.push_str(&format!("  - name: {}\n", quote(&set.name, false)));
    for (k, v) in &set.args {
        let key = if k == "hw_type" {
            "hardware_type"
        } else {
            k.as_str()
        };
        o.push_str(&format!("    {}: {}\n", quote_key(key), quote(v, false)));
    }
    o.push_str("    instructions:\n");
    for (name, args) in &set.insts {
        if args.is_empty() {
            o.push_str(&format!("      - {}\n", quote(name, false)));
        } else {
            let mut pairs = vec![("name".to_string(), name.clone())];
            pairs.extend(args.iter().cloned());
            o.push_str(&format!("      - {}\n", flow_map(&pairs)));
        }
    }
    o
}

fn stock_name_of(set: &InstSetDef) -> Option<&'static str> {
    let text = format!("instruction_sets:\n{}", inst_set_body(set));
    let node = yaml::parse(&text).ok()??;
    let mine = experiment::render_instruction_sets_node(
        node.get(naming::SECTION_INSTRUCTION_SETS)?,
        "",
        "",
    )
    .ok()?;
    STOCK_SETS.iter().find_map(|s| {
        let n = yaml::parse(s.text).ok()??;
        let stock = experiment::render_instruction_sets_node(
            n.get(naming::SECTION_INSTRUCTION_SETS)?,
            "",
            "",
        )
        .ok()?;
        (stock == mine).then_some(s.name)
    })
}

fn inst_sets_yaml(sets: &[InstSetDef], full: bool) -> String {
    let mut o = String::new();
    if full {
        o.push_str(
            "# The instruction sets: a stock set by its name (heads_default, heads_sex,\n\
             # heads_modular, smt, transsmt, experimental), or a set written out with\n\
             # its name, hardware_type and instructions, or a YAML file of sets.\n",
        );
    }
    if sets.is_empty() {
        o.push_str("instruction_sets: []\n");
        return o;
    }
    o.push_str("instruction_sets:\n");
    for s in sets {
        match stock_name_of(s) {
            Some(name) => o.push_str(&format!("  - {name}\n")),
            None => o.push_str(&inst_set_body(s)),
        }
    }
    o
}

// ---------------------------------------------------------------------------
// Environment
// ---------------------------------------------------------------------------

#[derive(Debug)]
enum EnvItem {
    Resource {
        kind: Option<&'static str>,
        name: String,
        cells: Option<String>,
        kv: Vec<(String, String)>,
    },
    Grid {
        name: String,
        kv: Vec<(String, String)>,
    },
    Reaction {
        name: String,
        task: Option<(String, String)>,
        parts: Vec<(String, Vec<(String, String)>)>,
    },
    SetActive(String, String, String),
}

/// `parse_setting` over the `:`-separated entries: lower-case keys.
fn settings_of(text: &str, what: &str) -> Result<Vec<(String, String)>, String> {
    let mut out = Vec::new();
    for entry in entries_of(text) {
        match entry.split_once('=') {
            Some((k, v)) if !k.is_empty() && !v.is_empty() => {
                out.push((k.to_ascii_lowercase(), v.to_string()))
            }
            _ => return Err(format!("cannot convert {what}: '{entry}' is not KEY=VALUE")),
        }
    }
    Ok(out)
}

fn parse_environment(lines: &[String]) -> Result<Vec<EnvItem>, String> {
    let mut items = Vec::new();
    for line in lines {
        let mut desc = line.clone();
        let kw = pop_word(&mut desc).to_ascii_uppercase();
        match kw.as_str() {
            "RESOURCE" | "GRADIENT_RESOURCE" | "CELL" => {
                if desc.is_empty() {
                    return Err(format!("cannot convert '{line}': no resources listed"));
                }
                while !desc.is_empty() {
                    let mut cur = pop_word(&mut desc);
                    let name = pop(&mut cur, ':');
                    let what = format!("resource '{name}'");
                    let (kind, cells) = match kw.as_str() {
                        "CELL" => (Some("cell"), Some(pop(&mut cur, ':'))),
                        "GRADIENT_RESOURCE" => (Some("gradient"), None),
                        _ => (None, None),
                    };
                    let kv = settings_of(&cur, &what)?;
                    items.push(EnvItem::Resource {
                        kind,
                        name,
                        cells,
                        kv,
                    });
                }
            }
            "GRID" => {
                let name = pop(&mut desc, ':');
                let kv = split_args(&desc, &format!("grid '{name}'"))?;
                items.push(EnvItem::Grid { name, kv });
            }
            "REACTION" => {
                let name = pop_word(&mut desc);
                if name.is_empty() {
                    return Err(format!("cannot convert '{line}': a reaction needs a name"));
                }
                if desc.is_empty() {
                    items.push(EnvItem::Reaction {
                        name,
                        task: None,
                        parts: Vec::new(),
                    });
                    continue;
                }
                let mut trigger_info = pop_word(&mut desc);
                let task = pop(&mut trigger_info, ':');
                let mut parts = Vec::new();
                while !desc.is_empty() {
                    let mut entry = pop_word(&mut desc);
                    let ty = pop(&mut entry, ':').to_ascii_lowercase();
                    if !matches!(ty.as_str(), "process" | "requisite" | "context_requisite") {
                        return Err(format!(
                            "cannot convert '{line}': unknown entry type '{ty}'"
                        ));
                    }
                    let kv = settings_of(&entry, &format!("a {ty} of reaction '{name}'"))?;
                    parts.push((ty, kv));
                }
                items.push(EnvItem::Reaction {
                    name,
                    task: Some((task, trigger_info)),
                    parts,
                });
            }
            "SET_ACTIVE" => {
                let ty = pop_word(&mut desc);
                let name = pop_word(&mut desc);
                let active = pop_word(&mut desc);
                items.push(EnvItem::SetActive(ty, name, active));
            }
            _ => {
                return Err(format!(
                    "cannot convert '{line}': unknown environment keyword '{kw}'"
                ))
            }
        }
    }
    Ok(items)
}

/// Mapping entries with a repeated key gathered into a list at the place of
/// its first use.
fn grouped(pairs: &[(String, String)]) -> Vec<(String, Vec<String>)> {
    let mut out: Vec<(String, Vec<String>)> = Vec::new();
    for (k, v) in pairs {
        match out.iter_mut().find(|(key, _)| key == k) {
            Some((_, vs)) => vs.push(v.clone()),
            None => out.push((k.clone(), vec![v.clone()])),
        }
    }
    out
}

fn value_yaml(vs: &[String]) -> String {
    if vs.len() == 1 {
        quote(&vs[0], true)
    } else {
        yaml::flow_seq(vs)
    }
}

fn flow_map(pairs: &[(String, String)]) -> String {
    let inner: Vec<String> = grouped(pairs)
        .iter()
        .map(|(k, vs)| format!("{}: {}", quote_key(k), value_yaml(vs)))
        .collect();
    format!("{{{}}}", inner.join(", "))
}

/// A value of a mapping being written: a scalar (quoted as its place
/// needs) or a flow collection already in YAML form.
enum Val {
    S(String),
    Y(String),
}

impl Val {
    fn render(&self, flow: bool) -> String {
        match self {
            Val::S(s) => quote(s, flow),
            Val::Y(y) => y.clone(),
        }
    }
}

/// A list item that is a mapping: on one line when short, else one key a
/// line.
fn item_vals(out: &mut String, indent: usize, entries: &[(String, Val)]) {
    let inner: Vec<String> = entries
        .iter()
        .map(|(k, v)| format!("{}: {}", quote_key(k), v.render(true)))
        .collect();
    let flow = format!("{{{}}}", inner.join(", "));
    if flow.len() + indent + 2 <= 100 {
        out.push_str(&format!("{}- {flow}\n", " ".repeat(indent)));
        return;
    }
    for (i, (k, v)) in entries.iter().enumerate() {
        out.push_str(&" ".repeat(indent));
        out.push_str(if i == 0 { "- " } else { "  " });
        out.push_str(&format!("{}: {}\n", quote_key(k), v.render(false)));
    }
}

fn item(out: &mut String, indent: usize, pairs: &[(String, String)]) {
    let entries: Vec<(String, Val)> = grouped(pairs)
        .into_iter()
        .map(|(k, vs)| {
            let v = if vs.len() == 1 {
                Val::S(vs[0].clone())
            } else {
                Val::Y(yaml::flow_seq(&vs))
            };
            (k, v)
        })
        .collect();
    item_vals(out, indent, &entries);
}

fn environment_yaml(items: &[EnvItem], full: bool) -> String {
    let mut o = String::new();
    if full {
        o.push_str(
            "# The environment: resources (plain, 'cell' or 'gradient'), state grids,\n\
             # and reactions, each with its task, process and requisite.\n",
        );
    }
    if items.is_empty() {
        o.push_str("environment: {}\n");
        return o;
    }
    o.push_str("environment:\n");
    let section = |o: &mut String, key: &str, body: &str| {
        if !body.is_empty() {
            o.push_str(&format!("  {key}:\n{body}"));
        }
    };
    let mut resources = String::new();
    let mut grids = String::new();
    let mut reactions = String::new();
    let mut set_active = String::new();
    for it in items {
        match it {
            EnvItem::Resource {
                kind,
                name,
                cells,
                kv,
            } => {
                let mut pairs = vec![("name".to_string(), name.clone())];
                if let Some(k) = kind {
                    pairs.push(("kind".into(), k.to_string()));
                }
                // An empty cell list is written only when entries follow it.
                if let Some(c) = cells {
                    if !c.is_empty() || !kv.is_empty() {
                        pairs.push(("cells".into(), c.clone()));
                    }
                }
                pairs.extend(kv.iter().cloned());
                item(&mut resources, 4, &pairs);
            }
            EnvItem::Grid { name, kv } => {
                let mut pairs = vec![("name".to_string(), name.clone())];
                pairs.extend(kv.iter().cloned());
                item(&mut grids, 4, &pairs);
            }
            EnvItem::Reaction { name, task, parts } => {
                let mut entries = vec![("name".to_string(), Val::S(name.clone()))];
                if let Some((task, args)) = task {
                    entries.push(("task".into(), Val::S(task.clone())));
                    if !args.is_empty() {
                        entries.push(("args".into(), task_args_val(args)));
                    }
                }
                for key in ["process", "requisite", "context_requisite"] {
                    let blocks: Vec<String> = parts
                        .iter()
                        .filter(|(t, _)| t == key)
                        .map(|(_, kv)| flow_map(kv))
                        .collect();
                    match blocks.len() {
                        0 => {}
                        1 => entries.push((key.into(), Val::Y(blocks[0].clone()))),
                        _ => entries.push((key.into(), Val::Y(format!("[{}]", blocks.join(", "))))),
                    }
                }
                item_vals(&mut reactions, 4, &entries);
            }
            EnvItem::SetActive(t, n, a) => item(
                &mut set_active,
                4,
                &[
                    ("type".into(), t.clone()),
                    ("name".into(), n.clone()),
                    ("active".into(), a.clone()),
                ],
            ),
        }
    }
    section(&mut o, "resources", &resources);
    section(&mut o, "grids", &grids);
    section(&mut o, "reactions", &reactions);
    section(&mut o, "set_active", &set_active);
    o
}

/// Task arguments as a mapping when they are `k=v` pairs joined by
/// commas, else as the text.
fn task_args_val(args: &str) -> Val {
    let mut pairs = Vec::new();
    for part in args.split(',') {
        match part.split_once('=') {
            Some((k, v))
                if !k.is_empty()
                    && !v.is_empty()
                    && !k.contains(':')
                    && !v.contains([':', '='])
                    && !pairs.iter().any(|(key, _): &(String, String)| key == k) =>
            {
                pairs.push((k.to_string(), v.to_string()))
            }
            _ => return Val::S(args.to_string()),
        }
    }
    Val::Y(flow_map(&pairs))
}

// ---------------------------------------------------------------------------
// Events
// ---------------------------------------------------------------------------

#[derive(Debug)]
struct EventDef {
    trigger: &'static str,
    timing: Vec<String>,
    action: String,
    args: String,
}

fn parse_events(lines: &[String]) -> Result<Vec<EventDef>, String> {
    let mut out = Vec::new();
    for line in lines {
        let mut cur = line.clone();
        let t = pop_word(&mut cur);
        let trigger = match t.as_str() {
            "u" | "update" => "update",
            "g" | "generation" => "generation",
            "b" | "births" => "births",
            "o" | "org_id" => "org_id",
            "i" | "immediate" => "immediate",
            _ => {
                return Err(format!(
                    "cannot convert the event '{line}': unknown trigger '{t}'"
                ))
            }
        };
        let mut timing = Vec::new();
        if trigger != "immediate" {
            let mut tw = pop_word(&mut cur);
            timing.push(pop(&mut tw, ':'));
            if !tw.is_empty() {
                timing.push(pop(&mut tw, ':'));
                if !tw.is_empty() {
                    timing.push(tw);
                }
            }
            if timing.iter().any(String::is_empty) {
                return Err(format!(
                    "cannot convert the event '{line}': an empty timing part"
                ));
            }
        }
        let action = pop_word(&mut cur);
        if action.is_empty() {
            return Err(format!("cannot convert the event '{line}': no action"));
        }
        out.push(EventDef {
            trigger,
            timing,
            action,
            args: cur,
        });
    }
    Ok(out)
}

fn events_yaml(events: &[EventDef], full: bool) -> String {
    let mut o = String::new();
    if full {
        o.push_str(
            "# The events: an action with its arguments, run at an update (or a\n\
             # generation, a birth count) given by 'at', then 'every' so many until\n\
             # 'until'. 'at: begin' is the start of the run.\n",
        );
    }
    if events.is_empty() {
        o.push_str("events: []\n");
        return o;
    }
    o.push_str("events:\n");
    for e in events {
        let mut pairs = Vec::new();
        if e.trigger != "update" {
            pairs.push(("trigger".to_string(), e.trigger.to_string()));
        }
        for (k, v) in ["at", "every", "until"].iter().zip(&e.timing) {
            pairs.push((k.to_string(), v.clone()));
        }
        pairs.push(("action".into(), e.action.clone()));
        if !e.args.is_empty() {
            pairs.push(("args".into(), e.args.clone()));
        }
        // Never one key a line: the events read as a table.
        o.push_str(&format!("  - {}\n", flow_map(&pairs)));
    }
    o
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn task_args_become_a_mapping_when_they_are_pairs() {
        assert_eq!(
            task_args_val("function=9,binary=1").render(true),
            "{function: 9, binary: 1}"
        );
        assert_eq!(
            task_args_val("op=add:width=2").render(false),
            "op=add:width=2"
        );
        assert_eq!(task_args_val("a=1,a=2").render(false), "a=1,a=2");
        assert_eq!(task_args_val("a=1,a=2").render(true), "'a=1,a=2'");
    }

    #[test]
    fn a_folder_converts_and_loads() {
        let d = std::env::temp_dir().join(format!("avida-convert-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        std::fs::write(
            d.join("avida.cfg"),
            "VERSION_ID 2.14.0\nWORLD_X 30\nCOPY_MUT_PROB 0.0025\nEVENT_FILE ev.cfg\n#include INST_SET=is.cfg\n",
        )
        .unwrap();
        std::fs::write(
            d.join("is.cfg"),
            "INSTSET mine:hw_type=0\nINST nop-A\nINST inc:redundancy=2:cost=0\n",
        )
        .unwrap();
        std::fs::write(
            d.join("environment.cfg"),
            "RESOURCE a:inflow=1 b\nREACTION NOT not process:resource=a:value=1.0 requisite:reaction=X:reaction=Y\n",
        )
        .unwrap();
        std::fs::write(
            d.join("ev.cfg"),
            "u begin Inject default-heads.org\ng 0:10:end Print x  y\n",
        )
        .unwrap();
        let dir = d.to_str().unwrap();
        let c = convert_folder(dir, &Options::default()).unwrap();
        assert_eq!(
            c.files_read,
            ["avida.cfg", "is.cfg", "environment.cfg", "ev.cfg"]
        );
        std::fs::write(d.join("avida.yaml"), &c.yaml).unwrap();
        let mut cfg = Config::new();
        let mut fb = Feedback::new();
        assert!(
            cfg.load_experiment("avida.yaml", dir, &mut fb),
            "{}\n{}",
            fb.render(),
            c.yaml
        );
        assert_eq!(cfg.world_x, 30);
        assert_eq!(cfg.copy_mut_prob, 0.0025);
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
                "RESOURCE a:inflow=1",
                "RESOURCE b",
                "REACTION NOT not process:resource=a:value=1.0 requisite:reaction=X:reaction=Y"
            ]
        );
        assert_eq!(
            cfg.experiment.events,
            ["u begin Inject default-heads.org", "g 0:10:end Print x y"]
        );
    }
}
