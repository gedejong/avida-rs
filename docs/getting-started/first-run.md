# Your first run

In this page you copy the stock configuration, run 1,000 updates of it and read the result. A run of this size takes roughly 10 to 15 seconds on a laptop.

## What a run needs

Avida reads its experiment from the *current directory*, so every experiment lives in a folder of its own. The stock folder is `avida-core/support/config` in the repository. The files that matter first are:

| File | Purpose |
|---|---|
| `avida.yaml` | The experiment file. It holds the settings in groups (world size, mutation rates, scheduling and many more), the instruction set the organisms use, the environment (the tasks the organisms can be rewarded for) and the events (what happens when: the first organism, the data files and the end of the run). See [the experiment file](../reference/config-yaml.md), and the pages on the [environment](../reference/environment-file.md), the [events](../reference/events-file.md) and [instruction sets](../reference/instruction-sets.md). |
| `default-heads.org` | The first organism, the *ancestor*, as a list of instructions. |
| `analyze.cfg` | A script for [analyze mode](../reference/analyze-mode.md), which is not used in a normal run. |

The other `instset-*.yaml` and `environment-*.yaml` files of the folder are sets and environments an experiment can name instead of its own.

## Steps

1. Make an experiment folder from the stock files. From the repository root:

   ```sh
   cp -r avida-core/support/config my-run
   cd my-run
   ```

   You should now be in a folder that holds `avida.yaml` and the other files above.

2. Shorten the run. The stock events end the run at update 100,000, which takes hours. Open `avida.yaml`, find the last entry of `events` at the end of the file, and change it from `{at: 100000, action: Exit}` to `{at: 1000, action: Exit}`. The `Exit` action ends the run at the update named in `at`.

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
- A smaller world: `avida -s 5 -set world.x 30 -set world.y 30`. Any setting of the experiment file can be overridden with `-set GROUP.KEY VALUE`.
- A different output folder: `avida -set general.data_dir run-a`.
- Quieter output: `avida -v0` drops the per-update progress lines. See [the command-line page](../front-ends/cli.md).
- Watch it live: `avida-tui .` in the same folder. See [the terminal interface](../front-ends/tui.md).

## If something goes wrong

- `error: configuration file 'avida.yaml' not found`: you are in the wrong folder. Avida looks for `avida.yaml` in the current directory unless you name another file with `-c`.
- `'avida.cfg' is in the old configuration format`: the folder was made for an older version of the program. Run `avida convert .` in it once; see [converting an old folder](../reference/config-yaml.md#converting-an-old-folder).
- A run that never seems to end: the stock events run to update 100,000. Shorten them as in step 2, or stop the run with Ctrl-C.

## Next step

[Output files](output-files.md).
