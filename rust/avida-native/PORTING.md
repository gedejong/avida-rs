# Native Rust port: rules for every crate in this workspace

This workspace is a from-scratch Rust implementation of Avida. It replaces the
C++ in `avida-core/` and `libs/apto/`; it does not link against it. The C++
tree and the built binary `cbuild/work/avida` remain in the repository as the
behavioural reference until the Rust binary reproduces every consistency test
in `avida-core/tests/`.

## Rules

1. **Behaviour parity is the specification.** Output files must match the C++
   byte for byte. That means: same RNG call order, same integer/float
   arithmetic (C `int` is `i32`, wrapping where C++ would overflow; `double`
   is `f64`; C casts truncate toward zero), same iteration order, same text
   formatting. Transliterate the C++ control flow; do not "improve" it. Where
   the C++ has a quirk or bug that affects observable behaviour, keep it and
   mark it with a `// QUIRK:` comment.
2. **No FFI, no unsafe.** `unsafe_code` is forbidden workspace-wide.
3. **Cite the source.** Every ported type and non-trivial function carries a
   doc comment naming the C++ file and symbol it came from.
4. **Stay in your crate.** Only edit files under your own crate directory. If
   you need something from another crate that does not exist yet, define a
   small trait or plain-data struct in your crate for it and note it in your
   report; the integrator will connect it.
5. **Errors.** Config/parse problems return `Result<_, String>` (or a small
   error enum) carrying the same message text the C++ prints. Never `panic!`
   on bad input; `unwrap`/`expect` only on true invariants.
6. **Tests.** Unit-test against values taken from the C++ source or from the
   reference binary and the config files in `avida-core/support/config/` and
   `avida-core/tests/*/config/`. Parsers must be tested by loading every such
   real file of their kind in the repo without error.
7. **Gates before reporting done** (run in `rust/avida-native`):
   `cargo test -p <your-crate>`, `cargo clippy -p <your-crate> --all-targets -- -D warnings`,
   `cargo fmt -p <your-crate>`.
8. **Do not** run git commands that change state, edit C++ files, or touch
   other crates, `Cargo.toml` of the workspace, or `rust/avida-rust`.

## Shared facts

- RNG: `avida_apto::AvidaRng` (bit-exact port of `Apto::RNG::AvidaRNG`), methods
  `get_double`, `get_uint(max)`, `get_int(max)`, `get_int_range(min,max)`,
  `p(prob)`, `get_rand_normal`, `get_rand_poisson`, `get_rand_binomial`.
- Existing FFI-shaped Rust in `rust/avida-rust/src/` (phenotype, task
  evaluators, resource-count helpers, CPU instruction logic, RNG) was verified
  against the C++ and may be used as reference or copied and de-FFI'd.
- The C++ reference binary is `cbuild/work/avida` (run from a directory
  containing `avida.cfg` etc.; `-s <seed>` sets the seed, `-set NAME VALUE`
  overrides a setting, `-a` runs analyze mode).

## Report format

End with: (a) the public API of your crate as a list of signatures, (b) what
is ported and tested, (c) anything stubbed, skipped, or uncertain, with the
C++ location of each.

## Working alongside other agents

- Up to four agents work concurrently, each in its own crate. `cargo` commands
  must always be scoped with `-p <your-crate>`; a sibling crate that does not
  compile is not your problem unless you depend on it, in which case report it.
- A crate may contain partial files from an earlier interrupted run. Read them,
  keep what is correct, finish or replace the rest.
- Work in vertical increments: get each module compiling and tested before
  starting the next, so an interruption never leaves the crate broken.
- Shared string and file-reading semantics (`cString`, `cStringUtil`,
  `cInitFile`) live in `avida-config` (`string_util`, `init_file`). Crates
  above it use those; crates that cannot depend on it yet write a private
  minimal reader and say so in the report.

## Which C++ is the reference

Parts of the current C++ tree delegate to FFI helpers in `rust/avida-rust`
(`avd_*` calls). Those helpers were written during the earlier strangler
migration and diverge from the original C++ in some edge cases. Where a C++
function now calls an `avd_*` helper, read the original body with
`git show 47f13dadb:<path>` (2025-01-27, the parent of the first Rust commit;
an earlier revision of this file wrongly named ef5c6d948, a 2016 commit that
predates `cellbox`, the resource inflow/outflow fix-up change and 170 other
upstream commits) and port that; use the
helper only as a reading aid. If the two differ in a way a consistency test
could observe, say so in the report.

