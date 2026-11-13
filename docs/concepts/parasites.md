# Parasites

A *parasite* in Avida is a self-replicating program that lives as an extra CPU thread inside a host organism, steals some of its CPU time, and spreads by infecting other hosts. Parasites give you coevolution: hosts evolve to resist, and parasites evolve to overcome the defences.

The experiments and tests of the program use the TransSMT hardware (hardware type 2) for parasites. The [host and parasite tutorial](../guides/tutorial-host-parasite.md) is a complete example.

## How it works

1. A host and a parasite are both genomes on TransSMT hardware. Each has *memory spaces*: regions for the genome, for the offspring being built, and for threads.
2. The events file injects parasites into hosts with `InjectParasite`: a parasite file, the memory-space label, and a range of cells.
3. A parasite copies itself instruction by instruction like a host. Instead of dividing an offspring into the world, it tries to *infect* another organism in its host's neighbourhood. With `BIRTH_METHOD 4` or `WORLD_GEOMETRY 7`, any organism in the population is a target.
4. Infection succeeds only if the target host can accept a new thread: the host must have fewer threads than `MAX_CPU_THREADS` (set it to 2 so each host carries at most one parasite) and must not have used the memory space the parasite wants.
5. Further conditions can depend on what the host and parasite do. `INJECT_IS_TASK_SPECIFIC` (also spelled `INFECTION_MECHANISM`) chooses the rule:

| Value | Infection succeeds when |
|---|---|
| 0 | Always (independent of tasks). |
| 1 | The parasite performs at least one task the host performs. |
| 2 | The parasite does at least one task the host does not. |
| 3 | The parasite and the host perform exactly the same tasks (matching alleles). |

The default of `INJECT_IS_TASK_SPECIFIC` in the settings table is 1. With a rule of 1 or 3, a host escapes by changing which tasks it performs, and parasites have to follow.

## Virulence

`PARASITE_VIRULENCE` is the probability that a CPU cycle goes to the parasite rather than the host. At 1 a parasite takes everything. `VIRULENCE_SOURCE` makes virulence come from the setting (0), be inherited from the parent parasite (1), or be controlled by the host (2), with `VIRULENCE_MUT_RATE` as the probability that an inherited virulence mutates. `INJECT_IS_VIRULENT 1` instead makes an infection sterilise the host and take all its cycles.

## Typical settings

The parasite experiment in the tutorial uses (in addition to the stock settings):

```text
HARDWARE_TYPE 2                  # deprecated; the instruction set's hw_type decides
BIRTH_METHOD 4                   # well-mixed population
INJECT_METHOD 1                  # reset the parasite thread on birth (required for SMT parasites)
MAX_CPU_THREADS 2                # at most one parasite per host
INJECT_IS_TASK_SPECIFIC 1        # the parasite must match one of the host's tasks
PARASITE_VIRULENCE 0.85          # share of cycles stolen
PARASITE_NO_COPY_MUT 1           # parasites ignore the copy mutation rate
REQUIRE_SINGLE_REACTION 1        # hosts must do at least one task to reproduce
```

with `#include INST_SET=instset-transsmt.cfg`.

## Data

`PrintParasiteData`, `PrintParasiteTasksData`, `PrintHostTasksData`, `PrintHostPhenotypeData` and `PrintParasitePhenotypeData` write the numbers of parasites, their tasks and the diversity of host and parasite phenotypes. Other actions inject parasites from genotype files (`InjectParasitePair`, `InjectParasiteSequence`), load them (`LoadParasiteGenotypeList`) and kill demes with the highest parasite load (`KillDemesHighestParasiteLoad`). The [actions reference](../reference/actions.md) lists them all.

## Parasites and demes

With demes, parasites can migrate between demes (`DEMES_PARASITE_MIGRATION_RATE`, which works with `DEMES_MIGRATION_METHOD 4`), and hosts can be protected from migrants by a parasite *memory score* (`DEMES_PARASITE_MIGRATION_MEMORY_SCORE_PROTECTIVE_THRESHOLD`).

## Next

[Predator and prey](predator-prey.md).
