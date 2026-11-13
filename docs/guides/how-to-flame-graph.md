# Draw a flame graph of genetic diversity

A *flame graph* in Avida is a picture of how far a population has diverged from its ancestor over time. Each column is one saved population, each row is a *phylogenetic depth* (the number of divides in a lineage since the ancestor), and the colour says how many organisms have that depth. It is related to a phylogeny, but it is not one and must not be used as one: it counts organisms by depth and keeps no record of who descends from whom.

This page replaces the old wiki page `Creating-a-flame-graph`, whose two tools (`flamegraph.py`, a Python 2 script, and `hist_map`, a C++ program with a Matlab step) were removed with the C++ tree. The recipe below does the same job with a short Python 3 script that you copy from this page. The script was run on a 1000-update run for this page; the old tools were not run.

If you were looking for a profile of where the *program* spends its CPU time, which is another thing that people call a flame graph, see [profiling the program](#profiling-the-program-itself) at the end.

## What you need

- Python 3 with `numpy` and `matplotlib`.
- A run that saved the population at regular intervals.

## Steps

1. Ask for population files in your events file. `SavePopulation` writes a file named `detail-<update>.spop` into the data folder. A time slice of 50 or 100 updates gives a smooth picture; more slices mean more disk space. The stock `events.cfg` saves every 500 updates (the old page said 50,000), which gives a coarse picture for a short run, so add a line of your own:

   ```text
   u begin Inject default-heads.org
   u 50:50:1000 SavePopulation
   u 1000 Exit
   ```

2. Run the experiment as usual. The run used for this page was `avida -s 7 -set WORLD_X 40 -set WORLD_Y 40 -set EVENT_FILE events.cfg` on the stock configuration and took about 6 seconds.

3. Save this script as `flame.py`:

   ```python
   #!/usr/bin/env python3
   """Draw a flame graph: one column per saved population, one row per
   phylogenetic depth, colour = log(1 + number of organisms at that depth).

   usage: flame.py OUTFILE FILE [FILE ...]   (OUTFILE ends in .png, .pdf or .svg)
   """
   import gzip
   import re
   import sys

   import matplotlib
   matplotlib.use("Agg")
   import matplotlib.pyplot as plt
   import numpy as np

   DEPTH_COLUMN = 14   # "Phylogenetic Depth" in a .spop file
   COUNT_COLUMN = 5    # "Number of currently living organisms"


   def update_of(name):
       """The update number in a name such as detail-250.spop or detail-250.spop.gz."""
       return int(re.search(r"(\d+)\.spop", name).group(1))


   def histogram(path):
       opener = gzip.open if path.endswith(".gz") else open
       counts = {}
       with opener(path, "rt") as handle:
           for line in handle:
               if not line.strip() or line.startswith("#"):
                   continue
               fields = line.split()
               depth = int(fields[DEPTH_COLUMN - 1])
               counts[depth] = counts.get(depth, 0) + int(fields[COUNT_COLUMN - 1])
       return counts


   def main():
       if len(sys.argv) < 3:
           sys.exit(__doc__)
       out, files = sys.argv[1], sorted(sys.argv[2:], key=update_of)
       rows = [histogram(f) for f in files]
       depth_max = max(max(r) for r in rows)
       matrix = np.zeros((len(rows), depth_max + 1))
       for i, row in enumerate(rows):
           for depth, count in row.items():
               matrix[i, depth] = count
       print(f"{len(rows)} files, depths 0 to {depth_max}")
       updates = [update_of(f) for f in files]
       fig, ax = plt.subplots(figsize=(8, 4))
       image = ax.imshow(np.log(matrix.T + 1), origin="lower", aspect="auto",
                         cmap="hot", extent=[updates[0], updates[-1], 0, depth_max])
       ax.set_xlabel("update")
       ax.set_ylabel("phylogenetic depth")
       fig.colorbar(image, label="log(1 + organisms)")
       fig.savefig(out, dpi=150, bbox_inches="tight")


   main()
   ```

4. Run it on all the files. The script sorts them by update number itself, so a shell pattern in any order works:

   ```sh
   python3 flame.py flame.png data/detail-*.spop
   ```

   It prints the number of files and the largest depth (here, `20 files, depths 0 to 44`). The number of files must equal the number you saved. The output format follows the extension: `.png`, `.pdf` or `.svg`.

## Reading the picture

The brightness of a cell is the logarithm of one plus the number of organisms, so a cell with one organism is dark red and a cell with a hundred is bright yellow to white. In the test run the bright band started at depth 0 at the first slice and climbed to depths of about 20 to 30 by update 1000, with a faint tail up to depth 44, and it widened as the lineages diverged. The band is the *bulk* of the population: the population at a given time has organisms at many depths, and the width of the band measures how different their histories are. A population whose bright band is narrow is nearly one lineage. Several separate bands mean separate lineages.

The columns are as in the old tools: depth is column 14 and the number of living organisms is column 5 of the `.spop` file (see the header of any detail file to confirm). The files also list genotypes that have died out (in the test run, 2712 of 3945 lines of the last file had a count of 0). They add nothing to the picture, because their count is 0.

## What differs from the old recipe

- The old page used the column numbers 14 and 5 for `.spop` files and 12 and 4 for the older `.pop` files. The port writes the first kind. The script reads those two columns, which you can change at the top.
- The old page used a shell pattern such as `detail-?00.spop detail-??00.spop` to keep the files in numerical order, because a plain `detail-*` sorts `detail-1000` before `detail-200`. The script does the sorting.
- The old Matlab recipe plotted the transpose of log(x + 1) with the `hot` colour map and `shading flat`. The script does the same with matplotlib.

## Profiling the program itself

This is a different tool, and about the program and not the population. To find out where an Avida run spends its CPU time, profile the `avida` binary with a sampling profiler.

- On macOS the system tool `sample` works and was run on the prebuilt release binary for this page. Start a run, then in another terminal run `sample <pid> 10 -file sample.txt`, where `<pid>` is the process id from `ps`. The file shows the call tree with the Rust function names of the simulation (for example `World::run`, `Stepper::end_update`, `HardwareCpu::execute_inst`). The release binary keeps its symbol names.
- Tools that draw a graphical flame graph from the same samples, such as `cargo flamegraph` (`cargo install flamegraph`, then `cargo flamegraph -p avida --release -- <avida arguments>` inside `rust/avida-native`) or `samply`, are not installed on the machine on which this page was written. This recipe is untested. If you use it, give the release profile debug information first (`CARGO_PROFILE_RELEASE_DEBUG=true`) so that the frames carry line numbers.

The `ARCHITECTURE.md` of the repository records a profile of the parallel mode (`update_organism_stats` about 23 percent, output about 21 percent), which is how the remaining sequential costs were found. See [Execution modes](../execution/index.md).

## See also

- [Output files](../getting-started/output-files.md) for the population files.
- [Systematics and genotypes](../concepts/systematics.md) for what depth means.
- [Line of descent tutorial](tutorial-line-of-descent.md), the tool to use when you need an actual phylogeny.
