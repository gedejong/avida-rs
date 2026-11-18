# The energy model

By default, Avida organisms are limited only by the CPU time they win. The *energy model* adds a second budget: each organism holds an amount of energy, and executing instructions uses it up. See the settings below for how it is given, spent and shared. Turn it on with `energy.enabled: 1`.

## How energy moves

- An organism receives energy when it is injected (`energy.given_on_inject`) and when it is born (`energy.given_at_birth`), and its parent passes on a fraction of its own energy (`energy.frac_parent_energy_given_to_org_at_birth`, stock 0.5).
- Reactions with a process of type `energy` pay out energy instead of bonus. `energy.apply_method` chooses when the reward is applied: 0 on divide, 1 on completion of the task, 2 on `sleep`.
- Instructions can carry an energy cost (`energy_cost` in the [instruction set file](../reference/instruction-sets.md)). `energy.num_cycles_exc_before_0_energy` sets how many virtual CPU cycles an organism executes before its energy is exhausted.
- `energy.cap` limits the energy an organism can store (-1 is no limit). `energy.frac_energy_decay_at_org_birth` is the fraction lost during a birth.
- `energy.frac_energy_transfer` is the fraction of a replaced organism's energy that the newborn takes.
- `energy.fix_metabolic_rate` fixes the metabolic rate to a constant (-1 leaves it free).

## Energy and demes

Demes can hold energy too: `energy.frac_parent_energy_given_to_deme_at_birth` and `energy.frac_energy_decay_at_deme_birth` apply to deme replication, and `energy.passed_on_deme_replication_method` chooses whether the offspring deme's energy is divided among its injected organisms or among its cells. Energy sharing between organisms has its own group, `ENERGY_SHARING_GROUP`.

## Data and actions

- `PrintDemeEnergyDistributionStats`, `PrintDemeSpacialEnergyStats` and `PrintDemesTotalAvgEnergy` report energy by deme.
- `SetNumInstBefore0Energy` changes the number of instructions an organism may run before its energy is used up.
- The stock consistency suite has two energy configurations, `energy_asex_1000u` and `energy_deme_level_res`, in `avida-core/tests`.

## Not supported

`PrintDemeEnergySharingStats` stops with `unsupported:` (the C++ program reads uninitialised counters). A few combinations of `energy.frac_energy_transfer` with kills or cell swaps of the running organism do too. See [unsupported features](../reference/unsupported.md).

## Related settings

The group `energy` in the [experiment file reference](../reference/config-yaml.md#energy) lists the rest, including thresholds for low and high energy (`energy.thresh_low`, `energy.thresh_high`) that instructions can test.
