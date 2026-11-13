# Map from the old wiki to this documentation

The old documentation is the wiki in the git submodule `documentation/`, written for the C++ program. This page says where each of its pages went, so the wiki can be removed when the maintainer decides.

Status words: **Ported** (content is here, checked against the Rust code or a run), **Merged** (folded into a broader page), **Deferred** (useful, not ported yet; keep the old page until it is) and **Dropped** (with the reason). No page is Deferred at the moment.

## Overview and getting started

| Old page | Status | New home or reason |
|---|---|---|
| `Home`, `Overview`, `About` | Merged | [Documentation home](../index.md) and [concepts](../concepts/index.md). The history and motivation text of `About` is not reproduced. |
| `New-user-information`, `Beginner-Documentation` | Ported | [Getting started](../getting-started/index.md), [Design an experiment](../guides/how-to-design-experiment.md). The "10 updates per generation" figure was replaced by a measured one. |
| `Configuration-and-command-reference`, `Experiment-and-analysis-guides` | Merged | [Reference](../reference/index.md), [Guides](../guides/index.md). They were link lists. |
| `Default-Ancestor-Guided-Tour` | Merged | [Organisms and genomes](../concepts/organisms-and-genomes.md), [Virtual CPUs](../concepts/hardware.md). The step-by-step trace of the ancestor is not reproduced; use the debugger in a [front end](../front-ends/index.md) or `TRACE` in analyze mode. |
| `Default-heads.org` | Merged | [Instruction sets and organism files](../reference/instruction-sets.md). |
| `Text-Viewer` and the `Text_View_*` images | Dropped | Describes the old ncurses viewer. Replaced by the [terminal interface](../front-ends/tui.md). |

## Configuration

