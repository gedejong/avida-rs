# Features that stop with `unsupported:`

A feature the port has not ported must fail loudly. It stops the run with a message of the form

```text
unsupported: SETTING=value (C++: file:symbol)
```

instead of running with different behaviour. The `C++:` part names the place in the original source (at the git tag `cpp-final`) that the stop stands for.

The maintainer's ruling is that where the original C++ behaviour is *undefined or crashes* (an out-of-bounds read, an uninitialised value, a NULL dereference, an endless recursion), the stop stays. Where the original behaviour is defined, the port implements it. As of the last count there are roughly 140 stop sites in the code; about 95 guard undefined behaviour of the C++ and stay. The authoritative list is the status section of `rust/avida-native/ARCHITECTURE.md` in the repository, and the code itself (search the crates for `unsupported:`); this page groups what matters to a user.

## Things you may meet

| Feature | What happens | Why |
|---|---|---|
| `SLICING_METHOD` 3 and 4 (and any value without a scheduler) | Stops at start-up. | The C++ program crashes on them. |
| `WORLD_GEOMETRY 6` (3-D lattice) | Stops. | The C++ builds the lattice by reading out of bounds. |
| `NUM_DEMES` that does not divide `WORLD_Y` (analyze mode) | Stops. | The C++ crashes. |
| `PrintDemeEnergySharingStats` | Stops. | The C++ reads uninitialised phenotype counters. |
| Deme network (`cDemeNetwork`) | Prints the `cDemeNetwork requires Boost` error and exits with code 50. | The reference binary was built without Boost; the port matches it. |
| Two uninitialised members of `cDemeCellEvent` | Stops when read. | Uninitialised in the C++. |
| `LOG_SLEEP_TIMES` with an organism outside cell 0 dying or sleeping | Stops. | The C++ indexes past a single-element array. |
| Deme resources of grid, torus or partial geometry in some paths | Stops. | Deme grids are not sized in those paths. |
| `ResetDemes` and the in-place `CompeteDemes` with `USE_AVATARS` | Stops. | The reference binary crashes there. |
| `HGT_FRAGMENT_SELECTION` 0 and 1 (match and trimmed placement) | Stops when reached. | `cGenomeUtil::FindSubstringMatch` returns a value read from freed memory. |
| A gradient habitat kill under HGT with competence | Stops. | The C++ crashes when the kill re-enters the resource update. |
| Path and den resources next to a habitat | Stops. | The C++ recurses until the stack overflows. |
| A navigation trace with an empty trace array, or an environment without reactions that prints one | Stops with an error. | The C++ reads before an empty array or recurses until the stack overflows. |
| `optimize` task arguments on which the reference crashes or hangs (`function=20`, too few or negative `numvars`, some negative `varlength`) | The environment fails to load with `unsupported: optimize ...`. | The reference binary crashes, aborts or does not finish. |
| `PROMOTER_EXE_LENGTH` values, `UOPS_PER_CYCLE` above 255, an odd number of `NON_1_RESOURCE_RATIOS` tokens, `TWO_FOLD_COST_SEX` with some energy settings, and similar narrow settings | Stop. | Each guards undefined C++ behaviour; the message names it. |
| Phenotypic plasticity with an organism that has no genotype, and several producers in a public-good plasticity test | Stops. | The C++ dereferences a null pointer, or orders a set by heap address. |
| A `MAX_PREY_BT` kill from a plasticity test or from a test CPU run by an event; a second kill in one test; a kill that draws random numbers | Stops. | See the note on queued kills in `PORTING.md`. |
| The ASL scripting language's `Config`, `World` and `Driver` objects | Fail when called. | They are signature-only stubs; the language itself is parsed and checked. |
| Hardware type 1 (the old SMT CPU) | Reported as an unsupported hardware type. | Not ported. |

## How to tell a stop from a bug

A stop is a clean exit with an error message that begins `unsupported:` and names the setting or instruction. A run that goes wrong without that message is a bug, and the maintainers want to know about it.

## Related

- The parallel execution mode has its own list of things it does not do: [execution modes](../execution/index.md#what-it-does-not-guarantee).
- A few behaviours differ in output formatting, not in simulation; they are listed in [the migration page](../migration/from-cpp.md).