## Floating point: the reference binary fuses multiply-add

The reference build (Apple clang, arm64, default `-ffp-contract=on`) compiles
`a*b + c`, `a*b - c`, `c - a*b` and the compound forms `x += a*b`, `x -= a*b`
into a single fused multiply-add, rounded once, whenever they are written as
one C++ expression. Separate statements are not fused, and neither is an
expression with a division between the multiply and the add. Use
`f64::mul_add` at exactly those sites (`a.mul_add(b, c)` for `a*b + c`,
`(-a).mul_add(b, c)` for `c - a*b`), otherwise results drift by one ulp and
data files stop matching. When in doubt, compile the C++ function in a small
harness on this machine and compare bit patterns.

This holds only for arithmetic the reference binary still runs as C++. Where
the current tree delegates to a Rust helper in `rust/avida-rust` (an `avd_*`
call, for example `cDoubleSum`, `cRunningAverage`, `cRunningStats`, the
inflow precalc tables, `Task_Optimize`, `get-res-stored`), rustc does not
contract, so the port must not use `mul_add` there either, even though the
pre-Rust C++ was fused. `otool -tv cbuild/work/avida` lists no
`fmadd`/`fmsub` in any `_avd_*` symbol.

`crates/analyze` and `crates/script` were audited site by site against the
reference disassembly (2026-11-12): each `mul_add` there matches a fused
instruction in the corresponding C++ function (vector `fmla` included), and
each product left unfused matches none, or sits in the 8-wide unrolled head
of `AnalyzeComplexityTwoSites`'s sum of products, which the C++ vectorises
without contraction. The ASL matrix product, absent from the reference
binary, was checked by compiling the same loop with the reference flags. No
site needed a change.

`%g`-style output must go through `avida_apto::fmt`, which reproduces macOS
libc including its trailing-zero quirk.

- A new crate must be created with `Cargo.toml` and `src/lib.rs` in the same step: workspace members are a glob, so a crate directory without a library target breaks `cargo` for every other agent.

## The C++ is retired (2026-11-12)

The C++ core (`avida-core/source`, `avida-core/include`, the CMake build, `libs/`) and the FFI crate `rust/avida-rust` were removed from the tree; the maintainer decided that this workspace supersedes the FFI migration. The last commit that has them is the git tag `cpp-final`.

What that changes for parity work:

- **The reference binary** (`cbuild/work/avida`) can no longer be rebuilt from the working tree. Check out `cpp-final` in a separate worktree (`git worktree add ../avida-cpp cpp-final`), build it there with `./build_avida`, and point `SUITE_REF` at the result. `tools/run_suite.py` caches the reference output, so the binary is needed only when the cache is cleared or a test is added.
- **Reading the original source**: `git show cpp-final:avida-core/source/main/cPopulation.cc`. The `C++: file:symbol` notes in doc comments and `unsupported:` messages refer to paths at that tag.
- **Reference-data generators** (`crates/resources/tools/build_ref.sh`, `crates/apto/tools/*.cc`) compile against the C++ and must be run from a `cpp-final` worktree. Their output is checked in under each crate's `testdata/`.
- Three tests compare tables with the C++ source text; the files they read are kept as `crates/tasks/testdata/reference/cTaskLib.cc`, `crates/config/testdata/reference/cAvidaConfig.h` and `crates/genome/testdata/reference/cHardwareCPU.cc`.

## The one approved departure from parity: `EXECUTION_MODE 1` (2026-11-12)

Rule 1 has one exception, which the maintainer approved explicitly: the
opt-in parallel execution mode (`EXECUTION_MODE 1`, a port-only setting with
no C++ counterpart). It reschedules the instructions of an update so that
organism-local ones run on worker threads, and its output is therefore not
byte-identical to the C++ or to the exact mode. Everything else stays under
rule 1:

- `EXECUTION_MODE 0` (the default) is the C++ scheduler, unchanged, and
  remains the oracle: the whole consistency suite runs in it.
