# Systematics and genotypes

*Systematics* is the bookkeeping that classifies organisms and records who descended from whom. Avida's systematics manager runs alongside the simulation and is what makes lineages, the dominant genotype and the saved population files possible.

## Genotypes

A *genotype* is the set of all organisms with the same genome (and the same hardware type and instruction set). Each new genotype gets an id, a record of its parent genotype, the update it was born and, when the last organism of it dies, the update it went extinct.

The program also gives each genotype a name made of its length and a short letter code. In `dominant.dat`, the dominant genotype of the stock run at update 1000 is named `099-aabev`: length 99, followed by the code. The name is for human reference only; the id is what identifies a genotype in files.

Counting genotypes:

- `count.dat` column 4 is the number of different genotypes alive, and column 5 the number of *threshold* genotypes: those with at least `THRESHOLD` organisms (stock 3). A genotype that stays below the threshold is not yet established in the population.
- In the first stock run at update 1000, about 2,870 genotypes were alive in a population of 3,598 organisms, of which about 400 were above the threshold.

## The tree of life

Because every genotype records its parent, the genotypes form a tree. Three tools use it:

- The population snapshot `detail-N.spop` lists the genotypes of the population with their parent ids, depth and the updates in which they were born and deactivated. [Analyze mode](../reference/analyze-mode.md) loads the file and can walk the tree, for example `FIND_LINEAGE num_cpus` follows the line of descent from the most abundant genotype back to the ancestor.
- `PrintDepthHistogram` and `PrintGenotypeAbundanceHistogram` summarise the tree and the abundance distribution.
- The `Ancestry` layer of the [front ends](../front-ends/index.md) colours each organism by its family: the family tree of the living population is cut into at most seven large families, and each gets its own colour.

## Lineage labels

`Inject` accepts a *lineage label* (the fourth argument), a number that the organism and all its descendants inherit. Giving several ancestors different labels lets you see which lineage wins. `PrintLineageCounts` writes the counts per label, and the Lineage map layer shows them.

## Coalescence clades

With `TRACK_CCLADES 1`, the program tracks coalescence clades and `PrintCCladeCounts` and its relatives report them. The ids go to the file named by `TRACK_CCLADES_IDS` (stock `coalescence.ids`).

## Turning it off

`DISABLE_GENOTYPE_CLASSIFICATION` turns off the tracking of historical genotypes to save memory. Many data files and the snapshot depend on the tracking, so leave it off unless you know you do not need them.

## Next

The other half of the documentation is the [reference](../reference/index.md), starting with the [settings](../reference/settings.md).
