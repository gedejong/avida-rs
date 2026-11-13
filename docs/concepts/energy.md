# The energy model

By default, Avida organisms are limited only by the CPU time they win. The *energy model* adds a second budget: each organism holds an amount of energy, and executing instructions uses it up. See the settings below for how it is given, spent and shared. Turn it on with `ENERGY_ENABLED 1`.

## How energy moves

- An organism receives energy when it is injected (`ENERGY_GIVEN_ON_INJECT`) and when it is born (`ENERGY_GIVEN_AT_BIRTH`), and its parent passes on a fraction of its own energy (`FRAC_PARENT_ENERGY_GIVEN_TO_ORG_AT_BIRTH`, stock 0.5).
- Reactions with a process of type `energy` pay out energy instead of bonus. `APPLY_ENERGY_METHOD` chooses when the reward is applied: 0 on divide, 1 on completion of the task, 2 on `sleep`.
- Instructions can carry an energy cost (`energy_cost` in the [instruction set file](../reference/instruction-sets.md)). `NUM_CYCLES_EXC_BEFORE_0_ENERGY` sets how many virtual CPU cycles an organism executes before its energy is exhausted.
- `ENERGY_CAP` limits the energy an organism can store (-1 is no limit). `FRAC_ENERGY_DECAY_AT_ORG_BIRTH` is the fraction lost during a birth.
- `FRAC_ENERGY_TRANSFER` is the fraction of a replaced organism's energy that the newborn takes.
- `FIX_METABOLIC_RATE` fixes the metabolic rate to a constant (-1 leaves it free).

## Energy and demes

Demes can hold energy too: `FRAC_PARENT_ENERGY_GIVEN_TO_DEME_AT_BIRTH` and `FRAC_ENERGY_DECAY_AT_DEME_BIRTH` apply to deme replication, and `ENERGY_PASSED_ON_DEME_REPLICATION_METHOD` chooses whether the offspring deme's energy is divided among its injected organisms or among its cells. Energy sharing between organisms has its own group, `ENERGY_SHARING_GROUP`.

## Data and actions

- `PrintDemeEnergyDistributionStats`, `PrintDemeSpacialEnergyStats` and `PrintDemesTotalAvgEnergy` report energy by deme.
- `SetNumInstBefore0Energy` changes the number of instructions an organism may run before its energy is used up.
- The stock consistency suite has two energy configurations, `energy_asex_1000u` and `energy_deme_level_res`, in `avida-core/tests`.

## Not supported

`PrintDemeEnergySharingStats` stops with `unsupported:` (the C++ program reads uninitialised counters). A few combinations of `FRAC_ENERGY_TRANSFER` with kills or cell swaps of the running organism do too. See [unsupported features](../reference/unsupported.md).

## Related settings

The ENERGY group in the [settings reference](../reference/settings.md#energy_group) lists the rest, including thresholds for low and high energy (`ENERGY_THRESH_LOW`, `ENERGY_THRESH_HIGH`) that instructions can test.
