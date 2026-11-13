# Avida documentation

Avida is a research platform for digital evolution. You write a small self-replicating program (an *organism*), place it in a grid world, and let it copy itself with occasional mistakes (*mutations*). Organisms that copy faster leave more offspring, so the population evolves. You decide what the world rewards by describing *tasks* (logic functions the organisms can compute) and the *resources* that pay for them, and you watch what evolution makes of it.

This documentation covers the native Rust port of Avida, which lives in `rust/avida-native` in the repository. It produces the same configuration files and the same output files as the original C++ program, byte for byte in its default (exact) mode. The C++ program itself has been removed from the tree.

## Where to start

| If you want to... | Read |
|---|---|
| Install the tools, build the program and run it once | [Getting started](getting-started/index.md) |
| Understand what the program wrote to disk | [Output files](getting-started/output-files.md) |
| Watch a run in a window or a terminal | [The three front ends](front-ends/index.md) |
| Learn the ideas (genomes, tasks, mutations, demes) | [Concepts](concepts/index.md) |
| Look up a setting, an action or a file format | [Reference](reference/index.md) |
| Repeat a known experiment from the literature | [Guides and tutorials](guides/index.md) |
| Run faster on many cores | [Execution modes](execution/index.md) |
| Move over from C++ Avida | [Migration from C++ Avida](migration/from-cpp.md) |
| Change the code | [For contributors](contributing/index.md) |
| Look up a word | [Glossary](reference/glossary.md) |

## The shortest path to a running experiment

1. Install Rust from [rustup.rs](https://rustup.rs/).
2. Build the command-line program: `cargo build --release -p avida` inside `rust/avida-native`.
3. Copy the stock configuration, `cp -r avida-core/support/config my-run`, and run `avida` inside that folder.

[Getting started](getting-started/index.md) walks through each step and says what you should see.

## How the documentation is organised

- [Getting started](getting-started/index.md): install, build, first run, output files.
- [The three front ends](front-ends/index.md): the command line (`avida`), the desktop application (`avida-gui`) and the terminal interface (`avida-tui`).
- [Concepts](concepts/index.md): how the simulation works, one topic per page, with diagrams.
- [Reference](reference/index.md): every setting, the environment, events and instruction-set file formats, the actions, the tasks, the analyze-mode language and a glossary.
- [Guides and tutorials](guides/index.md): nine worked experiments and a set of short how-to pages.
- [Execution modes](execution/index.md): the exact mode and the opt-in parallel mode, and what "parity with the C++" means.
- [For contributors](contributing/index.md): architecture, validation gates and how parity is tested.
- [Migration from C++ Avida](migration/from-cpp.md) and the [map from the old wiki](migration/wiki-map.md).

## Conventions in these pages

- Commands are shown for a Unix shell (macOS or Linux). On Windows, use the same commands in a shell that provides them, or adapt the paths.
- `NAME_IN_CAPITALS` is a setting of `avida.cfg`. Names in `CamelCase` such as `PrintAverageData` are actions that an events file schedules. Actions are not case-sensitive.
- Paths such as `rust/avida-native` are relative to the repository root.
- A page marks anything that stops with an `unsupported:` message. See [Features that stop with unsupported](reference/unsupported.md).
