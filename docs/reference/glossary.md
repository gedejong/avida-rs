# Glossary

Terms are listed alphabetically. A term in *italics* inside an entry has its own entry.

**Action.** Something an *event* can do, such as `Inject`, `PrintAverageData` or `Exit`. See the [actions reference](actions.md).

**Analyze mode.** A scripting mode of `avida` (`avida -a`) that loads saved genotypes and measures them in a *test CPU* instead of running a population. See [analyze mode](analyze-mode.md).

**Ancestor.** The first organism of a run, injected from an `.org` file.

**Avatar.** In experiments with movement, the body of an organism on the grid, separate from its cell. Needs `USE_AVATARS`.

**Batch.** In *analyze mode*, a group of loaded genotypes that commands work on.

**Birth chamber.** The place where newborns wait for a partner in sexual reproduction, or for mate choice. See [mating types](../concepts/mating-types.md).

**Bonus.** The multiplier built up from an organism's rewarded *tasks*. *Merit* is base merit times bonus.

**Cell.** One square of the world grid. A cell holds at most one organism.

**Clone file.** A file that stores a population for another run to start from. See `-l` on the [command-line page](../front-ends/cli.md).

**Deme.** A subpopulation of the world that can replicate and compete as a group. See [demes](../concepts/demes.md).

**Environment.** The set of *resources*, *tasks* and *reactions* defined in `environment.cfg`.

**Event.** A line of the events file: a trigger, a timing and an *action*. See [events file](events-file.md).

**Exact mode.** `EXECUTION_MODE 0`, the default: the original scheduler, whose output is byte-identical to the C++ program's. See [execution modes](../execution/index.md).

**Fitness.** *Merit* divided by *gestation time*: how fast an organism converts CPU time into offspring.

**Flame graph.** A picture of the distribution of phylogenetic depth over time, made from saved populations. See [how to draw one](../guides/how-to-flame-graph.md).

**Generation.** The number of replication steps between an organism and the first ancestor.

**Genome.** An organism's list of instructions.

**Genotype.** The set of all organisms with the same genome. See [systematics](../concepts/systematics.md).

**Germline.** In *demes*, the line of genomes that founds new demes.

**Gestation time.** The CPU cycles an organism needed to produce its last offspring.

**Head.** A pointer into the genome on the *heads CPU*: the instruction pointer, the read head, the write head and the flow head.

**Heads CPU.** Hardware type 0, the default virtual CPU. See [virtual CPUs](../concepts/hardware.md).

**Hardware type.** The kind of virtual CPU an *instruction set* is for (0, 2, 3, 4 or 5 in this port).

**Host.** An organism that carries a *parasite*.

**Input.** A number the environment hands to an organism through the `IO` instruction. Tasks test outputs against the inputs.

**Instruction.** One element of a genome, such as `nop-A` or `h-copy`.

**Instruction set.** The list of instructions available to an experiment, with costs and frequencies. See [instruction sets](instruction-sets.md).

**Label.** A run of no-operation instructions that marks a place in a genome and is found by `h-search`.

**Look sensor.** What the look instructions read from the grid and return in registers. See [the look sensor](look-sensor.md).

**Mating type.** The sex of an organism when `MATING_TYPES 1`: juvenile, female or male, set by instructions. See [mating types](../concepts/mating-types.md).

**Merit.** An organism's claim on CPU time. It starts at a base value tied to genome size and is multiplied by the rewards of tasks performed.

**Migration matrix.** The file named by `MIGRATION_FILE`, with a weight for each ordered pair of demes. See [the deme migration matrix](migration-matrix.md).

**Mutation.** A random change to a genome. See [mutations](../concepts/mutations.md).

**Nop.** A no-operation instruction (`nop-A`, `nop-B`, `nop-C`). It does nothing on its own but modifies the instruction before it.

**Organism.** A genome together with its virtual CPU, living in a cell.

**Parallel mode.** `EXECUTION_MODE 1`, the opt-in mode that runs organism-local instructions on several threads and is not byte-identical to the exact mode. See [execution modes](../execution/index.md).

**Parasite.** A thread that runs inside a host, steals CPU time and infects other hosts. See [parasites](../concepts/parasites.md).

**Phenotype.** What an organism has done and measured: merit, gestation time, tasks performed and more.

**Reaction.** A line of the environment file that links a *task* to a reward.

**Requisite.** A condition on a reaction, such as "only the first time".

**Resource bin.** A private store of one resource inside an organism, filled by collect instructions. See [internal resources](../concepts/internal-resources.md).

**Resource.** Something a reaction can consume. Resources can be global, spatial, gradient, per-deme or internal.

**Seed.** The starting value of the random number generator. The same seed on the same configuration and build gives the same run in the exact mode.

**Slip mutation.** A mutation that deletes or duplicates a stretch of the genome.

**Systematics.** The bookkeeping of genotypes and ancestry. See [systematics](../concepts/systematics.md).

**Task.** A test on an organism's output, for example "is the output the NAND of two inputs". See [tasks](tasks.md).

**Test CPU.** A separate virtual CPU that runs one genome in isolation to measure its fitness and tasks. Analyze mode and several statistics use it.

**Time slice.** The unit of CPU time: one instruction of one organism in the exact mode.

**Torus.** A grid that wraps at the edges, the stock world geometry.

**Update.** The unit of simulated time. One update is `AVE_TIME_SLICE` (stock 30) time slices per organism on average. See [the update cycle](../concepts/update-cycle.md).

**World.** The grid of cells, with its resources and demes.
