# Predator and prey

The experimental hardware (hardware type 3) lets organisms be *predators* or *prey* that move around the world, sense what is near them, and attack each other. These features work with the experimental CPU; the heads CPU does not run them.

This page describes the settings and files involved. The program contains worked configurations for them in its consistency suite, `avida-core/tests/pp-evolved` and `avida-core/tests/avatars-pred_look`, which are the best starting points. No tutorial for predator and prey experiments is included in this documentation yet.

## What you need

- An instruction set of hardware type 3, such as `instset-experimental.cfg` in the stock folder, and an experimental ancestor (`experimental.org` is in the stock folder).
- *Avatars*: `USE_AVATARS 1` lets organisms move and navigate in worlds where each organism's avatar occupies a cell; `USE_AVATARS 2` also lets organisms interact with one another. Predator and prey tests use `USE_AVATARS 2`.
- `PRED_PREY_SWITCH`, which switches predators on and chooses whether organisms may change role:

| Value | Meaning |
|---|---|
| -2 | No predators, but prey statistics are tracked. |
| -1 | No predators in the experiment (the default). |
| 0 | A predator cannot switch to being prey; prey can switch to predator. |
| 1 | Predators can switch to being prey. |
| 2 | Predators cannot switch to being prey, and prey cannot switch by changing their forage target (becoming a predator through an attack is still allowed). |

## Settings that shape the interaction

| Setting | Default | Meaning |
|---|---|---|
| `PRED_ODDS` | 1.0 | Probability that a predator's attack instruction succeeds. |
| `PRED_EFFICIENCY` | 1.0 | The fraction of the consumed prey's bonus, merit and resource bins that the predator gains. |
| `PRED_EFFICIENCY_POISON` | 1.0 | The same, subtracted, when the prey was poisonous. |
| `PRED_INJURY` | 0.0 | If an attack fails, the target's bonus, merit and internal resources are reduced by this fraction. |
| `PRED_CONFUSION` | 0 | Makes predators receive random data when they see prey that have several neighbours. |
| `MIN_PREY` | 0 | A positive value makes predator attacks fail when the number of prey falls below it; a negative value clones random prey. |
| `MAX_PREY` | 0 | Cap on prey; random prey are removed when it is exceeded. |

The full set is in [MOVEMENT_GROUP and the neighbouring groups](../reference/settings.md#movement_group) of the settings reference; search the page for `PRED_` and `PREY`.

## Seeing: the look instructions

Predators and prey find food, dens and each other with the look instructions. Which registers they read and write, what the habitat and search-type values mean, and the settings that impair or track the sensor are on the [look sensor page](../reference/look-sensor.md).

## Data

`PrintPreyAverageData`, `PrintPreyErrorData`, `PrintPreyVarianceData` and their `Predator` and `TopPredator` counterparts write statistics by role. `PrintPreyInstructionData` and `PrintPredatorInstructionData` give the instruction frequencies. `PrintTargets`, `PrintOrgLocData`, `PrintMiniTraces` and `PrintMicroTraces` write targets, locations and execution traces.

## Not supported

Some paths stop with `unsupported:` where the C++ program is undefined: a `MAX_PREY_BT` kill from a plasticity test or from a test CPU run by an event, and a few others listed in [unsupported features](../reference/unsupported.md).
