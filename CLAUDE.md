# Avida — Claude Code Context

This file is automatically loaded by Claude Code in every session for this project.

---

## What this repository is (as of 2026-11-12)

Avida is now the native Rust workspace in `rust/avida-native`. It supersedes the C++ core and the FFI migration (`rust/avida-rust`), both of which were removed from the tree. The last commit that contains them is the git tag **`cpp-final`**; `git show cpp-final:avida-core/source/<path>` reads any original file.

Still in the tree from the original program, because the port uses them:
- `avida-core/tests` — the consistency suite (configs and expected output)
- `avida-core/support/config` — the stock configuration
- `documentation` — the wiki (git submodule)

The old native viewers (`apps/viewer-macos`, `apps/viewer-windows`) wrapped the C++ core and were removed with it; the tag `cpp-final` has them.

Read `rust/avida-native/ARCHITECTURE.md` (crates, layering, status, what still stops with `unsupported:`) and `rust/avida-native/PORTING.md` (how parity is kept) before changing simulation code.

---

## Working rules

### 1. Plan-first execution
- If asked to plan, produce a concrete plan with clear scope and validation gates.
- Do not edit attached plan files; implement exactly from the plan unless the user requests changes.

### 2. Todo discipline
- Reuse existing todos from the plan; do not create duplicates.
- Keep exactly one todo `in_progress`; mark each `completed` as work finishes.

### 3. Implementation constraints
- **Behaviour parity with the C++ is the default.** Output is compared byte for byte. No silent policy changes: a deliberate departure from the original behaviour needs the maintainer's decision and a note in `PORTING.md`.
- A feature path that is not ported must fail loudly (`unsupported: SETTING=value (C++: file:symbol)`), never run with different behaviour.
- No `unsafe` (the workspace lints forbid it).
- Crates may depend only on crates below them in the layer table of `ARCHITECTURE.md`. The front ends (`gui`, `tui`) go through `session` and `viewmodel`, never through `world` directly.
- Floating-point expressions follow the reference binary's evaluation, including where it does not fuse multiply-add; see `PORTING.md` before "simplifying" arithmetic.
- **Instruction handler ordering**: a handler consumes its nop modifiers in the same order as the C++ did, relative to its guards. Moving a register lookup across a guard changes the instruction pointer when the guard fails.

### 4. Mandatory validation gates

Run inside `rust/avida-native`:
```
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

For any change to simulation behaviour, also the whole-suite comparison against the reference binary (needs `cbuild/work/avida`, built from the tag `cpp-final`):
```
cargo build --release -p avida
python3 tools/run_suite.py
```
Expected: 85 same, 34 trivial, 1 UNSUPPORTED, 1 REF-TIMEOUT, no DIFF.

The terminal interface's pty tests are timing-sensitive; run them on a machine that is not saturated, and check for stray `avida-tui` processes afterwards.

### 5. Documentation refresh
After gates pass, update `rust/avida-native/ARCHITECTURE.md` (status, counts, the list of `unsupported:` stops) and `documentation/Tech-Debt-Backlog.md`. Keep docs factual and metric-backed; no conflicting figures between them.

The `rust-migration-*.md` files at the repository root record the FFI migration and its supersession. They are history; do not extend them.

### 6. Commit hygiene
- Commit only when the user requests.
- Stage intended files explicitly; avoid unrelated submodule pointer changes.
- Use concise commit messages focused on *why* the change was made.

### 7. GitHub issue discipline
- **NEVER close issues.** Issues with the `tracking` label are protected by a CI workflow that auto-reopens them. Only a human maintainer may close tracking issues (by removing the label first).
- **NEVER use `Fixes #N` or `Closes #N`** in commit messages or PR descriptions for tracking issues. These keywords auto-close the referenced issue when merged. Use **`Relates to #N`** or **`Part of #N`** instead.
- When work completes part of a tracking issue, **add a progress comment** to the issue describing what was done, not close it.
- Only use `Fixes #N` for small, self-contained bug-fix issues that are fully resolved by a single PR.

---

## Current state (2026-11-12)

- 17 crates, about 200K lines of Rust: `apto`, `config`, `genome`, `tasks`, `environment`, `phenotype`, `systematics`, `resources`, `hardware`, `script`, `analyze`, `world`, `avida` (binary), `viewmodel`, `session`, `gui`, `tui`.
- Arena recording and playback: `avida-rec` (headless), `--record` and `--play` on the GUI and TUI (`ARCHITECTURE.md`, "Recording and playback").
- All five hardware types, all 423 distinct actions, analyze mode (batch, interactive, multi-core), demes, parasites, predator/prey, gene transfer, the energy model.
- Suite: 85 byte-identical, 34 fail the same way the C++ does, none differs.
- Default run (3000 updates, seed 42): about 47 s in exact mode against 57–64 s for the C++; 18 s in parallel mode on four threads. With gene transfer on: 35 s against 42 s.

### Open work
1. Parallel execution mode (`EXECUTION_MODE 1`, opt-in, not byte-identical by design): threads beyond four add nothing; the sequential phase and the statistics gather are the limit. Demes run since 2026-11-13; with implicit deme replication a deme generation takes about one update longer than in the exact mode (`ARCHITECTURE.md`, "Demes").
2. `unsupported:` stops: where the C++ behaviour is undefined or crashes the stop stays (maintainer's ruling, 2026-11-12). The stops with defined C++ behaviour were closed on 2026-11-13 except a few narrow cases listed in `documentation/Tech-Debt-Backlog.md` (a `MAX_PREY_BT` kill from a plasticity or event-time test CPU, merit driven to zero by a damaging gradient).
3. The GUI and TUI were reviewed from rendered sweeps (`cargo test -p avida-gui --test audit -- --ignored`, `cargo test -p avida-tui --test screens -- --ignored`); nobody has yet judged them in a real window or terminal.
4. A single long simulation step cannot be interrupted; on close the front ends wait 2 s and detach the thread.
