# Tutorial: population structure

**Question.** Offspring replace a neighbour (the default) or any organism in the world. Local replacement slows the spread of a better genotype and lets more lineages explore in parallel. Does it change what evolves?

**Arms.** `neighbours` (`reproduction.birth_method: neighborhood_random`, the stock value) and `well-mixed` (`reproduction.birth_method: population_random`, a random organism in the whole population). See [reproduction](../concepts/reproduction.md#where-the-offspring-goes).

## Steps

1. Start from the stock folder and the [replicate recipe](how-to-replicates.md).
2. Run:

   ```sh
   . ./lab.sh
   for s in 1 2 3 4; do
     run_arm neighbours $s 20000 -set reproduction.birth_method 0
     run_arm well-mixed $s 20000 -set reproduction.birth_method 4
   done
   ```

   That is about 10 minutes for 8 runs on a machine with eight free cores.

3. Compare the final fitness (`average.dat` column 4), the genome length (`dominant.dat` column 6 for the dominant genotype), and the tasks.

## What to expect

Pilot (2 seeds, 15,000 updates): no clear difference in fitness with two seeds (7,400 against 32,000, within the spread between seeds). The well-mixed populations had longer genomes (160 against 106). That is undecided and needs more seeds.

## Variations

- `world.geometry` changes the neighbourhood itself: bounded grid (1), torus (2, stock), clique (3), hexagonal (4), random connected (7), scale-free (8).
- `reproduction.prefer_empty: false` stops newborns from preferring empty cells.
