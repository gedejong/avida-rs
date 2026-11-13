# Analyze mode

Analyze mode is for studying a finished run. Instead of simulating a population, `avida` loads genotypes from a saved population file into memory, measures them again in a test CPU (an isolated virtual CPU that runs one genome at a time), and writes tables, lineages and landscapes. It is a small scripting language: a script is a list of commands, one per line.

## Running it

```sh
avida -a                  # run the script named by ANALYZE_FILE (stock: analyze.cfg)
avida -a -set ANALYZE_FILE my-analysis.cfg -set DATA_DIR out
avida -i                  # interactive: type commands, see the effect at once
```

Run it in an experiment folder, so that the configuration, the environment and the instruction set are the ones the population was evolved under; the test CPU uses them. Results go to `DATA_DIR`, except where a command names a different place. The stock `analyze.cfg` contains only comments.

Analyze mode uses the cores of the machine for the heavy commands (landscapes and others). `MAX_CONCURRENCY` limits the number of threads (stock -1: all of them). The `ANALYZE_OPTION_1` and `ANALYZE_OPTION_2` settings are strings your script can read.

## A first script

This script loads the population saved at update 1000 in the [first run](../getting-started/first-run.md), measures it again, and writes a table with three columns for each genotype:

```text
LOAD data/detail-1000.spop
RECALCULATE
DETAIL genotypes.dat length gest_time fitness
```

Run it as `avida -a -set ANALYZE_FILE my-analysis.cfg -set DATA_DIR out`. You should see the progress lines `Loading: ...`, `Running through test CPUs...` and `Detailing...`, and a file `out/genotypes.dat` with a header and one row per genotype.

## Batches

Loaded genotypes sit in a *batch*. There are many batches (the old documentation said up to 2,000), and every command works on the *current* batch, which is 0 at the start. `SET_BATCH n` switches the current batch, `NAME_BATCH` gives it a name, `PURGE_BATCH` empties it, `DUPLICATE a b` copies one batch into another, and `STATUS` prints the number of genotypes in each batch:

```text
Status Report:
  Batch 0 -- 1 genotypes.  <current>
```

## The line of descent of a final population

This is the script that the [line-of-descent tutorial](../guides/tutorial-line-of-descent.md) uses. It was run on the population saved at update 1000 of the first run (23 genotypes on the line, about a third of a second on a laptop):

```text
LOAD data/detail-1000.spop
FIND_LINEAGE num_cpus
RECALCULATE
PrecalcLandscape
DETAIL lineage.dat depth update_born length fitness frac_dead frac_neg frac_neut frac_pos task.0 task.1
```

- `FIND_LINEAGE num_cpus` keeps only the line of descent: it picks the genotype with the most organisms (`num_cpus`) and follows its parents back to the ancestor.
- `RECALCULATE` measures every genotype again in the test CPU.
- `PrecalcLandscape` (an [action](actions.md)) tests every one-point mutation of every genotype, and so is the expensive step. It is what fills `frac_dead`, `frac_neg`, `frac_neut` and `frac_pos`: the fractions of one-step mutations that are lethal, harmful, neutral and beneficial.
- `DETAIL` writes the columns you list, one row per genotype.

## Data columns for `DETAIL` and `FILTER`

The column names (also used by `FILTER`, `FIND_GENOTYPE` and others) are:

`name`, `viable`, `id`, `hw_type`, `inst_set`, `src`, `src_args`, `tag`, `parents`, `parent_id`, `parent2_id`, `parent_dist`, `ancestor_dist`, `lineage`, `num_cpus`, `total_cpus`, `num_units`, `total_units`, `length`, `copy_length`, `exe_length`, `merit`, `comp_merit`, `comp_merit_ratio`, `gest_time`, `efficiency`, `efficiency_ratio`, `fitness`, `div_type`, `mate_id`, `fitness_ratio`, `update_born`, `gen_born`, `update_dead`, `update_deactivated`, `depth`, `cells`, `gest_offset`, `frac_dead`, `frac_neg`, `frac_neut`, `frac_pos`, `complexity`, `land_fitness`, `mating_type`, `mate_preference`, `mating_display_a`, `mating_display_b`, `num_phen`, `num_trials`, `phen_entropy`, `phen_max_fitness`, `phen_max_fit_freq`, `phen_min_fitness`, `phen_min_freq`, `phen_avg_fitness`, `phen_likely_freq`, `phen_likely_fitness`, `prob_viable`, `parent_muts`, `task_order`, `sequence`, `alignment`, `executed_flags`, `alignment_executed_flags`, `task_list`, `link`, `html`, `total_task_count`, `total_task_performance_count`, `update`, `dom_num_cpus`, `dom_depth`, `dom_id`, `dom_sequence`.

