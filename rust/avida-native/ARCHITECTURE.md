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
| `world` (`avida-world`) | cells, demes, population, birth chamber, stats, actions, events, analyze-mode set-up, driver; also the port-only parallel execution mode (on a `rayon` pool) |
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
| `session` | owns a `World` on a simulation thread for the front ends: commands in, snapshots out, speed control, the debugger (breakpoints, `StopWhen`), bounded `shutdown` (`Drop` waits at most 2 s, then detaches the thread) |
| `gui` (`avida-gui`) | the egui/eframe desktop application: map, charts, organism and debugger views, New experiment, CSV/PNG export |
| `tui` (`avida-tui`) | the ratatui terminal interface on the same `session` and `viewmodel` layers; pty-tested for signals, hang-up and terminal restoration |

Whole-suite comparison against the C++ binary (`avida-core/tests`: 122 test directories, of which the 121 with a `test_list` are run, `string_producing_and_donating` has none; 84 run to a zero exit in C++): 85 are byte-identical (those 84 and `analyze_resize_max_batches`, whose exit code 1 the port reproduces; ignoring timestamps, the time-based seed and the data-directory path), 34 more fail the same way the C++ does, none differs (`collect_specification_analyze` prints -1 for the collect-specification counts as the FFI-migrated reference binary does, whose `last_collect_spec_counts` stays empty), 1 stops on a named unported feature (`SLICING_METHOD 4`, on which the C++ crashes), and for 1 (`_mutn_rate`) the reference binary times out. `crates/avida/tests/consistency.rs` pins the matching ones that have checked-in `expected/` output; `crates/avida/tests/stepper.rs` pins that pausing at arbitrary slice boundaries does not change a run.

