# Set up fixed-length organisms

Genomes of equal length are much easier to analyze: you can line them up without computing an alignment. Avida's mutations can change length, so you switch those off. Change these settings in `avida.cfg` (or with `-set`):

| Setting | Value | Why |
|---|---|---|
| `DIVIDE_INS_PROB` | 0.0 | No insertion mutations at divide (the stock value is 0.05). |
| `DIVIDE_DEL_PROB` | 0.0 | No deletion mutations at divide (the stock value is 0.05). |
| `OFFSPRING_SIZE_RANGE` | 1.0 | Offspring must be exactly the size of the parent (stock 2.0). |
| `STERILIZE_UNSTABLE` | 1 | Organisms whose copy loop cannot make a perfect copy (before mutation) are not allowed to reproduce. |

The last one matters because an organism can have a faulty copy loop that copies an instruction twice or skips one. That changes length without any insertion or deletion mutation, so turning off `DIVIDE_INS_PROB` and `DIVIDE_DEL_PROB` does not stop it.

`COPY_INS_PROB`, `COPY_DEL_PROB` and the slip probabilities are 0 in the stock configuration; keep them there.

A short check: with these settings, a 300-update stock run kept the dominant genotype at length 100 (`dominant.dat`, column 6). That is a smoke test, not a proof; confirm with a longer run before you rely on it.