In addition, `task.0`, `task.1` and so on give how often the genotype performed each task (numbered in the order of the reactions in `environment.cfg`), and a column name can be prefixed with `html.` for HTML output. Their headers print the task's name, as in the legend `# 5: Not`.

## Variables, loops and functions

Variables are set with `SET name value` and read with `$name`:

```text
SET d data
LOAD $d/detail-1000.spop
```

`FORRANGE i 1 3 ... END` repeats its body for `i` from 1 to 3 (an optional fourth argument is the step); `FOREACH` is a further loop command (see the original documentation for its form); `FUNCTION NAME ... END` defines a command of your own, whose arguments are `$1`, `$2` and so on. This script, which was run for this page, defines a command and a loop:

```text
SET d data
FUNCTION SHOW   # arg1 = the directory
  PURGE_BATCH
  LOAD $1/detail-0.spop
  RECALCULATE
  DETAIL show_$1.dat length fitness
END
FORRANGE i 1 2
  ECHO round $i
END
SHOW $d
```

It printed `::: round 1` and `::: round 2` and wrote `show_data.dat`. (`ECHO` prefixes its output with `:::`.) `INCLUDE file` runs another file at that point, so you can keep your own functions in one file.

## The commands

The command registry has 100 names. They are not case-sensitive in practice, but the convention is capitals. The ones used most:

| Command | Purpose |
|---|---|
| `LOAD file` | Load a population file (a `.spop` with a `#format` header). |
| `LOAD_ORGANISM file` | Load a single organism file (`.org`). |
| `LOAD_SEQUENCE sequence` | Load a genome written as the letters of its instruction set. |
| `LOAD_RESOURCES [file]` | Load resource levels over time, for tests that use resources. |
| `RECALCULATE`, `RECALC` | Run every genotype of the batch through the test CPU and record its measurements. `RECALC` takes its arguments in any order and can run several trials for plasticity. |
| `FILTER stat relation value` | Remove genotypes that fail a test, for example `FILTER fitness > 0.25`. On the population of the first run at update 1000 this reduced a batch from 8,433 to 5,253 genotypes. |
| `FIND_GENOTYPE`, `FIND_ORGANISM`, `FIND_LINEAGE`, `FIND_CLADE`, `FIND_LAST_COMMON_ANCESTOR` | Keep only some genotypes: the most abundant, a random one, a line of descent, a clade. |
| `SAMPLE_ORGANISMS`, `SAMPLE_GENOTYPES`, `KEEP_TOP`, `TRUNCATE_LINEAGE` | Reduce a batch. |
| `DETAIL file columns...` | Write a table with one row per genotype. |
| `DETAIL_AVERAGE`, `DETAIL_TIMELINE`, `DETAIL_BATCHES`, `DETAIL_INDEX` | Averages, tables over time and across batches. |
| `PRINT [dir] [file]` | Write each genotype as an organism file (default folder `archive/`). |
| `TRACE` | Write an execution trace of each genotype. |
| `PRINT_TASKS`, `PRINT_TASKS_QUALITY`, `PRINT_PHENOTYPES`, `PRINT_DIVERSITY`, `HISTOGRAM` | Task and diversity summaries. |
| `MAP_TASKS` (`MAP`), `MAP_MUTATIONS`, `MAP_DEPTH` | Maps of tasks along a genome, of mutations, and of tree depth. |
| `ANALYZE_KNOCKOUTS`, `ANALYZE_COMPLEXITY`, `ANALYZE_LINEAGE_COMPLEXITY`, `ANALYZE_POP_COMPLEXITY`, `ANALYZE_FITNESS_TWO_SITES`, `ANALYZE_COMPLEXITY_TWO_SITES` | Knockout and complexity analyses. |
| `MUTAGENIZE`, `RECOMBINE`, `RECOMBINE_SAMPLE` | Make mutants or recombinants of the batch. |
| `HAMMING`, `LEVENSTEIN`, `PRINT_DISTANCES`, `SPECIES`, `ALIGN` | Genome distances and alignment. |
| `PRINT_TREE_STATS`, `PRINT_CUMULATIVE_STEMMINESS`, `PRINT_GAMMA` | Statistics of the tree of genotypes. |
| `WRITE_CLONE`, `WRITE_INJECT_EVENTS`, `WRITE_INJECT_INITIAL`, `WRITE_COMPETITION`, `COMPETE` | Write the batch as a clone file or as injection events for a new run, and compete batches. |
| `SET`, `CONFIG_GET`, `CONFIG_SET`, `ENVIRONMENT` | Variables and settings. |
| `SET_BATCH`, `NAME_BATCH`, `TAG_BATCH`, `PURGE_BATCH`, `DUPLICATE`, `RENAME`, `STATUS` | Batch control. |
| `FUNCTION`, `FOREACH`, `FORRANGE`, `INCLUDE` (also `RUN`), `SYSTEM`, `INTERACTIVE`, `ECHO` (also `DEBUG`), `TEST`, `CLOSE_FILE` | Control flow and utilities. `SYSTEM` runs a shell command. |

