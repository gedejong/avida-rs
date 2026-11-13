# Tutorial: streamlining

**Question.** Can selection drive a genome toward the shortest program that still does the rewarded logic?

Normally Avida makes merit proportional to genome length, which cancels the advantage a short genome has in copying time. With `BASE_MERIT_METHOD 0` the base merit is a constant (`BASE_CONST_MERIT`, stock 100): every instruction that is not needed costs time, while the task rewards still multiply merit.

**Arms.**

| Arm | Setting |
|---|---|
| `size-neutral` | Stock: merit proportional to length (the control). |
| `streamlined` | `BASE_MERIT_METHOD 0` from the first update. |
| `evolve-then-streamline` | Stock until update 30,000, then constant merit. |

## Steps

1. Start from the stock folder and the [replicate recipe](how-to-replicates.md). All arms add slip mutations on divide (`DIVIDE_SLIP_PROB 0.05`), which cut out or double a stretch of the genome so that length can change by many instructions at once. With `0` length changes by at most one instruction per divide, and each deletion saves only about 1% of the copying time.
2. For the third arm make `events-switch.cfg`: the stock events with one added line:

   ```text
   u 30000 SetConfig BASE_MERIT_METHOD 0
   ```

3. Add the line `u 0:10000:end PrintDominantGenotype` to the events files. It writes the dominant genome every 10,000 updates so you can read the program as it shrinks.
4. Run the arms for as long as you can wait (the maintainer let them run until stopped, with 2 seeds per arm):

   ```sh
   . ./lab.sh
   for s in 1 2; do
     run_arm size-neutral $s 200000 -set DIVIDE_SLIP_PROB 0.05
     run_arm streamlined $s 200000 -set DIVIDE_SLIP_PROB 0.05 -set BASE_MERIT_METHOD 0
   done
   ```

   For the third arm use a copy of the recipe that reads `events-switch.cfg`.

5. Compare genome length over time (`dominant.dat` column 6, or `average.dat`), EQU counts in `tasks.dat`, and the dominant genomes.

## What to expect

Pilot (2 seeds, 12,000 updates, change-over at 6,000, run with the slip probability set to 0; the default with slip has not been piloted): the `evolve-then-streamline` runs matched `size-neutral` up to the change-over and then shrank, in seed 1 from 95 to 89 instructions in 6,000 updates while the control stayed at 97. The `streamlined` arm did not shrink in that time (97 and 88 against 98 and 114 for the control): early on, the task rewards are worth factors of two and more and swamp a 1% advantage in copying time. The effect should show over hundreds of thousands of updates, which is what the long run is for.
