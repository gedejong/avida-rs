# Events reference (`events`)

The events say what happens and when: which organism starts the run, which data files are written, when the environment changes, and when the run ends. They are the section `events` of the experiment file (`avida.yaml`), or a file of their own that the section names (`events: events-long.yaml`). The command line can name another events file: `-set events events-long.yaml`.

Every rule below was checked against the event-list parser in the `avida-world` crate (`crates/world/src/event_list.rs`) or by a short run. The program turns each entry into a line of the C++ events file (`u 0:100:end PrintAverageData`) and reads that line as the C++ did.

## Entry format

`events` is a list. Each entry is a mapping with these keys:

| Key | Meaning |
|---|---|
| `trigger` | What the timing counts (see below); `update` when left out. |
| `at` | When the event first fires: `begin` (the very start of the run, the default) or a number. |
| `every` | How often it repeats afterwards: a number, `all` (every possible tick) or `once` (do not repeat, the default). |
| `until` | The last time it may fire: a number, or `end` for no stop (the default). `until` needs `every`. |
| `action` | The action. Its name is not case-sensitive (`PrintAverageData` and `printaveragedata` write the same file). |
| `args` | The action's arguments, as one text (`args: default-heads.org 12`) or as a list of words. Each action defines its own; see the [actions reference](actions.md) or run `avida -e`. |

Comments start with `#`.

### Triggers

| `trigger` | Letter in the C++ file | The timing counts |
|---|---|---|
| `update` | `u` | Updates. |
| `generation` | `g` | The average generation of the population. |
| `immediate` | `i` | Nothing: an immediate event runs once, at the very start of the run, and takes no timing. |
| `births` | `b` | Births. |
| `org_id` | `o` | Organism ids (an interrupt triggered at a birth); rarely used. |

Anything else is an error that names the triggers.

### Timing

Examples:

| Entry | Meaning |
|---|---|
| `{at: begin, action: Inject, args: default-heads.org}` | Inject the ancestor at the start of the run. |
| `{at: 100, action: Exit}` | Stop the run at update 100. |
| `{at: 0, every: 100, until: end, action: PrintAverageData}` | Write the averages at update 0 and every 100 updates, until the end. |
| `{at: 100, every: 100, until: 5000, action: PrintCountData}` | At update 100 and every 100 after, until update 5000. |
| `{trigger: generation, at: 10, every: 10, action: SavePopulation}` | Every 10 generations, from generation 10. |
| `{at: 1, every: 1, until: end, action: ReplicateDemes, args: full_deme}` | Every update from update 1. |
| `{trigger: immediate, action: Echo, args: hello}` | Print a message at the very start of the run. |

An event with an interval of 100 that starts at 0 fires at the start of the run and then at updates 100, 200 and so on. Events fire at the boundary between updates, before the next update's time slices: see [the update cycle](../concepts/update-cycle.md).

## The stock events

```yaml
events:
  - {at: begin, action: Inject, args: default-heads.org}
  - {at: 0, every: 100, until: end, action: PrintAverageData}
  - {at: 0, every: 100, until: end, action: PrintDominantData}
  - {at: 0, every: 100, until: end, action: PrintCountData}
  - {at: 0, every: 100, until: end, action: PrintTasksData}
  - {at: 0, every: 100, until: end, action: PrintTimeData}
  - {at: 0, every: 100, until: end, action: PrintResourceData}
  - {at: 0, every: 500, until: end, action: SavePopulation}
  - {at: 100000, action: Exit}
```

What each entry does:

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
| `ChangeEnvironment` | `env_string` | Load one line of the C++ environment-file syntax during the run, for example `RESOURCE Food:inflow=5` (the [environment reference](environment-file.md) gives the keys), and update the resources. |
| `KillProb` | `[probability=0.9]` | Kill each organism with a probability. |
| `Echo` | `message` | Print a message. |
| `SetConfig` | `config_var value` | Change a setting during the run, named by its C++ name (`COPY_MUT_PROB`; the [experiment file reference](config-yaml.md#settings-by-group) lists both names). Whether a setting takes effect mid-run depends on the setting; `EXECUTION_MODE` does not. |

### `PrintData`

`PrintData fname column,column,...` writes a file with the columns you choose, in the order you give them:

```yaml
events:
  - {at: 0, every: 50, until: end, action: PrintData, args: 'mydata.dat update,ave_fitness,ave_gest,ave_merit,generation,num_cpus'}
```

The names are case-sensitive. These are the ones that exist: `update`, `generation`, `num_resamplings`, `num_failedResamplings`, `num_births`, `cumulative_births`, `num_deaths`, `breed_in`, `breed_true`, `bred_true`, `num_cpus`, `num_parasites`, `threads`, `num_no_birth`, `tot_cpus`, `ave_repro_rate`, `ave_merit`, `ave_age`, `ave_memory`, `ave_neutral`, `ave_lineage`, `ave_gest`, `ave_fitness`, `ave_copy_length`, `ave_exe_length`, `ave_speculative`, `speculative_waste`, `max_fitness`, `max_merit`, `min_fitness`. A name that does not exist prints a line such as `Data manager unable to find entry 'ave_length'` and the column is left out.

## Tips

- Keep several variants of an experiment in one folder by giving each its own events file (a file whose only key is `events`) and choosing it with `-set events events-variant.yaml`.
- To compare runs, schedule the same `Print...` events in each.
- An action that is not implemented stops with `unsupported:`; one that does not exist is an error when the events are read.
- The constructors of a few actions exit with status 1 and the C++ message when their arguments are invalid (for example `SetEnvironmentInputs` with other than three inputs).
