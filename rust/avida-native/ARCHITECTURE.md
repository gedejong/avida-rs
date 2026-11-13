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
| `session` | owns a `World` on a simulation thread for the front ends: commands in, snapshots out, speed control, the debugger (breakpoints, `StopWhen`), bounded `shutdown` (`Drop` waits at most 2 s, then detaches the thread). Also records the arena to a file (`recording`, `Command::Record`), plays a recording back through the same `Session` handle (`Session::play`), and ships the headless recorder `avida-rec` (see "Recording and playback") |
| `gui` (`avida-gui`) | the egui/eframe desktop application: map, charts, organism and debugger views, New experiment, CSV/PNG export; `--play FILE` plays a recording, `--record FILE` records the run |
| `tui` (`avida-tui`) | the ratatui terminal interface on the same `session` and `viewmodel` layers; pty-tested for signals, hang-up and terminal restoration; `--play FILE` and `--record FILE` as in the GUI |

Whole-suite comparison against the C++ binary (`avida-core/tests`: 122 test directories, of which the 121 with a `test_list` are run, `string_producing_and_donating` has none; 84 run to a zero exit in C++): 85 are byte-identical (those 84 and `analyze_resize_max_batches`, whose exit code 1 the port reproduces; ignoring timestamps, the time-based seed and the data-directory path), 34 more fail the same way the C++ does, none differs (`collect_specification_analyze` prints -1 for the collect-specification counts as the FFI-migrated reference binary does, whose `last_collect_spec_counts` stays empty), 1 stops on a named unported feature (`SLICING_METHOD 4`, on which the C++ crashes), and for 1 (`_mutn_rate`) the reference binary times out. `crates/avida/tests/consistency.rs` pins the matching ones that have checked-in `expected/` output; `crates/avida/tests/stepper.rs` pins that pausing at arbitrary slice boundaries does not change a run.

