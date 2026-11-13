# Output files

A run writes its files into the data directory, `data` by default. Change it with the setting `DATA_DIR` (for example `-set DATA_DIR run-a`). Which files appear is decided by the [events file](../reference/events-file.md): each `Print...` action writes one kind of file, and the stock `events.cfg` schedules six of them every 100 updates plus a population snapshot every 500.

The column lists below were read from the headers of files written by a real run of the stock configuration.

## The shape of a data file

All data files are plain text with columns separated by spaces. The first lines start with `#` and form the header: the file's title, the date, and a numbered legend of the columns. The data rows follow, one per time the event fired.

To read column 3 of `count.dat` (the number of organisms) without any special tools:

```sh
awk '!/^#/ && NF { print $1, $3 }' data/count.dat
```

You should see one line per recorded update: the update number and the organism count. The same works in Python, R or a spreadsheet, as long as you skip the `#` lines.

## The six files of the stock events file

| File | Written by | Columns (legend numbers) |
|---|---|---|
| `average.dat` | `PrintAverageData` | 1 update, 2 merit, 3 gestation time, 4 fitness, 5 repro rate, 6 deprecated size, 7 copied size, 8 executed size, 9 deprecated abundance, 10 proportion that gave birth this update, 11 proportion of breed-true organisms, 12 deprecated genotype depth, 13 generation, 14 neutral metric, 15 lineage label, 16 true replication rate. These are averages over the living organisms. |
| `dominant.dat` | `PrintDominantData` | The same kinds of numbers for the most abundant genotype, followed by 9 abundance, 10 births, 11 breed-true count, 12 gene depth, 13 breed-in, 14 max fitness, 15 genotype id and 16 genotype name. |
| `count.dat` | `PrintCountData` | 1 update, 2 instructions executed this update, 3 organisms, 4 genotypes, 5 threshold genotypes, 6 to 8 deprecated, 9 births this update, 10 deaths this update, 11 breed true, 12 breed-true organisms, 13 organisms that gave no birth, 14 single-threaded organisms, 15 multi-threaded organisms, 16 modified organisms. |
| `tasks.dat` | `PrintTasksData` | 1 update, then one column per task in the order of the reactions in `environment.cfg` (in the stock file: Not, Nand, And, OrNot, Or, AndNot, Nor, Xor, Equals). Each value is the number of organisms that have the task as a component of their merit. |
| `time.dat` | `PrintTimeData` | 1 update, 2 Avida time, 3 average generation, 4 number executed. |
| `resource.dat` | `PrintResourceData` | 1 update, then the amount of each resource. The stock environment has no named resources, so only the update column appears. |

## Population snapshots: `detail-N.spop`

`SavePopulation` writes the whole population at update N to `detail-N.spop` (the stock file schedules it at update 0 and every 500 updates). This is the file you load into [analyze mode](../reference/analyze-mode.md) or into a later run with the `LoadPopulation` action.

The file starts with `#filetype genotype_data`, a `#format` line that names the columns, and a numbered legend. Each data row describes one genotype:

| Column | Meaning |
|---|---|
| id, src, src_args | The genotype's id, how it arose and its arguments (`div:ext` is an injected first organism, `div:int` is a normal offspring). |
| parents | The id of the parent genotype. |
| num_units, total_units | How many organisms of this genotype are alive now, and how many ever lived. |
| length, merit, gest_time, fitness | The genotype's measured properties. |
| gen_born, update_born, update_deactivated, depth | When it arose and, once no organism has it, when it went extinct (`-1` while it lives); `depth` is its distance from the root of the tree of genotypes. |
| hw_type, inst_set | The hardware type and the name of the instruction set. |
| sequence | The genome, one letter per instruction. |
| cells, gest_offset, lineage | The cells occupied, gestation offsets and the lineage label. |

In `sequence`, each letter stands for one instruction, in the order of the instruction set file. With the stock heads instruction set, `a` is `nop-A`, `b` is `nop-B`, `c` is `nop-C`, and so on through `z`, which is `h-search`. The ancestor therefore begins `wzcag...`: `h-alloc`, `h-search`, `nop-C`, `nop-A`, `mov-head`. See [instruction sets](../reference/instruction-sets.md).

## Other output

Many more files exist, written by other `Print...` actions (instruction frequencies, genotype abundance histograms, deme statistics, parasite and host data, grid dumps). The [actions reference](../reference/actions.md) lists them all with their default file names.

Analyze mode writes its files into the same data directory, or wherever the script names them, with a different structure; see [analyze mode](../reference/analyze-mode.md).

## Plotting

Any plotting tool that reads whitespace-separated columns works. With Python and matplotlib, for instance:

```python
import matplotlib.pyplot as plt

rows = [line.split() for line in open("data/count.dat")
        if line.strip() and not line.startswith("#")]
plt.plot([float(r[0]) for r in rows], [float(r[2]) for r in rows])
plt.xlabel("update")
plt.ylabel("organisms")
plt.show()
```

The desktop application and the terminal interface draw the same quantities live, and the desktop application can export them as CSV or PNG. See [the front ends](../front-ends/index.md).

## Next step

Now that you have a run and its files, try [a tutorial](../guides/index.md) that changes the world and compares the outcome, or read about [the ideas behind the numbers](../concepts/index.md).
