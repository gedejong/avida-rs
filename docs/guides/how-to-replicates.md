# Run replicate seeds and several arms

An evolution run is one sample from a random process. To say anything about an effect you need several runs (*replicates*, each with its own random seed) of each variant (*arm*) of your experiment. This page shows a small shell recipe that the tutorials use. It needs only `avida` and a Unix shell.

## Layout

Work in a copy of the stock folder (see [your first run](../getting-started/first-run.md)). Each run gets its own data folder, `runs/ARM/seed-N`, and its own copy of the events file with the run length filled in.

## The recipe

Put this function in a file `lab.sh`, in the experiment folder, and load it with `. ./lab.sh`. Set `AVIDA` to the path of the binary.

```sh
AVIDA=${AVIDA:-avida}

# run_arm ARM SEED UPDATES [extra avida arguments...]
run_arm() {
  arm=$1; seed=$2; updates=$3; shift 3
  out=runs/$arm/seed-$seed
  mkdir -p "$out"
  # the run's own copy of events.cfg, with the Exit update replaced
  sed -E "s/^u [0-9]+ Exit/u $updates Exit/" events.cfg > "$out/events.cfg"
  "$AVIDA" -s "$seed" -set EVENT_FILE "$out/events.cfg" -set DATA_DIR "$out" "$@" > "$out/log.txt"
}
```

For the replacement to work, `events.cfg` must contain a line of the form `u 100000 Exit`, as the stock file does. The function was tested with two seeds and two arms: it produced the four run folders, each with its own `events.cfg` whose `Exit` line carried the chosen update.

## Using it

Two arms, four seeds each, 20,000 updates, one run after another:

```sh
. ./lab.sh
for s in 1 2 3 4; do
  run_arm control $s 20000
  run_arm variant $s 20000 -set ENVIRONMENT_FILE environment-variant.cfg
done
```

Everything after the three fixed arguments goes to `avida`, so an arm can differ in any setting (`-set NAME VALUE`), in its environment file or events file, or in the instruction set (`-def INST_SET file`).

Run several at once, one per core, with `xargs`:

```sh
export -f run_arm; export AVIDA
printf '%s\n' 1 2 3 4 5 6 7 8 | xargs -P 4 -I{} bash -c '. ./lab.sh; run_arm control {} 20000'
```

(Adjust `-P 4` to the number of cores you can spare. The exact mode runs each population on one thread, so one run per core is the efficient way to use a machine. If you use the [parallel mode](../execution/index.md), give each run `-set EXECUTION_THREADS 1`.)

## Reading the results

Each run's `tasks.dat`, `average.dat` and `count.dat` have the same layout, so a few lines of any scripting language compare arms. This `awk` command prints the last row of `tasks.dat` for every run:

```sh
for f in runs/*/seed-*/tasks.dat; do echo "$f: $(grep -v '^#' "$f" | tail -1)"; done
```

To see which task columns exist in what order, read the legend at the top of `tasks.dat`. With few seeds, compare the spread across seeds, not only the average: if the arms overlap, you cannot tell them apart.

## Timing

The maintainer's measurement for a full 60 by 60 world with eight runs sharing a machine: about 25 ms per update, so 20,000 updates take 8 to 9 minutes. Your machine will differ; do a 1,000-update run first and extrapolate.

## Starting again

Delete a run's folder to redo it. A function like the one above overwrites files in a folder that already exists, so start from a clean folder when you change an arm.
