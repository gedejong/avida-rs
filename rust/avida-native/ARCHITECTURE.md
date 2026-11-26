# Native port: crate layering and ownership model

Read `PORTING.md` first. This file fixes the decisions every crate above the
leaf layer has to share.

## Layers (each crate may depend only on crates listed before it)

| Crate (package) | Ports |
|---|---|
| `apto` (`avida-apto`) | RNG, schedulers, `%g` formatting, data-file writer, accumulators |
| `config` (`avida-config`) | `cAvidaConfig`, `cInitFile`, string utilities, command line; the experiment file (`avida.yaml`: grouping table, loader, converter from the old files) |
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
| `session` | owns a `World` on a simulation thread for the front ends: commands in, snapshots out, speed control, the debugger (breakpoints, `StopWhen`), bounded `shutdown` (`Drop` waits at most 2 s, then detaches the thread). Also records the arena to a file (`recording`, `Command::Record`), plays a recording back through the same `Session` handle (`Session::play`), and ships the headless recorder `avida-rec` (see "Recording and playback") |
| `gui` (`avida-gui`) | the egui/eframe desktop application: map, charts, organism and debugger views, New experiment, CSV/PNG export; `--play FILE` plays a recording, `--record FILE` records the run |
| `tui` (`avida-tui`) | the ratatui terminal interface on the same `session` and `viewmodel` layers; pty-tested for signals, hang-up and terminal restoration; `--play FILE` and `--record FILE` as in the GUI |

Whole-suite comparison against the C++ binary (`avida-core/tests`: 122 test directories, of which the 121 with a `test_list` are run, `string_producing_and_donating` has none; 84 run to a zero exit in C++): 85 are byte-identical (those 84 and `analyze_resize_max_batches`, whose exit code 1 the port reproduces; ignoring timestamps, the time-based seed and the data-directory path), 34 more fail the same way the C++ does, none differs (`collect_specification_analyze` prints -1 for the collect-specification counts as the FFI-migrated reference binary does, whose `last_collect_spec_counts` stays empty), 1 stops on a named unported feature (`SLICING_METHOD 4`, on which the C++ crashes), and for 1 (`_mutn_rate`) the reference binary times out. `crates/avida/tests/consistency.rs` pins the matching ones that have checked-in `expected/` output; `crates/avida/tests/stepper.rs` pins that pausing at arbitrary slice boundaries does not change a run.

Deme cell events, deme predicates and the deme network are in `world/src/deme_events.rs`. The reference binary is built without Boost, so the first use of a deme network prints the `cDemeNetwork requires Boost` error and exits with code 50 there and here; the two uninitialised `cDemeCellEvent` members (`m_use_gradient`, `m_eventID`) stop the run with `unsupported:` when read. The energy model runs (`energy.rs`, the birth chamber, `ReplaceDeme`, the deme printers), `FRAC_ENERGY_TRANSFER > 0` included (the deme cell energy that `InsertOrganism` takes up and `RemoveOrganism` gives back, whose `ModifyCell` calls also double the cell's other deme resources, as they do in the C++), and so do `APPLY_ENERGY_METHOD` 1 (energy applied on task completion) and 2 (on `sleep`). `LOG_SLEEP_TIMES` runs until an organism outside cell 0 dies or sleeps, where the C++ indexes past its single `sleep_log` array and the port stops. Avatars run with the heads CPU and the experimental hardware, predator/prey and the look sensor with the experimental hardware (`avatars.rs`, `pred_prey.rs`, `org_sensor.rs`); with `NEURAL_NETWORKING` and `USE_AVATARS 2` messages leave through the sender's output avatars and reach every input avatar in the target cell, the sender's own included under `SELF_COMMUNICATION` (`messaging.rs`; its own full `MESSAGE_RECV_BUFFER_SIZE` buffer drops the message as `ReceiveMessage` does, the hardware telling the world the buffer length before each send), and a `FindAV` that indexes outside the organism's avatars, which the C++ reads out of bounds, stops the run; the mini and micro traces those experiments print (`PrintMiniTraces`, `PrintMicroTraces`, `LoadMiniTraceQ`, `PrintReproData`, `LoadPopulation` with `load_traceq`) are in `world/src/mini_trace.rs` and `hardware/src/mini_trace.rs`, the top-navigation trace (`PrintTopNavTrace`, `PrintNavTrace`, `FlushTopNavTrace`) in `world/src/mini_trace/nav_trace.rs`. Where the C++ prints a navigation trace that is empty (the heads CPU and TransSMT record none, nor do organisms born after the queue was set) it reads before an empty array, and an environment without reactions makes it recurse until the stack overflows; the port stops with an error in both cases.

Event-file constructors that call `exit(1)` (`CompeteDemesByTaskCountAndEfficiency` without an initial deme energy, `SetEnvironmentInputs` with other than three inputs or inputs that do not begin 0F, 33, 55) print the C++ message to standard error and exit with status 1 (`actions::cerr_exit`). `SetRedundancy` and `SetProbFail` change the one shared `InstSet` (atomics and a lock inside `genome/src/inst_set.rs`), so every hardware type and the parasites see the change. `InjectClone` with `USE_AVATARS` places the clone's avatar (`PlaceAvatar` next to the original, `CopyDeme`, `DivideDemes`, `SpawnDeme`, `CompeteDemes` copies) and `MixPopulation` with `USE_AVATARS` leaves the avatars where they are; `ResetDemes` and the in-place `CompeteDemes` reset with `USE_AVATARS` still stop, because the reference binary crashes there (`PlaceAvatar` reads the original organism after `ActivateOrganism` deleted it). `ChangeEnvironment` and the other callers of `UpdateResourceCount` run with deme resources of global geometry; a grid, torus or partial deme resource still stops (unsized deme grids).

The birth chamber has every selection handler of the original, the per-deme `cBirthDemeHandler` included (`world/src/birth_chamber.rs`). State grids (`STATE_GRID` in the environment, the `sg-*` instructions and their tasks) run. Predatory, damaging and deadly gradient resources act on the population through `world/src/habitat.rs`: the population's resource count is read through `World::with_pop_resources`, which carries out the kills and injuries `cGradientCount` asks for. The resource reads of `OrgInterface` carry the running organism's state, so a damaging gradient that injures it during such a read injures that state; the same holds for the group count of an offspring's group placement and for `UpdateRandomResources`, which now receive it. The resource update that the HGT fragment bookkeeping (`AdjustHGTResource`, from `AddGenomeFragments` on a kill, `ClearFragments` and the fragment taken in `DoHGTMutation`) starts carries the running organism's state as well, so a damaging gradient that injures it there acts on that state (`crates/avida/tests/actions/gap_hgt_damaging_*`); path and den resources next to a habitat still stop the run (the C++ recurses until the stack overflows) and the avatar branches. A deadly or predatory hit on the running organism sets `to_die`, which each hardware folds in at the end of `SingleProcess` (`OrgInterface::take_habitat_to_die`) where the C++ reads `GetToDie()`: speculative execution (`m_spec_die`) and implicit reproduction (`Die` before `CheckImplicitRepro`) follow the C++. Lateral gene transfer on divide (`GetLGTFragment`) is in `world/src/hgt.rs`, and so is horizontal gene transfer (`ENABLE_HGT`: the fragments dying organisms leave in their cells, uptake by competence, conjugation and donation, fragment metabolism, `PrintHGTData`). The match and trimmed fragment placements (`HGT_FRAGMENT_SELECTION` 0 and 1) stop the run when reached, because `cGenomeUtil::FindSubstringMatch` returns a value read from memory it has just freed; so does a gradient habitat kill under HGT with competence, which crashes the C++ when the kill re-enters the resource update. `Inst_SetRandPFTOnce` under `MAX_PREY_BT` in a test CPU kills the population organism it picks when the test ends (`HostOp::KillCell`, carried out by `World::flush_host`) in every test whose end the world answers at once: the divide-time tests (`Divide_TestFitnessMeasures`, the genotype's test fitness), the tests of the events that make one test (`PrintDominantGenotype`, the genome files of the mini-trace actions, the cached genotype test metrics) and the plasticity test inside an output test (flushed when the step ends); the test world's list of live organisms and its prey and predator counts go on without the victim for the later runs of the same test (`WorldHost::kill_ok`, `crates/avida/tests/actions/gap_exp_max_prey_bt_event_test`, `gap_exp_max_prey_bt_divide_kills`, `gap_exp_max_prey_bt_plasticity`). Still stopping: a pick of the dividing organism itself (the C++ dies with SIGSEGV there, checked on the reference for seeds 11, 15 and 16 of the divide-kills configuration), a kill from a loop of tests (`PrintTaskSnapshot`, `PrintSoloTaskSnapshot`, `PrintDominantForagerGenotypes`, the `hist_*` analyses with a test world: the loop reads the population between the tests) or from the precalculated phenotype of a birth, a read of a group size or of a neighbour's kaboom flag after a kill in the same test, and a kill that draws random numbers. The `live_org_forage_targets` stop of the test CPU is unreachable, since every test CPU run from a live organism's step lends that organism's forage target, but stays as a guard.

Phenotypic-plasticity bonuses run (`phenplastbonus=nobonus`, `fracbonus` or `fullbonus` on a reaction process). As in the C++, the first output test of a genotype that reaches such a reaction runs `cPhenPlastUtil::TestPlasticity` from inside `cEnvironment::GetTaskProbability`, between the task test and the reaction's processes: `GENOTYPE_PHENPLAST_CALC` trials of the genotype's genome on one fresh test CPU, with random inputs when there is more than one trial, drawing from the world RNG. The summary is kept per genotype in `WorldHost::phenplast`, which `PrintTaskProbHistogram` and `PrintPlasticGenotypeSummary` share, and summaries of genotypes that have left the systematics arena are dropped whenever the cache has doubled. No layer reaches above itself: the environment lends itself to the evaluator (`TaskEvaluator::task_probability`), the hardware crate adds the output test's RNG and statistics (`PlasticityTest`) and what the world read before lending them (`PlasticityServices` in `OutputServices`: hardware manager, population counts, the genotype's genome), and `WorldHost` runs the test CPUs as the world of a test organism in cell -1, restoring the tested organism's cell and kaboom snapshot afterwards. `crates/avida/tests/actions/phenplast_*` pin the three methods against the reference binary. `consume-public-good` runs inside a plasticity test, including when the visited cell is the tested organism's own: the organism is out of its cell while it runs, so the output test hands the host its current kaboom flag (`PlasticityTest::own_kaboom`), the test CPUs read it as the live organism's flag, and a `ClearKaboomExecuted()` they make is written back to the organism (`crates/avida/tests/actions/phenplast_public_good` pins the read against the reference binary; the write-back follows the C++ but no configuration found makes it visible in output). The test organism sits in cell -1, so it visits the grid's corner cells with wrapped offsets, and a single producer reaches the count of three only in a world two cells wide or high, where wrapped offsets revisit cells; with two or more distinct producers the choice stays an `unsupported:` stop, because the C++ indexes a `std::set<cOrganism*>` ordered by heap address. One path stops for good: an organism without a genotype reaching `cEnvironment::GetTaskProbability`, where `cOrganism::SystematicsGroup("genotype")` returns a null `GroupPtr` and `cPhenPlastUtil::GetTaskProbability` calls `bg->GetData<cPhenPlastSummary>()` on it, a null dereference that crashes the reference binary (a test CPU organism never gets there, because test mode returns -1 first, and no configuration that reaches it with a population organism has been found).

`optimize` tasks evaluate as the reference binary's helper `avd_task_eval_optimize` does, edge cases included (an unknown function, a proportional quality whose terms are not positive, binary decoding whose total is 0, `varlength` past the stored outputs), and print `quality > 1!` as the C++ does (`tasks/src/eval_misc.rs`; in population runs those lines still reach standard output ahead of the buffered update lines, because `OrgTaskWorld` does not yet forward `TaskWorld::task_stdout` to the world host). Arguments on which the reference binary crashes, aborts or does not finish are refused when the environment is loaded with `unsupported: optimize ...`: `function=20` (a string argument `Load_Optimize` never declares), a negative `numvars` with a decoding function or function 19, fewer variables than the function reads, and a negative `varlength` with the binary decoding, with function 19 and two or more variables, or with function 21 (which hangs only once the outputs it reads are all 1). The fused multiply-adds of `analyze` and `script` were checked site by site against the reference disassembly; all match (PORTING.md).

Analyze mode (`avida -a`) sets up the whole population as `cWorld::setup` does, through the `SetupHooks` of `world/src/analyze_setup.rs` (one implementation, the one population runs use), so `NUM_DEMES` above 1, `WORLD_GEOMETRY` 7 and 8, deme resources, gradient resources on a grid or torus, `ENABLE_HGT` with an `hgt=1` resource and `DEMES_MIGRATION_METHOD 4` all draw from the world RNG and print their set-up lines exactly as the reference does; `crates/avida/tests/actions/an2_*` (demes, migration matrix, both random topologies, deme resource, gradient on grid and torus, HGT, and all together; each runs `LOAD`, `RECALCULATE`, `DETAIL` and RNG-drawing commands) pin that against the reference binary. The `unsupported:` stops of `crates/analyze/src/world.rs` for these settings belong to the hook-less `AnalyzeWorld::initialize`, which only this crate's own tests use (`analyze` may not depend on `world`); `avida -a` never reaches them, nor the "world RNG" stops, which only a host without a world RNG (the script tests' mock) can hit. What still stops in `avida -a` are set-ups on which the reference binary crashes (SIGSEGV, exit 139, a NULL `WorldDriver` dereferenced while reporting): `NUM_DEMES` that does not divide `WORLD_Y`, an unknown `SLICING_METHOD`, two `hgt=1` resources, `ENABLE_HGT` without an `hgt=1` resource, and a gradient halo narrower than twice its height.

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
read from the experiment file (`execution.mode`, `execution.threads`) or
`-set` like any other (`config/src/port_settings.rs`;
the C++-format printers leave them out, the `-r` review lists them when they
differ from their defaults):

| Setting | Values |
|---|---|
| `EXECUTION_MODE` | `0` exact (default): the C++ scheduler, one instruction per time slice, byte parity. `1` parallel. Anything else is rejected at start-up. |
| `EXECUTION_THREADS` | worker threads of the parallel mode; `0` (default) or less means every core `std::thread::available_parallelism` reports |

The other port-only settings are `DROP_EXTINCT_GENOTYPES` (see "Dropping extinct genotypes"), and `EXPECTED_QUALITY_DRAWS` (default 8) with `EXPECTED_QUALITY_MIN` (default 0.25), which pay a genotype's average task quality to tasks with `expect=1` (exact mode only; PORTING.md, "Expected quality per genotype").

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
   type's allow-list (`CPU_PARALLEL_METHODS` for the heads CPU, and
   `SMT_PARALLEL_METHODS` for TransSMT, see "TransSMT in bursts": no-ops,
   conditionals, flow control, stacks, registers, arithmetic, heads,
   copying, allocation, search, threads; every handler on it was read for
   what it reaches, and a refused request stops the run with an
   `internal:` error instead of computing something else; where they are
   organism-local, the heads CPU's input, output and divide instructions
   run in the burst too, see "Output tests and divides in the burst"). Mutations
   drawn in a burst (copy mutations, `if-p-*`, `PROB_FAIL`) come from the
   organism's stream. The executed instructions then advance the
   instruction counter and the resource clock exactly as that many exact
   time slices would.
3. **Sequential phase.** The organisms that stopped run that one
   instruction through the exact code path (`World::process_step`,
   non-speculative), in the order of a random permutation of the cells
   drawn for the update from a stream of its own. An organism whose burst
   ended in a divide (see "Output tests and divides in the burst") runs
   only the world's half of it there (`World::commit_divide`).
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
- With demes, a deme whose bursts cross a replication threshold replicates
  at the end of that round, not at the crossing instruction, and a deme
  generation takes about one update longer than in the exact mode (see
  "Demes").
- The speculative statistics of the status line are zero and `SPECULATIVE`
  is ignored. The front ends' debugger steps whole updates in this mode.
- `SetConfig EXECUTION_MODE` during a run has no effect; the mode is read
  when the run starts.

### Unsupported, and where it falls back

Nothing is rejected at start-up any more (`NUM_DEMES` above 1 was, until
2026-11-13; see "Demes" below). These organisms or runs execute every
instruction through the sequential phase, so they gain nothing: hardware types other than the heads CPU and TransSMT
(experimental, GP8, BCR), organisms with a tracer or a mini,
micro, navigation or reproduction trace, `THREAD_SLICING_METHOD 1`,
promoters (heads CPU), constitutive regulation (heads CPU), implicit
reproduction, instructions with resource costs, the energy model (TransSMT),
and whole runs at `VERBOSITY` 3 or more (slip mutations
print) or with `TASK_SWITCH_PENALTY` (charged to the deme from the cost
check). Runs where the output tests or the divides cannot be organism-local
(see "Output tests and divides in the burst") still run their bursts, and
stop them before each `IO` or `h-divide`, as before that section.

### TransSMT in bursts

