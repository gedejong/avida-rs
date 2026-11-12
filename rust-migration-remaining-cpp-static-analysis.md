# Remaining C++ Static Analysis Baseline

> **Closed 2026-11-12.** The maintainer decided that the native workspace `rust/avida-native` supersedes this FFI migration. The C++ core and `rust/avida-rust` were removed from the tree; the last commit that contains them is the git tag `cpp-final`. This document is kept as history. Current state and open work: `rust/avida-native/ARCHITECTURE.md` and `CLAUDE.md`.

This document captures a quantitative baseline of the remaining C++ code to migrate.

## Tooling Used

- **Availability check**: `cloc`, `tokei`, `lizard`, `cppcheck`, `clang-tidy`, `bear` were not installed in the environment.
- **Static analysis approach used**:
  - Repository-wide lexical scan with Python over `avida-core`
  - Pattern-based metrics using include/function/branch tokens
  - Existing code search via `rg` tool integration

## Scope

- Scanned extensions: `.cc`, `.cpp`, `.cxx`, `.h`, `.hpp`, `.hh`
- Primary root: `avida-core`
- "Remaining C++" means files not currently routed through Rust FFI seams (no `rust/running_stats_ffi.h` include and no `avd_*` call path detected).

## High-Level Numbers

- Total C/C++ files in scope: **445**
- Total LOC in scope: **173,750**
- Approx. code LOC in scope: **140,333**
- Implementation files (`.cc/.cpp/.cxx`): **184**
- Implementation LOC: **132,901**
- Approx. implementation code LOC: **109,388**
- FFI-touched files: **22**
- Remaining files to migrate (non-FFI touched): **423**
- Remaining approximate code LOC: **116,487**

## Subsystem Footprint (remaining-first planning signal)

Top subsystems by remaining code/complexity mass:

| Subsystem | Files | Approx. code LOC | Branch token count | Avg priority score |
|---|---:|---:|---:|---:|
| `main` | 43 | 32,383 | 6,051 | 9.32 |
| `cpu` | 16 | 25,216 | 4,887 | 17.62 |
| `actions` | 7 | 13,030 | 2,004 | 21.48 |
| `analyze` | 10 | 12,625 | 2,181 | 14.25 |
| `targets` | 26 | 5,631 | 1,043 | 4.39 |
| `script` | 12 | 5,167 | 1,275 | 5.50 |
| `viewer` | 8 | 4,791 | 270 | 6.19 |

## Top Hotspot Files (remaining C++)

Weighted priority score combines:
- 55% normalized code LOC
- 30% normalized branch-token count
- 15% normalized include count

| Score | Code LOC | Branches | Includes | File |
|---:|---:|---:|---:|---|
| 100.00 | 9,413 | 1,751 | 56 | `source/analyze/cAnalyze.cc` |
| 89.67 | 8,037 | 1,711 | 50 | `source/main/cPopulation.cc` |
| 84.17 | 8,759 | 1,488 | 28 | `source/cpu/cHardwareCPU.cc` |
| 66.16 | 6,270 | 1,442 | 18 | `source/cpu/cHardwareExperimental.cc` |
| 56.16 | 5,166 | 969 | 35 | `source/actions/PopulationActions.cc` |
| 51.07 | 4,898 | 654 | 42 | `source/actions/PrintActions.cc` |
| 41.23 | 3,548 | 915 | 18 | `source/main/cTaskLib.cc` |
| 41.06 | 4,284 | 482 | 29 | `source/main/cStats.cc` |
| 35.78 | 3,309 | 694 | 17 | `source/cpu/cHardwareBCR.cc` |
| 29.97 | 2,691 | 566 | 17 | `source/cpu/cHardwareGP8.cc` |

## Coupling Signals (include graph proxies)

Most included local headers (in-degree hotspots):

- `source/main/cWorld.h` (71)
- `source/tools/cString.h` (70)
- `source/main/cOrganism.h` (55)
- `source/main/cPopulation.h` (44)
- `source/main/cStats.h` (42)
- `include/public/avida/core/Types.h` (41)
- `source/main/cEnvironment.h` (41)

Most include-heavy implementation files (out-degree hotspots):

- `source/analyze/cAnalyze.cc` (45 local includes)
- `source/actions/PrintActions.cc` (37)
- `source/main/cPopulation.cc` (36)
- `source/cpu/cHardwareCPU.cc` (25)
- `source/actions/PopulationActions.cc` (25)
- `source/main/cEnvironment.cc` (25)

## Already Rust-Touched C++ Footprint

