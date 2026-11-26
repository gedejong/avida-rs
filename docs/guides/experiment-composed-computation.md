# Can populations evolve composed computation?

This page describes an experiment in progress. It asks whether an instruction set with subroutines helps a population evolve a function that has to be built in several steps, a 2-bit adder made from `nand`, shifts and `inc`. The design below was fixed before the main runs. Results are added at the end as each stage finishes, and a negative result is reported as one.

Everything on this page is an extension of the Rust port. The C++ Avida has none of it, and a configuration that does not ask for it runs exactly as before.

## The pieces

### An instruction set with subroutines

The stock set `heads_modular` (written out in `instset-heads-modular.yaml`) is the stock heads set plus three more registers (DX, EX, FX, addressed with `nop-D`, `nop-E`, `nop-F`) and the instructions `call` and `return`. Select it with `-set instruction_sets heads_modular`. (The runs of stages 3 to 7 used the old file `instset-heads-modular.cfg`, which held the same set.)

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

`environment-adder-ladder.yaml` (named with `-set environment environment-adder-ladder.yaml`; the runs used the old `environment-adder-ladder.cfg`) has eight reactions, each on its own resource that runs short when many organisms use it, each paid once per organism.

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
| Mutations | stock rates, plus `mutation.divide_slip_prob: 0.05` in both arms |
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
| `sex` | `divide-sex` in place of `h-divide`: offspring recombine with another offspring; run with `genealogy.drop_extinct_genotypes: true` (see below) |
| `pop-30` | a 30 x 30 world, resource inflow scaled to the area |
| `pop-120` | a 120 x 120 world, resource inflow scaled to the area |
| `seasons` | NOT, NAND, XOR and AND pay nothing in every other period of 2,000 updates |
| `min-genome` | `divide.min_genome_size: 50`: a divide that would leave fewer than 50 instructions fails |

The first four are the factors planned from the start. The fifth was added after stage 3, because of the genome shrinking seen there; the value 50 is half the ancestor's length and was chosen without a trial run. Seeds 1 to 10, 100,000 updates, exact mode, and the outcomes and tests of stage 3. Each arm is compared with the base only, so five comparisons are made per measure; p values are reported uncorrected and with a Holm correction over the five.

One change was made to the `sex` arm after the batch had started and before any `sex` run finished. With two parents every ancestor stays in memory as a historic genotype, and the first six `sex` runs grew to more than 4 GB each and were stopped at about 65,000 updates; their output is not used. The arm now runs with the new setting `genealogy.drop_extinct_genotypes: true`, which drops a genotype when its last organism dies and changes nothing that organisms compute (the data files of a run are byte-identical with and without it). The measures that need ancestry, MODES complexity and the phylogeny measures, are therefore not available for the `sex` arm. Its primary outcome, the genome length, the tasks performed and the reuse measures are reported as for the other arms.

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
| `exact+steps+islands` | ten demes of 60 x 6 cells (`demes.count: 10`); an offspring is born in another deme, at a random cell, with probability 0.001 (`demes.migration_rate`) |

The four steps are those of the stock logic environment, with its values. In the classic experiment on this question (Lenski and others, 2003) populations reached the hardest logic function only when the simpler ones in between were paid. Every arm lists the same twelve reactions in the same order (NOT, NAND, AND, ORN, OR, ANDN, NOR, XOR and the four field rungs), so that the output files agree; in `exact` the four steps are worth nothing, and an organism that performs one is counted without being paid. The migration rate was chosen without a trial run: with 360 organisms in a deme it is about one migrant per deme in three generations.

Seeds 1 to 10, 100,000 updates, exact mode, modular set, a 60 x 60 world, `divide.min_genome_size: 50`, the other settings of stage 4. Primary outcome: the number of seeds that hold XOR at the last sample. Secondary: the same for HALFADD_PLANES, which at full quality is a real half adder; the highest rung held; the first update at which XOR is held; and the measures of stage 4. Tests: Fisher's exact test, two-sided, for `exact+steps` against `exact` and for `exact+steps+islands` against `exact+steps`, reported uncorrected and with a Holm correction over the two. For each arm the most abundant genotypes of the runs that hold XOR or more are run in the test CPU and their outputs compared with the functions paid, which is the check that was missing in stages 3 and 4.

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
| `divide.min_genome_size` | 50 | 100 | the ancestor's length; genomes cannot shrink below the room they started with |
| `mutation.copy_prob` | 0.0075 | 0.0025 | the classic rate; at 0.0075 a genome of 110 instructions takes about 0.8 mutations per copy |

Seeds 31 to 38 in each of the eight arms (64 runs, none of the seeds used before), 100,000 updates, no stopping rule, every other setting of stage 5; the population is saved every 10,000 updates and the resource maps are not written. The runs are dealt over rented servers so that each server has every arm in proportion to its cores.

**Primary outcome.** A population has XOR if it holds it (360 organisms at a sample) at any sample of its run. For each of the three settings: the populations with XOR at the other value against those at the present value, 32 against 32, Fisher's exact test, two-sided, reported uncorrected and with a Holm correction over the three. With about one population in nine finding XOR at the present values, a test reaches p < 0.05 from 12 of 32 against 3 or 4 of 32.

**Secondary, without tests.** The count in each of the eight arms, which shows whether two settings need each other; the time to XOR; whether XOR is still held at 100,000; the highest rung held; the genome length over time in each arm, in particular whether populations with the floor of 100 sit at the floor. The most abundant genotypes of populations that hold XOR are run in the test CPU as before.

The floor of 100 stops genomes from shrinking further and leaves the pressure to shrink in place. If the floor matters, where that pressure comes from is the next question, since the merit setting in use (`time.base_merit_method: least_size`) is meant to make length neutral.

Lab folder `18-xor-factorial`. Before the runs: a 1,500-update trial of four arms wrote every file, and the baseline arm differs from `exact+steps` of stage 5 in its outputs only.

#### Results (2026-11-18)

All 64 runs ran to 100,000 updates, on six rented servers of two cheap types, each carrying all eight arms in equal numbers.

**Primary outcome: the pay of XOR decides it.**

| Setting | Other value | Present value | Fisher p | after Holm |
|---|---|---|---|---|
| Pay of XOR, 8 against 2 | 32 of 32 | 9 of 32 | 4 x 10^-10 | 1 x 10^-9 |
| `mutation.copy_prob`, 0.0025 against 0.0075 | 24 of 32 | 17 of 32 | 0.12 | 0.23 |
| `divide.min_genome_size`, 100 against 50 | 18 of 32 | 23 of 32 | 0.30 | 0.30 |

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

### Stage 9: graded pay, with and without the genotype's expected quality

Stage 8 left one step unmade: combining two computed values. An exact rung pays nothing for an answer that is nearly right, so the population gets no signal on the way there. Graded pay gives that signal, and on masked inputs the partly right answers form a slope of their own. The average quality of an answer against the packed two-bit adder, over all operand pairs:

| Answer | Bits right | Quality |
|---|---|---|
| an input, or zero | 54 % | 0.08 |
| `a OR b` | 67 % | 0.33 |
| `a XOR b` | 79 % | 0.58 |
| `(a XOR b) OR carry` | 92 % | 0.83 |
| `(a XOR b) XOR carry` (PARTIAL) | 96 % | 0.92 |
| the sum | 100 % | 1.00 |

A trial before stage 8 showed why graded pay failed on a single draw: with eight fields per draw the quality of one answer scatters by about 0.16, so a threshold is passed by luck in a large share of organisms. Expected quality (`expect=1`, `PORTING.md`) pays a genotype its average over 8 test runs and nothing below an average of 0.5, which takes the luck out.

**Question.** Does tiered pay for the two-bit adder get populations past the combining step, and does it need expected quality to do so?

**Arms** (lab folder `20-expected-pay`), each 12 populations, seeds 41 to 52, on the masked inputs of stage 8:

| Arm | Change |
|---|---|
| `exact` | the 12 runs of `pay+masked` from stage 8, not run again |
| `tiers` | three tiers of ADD2 are paid on the organism's own draw: quality at least 0.45 pays 9, at least 0.70 pays 10, at least 0.85 pays 11 |
| `tiers+expected` | the same tiers, paid on the genotype's expected quality |

The tiers are separate reactions because a reaction pays once per gestation: an organism that outputs XOR first and the sum later collects the lowest tier on the first output and the higher ones on the second.

**Measurement.** ADD2 (exact, pay 12), PARTIAL and CARRY (exact, unpaid) are judged on the organism's own draw in every arm, so the outcome is measured the same way whatever the pay rule.

**Primary outcome.** Populations in which exact ADD2 is held (360 organisms at a sample). Two planned comparisons, Fisher exact with Holm correction: `tiers+expected` against `exact`, and `tiers+expected` against `tiers`.

**Secondary.** Populations holding PARTIAL; the highest tier held in each population; the time to each.

**Known weakness.** Plain XOR earns the lowest tier, so that tier pays for nothing new. It is kept because without it there is no first step on the slope.

#### Results (2026-11-19)

All 24 runs ran to 100,000 updates. One run of `tiers+expected` was run a second time because its server stopped answering before the result was fetched; a run with the same seed gives the same result.

**Primary outcome: negative.** No population in any arm held the exact two-bit adder: 0 of 12 in `tiers+expected` against 0 of 12 in `exact` and against 0 of 12 in `tiers` (Fisher p = 1 for both).

Populations (of 12) that held a rung at some sample, and the most organisms performing it at any sample in any run:

| Arm | XOR | HALFADD_PLANES | lowest tier | middle tier | top tier | PARTIAL | ADD2 |
|---|---|---|---|---|---|---|---|
| `exact` | 10 | 8 | | | | 0 (34) | 0 (34) |
| `tiers` | 5 | 1 | 12 | 6 | 0 (211) | 0 (71) | 0 (69) |
| `tiers+expected` | 12 | 4 | 12 | 2 | 0 (10) | 0 (82) | 0 (82) |

**Expected quality takes the luck out, as designed.** On a single draw the lowest tier is held by all 12 populations while only 5 hold XOR. With expected quality all 12 hold XOR (12 of 12 against 5 of 12, Fisher p = 0.005, not a planned comparison), and the lowest tier is first held within a few thousand updates of XOR in every population. The middle tier is held at the end by 4 populations on a single draw and by none with expected quality.

**It does not carry a population over the combining step.** PARTIAL is performed by at most 82 organisms at any sample, CARRY by at most 269, and the top tier by at most 10 in `tiers+expected`.