Deme cell events, deme predicates and the deme network are in `world/src/deme_events.rs`. The reference binary is built without Boost, so the first use of a deme network prints the `cDemeNetwork requires Boost` error and exits with code 50 there and here; the two uninitialised `cDemeCellEvent` members (`m_use_gradient`, `m_eventID`) stop the run with `unsupported:` when read. The energy model runs (`energy.rs`, the birth chamber, `ReplaceDeme`, the deme printers), `FRAC_ENERGY_TRANSFER > 0` included (the deme cell energy that `InsertOrganism` takes up and `RemoveOrganism` gives back, whose `ModifyCell` calls also double the cell's other deme resources, as they do in the C++), and so do `APPLY_ENERGY_METHOD` 1 (energy applied on task completion) and 2 (on `sleep`). `LOG_SLEEP_TIMES` runs until an organism outside cell 0 dies or sleeps, where the C++ indexes past its single `sleep_log` array and the port stops. Avatars run with the heads CPU and the experimental hardware, predator/prey and the look sensor with the experimental hardware (`avatars.rs`, `pred_prey.rs`, `org_sensor.rs`); with `NEURAL_NETWORKING` and `USE_AVATARS 2` messages leave through the sender's output avatars and reach every input avatar in the target cell, the sender's own included under `SELF_COMMUNICATION` (`messaging.rs`), and a `FindAV` that indexes outside the organism's avatars, which the C++ reads out of bounds, stops the run; the mini and micro traces those experiments print (`PrintMiniTraces`, `PrintMicroTraces`, `LoadMiniTraceQ`, `PrintReproData`, `LoadPopulation` with `load_traceq`) are in `world/src/mini_trace.rs` and `hardware/src/mini_trace.rs`, the top-navigation trace (`PrintTopNavTrace`, `PrintNavTrace`, `FlushTopNavTrace`) in `world/src/mini_trace/nav_trace.rs`. Where the C++ prints a navigation trace that is empty (the heads CPU and TransSMT record none, nor do organisms born after the queue was set) it reads before an empty array, and an environment without reactions makes it recurse until the stack overflows; the port stops with an error in both cases.

The birth chamber has every selection handler of the original, the per-deme `cBirthDemeHandler` included (`world/src/birth_chamber.rs`). State grids (`STATE_GRID` in the environment, the `sg-*` instructions and their tasks) run. Predatory, damaging and deadly gradient resources act on the population through `world/src/habitat.rs`: the population's resource count is read through `World::with_pop_resources`, which carries out the kills and injuries `cGradientCount` asks for. The resource reads of `OrgInterface` carry the running organism's state, so a damaging gradient that injures it during such a read injures that state; an injury of the running organism during a read the world makes without its state at hand stops the run, and so do path and den resources next to a habitat (the C++ recurses until the stack overflows) and the avatar branches. Lateral gene transfer on divide (`GetLGTFragment`) is in `world/src/hgt.rs`, and so is horizontal gene transfer (`ENABLE_HGT`: the fragments dying organisms leave in their cells, uptake by competence, conjugation and donation, fragment metabolism, `PrintHGTData`). The match and trimmed fragment placements (`HGT_FRAGMENT_SELECTION` 0 and 1) stop the run when reached, because `cGenomeUtil::FindSubstringMatch` returns a value read from memory it has just freed; so does a gradient habitat kill under HGT with competence, which crashes the C++ when the kill re-enters the resource update.

Phenotypic-plasticity bonuses run (`phenplastbonus=nobonus`, `fracbonus` or `fullbonus` on a reaction process). As in the C++, the first output test of a genotype that reaches such a reaction runs `cPhenPlastUtil::TestPlasticity` from inside `cEnvironment::GetTaskProbability`, between the task test and the reaction's processes: `GENOTYPE_PHENPLAST_CALC` trials of the genotype's genome on one fresh test CPU, with random inputs when there is more than one trial, drawing from the world RNG. The summary is kept per genotype in `WorldHost::phenplast`, which `PrintTaskProbHistogram` and `PrintPlasticGenotypeSummary` share, and summaries of genotypes that have left the systematics arena are dropped whenever the cache has doubled. No layer reaches above itself: the environment lends itself to the evaluator (`TaskEvaluator::task_probability`), the hardware crate adds the output test's RNG and statistics (`PlasticityTest`) and what the world read before lending them (`PlasticityServices` in `OutputServices`: hardware manager, population counts, the genotype's genome), and `WorldHost` runs the test CPUs as the world of a test organism in cell -1, restoring the tested organism's cell and kaboom snapshot afterwards. `crates/avida/tests/actions/phenplast_*` pin the three methods against the reference binary. Two paths still stop: `consume-public-good` in a plasticity test reading the tested organism's own cell (the C++ reads that live organism's flag; here it is out of its cell, as for any test CPU a living organism runs), and an organism without a genotype, where the C++ would read a NULL group.

`optimize` tasks evaluate as the reference binary's helper `avd_task_eval_optimize` does, edge cases included (an unknown function, a proportional quality whose terms are not positive, binary decoding whose total is 0, `varlength` past the stored outputs), and print `quality > 1!` as the C++ does (`tasks/src/eval_misc.rs`; in population runs those lines still reach standard output ahead of the buffered update lines, because `OrgTaskWorld` does not yet forward `TaskWorld::task_stdout` to the world host). Arguments on which the reference binary crashes, aborts or does not finish are refused when the environment is loaded with `unsupported: optimize ...`: `function=20` (a string argument `Load_Optimize` never declares), a negative `numvars` with a decoding function or function 19, fewer variables than the function reads, and a negative `varlength` with the binary decoding, with function 19 and two or more variables, or with function 21 (which hangs only once the outputs it reads are all 1). The fused multiply-adds of `analyze` and `script` were checked site by site against the reference disassembly; all match (PORTING.md).

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

## Parallel execution mode (`EXECUTION_MODE 1`)

The one place where the port departs from parity on purpose (approved by
the maintainer; see PORTING.md). Two settings that the C++ does not have,
read from `avida.cfg` or `-set` like any other (`config/src/port_settings.rs`;
the C++-format printers leave them out, the `-r` review lists them when they
differ from their defaults):

| Setting | Values |
|---|---|
| `EXECUTION_MODE` | `0` exact (default): the C++ scheduler, one instruction per time slice, byte parity. `1` parallel. Anything else is rejected at start-up. |
| `EXECUTION_THREADS` | worker threads of the parallel mode; `0` (default) or less means every core `std::thread::available_parallelism` reports |

### What an update does

`world/src/parallel.rs` (`ParallelExec`, driven by `Stepper::step_slice`,
which runs a whole update as one slice) and `hardware/src/parallel.rs`
(the organism side):

1. **Budgets.** The update's `CalculateUpdateSize()` instructions, less the
   carry-over of step 5, are shared out among the organisms the scheduler
   would run, in proportion to their `AdjustSchedule` priority (equal shares
   under `SLICING_METHOD 0`). Under the deterministic slicing methods (0
   and 2) the shares are rounded by largest remainder, with the fractional
   parts carried to the next update. Under the lottery methods (1, the
   default, and 5) each budget is a Poisson draw with the share as its mean,
   which is what a lottery over the update's time slices gives an organism;
   the draws come in cell order from a stream of their own and are then
   moved to the target one instruction at a time, by draws weighted like the
   shares. The grants and rescales of step 4 are random in the same way.
   Exact shares under a lottery method made cells stay empty longer (see
   "Statistical equivalence").
2. **Parallel phase.** Each organism with budget left runs a burst on the
   worker pool: speculative steps against a `LocalInterface` that serves
   the configuration, the environment, the hardware manager, a few stats
   read at the start of the phase and the organism's own random stream
   (seeded from the run seed and the organism's uid), and refuses
   everything else. A step stops before its instruction when the
   instruction set marks it `STALL` or its handler is not on the hardware
   type's allow-list (`CPU_PARALLEL_METHODS` for the heads CPU: no-ops,
   conditionals, flow control, stacks, registers, arithmetic, heads,
   copying, allocation, search, threads; every handler on it was read for
   what it reaches, and a refused request stops the run with an
   `internal:` error instead of computing something else). Mutations
   drawn in a burst (copy mutations, `if-p-*`, `PROB_FAIL`) come from the
   organism's stream. The executed instructions then advance the
   instruction counter and the resource clock exactly as that many exact
   time slices would.
3. **Sequential phase.** The organisms that stopped run that one
   instruction through the exact code path (`World::process_step`,
   non-speculative), in the order of a random permutation of the cells
   drawn for the update from a stream of its own.
4. **Changes of rate.** Each organism keeps an update clock (how far
   through the update its spending has taken it, 0 to 1). An organism whose
   priority changed during its step (a divide that changed its merit) has
   the rest of its budget scaled to its new share; an organism born during
   the update gets its full share times one minus its parent's clock, and
   its clock starts at its parent's, which is what the exact scheduler
   gives it in expectation and keeps a chain of births within one update
   finite.
5. Phases 2 to 4 repeat until no budget is left. Budgets lost with
   organisms that died and the grants of step 4 make an update run a little
   fewer or more instructions than its size; the difference (up to one
   update's size) is carried over to the next update's target, so a run
   executes the same total as the exact mode for the same update sizes.

### Guarantees

- `EXECUTION_MODE 0` is unchanged: the hooks it shares with the parallel
  mode (the priority mirror in `AdjustSchedule`, `last_step_cell` in
  `process_step`, the `parallel_burst` test in the heads CPU's speculative
  stall check, the `OnceLock` in `ResourceLib`) compute nothing the exact
  mode reads, and `update_organism_stats`, split into reading an organism
  (`gather_org_stats`) and adding it to the sums (`OrgStatsTotals::add`)
  so that the parallel mode can do the reading on its worker pool, adds
  the same values in the same order. The whole consistency suite is
  byte-identical with the change, as before it.
- A parallel run is a function of its configuration and seed: bursts touch
  only their own organism, their results are merged in a fixed order, and
  everything else is sequential. `EXECUTION_THREADS` and machine load do not
  change a byte of the output (`crates/avida/tests/parallel.rs` compares 1, 2
  and 4 threads on a growing population and 1 and 4 on a full world, where
  the worker pool runs most bursts).
- Every instruction runs the same handler as in the exact mode; only the
  order of instructions between organisms within an update, and the random
  streams, differ.
- Statistical equivalence with the exact mode on the outcomes in
  `tools/parallel_stats.py` (see the numbers below).

### Not guaranteed

- Byte parity with the C++ or with the exact mode, for any seed. The
  world generator is drawn from in a different order (bursts draw from the
  organisms' streams), so runs diverge from the first mutation.
- The interleaving within an update. In the exact mode organisms take
  turns of one instruction (speculative slices of up to 33) by a lottery on
  merit; here each organism runs its organism-local instructions up to its
  next world instruction at once, world instructions go in rounds, and an
  organism dies, is born or changes merit at round granularity.
- Per-update instruction counts (`count.dat` column 2) vary by the
  carry-over (a few percent of an update); the run total does not.
- With one deme the deme clock counts only the instructions of the
  sequential phase (a speculative exact slice skips it as well).
- The speculative statistics of the status line are zero and `SPECULATIVE`
  is ignored. The front ends' debugger steps whole updates in this mode.
- `SetConfig EXECUTION_MODE` during a run has no effect; the mode is read
  when the run starts.

### Unsupported, and where it falls back

Rejected at start-up with `unsupported in EXECUTION_MODE 1: ...`:
`NUM_DEMES` above 1 (every exact time slice updates the running organism's
deme and may replicate it). Everything else runs, but these organisms or
runs execute every instruction through the sequential phase, so they gain
nothing: hardware types other than the heads CPU (TransSMT and its
parasites, experimental, GP8, BCR), organisms with a tracer or a mini,
micro, navigation or reproduction trace, `THREAD_SLICING_METHOD 1`,
promoters, constitutive regulation, implicit reproduction, instructions
with resource costs, and whole runs at `VERBOSITY` 3 or more (slip mutations
print) or with `TASK_SWITCH_PENALTY` (charged to the deme from the cost
check).

### Diagnostics and performance

`AVIDA_EXECUTION_STATS=1` in the environment prints one line at the end of
`World::run` to stderr: rounds per update, the share of instructions run in
the sequential phase, the newborn grants, the rescales, the carry-over,
bursts per update and the share run inline, and the wall time in
allocation, parallel and sequential phases. It is not a C++ output.

### Statistical equivalence (2026-11-12)

`tools/parallel_stats.py run <config> <out>` then `compare <out>`: the
default configuration (`support/config`, heads CPU, 60 x 60, merit-weighted
slicing), seeds 101 to 116 in each mode, 5000 updates; medians with
interquartile ranges and two-sided Mann-Whitney p values (exact without
ties):

| Outcome at update 5000 | Exact | Parallel | p |
|---|---|---|---|
| log10 average fitness | 3.42 [2.56, 3.54] | 3.33 [3.04, 3.84] | 0.64 |
| average genome length | 102.2 [100.0, 105.1] | 100.1 [97.0, 105.1] | 0.36 |
| tasks held by at least 5% of the organisms | 6 [5, 6] | 6 [5, 7] | 0.71 |
| first update with NOT | 527 [436, 609] | 557 [430, 684] | 0.65 |
| first update with NAND | 650 [534, 856] | 643 [556, 788] | 0.69 |
| average generation | 1062 [954, 1146] | 1071 [986, 1148] | 0.70 |
| average gestation time | 371 [361, 383] | 355 [347, 407] | 0.56 |
| organisms | 3593 [3592, 3594] | 3590 [3589, 3592] | 0.031 |
| instructions in the last update | 107,800 | 107,700 | 0.99 |

**Empty cells: cause and fix.** The run of this comparison before the
budgets of step 1 were made random had 3590 organisms against 3593
(p = 0.031). Instrumented builds (one log line per death, birth and update)
traced the difference to how long a cell stays empty, not to how often one
empties. Over updates 800 to 1500 of eight evolving runs per mode, the
parallel mode had 5.41 empty cells against 3.44 (p = 0.002), and a cell
emptied by a death was refilled within the same update 11% of the time
against 18% (p = 0.005). Only deaths of old age (`DEATH_METHOD 2`) empty
cells in this configuration, about 99% of them organisms that never divided.
Their rate was higher as well (1.53 against 1.26 per update, p = 0.05), but
that is not a direct effect of the schedule: from one starting population
(an exact run's population at update 1000, loaded with `LoadPopulation`;
twelve seeds per variant, updates 100 to 300 of 300) the two modes have the
same rate of deaths of old age (1.63 against 1.64 per update, p = 0.25)
while the empty cells still differ (5.49 against 4.43, p < 0.001). In
evolving runs the rate follows what each population evolved; the spread of
merit and the share of sterile offspring vary greatly between seeds.

The cause was the deterministic budget. `SLICING_METHOD 1` hands out each
time slice by a lottery on merit, so the instructions an organism receives
in an update vary like a Poisson count; the parallel mode gave every
organism exactly its share. Related organisms (equal merit and gestation
time) then stay in phase indefinitely. A sterile offspring dies after
`AGE_LIMIT` (20) times its length in instructions, about 5.1 of its
relatives' gestation times in these runs, so it dies at the same point of
their cycle every time, just after their divisions, and its cell waits
nearly a whole gestation for a neighbour's offspring. The lottery spreads
those phases. The decisive comparison, from the same starting population:
the exact mode under `SLICING_METHOD 2` (the C++'s deterministic integrated
scheduler) has the parallel mode's 5.49 empty cells and its same-update
refill rate (0.10, against 0.16 under the lottery), while the exact mode
without speculative slices (`SPECULATIVE 0`) does not differ from the
default (4.64 empty cells, p = 0.51). Bursts and rounds were not the cause.
(An earlier note here, from evolving runs of a four-window variant,
attributed about half of the excess to bursts; from a common start the
deterministic budget accounts for all of it.)

With random budgets (step 1 as now), from the same start: 4.56 empty cells
against 4.43 (p = 0.47), deaths of old age 1.58 against 1.64 (p = 0.18).
Evolving, updates 800 to 1500 of eight seeds: 4.29 empty cells against 3.44
(p = 0.20; one seed has 9.2 after a selective sweep), same-update refills
0.17 against 0.18 (p = 0.80), deaths of old age 1.45 against 1.26
(p = 0.38). A small difference remains from the common start: 173.4 births
per update against 176.3 (p = 0.010), same-update refills 0.13 against 0.16
(p = 0.02). The instrumented build points to bursts running ahead: an
organism that dies in an update has usually spent most of that update's
share in the first round's burst before its death comes up in a sequential
phase (0.80 of its share, against 0.46 in the exact mode; 3.4% of all
instructions go to organisms in their last update, against 2.2%), and an
organism that dies in a late round leaves too little of the update for a
neighbour's offspring to fill its cell within it. Both follow from running
bursts before world instructions; refunding the budget of the dead would
change the instructions per update.

Two variants were measured and rejected: ordering each sequential phase by
the organisms' update clocks instead of the random permutation moved the
average generation to 980 (p = 0.09 against the exact mode, 0.06 against the
permutation) and the organisms to 3588 (p < 0.001); cutting each update into
four clock windows changed neither statistic and tripled the bursts. An
earlier design that measured a newborn's grant against its parent's own
grant instead of the update clock let chains of fast replicators earn grants
without end (one seed never finished an update); the update clock and the
bounded carry-over fix that.

### Performance

Measured on 2026-11-12 with the current build (random budgets) on an Apple
M1 Pro (8 performance and 2 efficiency cores) with nothing else computing
(load average 5 to 7 from desktop applications). Default configuration,
seed 42, 3000 updates, release build, one run at a time
(`AVIDA_EXECUTION_STATS=1`):

| Run | Wall time | Against exact |
|---|---|---|
| exact mode | 47.2 s | 1.0 |
| parallel, 1 thread | 28.6 s | 1.65 |
| parallel, 2 threads | 21.0 s | 2.25 |
| parallel, 4 threads | 18.1 s | 2.61 |
| parallel, 8 threads | 18.1 s (6.3 s system) | 2.61 |

The four parallel runs wrote identical files. With 4 threads the update
loop took 14.0 s (33 rounds per update, 1.8% of the instructions in the
sequential phase); 4 s are outside it. The same measurement of the build
with deterministic budgets gave 42.2, 27.0, 21.5, 19.8 and 19.6 s.

The figures in the rest of this section are older and were taken while
other simulations ran beside the measured one; they show where the time
goes, not how fast the mode is. Same configuration:

| Build and run | Wall time | Load average during the run |
|---|---|---|
| final, exact mode | 50.4 s | 8.5 to 10.5 |
| final, parallel, 1 thread | 35.1 s | 9.6 to 10.5 |
| final, parallel, 4 threads | 26.8 s (37.0 s user, 2.8 s system) | 9.6 to 10.9 |
| earlier, exact mode | 54.5 s | 8 to 11 |
| earlier, parallel, 1 thread | 30.3 s | 3 to 5 |
| earlier, parallel, 8 threads | 21.7 s (37.4 s user, 11.3 s system) | 3 to 5 |

During the final runs four other single-threaded simulations held four of
the eight performance cores, so the 4-thread run competed for cores and 8
threads were not tried. The earlier build differs from the final one in how
newborns are granted budget (step 4) and in having no bound on the
carry-over; the burst and sequential code paths are the same. The final
build reproduces the statistics run of seed 101 above byte for byte.

Most of the single-thread gain comes from bursts themselves: an
organism-local instruction in a burst skips the scheduler draw, the
speculative bookkeeping and the interface dispatch of an exact time slice.
In the final 4-thread run the update loop took 19.6 s of the 26.8 s, of
which 3% went to allocation, 41% to the parallel phases and 55% to the
sequential phases, at 26 rounds per update (186 at most), 5,200 bursts per
update (11% of them inline) and 2.3% of the instructions in the sequential
phase. The first round of an update runs nearly every organism; the other
25 average a few dozen bursts each. The remaining 7 s are outside the
update loop (`update_organism_stats` and output, both sequential) and are
the same with 1 thread, which bounds what more threads can give. A profile
of the main thread of the earlier 8-thread run put `update_organism_stats`
at about 23%, output at 21%, divides (in the sequential phase) at 20% and
waiting for the pool at 20%.

Bursts go to the worker pool only when a round has at least
`POOL_MIN_ORGANISMS` (256) of them, and run inline otherwise, because
handing a few dozen short bursts to the pool costs more than it gains. The
pool is sensitive to load: with other processes on every core an 8-thread
run took 91 s, and a larger threshold (2048) was the fastest of those
tried under load (17.7 s against 25.5 s at 48 in the update loop over 1500
updates). For batches of replicate runs, where the cores are busy anyway,
`EXECUTION_THREADS 1` keeps the single-thread gain without the contention
(`tools/parallel_stats.py` runs the parallel mode that way). The wall times
of the statistics runs above are not comparable between the modes: the two
sets ran hours apart under very different load.

Where further speed would come from, in the order of the profile: a
parallel gather for `update_organism_stats`, output written off the main
thread, fewer rounds per update (after the first, a round advances only a
few dozen organisms past one world instruction each), and the main thread taking
part in the pool instead of waiting on it.

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
