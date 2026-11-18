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

#### Results (2026-11-17)

All 30 runs finished. They ran on one rented Linux server (x86-64, 16 shared cores), not on the Mac that ran stages 3 and 4; a 2,000-update trial of two arms gave the same task counts on both machines, but the port's byte identity is pinned on macOS arm64 only, so the comparison with `min-genome` of stage 4 is across platforms and the three arms of this stage, which shared a machine, are compared with each other only.

| Arm | Seeds holding XOR at the end | Seeds holding HALFADD_PLANES | Seeds holding all four steps | Median generations | Median genome length |
|---|---|---|---|---|---|
| `exact` | 0 of 10 | 0 of 10 | 0 of 10 | 30,004 | 51 |
| `exact+steps` | 1 of 10 (seed 1) | 1 of 10 | 10 of 10 | 7,694 | 110 |
| `exact+steps+islands` | 2 of 10 (seeds 5, 7) | 2 of 10 | 10 of 10 | 8,797 | 110 |

A seed holds a function when at least a tenth of the population performs it at update 100,000; the counts are far from that line on both sides (2,592 to 3,066 organisms of about 3,600 in the three seeds that hold XOR, 19 at most in the others).

**Primary outcome: no difference shown.** `exact+steps` against `exact`: 1 of 10 against 0 of 10, Fisher exact p = 1.0. `exact+steps+islands` against `exact+steps`: 2 of 10 against 1 of 10, p = 1.0. Both stay at 1.0 after the Holm correction. With ten seeds an arm the tests could have shown only a large effect; they do not show that paying the steps or spreading the search over islands changes how often XOR is reached.

**What the runs do show.**

- XOR was made and kept for the first time in this experiment: three populations, all in arms that pay the steps, none of the ten that do not. Pooling the two paying arms (3 of 20 against 0 of 10, Fisher exact p = 0.53) was not a planned test and shows no difference either.
- With the field rungs paid at full quality only, the approximation of stages 3 and 4 is gone: no seed of `exact` holds HALFADD_PLANES, against 10 of 10 for `min-genome` in stage 4 under the lenient rule.
- Each population that holds XOR also holds HALFADD_PLANES, which at full quality is a real half adder on sixteen one-bit fields. In seeds 1 and 5 the rung was held 200 and 900 updates after XOR; in seed 7 it took 46,000 updates more.
- No population went further: HALFADD (the packed form, which needs the carry shifted and joined with the sum) was performed by 2 organisms at most at any sample, ADD2_PLANES by 3.
- Paying the steps changes the populations themselves (`exact+steps` against `exact`, Mann-Whitney, exact p = 0.00001 each): they hold all four steps in every seed, their genomes are twice as long (median 110 against 51), and the information they hold is twice as large (median 86.5 against 41 sites). Longer genomes replicate more slowly, so in the same 100,000 updates the paying arms had a quarter of the generations of `exact` (about 7,700 against 30,000). The classic experiment ran for about 15,000 generations; the paying arms here stopped at half of that.
- Islands against one population: no difference in any measure (p from 0.24 to 1.0).

**The check of the outputs.** The five most abundant genotypes of each of the three populations that hold XOR were run in the test CPU on 30 random input triples. In each population the most abundant genotype performed XOR and HALFADD_PLANES in 30 of 30 rounds; of the fifteen genotypes nine did. XOR is judged on all 32 bits by the stock logic task, a different code path from `field_arith`, so the half adder credit rests on a real XOR.

**A leak the check found.** HALFADD_PLANES compares the even bits only (sixteen fields of two bits). On those bits `a AND NOT b` equals `a XOR b` whenever `b` has no even bit set that `a` lacks. For two of the three input pairs the fixed top bytes always satisfy this, and the 24 random bits do so with probability (3/4)^12, about 3 %. An organism that outputs ANDN and then AND is therefore paid for the half adder on about one input draw in thirty, without making XOR. This is where the small counts of HALFADD_PLANES in populations without XOR come from (26 organisms at most in `exact`, 179 in `exact+steps`). The credit is not inherited and no population held the rung this way, so the counts above stand; a rung judged on more than one input draw would close the leak.