**The tiers pay for many different outputs, which the design did not foresee.** A reaction fires on the first output that passes its threshold, and the organisms of these populations output a series of different logic results in one gestation. Each output is one more chance to land near the sum. In the test CPU (the four most abundant genotypes of four populations, 30 draws of masked inputs each) genotypes that do not perform XOR pass the lowest tier in 20 to 29 of 30 draws; genotypes that perform every logic task pass the middle tier in 12 to 18 draws and the top tier in 2 to 5. Expected quality averages over draws of inputs and does not touch this: a genotype that reliably outputs ten values reliably passes a low threshold. None of the sixteen genotypes performs CARRY or PARTIAL in more than four draws. The table of average qualities in the design above holds for an organism with one output and does not describe these organisms.

**The tiers cost something below them.** HALFADD_PLANES is held by 8 populations in `exact`, 1 in `tiers` and 4 in `tiers+expected`. This was not a planned comparison.

Across stages 8 and 9 the step that combines two computed values has been offered exact pay, a paid step on either side of it, graded pay and graded pay without the luck, and no population has made it.

Verdict: negative on the primary outcome, accepted by the maintainer on 2026-11-19.

### After stage 9: how far away is the combining step

A check on the saved populations of stage 8, not a stage of its own. From five populations of `pay+masked+steps` that hold XOR and CARRY, the most abundant genotype that performs both in 30 of 30 draws was taken (two from one population), and every single mutation of it was run in the test CPU on three draws of masked inputs: each instruction of the set at each position as a substitution and as an insertion, and each deletion.

| Genotype (seed, length) | Single mutants | Keep XOR and CARRY | PARTIAL or ADD2 in 2 of 3 draws |
|---|---|---|---|
| 41, 635 | 36,830 | 26,505 | 0 |
| 41, 635 (second) | 36,830 | 26,458 | 0 |
| 43, 827 | 47,966 | 38,776 | 0 |
| 44, 655 | 37,990 | 32,031 | 0 |
| 47, 250 | 14,500 | 8,834 | 0 |

**No single mutation of any of the five makes the combining step,** out of 174,116 tried. Seven in ten leave XOR and CARRY intact, so the populations can drift, but the step itself needs at least two changes, and nothing on the ladder pays for the first of them. That fits what stages 8 and 9 found: pay on the far side of the step, exact or graded, cannot select for a first change that earns nothing.

Limits: duplications of a stretch of the genome (slip mutations, which these runs allow) were not scanned, and a mutant that makes the step on fewer than two of three draws would be missed.

**The logic steps on the two computed values are no closer.** The same scan was repeated on four of the genotypes (137,286 single mutants) for five new exact functions of `x = a ^ b` and `c = (a & b) << 1` (`field_arith` with `fn=nand_xc`, `and_xc`, `or_xc`, `andn_xc`, `andn_cx`), the steps from which PARTIAL (`x ^ c`) would be composed as XOR was composed in stage 7. It used only input draws on which no step equals `x`, `c`, zero or all ones for any pair of inputs. No mutant performs `or_xc`, `andn_xc` or `andn_cx` in 2 of 3 draws. 121 mutants pass `nand_xc` or `and_xc` in 2 of 3 draws, and none of them in more than 17 of 30. On ordinary draws `x & c` is zero in about a third of the cases, so `nand_xc`, `and_xc` and `andn_cx` would also be paid by coincidence. A ladder of these steps was therefore not run: what is missing is any output that depends on both computed values, whatever the function.

### Stage 10: the same ladder on the experimental hardware

**Question.** Does the hardware change what populations reach above XOR? The experimental hardware (type 3) finds its labels by direct search and has no `call`.

**Arms** (lab folder `23-heads-ex`), each 12 populations, seeds 41 to 52, 100,000 updates, on the ladder of stage 8 with both adder steps paid (CARRY 9, PARTIAL 10) and masked inputs:

| Arm | Change |
|---|---|
| `modular` | the modular heads set, as in stage 8 |
| `ex` | a heads-style set on the experimental hardware (`label`, `search-lbl-direct-s`, `if-copied-lbl-direct`), with an ancestor written for it |

#### Results (2026-11-20)

Populations (of 12) that hold a rung at the end:

| Arm | CARRY | XOR | HALFADD_PLANES | PARTIAL | ADD2 |
|---|---|---|---|---|---|
| `modular` | 12 | 7 | 4 | 0 | 0 |
| `ex` | 12 | 9 | 7 | 0 | 0 |

**Negative.** Neither arm reaches PARTIAL or ADD2, and the differences below them are within chance (Fisher p about 0.67 for XOR and 0.41 for HALFADD_PLANES). The hardware is not what holds populations below the combining step.

Verdict: negative, accepted by the maintainer on 2026-11-20.

### Stage 11: one judged output, graded pay, and gate instructions

Stage 9 ended with two lessons: graded pay must judge one designated output, and nothing pays for the first of the two or more changes the combining step needs. This stage changes both.

**One judged output with graded pay.** A reaction with `requisite: {divide_only: 1}` is tested at divide, on the organism's newest output, once per life. The reaction GRADED pays `2^(12 q)` for the quality `q` of that output against the packed two-bit sum (`field_arith`, `fn=add:bits=2:mode=packed:width=24:thresh=0.25`; quality is `max(0, 2f - 1)` for a share `f` of right bits). It uses no resource. Four unpaid reactions on the same output (Q50, Q70, Q90, Q100; quality at least 0.5, 0.7, 0.9 and exactly 1.0) are the measurement. The eight logic rungs NOT to XOR are paid as in stage 8, on masked inputs.

**Gate instructions** (`PORTING.md`). `gate-0` to `gate-15` each write one two-input logic function of BX and CX to a register; the instruction's number is the function's truth table, so `gate-6` is XOR and `gate-8` is AND. A copy mutation that hits a gate writes, three times in four, the gate whose table differs in one entry, so one mutation changes the result on a quarter of the input pairs.

What one output scores against the packed two-bit sum, calculated over masked inputs:

| Output | Mean quality | Pass 0.7 | Pass 0.9 | Exactly 1.0 |
|---|---|---|---|---|
| `a XOR b` | 0.58 | 26 % | 5 % | 1 % |
| XOR, with the shifted carry merged by OR | 0.83 | 74 % | 35 % | 35 % |
| XOR, with the shifted carry merged by XOR (the sum after one carry round) | 0.92 | 99 % | 74 % | 35 % |
| the sum | 1.00 | 100 % | 100 % | 100 % |

The sum is the one-round sum combined with a second carry. The half-built forms of that second round score no better than the one-round sum.

**Arms** (lab folder `24-soft-gates`), each 12 populations, seeds 41 to 52, 100,000 updates:

| Arm | Instruction set |
|---|---|
| `graded` | the modular heads set |
| `graded+gates` | the same with `gate-0` to `gate-15` |

#### Results (2026-11-20)

The outcome measure was chosen after the data were seen: a population is past the XOR level when most of the organisms that pass Q50 also pass Q70.

| Arm | Past the XOR level | Hold the sum |
|---|---|---|
| `graded` | 0 of 12 | 0 |
| `graded+gates` | 5 of 12 | 0 |

**With gates, five populations get past XOR; without, none** (Fisher exact p = 0.037). They crossed at updates 28,900 to 95,300. In `graded` seven of twelve populations lost XOR during the run.

**What the five compute**, from the most common organism of each run alone with mutation off: three match the one-round sum (99 to 100 % pass 0.7, 55 to 71 % pass 0.9, 21 to 43 % score 1.0), one is an intermediate below it, and one had too few offspring to judge. For the first time a population's output depends on two computed values, the step no population made in stages 8 and 9. The genomes are 224 to 1,019 instructions long and mostly gates.

Verdict: positive for the step past XOR, on a measure chosen after the fact; no population holds the sum. Accepted by the maintainer on 2026-11-20.

### Stage 12: three times the run length, and whether the gate mutation rule matters

**Question.** Does the sum appear with more time, does the gates' special mutation rule matter, and does a higher mutation rate help?

**Arms** (lab folder `25-gates-long`), each 10 populations, seeds 41 to 50, 300,000 updates, on the environment of stage 11:

| Arm | Change |
|---|---|
| `gates` | the `graded+gates` arm of stage 11 |
| `pgates` | `pgate-0` to `pgate-15` in place of the gates: the same functions, and a copy mutation writes a random instruction as for any other |
| `gates-mut3x` | `gates` with a copy mutation rate of 0.0075 in place of 0.0025 |

#### Results (2026-11-20)

All 30 runs ran to 300,000 updates. Populations at the end, by the shares of Q70, Q90 and Q100 among the organisms that pass Q50:

| Arm | XOR level or just above | One-round sum | Other intermediate | The sum |
|---|---|---|---|---|
| `gates` | 4 | 4 | 1 | 1 |
| `pgates` | 5 | 2 | 1 | 2 |
| `gates-mut3x` | 5 | 4 | 0 | 1 |

**Four populations hold the two-bit adder.** The most common organism of each, alone with mutation off, scores exactly 1.0 on every judged output:

| Population | Genome length | Judged outputs | Exactly 1.0 |
|---|---|---|---|
| `pgates` seed 42 | 448 | 1,735 | 100 % |
| `pgates` seed 44 | 211 | 10,679 | 100 % |
| `gates-mut3x` seed 49 | 190 | 10,800 | 100 % |
| `gates` seed 46 | 398 | 2,743 | 100 % |
| `gates` seed 44 (one-round sum, for comparison) | 332 | 5,062 | 31.7 % |

In three of them 95 to 98 % of the population scores 1.0 at the end; in `gates` seed 46 the sum was still spreading (84 %).

**No arm stands out.** One, two and one of ten is no difference: neither the gates' mutation rule nor the tripled mutation rate shows a benefit. The two arms with the rule and the one without it get past the XOR level equally often (6, 6 and 5 of 10).

**Time mattered.** Three of the four reached the sum after update 150,000; stage 11 stopped at 100,000.

**How they got there:**

| Population | Route | The sum spreads in |
|---|---|---|
| `pgates` 42 | one-round sum from update 80,000 to 155,000 | about 2,200 updates |
| `gates-mut3x` 49 | XOR level until 147,000, one-round sum until 188,000 | about 4,000 updates |
| `pgates` 44 | the OR form from 40,000 to 70,000, never the one-round sum | about 4,000 updates |
| `gates` 46 | one-round sum from 36,000 to 250,000 | more than 50,000 updates, unfinished |

There are at least two routes, and the wait below the sum ranges from 30,000 to more than 200,000 updates.

