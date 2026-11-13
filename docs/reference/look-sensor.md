# The look sensor (look-ahead and its relatives)

The look instructions let an organism see what lies ahead of it on the grid: food, hills, walls, nests, dens, or other organisms. They are the organism's eyes in [predator and prey](../concepts/predator-prey.md) experiments. An organism passes four numbers in registers (what to look for, how far, how, and which one) and receives eight numbers back (what it used, what it found, how much and where).

This page describes the family of instructions that share that register layout. It was written from the code of the sensor (`crates/hardware/src/org_sensor.rs` and the look handlers of the experimental and BCR hardware) and checked by running the suite configuration `avida-core/tests/lookAhead_v1` and a small world built for this page, both with the output tracking switched on. Where the old wiki page disagrees with the program, that is said.

## Which instructions and which hardware

| Instruction | Looks | Notes |
|---|---|---|
| `look-ahead` | In the direction the organism faces. | |
| `look-around` | Ahead, to the left or to the right, as the fifth register says. | Takes `abs(value) mod 3` of that register: 0 ahead, 1 left, 2 right. |
| `look-ft` and `look-around-ft` | As above. | When the id sought is not valid, it becomes the organism's forage target. |
| `look-ahead-intercept` and `look-around-intercept` | As above. | The group register holds the sighted organism's facing relative to the looker instead of its group. |

These instructions are in the experimental hardware (type 3) and the BCR hardware (type 5). The BCR and GP8 hardware (types 5 and 4) also have an `-ex` family (`look-ahead-ex`, `look-again-ex`, `look-around-ex` and `-ftx` forms) with another register layout and *sessions* that remember the settings of an earlier look. This page does not describe that family. The heads CPU has no look instructions.

A look fails (the instruction counts as not executed and writes nothing) in the test CPU, because it has no neighbouring cells, and for a predator when `PRED_CONFUSION` is 4. On the BCR hardware it also fails unless `WORLD_GEOMETRY` is 1, a bounded grid.

## The registers

The look reads four registers and writes eight. The first four of the eight are the same registers that were read, so the outputs overwrite the inputs. The eight positions of the table below form one sequence of registers: the first is the register named by the nop that follows the instruction, and `?BX?` when no nop follows. Each later position takes the next nop of the label if one follows, and otherwise the register after the previous one. The experimental hardware has 8 registers, AX to HX, and the count wraps from HX to AX. So `look-ahead` with no nop reads its inputs from BX, CX, DX and EX and writes its outputs to BX, CX, DX, EX, FX, GX, HX and AX, in the order of the table (the run later on this page relies on that: it sets CX to the distance).

| Position | Input (read) | Output (written) |
|---|---|---|
| 1 | Habitat sought | Habitat used |
| 2 | Distance to look | Distance to the first thing found, or the distance used for a count |
| 3 | Search type | Search type used |
| 4 | Id of the one sought | Id used |
| 5 | none (the direction, for `look-around`) | Count of things seen |
| 6 | none | Value seen |
| 7 | none | Group, or the number of the resource found |
| 8 | none | Forage target of the organism found |

The outputs always include the input values as the sensor corrected them, so an organism can find out what its instructions were turned into. For `look-around` the direction is read from the fifth position of the sequence, the register where the count is later written.

## What the inputs mean

### Habitat

| Value | What it looks for |
|---|---|
| 0 | Food: a resource that counts as edible in the cell. |
| 1 | Hills. |
| 2 | Walls (barriers). |
| 3 | Hidden nests. They cannot be seen: a look for habitat 3 reports "not seen". |
| 4 | Dens, which are visible nests. |
| 5 | Predatory resources (simulated predators). Each cell is detected with the detection probability of the resource. |
| above 5 | Other kinds of food. |
| -2 | Other organisms. |

