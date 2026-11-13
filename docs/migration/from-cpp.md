# Migration from C++ Avida

This page is for people who used the C++ Avida (the `avida` program built with CMake, or the Avida-ED and viewer applications) and now use the Rust port.

## What is the same

- **Configuration files.** `avida.cfg`, `environment.cfg`, `events.cfg`, instruction-set files, `.org` ancestors and `analyze.cfg` have the same formats and the same settings. The stock files in `avida-core/support/config` are unchanged, and a settings table of 431 entries (version id 2.14.0) is read from them. Existing experiment folders work as they are.
- **Output files.** The files `avida` writes have the same names, headers and formats. In the default exact mode, output is byte-identical to the C++ program's for the same configuration and seed (ignoring timestamps and the data-directory path). 85 of the 121 consistency tests are byte-identical, 34 fail the same way the C++ program does, none differ. See [execution modes](../execution/index.md).
- **The command line.** The same options (`-a`, `-c`, `-def`, `-e`, `-h`, `-i`, `-l`, `-r`, `-s`, `-set`, `-v`, `-v0` to `-v4`, `-w`, `--generate-config`). See [the command-line page](../front-ends/cli.md).
- **Actions and analyze commands.** All 423 distinct action names are known and implemented. Analyze mode (batch, interactive, multi-core) is ported.
- **Hardware types.** All five that the program supported (0, 2, 3, 4, 5), and demes, parasites, predator and prey, gene transfer and the energy model.

## What is new

| New | Where |
|---|---|
| `avida-gui`, a desktop application (egui) | [Desktop application](../front-ends/gui.md) |
| `avida-tui`, a terminal interface (ratatui) | [Terminal interface](../front-ends/tui.md) |
| `EXECUTION_MODE 1` and `EXECUTION_THREADS`, the opt-in parallel mode | [Execution modes](../execution/index.md) |
| Tasks that give partial credit for addition, increment and decrement (`add_lowbits`, `add_miss`, `add_lowbits4`, `add_miss4`, `inc_lowbits`, `dec_lowbits`, `arith_miss`) | [Task reference](../reference/tasks.md) |
| `unsupported:` stops where the C++ is undefined | [Unsupported features](../reference/unsupported.md) |
| Runs a little faster than the C++ in exact mode (the maintainer measured about 47 s against 57 to 64 s for the default 3,000-update run) | |

## What was removed

| Removed | What to use instead |
|---|---|
| The C++ core, the CMake build and `build_avida` | `cargo build --release -p avida` ([install](../getting-started/install.md)). The last commit with the C++ is the git tag `cpp-final`. |
| The FFI migration crate (`rust/avida-rust`) | The native workspace `rust/avida-native`. |
| The old native viewers (`apps/viewer-macos`, `apps/viewer-windows`) and the ncurses viewer | `avida-gui` and `avida-tui`. |
| A separate program for the ASL scripting language (`avida-s`) | None. The interpreter is in `crates/script` and is tested, but its `Config`, `World` and `Driver` objects are stubs, and no front end runs it. |
| The C++ developer guides | [For contributors](../contributing/index.md) and, in the repository, `ARCHITECTURE.md` and `PORTING.md`. |

If you need the original program, check out the tag: `git worktree add ../avida-cpp cpp-final`, and build it there with `./build_avida`. You then also have the reference binary used for parity checks.

## Where things moved

| In the C++ program | In the port |
|---|---|
| `cbuild/work/avida` | `rust/avida-native/target/release/avida` |
| The ncurses viewer | `avida-tui`, which also switches screens with letter keys; its key table is on the [terminal interface](../front-ends/tui.md) page. |
| Settings you could only change by editing `avida.cfg` | Also editable, with restart, on the configuration screen of both viewers. |
| `avida -a` reading `analyze.cfg` | Unchanged. |

## Differences to know about

- **Parallel mode is not exact.** It is off by default. Do not use it to reproduce published runs ([execution modes](../execution/index.md#what-it-does-not-guarantee)).
- **Some paths stop.** A run that ends with `unsupported:` hit a configuration that the C++ program handles with undefined behaviour or crashes on. The message names the C++ site.
- **Output formatting under `VERBOSITY 3`.** With `ChangeEnvironment` and `VERBOSITY 3`, the C++ prints resource set-up numbers in the stream's sticky format (`50.00000`) and the port plainly (`50`).
- **Two settings are new** and are listed apart: `EXECUTION_MODE` and `EXECUTION_THREADS` ([settings](../reference/settings.md#settings-of-the-rust-port)).
- **Hardware type 1** (the old SMT CPU) is not available.
- **Old documentation errors.** Several statements in the old wiki do not match the program: the `MUTATION` environment directive does not exist; `POP_CAP_ELDEST` is a cap in organisms, not a 0 or 1 switch; `PrintData` has 30 column names, not the longer list of the old page; the `INFECTION_MECHANISM` values are 0 to 3. These pages use the program's own tables.

## A migration checklist

1. Build `avida` as described in [Install and build](../getting-started/install.md).
2. Run your existing experiment folder with a fixed seed and compare a data file with one from your C++ build. They should be identical in the exact mode.
3. If a run stops with `unsupported:`, read the message and [the list](../reference/unsupported.md).
4. For long runs, try the parallel mode on a copy, and compare outcomes over several seeds rather than byte for byte.