Limits: the stage has no arm without gate instructions at this run length (stage 11 had one at 100,000 updates), and the classes of the first table are read from shares, with only the eight most telling populations tested directly.

Verdict: positive, the two-bit adder evolved in 4 of 30 populations. Accepted by the maintainer on 2026-11-20.

### Stage 13: wider sums, the size of the pay step, and the gates over a long run

Design fixed on 2026-11-21, before the main runs. Lab folder `28-weekend`. Every arm uses the environment and the instruction sets of stage 12; only the judged sum, the input mask and the pay of GRADED change. Copy mutation rate 0.0025 in every arm.

**Ancestors.** The most common organism of each of the four populations that hold the two-bit sum at the end of stage 12: `p42` and `p44` (`pgates` seeds 42 and 44; 448 and 211 instructions, pgate set), `m49` (`gates-mut3x` seed 49; 190, gate set; it evolved at a copy mutation rate of 0.0075) and `g46` (`gates` seed 46; 398, gate set).

**Wider sums.** `field_arith` with `bits=3` (four-bit fields, six of them in the low 24 bits, input mask `0xFF777777`) or `bits=4` (five-bit fields, four of them, mask `0xFF07BDEF`). The Q marks judge the same sum as GRADED.

What a ripple adder that stops after a given number of carry rounds scores, calculated over masked inputs:

| Judged sum | Adder | Mean quality | Exactly 1.0 |
|---|---|---|---|
| three bits | the two-bit adder | 0.967 | 67 % |
| four bits | the two-bit adder | 0.937 | 58 % |
| four bits | the three-bit adder | 0.988 | 88 % |

Measured before the runs, each ancestor alone with mutation off: against the three-bit sum 51 to 60 % of its outputs are exact (`p44` 52 %, the others 57 to 60 %); against the four-bit sum 9 % (`p44`) and 20 to 25 % (the others). The evolved adders are partly tied to the field layout they evolved on.

**Pay.** At `2^(12 q)` the last step to the three-bit sum is worth 1.31 times the pay, against 1.94 times for the last step to the two-bit sum in stage 12. At `2^(32 q)` it is worth 2.06 times. For four bits, `2^(64 q)` makes the last step (from the three-bit adder) worth 1.70 times.

**Arms:**

| Arm | Start | Judged and paid | Seeds | Updates |
|---|---|---|---|---|
| `v12-*` | each of the four ancestors | three-bit sum, `2^(12 q)` | 61 to 63 | 300,000 |
| `v32-*` | each of the four ancestors | three-bit sum, `2^(32 q)` | 61 to 63 | 300,000 |
| `w4-*` | each of the four ancestors | four-bit sum, `2^(64 q)` | 41 to 43 | 3,000,000 |
| `pgates-long` | the modular ancestor | two-bit sum, as `pgates` in stage 12 | 51 to 57 | 3,000,000 |
| `nogates-long` | the modular ancestor | two-bit sum, as `graded` in stage 11 (no gates) | 51 to 57 | 3,000,000 |

A population with its line of descent is saved every 50,000 updates.

**Questions and primary outcomes.**

1. Does the size of the pay step decide whether the three-bit sum is reached? Populations holding the three-bit sum at 300,000 updates (at least 90 % of the organisms that pass Q50 score 1.0, confirmed on the most common organism alone with mutation off), `v32` against `v12`, 12 against 12, Fisher exact.
2. Does the climb go on to four bits? Populations of `w4` holding the four-bit sum at the end, of 12, by the same rule. Secondary: the time to the three-bit level (a share at 1.0 near 88 %) and to the sum; how the sum is built (a carry round written out again, a loop or `call`), from the line of descent and the genomes.
3. Are the gate instructions necessary given time? Populations past the XOR level at the end (most organisms that pass Q50 also pass Q70, the rule of stages 11 and 12), `pgates-long` against `nogates-long`, 7 against 7, Fisher exact. Secondary: populations holding the two-bit sum, and the time to each.

**Seen before the runs.** In a 3,000-update smoke run of `v32-p44` (seed 41, not one of the seeds above) 91 % of judged outputs were exact at the end, and 8 of the 10 most common genotypes compute the three-bit sum alone. The three-bit step can therefore be fast, which is why the three-bit runs are short.

**Cut-off.** The runs are on rented servers with 32 cores in all (four with 8 cores each). At 08:00 CET on 2026-11-24 any run still going is stopped; a long arm is then compared at the update its slowest run reached.

**Change on 2026-11-21, after about 330,000 updates** (agreed with the maintainer the same day). By then 9 of the 12 `w4` populations showed the four-bit pattern (96 to 98 % of judged outputs exact), held from update 6,200 to 67,600, and the three `g46` populations stood at 83 to 89 %. In five populations (`m49` seed 41, `p44` seed 42, `p42` seed 43, `g46` seeds 41 and 43) most of the four most common genotypes compute the four-bit sum exactly alone with mutation off. Question 2 was answered, and nothing in the design pushes these populations further: on random inputs long carry chains are rare, so a four-bit adder is already exact on 88 % of eight-bit sums and a six-bit adder on 99 %, and wider sums need inputs with long carry chains. The `w4` runs were stopped between updates 380,600 and 432,100, and their cores went to a new arm:

| Arm | Start | Judged and paid | Seeds | Updates |
|---|---|---|---|---|
| `s4` | the modular ancestor, pgate set | four-bit sum, `2^(64 q)` | 51 to 62 | 3,000,000 |

4. Can a population climb from scratch to the four-bit sum? Populations of `s4` holding the four-bit sum at the end, of 12, by the rule of question 1. On the four-bit judge XOR scores 0.39, the one-round sum 0.79, the two-bit adder 0.94 and the three-bit adder 0.99. Secondary: the time to each level, compared with the time to the two-bit sum in `pgates-long`.

The stopped slots took queued three-bit runs, so for some hours each server runs 10 or 11 simulations on 8 cores.

#### Results (2026-11-24)

All runs ended as planned except two: `nogates-long` seed 51 ran slower than the others (a genome of about 50 instructions for most of the run) and was stopped at the cut-off at update 2,724,400, so the arm `nogates-long` is compared at that update; the `w4` runs were stopped early, as described above. The batch cost EUR 9.80. Every population counted below was confirmed on its most common organism, alone, with mutation off, on the last saved population (update 2,700,000 for `nogates-long`).

**Question 1: the size of the pay step does not decide the three-bit sum.** Populations holding it at 300,000 updates: 9 of 12 at `2^(12 q)` against 11 of 12 at `2^(32 q)` (Fisher exact p = 0.59).

| Ancestor | `v12`, share exact at the end | `v32` |
|---|---|---|
| `m49` | 97, 98, 97 % | 99, 98, 98 % |
| `p42` | 97, 95, 96 % | 98, 98, 98 % |
| `p44` | 96, 96, 96 % | 97, 98, 97 % |
| `g46` | 81, 89, 64 % | 91, 92, 87 % |

All misses come from `g46`. In three of its four populations below the bar the most common organism computes the three-bit sum exactly alone; the share is pulled down by offspring that do not, and its genomes are the longest (the most common organisms have 315 to 530 instructions). Only `v12-g46` seed 63 has not reached it (its most common organism is exact on 66 % of outputs).

**Question 2: the climb from the two-bit to the four-bit sum is short.** When the `w4` runs were stopped (updates 380,600 to 432,100), 10 of 12 populations held the four-bit sum by the rule, and in all 12 the most common organism computes it exactly alone. The two below the bar are `g46` seeds 41 and 43 (83 and 89.9 % exact). In the nine populations of `m49`, `p42` and `p44` the share stayed above 90 % from update 6,200 to 67,600. The genomes did not grow by a block per carry round: the most common organisms are 254 to 383 instructions long in eight of those nine (621 in `m49` seed 42), against 190 to 448 in the ancestors.

**Question 3: with gate instructions populations get past XOR; without them few do.**

| Arm | Past the XOR level | One-round sum or higher | Two-bit sum |
|---|---|---|---|
| `pgates-long` | 7 of 7 | 7 of 7 | 4 of 7 |
| `nogates-long` | 2 of 7 | 0 of 7 | 0 of 7 |

Past the XOR level: Fisher exact p = 0.021 (the planned comparison); the two-bit sum: p = 0.070. With gates, the share passing 0.7 first stayed above half at updates 57,600 to 963,100, and the four populations that hold the sum held it above 90 % from updates 112,400, 184,600, 206,900 and 413,800. Of the other three, seed 57 is exact on 71 % of outputs (its most common organism on 76.5 %), and seeds 51 and 56 are at the one-round sum (organisms 99/77/37 % and 99/71/31 % passing 0.7/0.9/1.0; calculated 99/74/35 %). Without gates, seed 55 is at the OR-merge form (organism 70/30/30 %; calculated 74/35/35 %) from update 201,800, and seed 56 a little past it (88/50/31 %) from 559,600; the five others are at the XOR level (26 to 29 % pass 0.7; calculated 26 %) after 2.7 million updates.

**Question 4: one population climbed from scratch to the four-bit sum, quickly; others followed late or not at all.** The shares passing 0.7/0.9/1.0 that each form gets on the four-bit judge, among outputs that pass 0.5: XOR 41/2/2 %, the OR-merge form 65/22/22 %, the one-round sum 84/18/18 %, the two-bit adder 99/58/58 %, the three-bit adder 100/88/88 %.

| Most common organism alone at 3,000,000 | Seeds | Population share exact |
|---|---|---|
| the four-bit sum, exact on every output | 52, 51 | 97 %, 87 % |
| exact on 96.6 % | 57 | 88 % |
| near the three-bit adder (85.8 % exact) | 56 | 79 % |
| between the one-round sum and the two-bit adder (89 to 98/54 to 68/24 to 32 %) | 53, 54, 55, 59, 61, 62 | 10 to 16 % |
| between XOR and the one-round sum (78 to 80/23 to 25/4 to 5 %) | 58, 60 | 2 to 3 % |

By the rule 1 of 12 holds the four-bit sum: seed 52, above 90 % from update 81,100, half of its outputs exact from 8,200. That is faster than any `pgates-long` population reached the two-bit sum. Seed 51 computes the four-bit sum exactly but stays below the bar on the share (its genome is 232 instructions; 87 % of outputs exact). Seeds 51, 56 and 57 were at the lower levels for more than a million updates: half of their outputs were exact from updates 1,188,200, 2,397,400 and 2,212,000. Seed 57 held above 90 % from 2,347,900 and fell back to 88 % by the end.