A habitat counts as valid only when some resource of the environment declares it with the `habitat` option of a [`GRADIENT_RESOURCE`](environment-file.md#gradient_resource), and -2 is always valid. An invalid value is replaced: a predator in a predator-prey experiment (`PRED_PREY_SWITCH` 0 or above, forage target -2 or lower) looks for organisms (-2), and anything else looks for food (0).

### Distance

A negative number becomes 1. A number above the limit becomes the limit. The limit is `LOOK_DIST` when that is not -1, and otherwise half of the longer side of the world, rounded (30 for a world of 60 by 60); `LOOK_DIST` itself is capped at the longer side. Distance 0 is only the organism's own cell. The old wiki called the default "half the long axis", which agrees.

### Search type

| Habitat | Values | Meaning |
|---|---|---|
| 0, 4 and above 5 | 0 | The closest cell whose amount is at least the threshold of the resource (an edible cell). The count is 1 and the value is the amount in that cell divided by the threshold, rounded down. |
| 0, 4 and above 5 | 1 | A sweep over the whole distance. It adds up the amounts of the cells whose resource is present but *below* the threshold, and reports the sum as the value. |
| 1, 2 and 5 | 0 or 1 | 0 stops at the first cell with the feature. 1 sweeps the whole distance. Both react to any amount above 0. |
| -2 | 0 | The closest organism of any kind. |
| -2 | 1, -1 | The closest predator (1), or the closest prey (-1). |
| -2 | 2, -2 | The number of predators (2), or of prey (-2), over the whole distance. |

Any other value is replaced: with 0 for resources (habitats other than -2) and, for organisms, with 1 for prey and -1 for predators in a predator-prey experiment (and 0 without one, where only 0, -1 and -2 are accepted). The old wiki gave -1 ("total value of resources") for food and 1 ("count edible cells"). In the program, -1 for food is invalid and becomes 0, and 1 sums the sub-threshold amounts as the table says. The two runs that show this are below.

### Id sought

For resources, a number from 0 up that names one resource of the environment, counted in the order of the environment file. The habitat then becomes the habitat of that resource, whatever the habitat register said. -1 means any resource of the habitat. An invalid number becomes -1. For organisms, it is the id of an organism: if that organism lives, the look reports on it alone, the search type is ignored (the output says -9) and its distance is found directly. A value that names the looker itself, or an organism that does not live, becomes -1 and the look is a general one.

## What comes back

| Case | Habitat | Distance | Type | Id | Count | Value | Group | Forage target |
|---|---|---|---|---|---|---|---|---|
| Nothing seen | used | -1 | used | used | 0 | 0 | -9 | -9 |
| Something seen | used | distance to it | used | used (the number of the organism, for an organism) | count | value | see below | see below |

The old wiki gave -9 for the value in the "nothing seen" case. In the program it is 0, as the first row of the table shows (the run below printed `0,0,0,0,-1,0,0,0,0,-9,-9`).

For resources, the group register holds the number of the first resource found and the forage target register stays -9. For an organism, the value is the *bonus* of the organism seen (rounded down), the group register is its group (its opinion) or, if it has none, its relative facing, and the forage target register is its forage target. With `USE_MIMICS 1`, an organism of forage target 1 may show a different forage target, with probability `MIMIC_ODDS`.

A look can report "seen" while the count is 0: a resource is present but is not edible, or the sweep of search type 1 found only sub-threshold amounts. The distance then still tells how far the sensor looked.

### Two runs that show the search types

A bounded 20 by 20 world with the line `RESOURCE food:geometry=grid:initial=800` in the environment file (2.0 units in every cell, above the threshold 1.0) and one organism that sets the distance register to 5 and executes `look-ahead`, with `TRACK_LOOK_OUTPUT 1`:

| Search type | Output (update, forage target, seen, habitat, distance, type, id, count, value, group, forage) | Reading |
|---|---|---|
| 0 | `0,-1,1,0,0,0,0,1,2,0,-9` | Found at distance 0 (the own cell), count 1, value 2, resource 0. |
| 1 | `0,-1,1,0,0,1,0,0,0,-9,-9` | Seen, but no amount below the threshold: count 0 and value 0. |

With `initial=200` instead (0.5 units in every cell, below the threshold) search type 0 reports "seen" with count 0 and value 0, and search type 1 reports distance 5, count 0 and value 9 (the sum of the sub-threshold amounts that were in reach, truncated).

The first column of each row is the update; the other ten are the columns that `TRACK_LOOK_OUTPUT` writes (see below).

## Settings

| Setting | Stock | Effect |
|---|---|---|
| `LOOK_DIST` | -1 | -1 leaves the distance limit to the instructions (half the longer side of the world). A number limits sight to that many cells. |
| `LOOK_DISABLE` | 0 | Impairs the sensor, as an experiment. 1 to 4 replace the habitat, distance, search type or id input by a random number. 5 randomises the direction of `look-around`. 6 to 13 replace one output (habitat, distance, search type, id, count, value, group, forage target, in that order) by a random number. The description in the settings table counts the outputs from 5 and not from 6, so follow this page, which reads the code. |
| `LOOK_DISABLE_TYPE` | 0 | Who is impaired: 0 predators, 1 prey, 2 both. |
| `LOOK_DISABLE_COMBO` | 0 | Experimental hardware only. Makes the sensor report "not found" for every food query (1), every look for predators (2) or every look for prey (3). |
| `PRED_CONFUSION` | 0 | When a predator sights prey, it may receive random numbers instead of the true ones. 1: the odds depend on the number of neighbours of that prey. 2: on the number of facings among them. 3: on the number of groups. 4: a predator's look always fails. |
| `USE_DISPLAY`, `USE_MIMICS`, `MIMIC_ODDS` | 0, 0, 1.0 | Whether organisms display data that others read through the sensor, and whether some organisms mimic another forage target. |
| `TRACK_LOOK_SETTINGS` | 0 | Writes `looksettings.dat`: one row for every look, with the update, the forage target of the looker, whether it saw something, and the final habitat, distance, search type and id. |
| `TRACK_LOOK_OUTPUT` | 0 | Writes `lookoutput.dat`: the same, plus count, value, group and forage target as the registers received them. |

Both tracking files have a header, then one comma-separated row per look. Switching them on is the easiest way to learn what an organism's look instructions really do. In the 1000-update run of `lookAhead_v1` they gave, for instance, `1,-2,0,-2,-1,-1,-1,0,0,-9,-9` for a predator that looked for the closest prey and saw none, and `699,3,1,4,0,0,1,1,1,1,-9` for a prey organism that stood in a den (habitat 4, resource 1).

All settings are in the [MOVEMENT_GROUP of the settings reference](settings.md#movement_group).

## A worked example from the test suite

The predator of `lookAhead_v1` (`pred-rotate-org0.org`) sets the habitat register to -2 (organisms), the distance to 25 and the search type to -1 (closest prey), and then executes `look-ahead` with the nops `nop-B nop-C nop-D`, so the habitat sits in BX, the distance in CX and the search type in DX. No nop names the id register, so it is the next one in order, EX, which the organism set to -1 (any). After the look the organism tests the count register with `if-not-0` and chooses between rotating and moving; read the full program in the test folder. The prey organisms of the same test look for food (habitat 0) and for dens (habitat 4) in the same way.

Predator and prey experiments need `USE_AVATARS 2` and the experimental hardware, as the [predator and prey page](../concepts/predator-prey.md) explains.

## Not supported

A few look cases that are undefined in the C++ program stop with `unsupported:` messages, for instance a look at an organism that has no state or a confusion calculation on an organism without an avatar. See [features that stop with unsupported](unsupported.md).
