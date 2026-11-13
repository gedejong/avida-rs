# Tutorial: line of descent

**Question.** Take an evolved genotype and walk back to the ancestor. What did each step on the way change: how fit was each genotype, and how many of its one-point mutations were harmful or beneficial?

This is analyze mode on a finished run. It needs a saved population, so first finish a run with the stock configuration (the first run of [getting started](../getting-started/first-run.md) saves `detail-1000.spop`). A run of the [stepping-stones tutorial](tutorial-stepping-stones.md), with its tasks, gives a more interesting line.

## Steps

1. In the run's folder, write `lineage.cfg`:

   ```text
   LOAD data/detail-1000.spop
   FIND_LINEAGE num_cpus
   RECALCULATE
   PrecalcLandscape
   DETAIL lineage.dat depth update_born length fitness frac_dead frac_neg frac_neut frac_pos task.0 task.1
   ```

   Use the update of your own saved population in the file name. If your environment has more tasks, add `task.2`, `task.3` and so on.

2. Run analyze mode:

   ```sh
   avida -a -set ANALYZE_FILE lineage.cfg -set DATA_DIR out
   ```

   You should see `Loading: ...`, `Performing lineage scan...`, `Running through test CPUs...`, `Precalculating landscape...` and `Detailing...`. The landscape step tests every one-point mutation of every genotype on the line (about 2,600 of them for a genome of 100 instructions), spread over all cores. It took about a third of a second for 23 genotypes in the example below.

3. Read `out/lineage.dat`. Its columns are, in order, the genotype's depth in the tree, the update it was born, its length, fitness, and the fractions of one-step mutations that are lethal, harmful, neutral and beneficial, then the task counts.

## What you see

For the stock run with seed 1 at update 1000, the last rows were:

```text
19 684 99 0.259459 0.235556 0.149495 0.323636 0.291313 0 0
20 838 99 0.254054 0.244848 0.137778 0.32 0.297374 0 0
22 893 99 0.252033 0.224242 0.12 0.302222 0.353535 0 0
23 906 99 0.252717 0.223838 0.135354 0.293333 0.347475 0
```

The line has 23 steps; fitness barely moves in this young population, and about a quarter of one-step mutations are lethal while around a third are beneficial. In the maintainer's longer run of the stepping-stones experiment (713 genotypes on the line), EQU appeared in a single mutation that also cost the organism NOT and NOR, one step after a mutation that had halved its fitness. 18% of all steps on the line lowered fitness, and the share of beneficial mutations among all possible ones fell from 57% for the ancestor to 3 to 7% for its adapted descendants.

## Going further

- `FIND_LINEAGE` follows the most abundant genotype. See [analyze mode](../reference/analyze-mode.md) for the other `FIND_*` commands and the data columns.
- Add `PRINT` to write each genotype of the line as an organism file, then read how the program changed.
