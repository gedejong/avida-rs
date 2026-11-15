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
| `sex` | `divide-sex` in place of `h-divide`: offspring recombine with another offspring |
| `pop-30` | a 30 x 30 world, resource inflow scaled to the area |
| `pop-120` | a 120 x 120 world, resource inflow scaled to the area |
| `seasons` | NOT, NAND, XOR and AND pay nothing in every other period of 2,000 updates |
| `min-genome` | `MIN_GENOME_SIZE 50`: a divide that would leave fewer than 50 instructions fails |

The first four are the factors planned from the start. The fifth was added after stage 3, because of the genome shrinking seen there; the value 50 is half the ancestor's length and was chosen without a trial run. Seeds 1 to 10, 100,000 updates, exact mode, and the outcomes and tests of stage 3. Each arm is compared with the base only, so five comparisons are made per measure; p values are reported uncorrected and with a Holm correction over the five.

Results: to be written when the runs have finished.

