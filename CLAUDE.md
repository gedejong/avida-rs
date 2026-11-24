# Avida — Claude Code Context

This file is automatically loaded by Claude Code in every session for this project.

---

## What this repository is (as of 2026-11-12)

Avida is now the native Rust workspace in `rust/avida-native`. It supersedes the C++ core and the FFI migration (`rust/avida-rust`), both of which were removed from the tree. The last commit that contains them is the git tag **`cpp-final`**; `git show cpp-final:avida-core/source/<path>` reads any original file.

Still in the tree from the original program, because the port uses them:
- `avida-core/tests` — the consistency suite (configs and expected output)
- `avida-core/support/config` — the stock configuration

Since 2026-11-18 an experiment is one YAML file, `avida.yaml` (settings in groups, instruction sets, environment, events; `.org` files and `analyze.cfg` stay files of their own). `-set` takes a dotted path (`-set mutation.copy_prob 0.0025`); the old `avida.cfg`, `environment.cfg` and `events.cfg` are refused, and `avida convert DIR` converts a folder of them. The suite's reference output was made by the reference binary from the old files; `tools/run_suite.py` compares the port's run of each test's `avida.yaml` with it, leaving out what the reference printed about its input files. See `docs/reference/config-yaml.md` and `PORTING.md` ("The experiment file departs from the C++ input format").
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

## Current state (2026-11-12)

- 17 crates, about 200K lines of Rust: `apto`, `config`, `genome`, `tasks`, `environment`, `phenotype`, `systematics`, `resources`, `hardware`, `script`, `analyze`, `world`, `avida` (binary), `viewmodel`, `session`, `gui`, `tui`.
- Arena recording and playback: `avida-rec` (headless), `--record` and `--play` on the GUI and TUI (`ARCHITECTURE.md`, "Recording and playback").
- All five hardware types, all 423 distinct actions, analyze mode (batch, interactive, multi-core), demes, parasites, predator/prey, gene transfer, the energy model.
- Suite: 85 byte-identical, 34 fail the same way the C++ does, none differs.
- Default run (3000 updates, seed 42): about 47 s in exact mode against 57–64 s for the C++; 13–15 s in parallel mode on four threads. With gene transfer on: 35 s against 42 s.

