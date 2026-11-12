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

`%g`-style output must go through `avida_apto::fmt`, which reproduces macOS
libc including its trailing-zero quirk.

- A new crate must be created with `Cargo.toml` and `src/lib.rs` in the same step: workspace members are a glob, so a crate directory without a library target breaks `cargo` for every other agent.
