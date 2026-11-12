//! Analyze mode (`avida -a`): `cAnalyze`
//! (avida-core/source/analyze/cAnalyze.cc at the reference commit) and the
//! parts of the world it drives.
//!
//! Ported here:
//!
//! * the command language: `cAnalyze::LoadCommandList`, `ProcessCommands`,
//!   `PreProcessArgs` (`$a`..`$z` globals, `$A`..`$Z` function locals,
//!   `$0`..`$9` function arguments, `$$`), the flow commands `FUNCTION`,
//!   `FOREACH`, `FORRANGE`, and `INCLUDE`/`RUN`;
//! * batches and genotypes (`cGenotypeBatch`, `cAnalyzeGenotype` with its
//!   copy-constructor omissions) and every command that only rearranges,
//!   filters or prints them (`LOAD`, `LOAD_SEQUENCE`, `LOAD_ORGANISM`,
//!   `FILTER`, `FIND_*`, `SAMPLE_*`, `KEEP_TOP`, `RENAME`, `ALIGN`,
//!   `DETAIL*`, `HISTOGRAM`, `PRINT_TASKS*`, `PRINT_PHENOTYPES`, batch
//!   control, `SET`, `CONFIG_GET`/`CONFIG_SET`, `ECHO`, `STATUS`, `SYSTEM`,
//!   `TEST`, ...);
//! * the genotype data-entry table (`cAnalyzeGenotype::buildDataCommandManager`)
//!   with `cFlexVar` values, comparisons and HTML colouring;
//! * the commands that run genomes in the test CPU ([`cpu`]):
//!   `RECALCULATE` / `RECALC` (`cPhenPlastGenotype`, all copied fields and
//!   the plasticity summary), `TRUNCATELINEAGE`, `PRINT`, `TRACE`, and the
//!   lazily computed landscape and plasticity data entries;
//! * `cLandscape` ([`landscape`]) and the actions `AnalyzeLandscape`,
//!   `PrecalcLandscape`, `FullLandscape`, `DumpLandscape`,
//!   `DeletionLandscape`, `InsertionLandscape`, `RandomLandscape`,
//!   `SampleLandscape`, `PredictWLandscape`, `PredictNuLandscape`,
//!   `PairTestLandscape`, `AnalyzePopulation` (in analyze mode only a
//!   warning), plus `VERBOSE` / `SetVerbose` ([`actions`]); the analyze job
//!   queue runs on `MAX_CONCURRENCY` worker threads with the output of the
//!   single-threaded queue (module `jobs`, which also runs `RECALCULATE`
//!   ahead on the workers);
//! * `cMutationalNeighborhood` / `cMutationalNeighborhoodResults`
//!   ([`mutational_neighborhood`]) and the `MutationalNeighborhood` action;
//! * population statistics and genome-pair commands: `PRINT_DIVERSITY`,
//!   `PRINT_DISTANCES`, `MAP_DEPTH`, `HAMMING`, `LEVENSTEIN`, `SPECIES`,
//!   `RECOMBINE`, `RECOMBINE_SAMPLE`, `MUTAGENIZE`, `WRITE_CLONE`,
//!   `WRITE_INJECT_EVENTS`, `WRITE_INJECT_INITIAL`, `WRITE_COMPETITION`,
//!   `ANALYZE_INSTRUCTIONS`, `ANALYZE_INST_POP`,
//!   `ANALYZE_MUTATION_TRACEBACK`, `ANALYZE_POP_COMPLEXITY`;
//! * mutant, knockout and offspring tests in the test CPU: `SAMPLE_OFFSPRING`,
//!   `MAP_TASKS` / `MAP`, `MAP_MUTATIONS`, `ANALYZE_KNOCKOUTS`,
//!   `ANALYZE_COMPLEXITY`, `ANALYZE_FITNESS_TWO_SITES`,
//!   `ANALYZE_LINEAGE_COMPLEXITY`, `ANALYZE_MUTS`, `ANALYZE_NEWINFO`,
//!   `MUTATION_REVERT`, `COMPETE`, and `CALC_FUNCTIONAL_MODULARITY`
//!   (`cModularityAnalysis`, with its data entries `tasks_done`,
//!   `sites_per_task.N`, ...);
//! * tree statistics: `PRINT_TREE_STATS`, `PRINT_CUMULATIVE_STEMMINESS`
//!   (`cAnalyzeTreeStats_CumulativeStemminess`) and `PRINT_GAMMA`
//!   (`cAnalyzeTreeStats_Gamma`);
//! * `INTERACTIVE` and `ANALYZE_MODE 2` (`avida -i`: the script, then
//!   `cAnalyze::RunInteractive` on standard input, [`Analyze::set_input`]
//!   for another source); at the end of the input the C++ prompts `>> `
//!   forever, which stops here with an error instead;
//! * the complexity, skeleton and modularity commands
//!   (modules `cmd_complexity`, `cmd_skeleton`, `cmd_modularity`):
//!   `ANALYZE_COMPLEXITY_TWO_SITES`, `ANALYZE_COMPLEXITY_DELTA`,
//!   `COMMUNITY_COMPLEXITY`, `ANALYZE_MATE_SELECTION`,
//!   `PRINT_RESOURCE_FITNESS_MAP`, `AVERAGE_MODULARITY`, `GET_SKELETONS`,
//!   `COUNT_NEW_SIG_LINEAGES`, `COUNT_NOVEL_SKELETONS`,
//!   `CALC_SKELETON_DIVERSITY`, `GET_LARGEST_SKELETON`,
//!   `ALL_COMPLEXITY_BARRIERS`, `CHANGE_DEBUG`, `COMPLEXITY_DEBUG`;
//! * `ANALYZE_REDUNDANCY_BY_INST_FAILURE` (module `cmd_redundancy`), on
//!   the nop-less copy of the instruction set the C++ makes; it stops with
//!   an error for the hardware types that read the copy's uninitialised
//!   stack size and uops per cycle (3, 4, 5);
//! * the analyze-mode world ([`AnalyzeWorld`]) and the front end
//!   ([`run_analyze`], or [`run_analyze_with`] given the [`SetupHooks`] of
//!   the population layer: `cWorld::setup` builds the migration matrix and
//!   the population in analyze mode too, and `-e` lists the actions).
//!
//! Not ported, failing with [`AnalyzeError::Unsupported`] naming the C++:
//! every registered action unless the host runs it
//! ([`AnalyzeHost::run_action`]). `AnalyzePopulation` outside analyze mode
//! is the population layer's (`avida-world`); here it cannot be reached,
//! as `RunFile` and `RunInteractive` both set analyze mode.
//! The `HillClimb` action fails as soon as a batch is not empty: its
//! `cLandscape::HillClimb` reads and writes one site past the end of the
//! genome.

