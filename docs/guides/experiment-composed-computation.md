# Can populations evolve composed computation?

This page describes an experiment in progress. It asks whether an instruction set with subroutines helps a population evolve a function that has to be built in several steps, a 2-bit adder made from `nand`, shifts and `inc`. The design below was fixed before the main runs. Results are added at the end as each stage finishes, and a negative result is reported as one.

Everything on this page is an extension of the Rust port. The C++ Avida has none of it, and a configuration that does not ask for it runs exactly as before.

## The pieces

### An instruction set with subroutines

`instset-heads-modular.cfg` is the stock heads set plus three more registers (DX, EX, FX, addressed with `nop-D`, `nop-E`, `nop-F`) and the instructions `call` and `return`. Select it with `-def INST_SET instset-heads-modular.cfg`.

Two properties shape organisms written for it. With six nops the complement of a label rotates every nop by one, so `call` followed by `nop-C nop-C` reaches the label `nop-D nop-D`. And `call` searches forward only, so a subroutine lies after its callers.

`modular-subroutine.org` is a hand-written organism that does NOT, NAND and AND through one shared subroutine. `default-heads-modular.org` is the stock ancestor adapted to the six-nop labels.

### A task for multi-bit functions

The task `field_arith` treats a 32-bit word as a row of small independent fields and scores a function field by field. Its arguments:

| Argument | Meaning |
|---|---|
| `fn` | `xor`, `and`, `or`, `halfadd` or `add` |
| `bits` | width of each operand, 1 to 4 |
| `mode` | `packed`: every result bit of every field in one output. `planes`: one result bit per output, over successive outputs |
| `thresh` | the lowest quality that is paid |
| `stride` | field spacing, when it should differ from the default of `bits` + 1 |

The quality is max(0, 2f - 1), where f is the fraction of the compared bits that are right. A random output scores about zero.

### The ladder

`environment-adder-ladder.cfg` has eight reactions, each on its own resource that runs short when many organisms use it, each paid once per organism.

| Rung | Task | Value |
|---|---|---|
| NOT, NAND | stock | 1 |
| XOR | stock | 2 |
| AND | stock | 3 |
| HALFADD_PLANES | sixteen 1-bit half adders, sums then carries as two outputs | 4 |
| HALFADD | the same in one output | 5 |
| ADD2_PLANES | ten 2-bit adders, three result bits as three outputs | 6 |
| ADD2 | the same in one output | 7 |

Merit is multiplied by 2 to the power of (value times the resource consumed), and a cell at rest gives half a unit.

The instruction sets used in the experiment have no `add` and no `sub`. Hand-written solutions exist for every upper rung in both sets (`ladder-*.org`), so every rung is reachable.

### The instruments

- **Ladder view** (Charts tab, or `J` in the terminal): per rung, how many organisms performed it over time. A rung is held when at least 1 % of the organisms perform it.
- **Reuse view** (Organism tab, or `V` in the terminal debugger): a knockout map. Each genome site is replaced in turn by a null instruction and the organism is run in the test CPU. A site is lethal, shared (two or more tasks lost), task (one lost), speed (fitness only) or neutral. The reuse index is the mean overlap of the site sets of each pair of tasks.
- **Arm comparison** (`avida-gui --compare DIR`, `avida-tui --compare DIR`): reads a folder of runs laid out as `DIR/<arm>/seed-<N>/` and shows per arm how many seeds reached each rung, the highest rung over time, and the measures per seed, with exact tests and n.

Two limits of the knockout measure matter when reading results. A site early in a pipeline, such as the instruction that reads the inputs, is shared by every task without being reused code; the count of `call` sites and their targets separates the two. And duplicated code hides sites, because knockouts are made one at a time.

## Design, fixed before the main runs

