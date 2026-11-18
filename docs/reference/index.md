# Reference

Look-up pages for every file format, setting, action and command. They are written for readers who already know the ideas; the [concepts](../concepts/index.md) pages explain those.

| Page | What it covers |
|---|---|
| [The experiment file (avida.yaml)](config-yaml.md) | The layout of the one file of an experiment, its groups and sections, the command line, `avida convert`, and every setting with its group, old name, type, default and description. |
| [Environment](environment-file.md) | The section `environment`: resources (plain, cell and gradient), grids, reactions and `set_active`, and the options of resources, processes and requisites. |
| [Task reference](tasks.md) | The tasks you can attach to a reaction, and the arguments of those that take any. |
| [Deme migration matrix](migration-matrix.md) | The file format of `MIGRATION_FILE`, the settings that use it and the actions that change and report it. |
| [The look sensor](look-sensor.md) | What the look instructions read and return, and the settings that govern them. |
| [Events](events-file.md) | The section `events`: the entry format, triggers and timing, the stock events and the most used actions. |
| [Actions reference](actions.md) | All 423 actions with their arguments. |
| [Instruction sets and organism files](instruction-sets.md) | The section `instruction_sets`, the stock sets, the format of `.org` files, and how to write an ancestor. |
| [Analyze mode](analyze-mode.md) | Running analyze mode, batches, variables and functions, the data columns and the command list. |
| [Features that stop with unsupported](unsupported.md) | What the port refuses to run, and why. |
| [Glossary](glossary.md) | Every term of this documentation, defined. |

The output files are described in [Output files](../getting-started/output-files.md), and the command-line options in [the command-line page](../front-ends/cli.md).

## How these pages were checked

Settings, task names, action names and analyze commands are generated from, or checked against, the tables in the Rust crates (`crates/config`, `crates/tasks`, `crates/world`, `crates/analyze`). File-format rules were checked against the parsers (`crates/environment`, `crates/world/src/event_list.rs`, `crates/genome`). Where a page quotes the program's older documentation for an explanation, it says so.
