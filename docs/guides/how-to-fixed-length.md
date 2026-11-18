# Set up fixed-length organisms

Genomes of equal length are much easier to analyze: you can line them up without computing an alignment. Avida's mutations can change length, so you switch those off. Change these settings in `avida.yaml` (or with `-set`):

| Setting | Value | Why |
|---|---|---|
| `mutation.divide_ins_prob` | 0.0 | No insertion mutations at divide (the stock value is 0.05). |
| `mutation.divide_del_prob` | 0.0 | No deletion mutations at divide (the stock value is 0.05). |
| `divide.offspring_size_range` | 1.0 | Offspring must be exactly the size of the parent (stock 2.0). |
| `reversion.sterilize_unstable` | 1 | Organisms whose copy loop cannot make a perfect copy (before mutation) are not allowed to reproduce. |

The last one matters because an organism can have a faulty copy loop that copies an instruction twice or skips one. That changes length without any insertion or deletion mutation, so turning off `mutation.divide_ins_prob` and `mutation.divide_del_prob` does not stop it.

`mutation.copy_ins_prob`, `mutation.copy_del_prob` and the slip probabilities are 0 in the stock configuration; keep them there.

A short check: with these settings, a 300-update stock run kept the dominant genotype at length 100 (`dominant.dat`, column 6). That is a smoke test, not a proof; confirm with a longer run before you rely on it.