| Choice | Value |
|---|---|
| Arms | modular set against stock set, both without `add` and `sub` |
| Ancestor | the stock ancestor in each arm's own set |
| Seeds | 10 per arm, seeds 1 to 10 |
| Length | 100,000 updates |
| World | 60 x 60, exact execution mode |
| Mutations | stock rates, plus `DIVIDE_SLIP_PROB 0.05` in both arms |
| Threshold | 0.75 on the four `field_arith` rungs |
| Fields | ten 3-bit fields for the 2-bit adder |
| Primary outcome | number of seeds per arm that ever held each rung; highest rung held at the end |
| Secondary | MODES complexity, reuse index, shared sites, `call` sites of the dominant genotype |
| Tests | Fisher's exact test per rung, Mann-Whitney U per measure, both two-sided |

The threshold was first set at 0.25. Measurement before any run showed that trivial outputs cleared it (a constant 0 scores 0.25 on HALFADD, and the XOR of the inputs scores 0.45), so it was raised to 0.75.

## What was seen before the main runs

These are checks of the set-up, not results.

- In a world seeded with the four hand-written upper-rung organisms, the HALFADD_PLANES organism, which also does XOR and AND and copies itself in about 220 cycles, displaced the three higher-rung organisms within about 500 updates. Those do only their own rung and need 345 to 680 cycles.
- A pilot of 3 seeds per arm for 20,000 updates wrote every output file. One seed per arm reached HALFADD_PLANES. The other four ended holding no rung, with genomes of 21 to 25 instructions.

## Results

### Stage 3: modular set against stock set

The main runs finished on 2026-11-15: 10 seeds per arm, 100,000 updates each, all 20 complete. The outcome is negative. Neither arm came near the adder, and the modular set did no better than the stock set.

**How far each arm climbed.** A seed counts for a rung when at least 1 % of its organisms performed it, at any sample (ever) or at the last one (end).

| Rung | Modular, ever | Stock, ever | p | Modular, end | Stock, end | p |
|---|---|---|---|---|---|---|
| NOT | 9 | 8 | 1.00 | 1 | 4 | 0.30 |
| NAND | 8 | 8 | 1.00 | 1 | 4 | 0.30 |
| XOR | 0 | 0 | 1.00 | 0 | 0 | 1.00 |
| AND | 1 | 3 | 0.58 | 1 | 2 | 1.00 |
| HALFADD_PLANES | 1 | 3 | 0.58 | 1 | 3 | 0.58 |
| HALFADD | 0 | 0 | 1.00 | 0 | 0 | 1.00 |
| ADD2_PLANES | 0 | 0 | 1.00 | 0 | 0 | 1.00 |
| ADD2 | 0 | 0 | 1.00 | 0 | 0 | 1.00 |

Counts are seeds out of 10; p is Fisher's exact test, two-sided. No difference between the arms is significant, and where the counts differ the stock arm is ahead.

The highest rung reached was HALFADD_PLANES, the lowest of the four multi-bit rungs: modular seed 3 and stock seeds 2, 5 and 9. The highest rung held at the end did not differ between the arms (Mann-Whitney U = 35.5 against 64.5, n = 10 and 10, p = 0.30).

**Most populations lost every task early.** In 12 of the 20 runs the population picked up NOT or NAND between updates 1,000 and 2,800, lost them again before update 4,000 (one modular seed kept NAND until 11,700), and held no rung for the remaining 96,000 updates. Three more runs never held a rung at all. In these runs the genome shrank from the ancestor's 100 instructions to between 19 and 24, and the gestation time fell from 389 cycles to about 30. A NOT or NAND multiplies merit by at most 1.41 (2 to the power of half a unit of resource) while the shrinking tripled the fitness (merit over gestation time rose from 0.25 to 0.74 in modular seed 7), so the short replicators displaced the organisms that computed.

The four runs that reached HALFADD_PLANES went another way. In three of them AND (worth up to 2.8 times) and HALFADD_PLANES (up to 4 times) appeared between updates 2,000 and 2,600, before the short replicators had taken over, and these populations then kept their tasks to the end with genomes of 46 to 60 instructions. Stock seed 9 kept NOT and NAND throughout and found HALFADD_PLANES at update 20,500. Stock seed 10 held NAND from update 2,100 and NOT from 4,300 and found nothing higher.

