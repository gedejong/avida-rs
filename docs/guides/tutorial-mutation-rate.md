# Tutorial: mutation rate and the error threshold

**Question.** Too little mutation adapts slowly; too much destroys the genome faster than selection can repair it. Where is the *error threshold*, the rate above which the population can no longer keep its genome?

**Arms.** One arm per value of the copy mutation rate, `COPY_MUT_PROB`.

## Steps

1. Use the stock folder with the [replicate recipe](how-to-replicates.md). Nothing needs editing: the rate is set on the command line.
2. Run a sweep. The maintainer's default is eight rates and 2 seeds each, 20,000 updates (about 15 minutes):

   ```sh
   . ./lab.sh
   for m in 0.0005 0.002 0.0075 0.02 0.04 0.07 0.1 0.15; do
     for s in 1 2; do
       run_arm copy-mut-$m $s 20000 -set COPY_MUT_PROB $m
     done
   done
   ```

   You should see one folder per rate and seed under `runs/`. The stock rate is 0.0075.

3. For each run, read the last row of `average.dat` (column 4 is fitness) and `count.dat` (column 3 is the number of organisms). A run that goes extinct stops having organisms; its `count.dat` shows 0.
4. To find the threshold more precisely, run again with rates between the last surviving one and the first failing one, for example `0.05 0.06 0.07`.

## What to expect

In the pilot (one seed, 15,000 updates), fitness peaked at a rate of 0.002 (8,393) and fell on both sides: 821 at 0.0005, 3,643 at the default 0.0075 and 2.1 at 0.04. At 0.07 the population survived without doing any task. At 0.1 and 0.15 it went extinct within 2,000 updates.

With a genome of about 100 instructions, a rate of 0.0075 gives 0.75 substitutions per copy; at 0.07 it is 7. See [mutations](../concepts/mutations.md) for the other kinds of mutation you can vary the same way.
