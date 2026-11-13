# Deme migration matrix (MIGRATION_FILE)

When offspring or parasites migrate between [demes](../concepts/demes.md), you can give every ordered pair of demes its own weight. The weights live in a small text file, the *connection-weight matrix*, which is read when the run starts and can be replaced or edited by events. This page describes the file, the settings that switch it on, the actions that change and report it, and the mistakes it rejects.

## Switching it on

| Setting | Stock | What to set |
|---|---|---|
| `NUM_DEMES` | 0 | The number of demes. The matrix must have exactly this many rows and columns. |
| `DEMES_MIGRATION_METHOD` | 0 | 4, to draw the target deme from the matrix. The other methods (0 to 3) ignore the file. |
| `DEMES_MIGRATION_RATE` | 0.0 | The probability, between 0 and 1, that a newborn migrates at all. With 0.0 there is no migration, and the offspring counters (below) are not allocated. |
| `DEMES_PARASITE_MIGRATION_RATE` | 0.0 | The probability that a parasite migrates. It only works with method 4, and a value above 0 at the start of the run allocates the parasite counters. |
| `MIGRATION_FILE` | `-` | The name of the matrix file, looked for in the working directory. It is read only when `DEMES_MIGRATION_METHOD` is 4. |

`MIGRATION_RATE` (stock 0.0) is a different mechanism: with that probability an offspring lands in a uniformly chosen other deme, whatever the method and the matrix. The action `SetMigrationRate` changes `MIGRATION_RATE`, not `DEMES_MIGRATION_RATE`.

## The file format

The file is a table of numbers separated by commas. Everything after a `#` on a line is a comment, and blank lines are skipped. Spaces around a number are ignored. Row *i* (counting from 0, not counting comment and blank lines) holds the weights *from* deme *i*, and column *j* is the target deme *j*:

```text
# Migration weights, one row per source deme, one column per target deme.
# From deme 0: mostly stays home (5.0), rarely goes to 1 (0.1), never to 2, often to 3 (4.0).
5.0,   0.10,  0.0,  4.0
3.0,   0.20,  0.8,  2.0
7.0,   0.30,  0.0,  1.0
2.0,   1.30,  9.0,  0.0
```

The weights are relative. When an offspring of deme *i* migrates, the program sums row *i*, draws one random number between 0 and that sum, and walks along the row subtracting weights until the number is used up (a roulette wheel). The weights of a row therefore need not add up to 1. For deme 0 above the sum is 9.1, so the chances are 5.0 / 9.1 = 55 percent for deme 0 itself, 1 percent for deme 1, none for deme 2 and 44 percent for deme 3. The entry on the diagonal is the chance of being placed in a random cell of the offspring's own deme, and it counts as a migration event for the counters.

Three rules apply, and a file that breaks one stops the run at start-up with the error message in the last column:

| Rule | Error message |
|---|---|
| No weight is negative. | `Cannot have a negative connection in connection matrix` |
| No row sums to 0. To switch migration off, use the identity matrix (1 on the diagonal, 0 elsewhere). | `Cannot have a row sum of 0.0 in connection matrix` |
| The matrix is square with `NUM_DEMES` rows. | `The number of demes in the migration matrix (3) did not match the NUM_DEMES (4) parameter in avida.cfg.` for the rows, and `The number of columns in row 2 did not match total number of demes` for a row of the wrong length. |

These four messages were produced by running the program with a matrix of each kind. A suite example is `avida-core/tests/migrationmatrix_load_reset_write`, which uses a 10 by 10 matrix of ones (every deme equally likely, itself included) in `migration.mat` and a second matrix in `migration2.mat`.

## Actions

| Action | Arguments | What it does |
|---|---|---|
| `SetMigrationMatrix` | file name | Reads a new matrix and replaces the current one. See the warning below. |
| `AlterMigrationConnection` | from deme, to deme, amount | Adds `amount` (it may be negative) to one weight. A repeated event, such as `u 10000:100:20000 AlterMigrationConnection 0 1 -3.0`, changes the weight step by step. |
| `PrintMigrationMatrix` | optional file name | Writes the current matrix as comma-separated rows. The default name is `migration_matrix.<update>.dat`. |
| `DumpOffspringMigrationCounts` | optional file name | Writes a `NUM_DEMES` by `NUM_DEMES` table of how many offspring went from the deme of the row to the deme of the column since the last dump, then sets all counts to zero. The default name is `counts_offspring_migration.<update>.dat`. |
| `DumpParasiteMigrationCounts` | optional file name | The same for parasites, with the default name `counts_parasite_migration.<update>.dat`. |
| `SetMigrationRate` | rate | Sets `MIGRATION_RATE`, the uniform mechanism, not the matrix. |

An example events file for a 4-deme world of 10 by 10 cells (start it with `-set WORLD_X 10 -set WORLD_Y 40 -set NUM_DEMES 4 -set DEMES_MIGRATION_METHOD 4 -set DEMES_MIGRATION_RATE 0.1 -set MIGRATION_FILE weights.mat`, with the matrix above saved as `weights.mat`):

```text
u begin InjectDemes default-heads.org
u 0 PrintMigrationMatrix matrix_start.dat
u 100:100:end DumpOffspringMigrationCounts
u 300 Exit
```

`matrix_start.dat` echoes the matrix with the numbers in their shortest form:

```text
5,0.1,0,4
3,0.2,0.8,2
7,0.3,0,1
2,1.3,9,0
```

and the first dump, `counts_offspring_migration.100.dat`, of a run with seed 5 was:

```text
6,0,0,4
2,0,3,3
4,0,0,2
0,1,6,0
```

Row 0 shows 10 offspring: 6 stayed in deme 0, none went to deme 1 or deme 2 and 4 went to deme 3, close to the expected 55, 1, 0 and 44 percent. Counts are random, so a different seed gives different numbers.

## Things that differ from the old wiki

- A negative weight or a zero row sum produced by `AlterMigrationConnection` does not stop the run in the port. The event adds the amount, prints no message, and the matrix then holds the value (a run that lowered a weight of 0 twice by 0.5 printed -1 for it in `PrintMigrationMatrix`). The old page promised an error and an exit. The C++ code only asserted this, and the reference binary was built without assertions.
- `SetMigrationMatrix` with a file that breaks a rule is silent too: the matrix is emptied first, and rows read before the faulty one stay. The run goes on, and stops with `error: migration matrix row out of range` when the next migration draws from a row that is missing. Check the file with a trial run before you use it in an event.
- The counters are allocated at start-up, and only when `DEMES_MIGRATION_RATE` (for offspring) or `DEMES_PARASITE_MIGRATION_RATE` (for parasites) is above 0 then. A dump without them stops the run with `error: DumpMigrationCounts: no migration matrix`. Starting with the rate at 0 and raising it later does not allocate them.

## See also

- [Demes](../concepts/demes.md), for how demes are set up and how they replicate.
- The [settings reference](settings.md#deme_group) for the full text of the deme settings.
- [Actions reference](actions.md).
