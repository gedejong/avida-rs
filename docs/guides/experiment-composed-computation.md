# Can populations evolve composed computation?

This series asks whether an instruction set with subroutines helps a population evolve a function that has to be built in several steps, a 2-bit adder made from `nand`, shifts and `inc`. The stages follow below in order; each states its design before the runs and reports its result as found, negative results included. Everything here is an extension of the Rust port; the C++ Avida has none of it, and a configuration that does not ask for it runs as before.

## The pieces

### An instruction set with subroutines

`heads_modular` (`instset-heads-modular.yaml`, selected with `-set instruction_sets heads_modular`; stages 3 to 7 used the old `instset-heads-modular.cfg`, the same set) is the stock heads set plus three registers (DX, EX, FX, addressed with `nop-D`, `nop-E`, `nop-F`) and the instructions `call` and `return`. With six nops the complement of a label rotates every nop by one, so `call` followed by `nop-C nop-C` reaches `nop-D nop-D`. `call` searches forward only, so a subroutine lies after its callers. `modular-subroutine.org` is a hand-written organism that does NOT, NAND and AND through one shared subroutine; `default-heads-modular.org` is the stock ancestor adapted to the six-nop labels.

### A task for multi-bit functions

The task `field_arith` treats a 32-bit word as a row of small independent fields and scores a function field by field. Its arguments are `fn` (`xor`, `and`, `or`, `halfadd` or `add`), `bits` (operand width, 1 to 4), `mode` (`packed`: all result bits in one output; `planes`: one result bit per output, over successive outputs), `thresh` (the lowest quality that is paid) and `stride` (field spacing, default `bits` + 1). The quality is max(0, 2f - 1), where f is the fraction of compared bits that are right, so a random output scores about zero.

### The ladder

`environment-adder-ladder.yaml` (the runs used the old `.cfg`) has eight reactions, each on its own resource that runs short when many organisms use it, each paid once per organism. Merit is multiplied by 2 to the power of (value times resource consumed).

| Rung | Task | Value |
|---|---|---|
| NOT, NAND | stock | 1 |
| XOR | stock | 2 |
| AND | stock | 3 |
| HALFADD_PLANES | sixteen 1-bit half adders, sums then carries as two outputs | 4 |
| HALFADD | the same in one output | 5 |
| ADD2_PLANES | ten 2-bit adders, three result bits as three outputs | 6 |
| ADD2 | the same in one output | 7 |

The instruction sets have no `add` or `sub`; hand-written solutions for every upper rung exist in both sets (`ladder-*.org`).

### The instruments

The Ladder view (Charts tab, or `J` in the terminal) shows per rung how many organisms performed it over time; a rung is held when at least 1 % do. The Reuse view (Organism tab, or `V` in the terminal debugger) is a knockout map: each site is replaced in turn by a null instruction and the organism run in the test CPU, and a site is lethal, shared (two or more tasks lost), task (one lost), speed or neutral; the reuse index is the mean overlap of the site sets of each pair of tasks. The arm comparison (`avida-gui --compare DIR`, `avida-tui --compare DIR`) reads `DIR/<arm>/seed-<N>/` and shows per arm how many seeds reached each rung, with exact tests. The knockout measure has two limits: an early site such as the input read is shared by every task without being reused code (the count of `call` sites and targets separates the two), and duplicated code hides sites because knockouts are made one at a time.

## Stages

### Stages 1 and 2: design and demonstrations

The main runs were designed before they started. Arms: modular set against stock set, neither with `add` or `sub`. Seeds 1 to 10 per arm, 100,000 updates, 60 x 60, exact mode, stock mutation rates plus `mutation.divide_slip_prob: 0.05`, threshold 0.75 on the four `field_arith` rungs, ten 3-bit fields for the 2-bit adder. Primary outcome: seeds per arm that ever held each rung, and the highest rung held at the end; secondary: MODES complexity, reuse index, shared sites and `call` sites of the dominant genotype. Tests: Fisher's exact test per rung, Mann-Whitney U per measure, two-sided. The threshold was first 0.25 and was raised because trivial outputs cleared it (a constant 0 scores 0.25 on HALFADD, the XOR of the inputs 0.45). In a world seeded with the four hand-written upper-rung organisms, the HALFADD_PLANES organism (also XOR and AND, about 220 cycles per copy) displaced the others (345 to 680 cycles) within about 500 updates.

### Stage 3: modular set against stock set

Lab folder `13-adder-ladder`. Does the modular set climb the ladder further than the stock set? Ten seeds per arm, 100,000 updates, all 20 runs complete on 2026-11-15.

**Results.** Seeds out of 10 holding the rung at any sample (ever) or the last (end); p is Fisher's exact test.

| Rung | Modular, ever | Stock, ever | p | Modular, end | Stock, end | p |
|---|---|---|---|---|---|---|
| NOT | 9 | 8 | 1.00 | 1 | 4 | 0.30 |
| NAND | 8 | 8 | 1.00 | 1 | 4 | 0.30 |
| XOR | 0 | 0 | 1.00 | 0 | 0 | 1.00 |
| AND | 1 | 3 | 0.58 | 1 | 2 | 1.00 |
| HALFADD_PLANES | 1 | 3 | 0.58 | 1 | 3 | 0.58 |
| HALFADD, ADD2_PLANES, ADD2 | 0 | 0 | 1.00 | 0 | 0 | 1.00 |

No difference is significant, and the highest rung at the end did not differ (Mann-Whitney U = 35.5 against 64.5, p = 0.30). In 12 of 20 runs the population picked up NOT or NAND between updates 1,000 and 2,800, lost them before update 4,000 and held no rung afterwards; three more never held one. The genome shrank from 100 instructions to 19 to 24 and gestation fell from 389 cycles to about 30. A NOT or NAND multiplies merit by at most 1.41 while the shrinking tripled fitness, so short replicators displaced the organisms that computed. Three of the four runs that reached HALFADD_PLANES found AND and HALFADD_PLANES between updates 2,000 and 2,600, before the short replicators took over, and kept them with genomes of 46 to 60 instructions. In 18 of 20 seeds no rung was added after update 4,000. The instruction set played no visible part. `call` was not used: the one modular seed that climbed (seed 3) has a single `call` whose knockout costs 0.9 % of fitness and loses no task, and its shared sites are inputs, `nand` and `swap` on one straight run of code. This does not show that subroutines are useless, since no run reached a rung where reuse would pay.

The prediction that the modular arm climbs further did not hold. Verdict: negative, accepted by the maintainer on 2026-11-15.

### Stage 4: search conditions

Lab folder `14-search-conditions`. Do population size, sex, seasons or a minimum genome size bring populations further up the ladder? One factor at a time against the stage 3 modular arm (its 10 runs are the base); design fixed 2026-11-15.

| Arm | Change against the base |
|---|---|
| `sex` | `divide-sex` in place of `h-divide`; run with `genealogy.drop_extinct_genotypes: true` |
| `pop-30` | a 30 x 30 world, inflow scaled to the area |
| `pop-120` | a 120 x 120 world, inflow scaled to the area |
| `seasons` | NOT, NAND, XOR and AND pay nothing in every other period of 2,000 updates |
| `min-genome` | `divide.min_genome_size: 50`: a divide leaving fewer than 50 instructions fails |

`min-genome` was added after stage 3 because of the shrinking seen there; 50 is half the ancestor's length. Seeds 1 to 10, 100,000 updates, exact mode. Primary test: Fisher's exact test on seeds holding HALFADD_PLANES at the end, each arm against `base`, with a Holm correction over five. The first six `sex` runs grew past 4 GB (with two parents every ancestor stays in memory) and were discarded; `genealogy.drop_extinct_genotypes` changes nothing organisms compute but makes MODES complexity and phylogeny measures unavailable for `sex`.

**Results.** All 50 runs complete on 2026-11-16. Seeds holding the rung at the last sample, any sample in brackets:

| Rung | `base` | `sex` | `pop-30` | `pop-120` | `seasons` | `min-genome` |
|---|---|---|---|---|---|---|
| NOT | 1 (9) | 3 (10) | 0 (6) | 0 (10) | 0 (7) | 10 (10) |
| NAND | 1 (8) | 3 (8) | 0 (5) | 0 (9) | 0 (6) | 10 (10) |
| XOR | 0 (0) | 0 (0) | 0 (0) | 0 (0) | 0 (0) | 0 (0) |
| AND | 1 (1) | 3 (3) | 0 (0) | 0 (0) | 0 (1) | 10 (10) |
| HALFADD_PLANES | 1 (1) | 3 (3) | 0 (0) | 0 (0) | 0 (0) | 10 (10) |

Nothing above HALFADD_PLANES was held by 1 % of a population in any of the 50 runs. Primary test: `min-genome` 10 against 1, p = 0.00012 (Holm 0.0006); `sex` 3 against 1, p = 0.58 (1.00); `pop-30`, `pop-120` and `seasons` 0 against 1, p = 1.00. In all ten `min-genome` seeds NOT and NAND were held from before update 2,500 and never lost, AND came between updates 2,000 and 15,500 and HALFADD_PLANES between 2,400 and 7,300; nine dominant genotypes have exactly 50 instructions, the tenth 77. With shrinking forbidden, populations kept what they found, but no rung was added after update 15,500. Population size (900 to 14,400 organisms) had no effect on the rungs. In `sex` genomes stayed longer (median 41.5 against 21.5) and low rungs lasted longer (NOT held until update 75,500 to 92,500 in three seeds that ended with nothing, where no `base` seed kept it past 3,800). In `seasons` no seed held a rung at the end. `call` was not used as a subroutine: `PrintReuseSurvey` on 65 genotypes (five each from the ten `min-genome` and three climbing `sex` runs) found no site executed from two different `call` sites; the `call`s present work as jumps. The step from HALFADD_PLANES to HALFADD was not taken in any of the 14 runs that held the lower rung.

