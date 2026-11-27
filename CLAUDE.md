# Avida — Claude Code Context

This file is automatically loaded by Claude Code in every session for this project.

---

## What this repository is (as of 2026-11-12)

Avida is now the native Rust workspace in `rust/avida-native`. It supersedes the C++ core and the FFI migration (`rust/avida-rust`), both of which were removed from the tree. The last commit that contains them is the git tag **`cpp-final`**; `git show cpp-final:avida-core/source/<path>` reads any original file.

Still in the tree from the original program, because the port uses them:
- `avida-core/tests` — the consistency suite (configs and expected output)
- `avida-core/support/config` — the stock configuration
- `documentation` — the wiki (git submodule)

Since 2026-11-18 an experiment is one YAML file, `avida.yaml` (settings in groups, instruction sets, environment, events; `.org` files and `analyze.cfg` stay files of their own). `-set` takes a dotted path (`-set mutation.copy_prob 0.0025`); the old `avida.cfg`, `environment.cfg` and `events.cfg` are refused, and `avida convert DIR` converts a folder of them. The suite's reference output was made by the reference binary from the old files; `tools/run_suite.py` compares the port's run of each test's `avida.yaml` with it, leaving out what the reference printed about its input files. See `docs/reference/config-yaml.md` and `PORTING.md` ("The experiment file departs from the C++ input format").

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

CI runs the same suite on macOS arm64 from a packed snapshot of the reference output (`tools/suite-ref.tar.xz`), without the reference binary. After adding or changing a test under `avida-core/tests`, refresh it: delete `target/suite/ref`, run the suite with the reference binary and `SUITE_REF_TESTS` set to a copy of `avida-core/tests` in the old input format (the reference binary does not read `avida.yaml`; take the tests of a `cpp-final` worktree and write the changed test in the old files too), then `python3 tools/suite_ref.py pack`. The compiler version is pinned in `rust/avida-native/rust-toolchain.toml` so that CI and local gates agree; raise it deliberately.

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

## Current state (2026-11-27)

- 17 crates, about 200K lines of Rust: `apto`, `config`, `genome`, `tasks`, `environment`, `phenotype`, `systematics`, `resources`, `hardware`, `script`, `analyze`, `world`, `avida` (binary), `viewmodel`, `session`, `gui`, `tui`.
- Arena recording and playback: `avida-rec` (headless), `--record` and `--play` on the GUI and TUI (`ARCHITECTURE.md`, "Recording and playback").
- All five hardware types, all 423 distinct actions, analyze mode (batch, interactive, multi-core), demes, parasites, predator/prey, gene transfer, the energy model.
- Suite: 85 byte-identical, 34 fail the same way the C++ does, none differs.
- Default run (3000 updates, seed 42): about 47 s in exact mode against 57–64 s for the C++; 13–15 s in parallel mode on four threads. With gene transfer on: 35 s against 42 s.

### Open work
1. Parallel execution mode (`execution.mode: 1`, opt-in, not byte-identical by design). Output tests and divides run inside the bursts on the stock environment and on worlds of per-cell grids, global pools, both mixed, `PARTIAL` resources and gradients that do not act on organisms (accepted by the maintainer 2026-11-14). Predatory, deadly and damaging gradients still run their output tests sequentially. Eight threads give about 1.2x over four, bound by the divide commits and the main thread's serial share. Demes run; with implicit deme replication a deme generation takes about one update longer than in the exact mode. Details in `PORTING.md` and `ARCHITECTURE.md` ("Demes").
2. `unsupported:` stops: where the C++ behaviour is undefined or crashes the stop stays (maintainer's ruling, 2026-11-12). The stops with defined C++ behaviour are closed, except a kill from a loop of test CPU runs or the precalculated phenotype (`documentation/Tech-Debt-Backlog.md`). Merit driven to zero by a damaging gradient segfaults in the reference binary and stays stopped.
3. GUI and TUI: reviewed from rendered sweeps (`cargo test -p avida-gui --test audit -- --ignored`, `cargo test -p avida-tui --test screens -- --ignored`) and accepted by the maintainer in a real window and terminal on 2026-11-14.
4. Composed computation: a series of experiments, extensions beyond the C++ (`PORTING.md`), recorded stage by stage on `docs/guides/experiment-composed-computation.md` with lab folders in `~/avida-lab`. Built for it: the modular heads set (`instset-heads-modular.yaml`, `call`, `return`), the task `field_arith` (adder steps and sums on bit fields, such as `carry`, `partial`, `add` and `add3` to `add8`; graded pay `2^(v q)`; `expect=1` pays the genotype's expected quality, exact mode only), the gate instructions `gate-0..15` and `pgate-0..15`, `slip_fill_mode`, `DROP_EXTINCT_GENOTYPES` (port-only), the Ladder and Reuse views, `PrintKnockoutMap`, `PrintReuseData` and the arm comparison (`--compare DIR`). In short: stages 3 to 10 (2026-11-15 to 11-20) found two walls, below XOR (crossed by paying XOR 8 above steps of 3 and 4) and at the step that combines two computed values; one judged output, graded pay and the gate instructions crossed the second (stage 11); evolved adders reached two and four bits (stages 12 and 13), are straight-line code, and grew their carry rounds by tandem duplication; copying slips make that growth fast (stage 14); copying composes an adder with itself, one carry round short (stage 15); few of those adders re-adapt to eight inputs (stage 16, 3 of 30 exact); on a ladder `add3` to `add8`, copying took 18 of 24 populations onto four to eight operands against none without, mostly to approximate sums that lose carries, and 3 of 24 hold an exact higher rung (stage 17, `34-ladder`); most climbs needed a copy of code that reads and adds an input, usually together with a read removed elsewhere. Every stage up to 17 was accepted by the maintainer. Batches run on rented Hetzner Cloud servers with `tools/cloud/cloud.sh` (maintainer's bound: cheap shared types only, at most 32 cores) and five cores of this Mac. Filed for later: a pure, concatenative computing core (maintainer, 2026-11-24) and the other candidates at the end of the experiment page. The wide-adder files (`environment-adder-wide.yaml`, `ladder-add3-*.org`, `ladder-add4-*.org`) need a redesign before use.
5. A single long simulation step cannot be interrupted; on close the front ends wait 2 s and detach the thread.