**Reading.** The step that combines two computed values, which no population made in stages 8 to 10, is made reliably once it has been made once: from an evolved two-bit adder the three- and four-bit sums followed in nearly every population, at either pay step. From scratch the obstacle is the first combining step, as before: with gate instructions every population passed XOR and four of seven reached the two-bit sum in 3 million updates, without them none reached even the one-round sum. With the four-bit judge one population of twelve went from scratch to the four-bit sum in 81,000 updates; whether the steeper pay of that judge (`2^(64 q)`, so that one step up from XOR multiplies the pay by about 2^25) explains it was not tested.

Limits: the three- and four-bit questions start from four ancestors only, so 12 populations per arm are 4 lineages, not 12 independent origins; the `w4` arms were stopped after about 400,000 updates on the maintainer's agreement once their outcome was clear; and the comparison of pay steps covers only the step from two to three bits.

Verdict: positive on questions 2 and 3, negative on question 1 (no effect of the pay step), and 1 of 12 on question 4. Accepted by the maintainer on 2026-11-24.

### After stage 13: how the adders work and how they were built

A study of saved populations, not a stage of its own (2026-11-24, lab folder `29-lineage`). Nine lines of descent: stage 12 `pgates` seeds 42 and 44, `gates-mut3x` seed 49 and `gates` seed 46; stage 13 `pgates-long` seed 54, `s4` seeds 51 and 52, `w4-m49` seed 42 and `w4-g46` seed 42.

**Method.** The line of descent runs from the most abundant genotype of the last saved population back to the injected ancestor, through the parent column of the saved population (1,519 to 38,311 genotypes). Every genotype on it was run in the test CPU on 100 draws of masked inputs, and its level read from the shares passing 0.7, 0.9 and 1.0, as in the stages. A change of level counts when it holds for the next 5 genotypes (40 on the lines from scratch at 3 million updates). Where one birth brought several changes, every subset of them was run on 200 draws to find which were needed. For the logic, the most abundant genotype of each line was traced in the test CPU on inputs whose sum needs every carry round; the dataflow to the judged output was rebuilt from the instructions (the destination register from the `nop` that follows) and checked against the sum on every pair of field values. The tools are a scratch test, `crates/hardware/tests/zz_logic.rs` (not committed), and the scripts in the lab folder.

**All nine adders are straight-line code and exact on every pair of field values.** None uses a loop or `call`; every value on the way to the output is computed once, at its own place in the genome. They use two algorithms.

*Ripple with the sum updated in place* (6 of 9: stage 12 `pgates` 42 and 44, `gates-mut3x` 49, `pgates-long` 54, `s4` 52, `w4-m49` 42). With `s0 = a ^ b` and the carry word `c1 = (a & b) << 1`, each round is

| Instruction | Effect |
|---|---|
| `gate-4` (AND NOT) | BX = BX & ~CX: the carry, minus the bits where the new sum is set |
| `shift-l` | BX = BX << 1: the next carry word |
| `gate-6`, `nop-C` (XOR) | CX = BX ^ CX: the next partial sum |

The next carry is taken from the new partial sum (`c & ~(s ^ c)` is `c & s`), so the round needs two registers only, carry in BX and partial sum in CX, and leaves them in the same layout. Four lines use this round, sometimes with an instruction of no effect in between; three of them found it independently from scratch (`gates-mut3x` 49 in stage 12 with gates, `pgates-long` 54 and `s4` 52 in stage 13 with pgates), and `w4-m49` 42 inherited it; `pgates` 44 uses the mirrored gate (`gate-2`, `~BX & CX`); `pgates` 42 keeps the carry complemented, in a five-instruction round (NOR, XOR, `shift-l`, `inc`, XNOR: shifting `~x` left and adding one gives `~(x << 1)`). Some merge the last carry with OR, which is exact once no carry is left. The four-bit adder of `s4` 52 is 13 operations between positions 327 and 344 of its 375 instructions.

*Carry vector by iteration* (3 of 9: `gates` 46 of stage 12, its descendant `w4-g46` 42, and `s4` 51). With `g = a & b` and `t = a ^ b` (or `a | b`), the organism computes the carry into every bit at once, `K = (g | (t & K)) << 1` repeated from `K = g << 1`, and outputs `a ^ b ^ K`. A round needs `g`, `t` and `K` at the same time; with six registers, these organisms read the inputs again with `IO` inside the rounds, and their genomes are longer (232 to 575 instructions).

**The first combining step took one change in each of the five lines where it was tested.** In the births that took a population from the XOR level to the OR-merge form or the one-round sum, the parent was one change away from the step; the other changes of the same birth were not needed.

| Line | Update | The change that was needed |
|---|---|---|
| `pgates` 42 | 49,725 | `pgate-3` (NOT) to `pgate-1` (NOR); two more substitutions in the birth not needed |
| `pgates` 44 | 28,835 | one instruction inserted |
| `gates-mut3x` 49 | 145,552 | `nop-C` to `nop-B`, which sends a gate's result to another register; two more not needed |
| `gates` 46 | 28,245 | `nop-C` to `gate-6` (XOR); one more not needed |
| `pgates-long` 54 | 54,731 | `nop-C` to `pgate-4` (AND NOT); four more not needed |

**A population at the XOR level is not one mutation from the step; the lines got there by a change that did nothing at the time.** The one-mutation neighbourhood of a genotype here is every single substitution and every tandem duplication of up to 60 instructions, each run in the test CPU on 20 draws, the candidates again on 400; a neighbour gets past the XOR level when more than 60 % of its outputs passing 0.5 also pass 0.7 (XOR 26 to 38 %, the OR-merge form 74 %). The most common organism at update 50,000 of each of the seven `pgates-long` and the seven `nogates-long` populations of stage 13 has no such neighbour. On the lines of descent, the parent of the step has 3 to 34 (the change that was made among them), and going back along each line, the first genotype that has one is:

| Line | One mutation from the step since | The change that put it there | Its effect then | Step made at |
|---|---|---|---|---|
| `pgates` 42 | update 39,923 | `if-n-equ` to `pgate-11` (with a second substitution not needed) | outputs passing 0.5 from 77 to 88 %, the others unchanged | 49,725 |
| `gates` 46 | 24,670 | `swap-stk` to `nop-D` after a `gate-8` | none | 28,245 |
| `gates-mut3x` 49 | 145,219 | `gate-5` to `if-n-equ` | none | 145,552 |
| `pgates-long` 54 | 51,773 | `swap-stk` to `pgate-14` (with a second substitution not needed) | outputs passing 0.5 from 77 to 93 %, of those passing 0.7 from 34 to 44 % | 54,731 |

In the genotype that made the step, undoing that change takes the step's gain away again (back to the XOR level in `pgates` 42, `gates` 46 and `pgates-long` 54; `gates-mut3x` 49 then passes no 0.5 at all). The wait between the two was 333 to 9,800 updates. What the changes did can be read in two of them. In `gates-mut3x` 49 the carry round (`gate-4`, `shift-l`, `gate-6`) was already written out, but a `gate-5` between its first two instructions wrote `~CX` into BX and destroyed the carry; the `if-n-equ` in its place skips `shift-l` only when carry and partial sum are equal, which here means both are 0, so the round became intact while the output still came from another register, and the step (`nop-C` to `nop-B` after an `IO`) output it. In `gates` 46 the `nop-D` sent the result of `gate-8`, `a & b`, into DX, where nothing overwrote it. In both, a computed value was kept alive before anything used it. Lenski and others (2003) found that complex functions in Avida came by way of mutations that were neutral or even harmful when they arose; Blount, Borland and Lenski (2008) call such a preparing change in *E. coli* potentiation.

The single-mutant scan after stage 9 found no single change that made the step either; the difference with gates lies in what the lines found a few changes earlier, and four lines do not say how often that happens without gates.

**Each further carry round came, in 12 of 15 steps, from a tandem duplication of the round's code.** A slip mutation (`divide_slip_prob` 0.05) copied a stretch of the genome next to itself, and the copy held a round.

| Line | Step | Update | Duplicated stretch |
|---|---|---|---|
| `pgates` 42 | one-round sum to two-bit sum | 152,526 | 13 instructions |
| `pgates` 44 | OR-merge form to two-bit sum | 68,810 | 14 |
| `gates-mut3x` 49 | one-round sum to two-bit sum | 185,536 | 6 (a deletion and a substitution in the same birth not needed) |
| `pgates-long` 54 | one-round sum to two-bit sum | 104,244 | 50 |
| `s4` 52 | XOR level to one-round sum | 5,425 | 16 |
| `s4` 52 | to the two-bit level | 5,505 | 8, with a substitution next to it |
| `s4` 52 | to the three-bit level | 8,331 | 8 |
| `s4` 52 | to the four-bit sum | 63,540 | 5 (a substitution in the same birth not needed) |
| `s4` 51 | two-bit to three-bit level | 1,181,622 | 34 |
| `s4` 51 | to the four-bit sum | 1,502,727 | 40 |
| `w4-m49` 42 | two-bit sum to three-bit level | 871 | 6 |
| `w4-m49` 42 | to the four-bit sum | 1,368 | 12 |

The three other steps belong to the carry-vector line of `gates` 46: its two-bit sum came by substitutions between updates 242,881 and 276,937, and its descendant `w4-g46` 42 reached the three- and four-bit sums by single substitutions at updates 14,034 and 203,931.

**From each two-bit adder, a duplication is the only single mutation that gives the three-bit sum.** The neighbourhoods of the four stage 13 ancestors on the three-bit judge (duplications up to 120 instructions):

| Ancestor | Length | Substitutions giving the three-bit sum | Duplications giving it | Their lengths | Rate per birth (`v32` settings) |
|---|---|---|---|---|---|
| `m49` | 190 | 0 of 8,360 | 19 of 15,660 | 6 to 107 | 2.6e-5 |
| `p44` | 211 | 0 of 9,284 | 19 of 18,180 | 14 to 120 | 2.1e-5 |
| `p42` | 448 | 0 of 19,712 | 12 of 46,620 | 12 to 117 | 3.0e-6 |
| `g46` | 398 | 0 of 17,512 | 402 of 40,620 | 30 to 120, most near 120 | 1.3e-4 |