**Correction (2026-11-16).** Found after the stage was accepted: the organisms that hold HALFADD_PLANES in stages 3 and 4 are not half adders. The most abundant genotypes of the eleven climbing `base` and `min-genome` runs, run in the test CPU on 30 random input triples each, output NAND and then AND. NAND agrees with the sum bit (XOR) on three of four input combinations, so the pair is right on 87.5 % of the compared bits, a quality of 0.75, the threshold. No run in either stage held XOR at any sample. The counts and tests stand as counts of the rung as defined, and so does the finding that a minimum genome size stops the loss of tasks (NOT, NAND and AND are real, kept in 10 of 10 seeds against 1 of 10). The reading that these populations compute a half adder does not stand: the highest real step reached was AND, and the wall is XOR. The threshold had not been checked against a sum plane made of NAND or OR.

Verdict: `min-genome` changed the outcome, the other conditions showed no difference; accepted by the maintainer on 2026-11-16.

### Stage 5: the wall below XOR

Lab folder `15-xor-wall`. Does paying the intermediate logic steps, and then islands, let populations make XOR? Design fixed 2026-11-16, on the `min-genome` arm of stage 4.

| Arm | Change against the arm above it |
|---|---|
| `exact` | the four field rungs pay only at full quality (`thresh=1`) |
| `exact+steps` | ORN 2, OR 3, ANDN 3, NOR 4 are paid, each on its own resource |
| `exact+steps+islands` | ten demes of 60 x 6 cells (`demes.count: 10`); migration probability 0.001 (`demes.migration_rate`) |

Seeds 1 to 10, 100,000 updates, exact mode, modular set, 60 x 60, `divide.min_genome_size: 50`. Primary outcome: seeds holding XOR at the last sample (a tenth of the population performing it). Tests: Fisher's exact test, `exact+steps` against `exact` and `exact+steps+islands` against `exact+steps`, Holm over two. The most abundant genotypes of XOR populations are run in the test CPU and their outputs compared with the functions paid.

**Results (2026-11-17).** All 30 runs finished, on one rented Linux server, so comparison with stage 4 is across platforms.

| Arm | Seeds holding XOR | Seeds holding HALFADD_PLANES | Seeds holding all four steps | Median generations | Median genome length |
|---|---|---|---|---|---|
| `exact` | 0 of 10 | 0 of 10 | 0 of 10 | 30,004 | 51 |
| `exact+steps` | 1 of 10 (seed 1) | 1 of 10 | 10 of 10 | 7,694 | 110 |
| `exact+steps+islands` | 2 of 10 (seeds 5, 7) | 2 of 10 | 10 of 10 | 8,797 | 110 |

Primary outcome: 1 of 10 against 0 of 10, p = 1.0; 2 of 10 against 1 of 10, p = 1.0; both 1.0 after Holm. XOR was made and kept for the first time in the series, by three populations, all in arms that pay the steps (pooled 3 of 20 against 0 of 10, p = 0.53, not planned). With full-quality pay no `exact` seed holds HALFADD_PLANES, against 10 of 10 for `min-genome` under the lenient rule; every XOR population also holds it, a real half adder. No population went further (HALFADD by 2 organisms at most, ADD2_PLANES by 3). Paying the steps changed the populations (Mann-Whitney p = 0.00001): genomes twice as long (110 against 51), twice the information (86.5 against 41 sites), and a quarter of the generations in 100,000 updates (about 7,700 against 30,000; the classic experiment ran about 15,000). Islands made no difference in any measure (p 0.24 to 1.0). In the test CPU the most abundant genotype of each XOR population performed XOR and HALFADD_PLANES in 30 of 30 rounds. The check also found a leak: HALFADD_PLANES compares even bits only, where `a AND NOT b` equals `a XOR b` with probability about 3 % on a random draw, so an organism outputting ANDN then AND is paid for the half adder on about one draw in thirty without making XOR. This explains small HALFADD_PLANES counts without XOR (26 organisms at most in `exact`, 179 in `exact+steps`); no population held the rung this way, and a rung judged on more than one draw would close the leak.

Verdict: negative on the primary outcome, accepted by the maintainer on 2026-11-17.

### Stage 6: more time below XOR

Lab folder `16-xor-time`. Does paying the steps matter given more seeds and time? Design fixed and revised 2026-11-17, before any run. Arms `exact-long` and `steps-long` (`exact` and `exact+steps` of stage 5), seeds 11 to 26 (sixteen an arm), at most 300,000 updates; a run stops at the first population save 50,000 or more updates after at least 360 organisms first perform XOR. Primary outcome: time to the first XOR hold, censored at 300,000, by a log-rank test with the p value from 100,000 relabellings. A population counts as holding HALFADD_PLANES only if it also holds XOR.

**Results (2026-11-17).** All 32 runs are in; `exact-long` seeds 17 and 25 were stopped by hand at updates 277,100 and 277,700 with no XOR. `exact-long` held XOR in 0 of 16 populations, `steps-long` in 1 of 16 (seed 13, update 90,900, generation 7,739). Log-rank permutation p = 1.0 (chi-square p = 0.32), the same on the generation clock. More time did not help: fifteen paid populations ran 300,000 updates without XOR, and the four populations that found XOR in stages 5 and 6 did so before generation 7,800. Seed 13 held XOR and HALFADD_PLANES, nothing above, when stopped at 150,000. Genomes shrink (unplanned): median length at updates 10,000, 100,000 and 290,000 was 199, 150 and 74 for `steps-long` and 83, 52 and 53 for `exact-long`; seed 13 found XOR at 141 instructions. XOR is found and does not pay: the 32 paid populations of stages 5 and 6 without a XOR hold had 462 episodes with ten or more organisms performing it, most lasting about 200 updates, the longest 9,100, the largest 276 organisms. At the settled resource levels ANDN multiplies merit by about 2.1, NOR by 2.6 and a new, rare XOR by 2.0; the ladder pays XOR 2 where the steps of stage 5 pay 3 and 4, so turning an ANDN into XOR loses a little. Three episodes examined in the test CPU fit the two explanations stage 7 tests and do not prove them.

Verdict: negative on the primary outcome, accepted by the maintainer on 2026-11-18.

### Stage 7: what keeps populations from XOR

Design fixed on 2026-11-17. A random search over six settings (lab folder `17-xor-search`, kept) was set aside before it ran, because stage 6 gave two sharp explanations that a factorial tests directly.