| Old page | Status | New home or reason |
|---|---|---|
| `Avida.cfg`, `General-settings`, `File-settings`, `Topology-settings`, `Mutation-settings`, `Mutations-reversion-settings`, `Reproduction-settings`, `Divide-settings`, `Time-slicing-settings`, `Analysis-settings`, `Html-output-color-settings`, `Settings-for-genealogy-info-in-the-test-cpu` | Ported | [Settings reference](../reference/settings.md), generated from the settings table, plus the concept pages ([mutations](../concepts/mutations.md), [reproduction](../concepts/reproduction.md), [update cycle](../concepts/update-cycle.md)). |
| `Energy-model-configuration` | Ported | [The energy model](../concepts/energy.md) and the settings reference. |
| `Resource-hoarding-settings`, `Internal-resources` | Ported | [Internal resources](../concepts/internal-resources.md), with the settings in the [settings reference](../reference/settings.md#hoard_resource_group). The page was checked against the code and by runs of the collect instructions. Differences from the old page: `COLLECT_PROB_DIVISOR` defaults to 1, `collect-specific` ignores `COLLECT_AMOUNT` on the heads CPU, and there are two more instructions (`collect-specific-ratio`, `collect-needed`) in the port's library. |
| `Environment-file`, `Gradient-resources` | Ported | [Environment file reference](../reference/environment-file.md). The `MUTATION` directive described there does not exist in the program. |
| `Events-file`, `List-of-actions`, `PrintData-Options` | Ported | [Events file](../reference/events-file.md), [Actions](../reference/actions.md). The old `PrintData` list was longer than the program's 30 names. |
| `Instruction-Set`, `Instset-heads.cfg` | Ported | [Instruction sets](../reference/instruction-sets.md) and [hardware](../concepts/hardware.md). The old page described `instset-classic.cfg`, which no longer exists. |
| `Setting-up-fixed-length-organisms` | Ported | [Fixed-length organisms](../guides/how-to-fixed-length.md). |
| `Quirks` | Ported | [Quirks](../guides/quirks.md), marked as not re-verified. |

## Topics

| Old page | Status | New home or reason |
|---|---|---|
| `Deme-introduction` | Ported | [Demes](../concepts/demes.md). |
| `Deme-migration` | Ported | [Deme migration matrix](../reference/migration-matrix.md), run with matrices of each kind. The old page said a bad `AlterMigrationConnection` ends the run; in the port it does not. |
| `How-to-create-a-deme-replication-event` | Dropped | A C++ source-code tutorial. |
| `Introduction-to-parasites` | Ported | [Parasites](../concepts/parasites.md); the old page's `INFECTION_MECHANISM` options differed from the program's, so the program's table is used. |
| `Using-mating-types-(separate-sexes)` | Ported | [Mating types and mate choice](../concepts/mating-types.md), run with the consistency test `mating_types`. Differences from the old page: the mate-assessment noise is a variance, not a standard deviation; a juvenile cannot divide; mating types are ignored when `NUM_DEMES` is above 1. |
| `LookSensor` | Ported | [The look sensor](../reference/look-sensor.md), checked against the sensor code and by runs with the output tracking on. The old page was wrong about the value register of a failed look and about search types for food. See also [predator and prey](../concepts/predator-prey.md). |
| `Hand-written-Logic-Nine-Programs` | Ported | [Hand-written programs for the nine logic tasks](../guides/hand-written-logic-programs.md). Each program was run with a trace in the test CPU and earned its task. The register tables of the old page are replaced by comments. |
| `Creating-a-flame-graph` | Ported | [Draw a flame graph of genetic diversity](../guides/how-to-flame-graph.md), with a new Python 3 script in place of the removed `flamegraph.py` and `hist_map`. The old page is about phylogenetic depth over time, not about CPU profiling; a short note on profiling the program is at the end of the new page. |

## Analyze mode

| Old page | Status | New home or reason |
|---|---|---|
| `Analyze-File`, `Sample-analyze-programs` | Ported | [Analyze mode](../reference/analyze-mode.md). Not every command's description was re-run (the page says which were). `LOAD_MULTI_DETAIL` from the old page is not in the program's command registry. |
| `Analyze-mode---using-variables`, `---building-your-own-commands`, `---working-with-batches` | Ported | The variables, functions and batches sections of analyze mode, with runnable examples. |
| `Analyze-mode---finding-lineages` | Ported | [Line of descent tutorial](../guides/tutorial-line-of-descent.md). |
| `Analyze-mode---testing-a-genome-sequence` | Merged | `LOAD_SEQUENCE`, `RECALCULATE` and `TRACE` on the [analyze page](../reference/analyze-mode.md). |
| `Analyze-mode---try-it-yourself-exercises` | Dropped | Exercises for a course; the tutorials replace them. |

## Developer material

| Old page | Status | New home or reason |
|---|---|---|
| `Developers-Guide`, `Development---Getting-Started`, `Development---Coding-Style-Guide` | Dropped | They describe the C++ tree, the CMake build and C++ style. Replaced by [For contributors](../contributing/index.md). |
| `Development---Tutorial---Birth-Death-Cycle`, `---C-Plus-Plus-Intro`, `---Environment`, `---Genome`, `---Instruction Checklist`, `---Life-Cycle`, `---Task-Checklist` | Dropped | C++ code tutorials. The concepts they explain are in [concepts](../concepts/index.md); how to add an instruction or a task in Rust is in [architecture](../contributing/architecture.md#adding-things). |
| `Avida-3-API` | Dropped | A design for a future API that was not built. |
| `Wave5-cResourceCount-Starter-Seam`, `Tech-Debt-Backlog`, `archive/*` | Dropped | History and planning of the FFI migration and the maintainer's backlog, not user documentation. The `rust-migration-*.md` files at the repository root remain as history. |

## Glossary and images

| Old material | Status | New home or reason |
|---|---|---|
| `glossary/*` (44 pages: terms such as Genome, Genotype, Label, Registers and one page for each default instruction) | Merged | [Glossary](../reference/glossary.md) for the terms, and the instruction table in [virtual CPUs](../concepts/hardware.md#the-default-instruction-set). |
| `images/cpu2.gif`, `images/AvidaLogo.gif`, `images/host-parasite-both-small.png` | Dropped | Replaced by Mermaid diagrams with text descriptions. |
| `images/inres_*.gif` | Dropped | Nine diagrams of internal resources. Replaced by a Mermaid diagram and tables on the [internal resources](../concepts/internal-resources.md) page. |

## Before removing the wiki

No page of the wiki is marked Deferred any more. What the new pages do not carry over is listed in the table rows above: the history text of `About`, the step-by-step trace of the default ancestor, the register tables of the hand-written logic programs, the exercises of the analyze tutorial and the unverified `Quirks` content. The ported pages disagree with the old wiki in several places, which the rows name. Decide whether those departures are acceptable, then remove the submodule.