- Code for the parallel mode lives in `crates/world/src/parallel.rs` and
  `crates/hardware/src/parallel.rs`; its hooks in the exact code paths
  (`World::process_step`, `HardwareCpu::single_process`, `AdjustSchedule`,
  `UpdateOrganismStats`, whose reading of the organisms the parallel mode
  runs on its worker pool) must not change what the exact mode computes,
  and a change there is checked with `tools/run_suite.py`.
- The parallel mode's budgets follow the character of `SLICING_METHOD`:
  exact shares under the deterministic methods, Poisson-distributed ones
  under the lottery methods. Exact shares under a lottery method were
  measured to leave cells empty longer (ARCHITECTURE.md, "Statistical
  equivalence").
- A parallel run depends on its configuration and seed only, never on
  `EXECUTION_THREADS` or the machine (`crates/avida/tests/parallel.rs`).
- Equivalence with the exact mode is statistical and is checked with
  `tools/parallel_stats.py` (many seeds per mode, Mann-Whitney per
  outcome). A change to the parallel mode's scheduling needs a fresh run of
  it, reported with the change.
- Global (well-mixed) resource pools (2026-11-14): when every resource of
  the population is a global pool and the environment passes
  `output_is_local_global` (no requisite that names other reactions, no
  `divide_only`, and the per-cell conditions), the burst evaluates the tasks
  of an output test and leaves the processes of the reactions that use a
  resource to the world. The organism stops after that instruction; at the
  start of the sequential phase the world runs `DoProcesses` for the
  deferred reactions against the pool as it is, one organism after the
  other in the update's permutation order, with the exact path's code. The
  pool arithmetic is not approximated and the pool never goes below zero.
  What differs from the exact mode is order and timing: the tests of a
  round settle at its end, and the pool's inflow and outflow advance
  between rounds. Fresh run of `tools/parallel_stats.py` (12 seeds per
  mode, 30x30, nine global pools): seven outcomes agree (p from 0.42 to
  0.98); the number of organisms differs by 2 cells in 900 (p = 0.001),
  the same kind of lingering empty cell already recorded for the stock
  configuration, cause not traced. Accepted by the maintainer on 2026-11-14.
  Global environments outside the gate run their output tests in the
  sequential phase as before.
- Mixed per-cell grids and global pools (2026-11-14): when every resource
  is a plain per-cell grid or a global pool, both kinds occur, no reaction
  uses both, and the environment passes `output_is_local_mixed` (the
  global-pool gate), the burst runs the processes of reactions that use
  grids against the organism's cell and defers those that use a pool, as
  for pools alone. A test that triggers both kinds becomes a pending
  output: the grid half is done in the burst, the pool half and the cell's
  whole change are applied by the world in the permutation order.
  Differences from the exact mode: tests settle at the end of the round,
  the pool advances by the round's time, and two `add` bonuses of one test
  may be summed in the other order (last bit). Fresh run of
  `tools/parallel_stats.py` (36 seeds per mode, 30x30, one grid and one
  pool): no outcome significant (smallest p = 0.074 before correction).
  Accepted by the maintainer on 2026-11-14. The rule that no reaction uses
  both kinds was lifted later the same day (next entry).
- Reactions on both kinds, `PARTIAL` resources and gradients (2026-11-14):
  (1) A reaction that takes from or gives to a global pool waits for the
  sequential phase as a whole even when it also uses a per-cell grid; its
  grid reads are the amounts the test saw (`PendingOutput::grid_levels`),
  its pool reads the pool at settlement. `output_is_local_mixed` refuses a
  world where such a waiting reaction takes from a grid that a later
  non-waiting reaction also takes from, because the settlement order (burst
  reactions first) would otherwise differ from the library order. Two `add`
  bonuses of one test may be summed in the other order (last bit).
  (2) A `PARTIAL` resource is a global pool in the exact mode with its
  amount read as 0 for a cell off its list (`doOutput`); it is treated as a
  pool and the settlement applies the same mask
  (`ResourceCount::mask_partial`). (3) A gradient is read and changed in
  the organism's own cell like a plain grid, including the pending `delta`
  its `StateAll` leaves, which `ModifyCell` folds in; the burst's copy of
  the cell does the same. Predatory, deadly and damaging gradients (their
  update calls back into the population) keep the output tests in the
  sequential phase. Differences from the exact mode: tests settle at the
  end of a round; a gradient's updates are run at the first phase of each
  update, where the exact mode lumps them into the next output test when no
  organism tests for several updates. Fresh run: 20 seeds per mode, 30x30,
  1500 updates, five worlds (grid and pool in one reaction, two `PARTIAL`
  pools, a global-pool control on the path accepted above, a gradient, a
  gradient and a pool); of 34 comparisons three have p < 0.05 before
  correction and none after Holm: the first pool's amount is lower in the
  parallel mode in the `PARTIAL` world (54.9 vs 43.6, p = 0.002) and
  equally in the control (40.1 vs 29.1, p = 0.009), so the shift belongs to
  the pool settlement and not to the `PARTIAL` mask. Its cause was traced
  the same day: the pool's clock advances by executed instructions and the
  tests of a round settle at its end, so with sparse tests an update's
  tests cluster in its last fifth, where the exact mode spreads them
  evenly. Consumption per update is the same (19.82 against 19.82 in a
  fixed population) and a pool nobody consumes follows the same curve to
  0.002. A pool read at an update boundary (`PrintResourceData`) is the
  trough of a sawtooth, lower than the exact mode's level by about
  x/(e^x - 1), x being the fraction of the pool taken per update (measured
  ratios 0.85, 0.71, 0.50, 0.29 at x of 0.46, 0.88, 1.7, 3.3), independent
  of the inflow rate and growing with the update size; and
  the control has 900 vs 899 organisms (p = 0.035), the empty-cell effect
  recorded above. Accepted by the maintainer on 2026-11-14.

ARCHITECTURE.md ("Parallel execution mode") describes what the mode
guarantees and what it does not.

## Extensions beyond the C++ no longer need a decision each (2026-11-15)

The maintainer's ruling of 2026-11-15: "From here on it's ok to depart from
the C++ version! We're now in new terrain." Read here as: new, opt-in
features (instruction sets, tasks, actions, views, execution options) may be
added without asking for each one. Two things stay as they were. Every
extension gets an entry in this file that says what it is and how it was
checked. And what the C++ did is still reproduced: a configuration that does
not name an extension runs as before, and the consistency suite stays at no
DIFF.

## Tasks the C++ does not have

Additions decided by the maintainer. They change nothing for a configuration
that does not name them.

- `add_lowbits` (2026-11-12): `add` with partial credit. The task quality is
  the number of low bits in which the output agrees with the sum of two
  inputs before the first wrong bit, divided by 32; fewer than 8 right bits
  count as none. Made for the lab
  experiment "an adder from NAND", where the all-or-nothing `add` gives
  selection nothing to act on. `crates/tasks/src/eval_math.rs`.
- `add_miss` (2026-11-12): scores 1 for an output that `add_lowbits` gives
  nothing, once two inputs have been read. With a small negative reaction
  value it makes wrong outputs cost merit; without it, populations in the lab
  pilot put out values in a loop until one agreed by chance.
- `add_lowbits4`, `add_miss4` (2026-11-13): the same two with a minimum of 4
  right low bits. With 8, the XOR of the inputs paid about as rarely as a
  guess did, so a penalty on misses could not tell them apart.
- `inc_lowbits`, `dec_lowbits`, `arith_miss` (2026-11-13): the increment
  and the decrement of an input with the same partial credit (minimum 6
  right low bits; only inputs whose lowest bit makes the step carry count,
  because flipping the lowest bit of any input is its increment or its
  decrement), as stepping stones to `add`, and one miss task for
  environments that reward them together with `add_lowbits4`.

## Measurement actions the C++ does not have (2026-11-14)

The maintainer asked for open-endedness measures from the research
literature (2026-11-13). They are observers: a run with them writes every
other output file byte for byte as a run without them, in the exact mode and
in the parallel mode (`crates/avida/tests/modes.rs` holds that for
`PrintMODESData`). The event-file gate accepts their names although the C++
action library does not know them.

- `PrintMODESData <file> <resolution> <t> [nearest|any]`: change, novelty,
  complexity and ecology of Dolson, Vostinar, Wiser and Ofria, "The MODES
  Toolbox", Artificial Life 25(1), 2019. `crates/world/src/modes.rs`; the
  module documentation lists where it departs from the paper's reference
  code (no rows before the first filter, one sample per generation crossing,
  skeletons only for persistent genotypes, test-CPU resources at the level
  of an unexploited cell). The filter defaults to `nearest`, as in the
  reference code, by the maintainer's choice of 2026-11-14 over the paper's
  wording (`any`).
- `PrintPhylogenyMetrics [file]` and `SavePhylogeny [file]`: the phylogeny
  measures of Dolson, Lalejini, Jorgensen and Ofria, "Interpreting the Tape
  of Life", Artificial Life 26(1), 2020, computed as the Empirical library's
  `Systematics.hpp` does, over the genotype tree below the most recent common
  ancestor, and that tree in the ALife standard phylogeny CSV.
  `crates/systematics/src/phylometrics.rs`, `crates/world/src/phylo_actions.rs`;
  `ARCHITECTURE.md` lists the conventions chosen where the paper and the
  reference code disagree.

## Extension beyond the C++: the modular heads set (opt-in, 2026-11-15)

`avida-core/support/config/instset-heads-modular.cfg` (`heads_modular`,
hardware type 0) adds `nop-D`, `nop-E`, `nop-F` (registers DX, EX, FX) to
the stock heads set and enables the C++'s own `call` and `return`, so that
a subroutine and scratch storage are cheap. The C++ has no such set, so
runs with it have no reference to compare with. Every other set is
unchanged: the heads CPU has `max(3, nops of the set)` registers (at most
6), label complements and the next-register rotation use that count where
the C++ uses `NUM_NOPS`, and at 3 the arithmetic is the C++'s. The new nops
sit just before `NULL` in `CPU_INST_DEFS`, so every C++ library index is
unchanged. The checks are the consistency suite, the default run (output
identical, time within noise: 44.5 s before, 43.6 to 44.1 s after) and
`crates/hardware/tests/modular.rs`. Select the set with
`-def INST_SET instset-heads-modular.cfg`; `-set INST_SET` does not select
it, because `avida.cfg` includes the set by a directive.
`modular-subroutine.org` is a hand-written replicator that does NOT, NAND
and AND through one shared `nand; return` subroutine. Two properties of the
C++'s `call` shape such organisms: it searches for its label forward only,
so a subroutine lies after its callers, and without a label it jumps to BX.

## Extension beyond the C++: the task `field_arith` and the adder ladder (opt-in, 2026-11-15)

Accepted by the maintainer on 2026-11-15, with the threshold, field width and
reward values below. `field_arith` (`crates/tasks/src/eval_field.rs`)
is a task with no C++ counterpart; a configuration that does not name it is
unchanged, and the consistency suite is byte-identical with it in the tree.
It treats a 32-bit word as a row of independent small fields and scores a
function of two inputs field by field: arguments `fn` (`xor`, `and`, `or`,
`halfadd`, `add`), `bits` (operand width, 1 to 4), `mode` (`packed`: all
result bits of every field in the newest output; `planes`: one result bit per
output, over successive outputs), `thresh` and `stride`. The quality is
max(0, 2f - 1) for the fraction f of the compared bits that are right, so a
random output scores about zero. `environment-adder-ladder.cfg` orders eight
reactions as rungs ending in a 2-bit adder; its threshold was raised from
0.25 to 0.75 before any experiment, because constant and near-trivial outputs
cleared 0.25 (measured chance levels in `ARCHITECTURE.md`). The hand-written
`ladder-*.org` task blocks show every rung is reachable without `add` or
`sub` in both the stock and the modular set. `default-heads-modular.org` is
the stock ancestor with its search label adapted to six nops.

Additions of 2026-11-18, for stage 8 of the composed-computation experiment
(`docs/guides/experiment-composed-computation.md`). None changes a
configuration that does not name it.

- `field_arith` argument `width` (default 32): only the `width / stride`
  whole fields in the low `width` bits are judged. `width=24` keeps the top
  byte of the inputs (0x0F, 0x33, 0x55, which the logic tasks need to see
  every row of the truth table) out of the arithmetic.
- `field_arith` functions `carry`, `(a & b) << 1`, and `partial`,
  `(a ^ b) ^ ((a & b) << 1)`, both in `w + 1` result bits: the two steps of a
  ripple adder before `add`.
- Environment line `INPUT_AND_MASK <mask>` (hexadecimal with `0x`, or
  decimal): the mask is ANDed onto every input `SetupInputs` hands out, in
  the world and in the test CPU. It is applied after the random draws, so a
  run with a mask draws the same random numbers as one without. A mask that
  clears bits of the top byte makes the logic tasks see incomplete truth
  tables; keep the top byte set (`0xFF......`).
  `crates/environment/src/evaluation.rs`.

## Extension beyond the C++: knockout map and reuse measures (opt-in, 2026-11-15)

Accepted by the maintainer on 2026-11-15. `PrintKnockoutMap [file]` and
`PrintReuseData [file]` (event actions with no C++ counterpart),
`Command::KnockoutMap` of the session and the Reuse views of the desktop app
and the terminal interface map single-site knockouts of a genome in the test
CPU. A knockout replaces a site by the NULL instruction of `ActivateNullInst`
in a private copy of the instruction sets (as `PrintMODESData` does) and
compares with the unmodified genome under the same conditions: the default
fixed test inputs, the detached world's RNG re-seeded before every run,
resources at the unexploited level. A site is lethal, shared (loses two or
more of the base's tasks), task (one), speed (fitness only) or neutral. The
measures and their known weaknesses (a broken early pipeline step looks like
reuse, redundancy hides sites) are in `ARCHITECTURE.md`. The actions observe
only: `crates/avida/tests/knockout.rs` shows the other output byte-identical
with and without them, in the exact mode and in the parallel mode on 1 and 4
threads. A playback answers with a one-line reason, because a recording
carries no instruction set, environment or configuration.

`PrintReuseSurvey [file] [top]` (2026-11-15, added under the maintainer's
ruling of that day on opt-in extensions) answers two weaknesses the stage 3
experiment showed. It writes the same measures for the `top` most abundant
genotypes that replicate and perform two or more tasks in the test CPU
(looked for among the 50 most abundant), where `PrintReuseData` reports only
the dominant genotype, which often performs none. And it adds subroutine
sites: genome sites executed in the base test run with two or more distinct
`call` sites as the innermost active call, with the count of those that are
not neutral under knockout. The test CPU reports this through a call probe
(`crates/hardware/src/call_probe.rs`) that only an instrumented test run
attaches; every other run tests one `Option` per instruction and its output
is unchanged (suite at no DIFF, observer test in
`crates/avida/tests/knockout.rs`). The action works on a population loaded
with `LoadPopulation`, so finished runs can be surveyed from their saved
population. `PrintReuseData` and its file format are unchanged.

## Extension beyond the C++: dropping extinct genotypes (opt-in, 2026-11-15)

`DROP_EXTINCT_GENOTYPES 1` (port-only setting, default 0) removes a genotype
when its last organism dies. The default keeps the C++ behaviour: a genotype
without organisms stays as a historic genotype while another genotype names
it as a parent. With two-parent reproduction lineages never coalesce, so
nearly every generation stays in memory (86,936 historic genotypes for 3,522
living ones after 3,200 updates of a 60x60 `divide-sex` world, several GB
over 100,000 updates); the maintainer chose this setting on 2026-11-15 to
make such runs possible. With the setting on a genotype copies its depth and
its parents' ids at birth and holds no reference to its parents. Nothing an
organism computes changes: the data files of a sexual and an asexual run are
byte-identical with and without it, in the exact and the parallel mode
(`crates/avida/tests/drop_extinct.rs`); a saved population has no historic
rows and loads only with the setting on. What needs the ancestry of extinct
genotypes stops with `unsupported:`: `PrintMODESData`,
`PrintPhylogenyMetrics` and `SavePhylogeny` when the events are read, and the
kin-distance check of the donate instructions at first use. The front ends
show no Ancestry layer in such a run.

## Extension beyond the C++: expected quality per genotype (opt-in, 2026-11-18)

Today a task's quality for an organism is the one draw of inputs it has in
hand when it outputs, so an organism that is right on most inputs is paid
for the luck of the draw. A task with `expect=1` (`field_arith` only so far;
`TaskLib::task_expected(id)` is the generic accessor, so other tasks can opt
in) is paid the genotype's average quality over several test-CPU runs with
random inputs instead. Two port-only settings
(`config/src/port_settings.rs`, like `EXECUTION_MODE`): `EXPECTED_QUALITY_DRAWS`
(default 8; the number of runs; 0 turns the feature off, and then a task with
`expect=1` stops the run at load with a message naming both) and
`EXPECTED_QUALITY_MIN` (default 0.25; a stored average below it counts as 0).

**What changes.** Only task entries with `expect=1`, and only when the
organism's own draw gives a quality above 0 (a draw of 0 stays 0 and the
reaction does not fire, as before). The value the environment gets from
`Environment::test_output` is then the genotype's stored average for that
entry, which may be 0, so the reaction then does not fire. Everything
downstream of the quality (the task mark, `cur_task_quality`, resource
consumption, the `pow` bonus) sees that one value. The hook is
`TaskEvaluator::expected_quality`, called right after the task's own test;
every evaluator but a population organism's keeps the quality (the test
CPU's evaluators never substitute, which is what keeps the trial runs from
recursing).