A neighbour counts when it breeds true and at least 95 % of its outputs passing 0.5 are exact on 400 draws; the ancestors are at 66 to 71 %. The rate per birth is the slip probability 0.05 times the share of the (from, to) pairs of a slip that make one of these duplications. At about 136 births per update (60 x 60), a population of `m49` or `p44` makes such a mutant every 300 updates or so and one of `p42` every 2,500; they took 3,400 to 9,300, 2,800 to 3,600 and 16,400 to 35,000 updates to reach 90 % exact in `v32`, so the ancestor with the rarest route was the slowest of the three, and each took about ten times as long, or more, as the first such mutant needs to appear. `g46` makes them most often and was the slowest (52,000 to 278,000): its duplications are long, near the 120 the search stopped at, and its populations, with the longest genomes, kept many offspring that did not compute the sum (stage 13, question 1).

**Reading.** A round written as a block that takes carry and partial sum in two registers and leaves them in the same two registers is a function that can be applied again, and a tandem duplication applies it again: one mutation, one more carry round. That explains why stage 13 found the climb from an evolved two-bit adder short and the first combining step the hard one: before the first round exists there is nothing to duplicate, and the first round was reached only after a change that did nothing at the time had kept a computed value alive, so the wait at the XOR level is a wait for a neutral change in the right place followed by a second, selected one. The carry-vector adder of `g46` can be extended by duplication too, but only by long ones, and its lines in fact moved by substitutions. Duplication stands in for the loop that no population built: the rounds are unrolled, and `call` stayed unused. The sizes of the duplicated stretches suggest that a compact round is extended sooner (the 6- and 12-instruction duplications of `w4-m49` 42 within 1,400 updates, the 34- and 40-instruction ones of `s4` 51 after a million), but nine lines do not test that.

Limits: nine lines of descent, chosen among the populations that reached the sum, so the study says how the sum was built where it was built and nothing about why other populations stalled. Levels come from 100 test-CPU draws per genotype, so a level change can be placed a few genotypes off. Several stretches of the lines (274 genotypes in `gates-mut3x` 49) are genotypes whose offspring differ from themselves in the test CPU; they were counted by the tasks of their own gestation. The rebuilt dataflow is that of one input set; the exactness on every field pair and on 100 random draws suggests no input-dependent branch, without proving it. The neighbourhoods hold single mutations only (no two at once, no insertion of one instruction, which the settings do not make), duplications only up to 60 or 120 instructions, and slip deletions not at all; a screen on 20 draws can miss a neighbour that is better only on some inputs. The potentiation search is a bisection and assumes that once a line is one mutation away it stays so; four lines are a small sample.

**A loop puts reproduction behind the adder's control sites** (measured 2026-11-24, at the maintainer's question whether the single instruction pointer keeps loops out). No line built a loop. To see what one would cost under mutation, the four unrolled rounds of the `s4` seed 52 adder (sites 331 to 346, after its first round) were replaced by hand with one round in a loop that runs until the carry is zero: a zero in AX, the carry and the partial sum saved on the stack around an `h-search` that marks the loop's start, the round, the exit test `if-n-equ nop-A` and `mov-head` back (23 sites in place of 16). In this instruction set a loop cannot mark its start without moving the round's two registers aside, because `h-search` clears BX and CX and `get-head` writes CX. The loop version computes the four-bit sum exactly on 400 draws, and without its backward jump it falls back to one carry round (21 % exact). Every substitution at the adder's sites, on the four-bit judge with 20 draws:

| Version | Adder sites | Substitutions there | Inviable | Breed true, sum broken (exact share below 0.9) | Sum intact |
|---|---|---|---|---|---|
| unrolled (evolved) | 20 | 880 | 12.5 % | 79.1 % | 8.4 % |
| loop | 27 | 1,188 | 35.8 % | 56.4 % | 7.8 % |

Almost every inviable one never reaches the divide: on 10 draws each, 12.0 % and 34.6 % of the draws end without a divide, against 0.5 % and 0.9 % that divide with an offspring that differs. In the unrolled adder one site stops the organism whatever replaces it (site 333, 89 %; not traced); the others are at 5 to 9 %, the background of the whole genome. In the loop six sites do: the zero and its register (97 and 100 %), `h-search` (83 %), the round's `nop-C` (92 %) and the exit test (95 and 98 %), and four more are at 45 to 54 % (the carry step and the saves). A changed exit test or body leaves the carry nonzero, the loop does not end, and the organism dies of old age without dividing; a changed backward jump is mild (6 %), since the loop then runs once. Outside the adder the two versions are the same (14.4 and 14.0 % of substitutions inviable).

The extra never-dividing substitutions come to about seven sites' worth, which at `copy_prob` 0.0025 is about 1.7 % more offspring that never divide. The loop also costs time: gestation 852 to 861 cycles against 823 (the number of iterations depends on the inputs), merit 1.9 % higher (seven more sites executed), fitness 1.6 to 2.5 % lower. An iteration executes nine instructions; an unrolled round executes three and copies four sites, and the default merit pays for them. In a population of 3,600 both costs are large enough for selection to act on (N s around 60). The adder is about equally brittle either way, with 8 % of its substitutions keeping the sum; the difference is what a broken adder costs. Unrolled, it falls back to a lower level and still reproduces; in a loop it often stops reproduction, which matters most on the way up, while the lower levels carry the population. Limits: one hand-written loop in one genome, stopping when the carry is zero; a counted loop has the same kinds of control sites, and an evolved loop could be laid out differently.

The tools are in the lab folder `29-lineage` (`nbhd.py`, `confirm.py`, `pot.py`, `loopvar.py`, `loopclass.py`, with the scratch test).

### Stage 14: does the extension of the adder need duplication?

Design fixed on 2026-11-24, before any run, and started on the maintainer's go the same day (rented servers, at most USD 15). Lab folder `30-slip`.

**Question.** The lineage study found that each extra carry round came, in 12 of 15 cases, from a tandem duplication, and the one-mutation neighbourhoods say why: from each of the four two-bit adders no single substitution gives the three-bit sum, while 12 to 402 duplications do. If that is the route, populations that cannot copy a stretch of their own code should extend the adder far more slowly or not at all, even with the same rate of insertions and deletions.

**Arms.** All four use the `v32` arms of stage 13 unchanged (the three-bit judge, `2^(32 q)`, the four two-bit ancestors `m49`, `p42`, `p44`, `g46`, copy mutation 0.0025, 60 x 60, exact mode) and differ only in what a slip mutation (`divide_slip_prob` 0.05 per divide, half of them deletions) inserts:

| Arm | `mutation.slip_fill_mode` | `divide_slip_prob` | The inserted stretch |
|---|---|---|---|
| `dup` | `duplication` | 0.05 | a copy of the stretch, in order (stage 13) |
| `scrambled` | `scrambled` | 0.05 | the same instructions in shuffled order |
| `random` | `random` | 0.05 | random instructions of the same length |
| `noslip` | (any) | 0 | no slip mutation |

Six seeds per ancestor (71 to 76), so 24 populations per arm and 96 runs, each 150,000 updates (first proposed with three seeds and 100,000 updates; raised before the runs because `g46` needed more than 100,000 updates in stage 13 and the cost is small). In stage 13 the population's share of organisms scoring 1.0, among those passing Q50, reached 90 % in every `v32` population of `m49`, `p42` and `p44` between updates 3,400 and 35,000, and in those of `g46` between 52,000 and 278,000.

**Outcome.** Populations holding the three-bit sum at 150,000 updates by the rule of stage 13 (at least 90 % of the organisms that pass Q50 score 1.0, confirmed on the most common organism alone with mutation off). Planned comparison: `dup` against `random`, which keeps the length changes and takes away only the copying, Fisher exact, two-sided. Secondary: `scrambled` against `random` (does the order of the copied instructions matter, or only their kinds), `noslip` against `random` (does an inserted random stretch help at all), and the time to 80 % exact in each arm.

**Prediction**, written before the runs: `dup` 20 or more of 24 (all 18 of `m49`, `p42` and `p44`, and some of `g46`); `random`, `scrambled` and `noslip` clearly fewer. A scrambled copy keeps a round only when the shuffle happens to restore the order (1 in 360 for the shortest round found, six instructions with one repeated), so `scrambled` should behave like `random`. If `random` does as well as `dup`, the duplications on the lines of descent were one route among several, and the reading after stage 13 is wrong.

**Cost.** Stage 13's `v32` runs took 5 to 9 hours for 300,000 updates on the rented cores, so about 2.5 to 4.5 hours for 150,000; 96 runs on 32 rented cores, about 12 hours and about EUR 2.

**Results.** All 96 runs reached 150,000 updates; the last server was deleted on 2026-11-25 at 01:15, about 12 hours after the start, and the batch cost about EUR 1.60.

| Arm | `m49` | `p42` | `p44` | `g46` | Hold the three-bit sum | Reach 80 % exact | Update of 80 % exact |
|---|---|---|---|---|---|---|---|
| `dup` | 6 of 6 | 6 of 6 | 6 of 6 | 2 of 6 | **20 of 24** | 24 of 24 | 2,000 to 42,000 (`m49` and `p44` 2,000 to 7,000, `g46` 8,000 to 16,000, `p42` 14,000 to 42,000) |
| `scrambled` | 3 of 6 | 1 of 6 | 3 of 6 | 0 of 6 | **7 of 24** | 7 of 24 | 13,000 to 132,000 |
| `random` | 0 of 6 | 3 of 6 | 0 of 6 | 0 of 6 | **3 of 24** | 4 of 24 | 56,000 to 122,000 |
| `noslip` | 0 of 6 | 2 of 6 | 3 of 6 | 0 of 6 | **5 of 24** | 5 of 24 | 62,000 to 144,000 |

The confirmation was run on the most common organism of every population, alone with mutation off, on 400 draws: it is exact on all its judged outputs or at the ancestors' 70.5 %, with three exceptions at 65.5, 70.8 and 82.3 %, all in populations below the bar. No population passed the share and failed the confirmation. Five populations have an exact most common organism but stand below 90 %: four `dup` populations of `g46` (83.0 to 90.0 %) and `random` `g46` seed 74 (85.6 %); as in stage 13, `g46`'s share is pulled down by offspring that do not compute the sum, and its genomes are the longest (the most common organisms of `dup` have 431 to 531 instructions).

Planned comparison: `dup` 20 of 24 against `random` 3 of 24, Fisher exact two-sided p = 1.4e-6. The prediction is met as written: all 18 populations of `m49`, `p42` and `p44`, and 2 of 6 of `g46`. Secondary: `scrambled` against `random` 7 against 3 (p = 0.29), `noslip` against `random` 5 against 3 (p = 0.70). Not planned: `dup` against `scrambled`, p = 0.00036.

