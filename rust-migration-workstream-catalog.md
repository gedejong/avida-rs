# Migration Workstream Catalog (Remaining C++)

> **Closed 2026-11-12.** The maintainer decided that the native workspace `rust/avida-native` supersedes this FFI migration. The C++ core and `rust/avida-rust` were removed from the tree; the last commit that contains them is the git tag `cpp-final`. This document is kept as history. Current state and open work: `rust/avida-native/ARCHITECTURE.md` and `CLAUDE.md`.

This catalog breaks the full migration into concrete workstreams and slice queues.

Related documents:

- `rust-migration-remaining-cpp-static-analysis.md`
- `rust-migration-full-plan.md`

## Workstream Prioritization

Priority order is based on:

1. Remaining code and branch mass
2. Coupling impact
3. Deterministic seam availability
4. Existing test/fixture leverage

## Workstream A: Resource Runtime Completion (`source/main`) — COMPLETE

Objective: complete `cResourceCount`/`cSpatialResCount` policy extraction.

Status: All deterministic helper seams extracted (42 C++ call-sites, 35+ FFI exports). Resource runtime C++ is orchestration-only. Parity coverage locked. Archived.

## Workstream B: CPU Execution Core (`source/cpu`)

Objective: migrate high-complexity execution logic safely.

Slice queue:

1. instruction family dispatch classifiers
2. stack/register deterministic helpers
3. control-flow and mutation selection policies
4. hardware-variant shared-kernel extraction

Primary files:

- `source/cpu/cHardwareCPU.cc`
- `source/cpu/cHardwareExperimental.cc`
- `source/cpu/cHardwareBCR.cc`
- `source/cpu/cHardwareGP8.cc`

## Workstream C: Population + Environment Core (`source/main`) — ACTIVE PRIORITY

Objective: migrate high-mass runtime state transition logic. This workstream now holds the highest-value fresh targets.

Slice queue:

1. **`cEnvironment.cc` reaction/process type dispatch classifiers** — NEXT CANDIDATE, 0 avd_ calls, string→enum patterns (process type, entry type, bonus type), very seam-ready
2. **`cStats.cc` task-count filtering and resource-gradient classification** — 0 avd_ calls, pure aggregation/filtering patterns, high value
3. `cPopulation` opinion/group assignment and forager classification helpers — 7 avd_ calls, large remaining decision surface
4. `cTaskLib` remaining name-dispatch and span/fibonacci scoring — 39 avd_ calls, diminishing returns
5. `cEnvironment` deeper reaction/requisite configuration parsing
6. `cStats` spatial resource data formatting and germline classification

Pending cross-file reuse:

- **`avd_cpop_is_divide_method_split`** and **`avd_cpop_is_generation_inc_both`**: wire into `cHardwareCPU.cc` (~8 sites) and `cPopulation.cc` where `DIVIDE_METHOD_SPLIT` checks appear.
- **`avd_deme_base_merit`**: may apply in `cPhenotype.cc` `CalcSizeMerit` BASE_MERIT_CONST case.

Primary files:

- `source/main/cEnvironment.cc` (2,095 LOC, 6 avd_ calls)
- `source/main/cStats.cc` (5,272 LOC, 9 avd_ calls)
- `source/main/cPopulation.cc` (9,475 LOC, 21 avd_ calls)
- `source/main/cTaskLib.cc` (4,255 LOC, 39 avd_ calls)
- `source/main/cPhenotype.cc` (2,447 LOC, 8 avd_ calls)
- `source/main/cDeme.cc` (1,707 LOC, 9 avd_ calls)
- `source/main/cOrganism.cc` (1,710 LOC, 7 avd_ calls)
- `source/main/cGradientCount.cc` (1,140 LOC, 7 avd_ calls)
- `source/main/cOrgSensor.cc` (1,443 LOC, 4 avd_ calls)
- `source/main/cResource.cc` (3 avd_ calls)

## Workstream D: Analyze and Action Pipeline (`source/analyze`, `source/actions`) — NEAR SATURATION

Objective: move heavy decision/aggregation logic while keeping I/O glue stable.

Current status: Policy/validation extraction is near-complete for deterministic seams.
- `PopulationActions.cc`: 45 avd_ calls across 16 unique helpers — validation/dispatch/guard patterns largely extracted
- `cAnalyze.cc`: 26 avd_ calls across 8 unique helpers — token/output short-circuit patterns extracted
- `PrintActions.cc`: 18 avd_ calls across 2 unique helpers — instruction output routing extracted

Remaining slice queue (maintenance-mode):

1. Residual `cAnalyze` file-type header generation and batch genotype iteration helpers (if seam-ready patterns emerge)
2. Residual `PopulationActions` genome-loading error classification (requires error context — lower priority)
3. fixture expansion for output parity across platforms

Primary files:

