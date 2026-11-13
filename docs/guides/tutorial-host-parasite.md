# Tutorial: host and parasite

**Question.** If a parasite can infect only a host that performs the same task, do hosts escape by changing tasks, and do parasites follow? (Zaman and others, PLoS Biology, 2014.)

This tutorial uses TransSMT hardware. Its files are already in the repository, in the consistency test `avida-core/tests/parasites_limited_resources/config`: `avida.cfg`, `environment.cfg`, `instset-transsmt.cfg`, an evolved host that does NOT (`evolved-not.org`) and a parasite (`parasite-smt.org`).

**Arms.** `parasites` and `no-parasites`.

## Steps

1. Copy the test configuration into a working folder and replace its `events.cfg` with:

   ```text
   u begin Inject evolved-not.org
   u 2000 InjectParasite parasite-smt.org ABB 0 400
   u 0:100:end PrintAverageData
   u 0:100:end PrintCountData
   u 0:100:end PrintTasksData
   u 0:100:end PrintParasiteTasksData
   u 0:100:end PrintHostPhenotypeData
   u 0:100:end PrintParasitePhenotypeData
   u 20000 Exit
   ```

   The `InjectParasite` line infects cells 0 to 400 at update 2000, with the memory-space label `ABB`. For the control arm, make a copy `events-no-parasites.cfg` without that line.

2. Run four seeds of each arm for 30,000 updates with the [replicate recipe](how-to-replicates.md). Select the control's events file by editing the recipe's `sed` source or by keeping the control in a separate folder.

3. Compare `host_phenotype_count.dat` and `parasite_phenotype_count.dat`, and `parasite_tasks.dat`.

## What the configuration does

The settings that matter, all in `avida.cfg`: `BIRTH_METHOD 4` (well-mixed), `MAX_CPU_THREADS 2` (a host carries at most one parasite), `INJECT_METHOD 1`, `INJECT_IS_TASK_SPECIFIC 1` (the parasite must match a task of the host), `PARASITE_VIRULENCE 0.85` and `REQUIRE_SINGLE_REACTION 1`. The environment gives every task the same limited resource `resECHO` and forbids earning more than one task, so each host performs exactly one. See [parasites](../concepts/parasites.md).

## What to expect

In the pilot (2 seeds, 12,000 updates), about half of the hosts moved from NOT to NAND within 1,000 updates of the parasites' arrival, and the parasite count on NOT collapsed. Host phenotype diversity ended at 1.78 with parasites against 0.19 without. In both pilot runs the parasites had nearly died out by the end; longer runs and more seeds will show how often they persist.