Current FFI integration spans **71 unique `avd_*` exported functions** across **7 Rust helper modules** with **179 C++ call-sites** in **7 actively-routed implementation files**.

| C++ file | `avd_*` calls | LOC | Migration saturation |
|---|---:|---:|---|
| `source/actions/PopulationActions.cc` | 45 | 6,448 | High — policy/validation extraction near-complete |
| `source/main/cResourceCount.cc` | 42 | — | High — setter/getter/dispatch fully extracted |
| `source/main/cTaskLib.cc` | 39 | 4,255 | High — scoring/classification chains extracted |
| `source/analyze/cAnalyze.cc` | 26 | 11,459 | Medium — token/output policy extracted; large orchestration core remains |
| `source/actions/PrintActions.cc` | 18 | 5,868 | Medium — instruction output routing extracted |
| `source/main/cPopulation.cc` | 7 | 9,475 | Low — deme routing only; large decision surface untouched |
| `source/cpu/cHardwareCPU.cc` | 2 | 11,043 | Low — dispatch pilot only; massive execution core untouched |

Additionally, prior-wave FFI integration continues in:

- `source/main/cSpatialResCount.cc`
- `source/main/cResourceHistory.cc`
- `source/main/cEventList.cc`
- `source/data/{Provider,Manager,Package,TimeSeriesRecorder}.cc`
- selected `source/tools` modules and unit-test parity harnesses

**Remaining untouched files** (0 `avd_*` calls, >500 LOC):

| File | LOC | Seam readiness |
|---|---:|---|
| `source/main/cPopulationInterface.cc` | 2,424 | Medium — organism interface dispatch |
| `source/main/cOrganism.cc` | 1,710 | Medium — lifecycle policy |
| `source/main/cDeme.cc` | 1,707 | Low — heavy state management |
| `source/main/cLandscape.cc` | 1,003 | Low — fitness landscape math |
| `source/main/cBirthChamber.cc` | 602 | Low — birth selection |

## How to Use This Baseline

- Treat this as the planning baseline for the comprehensive roadmap in `rust-migration-full-plan.md`.
- Refresh this baseline after each major wave (or every 2-3 slices) to track:
  - remaining code LOC
  - top hotspot drift
  - FFI-touched file growth
  - coupling hotspot reduction

## Refresh Log

- **2026-03-16 refresh**: Repository-wide scan repeated; high-level planning metrics remain unchanged from this baseline (`445` files, `173,750` LOC, `184` implementation files with `132,901` LOC, `22` FFI-touched files, `423` remaining non-FFI-touched files). No reprioritization trigger from code-mass drift; ordering changes should be driven by seam-readiness and validation status instead.
- **2026-03-16 planning addendum**: Next planning action is a performance-first characterization pass (call-frequency hotspot ranking + benchmark harness planning) to complement static code-mass ranking before selecting the next executable extraction slice.
- **2026-03-16 performance addendum**: Performance-first characterization pass completed (`documentation/performance-hotspot-baseline-2026-03-16.md`); runtime call-frequency model identifies `cHardwareCPU::SingleProcess`/`SingleProcess_ExecuteInst` and `cPopulation::ProcessStep` orchestration path as top execution hotspots, with `cResourceCount` update/dispatch path benchmarked as immediate baseline support.
- **2026-03-16 CPU pilot addendum**: First `cHardwareCPU` dispatch-classification seam completed (`avd_cpu_dispatch_family`, `avd_cpu_dispatch_counted_opcode`) with full gate validation, reducing pre-dispatch policy logic in `SingleProcess_ExecuteInst` to an additive Rust helper boundary while preserving C++ execution ownership/order.
- **2026-03-16 analyze addendum**: `cAnalyze` filename-token html classification follow-on completed (`avd_analyze_is_html_filename_token`) with full gate validation, clearing remaining direct `filename == "html"` policy checks in analyzed output-selection call-sites while preserving C++ ownership/flow.
- **2026-03-16 action addendum**: `PopulationActions` deme-injection seed/loop dispatch pilot seam completed (`avd_popaction_deme_loop_start_index`, `avd_popaction_seed_deme_action`) with full gate validation, centralizing deterministic per-deme injection dispatch policy while preserving C++ orchestration order and ownership.
- **2026-03-16 print-action addendum**: `PrintActions` instruction filename-mode selection pilot seam completed (`avd_printaction_instruction_filename_mode`) with full gate validation, centralizing deterministic default/override filename mode policy in instruction data print actions while preserving C++ output ownership and ordering.
- **2026-03-16 population addendum**: `cPopulation` deme-routing pilot seam completed (`avd_cpop_should_check_implicit_deme_repro`, `avd_cpop_should_run_speculative_deme_block`) with full gate validation, centralizing deterministic deme-block gating policy in `ProcessStep` and `ProcessStepSpeculative` while preserving C++ update/repro sequencing and ownership.
- **2026-03-16 print-action follow-on addendum**: `PrintActions` instruction filename-mode selection expansion completed by routing remaining instruction-data action constructors through `avd_printaction_instruction_filename_mode`, with full gate validation and preserved C++ output ordering/ownership semantics.
- **2026-03-16 population-action follow-on addendum**: `PopulationActions` cell-range normalization expansion completed (`avd_popaction_normalize_cell_end`) by routing repeated constructor range normalization policy across injection/parasite actions, with full gate validation and preserved C++ ownership/flow semantics.
- **2026-03-17 debt-tranche refresh**: ABI symbol baseline refreshed to 268 symbols (was 159). FFI coverage now spans 18 actively-routed C++ implementation files with ~260 call-sites. All prior "untouched" targets (`cStats`, `cEnvironment`, `cHardwareExperimental`, `cHardwareBCR`, `cHardwareGP8`) now have Rust seams. Newly touched files: `cResource.cc`, `cGradientCount.cc`, `cOrgSensor.cc`, `cHardwareTransSMT.cc`. Remaining untouched high-LOC files updated.
- **2026-03-17 comprehensive refresh**: FFI footprint recharacterized — 71 unique `avd_*` exports, 179 C++ call-sites across 7 actively-routed files. `PopulationActions` (45 calls), `cResourceCount` (42), and `cTaskLib` (39) approaching policy-extraction saturation. Identified `cStats.cc` and `cEnvironment.cc` as fresh high-value untouched targets (0 `avd_*` calls each). Reprioritization triggered: pivot from near-saturated action/analyze follow-ons to untouched `cEnvironment`/`cStats` targets and deeper `cHardwareCPU`/`cPopulation` extraction.

