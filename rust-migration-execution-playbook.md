# Migration Execution Playbook

This playbook defines how the full migration is executed, measured, and governed.

Related:

- `rust-migration-full-plan.md`
- `rust-migration-workstream-catalog.md`
- `rust-migration-remaining-cpp-static-analysis.md`

## Standard Slice Template

Every slice should include:

1. Additive FFI seam in `running_stats_ffi.h`
2. Rust helper implementation in `rust/avida-rust/src/*`
3. C++ call-site routing to new helper
4. Rust parity tests
5. C++ parity tests
6. Full gates + docs refresh

## Mandatory Gates Per Slice

- `cargo +stable fmt --check`
- `cargo +stable test --all-targets`
- `cargo +stable clippy --all-targets -- -D warnings`
- `python3 scripts/ci_ffi_analysis.py --output target/ffi-analysis`
- `python3 scripts/ci_abi_guard.py --baseline abi/avd_symbols_baseline.txt --current target/ffi-analysis/avd_symbols.txt`
- `./build_avida -DAVD_UNIT_TESTS:BOOL=ON -DAPTO_UNIT_TESTS:BOOL=OFF`
- `./cbuild/work/unit-tests`
- `./run_tests --mode=slave`

## Program Metrics and Reporting

Track weekly:

- Remaining C++ code LOC
- FFI-touched C++ file count
- Hotspot reduction progress (top-10 file list drift)
- CI pass/fail trends for:
  - `tests`
  - `tests-rust`
  - `coverage-rust`
  - `perf-rust`
  - `ffi-analysis-rust`

## Performance Characterization Step (planning-grade)

When performance-first reprioritization is requested, run a dedicated characterization pass before the next extraction slice:

1. Identify top call-frequency hotspots from available profiling/tracing evidence.
2. Produce a short code-path analysis for the top functions (hot loops, branching shape, allocation and math intensity, coupling risk).
3. Define benchmark harness scope for the shortlisted hotspots (input shapes, warmup/iterations, deterministic fixtures, pass/fail thresholds, artifact locations).
4. Record one explicit next executable slice tied to the highest-value hotspot or benchmark-enabling seam.

Required output artifacts for this step:

- Ranked hotspot table (function, file, evidence signal, expected impact).
- Benchmark plan (target function(s), harness location, baseline metric to capture).
- Updated roadmap/workstream/waves/backlog entries with the same next candidate.

Latest completed artifact:

- `documentation/performance-hotspot-baseline-2026-03-16.md` (includes ranked hotspots, focused analysis, and benchmark harness update evidence).
- `cAnalyze` filename-token html classification follow-on (`avd_analyze_is_html_filename_token`) completed with full mandatory gates and roadmap/backlog refresh.
- `cAnalyze` output file-type resolution short-circuit follow-on (`avd_analyze_output_file_type_short_circuit_kind`) completed with full mandatory gates and roadmap/backlog refresh.
- `cAnalyze` output sink-selection short-circuit follow-on (`avd_analyze_output_sink_short_circuit_kind`) completed with full mandatory gates and roadmap/backlog refresh.
- `cAnalyze` output file-handle mode short-circuit follow-on (`avd_analyze_output_file_handle_mode_short_circuit_kind`) completed with full mandatory gates and roadmap/backlog refresh.
- `cAnalyze` output token-presence short-circuit follow-on (`avd_analyze_output_token_presence_short_circuit_kind`) completed with full mandatory gates and roadmap/backlog refresh.
- `cPopulation` deme counter-update short-circuit follow-on (`avd_cpop_should_update_deme_counters`) completed with full mandatory gates and roadmap/backlog refresh.
- `cPopulation` multi-deme block short-circuit follow-on (`avd_cpop_should_run_multi_deme_block`) completed with full mandatory gates and roadmap/backlog refresh.
- `cPopulation` speculative-step multi-deme routing alignment follow-on (shared `avd_cpop_should_run_multi_deme_block` policy) completed with full mandatory gates and roadmap/backlog refresh.
- `PopulationActions` deme-injection seed/loop dispatch pilot seam (`avd_popaction_deme_loop_start_index`, `avd_popaction_seed_deme_action`) completed with full mandatory gates and roadmap/backlog refresh.
- `PrintActions` instruction filename-mode selection pilot seam (`avd_printaction_instruction_filename_mode`) completed with full mandatory gates and roadmap/backlog refresh.
- `cPopulation` deme-routing pilot seam (`avd_cpop_should_check_implicit_deme_repro`, `avd_cpop_should_run_speculative_deme_block`) completed with full mandatory gates and roadmap/backlog refresh.
- `PrintActions` instruction filename-mode follow-on expansion (remaining instruction-data action constructors) completed with full mandatory gates and roadmap/backlog refresh.
- `PopulationActions` cell-range normalization follow-on expansion (`avd_popaction_normalize_cell_end`) completed with full mandatory gates and roadmap/backlog refresh.
- **2026-03-17 comprehensive planning refresh**: FFI footprint recharacterized (71 unique exports, 179 C++ call-sites). Identified saturation in action/analyze workstreams and pivoted priority to untouched `cEnvironment.cc` and `cStats.cc` targets. Next executable slice: `cEnvironment.cc` reaction-process-type dispatch classifiers.

