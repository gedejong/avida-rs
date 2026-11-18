# Design an experiment: a checklist

Avida has hundreds of settings, and most experiments change a handful. This checklist names the decisions in the order they are usually made, and where each is made. It adapts the old beginner's guide.

Start from the stock folder (see [your first run](../getting-started/first-run.md)) and change one thing at a time.

1. **Size and shape of the world.** `world.x`, `world.y` set the carrying capacity (one organism per cell, so 60 by 60 is 3,600). `world.geometry` sets the shape: the stock torus (2) has no edges. See [reproduction](../concepts/reproduction.md#where-the-offspring-goes).
2. **The ancestor.** The `Inject` entry of the events. The stock `default-heads.org` is haploid and asexual and a good first choice. To use another instruction set, name it in `instruction_sets` and inject an ancestor written for it ([instruction sets](../reference/instruction-sets.md)).
3. **Birth and death.** `reproduction.birth_method` (where newborns go), `reproduction.death_method` and `reproduction.age_limit` (death by old age), `general.population_cap` (a smaller cap than the world leaves space free; `general.population_cap_eldest` is also a cap in organisms, whose victim is the oldest organism). Both caps are numbers of organisms, with 0 for no cap.
4. **Resources.** The stock resource is unlimited. To make organisms compete, define resources with inflow and outflow in the `environment` section ([environment](../reference/environment-file.md)).
5. **Tasks and rewards.** The `reactions` of the environment: which tasks, what each is worth (`value`), how rewards combine (`type`) and how often (`requisite`). Do not rename the `task` of a reaction; it is the task's identity, while the reaction's `name` is free ([tasks](../reference/tasks.md)).
6. **Mutation.** `mutation.copy_prob` and the divide mutations ([mutations](../concepts/mutations.md)). If you will analyze genomes, consider [fixed-length organisms](how-to-fixed-length.md).
7. **Length of the run.** The `Exit` entry of the events. A generation takes about 13 updates for the stock ancestor at the start (gestation of about 389 cycles at 30 per update) and fewer as organisms improve.
8. **Data.** The `Print...` events, how often, and a `SavePopulation` at the end if you plan to use [analyze mode](../reference/analyze-mode.md). Printing less often makes smaller files; the stock events print every 100 updates.
9. **Replicates.** One run proves nothing: use several seeds per arm ([replicates](how-to-replicates.md)).
10. **Run a short test first.** A 500 to 1,000-update run with the final configuration, to catch typos: an unknown setting, a missing file and a wrong ancestor all produce an error message at start-up.

Check a configuration against the defaults with `avida -r`, which lists the settings that differ.