**Reading.** Copying a stretch of the genome next to itself is the fast route to the next carry round: every `dup` population reached 80 % exact by update 42,000, all twelve of `m49` and `p44` within 7,000, while the populations without copying that got there at all took 56,000 to 144,000 updates. It is not the only route. 8 of the 48 populations that could not copy (`random` and `noslip`) extended the adder, all from `p42` and `p44`; none from `m49` (0 of 12) and none from `g46` by the rule. An inserted random stretch did no better than no slip at all (3 against 5), so the slow route is more likely a series of substitutions and single insertions (all arms keep the stock divide insertion and deletion, 0.05 each) than a lucky random insert. `scrambled` sits between, with 3 of 6 populations of `m49` and of `p44` and none as fast as `dup`. A scrambled copy carries the round's instructions in the wrong order, and for a short round a shuffle can restore the order; whether that is how these seven got there needs their lines of descent. A comparison of ends does not show it: after 150,000 updates only 44 to 77 sites of an ancestor still align with the most common organism. The reading after stage 13 holds where it matters: duplication is what makes the step from the two-bit to the three-bit sum short.

Limits: four ancestors with six seeds each and one step, from the two-bit to the three-bit sum. The route in each population is inferred from its arm, not traced. The rule misses `g46` populations whose most common organism computes the sum.

Verdict: positive on the planned comparison, as predicted. Accepted by the maintainer on 2026-11-25.

The tools are in the lab folder `30-slip` (`confirm.py` for the confirmation and `confirm.tsv` with its results, `diffanc.py` for the comparison with the ancestor).