## Risk Controls

- Keep C++ ownership unless a slice explicitly targets ownership migration.
- Avoid mixed ownership boundaries across FFI in same slice.
- Introduce no behavior-changing refactors without parity tests first.
- Avoid multi-hotspot bundled slices; keep blast radius small.

## Deferred Tech-Debt Cadence

Non-blocking debt is intentionally deferred to preserve slice throughput. Track and execute a focused debt tranche on this cadence:

- Trigger debt tranche after every 2-3 functional slices.
- Scope tranche to: magic-number cleanup, helper-surface consolidation, backlog pruning, and hotspot-depth prep.
- If any deferred item becomes correctness-, ABI-, or gate-blocking, promote it immediately into the active next candidate.

## Definition of Done (per wave)

- Planned wave slice list completed
- All wave touchpoints have parity tests
- No open critical regressions in CI lanes
- Roadmap/backlog updated with next candidate
- Any in-flight slice with local code changes is tracked as "in progress" until mandatory gates pass and the change set is ready to commit.

## Retirement Policy for Legacy C++

Only remove C++ implementation when:

1. Rust seam has shipped and stabilized through CI for multiple slices
2. Functional parity is explicitly covered in Rust + C++ tests
3. No remaining call sites rely on old implementation
4. Removal diff is isolated and reviewable

## Operational Cadence

- Start each slice from current top candidate in roadmap/backlog.
- Run full gates before commit.
- Commit/push per slice for traceability.
- Recompute static baseline every major milestone and refresh planning docs.

## Native port update (2026-11-12)

The slice template, the mandatory gates and the retirement policy above govern the FFI track (`rust/avida-rust` plus C++ call sites). Changes confined to the native workspace `rust/avida-native` use the gates below instead. Nothing here changes the FFI gates.

### Gates for the native workspace

Run in `rust/avida-native`:

- `cargo fmt --check`
- `cargo clippy --workspace --all-targets -- -D warnings`
- `cargo test --workspace`
- The consistency suite: run the native `avida` binary and the reference binary `cbuild/work/avida` over `avida-core/tests/*` and compare output byte for byte, ignoring timestamps, the time-based seed and the data-directory path. Acceptable outcomes are same, or a trivial difference of that kind. Any DIFF blocks the change. Last measured 2026-11-12: 121 tests, 85 same, 34 trivial differences, 0 DIFF, 1 unsupported (the reference crashes on `SLICING_METHOD 4`), 1 reference timeout.
- When a change touches an area covered by them, also the pinned list `crates/avida/tests/consistency_passing.txt` (`crates/avida/tests/consistency.rs`), the action fixtures under `crates/avida/tests/actions/` (`crates/avida/tests/actions.rs`), the stepper test (`crates/avida/tests/stepper.rs`), and the instruction-trace comparisons in `crates/hardware/tests`.

Per-crate gates while working on one crate (from `PORTING.md`): `cargo test -p <crate>`, `cargo clippy -p <crate> --all-targets -- -D warnings`, `cargo fmt -p <crate>`.

These gates are not yet in CI. Until the CI lane exists (open item in `documentation/Tech-Debt-Backlog.md`), they are run by hand and their results quoted with a date, as the suite figures above are.

### Rules specific to the native track

- Behaviour parity with the C++ is the specification: same RNG call order, integer and float arithmetic, iteration order and text formatting. Quirks that affect output are kept and marked `// QUIRK:`.
- No `unsafe` and no FFI (`unsafe_code = "forbid"` in the workspace). Each ported type and non-trivial function cites the C++ file and symbol.
- A path that is not ported, or that the C++ reaches only through undefined behaviour, must stop with `Err("unsupported: ... (C++: file:symbol)")`, never run with silently different output.
- Do not edit `rust/avida-rust` or the C++ from a native-track change, and do not edit the native workspace from an FFI slice.

### Definition of done and retirement for the native track

- A native change is done when the four gates above pass and the affected documents (`rust/avida-native/ARCHITECTURE.md`, `rust/avida-native/docs/ontology.md`, `rust-migration-waves.md`) state the same figures.
- Retirement of `rust/avida-rust` and the C++ is an open maintainer decision. The "Retirement Policy for Legacy C++" above would need a native-port reading (for example, zero DIFF over a stated period, remaining stops classified, front ends reviewed by a human). It is not met by any claim in this update.