Verdict: negative on the primary outcome, accepted by the maintainer on 2026-11-17.

### Stage 6: more time below XOR

Design, fixed on 2026-11-17 before any stage 6 run and revised the same day, still before any run, at the maintainer's request for earlier cut-offs (the revision is the stopping rule and the test). Stage 5 could not show that paying the steps matters: 1 of 10 seeds against 0 of 10. Two things kept that test weak. Ten seeds an arm show only a large effect, and the arm that pays the steps had about 7,700 generations in its 100,000 updates, half of the classic experiment's 15,000, because its genomes are twice as long. Stage 6 repeats the two one-population arms with more seeds and more time.

| Arm | What it is |
|---|---|
| `exact-long` | `exact` of stage 5: field rungs at full quality only, the four steps listed and worth nothing |
| `steps-long` | `exact+steps` of stage 5: the four steps paid |

Seeds 11 to 26 (sixteen an arm, none of them used in stage 5), at most 300,000 updates, every setting of stage 5 that acts on a run. Two outputs differ: the population is saved every 10,000 updates, and the resource maps, which no analysis reads, are not written.

**Stopping rule.** A population first holds XOR at the first sample (every 100 updates) at which at least 360 organisms, a tenth of the world, perform it. A run is stopped at the first population save 50,000 updates or more after that, so 50,000 to 60,000 updates later; a run that never holds XOR runs to 300,000. In stage 5 XOR arrived abruptly and the runs that never made it did not look different before, so there is no early sign by which to stop a run that will fail; what can be saved is the time after success. The 50,000 updates are kept to see what follows XOR.

**Primary outcome.** The time to the first hold of XOR in updates, censored at 300,000, `steps-long` against `exact-long`: a log-rank test, two-sided, with the p value from 100,000 relabellings of the runs (the chi-square p is reported beside it and is rough with few events). One test. It uses when each population got there, where the count at the end of stage 5 used only whether.

**Secondary, without tests unless stated.** The same log-rank test on the generation clock, since a paying population has about a quarter of the generations per update; the Kaplan-Meier share of populations holding XOR by 100,000, 200,000 and 300,000 updates, which with stage 5 says whether the rate grows with time; whether XOR, once held, is still held when the run stops; the highest rung held in the 50,000 updates after XOR, and whether any population passes HALFADD_PLANES to HALFADD. The most abundant genotypes of the populations that hold XOR are again run in the test CPU. The leak in HALFADD_PLANES found in stage 5 is left open in this stage, so that its arms stay those of stage 5; a population counts as holding that rung only if it also holds XOR.

Lab folder `16-xor-time` (`run_all.sh` applies the stopping rule, `analyse.py` the tests). Before the runs: a 3,000-update trial of each arm with a stand-in trigger (NOT held by 5 organisms) stopped all four runs at the right save and wrote every file; the log-rank code reproduces the textbook value for the Freireich leukaemia data (chi-square 16.79).

#### Results (2026-11-17)

All 32 runs are in. Thirty ran to their end; the last two (`exact-long`, seeds 17 and 25) were stopped by hand at updates 277,100 and 277,700 at the maintainer's instruction, with no organism performing XOR, and enter the analysis as runs without XOR up to there. The runs were spread over five rented servers of two cheap types, eight runs of each arm on one and two of each arm on each of the other four, so that no arm is tied to a machine.

| Arm | Populations that held XOR | First held |
|---|---|---|
| `exact-long` | 0 of 16 | |
| `steps-long` | 1 of 16 (seed 13) | update 90,900, generation 7,739 |

**Primary outcome: no difference shown.** Log-rank on the time to XOR, permutation p = 1.0 (chi-square p = 0.32); the same on the generation clock.

**More time did not help.** Fifteen populations that are paid for the steps ran 300,000 updates without holding XOR. The share holding XOR by 100,000 updates is 0.06 here and was 3 of 20 in stage 5; nothing was added between 100,000 and 300,000. XOR does not arrive at a steady rate: the four populations that have found it in stages 5 and 6 did so before generation 7,800, in runs that went on to about 23,000.

