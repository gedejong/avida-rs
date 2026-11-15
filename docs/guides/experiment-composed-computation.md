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

To be written when the main runs have finished.