Deme cell events, deme predicates and the deme network are in `world/src/deme_events.rs`. The reference binary is built without Boost, so the first use of a deme network prints the `cDemeNetwork requires Boost` error and exits with code 50 there and here; the two uninitialised `cDemeCellEvent` members (`m_use_gradient`, `m_eventID`) stop the run with `unsupported:` when read. The energy model runs (`energy.rs`, the birth chamber, `ReplaceDeme`, the deme printers), `FRAC_ENERGY_TRANSFER > 0` included (the deme cell energy that `InsertOrganism` takes up and `RemoveOrganism` gives back, whose `ModifyCell` calls also double the cell's other deme resources, as they do in the C++), and so do `APPLY_ENERGY_METHOD` 1 (energy applied on task completion) and 2 (on `sleep`). `LOG_SLEEP_TIMES` runs until an organism outside cell 0 dies or sleeps, where the C++ indexes past its single `sleep_log` array and the port stops. Avatars run with the heads CPU and the experimental hardware, predator/prey and the look sensor with the experimental hardware (`avatars.rs`, `pred_prey.rs`, `org_sensor.rs`); with `NEURAL_NETWORKING` and `USE_AVATARS 2` messages leave through the sender's output avatars and reach every input avatar in the target cell, the sender's own included under `SELF_COMMUNICATION` (`messaging.rs`; its own full `MESSAGE_RECV_BUFFER_SIZE` buffer drops the message as `ReceiveMessage` does, the hardware telling the world the buffer length before each send), and a `FindAV` that indexes outside the organism's avatars, which the C++ reads out of bounds, stops the run; the mini and micro traces those experiments print (`PrintMiniTraces`, `PrintMicroTraces`, `LoadMiniTraceQ`, `PrintReproData`, `LoadPopulation` with `load_traceq`) are in `world/src/mini_trace.rs` and `hardware/src/mini_trace.rs`, the top-navigation trace (`PrintTopNavTrace`, `PrintNavTrace`, `FlushTopNavTrace`) in `world/src/mini_trace/nav_trace.rs`. Where the C++ prints a navigation trace that is empty (the heads CPU and TransSMT record none, nor do organisms born after the queue was set) it reads before an empty array, and an environment without reactions makes it recurse until the stack overflows; the port stops with an error in both cases.

Event-file constructors that call `exit(1)` (`CompeteDemesByTaskCountAndEfficiency` without an initial deme energy, `SetEnvironmentInputs` with other than three inputs or inputs that do not begin 0F, 33, 55) print the C++ message to standard error and exit with status 1 (`actions::cerr_exit`). `SetRedundancy` and `SetProbFail` change the one shared `InstSet` (atomics and a lock inside `genome/src/inst_set.rs`), so every hardware type and the parasites see the change. `InjectClone` with `USE_AVATARS` places the clone's avatar (`PlaceAvatar` next to the original, `CopyDeme`, `DivideDemes`, `SpawnDeme`, `CompeteDemes` copies) and `MixPopulation` with `USE_AVATARS` leaves the avatars where they are; `ResetDemes` and the in-place `CompeteDemes` reset with `USE_AVATARS` still stop, because the reference binary crashes there (`PlaceAvatar` reads the original organism after `ActivateOrganism` deleted it). `ChangeEnvironment` and the other callers of `UpdateResourceCount` run with deme resources of global geometry; a grid, torus or partial deme resource still stops (unsized deme grids).

The birth chamber has every selection handler of the original, the per-deme `cBirthDemeHandler` included (`world/src/birth_chamber.rs`). State grids (`STATE_GRID` in the environment, the `sg-*` instructions and their tasks) run. Predatory, damaging and deadly gradient resources act on the population through `world/src/habitat.rs`: the population's resource count is read through `World::with_pop_resources`, which carries out the kills and injuries `cGradientCount` asks for. The resource reads of `OrgInterface` carry the running organism's state, so a damaging gradient that injures it during such a read injures that state; the same holds for the group count of an offspring's group placement and for `UpdateRandomResources`, which now receive it. The resource update that the HGT fragment bookkeeping (`AdjustHGTResource`, from `AddGenomeFragments` on a kill, `ClearFragments` and the fragment taken in `DoHGTMutation`) starts carries the running organism's state as well, so a damaging gradient that injures it there acts on that state (`crates/avida/tests/actions/gap_hgt_damaging_*`); path and den resources next to a habitat still stop the run (the C++ recurses until the stack overflows) and the avatar branches. A deadly or predatory hit on the running organism sets `to_die`, which each hardware folds in at the end of `SingleProcess` (`OrgInterface::take_habitat_to_die`) where the C++ reads `GetToDie()`: speculative execution (`m_spec_die`) and implicit reproduction (`Die` before `CheckImplicitRepro`) follow the C++. Lateral gene transfer on divide (`GetLGTFragment`) is in `world/src/hgt.rs`, and so is horizontal gene transfer (`ENABLE_HGT`: the fragments dying organisms leave in their cells, uptake by competence, conjugation and donation, fragment metabolism, `PrintHGTData`). The match and trimmed fragment placements (`HGT_FRAGMENT_SELECTION` 0 and 1) stop the run when reached, because `cGenomeUtil::FindSubstringMatch` returns a value read from memory it has just freed; so does a gradient habitat kill under HGT with competence, which crashes the C++ when the kill re-enters the resource update. `Inst_SetRandPFTOnce` under `MAX_PREY_BT` in a test CPU run by a divide (`Divide_TestFitnessMeasures`, the genotype's test fitness) kills the population organism it picks when the test ends (`HostOp::KillCell`, `crates/avida/tests/actions/gap_exp_max_prey_bt_test_cpu`); the same from a plasticity test or from a test CPU run by an event, a pick of the dividing organism itself, a second kill in one test, and a kill that draws random numbers still stop the run. The `live_org_forage_targets` stop of the test CPU is unreachable, since every test CPU run from a live organism's step lends that organism's forage target, but stays as a guard.

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
sex families, and, in runs whose environment has global resources or non-pure tasks,
every output test (the burst stops before the instruction as before).

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

What stays sequential, and why: global (well-mixed) resources, and mixed
environments. One pool is read and consumed by every organism, so the result
of an output test depends on the order of all the others, and on the resource
clock at the instruction. A deterministic scheme exists in principle (stop the
burst *before* an `IO` whose triggered reaction has a global resource, which
needs a non-mutating look at the register the handler will output and a probe
evaluation of the task) but it was not built: the handler consumes its nop
modifier and the output buffer before the test, so the probe has to be written
for each of the four output handlers. Such runs keep stopping the burst
before each `IO`, as before this section.

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