**After XOR.** Seed 13 held XOR and HALFADD_PLANES when it was stopped at 150,000, 59,100 updates after XOR, and nothing above them. Its most abundant genotype performs XOR and HALFADD_PLANES in 30 of 30 rounds in the test CPU, as do four of its five most abundant.

**Why not later: genomes shrink.** Median genome length over the populations of an arm:

| Update | `steps-long` | `exact-long` |
|---|---|---|
| 10,000 | 199 | 83 |
| 50,000 | 176 | 51 |
| 100,000 | 150 | 52 |
| 200,000 | 99 | 52 |
| 290,000 | 74 | 53 |

Populations paid for nothing fall to the floor of 50 instructions within 50,000 updates; the paid ones lose length through the whole run and some reach the floor. Seed 13 found XOR at 141 instructions. This was not a planned measure.

**Why not at all: XOR is found and does not pay.** In the 32 populations of stages 5 and 6 that are paid for the steps and never held XOR there are 462 episodes with ten or more organisms performing it; most last about 200 updates, the longest 9,100, the largest reaches 276 organisms. Three episodes fell on a population save, and their organisms were run in the test CPU beside the most abundant genotype of the same population:

| Population | Most abundant genotype | Organisms performing XOR |
|---|---|---|
| seed 11 at 270,000 | all seven functions below XOR, length 50 | XOR without ANDN, length 50 |
| seed 11 at 70,000 | all seven, length 80 | XOR without ANDN and NOT, some without NOR |
| seed 20 at 110,000 | all seven, length 140 | XOR and all seven, length 133 |

At the resource levels the runs settle at (about 0.7 units a cell for a function most organisms perform) ANDN multiplies merit by about 2.1 and NOR by 2.6, and XOR, new and rare, by 2.0. The ladder pays XOR 2 where the steps added in stage 5 pay 3 and 4, so an organism that turns its ANDN into XOR loses a little. In the classic experiment pay doubles per level. Three episodes are few; they fit the two explanations stage 7 tests, and do not prove them.

Verdict: negative on the primary outcome, accepted by the maintainer on 2026-11-18.

### Stage 7: what keeps populations from XOR

Design, fixed on 2026-11-17 before any stage 7 run. A random search over six settings was designed for this stage first and set aside before it ran (lab folder `17-xor-search`, kept): stage 6 gave two sharp explanations, and a factorial tests those directly.

Three settings of `exact+steps`, each at its present value and at one other, in all eight combinations:

| Setting | Now | Other | Why |
|---|---|---|---|
| Pay of XOR | 2 | 8 | pay then doubles from NOR (4) to XOR, as per level in the classic experiment; turning a step into XOR pays |
| `MIN_GENOME_SIZE` | 50 | 100 | the ancestor's length; genomes cannot shrink below the room they started with |
| `COPY_MUT_PROB` | 0.0075 | 0.0025 | the classic rate; at 0.0075 a genome of 110 instructions takes about 0.8 mutations per copy |

Seeds 31 to 38 in each of the eight arms (64 runs, none of the seeds used before), 100,000 updates, no stopping rule, every other setting of stage 5; the population is saved every 10,000 updates and the resource maps are not written. The runs are dealt over rented servers so that each server has every arm in proportion to its cores.

**Primary outcome.** A population has XOR if it holds it (360 organisms at a sample) at any sample of its run. For each of the three settings: the populations with XOR at the other value against those at the present value, 32 against 32, Fisher's exact test, two-sided, reported uncorrected and with a Holm correction over the three. With about one population in nine finding XOR at the present values, a test reaches p < 0.05 from 12 of 32 against 3 or 4 of 32.

**Secondary, without tests.** The count in each of the eight arms, which shows whether two settings need each other; the time to XOR; whether XOR is still held at 100,000; the highest rung held; the genome length over time in each arm, in particular whether populations with the floor of 100 sit at the floor. The most abundant genotypes of populations that hold XOR are run in the test CPU as before.

The floor of 100 stops genomes from shrinking further and leaves the pressure to shrink in place. If the floor matters, where that pressure comes from is the next question, since the merit setting in use (`BASE_MERIT_METHOD 4`) is meant to make length neutral.