The rest of the registry: `ANALYZE_REDUNDANCY_BY_INST_FAILURE`, `ANALYZE_NEWINFO`, `ANALYZE_MUTS`, `ANALYZE_INSTRUCTIONS`, `ANALYZE_INST_POP`, `ANALYZE_MUTATION_TRACEBACK`, `ANALYZE_MATE_SELECTION`, `ANALYZE_COMPLEXITY_DELTA`, `AVERAGE_MODULARITY`, `CALC_FUNCTIONAL_MODULARITY`, `CALC_SKELETON_DIVERSITY`, `CHANGE_DEBUG`, `COMMUNITY_COMPLEXITY`, `COMPLEXITY_DEBUG`, `COUNT_NEW_SIG_LINEAGES`, `COUNT_NOVEL_SKELETONS`, `FIND_SEX_LINEAGE`, `GET_LARGEST_SKELETON`, `GET_SKELETONS`, `ALL_COMPLEXITY_BARRIERS`, `MUTATION_REVERT`, `PRINT_RESOURCE_FITNESS_MAP`, `SAMPLE_OFFSPRING`, `TRUNCATELINEAGE`.

The names of the commands were checked against the registry in `crates/analyze/src/analyze.rs`. The descriptions come from the program's old documentation, and these commands were also run for this page: `LOAD`, `LOAD_SEQUENCE`, `RECALCULATE`, `FILTER`, `FIND_GENOTYPE`, `FIND_LINEAGE`, `DETAIL`, `PRINT`, `STATUS`, `ECHO`, `SET`, `FUNCTION`, `FORRANGE` and the action `PrecalcLandscape`. For the others, try the command on a small batch before you rely on it.

## Every action works too

All [actions](actions.md) can be called inside an analyze script (the landscape actions, such as `PrecalcLandscape`, `AnalyzeLandscape`, `FullLandscape` and `RandomLandscape`, are the usual ones).

## Not supported

Analyze mode sets up the whole population as a normal run does, so demes, gradient resources and gene transfer set-ups work. What still stops are set-ups on which the reference binary itself crashes: `NUM_DEMES` that does not divide `WORLD_Y`, an unknown `SLICING_METHOD`, two `hgt=1` resources, `ENABLE_HGT` without an `hgt=1` resource, and a gradient halo narrower than twice its height. The ASL scripting language (`crates/script`) is parsed and checked, but its `Config`, `World` and `Driver` objects are stubs that fail when called. See [unsupported features](unsupported.md).