In 18 of the 20 seeds no rung was added after update 4,000; the exceptions are stock seed 9 (HALFADD_PLANES at 20,500) and stock seed 10 (NOT at 4,300). What decided the outcome was a race between shrinking the genome and finding a rung worth more than the shrinking, and the instruction set played no visible part in that race.

**Secondary measures.** Medians with the range in brackets; Mann-Whitney U, two-sided, exact.

| Measure | Modular | Stock | n | p |
|---|---|---|---|---|
| MODES complexity (last sample) | 30 (26 to 52) | 31 (26 to 44) | 10, 10 | 0.42 |
| Genome length of the dominant genotype | 21.5 (20 to 60) | 23 (19 to 52) | 10, 10 | 0.69 |
| Tasks performed by the dominant genotype | 0 (0 to 4) | 0 (0 to 4) | 10, 10 | 0.30 |
| Reuse index | 0.34 | 0.35 (0.32 to 0.43) | 1, 3 | 1.00 |
| Shared sites | 11 | 11 (3 to 13) | 1, 3 | 1.00 |

The reuse measures need a dominant genotype that performs two tasks, which leaves one modular seed and three stock seeds; nothing can be concluded from them about the arms.

**Was `call` used?** No. The dominant genotype of the one modular seed that climbed (seed 3, 60 instructions, performing NOT, NAND, AND and HALFADD_PLANES) contains one `call`. Knocking it out costs 0.9 % of fitness and loses no task. Its 11 shared sites are inputs (`IO`), `nand` and `swap` instructions that several tasks pass through in one straight run of code, which is the first limit of the knockout measure named above and not reuse through a subroutine.

**What this does and does not show.** It shows that under this design the populations do not get far enough up the ladder for subroutines to matter: the half adder in two outputs is within reach of both sets without them, and nothing beyond it was reached by either. It does not show that subroutines are useless for composed computation, because no run reached a rung where a second use of the same code would pay. With 10 seeds per arm, a difference as large as 1 seed against 3 is well within chance.

The design was not changed after the runs. The observation about the early race is a candidate for the next stage, where population size, sex and changing environments are varied one at a time.

**Runs to look at.** In the lab folder `13-adder-ladder`:

| Run | Why |
|---|---|
| modular seed 3 | best of the modular arm: HALFADD_PLANES from update 2,300 |
| modular seed 7 | typical: NOT and NAND up to update 3,800, then nothing |
| stock seed 2 | best of the stock arm: HALFADD_PLANES from update 2,600, highest complexity of the arm |
| stock seed 1 | typical: NOT and NAND up to update 2,400, then nothing |

The maintainer looked at these demonstrations and accepted the negative result on 2026-11-15.

### Stage 4: search conditions

Design, fixed on 2026-11-15 before any stage 4 run. One factor is changed at a time against the stage 3 modular arm, whose 10 runs serve as the base. The arms did not differ in stage 3; the modular set was chosen because the question is whether subroutines come into use once populations climb further.

| Arm | Change against the base |
|---|---|
| `sex` | `divide-sex` in place of `h-divide`: offspring recombine with another offspring; run with `DROP_EXTINCT_GENOTYPES 1` (see below) |
| `pop-30` | a 30 x 30 world, resource inflow scaled to the area |
| `pop-120` | a 120 x 120 world, resource inflow scaled to the area |
| `seasons` | NOT, NAND, XOR and AND pay nothing in every other period of 2,000 updates |
| `min-genome` | `MIN_GENOME_SIZE 50`: a divide that would leave fewer than 50 instructions fails |

The first four are the factors planned from the start. The fifth was added after stage 3, because of the genome shrinking seen there; the value 50 is half the ancestor's length and was chosen without a trial run. Seeds 1 to 10, 100,000 updates, exact mode, and the outcomes and tests of stage 3. Each arm is compared with the base only, so five comparisons are made per measure; p values are reported uncorrected and with a Holm correction over the five.

