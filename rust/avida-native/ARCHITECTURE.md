# Native port: crate layering and ownership model

Read `PORTING.md` first. This file fixes the decisions every crate above the
leaf layer has to share.

## Layers (each crate may depend only on crates listed before it)

| Crate (package) | Ports |
|---|---|
| `apto` (`avida-apto`) | RNG, schedulers, `%g` formatting, data-file writer, accumulators |
| `config` (`avida-config`) | `cAvidaConfig`, `cInitFile`, string utilities, command line |
| `genome` (`avida-genome`) | `Instruction`, `InstructionSequence`, `Genome`, `cCPUMemory`, `cInstSet`, inst-set registry |
| `tasks` (`avida-tasks`) | `cTaskLib`, task evaluators, arg schema |
| `environment` (`avida-environment`) | resources, reactions, `cEnvironment` parser and evaluation |
| `phenotype` (`avida-phenotype`) | `cMerit`, `cMutationRates`, `cPhenotype`, `cCPUTestInfo` data |
| `systematics` (`avida-systematics`) | `Systematics::Manager`, genotype/clade arbiters, groups, sexual ancestry |
| `resources` (`avida-resources`) | `cResourceCount`, spatial and gradient resources |
| `hardware` (`avida-hardware`) | `OrgInterface` trait, `Organism`, `Hardware` trait, `cHardwareBase`, every CPU type, `cTestCPU` |
| `script` (`avida-script`) | the ASL lexer, parser, semantic analysis and interpreter (depends on `apto` and `config` only) |
| `analyze` (`avida-analyze`) | `cAnalyze` command loop, batches, landscapes, the analyze job queue (depends on the crates up to `hardware`, not on `world`) |
| `world` (`avida-world`) | cells, demes, population, birth chamber, stats, actions, events, analyze-mode set-up, driver |
| `avida` (binary) | `targets/avida/primitive.cc` |
| `viewmodel` (`avida-viewmodel`) | nothing in the C++: snapshots of a `World` for front ends |
| `session` (`avida-session`) | nothing in the C++: a `World` on its own thread, driven by commands |
| `gui` (`avida-gui`), `tui` (`avida-tui`) | replace the ncurses viewer; depend on `session` and `viewmodel`, never on `world` directly |

## Status (2026-11-12)

| Crate | State |
|---|---|
| `apto`, `config`, `genome`, `tasks`, `environment`, `phenotype`, `resources` | ported and gated |
| `script` | ported; the 28 `_asl_*` tests match a scratch-built C++ oracle (no `avida-s` reference binary exists); the `Config`/`World`/`Driver` native objects are signature-only stubs that error when called |
| `systematics` | genotype arbiter, groups, `.spop` writer: used by the population run |
| `hardware` | organism, `OrgInterface`, the complete heads CPU (hw type 0; 480 instruction traces identical to the C++ binary), test CPU. TransSMT (hw type 2) and experimental (hw type 3) hardware; GP8 (hw type 4) and BCR (hw type 5) hardware (2,700 random and hand-written test-CPU traces, mini traces of 500 injected organisms and population runs of up to 600 updates identical to the C++ binary; fixtures in `tests/fixtures/gene_env`) |
| `world` | world set-up, organism activation and death, offspring placement, the birth chamber (sexual birth, mating types), scheduling, the run loop as a resumable `Stepper`, events, the action library (the C++ makes 432 registrations, nine names twice, so 423 distinct action names; all are known and implemented; `PrintDemeEnergySharingStats` stops because the C++ reads uninitialised phenotype counters), `Stats` printers, `SavePopulation`/`LoadPopulation`. Demes, groups, parasites, avatars and predator-prey runs are supported; anything unported is rejected at start-up or stops the run with `unsupported: ...` |
| `analyze` | command loop, batches, landscapes and the test-CPU commands on the real test CPU; `run_analyze` is the entry point |
| `avida` binary | population runs and analyze mode |
| `viewmodel` | read-only, toolkit-free snapshots of a `World` for front ends (`capture`), palettes, legends, histograms |
| `session` | owns a `World` on a simulation thread for the front ends: commands in, snapshots out, speed control, the debugger (breakpoints, `StopWhen`), bounded `shutdown` |
| `gui` (`avida-gui`) | the egui/eframe desktop application: map, charts, organism and debugger views, New experiment, CSV/PNG export |
| `tui` (`avida-tui`) | the ratatui terminal interface on the same `session` and `viewmodel` layers; pty-tested for signals, hang-up and terminal restoration |

