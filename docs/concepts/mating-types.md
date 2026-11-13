# Mating types and mate choice

By default, sexual reproduction in Avida lets any organism recombine with any other. With *mating types* the population has separate sexes: only a female and a male can mate, each organism decides for itself which sex it becomes, and a female (or a male) can prefer mates with certain traits. This page is the detailed guide. The [reproduction page](reproduction.md#sexual-reproduction) covers sexual reproduction without sexes, which you should read first.

The worked example of the program is the consistency test `avida-core/tests/mating_types`. Everything below was checked against the Rust code. The instructions, the costs and the output files were also exercised by running that configuration for 200 updates; `LEKKING`, `FORCED_MATE_PREFERENCE`, `MATE_IN_GROUPS` and the noise settings were read from the code and not run.

## What you need

1. `MATING_TYPES 1` in `avida.cfg`.
2. An instruction set that contains the mating instructions (the sexes are set by instructions, so the stock sets, which have none, are not enough).
3. At least one organism of each sex, or organisms that can become one.

The consistency test shows an instruction set for it (`instset-heads-mating-types.cfg`): the heads instructions with `h-divide` replaced by `div-sex-mating-type`, plus these lines:

```text
INST set-mating-type-male
INST set-mating-type-female
INST div-sex-mating-type:female_cost=5:choosy_female_cost=5
INST if-mating-type-juvenile
INST if-mating-type-male
INST if-mating-type-female
INST set-mate-preference-random
INST set-mate-preference-highest-display-a
INST set-mate-preference-highest-merit
INST increment-mating-display-a
```

## The sexes and the instructions

Every organism starts as a *juvenile* (mating type -1). The other types are female (0) and male (1).

| Instruction | Effect |
|---|---|
| `set-mating-type-male` | Becomes male. Fails if the organism is already female. |
| `set-mating-type-female` | Becomes female. Fails if the organism is already male. |
| `set-mating-type-juvenile` | Becomes a juvenile again, from any type. This is the only way to change sex: do it, then set the other one. |
| `div-sex-mating-type` | The divide instruction for this system. It fails for a juvenile, and otherwise behaves like `divide-sex`. |
| `if-mating-type-male`, `if-mating-type-female`, `if-mating-type-juvenile` | Execute the next instruction only when the organism has that type, so a genome can behave differently by sex. Without these an organism cannot read its own sex. |

Once set, a sex cannot be switched by the other `set-mating-type` instruction: the second call fails. Only `set-mating-type-juvenile` resets it. The old wiki said the same, but did not say that a juvenile can never divide with `div-sex-mating-type`.

To seed a population, make one organism file per sex. Take your ancestor and put `set-mating-type-male` or `set-mating-type-female` near the start, and use `div-sex-mating-type` as its divide. Then inject both in the events file:

```text
u begin InjectRange default-heads-sex-male.org 0 20
u begin InjectRange default-heads-sex-female.org 40 50
```

The test injects several kinds (a male with a display, females with different preferences) into ranges of cells.

## The birth chamber

A divide with `div-sex-mating-type` does not place an offspring at once. The offspring genome goes to the *birth chamber*, a waiting area that holds the genomes of recent divides. The rules, as the code applies them:

1. A juvenile parent never gets an offspring placed or stored.
2. A female parent looks in the chamber for a waiting offspring of a male parent, a male parent for one of a female parent. If it finds a candidate, the two genomes recombine and the offspring is placed into the population by the usual birth method.
3. If there is no suitable partner, the new offspring waits in the chamber.
4. An entry is dropped when it has waited longer than `MAX_BIRTH_WAIT_TIME` updates. The stock value, -1, means it waits for ever, so set something like 3 updates (the value the old wiki found to work and the test uses).
5. The chamber is limited to `MAX_GLOBAL_BIRTH_CHAMBER_SIZE` entries (3600 by default). When it is full, the oldest entry is overwritten.

The chamber has no spatial structure: any female can meet any male, wherever they are. Where the recombined offspring *lands* is still decided by `BIRTH_METHOD`. If you want the whole population to be well mixed, use `BIRTH_METHOD 4`, as the test does.

Two exceptions. If `NUM_DEMES` is above 1, `MATING_TYPES` is ignored and every deme runs its own chamber of the ordinary kind, so mating types do not work with demes. And with `MATE_IN_GROUPS 1`, mates must belong to the same group (the organism's opinion).

`LEKKING 1` makes the female the chooser: the offspring of a male always goes into the chamber to wait, and the offspring of a female never waits, so a female that divides must find an existing male offspring or lose her offspring.

## Mate choice

An organism that divides and finds several candidates of the other sex picks one with its *mate preference*, which it sets with an instruction:

| Instruction | Preference value | The organism picks the candidate with |
|---|---|---|
| `set-mate-preference-random` | 0 | No preference: a random one. This is the default. |
| `set-mate-preference-highest-display-a` | 1 | The highest display A. |
| `set-mate-preference-highest-display-b` | 2 | The highest display B. |
| `set-mate-preference-highest-merit` | 3 | The highest merit. |
| `set-mate-preference-lowest-display-a`, `-b` and `-merit` | 4, 5, 6 | The lowest of the same traits. |

The old wiki listed only the first four. The rule applies to whichever sex is the one that divides when a candidate is already waiting, so with `LEKKING 0` a male that divides after a female has also chooses. `FORCED_MATE_PREFERENCE` (stock -1, off) overrides everything: 0 makes all organisms choose randomly, and 1, 2 and 3 make them all prefer the highest display A, display B or merit.

A *display* is a number that an organism raises or sets during its life. It is 0 at birth, and it is reset to 0 again after each divide; the value at the moment of the divide is the one that potential partners see.

| Instruction | Effect |
|---|---|
| `increment-mating-display-a`, `increment-mating-display-b` | Adds 1 to the display. |
| `set-mating-display-a`, `set-mating-display-b` | Copies the value of `?BX?` into the display. The register can be chosen by a following nop. |

A large display therefore needs either many executions of an increment instruction before the divide, or a large number computed into a register first.

### Noisy assessment

`NOISY_MATE_ASSESSMENT 1` makes organisms misjudge the traits of the candidates. Each assessed value gets a normally distributed error with a mean of 0 and a *variance* equal to the value times `MATE_ASSESSMENT_CV`. The code passes that product where a variance is expected, so the standard deviation is its square root: for a display of 100 and a coefficient of 0.1 it is about 3.2, not 10 as the name "coefficient of variation" and the old wiki suggest. The stock coefficient in the port is 0.1 (the test sets 0.25).

## Costs

Instruction costs can depend on sex. In the instruction-set file, add options to the `INST` line:

```text
INST div-sex-mating-type:female_cost=5:choosy_female_cost=5
```

`female_cost` is added to the instruction's cost for a female. `choosy_female_cost` is added on top for a female whose preference is not random. The costs are CPU cycles: the instruction takes the sum of its normal `cost` and the female terms as its total, and a total of 0 or 1 is the single normal cycle. With the options above and no `cost`, a male divides in 1 cycle, a female with a random preference in 5 and a choosy female in 10. (The old wiki's example, 50 and 50, comes to 101 cycles with a base cost of 1.) Males then divide faster, their offspring pile up in the chamber, and females find partners available, so they can afford to choose.

## Data

| Action | File and content |
|---|---|
| `PrintMatingTypeHistogram` | `mating_type_histogram.dat`: the number of juveniles, females and males in the population (columns for types -1, 0 and 1). |
| `PrintMatingDisplayData` | `mating_display_data.dat`: the average display A and display B of juveniles, females and males. |
| `PrintFemaleMatePreferenceData` | `female_mate_preference_data.dat`: the number of females for each preference (random, highest display A, highest display B, highest merit). The lowest-value preferences are not counted. |
| `PrintBirthChamberMatingTypeHistogram` | `birth_chamber_mating_type_histogram.dat`: the same three counts for the offspring waiting in the chamber. |
| `PrintBirthChamber` | A file `birth_chamber/bc-<update>.dat` that lists every waiting entry: genome, timestamp, merit, mating type, preference, both displays and group. |
| `PrintSuccessfulMates` | A file `mates/mates-<update>.dat` that lists the successful pairs of the update: the chosen mate and the chooser, with the same columns. |

With `PrintBirthChamber` and `PrintSuccessfulMates` at the same update, the waiting candidates and the chosen ones together let you estimate sexual selection differentials. A 200-update run of the test with the four histogram actions every 100 updates gave a population of about 1780 females and 1760 males at update 100 (with 58 juveniles),; the average display A was 2.6 for females and 7.2 for males.

In analyze mode, `DETAIL` accepts the columns `mating_type`, `mate_preference`, `mating_display_a` and `mating_display_b`. They tell you which sex a genome develops into, which preference it ends up with and how large its displays are, which helps when a genome contains several `set-mating-type` instructions and it is unclear which runs first. Loading the female and the male ancestors of the test and running `RECALCULATE` gave, for the female, type 0 and preference 1 (highest display A) and, for the male, type 1 with display A of 13.

## Memory

Recombination between sexes creates many new genotypes. If you do not need the genealogy, `DISABLE_GENOTYPE_CLASSIFICATION 1` stops the program from keeping every genotype that ever lived, and the test uses it. The price is that the program no longer records the history of genotypes, so analyses that need ancestry, such as the [line of descent](../guides/tutorial-line-of-descent.md), are not available for such a run.

## See also

- [Reproduction, birth and sex](reproduction.md).
- The [settings reference](../reference/settings.md#mating_types_group) for the mating-type settings and the [actions reference](../reference/actions.md) for the print actions.
- [Sex versus asex](../guides/tutorial-sex-vs-asex.md), a tutorial on sexual reproduction without sexes.
