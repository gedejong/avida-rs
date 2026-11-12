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

ARCHITECTURE.md ("Parallel execution mode") describes what the mode
guarantees and what it does not.

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
