# Design an experiment: a checklist

Avida has hundreds of settings, and most experiments change a handful. This checklist names the decisions in the order they are usually made, and where each is made. It adapts the old beginner's guide.

Start from the stock folder (see [your first run](../getting-started/first-run.md)) and change one thing at a time.

1. **Size and shape of the world.** `WORLD_X`, `WORLD_Y` set the carrying capacity (one organism per cell, so 60 by 60 is 3,600). `WORLD_GEOMETRY` sets the shape: the stock torus (2) has no edges. See [reproduction](../concepts/reproduction.md#where-the-offspring-goes).
2. **The ancestor.** The `Inject` line of `events.cfg`. The stock `default-heads.org` is haploid and asexual and a good first choice. To use another instruction set, include it and inject an ancestor written for it ([instruction sets](../reference/instruction-sets.md)).
3. **Birth and death.** `BIRTH_METHOD` (where newborns go), `DEATH_METHOD` and `AGE_LIMIT` (death by old age), `POPULATION_CAP` (a smaller cap than the world leaves space free; `POP_CAP_ELDEST` is also a cap in organisms, whose victim is the oldest organism). Both caps are numbers of organisms, with 0 for no cap.
4. **Resources.** The stock resource is unlimited. To make organisms compete, define resources with inflow and outflow in `environment.cfg` ([environment file](../reference/environment-file.md)).
5. **Tasks and rewards.** The `REACTION` lines of `environment.cfg`: which tasks, what each is worth (`value`), how rewards combine (`type`) and how often (`requisite`). Do not rename a task in a reaction line; the second word of the line is the task's identity ([tasks](../reference/tasks.md)).
6. **Mutation.** `COPY_MUT_PROB` and the divide mutations ([mutations](../concepts/mutations.md)). If you will analyze genomes, consider [fixed-length organisms](how-to-fixed-length.md).
7. **Length of the run.** The `Exit` line of `events.cfg`. A generation takes about 13 updates for the stock ancestor at the start (gestation of about 389 cycles at 30 per update) and fewer as organisms improve.
8. **Data.** The `Print...` events, how often, and a `SavePopulation` at the end if you plan to use [analyze mode](../reference/analyze-mode.md). Printing less often makes smaller files; the stock file prints every 100 updates.
9. **Replicates.** One run proves nothing: use several seeds per arm ([replicates](how-to-replicates.md)).
10. **Run a short test first.** A 500 to 1,000-update run with the final configuration, to catch typos: an unknown setting, a missing file and a wrong ancestor all produce an error message at start-up.

Check a configuration against the defaults with `avida -r`, which lists the settings that differ.
