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
- **Behaviour parity with the C++ is the default for what the C++ did.** Output is compared byte for byte, and the consistency suite stays at no DIFF. Since 2026-11-15 new opt-in features beyond the C++ (instruction sets, tasks, actions, views) may be added without a decision for each (maintainer: "From here on it's ok to depart from the C++ version"); each still gets a `PORTING.md` entry. Changing what an existing configuration does still needs the maintainer's decision.
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

CI runs the same suite on macOS arm64 from a packed snapshot of the reference output (`tools/suite-ref.tar.xz`), without the reference binary. After adding or changing a test under `avida-core/tests`, refresh it: delete `target/suite/ref`, run the suite with the reference binary, then `python3 tools/suite_ref.py pack`. The compiler version is pinned in `rust/avida-native/rust-toolchain.toml` so that CI and local gates agree; raise it deliberately.

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
- Default run (3000 updates, seed 42): about 47 s in exact mode against 57–64 s for the C++; 13–15 s in parallel mode on four threads. With gene transfer on: 35 s against 42 s.

### Open work
1. Parallel execution mode (`EXECUTION_MODE 1`, opt-in, not byte-identical by design): since 2026-11-13 output tests and divides run inside the bursts on the stock environment, as do worlds whose resources are all plain per-cell grids; since 2026-11-14 a world whose resources are all global (well-mixed) pools also runs its output tests in the bursts and settles the pool at the end of each round (`PORTING.md`; 12.5 s to 7.7 s on four threads for a 60x60 nine-pool world, accepted by the maintainer the same day); worlds mixing plain grids and global pools do the same (6.5 s to 3.9 s on four threads for a 60x60 world with four pools and five grids), since later that day also when a reaction uses both kinds, with `PARTIAL` resources (treated as masked pools) and with gradients that do not act on organisms (10.0 to 8.0 s, 11.4 to 6.9 s and 9.6 to 6.5 s on 60x60 worlds; accepted by the maintainer the same day). Only predatory, deadly and damaging gradients still run their output tests sequentially. Measured 2026-11-14: the divide commits (about a quarter of the update loop) and the main thread's serial share (about 40%) bound eight threads at about 1.3x over four; the machine gives about 1.2x. Demes run since 2026-11-13; with implicit deme replication a deme generation takes about one update longer than in the exact mode (`ARCHITECTURE.md`, "Demes").
2. `unsupported:` stops: where the C++ behaviour is undefined or crashes the stop stays (maintainer's ruling, 2026-11-12). The stops with defined C++ behaviour were closed on 2026-11-13 and on 2026-11-15 (a `MAX_PREY_BT` kill from a plasticity or event-time test CPU), except a kill from a loop of test CPU runs or the precalculated phenotype (`documentation/Tech-Debt-Backlog.md`). Merit driven to zero by a damaging gradient segfaults in the reference binary and stays stopped.
3. The GUI and TUI were reviewed from rendered sweeps (`cargo test -p avida-gui --test audit -- --ignored`, `cargo test -p avida-tui --test screens -- --ignored`); the maintainer judged the desktop app in a real window on 2026-11-14 ("the window looks good", after the condition form, range slider and scatter jitter fixes), and the terminal interface in a real terminal the same day ("tui looks good. No obvious bugs so far").
4. Composed computation (extensions beyond the C++, `PORTING.md`; experiment page `docs/guides/experiment-composed-computation.md`): the modular heads set (`instset-heads-modular.cfg`: six registers, `call`, `return`), the task `field_arith` and the ladder `environment-adder-ladder.cfg`, the Ladder view, the knockout map and reuse measures (`PrintKnockoutMap`, `PrintReuseData`, the Reuse views), and the arm-comparison view (`--compare DIR` on both front ends). The maintainer saw the stage 1 and stage 2 demonstrations on 2026-11-15 ("checked step 1-2. lgtm"). The stage 3 experiment (modular against stock, 10 seeds per arm, 100,000 updates) finished the same day with a negative result, which the maintainer accepted: neither arm got past HALFADD_PLANES (1 modular seed, 3 stock seeds, Fisher exact p = 0.58), and `call` was not used. Stage 4 (sex, population size, seasons and a minimum genome size, one factor at a time on the modular set) finished on 2026-11-16 (lab folder `14-search-conditions`, results on the experiment page, accepted by the maintainer the same day): with `MIN_GENOME_SIZE 50` all 10 seeds reach and keep HALFADD_PLANES against 1 of 10 in the base arm (Fisher exact p = 0.00012, 0.0006 after Holm over five); sex (3 of 10), a 30x30 and a 120x120 world and seasons (0 of 10 each) show no difference; no arm got past HALFADD_PLANES and `call` was not used as a subroutine. A check on 2026-11-16 showed that the HALFADD_PLANES rung of stages 3 and 4 was cleared by NAND then AND (87.5 % of the bits, the threshold) and that no run made XOR; the experiment page carries the correction. Stage 5 (design fixed on the page; lab folder `15-xor-wall`, the maintainer starts the batch) pays the field rungs at full quality only, then adds the stock logic steps below XOR, then islands. The `sex` arm of stage 4 ran with `DROP_EXTINCT_GENOTYPES 1` (port-only setting, `PORTING.md`): with two parents the C++ bookkeeping keeps every ancestor, several GB per run.
5. A single long simulation step cannot be interrupted; on close the front ends wait 2 s and detach the thread.
