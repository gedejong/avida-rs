# How parity is tested

Parity with the C++ program is the specification of the exact mode. Three mechanisms keep it.

## 1. The consistency suite

`avida-core/tests` holds 122 test directories taken from the original program, each with a `config` folder and, for most, an `expected` folder with the reference output. 121 have a `test_list` and are run (the one without, `string_producing_and_donating`, is skipped).

Two tools use them:

- `cargo test` includes `crates/avida/tests/consistency.rs`, which runs the port on the tests that have checked-in expected output and compares every file. `crates/avida/tests/consistency_passing.txt` lists the pinned names.
- `tools/run_suite.py` runs every test with the C++ reference binary (the output is cached) and with the Rust binary, and compares standard output and every file written, ignoring timestamps, the time-based seed and the data-directory path.

The last recorded result: 85 byte-identical (84 that run to a zero exit in C++, plus `analyze_resize_max_batches`, whose exit code 1 the port reproduces), 34 more that fail the same way the C++ does, none different, 1 stop on a named unported feature (`SLICING_METHOD 4`, on which the C++ crashes), and 1 where the reference binary times out (`_mutn_rate`).

## 2. Action fixtures and traces

For individual areas, the repository pins behaviour against the reference binary with targeted cases: 242 action fixture directories under `crates/avida/tests/actions` (each a small configuration whose output was compared with the reference), instruction traces of the heads CPU (480 identical), and thousands of random and hand-written test-CPU traces for the other hardware types. `crates/avida/tests/stepper.rs` pins that pausing a run at arbitrary slice boundaries does not change it, which the viewers rely on.

## 3. The statistical check for the parallel mode

The parallel mode cannot match byte for byte, so it is checked statistically with `tools/parallel_stats.py`:

```sh
python3 tools/parallel_stats.py run CONFIG_DIR OUT_DIR --seeds 101-116 --updates 5000
python3 tools/parallel_stats.py compare OUT_DIR
```

`run` makes one copy of the configuration per mode and seed, runs both modes, and records a handful of outcomes per run (fitness, genome length, tasks held, the first update with NOT and NAND, generation, gestation time, organisms, instructions per update). `compare` prints the median of each outcome per mode and a two-sided Mann-Whitney U test. A change to the parallel mode's scheduling needs a fresh run, reported with the change. `crates/avida/tests/parallel.rs` separately pins that the parallel mode's output does not depend on the number of threads.

## The reference binary

The C++ was removed from the working tree. Build the reference binary from the tag `cpp-final`: see [validation gates](validation.md). Three tests compare tables with the C++ source text, and the files they read are kept under each crate's `testdata/reference`.

## Floating point

The reference binary (Apple clang, arm64, default `-ffp-contract=on`) fuses `a*b + c` into a single multiply-add when written as one C++ expression. The port uses `f64::mul_add` at exactly those sites, and nowhere else; otherwise results drift by one unit in the last place and data files stop matching. `%g`-style output goes through `avida_apto::fmt`, which reproduces macOS libc including its trailing-zero quirk. The details are in `PORTING.md`.