pub mod actions;
mod analyze;
mod apto_map;
mod cmd_complexity;
mod cmd_control;
mod cmd_interactive;
mod cmd_load;
mod cmd_modularity;
mod cmd_mutation;
mod cmd_output;
mod cmd_population;
mod cmd_reduce;
mod cmd_redundancy;
mod cmd_skeleton;
mod cmd_tree;
pub mod cpu;
pub mod data_entry;
mod error;
pub mod flex;
pub mod genotype;
pub mod host;
mod jobs;
pub mod landscape;
pub mod mutational_neighborhood;
mod run;
pub mod verbose;
pub mod world;

pub use analyze::{Analyze, Command, GenotypeBatch, INITIAL_BATCHES, MAX_BATCHES};
pub use error::AnalyzeError;
pub use flex::FlexVar;
pub use genotype::{AnalyzeGenotype, PhenPlastSummary, RecalcResult};
pub use host::{AnalyzeHost, CpuTestOptions, TestContext, WorkerWorld};
pub use jobs::{worker_count, WorkerStats};
pub use landscape::Landscape;
pub use mutational_neighborhood::MutationalNeighborhood;
pub use run::{run_analyze, run_analyze_with};
pub use world::{AnalyzeWorld, NoSetupHooks, SetupHooks, WorldInit};