One change was made to the `sex` arm after the batch had started and before any `sex` run finished. With two parents every ancestor stays in memory as a historic genotype, and the first six `sex` runs grew to more than 4 GB each and were stopped at about 65,000 updates; their output is not used. The arm now runs with the new setting `DROP_EXTINCT_GENOTYPES 1`, which drops a genotype when its last organism dies and changes nothing that organisms compute (the data files of a run are byte-identical with and without it). The measures that need ancestry, MODES complexity and the phylogeny measures, are therefore not available for the `sex` arm. Its primary outcome, the genome length, the tasks performed and the reuse measures are reported as for the other arms.

#### Results

The runs finished on 2026-11-16: 10 seeds per arm, 100,000 updates each, all 50 complete. One arm changed the outcome. A minimum genome size brought every seed to HALFADD_PLANES and kept it there. Sex, population size and seasons made no difference that these runs can show. No arm got past HALFADD_PLANES, and no arm used `call` as a subroutine.

**How far each arm climbed.** Seeds out of 10 that held the rung at the last sample, and in brackets at any sample.

| Rung | `base` | `sex` | `pop-30` | `pop-120` | `seasons` | `min-genome` |
|---|---|---|---|---|---|---|
| NOT | 1 (9) | 3 (10) | 0 (6) | 0 (10) | 0 (7) | 10 (10) |
| NAND | 1 (8) | 3 (8) | 0 (5) | 0 (9) | 0 (6) | 10 (10) |
| XOR | 0 (0) | 0 (0) | 0 (0) | 0 (0) | 0 (0) | 0 (0) |
| AND | 1 (1) | 3 (3) | 0 (0) | 0 (0) | 0 (1) | 10 (10) |
| HALFADD_PLANES | 1 (1) | 3 (3) | 0 (0) | 0 (0) | 0 (0) | 10 (10) |
| HALFADD, ADD2_PLANES, ADD2 | 0 (0) | 0 (0) | 0 (0) | 0 (0) | 0 (0) | 0 (0) |

The test of the primary outcome is Fisher's exact test, two-sided, on the number of seeds holding HALFADD_PLANES at the end, each arm against `base`.

| Arm | Seeds | p | p, Holm over five |
|---|---|---|---|
| `min-genome` | 10 against 1 | 0.00012 | 0.0006 |
| `sex` | 3 against 1 | 0.58 | 1.00 |
| `pop-30` | 0 against 1 | 1.00 | 1.00 |
| `pop-120` | 0 against 1 | 1.00 | 1.00 |
| `seasons` | 0 against 1 | 1.00 | 1.00 |

**A minimum genome size stops the loss of tasks.** In all ten `min-genome` seeds NOT and NAND were held from before update 2,500 and never lost. AND followed between updates 2,000 and 15,500 and HALFADD_PLANES between 2,400 and 7,300, and both stayed to the end. Nine of the ten dominant genotypes have exactly 50 instructions, the smallest the setting allows; the tenth has 77. In stage 3 the populations lost their tasks because a shorter genome paid more than a low rung. With that road closed the populations kept what they found, and the next two rungs followed in every seed. The one `base` seed that held HALFADD_PLANES is also the one `base` seed with a long genome (60 instructions).

The setting did not carry any seed further. No rung was added after update 15,500 in any `min-genome` run, and none of the three upper rungs was held by 1 % of a population at any sample in any of the 50 runs.

**Population size.** The 16-fold range from 900 to 14,400 organisms shows no effect on the rungs. Neither arm held a rung at the end, and no rung outlived update 4,300 in either. The larger world found the low rungs more reliably (NOT in 10 seeds against 6, NAND in 9 against 5) and lost them the same way.

**Sex.** Three `sex` seeds held HALFADD_PLANES at the end against one `base` seed, a difference well within chance at this number of seeds. Two things did change. Genomes stayed longer (median 41.5 instructions against 21.5, the three climbing seeds at 79, 157 and 221), and the low rungs lasted longer before they were lost: NOT was held until update 75,500, 84,400 and 92,500 in three of the seeds that ended with nothing, where no `base` seed kept it past 3,800.