`HardwareTransSmt::burst_step` (hardware type 2; `hardware_transsmt.rs`) is
the burst counterpart of `SingleProcess` for `THREAD_SLICING_METHOD 0`.
The allow-list is `SMT_PARALLEL_METHODS` in `hardware/src/parallel.rs`: the
no-ops, `Val-*` arithmetic, `SetMemory`, `Inst-Read` (its copy mutation
draws from the organism's stream), `Inst-Write`, the `If-*` tests, the
`Head-*` instructions, `Search`, the `Push-*` and `Val-Copy`/`Val-Delete`
stack moves, the thread instructions (`Thread-Create`, `-Cancel`,
`-Kill`: threads are only marked) and `Call-Flow`, `Call-Label`, `Return`.
`IO`, the `Divide` family, `Inject`, `Apoptosis`, `Rotate-*` and
`Collect-Unit` stop the burst and run in the sequential phase. Parasites
need no special case: a parasite's threads, memory spaces and its entry in
the host's parasite list all belong to the host organism, so a burst runs
the parasite thread's organism-local instructions like the host's (the
`ParasiteUnit` is only read). An infection is an `Inject` of another
organism and so happens in the sequential phase.

Details that keep the schedule right: with `PARASITE_VIRULENCE` other than
-1 every step draws which of threads 0 and 1 runs. A burst step that draws
and then finds its instruction not on the list leaves the draw in
`pending_draw`, which the world's step uses instead of drawing again;
without that, the thread of a world instruction would be biased towards
whichever thread the burst's draw had not stopped on. The organism's last
step (`max_executed` reached, whose death reaches the world) and a step in
a state only the exact path reports (a thread-1 without owner) also go to
the world. The exact mode never sets `pending_draw` and never calls the
burst step. Organisms with `THREAD_SLICING_METHOD 1` or `ENERGY_ENABLED`
(whose cost check puts the organism to sleep through the world) stay
sequential. A gradient habitat's `SetToDie` is read in the sequential
phase only.

Statistical equivalence (2026-11-13): the stock host-parasite configuration
(`avida-lab/04-host-parasite`: 50 x 50, TransSMT, `PARASITE_VIRULENCE 0.85`,
parasites injected at update 600 into cells 0 to 399), seeds 101 to 116 in
each mode, 1500 updates, the mean of the last three samples of each run,
medians with interquartile ranges and Mann-Whitney p values:

| Outcome | Exact | Parallel | p |
|---|---|---|---|
| organisms | 2303 [2281, 2325] | 2300 [2286, 2329] | 0.61 |
| genotypes | 1562 [1538, 1578] | 1540 [1523, 1589] | 0.62 |
| average gestation time | 2671 [2604, 2739] | 2642 [2546, 2715] | 0.51 |
| average fitness | 0.1427 [0.1410, 0.1452] | 0.1436 [0.1423, 0.1463] | 0.51 |
| average generation | 18.90 [18.83, 19.05] | 18.95 [18.87, 19.38] | 0.56 |
| organisms with NOT | 2255 [2232, 2301] | 2273 [2236, 2288] | 0.84 |
| parasites | 1485 [1447, 1532] | 1490 [1443, 1543] | 0.84 |

Without the parasites (the same configuration with `events-no-parasites.cfg`,
16 seeds each) every outcome has p of 0.13 or more (organisms 2432 against
2430, p = 0.93). `crates/avida/tests/parallel.rs` repeats the comparison
at a size a test can run (30 x 30, 8 seeds, Welch z below 3.5 for each
outcome, with and without parasites), and checks that 1, 2 and 4 threads
write the same files.

### Demes (2026-11-13)

Every exact time slice also advances the resource clock of every deme, adds
the slice to the time used of the running organism's deme
(`cDeme::IncTimeUsed`) and checks that deme for implicit replication
(`DEMES_REPLICATE_CPU_CYCLES`, `_TIME`, `_BIRTHS`, `_ORGS`). The sequential
phase does all of that through `World::process_step`. For the bursts, the
parallel phase advances the deme resource clocks by the instructions the
round executed, and the next sequential phase starts with
`ParallelExec::settle_deme_bursts`: each burst's instructions go to its
deme's time used (normalized with the organism's merit after the burst),
and every deme a burst ran in is checked once, in deme order. The founders
of a replication get the share of the update left at the clock at which the
deme's bursts crossed the threshold (interpolated over the round).

Measured on the configuration of `demes_germline` (100 demes of 10 x 10,
germline replication, 300 updates, four seeds per mode, each trigger on its
own):

| Trigger | Replications, exact | parallel | Deme age at replication, exact | parallel |
|---|---|---|---|---|
| `DEMES_REPLICATE_CPU_CYCLES 3000` | 457 to 462 | 452 to 463 | 39.6 to 39.8 | 40.5 to 41.0 |
| `DEMES_REPLICATE_TIME 60` | 71 to 77 | 66 to 72 | 191.5 to 192.1 | 190.0 to 190.5 |
| `DEMES_REPLICATE_BIRTHS 20` | 261 to 266 | 261 to 268 | 65.8 to 66.6 | 67.1 to 67.4 |
| `DEMES_REPLICATE_ORGS 15` | 283 to 293 | 273 to 286 | 60.9 to 61.3 | 61.6 to 62.0 |

The counts agree. Under three of the four triggers a deme generation takes
about one update (1.5% to 3%) longer in the parallel mode, also under the
two triggers that only the exact code path fires, and with
`DEMES_REPLICATE_TIME` the demes replicate with about 7% fewer births
(320 to 335 against 344 to 355). Growth without replication does not differ
(1014 to 1099 organisms at update 60 against 1032 to 1083), and granting the
founders from the crossing clock instead of the end of the round moved the
deme age by 0.1 at most. What is left is, as far as these measurements
show, the bursts running ahead of "Statistical equivalence" above on a
larger scale: a replication kills two demes at once, and their organisms
have spent most of their update's share before the kill, which the exact
scheduler would have handed to the living. This attribution was not
isolated by an instrumented build. The other deme configurations of the
consistency suite (events that replicate, kill or compete demes, germlines,
migration, deme resources, the energy model) run in the parallel mode with
output that does not depend on the thread count; deme occupancy under
`SetMigrationRate` matched the exact mode over eight seeds.

`crates/avida/tests/parallel.rs::demes_replicate_on_the_time_their_bursts_used`
pins the thread independence and the replication counts for the two
triggers the bursts feed.

### Diagnostics and performance

`AVIDA_EXECUTION_STATS=1` in the environment prints one line at the end of
`World::run` to stderr: rounds per update, the share of instructions run in
the sequential phase, the newborn grants, the rescales, the carry-over,
bursts per update and the share run inline, and the wall time in
allocation, parallel and sequential phases. Since 2026-11-14 it also splits
the main thread's time: `begin_update`, the parallel phase outside the bursts
(taking the organisms out of their cells, the merge loop after the bursts),
and the sequential phase into its settle step, the divide commits, the other
steps and the tail. It is not a C++ output.

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

#### 120 seeds per mode (2026-11-13)

The 12 to 16 seed comparisons above and in the later subsections showed
generation or gestation time leaning the same way three times (raw p of 0.03
to 0.04). To tell a small systematic bias from noise, both modes were run on
120 seeds each (1001 to 1120, `tools/parallel_stats.py`, build of the commit
after 34ae22603 plus the cheaper commit):

| Outcome | Stock 60x60, 5000 updates: exact, parallel, p | 30x30 with two per-cell resources, 3000 updates: exact, parallel, p |
|---|---|---|
| generation | 999.6, 1005, 0.58 | 255.5, 254.7, 0.32 |
| gestation | 372.3, 379.4, 0.69 | 345.3, 349.8, 0.80 |
| log10 fitness | 3.38, 3.23, 0.16 | -0.542, -0.538, 0.77 |
| genome length | 102.4, 102.8, 0.79 | 103.1, 103.5, 0.21 |
| first NOT | 535.5, 526, 0.67 | 537.5, 573, 0.18 |
| first NAND | 690.5, 709, 0.52 | 724, 795, 0.65 |
| tasks held | 6, 6, 0.27 | 0, 0, 0.95 |
| organisms | 3592, 3592, 0.47 | 3598, 3598, 1.00 |
| instructions in the last update | 1.078e5, 1.078e5, 0.16 | 1.079e5, 1.080e5, 0.12 |

Medians; two-sided Mann-Whitney U. No outcome is below 0.12 in either world,
and the earlier lean in generation and gestation does not reappear. With 120
seeds per mode a shift of about 0.37 standard deviations would be found with
80% power, so a smaller bias is not excluded. Not covered: transsmt and
parasites, demes, and worlds larger than 60x60.

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

#### Scaling work of 2026-11-13

Measured with the counters of `AVIDA_EXECUTION_STATS` (they now also give the
time of the end-of-update statistics and of the rest of the end of an update,
and the bursts run on the pool against inline) and `sample` on the main
thread. At 4 to 8 threads the main thread spent 45 to 60% of its time in the
sequential phases (divides, and the one `IO` per gestation that reaches the
world), 25 to 30% waiting for bursts, and 10 to 15% in the statistics; the
workers' bursts reached 3.5x (4 threads) to 4.3x (8 threads) of one thread,
so the sequential phase and the end of the update are the limit. The output
of a seed did not change with any of these steps (byte-identical data files
before and after, for 1, 4 and 8 threads on the stock configuration and for 4
on `10-continents`).

