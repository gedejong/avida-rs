# Things that surprise people (quirks)

These notes were collected by users of the original program. They are carried over from the old wiki and have **not** been re-verified for the port. The port reproduces the C++ behaviour, quirks included, so they are expected to hold. Check one on a short run before relying on it.

- **Printing genomes during a run changes the run.** Printing genomes passes organisms through the test CPU, which draws random numbers from the run's generator. A run cannot be repeated if events that print genomes are added, removed or changed.
- **`ZeroMuts` does not take effect at once.** It sets all mutation rates to zero, but organisms only check them when they act on them, mostly when they divide. You do not have a zero mutation rate until all currently living organisms have been replaced. The old wiki also says it does not work at update 0, where the configuration overrides it.
- **`BIRTH_METHOD 3` replaces the parent when no neighbouring cell is empty.** To allow only empty cells you also need `ALLOW_PARENT 0`, which the old notes say does not yet work with method 3.
- **When demes divide** (`DEMES_DIVIDE_METHOD 0`), both the source and the target deme are replaced.
- **`DIVIDE_METHOD 0`** leaves only part of the mother's state untouched: the pointers and register values. Almost everything else is reset.
- **A limited spatial resource with no diffusion** that flows into a part of the grid with no organisms is not reported in the data files, despite the inflow.
- **The count of `nop-A` across genomes overstates its use.** When an offspring is allocated its genome is filled with `nop-A` instructions as placeholders, so organisms in the middle of being copied add to the count.

For output-format differences between this port and the C++ program, see [Migration from C++ Avida](../migration/from-cpp.md).