## Native port addendum (2026-11-12)

The baseline above counts the remaining C++ that has not been routed through Rust FFI seams. A complete native port in `rust/avida-native` makes a second measurement relevant: how much of the C++ has a native Rust counterpart, regardless of FFI routing. The figures below were re-measured on 2026-11-12 with `find ... | xargs cat | wc -l`; the baseline figures above were not re-measured and are left as recorded on 2026-03-16 and 2026-03-17.

| Item | Files | Lines |
|---|---:|---:|
| C++ in `avida-core/source` (`.cc`, `.cpp`, `.h`), 2026-11-12 | 387 | 167,492 |
| of which `.cc`/`.cpp` | not counted | 132,569 |
| FFI crate `rust/avida-rust/src` | 45 | 28,179 |
| Native workspace `rust/avida-native/crates/*/src` | 306 | 189,119 |
| Native workspace `rust/avida-native/crates/*/tests` | 42 | 12,990 |

The C++ line count here covers `avida-core/source` only, and the baseline's 173,750 lines covered all of `avida-core` including `include/`, so the two are not comparable and no trend is claimed.

- **Planning consequence.** The "Remaining C++" definition (files not touched by FFI) no longer predicts the work left, because the native port replaces files whether or not they have FFI seams. The class-by-class native status is in `rust/avida-native/docs/ontology.md` (generated from a 2026-11-10 snapshot; it predates the `session`, `viewmodel`, `gui` and `tui` crates and its counts should be regenerated).
- **Hotspots.** The hotspot files listed above (`cAnalyze.cc`, `cPopulation.cc`, `cHardwareCPU.cc`, `cHardwareExperimental.cc`, `PopulationActions.cc`, `PrintActions.cc`, `cTaskLib.cc`, `cStats.cc`, `cHardwareBCR.cc`, `cHardwareGP8.cc`) each have a native counterpart in `crates/analyze`, `crates/world`, `crates/hardware`, `crates/tasks`. What remains unported is tracked as `unsupported:` stops (175 source lines in total: `world` 109, `hardware` 59, `gui` 4, `analyze` 3; most guard undefined behaviour in the C++) and not as remaining LOC.
- **Refresh rule.** If the native port is accepted, replace the metrics of this document by: suite DIFF count, number of stops that are gaps, and C++ lines still needed as the reference oracle. If the FFI track continues instead, keep refreshing the baseline as before.
- **Not verified here.** No static analysis tool was run for this addendum; line counts are raw `wc -l`, not code-only counts.
