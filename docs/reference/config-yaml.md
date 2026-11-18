# The experiment file (avida.yaml)

An Avida experiment is one YAML file, `avida.yaml`, in the folder the run
starts in. It holds the settings that differ from their defaults, the
instruction sets, the environment and the events. Organism files (`.org`)
and analyze scripts (`analyze.cfg`) stay files of their own, which the
events and the settings name by path.

The C++ program read four kinds of files instead (`avida.cfg`,
`environment.cfg`, `events.cfg` and an instruction-set file). Avida refuses
them now and points to `avida convert`, which writes the experiment file from
such a folder (see [Converting an old folder](#converting-an-old-folder)).

This page is generated from the table in
`rust/avida-native/crates/config/src/naming.rs`, which is the one place where
the group and key of a setting are defined.

## Layout

```yaml
world:
  x: 60
  y: 60
mutation:
  copy_prob: 0.0075

instruction_sets:
  - heads_default

environment:
  resources:
    - {name: glucose, inflow: 100, outflow: 0.01}
  reactions:
    - name: NOT
      task: not
      process: {resource: glucose, value: 1.0, type: pow}
      requisite: {max_count: 1}

events:
  - {at: begin, action: Inject, args: default-heads.org}
  - {at: 0, every: 100, action: PrintAverageData}
  - {at: 100000, action: Exit}
```

The top level holds the groups of settings (listed
[below](#settings-by-group)) and the three sections `instruction_sets`,
`environment` and `events`. Every part may be left out. A setting that is
left out keeps its default, a missing section is empty, and a run without an
instruction set stops with `error: no instruction sets defined`.

A key that Avida does not know is refused with the file, the line and the
nearest known name, for example
`avida.yaml:4: unknown setting 'mutation.copy_prop'; did you mean 'mutation.copy_prob'?`.
An old setting name gets its new place in the answer.

### Values

Every value is read by the parser the C++ program used for it, so a number
means what it meant in `avida.cfg`. A flag is an on/off switch: it takes
`true` or `false`, and `1` or `0`, and refuses anything else (`avida convert`
writes `true` and `false`). A setting whose type is an integer also takes
`true` and `false` (1 and 0). A number setting refuses a value that does not
start like a number (the C++ program read such a value as 0). YAML does not interpret anything here: `1.0`, `0x10` and `yes` reach the
parser as the text written.

### The command line

`-set` names a setting by its group and key: `-set mutation.copy_prob 0.0025`,
`-set world.x 100`. It also replaces a section with the content of a file
that holds just that section: `-set events events-long.yaml`,
`-set environment environment-poor.yaml`. `-set instruction_sets` takes the
name of a stock set or such a file. A list cannot be given on the command
line. `-c FILE` reads another experiment file than `avida.yaml`. The old
`-def` option is gone, since the experiment file has no `#include` or `$NAME`
substitution.

### Section files

Each section may be the name of a YAML file instead of its content. That
file holds the section under the same key and nothing else, so one set of
events can serve several experiments:

```yaml
# avida.yaml
events: events-long.yaml

# events-long.yaml
events:
  - {at: begin, action: Inject, args: default-heads.org}
  - {at: 200000, action: Exit}
```

Paths are relative to the folder the run starts in. A section file cannot
name a further file, and there is no general include.

## Instruction sets

`instruction_sets` is a list. An item is one of three things.

- The name of a stock set: `heads_default`, `heads_sex`, `heads_modular`, `smt`, `transsmt` and `experimental`. These ship with
  Avida (`avida-core/support/config/instset-*.yaml`), so an experiment needs
  no copy.
- The name of a YAML file (ending in `.yaml` or `.yml`) whose
  `instruction_sets` list is taken in.
- A set written out, with the keys `name`, `hardware_type`, `stack_size`, `uops_per_cycle` and `instructions`.

```yaml
instruction_sets:
  - name: my_heads
    hardware_type: 0
    instructions:
      - nop-A
      - {name: h-alloc, cost: 2}
      - {name: inc, redundancy: 3}
```

`hardware_type` is the C++ `HARDWARE_TYPE` (0 heads, 1 SMT, 2 transposon
SMT, 3 experimental, 4 GP8, 5 BCR); `stack_size` and `uops_per_cycle` are
optional. An instruction is its name, or a mapping with its `name` and any of
`redundancy`, `cost`, `initial_cost`, `energy_cost`, `addl_time_cost`, `female_cost`, `choosy_female_cost`, `post_cost`, `prob_fail`, `res_cost`, `fem_res_cost`, `bonus_cost` and `inst_code`. The first set is the default set of the run, as the first
`INSTSET` was in `avida.cfg`.

## Environment

`environment` is a mapping with up to four lists, which are loaded in this
order: `resources`, `grids`, `reactions` and `set_active`. Its key
`input_and_mask` holds the mask of the environment line `INPUT_AND_MASK`
(hex with `0x`, such as `0xFF6DB6DB`, or decimal), which the file passes on
as written.

A resource has a `name` and its parameters. A plain resource (the C++
`RESOURCE`) takes `inflow`, `outflow`, `initial`, `geometry`, `cells`, `inflowx1`, `inflowx`, `inflowx2`, `inflowy1`, `inflowy`, `inflowy2`, `outflowx1`, `outflowx`, `outflowx2`, `outflowy1`, `outflowy`, `outflowy2`, `xdiffuse`, `xgravity`, `ydiffuse`, `ygravity`, `deme`, `collectable`, `energy` and `hgt`. `kind: cell` makes a resource of
single cells (`CELL`), with `cells` (the cell list, such as `0..9,15`) and
`initial`, `inflow` and `outflow`. `kind: gradient` makes a gradient resource
(`GRADIENT_RESOURCE`) with `peakx`, `peaky`, `height`, `spread`, `plateau`, `decay`, `max_x`, `max_y`, `min_x`, `min_y`, `move_a_scaler`, `updatestep`, `halo`, `halo_inner_radius`, `halo_anchor_x`, `halo_anchor_y`, `move_speed`, `move_resistance`, `halo_width`, `plateau_inflow`, `plateau_outflow`, `cone_inflow`, `cone_outflow`, `gradient_inflow`, `initial`, `common`, `floor`, `habitat`, `min_size`, `max_size`, `config`, `count`, `resistance`, `damage`, `deadly`, `path`, `hammer`, `threshold` and `refuge`. A resource named twice is
changed by its second entry, as a second `RESOURCE` or `CELL` line changed it.

A state grid (`GRID`) has a `name` and `width`, `height`, `initx`, `inity`, `initfacing`, `states` and `grid`.

A reaction has the keys `name`, `task`, `args`, `process`, `requisite` and `context_requisite`. `args` holds the task's
arguments, as a mapping (`{function: 9, binary: 1}`) or as the text the C++
line had after the task's colon. The file passes the arguments on as
written, and the task checks them. `process`, `requisite` and
`context_requisite` are a mapping or a list of mappings, one per C++
`process:`, `requisite:` or `context_requisite:` entry. A process takes
`resource`, `value`, `type`, `max`, `min`, `frac`, `ksubm`, `product`, `conversion`, `inst`, `random`, `lethal`, `sterilize`, `deme`, `germline`, `detect`, `threshold`, `detectionerror`, `string`, `depletable`, `phenplastbonus` and `internal`. A requisite takes `reaction`, `noreaction`, `min_count`, `max_count`, `reaction_min_count`, `reaction_max_count`, `divide_only`, `min_tot_count`, `max_tot_count`, `parasite_only` and `cellbox` (`cellbox` only
outside a context requisite); `reaction` and `noreaction` may be lists. A
reaction with only a `name` is declared before its definition, as a
`REACTION` line with only a name was.

```yaml
environment:
  resources:
    - {name: glucose, inflow: 100, outflow: 0.01}
    - {name: spot, kind: cell, cells: '0..9', initial: 5}
  reactions:
    - name: OPT
      task: optimize
      args: {function: 9, binary: 1}
      process: [{value: 1.0, type: pow}, {resource: glucose, value: 0.5}]
      requisite: {reaction: [NOT, AND], max_count: 1}
```

A value in the environment is one word without `:`, since it is rendered into
the C++ line format.

## Events

`events` is a list of mappings with the keys `trigger`, `at`, `every`, `until`, `action` and `args`.

| Key | Meaning |
|---|---|
| `trigger` | `update` (the default), `generation`, `births`, `org_id` or `immediate` |
| `at` | when the event first runs: `begin` (the default) or a number |
| `every` | the interval: a number, `all` (every update) or `once` |
| `until` | the last time it runs: a number or `end` |
| `action` | the action, as listed in [Actions](actions.md) |
| `args` | the action's arguments, as text or as a list of words |

`until` needs `every`, and an `immediate` event has no timing. The C++ line
`u 0:100:end PrintAverageData` is `{at: 0, every: 100, until: end, action:
PrintAverageData}`.

## Converting an old folder

`avida convert DIR` reads `DIR/avida.cfg` with its includes, its
environment, events and instruction-set files, and writes `DIR/avida.yaml`.
The old files are read by the old readers, so `#include`, `-def` values and
`$NAME` substitution are resolved on the way. Only the settings that differ
from their defaults are written; `--full` writes every setting with its
description, as in the stock `avida-core/support/config/avida.yaml`. An
instruction set that equals a stock set is written as the stock set's name.

```
avida convert arm-a                        # writes arm-a/avida.yaml
avida convert arm-a -set EVENT_FILE events-long.cfg
avida convert arm-a -def INST_SET instset-heads-modular.cfg
avida convert --section events events-long.cfg   # writes events-long.yaml
```

`-c FILE` reads another old configuration file, `-set NAME VALUE` changes an
old setting before writing (with the old name), `-o FILE` writes elsewhere
(`-` for standard output), `--remove-old` deletes the old files that were
read and `--force` overwrites an existing file. With `--section`, one old
environment, events or instruction-set file becomes a section file.

A run of the old kind such as
`avida -s 7 -set COPY_MUT_PROB 0.01 -set EVENT_FILE events-long.cfg -set DATA_DIR out-7`
becomes, after `avida convert arm-a` and
`avida convert arm-a --section events events-long.cfg`,
`avida -s 7 -set mutation.copy_prob 0.01 -set events events-long.yaml -set general.data_dir out-7`.

## Settings by group

Each group lists its settings with the C++ name, the type and the default.
The descriptions are those of the C++ program, with `true` and `false` for the 0 and 1 of a flag.

### `general`

General settings.

| Key | C++ name | Type | Default | Description |
|---|---|---|---|---|
| `verbosity` | `VERBOSITY` | integer | `1` | 0 = No output at all 1 = Normal output 2 = Verbose output, detailing progress 3 = High level of details, as available 4 = Print Debug Information, as applicable |
| `random_seed` | `RANDOM_SEED` | integer | `-1` | Random number seed (<0 for based on time) |
| `speculative` | `SPECULATIVE` | flag | `true` | Enable speculative execution (pre-execute instructions that don't affect other organisms) |
| `population_cap` | `POPULATION_CAP` | integer | `0` | Carrying capacity in number of organisms (use 0 for no cap) |
| `population_cap_eldest` | `POP_CAP_ELDEST` | integer | `0` | Carrying capacity in number of organisms (use 0 for no cap). Will kill oldest organism in population, but still use birth method to place new offspring. |
| `data_dir` | `DATA_DIR` | text | `data` | Directory in which config files are found |

### `world`

World size and topology.

| Key | C++ name | Type | Default | Description |
|---|---|---|---|---|
| `x` | `WORLD_X` | integer | `60` | Width of the Avida world |
| `y` | `WORLD_Y` | integer | `60` | Height of the Avida world |
| `geometry` | `WORLD_GEOMETRY` | integer | `2` | 1 = Bounded Grid (WOLRD_X x WORLD_Y) 2 = Toroidal Grid (WOLRD_X x WORLD_Y; wraps at edges 3 = Clique (all population cells are connected) 4 = Hexagonal grid 5 = Partial 6 = 3D Lattice (under development) 7 = Random connected 8 = Scale-free (detailed below) |
| `scale_free_m` | `SCALE_FREE_M` | integer | `3` | Number of connections per cell in a scale-free geometry |
| `scale_free_alpha` | `SCALE_FREE_ALPHA` | number | `1.0` | Attachment power (1=linear) |
| `scale_free_zero_appeal` | `SCALE_FREE_ZERO_APPEAL` | number | `0.0` | Appeal of cells with zero connections |

### `mutation`

Mutation rates.

| Key | C++ name | Type | Default | Description |
|---|---|---|---|---|
| `copy_prob` | `COPY_MUT_PROB` | number | `0.0075` | Substitution rate (per copy) |
| `copy_ins_prob` | `COPY_INS_PROB` | number | `0.0` | Insertion rate (per copy) |
| `copy_del_prob` | `COPY_DEL_PROB` | number | `0.0` | Deletion rate (per copy) |
| `copy_uniform_prob` | `COPY_UNIFORM_PROB` | number | `0.0` | Uniform mutation probability (per copy) - Randomly apply insertion, deletion or substition mutation |
| `copy_slip_prob` | `COPY_SLIP_PROB` | number | `0.0` | Slip rate (per copy) |
| `point_prob` | `POINT_MUT_PROB` | number | `0.0` | Point (Cosmic-Ray) substitution rate (per-location per update) |
| `point_ins_prob` | `POINT_INS_PROB` | number | `0.0` | Point (Cosmic-Ray) insertion rate (per-location per update) |
| `point_del_prob` | `POINT_DEL_PROB` | number | `0.0` | Point (Cosmic-Ray) deletion rate (per-location per update) |
| `inst_point_prob` | `INST_POINT_MUT_PROB` | number | `0.0` | Mutation rate (per-location per time instruction (point-mut) is executed) |
| `inst_point_slope` | `INST_POINT_MUT_SLOPE` | number | `0.0` | Slope for point mutation rate |
| `inst_point_repair_cost` | `INST_POINT_REPAIR_COST` | integer | `0` | The cost, in cycles, of avoiding mutations when the point-mut instruction is executed |
| `point_repair_start` | `POINT_MUT_REPAIR_START` | flag | `false` | The starting condition for repairs (true = on, false = off) |
| `div_prob` | `DIV_MUT_PROB` | number | `0.0` | Substitution rate (per site, applied on divide) |
| `div_ins_prob` | `DIV_INS_PROB` | number | `0.0` | Insertion rate (per site, applied on divide) |
| `div_del_prob` | `DIV_DEL_PROB` | number | `0.0` | Deletion rate (per site, applied on divide) |
| `div_uniform_prob` | `DIV_UNIFORM_PROB` | number | `0.0` | Uniform mutation probability (per site, applied on divide) - Randomly apply insertion, deletion or point mutation |
| `div_slip_prob` | `DIV_SLIP_PROB` | number | `0.0` | Slip rate (per site, applied on divide) |
| `div_trans_prob` | `DIV_TRANS_PROB` | number | `0.0` | Translocation rate (per site, applied on divide) |
| `div_lgt_prob` | `DIV_LGT_PROB` | number | `0.0` | Lateral Gene Transfer rate (per site, applied on divide) |
| `divide_prob` | `DIVIDE_MUT_PROB` | number | `0.0` | Substitution rate (max one, per divide) |
| `divide_ins_prob` | `DIVIDE_INS_PROB` | number | `0.05` | Insertion rate (max one, per divide) |
| `divide_del_prob` | `DIVIDE_DEL_PROB` | number | `0.05` | Deletion rate (max one, per divide) |
| `divide_uniform_prob` | `DIVIDE_UNIFORM_PROB` | number | `0.0` | Uniform mutation probability (per divide) - Randomly apply insertion, deletion or point mutation |
| `divide_slip_prob` | `DIVIDE_SLIP_PROB` | number | `0.0` | Slip rate (per divide) - creates large deletions/duplications |
| `divide_trans_prob` | `DIVIDE_TRANS_PROB` | number | `0.0` | Translocation rate (per divide) - creates large deletions/duplications |
| `divide_lgt_prob` | `DIVIDE_LGT_PROB` | number | `0.0` | Lateral Gene Transfer rate (per divide) - creates large deletions/duplications |
| `divide_poisson_mean` | `DIVIDE_POISSON_MUT_MEAN` | number | `0.0` | Substitution rate (Poisson distributed, per divide) |
| `divide_poisson_ins_mean` | `DIVIDE_POISSON_INS_MEAN` | number | `0.0` | Insertion rate (Poisson distributed, per divide) |
| `divide_poisson_del_mean` | `DIVIDE_POISSON_DEL_MEAN` | number | `0.0` | Deletion rate (Poisson distributed, per divide) |
| `divide_poisson_slip_mean` | `DIVIDE_POISSON_SLIP_MEAN` | number | `0.0` | Slip rate (Poisson distributed, per divide) |
| `divide_poisson_trans_mean` | `DIVIDE_POISSON_TRANS_MEAN` | number | `0.0` | Translocation rate (Poisson distributed, per divide) |
| `divide_poisson_lgt_mean` | `DIVIDE_POISSON_LGT_MEAN` | number | `0.0` | Lateral Gene Transfer rate (Poisson distributed, per divide) |
| `inject_prob` | `INJECT_MUT_PROB` | number | `0.0` | Substitution rate (per site, applied on inject) |
| `inject_ins_prob` | `INJECT_INS_PROB` | number | `0.0` | Insertion rate (per site, applied on inject) |
| `inject_del_prob` | `INJECT_DEL_PROB` | number | `0.0` | Deletion rate (per site, applied on inject) |
| `slip_fill_mode` | `SLIP_FILL_MODE` | integer | `0` | Fill insertions from slip mutations with: 0 = Duplication 1 = nop-X 2 = Random 3 = Scrambled 4 = nop-C |
| `slip_copy_mode` | `SLIP_COPY_MODE` | integer | `0` | How to handle 'on-copy' slip mutations: 0 = actual read head slip 1 = instant large mutation (obeys slip mode) |
| `trans_fill_mode` | `TRANS_FILL_MODE` | integer | `0` | Fill insertions from translocation mutations with: 0 = Duplication 1 = Scrambled |
| `lgt_fill_mode` | `LGT_FILL_MODE` | integer | `0` | Fill insertions from lateral gene transfer mutations with: 0 = Duplication 1 = scrambled |
| `lgt_source_region` | `LGT_SOURCE_REGION` | integer | `0` | Extract gene for lateral transfer from: 0 = Local Neighborhood 1 = Entire Population |
| `parent_prob` | `PARENT_MUT_PROB` | number | `0.0` | Parent substitution rate (per-site, applied on divide) |
| `parent_ins_prob` | `PARENT_INS_PROB` | number | `0.0` | Parent insertion rate (per-site, applied on divide) |
| `parent_del_prob` | `PARENT_DEL_PROB` | number | `0.0` | Parent deletion rate (per-site, applied on divide) |
| `special_line` | `SPECIAL_MUT_LINE` | integer | `-1` | If this is >= 0, ONLY this line is mutated |
| `meta_copy_prob` | `META_COPY_MUT` | number | `0.0` | Prob. of copy mutation rate changing (per gen) |
| `meta_std_dev` | `META_STD_DEV` | number | `0.0` | Standard deviation of meta mutation size. |
| `rate_source` | `MUT_RATE_SOURCE` | integer | `1` | 1 = Mutation rates determined by environment. 2 = Mutation rates inherited from parent. |

### `reproduction`

Birth and death.

| Key | C++ name | Type | Default | Description |
|---|---|---|---|---|
| `divide_failure_resets` | `DIVIDE_FAILURE_RESETS` | flag | `false` | When Divide fails, organisms are interally reset |
| `birth_method` | `BIRTH_METHOD` | integer | `0` | Which organism should be replaced when a birth occurs? 0 = Random organism in neighborhood 1 = Oldest in neighborhood 2 = Largest Age/Merit in neighborhood 3 = None (use only empty cells in neighborhood) 4 = Random from population (Mass Action) 5 = Oldest in entire population 6 = Random within deme 7 = Organism faced by parent 8 = Next grid cell (id+1) 9 = Largest energy used in entire population 10 = Largest energy used in neighborhood 11 = Local neighborhood dispersal 12 = Kill offpsring after recording birth stats (for behavioral trials) 13 = Kill parent and offpsring (for behavioral trials) |
| `prefer_empty` | `PREFER_EMPTY` | flag | `true` | Overide BIRTH_METHOD to preferentially choose empty cells for offsping? |
| `allow_parent` | `ALLOW_PARENT` | flag | `true` | Should parents be considered when deciding where to place offspring? |
| `dispersal_rate` | `DISPERSAL_RATE` | number | `0.0` | Rate of dispersal under birth method 11 (poisson distributed random connection list hops) |
| `death_prob` | `DEATH_PROB` | number | `0.0` | Probability of death when dividing. (@MAM: actually seems to be applied per-update?) |
| `death_prob_parent` | `DEATH_PROB_PARENT` | number | `0.0` | Probability of death when dividing. (@MAM: proper DEATH_PROB impl) |
| `death_method` | `DEATH_METHOD` | integer | `2` | When should death by old age occur? 0 = Never 1 = When executed AGE_LIMIT (+deviation) total instructions 2 = When executed genome_length * AGE_LIMIT (+dev) instructions |
| `age_limit` | `AGE_LIMIT` | integer | `20` | See DEATH_METHOD |
| `age_deviation` | `AGE_DEVIATION` | integer | `0` | Creates a normal distribution around AGE_LIMIT for time of death |
| `juvenile_period` | `JUV_PERIOD` | integer | `0` | Number of CPU cycles before newborn orgs can execute various instructions / behaviors |
| `alloc_method` | `ALLOC_METHOD` | integer | `0` | When allocating blank tape, how should it be initialized? 0 = Allocated space is set to default instruction. 1 = Set to section of dead genome (creates potential for recombination) 2 = Allocated space is set to random instruction. |
| `divide_method` | `DIVIDE_METHOD` | integer | `1` | 0 = Divide leaves state of mother untouched. 1 = Divide resets state of mother(effectively creating 2 offspring) 2 = Divide resets state of current thread only (use with parasites) |
| `epigenetic_method` | `EPIGENETIC_METHOD` | integer | `0` | Inheritance of state information other than genome 0 = none 1 = offspring inherits registers and stacks of first thread 1 = parent maintains registers and stacks of first thread 1 = offspring and parent keep state information |
| `generation_inc_method` | `GENERATION_INC_METHOD` | integer | `1` | 0 = Only increase generation of offspring on divide. 1 = Increase generation of both parent and offspring (suggested with DIVIDE_METHOD 1). |
| `reset_inputs_on_divide` | `RESET_INPUTS_ON_DIVIDE` | integer | `0` | Reset environment inputs of parent upon successful divide. |
| `inherit_merit` | `INHERIT_MERIT` | flag | `true` | Should merit be inhereted from mother parent? (in asexual) |
| `inherit_multithread` | `INHERIT_MULTITHREAD`, `INHERIT_MULTI_THREAD_CLASSIFICATION` | flag | `false` | Should offspring of parents with multiple threads be marked multithreaded? |
| `repro_method` | `REPRO_METHOD` | integer | `1` | Replace existing organism: 1=yes DEPRECATED: Use BIRTH_METHOD 3 instead. |
| `legacy_grid_local_selection` | `LEGACY_GRID_LOCAL_SELECTION` | flag | `false` | Enable legacy grid local mate selection. DEPRECATED: Birth chameber now uses population structure) |

### `divide`

Divide restrictions and triggers (conditions for a successful divide).

| Key | C++ name | Type | Default | Description |
|---|---|---|---|---|
| `offspring_size_range` | `OFFSPRING_SIZE_RANGE`, `CHILD_SIZE_RANGE` | number | `2.0` | Maximal differential between offspring and parent length. (Checked BEFORE mutations applied on divide.) |
| `min_copied_lines` | `MIN_COPIED_LINES` | number | `0.5` | Code fraction that must be copied before divide |
| `min_exe_lines` | `MIN_EXE_LINES` | number | `0.5` | Code fraction that must be executed before divide |
| `min_genome_size` | `MIN_GENOME_SIZE` | integer | `0` | Minimum number of instructions allowed in a genome. 0 = OFF |
| `max_genome_size` | `MAX_GENOME_SIZE` | integer | `0` | Maximum number of instructions allowed in a genome. 0 = OFF |
| `min_cycles` | `MIN_CYCLES` | integer | `0` | Min number of CPU cycles (age) required before reproduction. |
| `require_allocate` | `REQUIRE_ALLOCATE` | flag | `true` | (Original CPU Only) Require allocate before divide? |
| `required_task` | `REQUIRED_TASK` | integer | `-1` | Task ID required for successful divide |
| `immunity_task` | `IMMUNITY_TASK` | integer | `-1` | Task providing immunity from the required task |
| `required_reaction` | `REQUIRED_REACTION` | integer | `-1` | Reaction ID required for successful divide |
| `immunity_reaction` | `IMMUNITY_REACTION` | integer | `-1` | Reaction ID that provides immunity for successful divide |
| `require_single_reaction` | `REQUIRE_SINGLE_REACTION` | integer | `0` | If set to 1, at least one reaction is required for a successful divide; if set to n > 1, at least n reactions are required for a successful divide |
| `max_unique_task_count` | `MAX_UNIQUE_TASK_COUNT` | integer | `-1` | Division will fail if organism performs more than MAX_TASK_COUNT when MAX_TASK_COUNT >= 0 |
| `required_bonus` | `REQUIRED_BONUS` | number | `0.0` | Required bonus to divide |
| `require_exact_copy` | `REQUIRE_EXACT_COPY` | flag | `false` | Require offspring to be an exact copy (checked before divide mutations) |
| `required_resource` | `REQUIRED_RESOURCE` | integer | `-1` | ID of resource required in organism's internal bins for successful divide (resource not consumed) |
| `required_resource_level` | `REQUIRED_RESOURCE_LEVEL` | number | `0.0` | Level of resource needed for REQUIRED_RESOURCE |
| `required_pred_habitat` | `REQUIRED_PRED_HABITAT` | integer | `-1` | Required resource habitat type in cell for predators to reproduce |
| `required_pred_habitat_value` | `REQUIRED_PRED_HABITAT_VALUE` | number | `0` | Level of resource needed for REQUIRED_PRED_HABITAT |
| `required_prey_habitat` | `REQUIRED_PREY_HABITAT` | integer | `-1` | Required resource habitat type in cell for prey to reproduce |
| `required_prey_habitat_value` | `REQUIRED_PREY_HABITAT_VALUE` | number | `0` | Level of resource needed for REQUIRED_PREY_HABITAT |
| `implicit_repro_bonus` | `IMPLICIT_REPRO_BONUS` | integer | `0` | Call Inst_Repro to divide upon achieving this bonus. 0 = OFF |
| `implicit_repro_cpu_cycles` | `IMPLICIT_REPRO_CPU_CYCLES` | integer | `0` | Call Inst_Repro after this many cpu cycles. 0 = OFF |
| `implicit_repro_time` | `IMPLICIT_REPRO_TIME` | integer | `0` | Call Inst_Repro after this time used. 0 = OFF |
| `implicit_repro_end` | `IMPLICIT_REPRO_END` | flag | `false` | Call Inst_Repro after executing the last instruction in the genome. |
| `implicit_repro_energy` | `IMPLICIT_REPRO_ENERGY` | number | `0.0` | Call Inst_Repro if organism accumulates this amount of energy. |

### `recombination`

Sexual recombination and modularity.

| Key | C++ name | Type | Default | Description |
|---|---|---|---|---|
| `prob` | `RECOMBINATION_PROB` | number | `1.0` | Probability of recombination in div-sex |
| `max_birth_wait_time` | `MAX_BIRTH_WAIT_TIME` | integer | `-1` | Updates incipiant orgs can wait for crossover (-1 = unlimited) |
| `module_num` | `MODULE_NUM` | integer | `0` | Number of modules in the genome |
| `cont_rec_regs` | `CONT_REC_REGS` | flag | `true` | Are (modular) recombination regions continuous? |
| `correspond_rec_regs` | `CORESPOND_REC_REGS` | flag | `true` | Are (modular) recombination regions swapped randomly or with corresponding positions? |
| `two_fold_cost_sex` | `TWO_FOLD_COST_SEX` | flag | `false` | false = Both offspring are born (no two-fold cost) true = only one recombined offspring is born. |
| `same_length_sex` | `SAME_LENGTH_SEX` | flag | `false` | false = Recombine with any genome true = Recombine only w/ same length |
| `allow_mate_selection` | `ALLOW_MATE_SELECTION` | flag | `false` | Allow organisms to select mates (requires instruction set support) |

### `mating`

Mating types and mate choice.

| Key | C++ name | Type | Default | Description |
|---|---|---|---|---|
| `types` | `MATING_TYPES` | flag | `false` | Turn on separate mating types (i.e., males/females; off by default; requires instruction set support) |
| `lekking` | `LEKKING` | flag | `false` | Offspring from males go directly into birth chamber to await female choice (off by default) |
| `max_global_birth_chamber_size` | `MAX_GLOBAL_BIRTH_CHAMBER_SIZE` | integer | `3600` | Maximum number of waiting that can be stored in the birth chamber in a well-mixed population (3600 by default) |
| `disable_genotype_classification` | `DISABLE_GENOTYPE_CLASSIFICATION` | flag | `false` | Disable tracking of historical genotypes to conserve memory (off by default) |
| `noisy_assessment` | `NOISY_MATE_ASSESSMENT` | flag | `false` | Is mate assessment perfect (false) or noisy (true) (false by default) |
| `assessment_cv` | `MATE_ASSESSMENT_CV` | number | `0.1` | Coefficient of variation for how noisy mate assessment is (0.1 by default) |
| `forced_preference` | `FORCED_MATE_PREFERENCE` | integer | `-1` | Force all females to use a specific mate preference; -1 = off (mate preferences can evolve); 0 = all females mate randomly; 1 = all prefer highest display A; 2 = highest display B; 3 = highest merit |
| `in_groups` | `MATE_IN_GROUPS` | flag | `false` | Require all mating to happen within groups |

### `parasites`

Parasites.

| Key | C++ name | Type | Default | Description |
|---|---|---|---|---|
| `log_injections` | `LOG_PARASITE_INJECTIONS` | flag | `false` | Log parasite replications? |
| `inject_method` | `INJECT_METHOD` | integer | `0` | What should happen to a parasite when it gives birth? 0 = Leave the parasite thread state untouched. 1 = Resets the state of the calling thread (for SMT parasites, this must be 1) |
| `infection_mechanism` | `INFECTION_MECHANISM`, `INJECT_IS_TASK_SPECIFIC` | integer | `1` | 0: Infection always succeeds. 1: Infection succeeds if parasite matches at least one host task. 2: Infection succeeds if parasite does NOT match at least one task. 3: Parasite tasks must match host tasks exactly (Matching Alleles). |
| `inject_qma_exponent` | `INJECT_QMA_EXPONENT` | number | `0.2` | The exponent of the equation proportion_overlap^x that determines the probability of infection succeding given the amount a host and parasite phenotype match. |
| `inject_sterilizes_host` | `INJECT_STERILIZES_HOST` | flag | `false` | Infection causes host steralization |
| `inject_is_virulent` | `INJECT_IS_VIRULENT` | flag | `false` | Infection causes host steralization and takes all cpu cycles (setting this to 1 will override inject_virulence) |
| `skip_reactions` | `PARASITE_SKIP_REACTIONS` | flag | `true` | Parasite tasks do not get processed in the environment (true) or they do trigger reactions (false) |
| `inject_skip_first_task` | `INJECT_SKIP_FIRST_TASK` | flag | `false` | They cannot match the first task the host is doing to infect |
| `inject_default_success` | `INJECT_DEFAULT_SUCCESS` | number | `0.0` | If injection is task specific, with what probability should non-matching parasites infect the host |
| `virulence` | `PARASITE_VIRULENCE` | number | `-1` | The probabalistic percentage of cpu cycles allocated to the parasite instead of the host. Ensure INJECT_IS_VIRULENT is set to 0. This only works for single infection at the moment. Note that this should be set to a default even if virulence is evolving. |
| `virulence_source` | `VIRULENCE_SOURCE` | integer | `0` | Virulence is set by config (0) or inhereted from parent (1) or host-controlled, i.e. donation (2) |
| `virulence_mut_rate` | `VIRULENCE_MUT_RATE` | number | `0.1` | The probability that virulence will mutate if it is inhereted from the parent or host-controlled (VIRULENCE_SOURCE = 1 & 2) |
| `virulence_sd` | `VIRULENCE_SD` | number | `0.1` | New Virulence will be drawn from a normal distribution centered on the parental virulence, with this standard deviation |
| `mem_spaces` | `PARASITE_MEM_SPACES` | flag | `true` | Parasites get their own memory spaces |
| `no_copy_mut` | `PARASITE_NO_COPY_MUT` | flag | `false` | Parasites do not get copy mutation rates |
| `use_genotype_file` | `PARASITE_USE_GENOTYPE_FILE` | flag | `false` | Parasite Genotypes are loaded from a file rather than replicated from parent -- see LoadParasiteGenotypeList |
| `host_use_genotype_file` | `HOST_USE_GENOTYPE_FILE` | flag | `false` | Host Genotypes are loaded from a file rather than replicated from parent -- see LoadHostGenotypeList |
| `full_vertical_trans` | `FULL_VERTICAL_TRANS` | number | `0.0` | Determines if offspring of infected host is automatically infected. 0 for no, 1 for yes. If you want to keep parent infected as well, you need to set DIVIDE_METHOD to 2. |

### `cpu`

Details on how the CPU works.

| Key | C++ name | Type | Default | Description |
|---|---|---|---|---|
| `io_expire` | `IO_EXPIRE` | flag | `true` | Is the expiration functionality of '-expire' I/O instructions enabled? |
| `poison_penalty` | `POISON_PENALTY` | number | `0.01` | Metabolic rate penalty applied when the 'poison' instruction is executed. |

### `mp`

Multiple, distributed populations.

| Key | C++ name | Type | Default | Description |
|---|---|---|---|---|
| `enabled` | `ENABLE_MP` | flag | `false` | Enable multi-process Avida; false = disabled (default), true = enabled. |
| `scheduling_style` | `MP_SCHEDULING_STYLE` | integer | `0` | Style of scheduling: 0=non-MP aware (default) 1=MP aware, integrated across worlds. |

### `demes`

Demes and germlines.

| Key | C++ name | Type | Default | Description |
|---|---|---|---|---|
| `migration_file` | `MIGRATION_FILE` | text | `-` | NxN file that describes connectivity weights between demes |
| `count` | `NUM_DEMES` | integer | `1` | Number of independent groups in the population |
| `competition_style` | `DEMES_COMPETITION_STYLE` | integer | `0` | How should demes compete? 0=Fitness proportional selection 1=Tournament selection |
| `tournament_size` | `DEMES_TOURNAMENT_SIZE` | integer | `0` | Number of demes that participate in a tournament |
| `override_fitness` | `DEMES_OVERRIDE_FITNESS` | flag | `false` | Should the calculated fitness be overridden? false = no, use it (default) true = yes (all fitnesses=1) |
| `use_germline` | `DEMES_USE_GERMLINE` | integer | `0` | Should demes use a distinct germline? 0: No, 1: Traditional germ lines, 2: Genotype tracking, 3: Organism flagging germline |
| `prevent_sterile` | `DEMES_PREVENT_STERILE` | flag | `false` | Prevent sterile demes from replicating? |
| `reset_resources` | `DEMES_RESET_RESOURCES` | integer | `0` | Reset resources in demes on replication? 0 = reset both demes 1 = reset target deme 2 = deme resources remain unchanged |
| `replicate_size` | `DEMES_REPLICATE_SIZE` | integer | `1` | Number of identical organisms to create or copy from the source deme to the target deme |
| `log_replicate` | `LOG_DEMES_REPLICATE` | flag | `false` | Log deme replications? |
| `replicate_log_start` | `DEMES_REPLICATE_LOG_START` | integer | `0` | Update at which to start logging deme replications |
| `prob_org_transfer` | `DEMES_PROB_ORG_TRANSFER` | number | `0.0` | Probablity of an organism being transferred from the source deme to the target deme |
| `organism_selection` | `DEMES_ORGANISM_SELECTION` | integer | `0` | How should organisms be selected for transfer from source to target during deme replication? 0 = random with replacement 1 = sequential 2-6 = created, but not sure what they do 7 = organism(s) flagged germline 8=one of the organisms flagged as part of the germline |
| `organism_placement` | `DEMES_ORGANISM_PLACEMENT` | integer | `0` | How should organisms be placed during deme replication. 0 = cell-array middle 1 = deme center 2 = random placement 3 = sequential |
| `organism_facing` | `DEMES_ORGANISM_FACING` | integer | `0` | Which direction should organisms face after deme replication. 0 = unchanged 1 = northwest. 2 = random. |
| `max_age` | `DEMES_MAX_AGE` | integer | `500` | The maximum age of a deme (in updates) to be used for age-based replication |
| `max_births` | `DEMES_MAX_BIRTHS` | integer | `100` | Max number of births that can occur within a deme; used with birth-count replication |
| `mim_events_killed_ratio` | `DEMES_MIM_EVENTS_KILLED_RATIO` | number | `0.7` | Minimum ratio of events killed required for event period to be a success. |
| `mim_successful_event_periods` | `DEMES_MIM_SUCCESSFUL_EVENT_PERIODS` | integer | `1` | Minimum number of consecutive event periods that must be a success. |
| `germline_copy_mut` | `GERMLINE_COPY_MUT` | number | `0.0075` | Prob. of copy mutations during germline replication |
| `germline_ins_mut` | `GERMLINE_INS_MUT` | number | `0.05` | Prob. of insertion mutations during germline replication |
| `germline_del_mut` | `GERMLINE_DEL_MUT` | number | `0.05` | Prob. of deletion mutations during germline replication |
| `replicate_cpu_cycles` | `DEMES_REPLICATE_CPU_CYCLES` | number | `0.0` | Replicate a deme immediately after it has used this many cpu cycles per org in deme (0 = OFF). |
| `replicate_time` | `DEMES_REPLICATE_TIME` | number | `0.0` | Number of CPU cycles used by a deme to trigger its replication (normalized by number of orgs in deme and organism merit; 0 = OFF). |
| `replicate_births` | `DEMES_REPLICATE_BIRTHS` | integer | `0` | Number of offspring produced by a deme to trigger its replication (0 = OFF). |
| `replicate_orgs` | `DEMES_REPLICATE_ORGS` | integer | `0` | Number of organisms in a deme to trigger its replication (0 = OFF). |
| `replication_only_resets` | `DEMES_REPLICATION_ONLY_RESETS` | integer | `0` | Kin selection mode.  On replication: 0 = Nothing extra 1 = reset deme resources 2 = reset resources and re-inject organisms |
| `migration_rate` | `DEMES_MIGRATION_RATE` | number | `0.0` | Probability of an offspring being born in a different deme. |
| `parasite_migration_rate` | `DEMES_PARASITE_MIGRATION_RATE` | number | `0.0` | Probability of a parasite migrating to a different deme. Note: only works with DEMES_MIGRATION_METHOD 4. |
| `migration_method` | `DEMES_MIGRATION_METHOD` | integer | `0` | Which demes can an offspring land in when it migrates? 0 = Any other deme 1 = Eight neighboring demes 2 = Two adjacent demes in list 3 = Proportional based on the number of points 4 = Use the weight matrix specified in MIGRATION_FILE |
| `parasite_migration_target_selection_method` | `DEMES_PARASITE_MIGRATION_TARGET_SELECTION_METHOD` | integer | `0` | How should a target cell be chosen in the migrated-to deme? 0 = Select a cell randomly, if it is not occupied infection fails 1 = Select an occupied cell randomly |
| `parasite_migration_memory_score_protective_threshold` | `DEMES_PARASITE_MIGRATION_MEMORY_SCORE_PROTECTIVE_THRESHOLD` | number | `1.0` | Above what parasite memory score should hosts be protected from incoming parasite migration? |
| `num_x` | `DEMES_NUM_X` | integer | `0` | Simulated number of demes in X dimension. Used only for migration. |
| `partition_interval` | `DEMES_PARTITION_INTERVAL` | integer | `0` | Restrict deme replication to within partitions of DEMES_PARTITION_INTERVAL consecutive deme ids. No restriction if 0. Partition size cannot be smaller than two. Does not affect migration. |
| `seed_method` | `DEMES_SEED_METHOD` | integer | `0` | Deme seeding method. 0 = Maintain old consistency 1 = New method using genotypes |
| `divide_method` | `DEMES_DIVIDE_METHOD` | integer | `0` | Deme divide method. Only works with DEMES_SEED_METHOD 1 0 = Replace source and target demes 1 = Replace target deme, reset source deme to founders 2 = Replace target deme, leave source deme unchanged 3 = Replace the target deme, and reset the number of resources consumed by the source deme. 4 = Replace the target deme,  reset the number of resources consumed by the source deme, and kill the germ line organisms of the source deme |
| `default_germline_propensity` | `DEMES_DEFAULT_GERMLINE_PROPENSITY` | number | `0.0` | Default germline propensity of organisms in deme. For use with DEMES_DIVIDE_METHOD 2. |
| `founder_germline_propensity` | `DEMES_FOUNDER_GERMLINE_PROPENSITY` | number | `-1.0` | Default germline propensity of founder organisms in deme. For use with DEMES_DIVIDE_METHOD 2. <0 = OFF |
| `prefer_empty` | `DEMES_PREFER_EMPTY` | flag | `false` | Give empty demes preference as targets of deme replication? |
| `protection_points` | `DEMES_PROTECTION_POINTS` | integer | `0` | The number of points a deme receives for each suicide. |
| `uniform_migration_rate` | `MIGRATION_RATE` | number | `0.0` | Uniform probability of offspring migrating to a new deme. |
| `track_shannon_info` | `DEMES_TRACK_SHANNON_INFO` | integer | `0` | Enable shannon mutual information tracking for demes. |
| `mut_orgs_on_replication` | `DEMES_MUT_ORGS_ON_REPLICATION` | flag | `false` | Mutate orgs using germline mutation rates when they are copied to a new deme (using DEMES_SEED_METHOD 1): false = off, true = on |
| `orgs_start_in_germ` | `DEMES_ORGS_START_IN_GERM` | flag | `false` | Are orgs considered part of the germline at start? |

### `reversion`

Mutation reversion (most of these slow Avida down a lot and should normally be 0.0).

| Key | C++ name | Type | Default | Description |
|---|---|---|---|---|
| `revert_fatal` | `REVERT_FATAL` | number | `0.0` | Prob of lethal mutations being reverted on birth |
| `revert_detrimental` | `REVERT_DETRIMENTAL` | number | `0.0` | Prob of harmful (but non-lethal) mutations reverting on birth |
| `revert_neutral` | `REVERT_NEUTRAL` | number | `0.0` | Prob of neutral mutations being reverted on birth |
| `revert_beneficial` | `REVERT_BENEFICIAL` | number | `0.0` | Prob of beneficial mutations being reverted on birth |
| `revert_taskloss` | `REVERT_TASKLOSS` | number | `0.0` | Prob of mutations that cause task loss without any tasks gained being reverted on birth |
| `revert_equals` | `REVERT_EQUALS` | number | `0.0` | Prob of mutations which grant EQU being reverted on birth |
| `sterilize_fatal` | `STERILIZE_FATAL` | number | `0.0` | Prob of lethal mutations steralizing an offspring (typically no effect!) |
| `sterilize_detrimental` | `STERILIZE_DETRIMENTAL` | number | `0.0` | Prob of harmful (but non-lethal) mutations sterilizing an offspring |
| `sterilize_neutral` | `STERILIZE_NEUTRAL` | number | `0.0` | Prob of neutral mutations sterilizing an offspring |
| `sterilize_beneficial` | `STERILIZE_BENEFICIAL` | number | `0.0` | Prob of beneficial mutations sterilizing an offspring |
| `sterilize_taskloss` | `STERILIZE_TASKLOSS` | number | `0.0` | Prob of mutations causing task loss without task gain sterilizing an offspring |
| `sterilize_unstable` | `STERILIZE_UNSTABLE`, `FAIL_IMPLICIT` | integer | `0` | Should genotypes that cannot replicate perfectly not be allowed to replicate? |
| `neutral_max` | `NEUTRAL_MAX` | number | `0.0` | Percent benifical change from parent fitness to be considered neutral. |
| `neutral_min` | `NEUTRAL_MIN` | number | `0.0` | Percent deleterious change from parent fitness to be considered neutral. |

### `time`

Time slicing and merit.

| Key | C++ name | Type | Default | Description |
|---|---|---|---|---|
| `ave_time_slice` | `AVE_TIME_SLICE` | integer | `30` | Average number of CPU-cycles per org per update |
| `slicing_method` | `SLICING_METHOD` | integer | `1` | 0 = CONSTANT: all organisms receive equal number of CPU cycles 1 = PROBABILISTIC: CPU cycles distributed randomly, proportional to merit. 2 = INTEGRATED: CPU cycles given out deterministicly, proportional to merit 3 = DEME_PROBABALISTIC: Demes receive fixed number of CPU cycles, awarded probabalistically to members 4 = CROSS_DEME_PROBABALISTIC: Demes receive CPU cycles proportional to living population size, awarded probabalistically to members |
| `base_merit_method` | `BASE_MERIT_METHOD` | integer | `4` | How should merit be initialized? 0 = Constant (merit independent of size) 1 = Merit proportional to copied size 2 = Merit prop. to executed size 3 = Merit prop. to full size 4 = Merit prop. to min of executed or copied size 5 = Merit prop. to sqrt of the minimum size 6 = Merit prop. to num times MERIT_BONUS_INST is in genome. |
| `base_const_merit` | `BASE_CONST_MERIT` | integer | `100` | Base merit value for BASE_MERIT_METHOD 0 |
| `merit_bonus_inst` | `MERIT_BONUS_INST` | integer | `0` | Instruction ID to count for BASE_MERIT_METHOD 6 |
| `merit_bonus_effect` | `MERIT_BONUS_EFFECT` | integer | `0` | Amount of merit earn per instruction for BASE_MERIT_METHOD 6 (-1 = penalty, 0 = no effect) |
| `fitness_valley` | `FITNESS_VALLEY` | flag | `false` | in BASE_MERIT_METHOD 6, this creates valleys from FITNESS_VALLEY_START to FITNESS_VALLEY_STOP (false = off, true = on) |
| `fitness_valley_start` | `FITNESS_VALLEY_START` | integer | `0` | if FITNESS_VALLEY = 1, orgs with num_key_instructions from FITNESS_VALLEY_START to FITNESS_VALLEY_STOP get fitness 1 (lowest) |
| `fitness_valley_stop` | `FITNESS_VALLEY_STOP` | integer | `0` | if FITNESS_VALLEY = 1, orgs with num_key_instructions from FITNESS_VALLEY_START to FITNESS_VALLEY_STOP get fitness 1 (lowest) |
| `default_bonus` | `DEFAULT_BONUS` | number | `1.0` | Initial bonus before any tasks |
| `merit_default_bonus` | `MERIT_DEFAULT_BONUS` | integer | `0` | Instead of inheriting bonus from parent, use this value instead (0 = off) |
| `merit_inc_apply_immediate` | `MERIT_INC_APPLY_IMMEDIATE` | flag | `false` | Should merit increases (above current) be applied immediately, or delayed until divide? |
| `task_refractory_period` | `TASK_REFRACTORY_PERIOD` | number | `0.0` | Number of updates after taske until regain full value |
| `fitness_method` | `FITNESS_METHOD` | integer | `0` | 0 = default, 1 = sigmoidal, |
| `fitness_coeff_1` | `FITNESS_COEFF_1` | number | `1.0` | 1st FITNESS_METHOD parameter |
| `fitness_coeff_2` | `FITNESS_COEFF_2` | number | `1.0` | 2nd FITNESS_METHOD parameter |
| `max_cpu_threads` | `MAX_CPU_THREADS` | integer | `1` | Maximum number of Threads a CPU can spawn |
| `thread_slicing_method` | `THREAD_SLICING_METHOD` | integer | `0` | Formula for allocating CPU cycles across threads in an organism (num_threads-1) * THREAD_SLICING_METHOD + 1 0 = One thread executed per time slice. 1 = All threads executed each time slice. |
| `no_cpu_cycle_time` | `NO_CPU_CYCLE_TIME` | flag | `false` | Don't count each CPU cycle as part of gestation time |
| `max_label_exe_size` | `MAX_LABEL_EXE_SIZE` | integer | `1` | Max nops marked as executed when labels are used |
| `precalc_phenotype` | `PRECALC_PHENOTYPE` | integer | `0` | 0 = Disabled 1 = Assign precalculated merit at birth (unlimited resources only) 2 = Assign precalculated gestation time 3 = Assign precalculated merit AND gestation time. 4 = Assign last instruction counts 5 = Assign last instruction counts and merit 6 = Assign last instruction counts and gestation time 7 = Assign everything currently supported Fitness will be evaluated for organism based on these settings. |
| `genotype_phenplast_calc` | `GENOTYPE_PHENPLAST_CALC` | integer | `100` | Number of times to test a genotype's plasticity during runtime. |

### `altruism`

Altruism.

| Key | C++ name | Type | Default | Description |
|---|---|---|---|---|
| `merit_given` | `MERIT_GIVEN` | number | `0.0` | Fraction of merit donated with 'donate' command |
| `merit_received` | `MERIT_RECEIVED` | number | `0.0` | Multiplier of merit given with 'donate' command |
| `max_donate_kin_dist` | `MAX_DONATE_KIN_DIST` | integer | `-1` | Limit on distance of relation for donate; -1=no max |
| `max_donate_edit_dist` | `MAX_DONATE_EDIT_DIST` | integer | `-1` | Limit on genetic (edit) distance for donate; -1=no max |
| `min_gb_donate_threshold` | `MIN_GB_DONATE_THRESHOLD` | integer | `-1` | threshold green beard donates only to orgs above this donation attempt threshold; -1=no thresh |
| `donate_thresh_quanta` | `DONATE_THRESH_QUANTA` | integer | `10` | The size of steps between quanta donate thresholds |
| `max_donates` | `MAX_DONATES` | integer | `1000000` | Limit on number of donates organisms are allowed. |

### `kaboom`

Kaboom.

| Key | C++ name | Type | Default | Description |
|---|---|---|---|---|
| `prob` | `KABOOM_PROB` | number | `-1` | The probability (in decimal) that an explosion will occur when the instruction is encountered. -1 is default probability and allows the organism to change the probability. |
| `radius` | `KABOOM_RADIUS` | integer | `2` | Radius of all explosions (kaboom and kaboom5) |
| `effect` | `KABOOM_EFFECT` | integer | `10` | Merit increased or decreased for surrounding kin or non-kin |
| `hamming` | `KABOOM_HAMMING` | integer | `0` | Hamming distance of kaboom's threshold, set to -1 to have adjustable, default is 0 for clone altruists. |
| `hamming1` | `KABOOM1_HAMMING` | integer | `1` | Hamming distance of kaboom1's threshold, set to -1 to have adjustable, default is 1. |
| `hamming2` | `KABOOM2_HAMMING` | integer | `2` | Hamming distance of kaboom2's threshold, set to -1 to have adjustable, default is 2. |
| `hamming3` | `KABOOM3_HAMMING` | integer | `3` | Hamming distance of kaboom3's threshold, set to -1 to have adjustable, default is 3. |
| `hamming4` | `KABOOM4_HAMMING` | integer | `4` | Hamming distance of kaboom4's threshold, set to -1 to have adjustable, default is 4. |
| `hamming5` | `KABOOM5_HAMMING` | integer | `5` | Hamming distance of kaboom5's threshold, set to -1 to have adjustable, default is 5 for clan altruists. |
| `no_mut_insts` | `NO_MUT_INSTS` | text | `` | A list of the instruction symbols that should not be able to be mutated out, NOT checked for all mutation types. |
| `noisy_qs_sd` | `NOISY_QS_SD` | number | `.05` | The standard deviation for the gaussian distribution (mean = 1) that is multiplied with the kin percent found for NoisyQuorum inst |

### `genealogy`

Genealogy.

| Key | C++ name | Type | Default | Description |
|---|---|---|---|---|
| `threshold` | `THRESHOLD` | integer | `3` | Number of organisms in a genotype needed for it to be considered viable. |
| `test_cpu_time_mod` | `TEST_CPU_TIME_MOD` | integer | `20` | Time allocated in test CPUs (multiple of length) |
| `drop_extinct_genotypes` | `DROP_EXTINCT_GENOTYPES` | flag | `false` | false = keep a genotype without living organisms while another genotype names it as a parent (as the C++ does) true = drop it as soon as its last organism dies: memory stays flat with sexual reproduction, ancestry (lineages, coalescence, kin distance) is not tracked and actions that need it stop |

### `network`

Organism network communication.

| Key | C++ name | Type | Default | Description |
|---|---|---|---|---|
| `drop_prob` | `NET_DROP_PROB` | number | `0.0` | Message drop rate |
| `log_messages` | `NET_LOG_MESSAGES` | flag | `false` | Whether all messages are logged; false (default) or true. |
| `log_retmessages` | `NET_LOG_RETMESSAGES` | flag | `false` | Whether retrieved messages are logged; false (default) or true. |

### `messaging`

Organism message-based communication.

| Key | C++ name | Type | Default | Description |
|---|---|---|---|---|
| `send_buffer_size` | `MESSAGE_SEND_BUFFER_SIZE` | integer | `1` | Size of message send buffer (stores messages that were sent) TASKS NOT CHECKED ON 0! -1=inf, default=1. |
| `recv_buffer_size` | `MESSAGE_RECV_BUFFER_SIZE` | integer | `8` | Size of message receive buffer (stores messages that are received); -1=inf, default=8. |
| `recv_buffer_behavior` | `MESSAGE_RECV_BUFFER_BEHAVIOR` | integer | `0` | Behavior of message receive buffer; 0=drop oldest (default), 1=drop incoming |
| `active_messages_enabled` | `ACTIVE_MESSAGES_ENABLED` | integer | `0` | Enable active messages. 0 = off 2 = message creates parallel thread |
| `check_task_on_send` | `CHECK_TASK_ON_SEND` | flag | `true` | false: Don't check tasks on send, true: Check tasks on send (default) |
| `neural_networking` | `NEURAL_NETWORKING` | flag | `false` | Turns neural networking system on/off. Requires USE_AVATARS be turned on. |
| `self_communication` | `SELF_COMMUNICATION` | flag | `false` | Allows organisms to create self communication loops. An organism's input avatars can receive messages from it's own output avatars. |

### `buy_sell`

Buying and selling.

| Key | C++ name | Type | Default | Description |
|---|---|---|---|---|
| `save_received` | `SAVE_RECEIVED` | flag | `false` | Enable storage of all inputs bought from other orgs |

### `hoarding`

Resource hoarding.

| Key | C++ name | Type | Default | Description |
|---|---|---|---|---|
| `use_resource_bins` | `USE_RESOURCE_BINS` | flag | `false` | Enable resource bin use.  This serves as a guard on most resource hoarding code. |
| `absorb_resource_fraction` | `ABSORB_RESOURCE_FRACTION` | number | `.0025` | Fraction of available environmental resource an organism absorbs. |
| `multi_absorb_type` | `MULTI_ABSORB_TYPE` | integer | `0` | What to do if a collect instruction is called on a range of resources. 0 = absorb a random resource in the range 1 = absorb the first resource in the range 2 = absorb the last resource in the range 3 = absorb ABSORB_RESOURCE_FRACTION / (# of resources in range) of each resource in the range |
| `max_total_stored` | `MAX_TOTAL_STORED` | number | `-1` | Maximum total amount of all resources an organism can store. <0 = no maximum |
| `use_stored_fraction` | `USE_STORED_FRACTION` | number | `1.0` | The fraction of stored resource to use. |
| `env_fraction_threshold` | `ENV_FRACTION_THRESHOLD` | number | `1.0` | The fraction of available environmental resource to compare available stored resource to when deciding whether to use stored resource. |
| `return_stored_on_death` | `RETURN_STORED_ON_DEATH` | flag | `true` | Return an organism's stored resources to the world when it dies? |
| `split_on_divide` | `SPLIT_ON_DIVIDE` | flag | `true` | Split mother cell's resources between two daughter cells on division? |
| `collect_specific_resource` | `COLLECT_SPECIFIC_RESOURCE` | integer | `0` | Resource to be collected by the "collect-specific" instruction. |
| `non_1_resource_ratios` | `NON_1_RESOURCE_RATIOS` | text | `1:1` | Resources to be collected by the "collect-specific-ratio" instruction in a non 1:1 ratio. Specify as 'resourceID1:ratio1, resouceID2:ratio2' etc |
| `collect_amount` | `COLLECT_AMOUNT` | number | `1` | The amount to collect for collect-specific, or for a resource with a ratio of 1 in collect-specific-ratio |
| `resource_given_on_inject` | `RESOURCE_GIVEN_ON_INJECT` | number | `0.0` | Units of collect-specific resources given on inject. |
| `resource_given_at_birth` | `RESOURCE_GIVEN_AT_BIRTH` | number | `0.0` | Units of collect-specific resources given to offspring upon birth (will be added to SPLIT_ON_DIVIDE amount for collect-specific resource if both enabled. |
| `collect_prob_divisor` | `COLLECT_PROB_DIVISOR` | integer | `1` | Divisor for probabilistic collect instructions. |

### `analyze`

Analysis mode.

| Key | C++ name | Type | Default | Description |
|---|---|---|---|---|
| `file` | `ANALYZE_FILE` | text | `analyze.cfg` | File used for analysis mode |
| `max_concurrency` | `MAX_CONCURRENCY` | integer | `-1` | Maximum number of analyze threads, -1 == use all available. |
| `inject_resets_tasks` | `INJECT_RESETS_TASKS` | flag | `false` | Executing INJECT (semi-succesfully) will trigger last_task_count to be writen from current_task_count |
| `option_1` | `ANALYZE_OPTION_1` | text | `` | String variable accessible from analysis scripts |
| `option_2` | `ANALYZE_OPTION_2` | text | `` | String variable accessible from analysis scripts |
| `mode` | `ANALYZE_MODE` | integer | `0` | 0 = Disabled 1 = Enabled 2 = Interactive DEPRECATED: use command line options -a[nalyze] or -i[nteractive]) |

### `energy`

Energy model.

| Key | C++ name | Type | Default | Description |
|---|---|---|---|---|
| `enabled` | `ENERGY_ENABLED` | flag | `false` | Enable Energy Model (true = on, false = off) |
| `given_on_inject` | `ENERGY_GIVEN_ON_INJECT` | number | `0.0` | Energy given to organism upon injection. |
| `given_at_birth` | `ENERGY_GIVEN_AT_BIRTH` | number | `0.0` | Energy given to offspring upon birth. |
| `frac_parent_energy_given_to_org_at_birth` | `FRAC_PARENT_ENERGY_GIVEN_TO_ORG_AT_BIRTH` | number | `0.5` | Fraction of parent's energy given to offspring organism. |
| `frac_parent_energy_given_to_deme_at_birth` | `FRAC_PARENT_ENERGY_GIVEN_TO_DEME_AT_BIRTH` | number | `0.5` | Fraction of parent's energy given to offspring deme. |
| `frac_energy_decay_at_org_birth` | `FRAC_ENERGY_DECAY_AT_ORG_BIRTH` | number | `0.0` | Fraction of energy lost due to decay during organism reproduction. |
| `frac_energy_decay_at_deme_birth` | `FRAC_ENERGY_DECAY_AT_DEME_BIRTH` | number | `0.0` | Fraction of energy lost due to decay during deme reproduction. |
| `num_cycles_exc_before_0_energy` | `NUM_CYCLES_EXC_BEFORE_0_ENERGY` | integer | `0` | Number of virtual CPU cycles executed before energy is exhausted. |
| `cap` | `ENERGY_CAP` | number | `-1.0` | Maximum amount of energy that can be stored in an organism.  -1 = no max |
| `apply_method` | `APPLY_ENERGY_METHOD` | integer | `0` | When should rewarded energy be applied to current energy? 0 = on divide 1 = on completion of task 2 = on sleep |
| `fix_metabolic_rate` | `FIX_METABOLIC_RATE` | number | `-1.0` | Fix organism metobolic rate to value.  This value is static.  Feature disabled by default (value == -1) |
| `frac_energy_transfer` | `FRAC_ENERGY_TRANSFER` | number | `0.0` | Fraction of replaced organism's energy take by new resident |
| `log_sleep_times` | `LOG_SLEEP_TIMES` | flag | `false` | Log sleep start and end times (true = on, false = off). WARNING: may use lots of memory. |
| `frac_energy_relinquish` | `FRAC_ENERGY_RELINQUISH` | number | `1.0` | Fraction of organisms energy to relinquish |
| `passed_on_deme_replication_method` | `ENERGY_PASSED_ON_DEME_REPLICATION_METHOD` | integer | `0` | Who get energy passed from a parent deme 0 = Energy divided among organisms injected to offspring deme 1 = Energy divided among cells in offspring deme |
| `inherit_exe_rate` | `INHERIT_EXE_RATE` | flag | `false` | Inherit energy rate from parent? false = no, true = yes |
| `attack_decay_rate` | `ATTACK_DECAY_RATE` | number | `0.0` | Percent of cell's energy decayed by attack |
| `thresh_low` | `ENERGY_THRESH_LOW` | number | `.33` | Threshold percent below which energy level is considered low.  Requires ENERGY_CAP. |
| `thresh_high` | `ENERGY_THRESH_HIGH` | number | `.75` | Threshold percent above which energy level is considered high.  Requires ENERGY_CAP. |
| `comparison_epsilon` | `ENERGY_COMPARISON_EPSILON` | number | `0.0` | Percent difference (relative to executing organism) required in energy level comparisons |
| `request_radius` | `ENERGY_REQUEST_RADIUS` | integer | `1` | Radius of broadcast energy request messages. |

### `energy_sharing`

Energy sharing.

| Key | C++ name | Type | Default | Description |
|---|---|---|---|---|
| `method` | `ENERGY_SHARING_METHOD` | integer | `0` | Method for sharing energy.  0=receiver must actively receive/request, 1=energy pushed on receiver |
| `pct` | `ENERGY_SHARING_PCT` | number | `0.0` | Percent of energy to share |
| `increment` | `ENERGY_SHARING_INCREMENT` | number | `0.01` | Amount to change percent energy shared |
| `resource_sharing_loss` | `RESOURCE_SHARING_LOSS` | number | `0.0` | Fraction of shared resource lost in transfer |
| `update_metabolic` | `ENERGY_SHARING_UPDATE_METABOLIC` | flag | `false` | Whether to update an organism's metabolic rate on donate or reception/application of energy (true = on, false = off) |

### `second_pass`

Tracking metrics known from an earlier run of the experiment.

| Key | C++ name | Type | Default | Description |
|---|---|---|---|---|
| `track_cclades` | `TRACK_CCLADES` | flag | `false` | Enable tracking of coalescence clades |
| `track_cclades_ids` | `TRACK_CCLADES_IDS` | text | `coalescence.ids` | File storing coalescence IDs |

### `gene_expression`

Gene expression CPU.

| Key | C++ name | Type | Default | Description |
|---|---|---|---|---|
| `max_programids` | `MAX_PROGRAMIDS` | integer | `16` | Maximum number of programids an organism can create. |
| `max_programid_age` | `MAX_PROGRAMID_AGE` | integer | `2000` | Max number of CPU cycles a programid executes before it is removed. |
| `implicit_gene_expression` | `IMPLICIT_GENE_EXPRESSION` | flag | `false` | Create executable programids from the genome without explicit allocation and copying? |
| `implicit_bg_promoter_rate` | `IMPLICIT_BG_PROMOTER_RATE` | number | `0.0` | Relative rate of non-promoter sites creating programids. |
| `implicit_turnover_rate` | `IMPLICIT_TURNOVER_RATE` | number | `0.0` | Number of programids recycled per CPU cycle. 0 = OFF |
| `implicit_max_programid_length` | `IMPLICIT_MAX_PROGRAMID_LENGTH` | integer | `0` | Creation of an executable programid terminates after this many instructions. 0 = disabled |

### `promoters`

Promoters.

| Key | C++ name | Type | Default | Description |
|---|---|---|---|---|
| `enabled` | `PROMOTERS_ENABLED` | flag | `false` | Use the promoter/terminator execution scheme. Certain instructions must also be included. |
| `inst_max` | `PROMOTER_INST_MAX` | integer | `0` | Maximum number of instructions to execute before terminating. 0 = off |
| `processivity` | `PROMOTER_PROCESSIVITY` | number | `1.0` | Chance of not terminating after each cpu cycle. |
| `processivity_inst` | `PROMOTER_PROCESSIVITY_INST` | number | `1.0` | Chance of not terminating after each instruction. |
| `to_register` | `PROMOTER_TO_REGISTER` | flag | `false` | Place a promoter's base bit code in register BX when starting execution from it? |
| `termination_resets` | `TERMINATION_RESETS` | flag | `false` | Does termination reset the thread's state? |
| `no_active_promoter_effect` | `NO_ACTIVE_PROMOTER_EFFECT` | integer | `0` | What happens when there are no active promoters? 0 = Start execution at the beginning of the genome. 1 = Kill the organism. 2 = Stop the organism from executing any further instructions. |
| `code_size` | `PROMOTER_CODE_SIZE` | integer | `24` | Size of a promoter code in bits. (Maximum value is 32) |
| `exe_length` | `PROMOTER_EXE_LENGTH` | integer | `3` | Length of promoter windows used to determine execution. |
| `exe_threshold` | `PROMOTER_EXE_THRESHOLD` | integer | `2` | Minimum number of bits that must be set in a promoter window to allow execution. |
| `inst_code_length` | `INST_CODE_LENGTH` | integer | `3` | Instruction binary code length (number of bits) |
| `inst_code_default_type` | `INST_CODE_DEFAULT_TYPE` | integer | `0` | Default value of instruction binary code value. 0 = All zeros 1 = Based off the instruction number |
| `constitutive_regulation` | `CONSTITUTIVE_REGULATION` | flag | `false` | Sense a new regulation value before each CPU cycle? |

### `colors`

Output colours for data files printed in HTML mode (the first six for lineages, the rest for mutation tests).

| Key | C++ name | Type | Default | Description |
|---|---|---|---|---|
| `diff` | `COLOR_DIFF` | text | `CCCCFF` | Color to flag stat that has changed since parent. |
| `same` | `COLOR_SAME` | text | `FFFFFF` | Color to flag stat that has NOT changed since parent. |
| `neg2` | `COLOR_NEG2` | text | `FF0000` | Color to flag stat that is significantly worse than parent. |
| `neg1` | `COLOR_NEG1` | text | `FFCCCC` | Color to flag stat that is minorly worse than parent. |
| `pos1` | `COLOR_POS1` | text | `CCFFCC` | Color to flag stat that is minorly better than parent. |
| `pos2` | `COLOR_POS2` | text | `00FF00` | Color to flag stat that is significantly better than parent. |
| `mut_pos` | `COLOR_MUT_POS` | text | `00FF00` | Color to flag stat that has changed since parent. |
| `mut_neut` | `COLOR_MUT_NEUT` | text | `FFFFFF` | Color to flag stat that has changed since parent. |
| `mut_neg` | `COLOR_MUT_NEG` | text | `FFFF00` | Color to flag stat that has changed since parent. |
| `mut_lethal` | `COLOR_MUT_LETHAL` | text | `FF0000` | Color to flag stat that has changed since parent. |

### `movement`

Movement and avatars.

| Key | C++ name | Type | Default | Description |
|---|---|---|---|---|
| `collisions_lethal` | `MOVEMENT_COLLISIONS_LETHAL` | integer | `0` | Are collisions during movement lethal (not applied to avatars)? (0=no, use swap; 1=yes, use collision selection type; 2=no, but movement fails) |
| `collisions_selection_type` | `MOVEMENT_COLLISIONS_SELECTION_TYPE` | integer | `0` | 0 = 50% chance 1 = binned vitality based |
| `vitality_bin_extremes` | `VITALITY_BIN_EXTREMES` | number | `1.0` | vitality multiplier for extremes (> 1 stddev from the mean population age) |
| `vitality_bin_center` | `VITALITY_BIN_CENTER` | number | `10.0` | vitality multiplier for center bin (with 1 stddev of the mean population age) |
| `deadly_boundaries` | `DEADLY_BOUNDARIES` | flag | `false` | Are bounded grid border cell deadly? If true, orgs stepping onto boundary cells will disappear into oblivion (aka die) |
| `step_counting_error` | `STEP_COUNTING_ERROR` | integer | `0` | % chance a step is not counted as part of easterly/northerly travel. |
| `use_avatars` | `USE_AVATARS` | integer | `0` | Set orgs to move & navigate in solo avatar worlds(1=yes, 2=yes, with org interactions). |
| `avatar_birth` | `AVATAR_BIRTH` | integer | `0` | 0 = Same as parent 1 = Random 2 = Cell faced by parent avatar 3 = next grid cell 4 = Center of the world |
| `avatar_birth_facing` | `AVATAR_BIRTH_FACING` | integer | `0` | 0 North 1 Random |
| `track_birth_locs` | `TRACK_BIRTH_LOCS` | flag | `false` | Log and print locations for all births place. |

### `sensing`

Sensing.

| Key | C++ name | Type | Default | Description |
|---|---|---|---|---|
| `look_dist` | `LOOK_DIST` | integer | `-1` | -1: use limits set inside look instructions >-1: limit sight distance of look instructions to this number of cells |
| `look_disable` | `LOOK_DISABLE` | integer | `0` | 0: none 1: input habitat register 2: input sight dist sought 3: input type of search (e.g. closest vs count vs total) 4: input resource/org id sought 5: output habitat used 6: output distance used 7: output search type used 8: output resource/org id used 9: output count (edible) 10: outptu amount/value seen 11: output id seen 12: output org forage target seen |
| `look_disable_type` | `LOOK_DISABLE_TYPE` | integer | `0` | 0: predators 1: prey 2: both predators and prey |
| `look_disable_combo` | `LOOK_DISABLE_COMBO` | integer | `0` | # 0: none # 1: return 'not found' for any food resource query # 2: return 'not found' for any looking-for-predator query # 3: return 'not found' for any looking-for-prey query |
| `pred_confusion` | `PRED_CONFUSION` | integer | `0` | # If 1, pred will get random data returned to registers when seen prey with odds of 0.1 * n, where n = # neighbors that that prey has. # If 2, pred will get random data returned to registers with odds of n/8, where n = # facings among neighbors that that prey has. # If 3, will be number of opinions seen / number of opinions in restricted list. # If 4, look executions fail for predators. |
| `track_look_settings` | `TRACK_LOOK_SETTINGS` | flag | `false` | # track (final) settings for look sensor use |
| `track_look_output` | `TRACK_LOOK_OUTPUT` | flag | `false` | # track (final) output from sensor use |
| `use_display` | `USE_DISPLAY` | integer | `0` | If 1, org display data is always 'on' (visible). If 2, org display is on and sensor does not set potential data. |
| `use_mimics` | `USE_MIMICS` | flag | `false` | If true, org's with forage target of 1 can show a deceptive ft number (as seen by other orgs via sensor) |
| `mimic_odds` | `MIMIC_ODDS` | number | `1.0` | Odds that a mimic will appear to other organisms as the thing it is mimicing. |
| `set_ft_at_birth` | `SET_FT_AT_BIRTH` | flag | `false` | Should offspring set forage target at birth? false: No, true: Yes |

### `pheromone`

Pheromones.

| Key | C++ name | Type | Default | Description |
|---|---|---|---|---|
| `enabled` | `PHEROMONE_ENABLED` | flag | `false` | Enable pheromone usage (true = on, false = off) |
| `amount` | `PHEROMONE_AMOUNT` | number | `1.0` | Amount of pheromone to add per drop |
| `drop_mode` | `PHEROMONE_DROP_MODE` | integer | `0` | Where to drop pheromone 0 = Half amount at src, half at dest 1 = All at source 2 = All at dest |
| `exploit_explore_prob` | `EXPLOIT_EXPLORE_PROB` | number | `0.00` | Probability of random exploration instead of pheromone trail following |
| `exploit_log_start` | `EXPLOIT_LOG_START` | integer | `0` | Update at which to start logging exploit moves |
| `explore_log_start` | `EXPLORE_LOG_START` | integer | `0` | Update at which to start logging explore moves |
| `log_inject` | `LOG_INJECT` | flag | `false` | Log injection of organisms (true = on, false = off) |
| `inject_log_start` | `INJECT_LOG_START` | integer | `0` | Update at which to start logging injection of organisms |

### `sync`

Synchronization.

| Key | C++ name | Type | Default | Description |
|---|---|---|---|---|
| `fitness_window` | `SYNC_FITNESS_WINDOW` | integer | `100` | Number of updates over which to calculate fitness (default=100). |
| `flash_lossrate` | `SYNC_FLASH_LOSSRATE` | number | `0.0` | P() to lose a flash send (0.0==off). |
| `test_flash_arrival` | `SYNC_TEST_FLASH_ARRIVAL` | integer | `-1` | CPU cycle at which an organism will receive a flash (off=-1, default=-1, analyze mode only.) |

### `consensus`

Consensus.

| Key | C++ name | Type | Default | Description |
|---|---|---|---|---|
| `hold_time` | `CONSENSUS_HOLD_TIME` | integer | `1` | Number of updates that consensus must be held for. |

### `reputation`

Reputation.

| Key | C++ name | Type | Default | Description |
|---|---|---|---|---|
| `raw_material_amount` | `RAW_MATERIAL_AMOUNT` | integer | `100` | Number of raw materials an organism starts with |
| `auto` | `AUTO_REPUTATION` | integer | `0` | Is an organism's reputation automatically computed based on its donations 0=no 1=increment for each donation + standing 2=+1 for donations given -1 for donations received 3=1 for donors -1 for recivers who have not donated 4=+1 for donors 5=+1 for donors during task check |
| `alt_benefit` | `ALT_BENEFIT` | integer | `1.00` | Number multiplied by the number of raw materials received from another organism to compute reward |
| `alt_cost` | `ALT_COST` | integer | `1.00` | Number multiplied by the number of your raw materials |
| `rotate_on_donate` | `ROTATE_ON_DONATE` | flag | `false` | Rotate an organism to face its donor (false = off, true = on) |
| `reward` | `REPUTATION_REWARD` | flag | `false` | Reward an organism for having a good reputation |
| `donation_failure_percent` | `DONATION_FAILURE_PERCENT` | integer | `0` | Percentage of times that a donation fails |
| `randomize_raw_material_amount` | `RANDOMIZE_RAW_MATERIAL_AMOUNT` | flag | `false` | Should all the organisms receive the same amount (false = same amount, true = randomized)? |
| `donation_restrictions` | `DONATION_RESTRICTIONS` | integer | `0` | 0=none 1=inter-species only 2=different tag only |
| `inherit` | `INHERIT_REPUTATION` | integer | `0` | 0=reputations are not inherited 1=reputations are inherited 2=tags are inherited |
| `specialists` | `SPECIALISTS` | flag | `false` | false = generalists allowed true = only specialists |
| `string_amount_cap` | `STRING_AMOUNT_CAP` | integer | `-1` | -1=no cap on string amounts #=CAP |
| `match_already_produced` | `MATCH_ALREADY_PRODUCED` | flag | `false` | false = off true = on |

### `groups`

Group formation and predator-prey.

| Key | C++ name | Type | Default | Description |
|---|---|---|---|---|
| `use_form_groups` | `USE_FORM_GROUPS` | integer | `0` | Enable organisms to form groups. 0=off, 1=on no restrict, 2=on restrict to defined |
| `default_group` | `DEFAULT_GROUP` | integer | `-1` | Default group to assign to organisms not asserting a group membership (-1 indicates disabled) |
| `inherit_opinion` | `INHERIT_OPINION` | flag | `true` | Should offspring inherit the parent's opinion? false: No... reset on divide, etc. true: Yes |
| `opinion_buffer_size` | `OPINION_BUFFER_SIZE` | integer | `1` | Size of the opinion buffer (stores opinions set over the organism's lifetime); -1=inf, default=1, cannot be 0. |
| `join_group_failure` | `JOIN_GROUP_FAILURE` | integer | `0` | Percent chance for failing to switch groups. If negative, is % chance of death. |
| `tolerance_window` | `TOLERANCE_WINDOW` | integer | `0` | Window of previous updates used to evaluate org's tolerance levels (0 indicates tolarance disabled, values <0 indicate % chance random migration for offspring) |
| `max_tolerance` | `MAX_TOLERANCE` | integer | `1` | Maximum tolerance level |
| `tolerance_variations` | `TOLERANCE_VARIATIONS` | integer | `0` | 0=all tolerance active, 1=only immigration tolerance active, 2=immigrants + sex |
| `track_tolerance` | `TRACK_TOLERANCE` | flag | `false` | Turn on/off detailed recording of tolerance change circumstances (Warning: can be slow) |
| `pred_prey_switch` | `PRED_PREY_SWITCH` | integer | `-1` | -2: no predators, but track prey stats -1: no predators in experiment 0: don't allow a predator to switch to being a prey (prey to pred always allowed) 1: allow predators to switch to being prey 2: don't allow a predator to switch to being a prey & don't allow prey to switch via set-forage-target (via attack allowed) ) |
| `pred_efficiency` | `PRED_EFFICIENCY` | number | `1.0` | Multiply the current bonus, merit, and resource bin amounts of the consumed prey by this value and add to current predator values (for bonus, merit, and bin consumption instructions). |
| `pred_efficiency_poison` | `PRED_EFFICIENCY_POISON` | number | `1.0` | Multiply the current bonus, merit, and resource bin amounts of the consumed prey by this value and subtract from current predator values if this prey is poisonous (ft == 2). |
| `pred_odds` | `PRED_ODDS` | number | `1.0` | Probability of success for predator 'attack' instructions. |
| `pred_injury` | `PRED_INJURY` | number | `0.0` | If an attack fails, target's bonus, merit, and internal resources are reduced by this fraction. |
| `min_prey` | `MIN_PREY` | integer | `0` | If positive (recommended for prey studies), predator attacks fail if num prey falls below this (0 = off). If negative (recommended for predator studies), random prey of genotype other than target will be cloned (using birth placement methods). |
| `max_pred` | `MAX_PRED` | integer | `0` | Population cap on number of predators (random predator will be removed when cap is exceeded). |
| `max_prey` | `MAX_PREY` | integer | `0` | Population cap on number of prey (random prey will be removed when cap is exceeded). For births, classification as prey is based on parent. |
| `max_prey_bt` | `MAX_PREY_BT` | integer | `0` | Population cap on number of prey (random prey will be removed when cap is exceeded) based on FT. |
| `track_group_attack_details` | `TRACK_GROUP_ATTACK_DETAILS` | integer | `0` | Track details around execution of EVERY group attack instructions for every update. 1 = as string in one file. 2 = as bits in new file for every update that this is on! |
| `marking_expire_date` | `MARKING_EXPIRE_DATE` | integer | `-1` | Number of updates markings in cells will remain effective on territory move. |
| `prey_mut_off` | `PREY_MUT_OFF` | flag | `false` | Turn off prey mutations in the exp hardware repro inst. |

### `deme_network`

Deme networks.

| Key | C++ name | Type | Default | Description |
|---|---|---|---|---|
| `type` | `DEME_NETWORK_TYPE` | integer | `0` | 0=topology, structure of network determines fitness. |
| `requires_connectedness` | `DEME_NETWORK_REQUIRES_CONNECTEDNESS` | flag | `true` | Whether the deme's network must be connected before an actual fitness is calculated. |
| `topology_fitness` | `DEME_NETWORK_TOPOLOGY_FITNESS` | integer | `0` | Network measure used to determine fitness; see cDemeTopologyNetwork.h. |
| `link_decay` | `DEME_NETWORK_LINK_DECAY` | integer | `0` | Number of updates after which a link decays; 0=no decay (default). |
| `remove_node_on_death` | `DEME_NETWORK_REMOVE_NODE_ON_DEATH` | flag | `false` | Whether death of an organism in the deme removes its links; false = no (default); true = yes. |

### `hgt`

Horizontal gene transfer.

| Key | C++ name | Type | Default | Description |
|---|---|---|---|---|
| `enabled` | `ENABLE_HGT` | flag | `false` | Whether HGT is enabled; false (default), true. |
| `source` | `HGT_SOURCE` | integer | `0` | Source of HGT fragments; 0=dead organisms (default), 1=parent. |
| `fragment_selection` | `HGT_FRAGMENT_SELECTION` | integer | `0` | Method used to select fragments for HGT mutation; 0=random (default), 1=trimmed selection 2=random placement. |
| `fragment_size_mean` | `HGT_FRAGMENT_SIZE_MEAN` | number | `10` | Mean size of fragments (default=10). |
| `fragment_size_variance` | `HGT_FRAGMENT_SIZE_VARIANCE` | number | `2` | Variance of fragments (default=2). |
| `max_fragments_per_cell` | `HGT_MAX_FRAGMENTS_PER_CELL` | integer | `100` | Max. allowed number of fragments per cell (default=100). |
| `diffusion_method` | `HGT_DIFFUSION_METHOD` | integer | `0` | Method to use for diffusion of genome fragments; 0=none (default). |
| `competence_p` | `HGT_COMPETENCE_P` | number | `0.0` | Probability that an HGT 'natural competence' mutation will occur on divide (default=0.0). |
| `insertion_mut_p` | `HGT_INSERTION_MUT_P` | number | `0.0` | Probability that an HGT mutation will result in an insertion (default=0.0). |
| `conjugation_method` | `HGT_CONJUGATION_METHOD` | integer | `0` | Method used to select the receiver and/or donor of an HGT conjugation; 0=random from neighborhood (default); 1=faced. |
| `conjugation_p` | `HGT_CONJUGATION_P` | number | `0.0` | Probability that an HGT conjugation mutation will occur on divide (default=0.0). |
| `fragment_xform` | `HGT_FRAGMENT_XFORM` | integer | `0` | Transformation to apply to each fragment prior to incorporation into offspring's genome; 0=none (default), 1=random shuffle, 2=replace with random instructions. |

### `inst_resources`

Resource-dependent instructions.

| Key | C++ name | Type | Default | Description |
|---|---|---|---|---|
| `resource` | `INST_RES` | text | `` | Resource upon which the execution of certain instruction depends |
| `floor` | `INST_RES_FLOOR` | number | `0.0` | Assumed lower level of resource in environment.  Used for probability dist. |
| `ceil` | `INST_RES_CEIL` | number | `0.0` | Assumed upper level of resource in environment.  Used for probability dist. |

### `alarm`

Alarms.

| Key | C++ name | Type | Default | Description |
|---|---|---|---|---|
| `bcast_hops` | `BCAST_HOPS` | integer | `1` | Number of hops to broadcast an alarm |
| `self` | `ALARM_SELF` | flag | `false` | Does sending an alarm move sender IP to alarm label? false = no true = yes |

### `division_of_labor`

Division of labor.

| Key | C++ name | Type | Default | Description |
|---|---|---|---|---|
| `age_poly_tracking` | `AGE_POLY_TRACKING` | flag | `false` | Print data for an age-task histogram |
| `reaction_thresh` | `REACTION_THRESH` | integer | `0` | The number of times the deme must perform each reaction in order to replicate |
| `task_switch_penalty` | `TASK_SWITCH_PENALTY` | integer | `0` | Cost of task switching in cycles |
| `task_switch_penalty_type` | `TASK_SWITCH_PENALTY_TYPE` | integer | `0` | Type of task switch cost: (0) none (1) learning, (2) retooling or context, (3) centrifuge |
| `res_for_deme_rep` | `RES_FOR_DEME_REP` | integer | `0` | The amount of resources that must be consumed prior to automatic deme replication |
| `learning_count` | `LEARNING_COUNT` | integer | `0` | The number of times a task must be performed to avoid efficiency penalties |

### `execution`

Execution mode of the port (no C++ counterpart).

| Key | C++ name | Type | Default | Description |
|---|---|---|---|---|
| `mode` | `EXECUTION_MODE` | integer | `0` | 0 = exact: the C++ scheduler, one instruction per time slice (byte parity with the C++) 1 = parallel: per-update instruction budgets, organism-local instructions on worker threads (deterministic for a seed, not byte-identical to the C++) |
| `threads` | `EXECUTION_THREADS` | integer | `0` | Worker threads for EXECUTION_MODE 1 (0 or -1 = all available cores) |

## Old names

Every C++ setting with its place in the experiment file.

| C++ name | In the experiment file |
|---|---|
| `ABSORB_RESOURCE_FRACTION` | `hoarding.absorb_resource_fraction` |
| `ACTIVE_MESSAGES_ENABLED` | `messaging.active_messages_enabled` |
| `AGE_DEVIATION` | `reproduction.age_deviation` |
| `AGE_LIMIT` | `reproduction.age_limit` |
| `AGE_POLY_TRACKING` | `division_of_labor.age_poly_tracking` |
| `ALARM_SELF` | `alarm.self` |
| `ALLOC_METHOD` | `reproduction.alloc_method` |
| `ALLOW_MATE_SELECTION` | `recombination.allow_mate_selection` |
| `ALLOW_PARENT` | `reproduction.allow_parent` |
| `ALT_BENEFIT` | `reputation.alt_benefit` |
| `ALT_COST` | `reputation.alt_cost` |
| `ANALYZE_FILE` | `analyze.file` |
| `ANALYZE_MODE` | `analyze.mode` |
| `ANALYZE_OPTION_1` | `analyze.option_1` |
| `ANALYZE_OPTION_2` | `analyze.option_2` |
| `APPLY_ENERGY_METHOD` | `energy.apply_method` |
| `ATTACK_DECAY_RATE` | `energy.attack_decay_rate` |
| `AUTO_REPUTATION` | `reputation.auto` |
| `AVATAR_BIRTH` | `movement.avatar_birth` |
| `AVATAR_BIRTH_FACING` | `movement.avatar_birth_facing` |
| `AVE_TIME_SLICE` | `time.ave_time_slice` |
| `BASE_CONST_MERIT` | `time.base_const_merit` |
| `BASE_MERIT_METHOD` | `time.base_merit_method` |
| `BCAST_HOPS` | `alarm.bcast_hops` |
| `BIRTH_METHOD` | `reproduction.birth_method` |
| `CHECK_TASK_ON_SEND` | `messaging.check_task_on_send` |
| `CHILD_SIZE_RANGE` | `divide.offspring_size_range` (an alias) |
| `COLLECT_AMOUNT` | `hoarding.collect_amount` |
| `COLLECT_PROB_DIVISOR` | `hoarding.collect_prob_divisor` |
| `COLLECT_SPECIFIC_RESOURCE` | `hoarding.collect_specific_resource` |
| `COLOR_DIFF` | `colors.diff` |
| `COLOR_MUT_LETHAL` | `colors.mut_lethal` |
| `COLOR_MUT_NEG` | `colors.mut_neg` |
| `COLOR_MUT_NEUT` | `colors.mut_neut` |
| `COLOR_MUT_POS` | `colors.mut_pos` |
| `COLOR_NEG1` | `colors.neg1` |
| `COLOR_NEG2` | `colors.neg2` |
| `COLOR_POS1` | `colors.pos1` |
| `COLOR_POS2` | `colors.pos2` |
| `COLOR_SAME` | `colors.same` |
| `CONSENSUS_HOLD_TIME` | `consensus.hold_time` |
| `CONSTITUTIVE_REGULATION` | `promoters.constitutive_regulation` |
| `CONT_REC_REGS` | `recombination.cont_rec_regs` |
| `COPY_DEL_PROB` | `mutation.copy_del_prob` |
| `COPY_INS_PROB` | `mutation.copy_ins_prob` |
| `COPY_MUT_PROB` | `mutation.copy_prob` |
| `COPY_SLIP_PROB` | `mutation.copy_slip_prob` |
| `COPY_UNIFORM_PROB` | `mutation.copy_uniform_prob` |
| `CORESPOND_REC_REGS` | `recombination.correspond_rec_regs` |
| `DATA_DIR` | `general.data_dir` |
| `DEADLY_BOUNDARIES` | `movement.deadly_boundaries` |
| `DEATH_METHOD` | `reproduction.death_method` |
| `DEATH_PROB` | `reproduction.death_prob` |
| `DEATH_PROB_PARENT` | `reproduction.death_prob_parent` |
| `DEFAULT_BONUS` | `time.default_bonus` |
| `DEFAULT_GROUP` | `groups.default_group` |
| `DEMES_COMPETITION_STYLE` | `demes.competition_style` |
| `DEMES_DEFAULT_GERMLINE_PROPENSITY` | `demes.default_germline_propensity` |
| `DEMES_DIVIDE_METHOD` | `demes.divide_method` |
| `DEMES_FOUNDER_GERMLINE_PROPENSITY` | `demes.founder_germline_propensity` |
| `DEMES_MAX_AGE` | `demes.max_age` |
| `DEMES_MAX_BIRTHS` | `demes.max_births` |
| `DEMES_MIGRATION_METHOD` | `demes.migration_method` |
| `DEMES_MIGRATION_RATE` | `demes.migration_rate` |
| `DEMES_MIM_EVENTS_KILLED_RATIO` | `demes.mim_events_killed_ratio` |
| `DEMES_MIM_SUCCESSFUL_EVENT_PERIODS` | `demes.mim_successful_event_periods` |
| `DEMES_MUT_ORGS_ON_REPLICATION` | `demes.mut_orgs_on_replication` |
| `DEMES_NUM_X` | `demes.num_x` |
| `DEMES_ORGANISM_FACING` | `demes.organism_facing` |
| `DEMES_ORGANISM_PLACEMENT` | `demes.organism_placement` |
| `DEMES_ORGANISM_SELECTION` | `demes.organism_selection` |
| `DEMES_ORGS_START_IN_GERM` | `demes.orgs_start_in_germ` |
| `DEMES_OVERRIDE_FITNESS` | `demes.override_fitness` |
| `DEMES_PARASITE_MIGRATION_MEMORY_SCORE_PROTECTIVE_THRESHOLD` | `demes.parasite_migration_memory_score_protective_threshold` |
| `DEMES_PARASITE_MIGRATION_RATE` | `demes.parasite_migration_rate` |
| `DEMES_PARASITE_MIGRATION_TARGET_SELECTION_METHOD` | `demes.parasite_migration_target_selection_method` |
| `DEMES_PARTITION_INTERVAL` | `demes.partition_interval` |
| `DEMES_PREFER_EMPTY` | `demes.prefer_empty` |
| `DEMES_PREVENT_STERILE` | `demes.prevent_sterile` |
| `DEMES_PROB_ORG_TRANSFER` | `demes.prob_org_transfer` |
| `DEMES_PROTECTION_POINTS` | `demes.protection_points` |
| `DEMES_REPLICATE_BIRTHS` | `demes.replicate_births` |
| `DEMES_REPLICATE_CPU_CYCLES` | `demes.replicate_cpu_cycles` |
| `DEMES_REPLICATE_LOG_START` | `demes.replicate_log_start` |
| `DEMES_REPLICATE_ORGS` | `demes.replicate_orgs` |
| `DEMES_REPLICATE_SIZE` | `demes.replicate_size` |
| `DEMES_REPLICATE_TIME` | `demes.replicate_time` |
| `DEMES_REPLICATION_ONLY_RESETS` | `demes.replication_only_resets` |
| `DEMES_RESET_RESOURCES` | `demes.reset_resources` |
| `DEMES_SEED_METHOD` | `demes.seed_method` |
| `DEMES_TOURNAMENT_SIZE` | `demes.tournament_size` |
| `DEMES_TRACK_SHANNON_INFO` | `demes.track_shannon_info` |
| `DEMES_USE_GERMLINE` | `demes.use_germline` |
| `DEME_NETWORK_LINK_DECAY` | `deme_network.link_decay` |
| `DEME_NETWORK_REMOVE_NODE_ON_DEATH` | `deme_network.remove_node_on_death` |
| `DEME_NETWORK_REQUIRES_CONNECTEDNESS` | `deme_network.requires_connectedness` |
| `DEME_NETWORK_TOPOLOGY_FITNESS` | `deme_network.topology_fitness` |
| `DEME_NETWORK_TYPE` | `deme_network.type` |
| `DISABLE_GENOTYPE_CLASSIFICATION` | `mating.disable_genotype_classification` |
| `DISPERSAL_RATE` | `reproduction.dispersal_rate` |
| `DIVIDE_DEL_PROB` | `mutation.divide_del_prob` |
| `DIVIDE_FAILURE_RESETS` | `reproduction.divide_failure_resets` |
| `DIVIDE_INS_PROB` | `mutation.divide_ins_prob` |
| `DIVIDE_LGT_PROB` | `mutation.divide_lgt_prob` |
| `DIVIDE_METHOD` | `reproduction.divide_method` |
| `DIVIDE_MUT_PROB` | `mutation.divide_prob` |
| `DIVIDE_POISSON_DEL_MEAN` | `mutation.divide_poisson_del_mean` |
| `DIVIDE_POISSON_INS_MEAN` | `mutation.divide_poisson_ins_mean` |
| `DIVIDE_POISSON_LGT_MEAN` | `mutation.divide_poisson_lgt_mean` |
| `DIVIDE_POISSON_MUT_MEAN` | `mutation.divide_poisson_mean` |
| `DIVIDE_POISSON_SLIP_MEAN` | `mutation.divide_poisson_slip_mean` |
| `DIVIDE_POISSON_TRANS_MEAN` | `mutation.divide_poisson_trans_mean` |
| `DIVIDE_SLIP_PROB` | `mutation.divide_slip_prob` |
| `DIVIDE_TRANS_PROB` | `mutation.divide_trans_prob` |
| `DIVIDE_UNIFORM_PROB` | `mutation.divide_uniform_prob` |
| `DIV_DEL_PROB` | `mutation.div_del_prob` |
| `DIV_INS_PROB` | `mutation.div_ins_prob` |
| `DIV_LGT_PROB` | `mutation.div_lgt_prob` |
| `DIV_MUT_PROB` | `mutation.div_prob` |
| `DIV_SLIP_PROB` | `mutation.div_slip_prob` |
| `DIV_TRANS_PROB` | `mutation.div_trans_prob` |
| `DIV_UNIFORM_PROB` | `mutation.div_uniform_prob` |
| `DONATE_THRESH_QUANTA` | `altruism.donate_thresh_quanta` |
| `DONATION_FAILURE_PERCENT` | `reputation.donation_failure_percent` |
| `DONATION_RESTRICTIONS` | `reputation.donation_restrictions` |
| `DROP_EXTINCT_GENOTYPES` | `genealogy.drop_extinct_genotypes` |
| `ENABLE_HGT` | `hgt.enabled` |
| `ENABLE_MP` | `mp.enabled` |
| `ENERGY_CAP` | `energy.cap` |
| `ENERGY_COMPARISON_EPSILON` | `energy.comparison_epsilon` |
| `ENERGY_ENABLED` | `energy.enabled` |
| `ENERGY_GIVEN_AT_BIRTH` | `energy.given_at_birth` |
| `ENERGY_GIVEN_ON_INJECT` | `energy.given_on_inject` |
| `ENERGY_PASSED_ON_DEME_REPLICATION_METHOD` | `energy.passed_on_deme_replication_method` |
| `ENERGY_REQUEST_RADIUS` | `energy.request_radius` |
| `ENERGY_SHARING_INCREMENT` | `energy_sharing.increment` |
| `ENERGY_SHARING_METHOD` | `energy_sharing.method` |
| `ENERGY_SHARING_PCT` | `energy_sharing.pct` |
| `ENERGY_SHARING_UPDATE_METABOLIC` | `energy_sharing.update_metabolic` |
| `ENERGY_THRESH_HIGH` | `energy.thresh_high` |
| `ENERGY_THRESH_LOW` | `energy.thresh_low` |
| `ENVIRONMENT_FILE` | the section `environment` |
| `ENV_FRACTION_THRESHOLD` | `hoarding.env_fraction_threshold` |
| `EPIGENETIC_METHOD` | `reproduction.epigenetic_method` |
| `EVENT_FILE` | the section `events` |
| `EXECUTION_MODE` | `execution.mode` |
| `EXECUTION_THREADS` | `execution.threads` |
| `EXPLOIT_EXPLORE_PROB` | `pheromone.exploit_explore_prob` |
| `EXPLOIT_LOG_START` | `pheromone.exploit_log_start` |
| `EXPLORE_LOG_START` | `pheromone.explore_log_start` |
| `FAIL_IMPLICIT` | `reversion.sterilize_unstable` (an alias) |
| `FITNESS_COEFF_1` | `time.fitness_coeff_1` |
| `FITNESS_COEFF_2` | `time.fitness_coeff_2` |
| `FITNESS_METHOD` | `time.fitness_method` |
| `FITNESS_VALLEY` | `time.fitness_valley` |
| `FITNESS_VALLEY_START` | `time.fitness_valley_start` |
| `FITNESS_VALLEY_STOP` | `time.fitness_valley_stop` |
| `FIX_METABOLIC_RATE` | `energy.fix_metabolic_rate` |
| `FORCED_MATE_PREFERENCE` | `mating.forced_preference` |
| `FRAC_ENERGY_DECAY_AT_DEME_BIRTH` | `energy.frac_energy_decay_at_deme_birth` |
| `FRAC_ENERGY_DECAY_AT_ORG_BIRTH` | `energy.frac_energy_decay_at_org_birth` |
| `FRAC_ENERGY_RELINQUISH` | `energy.frac_energy_relinquish` |
| `FRAC_ENERGY_TRANSFER` | `energy.frac_energy_transfer` |
| `FRAC_PARENT_ENERGY_GIVEN_TO_DEME_AT_BIRTH` | `energy.frac_parent_energy_given_to_deme_at_birth` |
| `FRAC_PARENT_ENERGY_GIVEN_TO_ORG_AT_BIRTH` | `energy.frac_parent_energy_given_to_org_at_birth` |
| `FULL_VERTICAL_TRANS` | `parasites.full_vertical_trans` |
| `GENERATION_INC_METHOD` | `reproduction.generation_inc_method` |
| `GENOTYPE_PHENPLAST_CALC` | `time.genotype_phenplast_calc` |
| `GERMLINE_COPY_MUT` | `demes.germline_copy_mut` |
| `GERMLINE_DEL_MUT` | `demes.germline_del_mut` |
| `GERMLINE_INS_MUT` | `demes.germline_ins_mut` |
| `HARDWARE_TYPE` | read by `avida convert` only (it chose a legacy instruction-set file) |
| `HGT_COMPETENCE_P` | `hgt.competence_p` |
| `HGT_CONJUGATION_METHOD` | `hgt.conjugation_method` |
| `HGT_CONJUGATION_P` | `hgt.conjugation_p` |
| `HGT_DIFFUSION_METHOD` | `hgt.diffusion_method` |
| `HGT_FRAGMENT_SELECTION` | `hgt.fragment_selection` |
| `HGT_FRAGMENT_SIZE_MEAN` | `hgt.fragment_size_mean` |
| `HGT_FRAGMENT_SIZE_VARIANCE` | `hgt.fragment_size_variance` |
| `HGT_FRAGMENT_XFORM` | `hgt.fragment_xform` |
| `HGT_INSERTION_MUT_P` | `hgt.insertion_mut_p` |
| `HGT_MAX_FRAGMENTS_PER_CELL` | `hgt.max_fragments_per_cell` |
| `HGT_SOURCE` | `hgt.source` |
| `HOST_USE_GENOTYPE_FILE` | `parasites.host_use_genotype_file` |
| `IMMUNITY_REACTION` | `divide.immunity_reaction` |
| `IMMUNITY_TASK` | `divide.immunity_task` |
| `IMPLICIT_BG_PROMOTER_RATE` | `gene_expression.implicit_bg_promoter_rate` |
| `IMPLICIT_GENE_EXPRESSION` | `gene_expression.implicit_gene_expression` |
| `IMPLICIT_MAX_PROGRAMID_LENGTH` | `gene_expression.implicit_max_programid_length` |
| `IMPLICIT_REPRO_BONUS` | `divide.implicit_repro_bonus` |
| `IMPLICIT_REPRO_CPU_CYCLES` | `divide.implicit_repro_cpu_cycles` |
| `IMPLICIT_REPRO_END` | `divide.implicit_repro_end` |
| `IMPLICIT_REPRO_ENERGY` | `divide.implicit_repro_energy` |
| `IMPLICIT_REPRO_TIME` | `divide.implicit_repro_time` |
| `IMPLICIT_TURNOVER_RATE` | `gene_expression.implicit_turnover_rate` |
| `INFECTION_MECHANISM` | `parasites.infection_mechanism` |
| `INHERIT_EXE_RATE` | `energy.inherit_exe_rate` |
| `INHERIT_MERIT` | `reproduction.inherit_merit` |
| `INHERIT_MULTITHREAD` | `reproduction.inherit_multithread` |
| `INHERIT_MULTI_THREAD_CLASSIFICATION` | `reproduction.inherit_multithread` (an alias) |
| `INHERIT_OPINION` | `groups.inherit_opinion` |
| `INHERIT_REPUTATION` | `reputation.inherit` |
| `INJECT_DEFAULT_SUCCESS` | `parasites.inject_default_success` |
| `INJECT_DEL_PROB` | `mutation.inject_del_prob` |
| `INJECT_INS_PROB` | `mutation.inject_ins_prob` |
| `INJECT_IS_TASK_SPECIFIC` | `parasites.infection_mechanism` (an alias) |
| `INJECT_IS_VIRULENT` | `parasites.inject_is_virulent` |
| `INJECT_LOG_START` | `pheromone.inject_log_start` |
| `INJECT_METHOD` | `parasites.inject_method` |
| `INJECT_MUT_PROB` | `mutation.inject_prob` |
| `INJECT_QMA_EXPONENT` | `parasites.inject_qma_exponent` |
| `INJECT_RESETS_TASKS` | `analyze.inject_resets_tasks` |
| `INJECT_SKIP_FIRST_TASK` | `parasites.inject_skip_first_task` |
| `INJECT_STERILIZES_HOST` | `parasites.inject_sterilizes_host` |
| `INST_CODE_DEFAULT_TYPE` | `promoters.inst_code_default_type` |
| `INST_CODE_LENGTH` | `promoters.inst_code_length` |
| `INST_POINT_MUT_PROB` | `mutation.inst_point_prob` |
| `INST_POINT_MUT_SLOPE` | `mutation.inst_point_slope` |
| `INST_POINT_REPAIR_COST` | `mutation.inst_point_repair_cost` |
| `INST_RES` | `inst_resources.resource` |
| `INST_RES_CEIL` | `inst_resources.ceil` |
| `INST_RES_FLOOR` | `inst_resources.floor` |
| `INST_SET` | the section `instruction_sets` |
| `INST_SET_LOAD_LEGACY` | read by `avida convert` only (it chose a legacy instruction-set file) |
| `IO_EXPIRE` | `cpu.io_expire` |
| `JOIN_GROUP_FAILURE` | `groups.join_group_failure` |
| `JUV_PERIOD` | `reproduction.juvenile_period` |
| `KABOOM1_HAMMING` | `kaboom.hamming1` |
| `KABOOM2_HAMMING` | `kaboom.hamming2` |
| `KABOOM3_HAMMING` | `kaboom.hamming3` |
| `KABOOM4_HAMMING` | `kaboom.hamming4` |
| `KABOOM5_HAMMING` | `kaboom.hamming5` |
| `KABOOM_EFFECT` | `kaboom.effect` |
| `KABOOM_HAMMING` | `kaboom.hamming` |
| `KABOOM_PROB` | `kaboom.prob` |
| `KABOOM_RADIUS` | `kaboom.radius` |
| `LEARNING_COUNT` | `division_of_labor.learning_count` |
| `LEGACY_GRID_LOCAL_SELECTION` | `reproduction.legacy_grid_local_selection` |
| `LEKKING` | `mating.lekking` |
| `LGT_FILL_MODE` | `mutation.lgt_fill_mode` |
| `LGT_SOURCE_REGION` | `mutation.lgt_source_region` |
| `LOG_DEMES_REPLICATE` | `demes.log_replicate` |
| `LOG_INJECT` | `pheromone.log_inject` |
| `LOG_PARASITE_INJECTIONS` | `parasites.log_injections` |
| `LOG_SLEEP_TIMES` | `energy.log_sleep_times` |
| `LOOK_DISABLE` | `sensing.look_disable` |
| `LOOK_DISABLE_COMBO` | `sensing.look_disable_combo` |
| `LOOK_DISABLE_TYPE` | `sensing.look_disable_type` |
| `LOOK_DIST` | `sensing.look_dist` |
| `MARKING_EXPIRE_DATE` | `groups.marking_expire_date` |
| `MATCH_ALREADY_PRODUCED` | `reputation.match_already_produced` |
| `MATE_ASSESSMENT_CV` | `mating.assessment_cv` |
| `MATE_IN_GROUPS` | `mating.in_groups` |
| `MATING_TYPES` | `mating.types` |
| `MAX_BIRTH_WAIT_TIME` | `recombination.max_birth_wait_time` |
| `MAX_CONCURRENCY` | `analyze.max_concurrency` |
| `MAX_CPU_THREADS` | `time.max_cpu_threads` |
| `MAX_DONATES` | `altruism.max_donates` |
| `MAX_DONATE_EDIT_DIST` | `altruism.max_donate_edit_dist` |
| `MAX_DONATE_KIN_DIST` | `altruism.max_donate_kin_dist` |
| `MAX_GENOME_SIZE` | `divide.max_genome_size` |
| `MAX_GLOBAL_BIRTH_CHAMBER_SIZE` | `mating.max_global_birth_chamber_size` |
| `MAX_LABEL_EXE_SIZE` | `time.max_label_exe_size` |
| `MAX_PRED` | `groups.max_pred` |
| `MAX_PREY` | `groups.max_prey` |
| `MAX_PREY_BT` | `groups.max_prey_bt` |
| `MAX_PROGRAMIDS` | `gene_expression.max_programids` |
| `MAX_PROGRAMID_AGE` | `gene_expression.max_programid_age` |
| `MAX_TOLERANCE` | `groups.max_tolerance` |
| `MAX_TOTAL_STORED` | `hoarding.max_total_stored` |
| `MAX_UNIQUE_TASK_COUNT` | `divide.max_unique_task_count` |
| `MERIT_BONUS_EFFECT` | `time.merit_bonus_effect` |
| `MERIT_BONUS_INST` | `time.merit_bonus_inst` |
| `MERIT_DEFAULT_BONUS` | `time.merit_default_bonus` |
| `MERIT_GIVEN` | `altruism.merit_given` |
| `MERIT_INC_APPLY_IMMEDIATE` | `time.merit_inc_apply_immediate` |
| `MERIT_RECEIVED` | `altruism.merit_received` |
| `MESSAGE_RECV_BUFFER_BEHAVIOR` | `messaging.recv_buffer_behavior` |
| `MESSAGE_RECV_BUFFER_SIZE` | `messaging.recv_buffer_size` |
| `MESSAGE_SEND_BUFFER_SIZE` | `messaging.send_buffer_size` |
| `META_COPY_MUT` | `mutation.meta_copy_prob` |
| `META_STD_DEV` | `mutation.meta_std_dev` |
| `MIGRATION_FILE` | `demes.migration_file` |
| `MIGRATION_RATE` | `demes.uniform_migration_rate` |
| `MIMIC_ODDS` | `sensing.mimic_odds` |
| `MIN_COPIED_LINES` | `divide.min_copied_lines` |
| `MIN_CYCLES` | `divide.min_cycles` |
| `MIN_EXE_LINES` | `divide.min_exe_lines` |
| `MIN_GB_DONATE_THRESHOLD` | `altruism.min_gb_donate_threshold` |
| `MIN_GENOME_SIZE` | `divide.min_genome_size` |
| `MIN_PREY` | `groups.min_prey` |
| `MODULE_NUM` | `recombination.module_num` |
| `MOVEMENT_COLLISIONS_LETHAL` | `movement.collisions_lethal` |
| `MOVEMENT_COLLISIONS_SELECTION_TYPE` | `movement.collisions_selection_type` |
| `MP_SCHEDULING_STYLE` | `mp.scheduling_style` |
| `MULTI_ABSORB_TYPE` | `hoarding.multi_absorb_type` |
| `MUT_RATE_SOURCE` | `mutation.rate_source` |
| `NET_DROP_PROB` | `network.drop_prob` |
| `NET_LOG_MESSAGES` | `network.log_messages` |
| `NET_LOG_RETMESSAGES` | `network.log_retmessages` |
| `NEURAL_NETWORKING` | `messaging.neural_networking` |
| `NEUTRAL_MAX` | `reversion.neutral_max` |
| `NEUTRAL_MIN` | `reversion.neutral_min` |
| `NOISY_MATE_ASSESSMENT` | `mating.noisy_assessment` |
| `NOISY_QS_SD` | `kaboom.noisy_qs_sd` |
| `NON_1_RESOURCE_RATIOS` | `hoarding.non_1_resource_ratios` |
| `NO_ACTIVE_PROMOTER_EFFECT` | `promoters.no_active_promoter_effect` |
| `NO_CPU_CYCLE_TIME` | `time.no_cpu_cycle_time` |
| `NO_MUT_INSTS` | `kaboom.no_mut_insts` |
| `NUM_CYCLES_EXC_BEFORE_0_ENERGY` | `energy.num_cycles_exc_before_0_energy` |
| `NUM_DEMES` | `demes.count` |
| `OFFSPRING_SIZE_RANGE` | `divide.offspring_size_range` |
| `OPINION_BUFFER_SIZE` | `groups.opinion_buffer_size` |
| `PARASITE_MEM_SPACES` | `parasites.mem_spaces` |
| `PARASITE_NO_COPY_MUT` | `parasites.no_copy_mut` |
| `PARASITE_SKIP_REACTIONS` | `parasites.skip_reactions` |
| `PARASITE_USE_GENOTYPE_FILE` | `parasites.use_genotype_file` |
| `PARASITE_VIRULENCE` | `parasites.virulence` |
| `PARENT_DEL_PROB` | `mutation.parent_del_prob` |
| `PARENT_INS_PROB` | `mutation.parent_ins_prob` |
| `PARENT_MUT_PROB` | `mutation.parent_prob` |
| `PHEROMONE_AMOUNT` | `pheromone.amount` |
| `PHEROMONE_DROP_MODE` | `pheromone.drop_mode` |
| `PHEROMONE_ENABLED` | `pheromone.enabled` |
| `POINT_DEL_PROB` | `mutation.point_del_prob` |
| `POINT_INS_PROB` | `mutation.point_ins_prob` |
| `POINT_MUT_PROB` | `mutation.point_prob` |
| `POINT_MUT_REPAIR_START` | `mutation.point_repair_start` |
| `POISON_PENALTY` | `cpu.poison_penalty` |
| `POPULATION_CAP` | `general.population_cap` |
| `POP_CAP_ELDEST` | `general.population_cap_eldest` |
| `PRECALC_PHENOTYPE` | `time.precalc_phenotype` |
| `PRED_CONFUSION` | `sensing.pred_confusion` |
| `PRED_EFFICIENCY` | `groups.pred_efficiency` |
| `PRED_EFFICIENCY_POISON` | `groups.pred_efficiency_poison` |
| `PRED_INJURY` | `groups.pred_injury` |
| `PRED_ODDS` | `groups.pred_odds` |
| `PRED_PREY_SWITCH` | `groups.pred_prey_switch` |
| `PREFER_EMPTY` | `reproduction.prefer_empty` |
| `PREY_MUT_OFF` | `groups.prey_mut_off` |
| `PROMOTERS_ENABLED` | `promoters.enabled` |
| `PROMOTER_CODE_SIZE` | `promoters.code_size` |
| `PROMOTER_EXE_LENGTH` | `promoters.exe_length` |
| `PROMOTER_EXE_THRESHOLD` | `promoters.exe_threshold` |
| `PROMOTER_INST_MAX` | `promoters.inst_max` |
| `PROMOTER_PROCESSIVITY` | `promoters.processivity` |
| `PROMOTER_PROCESSIVITY_INST` | `promoters.processivity_inst` |
| `PROMOTER_TO_REGISTER` | `promoters.to_register` |
| `RANDOMIZE_RAW_MATERIAL_AMOUNT` | `reputation.randomize_raw_material_amount` |
| `RANDOM_SEED` | `general.random_seed` |
| `RAW_MATERIAL_AMOUNT` | `reputation.raw_material_amount` |
| `REACTION_THRESH` | `division_of_labor.reaction_thresh` |
| `RECOMBINATION_PROB` | `recombination.prob` |
| `REPRO_METHOD` | `reproduction.repro_method` |
| `REPUTATION_REWARD` | `reputation.reward` |
| `REQUIRED_BONUS` | `divide.required_bonus` |
| `REQUIRED_PRED_HABITAT` | `divide.required_pred_habitat` |
| `REQUIRED_PRED_HABITAT_VALUE` | `divide.required_pred_habitat_value` |
| `REQUIRED_PREY_HABITAT` | `divide.required_prey_habitat` |
| `REQUIRED_PREY_HABITAT_VALUE` | `divide.required_prey_habitat_value` |
| `REQUIRED_REACTION` | `divide.required_reaction` |
| `REQUIRED_RESOURCE` | `divide.required_resource` |
| `REQUIRED_RESOURCE_LEVEL` | `divide.required_resource_level` |
| `REQUIRED_TASK` | `divide.required_task` |
| `REQUIRE_ALLOCATE` | `divide.require_allocate` |
| `REQUIRE_EXACT_COPY` | `divide.require_exact_copy` |
| `REQUIRE_SINGLE_REACTION` | `divide.require_single_reaction` |
| `RESET_INPUTS_ON_DIVIDE` | `reproduction.reset_inputs_on_divide` |
| `RESOURCE_GIVEN_AT_BIRTH` | `hoarding.resource_given_at_birth` |
| `RESOURCE_GIVEN_ON_INJECT` | `hoarding.resource_given_on_inject` |
| `RESOURCE_SHARING_LOSS` | `energy_sharing.resource_sharing_loss` |
| `RES_FOR_DEME_REP` | `division_of_labor.res_for_deme_rep` |
| `RETURN_STORED_ON_DEATH` | `hoarding.return_stored_on_death` |
| `REVERT_BENEFICIAL` | `reversion.revert_beneficial` |
| `REVERT_DETRIMENTAL` | `reversion.revert_detrimental` |
| `REVERT_EQUALS` | `reversion.revert_equals` |
| `REVERT_FATAL` | `reversion.revert_fatal` |
| `REVERT_NEUTRAL` | `reversion.revert_neutral` |
| `REVERT_TASKLOSS` | `reversion.revert_taskloss` |
| `ROTATE_ON_DONATE` | `reputation.rotate_on_donate` |
| `SAME_LENGTH_SEX` | `recombination.same_length_sex` |
| `SAVE_RECEIVED` | `buy_sell.save_received` |
| `SCALE_FREE_ALPHA` | `world.scale_free_alpha` |
| `SCALE_FREE_M` | `world.scale_free_m` |
| `SCALE_FREE_ZERO_APPEAL` | `world.scale_free_zero_appeal` |
| `SELF_COMMUNICATION` | `messaging.self_communication` |
| `SET_FT_AT_BIRTH` | `sensing.set_ft_at_birth` |
| `SLICING_METHOD` | `time.slicing_method` |
| `SLIP_COPY_MODE` | `mutation.slip_copy_mode` |
| `SLIP_FILL_MODE` | `mutation.slip_fill_mode` |
| `SPECIALISTS` | `reputation.specialists` |
| `SPECIAL_MUT_LINE` | `mutation.special_line` |
| `SPECULATIVE` | `general.speculative` |
| `SPLIT_ON_DIVIDE` | `hoarding.split_on_divide` |
| `STEP_COUNTING_ERROR` | `movement.step_counting_error` |
| `STERILIZE_BENEFICIAL` | `reversion.sterilize_beneficial` |
| `STERILIZE_DETRIMENTAL` | `reversion.sterilize_detrimental` |
| `STERILIZE_FATAL` | `reversion.sterilize_fatal` |
| `STERILIZE_NEUTRAL` | `reversion.sterilize_neutral` |
| `STERILIZE_TASKLOSS` | `reversion.sterilize_taskloss` |
| `STERILIZE_UNSTABLE` | `reversion.sterilize_unstable` |
| `STRING_AMOUNT_CAP` | `reputation.string_amount_cap` |
| `SYNC_FITNESS_WINDOW` | `sync.fitness_window` |
| `SYNC_FLASH_LOSSRATE` | `sync.flash_lossrate` |
| `SYNC_TEST_FLASH_ARRIVAL` | `sync.test_flash_arrival` |
| `TASK_REFRACTORY_PERIOD` | `time.task_refractory_period` |
| `TASK_SWITCH_PENALTY` | `division_of_labor.task_switch_penalty` |
| `TASK_SWITCH_PENALTY_TYPE` | `division_of_labor.task_switch_penalty_type` |
| `TERMINATION_RESETS` | `promoters.termination_resets` |
| `TEST_CPU_TIME_MOD` | `genealogy.test_cpu_time_mod` |
| `THREAD_SLICING_METHOD` | `time.thread_slicing_method` |
| `THRESHOLD` | `genealogy.threshold` |
| `TOLERANCE_VARIATIONS` | `groups.tolerance_variations` |
| `TOLERANCE_WINDOW` | `groups.tolerance_window` |
| `TRACK_BIRTH_LOCS` | `movement.track_birth_locs` |
| `TRACK_CCLADES` | `second_pass.track_cclades` |
| `TRACK_CCLADES_IDS` | `second_pass.track_cclades_ids` |
| `TRACK_GROUP_ATTACK_DETAILS` | `groups.track_group_attack_details` |
| `TRACK_LOOK_OUTPUT` | `sensing.track_look_output` |
| `TRACK_LOOK_SETTINGS` | `sensing.track_look_settings` |
| `TRACK_TOLERANCE` | `groups.track_tolerance` |
| `TRANS_FILL_MODE` | `mutation.trans_fill_mode` |
| `TWO_FOLD_COST_SEX` | `recombination.two_fold_cost_sex` |
| `USE_AVATARS` | `movement.use_avatars` |
| `USE_DISPLAY` | `sensing.use_display` |
| `USE_FORM_GROUPS` | `groups.use_form_groups` |
| `USE_MIMICS` | `sensing.use_mimics` |
| `USE_RESOURCE_BINS` | `hoarding.use_resource_bins` |
| `USE_STORED_FRACTION` | `hoarding.use_stored_fraction` |
| `VERBOSITY` | `general.verbosity` |
| `VIRULENCE_MUT_RATE` | `parasites.virulence_mut_rate` |
| `VIRULENCE_SD` | `parasites.virulence_sd` |
| `VIRULENCE_SOURCE` | `parasites.virulence_source` |
| `VITALITY_BIN_CENTER` | `movement.vitality_bin_center` |
| `VITALITY_BIN_EXTREMES` | `movement.vitality_bin_extremes` |
| `WORLD_GEOMETRY` | `world.geometry` |
| `WORLD_X` | `world.x` |
| `WORLD_Y` | `world.y` |