| Change | Effect |
|---|---|
| The sums of `update_organism_stats` by column: after the gather, every sum (and every group of eight message counts, and the task and reaction counts) takes the organisms in `live_org_list` order on a task of its own, so each ends with the bits of the walk. Gather and sums share one entry to the pool, the gather loads a block of eight organisms before working on them. | statistics 3.0 s to 2.4 s of a 17 s run (stock, 4 threads) |
| Plain grid resources update at once: with two or more grids of at least 4096 cells, `ResourceCount::do_updates` hands the pending per-update flows of the plain grids to an executor (`SpatialExecutor`, the pool), one job per resource. A plain grid draws no random numbers and calls nothing outside its own cells, so the amounts are the same bits (`executor_gives_the_same_amounts_as_the_plain_loop`). The exact mode has no executor and runs the old loop. | `10-continents` (nine 160 x 160 grids): the first `IO` of every update waited for 9 flows on the main thread, 26% of its time; wall time of 1000 updates at 4 threads 14.5 s to 11.0 s |
| Deleted organisms are freed on a worker (`Population::graveyard`, off while a parasite unit ever existed, since a unit's drop queues a systematics change) | 0.3 s of 17 s |

Tried and left out: finer or coarser burst chunks (1 to 64) and pool
thresholds (128, 1024) changed the pool time by less than the noise;
splitting the sum tasks further; finding the cells of newborns from the
placement instead of a scan of the cells (the scan costs 0.1 s of 17 s on the
stock configuration and 1.2 s of 82 s on `10-continents`, which does not pay
for a second path that must see every way an organism is placed).

#### Output tests and divides in the burst (2026-11-13)

What was left of the sequential phase after the steps above was two things,
both the exact code path: the one `IO` of each gestation (a task test through
the world) and the `h-divide` (`Divide_Main`, `ActivateOffspring`). Profile of
the main thread at 4 threads (`sample`, stock configuration): 34% of its time
in the sequential phase, 80% of that in the divide, 16% of the thread's time
in the output test. Most of both is organism-local, so the heads CPU's
handlers now run in the burst, and the sequential phase keeps what must see
the whole population.

**Output tests** (`hardware/src/local_output.rs`). `IO`, `IO-Feedback`,
`IO-bc-0.001`, `get`, `put`, `get-stack` and `load-stack` (`CPU_PARALLEL_IO_METHODS`;
the instruction set marks them `STALL`, which a burst overrides for these
only) run in a burst when `output_is_local` holds for the run: the
environment has no resources (or only per-cell ones, see "Per-cell resources
in the burst"), state grids or neighbour input and output;
every reaction's task is a pure function of the organism's buffers
(`TaskLib::is_organism_local`: the logic, arithmetic, `fib`, `all-ones`,
`royal-road` families); the reaction's processes only add to or multiply the
bonus (no resource, product, detection, lethality, sterilisation, germline
share, deme fraction, triggered instruction or `phenplastbonus`); and the
run has no avatars, energy model, task-switch penalty or immediate merit
update. The stock environment qualifies. `do_output_local` is the body of
`cOrganism::doOutput` on a shared borrow of the environment
(`Environment::test_output_shared`: the same loop as `test_output`, which
needs `&mut self` only for `xor-max` and the plasticity test) and the
cell's inputs (`LocalWorld::cell_inputs`, read through the organism's own
`input_pointer`). What the exact path adds to the world's statistics and to
the organism's deme (`AddNewTaskCount`, `AddOtherTaskCounts`,
`AddNewReactionCount`, `AgeTaskEvent`, the deme's task and reaction counts)
the organism writes as `OutEvent`s into its own list, and the world replays
them in cell order after the parallel phase (`replay_out_events`), so the
statistics end with the same counts as with the output tests in the
sequential phase (`local_output_test_matches_the_world_path`). The output
test draws no random numbers.

**Divides.** `h-divide` and its multiplier variants
(`CPU_PARALLEL_DIVIDE_METHODS`) run in a burst when `divide_is_local`
holds as well (output tests local; no groups, habitat requirements, test
CPU on divide or for sterilisation, epigenetic inheritance, precalculated
phenotypes, input reset on divide, tolerance, inherited reputation, genotype
file, gene transfer or forage target at birth) and the organism cannot
die of age or be marked to die, has no additional time cost for the
instruction and no lateral gene transfer rate (all of which the exact order
would resolve after resetting the organism, so such a step stays in the
sequential phase). The handler runs completely: viability checks (a failed
divide is now an ordinary step of the burst), the copy mutations (from the
organism's stream), the last output test with its `on_divide` flag, and the
parent's own tail (`full_reset`, `clear_flags`). Where the exact path
calls `m_interface->Divide`, `LocalInterface::divide_with_traces` builds the
offspring's organism (`Organism::new`, a quarter of the divide) and keeps it
with the offspring's genome in the parent's `ParallelOrgState::pending_birth`
(`PendingBirth`); it answers that the parent lives, and the burst ends with
the divide as its last instruction. The sequential phase then calls
`World::commit_divide` instead of stepping the organism: the parent goes out
of its cell as in `process_step`, `ActivateOffspring` runs, the birth chamber
adopts the prebuilt organism where it would have built one
(`adopt_prebuilt_org`: the world numbers it, and its generation is the
parent's after `DivideReset`, which has run by now), and the step's epilogue
follows (the parent dies if it is to die, or if the offspring replaced it).
The burst counted the instruction; the commit does not. Commits go in the
update's permutation order like the other sequential steps.

The only reorderings against the exact path are the parent's tail and the
parent's `time_used` check, both before `ActivateOffspring` instead of
after; the gating above is what makes them unobservable (`ActivateOffspring`
reads of the parent's hardware are `EPIGENETIC_METHOD` and
`PRECALC_PHENOTYPE`, both excluded). With demes, the deme's normalized time
of the step in which an organism divides uses its merit before the divide,
not after.

**What stays sequential, and why.** The commit is `ActivateOffspring` less
the construction of the organism: `DivideReset` of the parent (draws from the
world's generator and adds to its statistics), the birth chamber's
classification of the offspring in the systematics (shared genotype tables),
`PositionOffspring` (world generator, the occupancy of the cells, the
neighbourhood), the kill of the cell's previous occupant (systematics,
statistics, schedule) and the placement (cell inputs drawn from the world
generator, schedule, birth statistics). None of it can run before the
previous commit of the round has finished. Also still sequential: `get2`
and `put-reset` (redraw the cell's inputs), divides of the resampling and
sex families, and, in runs whose environment has non-pure tasks or
resources that are not per-cell grids, gradients that do not act on
organisms, or global or `PARTIAL` pools, every output test (the burst stops
before the instruction as before).

**Result.** One run of the stock configuration, 3000 updates, seed 42,
release build, one run at a time, interleaved (load average 6 to 8 from
desktop applications; a range is the spread of the runs, three at 4 and 8
threads, two at 1):

| Threads | Before | After | Sequential phase before, after |
|---|---|---|---|
| 1 | 28.5, 29.0 s | 29.2, 29.9 s | |
| 4 | 16.6 to 17.7 s | 13.0 to 13.6 s | 7.7 s (54%), 3.1 s (30%) |
| 8 | 15.9 to 16.2 s | 11.8 to 12.4 s | 8.0 s (60%), 3.2 s (34%) |

The sequential phase is the commits now (151 per update, 6.8 microseconds
each) and the instructions that stop for other reasons; it was 1.8% of the
instructions in 33 rounds per update and is 2 rounds per update (at most 5):
a round now ends in the divides it ran, not in each `IO`. One thread is 3%
slower than before; not investigated (the burst now carries an event list
for the output tests and one more copy of the offspring's genome per divide,
and with one thread nothing waits for the sequential phase, so there is
nothing to win there). Eight threads gain 9% over four (12.0 against
13.2 s): what remains is the commits, 3 s, the statistics at the end of the
update, 2.1 s, and the bursts, which cost about 60 ns per instruction on one
thread and 90 ns per instruction and core with four, on a machine whose
performance cores the desktop was also using. The
output of a seed is identical for 1, 4 and 8 threads (compared file by
file, `crates/avida/tests/parallel.rs`); it is not the output the parallel
mode wrote before this change (the divides draw from the organisms'
streams, and a round ends elsewhere), so the statistics comparison was
repeated:

| Outcome at update 5000 (16 seeds each) | Exact | Parallel | p |
|---|---|---|---|
| log10 average fitness | 3.42 [2.56, 3.54] | 3.30 [2.83, 3.85] | 0.81 |
| average genome length | 102.2 [100.0, 105.1] | 104.0 [99.6, 110.9] | 0.54 |
| tasks held by at least 5% of the organisms | 6 [5, 6] | 6 [5.75, 7] | 0.18 |
| first update with NOT | 527 [436, 609] | 607 [440, 689] | 0.29 |
| first update with NAND | 650 [534, 856] | 654 [562, 843] | 0.68 |
| average generation | 1062 [954, 1146] | 959 [842, 1104] | 0.13 |
| average gestation time | 371 [361, 383] | 385 [375, 423] | 0.032 |
| organisms | 3593 [3592, 3594] | 3592 [3590, 3593] | 0.080 |
| instructions in the last update | 107,800 | 107,800 | 0.93 |

Nothing is significant after the correction for nine outcomes (the
smallest, 0.032, becomes 0.29 with Holm's). The build before this change on
the same seeds gave 0.039 for the average generation (941 against 1062) and
0.93 for the gestation time; between the two parallel builds the smallest p
is 0.08 (gestation time). The statistics test of the crate repeats the
comparison for the heads CPU at 30 x 30 over 8 seeds
(`heads_cpu_with_bursts_for_divides_matches_the_exact_mode_statistically`).

Considered and not built: running the placement (`PositionOffspring`, the
occupant's kill) speculatively in the burst, which needs the world's
occupancy and generator; and a second pipeline stage that commits round `r`
while the workers run the bursts of the organisms that did not divide, which
in this schedule are finished by round 1.

Where further speed would come from: the commit (3 s of 13), whose largest
parts in the profile are the placement and the kill of the occupant, and
the classification of the offspring and the release of the occupant's
genotype in the systematics; the statistics at the end of the update
(2.1 s), whose sums already take one task per column; and the burst itself,
which does not scale beyond the performance cores of the machine.

#### Per-cell resources in the burst (2026-11-13)

Environments with resources used to keep every output test in the
sequential phase. When every resource of the population is a plain spatial
grid (`ResourceCount::are_plain_cell_grids`: no global or `PARTIAL`
resource, no gradient, a grid cell for every world cell) the output tests
run in the burst too. The gate is `output_is_local(cfg, env, cell_resources)`
with the extra conditions: no deme, energy, HGT, deadly or predatory
resource, no resource bins, no `internal` or `detect` process, and every
`resource` and `product` of a process an index of the library. It is decided
each update. The `10-continents` configuration qualifies.

Why it is sound. An organism's output test reads and changes only the
amounts in its own cell. During a parallel phase nothing else changes a
cell's amount: grids advance once per update of the world (the lazy
`DoUpdates` has no global resource to advance by the resource clock),
other organisms' output tests touch their own cells, and every instruction
that reaches another cell (`collect`, movement, sensing) is outside the
burst. So:

1. At the start of each parallel phase the world brings the grids up to date
   (`update_resources`, the call the first output test of the update would
   make) and lends the bursts a read-only view (`CellResources`,
   `ResourceCount::frozen_cell_amounts`).
2. The first output test of a burst copies the cell's amounts into the
   organism's `LocalInterface` and evaluates against them
   (`res_count`); after the test it folds `produced - consumed` into that
   copy with the arithmetic of `ModifyCell` (`amount += 0.0 + change`), so a
   second or third test of the same burst sees what the first took, to the
   bit. A change that is not all zero is also written as an
   `OutEvent::ResourceChange`.
3. After the phase `replay_out_events` applies the changes in cell order
   (`ResourceCount::modify_cell_updated`, `ModifyCell` without the lazy
   update) before the sequential phase runs, so a `collect` or an output test
   of a world instruction in the same round sees them. Cells are independent,
   so the order cannot change an amount; the thread count does not enter.

`cell_resource_output_tests_match_the_world_path` runs six output tests
(repeated NOT, a byproduct, two resources) through the world and through one
burst interface and compares the phenotype after each test and the cell's
amounts bit for bit. Resource totals are conserved as in the exact mode because
the same change is applied to the same cell by the same arithmetic; no cell can
go below zero (`consumed` is capped by the level, as in `DoProcesses`;
`cell_resource_world_does_not_depend_on_the_thread_count` checks it).

What stays sequential, and why: the global pools of environments that the
section on global pools below does not admit (a world of grids and pools
together is handled in "Mixed grids and global pools in the burst"; gradients
and `PARTIAL` resources in "Reactions on both kinds, PARTIAL resources and
gradients in the burst").

**Result** (`10-continents` configuration, seed 1, release build, one run at
a time, interleaved, load average 6.5 to 9 from desktop applications):

| Run | Before | After |
|---|---|---|
| 1000 updates, 4 threads | 8.36, 8.38 s in updates | 6.47, 7.73 s |
| 1000 updates, 8 threads | 7.56, 8.89 s | 5.70, 6.82 s |
| 3000 updates, 4 threads | 117.3 s (66.6% of it sequential phases, 23 rounds per update) | 77.6 s (29.5%, 2.0 rounds) |
| stock configuration, 3000 updates, 4 threads | 12.0, 12.2 s | 12.1, 12.4 s |

The sequential share left on the continents world is the divide commits (196 per
update at update 1000), as on the stock configuration. At 1000 updates the world
holds about 4600 organisms and the gain is a fifth; the full world gains a third.
The output for a seed is identical for 1, 4 and 8 threads (every data file,
the grids' `.m` files and the population save compared at 1000 updates). It
differs from what the build before this change wrote.

Equivalence, 12 seeds in each mode, 3000 updates, 30 x 30 world with the two
resources of the unit test (`tools/parallel_stats.py`; the exact mode against the
parallel mode at one thread):

| Outcome | Exact | Parallel | p |
|---|---|---|---|
| log10 average fitness | -0.534 | -0.561 | 0.078 |
| average genome length | 103.1 | 104.1 | 0.55 |
| first update with NOT | 518 | 563 | 0.27 |
| first update with NAND | 599 | 713 | 0.69 |
| average generation | 262 | 243 | 0.033 |
| average gestation time | 338 | 356 | 0.068 |
| organisms | 3598 | 3598 | 0.26 |
| instructions in the last update | 107,900 | 108,100 | 0.14 |

Nothing is significant after Holm's correction (the smallest, 0.033, becomes
0.30 for nine outcomes; tasks held is 0 in both modes, as the resources limit
the rewards). The generation and gestation time lean the same way as in the
stock configuration's comparison above; that run is the reference for whether it
is a real shift. The crate's `cell_resources_in_bursts_match_the_exact_mode_statistically`
repeats a smaller comparison with the loose |z| < 3.5 bound.

#### Global resource pools in the burst (2026-11-14)

A global (well-mixed) resource is one amount that every organism reads and
consumes, so no organism can evaluate its output test against a copy of it:
what an organism consumes (`frac` of the level the test finds, capped by `max`,
scaled by the task quality) depends on everything the organisms before it
took, and the bonus the phenotype receives depends on what was consumed. A
scheme that lets every organism consume from a snapshot of the pool was
rejected: with the classic environment (`frac=0.0025`) 400 organisms that
complete the task in a round take the whole snapshot between them, and the
thousands a full world has overdraw it several times, so that either the
organisms are paid for resource that does not exist or their bonuses have to be
taken back after the burst has gone on (a `pow` or `mult` bonus cannot be undone
by a later one). The pool's half of the test is therefore not approximated: the
world runs it, one organism after another, with the arithmetic of the exact
path.

The test is split where it depends on the pool:

1. In the burst (`do_output_local` with `LocalOutput::pending`), `TestOutput`
   runs as in a world without resources, up to the processes of the reactions
   that use a resource (a `resource` or a `product`). The tasks read only the
   organism's buffers. Those reactions are recorded as
   `DeferredReaction { reaction, task_quality, task_count }` and the
   `ReactionResult` so far is kept as the organism's `PendingOutput`. A test
   that triggers no such reaction is applied at once, as in a world without
   resources. After an instruction that left a `PendingOutput` the burst stops
   (`needs_world`, also when its budget is spent): what the organism does next,
   its next output test and a divide, reads what the pool's half writes.
2. At the start of the sequential phase (`ParallelExec::settle_pool_tests`)
   the world takes the pending tests of the organisms that stopped, in the
   update's permutation order and before any other step of the phase, so that
   no step can kill an organism whose test is pending. For each: the
   pool's amounts now (`ResourceCount::get_resources`, which first advances
   the pool by the time the round used, as the first output test of a round
   does in the exact mode), the processes of the deferred reactions in library
   order (`Environment::finish_deferred`, which runs `DoProcesses`
   itself), `Phenotype::apply_test_output` with the world's own statistics
   and deme sinks, and the change (produced less consumed) applied to the pool
   (`ModifyCell`, as `cOrganism::doOutput` does). The organism, which executed
   its instruction in the burst, is marked as having run in this phase and
   goes to the next round with its remaining budget.

Admitted (`output_is_local_global`, with `ResourceCount::are_global_pools`):
every resource a global pool (no grid, gradient or `PARTIAL` list), and the
conditions of the per-cell case (no deme, energy, HGT, deadly or predatory
resource, no bins, `detect`, `internal` or random-resource process). Two more,
because a deferred reaction is neither triggered nor counted when the next
reaction of the same test is examined: no requisite that names other reactions
(`reaction`, `noreaction`, total counts), and none that lets a reaction fire on
divide (`divide_only`), whose test would leave the pool's half to the world
with the divide under way. A reaction's requisites on its own count
(`max_count`) and on its task's count are exact: the next test comes after
the pool's half. With the burst running the output tests, divides run in it too
(`divide_local`).

What is the same as the exact mode: the arithmetic of every test (the pool is
read and changed by the same code, bit for bit, for the same sequence of tests:
`global_pool_output_tests_settle_as_the_exact_path_does` runs 4500 tests of
900 organisms on two pools that run dry in both paths and compares each
phenotype and each pool amount to the bit), the pool never below zero
(`consumed` is capped by the level), and the amount that leaves the pool being
the amount the organisms consumed (the same test checks the sum).

What differs, and by how much. In the exact mode the tests of an update come in
the order of the time slices, interleaved with every other organism's
instructions, and the pool's inflow and outflow advance between any two tests.
Here the tests of one round come at its end, in the order of the update's
permutation (every update has a fresh one, so no organism is always first),
and the pool advances by the round's time between rounds. The difference is
bounded by what the pool does in a round: with the classic environment (inflow
100 and outflow 0.01 per update) a round is a fraction of an update and the
pool moves by that fraction of its inflow. A round ends at each organism's
next pool-consuming test, so an organism cannot perform two such tests in one
round.

**Result, global pools** (`environment-9resource.cfg`, now `misc/environment-9resource.yaml`, the classic limited
environment with nine pools of inflow 100 and outflow 0.01, stock `default-heads.org`, 60 x 60
world, 2000 updates, seed 42, release build, one run at a time, load average 5
to 7 from desktop applications):

| Run | Before | After |
|---|---|---|
| exact mode | 30.5 s | 29.9 s (unchanged) |
| parallel, 1 thread | 20.6 s | 18.4 s |
| parallel, 4 threads | 12.5 s | 7.7 s |
| rounds per update | 26.9 | 3.6 |
| sequential share of the time in updates, 4 threads | 63.3% | 28.3% |
| commits per update | 0 | 141 |

The output for a seed is identical for 1, 2 and 4 threads (the 60 x 60 run
compared for 1 and 4; `global_pool_world_does_not_depend_on_the_thread_count`
for 1, 2 and 4 on the 30 x 30 world). The sequential phase that remains is the
pool's half of the tests (181 per update here), the divide commits and the
world-level instructions.

Equivalence, 12 seeds in each mode, 3000 updates, 30 x 30 world with the same
environment (`tools/parallel_stats.py`; the exact mode against the parallel
mode at one thread; medians, Mann-Whitney p):

| Outcome | Exact | Parallel | p |
|---|---|---|---|
| log10 average fitness | 0.714 | 0.779 | 0.55 |
| average genome length | 104.3 | 103.2 | 0.93 |
| tasks held by at least 5% of the organisms | 6 | 6 | 0.98 |
| first update with NOT | 997 | 819 | 0.42 |
| first update with NAND | 1128 | 948 | 0.67 |
| average generation | 388 | 398 | 0.55 |
| average gestation time | 377 | 371 | 0.67 |
| organisms | 900 | 898 | 0.001 |

The one outcome that differs is the number of organisms at the end: two cells
of 900 empty against none or one. It is of the kind "Statistical equivalence"
above traces to how long an emptied cell stays empty in the parallel mode
(3590 against 3593 organisms on the stock configuration, p = 0.031). The
divides of these worlds now commit at the round's end as the others do; the
build before this section, which ran every divide in the sequential phase, had
899 against 900 (p = 0.10) on the same seeds. That this is the same cause was
not traced (no empty-cell count was taken here).

#### Mixed grids and global pools in the burst (2026-11-14)

A world whose resources are some plain per-cell grids and some global pools
used to stop every burst at an output test (17.9 rounds per update and 54%
of the update loop in the sequential phase on the benchmark below). It now
runs as the two sections above do, each kind where it can be: the grids'
processes in the burst against the organism's own cell, the pools' processes
in the sequential phase against the pool.

The gate. `ResourceCount::grid_or_global_kinds` says, per resource, whether
it is a pool, and refuses the world if any resource is a grid without a cell
for every cell of the world (since 2026-11-14 a `PARTIAL` list counts as a
pool and a gradient that does not act on organisms as a grid, see the next
section). The
world uses it when there are both kinds (all one kind keeps the two gates
above, unchanged). `output_is_local_mixed` then asks what
`output_is_local_global` asks (no requisite that names other reactions, none
on divide, the per-cell conditions). A reaction is entirely in the burst or
entirely deferred, which is what makes the split sound: the processes of
different reactions meet only in `ReactionResult`'s sums, and no process
reads what a reaction of the other kind consumed (`consumed(res)` is
per-resource, and a resource is of one kind). (Until 2026-11-14 the gate
also refused a reaction that uses both kinds; see the next section.)

What runs where. `test_output_shared` defers a triggered reaction only if
one of its processes uses a resource `pools` marks (`pools: None` is the old
"any resource"); the others run in the burst against the organism's copy of
its cell's amounts, which `frozen_cell_amounts` fills (a pool reads 0 there:
nothing in the burst reads it). A test that triggers no pool reaction is
finished in the burst, as for grids alone, and its change to the cell goes
to the world as an `OutEvent::ResourceChange`. A test that does trigger one
becomes a `PendingOutput` with the grid reactions' consumption and
production already in its result; the burst stops. At the start of the
sequential phase the world gets the amounts the organism's cell sees
(`get_cell_resources`: the pools' and the cell's grids), runs the deferred
processes, applies `Phenotype::apply_test_output`, and applies the change of
the whole result (grid and pool parts) with `modify_cell`, which is what
`doOutput` does. The grids are brought up to date at the start of every
parallel phase, as for grids alone (`update_resources`; that call also
advances the pools by the clock time accumulated so far, which in a world
of pools alone happens at the first settled test).

Arithmetic. A test that triggers one reaction of each kind with an `add`
process each can add the two bonuses in the other order than the exact mode
(the burst's first), which differs in the last bit at most; `mult` and `pow`
bonuses and the sums per resource are independent of the order.
`mixed_resource_output_tests_settle_as_the_exact_path_does` runs six tests for
each of 3,600 organisms (a NOT test triggers a reaction on the grid and one
on the pool, a NAND test the pool's, an ORN test the grid's; the pool runs
dry) through the exact path and through a burst plus the world's half, and
compares each phenotype and the amounts in the pool and in the organism's
cell bit for bit; two thirds of the tests wait for the world.

Timing (benchmark: the 9-resource chemostat with four pools and five
per-cell grids, stock 60 x 60 world, 1000 updates, seed 42, release build,
one run at a time; load average 10 to 11, the maintainer's own six-thread run
on the machine, so only the ratios mean anything):

| Run | Before | After |
|---|---|---|
| 4 threads | 6.52 s (17.9 rounds/update, 54% sequential) | 3.90 s (2.1 rounds, 20%) |
| 8 threads | 4.81 s | 3.42 s |
| 1 thread | | 7.87 s |

The output for a seed is identical for 1, 4 and 8 threads (every data file
compared; `mixed_resource_world_does_not_depend_on_the_thread_count` does
it for 1, 2 and 4 on a full world). It is not what the build before wrote
for such a world (the tests settle at the end of a round, as in the global
pools), so the statistics comparison was rerun, 36 seeds in each mode,
3000 updates, a 30 x 30 world with one grid and one pool (NOT has a reaction
on each; `tools/parallel_stats.py`, the parallel mode at one thread; medians,
Mann-Whitney p):

| Outcome | Exact | Parallel | p |
|---|---|---|---|
| log10 average fitness | -0.437 | -0.515 | 0.074 |
| average genome length | 104.4 | 101.8 | 0.145 |
| tasks held by at least 5% of the organisms | 2 | 2 | 0.64 |
| first update with NOT | 739 | 809 | 0.36 |
| first update with NAND | 1044 | 1294 | 0.39 |
| average generation | 294.7 | 286.2 | 0.55 |
| average gestation time | 383.4 | 367.3 | 0.14 |
| organisms | 899 | 899 | 0.30 |
| instructions in the last update | 26,970 | 26,920 | 0.81 |

Nothing is significant (the smallest, 0.074, is 0.67 after Holm's correction
for nine outcomes). With the first 12 seeds alone three outcomes had p between
0.017 and 0.04 (genome length, first NAND, gestation time) and went away
with the other 24, so 12 seeds are too few for this environment, whose runs
scatter widely; the build before this change, 12 seeds, had no p below 0.14.
The crate's `mixed_resources_in_bursts_match_the_exact_mode_statistically`
repeats a smaller comparison with the loose |z| < 3.5 bound.

#### Reactions on both kinds, PARTIAL resources and gradients in the burst (2026-11-14)

Three worlds that kept every output test in the sequential phase now run in
the bursts: those with a reaction that uses a grid and a pool, those with
`PARTIAL` resources, and those with gradient resources that do not act on
organisms.

**A reaction on both kinds.** `DoProcesses` reads the amounts it is given
once per test (`res_count` is not updated between reactions or processes),
and its only other state is `ReactionResult`: what the result already holds
of the resource (`consumed(res)`), and the bonus sums. So a reaction that
takes from or gives to a pool waits for the world as a whole, whatever else
it uses, and the grid half of it is not split off. The waiting reactions run
at the settlement after the reactions that ran in the burst, with the
burst's result as their starting point. Two things make that equal to the
exact mode's library order. (1) The grids it reads are the amounts the test
saw: `PendingOutput::grid_levels` holds the burst's copy of the cell's
amounts, and `settle_levels` puts them in place of the cell's amounts at the
settlement (the pools read the pools as they are then). A reaction that takes
from a grid and gives to a pool, or the reverse, therefore computes what the
exact mode computes. (2) `DoProcesses` subtracts what the result already
holds from the amount a process takes from, so two reactions that take from
the same grid meet in library order. `output_is_local_mixed` refuses a world
in which a waiting reaction takes from a grid that a reaction that does not
wait, and comes later in the library, also takes from (the waiting one would
run second). The bonus sums of the two halves are added in the order of the
settlement (the burst's reactions first), which may differ in the last bit
from the library's for two `add` bonuses; `mult` and `pow` bonuses and the
sums per resource do not depend on the order.

**`PARTIAL`.** In the exact mode a `PARTIAL` resource is a pool (its
geometry is not spatial: `fill_curr_grid_res_cnt` hands out the one global
amount, `ModifyCell` changes it) whose amount `doOutput` sets to 0 for an
organism whose cell is not on the resource's list (`cell_id_lists`), both for
reading and, because `consumed` is then 0, for taking. Producing into it is
not masked. So `are_global_pools` and `grid_or_global_kinds` count it as a
pool, and the settlement applies `ResourceCount::mask_partial` to the
amounts the deferred processes read. Nothing else differs from a pool.

**Gradients.** A gradient (`SpatialResource::Gradient`) moves, plateaus and
refills only in its update, which runs once per update of the world at the
start of the parallel phase (`update_resources`, as for grids); during the
phase an organism's output test reads and changes its own cell as with a
plain grid (`ModifyCell` is the same function, and it sets the gradient's
`modified` flag, which the next update reads, when the replay applies the
change). `ResourceCount::are_cell_grids` and `grid_or_global_kinds` accept
one unless its update calls back into the population
(`GradientCount::acts_on_organisms`: predatory, deadly or damaging; the
environment gate refuses `damage` too, as it refused `deadly` and
`predator`). Those stay in the sequential phase, because the callbacks
kill or injure the organisms of the cells, which in the parallel phase are
out of the population in the workers' hands; running the update before
the bursts would also fire them at another time than the exact mode's lazy
update, at the first output test of the update. One thing a gradient has that
a plain grid does not: `StateAll` does nothing for it, so inflow and flow
leave a pending change (`delta`) in the cells that `ModifyCell` folds into
the amount with the organism's own change. The burst's copy of the cell
follows that (`CellResources::cell_deltas`, `ResourceCount::frozen_cell_deltas`;
the copy takes `delta + change` and clears the delta, as the cell does), so a
second test of the burst reads what `ModifyCell` would have made of the
first. Differences from the exact mode: a world whose organisms do no output
test for some updates has the gradient's updates lumped into the next test
there, and run one by one here (every update, the first phase); the world's
random stream is drawn from at other moments in either case, as everywhere
in the mode.

Tests (`crates/avida/tests/parallel.rs`):

| Test | What it pins |
|---|---|
| `reactions_on_grids_and_pools_settle_as_the_exact_path_does` | 21,600 tests (six for each of 3,600 organisms) on a grid and a pool with NOT (grid to pool), NAND (pool to grid) and ORN (grid): each phenotype and every amount in the cell identical to the bit with the exact path; the pool runs dry |
| `partial_resource_output_tests_settle_as_the_exact_path_does` | the same for two `PARTIAL` pools on halves of the world, and for one next to a grid; a mutant without `mask_partial` fails it |
| `gradient_and_pool_output_tests_settle_as_the_exact_path_does` | a gradient taken from by NOT and given to by NAND with a pool |
| `gradient_output_tests_in_one_burst_match_the_world_path` | five tests of one burst on the gradient's richest cell with a pending change of 0.375 in it; a mutant that ignores the deltas fails it |
| `mixed_gate_keeps_the_library_order_of_the_reactions_taking_from_a_grid` | the gate takes the one library order and refuses the other |
| `*_world_does_not_depend_on_the_thread_count` (three) | 1, 2 and 4 threads write the same files for each kind of world |
| `*_in_bursts_match_the_exact_mode_statistically` (three) | six seeds, loose |z| < 3.5 bound; the output tests ran in the bursts |

**Statistics** (20 seeds per mode, seeds 1 to 20, 30 x 30, 1500 updates,
`not-ancestor.org`, parallel mode at one thread; medians and
Mann-Whitney p; "tasks" is the sum over the tasks of the number of
organisms performing them at the last update):

| World | Measure | Exact | Parallel | p |
|---|---|---|---|---|
| grid and pool, NOT and NAND on both (`both`) | organisms | 899 [897.75, 900] | 899 [898, 900] | 0.867 |
|  | tasks_total | 616 [534.25, 717.5] | 624.5 [525, 736.5] | 0.904 |
|  | mean_fitness | 0.30158 [0.29695, 0.32969] | 0.30629 [0.30197, 0.39024] | 0.121 |
|  | mean_merit | 112.15 [109.58, 122.86] | 113.99 [111.95, 238.63] | 0.192 |
|  | gestation | 373.67 [369.13, 380.29] | 373.81 [366.34, 392.44] | 0.841 |
|  | resource0 | 29.239 [29.064, 29.67] | 29.023 [27.969, 29.503] | 0.253 |
|  | resource1 | 32.168 [24.102, 44.92] | 25.528 [21.673, 35.157] | 0.211 |
| two `PARTIAL` pools (`partial`) | organisms | 899 [898.75, 899.25] | 900 [898, 900] | 0.418 |
|  | tasks_total | 844.5 [808.5, 1065.5] | 821.5 [740.25, 877.5] | 0.183 |
|  | mean_fitness | 0.39437 [0.37945, 0.55545] | 0.38421 [0.37431, 0.39835] | 0.301 |
|  | mean_merit | 150.59 [136.36, 204.86] | 143.84 [137.92, 156.51] | 0.529 |
|  | gestation | 373.2 [367.31, 381.73] | 369.97 [363.81, 388.33] | 0.841 |
|  | resource0 | 54.922 [48.645, 67.901] | 43.612 [37.148, 51.489] | 0.002 |
|  | resource1 | 54.722 [44.684, 76.638] | 49.113 [37.895, 60.261] | 0.341 |
| control: the same two pools, global (`pools`; the path of the build before, unchanged) | organisms | 900 [899, 900] | 899 [898, 899] | 0.035 |
|  | tasks_total | 1067.5 [1022.8, 1235.5] | 1166 [1015, 1322.8] | 0.298 |
|  | mean_fitness | 0.37845 [0.37595, 0.39727] | 0.38878 [0.37994, 0.56959] | 0.142 |
|  | mean_merit | 140.59 [136.23, 160.23] | 143.88 [140.96, 211.01] | 0.076 |
|  | gestation | 367.59 [362.43, 373.68] | 370.04 [366.6, 375.99] | 0.304 |
|  | resource0 | 40.13 [32.62, 47.122] | 29.095 [23.752, 40.362] | 0.009 |
|  | resource1 | 37.02 [31.863, 44.531] | 35.844 [25.173, 47.273] | 0.602 |
| gradient (`gradient`) | organisms | 898 [897, 899] | 899 [898, 900] | 0.054 |
|  | tasks_total | 461.5 [438.5, 650.25] | 523.5 [439, 1212] | 0.495 |
|  | mean_fitness | 0.65065 [0.56933, 1.2662] | 0.65056 [0.6028, 1.4031] | 0.678 |
|  | mean_merit | 307.78 [213.48, 460.73] | 260.86 [232.87, 504.46] | 0.799 |
|  | gestation | 380.82 [372.96, 425.92] | 375.66 [364.63, 384.28] | 0.072 |
|  | resource0 | 2373.5 [1893.6, 4245.2] | 2627.1 [2162.8, 4095.7] | 0.583 |
| gradient and pool (`gradpool`) | organisms | 898 [897, 899] | 898 [896.75, 899] | 0.608 |
|  | tasks_total | 1086 [879.25, 1419.8] | 1146 [968, 1402] | 0.478 |
|  | mean_fitness | 1.0368 [0.74703, 1.6463] | 1.084 [0.83819, 2.0372] | 0.583 |
|  | mean_merit | 492.58 [303.22, 636.32] | 471.31 [297.01, 740.43] | 0.758 |
|  | gestation | 387.7 [359.21, 419.5] | 371.88 [359.08, 393.94] | 0.289 |
|  | resource0 | 2907.3 [1914.8, 4549.6] | 3353.1 [1873.6, 4640.2] | 0.583 |
|  | resource1 | 42.477 [31.726, 63.36] | 47.227 [39.935, 61.965] | 0.478 |

Thirty-four measures were compared; most are close, and three have p below
0.05: the first pool's amount in the `partial` world (54.9 in the exact mode
against 43.6, p = 0.002), the same in the control with global pools (40.1
against 29.1, p = 0.009; that path is the one of the build before, so the
effect is not the change of this section's), and the number of organisms
in the control (900 against 899, p = 0.035, the lingering empty cell of the
other sections); the gradient world's 898 against 899 (p = 0.054) is the same
kind. The lower first pool is a difference of the pool-settling scheme (the
tests of a round settle at its end and the pool advances between rounds,
"Global resource pools in the burst"), seen here at the pool NOT feeds on;
its cause was not traced further. With Holm's correction for the 34 measures
none of them is significant (the smallest, 0.002, becomes 0.068).

**Timing** (60 x 60, 2000 updates, seed 42, 4 threads, release build, one run
at a time, load average about 6 from other applications, so only the ratios
mean anything): 

| World | Before (output tests in the sequential phase) | After | Exact mode |
|---|---|---|---|
| grid and pool, NOT and NAND on both | 10.0 s (15.6 rounds/update) | 8.0 s (3.3 rounds, 0% sequential) | 28.8 s |
| two `PARTIAL` pools | 11.4 s | 6.9 s | 29.5 s |
| gradient | 9.6 s (33.5 rounds/update) | 6.5 s (2.0 rounds, 0% sequential) | 27.0 s |
| gradient and pool | 9.9 s | 6.9 s | 29.3 s |

#### Profile at 4 and 8 threads, and the cheaper commit (2026-11-13)

Where the time went before this step (stock configuration, 3000 updates, seed
42, `AVIDA_EXECUTION_STATS` and `sample` on the main thread; Apple M1 Pro with
the desktop running, load average 6 to 13):

| Part | 4 threads (12.3 s) | 8 threads (10.6 s) |
|---|---|---|
| bursts on the pool | 5.7 s | 4.2 s |
| divide commits (sequential) | 2.9 s | 2.9 s |
| end-of-update statistics | 1.9 s | 1.7 s |
| systematics at the end of the update | 0.78 s | 0.75 s |
| budget allocation (Poisson draws, one stream) | 0.6 s | 0.6 s |

The main thread at 8 threads: 36% waiting for bursts, 26% in commits, 15% in
the statistics (waiting for the pool), 7% in the systematics, 4% in the
allocation. Everything on the main thread but the waits is serial, so about a
third of the wall time does not shrink with threads. The bursts themselves
give 5.7 to 4.2 s for twice the threads: the sum of the bursts' own run time
(`busy`, measured with a counter in the burst closure, not kept) was 17.8 s on
one thread, 19.0 s on four and 21 to 23 s on eight, so the work per
instruction grows by a fifth to a quarter (shared cache and memory, and the
desktop taking cores); the wait for the first burst to start was 0.1 s (4
threads) and 0.3 s (8) over the run, and the wait from the last burst's end to
the return of `install` 0.1 s and 0.5 s. At 8 threads the pool was 66% busy
over its phases. The statistics are not compute bound at this size: the
longest column sum takes 0.07 ms, the phase 0.2 ms (spawning tasks to threads
that have gone to sleep), and the gather 0.3 ms per update; a cold pool entry
costs about 0.1 ms, which is why moving the Poisson draws to the pool (blocks
of 256 organisms, own streams) made the run slower (allocation 9.2% against
8.9%) and was dropped.

| Change | Effect | Parallel output per seed |
|---|---|---|
| Organisms moved by value in a commit (3.4 KB each; `memmove` was 31% of the commit's samples) are now `Box<Organism>` end to end: `PendingBirth::organism`, `Offspring::org`, `adopt_prebuilt_org`, `activate_organism`, and the graveyard keeps the box instead of taking it apart | commit 6.4 us to about 4.3 us | unchanged |
| `activate_offspring` no longer builds the `Genome` it hands the birth chamber when the chamber adopts a prebuilt organism (a copy of the sequence, a property map and a string, never read) | a few percent of the commit | unchanged |
| `GenotypeArbiter::perform_update` resets the threshold genotypes from a list kept for the purpose (`thresholds`) instead of walking every active list, and sweeps the historic list only while `prune_pending` is set (a released passive reference, a loaded genotype, or a sweep that removed or stopped); a sweep that would remove nothing has no effect, so the exact mode is unchanged | systematics 0.75 s to 0.36 s | unchanged |

Measured one run at a time, interleaved with the build before, load average 6
to 11:

| Run | Before | After |
|---|---|---|
| stock, 1 thread | 27.6, 27.7 s | 26.4, 26.9 s |
| stock, 4 threads | 12.2, 12.0, 12.1 s | 10.9, 10.7, 10.7 s |
| stock, 8 threads | 10.6, 10.6, 10.3 s | 9.1, 9.3, 9.7 s |
| `10-continents`, 1 thread | 205.6 s | 205.4 s |
| `10-continents`, 4 threads | 86.1, 87.1 s | 78.5, 78.2 s |
| `10-continents`, 8 threads | 70.3, 70.2 s | 63.6, 62.7 s |

The gain is the same at 4 and at 8 threads (about 11 and 12%): the sequential
share is what shrank, not the scaling of the pool, so 4 to 8 threads still
gives 1.15x (stock) and 1.24x (continents). The output of a seed is
byte-identical to the build before and for 1, 4 and 8 threads (every data file
and the continents' grid files compared). The reference suite is unchanged (85
same, 34 trivial, 1 UNSUPPORTED, 1 REF-TIMEOUT).

Where the remaining time is, and why it is mostly not more code: the commit
(about 2 s at 8 threads) is now dominated by the systematics' own structures
(the sparse lists' segment search on each change of abundance, 16% of the
commit; the genome comparison in the hash bucket; frees of genotypes), all
cache-miss bound and order-visible. The end-of-update statistics and the
bursts are bound by memory traffic and by the cores the desktop leaves, and
each pool entry costs about 0.1 ms. On `10-continents` the nine grids are nine
equal jobs, so 8 threads take two waves (4 threads three); a grid's flow is
order-dependent in its sums and cannot be split without changing bits.
Tried and left out, no gain: loading the next organism's memory before a burst
(slower, +3%), the calling thread working and spinning for the end of a burst
phase instead of sleeping on the latch (0.8% at 4 threads, none at 8),
Poisson draws on the pool (slower, see above). Not tried: running the
end-of-update systematics beside the statistics gather (the data-file actions
between them may read the systematics), and committing round r while the
bursts of the rest run, which needs the occupancy of cells whose organisms are
out on workers.

#### The 4-to-8-thread ceiling, measured (2026-11-14)

Where the time of 1000 updates of the stock configuration (60 x 60, seed
42) goes at each thread count, from the counters of `AVIDA_EXECUTION_STATS`
(now with the main thread's time in `begin_update`, in the parallel phase
outside the bursts, and in the parts of the sequential phase). Seconds for
1000 updates are also milliseconds per update. Release build, one run at a
time, the machine's other load 5 to 7 (11 later in the day: the maintainer's
own six-thread simulation ran beside it), so the third decimal is noise.
The population grows to 3600 organisms over the first 150 updates; the
average update runs 66,800 instructions, 2,400 bursts and 101 divide commits.

| Part (s per 1000 updates) | 1 thread | 4 threads | 8 threads | Serial? |
|---|---|---|---|---|
| bursts (pool, or inline with 1 thread) | 3.8 to 4.4 | 1.10 to 1.43 | 0.74 to 1.21 | the span at 8 threads is 78% of ideal; entry and exit of the pool 0.13 |
| divide commits (101 per update, 4.5 to 5.3 us each) | 0.67 | 0.50 to 0.52 | 0.48 to 0.53 | yes |
| end-of-update statistics | 1.14 | 0.49 | 0.43 | gather 0.22, sums 0.13 (critical path), wake-ups |
| budget allocation (Poisson draws, one stream) | 0.16 | 0.16 | 0.16 | yes |
| `begin_update`, merge and prep of the bursts, grants | 0.09 | 0.09 | 0.10 | yes |
| systematics, output, rest of the end of an update | 0.10 | 0.11 | 0.08 to 0.15 | yes |

Amdahl budget at 8 threads: about 0.85 s of the 2.0 to 2.2 s update loop is on
the main thread alone (commits 0.48, allocation 0.16, the end of the update
0.08 to 0.15, the rest 0.1), so the serial share is 40%. With perfect scaling
of everything else the loop would be 1.5 s at 8 threads and 2.2 s at 4, a
ratio of 1.45; with bursts at the span efficiency measured here (0.74 s for
3.8 s of work on 8 threads) and the wake-up cost of each pool entry, the
bound is about 1.3, and 1.2 is what the machine gives with its other load.
The commit alone is a quarter of the loop at either thread count and cannot
be spread (see above).

What changed, all byte-identical in the output of a seed (stock, 160 x 160,
`misc/environment-9resource.cfg`, now `.yaml`, each at 1, 4 and 8 threads where run, the
data files compared against the build before):

| Change | Effect |
|---|---|
| Statistics: the per-instruction message counts (26 columns, three pool tasks of 80 us each, the longest of the phase) are not summed when no organism has any (the usual case: `OrgStatsRecord::from_message_nonzero`, read in the gather): each column then gets only its count (`Accumulator::add_zeros`, the state of repeated `Add(0)`) | statistics 0.43 to 0.35-0.38 s at 8 threads (0.49 to 0.40-0.46 at 4); the exact mode is unchanged (it keeps the walk) |
| `GenotypeArbiter::update_provided_values` computes the entropy term of each abundance once per call (most genotypes have an abundance of 1 or 2), in the same order of additions | end-of-update rest 0.107 to 0.080-0.088 s; also in the exact mode, same bits |

Not worth it, measured: the Poisson draws are 25 ns each (3600 per update,
0.09 ms), the rest of the allocation is the 3600 writes into the organisms
(0.07 ms); neither can leave one thread without changing the stream.
Bundling the column tasks of the statistics (14 spawns to 4) changed nothing
(0.137 against 0.136 to 0.144 s): the wake-ups are not in the spawns. The
commit's profile (719 samples of the main thread at 8 threads): classifying
the offspring 26%, the kill of the occupant 24% (both mostly the systematics'
`adjust_genotype`, whose sparse-list segment search is 16% of the commit),
`divide_reset` and `setup_offspring` of the phenotypes 15% (copies of 15
vectors that the exact path makes too, cold in the main thread's cache
because another core ran the burst), the genome comparison in the hash bucket
7%. A scheme that moves any of this to the burst changes what the commit
reads, and was not attempted. Not attempted either: a pool of our own with
spinning workers (the 0.13 s of entry and exit; rayon's workers sleep between
phases; the previous section tried the calling thread spinning at the end of a
phase and found 0.8%), and the end-of-update systematics beside the
statistics gather (the data-file actions between them read the systematics,
and `update_provided_values` is only 4% of the loop).

## Recording and playback (2026-11-13)

A run can be recorded to a file of `Frame` snapshots and played back in
`avida-gui` and `avida-tui`. The user page is `docs/guides/how-to-record-and-replay.md`;
the format is documented in `crates/session/src/recording.rs`.

### Layering

- `viewmodel` is unchanged: a recorded frame is the `Frame` that `capture`
  makes for a subscription of every layer (or the layers asked for).
- `session` holds all of it: `recording` (format, `RecordingWriter`,
  `Recording` reader), `playback` (the player thread), `cli` (the
  `--record*` options shared by the programs) and the `avida-rec` binary.
  The `avida` binary does not depend on any of it.
- Recording is a command of the simulation thread: `Command::Record(RecordSpec)`,
  `StopRecording`, `RecordNow`. Frames are appended in `Sim::end` (an update
  that ends at a multiple of the interval) and when the run finishes, always
  between updates, always through the read-only `capture`. The determinism
  test `tests/recording.rs` pins that a recorded run writes the data files
  of an unrecorded one. `Session::recording_status` reports frames, bytes and
  the time spent capturing and writing.
- Playback is a `Session` whose thread is the player (`Session::play`,
  `play_with_wake`) instead of a world. The front ends keep one handle:
  `latest_frame`, `drain_rows` (with `drain_rows_reset`, which says that a seek
  backwards restarts the chart's rows), `status`, `setup`, `drain_events` and
  `send`. `Run`, `Pause`, `StepUpdate` (next frame), `StepSlice` (previous
  frame), `RunToUpdate` (seek to the frame at or before the update),
  `SeekFrame` and `SetSpeed` (updates of the recorded run per second) work;
  a `Subscribe` is ignored (the frames have the recorded layers); the commands
  that need a world answer with an `Error` event. `Session::playback_info`
  says where the playback stands and is `None` for a simulation.
- The front ends add a playback bar (GUI) or header, footer and keys (TUI),
  and refuse what needs a world with a message. `--play FILE` opens a
  recording; `--record FILE [--record-every N] [--record-layers LIST]
  [--record-top N]` records the run.

### Format (version 1)

All integers little-endian. `"AVIDAREC" version:u32 header_len:u32 header`
then records, each `body_len:u32 update:i32 row_len:u32 row frame_raw_len:u32 frame`
where `row` is the zlib-compressed `UpdateRow` and `frame` the zlib-compressed
rest of the frame: world-level flags, the map (genotype ids, every recorded
layer, task bits, top genotypes and their counts, the ancestry families), the
dominant genotype, the genotype table and the environment. The header holds
what does not change: world size, seed, instruction names, the recorded layer
list, the interval, the `avida` arguments. Every record is independently
decodable, so a killed run leaves a readable file; the reader indexes the
records with one scan that reads eight bytes per record, no trailer, and
`Recording::refresh` picks up records appended later. Unknown versions are
rejected with their number; truncated or damaged data is an error value, never
a panic (a cut file opens up to its last complete record, `Recording::tail`
says what follows).

Not recorded: the inspected organism, the lineage list, and
`MapFrame::genotype_cells` (derived from the genotype ids when a record is read,
which saved a fifth of the file). Per-cell arrays are stored plane by plane
(all first bytes, then all second bytes, ...) before compression, which turns
`f32` and `i32` arrays into long runs. The compressor is zlib from the pure Rust
`miniz_oxide` (level 6): no `unsafe`, no C toolchain, an Adler-32 per record, and
already in the build through the PNG code. Raising the level from 3 to 6 to 9
took a 60 x 60 frame from 43.6 to 42.7 to 42.4 KB of layers; `lz4_flex` (no
entropy coding) and `ruzstd` (a compressor that is weaker than DEFLATE at its
highest levels) were not measured, since compression is 3 to 20 ms of a frame
that takes 10 to 35 ms to capture and write.

### Size and cost (stock configuration, seed 42, every layer, a frame every 50 updates)

| World | Updates | Frames | File | Per frame |
|---|---|---|---|---|
| 60 x 60, exact | 3000 | 61 | 2.9 MiB | 49 KB (13.5 bytes per cell) |
| 120 x 120, parallel (4 threads) | 1500 | 31 | 3.3 MiB | 109 KB |
| 160 x 160, parallel (4 threads) | 1500 | 31 | 4.3 MiB | 142 KB |

The per-cell cost rises as the world fills and diversifies; the full 60 x 60
world is the better guide. Four layers (genotype, ancestry, fitness, age) take
about 70 % of the all-layer size and genotype with ancestry 40 % (60 x 60, 1500
updates), so `--record-every` is the lever that matters: 6000 frames of a full 160 x 160 world (every 50 updates for 300,000)
come to about 2 GB with every layer. Capturing, compressing and writing a frame
took 10 ms (60 x 60) to 35 ms (160 x 160), 1 to 3 % of the run.

### Limits

- A playback cannot inspect an organism, debug, restart, save the population
  or change settings; only the recorded layers are on offer; charts have one
  point per recorded frame.
- A frame is built in memory before it is compressed (a few MB at 160 x 160),
  and a playback decodes one frame at a time; the index is 16 bytes per record.
- A recording is not appended to by a later run and is not cut or merged by any
  tool here. A restart inside a recording session ends the recording.
- There is no live follow mode: a playback sees records appended since it opened
  when it is next asked to run or step past its last frame.
- Nobody has looked at the playback screens in a real window or terminal; they
  were checked in the offscreen GUI harness and the TUI's test terminal.

## Open-endedness metrics: MODES (`PrintMODESData`)

A port-only extension (the C++ has no such action; `world/src/modes.rs`,
tests in `world/src/modes/tests.rs` and `crates/avida/tests/modes.rs`). It
computes the change, novelty, complexity and ecology metrics of Dolson et al.,
"The MODES Toolbox" (Artificial Life 25(1), 2019), as the Empirical
`OEETracker` / `CoalescenceFilter` / `Skeletonize` code computes them, on the
genotype tracker's genotypes.

```
u 0:1:end PrintMODESData <file> <resolution> <t> [nearest|any]
```

`resolution` and `t` are in generations of the average generation; `t` must be
a positive multiple of `resolution` (anything else stops the start-up).
Several instances (other file, resolution, `t` or mode) coexist and share the
skeleton cache. The action fires every update and samples only when
`floor(generation / resolution)` exceeds the last sampled index, one sample
per crossing (the reference drops a sample when its rounded generation skips a
multiple). Per-update cost of a firing that does not sample: one division.

**Method.** A sample stores `(genotype id, organisms)` of every living genotype
(`Window` keeps the last `t / resolution + 1`). When the window is full, the
oldest snapshot is evaluated: its persistent genotypes (alive now, or with a
living descendant now) get a skeleton, `F` maps skeleton to summed snapshot
count, and `change = |F \ F'|`, `novelty = |F \ S|`, `complexity` = longest
skeleton, `ecology` = entropy in bits. No row is written before a snapshot
`t` generations old exists. `nearest` (default) is the reference lookup (the
first ancestor-or-self alive at the snapshot, so an ancestor that has a
snapshot descendant is left out unless it survives by itself); `any` is the
paper's wording (every snapshot genotype with a living descendant). Skeletons
are computed only for persistent genotypes, cached per genotype id, and the
cache keeps only ids that a snapshot of any instance still holds. The skeleton
keeps the symbols of the sites whose replacement by NULL strictly lowers test
fitness, positions dropped (reference behaviour); complexity counts
instructions.

**Test-CPU resources.** The default test CPU (`RES_INITIAL`) gives a limited
resource its `initial=` level, usually 0, and `DoProcesses` pays nothing from a
resource at 0, so in a `geometry=grid` / `frac=:max=` world no task is worth
anything and no site needed only for a task is informative. The action uses
the Avida mechanism for other levels, `SetResourceOptions(RES_CONSTANT,
history, 0, 0)`, with one history entry that gives each resource
`max(initial, inflow per cell / outflow)` (the level of an unexploited cell;
1 unit for the 160 x 160 "continents" environment). Not modelled: diffusion,
gradient resources, instructions that read the cell's own resources (the test
CPU's cell resources stay 0). Pinned by
`a_rewarded_task_lengthens_the_skeleton_in_a_spatial_resource_world`: a NOT
organism (6 inserted instructions) has skeleton length 21 against 15 for the
replicator alone at these levels, and 17 under `RES_INITIAL`, where the
instructions that only make the task (IO, push, nand, IO) are absent.

**Observation only.** The test CPU runs through
`avida_analyze::world::with_detached_test_world`: own RNG (re-seeded per
genotype), null statistics, own `std::rand` state, no population, diagnostics
discarded, on a private copy of the instruction sets (`InstSetRegistry::
duplicate`, `HardwareManager::from_registry`) in which `ActivateNullInst` has
run, so the live sets (instruction numbering, mutation, output files) are
untouched. The run's environment is only read; `xor-max` (which repoints a
reaction) is refused at the first firing. Results are cached in the action,
not in `GenomeTestMetrics`. `observation_only_*` compare every other data file
byte for byte (time stamps aside) with a run without the action, in the exact
mode and in the parallel mode on 1 and 4 threads, and the MODES files of 1 and
4 threads against each other. `DISABLE_GENOTYPE_CLASSIFICATION 1` is refused.

**Output** (`data/<file>`): header comments (source of the definitions,
parameters, conventions), then one row per evaluated sample with columns update,
average generation, update of the snapshot evaluated, change, novelty,
complexity, ecology (bits), persistent genotypes, distinct persistent
skeletons, living genotypes in the snapshot, cumulative novelty.

**Cost** (stock 60 x 60, 5000 updates, release): 84 to 87 s with and without
the action on a shared machine (no measurable difference); with resolution 50
and `t` 200, 11 rows and at most 461 skeletons (persistent genotypes summed over
the rows; about 100 test runs each, fewer when cached).

**Validation against the paper (2026-11-14).** The paper's Avida experiment
was repeated: well-mixed (`BIRTH_METHOD 4`), 500 organisms, the empty and the
Logic-9 environment, 10 seeds each, 300,000 updates (about 57,000
generations). Mean change per sample at a persistence lag of 100, 500, 1000
and 2000 generations: 8.8, 1.63 ± 0.10, 1.08, 1.00 in the empty environment
and 5.9, 1.53 ± 0.06, 1.07, 1.00 in Logic-9. As in the paper a longer lag
lowers change, novelty follows change, ecology stays low (0.4 to 0.5 bits at
a lag of 500), complexity rises and then falls in the empty environment (48
at the first sample, about 40 afterwards) and keeps rising in Logic-9 (53 to
about 68). Not reproduced: the paper's higher change in Logic-9 than in the
empty environment; the cause is not known (the paper does not give its
sampling resolution). The `any` filter gives 2.40 where `nearest` gives 1.63.

## Open-endedness metrics: phylogeny

Two event actions beyond the C++ program (nothing like them exists in avida-core; `PORTING.md` and the reference comparison are untouched): `PrintPhylogenyMetrics [file=phylogeny.dat]` writes one row per firing, `SavePhylogeny [file=phylogeny_tree.csv]` rewrites a CSV of the current tree. Both only read the genotype tracker: no random draw, no write to the population or the systematics. `crates/avida/tests/phylogeny.rs` runs a 60x60 world with `PrintPhylogenyMetrics` every 10 updates (and `SavePhylogeny` at updates 50 and 90) against the same run without it, and every other data file and the status lines are byte-identical (time stamps aside) in the exact mode and in the parallel mode on one and four threads; the metrics file is identical for one and four threads and for repeated exact runs.

**Layering.** `systematics/src/phylometrics.rs` holds pure functions over a snapshot (`Vec<PhyloNode>`: id, parent index, abundance, origination update, tracker depth, destruction update) and knows nothing of the world. `world/src/phylo_actions.rs` builds the snapshot from the genotype arbiter (the active size lists, size 0 included, plus the historic list; `Begin()`/`Next()` stops at size 1 and skips the historic ancestors, so it cannot be used) and writes the rows. A genotype is living when `num_units() > 0`; its parent is `parents()[0]`, as `updateCoalescent` follows (a sexual genotype's other parents are ignored); origination is `update_born()`. The tree is the snapshot pruned to living genotypes and their ancestors, rooted at the MRCA: the topmost node of a living lineage that is living or has two or more children (`GetMRCA`; the tracker's own `coalescent` follows the best genotype's line only and never tests the root, so the two can differ). Pre-MRCA taxa are excluded. With several roots or nothing living there is no MRCA: the MRCA columns are -1 and the metrics nan.

**Metrics (sources: Empirical `Systematics.hpp`; Dolson et al. 2020)**

| Column | Definition | Living internal node |
|---|---|---|
| nodes, PD | nodes of the tree from the MRCA down; PD = edges = nodes - 1 (Empirical returns ancestors + living - 1 over the whole tree, pre-MRCA ancestors included; the paper's text says "nodes") | an ordinary node |
| pairwise distance sum, mean, variance | path length in edges over all unordered pairs of living genotypes, unweighted. Sum: each edge to a child c lies on live(c) (n - live(c)) paths. Sum of squares: one bottom-up pass carrying (count, sum of depths, sum of squared depths), charging each cross pair at its lowest common ancestor. Exact in 128-bit integers; variance = (N sum d^2 - (sum d)^2) / (N (N - 1)) | a taxon at distance 0 from its own node, as `GetPairwiseDistances` pairs a living taxon with the subtrees below it |
| evolutionary distinctiveness mean, sum, variance | `GetEvolutionaryDistinctiveness` with time = the current update: the first branch is measured from now to the origination of the first ancestor that is living, branching or the MRCA (so a leaf's own life counts), then origination differences; each segment is divided by the living genotypes below its upper end (living descendants + 1 for a living split point, living descendants for a dead branch point). One top-down pass: segments telescope, so a taxon's value is (now - o(p)) / live(taxon) + G(p) with p its nearest split ancestor and G accumulated down the tree | a split point whose divisor restarts at itself plus its living descendants; the MRCA contributes 0 |
| Sackin | sum over living genotypes of (ancestors strictly between it and the MRCA with two or more children, + 1). The MRCA itself contributes 1; `GetBranchesToRoot` would walk past it and count branch points above (pre-MRCA taxa are excluded here) | counts as a branching point for the taxa below only if it has two or more children (the code tests `GetNumOff() > 1`, not aliveness) |
| Colless-like | `RecursiveCollessStep` as written (skip unifurcations; n value of a child = sum of its n values + ln(children of the child + e); add the mean absolute deviation from the median), not Mir et al. 2018. Iterative: per node (sum of n values, total), reverse breadth-first | ignored: only the shape counts |
| Shannon | bits, abundance weighted, all living genotypes (`CalcDiversity`) | an ordinary taxon |
| mean depth | edges below the MRCA, mean over living genotypes | |
| lineage length | edges from the founder to the most abundant genotype (lowest id on ties) along first parents | |

Variances are sample variances (n - 1) as `emp::Variance`, nan below two values. Children are summed in ascending id order wherever floating-point order matters (the reference iterates a `std::set` of pointers, so its order is unspecified). `mrca_changes` counts rows whose MRCA id differs from the previous row's (the first row counts 0; several changes between two rows count once), kept per file name.

Phenotypic volatility (task-phenotype changes and distinct phenotypes along the lineage) is not reported: the tracker keeps no task phenotype of an extinct ancestor (`Genotype::task_counts` exists but nothing feeds it, as in the C++, and a test CPU run on an ancestor's genome would not be its phenotype in the run). It needs the gestation hook (`handle_unit_gestation`) to record the organism's task vector at a genotype's first gestation; that is a read of data the world already has, but it is a change to a shared struct and was left out.

**Cost.** Everything is O(nodes) or O(nodes log nodes) (the sorts) and iterative: a caterpillar of 60,000 branch points is a unit test. On a release build one call takes about 33 ms for 200,000 snapshot nodes with 20,000 living genotypes (about 70,000 to 134,000 pruned nodes) and 16 ms for 100,000 nodes; `cargo test --release -p avida-systematics --test phylometrics large_tree_timing -- --ignored --nocapture`.

**Verification.** `systematics/tests/phylometrics.rs` compares the fast code on 4,000 random trees (chains, multifurcations, dead prefixes above the MRCA, living internal nodes, one or two living taxa, shuffled node order) and four long-chain trees with brute force: pairwise sum, sum of squares, mean and variance from the list of all pairs; PD, Sackin, evolutionary distinctiveness and the MRCA as transcriptions of the reference functions; Colless-like as a transcription of `RecursiveCollessStep`. Floating-point results agree to 1e-9 relative (the reference sums evolutionary distinctiveness bottom up, the fast code top down). Hand-computed cases pin the living-internal-node conventions.

**`SavePhylogeny`** writes the ALife standard phylogeny format (`id,ancestor_list,origin_time,destruction_time,num_orgs,depth`), the whole pruned tree with pre-MRCA ancestors, in ascending id order, one ancestor per row (`[none]` for a root), `inf` for a living genotype, an empty destruction time when an extinct genotype's is unknown. It rewrites its file at every firing.

### Open-endedness view (2026-11-14)

Both front ends chart what `PrintMODESData` and `PrintPhylogenyMetrics` write,
live. It is observation only: the actions append to a bounded in-memory
history besides writing their rows, and nothing else changes (no output file,
no random draw; `crates/avida/tests/modes.rs`, `phylogeny.rs` and the reference
suite are the check).

Data path, bottom to top:

1. `world`: each `PrintMODESData` instance (`modes::Instance`) and each
   `PrintPhylogenyMetrics` file (`phylo_actions::MrcaTrack`) keeps the last
   `modes::HISTORY_CAP` (2,000) rows it wrote, oldest dropped first
   (`push_capped`). `ModesState::history()` and `PhyloState::history()` copy
   them out (`ModesRow`, `PhyloRow`: every MODES column, and the phylogeny columns a
   chart shows; the other columns stay in the data file).
2. `viewmodel::capture` copies both into `Frame::open_ended` on every frame
   (empty when the events file has neither action, so nothing is cloned
   then). This is the mechanism the session already uses for everything a
   front end draws, so `session` needed no new command or subscription. A
   recording stores the layers it asks for and not this field: the recording
   format is unchanged, a recorded `Frame` reads back with `open_ended`
   empty, and a playback shows `PLAYBACK_NOTE` ("only available in a live
   run").
3. `viewmodel::open_ended::build(&OpenEnded, &OeSources)` makes an `OeView`:
   groups (one per MODES instance, labelled by its persistence lag `t` and
   filter mode; one per phylogeny file), each with one `OeChart` per measure
   (title, x label, whole-number flag, one-sentence explanation, points with
   non-finite values dropped). MODES charts (change, novelty, complexity,
   ecology) are against the average generation; phylogeny charts (living
   genotypes, phylogenetic diversity, mean pairwise distance, mean
   evolutionary distinctiveness, Shannon diversity, depth of the common
   ancestor, changes of the common ancestor) against the update. `OeSources`
   is read from the run's event lines (`ExperimentSetup::events`) and is how
   the empty states are told apart: neither action (a notice and both example
   event lines), an action with no row yet (MODES: "rows start after
   resolution + t generations"), or one action missing (a notice and its
   line).
4. The front ends draw the view; neither touches `world`.
   - GUI (`gui/src/ui/open_ended.rs`): the last view of the Charts tab,
     "Open-ended" (Charts, then the segmented control; or the command
     palette, "Chart: Open-ended"). One small plot per measure with a shared
     x axis per group, the time series' zoom, drag and hover read-out (the
     title shows the value at the pointer), and an info mark whose tooltip is
     the explanation. The page holds group titles, chart titles and values:
     the MODES paragraph and a group's caption are tooltips of the group
     title, the interaction hints tooltips of the chart titles, and a missing
     action's notice shows its first sentence (the rest is a tooltip) and the
     line to add.
   - TUI (`tui/src/ui/open_ended.rs`): screen `K` ("Open" in the tab strip, last
     in the cycle). Charts in a grid (two columns from 100 columns), paged to
     fit; Up and Down choose a chart, whose explanation is written under the
     charts; Left and Right switch group. A terminal has no hover, so each
     title carries the newest value.

Tests: `viewmodel/src/open_ended.rs` (series and the empty states),
`viewmodel/tests/open_ended.rs` (the frame's rows equal the data files' rows on
a real run), `world/src/modes/tests.rs` (the cap), `tui/src/ui/tests.rs`, and
the sweeps `gui/tests/audit.rs` (`open_endedness_in_both_themes_and_sizes`)
and `tui/tests/screens.rs` (`Screen::OpenEnded` in every state, and
`the_open_endedness_screen_with_rows`).

## Ladder view (2026-11-15)

Both front ends show how far up the environment's reaction list a population
has climbed. The reactions in file order are the rungs (the top rung drawn
first); each rung is a strip over the updates of the share of the organisms
that performed it, 0 to the whole population, all strips on one x axis. It is
observation only: nothing is added to the world, a recording or the session.

Data path:

1. `viewmodel::ladder::LadderHistory` folds the `UpdateRow`s the front ends
   already receive (`UpdateRow::task_last_count` and `orgs`) into a bounded
   series (2,000 points; past that every other point goes and the stride
   doubles, the newest row always last). For each task it also keeps the
   first update at which any organism performed it and the first at which at
   least 1 % of the organisms did. Both are taken from every row pushed, so
   thinning the series cannot lose them (`LadderHistory::first_reached`,
   `first_held`).
2. `ladder::build(history, env, subset)` makes a `LadderView`: one `Rung` per
   reaction that has a task (name, counts per point, state never reached /
   lost / present, the two dates, the subset's count when a filter is set),
   the headline (highest rung held by at least 1 % now, highest ever reached)
   and a nearest-update lookup for hovers.
3. The front ends own a `LadderHistory` next to their `RowHistory`, feed it in
   `apply_rows` and clear it where the row history is cleared (a restart, a
   seek backwards in a playback). Neither touches `world`.

Rungs are reactions (named by the reaction), counts are per task: the rows,
the recording and the per-cell task bits carry `GetTaskLastCount(k)` and one
flag per task, not per reaction. A reaction takes the count of the task that
triggers it; two reactions on one task are two rungs with the same series and
each names the other in its hover. A reaction without a task is not a rung.
A task can be performed without its reaction triggering (a requisite or a
depleted resource), so a rung means "performed", not "rewarded".

Playback: the recording stores an `UpdateRow` per frame, so the ladder is
filled from the frames at the recorded interval, up to the frame on show; a
seek backwards rebuilds it, so the dates are those of the frames seen so far
(a rung reached after the shown frame is not yet reached). First-reached and
first-held have the resolution of the interval (a multiple of it). The subset
count is the shown frame's (task bits of its map); there is no subset history.

- GUI (`gui/src/ui/ladder.rs`): "Ladder" beside "Tasks" in the Charts tab (also
  the command palette, "Chart: Ladder"). With a panel narrower than 500
  points the six views are two rows of three. A strip is an area in the
  task's colour (`task_color`, as the Tasks view), a ring marks the first
  performance and a dot the first update at 1 %; an unreached rung is a dotted
  baseline and "never", a lost one keeps its past and says "lost". Hover on a
  row reads update, count and share at the pointer (with the dates and the
  subset's count); a click makes "performs task" a filter condition (a second
  click removes it, Shift-click "does not perform"); the filter colour and the
  subset's "n of m" are in the right column while a filter is set.
- TUI (`tui/src/ui/ladder.rs`): screen `J` ("Ladder", last in the Tab cycle).
  One line per rung, a strip of eight-level blocks, `o` first performed, `*`
  first at 1 % (`@` both in one cell), `never` and `lost` in words. Up and Down
  choose a rung, whose dates are written under the table; Enter toggles
  "performs this task" in the filter (`+` marks a rung the filter uses); the
  subset's count is a column while a filter is set. The tab strip now has 13
  tabs; at 80 columns it starts at the first tab from which the current one
  fits (a `<` marks tabs cut off at the left).

Tests: `viewmodel/src/ladder.rs` (first reached and held, thinning that keeps
them, reactions sharing a task, lost against never reached, the playback
derivation, subset), `gui/src/ui/ladder.rs` (hover text),
`gui/tests/ladder.rs` (the stock environment's rungs in order, a click makes
and removes the condition, the empty state, a recording up to the shown frame,
screenshots in both themes at 1280 x 800 and 760 x 640),
`tui/src/ui/ladder.rs` (100 x 30, 80 x 24, 40 x 10, keys, filter, tab strip)
and `tui/tests/playback.rs` (`a_recording_fills_the_ladder...`).

## Chart filtering and brushing (2026-11-14)

The Charts tab of the desktop GUI links its views and the map through one
organism filter ("brushing"). The filter is view state and nothing else: it
changes which layers the GUI asks the session for, never the simulation, an
output file or a recording (`crates/gui/src/model/mod.rs`,
`filtering_only_changes_the_subscription_and_ends_with_the_run`). It is
cleared when another run is opened, closed or restarted, and it survives
switching chart views and map layers.

**Model** (`viewmodel/src/filter.rs`, toolkit-free, shared with any front
end). A `Filter` is a list of `Condition`s joined by AND; an empty filter
selects every organism. `Filter::select(&MapFrame)` returns a `Selection`: one
flag per cell (never set for an empty cell), the number of matching and
occupied cells, and the conditions it could not apply.

| Condition | Source in the frame |
|---|---|
| performs / does not perform task `k` | `MapFrame::task_bits` (always captured) |
| range on fitness, merit, gestation time, genome length, age, generation, tasks performed, threads (`Prop`) | the property's float layer (`MapFrame::layers`) |
| genotype | `MapFrame::genotype_id` |
| family | the Ancestry layer's values and `MapFrame::families`, the same grouping the map layer colours |
| region | the cell's `x = c % w`, `y = c / w`; a rectangle of cells |

A condition whose layer is not in the frame (a recording that did not keep
it) is left out of the selection and listed in `Selection::ignored`; its chip
carries a warning. Lineage depth (the phylogeny depth of a genotype) is not a
condition: no layer carries it, and adding one changes the recording format;
`generation` is the per-organism count of ancestors.

**Data path.** `AppState::subscription()` asks for the map layer, and, while
a filter is active, all eight property layers (the subset's series reads
every one) plus the Ancestry layer for a family condition; the scatter view
adds its two axes, and the open "Add condition" form adds Ancestry to list
families. `AppState::sync_subscription()` (every frame, from `logic`) and
`edit_filter` re-send the subscription when that set changes; no other command
is sent. `apply_frame` and `edit_filter` recompute the `Selection` for the
newest frame (`Filtering::refresh`); everything below reads it.
`render_map` dims unselected organisms to alpha 70 of 255 and leaves their
colours (the scatter view reads those colours from the map image,
`AppState::map_rgba`).

| View | With a filter |
|---|---|
| Map | unselected organisms dimmed, not hidden; a filter-coloured outline around the world; the regions as rectangles. Drag with the region tool on (toolbar, "Add condition" form, command palette) or with Shift held draws a region; the tool is used once. |
| Histogram | the selected organisms' counts in the same bins, in the filter colour, over the whole population's; "N matching (P%)". Dragging sideways sets the range of the layer's property (whole numbers rounded inwards), a click removes it. The bar charts of genotypes and families overlay the subset per bar; a click on a bar toggles the genotype or family. |
| Tasks | per task a second thin bar and a second share for the selected organisms next to everyone's (the frame's `task_counts`, not the row's, so both use the same organisms); a click on a row toggles "performs", Shift-click "does not perform"; the swatch still hides or shows the task's line. |
| Time series | for measures with an organism-level counterpart (organisms, genotypes, average and maximum fitness and merit, gestation time, generation, age, genome size, threads) a second line of the subset's mean (`SubsetRow`, `subset_value`). One y axis per chart; the measure chips, linear or log value axis and a moving average are controls of the view. |
| Scatter | any two properties, one dot per organism coloured as the map is; selected organisms larger with a filter-coloured outline, the rest fainter; per-axis log scale; dragging a rectangle sets both ranges; a click inspects the nearest dot. Painted as one mesh over a plot that only draws axes and grid. |

The filter colour (`Tokens::filter`, magenta) is the one mark of the subset
everywhere; it is in neither viridis, cividis nor Okabe-Ito. Chart text uses
text colours.

**The subset's series.** A live run keeps no per-organism past, so the subset
line starts at the update the filter was set and grows with every frame
(`SubsetHistory`, one row per frame, 50,000 rows at most). In a playback the
GUI starts a worker (`model/scan.rs`) that opens the recording file again,
applies the filter to every frame and sends one `SubsetRow` per frame back; the
line fills in over the whole run (up to the frame on screen) while the worker
reads. It reads only, and a new filter stops the old worker.

**Tests.** `viewmodel/src/filter.rs` (each condition, AND, empty filter,
counts, ignored conditions, histogram overlay, task counts, subset row,
scatter points and thinning), `gui/src/model/filtering.rs`, `layers.rs`,
`metrics.rs`, `ui/histogram.rs`, `ui/scatter.rs`, and `gui/tests/filter.rs`
(the chips remove conditions; a drag on the histogram, a click on a task, a
Shift-drag region, a rectangle on the scatter plot and the Add condition form
make them; a recording gives the subset line over every frame; a recording
without a layer does not apply the condition). The review sweep is
`gui/tests/audit.rs` (`chart_filtering_in_both_themes_and_sizes`, on a
synthetic recording from `gui/tests/common/synth.rs`) and the measurements
`gui/tests/perf.rs` (`filtering_costs_on_a_160_by_160_world`).

**Cost** (160 x 160 world, 25,600 organisms, release build, a machine under
other load; `gui/tests/perf.rs`, `viewmodel/tests/filter_cost.rs`):

| What | Median |
|---|---|
| `capture` on the simulation thread, genotype layer only / fitness only | 1.1 ms / 1.4 ms |
| the same with the eight property layers (a filter is active) / and Ancestry | 3.9 ms / 4.2 ms |
| `Filter::select`, three conditions (range, task, region) | 0.06 ms |
| `subset_row` (means of eight properties, maxima, distinct genotypes) | 0.34 ms |
| `scatter_points` and `thin_points` | 0.07 ms |
| UI frame (UI pass and tessellation), scatter, no filter / 12,321 of 25,600 selected | 0.93 ms / 1.17 ms (p95 1.5 ms) |
| UI frame, histogram, no filter / with a filter | 0.23 ms / 0.51 ms |
| UI frame, tasks, no filter / with a filter | 0.43 ms / 0.59 ms |
| UI frame, time series with the subset line | 0.34 ms |
| UI frame while the run advances, scatter with a filter | 1.8 ms (p95 4.6 ms) |

The scatter plot therefore draws every organism: it thins (`MAX_DOTS`, 60,000)
only beyond what the largest worlds have, and then keeps the selected ones
first and every k-th of the rest, so the picture is the same from frame to
frame. The cost of a filter is the extra 2.8 ms of capture per frame, paid on
the simulation thread only while a filter is active.

**Terminal interface** (`crates/tui/src/filtering.rs`, `ui/filter_bar.rs`,
`ui/filter_dialog.rs`). The same `Filter` and `Selection`, as view state of
`App` (`App::filtering`); the subset's series (`SubsetHistory`) is a copy of
the desktop one, without the playback scan. `App::subscription()` adds the
layers of `Filtering::layers()` (the eight properties while a filter is
active, Ancestry for a family condition, the chosen property's layer or
Ancestry while the dialog is on that step); `App::sync_subscription()` (after
every edit, step of the dialog and frame) sends a `Subscribe` only when that
set of layers changed, and nothing else goes to the session. The filter is
cleared by `reset_run`. Keys: `/` opens the "Filter organisms" dialog (on the
Config screen `/` still filters the settings; the palette has the dialog on
every screen), `\` clears the filter, and the palette also lists "Show or
remove the filter's conditions". The dialog has an "Add condition" list (kind,
then the task list, the family list, or a comparison with typed numbers beside
the property's current minimum and maximum, or a rectangle typed over the cells
in view) and a "Conditions" list (`Del` or `x` removes one, `c` clears). Every
screen with the map or a chart (Map, Stats, Charts, Hist) gets a one-line bar
with the same labels as the desktop chips and "N of M organisms match (P%)";
nothing matching reads "No organism matches". The map greys the organisms the
filter leaves out (the same grey at every colour depth, so it never reads as an
empty cell; in 16 colours it is dark grey); without colour they are the light
shade and the matching ones keep the upper shades or their letters. The
histogram draws the matching part of each bin or genotype bar solid and the
rest shaded, with "n of m"; the task list shows both shares as bars with
percentages; a time series gets a dotted second line (half blocks without
colour) for every measure that has a subset value, from the update the filter
was set. Not in the terminal interface: the scatter view, drag selection and
the family histogram overlay.

**Ranges that hold still (2026-11-20, desktop GUI).** At the maintainer's
request ("quick up, slow down, like in an equalizer") the automatic ranges of
the map's colour scale, the histogram's axes and the time-series and scatter
axes widen at once, with 15 % headroom, when a value falls outside, and narrow
only while more than 30 % of the range lies unused on a side, closing about two
thirds of the gap in 4 s (`crates/gui/src/model/hold.rs`, `ui/ease.rs`;
`MapSettings::hold_range` for the map). On a log scale the range is held in the
logarithm. A plot switched to another property or scale, or not drawn in the
last frames, starts at its values again; a locked map range is not touched.

## Modular heads set (opt-in extension, 2026-11-14)

`avida-core/support/config/instset-heads-modular.yaml` (set `heads_modular`, hardware type 0) is the stock heads set plus `nop-D`, `nop-E`, `nop-F`, `call` and `return`. It is a port-only extension for experiments on code reuse; no stock configuration reaches it, and with every existing set the heads CPU computes what it did before (the consistency suite, and the default run's time, are the checks).

- **Registers.** The heads CPU has `num_regs = max(3, nops of the set)` registers, at most six (`hardware_cpu::reg_count`, read once when the hardware is created). `nop-D`..`nop-F` are library entries placed just before `NULL` in `CPU_INST_DEFS` (every C++ library index is unchanged) and declared as nops through `InstLib::with_extra_nops`; a set must list its nops first, as always. `LocalThread::reg` is always `[i32; 6]`, zeroed wherever AX to CX were (thread reset, divide reset, `reset`, epigenetic save and restore, thread copy). `num_regs` is also the modulus of label complements (the C++ `NUM_NOPS`) and of the next-register rotation (`find_next_register`, now a method), so with six nops `call C C` looks for `D D`, and `A->B->...->F->A`. At three the arithmetic is the C++'s. The trace (`print_status`) prints `AX` to `FX` as it prints `AX` to `CX`, the view model (`capture.rs`) hands the front ends `num_regs` registers, and both inspectors already named registers `AX`, `BX`, ... without a fixed count (the desktop inspector puts four to six registers three to a row so ten-digit values fit; its stack items carry accessible names, `Local stack level 0: 15`).
- **Subroutines.** The existing `call` and `return` are enough and are unchanged (the active stack, depth 10, carries the return address; `call` pushes its own position, so `return` resumes at the call's label nops, which run as no-ops). Two properties of the existing pair matter to anyone writing organisms: the label search is forward only and does not wrap, so a subroutine must lie after its callers in the genome, and a `call` without a label jumps to BX.
- **Bursts.** `Inst_Nop`, `Inst_Call`, `Inst_Return`, `Inst_Push` and `Inst_Pop` are already in `CPU_PARALLEL_METHODS`; nothing new was classified.
- **Organism and tests.** `modular-subroutine.org` replicates exactly and does NOT, NAND and AND through one `nand` subroutine called from three places (`crates/hardware/tests/modular.rs`: test CPU, register wrap at 3 and at 6, six-nop label complement, burst classification; `crates/avida/tests/modular.rs`: zero-mutation run and a 1, 2 and 4 thread parallel run; `crates/tui/tests/modular.rs`, `crates/gui/tests/modular.rs`: stepping through a `call` and its `return`).
- **Running it.** `avida -set instruction_sets heads_modular` (the stock set by name) with events that inject the organism, or `instruction_sets: [heads_modular]` in the experiment file.

## Field arithmetic task and the adder ladder (opt-in extension, 2026-11-15)

`field_arith` (`crates/tasks/src/eval_field.rs`) is a task the C++ does not have. It rewards multi-bit arithmetic computed bit-parallel across fields of the 32-bit word, judged on one output or on a sequence of outputs, with graded credit. No configuration reaches it unless an environment names it; the other tasks and the exact-mode output are unchanged (the consistency suite is the check).

- **Arguments** (`fn=add:bits=2:mode=packed:thresh=0.75`, `:` or `,` between them, names and values case-insensitive). `fn` (required): `xor`, `and`, `or`, `halfadd`, `add`, `add3` (`a + b + c` of the three newest inputs, 2026-11-25), `add4` to `add8` (the sum of the k newest inputs, k = 4 to 8, 2026-11-26; the environment then hands out k inputs), `carry` (`(a & b) << 1`), `partial` (`(a ^ b) ^ carry`), and the logic steps on the two computed words `x = a ^ b` and `c = carry`: `nand_xc`, `and_xc`, `or_xc`, `andn_xc`, `andn_cx` (2026-11-19). `bits` = w, the operand width, 1 to 4 (default 1); `halfadd` needs `bits=1` and is `add` with `bits=1`. `mode`: `packed` (default) or `planes`. `thresh` in 0 to 1 (default 1). `stride` (default: the result width): the field width, from the result width to 32.
- **Operands.** The two newest inputs: `a = input[1]` (the older) and `b = input[0]` (the newer), as the organism's input buffer holds them when the output is tested, which is before the IO that outputs also reads its next input. `add3` also reads `c = input[2]`, and a sum of k operands reads `input[0]` to `input[k - 1]`. With fewer stored inputs than operands the quality is 0.
- **Fields.** The word is cut into `n = 32 / stride` whole fields from bit 0. The result width is r = w + ceil(log2 k) for a sum of k operands (w + 2 for `add3` and `add4`, w + 3 for `add5` to `add8`), w + 1 for `add` and `halfadd`, w for the logic functions, and the field width F defaults to r. The operand of a field is the low w bits of the field (the rest is masked away, so no field sum leaves its field); the expected result of a field is `fn(a_field, b_field)` in r bits. With `stride=4`, `bits=2`, `fn=add` this is `((a & 0x33333333) + (b & 0x33333333)) & 0x77777777`, eight 2-bit adders; the default stride 3 gives ten adders in 3-bit fields (30 bits compared).
- **Modes.** `packed`: the newest output holds every field's result in place; the r result bits of every whole field are compared. `planes`: the result comes as r successive outputs, oldest first and least significant first; output i holds bit i of every field's result at the field's bit 0 (bit `k * stride`), and only those positions are compared. In `planes` mode fewer than r stored outputs give quality 0. The window is the last r outputs at every output test, so an organism that writes planes in a loop is judged at every output.
- **Quality.** With f the fraction of compared bits that are right, q = max(0, 2f - 1), and q below `thresh` counts as 0. The task returns q; the environment marks the task done when q is not 0 and passes q to the reaction's processes, where `consumed = q * min(frac * resource, max)` (`max` alone for an infinite resource) and `bonus = consumed * value`. `type=add` adds `bonus` to the bonus, `type=mult` multiplies the bonus by it, `type=pow` multiplies by `2^bonus`, `type=lin` adds `bonus * task_count`. (`crates/environment/src/evaluation.rs`, `do_processes`; pinned for `add`, `mult` and `pow` by `quality_scales_what_a_reaction_consumes_and_so_the_bonus` in `crates/hardware/tests/field_arith.rs`.) With `requisite:max_count=1` the reaction pays once, at the first output whose quality reaches `thresh`, so the quality that pays is that of the first qualifying output (for `planes` often a first plane judged against an older output as the second).
- **Output buffer.** The organism's output buffer has the capacity `OUTPUT_SIZE_DEFAULT` = 1 unless an environment asks for more: a task reports its need through `EnvReqs::set_min_outputs` while it loads (as `matchstr`, `optimize`, `all-ones` do) and `Environment::output_size` becomes the largest need, which `Organism::new` uses. `planes` asks for r (2 for `halfadd`, up to 5 for `add` with `bits=4`), `packed` for 1, and the task asks for 2 inputs (the default is 3). An environment without a `planes` task keeps a buffer of 1, so the default path is unchanged.
- **Parallel mode.** `TaskLib::is_organism_local` lists `FieldArith` (it reads the two buffers only, writes nothing and prints nothing), so output tests with it run in the bursts like those of the other pure tasks (`local_output::output_is_local`). Pinned by `parallel_run_with_field_arith_does_not_depend_on_the_thread_count` (1, 2 and 4 threads, stock and modular set, `crates/avida/tests/field_arith.rs`) and by `the_ladder_passes_the_parallel_mode_gate_and_sizes_the_output_buffer`.
- **The ladder.** `environment-adder-ladder.yaml` has eight reactions in rung order, each on its own depletable grid resource and each once per organism (`requisite:max_count=1`): NOT, NAND (rung 0), XOR, AND, HALFADD_PLANES, HALFADD, ADD2_PLANES, ADD2 (rungs 1 to 6; the last four are `field_arith` with `thresh=0.75`, raised from 0.25 before any experiment because of the chance levels below). Resources are scaled to the stock 60 x 60 world (0.05 a cell and update in, 0.05 of the amount out).
- **Hand-written solutions** (`ladder-*.org`, tests in `crates/hardware/tests/field_arith.rs`): for each field rung and each of the stock heads set and the modular set an organism with no `add` and no `sub` that earns quality 1 on the task for random inputs (test CPU, 30 input triples each). Constants are constructible: `inc` and `shift-l` build `0x55555555` (and the three-bit pattern) in a loop, and the carry-propagation loop `(x, y) = (x ^ y, (x & y) << 1)` made of `nand` is an adder for any field width. Packed organisms need the mask constant and a carry loop of w + 1 rounds; `planes` organisms need neither (only bit 0 of every field is read, so shifts and `nand` suffice). Genome lengths (without the no-op modifiers): HALFADD_PLANES 32 (23), HALFADD 83 (57), ADD2_PLANES 90 (66), ADD2 113 (77) in the stock set; 49 (30), 82 (50), 78 (49), 89 (53) in the modular set, where the carry round is a subroutine. Every IO that outputs also reads the next input, which moves the two newest inputs: `planes` organisms read three inputs first, so that the pair at the last output is known to them (c and a for two planes, a and b for three).
- **Verifiable chance level.** A uniformly random output reaches q >= 0.25 for HALFADD in 10.8 % of runs (32 compared bits) and for ADD2 in 10.0 % (30 bits); q >= 0.5 in 0.35 % and 0.26 %. The result bits are not balanced (the carry bit of a half adder is 1 in a quarter of the fields), so constant outputs score above chance: the output 0 gives q = 0.25 on average for HALFADD, `a ^ b` gives 0.45 for both packed rungs, and the fixed top byte of every input fixes a quarter of the compared bits (a constant that is right there and 0 below gives 0.44 on HALFADD, 0.26 on ADD2). The best of seven naive outputs (0, -1, a, b, a ^ b, a & b, a | b) reaches q >= 0.25 in 96 to 99 % of runs, >= 0.5 in 38 %, >= 0.75 in under 1 %; so a threshold of 0.25 pays trivial outputs on the packed rungs, and 0.75 or more separates them.

## Gate and sensing instructions (opt-in extensions, 2026-11-19 and 2026-11-20)

New entries at the end of `CPU_INST_DEFS` (`crates/hardware/src/cpu_inst_lib.rs`, 566 to 600 entries, every C++ index unchanged); no set reaches them unless it names them. PORTING.md has the definitions.

- **`gate-0` to `gate-15`** (`hardware_cpu_insts.rs`, `gate_fn`): `?BX? = table(BX, CX)` bit by bit, bit `2a + b` of the table number being the result for operand bits `a` and `b`. They are in `CPU_PARALLEL_METHODS` (`parallel.rs`) and run in the bursts. A copy mutation in `h-copy` that reads a gate writes, with probability 0.75 (`GATE_STEP_PROB`), the gate one truth-table entry away (`copy_mutated_inst`); when the set lacks that gate, or otherwise, a random instruction of the set. Two more random numbers are drawn only when the instruction read is a gate, so sets without gates draw as before.
- **`pgate-0` to `pgate-15`**: the same functions under other names, so the rule above does not apply to them; a mutation writes a random instruction.
- **`sense-ahead-resource0`** (`hardware_cpu_insts_res.rs`): the amount of resource 0 in the faced cell into BX. **`sense-climb-resource0`**: the amount under the organism into BX, and the bonus multiplied by `amount / highest` when the amount beats the highest this organism has read in this life (`HardwareCpu::climb_best`, cleared at a divide). Neither is in the organism-local list of the parallel mode; they run in its sequential part.
- **Tests.** `gate_tests` in `hardware_cpu_insts.rs`: the truth tables; the gate rule's share on the four neighbouring tables and on the rest (40,000 draws, within 4 standard errors); the fall-back to a random instruction when a neighbour is missing from the set; and a non-gate drawing exactly as `random_inst` does. `crates/avida/tests/sense_ahead.rs`: the faced cell against the organism's own. `crates/tasks/tests/field_arith.rs`: the `_xc` steps compose `partial`.
- **Measured against the configured rates.** `tools/validate_mutations.py` places copies of one ancestor with empty cells around each, stops after their first divide and compares the unselected offspring with the ancestor: 23 mutation settings on three hardware types, 120 tests, none below p = 0.001 (2026-11-20), the gate rule included (78.0 % of gate mutations one table entry away, 77.7 % expected).

## Knockout map and reuse measures (port-only extension, 2026-11-15)

Experiments ask whether evolved organisms compute several rewarded tasks with shared code (a subroutine that several tasks use) or with separate copies. The instrument is the knockout (Lenski, Ofria, Pennock, Adami 2003, "The evolutionary origin of complex features"): replace one site by a null instruction, run the organism in the test CPU, see what it no longer does. Code: `world/src/knockout.rs` (data types, classification, measures, call targets, file text; pure, unit-tested), `world/src/knockout_run.rs` (the test runs, the two actions, `World::knockout_map_of`, `knockout_map_of_cell`), `session` (`Command::KnockoutMap`, `SessionEvent::KnockoutMap`), `viewmodel` (re-exports), `gui/src/ui/reuse.rs`, `tui/src/ui/reuse.rs`. Tests: `crates/avida/tests/knockout.rs`, `crates/session/tests/knockout.rs`, `crates/gui/tests/reuse.rs`, `crates/tui/tests/reuse.rs`.

**The knockout.** The NULL instruction of `cInstSet::ActivateNullInst`, which `CalcKnockouts`, the functional-modularity analysis and the MODES skeletons use (the set does nothing for it and mutation never produces it). It is activated in a private copy of the instruction sets (`World::modes_private_hw`), so the run's sets, numbering and output are untouched. `nop-X` is not used: a nop would extend a label that precedes the site and change what the neighbouring instruction does.

**Test conditions, and the inputs.** A fresh `TestCpu` and `CpuTestInfo::default()` per genome: `use_random_inputs` is off, so `Environment::setup_inputs` gives the fixed inputs `0x0f13149f 0x3308e53e 0x556241eb` (the receive buffer is fixed the same way), with no draw from any random stream. The RNG of the detached test world is nevertheless re-seeded (`KNOCKOUT_TEST_SEED`) before every test run, so an environment with `use_specific_inputs` and a mask (which draws) gives the base and every knockout the same inputs too; `sites_from` compares `test_cpu_inputs()` of every knockout with the base's and fails the map if they differ, and the inputs are written to the map file. Resources are held at the unexploited level of `PrintMODESData` (`modes_levels`, `RES_CONSTANT`), so a task paid from a limited resource with `initial=0` still counts. Fitness is `GetGenotypeFitness()`, the tasks are the test organism's `last_task_count`.

**Classes** (a site is the first that applies): lethal (the knockout's test fitness is 0: it does not replicate); shared (the knockout loses two or more of the tasks the base performs); task (exactly one); speed (no task lost, fitness differs from the base's, up or down); neutral. Lost = performed by the base with count above 0, and 0 in the knockout.

**Measures** (organism performing `k >= 2` tasks; lethal sites are left out of every task's site set `S_t`, they lose everything): shared sites = sites in two or more `S_t`; pairwise overlap `|S_t & S_u| / |S_t | S_u|` (undefined when both are empty); reuse index = mean of the defined pairwise overlaps; sites per task = `|S_t|`. Undefined values are `-1` in the data file and `-` in the views. Where the set has `call`: the call sites, the distinct targets (resolved statically as `Inst_Call` does, the complement of the label of nops after the `call`, searched forward without wrapping, with the register modulus of the set; a `call` without a label jumps to BX and counts as one "dynamic" target) and the call sites whose class is not neutral (lethal included).

*Known weaknesses.* A knockout that breaks an early step of a pipeline removes every later task, so a site "serves" tasks that depend on it as a predecessor, not as a subroutine they call: the shared-site count and the index cannot tell a shared subroutine from a shared prefix. Redundancy hides sites: with two copies of a computation neither knockout loses a task, so a reused module can show no shared sites and tasks with duplicated code look separate. One site at a time: epistasis is not seen. A task counts as performed or not; a knockout that lowers a count without zeroing it is a `speed` site.

**`modular-subroutine.org`** (57 sites, base fitness 3.96, NOT, NAND and AND): 15 lethal, 14 shared, 16 one-task, 12 speed, 0 neutral sites; the subroutine's `nand` (site 53) and both nops of its label (51, 52) lose all three tasks; the `call` sites 15, 23 and 39 lose {NOT}, {NAND, AND} and {AND}; shared sites 14, reuse index 0.383 (NOT and NAND 0.368, NOT and AND 0.241, NAND and AND 0.538), sites per task 11, 15 and 25, three call sites, one target, three non-neutral. The `return` and the end marker are lethal, the first `IO` (output of leftover junk) is shared because it consumes the first input.

**Headless output** (observers: a run with the actions writes the same other files, in the exact mode and in the parallel mode on 1 and 4 threads; `observation_only_*` in `crates/avida/tests/knockout.rs`):

```
u 0:1000:end PrintKnockoutMap [file=knockout_map.dat]
u 0:1000:end PrintReuseData   [file=reuse.dat]
```

Both take the dominant genotype at the update (once per update and genotype when both fire). `PrintKnockoutMap` appends a block per firing: comments (update, genotype id, length, base fitness, tasks performed, the task names in bit-string order, the test inputs, the column names), then a line per site: position, instruction, class, relative fitness, tasks lost as a string of `0` and `1` (one character per environment task, `1` = lost). `PrintReuseData` writes a row per firing: update, genotype id, genome length, tasks performed, shared sites, reuse index, call sites, non-neutral call sites, distinct call targets (the last column is an addition to the list asked for). Needs the genotype tracker (`DISABLE_GENOTYPE_CLASSIFICATION 0`); `xor-max` is refused as for MODES.

**`PrintReuseSurvey [filename=reuse_survey.dat] [top=5]`** (observer, 2026-11-15; `PrintReuseData` and `PrintKnockoutMap` are unchanged). `PrintReuseData` measures only the dominant genotype, which often performs no task or one, so most runs give "not defined". The survey looks at several genotypes: at the update it fires it takes the 50 most abundant living genotypes (ties by genotype id), keeps the first `top` that replicate in the test CPU and perform at least two tasks, and writes a row per genotype: update, genotype id, living organisms, genome length, tasks performed, shared sites, reuse index, call sites, non-neutral call sites, distinct call targets (the same definitions as `PrintReuseData`), subroutine sites and non-neutral subroutine sites. When no genotype qualifies it writes one row with `-1` in every column but the update, so the file shows that the survey ran. The 50 bounds the cost (each candidate costs one test run, each qualifying genotype `length + 1`). It draws nothing from the run's random stream and changes no state, so it works on a population loaded with `LoadPopulation` at update 0 (the genotype tracker classifies the loaded organisms and the test CPU needs no task history): `u begin LoadPopulation detail-100000.spop`, `u 0 PrintReuseSurvey`, `u 1 Exit` surveys a finished run.

*Subroutine reuse.* "Shared sites" cannot tell a shared subroutine from a shared prefix (see the weaknesses above), so the survey adds a measure from execution instead of knockouts. In the base test-CPU run of the genome (the depth-0 organism) a probe (`hardware/src/call_probe.rs`, attached through `CpuTestInfo::set_call_probe`, opt-in) keeps a shadow stack of the `call` sites that are active: a `call` that jumps pushes its own site, a `return` pops (an empty stack is left alone). Every executed instruction is recorded against the innermost active call site. Definition: **subroutine sites** = the genome sites that executed with at least two distinct call sites, taken one at a time, as the innermost active call; **non-neutral subroutine sites** = of those, the sites whose single-site knockout class is not neutral (lethal, shared, task or speed, as for call sites). A site called twice from one call site (a loop) counts once; the `call` and `return` of a body count in the context in which they run; a body entered only through another subroutine counts for its own innermost call site. Both columns are `-1` where the set has no `call` (or the hardware is not the heads CPU). Limits: only the sites the test run executes are seen, the shadow stack follows `call` and `return` and not the data stack (an organism that overwrites its return address is tracked by which `return` ran), and one input set is used. A hardware without a probe pays one `Option` test per instruction and per `call` or `return`; no other run changes (suite unchanged). On `modular-subroutine.org` the body `nand; return` is entered from three distinct call sites, so the count is 2 sites, both non-neutral (the label nops are skipped by the jump); a stock or straight-line organism gives 0, a stock set -1. Tests: `crates/avida/tests/knockout.rs` (`the_shared_subroutine_is_counted_as_subroutine_reuse`, `the_survey_*`, `survey_arguments`), `world/src/knockout.rs` and `hardware/src/call_probe.rs` unit tests.

**Cost.** One call is `length + 1` test runs on the thread that fires the event. Release build, modular-subroutine organism padded with executed `push`/`pop` pairs: 1.5 ms at 57 sites, 2.8 ms at 101, 7.5 ms at 201, 14.5 ms at 301 (the debug profile is about ten times slower; `cost_of_one_call_by_genome_length`, `--ignored`). The cost per run grows with the gestation (the test CPU allows 20 cycles per site), and an organism that loops until the cycle limit costs the most. An interval of 100 to 1000 updates costs nothing measurable; every update is affordable for genomes up to a few hundred sites in a small world.

**Front ends.** The desktop app's organism view has a Reuse tab (`ui/reuse.rs`): a button, then a header (tasks, shared sites, reuse index with the pairwise overlaps in its hover, sites per task, calls), a legend with class counts, and the genome as a strip of coloured cells (lethal `danger`, shared `accent`, one task `success`, speed `warning`, neutral `control_active`) over one row of marks per performed task in the task colour of the charts, wrapped into blocks of up to 50 to 60 sites; hovering a column gives the site, instruction, class, tasks lost and relative fitness. The request is `Command::KnockoutMap { cell, org_id }`: the simulation thread computes it between slice batches (the UI thread never does; the run waits for the milliseconds it takes), in the parallel mode inside an update the request waits for the update's end (or is refused when paused there), and the answer is `SessionEvent::KnockoutMap`. The terminal interface has the same on the debugger screen: key `v` (help, palette) replaces the memory listing by the measures, a line of class letters (`L` lethal, `S` shared, `T` one task, `R` speed, `.` neutral), and a listing with a row per site and a column per performed task; Up, Down, PageUp, PageDown, Home and End move the selection, `v` returns to the memory. A playback cannot run a test CPU (a recording carries the genome and the task names, not the instruction set, the reactions, resources and configuration), so it answers with one line saying so, on the desktop app's Organism tab and on the terminal's debugger screen.

## Arm comparison view (2026-11-15)

Both front ends can open a finished or running batch of runs, `<batch>/<arm>/seed-<N>/`, and show across seeds how far up the reward ladder each arm gets, with complexity and code-reuse measures. It reads data files only: nothing is simulated, nothing under `world`, `hardware` or `tasks` is involved, and no run is touched. An arm is any sub-directory of the batch that holds `seed-*` folders and a rung is a column of `tasks.dat` in file order, so the view compares whatever the experiment varies (instruction sets, sex, population size, changing environments) with no arm or task name built in.

Code: `viewmodel/src/compare/` (`mod.rs` the loader, types and summaries; `dat.rs` the data-file reader; `stats.rs` the statistics; `view.rs` the state both front ends keep), `gui/src/ui/compare.rs` and `gui/src/model/compare.rs`, `tui/src/compare.rs`. The module is pure (files in, plain data out) and has no dependency on `world`.

**Files read per run.** `tasks.dat` with `count.dat` (the ladder), `reuse.dat` (reuse index, shared sites, tasks performed of the dominant genotype), `dominant.dat` (genome length), the first `modes*.dat` (complexity) and the `.done` marker. Columns are found by the titles in the `#` header, never by position. `phylogeny.dat` and `knockout_map.dat` are not read. A missing optional file leaves its panels at "not recorded"; a file that cannot be read or lacks a column adds one line to `Run::problems` and the rest of the run still loads; a last line cut short by a run that is still writing is dropped. A run without `.done` is listed as running with the update of its last row and counts with its values so far; "Finished runs only" (`f` in the terminal) leaves such runs out. A run whose task count differs from the batch's most common one is reported and left out of the ladder.

**Rung names** are the reaction names, in order, of the first environment found at `<batch>/`, `<batch>/<arm>/`, a run folder, or `<batch>/../config-<arm>/` whose reaction count equals the task columns (the environment of the folder's `avida.yaml`, or in a batch run before the experiment file, its `environment.cfg`, whose names alone are read) (`tasks.dat` names tasks, and the four field-arithmetic rungs are all "FieldArith"). Otherwise the column titles are used, with `#<rung number>` after a title that occurs twice.

**Definitions.** A rung is held where `ladder::is_held` says so (at least one organism and at least 1 % of the organisms, the Ladder view's rule), judged per row of `tasks.dat` against the population in `count.dat` for the same update. Per run: the highest rung held at the last row and at any row, the first update at which each rung was held and was performed by any organism, and the last row of each measure (`-1` in a file is "undefined" and is not plotted). Per arm: per rung the number of seeds that ever held it and that hold it at the end, and for each measure the median and quartiles of the seeds that have a value (quartiles by linear interpolation at `(n - 1) p`). The rung measures are numbered from 1 for the lowest rung, 0 for none.

**Tests between two arms** (the pair is the arms when there are two, or the two the user picks; they are descriptive and the views never call a result significant, they show p with n):

- Mann-Whitney U, two-sided, for each measure. U is the rank sum of a sample (ties share the mean rank) minus `n (n + 1) / 2`. For `n_a + n_b` up to 40 the p-value is exact: the share of all `C(n_a + n_b, n_a)` ways of giving `n_a` of the pooled midranks to the first sample whose U lies at least as far from `n_a n_b / 2` as the observed one (a permutation test, computed by dynamic programming on integer doubled ranks, so ties are exact). Above 40 it is the normal approximation `p = erfc(z / sqrt 2)` with `z = (|U - n_a n_b / 2| - 0.5) / sd` (continuity correction) and `sd^2 = n_a n_b / 12 * ((N + 1) - sum(t^3 - t) / (N (N - 1)))` (tie correction, `t` the sizes of the tie groups). The method used is part of the result and shown in the hover.
- Fisher's exact test, two-sided, for "seeds that held rung k" on `[[a_yes, a_no], [b_yes, b_no]]`: the sum of the hypergeometric probabilities of all tables with the same margins that are no more probable than the observed one (relative tolerance 1e-7), once with "yes" meaning held at some row and once held at the last row.

**Desktop app.** `avida-gui --compare DIR`, "Compare runs..." on the start screen, in the experiment menu and in the command palette (Esc or Close leaves). Four panels: rungs reached (a dot per seed on each rung, per arm: filled is held at the end, a ring held earlier, faint never; a line under a dot is a running run; the count "k of n" and Fisher's p per rung), highest rung over time (a thin step line per seed in the arm's colour, the arm's median thick, a dot at the end of each running run, hover names the seed near the pointer), measures (a strip per measure with a dot per seed and arm, the median as a bar, the quartiles as a line, a ring for a running run, the Mann-Whitney p with both sample sizes), and a run table (selecting a row shows its folder and a "Copy path" button). Arm colours are the Okabe-Ito set in a fixed order by arm name, darkened a little in the light theme, and are never used for text. Explanations are in hovers.

**Terminal.** `avida-tui --compare DIR`: a Summary page (rung table of "ever (end)" per arm with Fisher's p; a measure table of median (quartiles) per arm with Mann-Whitney p and both n) and a Runs page. `Tab` switches, `f` finished runs only, `p` the next pair of arms, `r` reloads, `q` or Esc quits; `J` and `V` are not used. The summary fits 80 x 24 for eight rungs. The terminal page has no plots: the dot and line views are the desktop app's.

Tests: `viewmodel/src/compare/stats.rs` (quartiles, `erfc` against tabulated values, Mann-Whitney exact without and with ties and the normal approximation with its tie correction against hand-computed values and an enumeration, Fisher against hand-computed tables including `[[8, 2], [1, 5]]` = 400/11440), `viewmodel/src/compare/dat.rs`, `viewmodel/tests/compare.rs` (loader and summaries on batches the test writes under `CARGO_TARGET_TMPDIR`: arms, seed order, 1 % rule, running and cut runs, malformed and missing files, other task counts, rung names, medians over runs of different length, the state across a reload), `gui/tests/compare.rs` (opening from the command line and from the start screen, panel text, running runs, finished only, the folder of a run, three arms, screenshots in both themes at 1000 x 650 and 1280 x 800), `tui/tests/compare.rs` (buffers at 80 x 24 and 100 x 30, keys).

## Dropping extinct genotypes (`DROP_EXTINCT_GENOTYPES`, port-only, 2026-11-15)

The genotype arbiter keeps a genotype that has no living organism as a "historic" genotype for as long as another genotype names it as a parent, as the C++ does. With sexual reproduction (`divide-sex`: two parents per genotype) the lineages never coalesce, so most of every generation stays: a 60x60 run held 86,936 historic genotypes beside 3,522 living ones after 3,200 updates, and memory grew about 30 MB per 1,000 updates (about 100 MB with the ancestry observers on). `DROP_EXTINCT_GENOTYPES 1` (port-only like `EXECUTION_MODE`: `crates/config/src/port_settings.rs`; `genealogy.drop_extinct_genotypes` in the experiment file or with `-set`) trades the ancestry for flat memory. The default `0` is the C++ behaviour and every existing output is unchanged.

**What it does.** A genotype that loses its last living organism leaves the arena at the point where it would otherwise become historic, whether or not other genotypes name it as a parent. To make that possible a genotype keeps no handle to, and no passive reference on, its parents: `GenotypeArbiter::drop_extinct` makes `classify_new_unit_with_parents` skip the parent handles and the parents' passive-reference increment. What a child copies at birth stays: its own depth (parent depth plus one, so `PrintDepthHistogram` and the depth columns are exact), its parents' ids as text (`parent_str`), so the `parents` column of a `.spop` file (written from that text) and the `parents` property are exact. A genotype with another passive reference (a deme's founder genotype, `deme_founders.rs`) is still kept as historic until that reference is released, so the historic list is bounded by the number of such references and not by the run.

**What does not change.** Nothing an organism computes: genotype ids come from the same counter, a genotype that dies out and reappears is a new genotype in both modes (the genome hash lookup only finds active genotypes), thresholds, names and the best-genotype bookkeeping do not look at ancestors. `crates/avida/tests/drop_extinct.rs` compares `average.dat`, `count.dat`, `tasks.dat`, `dominant.dat`, `time.dat` and `resource.dat` of a sexual 20x20 run with and without the setting, in the exact and the parallel mode: byte-identical (time stamps aside). The same files of the 60x60 modular sexual configuration of the lab (3,000 updates, seed 1, with `PrintReuseData` and the `.m` resource maps) and of the stock asexual run (1,000 updates, seed 42) are identical as well. Only what describes the dropped history differs: `SavePopulation` writes one row per living genotype instead of one per genotype ever referenced (3,555 against 83,613 lines in the 3,000-update lab run), and the arbiter's provided values `ancestral` (historic count) and `coalescent_depth` (found by walking parent handles, so it stays -1) differ; no data file the port writes prints them.

**What it blocks.** Anything that walks parent links or the historic list stops with `unsupported: <what> with DROP_EXTINCT_GENOTYPES=1 (...)`:

| Needs ancestry | Where it stops |
|---|---|
| `PrintMODESData` (persistent lineages) | when the event file is read |
| `PrintPhylogenyMetrics`, `SavePhylogeny` (the tree of ancestors) | when the event file is read |
| the kin-distance test of the donate instructions (`MAX_DONATE_KIN_DIST` other than -1, `SexualAncestry`) | when an organism first executes it |

Left alone on purpose: the standard `Print*Data` files, `PrintKnockoutMap`, `PrintReuseData`, `PrintReuseSurvey`, `SavePopulation`, the depth histograms (depth is copied), `PrintLineageCounts` and the lineage label (a per-organism label, inherited, independent of the tracker), the clade arbiter and `PrintCClade*` (a separate arbiter), the phenotypic-plasticity summaries (they follow the arena), analyze mode (a separate world that reads a saved population as a file; the setting does not apply there). Front ends: the Ancestry map layer needs the phylogeny of the living genotypes; with the setting on the frame does not carry the layer (`viewmodel::capture`, like the Genotype layer that is not a value layer), and the GUI and TUI show their existing "no values yet" text and an empty family list. There is no one-line reason in the frame for a missing layer (playback's one-line reasons exist for commands, such as the knockout map, not for layers).

**Save and load.** `SavePopulation` works: the file holds the living genotypes (and any held by a deme founder), each with its `parents` ids, which name genotypes that are not in the file. `LoadPopulation` with the setting on reads such a file, and also a full file written by the default: the parent ids stay as text, every unreferenced loaded genotype is swept at the end of the first update (in this mode the sweep of the historic list repeats until it removes nothing), and the organisms are reactivated by genotype id as before. A file written with the setting on cannot be loaded with it off: the C++ loader looks the parents up and the port stops with `parent genotype N of genotype M not found`.

**Memory.** Sexual 60x60 modular configuration of the lab (seed 1, release build, no ancestry actions), physical footprint from `vmmap --summary`:

| update | off | on |
|---|---|---|
| 2,000 | 111.1 MB | 52.4 MB |
| 4,000 | 184.9 MB | 53.4 MB |
| 6,000 | 191.3 MB | 54.7 MB |

Cost: none measurable; the removal is the one `removeGenotype` already does, a little earlier.

## The experiment file (`avida.yaml`, 2026-11-18)

By the maintainer's decision of 2026-11-18 one YAML file per experiment
replaces `avida.cfg`, the environment file, the events file and the
instruction-set files (PORTING.md, "The experiment file departs from the
C++ input format"; the user's description is
`docs/reference/config-yaml.md`). All of it lives in `config`:

| File | Role |
|---|---|
| `naming.rs` | the grouping table: one row per C++ setting and per port-only setting, its group and key, the sections that replaced `EVENT_FILE`, `ENVIRONMENT_FILE` and `INST_SET`; the single source of truth for the loader, `-set`, the converter, the settings editors and the reference page |
| `yaml.rs` | the YAML reader (on the event stream of `saphyr-parser` 0.0.6): scalars stay the text of the file, anchors and aliases are refused, every node keeps its line for messages |
| `experiment.rs` | `Config::load_experiment` and `Config::set_path`: unknown keys refused with the nearest name, values parsed by the old `load_str_by_id`, sections rendered into the C++ lines the unchanged parsers read (`Experiment::environment`, `Experiment::events`, `Config::instsets`), stock instruction sets by name (`heads_default`, `heads_sex`, `heads_modular`, `smt`, `transsmt`, `experimental`, compiled in from their files in `avida-core/support/config`), section files |
| `convert.rs` | `avida convert DIR` and `avida convert --section`: the old files to YAML, with the helpers tests use to vary a copied experiment (`section_lines`, `replace_section`, `without_old_input_notes`). It is the only user of the old readers: `Config::load_old_format` (`cAvidaConfig::Load`, with `#include`, `-def` and `$NAME`) and `Config::set_from_map` are private to the crate, and the converter parses old environment, events and instruction-set files itself |
| `reference.rs` | writes `docs/reference/config-yaml.md` from the table (`AVIDA_UPDATE_REFERENCE=1` with the test `reference_page_is_current`) |

The world reads only what `Config` holds: `World::new(config, working_dir)`
takes the environment, event and instruction-set lines from it.
`Environment::load` (the environment file through `cInitFile`),
`convert_legacy_inst_set_file` (`ConvertLegacyInstSetFile`), `Config::print`
(the old `avida.cfg` writer) and the `-def` variables of `World::new` and
`AnalyzeWorld::initialize` were removed with the old format; `-def` on the
command line stops with a message that names `-set` and `avida convert`. The front ends get the
settings with their dotted path, group and old name through
`session::Setting`; their settings editors make `-set group.key value`
arguments for a restart, and the new-experiment bundle of the desktop
application is `avida.yaml` with the stock section files and organisms.

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
