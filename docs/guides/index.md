# Guides and tutorials

Two kinds of pages live here: short how-to pages for common jobs, and nine tutorials, each a small experiment from the digital-evolution literature that you can run and compare with a known result.

## How-to guides

- [Design an experiment: a checklist](how-to-design-experiment.md)
- [Run replicate seeds and several arms](how-to-replicates.md)
- [Set up fixed-length organisms](how-to-fixed-length.md)
- [Things that surprise people (quirks)](quirks.md)
- [Hand-written programs for the nine logic tasks](hand-written-logic-programs.md)
- [Draw a flame graph of genetic diversity](how-to-flame-graph.md), with a note on profiling the program itself

## Tutorials

Each tutorial compares two or more *arms* (variants of one experiment) over several random seeds. They share a method, described in [Run replicate seeds and several arms](how-to-replicates.md); read that page first.

| Tutorial | Question | Runs from |
|---|---|---|
| [Stepping stones](tutorial-stepping-stones.md) | Do complex features need simpler ones to evolve first? | The stock folder. |
| [Mutation rate](tutorial-mutation-rate.md) | How much mutation is too much (the error threshold)? | The stock folder. |
| [Limited resources](tutorial-limited-resources.md) | Does competition for resources keep more kinds of organism alive? | The stock folder plus one environment file. |
| [Host and parasite](tutorial-host-parasite.md) | Do parasites drive hosts to change their tasks? | A configuration of the consistency suite. |
| [Sex and asex](tutorial-sex-vs-asex.md) | Does recombination speed up adaptation? | The stock folder. |
| [Population structure](tutorial-population-structure.md) | Does local reproduction change what evolves? | The stock folder. |
| [Line of descent](tutorial-line-of-descent.md) | What was the path from the ancestor to an evolved genotype? | A finished run, in analyze mode. |
| [Streamlining](tutorial-streamlining.md) | Can selection shrink a genome to the shortest program for the task? | The stock folder. |
| [An adder from NAND](tutorial-adder-from-nand.md) | Can evolution build addition without arithmetic instructions? | The stock folder plus new instruction set and environments. |

## About the results quoted

The observations in the tutorials come from short pilot runs by the maintainer, usually with one to four seeds per arm. They illustrate what to look for; they are not evidence. Replicate with more seeds before drawing any conclusion, and report what you find.