### Open work
1. Parallel execution mode (`EXECUTION_MODE 1`, `execution.mode: 1` in the experiment file, opt-in, not byte-identical by design): since 2026-11-13 output tests and divides run inside the bursts on the stock environment, as do worlds whose resources are all plain per-cell grids; since 2026-11-14 a world whose resources are all global (well-mixed) pools also runs its output tests in the bursts and settles the pool at the end of each round (`PORTING.md`; 12.5 s to 7.7 s on four threads for a 60x60 nine-pool world, accepted by the maintainer the same day); worlds mixing plain grids and global pools do the same (6.5 s to 3.9 s on four threads for a 60x60 world with four pools and five grids), since later that day also when a reaction uses both kinds, with `PARTIAL` resources (treated as masked pools) and with gradients that do not act on organisms (10.0 to 8.0 s, 11.4 to 6.9 s and 9.6 to 6.5 s on 60x60 worlds; accepted by the maintainer the same day). Only predatory, deadly and damaging gradients still run their output tests sequentially. Measured 2026-11-14: the divide commits (about a quarter of the update loop) and the main thread's serial share (about 40%) bound eight threads at about 1.3x over four; the machine gives about 1.2x. Demes run since 2026-11-13; with implicit deme replication a deme generation takes about one update longer than in the exact mode (`ARCHITECTURE.md`, "Demes").
2. `unsupported:` stops: where the C++ behaviour is undefined or crashes the stop stays (maintainer's ruling, 2026-11-12). The stops with defined C++ behaviour were closed on 2026-11-13 and on 2026-11-15 (a `MAX_PREY_BT` kill from a plasticity or event-time test CPU), except a kill from a loop of test CPU runs or the precalculated phenotype (`documentation/Tech-Debt-Backlog.md`). Merit driven to zero by a damaging gradient segfaults in the reference binary and stays stopped.
3. The GUI and TUI were reviewed from rendered sweeps (`cargo test -p avida-gui --test audit -- --ignored`, `cargo test -p avida-tui --test screens -- --ignored`); the maintainer judged the desktop app in a real window on 2026-11-14 ("the window looks good", after the condition form, range slider and scatter jitter fixes), and the terminal interface in a real terminal the same day ("tui looks good. No obvious bugs so far").
4. Composed computation (extensions beyond the C++, `PORTING.md`; experiment page `docs/guides/experiment-composed-computation.md`): the modular heads set (`instset-heads-modular.yaml`: six registers, `call`, `return`), the task `field_arith` and the ladder `environment-adder-ladder.yaml`, the Ladder view, the knockout map and reuse measures (`PrintKnockoutMap`, `PrintReuseData`, the Reuse views), and the arm-comparison view (`--compare DIR` on both front ends). The maintainer saw the stage 1 and stage 2 demonstrations on 2026-11-15 ("checked step 1-2. lgtm"). The stage 3 experiment (modular against stock, 10 seeds per arm, 100,000 updates) finished the same day with a negative result, which the maintainer accepted: neither arm got past HALFADD_PLANES (1 modular seed, 3 stock seeds, Fisher exact p = 0.58), and `call` was not used. Stage 4 (sex, population size, seasons and a minimum genome size, one factor at a time on the modular set) finished on 2026-11-16 (lab folder `14-search-conditions`, results on the experiment page, accepted by the maintainer the same day): with `MIN_GENOME_SIZE 50` all 10 seeds reach and keep HALFADD_PLANES against 1 of 10 in the base arm (Fisher exact p = 0.00012, 0.0006 after Holm over five); sex (3 of 10), a 30x30 and a 120x120 world and seasons (0 of 10 each) show no difference; no arm got past HALFADD_PLANES and `call` was not used as a subroutine. A check on 2026-11-16 showed that the HALFADD_PLANES rung of stages 3 and 4 was cleared by NAND then AND (87.5 % of the bits, the threshold) and that no run made XOR; the experiment page carries the correction. Stage 5 (design fixed on the page; lab folder `15-xor-wall`, the maintainer starts the batch) pays the field rungs at full quality only, then adds the stock logic steps below XOR, then islands. The `sex` arm of stage 4 ran with `DROP_EXTINCT_GENOTYPES 1` (port-only setting, `PORTING.md`): with two parents the C++ bookkeeping keeps every ancestor, several GB per run. Stage 5 ran on 2026-11-17 (30 runs, accepted negative): XOR was held by 0, 1 and 2 of 10 populations in its three arms. Stage 6 (three times the run length, 16 seeds per arm, lab folder `16-xor-time`, accepted negative on 2026-11-18): 0 and 1 of 16. Stage 7 (lab folder `18-xor-factorial`, 64 runs, accepted 2026-11-18) found the cause: with XOR paid 8 where the ladder paid 2, below steps paying 3 and 4, 32 of 32 populations hold XOR against 9 of 32 (Fisher p = 4e-10); a copy mutation rate of 0.0025 against 0.0075 and a genome floor of 100 against 50 show no difference in the planned tests. 25 of the 64 hold HALFADD_PLANES and none anything above it; that is the next wall. Runs since stage 5 are rented by the hour on cheap Hetzner Cloud servers with `tools/cloud/cloud.sh` (the maintainer's bound: cheap shared types only, at most 32 cores). Stage 8 (lab folder `19-above-xor`, 48 runs, accepted negative on 2026-11-18): neither corrected pay above XOR nor inputs pre-masked to the two-bit adder's layout (`INPUT_AND_MASK`, with `field_arith` judged on `width=24`) gets any of 12 populations per arm above HALFADD_PLANES; with the two adder steps paid, all 12 hold CARRY (`fn=carry`) and none PARTIAL (`fn=partial`), the step that combines two computed values. Stage 9 (lab folder `20-expected-pay`, 24 runs, accepted negative on 2026-11-19): three graded tiers of ADD2, paid on the organism's own draw or on the genotype's expected quality (`expect=1` on a `field_arith` task, `expected_quality.draws`, `expected_quality.min`; exact mode only), get no population to ADD2 or PARTIAL; expected quality removes the luck of a single draw (XOR held by 12 of 12 against 5 of 12), and a tier that pays on the first output passing a threshold rewards organisms for many different outputs. A next design should judge one designated output. Stage 10 (lab folder `23-heads-ex`, accepted negative on 2026-11-20): the experimental hardware does no better than the modular set below the combining step. Stage 11 (`24-soft-gates`, accepted 2026-11-20): one judged output, graded pay (`2^(v q)`) and the gate instructions `gate-0..15` (the 16 two-input functions; a copy mutation of a gate mostly writes the gate one truth-table entry away) take 5 of 12 populations past XOR against 0 of 12 without gates (p = 0.037, a measure chosen after the data). Stage 12 (`25-gates-long`, 300,000 updates, accepted 2026-11-20): 4 of 30 populations hold the two-bit adder, exact on every output; the gate mutation rule (`gate` against `pgate`, the same functions mutated as any instruction) and a tripled mutation rate make no difference. Stage 13 (`28-weekend`, 62 cloud runs over a weekend for EUR 9.80, accepted 2026-11-24): from an evolved two-bit adder the three- and four-bit sums follow in nearly every population (the pay step `2^(12q)` against `2^(32q)` makes no difference); from scratch over 3 million updates 4 of 7 populations with gates reach the two-bit sum and 0 of 7 without (past XOR 7 of 7 against 2 of 7, p = 0.021); with the four-bit judge paid `2^(64q)` 1 of 12 holds the four-bit sum from scratch (2 compute it exactly). A study of nine lines of descent (`29-lineage`, on the page after stage 13) found every adder straight-line code, without loops or `call`; the first combining step took one mutation in each of five lines tested, and 12 of 15 steps that added a carry round were tandem duplications of the round's code. Candidate later experiments are listed at the end of the experiment page. The wide-adder files (`environment-adder-wide.yaml`, `ladder-add3-*.org`, `ladder-add4-*.org`) are preparation for a later stage and need a redesign before use: ADD2 and ADD3 lay their fields out differently.
5. A single long simulation step cannot be interrupted; on close the front ends wait 2 s and detach the thread.