- `source/analyze/cAnalyze.cc` (11,459 LOC, 26 avd_ calls)
- `source/actions/PopulationActions.cc` (6,448 LOC, 45 avd_ calls)
- `source/actions/PrintActions.cc` (5,868 LOC, 18 avd_ calls)

## Workstream E: Script/Viewer/Systematics Tail

Objective: complete remaining computation-heavy tails after core runtime stabilization.

Slice queue:

1. script AST/semantic deterministic walkers
2. viewer data projection/aggregation helpers
3. systematics deterministic grouping/metrics helpers

## Cross-Cutting Workstream F: ABI + Quality Infrastructure

Objective: keep migration safe at scale.

Ongoing tasks:

- Keep additive ABI-only growth policy
- Maintain symbol baseline and CI FFI analysis
- Expand microbenchmarks for newly migrated hot paths
- Track coverage floor and per-module migration parity status
- Maintain a profile-driven hotspot list (call-frequency ranked) to steer benchmark-first planning.

## Suggested Milestones

- **M1**: Resource runtime completion (Workstream A)
- **M2**: CPU core first tranche (Workstream B slices 1-2)
- **M3**: Main runtime core first tranche (Workstream C slices 1-2)
- **M4**: Analyze/action stabilization (Workstream D slices 1-2)
- **M5**: Remaining tails + retirement cleanup

## Updated Priority Order (Top 5 executable candidates, 2026-03-18)

1. **Cross-file reuse pass**: Wire `avd_cpop_is_divide_method_split` / `avd_cpop_is_generation_inc_both` into `cHardwareCPU.cc` and `cPopulation.cc` (~8 call-sites, zero new Rust code)
2. **Workstream B: deeper `cHardwareCPU.cc`/`cHardwareExperimental.cc`** — 16/9 avd_ calls in 11K/6K LOC, still significant remaining decision surface
3. **Workstream C: deeper `cPopulation.cc`** — 21 avd_ calls in 9.5K LOC, deme replication/migration dispatch patterns remain
4. **Workstream C: `cPopulationInterface.cc`** — 0 avd_ calls, 2.4K LOC, last large untouched main file
5. **Workstream C: `cLandscape.cc`** — 0 avd_ calls, 1K LOC, fitness landscape math

## Native port update (2026-11-12)

The native workspace `rust/avida-native` adds a second track beside Workstreams A to F. Per-subsystem status is in `rust-migration-waves.md`, section "Native port wave (2026-11-12)".

| Workstream | Effect of the native port |
|---|---|
| A: Resource runtime (`source/main`) | Complete in the FFI track. Ported again natively in `crates/resources` (`cResourceCount`, spatial and gradient resources, 5,971 src lines). No action needed. |
| B: CPU execution core (`source/cpu`) | Overtaken if the port is accepted. Native `crates/hardware` (37,235 src lines) holds the heads CPU, TransSMT, experimental, GP8 and BCR hardware and the test CPU. Instruction-level gaps are in progress. The FFI handler queue (146 remaining, 372/518 delegating) is paused pending the maintainer decision. |
| C: Population and environment core (`source/main`) | Overtaken if accepted. Native `crates/world`, `environment`, `phenotype`, `tasks` and `systematics`. The "Pending cross-file reuse" items above only matter if the FFI track continues. |
| D: Analyze and action pipeline | Overtaken if accepted. Native `crates/analyze` (19,897 src lines) and the action library in `crates/world`. Active work: analyze-mode set-up. |
| E: Script, viewer, systematics tail | Native `crates/script`, `systematics`, and a new front-end layer (`session`, `viewmodel`, `gui`, `tui`) that is an alternative to the ncurses viewer and the parked eframe viewer of Issues #73 to #81. |
| F: ABI and quality infrastructure | Applies to `rust/avida-rust` only. The native workspace has no ABI (no FFI, `unsafe_code = "forbid"`). It needs its own CI lane (open item in `documentation/Tech-Debt-Backlog.md`). |

New workstreams for the native track:

- **Workstream G: Native workspace governance.** Version control (commit strategy), CI (fmt, clippy, tests, consistency suite), and refreshing `docs/ontology.md` and `docs/ontology.json`. Status: not started; entries in the backlog.
- **Workstream H: Native feature gaps.** Classify and close the remaining `unsupported:` stops (hardware crate and analyze-mode set-up in progress). Slice queue: recount after the in-progress work lands, then classify (undefined behaviour in C++, C++ crash, structural limit, gap).
- **Workstream I: Front-end review.** Human review of `gui` and `tui` in real windows and terminals; head display for BCR and GP8 in the debuggers.
- **Workstream J: Retirement decision.** Maintainer decision on superseding the FFI track, then acceptance criteria, then archiving `rust/avida-rust` and the C++. Marked open.

Priority note: until the maintainer decides, no ordering between the FFI queue (Workstreams B and C) and Workstreams G to J is implied. The "Updated Priority Order (Top 5 executable candidates, 2026-03-18)" above remains the last FFI-track ordering.
