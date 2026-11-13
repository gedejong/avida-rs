# Mutations

Evolution needs variation, and in Avida the variation comes from mutations: random changes to a genome as it is copied or as the organism is born or lives. All mutation rates are settings in the MUTATION group of `avida.cfg`, and all of them can be changed during a run with events. The full list is in the [settings reference](../reference/settings.md#mutation_group).

## Kinds of change

| Kind | What happens |
|---|---|
| Point mutation (substitution) | One instruction is replaced by a random instruction of the instruction set. |
| Insertion | A random instruction is added at a random position. |
| Deletion | An instruction is removed. |
| Slip | A stretch of the genome is deleted or duplicated, so the genome length can change by many instructions at once. `SLIP_FILL_MODE` chooses what fills a slipped insertion (a duplicate of the stretch, `nop-X`, random instructions, scrambled instructions or `nop-C`). |
| Uniform | A single draw that picks a substitution, an insertion or a deletion at random. |

Which instruction a substitution picks depends on the `redundancy` of each instruction in the instruction set: an instruction with redundancy 2 is twice as likely to be chosen as one with redundancy 1, and an instruction with redundancy 0 can be injected but never mutated into. See [instruction sets](../reference/instruction-sets.md).

## When mutations happen

| When | Settings | Unit |
|---|---|---|
| While copying (`h-copy`) | `COPY_MUT_PROB`, `COPY_INS_PROB`, `COPY_DEL_PROB`, `COPY_UNIFORM_PROB`, `COPY_SLIP_PROB` | Per instruction copied |
| At birth, per site | `DIV_MUT_PROB`, `DIV_INS_PROB`, `DIV_DEL_PROB`, `DIV_UNIFORM_PROB`, `DIV_SLIP_PROB` | Per site of the offspring, applied on divide |
| At birth, per divide | `DIVIDE_MUT_PROB`, `DIVIDE_INS_PROB`, `DIVIDE_DEL_PROB`, `DIVIDE_UNIFORM_PROB`, `DIVIDE_SLIP_PROB` | At most one per divide |
| At birth, Poisson | `DIVIDE_POISSON_MUT_MEAN`, `DIVIDE_POISSON_INS_MEAN`, `DIVIDE_POISSON_DEL_MEAN`, `DIVIDE_POISSON_SLIP_MEAN` | A Poisson-distributed number per divide |
| During life | `POINT_MUT_PROB` | Per location per update |
| On injection | `INJECT_MUT_PROB`, `INJECT_INS_PROB`, `INJECT_DEL_PROB` | Per site of an injected organism |

The stock values are `COPY_MUT_PROB 0.0075` (a substitution for 0.75% of copied instructions), `DIVIDE_INS_PROB 0.05` and `DIVIDE_DEL_PROB 0.05` (at most one insertion and one deletion per divide, each with probability 5%); all the others are 0.

For a 100-instruction genome, the stock copy rate gives on average 0.75 substitutions per generation. Since most substitutions in a well-adapted genome are harmful, the mutation rate sets how fast evolution explores and how much of the population's effort goes into repairing damage. At high rates the population cannot keep its genome intact at all, which is called the *error threshold*. The [mutation-rate tutorial](../guides/tutorial-mutation-rate.md) finds it.

## Changing rates during a run

The events file can change rates at any time:

```text
u 5000 SetMutProb COPY_MUT 0.02
```

`SetMutProb` takes the kind of mutation (`COPY_MUT` is the default), the new probability, and optionally a range of cells (`start_cell`, `end_cell`). The kind is one of `POINT`, `COPY_MUT`, `COPY_INS`, `COPY_DEL`, `COPY_UNIFORM`, `COPY_SLIP`, `DIV_MUT`, `DIV_INS`, `DIV_DEL`, `DIV_UNIFORM`, `DIV_SLIP`, `DIVIDE_MUT`, `DIVIDE_INS`, `DIVIDE_DEL`, `DIVIDE_UNIFORM`, `DIVIDE_SLIP`, `PARENT_MUT`, `PARENT_INS`, `PARENT_DEL`, `DEATH`, `POINT_MUT`, `POINT_INS`, `POINT_DEL`, `INJECT_MUT`, `INJECT_INS` or `INJECT_DEL`. `ModMutProb` takes the same arguments and modifies the rate instead of replacing it. `ZeroMuts` switches all mutations off. See the [actions reference](../reference/actions.md).

## Mutation data

- `PrintDivideMutData` writes per-divide mutation counts, and `PrintMutationRateData` the rates.
- In [analyze mode](../reference/analyze-mode.md), landscape commands (`PrecalcLandscape`, `ANALYZE_KNOCKOUTS`, `MAP_MUTATIONS`, `MUTAGENIZE` and others) test every single-point mutation of a genome in the test CPU and report the fractions that are lethal, harmful, neutral and beneficial (`frac_dead`, `frac_neg`, `frac_neut`, `frac_pos`).

## Mutation reversion

The REVERSION group (`REVERT_FATAL`, `REVERT_DETRIMENTAL`, `REVERT_NEUTRAL`, `REVERT_BENEFICIAL` and the related settings) makes the program test each newborn in the test CPU and reverse its mutations with the given probability when they are lethal, harmful, neutral or beneficial. The settings' own description warns that most of these slow Avida down a lot and should be 0.0 normally.

## Next

[Demes](demes.md).
