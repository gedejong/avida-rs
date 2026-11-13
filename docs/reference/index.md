# Reference

Look-up pages for every file format, setting, action and command. They are written for readers who already know the ideas; the [concepts](../concepts/index.md) pages explain those.

| Page | What it covers |
|---|---|
| [Settings reference (avida.cfg)](settings.md) | All 431 settings of `avida.cfg` plus the two port-only settings, with types, defaults and descriptions, grouped as the file groups them. |
| [Environment file (environment.cfg)](environment-file.md) | The directives `RESOURCE`, `GRADIENT_RESOURCE`, `CELL`, `REACTION`, `SET_ACTIVE` and `GRID`, and the options of resources, processes and requisites. |
| [Task reference](tasks.md) | The tasks you can attach to a reaction, and the arguments of those that take any. |
| [Deme migration matrix](migration-matrix.md) | The file format of `MIGRATION_FILE`, the settings that use it and the actions that change and report it. |
| [The look sensor](look-sensor.md) | What the look instructions read and return, and the settings that govern them. |
| [Events file (events.cfg)](events-file.md) | The event line format, the timing syntax, the stock file and the most used actions. |
| [Actions reference](actions.md) | All 423 actions with their arguments. |
| [Instruction sets and organism files](instruction-sets.md) | The format of `instset-*.cfg` files and `.org` files, and how to write an ancestor. |
| [Analyze mode](analyze-mode.md) | Running analyze mode, batches, variables and functions, the data columns and the command list. |
| [Features that stop with unsupported](unsupported.md) | What the port refuses to run, and why. |
| [Glossary](glossary.md) | Every term of this documentation, defined. |

The output files are described in [Output files](../getting-started/output-files.md), and the command-line options in [the command-line page](../front-ends/cli.md).

## How these pages were checked

Settings, task names, action names and analyze commands are generated from, or checked against, the tables in the Rust crates (`crates/config`, `crates/tasks`, `crates/world`, `crates/analyze`). File-format rules were checked against the parsers (`crates/environment`, `crates/world/src/event_list.rs`, `crates/genome`). Where a page quotes the program's older documentation for an explanation, it says so.