Lab folder `18-xor-factorial`. Before the runs: a 1,500-update trial of four arms wrote every file, and the baseline arm differs from `exact+steps` of stage 5 in its outputs only.

#### Results (2026-11-18)

All 64 runs ran to 100,000 updates, on six rented servers of two cheap types, each carrying all eight arms in equal numbers.

**Primary outcome: the pay of XOR decides it.**

| Setting | Other value | Present value | Fisher p | after Holm |
|---|---|---|---|---|
| Pay of XOR, 8 against 2 | 32 of 32 | 9 of 32 | 4 x 10^-10 | 1 x 10^-9 |
| `COPY_MUT_PROB`, 0.0025 against 0.0075 | 24 of 32 | 17 of 32 | 0.12 | 0.23 |
| `MIN_GENOME_SIZE`, 100 against 50 | 18 of 32 | 23 of 32 | 0.30 | 0.30 |

Per arm (x: pay of XOR; f: floor; m: copy mutation rate in units of 0.0001):

| Arm | XOR ever held | HALFADD_PLANES held at the end | Median first hold of XOR | Median genome length at 100,000 | Median generations |
|---|---|---|---|---|---|
| `x2-f50-m75` (stage 5's arm) | 1 of 8 | 1 | 40,500 | 162 | 7,480 |
| `x2-f100-m75` | 0 of 8 | 0 | | 132 | 7,559 |
| `x2-f50-m25` | 6 of 8 | 3 | 30,400 | 436 | 5,282 |
| `x2-f100-m25` | 2 of 8 | 1 | 35,000 | 418 | 3,193 |
| `x8-f50-m75` | 8 of 8 | 5 | 10,250 | 147 | 7,417 |
| `x8-f100-m75` | 8 of 8 | 2 | 26,200 | 101 | 7,321 |
| `x8-f50-m25` | 8 of 8 | 6 | 8,450 | 290 | 5,288 |
| `x8-f100-m25` | 8 of 8 | 7 | 12,050 | 605 | 2,917 |

Every population that held XOR at some sample held it at the end.

**The mutation rate matters where XOR is poorly paid.** The primary test of the mutation rate is blunted by the arms in which every population finds XOR. At pay 2, XOR was held by 8 of 16 populations at 0.0025 against 1 of 16 at 0.0075 (Fisher p = 0.015; a comparison chosen after seeing the table, so a lead and no more).

**The mutation rate sets the genome length; the floor does not.** At 0.0025 genomes stay at 290 to 605 instructions through the run, at 0.0075 they fall towards 100 to 160. The shrinking seen in stage 6 is the population shedding code that the high rate makes costly to copy. A floor of 100 does nothing for XOR (at pay 2: 2 of 16 against 7 of 16 at floor 50, p = 0.11, the wrong way round if anything). Long genomes are slow: the low-rate arms pass 2,900 to 5,300 generations in the time the others pass 7,400.

**In the test CPU.** The most abundant genotypes of six populations that hold XOR, from five arms, were run on 30 random inputs each. Every genotype that carries XOR performs it in 30 of 30 rounds, and all but one of them HALFADD_PLANES as well, also in 30 of 30. With XOR at 8 several of them have given up NOR (pay 4) or OR, the trade of stage 6 in the profitable direction.

**The wall has moved up one step.** 25 of the 64 populations hold HALFADD_PLANES at the end and none holds anything above it: HALFADD was performed by at most 7 organisms at any sample, ADD2_PLANES by 5, ADD2 by 2. HALFADD_PLANES asks for XOR and AND on one pair of bits; HALFADD asks for the same in two-bit fields laid side by side.

**What this says about stages 5 and 6.** Their wall below XOR was made by the pay scale: the steps added in stage 5 pay 3 and 4 and XOR was left at 2. Neither more time nor islands could help against a reward that pointed the other way.

Verdict: positive for the pay of XOR, no difference shown for the other two settings; accepted by the maintainer on 2026-11-18.


### Stage 8: the rungs above the one-bit half adder

Design, fixed on 2026-11-18 before any stage 8 run.

**What the stage 7 runs show about the next wall.** HALFADD, ADD2_PLANES and ADD2 were performed by at most 7 organisms at any sample. Every genotype of four saved populations that showed such organisms was run in the test CPU, and none performs any of the three, even once: the counts are chance. An organism that outputs plain XOR matches the packed half adder on about one draw in 6,000. So where XOR was found often and not kept, these rungs are not found.

**Why they are hard to find.** HALFADD_PLANES is judged on every second bit, which masks the operands for the organism. The packed rungs compare the whole word, so the organism has to build the mask itself (for HALFADD the constant `0x55555555`, a loop that runs 16 times), mask both inputs, shift the carry and merge; none of these pieces is paid. The hand-written HALFADD has 82 instructions against 49 for HALFADD_PLANES. The pay scale also repeats the fault of stage 5: the four rungs pay 4, 5, 6 and 7, below XOR's 8.

**Additions to the simulator for this stage** (`PORTING.md`): the environment line `INPUT_AND_MASK`, which clears bits of the inputs every organism is given; the argument `width` of `field_arith`, which limits the judged fields to the low bits of the word; and the functions `carry` and `partial`, the two steps of a ripple adder.

**Arms.** All start from `x8-f50-m25` of stage 7 (XOR paid 8, floor 50, copy mutation rate 0.0025). In all of them the four field rungs are judged on the low 24 bits (`width=24`), which leaves the top byte of the inputs, on which the logic tasks rely, out of the arithmetic.

| Arm | Change |
|---|---|
| `base` | none: the rungs above XOR pay 4, 5, 6, 7 |
| `pay` | they pay 9, 10, 11, 12, so that every rung pays more than any rung below it |
| `pay+masked` | `pay`, and `INPUT_AND_MASK 0xFF6DB6DB`: the low 24 bits of every input hold two-bit operands in three-bit fields, the layout of ADD2. ADD2 is then XOR, AND, a shift, a second XOR and AND, a shift and a merge, with no mask to build |
| `pay+masked+steps` | `pay+masked`, and the two steps of the adder are paid on resources of their own: CARRY, `(a & b) << 1`, pays 9 and PARTIAL, `(a ^ b) ^ carry`, pays 10 |

CARRY and PARTIAL are judged in every arm and unpaid outside the last. The masked arms leave HALFADD as hard as it was, since its fields are laid out differently; one mask cannot serve both packed rungs.

Seeds 41 to 52 in each arm (48 runs, none of the seeds used before), 100,000 updates, no stopping rule, outputs as in stage 7.

**Primary outcome.** A population is above the wall if at any sample 360 organisms hold ADD2_PLANES or ADD2. Three planned comparisons, Fisher's exact test, two-sided, 12 against 12, uncorrected and with a Holm correction over the three: `pay` against `base` (is it the pay), `pay+masked` against `pay` (is it the mask), `pay+masked+steps` against `pay+masked` (do paid steps carry populations up). A test reaches p < 0.05 from 5 of 12 against 0 of 12.

**Secondary, without tests.** Each of HALFADD, ADD2_PLANES, ADD2, CARRY and PARTIAL: populations holding it at some sample and at the end, and the time it was first held. Whether a population that holds ADD2 uses `call` for the two rounds (the reuse measures and the test CPU). Genome length and generations as before.

**Known properties of the design.** With masked inputs a plain XOR output equals the two-bit sum whenever no field carries, about one draw in a hundred; such organisms are paid for ADD2 on that draw. This is far below the 360 organisms of the outcome but it is pay for XOR that the unmasked arms do not give. An arm with graded credit (pay in proportion to the bits right) was planned and dropped after a 6,000-update trial of the mechanics: about four in ten organisms were paid for outputs that are right by luck on one draw, more with every further output they made, which rewards outputting often and drains the rungs' resources.

Lab folder `19-above-xor`. Before the runs: a 4,000-update trial of the four arms wrote every file.

#### Results (2026-11-18)

All 48 runs ran to 100,000 updates, on five rented servers of two cheap types.

**Primary outcome: negative.** No population in any arm got above the wall, so each of the three comparisons is 0 of 12 against 0 of 12 (Fisher p = 1).

Populations (of 12) that held a rung at some sample, and the most organisms performing it at any sample in any run:

| Arm | XOR | HALFADD_PLANES | CARRY | PARTIAL | HALFADD | ADD2_PLANES | ADD2 |
|---|---|---|---|---|---|---|---|
| `base` | 11 | 7 | 0 (151) | 0 (7) | 0 (7) | 0 (6) | 0 (7) |
| `pay` | 10 | 7 | 0 (146) | 0 (6) | 0 (4) | 0 (5) | 0 (6) |
| `pay+masked` | 10 | 8 | 0 (147) | 0 (34) | 0 (13) | 0 (4) | 0 (34) |
| `pay+masked+steps` | 9 | 4 | 12 (3,441) | 0 (43) | 0 (18) | 0 (5) | 0 (41) |

**The pay above XOR is not what holds populations back,** and neither is the mask: `pay` and `pay+masked` look like `base`. The higher counts of ADD2 and PARTIAL in the masked arms (34 to 43 organisms) are the chance matches the design names.

**A paid single step is found by every population.** With CARRY paid, all 12 populations hold it, first between updates 2,700 and 21,700, and keep it. Unpaid, it turns up in at most 151 organisms and is held by none.

**The step that combines two results is found by none.** PARTIAL is the XOR of the first sum with the carry. At the end of the run 7 of the 12 populations of `pay+masked+steps` hold XOR and CARRY together in about 3,000 organisms each, the two ingredients, and none holds PARTIAL. Making it asks the organism to keep one computed value while it computes the other and then to apply XOR to the pair, which is a second use of the code it already has for XOR.

**Paying CARRY cost something below it.** In `pay+masked+steps` two populations that held XOR lost it, 5 of 12 end without XOR, and only 4 hold HALFADD_PLANES against 7 or 8 in the other arms. This was not a planned comparison.

Across stages 5 to 8 the pattern is the same: a function that one further step reaches is found within some ten thousand updates once it pays more than what it displaces (XOR in stage 7, CARRY here), and a function that needs two intermediate results kept and combined is not found at all in 100,000 updates.

**In the test CPU.** The four most abundant genotypes of four populations of `pay+masked+steps` were run on 30 draws of masked inputs each. Ten of the sixteen perform CARRY in 30 of 30 rounds; three of those, one in each of three populations, also perform XOR in 30 of 30, and none of the sixteen performs PARTIAL in more than one round. The most abundant genotype is carried by 4 to 15 of the 3,600 organisms, so these are samples of very mixed populations.

Verdict: negative on the primary outcome, accepted by the maintainer on 2026-11-18.

## Candidate future experiments

Ideas from the discussion with the maintainer on 2026-11-18, unordered; the order is decided later. Stage 9 (the first two) is the one being prepared.

- **Expected pay per genotype.** A new genotype is run in the test CPU on many draws of inputs and every copy is paid its average quality, in place of the one draw an organism is judged on now. This removes pay for lucky draws (the half-adder leak of stage 5, the lottery that ruled out graded credit in stage 8) and makes graded credit usable.
- **Pay for information.** From the same test runs, pay a genotype for how much its outputs tell about the target (mutual information), so that an organism that outputs the ingredients of the next step is paid before any single output is right.
- **Competition between organisms as the source of selection.** Host and parasite on computed keys (a parasite infects a host only by performing a function the host performs; Zaman and others 2014 found hosts reach more complex functions with parasites than without), with the ladder kept as unpaid measurement. If that moves populations, a prediction contest between neighbours: an organism gains merit by predicting a value its neighbour computes and the neighbour loses it, which needs new instructions.
- **Differentiable programs** as a fallback: a distribution over instructions at each genome position and fitness as an expectation with a gradient. It replaces mutation and selection with something close to gradient descent, and is kept apart from the evolutionary line for that reason.
- **Wider adders** (ADD3, ADD4; files prepared, layout to be redesigned so that a rung keeps the fields of the rung below).
