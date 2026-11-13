# Reproduction, birth and sex

Reproduction in Avida is something an organism does with its own instructions: it allocates memory, copies its genome into it one instruction at a time, and executes a divide instruction. This page covers the rules that surround that act: when a divide is accepted, where the offspring goes, who dies, and how sexual reproduction works.

## When a divide succeeds

The divide instruction (`h-divide` in the stock set) only succeeds if the organism meets conditions that the DIVIDE group of `avida.cfg` sets. The ones most often changed:

| Setting | Stock value | Condition |
|---|---|---|
| `MIN_COPIED_LINES` | 0.5 | At least this fraction of the genome must have been copied. |
| `MIN_EXE_LINES` | 0.5 | At least this fraction must have been executed. |
| `OFFSPRING_SIZE_RANGE` | 2.0 | The offspring may be at most this factor longer, or shorter, than the parent. |
| `REQUIRE_ALLOCATE` | 1 | The organism must have allocated memory first (original CPU only). |
| `REQUIRE_SINGLE_REACTION` | 0 | If 1 (or n), at least one (or n) reactions must have been triggered. Useful to force organisms to do a task. |
| `REQUIRED_TASK`, `REQUIRED_REACTION` | -1 (off) | A given task or reaction is required. |
| `REQUIRE_EXACT_COPY` | 0 | The offspring must be an exact copy (checked before divide mutations). |

If the conditions fail, the divide fails and the organism carries on (or, with `DIVIDE_FAILURE_RESETS 1`, resets itself).

## Where the offspring goes

`BIRTH_METHOD` decides which organism the newborn replaces:

| Value | The newborn replaces |
|---|---|
| 0 (default) | A random organism in the parent's neighbourhood |
| 1 | The oldest in the neighbourhood |
| 2 | The one with the largest age to merit ratio in the neighbourhood |
| 3 | Nobody: only empty cells in the neighbourhood are used |
| 4 | A random organism anywhere in the population ("mass action", a well-mixed world) |
| 5 | The oldest organism in the whole population |
| 6 | A random organism within the parent's deme |
| 7 | The organism the parent faces |
| 8 | The next grid cell (id + 1) |
| 9 | The organism that has used the most energy in the whole population |
| 10 | The organism that has used the most energy in the neighbourhood |
| 11 | Local neighbourhood dispersal (see `DISPERSAL_RATE`) |
| 12 | Nobody: the offspring is killed after its birth statistics are recorded (behavioural trials) |
| 13 | Both parent and offspring are killed (behavioural trials) |

`PREFER_EMPTY` (stock 1) overrides the choice so that an empty cell in the neighbourhood is used if one exists. The *neighbourhood* is the cell's connections, which `WORLD_GEOMETRY` defines: a bounded grid, a torus (the stock choice, a grid that wraps at the edges), a clique where every cell neighbours every other, a hexagonal grid, a partial grid, a random connected graph or a scale-free graph. The 3-D lattice (`WORLD_GEOMETRY 6`) is listed in the setting but stops with an `unsupported:` message because the C++ program reads out of bounds.

A world with local birth (method 0) and one with global birth (method 4) behave very differently: local birth makes lineages spread slowly and lets many lineages explore in parallel. [Population structure](../guides/tutorial-population-structure.md) compares them.

## Death

Organisms die in four ways: of old age, when replaced by a newborn, when an event kills them (`KillProb`, `KillRectangle` and others) and, with some settings, by resources or predators. Old-age death is controlled by `DEATH_METHOD` (stock 2: after `AGE_LIMIT` times the genome length in executed instructions) and `AGE_LIMIT` (stock 20). `POPULATION_CAP` and `POP_CAP_ELDEST` cap the number of organisms.

## What happens to the parent

`DIVIDE_METHOD` 1 (stock) resets the parent's state at a divide, so that both parent and offspring begin new cycles. `DIVIDE_METHOD` 0 leaves the parent untouched, and 2 resets only the thread that divided (for parasites).

## Sexual reproduction

With the instruction `divide-sex` in place of `h-divide`, a divide does not place the offspring at once. The newborn waits in the *birth chamber* for another newborn, and the two genomes are recombined into new offspring. The stock configuration has `instset-heads-sex.cfg` for this (the same set with `divide-sex` instead of `h-divide`), and an ancestor, `default-heads-sex.org`, to match.

The settings that apply:

| Setting | Stock | Meaning |
|---|---|---|
| `RECOMBINATION_PROB` | 1.0 | Probability that recombination takes place. |
| `TWO_FOLD_COST_SEX` | 0 | 0: both recombined offspring are born. 1: only one is born, which gives sex its two-fold cost. |
| `SAME_LENGTH_SEX` | 0 | 1: recombine only with a genome of the same length. |
| `MODULE_NUM`, `CONT_REC_REGS`, `CORESPOND_REC_REGS` | 0, 1, 1 | Modular recombination: number of modules, whether recombination regions are continuous, and whether they are swapped at corresponding positions. |
| `MAX_BIRTH_WAIT_TIME` | -1 | Updates a newborn may wait for a partner (-1 is unlimited). |
| `ALLOW_MATE_SELECTION` | 0 | Let organisms choose mates (needs instruction-set support). |

To run an experiment with sex, start from the stock folder and include the sex instruction set and ancestor:

```sh
avida -def INST_SET instset-heads-sex.cfg
```

with `Inject default-heads-sex.org` in the events file. The [sex versus asex tutorial](../guides/tutorial-sex-vs-asex.md) does exactly this.

### Mating types

`MATING_TYPES 1` gives organisms separate sexes (female and male), with optional mate choice (`LEKKING`, `FORCED_MATE_PREFERENCE`, `NOISY_MATE_ASSESSMENT` and `MATE_ASSESSMENT_CV`) and a *birth chamber* (limited to `MAX_GLOBAL_BIRTH_CHAMBER_SIZE` waiting entries in a well-mixed world). Mating types need instruction-set support. The `mating_types` test of the consistency suite (`avida-core/tests/mating_types`) is a worked configuration. The instructions, the birth chamber rules, mate preferences, costs and output files are in [mating types and mate choice](mating-types.md). See also the [settings reference](../reference/settings.md#mating_types_group).

## Next

[Mutations](mutations.md).
