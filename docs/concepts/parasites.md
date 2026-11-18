# Parasites

A *parasite* in Avida is a self-replicating program that lives as an extra CPU thread inside a host organism, steals some of its CPU time, and spreads by infecting other hosts. Parasites give you coevolution: hosts evolve to resist, and parasites evolve to overcome the defences.

The experiments and tests of the program use the TransSMT hardware (hardware type 2) for parasites. The [host and parasite tutorial](../guides/tutorial-host-parasite.md) is a complete example.

## How it works

1. A host and a parasite are both genomes on TransSMT hardware. Each has *memory spaces*: regions for the genome, for the offspring being built, and for threads.
2. An event injects parasites into hosts with `InjectParasite`: a parasite file, the memory-space label, and a range of cells.
3. A parasite copies itself instruction by instruction like a host. Instead of dividing an offspring into the world, it tries to *infect* another organism in its host's neighbourhood. With `reproduction.birth_method: population_random` or `world.geometry: random_connected`, any organism in the population is a target.
4. Infection succeeds only if the target host can accept a new thread: the host must have fewer threads than `time.max_cpu_threads` (set it to 2 so each host carries at most one parasite) and must not have used the memory space the parasite wants.
5. Further conditions can depend on what the host and parasite do. `parasites.infection_mechanism` (`INJECT_IS_TASK_SPECIFIC` or `INFECTION_MECHANISM` in the old `avida.cfg`) chooses the rule:

| Value | Infection succeeds when |
|---|---|
| 0 | Always (independent of tasks). |
| 1 | The parasite performs at least one task the host performs. |
| 2 | The parasite does at least one task the host does not. |
| 3 | The parasite and the host perform exactly the same tasks (matching alleles). |

The default of `parasites.infection_mechanism` is 1. With a rule of 1 or 3, a host escapes by changing which tasks it performs, and parasites have to follow.

## Virulence

`parasites.virulence` is the probability that a CPU cycle goes to the parasite rather than the host. At 1 a parasite takes everything. `parasites.virulence_source` makes virulence come from the setting (0), be inherited from the parent parasite (1), or be controlled by the host (2), with `parasites.virulence_mut_rate` as the probability that an inherited virulence mutates. `parasites.inject_is_virulent: true` instead makes an infection sterilise the host and take all its cycles.

## Typical settings

The parasite experiment in the tutorial uses (in addition to the stock settings):

```yaml
reproduction:
  birth_method: 4                # well-mixed population
time:
  max_cpu_threads: 2             # at most one parasite per host
divide:
  require_single_reaction: 1     # hosts must do at least one task to reproduce
parasites:
  inject_method: 1               # reset the parasite thread on birth (required for SMT parasites)
  infection_mechanism: 1         # the parasite must match one of the host's tasks (the default)
  virulence: 0.85                # share of cycles stolen
  no_copy_mut: true              # parasites ignore the copy mutation rate
instruction_sets: [transsmt]     # hardware type 2
```

The old `HARDWARE_TYPE 2` line has no place in the file: the hardware type is the instruction set's.

## Data

`PrintParasiteData`, `PrintParasiteTasksData`, `PrintHostTasksData`, `PrintHostPhenotypeData` and `PrintParasitePhenotypeData` write the numbers of parasites, their tasks and the diversity of host and parasite phenotypes. Other actions inject parasites from genotype files (`InjectParasitePair`, `InjectParasiteSequence`), load them (`LoadParasiteGenotypeList`) and kill demes with the highest parasite load (`KillDemesHighestParasiteLoad`). The [actions reference](../reference/actions.md) lists them all.

## Parasites and demes

With demes, parasites can migrate between demes (`demes.parasite_migration_rate`, which works with `demes.migration_method: matrix`), and hosts can be protected from migrants by a parasite *memory score* (`demes.parasite_migration_memory_score_protective_threshold`).

## Next

[Predator and prey](predator-prey.md).
