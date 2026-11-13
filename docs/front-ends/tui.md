# The terminal interface: `avida-tui`

`avida-tui` shows a running experiment inside a terminal: a map of the world, statistics, charts, the genotypes, an inspector for single organisms, an organism debugger, the environment, a log and the configuration. It needs a terminal that is a real terminal (not a pipe) and, for colour, a terminal that supports colour; it falls back to fewer colours or none.

## Start it

```sh
avida-tui [OPTIONS] [CONFIG_DIR] [AVIDA_ARGS...]
```

`CONFIG_DIR` is the experiment folder (the one with `avida.cfg`, `events.cfg` and the environment file). It defaults to the current directory. Everything after it goes to the `avida` command line, for example:

```sh
avida-tui my-run -s 7 -set WORLD_X 30
```

You should see the map of an empty world with a tab strip along the top. The run starts paused. Press Space to start it.

Options, which must come before `CONFIG_DIR`:

| Option | Meaning |
|---|---|
| `--colors MODE` | `auto` (default), `truecolor`, `256`, `16` or `none`. The environment variable `NO_COLOR` also turns colour off; this option wins over it. |
| `--theme NAME` | `auto` (default), `dark` or `light`: the terminal background the colours are chosen for. `auto` reads `AVIDA_TUI_THEME`, then `COLORFGBG`, and assumes dark. |
| `--help` | Show the help text. |
| `--version` | Show the version. |

Colour is never the only carrier of information: the map layers have legends, and a mode without colour (`--colors none`) still shows every screen.

## The screens

Each screen has a letter key. The tab strip shows them in this order, and `Tab` and `Shift-Tab` step through it.

| Key | Screen | What it shows |
|---|---|---|
| `M` | Map | The world as a grid of cells coloured by a chosen layer. |
| `S` | Stats | The numbers of the current update. |
| `G` | Charts | Time series of the statistics. |
| `T` | Genotypes | A table of genotypes, an abundance chart, or the lineage of the selected genotype. |
| `Z` | Zoom | The organism inspector: phenotype and CPU state of the selected organism. |
| `U` | Debug | The organism debugger: registers, heads, stacks and a memory listing with breakpoints. |
| `E` | Env | The environment: tasks, reactions and resources. |
| `H` | Hist | Histograms. |
| `D` | Log | The log of messages and warnings. |
| `F` | Cfg | The effective configuration, with overrides and restart. |
| `O` | Opts | Options of the viewer. |
| `B` | Blank | A blank screen with only the status line. |

`Esc` returns to the map, or deselects.

## Keys

The program's key table is the single source for the help overlay (`?`), the command palette (`:` or `Ctrl-P`) and the key handling, so this list matches what the program does. Letters work in either case, except where a row says otherwise.

### Run

| Key | Action |
|---|---|
| `Space`, `P` | Run or pause |
| `.`, `N` | Step one update |
| `,` | Step one time slice |
| `+`, `=` | Faster |
| `-`, `_` | Slower |
| `W` | Save the population |
| `X` | Extract the inspected genome to a file |
| `R` | Restart the run, with any configuration overrides made on the Cfg screen |

### Move and select

| Key | Action |
|---|---|
| Arrow keys, `h` `j` `k` `l` | Move the cursor |
| `PgUp`, `PgDn` | Page up and down |
| `Home`, `End` | Top and bottom (the log follows with `End`) |
| `Enter` | Inspect the cell, or pick the row |
| `C` | Inspect the next organism |

### Map

| Key | Action |
|---|---|
| `[` and `]` | Previous and next layer |
| `p` | Switch the colour palette |
| `L` | Switch between a linear and a logarithmic scale |
| `i`, `I` | Wide cells (symbols) |

### Screen-specific keys

| Screen | Key | Action |
|---|---|---|
| Zoom, Debug | `t` | Show the next thread |
| Debug | `i` | Step the watched organism one instruction |
| Debug | `b`, `Enter` | Set or clear a breakpoint on the selected line |
| Debug | `g` | Run to the next breakpoint |
| Debug | `f` | Follow offspring on or off |
| Debug | `Delete` | Clear all breakpoints |
| Debug | `Home` | Select the instruction pointer and follow it |
| Debug | `y` | Show the next memory space |
| Genotypes | `v` | Switch between table, abundance chart and lineage |
| Genotypes | `l` | Trace the lineage of the selected genotype |
| Log | `v` | Show only warnings and errors, or everything |
| Cfg | `/` | Filter the settings |
| Cfg | `e`, `Enter` | Edit the selected setting |
| Cfg | `c` | Show only changed settings |
| Cfg | `Delete` | Drop the override of the selected setting |

### Other

| Key | Action |
|---|---|
| `:` or `Ctrl-P` | Command palette |
| `?` or `F1` | Help |
| `q` | Quit (asks first) |
| `Q` or `Ctrl-C` | Quit now |
| `Ctrl-L` | Redraw the screen |
| `A` | Analyze mode: not part of the live viewer; the program answers with a message that tells you to run `avida` with an analyze file |

## Map layers

The map colours each cell by one layer. A layer is one number per cell: the occupant's value, or, for resources, the amount in the cell. The layers, in display order:

| Layer | What it shows |
|---|---|
| Genotype | Which genotype the organism has. |
| Fitness | Merit divided by gestation time. |
| Merit | The organism's share of CPU time. |
| Gestation time | CPU cycles the organism needed for its last offspring. |
| Age | Updates lived. |
| Generation | Ancestors between the first organism and this one. |
| Genome length | Instructions in the genome. |
| Lineage label | The label inherited from the injected ancestor. |
| Breeds true | 1 when the organism is an exact copy of its parent. |
| Modified | 1 when the organism's phenotype was modified. |
| Threads | CPU threads the organism runs. |
| Tasks performed | Distinct tasks in the last gestation. |
| Ancestry | The family the organism belongs to. The family tree of the living population is cut into at most seven families, one colour each; a branch counts as a family only when it and the rest each hold at least 2% of the population. The legend lists each family by the genotype at its root, with its number of organisms. |
| One layer per task | How often the organism performed that task in its last gestation. |
| One layer per resource | The amount of the resource in each cell. |

Empty cells have no value in the per-organism layers.

## Speed and the parallel mode

`+` and `-` change the pace. In the [parallel execution mode](../execution/index.md), the debugger steps whole updates, and the speculative statistics of the status line read zero.

## Terminal behaviour

The interface restores the terminal on quit, on `SIGINT`, `SIGTERM` and on hang-up (tests exercise these with a pseudo-terminal). If the terminal is not interactive (standard input or output is a pipe), it prints a message and exits with status 1.

## Related

- The same views in a window: [desktop application](gui.md).
- Quantities on the map: [organisms and genomes](../concepts/organisms-and-genomes.md).
