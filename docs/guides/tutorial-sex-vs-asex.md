# Tutorial: sex and asex

**Question.** Does recombination speed adaptation? The two arms use the same world; in the sexual arm, offspring recombine with another newborn.

**Arms.** `asexual` (stock, instruction `h-divide`) and `sexual` (instruction `divide-sex` in place of `h-divide`).

## Steps

1. Start from the stock folder and the [replicate recipe](how-to-replicates.md). The stock folder already has the sexual instruction set `instset-heads-sex.cfg` and its ancestor `default-heads-sex.org`.
2. Make `events-sex.cfg`: a copy of `events.cfg` in which the first line reads `u begin Inject default-heads-sex.org`.
3. Run the arms. The sexual arm needs the sex instruction set, the sex ancestor, and its own events file. Because the recipe copies `events.cfg`, the simplest way is to keep the sexual arm in a second folder whose `events.cfg` is `events-sex.cfg`, or to call `avida` directly:

   ```sh
   for s in 1 2 3 4; do
     run_arm asexual $s 20000
     mkdir -p runs/sexual/seed-$s
     sed -E "s/^u [0-9]+ Exit/u 20000 Exit/" events-sex.cfg > runs/sexual/seed-$s/events.cfg
     avida -s $s -def INST_SET instset-heads-sex.cfg \
       -set EVENT_FILE runs/sexual/seed-$s/events.cfg -set DATA_DIR runs/sexual/seed-$s > runs/sexual/seed-$s/log.txt
   done
   ```

4. Compare fitness (`average.dat` column 4) and `tasks.dat`, in particular the `Equals` column.

If you forget the ancestor, the run stops with `error: invalid instruction set 'heads_default' defined in organism 'default-heads.org'`: the ancestor's instruction set does not exist in a run that includes only the sex set. See [instruction sets](../reference/instruction-sets.md#organism-files).

## What to expect

Pilot (2 seeds, 15,000 updates): the sexual populations ended far fitter (median 1.3 million against 7,400) and found EQU in both runs, the asexual in neither. Two seeds cannot carry a claim like that. This is the tutorial to repeat with eight seeds.

Settings that change how sex works are listed in [reproduction, birth and sex](../concepts/reproduction.md#sexual-reproduction), for example `TWO_FOLD_COST_SEX 1`, which lets only one recombined offspring be born, so that sex pays its two-fold cost.
