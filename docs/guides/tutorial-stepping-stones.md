# Tutorial: stepping stones

**Question.** Do complex features evolve by building on simpler ones? Lenski, Ofria, Pennock and Adami (Nature, 2003) showed with Avida that the hardest of the nine logic tasks, EQU, evolves when simpler tasks are rewarded as stepping stones, and hardly at all when EQU is the only reward.

**Arms.**

| Arm | Environment |
|---|---|
| `all-rewarded` | The stock environment: nine tasks, rewards rising with difficulty. |
| `equ-only` | Only EQU is rewarded. |

## Steps

1. Make a working folder from the stock configuration and load the [replicate recipe](how-to-replicates.md):

   ```sh
   cp -r avida-core/support/config stepping && cd stepping
   ```

2. Create the second environment, `environment-equ-only.cfg`, containing the one line:

   ```text
   REACTION  EQU  equ   process:value=5.0:type=pow  requisite:max_count=1
   ```

   You should see a file of one line. It is the stock EQU reaction, without the other eight.

3. Run both arms for several seeds. The original study ran 50,000 updates; the maintainer's default is 8 seeds per arm, about 45 minutes on a performance-core machine:

   ```sh
   . ./lab.sh
   for s in 1 2 3 4 5 6 7 8; do
     run_arm all-rewarded $s 50000
     run_arm equ-only $s 50000 -set ENVIRONMENT_FILE environment-equ-only.cfg
   done
   ```

4. Read the last row of `tasks.dat` for each run. In the `all-rewarded` arm the last column is `Equals`. In the `equ-only` arm there is only one task column, also `Equals`. Count the runs in which it is above zero.

## What to expect

In the maintainer's pilot (4 seeds, 40,000 updates), EQU appeared in 3 of 4 `all-rewarded` populations and in 0 of 4 `equ-only` ones. The `equ-only` populations did no task at all and ended with a fitness of 0.42 against a median of 1.2 million in the other arm. Four seeds are an illustration, not evidence; with eight, you can start to compare.

The explanation is in [the environment page](../concepts/environment-and-tasks.md): with all nine tasks rewarded, selection has something to act on at every step, while EQU alone gives no advantage until one lucky mutation builds the whole function.

## Going further

- Trace one successful run back to the ancestor with the [line-of-descent tutorial](tutorial-line-of-descent.md). It shows how EQU arose.
- Reward only some tasks (delete reaction lines) and see which sets support EQU.
