# Demes

A *deme* is a subpopulation: a group of cells that the program treats as a unit. Demes make it possible to study group-level selection, where whole groups replicate, compete and die, on top of the selection among individuals inside each group.

## Setting up demes

The world is one grid, which the program cuts into `NUM_DEMES` equal parts. The deme's width is the world's width (`WORLD_X`) and its height is `WORLD_Y` divided by `NUM_DEMES`. For 4 demes of 10 by 10 cells, use:

```text
WORLD_X 10
WORLD_Y 40
NUM_DEMES 4
```

`WORLD_Y` must be divisible by `NUM_DEMES`. In analyze mode a setting where it is not stops with an `unsupported:` message, because the C++ program crashes there.

## Starting a run with demes

The `Inject` action places one organism in one cell. Several actions do it per deme:

| Action | Effect |
|---|---|
| `InjectDemes` | Injects the organism into one cell of every deme. |
| `InjectAll` | Injects it into every cell of the world. |
| `InjectDemesRandom`, `InjectDemesFromNest`, `InjectModuloDemes` | Variants that choose cells differently. See the [actions reference](../reference/actions.md). |

## Deme replication and competition

A deme can *replicate*: its organisms seed another deme, which they replace. When it replicates is decided either by an event or by a setting.

- By event: `ReplicateDemes` takes a trigger (the default is `full_deme`: replicate when the deme is full). `ReplicateDemesHighestBirthCount` and `ReplicateDemesHighestFecundity` replicate the demes with the highest births or fecundity.
- By setting (implicit replication): `DEMES_REPLICATE_CPU_CYCLES`, `DEMES_REPLICATE_TIME`, `DEMES_REPLICATE_BIRTHS` and `DEMES_REPLICATE_ORGS` trigger a deme's replication when it has used that many CPU cycles per organism, that much normalised time, that many births, or that many organisms. A value of 0 switches a trigger off.
- By competition: `CompeteDemes` chooses which demes replicate by fitness proportional (`DEMES_COMPETITION_STYLE 0`) or tournament selection (`1`, with `DEMES_TOURNAMENT_SIZE`). The fitness of a deme is computed from its organisms' behaviour (variants: `CompeteDemesByTaskCount`, `CompeteDemesByEnergyDistribution`, `CompeteDemesByNetwork`, `CompeteDemes_AttackKillAndEnergyConserve`).

`ResetDemes` resets all demes to their founders.

### Germlines

With `DEMES_USE_GERMLINE` set, a deme keeps a distinct *germline*: a line of genomes that founds the next deme, separate from the soma that does the work. Replication uses the `GERMLINE_*_MUT` rates.

### Migration

An offspring can be born in a different deme: `DEMES_MIGRATION_RATE` is the probability, and `DEMES_MIGRATION_METHOD` chooses the targets (any other deme, the eight neighbouring demes, two adjacent demes, weighted by points, or a weight matrix that `SetMigrationMatrix` and `AlterMigrationConnection` can change). The matrix file, its rules and the actions that report migration counts are described in [the deme migration matrix](../reference/migration-matrix.md).

## An example you can run

This events file puts one organism in each of four demes of 10 by 10 cells, replicates each deme when it is full, and prints the deme statistics:

```text
u begin InjectDemes default-heads.org
u 0:50:end PrintCountData
u 0:50:end PrintDemeStats
u 1:1:end ReplicateDemes full_deme
u 300 Exit
```

Run it in a copy of the stock folder with `avida -s 3 -set WORLD_X 10 -set WORLD_Y 40 -set NUM_DEMES 4`. You should see a number of `deme_*.dat` files appear next to `count.dat` in the data folder (`deme_fitness.dat`, `deme_gest_time.dat`, `deme_merit.dat`, `deme_task.dat`, `deme_resources.dat`, `deme_instruction-0-heads_default.dat` to `deme_instruction-3-...` and others). In `count.dat` the number of organisms rises as demes fill and falls when a replication replaces demes with founders.

## Deme resources and the energy model

Resources can be kept per deme (`deme` option of a `RESOURCE` line, and the `SetDemeResource...` actions), and organisms can share energy through their deme. See [environment, tasks and resources](environment-and-tasks.md) and [the energy model](energy.md).

## In the parallel execution mode

Demes run in the [parallel mode](../execution/index.md). With implicit deme replication, a deme generation takes about one update longer than in the exact mode, because a deme whose organisms cross a replication threshold in the middle of a round replicates at the end of that round.

## Not supported

- `PrintDemeEnergySharingStats` stops with `unsupported:` because the C++ program reads uninitialised counters.
- The deme network needs Boost, which the reference binary was built without: the first use of a deme network prints the `cDemeNetwork requires Boost` error and exits with code 50, here and in the reference.
- Deme resources of grid, torus or partial geometry stop with `unsupported:` in a few paths (unsized deme grids).

See [Features that stop with unsupported](../reference/unsupported.md).
