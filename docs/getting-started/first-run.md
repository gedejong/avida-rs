# Your first run

In this page you copy the stock configuration, run 1,000 updates of it and read the result. A run of this size takes roughly 10 to 15 seconds on a laptop.

## What a run needs

Avida reads its settings from files in the *current directory*, so every experiment lives in a folder of its own. The stock set of files is in `avida-core/support/config` in the repository. The ones that matter first are:

| File | Purpose |
|---|---|
| `avida.cfg` | All settings: world size, mutation rates, scheduling and many more. See the [settings reference](../reference/settings.md). |
| `environment.cfg` | The tasks the organisms can be rewarded for. See the [environment file](../reference/environment-file.md). |
| `events.cfg` | What happens when: the first organism, the data files and the end of the run. See the [events file](../reference/events-file.md). |
| `instset-heads.cfg` | The instruction set the organisms use. See [instruction sets](../reference/instruction-sets.md). |
| `default-heads.org` | The first organism, the *ancestor*, as a list of instructions. |
| `analyze.cfg` | A script for [analyze mode](../reference/analyze-mode.md), which is not used in a normal run. |

## Steps

1. Make an experiment folder from the stock files. From the repository root:

   ```sh
   cp -r avida-core/support/config my-run
   cd my-run
   ```

   You should now be in a folder that holds `avida.cfg` and the other files above.

2. Shorten the run. The stock `events.cfg` ends the run at update 100,000, which takes hours. Open `events.cfg` and change the last line from `u 100000 Exit` to `u 1000 Exit`. The `Exit` action ends the run at the update named before it.

3. Run it with a fixed random seed, so that you can repeat the run exactly:

   ```sh
   /path/to/avida -s 5
   ```

   You should see a banner, the line `Random Seed: 5`, and then one progress line per update, ending with something like this:

   ```text
   UD: 1000    Gen: 77.33130   Fit: 0.2577421  Orgs: 3598
   ```

4. List the output:

   ```sh
   ls data
   ```

   You should see `average.dat`, `count.dat`, `dominant.dat`, `resource.dat`, `tasks.dat`, `time.dat` and population snapshots named `detail-0.spop`, `detail-500.spop` and `detail-1000.spop`.

## Reading the progress line

Each line reports one finished update:

| Field | Meaning |
|---|---|
| `UD` | The update number. One update is about 30 virtual CPU cycles per organism. See [the update cycle](../concepts/update-cycle.md). |
| `Gen` | The average generation of the organisms: how many replication steps lie between them and the first ancestor. |
| `Fit` | The average fitness, which is merit divided by gestation time. See [organisms and genomes](../concepts/organisms-and-genomes.md). |
| `Orgs` | The number of living organisms. The stock world is 60 by 60 cells, so it fills at 3,600. |

With seed 1 on the stock configuration, `Orgs` grows from 1 to 74 at update 100, 1,033 at update 300 and 3,108 at update 500, and the world is full (3,600 cells) around update 700. The first logic tasks (NOT, then NAND) appear in `tasks.dat` around updates 700 to 1,000, and with other seeds they appear earlier or later, so a run this short may still show zeros. The stock environment rewards nine logic functions; [Output files](output-files.md) shows how to read the counts.

## Useful variations

- A different random seed: `avida -s 6`. The same seed on the same build and configuration gives the same run.
- A smaller world: `avida -s 5 -set WORLD_X 30 -set WORLD_Y 30`. Any setting in `avida.cfg` can be overridden with `-set NAME VALUE`.
- A different output folder: `avida -set DATA_DIR run-a`.
- Quieter output: `avida -v0` drops the per-update progress lines. See [the command-line page](../front-ends/cli.md).
- Watch it live: `avida-tui .` in the same folder. See [the terminal interface](../front-ends/tui.md).

## If something goes wrong

- `error: configuration file 'avida.cfg' not found`: you are in the wrong folder. Avida looks for `avida.cfg` in the current directory unless you name another file with `-c`.
- A run that never seems to end: the stock `events.cfg` runs to update 100,000. Shorten it as in step 2, or stop the run with Ctrl-C.

## Next step

[Output files](output-files.md).
