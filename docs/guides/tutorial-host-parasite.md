# Tutorial: host and parasite

**Question.** If a parasite can infect only a host that performs the same task, do hosts escape by changing tasks, and do parasites follow? (Zaman and others, PLoS Biology, 2014.)

This tutorial uses TransSMT hardware. Its files are already in the repository, in the consistency test `avida-core/tests/parasites_limited_resources/config`: the experiment file `avida.yaml` (with the stock set `transsmt` and an environment of its own), an evolved host that does NOT (`evolved-not.org`) and a parasite (`parasite-smt.org`).

**Arms.** `parasites` and `no-parasites`.

## Steps

1. Copy the test configuration into a working folder and write `events-parasites.yaml`:

   ```yaml
   events:
     - {at: begin, action: Inject, args: evolved-not.org}
     - {at: 2000, action: InjectParasite, args: parasite-smt.org ABB 0 400}
     - {at: 0, every: 100, until: end, action: PrintAverageData}
     - {at: 0, every: 100, until: end, action: PrintCountData}
     - {at: 0, every: 100, until: end, action: PrintTasksData}
     - {at: 0, every: 100, until: end, action: PrintParasiteTasksData}
     - {at: 0, every: 100, until: end, action: PrintHostPhenotypeData}
     - {at: 0, every: 100, until: end, action: PrintParasitePhenotypeData}
     - {at: 20000, action: Exit}
   ```

   The `InjectParasite` entry infects cells 0 to 400 at update 2000, with the memory-space label `ABB`. For the control arm, make a copy `events-no-parasites.yaml` without that entry.

2. Run four seeds of each arm for 30,000 updates with the [replicate recipe](how-to-replicates.md), naming each arm's events in `EVENTS`: `EVENTS=events-parasites.yaml run_arm parasites $s 30000` and `EVENTS=events-no-parasites.yaml run_arm no-parasites $s 30000`.

3. Compare `host_phenotype_count.dat` and `parasite_phenotype_count.dat`, and `parasite_tasks.dat`.

## What the configuration does

The settings that matter, all in `avida.yaml`: `reproduction.birth_method: population_random` (well-mixed), `time.max_cpu_threads: 2` (a host carries at most one parasite), `parasites.inject_method: reset_thread`, `parasites.infection_mechanism` at its default 1 (the parasite must match a task of the host), `parasites.virulence: 0.85` and `divide.require_single_reaction: 1`. The environment gives every task the same limited resource `resECHO` and forbids earning more than one task, so each host performs exactly one. See [parasites](../concepts/parasites.md).

## What to expect

In the pilot (2 seeds, 12,000 updates), about half of the hosts moved from NOT to NAND within 1,000 updates of the parasites' arrival, and the parasite count on NOT collapsed. Host phenotype diversity ended at 1.78 with parasites against 0.19 without. In both pilot runs the parasites had nearly died out by the end; longer runs and more seeds will show how often they persist.
