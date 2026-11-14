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
resources that are not per-cell grids or global pools (or a reaction that
uses both), every output test (the burst stops before the instruction as
before).

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
together is handled in "Mixed grids and global pools in the burst").

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

**Result, global pools** (`environment-9resource.cfg`, the classic limited
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
it is a pool, and refuses the world if any resource is a gradient, a
`PARTIAL` list or a grid without a cell for every cell of the world. The
world uses it when there are both kinds (all one kind keeps the two gates
above, unchanged). `output_is_local_mixed` then asks what
`output_is_local_global` asks (no requisite that names other reactions, none
on divide, the per-cell conditions) and one more thing: no reaction takes
from or gives to both kinds. A reaction is then entirely in the burst or
entirely deferred, which is what makes the split sound: the processes of
different reactions meet only in `ReactionResult`'s sums, and no process
reads what a reaction of the other kind consumed (`consumed(res)` is
per-resource, and a resource is of one kind).

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
`misc/environment-9resource.cfg`, each at 1, 4 and 8 threads where run, the
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