**Seasons.** No seed held a rung at the end. Seed 3, which in the `base` arm is the seed that climbed, touched AND at update 2,000, the moment the first season without pay began, and lost it.

**Secondary measures.** Medians with the range in brackets; Mann-Whitney U against `base`, two-sided, exact; n = 10 and 10 unless stated. The Holm correction is over the comparisons that exist for the measure: five, or four where the `sex` arm has no value.

| Measure | `base` | `sex` | `pop-30` | `pop-120` | `seasons` | `min-genome` |
|---|---|---|---|---|---|---|
| Genome length of the dominant genotype | 21.5 (20 to 60) | 41.5 (27 to 221) | 23 (22 to 26) | 23.5 (21 to 25) | 21 (19 to 25) | 50 (50 to 77) |
| p; Holm | | 0.0004; 0.0021 | 0.12; 0.24 | 0.08; 0.24 | 0.54; 0.54 | 0.0004; 0.0021 |
| MODES complexity (last sample) | 30 (26 to 52) | not available | 24 (22 to 29) | 34.5 (31 to 36) | 27.5 (25 to 32) | 41 (40 to 55) |
| p; Holm | | | 0.0001; 0.0006 | 0.035; 0.070 | 0.075; 0.075 | 0.0009; 0.0026 |
| Tasks performed by the dominant genotype | 0 (0 to 4) | 0 (0 to 3) | 0 (0 to 0) | 0 (0 to 0) | 0 (0 to 0) | 4 (3 to 4) |
| p; Holm | | 0.58; 1.00 | 1.00; 1.00 | 1.00; 1.00 | 1.00; 1.00 | 0.0001; 0.0006 |
| Reuse index | 0.34 (n = 1) | 0.47 (0.44 to 0.64, n = 3) | none | none | none | 0.37 (0.33 to 0.65) |
| Shared sites | 11 (n = 1) | 10 (10 to 12, n = 3) | none | none | none | 13 (8 to 15) |

Complexity follows population size (lower at 900 organisms, higher at 14,400) without any rung being held, so here it measures something other than climbing the ladder. The reuse measures need a dominant genotype that performs two tasks. With one such seed in `base` they cannot be tested against it (p = 0.36 and 0.64 for `min-genome`, n = 1 and 10).

**Was `call` used?** Not as a subroutine. `PrintReuseSurvey` was run on the final population of the ten `min-genome` runs and of the three climbing `sex` runs, five genotypes each, the most abundant ones that replicate and perform two or more tasks. None of the 65 genotypes has a subroutine site, a site executed from two different `call` sites. 31 of them contain one to four `call` instructions, each genotype with one target (two in one genotype); in 8 genotypes a knockout of a `call` is not neutral. These calls work as jumps. The shared sites are, as in stage 3, inputs and `nand` instructions that several tasks pass through in one run of code.

**What this does and does not show.** It shows that what held the stage 3 populations down was the shrinking of the genome: with shrinking below 50 instructions forbidden, 10 of 10 seeds reach and keep the half adder in two outputs, against 1 of 10. It shows no effect of population size, of seasons or of sex on how far the ladder is climbed; with 10 seeds per arm an effect smaller than about 1 seed against 6 would not be found. It does not show that any of these conditions brings composed computation closer. The ceiling is the same in every arm, the step from HALFADD_PLANES to HALFADD (both result bits in one output) was not taken in any of the 14 runs that held the lower rung, most of them for more than 90,000 updates, and subroutines stayed unused. The value 50 was chosen without a trial run and was not varied, so nothing is known about other values.

The design was not changed after the runs, apart from the memory setting of the `sex` arm described above.

**Runs to look at.** In the lab folder `14-search-conditions`, `./compare.sh runs` opens the arm comparison (`./compare.sh runs tui` in the terminal); choose `base` and one arm at a time.