**The table.** Made lazily, the first time a living organism of the genotype
has a positive own quality on an expected entry: `EXPECTED_QUALITY_DRAWS`
runs of the genome on one test CPU with random inputs
(`use_random_inputs(true)`; `INPUT_AND_MASK` and specific inputs apply in
`Environment::setup_inputs` as for any test). Per run and expected entry the
quality is `last_task_quality / last_task_count` (the mean over the times the
task was performed in that run; a run that does not perform it, or whose
organism never divides, counts 0); the table is the mean over the runs, with
averages below `EXPECTED_QUALITY_MIN` stored as 0. It is kept per genotype
(group id) in `WorldHost::expected`, dropped with the genotype
(`prune_expected`, the way `prune_phenplast` does it: a sweep once the cache
has doubled, at least 512 tables, keeping what is in the genotype arbiter;
with `DROP_EXTINCT_GENOTYPES 1` that is the living genotypes only;
`World::prune_expected_tables` sweeps at once). A genotype whose table was
dropped and is asked for again gets a new table, from the same private
stream, so the same values.

**Random numbers and side effects.** The trial runs draw from a private
stream (`avida_hardware::parallel::stream` of the run's seed and an FNV-1a
hash of the genome's text, not of the genotype id), never from the world's
RNG: a run with the feature on and no expected task is byte-identical to
today, and with expected tasks the world's stream is disturbed only through
the changed pay. The same genome has the same table in a run, and a table
depends on the seed. The runs report nothing to the world's `Stats` (the
phenotype's task counters go to a stand-in that drops them), leave no host
operation behind (`WorldHost::pending` is cut back; the kaboom snapshot is
restored as after a plasticity test), and a kill asked for by a trial
(`MAX_PREY_BT`) stops the run instead of being dropped. The set-up is the
plasticity test's (`WorldHost::run_in_test_world`, shared with
`test_plasticity`): the test organisms see the host as the world in cell -1.

