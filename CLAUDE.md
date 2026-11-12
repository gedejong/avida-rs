# Avida — Claude Code Context

This file is automatically loaded by Claude Code in every session for this project.

---

## What this repository is (as of 2026-11-12)

Avida is now the native Rust workspace in `rust/avida-native`. It supersedes the C++ core and the FFI migration (`rust/avida-rust`), both of which were removed from the tree. The last commit that contains them is the git tag **`cpp-final`**; `git show cpp-final:avida-core/source/<path>` reads any original file.

Still in the tree from the original program, because the port uses them:
- `avida-core/tests` — the consistency suite (configs and expected output)
- `avida-core/support/config` — the stock configuration
- `documentation` — the wiki (git submodule)

`apps/viewer-macos` and `apps/viewer-windows` are the old native viewers. They wrapped the C++ core and no longer build; whether to delete them is the maintainer's decision.

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

- 17 crates, about 189K lines of Rust: `apto`, `config`, `genome`, `tasks`, `environment`, `phenotype`, `systematics`, `resources`, `hardware`, `script`, `analyze`, `world`, `avida` (binary), `viewmodel`, `session`, `gui`, `tui`.
- All five hardware types, all 423 distinct actions, analyze mode (batch, interactive, multi-core), demes, parasites, predator/prey, gene transfer, the energy model.
- Suite: 85 byte-identical, 34 fail the same way the C++ does, none differs.
- Default run (3000 updates, seed 42): about 45 s against 57–64 s for the C++.

### Open work
1. `phenplastbonus` (needs test CPUs run from inside task evaluation; cross-crate restructuring).
2. Edge cases of `Task_Optimize` (unknown function, negative threshold, `tot == 0`).
3. Audit of `analyze` and the ASL interpreter for multiply-add fusion, site by site.
4. Gene-transfer runs are 11–13% slower than the C++ (per-slice genome copy).
5. The GUI's `Session` drop joins the simulation thread without a time limit.
6. Settings that stop with `unsupported:` because the C++ behaviour there is undefined are listed in `ARCHITECTURE.md`; each needs a decision, not a port.
