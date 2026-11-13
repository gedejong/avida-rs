# Settings reference (avida.cfg)

This page lists every setting that `avida.cfg` accepts, grouped as the file groups them. The list was generated from the settings table of the `avida-config` crate (`crates/config/src/config_table.rs`), so the names, types and defaults are the ones the program uses. The descriptions are the program's own, lightly reformatted; where one is terse, the [concepts](../concepts/index.md) pages explain the machinery behind it.

There are 431 settings in 40 groups, plus two settings that exist only in the Rust port (see [Settings of the Rust port](#settings-of-the-rust-port)).

## How settings are given

- One setting per line in `avida.cfg`: the name, whitespace, the value. Everything after `#` is a comment.
- Any setting can be overridden on the command line with `-set NAME VALUE`, which wins over the file. Example: `avida -set WORLD_X 30 -set WORLD_Y 30`.
- `avida -r` prints a review of the settings that differ from their defaults.
- A setting missing from the file takes the default listed here. `avida -w` warns about each default that gets used.
- Types are `int`, `double` (a decimal number), `bool` (`0` or `1`) and `string`.
- Settings of groups you do not use have no effect. Several groups describe features that need a particular hardware type or an organism instruction; the group's description says which.

Most runs change a few dozen settings at most. The groups you will meet first are [GENERAL_GROUP](#general_group), [TOPOLOGY_GROUP](#topology_group), [CONFIG_FILE_GROUP](#config_file_group), [MUTATION_GROUP](#mutation_group), [REPRODUCTION_GROUP](#reproduction_group), [DIVIDE_GROUP](#divide_group) and [TIME_GROUP](#time_group).

## Settings of the Rust port

Two settings do not exist in the C++ Avida. They are read like any other setting, and the C++-format printers leave them out; `avida -r` lists them when they differ from their defaults.

| Setting | Type | Default | Meaning |
|---|---|---|---|
| `EXECUTION_MODE` | int | `0` | `0` is the exact mode: the C++ scheduler, byte-identical output. `1` is the opt-in parallel mode. Any other value stops at start-up. See [Execution modes](../execution/index.md). |
| `EXECUTION_THREADS` | int | `0` | Worker threads for `EXECUTION_MODE 1`. `0` or less means every core the machine reports. |

## Groups


- [GENERAL_GROUP](#general_group) (5 settings)
- [TOPOLOGY_GROUP](#topology_group) (6 settings)
- [CONFIG_FILE_GROUP](#config_file_group) (5 settings)
- [MUTATION_GROUP](#mutation_group) (47 settings)
- [REPRODUCTION_GROUP](#reproduction_group) (18 settings)
- [DIVIDE_GROUP](#divide_group) (26 settings)
- [RECOMBINATION_GROUP](#recombination_group) (8 settings)
- [MATING_TYPES_GROUP](#mating_types_group) (8 settings)
- [PARASITE_GROUP](#parasite_group) (18 settings)
- [ARCHETECTURE_GROUP](#archetecture_group) (2 settings)
- [MP_GROUP](#mp_group) (2 settings)
- [DEME_GROUP](#deme_group) (43 settings)
- [REVERSION_GROUP](#reversion_group) (14 settings)
- [TIME_GROUP](#time_group) (22 settings)
- [ALTRUISM_GROUP](#altruism_group) (7 settings)
- [KABOOM_GROUP](#kaboom_group) (11 settings)
- [GENEOLOGY_GROUP](#geneology_group) (2 settings)
- [ORGANISM_NETWORK_GROUP](#organism_network_group) (3 settings)
- [ORGANISM_MESSAGING_GROUP](#organism_messaging_group) (7 settings)
- [BUY_SELL_GROUP](#buy_sell_group) (1 settings)
- [HOARD_RESOURCE_GROUP](#hoard_resource_group) (14 settings)
- [ANALYZE_GROUP](#analyze_group) (4 settings)
- [ENERGY_GROUP](#energy_group) (21 settings)
- [ENERGY_SHARING_GROUP](#energy_sharing_group) (5 settings)
- [SECOND_PASS_GROUP](#second_pass_group) (2 settings)
- [GX_GROUP](#gx_group) (6 settings)
- [PROMOTER_GROUP](#promoter_group) (13 settings)
- [COLORS_GROUP](#colors_group) (10 settings)
- [MOVEMENT_GROUP](#movement_group) (21 settings)
- [PHEROMONE_GROUP](#pheromone_group) (8 settings)
- [SYNCHRONIZATION_GROUP](#synchronization_group) (3 settings)
- [CONSENSUS_GROUP](#consensus_group) (1 settings)
- [REPUTATION_GROUP](#reputation_group) (13 settings)
- [GROUP_FORMATION_GROUP](#group_formation_group) (21 settings)
- [DEME_NETWORK_GROUP](#deme_network_group) (5 settings)
- [HGT_GROUP](#hgt_group) (12 settings)
- [INST_RES_GROUP](#inst_res_group) (3 settings)
- [ALARM_GROUP](#alarm_group) (2 settings)
- [DIVISION_OF_LABOR_GROUP](#division_of_labor_group) (6 settings)
- [DEPRECATED_GROUP](#deprecated_group) (6 settings)

## GENERAL_GROUP

General Settings.

| Setting | Type | Default | Description |
|---|---|---|---|
| `VERBOSITY` | int | `1` | 0 = No output at all<br>1 = Normal output<br>2 = Verbose output, detailing progress<br>3 = High level of details, as available<br>4 = Print Debug Information, as applicable |
| `RANDOM_SEED` | int | `-1` | Random number seed (&lt;0 for based on time) |
| `SPECULATIVE` | bool | `1` | Enable speculative execution<br>(pre-execute instructions that don't affect other organisms) |
| `POPULATION_CAP` | int | `0` | Carrying capacity in number of organisms (use 0 for no cap) |
| `POP_CAP_ELDEST` | int | `0` | Carrying capacity in number of organisms (use 0 for no cap). Will kill oldest organism in population, but still use birth method to place new offspring. |

## TOPOLOGY_GROUP

World topology.

| Setting | Type | Default | Description |
|---|---|---|---|
| `WORLD_X` | int | `60` | Width of the Avida world |
| `WORLD_Y` | int | `60` | Height of the Avida world |
| `WORLD_GEOMETRY` | int | `2` | 1 = Bounded Grid (WOLRD_X x WORLD_Y)<br>2 = Toroidal Grid (WOLRD_X x WORLD_Y; wraps at edges<br>3 = Clique (all population cells are connected)<br>4 = Hexagonal grid<br>5 = Partial<br>6 = 3D Lattice (under development)<br>7 = Random connected<br>8 = Scale-free (detailed below) |
| `SCALE_FREE_M` | int | `3` | Number of connections per cell in a scale-free geometry |
| `SCALE_FREE_ALPHA` | double | `1.0` | Attachment power (1=linear) |
| `SCALE_FREE_ZERO_APPEAL` | double | `0.0` | Appeal of cells with zero connections |

## CONFIG_FILE_GROUP

Other configuration Files.

| Setting | Type | Default | Description |
|---|---|---|---|
| `DATA_DIR` | string | `data` | Directory in which config files are found |
| `EVENT_FILE` | string | `events.cfg` | File containing list of events during run |
| `ANALYZE_FILE` | string | `analyze.cfg` | File used for analysis mode |
| `ENVIRONMENT_FILE` | string | `environment.cfg` | File that describes the environment |
| `MIGRATION_FILE` | string | `-` | NxN file that describes connectivity weights between demes |

## MUTATION_GROUP

Mutation rates.

| Setting | Type | Default | Description |
|---|---|---|---|
| `COPY_MUT_PROB` | double | `0.0075` | Substitution rate (per copy) |
| `COPY_INS_PROB` | double | `0.0` | Insertion rate (per copy) |
| `COPY_DEL_PROB` | double | `0.0` | Deletion rate (per copy) |
| `COPY_UNIFORM_PROB` | double | `0.0` | Uniform mutation probability (per copy)<br>- Randomly apply insertion, deletion or substition mutation |
| `COPY_SLIP_PROB` | double | `0.0` | Slip rate (per copy) |
| `POINT_MUT_PROB` | double | `0.0` | Point (Cosmic-Ray) substitution rate (per-location per update) |
| `POINT_INS_PROB` | double | `0.0` | Point (Cosmic-Ray) insertion rate (per-location per update) |
| `POINT_DEL_PROB` | double | `0.0` | Point (Cosmic-Ray) deletion rate (per-location per update) |
| `INST_POINT_MUT_PROB` | double | `0.0` | Mutation rate (per-location per time instruction (point-mut) is executed) |
| `INST_POINT_MUT_SLOPE` | double | `0.0` | Slope for point mutation rate |
| `INST_POINT_REPAIR_COST` | int | `0` | The cost, in cycles, of avoiding mutations when the point-mut instruction is executed |
| `POINT_MUT_REPAIR_START` | int | `0` | The starting condition for repairs (on=1; off=0) |
| `DIV_MUT_PROB` | double | `0.0` | Substitution rate (per site, applied on divide) |
| `DIV_INS_PROB` | double | `0.0` | Insertion rate (per site, applied on divide) |
| `DIV_DEL_PROB` | double | `0.0` | Deletion rate (per site, applied on divide) |
| `DIV_UNIFORM_PROB` | double | `0.0` | Uniform mutation probability (per site, applied on divide)<br>- Randomly apply insertion, deletion or point mutation |
| `DIV_SLIP_PROB` | double | `0.0` | Slip rate (per site, applied on divide) |
| `DIV_TRANS_PROB` | double | `0.0` | Translocation rate (per site, applied on divide) |
| `DIV_LGT_PROB` | double | `0.0` | Lateral Gene Transfer rate (per site, applied on divide) |
| `DIVIDE_MUT_PROB` | double | `0.0` | Substitution rate (max one, per divide) |
| `DIVIDE_INS_PROB` | double | `0.05` | Insertion rate (max one, per divide) |
| `DIVIDE_DEL_PROB` | double | `0.05` | Deletion rate (max one, per divide) |
| `DIVIDE_UNIFORM_PROB` | double | `0.0` | Uniform mutation probability (per divide)<br>- Randomly apply insertion, deletion or point mutation |
| `DIVIDE_SLIP_PROB` | double | `0.0` | Slip rate (per divide) - creates large deletions/duplications |
| `DIVIDE_TRANS_PROB` | double | `0.0` | Translocation rate (per divide) - creates large deletions/duplications |
| `DIVIDE_LGT_PROB` | double | `0.0` | Lateral Gene Transfer rate (per divide) - creates large deletions/duplications |
| `DIVIDE_POISSON_MUT_MEAN` | double | `0.0` | Substitution rate (Poisson distributed, per divide) |
| `DIVIDE_POISSON_INS_MEAN` | double | `0.0` | Insertion rate (Poisson distributed, per divide) |
| `DIVIDE_POISSON_DEL_MEAN` | double | `0.0` | Deletion rate (Poisson distributed, per divide) |
| `DIVIDE_POISSON_SLIP_MEAN` | double | `0.0` | Slip rate (Poisson distributed, per divide) |
| `DIVIDE_POISSON_TRANS_MEAN` | double | `0.0` | Translocation rate (Poisson distributed, per divide) |
| `DIVIDE_POISSON_LGT_MEAN` | double | `0.0` | Lateral Gene Transfer rate (Poisson distributed, per divide) |
| `INJECT_MUT_PROB` | double | `0.0` | Substitution rate (per site, applied on inject) |
| `INJECT_INS_PROB` | double | `0.0` | Insertion rate (per site, applied on inject) |
| `INJECT_DEL_PROB` | double | `0.0` | Deletion rate (per site, applied on inject) |
| `SLIP_FILL_MODE` | int | `0` | Fill insertions from slip mutations with:<br>0 = Duplication<br>1 = nop-X<br>2 = Random<br>3 = Scrambled<br>4 = nop-C |
| `SLIP_COPY_MODE` | int | `0` | How to handle 'on-copy' slip mutations:<br>0 = actual read head slip<br>1 = instant large mutation (obeys slip mode) |
| `TRANS_FILL_MODE` | int | `0` | Fill insertions from translocation mutations with:<br>0 = Duplication<br>1 = Scrambled |
| `LGT_FILL_MODE` | int | `0` | Fill insertions from lateral gene transfer mutations with:<br>0 = Duplication<br>1 = scrambled |
| `LGT_SOURCE_REGION` | int | `0` | Extract gene for lateral transfer from:<br>0 = Local Neighborhood<br>1 = Entire Population |
| `PARENT_MUT_PROB` | double | `0.0` | Parent substitution rate (per-site, applied on divide) |
| `PARENT_INS_PROB` | double | `0.0` | Parent insertion rate (per-site, applied on divide) |
| `PARENT_DEL_PROB` | double | `0.0` | Parent deletion rate (per-site, applied on divide) |
| `SPECIAL_MUT_LINE` | int | `-1` | If this is >= 0, ONLY this line is mutated |
| `META_COPY_MUT` | double | `0.0` | Prob. of copy mutation rate changing (per gen) |
| `META_STD_DEV` | double | `0.0` | Standard deviation of meta mutation size. |
| `MUT_RATE_SOURCE` | int | `1` | 1 = Mutation rates determined by environment.<br>2 = Mutation rates inherited from parent. |

## REPRODUCTION_GROUP

Birth and Death config options.

| Setting | Type | Default | Description |
|---|---|---|---|
| `DIVIDE_FAILURE_RESETS` | int | `0` | When Divide fails, organisms are interally reset |
| `BIRTH_METHOD` | int | `0` | Which organism should be replaced when a birth occurs?<br>0 = Random organism in neighborhood<br>1 = Oldest in neighborhood<br>2 = Largest Age/Merit in neighborhood<br>3 = None (use only empty cells in neighborhood)<br>4 = Random from population (Mass Action)<br>5 = Oldest in entire population<br>6 = Random within deme<br>7 = Organism faced by parent<br>8 = Next grid cell (id+1)<br>9 = Largest energy used in entire population<br>10 = Largest energy used in neighborhood<br>11 = Local neighborhood dispersal<br>12 = Kill offpsring after recording birth stats (for behavioral trials)<br>13 = Kill parent and offpsring (for behavioral trials) |
| `PREFER_EMPTY` | int | `1` | Overide BIRTH_METHOD to preferentially choose empty cells for offsping? |
| `ALLOW_PARENT` | int | `1` | Should parents be considered when deciding where to place offspring? |
| `DISPERSAL_RATE` | double | `0.0` | Rate of dispersal under birth method 11<br>(poisson distributed random connection list hops) |
| `DEATH_PROB` | double | `0.0` | Probability of death when dividing. (@MAM: actually seems to be applied per-update?) |
| `DEATH_PROB_PARENT` | double | `0.0` | Probability of death when dividing. (@MAM: proper DEATH_PROB impl) |
| `DEATH_METHOD` | int | `2` | When should death by old age occur?<br>0 = Never<br>1 = When executed AGE_LIMIT (+deviation) total instructions<br>2 = When executed genome_length * AGE_LIMIT (+dev) instructions |
| `AGE_LIMIT` | int | `20` | See DEATH_METHOD |
| `AGE_DEVIATION` | int | `0` | Creates a normal distribution around AGE_LIMIT for time of death |
| `JUV_PERIOD` | int | `0` | Number of CPU cycles before newborn orgs can execute various instructions / behaviors |
| `ALLOC_METHOD` | int | `0` | When allocating blank tape, how should it be initialized?<br>0 = Allocated space is set to default instruction.<br>1 = Set to section of dead genome (creates potential for recombination)<br>2 = Allocated space is set to random instruction. |
| `DIVIDE_METHOD` | int | `1` | 0 = Divide leaves state of mother untouched.<br>1 = Divide resets state of mother(effectively creating 2 offspring)<br>2 = Divide resets state of current thread only (use with parasites) |
| `EPIGENETIC_METHOD` | int | `0` | Inheritance of state information other than genome<br>0 = none<br>1 = offspring inherits registers and stacks of first thread<br>1 = parent maintains registers and stacks of first thread<br>1 = offspring and parent keep state information |
| `GENERATION_INC_METHOD` | int | `1` | 0 = Only increase generation of offspring on divide.<br>1 = Increase generation of both parent and offspring<br>(suggested with DIVIDE_METHOD 1). |
| `RESET_INPUTS_ON_DIVIDE` | int | `0` | Reset environment inputs of parent upon successful divide. |
| `INHERIT_MERIT` | int | `1` | Should merit be inhereted from mother parent? (in asexual) |
| `INHERIT_MULTITHREAD`, `INHERIT_MULTI_THREAD_CLASSIFICATION` | int | `0` | Should offspring of parents with multiple threads be marked multithreaded? |

## DIVIDE_GROUP

Divide restrictions and triggers - settings describe conditions for a successful divide.

| Setting | Type | Default | Description |
|---|---|---|---|
| `OFFSPRING_SIZE_RANGE`, `CHILD_SIZE_RANGE` | double | `2.0` | Maximal differential between offspring and parent length.<br>(Checked BEFORE mutations applied on divide.) |
| `MIN_COPIED_LINES` | double | `0.5` | Code fraction that must be copied before divide |
| `MIN_EXE_LINES` | double | `0.5` | Code fraction that must be executed before divide |
| `MIN_GENOME_SIZE` | int | `0` | Minimum number of instructions allowed in a genome. 0 = OFF |
| `MAX_GENOME_SIZE` | int | `0` | Maximum number of instructions allowed in a genome. 0 = OFF |
| `MIN_CYCLES` | int | `0` | Min number of CPU cycles (age) required before reproduction. |
| `REQUIRE_ALLOCATE` | int | `1` | (Original CPU Only) Require allocate before divide? |
| `REQUIRED_TASK` | int | `-1` | Task ID required for successful divide |
| `IMMUNITY_TASK` | int | `-1` | Task providing immunity from the required task |
| `REQUIRED_REACTION` | int | `-1` | Reaction ID required for successful divide |
| `IMMUNITY_REACTION` | int | `-1` | Reaction ID that provides immunity for successful divide |
| `REQUIRE_SINGLE_REACTION` | int | `0` | If set to 1, at least one reaction is required for a successful divide; if set to n > 1, at least n reactions are required for a successful divide |
| `MAX_UNIQUE_TASK_COUNT` | int | `-1` | Division will fail if organism performs more than MAX_TASK_COUNT when MAX_TASK_COUNT >= 0 |
| `REQUIRED_BONUS` | double | `0.0` | Required bonus to divide |
| `REQUIRE_EXACT_COPY` | int | `0` | Require offspring to be an exact copy (checked before divide mutations) |
| `REQUIRED_RESOURCE` | int | `-1` | ID of resource required in organism's internal bins for successful<br>divide (resource not consumed) |
| `REQUIRED_RESOURCE_LEVEL` | double | `0.0` | Level of resource needed for REQUIRED_RESOURCE |
| `REQUIRED_PRED_HABITAT` | int | `-1` | Required resource habitat type in cell for predators to reproduce |
| `REQUIRED_PRED_HABITAT_VALUE` | double | `0` | Level of resource needed for REQUIRED_PRED_HABITAT |
| `REQUIRED_PREY_HABITAT` | int | `-1` | Required resource habitat type in cell for prey to reproduce |
| `REQUIRED_PREY_HABITAT_VALUE` | double | `0` | Level of resource needed for REQUIRED_PREY_HABITAT |
| `IMPLICIT_REPRO_BONUS` | int | `0` | Call Inst_Repro to divide upon achieving this bonus. 0 = OFF |
| `IMPLICIT_REPRO_CPU_CYCLES` | int | `0` | Call Inst_Repro after this many cpu cycles. 0 = OFF |
| `IMPLICIT_REPRO_TIME` | int | `0` | Call Inst_Repro after this time used. 0 = OFF |
| `IMPLICIT_REPRO_END` | int | `0` | Call Inst_Repro after executing the last instruction in the genome. |
| `IMPLICIT_REPRO_ENERGY` | double | `0.0` | Call Inst_Repro if organism accumulates this amount of energy. |

## RECOMBINATION_GROUP

Sexual Recombination and Modularity.

| Setting | Type | Default | Description |
|---|---|---|---|
| `RECOMBINATION_PROB` | double | `1.0` | Probability of recombination in div-sex |
| `MAX_BIRTH_WAIT_TIME` | int | `-1` | Updates incipiant orgs can wait for crossover (-1 = unlimited) |
| `MODULE_NUM` | int | `0` | Number of modules in the genome |
| `CONT_REC_REGS` | int | `1` | Are (modular) recombination regions continuous? |
| `CORESPOND_REC_REGS` | int | `1` | Are (modular) recombination regions swapped randomly<br>or with corresponding positions? |
| `TWO_FOLD_COST_SEX` | int | `0` | 0 = Both offspring are born (no two-fold cost)<br>1 = only one recombined offspring is born. |
| `SAME_LENGTH_SEX` | int | `0` | 0 = Recombine with any genome<br>1 = Recombine only w/ same length |
| `ALLOW_MATE_SELECTION` | bool | `0` | Allow organisms to select mates (requires instruction set support) |

## MATING_TYPES_GROUP

Mating Types and Mate Choice.

| Setting | Type | Default | Description |
|---|---|---|---|
| `MATING_TYPES` | bool | `0` | Turn on separate mating types (i.e., males/females; off by default; requires instruction set support) |
| `LEKKING` | bool | `0` | Offspring from males go directly into birth chamber to await female choice (off by default) |
| `MAX_GLOBAL_BIRTH_CHAMBER_SIZE` | int | `3600` | Maximum number of waiting that can be stored in the birth chamber in a well-mixed population (3600 by default) |
| `DISABLE_GENOTYPE_CLASSIFICATION` | bool | `0` | Disable tracking of historical genotypes to conserve memory (off by default) |
| `NOISY_MATE_ASSESSMENT` | bool | `0` | Is mate assessment perfect (0) or noisy (1) (0 by default) |
| `MATE_ASSESSMENT_CV` | double | `0.1` | Coefficient of variation for how noisy mate assessment is (0.1 by default) |
| `FORCED_MATE_PREFERENCE` | int | `-1` | Force all females to use a specific mate preference; -1 = off (mate preferences can evolve); 0 = all females mate randomly; 1 = all prefer highest display A; 2 = highest display B; 3 = highest merit |
| `MATE_IN_GROUPS` | bool | `0` | Require all mating to happen within groups |

## PARASITE_GROUP

Parasite config options.

| Setting | Type | Default | Description |
|---|---|---|---|
| `LOG_PARASITE_INJECTIONS` | bool | `0` | Log parasite replications? |
| `INJECT_METHOD` | int | `0` | What should happen to a parasite when it gives birth?<br>0 = Leave the parasite thread state untouched.<br>1 = Resets the state of the calling thread (for SMT parasites, this must be 1) |
| `INFECTION_MECHANISM`, `INJECT_IS_TASK_SPECIFIC` | int | `1` | 0: Infection always succeeds.<br>1: Infection succeeds if parasite matches at least one host task.<br>2: Infection succeeds if parasite does NOT match at least one task.<br>3: Parasite tasks must match host tasks exactly (Matching Alleles). |
| `INJECT_QMA_EXPONENT` | double | `0.2` | The exponent of the equation proportion_overlap^x that determines the probability of infection succeding given the amount a host and parasite phenotype match. |
| `INJECT_STERILIZES_HOST` | int | `0` | Infection causes host steralization |
| `INJECT_IS_VIRULENT` | int | `0` | Infection causes host steralization and takes all cpu cycles (setting this to 1 will override inject_virulence) |
| `PARASITE_SKIP_REACTIONS` | int | `1` | Parasite tasks do not get processed in the environment (1) or they do trigger reactions (0) |
| `INJECT_SKIP_FIRST_TASK` | int | `0` | They cannot match the first task the host is doing to infect |
| `INJECT_DEFAULT_SUCCESS` | double | `0.0` | If injection is task specific, with what probability should non-matching parasites infect the host |
| `PARASITE_VIRULENCE` | double | `-1` | The probabalistic percentage of cpu cycles allocated to the parasite instead of the host. Ensure INJECT_IS_VIRULENT is set to 0. This only works for single infection at the moment. Note that this should be set to a default even if virulence is evolving. |
| `VIRULENCE_SOURCE` | int | `0` | Virulence is set by config (0) or inhereted from parent (1) or host-controlled, i.e. donation (2) |
| `VIRULENCE_MUT_RATE` | double | `0.1` | The probability that virulence will mutate if it is inhereted from the parent or host-controlled (VIRULENCE_SOURCE = 1 & 2) |
| `VIRULENCE_SD` | double | `0.1` | New Virulence will be drawn from a normal distribution centered on the parental virulence, with this standard deviation |
| `PARASITE_MEM_SPACES` | int | `1` | Parasites get their own memory spaces |
| `PARASITE_NO_COPY_MUT` | int | `0` | Parasites do not get copy mutation rates |
| `PARASITE_USE_GENOTYPE_FILE` | int | `0` | Parasite Genotypes are loaded from a file rather than replicated from parent -- see LoadParasiteGenotypeList |
| `HOST_USE_GENOTYPE_FILE` | int | `0` | Host Genotypes are loaded from a file rather than replicated from parent -- see LoadHostGenotypeList |
| `FULL_VERTICAL_TRANS` | double | `0.0` | Determines if offspring of infected host is automatically infected. 0 for no, 1 for yes. If you want to keep parent infected as well, you need to set DIVIDE_METHOD to 2. |

## ARCHETECTURE_GROUP

Details on how CPU should work.

| Setting | Type | Default | Description |
|---|---|---|---|
| `IO_EXPIRE` | bool | `1` | Is the expiration functionality of '-expire' I/O instructions enabled? |
| `POISON_PENALTY` | double | `0.01` | Metabolic rate penalty applied when the 'poison' instruction is executed. |

## MP_GROUP

Config options for multiple, distributed populations.

| Setting | Type | Default | Description |
|---|---|---|---|
| `ENABLE_MP` | int | `0` | Enable multi-process Avida; 0=disabled (default),<br>1=enabled. |
| `MP_SCHEDULING_STYLE` | int | `0` | Style of scheduling:<br>0=non-MP aware (default)<br>1=MP aware, integrated across worlds. |

## DEME_GROUP

Demes and Germlines.

| Setting | Type | Default | Description |
|---|---|---|---|
| `NUM_DEMES` | int | `1` | Number of independent groups in the population |
| `DEMES_COMPETITION_STYLE` | int | `0` | How should demes compete?<br>0=Fitness proportional selection<br>1=Tournament selection |
| `DEMES_TOURNAMENT_SIZE` | int | `0` | Number of demes that participate in a tournament |
| `DEMES_OVERRIDE_FITNESS` | int | `0` | Should the calculated fitness is used?<br>0=yes (default)<br>1=no (all fitnesses=1) |
| `DEMES_USE_GERMLINE` | int | `0` | Should demes use a distinct germline? 0: No, 1: Traditional germ lines, 2: Genotype tracking, 3: Organism flagging germline |
| `DEMES_PREVENT_STERILE` | int | `0` | Prevent sterile demes from replicating? |
| `DEMES_RESET_RESOURCES` | int | `0` | Reset resources in demes on replication?<br>0 = reset both demes<br>1 = reset target deme<br>2 = deme resources remain unchanged |
| `DEMES_REPLICATE_SIZE` | int | `1` | Number of identical organisms to create or copy from the<br>source deme to the target deme |
| `LOG_DEMES_REPLICATE` | bool | `0` | Log deme replications? |
| `DEMES_REPLICATE_LOG_START` | int | `0` | Update at which to start logging deme replications |
| `DEMES_PROB_ORG_TRANSFER` | double | `0.0` | Probablity of an organism being transferred from the<br>source deme to the target deme |
| `DEMES_ORGANISM_SELECTION` | int | `0` | How should organisms be selected for transfer from<br>source to target during deme replication?<br>0 = random with replacement<br>1 = sequential<br>2-6 = created, but not sure what they do<br>7 = organism(s) flagged germline<br>8=one of the organisms flagged as part of the germline |
| `DEMES_ORGANISM_PLACEMENT` | int | `0` | How should organisms be placed during deme replication.<br>0 = cell-array middle<br>1 = deme center<br>2 = random placement<br>3 = sequential |
| `DEMES_ORGANISM_FACING` | int | `0` | Which direction should organisms face after deme replication.<br>0 = unchanged<br>1 = northwest.<br>2 = random. |
| `DEMES_MAX_AGE` | int | `500` | The maximum age of a deme (in updates) to be<br>used for age-based replication |
| `DEMES_MAX_BIRTHS` | int | `100` | Max number of births that can occur within a deme;<br>used with birth-count replication |
| `DEMES_MIM_EVENTS_KILLED_RATIO` | double | `0.7` | Minimum ratio of events killed required for event period to be a success. |
| `DEMES_MIM_SUCCESSFUL_EVENT_PERIODS` | int | `1` | Minimum number of consecutive event periods that must be a success. |
| `GERMLINE_COPY_MUT` | double | `0.0075` | Prob. of copy mutations during germline replication |
| `GERMLINE_INS_MUT` | double | `0.05` | Prob. of insertion mutations during germline replication |
| `GERMLINE_DEL_MUT` | double | `0.05` | Prob. of deletion mutations during germline replication |
| `DEMES_REPLICATE_CPU_CYCLES` | double | `0.0` | Replicate a deme immediately after it has used this many<br>cpu cycles per org in deme (0 = OFF). |
| `DEMES_REPLICATE_TIME` | double | `0.0` | Number of CPU cycles used by a deme to trigger its replication<br>(normalized by number of orgs in deme and organism merit; 0 = OFF). |
| `DEMES_REPLICATE_BIRTHS` | int | `0` | Number of offspring produced by a deme to trigger its replication (0 = OFF). |
| `DEMES_REPLICATE_ORGS` | int | `0` | Number of organisms in a deme to trigger its replication (0 = OFF). |
| `DEMES_REPLICATION_ONLY_RESETS` | int | `0` | Kin selection mode.  On replication:<br>0 = Nothing extra<br>1 = reset deme resources<br>2 = reset resources and re-inject organisms |
| `DEMES_MIGRATION_RATE` | double | `0.0` | Probability of an offspring being born in a different deme. |
| `DEMES_PARASITE_MIGRATION_RATE` | double | `0.0` | Probability of a parasite migrating to a different deme. Note: only works with DEMES_MIGRATION_METHOD 4. |
| `DEMES_MIGRATION_METHOD` | int | `0` | Which demes can an offspring land in when it migrates?<br>0 = Any other deme<br>1 = Eight neighboring demes<br>2 = Two adjacent demes in list<br>3 = Proportional based on the number of points<br>4 = Use the weight matrix specified in MIGRATION_FILE |
| `DEMES_PARASITE_MIGRATION_TARGET_SELECTION_METHOD` | int | `0` | How should a target cell be chosen in the migrated-to deme?<br>0 = Select a cell randomly, if it is not occupied infection fails<br>1 = Select an occupied cell randomly |
| `DEMES_PARASITE_MIGRATION_MEMORY_SCORE_PROTECTIVE_THRESHOLD` | double | `1.0` | Above what parasite memory score should hosts be protected from incoming parasite migration? |
| `DEMES_NUM_X` | int | `0` | Simulated number of demes in X dimension. Used only for migration. |
| `DEMES_PARTITION_INTERVAL` | int | `0` | Restrict deme replication to within partitions of DEMES_PARTITION_INTERVAL consecutive deme ids. No restriction if 0. Partition size cannot be smaller than two. Does not affect migration. |
| `DEMES_SEED_METHOD` | int | `0` | Deme seeding method.<br>0 = Maintain old consistency<br>1 = New method using genotypes |
| `DEMES_DIVIDE_METHOD` | int | `0` | Deme divide method. Only works with DEMES_SEED_METHOD 1<br>0 = Replace source and target demes<br>1 = Replace target deme, reset source deme to founders<br>2 = Replace target deme, leave source deme unchanged<br>3 = Replace the target deme, and reset the number of resources consumed by the source deme.<br>4 = Replace the target deme,  reset the number of resources consumed by the source deme, and kill the germ line organisms of the source deme |
| `DEMES_DEFAULT_GERMLINE_PROPENSITY` | double | `0.0` | Default germline propensity of organisms in deme.<br>For use with DEMES_DIVIDE_METHOD 2. |
| `DEMES_FOUNDER_GERMLINE_PROPENSITY` | double | `-1.0` | Default germline propensity of founder organisms in deme.<br>For use with DEMES_DIVIDE_METHOD 2.<br>&lt;0 = OFF |
| `DEMES_PREFER_EMPTY` | int | `0` | Give empty demes preference as targets of deme replication? |
| `DEMES_PROTECTION_POINTS` | int | `0` | The number of points a deme receives for each suicide. |
| `MIGRATION_RATE` | double | `0.0` | Uniform probability of offspring migrating to a new deme. |
| `DEMES_TRACK_SHANNON_INFO` | int | `0` | Enable shannon mutual information tracking for demes. |
| `DEMES_MUT_ORGS_ON_REPLICATION` | int | `0` | Mutate orgs using germline mutation rates when they are copied to a new deme (using DEMES_SEED_METHOD 1): 0=OFF, 1=ON |
| `DEMES_ORGS_START_IN_GERM` | int | `0` | Are orgs considered part of the germline at start? |

## REVERSION_GROUP

Mutation Reversion Most of these slow down avida a lot, and should be set to 0.0 normally.

| Setting | Type | Default | Description |
|---|---|---|---|
| `REVERT_FATAL` | double | `0.0` | Prob of lethal mutations being reverted on birth |
| `REVERT_DETRIMENTAL` | double | `0.0` | Prob of harmful (but non-lethal) mutations reverting on birth |
| `REVERT_NEUTRAL` | double | `0.0` | Prob of neutral mutations being reverted on birth |
| `REVERT_BENEFICIAL` | double | `0.0` | Prob of beneficial mutations being reverted on birth |
| `REVERT_TASKLOSS` | double | `0.0` | Prob of mutations that cause task loss without any tasks gained being reverted on birth |
| `REVERT_EQUALS` | double | `0.0` | Prob of mutations which grant EQU being reverted on birth |
| `STERILIZE_FATAL` | double | `0.0` | Prob of lethal mutations steralizing an offspring (typically no effect!) |
| `STERILIZE_DETRIMENTAL` | double | `0.0` | Prob of harmful (but non-lethal) mutations sterilizing an offspring |
| `STERILIZE_NEUTRAL` | double | `0.0` | Prob of neutral mutations sterilizing an offspring |
| `STERILIZE_BENEFICIAL` | double | `0.0` | Prob of beneficial mutations sterilizing an offspring |
| `STERILIZE_TASKLOSS` | double | `0.0` | Prob of mutations causing task loss without task gain sterilizing an offspring |
| `STERILIZE_UNSTABLE`, `FAIL_IMPLICIT` | int | `0` | Should genotypes that cannot replicate perfectly not be allowed to replicate? |
| `NEUTRAL_MAX` | double | `0.0` | Percent benifical change from parent fitness to be considered neutral. |
| `NEUTRAL_MIN` | double | `0.0` | Percent deleterious change from parent fitness to be considered neutral. |

## TIME_GROUP

Time Slicing.

| Setting | Type | Default | Description |
|---|---|---|---|
| `AVE_TIME_SLICE` | int | `30` | Average number of CPU-cycles per org per update |
| `SLICING_METHOD` | int | `1` | 0 = CONSTANT: all organisms receive equal number of CPU cycles<br>1 = PROBABILISTIC: CPU cycles distributed randomly, proportional to merit.<br>2 = INTEGRATED: CPU cycles given out deterministicly, proportional to merit<br>3 = DEME_PROBABALISTIC: Demes receive fixed number of CPU cycles, awarded probabalistically to members<br>4 = CROSS_DEME_PROBABALISTIC: Demes receive CPU cycles proportional to living population size, awarded probabalistically to members |
| `BASE_MERIT_METHOD` | int | `4` | How should merit be initialized?<br>0 = Constant (merit independent of size)<br>1 = Merit proportional to copied size<br>2 = Merit prop. to executed size<br>3 = Merit prop. to full size<br>4 = Merit prop. to min of executed or copied size<br>5 = Merit prop. to sqrt of the minimum size<br>6 = Merit prop. to num times MERIT_BONUS_INST is in genome. |
| `BASE_CONST_MERIT` | int | `100` | Base merit value for BASE_MERIT_METHOD 0 |
| `MERIT_BONUS_INST` | int | `0` | Instruction ID to count for BASE_MERIT_METHOD 6 |
| `MERIT_BONUS_EFFECT` | int | `0` | Amount of merit earn per instruction for BASE_MERIT_METHOD 6 (-1 = penalty, 0 = no effect) |
| `FITNESS_VALLEY` | int | `0` | in BASE_MERIT_METHOD 6, this creates valleys from<br>FITNESS_VALLEY_START to FITNESS_VALLEY_STOP<br>(0 = off, 1 = on) |
| `FITNESS_VALLEY_START` | int | `0` | if FITNESS_VALLEY = 1, orgs with num_key_instructions<br>from FITNESS_VALLEY_START to FITNESS_VALLEY_STOP<br>get fitness 1 (lowest) |
| `FITNESS_VALLEY_STOP` | int | `0` | if FITNESS_VALLEY = 1, orgs with num_key_instructions<br>from FITNESS_VALLEY_START to FITNESS_VALLEY_STOP<br>get fitness 1 (lowest) |
| `DEFAULT_BONUS` | double | `1.0` | Initial bonus before any tasks |
| `MERIT_DEFAULT_BONUS` | int | `0` | Instead of inheriting bonus from parent, use this value instead (0 = off) |
| `MERIT_INC_APPLY_IMMEDIATE` | bool | `0` | Should merit increases (above current) be applied immediately, or delayed until divide? |
| `TASK_REFRACTORY_PERIOD` | double | `0.0` | Number of updates after taske until regain full value |
| `FITNESS_METHOD` | int | `0` | 0 = default, 1 = sigmoidal, |
| `FITNESS_COEFF_1` | double | `1.0` | 1st FITNESS_METHOD parameter |
| `FITNESS_COEFF_2` | double | `1.0` | 2nd FITNESS_METHOD parameter |
| `MAX_CPU_THREADS` | int | `1` | Maximum number of Threads a CPU can spawn |
| `THREAD_SLICING_METHOD` | int | `0` | Formula for allocating CPU cycles across threads in an organism<br>(num_threads-1) * THREAD_SLICING_METHOD + 1<br>0 = One thread executed per time slice.<br>1 = All threads executed each time slice. |
| `NO_CPU_CYCLE_TIME` | int | `0` | Don't count each CPU cycle as part of gestation time |
| `MAX_LABEL_EXE_SIZE` | int | `1` | Max nops marked as executed when labels are used |
| `PRECALC_PHENOTYPE` | int | `0` | 0 = Disabled<br>1 = Assign precalculated merit at birth (unlimited resources only)<br>2 = Assign precalculated gestation time<br>3 = Assign precalculated merit AND gestation time.<br>4 = Assign last instruction counts<br>5 = Assign last instruction counts and merit<br>6 = Assign last instruction counts and gestation time<br>7 = Assign everything currently supported<br>Fitness will be evaluated for organism based on these settings. |
| `GENOTYPE_PHENPLAST_CALC` | int | `100` | Number of times to test a genotype's<br>plasticity during runtime. |

## ALTRUISM_GROUP

Altrusim.

| Setting | Type | Default | Description |
|---|---|---|---|
| `MERIT_GIVEN` | double | `0.0` | Fraction of merit donated with 'donate' command |
| `MERIT_RECEIVED` | double | `0.0` | Multiplier of merit given with 'donate' command |
| `MAX_DONATE_KIN_DIST` | int | `-1` | Limit on distance of relation for donate; -1=no max |
| `MAX_DONATE_EDIT_DIST` | int | `-1` | Limit on genetic (edit) distance for donate; -1=no max |
| `MIN_GB_DONATE_THRESHOLD` | int | `-1` | threshold green beard donates only to orgs above this<br>donation attempt threshold; -1=no thresh |
| `DONATE_THRESH_QUANTA` | int | `10` | The size of steps between quanta donate thresholds |
| `MAX_DONATES` | int | `1000000` | Limit on number of donates organisms are allowed. |

## KABOOM_GROUP

Kaboom.

| Setting | Type | Default | Description |
|---|---|---|---|
| `KABOOM_PROB` | double | `-1` | The probability (in decimal) that an explosion will occur when the instruction is encountered. -1 is default probability and allows the organism to change the probability. |
| `KABOOM_RADIUS` | int | `2` | Radius of all explosions (kaboom and kaboom5) |
| `KABOOM_EFFECT` | int | `10` | Merit increased or decreased for surrounding kin or non-kin |
| `KABOOM_HAMMING` | int | `0` | Hamming distance of kaboom's threshold, set to -1 to have adjustable, default is 0 for clone altruists. |
| `KABOOM1_HAMMING` | int | `1` | Hamming distance of kaboom1's threshold, set to -1 to have adjustable, default is 1. |
| `KABOOM2_HAMMING` | int | `2` | Hamming distance of kaboom2's threshold, set to -1 to have adjustable, default is 2. |
| `KABOOM3_HAMMING` | int | `3` | Hamming distance of kaboom3's threshold, set to -1 to have adjustable, default is 3. |
| `KABOOM4_HAMMING` | int | `4` | Hamming distance of kaboom4's threshold, set to -1 to have adjustable, default is 4. |
| `KABOOM5_HAMMING` | int | `5` | Hamming distance of kaboom5's threshold, set to -1 to have adjustable, default is 5 for clan altruists. |
| `NO_MUT_INSTS` | string | (empty) | A list of the instruction symbols that should not be able to be mutated out, NOT checked for all mutation types. |
| `NOISY_QS_SD` | double | `.05` | The standard deviation for the gaussian distribution (mean = 1) that is multiplied with the kin percent found for NoisyQuorum inst |

## GENEOLOGY_GROUP

Geneology.

| Setting | Type | Default | Description |
|---|---|---|---|
| `THRESHOLD` | int | `3` | Number of organisms in a genotype needed for it<br>to be considered viable. |
| `TEST_CPU_TIME_MOD` | int | `20` | Time allocated in test CPUs (multiple of length) |

## ORGANISM_NETWORK_GROUP

Organism Network Communication.

| Setting | Type | Default | Description |
|---|---|---|---|
| `NET_DROP_PROB` | double | `0.0` | Message drop rate |
| `NET_LOG_MESSAGES` | int | `0` | Whether all messages are logged; 0=false (default), 1=true. |
| `NET_LOG_RETMESSAGES` | int | `0` | Whether retrieved messages are logged; 0=false (default), 1=true. |

## ORGANISM_MESSAGING_GROUP

Organism Message-Based Communication.

| Setting | Type | Default | Description |
|---|---|---|---|
| `MESSAGE_SEND_BUFFER_SIZE` | int | `1` | Size of message send buffer (stores messages that were sent)<br>TASKS NOT CHECKED ON 0!<br>-1=inf, default=1. |
| `MESSAGE_RECV_BUFFER_SIZE` | int | `8` | Size of message receive buffer (stores messages that are received); -1=inf, default=8. |
| `MESSAGE_RECV_BUFFER_BEHAVIOR` | int | `0` | Behavior of message receive buffer; 0=drop oldest (default), 1=drop incoming |
| `ACTIVE_MESSAGES_ENABLED` | int | `0` | Enable active messages.<br>0 = off<br>2 = message creates parallel thread |
| `CHECK_TASK_ON_SEND` | bool | `1` | 0: Don't check tasks on send, 1: Check tasks on send (default) |
| `NEURAL_NETWORKING` | bool | `0` | Turns neural networking system on/off.<br>Requires USE_AVATARS be turned on. |
| `SELF_COMMUNICATION` | bool | `0` | Allows organisms to create self communication loops.<br>An organism's input avatars can receive messages from it's own output avatars. |

## BUY_SELL_GROUP

Buying and Selling Parameters.

| Setting | Type | Default | Description |
|---|---|---|---|
| `SAVE_RECEIVED` | bool | `0` | Enable storage of all inputs bought from other orgs |

## HOARD_RESOURCE_GROUP

Resource Hoarding Parameters.

| Setting | Type | Default | Description |
|---|---|---|---|
| `USE_RESOURCE_BINS` | bool | `0` | Enable resource bin use.  This serves as a guard on most resource hoarding code. |
| `ABSORB_RESOURCE_FRACTION` | double | `.0025` | Fraction of available environmental resource an organism absorbs. |
| `MULTI_ABSORB_TYPE` | int | `0` | What to do if a collect instruction is called on a range of resources.<br>0 = absorb a random resource in the range<br>1 = absorb the first resource in the range<br>2 = absorb the last resource in the range<br>3 = absorb ABSORB_RESOURCE_FRACTION / (# of resources in range) of each resource in the range |
| `MAX_TOTAL_STORED` | double | `-1` | Maximum total amount of all resources an organism can store.<br>&lt;0 = no maximum |
| `USE_STORED_FRACTION` | double | `1.0` | The fraction of stored resource to use. |
| `ENV_FRACTION_THRESHOLD` | double | `1.0` | The fraction of available environmental resource to compare available stored resource to when deciding whether to use stored resource. |
| `RETURN_STORED_ON_DEATH` | bool | `1` | Return an organism's stored resources to the world when it dies? |
| `SPLIT_ON_DIVIDE` | bool | `1` | Split mother cell's resources between two daughter cells on division? |
| `COLLECT_SPECIFIC_RESOURCE` | int | `0` | Resource to be collected by the "collect-specific" instruction. |
| `NON_1_RESOURCE_RATIOS` | string | `1:1` | Resources to be collected by the "collect-specific-ratio" instruction in a non 1:1 ratio. Specify as 'resourceID1:ratio1, resouceID2:ratio2' etc |
| `COLLECT_AMOUNT` | double | `1` | The amount to collect for collect-specific, or for a resource with a ratio of 1 in collect-specific-ratio |
| `RESOURCE_GIVEN_ON_INJECT` | double | `0.0` | Units of collect-specific resources given on inject. |
| `RESOURCE_GIVEN_AT_BIRTH` | double | `0.0` | Units of collect-specific resources given to offspring upon birth (will be added to SPLIT_ON_DIVIDE amount for collect-specific resource if both enabled. |
| `COLLECT_PROB_DIVISOR` | int | `1` | Divisor for probabilistic collect instructions. |

## ANALYZE_GROUP

Analysis Settings.

| Setting | Type | Default | Description |
|---|---|---|---|
| `MAX_CONCURRENCY` | int | `-1` | Maximum number of analyze threads, -1 == use all available. |
| `INJECT_RESETS_TASKS` | int | `0` | Executing INJECT (semi-succesfully) will trigger last_task_count to be writen from current_task_count |
| `ANALYZE_OPTION_1` | string | (empty) | String variable accessible from analysis scripts |
| `ANALYZE_OPTION_2` | string | (empty) | String variable accessible from analysis scripts |

## ENERGY_GROUP

Energy Settings.

| Setting | Type | Default | Description |
|---|---|---|---|
| `ENERGY_ENABLED` | bool | `0` | Enable Energy Model. 0/1 (off/on) |
| `ENERGY_GIVEN_ON_INJECT` | double | `0.0` | Energy given to organism upon injection. |
| `ENERGY_GIVEN_AT_BIRTH` | double | `0.0` | Energy given to offspring upon birth. |
| `FRAC_PARENT_ENERGY_GIVEN_TO_ORG_AT_BIRTH` | double | `0.5` | Fraction of parent's energy given to offspring organism. |
| `FRAC_PARENT_ENERGY_GIVEN_TO_DEME_AT_BIRTH` | double | `0.5` | Fraction of parent's energy given to offspring deme. |
| `FRAC_ENERGY_DECAY_AT_ORG_BIRTH` | double | `0.0` | Fraction of energy lost due to decay during organism reproduction. |
| `FRAC_ENERGY_DECAY_AT_DEME_BIRTH` | double | `0.0` | Fraction of energy lost due to decay during deme reproduction. |
| `NUM_CYCLES_EXC_BEFORE_0_ENERGY` | int | `0` | Number of virtual CPU cycles executed before energy is exhausted. |
| `ENERGY_CAP` | double | `-1.0` | Maximum amount of energy that can be stored in an organism.  -1 = no max |
| `APPLY_ENERGY_METHOD` | int | `0` | When should rewarded energy be applied to current energy?<br>0 = on divide<br>1 = on completion of task<br>2 = on sleep |
| `FIX_METABOLIC_RATE` | double | `-1.0` | Fix organism metobolic rate to value.  This value is static.  Feature disabled by default (value == -1) |
| `FRAC_ENERGY_TRANSFER` | double | `0.0` | Fraction of replaced organism's energy take by new resident |
| `LOG_SLEEP_TIMES` | bool | `0` | Log sleep start and end times. 0/1 (off/on)<br>WARNING: may use lots of memory. |
| `FRAC_ENERGY_RELINQUISH` | double | `1.0` | Fraction of organisms energy to relinquish |
| `ENERGY_PASSED_ON_DEME_REPLICATION_METHOD` | int | `0` | Who get energy passed from a parent deme<br>0 = Energy divided among organisms injected to offspring deme<br>1 = Energy divided among cells in offspring deme |
| `INHERIT_EXE_RATE` | int | `0` | Inherit energy rate from parent? 0=no  1=yes |
| `ATTACK_DECAY_RATE` | double | `0.0` | Percent of cell's energy decayed by attack |
| `ENERGY_THRESH_LOW` | double | `.33` | Threshold percent below which energy level is considered low.  Requires ENERGY_CAP. |
| `ENERGY_THRESH_HIGH` | double | `.75` | Threshold percent above which energy level is considered high.  Requires ENERGY_CAP. |
| `ENERGY_COMPARISON_EPSILON` | double | `0.0` | Percent difference (relative to executing organism) required in energy level comparisons |
| `ENERGY_REQUEST_RADIUS` | int | `1` | Radius of broadcast energy request messages. |

## ENERGY_SHARING_GROUP

Energy Sharing Settings.

| Setting | Type | Default | Description |
|---|---|---|---|
| `ENERGY_SHARING_METHOD` | int | `0` | Method for sharing energy.  0=receiver must actively receive/request, 1=energy pushed on receiver |
| `ENERGY_SHARING_PCT` | double | `0.0` | Percent of energy to share |
| `ENERGY_SHARING_INCREMENT` | double | `0.01` | Amount to change percent energy shared |
| `RESOURCE_SHARING_LOSS` | double | `0.0` | Fraction of shared resource lost in transfer |
| `ENERGY_SHARING_UPDATE_METABOLIC` | bool | `0` | 0/1 (off/on) - Whether to update an organism's metabolic rate on donate or reception/application of energy |

## SECOND_PASS_GROUP

Tracking metrics known after the running experiment previously.

| Setting | Type | Default | Description |
|---|---|---|---|
| `TRACK_CCLADES` | int | `0` | Enable tracking of coalescence clades |
| `TRACK_CCLADES_IDS` | string | `coalescence.ids` | File storing coalescence IDs |

## GX_GROUP

Gene Expression CPU Settings.

| Setting | Type | Default | Description |
|---|---|---|---|
| `MAX_PROGRAMIDS` | int | `16` | Maximum number of programids an organism can create. |
| `MAX_PROGRAMID_AGE` | int | `2000` | Max number of CPU cycles a programid executes before it is removed. |
| `IMPLICIT_GENE_EXPRESSION` | int | `0` | Create executable programids from the genome without explicit allocation and copying? |
| `IMPLICIT_BG_PROMOTER_RATE` | double | `0.0` | Relative rate of non-promoter sites creating programids. |
| `IMPLICIT_TURNOVER_RATE` | double | `0.0` | Number of programids recycled per CPU cycle. 0 = OFF |
| `IMPLICIT_MAX_PROGRAMID_LENGTH` | int | `0` | Creation of an executable programid terminates after this many instructions. 0 = disabled |

## PROMOTER_GROUP

Promoters.

| Setting | Type | Default | Description |
|---|---|---|---|
| `PROMOTERS_ENABLED` | int | `0` | Use the promoter/terminator execution scheme.<br>Certain instructions must also be included. |
| `PROMOTER_INST_MAX` | int | `0` | Maximum number of instructions to execute before terminating. 0 = off |
| `PROMOTER_PROCESSIVITY` | double | `1.0` | Chance of not terminating after each cpu cycle. |
| `PROMOTER_PROCESSIVITY_INST` | double | `1.0` | Chance of not terminating after each instruction. |
| `PROMOTER_TO_REGISTER` | int | `0` | Place a promoter's base bit code in register BX when starting execution from it? |
| `TERMINATION_RESETS` | int | `0` | Does termination reset the thread's state? |
| `NO_ACTIVE_PROMOTER_EFFECT` | int | `0` | What happens when there are no active promoters?<br>0 = Start execution at the beginning of the genome.<br>1 = Kill the organism.<br>2 = Stop the organism from executing any further instructions. |
| `PROMOTER_CODE_SIZE` | int | `24` | Size of a promoter code in bits. (Maximum value is 32) |
| `PROMOTER_EXE_LENGTH` | int | `3` | Length of promoter windows used to determine execution. |
| `PROMOTER_EXE_THRESHOLD` | int | `2` | Minimum number of bits that must be set in a promoter window to allow execution. |
| `INST_CODE_LENGTH` | int | `3` | Instruction binary code length (number of bits) |
| `INST_CODE_DEFAULT_TYPE` | int | `0` | Default value of instruction binary code value.<br>0 = All zeros<br>1 = Based off the instruction number |
| `CONSTITUTIVE_REGULATION` | int | `0` | Sense a new regulation value before each CPU cycle? |

## COLORS_GROUP

Output colors for when data files are printed in HTML mode. There are two sets of these; the first are for lineages, and the second are for mutation tests.

| Setting | Type | Default | Description |
|---|---|---|---|
| `COLOR_DIFF` | string | `CCCCFF` | Color to flag stat that has changed since parent. |
| `COLOR_SAME` | string | `FFFFFF` | Color to flag stat that has NOT changed since parent. |
| `COLOR_NEG2` | string | `FF0000` | Color to flag stat that is significantly worse than parent. |
| `COLOR_NEG1` | string | `FFCCCC` | Color to flag stat that is minorly worse than parent. |
| `COLOR_POS1` | string | `CCFFCC` | Color to flag stat that is minorly better than parent. |
| `COLOR_POS2` | string | `00FF00` | Color to flag stat that is significantly better than parent. |
| `COLOR_MUT_POS` | string | `00FF00` | Color to flag stat that has changed since parent. |
| `COLOR_MUT_NEUT` | string | `FFFFFF` | Color to flag stat that has changed since parent. |
| `COLOR_MUT_NEG` | string | `FFFF00` | Color to flag stat that has changed since parent. |
| `COLOR_MUT_LETHAL` | string | `FF0000` | Color to flag stat that has changed since parent. |

## MOVEMENT_GROUP

Movement Features Settings.

| Setting | Type | Default | Description |
|---|---|---|---|
| `MOVEMENT_COLLISIONS_LETHAL` | int | `0` | Are collisions during movement lethal (not applied to avatars)? (0=no, use swap; 1=yes, use collision selection type; 2=no, but movement fails) |
| `MOVEMENT_COLLISIONS_SELECTION_TYPE` | int | `0` | 0 = 50% chance<br>1 = binned vitality based |
| `VITALITY_BIN_EXTREMES` | double | `1.0` | vitality multiplier for extremes (> 1 stddev from the mean population age) |
| `VITALITY_BIN_CENTER` | double | `10.0` | vitality multiplier for center bin (with 1 stddev of the mean population age) |
| `DEADLY_BOUNDARIES` | int | `0` | Are bounded grid border cell deadly? If == 1, orgs stepping onto boundary cells will disappear into oblivion (aka die) |
| `STEP_COUNTING_ERROR` | int | `0` | % chance a step is not counted as part of easterly/northerly travel. |
| `USE_AVATARS` | int | `0` | Set orgs to move & navigate in solo avatar worlds(1=yes, 2=yes, with org interactions). |
| `AVATAR_BIRTH` | int | `0` | 0 = Same as parent<br>1 = Random<br>2 = Cell faced by parent avatar<br>3 = next grid cell<br>4 = Center of the world |
| `AVATAR_BIRTH_FACING` | int | `0` | 0 North<br>1 Random |
| `TRACK_BIRTH_LOCS` | int | `0` | Log and print locations for all births place. |
| `LOOK_DIST` | int | `-1` | -1: use limits set inside look instructions<br>>-1: limit sight distance of look instructions to this number of cells |
| `LOOK_DISABLE` | int | `0` | 0: none<br>1: input habitat register<br>2: input sight dist sought<br>3: input type of search (e.g. closest vs count vs total)<br>4: input resource/org id sought<br>5: output habitat used<br>6: output distance used<br>7: output search type used<br>8: output resource/org id used<br>9: output count (edible)<br>10: outptu amount/value seen<br>11: output id seen<br>12: output org forage target seen |
| `LOOK_DISABLE_TYPE` | int | `0` | 0: predators<br>1: prey<br>2: both predators and prey |
| `LOOK_DISABLE_COMBO` | int | `0` | # 0: none<br># 1: return 'not found' for any food resource query<br># 2: return 'not found' for any looking-for-predator query<br># 3: return 'not found' for any looking-for-prey query |
| `PRED_CONFUSION` | int | `0` | # If 1, pred will get random data returned to registers when seen prey with odds of 0.1 * n, where n = # neighbors that that prey has.<br># If 2, pred will get random data returned to registers with odds of n/8, where n = # facings among neighbors that that prey has.<br># If 3, will be number of opinions seen / number of opinions in restricted list.<br># If 4, look executions fail for predators. |
| `TRACK_LOOK_SETTINGS` | int | `0` | # track (final) settings for look sensor use |
| `TRACK_LOOK_OUTPUT` | int | `0` | # track (final) output from sensor use |
| `USE_DISPLAY` | int | `0` | If 1, org display data is always 'on' (visible). If 2, org display is on and sensor does not set potential data. |
| `USE_MIMICS` | int | `0` | If 1, org's with forage target of 1 can show a deceptive ft number (as seen by other orgs via sensor) |
| `MIMIC_ODDS` | double | `1.0` | Odds that a mimic will appear to other organisms as the thing it is mimicing. |
| `SET_FT_AT_BIRTH` | int | `0` | Should offspring set forage target at birth? 0: No 1: Yes |

## PHEROMONE_GROUP

Pheromone Settings.

| Setting | Type | Default | Description |
|---|---|---|---|
| `PHEROMONE_ENABLED` | bool | `0` | Enable pheromone usage. 0/1 (off/on) |
| `PHEROMONE_AMOUNT` | double | `1.0` | Amount of pheromone to add per drop |
| `PHEROMONE_DROP_MODE` | int | `0` | Where to drop pheromone<br>0 = Half amount at src, half at dest<br>1 = All at source<br>2 = All at dest |
| `EXPLOIT_EXPLORE_PROB` | double | `0.00` | Probability of random exploration<br>instead of pheromone trail following |
| `EXPLOIT_LOG_START` | int | `0` | Update at which to start logging exploit moves |
| `EXPLORE_LOG_START` | int | `0` | Update at which to start logging explore moves |
| `LOG_INJECT` | bool | `0` | Log injection of organisms.  0/1 (off/on) |
| `INJECT_LOG_START` | int | `0` | Update at which to start logging injection of<br>organisms |

## SYNCHRONIZATION_GROUP

Synchronization settings.

| Setting | Type | Default | Description |
|---|---|---|---|
| `SYNC_FITNESS_WINDOW` | int | `100` | Number of updates over which to calculate fitness (default=100). |
| `SYNC_FLASH_LOSSRATE` | double | `0.0` | P() to lose a flash send (0.0==off). |
| `SYNC_TEST_FLASH_ARRIVAL` | int | `-1` | CPU cycle at which an organism will receive a flash (off=-1, default=-1, analyze mode only.) |

## CONSENSUS_GROUP

Consensus settings.

| Setting | Type | Default | Description |
|---|---|---|---|
| `CONSENSUS_HOLD_TIME` | int | `1` | Number of updates that consensus must be held for. |

## REPUTATION_GROUP

Reputation Settings.

| Setting | Type | Default | Description |
|---|---|---|---|
| `RAW_MATERIAL_AMOUNT` | int | `100` | Number of raw materials an organism starts with |
| `AUTO_REPUTATION` | int | `0` | Is an organism's reputation automatically computed based on its donations<br>0=no<br>1=increment for each donation + standing<br>2=+1 for donations given -1 for donations received<br>3=1 for donors -1 for recivers who have not donated<br>4=+1 for donors<br>5=+1 for donors during task check |
| `ALT_BENEFIT` | int | `1.00` | Number multiplied by the number of raw materials received from another organism to compute reward |
| `ALT_COST` | int | `1.00` | Number multiplied by the number of your raw materials |
| `ROTATE_ON_DONATE` | int | `0` | Rotate an organism to face its donor 0/1 (off/on) |
| `REPUTATION_REWARD` | int | `0` | Reward an organism for having a good reputation |
| `DONATION_FAILURE_PERCENT` | int | `0` | Percentage of times that a donation fails |
| `RANDOMIZE_RAW_MATERIAL_AMOUNT` | int | `0` | Should all the organisms receive the same amount 0/1 (off/on) |
| `DONATION_RESTRICTIONS` | int | `0` | 0=none<br>1=inter-species only<br>2=different tag only |
| `INHERIT_REPUTATION` | int | `0` | 0=reputations are not inherited<br>1=reputations are inherited<br>2=tags are inherited |
| `SPECIALISTS` | int | `0` | 0=generalists allowed<br>1=only specialists |
| `STRING_AMOUNT_CAP` | int | `-1` | -1=no cap on string amounts<br>#=CAP |
| `MATCH_ALREADY_PRODUCED` | int | `0` | 0=off<br>1=on |

## GROUP_FORMATION_GROUP

Group Formation Settings.

| Setting | Type | Default | Description |
|---|---|---|---|
| `USE_FORM_GROUPS` | int | `0` | Enable organisms to form groups. 0=off,<br>1=on no restrict,<br>2=on restrict to defined |
| `DEFAULT_GROUP` | int | `-1` | Default group to assign to organisms not asserting a group membership (-1 indicates disabled) |
| `INHERIT_OPINION` | int | `1` | Should offspring inherit the parent's opinion? 0: No... reset on divide, etc. 1: Yes |
| `OPINION_BUFFER_SIZE` | int | `1` | Size of the opinion buffer (stores opinions set over the organism's lifetime); -1=inf, default=1, cannot be 0. |
| `JOIN_GROUP_FAILURE` | int | `0` | Percent chance for failing to switch groups. If negative, is % chance of death. |
| `TOLERANCE_WINDOW` | int | `0` | Window of previous updates used to evaluate org's tolerance levels<br>(0 indicates tolarance disabled, values &lt;0 indicate % chance random migration for offspring) |
| `MAX_TOLERANCE` | int | `1` | Maximum tolerance level |
| `TOLERANCE_VARIATIONS` | int | `0` | 0=all tolerance active,<br>1=only immigration tolerance active,<br>2=immigrants + sex |
| `TRACK_TOLERANCE` | int | `0` | Turn on/off detailed recording of tolerance change circumstances (Warning: can be slow) |
| `PRED_PREY_SWITCH` | int | `-1` | -2: no predators, but track prey stats<br>-1: no predators in experiment<br>0: don't allow a predator to switch to being a prey (prey to pred always allowed)<br>1: allow predators to switch to being prey<br>2: don't allow a predator to switch to being a prey & don't allow prey to switch via set-forage-target (via attack allowed) ) |
| `PRED_EFFICIENCY` | double | `1.0` | Multiply the current bonus, merit, and resource bin amounts of the consumed prey by this value<br>and add to current predator values (for bonus, merit, and bin consumption instructions). |
| `PRED_EFFICIENCY_POISON` | double | `1.0` | Multiply the current bonus, merit, and resource bin amounts of the consumed prey by this value<br>and subtract from current predator values if this prey is poisonous (ft == 2). |
| `PRED_ODDS` | double | `1.0` | Probability of success for predator 'attack' instructions. |
| `PRED_INJURY` | double | `0.0` | If an attack fails, target's bonus, merit, and internal resources are reduced by this fraction. |
| `MIN_PREY` | int | `0` | If positive (recommended for prey studies), predator attacks fail if num prey falls below this (0 = off).<br>If negative (recommended for predator studies), random prey of genotype other than target will be cloned (using birth placement methods). |
| `MAX_PRED` | int | `0` | Population cap on number of predators (random predator will be removed when cap is exceeded). |
| `MAX_PREY` | int | `0` | Population cap on number of prey (random prey will be removed when cap is exceeded). For births, classification as prey is based on parent. |
| `MAX_PREY_BT` | int | `0` | Population cap on number of prey (random prey will be removed when cap is exceeded) based on FT. |
| `TRACK_GROUP_ATTACK_DETAILS` | int | `0` | Track details around execution of EVERY group attack instructions for every update.<br>1 = as string in one file.<br>2 = as bits in new file for every update that this is on! |
| `MARKING_EXPIRE_DATE` | int | `-1` | Number of updates markings in cells will remain effective on territory move. |
| `PREY_MUT_OFF` | int | `0` | Turn off prey mutations in the exp hardware repro inst. |

## DEME_NETWORK_GROUP

Deme network settings.

| Setting | Type | Default | Description |
|---|---|---|---|
| `DEME_NETWORK_TYPE` | int | `0` | 0=topology, structure of network determines fitness. |
| `DEME_NETWORK_REQUIRES_CONNECTEDNESS` | int | `1` | Whether the deme's network must be connected before an actual fitness is calculated. |
| `DEME_NETWORK_TOPOLOGY_FITNESS` | int | `0` | Network measure used to determine fitness; see cDemeTopologyNetwork.h. |
| `DEME_NETWORK_LINK_DECAY` | int | `0` | Number of updates after which a link decays; 0=no decay (default). |
| `DEME_NETWORK_REMOVE_NODE_ON_DEATH` | int | `0` | Whether death of an organism in<br>the deme removes its links;<br>0=no (default);<br>1=yes. |

## HGT_GROUP

Horizontal gene transfer settings.

| Setting | Type | Default | Description |
|---|---|---|---|
| `ENABLE_HGT` | int | `0` | Whether HGT is enabled; 0=false (default),<br>1=true. |
| `HGT_SOURCE` | int | `0` | Source of HGT fragments; 0=dead organisms (default),<br>1=parent. |
| `HGT_FRAGMENT_SELECTION` | int | `0` | Method used to select fragments for HGT mutation; 0=random (default),<br>1=trimmed selection<br>2=random placement. |
| `HGT_FRAGMENT_SIZE_MEAN` | double | `10` | Mean size of fragments (default=10). |
| `HGT_FRAGMENT_SIZE_VARIANCE` | double | `2` | Variance of fragments (default=2). |
| `HGT_MAX_FRAGMENTS_PER_CELL` | int | `100` | Max. allowed number of fragments per cell (default=100). |
| `HGT_DIFFUSION_METHOD` | int | `0` | Method to use for diffusion of genome fragments; 0=none (default). |
| `HGT_COMPETENCE_P` | double | `0.0` | Probability that an HGT 'natural competence' mutation will occur on divide (default=0.0). |
| `HGT_INSERTION_MUT_P` | double | `0.0` | Probability that an HGT mutation will result in an insertion (default=0.0). |
| `HGT_CONJUGATION_METHOD` | int | `0` | Method used to select the receiver and/or donor of an HGT conjugation;<br>0=random from neighborhood (default);<br>1=faced. |
| `HGT_CONJUGATION_P` | double | `0.0` | Probability that an HGT conjugation mutation will occur on divide (default=0.0). |
| `HGT_FRAGMENT_XFORM` | int | `0` | Transformation to apply to each fragment prior to incorporation into offspring's genome; 0=none (default),<br>1=random shuffle,<br>2=replace with random instructions. |

## INST_RES_GROUP

Resource-Dependent Instructions Settings.

| Setting | Type | Default | Description |
|---|---|---|---|
| `INST_RES` | string | (empty) | Resource upon which the execution of certain instruction depends |
| `INST_RES_FLOOR` | double | `0.0` | Assumed lower level of resource in environment.  Used for probability dist. |
| `INST_RES_CEIL` | double | `0.0` | Assumed upper level of resource in environment.  Used for probability dist. |

## ALARM_GROUP

Alarm Settings.

| Setting | Type | Default | Description |
|---|---|---|---|
| `BCAST_HOPS` | int | `1` | Number of hops to broadcast an alarm |
| `ALARM_SELF` | bool | `0` | Does sending an alarm move sender IP to alarm label?<br>0=no<br>1=yes |

## DIVISION_OF_LABOR_GROUP

Division of Labor settings.

| Setting | Type | Default | Description |
|---|---|---|---|
| `AGE_POLY_TRACKING` | bool | `0` | Print data for an age-task histogram |
| `REACTION_THRESH` | int | `0` | The number of times the deme must perform each reaction in order to replicate |
| `TASK_SWITCH_PENALTY` | int | `0` | Cost of task switching in cycles |
| `TASK_SWITCH_PENALTY_TYPE` | int | `0` | Type of task switch cost: (0) none (1) learning, (2) retooling or context, (3) centrifuge |
| `RES_FOR_DEME_REP` | int | `0` | The amount of resources that must be consumed prior to automatic deme replication |
| `LEARNING_COUNT` | int | `0` | The number of times a task must be performed to avoid efficiency penalties |

## DEPRECATED_GROUP

DEPRECATED (New functionality listed in comments).

| Setting | Type | Default | Description |
|---|---|---|---|
| `ANALYZE_MODE` | int | `0` | 0 = Disabled<br>1 = Enabled<br>2 = Interactive<br>DEPRECATED: use command line options -a[nalyze] or -i[nteractive]) |
| `REPRO_METHOD` | int | `1` | Replace existing organism: 1=yes<br>DEPRECATED: Use BIRTH_METHOD 3 instead. |
| `LEGACY_GRID_LOCAL_SELECTION` | bool | `0` | Enable legacy grid local mate selection.<br>DEPRECATED: Birth chameber now uses population structure) |
| `HARDWARE_TYPE` | int | `0` | 0 = Default, heads-based CPUs<br>1 = New SMT CPUs<br>2 = Transitional SMT<br>3 = Experimental CPU<br>4 = Multi-threaded Behavioral CPU |
| `INST_SET` | string | `-` | Instruction set file ('-' = use default for hardware type) |
| `INST_SET_LOAD_LEGACY` | int | `0` | Load legacy format instruction set file format |