**Question and arms.** Which of three settings of `exact+steps` keeps populations from XOR? Each was tried at its present value and at one other, in all eight combinations: the pay of XOR (2 or 8, so that pay doubles from NOR at 4 to XOR), `divide.min_genome_size` (50 or 100, the ancestor's length) and `mutation.copy_prob` (0.0075 or 0.0025, the classic rate). Seeds 31 to 38 per arm (64 runs), 100,000 updates, every other setting as in stage 5. Lab folder `18-xor-factorial`.

**Primary outcome.** A population has XOR if it holds it (360 organisms at a sample) at any sample. For each setting, populations with XOR at the other value against the present value, 32 against 32, Fisher exact, two-sided, uncorrected and with a Holm correction over the three.

**Results (2026-11-18).**

| Setting | Other value | Present value | Fisher p | after Holm |
|---|---|---|---|---|
| Pay of XOR, 8 against 2 | 32 of 32 | 9 of 32 | 4 x 10^-10 | 1 x 10^-9 |
| `mutation.copy_prob`, 0.0025 against 0.0075 | 24 of 32 | 17 of 32 | 0.12 | 0.23 |
| `divide.min_genome_size`, 100 against 50 | 18 of 32 | 23 of 32 | 0.30 | 0.30 |

All four pay-8 arms had XOR in 8 of 8 populations. Every population that held XOR at some sample held it at the end, and 25 of the 64 hold HALFADD_PLANES at the end. At pay 2, XOR was held by 8 of 16 populations at 0.0025 against 1 of 16 at 0.0075 (p = 0.015, a comparison chosen after seeing the table). The mutation rate sets the genome length and the floor does not: at 0.0025 genomes stay at 290 to 605 instructions, at 0.0075 they fall towards 100 to 160, and the low-rate arms pass 2,900 to 5,300 generations where the others pass 7,400. A floor of 100 does nothing for XOR (at pay 2: 2 of 16 against 7 of 16 at floor 50, p = 0.11). In the test CPU, the most abundant genotypes of six XOR-holding populations perform XOR in 30 of 30 rounds, and all but one HALFADD_PLANES as well.

No population holds anything above HALFADD_PLANES (HALFADD was performed by at most 7 organisms at any sample, ADD2_PLANES by 5, ADD2 by 2). The wall below XOR in stages 5 and 6 was made by the pay scale: the steps added in stage 5 pay 3 and 4 and XOR was left at 2.

Verdict: positive for the pay of XOR, no difference shown for the other two settings; accepted by the maintainer on 2026-11-18.

### Stage 8: the rungs above the one-bit half adder

Design fixed on 2026-11-18. Genotypes from the stage 7 runs that showed a few HALFADD, ADD2_PLANES or ADD2 organisms were run in the test CPU and perform none of them, so those counts are chance. The packed rungs compare the whole word, so the organism must build the mask, mask both inputs, shift the carry and merge, and none of these pieces is paid; the four rungs also pay 4 to 7, below XOR's 8.

**Additions to the simulator** (`PORTING.md`): the environment line `INPUT_AND_MASK`, which clears bits of the inputs every organism is given; the argument `width` of `field_arith`, which limits the judged fields to the low bits; and the functions `carry` (`(a & b) << 1`) and `partial` (`(a ^ b) ^ carry`), the two steps of a ripple adder.

**Arms** (lab folder `19-above-xor`), all from `x8-f50-m25` of stage 7, with the field rungs judged on the low 24 bits (`width=24`); seeds 41 to 52 per arm (48 runs), 100,000 updates.

| Arm | Change |
|---|---|
| `base` | none: the rungs above XOR pay 4, 5, 6, 7 |
| `pay` | they pay 9, 10, 11, 12 |
| `pay+masked` | `pay`, and `INPUT_AND_MASK 0xFF6DB6DB`: the low 24 bits of every input hold two-bit operands in three-bit fields, the layout of ADD2 |
| `pay+masked+steps` | `pay+masked`, and CARRY pays 9 and PARTIAL 10, on resources of their own |

CARRY and PARTIAL are judged in every arm and unpaid outside the last.

**Primary outcome.** A population is above the wall if at any sample 360 organisms hold ADD2_PLANES or ADD2. Planned comparisons, Fisher exact, 12 against 12, with Holm correction: `pay` against `base`, `pay+masked` against `pay`, `pay+masked+steps` against `pay+masked`.

**Results (2026-11-18).** Negative: no population got above the wall, so each comparison is 0 of 12 against 0 of 12 (p = 1). Populations (of 12) that held a rung at some sample:

| Arm | XOR | HALFADD_PLANES | CARRY | PARTIAL | ADD2 |
|---|---|---|---|---|---|
| `base` | 11 | 7 | 0 | 0 | 0 |
| `pay` | 10 | 7 | 0 | 0 | 0 |
| `pay+masked` | 10 | 8 | 0 | 0 | 0 |
| `pay+masked+steps` | 9 | 4 | 12 | 0 | 0 |

Neither the pay above XOR nor the mask holds populations back; the few ADD2 and PARTIAL organisms in the masked arms (at most 43) are chance matches. A paid single step is found by every population: with CARRY paid, all 12 hold it, first between updates 2,700 and 21,700. The step that combines two results is found by none: at the end 7 of 12 populations of `pay+masked+steps` hold XOR and CARRY together in about 3,000 organisms each and none holds PARTIAL. Paying CARRY cost something below it (not planned): 5 of 12 end without XOR and only 4 hold HALFADD_PLANES against 7 or 8 elsewhere. Across stages 5 to 8, a function one further step away is found within some ten thousand updates once it pays more than what it displaces, and a function that needs two intermediate results kept and combined is not found in 100,000 updates.

Verdict: negative, accepted by the maintainer on 2026-11-18.

### Stage 9: graded pay, with and without the genotype's expected quality

An exact rung pays nothing for a nearly right answer. The average quality of an answer against the packed two-bit adder over all operand pairs: an input or zero 0.08, `a OR b` 0.33, `a XOR b` 0.58, `(a XOR b) OR carry` 0.83, PARTIAL 0.92, the sum 1.00. On a single draw the quality of one answer scatters by about 0.16, so a threshold is passed by luck. Expected quality (`expect=1` on a `field_arith` task, `expected_quality.draws`, `expected_quality.min`; `PORTING.md`; exact mode only) pays a genotype its average quality over 8 test runs and nothing below an average of 0.5.

**Question.** Does tiered pay for the two-bit adder get populations past the combining step, and does it need expected quality?

**Arms** (lab folder `20-expected-pay`), 12 populations each, seeds 41 to 52, 100,000 updates, masked inputs: `exact` (the `pay+masked` runs of stage 8, not rerun); `tiers` (three separate ADD2 reactions on the organism's own draw: quality at least 0.45 pays 9, 0.70 pays 10, 0.85 pays 11); `tiers+expected` (the same tiers on the genotype's expected quality). ADD2 (exact, pay 12), PARTIAL and CARRY (exact, unpaid) are judged on the organism's own draw in every arm as the measurement.

**Primary outcome.** Populations holding exact ADD2 (360 organisms at a sample); `tiers+expected` against `exact` and against `tiers`, Fisher exact with Holm correction.

**Results (2026-11-19).** Negative: 0 of 12 in `tiers+expected` against 0 of 12 in `exact` and 0 of 12 in `tiers` (p = 1 for both); no population held PARTIAL either. Populations holding XOR: 10 (`exact`), 5 (`tiers`), 12 (`tiers+expected`); HALFADD_PLANES: 8, 1, 4. The lowest tier was held by 12 of 12 in both tiered arms, the middle tier by 6 and 2, the top tier by none. Expected quality removes the luck (not planned): all 12 hold XOR against 5 of 12 on a single draw (p = 0.005). It does not carry a population over the combining step: PARTIAL is performed by at most 82 organisms at any sample, the top tier by at most 10. The tiers pay for many different outputs: a reaction fires on the first output that passes its threshold, and these organisms output a series of different logic results per gestation, each a further chance to land near the sum. In the test CPU genotypes that do not perform XOR pass the lowest tier in 20 to 29 of 30 draws, and genotypes that perform every logic task pass the middle tier in 12 to 18 draws; averaging over input draws does not remove this, and the quality table holds only for an organism with one output. The tiers also cost something below them (not planned), as HALFADD_PLANES shows.

Verdict: negative, accepted by the maintainer on 2026-11-19. The combining step has now been offered exact pay, a paid step on either side, graded pay and graded pay without the luck, and no population has made it.

### After stage 9: how far away is the combining step

A check on saved populations of stage 8, not a stage. From five `pay+masked+steps` populations that hold XOR and CARRY, the most abundant genotype performing both in 30 of 30 draws was taken, and every single mutation of it (substitution and insertion of each instruction at each position, and each deletion) was run in the test CPU on three draws of masked inputs. Of 174,116 single mutants, seven in ten keep XOR and CARRY and none makes PARTIAL or ADD2 in 2 of 3 draws. The step needs at least two changes, and nothing on the ladder pays for the first, which fits stages 8 and 9. Limits: slip mutations (duplications of a stretch) were not scanned.

The same scan on four genotypes (137,286 single mutants) for five exact functions of `x = a ^ b` and `c = (a & b) << 1` (`field_arith` with `fn=nand_xc`, `and_xc`, `or_xc`, `andn_xc`, `andn_cx`, the steps from which PARTIAL would be composed) found no mutant performing `or_xc`, `andn_xc` or `andn_cx` in 2 of 3 draws; 121 mutants pass `nand_xc` or `and_xc` in 2 of 3 draws and none in more than 17 of 30. Three of these functions would also be paid by coincidence on ordinary draws, so a ladder of them was not run: what is missing is any output that depends on both computed values.

### Stage 10: the same ladder on the experimental hardware

**Question.** Does the hardware change what populations reach above XOR? The experimental hardware (type 3) finds labels by direct search and has no `call`. **Arms** (lab folder `23-heads-ex`), 12 populations each, seeds 41 to 52, 100,000 updates, on the stage 8 ladder with CARRY 9, PARTIAL 10 and masked inputs: `modular` (the modular heads set) and `ex` (a heads-style set with `label`, `search-lbl-direct-s`, `if-copied-lbl-direct`, and an ancestor written for it).

**Results (2026-11-20).** Populations holding a rung at the end, `modular` against `ex`: CARRY 12 and 12, XOR 7 and 9 (p about 0.67), HALFADD_PLANES 4 and 7 (p about 0.41), PARTIAL 0 and 0, ADD2 0 and 0. The hardware does not hold populations below the combining step.

Verdict: negative, accepted by the maintainer on 2026-11-20.

### Stage 11: one judged output, graded pay, and gate instructions

Stage 9 left two lessons: graded pay must judge one designated output, and nothing pays for the first of the two or more changes the combining step needs.

**One judged output with graded pay.** A reaction with `requisite: {divide_only: 1}` is tested at divide, on the organism's newest output, once per life. The reaction GRADED pays `2^(12 q)` for the quality `q` of that output against the packed two-bit sum (`field_arith`, `fn=add:bits=2:mode=packed:width=24:thresh=0.25`; `q` is `max(0, 2f - 1)` for a share `f` of right bits) and uses no resource. Four unpaid reactions on the same output, Q50, Q70, Q90 and Q100 (quality at least 0.5, 0.7, 0.9 and exactly 1.0), are the measurement. The eight logic rungs NOT to XOR are paid as in stage 8.

**Gate instructions** (`PORTING.md`). `gate-0` to `gate-15` each write one two-input logic function of BX and CX to a register; the instruction's number is the function's truth table, so `gate-6` is XOR and `gate-8` is AND. A copy mutation that hits a gate writes, three times in four, the gate whose table differs in one entry, so one mutation changes the result on a quarter of the input pairs.

What one output scores against the packed two-bit sum over masked inputs (mean quality; share passing 0.7 / 0.9 / exactly 1.0): `a XOR b` 0.58 (26 / 5 / 1 %); XOR with the shifted carry merged by OR 0.83 (74 / 35 / 35 %); XOR with the carry merged by XOR, the one-round sum, 0.92 (99 / 74 / 35 %); the sum 1.00 (100 % throughout).

**Arms** (lab folder `24-soft-gates`), 12 populations each, seeds 41 to 52, 100,000 updates: `graded` (the modular heads set) and `graded+gates` (the same with `gate-0` to `gate-15`).

**Results (2026-11-20).** The outcome measure was chosen after the data were seen: a population is past the XOR level when most of the organisms that pass Q50 also pass Q70. With gates 5 of 12 populations are past the XOR level against 0 of 12 without (Fisher p = 0.037); none holds the sum. They crossed at updates 28,900 to 95,300, while in `graded` seven of twelve lost XOR. The most common organism of each of the five, alone with mutation off: three match the one-round sum, one is an intermediate below it, one had too few offspring to judge. For the first time a population's output depends on two computed values. The genomes are 224 to 1,019 instructions long and mostly gates.

Verdict: positive for the step past XOR, on a measure chosen after the fact; no population holds the sum. Accepted by the maintainer on 2026-11-20.

### Stage 12: three times the run length, and whether the gate mutation rule matters

**Question.** Does the sum appear with more time, does the gates' mutation rule matter, and does a higher mutation rate help? **Arms** (lab folder `25-gates-long`), 10 populations each, seeds 41 to 50, 300,000 updates, environment of stage 11: `gates` (as `graded+gates`); `pgates` (`pgate-0` to `pgate-15` in place of the gates: the same functions, with a copy mutation writing a random instruction as for any other); `gates-mut3x` (`gates` at copy mutation rate 0.0075 in place of 0.0025).

**Results (2026-11-20).** All 30 runs ran to 300,000 updates. Populations at the end, by the shares of Q70, Q90 and Q100 among the organisms that pass Q50, at the XOR level or just above, at the one-round sum, at another intermediate, and holding the sum: `gates` 4, 4, 1, 1; `pgates` 5, 2, 1, 2; `gates-mut3x` 5, 4, 0, 1. Four populations hold the two-bit adder (`pgates` seeds 42 and 44, `gates-mut3x` seed 49, `gates` seed 46), the most common organism of each scoring exactly 1.0 on every judged output when alone with mutation off; their genomes are 190 to 448 instructions. No arm stands out: one, two and one of ten is no difference, and the arms get past the XOR level equally often (6, 6 and 5 of 10 for the two arms with the rule and the one without). Time mattered, since three of the four reached the sum after update 150,000. There are at least two routes: `pgates` 44 held the OR form from 40,000 to 70,000 and never the one-round sum, while the others passed through the one-round sum, and the wait below the sum ranges from 30,000 to more than 200,000 updates. Limit: no arm without gates at this run length.

Verdict: positive, the two-bit adder evolved in 4 of 30 populations. Accepted by the maintainer on 2026-11-20.

### Stage 13: wider sums, the size of the pay step, and the gates over a long run

Design fixed on 2026-11-21. Lab folder `28-weekend`. The environment and instruction sets of stage 12, copy mutation rate 0.0025; only the judged sum, the input mask and the pay of GRADED change.

**Ancestors.** The most common organism of each of the four populations holding the two-bit sum at the end of stage 12: `p42` and `p44` (`pgates` seeds 42 and 44), `m49` (`gates-mut3x` seed 49, evolved at copy rate 0.0075) and `g46` (`gates` seed 46). Alone with mutation off, 51 to 60 % of their outputs are exact against the three-bit sum and 9 to 25 % against the four-bit sum.

**Wider sums.** `field_arith` with `bits=3` (four-bit fields, mask `0xFF777777`) or `bits=4` (five-bit fields, mask `0xFF07BDEF`); the Q marks judge the same sum as GRADED. At `2^(12 q)` the last step to the three-bit sum is worth 1.31 times the pay, against 1.94 for the last step to the two-bit sum in stage 12; at `2^(32 q)` it is worth 2.06.

**Rule for holding a sum.** A population holds a sum when at least 90 % of the organisms that pass Q50 score 1.0, confirmed on the most common organism alone with mutation off.

**Arms.** `v12-*` and `v32-*`: each of the four ancestors, three-bit sum, paid `2^(12 q)` and `2^(32 q)`, seeds 61 to 63 (12 populations each), 300,000 updates. `w4-*`: each ancestor, four-bit sum, `2^(64 q)`, seeds 41 to 43, planned for 3,000,000 updates. `pgates-long` and `nogates-long`: the modular ancestor with and without gates, two-bit sum, seeds 51 to 57, 3,000,000 updates. A cut-off at 08:00 CET on 2026-11-24 applied to runs on rented 32-core servers.

**Change on 2026-11-21** (agreed with the maintainer). After about 330,000 updates 9 of 12 `w4` populations showed the four-bit pattern, so question 2 was answered; on random inputs long carry chains are rare, and nothing pushes further. The `w4` runs were stopped between updates 380,600 and 432,100 and their cores went to a new arm `s4`: the modular ancestor with the pgate set, four-bit sum, `2^(64 q)`, seeds 51 to 62, 3,000,000 updates.

**Primary outcomes.** (1) Does the size of the pay step decide the three-bit sum? Populations holding it at 300,000 updates, `v32` against `v12`, 12 against 12, Fisher exact. (2) Does the climb go on to four bits? Populations of `w4` holding the four-bit sum at the end, of 12. (3) Are the gates necessary given time? Populations past the XOR level at the end (the rule of stages 11 and 12), `pgates-long` against `nogates-long`, 7 against 7, Fisher exact. (4) Can a population climb from scratch to the four-bit sum? Populations of `s4` holding it at the end, of 12.

**Results (2026-11-24).** `nogates-long` seed 51 was slow and was stopped at the cut-off at update 2,724,400, so that arm is compared at that update. The batch cost EUR 9.80.

Question 1: the pay step does not decide the three-bit sum. 9 of 12 hold it at `2^(12 q)` against 11 of 12 at `2^(32 q)` (p = 0.59). All misses come from `g46`, whose genomes are the longest (315 to 530 instructions); in three of its four populations below the bar the most common organism computes the sum exactly alone and offspring that do not pull the share down.

Question 2: the climb from two to four bits is short. When the `w4` runs were stopped, 10 of 12 populations held the four-bit sum, and in all 12 the most common organism computes it exactly alone; the two below the bar are `g46` seeds 41 and 43. In the nine populations of `m49`, `p42` and `p44` the share stayed above 90 % from update 6,200 to 67,600, and genomes did not grow by a block per carry round (254 to 383 instructions in eight of the nine, against 190 to 448 in the ancestors).

Question 3: past the XOR level 7 of 7 in `pgates-long` against 2 of 7 in `nogates-long` (p = 0.021, planned). The one-round sum or higher was reached by 7 of 7 against 0 of 7, and the two-bit sum by 4 of 7 against 0 of 7 (p = 0.070). The four holders held the sum above 90 % from updates 112,400, 184,600, 206,900 and 413,800. Without gates five of seven populations are still at the XOR level after 2.7 million updates.

Question 4: 1 of 12 `s4` populations holds the four-bit sum (seed 52, above 90 % from update 81,100, faster than any `pgates-long` population reached the two-bit sum); seed 51 computes it exactly but its share is 87 %. Seeds 51, 56 and 57 stayed at lower levels for over a million updates before half their outputs were exact. On the four-bit judge, among outputs that pass 0.5, the shares passing 0.7, 0.9 and 1.0 are: XOR 41/2/2 %, OR-merge form 65/22/22 %, one-round sum 84/18/18 %, two-bit adder 99/58/58 %, three-bit adder 100/88/88 %.

Reading: the combining step is made reliably once it has been made once, so from an evolved two-bit adder the three- and four-bit sums followed in nearly every population at either pay step. From scratch the obstacle is the first combining step: with gates every population passed XOR and four of seven reached the two-bit sum in 3 million updates, without them none reached the one-round sum. Whether the steeper pay of the four-bit judge explains seed 52 was not tested. Limits: 12 populations per arm from four ancestors are 4 lineages, and the pay comparison covers only the step from two to three bits.

Verdict: positive on questions 2 and 3, negative on question 1, and 1 of 12 on question 4. Accepted by the maintainer on 2026-11-24.

### After stage 13: how the adders work and how they were built

A study of saved populations, not a stage of its own (2026-11-24, lab folder `29-lineage`). Nine lines of descent: stage 12 `pgates` seeds 42 and 44, `gates-mut3x` seed 49 and `gates` seed 46; stage 13 `pgates-long` seed 54, `s4` seeds 51 and 52, `w4-m49` seed 42 and `w4-g46` seed 42. Each runs from the most abundant genotype of the last saved population back to the injected ancestor through the parent column; every genotype on it was run in the test CPU on 100 draws and its level read from the shares passing 0.7, 0.9 and 1.0, and where one birth brought several changes every subset was run on 200 draws to find which were needed. For the logic, the most abundant genotype of each line was traced and its dataflow rebuilt and checked against the sum on every pair of field values.

**All nine adders are straight-line code and exact on every pair of field values.** None uses a loop or `call`; every value on the way to the output is computed once, at its own place in the genome. Six of nine (`pgates` 42 and 44, `gates-mut3x` 49, `pgates-long` 54, `s4` 52, `w4-m49` 42) ripple with the sum updated in place. With `s0 = a ^ b` and carry word `c1 = (a & b) << 1`, each round is `gate-4` (BX = BX & ~CX), `shift-l` and `gate-6`, `nop-C` (CX = BX ^ CX): a block that takes carry and partial sum in two registers and leaves them in the same two. Three lines found it independently from scratch (`gates-mut3x` 49, `pgates-long` 54, `s4` 52) and `w4-m49` 42 inherited it; `pgates` 44 uses the mirrored gate and `pgates` 42 keeps the carry complemented. The other three (`gates` 46, its descendant `w4-g46` 42, and `s4` 51) compute the carry into every bit at once by iterating `K = (g | (t & K)) << 1` and output `a ^ b ^ K`; they read the inputs again with `IO` inside the rounds, and their genomes are longer (232 to 575 instructions).

**The first combining step took one change in each of the five lines where it was tested.** In the births that took a population from the XOR level to the OR-merge form or the one-round sum, the parent was one change away from the step and the other changes of the birth were not needed: `pgates` 42 at update 49,725, `pgates` 44 at 28,835, `gates-mut3x` 49 at 145,552, `gates` 46 at 28,245 and `pgates-long` 54 at 54,731.

**A population at the XOR level is not one mutation from the step; the lines got there by a change that did nothing at the time.** The one-mutation neighbourhood is every substitution and every tandem duplication of up to 60 instructions, a neighbour counting as past the XOR level when more than 60 % of its outputs passing 0.5 also pass 0.7 (XOR 26 to 38 %, the OR-merge form 74 %). The most common organism at update 50,000 of each of the 14 `pgates-long` and `nogates-long` populations of stage 13 has no such neighbour; on the lines of descent the parent of the step has 3 to 34. Going back, the first genotype with one arose by a change that did little or nothing (`pgates` 42 at update 39,923, `gates` 46 at 24,670, `gates-mut3x` 49 at 145,219, `pgates-long` 54 at 51,773), 333 to 9,800 updates before the step, and undoing it in the genotype that made the step takes the gain away. In `gates-mut3x` 49 it made an already written carry round intact while the output still came from another register; in `gates` 46 a `nop-D` sent `a & b` into DX, where nothing overwrote it. In both, a computed value was kept alive before anything used it. Lenski and others (2003) found that complex functions in Avida came by way of mutations that were neutral or even harmful when they arose; Blount, Borland and Lenski (2008) call such a preparing change potentiation. Four lines do not say how often this happens without gates.

**Each further carry round came, in 12 of 15 steps, from a tandem duplication of the round's code**, made by a slip mutation (`divide_slip_prob` 0.05) that copied a stretch next to itself; the stretches were 5 to 50 instructions, and `w4-m49` 42 took its three- and four-bit sums by duplications of 6 and 12 instructions within 1,400 updates. The three other steps belong to the carry-vector line of `gates` 46, whose two-bit sum came by substitutions between updates 242,881 and 276,937 and whose descendant `w4-g46` 42 reached the three- and four-bit sums by single substitutions at updates 14,034 and 203,931.

**From each two-bit adder, a duplication is the only single mutation that gives the three-bit sum.** Neighbourhoods of the four stage 13 ancestors on the three-bit judge (duplications up to 120 instructions):

| Ancestor | Length | Substitutions giving the sum | Duplications giving it | Rate per birth (`v32` settings) |
|---|---|---|---|---|
| `m49` | 190 | 0 of 8,360 | 19 of 15,660 | 2.6e-5 |
| `p44` | 211 | 0 of 9,284 | 19 of 18,180 | 2.1e-5 |
| `p42` | 448 | 0 of 19,712 | 12 of 46,620 | 3.0e-6 |
| `g46` | 398 | 0 of 17,512 | 402 of 40,620 | 1.3e-4 |

At about 136 births per update (60 x 60), a population of `m49` or `p44` makes such a mutant every 300 updates or so and one of `p42` every 2,500; they took 3,400 to 9,300, 2,800 to 3,600 and 16,400 to 35,000 updates to reach 90 % exact in `v32`, about ten times as long as the first mutant needs to appear. `g46` makes them most often and was the slowest (52,000 to 278,000): its duplications are long, near the 120 the search stopped at, and its populations, with the longest genomes, kept many offspring that did not compute the sum.

**Reading.** Because a round is a function that can be applied again, a tandem duplication gives one more carry round in one mutation. That explains why the climb from an evolved two-bit adder was short and the first combining step the hard one: before the first round exists there is nothing to duplicate, and the first round was reached only after a change that did nothing at the time had kept a computed value alive. Duplication stands in for the loop that no population built, and `call` stayed unused. Limits: nine lines chosen among populations that reached the sum, so the study says nothing about why others stalled; neighbourhoods hold single mutations only, duplications up to 60 or 120 instructions, and no slip deletions.

**A loop puts reproduction behind the adder's control sites** (measured 2026-11-24, at the maintainer's question whether the single instruction pointer keeps loops out). The four unrolled rounds of the `s4` seed 52 adder (sites 331 to 346) were replaced by hand with one round in a loop that runs until the carry is zero (23 sites in place of 16). It computes the four-bit sum exactly on 400 draws, and without its backward jump falls back to one carry round (21 % exact). Every substitution at the adder's sites, on the four-bit judge with 20 draws:

| Version | Adder sites | Substitutions | Inviable | Breed true, sum broken | Sum intact |
|---|---|---|---|---|---|
| unrolled (evolved) | 20 | 880 | 12.5 % | 79.1 % | 8.4 % |
| loop | 27 | 1,188 | 35.8 % | 56.4 % | 7.8 % |

Almost every inviable one never reaches the divide. In the unrolled adder one site stops the organism whatever replaces it; in the loop six do and four more about half the time, because a changed exit test or body leaves the carry nonzero and the organism dies of old age without dividing. That comes to about 1.7 % more offspring that never divide at `copy_prob` 0.0025, and the loop costs time as well (gestation 852 to 861 cycles against 823, fitness 1.6 to 2.5 % lower, N s around 60 in a population of 3,600). The adder is about equally brittle either way; unrolled, a broken adder falls back to a lower level and still reproduces, while in a loop it often stops reproduction. Limits: one hand-written loop in one genome. Tools in `29-lineage` (`nbhd.py`, `confirm.py`, `pot.py`, `loopvar.py`, `loopclass.py`).

### Stage 14: does the extension of the adder need duplication?

Design fixed on 2026-11-24, before any run. Lab folder `30-slip`.

**Question.** Each extra carry round came, in 12 of 15 cases, from a tandem duplication, and from each of the four two-bit adders no single substitution gives the three-bit sum while 12 to 402 duplications do. If duplication is the route, populations that cannot copy a stretch of their own code should extend the adder far more slowly or not at all, with the same rate of insertions and deletions.