Whole-suite comparison against the C++ binary (`avida-core/tests`: 122 test directories, of which the 121 with a `test_list` are run, `string_producing_and_donating` has none; 84 run to a zero exit in C++): 85 are byte-identical (those 84 and `analyze_resize_max_batches`, whose exit code 1 the port reproduces; ignoring timestamps, the time-based seed and the data-directory path), 34 more fail the same way the C++ does, none differs (`collect_specification_analyze` prints -1 for the collect-specification counts as the FFI-migrated reference binary does, whose `last_collect_spec_counts` stays empty), 1 stops on a named unported feature (`SLICING_METHOD 4`, on which the C++ crashes), and for 1 (`_mutn_rate`) the reference binary times out. `crates/avida/tests/consistency.rs` pins the matching ones that have checked-in `expected/` output; `crates/avida/tests/stepper.rs` pins that pausing at arbitrary slice boundaries does not change a run.

Deme cell events, deme predicates and the deme network are in `world/src/deme_events.rs`. The reference binary is built without Boost, so the first use of a deme network prints the `cDemeNetwork requires Boost` error and exits with code 50 there and here; the two uninitialised `cDemeCellEvent` members (`m_use_gradient`, `m_eventID`) stop the run with `unsupported:` when read. The energy model runs (`energy.rs`, the birth chamber, `ReplaceDeme`, the deme printers), `FRAC_ENERGY_TRANSFER > 0` included (the deme cell energy that `InsertOrganism` takes up and `RemoveOrganism` gives back, whose `ModifyCell` calls also double the cell's other deme resources, as they do in the C++), and so do `APPLY_ENERGY_METHOD` 1 (energy applied on task completion) and 2 (on `sleep`). `LOG_SLEEP_TIMES` runs until an organism outside cell 0 dies or sleeps, where the C++ indexes past its single `sleep_log` array and the port stops. Avatars run with the heads CPU and the experimental hardware, predator/prey and the look sensor with the experimental hardware (`avatars.rs`, `pred_prey.rs`, `org_sensor.rs`); with `NEURAL_NETWORKING` and `USE_AVATARS 2` messages leave through the sender's output avatars and reach every input avatar in the target cell, the sender's own included under `SELF_COMMUNICATION` (`messaging.rs`), and a `FindAV` that indexes outside the organism's avatars, which the C++ reads out of bounds, stops the run; the mini and micro traces those experiments print (`PrintMiniTraces`, `PrintMicroTraces`, `LoadMiniTraceQ`, `PrintReproData`, `LoadPopulation` with `load_traceq`) are in `world/src/mini_trace.rs` and `hardware/src/mini_trace.rs`, the top-navigation trace (`PrintTopNavTrace`, `PrintNavTrace`, `FlushTopNavTrace`) in `world/src/mini_trace/nav_trace.rs`. Where the C++ prints a navigation trace that is empty (the heads CPU and TransSMT record none, nor do organisms born after the queue was set) it reads before an empty array, and an environment without reactions makes it recurse until the stack overflows; the port stops with an error in both cases.

The birth chamber has every selection handler of the original, the per-deme `cBirthDemeHandler` included (`world/src/birth_chamber.rs`). State grids (`STATE_GRID` in the environment, the `sg-*` instructions and their tasks) run. Predatory, damaging and deadly gradient resources act on the population through `world/src/habitat.rs`: the population's resource count is read through `World::with_pop_resources`, which carries out the kills and injuries `cGradientCount` asks for. The resource reads of `OrgInterface` carry the running organism's state, so a damaging gradient that injures it during such a read injures that state; an injury of the running organism during a read the world makes without its state at hand stops the run, and so do path and den resources next to a habitat (the C++ recurses until the stack overflows) and the avatar branches. Lateral gene transfer on divide (`GetLGTFragment`) is in `world/src/hgt.rs`, and so is horizontal gene transfer (`ENABLE_HGT`: the fragments dying organisms leave in their cells, uptake by competence, conjugation and donation, fragment metabolism, `PrintHGTData`). The match and trimmed fragment placements (`HGT_FRAGMENT_SELECTION` 0 and 1) stop the run when reached, because `cGenomeUtil::FindSubstringMatch` returns a value read from memory it has just freed; so does a gradient habitat kill under HGT with competence, which crashes the C++ when the kill re-enters the resource update.

## Ownership model

The C++ is a pointer graph (world ↔ population ↔ cell ↔ organism ↔ hardware ↔
interface). The Rust port uses three rules instead.

1. **Ids, not pointers.** Cells, demes, organisms, genotypes and resources are
   referred to by `i32`/`usize` ids that index arenas owned by the world. An
   organism stores its `cell_id` and its systematics group ids. The population
   keeps `cells: Vec<PopulationCell>` and, beside it, `organisms:
   Vec<Option<Box<Organism>>>` indexed by cell id, so a cell and its occupant
   can be borrowed independently (`cPopulationCell::InsertOrganism` /
   `RemoveOrganism` are population methods).

2. **Organism = state + hardware, borrowed separately.**
   `struct Organism { pub state: OrgState, pub hardware: Box<dyn Hardware> }`.
   `OrgState` holds everything `cOrganism` holds except the hardware and the
   interface pointer (phenotype, genome, buffers, mutation rates, opinion,
   messages, ...). Hardware methods take the rest explicitly:
   `fn single_process(&mut self, org: &mut OrgState, world: &mut dyn OrgInterface)`.
   `cOrganism` methods that touch the hardware become free functions or
   `Organism` methods; those that only touch state are `OrgState` methods that
   take `&mut dyn OrgInterface` where C++ used `m_interface`.

3. **The running organism is taken out of its cell.** `Population::process_step`
   does `organisms[cell_id].take()`, runs it against a `PopulationInterface { world,
   cell_id }` (the Rust `cPopulationInterface`), then puts it back unless it was
   marked for deletion. While it runs, the world records `running_cell`. Code
   that would have touched the running organism through the population
   (`KillOrganism` on its cell, live-org scans) must go through the
   `&mut Organism`/`&mut OrgState` it was handed, exactly where C++ checks
   `IsRunning()` and defers deletion with `SetToDelete()`. An organism that
   faces its own cell (a torus one cell wide or high) is its own neighbour,
   as in the C++: `with_neighbor` answers `Faced::SelfOrg` and the caller
   reads and writes the state it holds (`with_neighbor_state`).

`OrgInterface` is the port of `cOrgInterface` (same methods, snake_case, data
in and out instead of pointers: `get_neighbor()` becomes accessor methods on
the neighbour or `with_neighbor(&mut dyn FnMut(&mut Organism))`). The RNG and
"context" (`cAvidaContext`) are reached through the interface as
`world.rng()`; there is one RNG per world, as in C++. `cTestCPUInterface` is a
second implementation in the `hardware` crate.

World-level services the C++ reaches through `m_world->` (config, environment,
stats, inst sets) are exposed read-only on `OrgInterface` (`config()`,
`environment()`, `inst_set(id)`), with dedicated mutating methods for the stats
counters hardware and organisms bump.

## Config flags not yet ported

A feature path that is not ported yet must fail loudly at start-up
(`Err("unsupported: SETTING=value (C++: file:symbol)")`), never run with silently
different behaviour. Keep a list of such checks in the crate's `lib.rs` docs.

## What the consistency suite exercises (drives port order)

`docs/test_matrix.json` lists, per test directory under `avida-core/tests/`, the
actions scheduled, hardware types, non-default settings, analyze commands and
command-line arguments. Summary: 122 test directories, 83 with expected
output. Hardware types in use: 0 (heads CPU, 30 tests), 2 (TransSMT, 18),
3 (experimental, 4). 143 distinct instructions, about 70 distinct actions
(the core set `Inject`, `Exit`, `SavePopulation`, `PrintAverageData`,
`PrintCountData`, `PrintDominantData`, `PrintTasksData`, `PrintTimeData`,
`PrintResourceData` covers roughly half the tests), 10 analyze-mode tests
(`LOAD`, `DETAIL`, `RECALCULATE`, landscapes, lineage truncation), and 28 ASL
script tests that run the separate `avida-s` binary.

Port order: heads CPU default run, then resources and limited-resource tasks,
sexual birth and mating types, demes and germlines, parasites and TransSMT,
experimental hardware (avatars, predator/prey, sensors), analyze mode, the ASL
interpreter.
