# Tutorial: limited resources

**Question.** When every task draws on its own limited resource, doing what everyone else does pays less, so specialists can coexist (Cooper and Ofria, 2002). Does a limited-resource environment keep more kinds of organism alive than the stock, unlimited one?

**Arms.** `unlimited` (the stock environment) and `limited` (each task has its own resource).

## Steps

1. Start from the stock folder and the [replicate recipe](how-to-replicates.md).
2. Create `environment-limited.yaml`:

   ```yaml
   environment:
     resources:
       - {name: resNOT, inflow: 100, outflow: 0.01}
       - {name: resNAND, inflow: 100, outflow: 0.01}
       - {name: resAND, inflow: 100, outflow: 0.01}
       - {name: resORN, inflow: 100, outflow: 0.01}
       - {name: resOR, inflow: 100, outflow: 0.01}
       - {name: resANDN, inflow: 100, outflow: 0.01}
       - {name: resNOR, inflow: 100, outflow: 0.01}
       - {name: resXOR, inflow: 100, outflow: 0.01}
       - {name: resEQU, inflow: 100, outflow: 0.01}
     reactions:
       - {name: NOT, task: not, process: {resource: resNOT, value: 1.0, frac: 0.0025, max: 25}}
       - {name: NAND, task: nand, process: {resource: resNAND, value: 1.0, frac: 0.0025, max: 25}}
       - {name: AND, task: and, process: {resource: resAND, value: 2.0, frac: 0.0025, max: 25}}
       - {name: ORN, task: orn, process: {resource: resORN, value: 2.0, frac: 0.0025, max: 25}}
       - {name: OR, task: or, process: {resource: resOR, value: 4.0, frac: 0.0025, max: 25}}
       - {name: ANDN, task: andn, process: {resource: resANDN, value: 4.0, frac: 0.0025, max: 25}}
       - {name: NOR, task: nor, process: {resource: resNOR, value: 8.0, frac: 0.0025, max: 25}}
       - {name: XOR, task: xor, process: {resource: resXOR, value: 8.0, frac: 0.0025, max: 25}}
       - {name: EQU, task: equ, process: {resource: resEQU, value: 16.0, frac: 0.0025, max: 25}}
   ```

   This is a chemostat for each task: each resource has an inflow of 100 and an outflow rate of 0.01, so its equilibrium without consumers is 10,000 units. With a population of 3,600 and a neighbourhood of 9, an organism's fair share is about 9/3600, which is the fraction 0.0025 that one task completion may take. The reward is the default type `add`, so it adds to the bonus by `value` times the amount consumed. See the [environment reference](../reference/environment-file.md#a-small-environment-with-a-limited-resource).

3. Make sure the events in `avida.yaml` record the phenotypes and the resources. The stock events already hold the second of these entries; add the first:

   ```yaml
     - {at: 0, every: 100, until: end, action: PrintPhenotypeData}
     - {at: 0, every: 100, until: end, action: PrintResourceData}
   ```

4. Run 4 seeds of each arm for 30,000 updates (about 15 minutes):

   ```sh
   . ./lab.sh
   for s in 1 2 3 4; do
     run_arm unlimited $s 30000
     run_arm limited $s 30000 -set environment environment-limited.yaml
   done
   ```

5. Compare `phenotype_count.dat` (the default file name of `PrintPhenotypeData`; its legend names the Shannon diversity of task phenotypes) between the arms, and look at `resource.dat` for the limited arm: with specialists, the resources are drawn down unevenly.

## What to expect

In the pilot (2 seeds, 20,000 updates), the Shannon diversity of task phenotypes was 3.96 with limited resources against 2.86 without. Task counts level off below the population size in the limited arm, because a task that everyone does exhausts its resource.
