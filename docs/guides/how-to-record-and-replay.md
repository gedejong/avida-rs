# Record an experiment and play it back

A long run on a large world is hard to watch while it happens, and it cannot be rewound. A *recording* fixes that: the run saves a snapshot of the whole arena every few updates into one file, and `avida-gui` or `avida-tui` plays the file back later, on any machine, at any speed, forwards and backwards.

A playback shows the map with every recorded layer (genotype, ancestry families, fitness, merit, age, one layer per task and per resource, and so on), the charts, the statistics, the environment and the genotype table, each as it was at that update. It does not run the simulation: there is nothing to inspect inside an organism, no debugger, and no settings to change (see [What a playback cannot do](#what-a-playback-cannot-do)).

## Build the programs

From `rust/avida-native`:

```sh
cargo build --release -p avida-session   # avida-rec, the headless recorder
cargo build --release -p avida-tui       # the terminal interface
cargo build --release -p avida-gui       # the desktop application
```

The programs are in `target/release/`.

## Record without a window

`avida-rec` runs an experiment at full speed without a user interface and writes the recording. It takes what `avida` takes: a configuration folder (default: the current folder), `-s`, `-set`, `-def`. It computes exactly what `avida` computes, so the data files of a recorded run are the ones an unrecorded run writes.

```sh
cp -R avida-core/support/config my-run
avida-rec --record run.avrec --record-every 50 my-run -s 42
```

The stock `events.cfg` ends the run at update 100000. For a first try, copy it to `events-short.cfg`, change that line to `u 2000 Exit`, and add `-set EVENT_FILE events-short.cfg`. A line like this appears every ten seconds on standard error:

```text
update 1450  organisms 3591  frames 29  file 1.7 MiB  recording 0.3 s
```

The last field is the time the run has spent capturing and writing frames. When the run ends, the program prints the totals and exits with the run's own exit status.

For a big world in the parallel execution mode (see [Execution modes](../execution/index.md)), pass the settings as for `avida`:

```sh
avida-rec --record big.avrec --record-every 100 my-run -s 42 \
    -set WORLD_X 160 -set WORLD_Y 160 \
    -set EXECUTION_MODE 1 -set EXECUTION_THREADS 4
```

`Ctrl-C` (SIGINT) or SIGTERM stops the run at the end of the update in progress, writes that update as a last frame and leaves a complete file; the exit status is 128 plus the signal number (130 and 143). A run that is killed outright (SIGKILL, a power cut) leaves a file that is readable up to its last complete frame.

### Options

All options go before the configuration folder.

| Option | Meaning |
|---|---|
| `--record FILE` | The file to write (required). An existing file is replaced. Use the extension `.avrec`: the desktop application opens a dropped `.avrec` file. |
| `--record-every N` | Capture a frame whenever an update ends at a multiple of `N` updates. Default 100. The final update of a run that finishes is always captured. |
| `--record-layers LIST` | Comma-separated map layers to record, for example `genotype,fitness,ancestry,task[0],resource[1]`. Default: every layer the world has. The genotype layer is always recorded. The names are the ones the viewers use: `genotype`, `fitness`, `merit`, `gestation`, `age`, `generation`, `genome_len`, `lineage`, `ancestry`, `breed_true`, `modified`, `threads`, `task_count`, `task[K]`, `resource[R]`. |
| `--record-top N` | Genotypes in the table of each frame, most abundant first. Default 100. |
| `--progress SECS` | Seconds between progress lines (default 10; 0 for none). |
| `--verbose` | Also print the per-update `UD:` lines. |

## Record from a viewer

Both viewers take the same recording options, so you can record a run you are watching:

```sh
avida-tui --record run.avrec --record-every 50 my-run -s 42
avida-gui --record run.avrec --record-every 50 my-run -s 42
```

The viewer records at the interval you gave however you pause, step or throttle the run. Frames are captured between updates only, so pausing inside an update does not add a frame.

## Play it back

```sh
avida-tui --play run.avrec
avida-gui --play run.avrec
```

Dropping a `.avrec` file on the `avida-gui` window opens it too. A playback that cannot be opened says why: the file is not a recording, it has a version this program does not read, or it holds no complete frame yet.

### In the terminal interface

The header shows `PLAYBACK` (`PLAYING` while it plays), the update on show and the last recorded one, and the frame number. The bottom line is a position bar with the keys.

| Key | Does |
|---|---|
| `Space` | Play from here; pause. At the end of the recording `Space` plays it again from the start. |
| `.` and `,` | One frame forward and back. |
| `}` and `{` | Ten frames forward and back. |
| `>` and `<` | The last and the first frame. |
| `+` and `-` | Faster and slower. The speed is in updates of the recorded run per second, so with frames 50 updates apart a speed of 100 shows two frames a second. Max shows about 30 frames a second. |
| `[` `]`, `M` `S` `G` `T` `E` `H` `D` | The map layer, and the screens, as in a live run. |

### In the desktop application

A bar under the top bar shows `Playback`, the update on show and the last one, a slider over all frames and the frame number. The transport buttons work on frames: **Play** and **Pause**; the two step buttons go to the next and the previous frame (`.` and `,`); the field *Seek to…* takes an update number and shows the recorded frame at or before it. The speed menu sets updates of the recorded run per second, as above.

### Charts and tables

The charts and the statistics are drawn from the statistics row of each recorded frame, so a chart has one point per frame, not one per update. After a seek backwards the chart shows the frames up to the one on show. The genotype table is the one captured with the frame: the most abundant genotypes at that update.

## What a playback cannot do

A recording is a series of pictures, not a world that can be run again.

- No organism inspector, CPU view or debugger: organisms are not recorded. The *Organism* tab and the zoom and debug screens say so.
- No settings: the *Config* tab says so (in the terminal interface it also lists the `avida` arguments the run was started with). Restart and Save population are refused with a message. Exporting the map, the genotype table and the statistics works.
- Only the recorded layers are on offer. A layer left out with `--record-layers` is not in the layer menu.
- A genotype's lineage (the ancestor chain of the genotype table) is not recorded. The ancestry layer is: it colours the organisms by family.
- The statistics are those of the frames, not of every update.

## A recording that is still being written

Opening a file that a run is still writing works: the player reads the complete frames that are there (the desktop application's bar says *cut at the end* when the file ends inside a record). Pressing *Run* or *next frame* at the last frame looks for frames added since; there is no live follow mode.

## How big a recording gets

Each frame is compressed on its own (zlib), and a frame holds one number per cell per layer plus the tables. Measured with every layer (13 per-organism layers, one per task, resources) on the stock configuration, seed 42, a frame every 50 updates:

| World | Updates | Frames | File | Per frame | Per cell |
|---|---|---|---|---|---|
| 60 x 60, exact mode | 3000 | 61 | 2.9 MiB | 49 KB | 13.5 bytes |
| 120 x 120, parallel mode | 1500 | 31 | 3.3 MiB | 109 KB | 7.6 bytes |
| 160 x 160, parallel mode | 1500 | 31 | 4.3 MiB | 142 KB | 5.5 bytes |

The cost per cell is lowest while the world is still filling and rises with the number of different genotypes, because the genotype and ancestry layers have one distinct value per genotype. The 60 x 60 run is a full world; its 13.5 bytes per cell is a better guide to a long run than the two larger ones. For a 160 x 160 world with every layer, a frame every 50 updates for 300,000 updates (6000 frames) then takes about 2 GB at the high end. To make a recording smaller, record less often (`--record-every 200` quarters it), or fewer layers, or both. Fewer layers help less than one would think: on a full 60 x 60 world (1500 updates) every layer took 1.2 MiB, `--record-layers genotype,ancestry,fitness,age` 0.8 MiB and `--record-layers genotype,ancestry` 0.5 MiB, because the genotype and ancestry layers and the genotype table are most of a frame.

Capturing and writing a frame cost between 10 and 35 ms in these runs (a few per cent of the run time), so a frame every 50 updates does not slow a run noticeably.

## The file format

The format is documented in `rust/avida-native/ARCHITECTURE.md` (section "Recording and playback") and in the module documentation of `crates/session/src/recording.rs`. It starts with the text `AVIDAREC` and a version number; a program that does not know the version refuses the file.