**Arms.** All four use the `v32` arms of stage 13 unchanged (three-bit judge, `2^(32 q)`, the two-bit ancestors `m49`, `p42`, `p44`, `g46`, copy mutation 0.0025, 60 x 60, exact mode) and differ only in what a slip mutation (`divide_slip_prob` 0.05 per divide, half of them deletions) inserts, set by `mutation.slip_fill_mode`: `dup` (`duplication`) a copy of the stretch in order, as in stage 13; `scrambled` the same instructions in shuffled order; `random` random instructions of the same length; `noslip` no slip mutation (`divide_slip_prob` 0). Six seeds per ancestor (71 to 76): 24 populations per arm, 96 runs of 150,000 updates.

**Outcome.** Populations holding the three-bit sum at 150,000 updates by the rule of stage 13 (at least 90 % of the organisms that pass Q50 score 1.0, confirmed on the most common organism alone with mutation off). Planned comparison: `dup` against `random` (which keeps the length changes and takes away only the copying), Fisher exact, two-sided. Secondary: `scrambled` against `random`, `noslip` against `random`, and the time to 80 % exact.

**Prediction**, written before the runs: `dup` 20 or more of 24 (all 18 of `m49`, `p42` and `p44`, and some of `g46`); the others clearly fewer, `scrambled` like `random`, since a shuffle restores a round's order rarely (1 in 360 for the shortest round found). If `random` did as well as `dup`, the reading after stage 13 would be wrong.

**Results.** All 96 runs reached 150,000 updates; the batch cost about EUR 1.60.

| Arm | `m49` | `p42` | `p44` | `g46` | Hold the three-bit sum | Reach 80 % exact | Update of 80 % exact |
|---|---|---|---|---|---|---|---|
| `dup` | 6 of 6 | 6 of 6 | 6 of 6 | 2 of 6 | **20 of 24** | 24 of 24 | 2,000 to 42,000 |
| `scrambled` | 3 of 6 | 1 of 6 | 3 of 6 | 0 of 6 | **7 of 24** | 7 of 24 | 13,000 to 132,000 |
| `random` | 0 of 6 | 3 of 6 | 0 of 6 | 0 of 6 | **3 of 24** | 4 of 24 | 56,000 to 122,000 |
| `noslip` | 0 of 6 | 2 of 6 | 3 of 6 | 0 of 6 | **5 of 24** | 5 of 24 | 62,000 to 144,000 |

No population passed the share and failed the confirmation. Five populations have an exact most common organism but stand below 90 % (four `dup` populations of `g46` at 83.0 to 90.0 %, and `random` `g46` seed 74 at 85.6 %), because `g46`'s share is pulled down by offspring that do not compute the sum.

