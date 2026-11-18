# Tutorial: sex and asex

**Question.** Does recombination speed adaptation? The two arms use the same world; in the sexual arm, offspring recombine with another newborn.

**Arms.** `asexual` (stock, instruction `h-divide`) and `sexual` (instruction `divide-sex` in place of `h-divide`).

## Steps

1. Start from the stock folder and the [replicate recipe](how-to-replicates.md). The sexual instruction set is the stock set `heads_sex`, and its ancestor `default-heads-sex.org` is in the stock folder.
2. Make `events-sex.yaml`, the stock events with the sexual ancestor. It is the part of `avida.yaml` from the line `events:` to the end, with the first entry changed to `{at: begin, action: Inject, args: default-heads-sex.org}`:

   ```sh
   sed -n '/^events:/,$p' avida.yaml | sed 's/default-heads.org/default-heads-sex.org/' > events-sex.yaml
   ```

3. Run the arms. The sexual arm needs the sex instruction set, the sex ancestor and its own events, which the recipe reads from the file named by `EVENTS`:

   ```sh
   for s in 1 2 3 4; do
     run_arm asexual $s 20000
     EVENTS=events-sex.yaml run_arm sexual $s 20000 -set instruction_sets heads_sex
   done
   ```

4. Compare fitness (`average.dat` column 4) and `tasks.dat`, in particular the `Equals` column.

If you forget the ancestor, the run stops with `error: invalid instruction set 'heads_default' defined in organism 'default-heads.org'`: the ancestor's instruction set does not exist in a run that has only the sex set. See [instruction sets](../reference/instruction-sets.md#organism-files).

## What to expect

Pilot (2 seeds, 15,000 updates): the sexual populations ended far fitter (median 1.3 million against 7,400) and found EQU in both runs, the asexual in neither. Two seeds cannot carry a claim like that. This is the tutorial to repeat with eight seeds.

Settings that change how sex works are listed in [reproduction, birth and sex](../concepts/reproduction.md#sexual-reproduction), for example `recombination.two_fold_cost_sex: true`, which lets only one recombined offspring be born, so that sex pays its two-fold cost.