**Parallel mode.** `EXECUTION_MODE 1` with an `expect=1` task stops at
start-up with `unsupported: EXECUTION_MODE 1 with a task that has expect=1`.
The table is made at an output, which needs the world, and the bursts cannot
reach it; the substitution is not in the bursts' shared output test. Making
the tables at `activate_organism` would cost the runs for every new
genotype whether or not it ever outputs, and was not done.

**What it leaves alone.** Environments without `expect=1` (every stock and
reference configuration), the consistency suite, and the test CPU's own
output tests (a `PrintDominantGenotype` test, `RECALCULATE` and the other
analyze commands see the task's own quality).

**Cost.** One table is `EXPECTED_QUALITY_DRAWS` test-CPU runs of the genome, about
2 ms for 8 runs in a release build (the extra 153 s below over 67,271
tables; the two runs also differ in what the population does). On `19-above-xor/arms/pay+masked` (60x60, 3000 updates, seed 41, the
HALFADD, ADD2_PLANES and ADD2 reactions with `:thresh=0.5:expect=1` in place
of `:thresh=1`): 54.2 s without and 207.2 s with, 67,271 tables made, 9,755
kept at the end. A world in which almost every offspring is a new genotype
pays that for every one that does the task at all.
`AVIDA_EXECUTION_STATS=1` prints the count at the end of the run. Tests:
`crates/avida/tests/expected_quality.rs` (no change without an expected task,
exact solver, a partial solver against an independent mean, the minimum, the
load errors, the input mask, extinction) and
`expect_marks_the_entry_and_nothing_else` in `crates/tasks/tests/field_arith.rs`.

## Views the C++ does not have: Ladder and arm comparison (2026-11-15)

Observation only; no output of the program changes. The Ladder view (Charts
tab, `J` in the terminal) shows per task of the environment how many
organisms performed it over time; a rung counts as held at 1 % of the
organisms. `avida-gui --compare DIR` and `avida-tui --compare DIR` read the
data files of a batch (`DIR/<arm>/seed-<N>/`) and run no simulation. Their
statistics are descriptive and always shown with n: Mann-Whitney U two-sided
(exact permutation of the midranks up to 40 pooled values, otherwise the
normal approximation with tie and continuity corrections) and Fisher's exact
test two-sided.

## A test CPU's `MAX_PREY_BT` kill is carried out after the test (2026-11-13)

`Inst_SetRandPFTOnce` in a test CPU can kill a prey organism of the
population (`MAX_PREY_BT`). The C++ kills in the middle of the test; the
port queues the kill (`HostOp::KillCell`) and `World::flush_host` carries it
out when the test ends, in every test whose end the world answers at once
(`WorldHost::kill_ok`): the divide-time tests, the fitness tests,
single-test events (`PrintDominantGenotype`, the mini-trace genome files)
and, since 2026-11-15, the plasticity test inside an output test (flushed
when the step ends). Until then the test world's live list and prey and
predator counts follow each kill (swap-remove as `RemoveLiveOrg`), so later
runs of the same test see the C++'s population. The output is the same
because nothing between the two points reads the population. Stops: a kill
that draws random numbers; a group-size or kaboom-flag read after a pending
kill; a kill from a loop of tests or from the precalculated phenotype. A
pick of the dividing parent stops because the reference binary segfaults
there (exit by SIGSEGV on three of three seeds at the update where the port
stops). Pinned by `gap_exp_max_prey_bt_event_test`, `_divide_kills` and
`_plasticity` under `crates/avida/tests/actions`. The maintainer accepted
the ordering on 2026-11-13; its extension to event-time and plasticity
tests (2026-11-15) rests on the same argument and on byte-identical output
in the pinned cases and six further seeds.

## `Task_Optimize` follows the reference binary's helper (2026-11-12)

At `cpp-final`, `cTaskLib::Task_Optimize` checks that the output buffer is
full and hands the outputs to the Rust helper `avd_task_eval_optimize`; the
port follows that helper, because the reference binary runs it, wherever it
differs from the pre-Rust C++ (`crates/tasks/src/eval_misc.rs`,
`eval_optimize`): an unknown function gives quality .001 and `Fx` 0 at once;
proportional quality gives .001 when `maxFx - Fx + .001` or
`maxFx - minFx + .001` is not positive (the C++ asserted); binary decoding
whose powers of `basepow` sum to 0 leaves the variables 0 (the C++ divided by
zero); nothing past the stored outputs is read. The `quality > 1!` line goes
to standard output as in the C++ (through `TaskWorld::task_stdout`, which a
host must forward to keep the C++ order; `OrgTaskWorld` and `WorldHost` do,
the test CPU's `TestHost` does not). Arguments on which the reference binary
crashes, aborts or does not finish (`function=20`; too few or negative
`numvars`; a negative `varlength` with the binary decoding, function 19 or
function 21) stop the environment at load with `unsupported:`
(`optimize_unsupported`). Each case was run on the reference binary.
The maintainer confirmed on 2026-11-12 that the reference binary, not the
pre-Rust C++ source, is the behaviour to keep.