**After stage 14: how the seven `scrambled` populations built their round** (traced 2026-11-25, at the maintainer's request). On each line of descent, from the most common organism at update 150,000 back to the ancestor with every genotype run on 100 draws, the three-bit sum arrives in one birth, from 62 to 73 % exact to 100 %, and stays. In five of the seven lines that birth inserts a shuffled copy of the stretch that holds a carry round:

| Line | Update | The birth that brings the sum |
|---|---|---|
| `m49` seed 71 | 130,255 | a shuffled copy of 5 instructions, round and one more |
| `m49` seed 72 | 28,786 | a shuffled copy of 8 instructions |
| `m49` seed 76 | 10,921 | a copy of the 4-instruction round in its own order, and a substitution |
| `p44` seed 71 | 52,515 | a shuffled copy of 15 instructions, one of them changed |
| `p44` seed 72 | 97,652 | a shuffled copy of 15 instructions, and a substitution |
| `p42` seed 75 | 53,616 | one substitution (`pgate-0` to `pgate-3`) |
| `p44` seed 74 | 29,666 | one substitution (`swap` to `pgate-2`) |

The round of `m49` is four instructions (`gate-6 nop-C`, `gate-4`, `shift-l`): a shuffle keeps their order once in 24, and the other two copies work in another order in their place. The rounds of `p44` lie in longer stretches, most of whose instructions do nothing there, so a shuffle that keeps the few that matter in a working order is not rare either. Most `scrambled` populations therefore extended the adder by copying too: a shuffled copy carries the round's instructions to the right place. This fits the counts of stage 14: `scrambled` did better than `random` for `m49` (3 of 6 against 0) and `p44` (3 against 0), not for `p42` (1 against 3). The lab folder `30-slip` has the tools (`lod_eval.sh`, `lod_story.py`, `stepcheck.py`, `slipcheck.py`).

### Stage 15: is a whole adder composed by copying too?

Design fixed on 2026-11-25, before any run; to start on the maintainer's go. Lab folder `32-compose`.

**Question.** Stages 13 and 14 showed that a carry round is extended by copying it: a round that leaves its two registers as it found them is a function, and a tandem duplication applies it twice. An evolved adder is a function one level up: it reads two words and leaves their sum in a register, where the next input can meet it. Does copying compose whole functions the way it composed rounds, so that a population that adds two numbers comes to add three?

**Task.** A new opt-in function of `field_arith`, `add3`: the sum of the three newest inputs, `a + b + c`, in `w + 2` result bits per field (`PORTING.md`). With the inputs masked to 3-bit operands in 5-bit fields (`INPUT_AND_MASK 0xFF739CE7`; `bits=3`, `stride=5`, `width=24`), an evolved four-bit adder computes the two-operand sum exactly, and the three-operand sum (at most 21) fits the same fields, so the adder applied to its own sum and the next input computes it.

**Ancestors.** The most common organism of five populations that hold the four-bit sum at the end of stage 13: `s52` (`s4` seed 52, grown from scratch; 375 instructions, pgate set) and the four `w4` populations of seed 42, grown from the two-bit ancestors: `m49` (621, gate set), `g46` (575, gate set), `p42` (353, pgate set) and `p44` (317, pgate set). Each computes the two-operand sum of the new layout exactly on 400 draws, and the three-operand sum on none (its output passes 0.5 on the `add3` judge in 11 to 16 % of draws).

**Measured before the runs.** One-mutation neighbourhoods on the `add3` judge: every substitution and every tandem duplication up to 80 instructions (up to 200 for `p42` and `g46`), candidates confirmed on 400 draws. From `m49`, `s52` and `p44` single duplications give the three-operand sum exactly: 56, 7 and 7 of them, each copying a stretch that holds IO instructions together with the adder's rounds (16 to 73 instructions), at 7.2e-6, 2.5e-6 and 3.5e-6 per birth. From `p42` and `g46` no single mutation does, and from none of the five does a substitution.

**Arms.** `dup` (`slip_fill_mode` duplication) against `random` (random instructions of the same length), the planned comparison of stage 14; `scrambled` and `noslip` are left out, since neither differed from `random` there. Everything else as in the `w4` arms of stage 13: 60 x 60, copy mutation 0.0025, slip 0.05 per divide, the stock divide insertion and deletion, exact mode, one judged output (the newest at the divide). Pay on that output: `2^(64 q)` for `add3` (thresh 0.25) and `2^(32 q)` for the two-operand sum of the same layout (thresh 0.25), so that the adder stays paid while the three-operand sum pays more; the eight logic tasks as before. Unpaid marks: Q50 to Q100 on `add3`, and the exact two-operand sum. Six seeds per ancestor (91 to 96): 30 populations per arm, 60 runs, 150,000 updates each.

**Outcome.** Populations holding the three-operand sum at 150,000 updates by the rule of stage 13 (at least 90 % of the organisms that pass Q50 on `add3` score 1.0, confirmed on the most common organism alone with mutation off). Planned comparison: `dup` against `random`, Fisher exact, two-sided. Secondary: within `dup`, the three ancestors one duplication from the sum (18 populations) against the two that are not (12); the time to 80 % exact in each arm.

**Prediction**, written before the runs: `dup` 18 or more of 30, among them all 18 populations of `m49`, `s52` and `p44`; `random` clearly fewer; within `dup`, `p42` and `g46` fewer and later than the other three. If `random` does as well as `dup`, whole functions are not composed the way rounds were.

**Seen before the runs.** A smoke run of 3,000 updates of `dup-m49` and `dup-s52` on seed 81: in `dup-m49` 75 % of the organisms that pass Q50 on `add3` were exact at update 3,000, in `dup-s52` 3 of 1,106. Seed 81 was one of the planned seeds when the design above was written (81 to 86); the planned seeds were moved to 91 to 96 before any planned run, so that no planned population has been seen.

**Cost.** A simulation step costs about the same per update as in stage 14 (the population's time slice, not the genome length, sets it), so about 3 hours per run on the rented cores; 60 runs on 32 cores in two waves, about 6 to 8 hours and about EUR 1 to 1.50.

**Results.** All 60 runs reached 150,000 updates; the last server was deleted on 2026-11-25 at 16:08, six and a half hours after the start, and the batch cost about EUR 0.61. The most common organism of every population was run alone with mutation off on 400 draws of the `add3` judge.

| Arm | Measure | `s52` | `m49` | `p44` | `p42` | `g46` | All |
|---|---|---|---|---|---|---|---|
| `dup` | holds the three-operand sum (the rule) | 1 | 0 | 1 | 0 | 0 | **2 of 30** |
| `dup` | most common organism exact on 90 % of draws or more | 5 | 3 | 4 | 3 | 0 | 15 of 30 |
| `random` | holds the three-operand sum (the rule) | 0 | 0 | 0 | 0 | 0 | **0 of 30** |
| `random` | most common organism exact on 90 % of draws or more | 0 | 2 | 0 | 0 | 0 | 2 of 30 |

Two most common organisms are exact on every judged output: `dup` `p44` seed 94 (population 91.1 % exact) and `dup` `s52` seed 92 (96.0 %; it breeds true on 83 % of the draws). Fifteen others stand at one level, exact on 91.0 % of the draws (91.5 % for two), passing Q50 on all of them; on the same 400 draws they fail on the same 36. Four are at about 70 % (69.8 and 70.2 %), the rest at 0 to 33 %. `dup` `m49` seed 94 passes the population share (90.7 %), but its most common organism is at 91.0 %, so it fails the confirmation; `dup` `p44` seed 91 stands at 89.8 %. Every organism at 70 % or above has given up the exact two-operand sum (0 of 400 draws), which is paid less. Most `random` populations ended at one of two lower levels that also occur in `dup`: 6.9 % exact with Q50 on 65 % of the draws (10 populations) or 3.9 % with Q50 on 84 % (6), both with the exact two-operand sum on 3 % of the draws or fewer; their genomes shrank (76 to 576 instructions, 20 of 30 under 300).

**What the 91 % level is.** It matches an adder of four carry rounds applied twice. With 3-bit operands the first application gives `a + b` exactly (at most 14 in a field). The second adds `c` to that, and a carry that runs from bit 0 to bit 4 of a field needs a fifth round. A model of rounds of XOR, AND and shift on 200,000 random draws of the mask gives 90.9 % exact for four rounds then four, and 100 % for four then five. The genomes have not been read to confirm it.

Planned comparison: `dup` 2 of 30 against `random` 0 of 30, Fisher exact two-sided p = 0.49. The prediction (18 or more in `dup`) is not met. Secondary: within `dup`, the ancestors one duplication from the sum 2 of 18 against the other two 0 of 12 (p = 0.50). Time to 80 % exact: 14 of 30 `dup` populations, from update 3,200 to 140,500 (`p44` seeds 91 and 92 and `m49` seed 94 within 4,800); 2 of 30 `random`, at 107,100 and 148,400. Not planned, with the measure chosen after the data: a most common organism exact on 90 % of the draws or more, `dup` 15 of 30 against `random` 2 of 30, p = 0.00037; within `dup`, 12 of 18 against 3 of 12 (p = 0.060).

**Reading.** Copying composes the adder with itself: in half the `dup` populations the most common organism feeds an adder's sum and the next input to an adder, against 2 of 30 when the inserted stretch is random, and the fastest got there within 5,000 updates. The composition comes one carry round short, and in 150,000 updates two populations added the missing round. In stage 14 a missing round came by copying within 2,000 to 42,000 updates. Here the round helps only in the second application, and why it came so rarely is open. The single duplications measured before the runs, from `m49`, `s52` and `p44`, give the exact sum; a recount from `m49`, `p42` and `g46` at a bar of 88 % found no single mutation that reaches only the 91 % level. So the populations at 91 % got there by more than one mutation, or by an exact organism losing a round, and only their lines of descent can tell which. `p42` reached the 91 % level in 3 of 6 `dup` populations, although no single mutation from it gives the three-operand sum.

Limits: five ancestors, six seeds each, 150,000 updates. The 91 % level is read from the shares and a model, not from the genomes. The rule counts exact organisms only; it was fixed before the runs and is applied unchanged.

Verdict: negative on the planned comparison; the prediction failed. Not planned: copying composes the adder with itself, one carry round short, in 15 of 30 `dup` populations against 2 of 30 `random`. Accepted by the maintainer on 2026-11-25.

The tools are in the lab folder `32-compose` (`popconfirm.py` for the confirmation and `popconfirm.tsv` with its results; `confirm.py` with `HI=0.88` for the recount of the neighbourhoods).

**After stage 15: how the populations reached the 91 % level** (traced 2026-11-25, after the maintainer accepted the stage). Nine lines of descent, from the most common organism at update 150,000 back to the ancestor, every genotype run on 100 draws: the two exact populations, six with a most common organism at 91 %, and `random` `m49` seed 95.

On every line the 91 % level arrives in one birth, from at most 30 % exact to 87 to 98 %, but the births differ:

| Line | Update | The birth that brings the 91 % level |
|---|---|---|
| `dup` `p44` seed 92 | 1,421 | a tandem copy of 61 instructions |
| `dup` `p44` seed 91 | 2,943 | a duplication of 56 instructions |
| `dup` `p44` seed 94 | 20,724 | a tandem copy of 17 (a copy of 12 had given 70 % at 17,170; a copy of 10 gave the exact sum at 41,943) |
| `dup` `s52` seed 91 | 22,744 | a tandem copy of 25 |
| `dup` `p42` seed 93 | 20,748 | one substitution, `mov-head` to `nop-E` (27 % to 90 %), with no insertion of 8 or more on the line before it |
| `dup` `s52` seed 92 | 50,112 | one substitution, `h-copy` to `if-less` (2 % to 98 %), after copies of 78 and 204 instructions at 17,749 and 35,494; exact from 57,831 |
| `dup` `m49` seed 93 | 112,744 | a deletion of 34 and an insertion of 22, after a 70 % level from 78,076 |
| `random` `m49` seed 95 | 106,296 | two substitutions |

Two populations counted at 91 % were exact for most of the run. In `m49` seed 94 the exact sum arrived at update 4,773 in one birth, a tandem copy of 22 instructions and a substitution (a route measured before the runs), and the line stayed exact until update 148,615; the population's share stood at 89 to 93 % from update 5,000 on. `p44` seed 91 was exact from update 64,488 to 146,358, at 85 to 92 %. Their most common organisms at the end, with 7 and 12 copies, are recent variants one round short. So four populations held the exact sum for much of the run, at 85 to 97 %, and the others with a most common organism at 91 % stand at 77 to 88 %, one of them with the exact sum in part (below). The bar of 90 % falls inside the range of the exact populations, as it did for `g46` in stage 14.

**What the end populations hold.** 300 organisms drawn at random from each of six end populations, their genotypes run on 100 draws:

| Population | Exact | At 91 % | Other (lower, or not breeding true in the test CPU) |
|---|---|---|---|
| `dup` `s52` seed 92 | 54 % | 7 % | 39 % |
| `dup` `m49` seed 94 | 50 % | 14 % | 36 % |
| `dup` `p44` seed 94 | 41 % | 5 % | 54 % |
| `dup` `p44` seed 92 | 21 % | 18 % | 61 % |
| `dup` `p42` seed 93 | 0 % | 62 % | 38 % |
| `dup` `s52` seed 91 | 0 % | 59 % | 41 % |

At about 0.9 mutations per birth, a third to a half of the organisms compute neither level. Where the exact sum is held, 79 to 89 % of the organisms that compute one of the two levels are exact. `p44` seed 92, counted at 91 % by its most common genotype (7 copies), holds both levels about equally. The confirmation on the most common genotype, which has 4 to 27 copies among about 3,000 genotypes, says little about such a mix.

The missing round is one duplication away. The one-mutation neighbourhoods of three most common organisms at 91 % (`p42` seed 93, `p44` seed 92 and `s52` seed 91; every substitution and every tandem duplication up to 40 instructions, candidates confirmed on 400 draws at 99 %) contain 29, 285 and 2 duplications that give the exact sum (6 to 40 instructions), at 1.0e-5, 1.4e-4 and 1.5e-6 per birth, and no substitution that does. Each of these runs made 5.0 to 6.3 million births after update 30,000, so about 50, 860 and 10 such offspring were born.

**Competitions.** Each of the three organisms at 91 % (A) shares a 60 x 60 world, filled in alternate rows, with one competitor: the exact neighbour with the shortest copy, the one with the longest (for `s52` the two give the same genome), and as a control a copy of the same length as the shortest, placed elsewhere, that stays at 91 %. First with mutation off, one seed, 8,000 updates (a generation takes 26 to 52 updates). An exact neighbour of `p44` does not breed true even with mutation off: its offspring are 7 instructions shorter or longer, and exact too. The test CPU counts a genome as viable when a descendant within its test depth breeds true, as the C++ did, so the viable shares of the neighbourhoods include such genomes. The competitor's share counts its whole lineage.

| A | Exact, shortest copy | Exact, longest copy | Control |
|---|---|---|---|
| `p44` seed 92 | 100 % | 100 % | 89 % |
| `s52` seed 91 | 100 % | (the same genome) | 30 % |
| `p42` seed 93 | 91 % | 64 % | 54 % |

The first 500 updates move the controls too (`p44`'s to 84 %). After update 1,000 the exact lineages gain about 4 % per generation for `p44` and `s52` and 2 % for `p42`, and the controls drift. The pay rule suggests about 9 %: a draw with one carry bit wrong pays about 2^-5 of a full draw.

Then with the mutation rates of the runs (the shortest copy and the control, two seeds each, the competitor's lineage labelled), with 300 organisms of each end population run as above:

| A | Exact lineage at update 8,000 | Exact / at 91 %, of 300 | Control lineage | Exact / at 91 %, of 300 |
|---|---|---|---|---|
| `p44` seed 92 | 100 and 96 % | 35 / 10 and 34 / 8 % | 39 and 46 % | 3 / 39 and 0 / 48 % |
| `s52` seed 91 | 90 and 100 % | 36 / 14 and 49 / 9 % | 56 and 79 % | 0 / 52 and 0 / 53 % |
| `p42` seed 93 | 53 and 45 % | 26 / 34 and 21 / 43 % | 52 and 72 % | 0 / 70 and 0 / 60 % |

Four more seeds of `p42` (2026-11-26): the exact lineage ends at 65, 39, 56 and 56 % (six seeds: 39 to 65 %, mean 52 %), the control at 63, 81, 39 and 40 %. The exact lineage loses ground in the first 500 updates (to 33 to 46 %), as it did with mutation off. From update 1,000 on, the exact lineage gains in all six seeds, from a mean of 42 to 52 %, about 0.3 % per generation against about 2 % with mutation off; the controls move both ways.

**Offspring under mutation** (2026-11-26). 1,500 offspring of each genome, drawn with the runs' mutations in their order (a substitution per copied site at 0.0025, then one insertion, one deletion and one slip at 0.05 each), each run on 100 draws:

| Genome | Unchanged | Exact | At 91 % | Other |
|---|---|---|---|---|
| `p42` seed 93, at 91 % | 32.5 % | 0.0 % | 62.1 % | 37.9 % |
| `p42` seed 93, exact neighbour | 33.0 % | 61.2 % | 0.3 % | 38.5 % |
| `p44` seed 92, at 91 % | 39.5 % | 0.2 % | 68.1 % | 31.7 % |
| `p44` seed 92, exact neighbour | 39.0 % | 64.7 % | 0.5 % | 34.9 % |
| `s52` seed 91, at 91 % | 43.6 % | 0.0 % | 66.0 % | 34.0 % |
| `s52` seed 91, exact neighbour | 44.3 % | 67.5 % | 0.1 % | 32.3 % |

The exact genomes lose about as many offspring to mutation as the 91 % genomes: the differences are 0.6, 3.2 and -1.7 points, against a sampling error of about 1.8. An exact offspring falls back to 91 % in 0.1 to 0.5 % of births. So the small advantage of `p42`'s exact sum under mutation does not come from a less robust genome, at least not over one generation.

**Reading.** The step that was in reach was taken where it pays and arises often enough. In `p44` seed 92 it was taken: a fifth of the organisms at the end are exact, as many as at 91 %, and in competition the exact lineage wins with mutation on. In `s52` seed 91 it is rare, about 10 such offspring in the run, and with an advantage of about 4 % per generation most such offspring are lost by chance. In `p42` seed 93 it pays little under mutation, about 0.3 % per generation against about 2 % without, so the 50 or so such offspring had little chance; why mutation takes most of the advantage away is not measured. Where the exact sum is held under mutation, about a fifth of the organisms that compute either level are at 91 %, so a population that holds it stands near the bar of 90 % on the share. Limits: three organisms, one to six seeds per competition, 8,000 updates.

**A holding rule by sample** (tried on stage 15 after the data, 2026-11-26; it does not change the stage's verdict). A population holds a level when, in samples of 300 organisms drawn at random from each of its last two saved populations (here updates 100,000 and 150,000), at least half of the organisms that compute (viable in the test CPU, and passing Q50 on at least half of 100 draws) are exact. On stage 15 it counts 4 of 30 `dup` populations and 0 of 30 `random` (Fisher p = 0.11): `m49` seed 94, `p44` seeds 91 and 94 and `s52` seed 92, the four whose lines were exact for most of the run. In them 59 to 84 % of the computing organisms are exact; in every other population none are, except `p44` seed 92 (none at update 100,000, 49 % at 150,000) and `p44` seed 93 (1 of 153). Proposed for the next stage, in place of the confirmation on the most common genotype; the population's share in `tasks.dat` would still be reported. `sample_rule.py` and `sample_rule.tsv` in `32-compose`.

The lab folder `32-compose` has the tools (`lod_eval.sh`, `lod_levels.py`, `lod_copies.py`, `run_nbhd91.sh` with its log `nbhd91-run.log`, `sample_eval.py` for the samples; `make_comp.py`, `run_comp.sh`, `comp_count.py` for the competitions without mutation, `run_compmut.sh`, `run_compmut_seeds.sh` and `comp_mut_count.py` with it; `robust.py` and `robust.tsv` for the offspring under mutation).

### Stage 16: can the adders re-adapt to eight inputs?

Design written on 2026-11-26, before any planned run. Lab folder `33-compose4`.

**Why this stage.** The next question after stage 15 is whether composition repeats: whether a population that adds three numbers comes to add four, five or more. Sums of more operands need more inputs, and `field_arith` now has them (`add4` to `add8`, the sum of the `n` newest inputs read; `PORTING.md`). The adders of stage 15 do not survive the move to more inputs. They execute `IO` 18 to 27 times in a life, and each `IO` writes an output and reads the next input. With three inputs handed out in turn, any three consecutive reads are the three inputs, so their sum does not depend on how many reads came before. With eight, none of the eight organisms chosen as ancestors (below) is exact on the sum of the three newest reads. Four still output a fixed sum of three reads, but of other ones: `m49` seed 94 the newest, the second and the fourth newest on every draw, three of the `h` organisms other fixed positions on 90 to 94 % of the draws. The other four compute no such sum, and three of them barely reproduce (on 0, 2 and 56 % of the draws). A ladder of sums therefore needs ancestors that sum the three newest reads in a world of eight inputs, and this stage asks whether evolution provides them. A first design of the ladder was dropped before any run: the lab tool that judges single organisms in the test CPU gave each organism three inputs whatever the world, so its measurements of the four-operand sum were wrong. It now gives as many as the world hands out.

**Question.** Do the populations re-adapt, rewiring which reads the adder sums until it computes the sum of the three newest reads exactly? And does a complete module, exact on three operands in the old world, re-adapt more readily than one a carry round short?

**World.** The stage 15 world of each ancestor's family (the `dup` fill, from which all eight come), with two changes: 3-bit operands in 6-bit fields (`INPUT_AND_MASK 0xFF1C71C7`, the layout a sum of five or more operands needs), and eight inputs handed out (an unpaid mark of `add8` makes the environment ask for eight, as the C++ environment did for any task that asked for more). The judge is that of stage 15 with the lower rung unpaid: the newest output at the divide, the sum of the three newest reads paid `2^(64 q)` for q of at least 0.25, with unpaid marks Q50 to Q100 on it and unpaid marks of the two-operand sum (the two newest reads). Stage 15 paid the two-operand sum `2^(32 q)`; in a smoke run with that pay, a population from `m49` seed 94, whose output summed the newest, the second and the fourth newest reads, dropped the fourth and became exact on the two-operand sum within 1,000 updates (2,611 of 3,546 organisms at update 3,000).

**Ancestors.** From the stage 15 end populations (update 150,000): the most abundant organism exact on `add3` in each of the four populations that hold it by the rule by sample (`e`: `m49` seed 94, `p44` seeds 91 and 94, `s52` seed 92; 298 to 396 instructions), and the most abundant organism at the 91 % level in four populations without exact organisms (`h`: `m49` seed 93, `p44` seed 95, `s52` seed 91, `p42` seed 93; 257 to 443 instructions). In the new world, on 400 draws, none is exact on any draw; they pass Q50 on 0 to 20 % of the draws, and `p44` seeds 91 and 94 and `p42` seed 93 reproduce on 56, 0 and 2 % of them, the other five on all.

**Measured before the runs.** One-mutation neighbourhoods on this judge (every substitution and every tandem duplication up to 80 instructions on 20 draws, candidates confirmed on 400): no single mutation from any of the eight makes it exact. Some mutants are exact on the one or two of 20 draws on which they pass Q50, and none of them holds up on 400. Single mutations do bring several closer: 490 neighbours of `m49` seed 94 pass Q50 on at least 8 of the 20 draws (the ancestor on 17 % of 400), and 177 of `s52` seed 91 (the ancestor on 20 %), against at most 60 for each of the other six.

**Runs.** Six seeds per ancestor (101 to 106): 48 runs of 150,000 updates, exact mode, the population saved every 50,000 updates.

**Outcome.** The rule by sample proposed after stage 15, unchanged (`sample_rule.py`): from the saved populations at 100,000 and 150,000 updates, 300 organisms are drawn at random and each genotype is run alone on 100 draws of eight inputs. An organism computes when it reproduces on at least half of the draws and passes Q50 on at least half; it is exact when it is exact on at least 0.99 of the draws on which it passes Q50. A population holds the sum when at least half of the computing organisms are exact in both samples. Planned comparison: the populations that hold the sum, from the `e` ancestors against the `h` ancestors among the five that reproduce on every draw in the new world (`e`: `m49` seed 94 and `s52` seed 92, 12 populations; `h`: `m49` seed 93, `p44` seed 95 and `s52` seed 91, 18), Fisher exact, two-sided. All 48 are reported. Secondary: the count per ancestor; the populations whose computing organisms are at least half at the 91 % level or better (0.84 of the Q50 draws exact); the rule of stage 13 at update 150,000 (at least 90 % of the organisms that pass Q50 exact, confirmed on the most common genotype on 400 draws); the first update at which at least 80 % of the organisms that pass Q50 are exact.

**Prediction**, written before the runs: populations of `p44` seeds 91 and 94 and `p42` seed 93 mostly die at once or survive without the sum; of the 30 populations of the other five, 5 to 20 hold the sum at 150,000 updates, and at least half of those from `s52` seed 91 do. `e` and `h` do not differ: the smoke run suggests that how a module's reads are wired matters more here than whether it was complete in the old world.

**What follows.** The ancestors of stage 17, the ladder of sums, would come from the populations that hold the sum. Its design (rungs `add3` to `add8` on one judged output, rung `n` paid `2^(32 (n - 2) q)` when q is at least 0.75, which a model of 400,000 draws found to order the rungs cleanly where the threshold 0.25 of earlier stages did not) is kept in the lab folder and will be fixed once this stage has been seen.

**Seen before the runs.** Smoke runs of 3,000 updates on seed 81, not a planned seed, with only the three-operand sum paid: the population of `s52` seed 91 turned to it within 1,500 updates (2,670 of 3,451 organisms pass Q50 at update 3,000, 529 are exact on their own draw; in a sample of 300, 177 compute and none is yet exact on 99 % of 100 draws); that of `m49` seed 94 gave it up (one organism passing q of 0.25 at update 3,000); those of `p44` seeds 91 and 94 and `p42` seed 93 never grew past three organisms and died out within 300 updates (`p44` seed 94 in the run with the two-operand sum paid).

**Cost.** Stage 15's runs took 78 to 211 minutes each for 150,000 updates on the rented cores. 48 runs, of which up to 18 may end at once, on 32 rented cores and this Mac's five: about 3 to 4 hours, about EUR 0.50. The watcher stops the batch at EUR 1.50.

## Candidate future experiments

Ideas from the discussion with the maintainer on 2026-11-18, unordered; the order is decided later. Stage 9 took up the first; stages 11 and 12 reached the two-bit adder by another road (one judged output, graded pay, gate instructions).

- **Expected pay per genotype.** A new genotype is run in the test CPU on many draws of inputs and every copy is paid its average quality, in place of the one draw an organism is judged on now. This removes pay for lucky draws (the half-adder leak of stage 5, the lottery that ruled out graded credit in stage 8) and makes graded credit usable.
- **Pay for information.** From the same test runs, pay a genotype for how much its outputs tell about the target (mutual information), so that an organism that outputs the ingredients of the next step is paid before any single output is right.
- **Competition between organisms as the source of selection.** Host and parasite on computed keys (a parasite infects a host only by performing a function the host performs; Zaman and others 2014 found hosts reach more complex functions with parasites than without), with the ladder kept as unpaid measurement. If that moves populations, a prediction contest between neighbours: an organism gains merit by predicting a value its neighbour computes and the neighbour loses it, which needs new instructions.
- **Differentiable programs** as a fallback: a distribution over instructions at each genome position and fitness as an expectation with a gradient. It replaces mutation and selection with something close to gradient descent, and is kept apart from the evolutionary line for that reason.
- **Wider adders** (ADD3, ADD4; files prepared, layout to be redesigned so that a rung keeps the fields of the rung below).

Added on 2026-11-24, after the lineage study (no population built a loop or used `call`):

- **Loops that cannot stall reproduction.** Remove one obstacle to loops at a time, starting from the evolved adders: a second instruction pointer (`max_cpu_threads 2` with `fork-th`, so that a computation thread that never ends only slows the copy thread), and merit that does not pay for executed length (`base_merit_method` constant; the default `least_size` pays for every site executed, so an unrolled round pays for itself). Wider sums alone are unlikely to help: the measured loop's iteration takes about as long as an unrolled round takes to run and copy. The loop measurement after stage 13 puts the cost of a loop's control sites at about 1.7 % of offspring and its time cost at 1.6 to 2.5 % of fitness; a second thread or a bounded iteration removes the first, the merit rule decides the second.
- **A pure, concatenative core for computing** (filed by the maintainer as a possible later improvement). Keep the heads CPU for reproduction and add an opt-in stack language for computation: a data stack of words, the sixteen gates as stack operations (pop two, push one), `dup`, `swap`, `over`, `drop`, `shl`, `shr`, a quotation (the next block, marked by a label) and a `times` that runs it n times with n capped, so that iteration always ends. In such a language every stretch of code is a function from stack to stack, so a tandem duplication is always a composition, which is what made the evolved carry rounds extendable; named blocks make reuse what `call` was meant to be; and nothing computed is overwritten, which may make the preparing change before the first combining step unnecessary, or take away the neutral room it used. Precedents: Fontana and Buss's algorithmic chemistry (lambda calculus), Spector's Push and Pushpop (bounded `exec.do*times`, self-reproducing programs), Cartesian GP (pure feed-forward graphs that evolve adders). Comparison: the stage 13 setup from scratch, register gates against the stack core; the time to the first combining step, whether `times` replaces unrolled rounds, and whether a block is reused. About a day to build, with tests and `PORTING.md` entries.
