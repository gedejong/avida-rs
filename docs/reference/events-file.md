# Events file reference (`events.cfg`)

The events file says what happens and when: which organism starts the run, which data files are written, when the environment changes, and when the run ends. Its name is the setting `EVENT_FILE` (stock `events.cfg`).

Every rule below was checked against the event-list parser in the `avida-world` crate (`crates/world/src/event_list.rs`) or by a short run.

## Line format

```text
trigger  timing  action  [arguments...]
```

Blank lines and everything after `#` are ignored. The action name is not case-sensitive (`PrintAverageData` and `printaveragedata` write the same file). Arguments are separated by spaces, and each action defines its own; see the [actions reference](actions.md) or run `avida -e`.

### Triggers

| Letter | Long name | The timing counts |
|---|---|---|
| `u` | `update` | Updates. |
| `g` | `generation` | The average generation of the population. |
| `i` | `immediate` | Nothing: an immediate event runs once, at the very start of the run, and takes no timing. |
| `b` | `births` | Births. |
| `o` | `org_id` | Organism ids (an interrupt triggered at a birth); rarely used. |

Anything else is an error: `unrecognized event trigger`.

### Timing

Timing is `start`, or `start:interval`, or `start:interval:stop`, written with colons and no spaces.

| Part | Meaning |
|---|---|
| `start` | The first time the event fires. The word `begin` means the very start of the run. |
| `interval` | How often it repeats afterwards. The word `all` means every possible tick, and `once` means do not repeat (the default). |
| `stop` | The last time it may fire. The word `end` means no stop (the default). |

Examples:

| Line | Meaning |
|---|---|
| `u begin Inject default-heads.org` | Inject the ancestor at the start of the run. |
| `u 100 Exit` | Stop the run at update 100. |
| `u 0:100:end PrintAverageData` | Write the averages at update 0 and every 100 updates, until the end. |
| `u 100:100:5000 PrintCountData` | At update 100 and every 100 after, until update 5000. |
| `g 10:10 SavePopulation` | Every 10 generations, from generation 10. |
| `u 1:1:end ReplicateDemes full_deme` | Every update from update 1. |
| `i Echo hello` | Print a message at the very start of the run. |

An event with an interval of 100 that starts at 0 fires at the start of the run and then at updates 100, 200 and so on. Events fire at the boundary between updates, before the next update's time slices: see [the update cycle](../concepts/update-cycle.md).

## The stock file

```text
u begin Inject default-heads.org

u 0:100:end PrintAverageData
u 0:100:end PrintDominantData
u 0:100:end PrintCountData
u 0:100:end PrintTasksData
u 0:100:end PrintTimeData
u 0:100:end PrintResourceData

u 0:500:end SavePopulation
u 100000 Exit
```

What each line does:

- `Inject default-heads.org` puts the ancestor, read from that file, into cell 0 (`Inject`'s second argument is the cell, default 0).
- The six `Print...` events write the data files described in [output files](../getting-started/output-files.md).
- `SavePopulation` writes `detail-N.spop`.
- `Exit` ends the run. A run with no `Exit` and no other ending condition never stops.

## The most used actions

| Action | Arguments | Effect |
|---|---|---|
| `Inject` | `fname [cell_id=0] [merit=-1] [lineage_label=0] [neutral_metric=0] [trace_filename]` | Place one organism from a `.org` file. |
| `InjectAll`, `InjectRange` | see the actions reference | Inject into every cell, or a range of cells. |
| `Exit` | | End the run. |
| `ExitAveGeneration` | `generation` | End the run when the average generation reaches the value. |
| `Print...Data` | `[fname]` | Write one of the data files. |
| `PrintData` | `fname format` | Write chosen columns; see below. |
| `SavePopulation` | `[filename='detail'] [save_historic=1] ...` | Write a population snapshot. |
| `LoadPopulation` | `fname [update=-1] ...` | Start from a snapshot. |
| `SetMutProb` | `[mut_type='COPY_MUT'] [prob] [start_cell] [end_cell]` | Change a mutation rate. |
| `SetReactionValue` | `reaction_name value` | Change a reaction's reward. |
| `SetResourceInflow`, `SetResourceOutflow`, `SetResource` | `resource_name value` | Change a resource. |
| `ChangeEnvironment` | `env_string` | Load one line of [environment-file](environment-file.md) syntax during the run, for example `RESOURCE Food:inflow=5`, and update the resources. |
| `KillProb` | `[probability=0.9]` | Kill each organism with a probability. |
| `Echo` | `message` | Print a message. |
| `SetConfig` | `config_var value` | Change a setting during the run. Whether a setting takes effect mid-run depends on the setting; `EXECUTION_MODE` does not. |

### `PrintData`

`PrintData fname column,column,...` writes a file with the columns you choose, in the order you give them:

```text
u 0:50:end PrintData mydata.dat update,ave_fitness,ave_gest,ave_merit,generation,num_cpus
```

The names are case-sensitive. These are the ones that exist: `update`, `generation`, `num_resamplings`, `num_failedResamplings`, `num_births`, `cumulative_births`, `num_deaths`, `breed_in`, `breed_true`, `bred_true`, `num_cpus`, `num_parasites`, `threads`, `num_no_birth`, `tot_cpus`, `ave_repro_rate`, `ave_merit`, `ave_age`, `ave_memory`, `ave_neutral`, `ave_lineage`, `ave_gest`, `ave_fitness`, `ave_copy_length`, `ave_exe_length`, `ave_speculative`, `speculative_waste`, `max_fitness`, `max_merit`, `min_fitness`. A name that does not exist prints a line such as `Data manager unable to find entry 'ave_length'` and the column is left out.

## Tips

- Keep several variants of an experiment in one folder by giving each its own events file and choosing it with `-set EVENT_FILE events-variant.cfg`.
- To compare runs, schedule the same `Print...` events in each.
- An action that is not implemented stops with `unsupported:`; one that does not exist is an error when the file is read.
- Event-file constructors for a few actions exit with status 1 and the C++ message when their arguments are invalid (for example `SetEnvironmentInputs` with other than three inputs).
