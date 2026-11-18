//! The reference page of the experiment file,
//! `docs/reference/config-yaml.md`, written from [`crate::naming`] and the
//! key lists of [`crate::experiment`]. The test `reference_page_is_current`
//! fails when the page in the tree differs; run it with
//! `AVIDA_UPDATE_REFERENCE=1` to rewrite the page.

use crate::config::all_entries;
use crate::experiment::{
    CELL_KEYS, EVENT_KEYS, GRADIENT_KEYS, GRID_KEYS, INST_KEYS, INST_SET_KEYS, PROCESS_KEYS,
    REACTION_KEYS, REQUISITE_KEYS, RESOURCE_KEYS, STOCK_SETS,
};
use crate::naming::{self, Target, GROUPS, NAMES};

const INTRO: &str = r#"# The experiment file (avida.yaml)

An Avida experiment is one YAML file, `avida.yaml`, in the folder the run
starts in. It holds the settings that differ from their defaults, the
instruction sets, the environment and the events. Organism files (`.org`)
and analyze scripts (`analyze.cfg`) stay files of their own, which the
events and the settings name by path.

The C++ program read four kinds of files instead (`avida.cfg`,
`environment.cfg`, `events.cfg` and an instruction-set file). Avida refuses
them now and points to `avida convert`, which writes the experiment file from
such a folder (see [Converting an old folder](#converting-an-old-folder)).

This page is generated from the table in
`rust/avida-native/crates/config/src/naming.rs`, which is the one place where
the group and key of a setting are defined.

## Layout

```yaml
world:
  x: 60
  y: 60
mutation:
  copy_prob: 0.0075

instruction_sets:
  - heads_default

environment:
  resources:
    - {name: glucose, inflow: 100, outflow: 0.01}
  reactions:
    - name: NOT
      task: not
      process: {resource: glucose, value: 1.0, type: pow}
      requisite: {max_count: 1}

events:
  - {at: begin, action: Inject, args: default-heads.org}
  - {at: 0, every: 100, action: PrintAverageData}
  - {at: 100000, action: Exit}
```

The top level holds the groups of settings (listed
[below](#settings-by-group)) and the three sections `instruction_sets`,
`environment` and `events`. Every part may be left out. A setting that is
left out keeps its default, a missing section is empty, and a run without an
instruction set stops with `error: no instruction sets defined`.

A key that Avida does not know is refused with the file, the line and the
nearest known name, for example
`avida.yaml:4: unknown setting 'mutation.copy_prop'; did you mean 'mutation.copy_prob'?`.
An old setting name gets its new place in the answer.

### Values

Every value is read by the parser the C++ program used for it, so a number
means what it meant in `avida.cfg`. A flag is an on/off switch: it takes
`true` or `false`, and `1` or `0`, and refuses anything else (`avida convert`
writes `true` and `false`). A setting whose type is an integer also takes
`true` and `false` (1 and 0). A number setting refuses a value that does not
start like a number (the C++ program read such a value as 0). YAML does not interpret anything here: `1.0`, `0x10` and `yes` reach the
parser as the text written.

### The command line

`-set` names a setting by its group and key: `-set mutation.copy_prob 0.0025`,
`-set world.x 100`. It also replaces a section with the content of a file
that holds just that section: `-set events events-long.yaml`,
`-set environment environment-poor.yaml`. `-set instruction_sets` takes the
name of a stock set or such a file. A list cannot be given on the command
line. `-c FILE` reads another experiment file than `avida.yaml`. The old
`-def` option is gone, since the experiment file has no `#include` or `$NAME`
substitution.

### Section files

Each section may be the name of a YAML file instead of its content. That
file holds the section under the same key and nothing else, so one set of
events can serve several experiments:

```yaml
# avida.yaml
events: events-long.yaml

# events-long.yaml
events:
  - {at: begin, action: Inject, args: default-heads.org}
  - {at: 200000, action: Exit}
```

Paths are relative to the folder the run starts in. A section file cannot
name a further file, and there is no general include.

## Instruction sets

`instruction_sets` is a list. An item is one of three things.

- The name of a stock set: STOCK_NAMES. These ship with
  Avida (`avida-core/support/config/instset-*.yaml`), so an experiment needs
  no copy.
- The name of a YAML file (ending in `.yaml` or `.yml`) whose
  `instruction_sets` list is taken in.
- A set written out, with the keys INST_SET_KEYS.

```yaml
instruction_sets:
  - name: my_heads
    hardware_type: 0
    instructions:
      - nop-A
      - {name: h-alloc, cost: 2}
      - {name: inc, redundancy: 3}
```

`hardware_type` is the C++ `HARDWARE_TYPE` (0 heads, 1 SMT, 2 transposon
SMT, 3 experimental, 4 GP8, 5 BCR); `stack_size` and `uops_per_cycle` are
optional. An instruction is its name, or a mapping with its `name` and any of
INST_KEY_LIST. The first set is the default set of the run, as the first
`INSTSET` was in `avida.cfg`.

## Environment

`environment` is a mapping with up to four lists, which are loaded in this
order: `resources`, `grids`, `reactions` and `set_active`. Its key
`input_and_mask` holds the mask of the environment line `INPUT_AND_MASK`
(hex with `0x`, such as `0xFF6DB6DB`, or decimal), which the file passes on
as written.

A resource has a `name` and its parameters. A plain resource (the C++
`RESOURCE`) takes RESOURCE_KEY_LIST. `kind: cell` makes a resource of
single cells (`CELL`), with `cells` (the cell list, such as `0..9,15`) and
CELL_KEY_LIST. `kind: gradient` makes a gradient resource
(`GRADIENT_RESOURCE`) with GRADIENT_KEY_LIST. A resource named twice is
changed by its second entry, as a second `RESOURCE` or `CELL` line changed it.

A state grid (`GRID`) has a `name` and GRID_KEY_LIST.

A reaction has the keys REACTION_KEY_LIST. `args` holds the task's
arguments, as a mapping (`{function: 9, binary: 1}`) or as the text the C++
line had after the task's colon. The file passes the arguments on as
written, and the task checks them. `process`, `requisite` and
`context_requisite` are a mapping or a list of mappings, one per C++
`process:`, `requisite:` or `context_requisite:` entry. A process takes
PROCESS_KEY_LIST. A requisite takes REQUISITE_KEY_LIST (`cellbox` only
outside a context requisite); `reaction` and `noreaction` may be lists. A
reaction with only a `name` is declared before its definition, as a
`REACTION` line with only a name was.

```yaml
environment:
  resources:
    - {name: glucose, inflow: 100, outflow: 0.01}
    - {name: spot, kind: cell, cells: '0..9', initial: 5}
  reactions:
    - name: OPT
      task: optimize
      args: {function: 9, binary: 1}
      process: [{value: 1.0, type: pow}, {resource: glucose, value: 0.5}]
      requisite: {reaction: [NOT, AND], max_count: 1}
```

A value in the environment is one word without `:`, since it is rendered into
the C++ line format.

## Events

`events` is a list of mappings with the keys EVENT_KEY_LIST.

| Key | Meaning |
|---|---|
| `trigger` | `update` (the default), `generation`, `births`, `org_id` or `immediate` |
| `at` | when the event first runs: `begin` (the default) or a number |
| `every` | the interval: a number, `all` (every update) or `once` |
| `until` | the last time it runs: a number or `end` |
| `action` | the action, as listed in [Actions](actions.md) |
| `args` | the action's arguments, as text or as a list of words |

`until` needs `every`, and an `immediate` event has no timing. The C++ line
`u 0:100:end PrintAverageData` is `{at: 0, every: 100, until: end, action:
PrintAverageData}`.

## Converting an old folder

`avida convert DIR` reads `DIR/avida.cfg` with its includes, its
environment, events and instruction-set files, and writes `DIR/avida.yaml`.
The old files are read by the old readers, so `#include`, `-def` values and
`$NAME` substitution are resolved on the way. Only the settings that differ
from their defaults are written; `--full` writes every setting with its
description, as in the stock `avida-core/support/config/avida.yaml`. An
instruction set that equals a stock set is written as the stock set's name.

```
avida convert arm-a                        # writes arm-a/avida.yaml
avida convert arm-a -set EVENT_FILE events-long.cfg
avida convert arm-a -def INST_SET instset-heads-modular.cfg
avida convert --section events events-long.cfg   # writes events-long.yaml
```

`-c FILE` reads another old configuration file, `-set NAME VALUE` changes an
old setting before writing (with the old name), `-o FILE` writes elsewhere
(`-` for standard output), `--remove-old` deletes the old files that were
read and `--force` overwrites an existing file. With `--section`, one old
environment, events or instruction-set file becomes a section file.

A run of the old kind such as
`avida -s 7 -set COPY_MUT_PROB 0.01 -set EVENT_FILE events-long.cfg -set DATA_DIR out-7`
becomes, after `avida convert arm-a` and
`avida convert arm-a --section events events-long.cfg`,
`avida -s 7 -set mutation.copy_prob 0.01 -set events events-long.yaml -set general.data_dir out-7`.

## Settings by group
"#;

fn cell(s: &str) -> String {
    s.split('\n')
        .map(str::trim)
        .filter(|l| !l.is_empty())
        .collect::<Vec<_>>()
        .join(" ")
        .replace('|', "\\|")
}

fn code_list(keys: &[&str]) -> String {
    let quoted: Vec<String> = keys.iter().map(|k| format!("`{k}`")).collect();
    match quoted.len() {
        0 => String::new(),
        1 => quoted[0].clone(),
        n => format!("{} and {}", quoted[..n - 1].join(", "), quoted[n - 1]),
    }
}

/// The text of `docs/reference/config-yaml.md`.
pub fn reference_page() -> String {
    let stock: Vec<&str> = STOCK_SETS.iter().map(|s| s.name).collect();
    let inst_keys: Vec<&str> = INST_KEYS.iter().map(|(k, _)| *k).collect();
    let mut o = INTRO
        .replace("STOCK_NAMES", &code_list(&stock))
        .replace("INST_SET_KEYS", &code_list(INST_SET_KEYS))
        .replace("INST_KEY_LIST", &code_list(&inst_keys))
        .replace("RESOURCE_KEY_LIST", &code_list(RESOURCE_KEYS))
        .replace("CELL_KEY_LIST", &code_list(CELL_KEYS))
        .replace("GRADIENT_KEY_LIST", &code_list(GRADIENT_KEYS))
        .replace("GRID_KEY_LIST", &code_list(GRID_KEYS))
        .replace("REACTION_KEY_LIST", &code_list(REACTION_KEYS))
        .replace("PROCESS_KEY_LIST", &code_list(PROCESS_KEYS))
        .replace("REQUISITE_KEY_LIST", &code_list(REQUISITE_KEYS))
        .replace("EVENT_KEY_LIST", &code_list(EVENT_KEYS));
    o.push('\n');
    o.push_str("Each group lists its settings with the C++ name, the type and the default.\n");
    o.push_str("The descriptions are those of the C++ program, with `true` and `false` for the 0 and 1 of a flag.\n");
    for g in GROUPS {
        o.push_str(&format!("\n### `{}`\n\n{}.\n\n", g.key, g.title));
        o.push_str("| Key | C++ name | Type | Default | Description |\n|---|---|---|---|---|\n");
        for s in naming::settings_of_group(g.key) {
            let mut old = format!("`{}`", s.old);
            for a in s.aliases() {
                old.push_str(&format!(", `{a}`"));
            }
            o.push_str(&format!(
                "| `{}` | {} | {} | `{}` | {} |\n",
                s.key,
                old,
                s.kind().name(),
                cell(&s.default_display()),
                cell(s.desc())
            ));
        }
    }
    o.push_str("\n## Old names\n\nEvery C++ setting with its place in the experiment file.\n\n");
    o.push_str("| C++ name | In the experiment file |\n|---|---|\n");
    let mut rows: Vec<(String, String)> = Vec::new();
    for r in NAMES {
        let target = match r.target {
            Target::Setting(g, k) => format!("`{g}.{k}`"),
            Target::Section(s) => format!("the section `{s}`"),
            Target::ConverterOnly => {
                "read by `avida convert` only (it chose a legacy instruction-set file)".to_string()
            }
        };
        rows.push((r.old.to_string(), target.clone()));
        if let Some(e) = all_entries().find(|e| e.names[0] == r.old) {
            for a in &e.names[1..] {
                rows.push((a.to_string(), format!("{target} (an alias)")));
            }
        }
    }
    rows.sort();
    for (old, target) in rows {
        o.push_str(&format!("| `{old}` | {target} |\n"));
    }
    o
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reference_page_is_current() {
        let path = concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../../../docs/reference/config-yaml.md"
        );
        let page = reference_page();
        if std::env::var_os("AVIDA_UPDATE_REFERENCE").is_some() {
            std::fs::write(path, &page).unwrap();
        }
        let current = std::fs::read_to_string(path).unwrap_or_default();
        assert!(
            current == page,
            "docs/reference/config-yaml.md is out of date: run this test with AVIDA_UPDATE_REFERENCE=1"
        );
    }
}