Planned comparison: `dup` 20 of 24 against `random` 3 of 24, Fisher exact two-sided p = 1.4e-6. The prediction is met as written. Secondary: `scrambled` against `random` 7 against 3 (p = 0.29), `noslip` against `random` 5 against 3 (p = 0.70). Not planned: `dup` against `scrambled`, p = 0.00036.

**Reading.** Copying a stretch next to itself is the fast route to the next carry round: every `dup` population reached 80 % exact by update 42,000, all twelve of `m49` and `p44` within 7,000, while populations without copying that got there took 56,000 to 144,000 updates. It is the fast route and the only fast one, since 8 of the 48 populations that could not copy (`random` and `noslip`) extended the adder, all from `p42` and `p44`. An inserted random stretch did no better than no slip (3 against 5), so the slow route is more likely a series of substitutions and single insertions. Limits: four ancestors, six seeds each, one step; the route is inferred from the arm.

Verdict: positive on the planned comparison, as predicted. Accepted by the maintainer on 2026-11-25. Tools in `30-slip` (`confirm.py`, `diffanc.py`).

**After stage 14: how the seven `scrambled` populations built their round** (traced 2026-11-25). On each line of descent the three-bit sum arrives in one birth, from 62 to 73 % exact to 100 %, and stays. In five of the seven lines the birth inserts a shuffled copy of the stretch that holds a carry round (`m49` seeds 71, 72 and 76, `p44` seeds 71 and 72; copies of 4 to 15 instructions); the other two came by one substitution (`p42` seed 75, `p44` seed 74). A shuffle keeps the order of the four-instruction round of `m49` once in 24, and the rounds of `p44` lie in longer stretches mostly of instructions that do nothing there. Most `scrambled` populations therefore extended the adder by copying too, which fits `scrambled` doing better than `random` for `m49` (3 of 6 against 0) and `p44` (3 against 0), not for `p42` (1 against 3). Tools: `lod_eval.sh`, `lod_story.py`, `stepcheck.py`, `slipcheck.py`.

### Stage 15: is a whole adder composed by copying too?

Design fixed on 2026-11-25, before any run. Lab folder `32-compose`.

**Question.** An evolved adder is a function one level up from a carry round: it reads two words and leaves their sum in a register where the next input can meet it. Does copying compose whole adders the way it composed rounds, so that a population that adds two numbers comes to add three?

**Task.** A new opt-in function of `field_arith`, `add3`: the sum of the three newest inputs in `w + 2` result bits per field. With inputs masked to 3-bit operands in 5-bit fields (`INPUT_AND_MASK 0xFF739CE7`; `bits=3`, `stride=5`, `width=24`), an evolved four-bit adder applied to its own sum and the next input computes it.

