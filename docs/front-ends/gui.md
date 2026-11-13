# The desktop application: `avida-gui`

`avida-gui` is a desktop window (built with egui) that runs an experiment and shows it: a map of the world, six side tabs (statistics, charts, genotypes, one organism, the environment, the configuration), a debugger for a single organism, and exports of images and tables.

## Start it

```sh
avida-gui [CONFIG_DIR] [AVIDA_ARGS...]
```

`CONFIG_DIR` is the experiment folder that holds `avida.cfg`, `events.cfg` and the environment file. `AVIDA_ARGS` go to the world as the `avida` program takes them, for example `-s 7 -set WORLD_X 30`. They apply to every experiment you open in that run of the program. Without a folder, the start screen offers a folder picker and the list of recently opened experiments.

Logs go to standard error: warnings by default, more with `RUST_LOG=info` or `RUST_LOG=debug`.

On macOS, start it from the Finder, or through the application bundle described in [Install and build](../getting-started/install.md), not from inside `tmux` or over `ssh`. A bare binary outside the desktop session cannot take the focus. The program warns about it and floats its window above the others until you click it; `AVIDA_GUI_SKIP_SESSION_CHECK=1` skips the check.

## New experiment

The experiment menu at the left of the top bar (and the command palette) holds New experiment, Open experiment, Open recent, Save population, the exports, Restart and Close. **New experiment** (`Cmd N` on macOS, `Ctrl N` elsewhere) creates a folder from Avida's default configuration. The stock files are compiled into the program, so this needs nothing on disk: the configuration, events and environment files, every top-level instruction set and the matching ancestors. If you pick a folder that is not empty, the application makes a new `avida-experiment` folder inside it (`avida-experiment-2` if that exists). The alternative environments and instruction sets under `misc/` in the repository are not bundled.

## The window

- **Top bar**: the experiment menu, the transport controls (run and pause, step), the speed, the run state and a theme button.
- **Map**: the world, one square per cell, coloured by the chosen layer. Click a cell to select it and inspect its organism. Scroll or pinch to zoom, drag to pan, double-click to fit the world to the view. The arrow keys also pan, `+` and `-` zoom. A minimap and zoom buttons sit in the corners.
- **Legend**: what the colours mean for the current layer, with a button to lock its range so that colours stay comparable over time.
- **Side panel**, with six tabs:

| Tab | Content |
|---|---|
| Stats | The numbers of the current update. |
| Charts | Three views: Time series, Tasks, Histogram. |
| Genotypes | The genotypes by abundance. You can highlight one on the map, inspect the dominant one, or trace a lineage. |
| Organism | The selected organism's phenotype, CPU state and, in the debugger, its registers, heads, stacks and memory. |
| Environment | The tasks, reactions and resources. |
| Config | The effective configuration, with overrides, and restart. |

## Speed

Speed is either as fast as possible, or limited to a number of updates per second between 0.5 and 2,000. The speed control is in the top bar and the command palette lists presets.

## Map layers

The layers are the ones of the [terminal interface](tui.md#map-layers): genotype, fitness, merit, gestation time, age, generation, genome length, lineage label, breeds true, modified, threads, tasks performed, ancestry, plus one layer per task and one per resource. The genotype layer gives the seven most abundant genotypes their own colour and the rest grey.

You can change how the map is drawn:

- **Palette**: Viridis (dark purple to yellow) or Cividis (dark blue to yellow) for continuous layers. The genotype layer always uses the categorical Okabe-Ito palette.
- **Scale**: linear or logarithmic. Fitness and merit start logarithmic, because they span orders of magnitude once tasks are rewarded.
- **Colour vision**: the map and legend can be shown as a person with protanopia, deuteranopia, tritanopia or achromatopsia sees them, to check that a figure is readable.
- **Theme**: follow the system, light or dark.

## Keyboard

| Keys | Action |
|---|---|
| `Cmd K` / `Ctrl K` | Open the command palette |
| `Cmd O` / `Ctrl O` | Open an experiment |
| `Cmd N` / `Ctrl N` | New experiment |
| `Cmd S` / `Ctrl S` | Save the population |
| `Space` | Run or pause |
| `.` | Step one update |
| `,` | Step one time slice |
| `F` | Fit the map to the view |
| `I` | Step the inspected organism one instruction |
| `B` | Run to the next breakpoint |
| `1` to `9` | Choose the first nine map layers |
| `Esc` | Close the shortcut list, or clear the selection and highlight |
| `?` | Show the shortcut list |

The command palette lists every action by name with a fuzzy search, and shows each action's shortcut. Besides the actions above it can: run to a chosen update, set the speed, choose palettes and colour vision, lock the legend, highlight or inspect the dominant genotype, switch tabs and chart views, switch the theme, open recent experiments, restart or close the experiment, copy the log, and export.

## Exports

- **Export map as PNG**: the current map.
- **Export statistics as CSV**: the recorded history of the run's statistics.
- **Save the charts as PNG**.
- **Export the genotype table as CSV**.
- **Copy the log** to the clipboard.

## Not available here

- Analyze mode runs from the command line (`avida -a`), not from the desktop application.
- A single long simulation step cannot be interrupted. When you close the window in the middle of one, the application waits 2 seconds and then detaches the simulation thread.

## Related

- The same views in a terminal: [terminal interface](tui.md).
- What the numbers mean: [organisms and genomes](../concepts/organisms-and-genomes.md) and [the update cycle](../concepts/update-cycle.md).