| Run | Why |
|---|---|
| `min-genome` seed 8 | fastest of the arm: all four rungs by update 2,400 |
| `min-genome` seed 1 | slowest: HALFADD_PLANES at 7,300, AND at 15,500 |
| `sex` seed 7 | found AND and HALFADD_PLANES late, at about update 35,000, with a genome of 157 instructions |
| `sex` seed 8 | held NOT until update 92,500, then nothing |
| `pop-120` seed 1 | typical of the large world: NOT and NAND up to update 2,900, then nothing |

The maintainer looked at the comparison and accepted the stage on 2026-11-16.

#### Correction, found on 2026-11-16 after the stage was accepted

The organisms that hold HALFADD_PLANES in stages 3 and 4 are not half adders. The most abundant genotypes of the eleven climbing `base` and `min-genome` runs were run in the test CPU on 30 random input triples each, and the outputs of a sample were compared with the logic functions of the inputs. They output NAND and then AND. NAND agrees with the sum bit, XOR, on three of the four input combinations, so the pair is right on 87.5 % of the compared bits on average, a quality of 0.75, which is the threshold. The rung fired on 24 of the 30 triples for nearly every genotype (a calculation for random inputs gives 78 %). No run in either stage held XOR, the rung below, at any sample.

What stands: the counts and tests above, as counts of the rung as it is defined, and the finding that a minimum genome size stops the loss of tasks (NOT, NAND and AND are real, and were kept in 10 of 10 seeds against 1 of 10). What does not stand is the reading that these populations compute a half adder in two outputs. The highest real step reached in any run is AND, and the wall is XOR. The threshold of 0.75 was raised from 0.25 before the runs to exclude trivial outputs; it was not checked against a sum plane made of NAND or OR, and it should have been.

### Stage 5: the wall below XOR

Design, fixed on 2026-11-16 before any stage 5 run. The correction above moves the question. No population made XOR, and the ladder paid an approximation of the half adder. Two things are changed in turn on the `min-genome` arm of stage 4, the only condition in which populations keep what they find, and then the search is spread over islands.

| Arm | Change against the arm above it |
|---|---|
| `exact` | against `min-genome` of stage 4: the four field rungs pay only at full quality (`thresh=1`), so NAND then AND no longer clears HALFADD_PLANES |
| `exact+steps` | the four stock logic functions between NAND and XOR are paid: ORN 2, OR 3, ANDN 3, NOR 4, each on its own resource, once per organism |
| `exact+steps+islands` | ten demes of 60 x 6 cells (`NUM_DEMES 10`); an offspring is born in another deme, at a random cell, with probability 0.001 (`DEMES_MIGRATION_RATE`) |

The four steps are those of the stock logic environment, with its values. In the classic experiment on this question (Lenski and others, 2003) populations reached the hardest logic function only when the simpler ones in between were paid. Every arm lists the same twelve reactions in the same order (NOT, NAND, AND, ORN, OR, ANDN, NOR, XOR and the four field rungs), so that the output files agree; in `exact` the four steps are worth nothing, and an organism that performs one is counted without being paid. The migration rate was chosen without a trial run: with 360 organisms in a deme it is about one migrant per deme in three generations.

Seeds 1 to 10, 100,000 updates, exact mode, modular set, a 60 x 60 world, `MIN_GENOME_SIZE 50`, the other settings of stage 4. Primary outcome: the number of seeds that hold XOR at the last sample. Secondary: the same for HALFADD_PLANES, which at full quality is a real half adder; the highest rung held; the first update at which XOR is held; and the measures of stage 4. Tests: Fisher's exact test, two-sided, for `exact+steps` against `exact` and for `exact+steps+islands` against `exact+steps`, reported uncorrected and with a Holm correction over the two. For each arm the most abundant genotypes of the runs that hold XOR or more are run in the test CPU and their outputs compared with the functions paid, which is the check that was missing in stages 3 and 4.

Before the runs: a 3,000-update trial of each arm wrote every file, and the islands arm run without migration filled one deme only (358 organisms at update 2,000).

Results: to be written when the runs have finished.