**Ancestors.** The most common organism of five populations that hold the four-bit sum at the end of stage 13: `s52` (`s4` seed 52, from scratch; 375 instructions) and the four `w4` populations of seed 42, `m49` (621), `g46` (575), `p42` (353) and `p44` (317). Each computes the two-operand sum of the new layout exactly on 400 draws and the three-operand sum on none. One-mutation neighbourhoods measured before the runs: from `m49`, `s52` and `p44` single duplications give the three-operand sum exactly (56, 7 and 7 of them, each copying 16 to 73 instructions that hold IO instructions together with the adder's rounds); from `p42` and `g46` no single mutation does, and from none of the five does a substitution.

**Arms.** `dup` against `random` (`scrambled` and `noslip` left out, since neither differed from `random` in stage 14), otherwise as the `w4` arms of stage 13, one judged output (the newest at the divide), paid `2^(64 q)` for `add3` and `2^(32 q)` for the two-operand sum (both thresh 0.25). Six seeds per ancestor (91 to 96): 30 populations per arm, 60 runs of 150,000 updates.

**Outcome.** Populations holding the three-operand sum at 150,000 updates by the rule of stage 13 (at least 90 % of the organisms that pass Q50 on `add3` score 1.0, confirmed on the most common organism alone with mutation off). Planned comparison: `dup` against `random`, Fisher exact, two-sided. Secondary: within `dup`, the three ancestors one duplication from the sum (18 populations) against the other two (12); time to 80 % exact.

**Prediction**, written before the runs: `dup` 18 or more of 30, among them all 18 of `m49`, `s52` and `p44`; `random` clearly fewer; within `dup`, `p42` and `g46` fewer and later. A smoke run on seed 81 (moved out of the planned seeds before any planned run) had 75 % of the organisms passing Q50 exact at update 3,000 in `dup-m49` and 3 of 1,106 in `dup-s52`.

**Results.** All 60 runs reached 150,000 updates; the batch cost about EUR 0.61. The most common organism of every population was run alone with mutation off on 400 draws of the `add3` judge.

| Arm | Measure | `s52` | `m49` | `p44` | `p42` | `g46` | All |
|---|---|---|---|---|---|---|---|
| `dup` | holds the sum (the rule) | 1 | 0 | 1 | 0 | 0 | **2 of 30** |
| `dup` | most common organism exact on 90 % of draws or more | 5 | 3 | 4 | 3 | 0 | 15 of 30 |
| `random` | holds the sum (the rule) | 0 | 0 | 0 | 0 | 0 | **0 of 30** |
| `random` | most common organism exact on 90 % of draws or more | 0 | 2 | 0 | 0 | 0 | 2 of 30 |

Two most common organisms are exact on every judged output: `dup` `p44` seed 94 (population 91.1 % exact) and `dup` `s52` seed 92 (96.0 %). Fifteen others stand at one level, exact on 91.0 % of the draws (91.5 % for two), passing Q50 on all of them and failing on the same 36 draws; four are at about 70 %, the rest at 0 to 33 %. `dup` `m49` seed 94 passes the population share (90.7 %) but its most common organism is at 91.0 %, so it fails the confirmation; `dup` `p44` seed 91 stands at 89.8 %. Every organism at 70 % or above has given up the exact two-operand sum, which is paid less. Most `random` populations ended at one of two lower levels (6.9 % exact with Q50 on 65 % of draws, 10 populations; 3.9 % with Q50 on 84 %, 6), with shrunken genomes.

**What the 91 % level is.** It matches an adder of four carry rounds applied twice, one carry round short. With 3-bit operands the first application gives `a + b` exactly; the second adds `c` to that, and a carry that runs from bit 0 to bit 4 of a field needs a fifth round. A model of rounds of XOR, AND and shift on 200,000 random draws of the mask gives 90.9 % exact for four rounds then four, and 100 % for four then five. The genomes have not been read to confirm it.

Planned comparison: `dup` 2 of 30 against `random` 0 of 30, Fisher exact two-sided p = 0.49. The prediction (18 or more in `dup`) is not met. Secondary: within `dup`, the ancestors one duplication from the sum 2 of 18 against the other two 0 of 12 (p = 0.50). Time to 80 % exact: 14 of 30 `dup` populations, from update 3,200 to 140,500; 2 of 30 `random`, at 107,100 and 148,400. Not planned, with the measure chosen after the data: a most common organism exact on 90 % of the draws or more, `dup` 15 of 30 against `random` 2 of 30, p = 0.00037; within `dup`, 12 of 18 against 3 of 12 (p = 0.060).

**Reading.** Copying composes the adder with itself, one carry round short: in half the `dup` populations the most common organism feeds an adder's sum and the next input to an adder, against 2 of 30 with a random stretch, and the fastest got there within 5,000 updates. Two populations added the missing round; why it came so rarely is open. A recount from `m49`, `p42` and `g46` at a bar of 88 % (`confirm.py` with `HI=0.88`) found no single mutation that reaches only the 91 % level, so the populations at 91 % got there by more than one mutation or by an exact organism losing a round. `p42` reached the level in 3 of 6 `dup` populations although no single mutation from it gives the three-operand sum.

Verdict: negative on the planned comparison; the prediction failed. Accepted by the maintainer on 2026-11-25. Tools in `32-compose` (`popconfirm.py`, `popconfirm.tsv`).

**After stage 15.** Traced on 2026-11-25 and 2026-11-26, after the maintainer accepted the stage.

*How the 91 % level arose.* On nine lines of descent (the two exact populations, six at 91 %, and `random` `m49` seed 95), every genotype run on 100 draws, the level arrives in one birth, from at most 30 % exact to 87 to 98 %: in four `dup` lines by a tandem copy of 17 to 61 instructions (updates 1,421 to 22,744), in the others by one or two substitutions or by a deletion and an insertion. Two populations counted at 91 % were exact for most of the run (`m49` seed 94 from update 4,773 to 148,615, `p44` seed 91 from 64,488 to 146,358); their end organisms are recent variants one round short. The four populations that held the exact sum for much of the run stand at 85 to 97 %, the others at 91 % at 77 to 88 %, so the bar of 90 % falls inside the range of the exact populations, as it did for `g46` in stage 14.

*What the end populations hold.* In samples of 300 organisms from six end populations (100 draws each), exact organisms were 54, 50, 41 and 21 % in `dup` `s52` seed 92, `m49` seed 94, `p44` seed 94 and `p44` seed 92, and 0 % in `p42` seed 93 and `s52` seed 91, which held 62 and 59 % at the 91 % level. At about 0.9 mutations per birth, a third to a half of the organisms compute neither level; where the exact sum is held, 79 to 89 % of the organisms that compute one of the two levels are exact.

*The missing round is one duplication away.* The neighbourhoods of three most common organisms at 91 % (`p42` seed 93, `p44` seed 92, `s52` seed 91; duplications up to 40 instructions, confirmed on 400 draws at 99 %) contain 29, 285 and 2 duplications that give the exact sum, at 1.0e-5, 1.4e-4 and 1.5e-6 per birth, and no substitution that does. Each run made 5.0 to 6.3 million births after update 30,000, so about 50, 860 and 10 such offspring were born.

*Competitions and offspring.* Each of the three organisms at 91 % shared a 60 x 60 world with an exact neighbour or with a control that stays at 91 %. With mutation off the exact lineage gained about 4 % per generation for `p44` seed 92 and `s52` seed 91 and 2 % for `p42` seed 93 (the pay rule suggests about 9 %); with the runs' mutation rates it ended at 90 to 100 % of the world for the first two and at 39 to 65 % for `p42` (six seeds, about 0.3 % per generation). The exact genomes are not less robust: of 1,500 offspring drawn with the runs' mutations, 61 to 68 % are exact, they lose about as many offspring to mutation as the 91 % genomes, and an exact offspring falls back to 91 % in 0.1 to 0.5 % of births.

*Reading.* The step in reach was taken where it pays and arises often enough. In `p44` seed 92 a fifth of the end organisms are exact and the exact lineage wins with mutation on; in `s52` seed 91 such offspring are rare (about 10) and most are lost by chance; in `p42` seed 93 the exact sum pays little under mutation, for reasons not measured, so the 50 or so such offspring had little chance. Limits: three organisms, one to six seeds per competition.

*A holding rule by sample* (tried on stage 15 after the data, 2026-11-26; it does not change the verdict). A population holds a level when, in samples of 300 organisms drawn at random from each of its last two saved populations (updates 100,000 and 150,000), at least half of the organisms that compute (viable in the test CPU, and passing Q50 on at least half of 100 draws) are exact. On stage 15 it counts 4 of 30 `dup` populations and 0 of 30 `random` (Fisher p = 0.11): `m49` seed 94, `p44` seeds 91 and 94 and `s52` seed 92, the four whose lines were exact for most of the run, with 59 to 84 % of the computing organisms exact. It was proposed for the next stage in place of the confirmation on the most common genotype. Tools in `32-compose`: `sample_rule.py` and `sample_rule.tsv`, with `lod_eval.sh`, `lod_levels.py`, `lod_copies.py`, `run_nbhd91.sh`, `sample_eval.py`, `make_comp.py`, `run_comp.sh`, `run_compmut.sh` and `robust.py`.

### Stage 16: can the adders re-adapt to eight inputs?

Design written on 2026-11-26, before any planned run. Lab folder `33-compose4`.

**Why this stage.** Whether composition repeats, so that a population that adds three numbers comes to add more, needs more inputs, and `field_arith` now has them (`add4` to `add8`, the sum of the `n` newest inputs read). The adders of stage 15 execute `IO` 18 to 27 times in a life; with three inputs handed out any three consecutive reads are the three inputs, but with eight none of the ancestors chosen below is exact on the sum of the three newest reads (four sum three other reads, three of the rest barely reproduce). A ladder of sums needs ancestors that are, and this stage asks whether evolution provides them.

**Question.** Do the populations re-adapt, rewiring which reads the adder sums until it computes the sum of the three newest reads exactly? And does a complete module, exact on three operands in the old world, re-adapt more readily than one a carry round short?

**World.** The stage 15 world of each ancestor's family (the `dup` fill), with 3-bit operands in 6-bit fields (`INPUT_AND_MASK 0xFF1C71C7`) and eight inputs handed out (an unpaid mark of `add8` makes the environment ask for eight). The judge is that of stage 15 with the lower rung unpaid: the newest output at the divide, the sum of the three newest reads paid `2^(64 q)` for q of at least 0.25, with unpaid marks Q50 to Q100 on it and unpaid marks of the two-operand sum. A smoke run with the two-operand sum paid showed a population from `m49` seed 94 dropping the fourth read and becoming exact on the two-operand sum within 1,000 updates.

**Ancestors.** From the stage 15 end populations (update 150,000): the most abundant organism exact on `add3` in each of the four populations that hold it by the rule by sample (`e`: `m49` seed 94, `p44` seeds 91 and 94, `s52` seed 92; 298 to 396 instructions), and the most abundant organism at the 91 % level in four populations without exact organisms (`h`: `m49` seed 93, `p44` seed 95, `s52` seed 91, `p42` seed 93; 257 to 443 instructions). In the new world none is exact on any of 400 draws; they pass Q50 on 0 to 20 % of the draws, and `p44` seeds 91 and 94 and `p42` seed 93 reproduce on 56, 0 and 2 % of them, the other five on all. In the one-mutation neighbourhoods on this judge no single mutation from any of the eight makes it exact; 490 neighbours of `m49` seed 94 pass Q50 on at least 8 of 20 draws (the ancestor on 17 % of 400) and 177 of `s52` seed 91 (the ancestor on 20 %), against at most 60 for each of the other six.

**Runs.** Six seeds per ancestor (101 to 106): 48 runs of 150,000 updates, exact mode, the population saved every 50,000 updates.

**Outcome.** The rule by sample (`sample_rule.py`): from the saved populations at 100,000 and 150,000 updates, 300 organisms are drawn and each genotype is run alone on 100 draws of eight inputs. An organism computes when it reproduces on at least half of the draws and passes Q50 on at least half; it is exact when it is exact on at least 0.99 of the draws on which it passes Q50. A population holds the sum when at least half of the computing organisms are exact in both samples. Planned comparison: the populations that hold the sum, `e` against `h` among the five ancestors that reproduce on every draw in the new world (`e`: `m49` seed 94 and `s52` seed 92, 12 populations; `h`: `m49` seed 93, `p44` seed 95 and `s52` seed 91, 18), Fisher exact, two-sided. Secondary: the count per ancestor; the populations whose computing organisms are at least half at the 91 % level or better (0.84 of the Q50 draws exact); the rule of stage 13 at 150,000 updates; the first update at which at least 80 % of the organisms that pass Q50 are exact.

**Prediction**, written before the runs: `p44` seeds 91 and 94 and `p42` seed 93 mostly die at once or survive without the sum; of the 30 populations of the other five, 5 to 20 hold the sum at 150,000 updates, and at least half of those from `s52` seed 91 do. `e` and `h` do not differ.

**Results.** All 48 runs ended; the batch cost about EUR 0.51. Of the 18 populations of `p44` seeds 91 and 94 and `p42` seed 93, 17 died within 500 updates; `p44` seed 91 on seed 105 lived to 150,000. In the 30 populations of the five other ancestors the sum was taken up early or not at all: in 13, at least 100 organisms passed Q50 by update 2,500, and in the other 17 never more than 28 did in 150,000 updates.

| Ancestor | Took the sum up by update 2,500 | Computing organisms at 91 % or better | Holds the sum (the rule) | Rule of stage 13 at 150,000 |
|---|---|---|---|---|
| `e` `m49` seed 94 | 5 | 5 | 1 | 1 |
| `e` `s52` seed 92 | 0 | 0 | 0 | 0 |
| `h` `m49` seed 93 | 1 | 0 | 0 | 0 |
| `h` `p44` seed 95 | 2 | 2 | 2 | 2 |
| `h` `s52` seed 91 | 5 | 5 | 0 | 1 |
| `e` `p44` seeds 91 and 94, `h` `p42` seed 93 | 0 of 18 | 0 | 0 | 0 |
| All | 13 of 48 | 12 of 48 | **3 of 48** | 4 of 48 |

Each cell counts populations of six. "Took the sum up" is a measure chosen after the data.

Three populations hold the sum: `m49` seed 94 on seed 103 (126 of 183 computing organisms exact at update 100,000, 158 of 204 at 150,000) and `p44` seed 95 on seeds 102 (99 of 122, 119 of 140) and 105 (98 of 148, 129 of 165). Their most common organisms are exact on all 400 draws. A fourth, `s52` seed 91 on seed 106, passes the rule of stage 13 at 150,000 but not the rule: none of its 169 computing organisms at 100,000 is exact, and 147 of 182 are at 150,000.

Nine populations stand at the 91 % level (`m49` seed 94 on seeds 101, 102, 104 and 105, `s52` seed 91 on seeds 101, 103, 104 and 105, plus `s52` seed 91 on seed 106 until after update 100,000). The most common organisms of seven of them are exact on 89.5 % of 400 draws (93.0 % for one). The model of stage 15, an adder of four carry rounds applied twice, can take the three reads in three orders; in one order it fails on exactly the fields on which each of the seven fails, in all 1,600 fields of the 400 draws, and gives 91.1 % exact on 200,000 random draws of the 6-bit fields. This is stage 15's level, one carry round short. So `m49` seed 94, exact in the three-input world, came back one round short in four of the five populations that re-adapted, and `p44` seed 95, one round short in the old world, came back exact in both of its two. In the eight populations still at this level at 150,000 updates, the share never passed 88.8 %.

Planned comparison: populations that hold the sum, `e` 1 of 12 against `h` 2 of 18, Fisher exact two-sided p = 1.0. The prediction is not met: 3 of the 30, not 5 to 20, and none of `s52` seed 91 by the rule. The populations of `p44` seeds 91 and 94 and `p42` seed 93 died or lived without the sum, as predicted. Secondary: at the 91 % level or better, `e` 5 of 12 against `h` 7 of 18 (p = 1.0). The share reached 80 % in 12 populations, at updates 500 to 52,400; at the 91 % level the share stands near 84 %, so this measure dates the rewiring of the reads and does not separate exact from one round short. Not planned, with the measure chosen after the data: the two ancestors whose neighbourhoods were richest before the runs (490 and 177 single mutants) took the sum up in 10 of 12 populations, and the other three viable ancestors in 3 of 18 (Fisher p = 0.00054).

**Reading.** The adders rewired their reads quickly or not at all. Where the sum was taken up, it happened by update 2,500, while the world filled; elsewhere the population filled the world without the sum and in the rest of the 150,000 updates it never came, although a q of 0.25 on it multiplies merit by `2^16`. Which ancestor it was mattered more than whether it had been exact in the old world. Most populations that rewired came back one carry round short, including four of the five from an ancestor that had been exact. Two questions are open: why the sum was not taken up once the world was full, and how an exact ancestor came to lose a round while it rewired. Limits: eight ancestors, six seeds each; three holding populations, so the planned comparison says little either way.

Verdict: negative on the planned comparison; the prediction failed (3 of 30 hold the sum, against 5 to 20 predicted). Accepted by the maintainer on 2026-11-27. Tools in `33-compose4` (`sample_rule.py`, `sample_rule.tsv`, `popconfirm.py`, `popconfirm.tsv`, `errmodel.py`).

### Stage 17: does composition repeat up a ladder of sums?

Design written on 2026-11-27, before any planned run. Lab folder `34-ladder`.

**Why this stage.** Stages 14 and 15 found that copying extends an adder: by one carry round (stage 14), and by applying it to its own sum and the next input (stage 15, one round short). Stage 16 produced organisms that sum the three newest of eight reads exactly. This stage asks whether the step repeats: whether populations go on to four, five and more operands, and whether copying is what takes them there.

**Question.** On a ladder that pays each further operand, do populations of exact three-operand adders come to hold the sum of four or more inputs, and more often when a slip mutation copies its stretch (`dup`) than when it inserts random instructions of the same length (`random`)?

**World.** The stage 16 world of each ancestor's family (3-bit operands in 6-bit fields, `INPUT_AND_MASK 0xFF1C71C7`, eight inputs), with the judge replaced by a ladder on the same judged output, the newest output at the divide: rung `n` (`add3` to `add8`, the sum of the `n` newest reads) pays `2^(32 (n - 2) q)` when its quality q is at least 0.75, with unpaid marks of q of at least 0.5 and of exactness on each rung. The quality is `2f - 1` for the fraction f of result bits right, so 0.75 allows 2 wrong bits of 20 on `add3` and `add4` (5-bit results) and 3 of 24 on `add5` to `add8`. The threshold keeps the rungs apart. Every rung that passes multiplies the merit, and at a lower threshold an output between two sums would be paid on both. A model of exact outputs on 100,000 draws finds an exact rung `n` also paid on rung `n + 1` on 0.7 % (`add3`) to 2.2 % of the draws, against 10.9 % for `add3` on `add4` at a threshold of 0.5. To out-earn an exact rung `n`, an output on rung `n + 1` needs a q above `(n - 2) / (n - 1)`, which only the threshold decides for `add4`. The two fills, `dup` and `random`, differ only in `slip_fill_mode`.

**Ancestors.** The most common organism at update 150,000 of each stage 16 population that holds the exact sum (`m49` seed 94 on seed 103, 826 instructions; `p44` seed 95 on seeds 102 and 105, 493 and 438), and of `s52` seed 91 on seed 106, exact at update 150,000 only (595 instructions). All four are exact on `add3` on all 400 draws in stage 16 and reproduce on every draw. Their names here: `m49-103`, `p44-102`, `p44-105`, `s52-106`.

**Measured before the runs.** On 2,000 draws of the ladder the `m49-103` ancestor is paid on `add3` on every draw and on `add4` on 0.8 %, and passes Q50 on `add4` on 11.7 %. In the world such a lucky life is short: with 2^16 times its neighbours' merit it runs its next gestation within the same update, so the luck is worth about one quick generation in a hundred, and `tasks.dat` rarely shows it at the end of an update (in the smoke runs below, 0 to 5 organisms paid on `add4` summed over the 31 printed updates of a run, where the counts at Q50 on `add4` lead one to expect 200 to 340). One-mutation neighbourhoods on `add4` (every substitution and every tandem duplication up to 80 instructions on 20 draws, candidates confirmed on 400): no single mutation from any of the four is exact on `add4`, or exact on 85 % of its Q50 draws. The step is a cliff at this threshold. The neighbours most often paid on `add4` are duplications of 24 to 28 instructions at sites 329 and 330 of `p44-105`; on 400 draws they pass Q50 on `add4` on 58 to 64 % of the draws but are paid on it on only 16 to 21 %, and they have lost `add3` (paid on 0 to 1.5 %). In a population of exact `add3` organisms such a mutant is paid nothing on four lives in five. The first step therefore takes more than one mutation, with intermediates that change nothing or are not paid.

**Runs.** Four ancestors, two fills, six seeds (101 to 106): 48 runs of 300,000 updates, exact mode, the population saved every 50,000 updates.

**Outcome.** The rule by sample of stage 16 on each rung (`ladder_sample.py`): from the saved populations at 250,000 and 300,000 updates, 300 organisms are drawn at random and each genotype is run alone on 100 draws of the ladder. An organism computes when it reproduces on at least half of the draws and passes Q50 on some rung on at least half. Its level is the highest rung on which it passes Q50 on at least half of the draws and is exact on at least 0.99 of those. A population holds rung `n` when at least half of its computing organisms have a level of `n` or above, in both samples; its top rung is the highest it holds. Planned comparison: the populations that hold `add4` or above, `dup` (24) against `random` (24), Fisher exact, two-sided. Secondary: the top rung per population, `dup` against `random` (Mann-Whitney); the count per ancestor; the same with a level at 0.85 in place of 0.99 (one carry round short, as in stages 15 and 16, falls there); the first update at which at least half of the organisms that pass Q50 on `add4` are exact on it, from `tasks.dat`; the genome lengths at the end.

**Prediction**, written before the runs: the first step is rare because of the cliff. `dup` holds `add4` or above in 3 to 12 of 24 populations and `random` in 0 to 3; at most four populations hold `add6` or above. Where a population climbs, the step comes from a copy of the code that reads and adds an input.

**Seen before the runs.** Smoke runs of 3,000 updates on seed 81 (not a planned seed), all eight arms: every population filled the world (3,322 to 3,597 organisms at update 3,000), and 66 to 87 % of the organisms were exact on `add3` on their own draw; none was exact or paid on a higher rung. The rule tried on two of them, saved at 1,500 and 3,000 updates: `dup` `m49-103` holds `add3` (186 of 213 and 165 of 216 computing organisms exact), `random` `p44-105` not yet at update 1,500 (62 of 150, with 132 of 170 at 0.85 or better at 3,000).

**Cost.** A smoke run of 3,000 updates took 24 to 46 s on this Mac. Stage 16's runs of 150,000 updates took about an hour here and 3.5 to 4.4 hours on the rented cores, ten runs on each 8-core server. 48 runs of 300,000 updates on 32 rented cores and this Mac's five: about 8 to 14 hours, about EUR 1.0 to 1.8 (longer if genomes grow as rungs are added). The watcher stops the batch at EUR 3.00 or at 2026-11-28 18:00.

## Candidate future experiments

Ideas from discussions with the maintainer, unordered. Expected pay per genotype (2026-11-18) was taken up by stage 9.

- **Pay for information.** Pay a genotype for how much its outputs tell about the target (mutual information over test runs), so that an organism that outputs the ingredients of the next step is paid before any single output is right.
- **Competition as the source of selection.** Host and parasite on computed keys (a parasite infects a host only by performing a function the host performs; Zaman and others 2014 found hosts reach more complex functions with parasites), the ladder kept as unpaid measurement; then perhaps a prediction contest between neighbours, which needs new instructions.
- **Differentiable programs** as a fallback, kept apart from the evolutionary line: a distribution over instructions at each genome position and fitness as an expectation with a gradient.
- **Wider adders** (ADD3, ADD4; files prepared, to be redesigned so that a rung keeps the fields of the rung below).
- **Loops that cannot stall reproduction** (2026-11-24, after the lineage study found no loop or `call`): a second instruction pointer (`max_cpu_threads 2` with `fork-th`), and merit that does not pay for executed length (`base_merit_method` constant; the default pays for every site executed, so an unrolled round pays for itself). The measured loop costs about 1.7 % of offspring and 1.6 to 2.5 % of fitness in time.
- **A pure, concatenative core for computing** (filed by the maintainer, 2026-11-24). An opt-in stack language beside the heads CPU: a data stack, the sixteen gates as stack operations, `dup`, `swap`, `over`, `drop`, `shl`, `shr`, a quotation and a `times` with a capped count, so that iteration always ends. Every stretch of code is then a function from stack to stack, so a tandem duplication is always a composition; named blocks make reuse what `call` was meant to be; nothing computed is overwritten. Precedents: Fontana and Buss's algorithmic chemistry, Spector's Push and Pushpop, Cartesian GP. Comparison: the stage 13 setup from scratch, register gates against the stack core. About a day to build.
