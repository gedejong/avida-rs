# For contributors

This section is for people who change the Rust code. It points to the in-repository design documents instead of copying them, so there is one source of truth.

| Page | Purpose |
|---|---|
| [Architecture](architecture.md) | The crates, their layers, and how a front end reaches the simulation. |
| [Validation gates](validation.md) | What to run before you call a change done. |
| [How parity is tested](parity.md) | The consistency suite, the reference binary and the statistical check for the parallel mode. |
| [Profiling the program](../guides/how-to-flame-graph.md#profiling-the-program-itself) | How to see where a run spends its CPU time (`sample` was run; the flame-graph tools are an untested recipe). |

## The two documents to read first

In the repository, under `rust/avida-native`:

- `ARCHITECTURE.md`: the crate layering table, the status of each crate, the ownership model (ids instead of pointers, organism state and hardware borrowed separately, the running organism taken out of its cell), the parallel execution mode in full, and the list of what still stops with `unsupported:`.
- `PORTING.md`: the rules for keeping parity with the C++ (same random-number call order, same integer and floating-point behaviour, quirks kept and marked), the one approved departure from parity, the tasks the C++ does not have, and notes on individual decisions.

These two files are not part of this documentation, because they change with the code. Open them in the repository next to your editor.

## House rules in short

- **Behaviour parity with the C++ is the default.** Output is compared byte for byte. A deliberate departure needs the maintainer's decision and a note in `PORTING.md`.
- **A path that is not ported fails loudly** with `unsupported: SETTING=value (C++: file:symbol)`. It never runs with different behaviour.
- **No `unsafe`.** The workspace lints forbid it.
- **Layering.** A crate may depend only on crates below it. The front ends go through `session` and `viewmodel`, never through `world` directly.
- **Floating point follows the reference binary's evaluation**, including where it does not fuse a multiply-add. Read the floating-point section of `PORTING.md` before "simplifying" arithmetic.
- **Instruction handler ordering.** A handler consumes its nop modifiers in the same order as the C++ did, relative to its guards. Moving a register lookup across a guard changes the instruction pointer when the guard fails.
- **Original source for reference.** The C++ lives at the git tag `cpp-final`: `git show cpp-final:avida-core/source/<path>` reads any original file. The `C++: file:symbol` notes in doc comments refer to paths at that tag.

## Where the original program went

The C++ core, the CMake build, the old native viewers and the FFI migration crate were removed from the working tree. Still in the tree because the port uses them:

- `avida-core/tests`, the consistency suite (configurations and expected output).
- `avida-core/support/config`, the stock configuration.
- `documentation`, the old wiki (a git submodule), which these pages replace.
