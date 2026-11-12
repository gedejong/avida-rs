//! PrintActions.cc histogram, fitness and genotype-analysis printers
//! (avida-core/source/actions/PrintActions.cc) and the landscape actions of
//! LandscapeActions.cc as they behave in a run.
//!
//! Each action is the constructor (argument parsing, [`parse`]) and the
//! `Process` method (`run_hist_action`) of one `cAction*` class. Members the
//! C++ classes change between calls (first-run flags, the lineage labels of
//! `cActionPrintLineageCounts`, the histogram maximum the fitness
//! histograms round up by reference) live in [`HistState`], keyed by an id
//! given to each parsed action. The `cPhenPlastSummary` the plasticity
//! printers attach to a genotype (`cPhenPlastUtil`) is kept by genotype id
//! in `WorldHost::phenplast`, which `cEnvironment::GetTaskProbability`
//! shares.
//!
//! Analyze-mode branches are not reachable from a run and are left out. In
//! a run most landscape actions do nothing (their work sits in the
//! analyze-mode branch); some still create or comment their file, and
//! `cWorld::GetAnalyze()` builds a `cAnalyze` the first time a run asks for
//! it, whose job queue seeds itself from the world's random number
//! generator.

use std::collections::{BTreeMap, BTreeSet};
use std::sync::atomic::{AtomicUsize, Ordering};

use avida_analyze::cpu::PhenPlastGenotype;
use avida_analyze::PhenPlastSummary;
use avida_apto::fmt::fmt_double;
use avida_apto::AvidaRng;
use avida_config::feedback::Feedback;
use avida_config::string_util::{as_double, as_int, convert_double, pop_word, strtod, trim};
use avida_genome::{Genome, InstructionSequence};
use avida_hardware::test_cpu::{CpuTestInfo, TestCpu, TestWorld};
use avida_phenotype::Merit;
use avida_systematics::apto_map::AptoMap;
use avida_systematics::{GroupRef, TransmissionType};

use crate::actions::ParseContext;
use crate::stat_tools::DoubleSum;
use crate::world::{make_test_world, World};

/// The registered action names this module parses.
pub const HIST_ACTIONS: &[&str] = &[
    "PrintDepthHistogram",
    "PrintParasiteDepthHistogram",
    "PrintHostDepthHistogram",
    "PrintGenotypeAbundanceHistogram",
    "PrintLineageCounts",
    "PrintDominantGroupGenotypes",
    "PrintDetailedFitnessData",
    "PrintLogFitnessHistogram",
    "PrintRelativeFitnessHistogram",
    "PrintCCladeCounts",
    "PrintCCladeFitnessHistogram",
    "PrintCCladeRelativeFitnessHistogram",
    "PrintGenomicSiteEntropy",
    "PrintPhenotypicPlasticity",
    "PrintTaskProbHistogram",
    "PrintPlasticGenotypeSummary",
    "PrintGeneticDistanceData",
    "PrintPopulationDistanceData",
    "TestDominant",
    "CalcConsensus",
    "AnalyzeLandscape",
    "PrecalcLandscape",
    "FullLandscape",
    "DumpLandscape",
    "DeletionLandscape",
    "InsertionLandscape",
    "PredictWLandscape",
    "PredictNuLandscape",
    "RandomLandscape",
    "SampleLandscape",
    "HillClimb",
    "PairTestLandscape",
    "AnalyzePopulation",
    "MutationalNeighborhood",
];

/// `Driver().Abort` codes (avida/core/Types.h).
const INVALID_CONFIG: i32 = 2;
const INTERNAL_ERROR: i32 = 50;

/// Which genotypes a depth histogram bins.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DepthKind {
    /// `cActionPrintDepthHistogram`: every genotype.
    All,
    /// `cActionPrintParasiteDepthHistogram`: parasite genotypes; an empty
    /// histogram starts at depth 0.
    Parasite,
    /// `cActionPrintHostDepthHistogram`. QUIRK: it bins the *parasite*
    /// genotypes too (copied from the parasite action), without the
    /// empty-histogram fix.
    Host,
}

/// A parsed action of this module.
#[derive(Debug, Clone, PartialEq)]
pub enum HistAction {
    /// `cActionPrint{,Parasite,Host}DepthHistogram`
    DepthHistogram { kind: DepthKind, filename: String },
    /// `cActionPrintGenotypeAbundanceHistogram`
    GenotypeAbundanceHistogram { filename: String },
    /// `cActionPrintLineageCounts`
    LineageCounts { id: usize, filename: String },
    /// `cActionPrintDominantGroupGenotypes` (empty filename: archive names)
    DominantGroupGenotypes { filename: String },
    /// `cActionPrintDetailedFitnessData`
    DetailedFitnessData {
        save_max: i32,
        print_histo: i32,
        hist_fmax: f64,
        hist_fstep: f64,
        filenames: [String; 3],
    },
    /// `cActionPrintLogFitnessHistogram` / `cActionPrintRelativeFitnessHistogram`
    FitnessHistogram {
        id: usize,
        relative: bool,
        filename: String,
        mode: String,
        hist_fmin: f64,
        hist_fstep: f64,
        hist_fmax: f64,
    },
    /// `cActionPrintCCladeCounts`
    CCladeCounts { id: usize, filename: String },
    /// `cActionPrintCCladeFitnessHistogram` /
    /// `cActionPrintCCladeRelativeFitnessHistogram`
    CCladeFitnessHistogram {
        id: usize,
        relative: bool,
        filename: String,
        mode: String,
        hist_fmin: f64,
        hist_fstep: f64,
        hist_fmax: f64,
    },
    /// `cActionPrintGenomicSiteEntropy`
    GenomicSiteEntropy,
    /// `cActionPrintPhenotypicPlasticity`
    PhenotypicPlasticity { filename: String, num_trials: i32 },
    /// `cActionPrintTaskProbHistogram`
    TaskProbHistogram {
        id: usize,
        filename: String,
        weighted: bool,
    },
    /// `cActionPrintPlasticGenotypeSummary`
    PlasticGenotypeSummary { id: usize, filename: String },
    /// `cActionPrintGeneticDistanceData`: the reference genome is loaded on
    /// the first call (the C++ constructor loads it).
    GeneticDistanceData {
        id: usize,
        creature: String,
        filename: String,
    },
    /// `cActionPrintPopulationDistanceData`
    PopulationDistanceData {
        creature: String,
        filename: String,
        save_genotypes: i32,
    },
    /// `cActionTestDominant`
    TestDominant { filename: String },
    /// `cActionCalcConsensus`
    CalcConsensus { lines_saved: i32, inst_set: String },
    /// A landscape action whose run-mode `Process` does nothing
    /// (`PrecalcLandscape`, `DeletionLandscape`, `InsertionLandscape`,
    /// `RandomLandscape`, `SampleLandscape`, `HillClimb`,
    /// `PairTestLandscape`).
    LandscapeNoop,
    /// `cActionDumpLandscape` / `cActionPredictWLandscape` /
    /// `cActionPredictNuLandscape`: the file is created (and left empty)
    /// before the analyze-mode branch.
    LandscapeEmptyFile { filename: String },
    /// `cActionFullLandscape`: only the distance check runs.
    FullLandscape { distance: i32 },
    /// `cActionAnalyzeLandscape`
    AnalyzeLandscape { filename: String },
    /// `cActionMutationalNeighborhood`
    MutationalNeighborhood { filename: String },
    /// `cActionAnalyzePopulation`
    AnalyzePopulation {
        sample_prob: f64,
        landscape: i32,
        save_genotypes: i32,
        filename: String,
    },
}

/// State the actions of this module keep between calls.
#[derive(Debug, Default)]
pub struct HistState {
    /// Actions past their first call (`first_run` / `first_time` members).
    started: BTreeSet<usize>,
    /// `lineage_labels` of each `cActionPrintLineageCounts`.
    lineage_labels: BTreeMap<usize, Vec<i32>>,
    /// `m_hist_fmax` of each fitness histogram after `MakeHistogram`
    /// rounded it up.
    hist_fmax: BTreeMap<usize, f64>,
    /// `m_reference` of each `cActionPrintGeneticDistanceData`.
    references: BTreeMap<usize, Genome>,
    /// Whether `cWorld::GetAnalyze()` has built the `cAnalyze`.
    analyze_built: bool,
}

static NEXT_ACTION_ID: AtomicUsize = AtomicUsize::new(0);

fn next_id() -> usize {
    NEXT_ACTION_ID.fetch_add(1, Ordering::Relaxed)
}

/// `if (largs.GetSize()) x = largs.PopWord();`
struct Args(String);

impl Args {
    fn word(&mut self) -> Option<String> {
        if self.0.is_empty() {
            None
        } else {
            Some(pop_word(&mut self.0))
        }
    }
    fn word_or(&mut self, default: &str) -> String {
        self.word().unwrap_or_else(|| default.to_string())
    }
    fn upper_or(&mut self, default: &str) -> String {
        self.word()
            .map_or_else(|| default.to_string(), |w| w.to_ascii_uppercase())
    }
    fn double_or(&mut self, default: f64) -> f64 {
        self.word().map_or(default, |w| as_double(&w))
    }
    fn int_or(&mut self, default: i32) -> i32 {
        self.word().map_or(default, |w| as_int(&w))
    }
}

/// The constructor of the action `name` (a [`HIST_ACTIONS`] spelling) applied to
/// `args`; `None` for any other name.
pub fn parse(
    name: &str,
    args: &str,
    ctx: &ParseContext<'_>,
    feedback: &mut Feedback,
) -> Option<Result<HistAction, String>> {
    let _ = feedback;
    let mut a = Args(args.to_string());
    let depth = |kind, default: &str, a: &mut Args| HistAction::DepthHistogram {
        kind,
        filename: a.word_or(default),
    };
    let action = match name {
        "PrintDepthHistogram" => depth(DepthKind::All, "depth_histogram.dat", &mut a),
        "PrintParasiteDepthHistogram" => {
            depth(DepthKind::Parasite, "depth_parasite_histogram.dat", &mut a)
        }
        "PrintHostDepthHistogram" => depth(DepthKind::Host, "depth_host_histogram.dat", &mut a),
        "PrintGenotypeAbundanceHistogram" => HistAction::GenotypeAbundanceHistogram {
            filename: a.word_or("genotype_abundance_histogram.dat"),
        },
        "PrintLineageCounts" => HistAction::LineageCounts {
            id: next_id(),
            filename: a.word_or("lineage_counts.dat"),
        },
        "PrintDominantGroupGenotypes" => HistAction::DominantGroupGenotypes {
            filename: a.word_or(""),
        },
        "PrintDetailedFitnessData" => {
            let save_max = a.int_or(0);
            let print_histo = a.int_or(0);
            let hist_fmax = a.double_or(1.0);
            let hist_fstep = a.double_or(0.1);
            let f0 = a.word_or("fitness.dat");
            let f1 = a.word_or("fitness_histos.dat");
            let f2 = a.word_or("fitness_histos_testCPU.dat");
            HistAction::DetailedFitnessData {
                save_max,
                print_histo,
                hist_fmax,
                hist_fstep,
                filenames: [f0, f1, f2],
            }
        }
        "PrintLogFitnessHistogram" | "PrintRelativeFitnessHistogram" => {
            let relative = name == "PrintRelativeFitnessHistogram";
            let filename = a.word_or(if relative {
                "rel_fitness_hist.dat"
            } else {
                "fitness_log_hist.dat"
            });
            let mode = a.upper_or("CURRENT");
            let (min, step, max) = if relative {
                (0.0, 0.1, 2.0)
            } else {
                (-3.0, 0.5, 12.0)
            };
            HistAction::FitnessHistogram {
                id: next_id(),
                relative,
                filename,
                mode,
                hist_fmin: a.double_or(min),
                hist_fstep: a.double_or(step),
                hist_fmax: a.double_or(max),
            }
        }
        "PrintCCladeCounts" => HistAction::CCladeCounts {
            id: next_id(),
            filename: a.word_or("cclade_count.dat"),
        },
        "PrintCCladeFitnessHistogram" | "PrintCCladeRelativeFitnessHistogram" => {
            let relative = name == "PrintCCladeRelativeFitnessHistogram";
            let filename = a.word_or(if relative {
                "cclade_rel_fitness_hist.dat"
            } else {
                "cclade_fitness_hist.dat"
            });
            let mode = a.upper_or("CURRENT");
            let (min, step, max) = if relative {
                (0.0, 0.2, 2.0)
            } else {
                (-3.0, 0.5, 12.0)
            };
            HistAction::CCladeFitnessHistogram {
                id: next_id(),
                relative,
                filename,
                mode,
                hist_fmin: a.double_or(min),
                hist_fstep: a.double_or(step),
                hist_fmax: a.double_or(max),
            }
        }
        "PrintGenomicSiteEntropy" => HistAction::GenomicSiteEntropy,
        "PrintPhenotypicPlasticity" => HistAction::PhenotypicPlasticity {
            filename: a.word_or("phenplast"),
            num_trials: a.int_or(1000),
        },
        "PrintTaskProbHistogram" => HistAction::TaskProbHistogram {
            id: next_id(),
            filename: a.word_or("task_prob_hist.dat"),
            weighted: a.word().is_some_and(|w| as_int(&w) != 0),
        },
        "PrintPlasticGenotypeSummary" => HistAction::PlasticGenotypeSummary {
            id: next_id(),
            filename: a.word_or("genotype_plasticity.dat"),
        },
        "PrintGeneticDistanceData" => {
            // `creature_file = largs.PopWord();` (empty when there is none)
            let creature = pop_word(&mut a.0);
            HistAction::GeneticDistanceData {
                id: next_id(),
                creature,
                filename: a.word_or("genetic_distance.dat"),
            }
        }
        "PrintPopulationDistanceData" => HistAction::PopulationDistanceData {
            creature: a.word_or(""),
            filename: a.word_or(""),
            save_genotypes: a.int_or(0),
        },
        "TestDominant" => HistAction::TestDominant {
            filename: a.word_or("dom-test.dat"),
        },
        "CalcConsensus" => {
            let mut largs = args.to_string();
            trim(&mut largs);
            let mut a = Args(largs);
            HistAction::CalcConsensus {
                lines_saved: a.int_or(0),
                inst_set: a.word_or(ctx.default_inst_set),
            }
        }
        "PrecalcLandscape" | "DeletionLandscape" | "InsertionLandscape" | "RandomLandscape"
        | "SampleLandscape" | "HillClimb" | "PairTestLandscape" => HistAction::LandscapeNoop,
        "DumpLandscape" => HistAction::LandscapeEmptyFile {
            filename: a.word_or("land-dump.dat"),
        },
        "PredictWLandscape" | "PredictNuLandscape" => HistAction::LandscapeEmptyFile {
            filename: a.word_or("land-predict.dat"),
        },
        "FullLandscape" => {
            let _ = a.word();
            HistAction::FullLandscape {
                distance: a.int_or(1),
            }
        }
        "AnalyzeLandscape" => HistAction::AnalyzeLandscape {
            filename: a.word_or("land-analyze.dat"),
        },
        "MutationalNeighborhood" => HistAction::MutationalNeighborhood {
            filename: a.word_or("mut-neighborhood.dat"),
        },
        "AnalyzePopulation" => HistAction::AnalyzePopulation {
            sample_prob: a.double_or(1.0),
            landscape: a.int_or(0),
            save_genotypes: a.int_or(0),
            filename: a.word_or(""),
        },
        _ => return None,
    };
    Some(Ok(action))
}

/// One organism as `MakeHistogram` sees it: its genotype's genome, the
/// cell inputs (`GetOrgInterface().GetInputs()`), its current fitness and
/// merit, and (relative histograms) the parent genotype's fitness.
struct FitEntry {
    genome: Genome,
    inputs: Vec<i32>,
    fitness: f64,
    merit: Merit,
    /// `None` without parents; `Some(None)` when the parent genotype is gone.
    parent_fitness: Option<Option<f64>>,
    parents: String,
}

/// What `MakeHistogram` ends with: the histogram, or a
/// `Feedback().Error(msg)` followed by `Abort(code)`.
enum HistOutcome {
    Done(Vec<i32>),
    Abort(String, i32),
}

/// `cActionPrintLogFitnessHistogram::GetHistogramBinLabel` (`relative`
/// false) / `cActionPrintRelativeFitnessHistogram::GetHistogramBinLabel`.
/// QUIRK: the relative version counts two extra bins instead of three, so
/// its last two bins are both labelled `[>max]`.
fn bin_label(k: i32, min: f64, step: f64, max: f64, relative: bool) -> String {
    let extra = if relative { 2 } else { 3 };
    let num_bins = ((max - min) / step).ceil() as i32 + extra;
    if k == 0 {
        "Inviable".to_string()
    } else if k == 1 {
        format!("[<{}, {})", convert_double(min), convert_double(min))
    } else if k < num_bins - 1 {
        format!(
            "({}, {}]",
            convert_double(min + step * f64::from(k - 2)),
            convert_double(min + step * f64::from(k - 1))
        )
    } else {
        format!("[>{}]", convert_double(max))
    }
}

/// `MakeHistogram(orgs, gens, min, step, max, mode, world, ctx)` of the log
/// (`relative` false) or relative fitness histogram. `max` is rounded up to
/// a whole number of steps, as the C++ reference argument is.
fn make_histogram(
    tw: &mut TestWorld<'_>,
    entries: &[FitEntry],
    min: f64,
    step: f64,
    max: &mut f64,
    mode: &str,
    relative: bool,
) -> Result<HistOutcome, String> {
    let num_bins = ((*max - min) / step).ceil() as i32 + 3;
    *max = min + f64::from(num_bins - 3) * step;
    let mut histogram = vec![0i32; num_bins.max(0) as usize];
    let mut cpu = TestCpu::new(tw)?;
    let invalid_mode = if relative {
        "MakeHistogram: Invalid fitness mode requested."
    } else {
        "PrintLogFitnessHistogram::MakeHistogram: Invalid fitness mode requested."
    };
    for e in entries {
        let mut parent_fitness = 1.0;
        if relative {
            if let Some(p) = e.parent_fitness {
                parent_fitness = p.ok_or(
                    "PrintRelativeFitnessHistogram: the parent genotype is gone \
                     (C++: NULL dereference)",
                )?;
            }
        }
        let mut info = CpuTestInfo::default();
        if mode == "TEST_CPU" || mode == "ACTUAL" {
            info.use_manual_inputs(e.inputs.clone());
            cpu.test_genome(tw, &mut info, &e.genome)?;
        }
        let fitness = match mode {
            "TEST_CPU" => info.colony_fitness(),
            "CURRENT" => e.fitness,
            "ACTUAL" => {
                if info.is_viable() {
                    let gestation = info
                        .test_phenotype(0)
                        .ok_or("MakeHistogram: no test phenotype")?
                        .gestation_time;
                    e.merit.calc_fitness(gestation)
                } else {
                    0.0
                }
            }
            _ => return Ok(HistOutcome::Abort(invalid_mode.to_string(), INVALID_CONFIG)),
        };
        let bin = if relative {
            if parent_fitness <= 0.0 {
                return Ok(HistOutcome::Abort(
                    format!(
                        "PrintRelativeFitness::MakeHistogram reports a parent fitness is zero.{}",
                        e.parents
                    ),
                    INTERNAL_ERROR,
                ));
            }
            let rfitness = fitness / parent_fitness;
            if fitness == 0.0 {
                0
            } else if rfitness < min {
                1
            } else if rfitness > *max {
                num_bins - 1
            } else {
                ((fitness / parent_fitness - min) / step) as i32 + 2
            }
        } else if fitness == 0.0 {
            0
        } else if fitness.log10() < min {
            1
        } else if fitness.log10() > *max {
            num_bins - 1
        } else {
            // QUIRK: `static_cast<int>(log10(fitness) - min / step) + 2`.
            (fitness.log10() - min / step) as i32 + 2
        };
        let slot = usize::try_from(bin)
            .ok()
            .and_then(|b| histogram.get_mut(b))
            .ok_or_else(|| {
                format!(
                    "MakeHistogram: bin {bin} of a {num_bins}-bin histogram \
                     (C++: write outside the array)"
                )
            })?;
        *slot += 1;
    }
    Ok(HistOutcome::Done(histogram))
}

/// `cActionPrintPlasticGenotypeSummary::HasPlasticTasks`
fn has_plastic_tasks(task_probs: &[f64]) -> bool {
    task_probs.iter().any(|&p| p != 0.0 && p != 1.0)
}

/// `(double)Apto::StrAs(prop)`: the property's text through `strtod`.
fn str_as_double(world: &World, g: GroupRef, name: &str) -> f64 {
    strtod(&world.systematics.property(g, name).string_value())
}

impl World {
    /// `Process(ctx)` of an action of this module.
    pub(crate) fn run_hist_action(&mut self, action: &HistAction) -> Result<(), String> {
        match action {
            HistAction::DepthHistogram { kind, filename } => self.hist_depth(*kind, filename),
            HistAction::GenotypeAbundanceHistogram { filename } => {
                self.hist_genotype_abundance(filename)
            }
            HistAction::LineageCounts { id, filename } => self.hist_lineage_counts(*id, filename),
            HistAction::DominantGroupGenotypes { filename } => {
                self.hist_dominant_group_genotypes(filename)
            }
            HistAction::DetailedFitnessData {
                save_max,
                print_histo,
                hist_fmax,
                hist_fstep,
                filenames,
            } => self.hist_detailed_fitness(
                *save_max,
                *print_histo,
                *hist_fmax,
                *hist_fstep,
                filenames,
            ),
            HistAction::FitnessHistogram {
                id,
                relative,
                filename,
                mode,
                hist_fmin,
                hist_fstep,
                hist_fmax,
            } => self.hist_fitness_histogram(
                *id,
                *relative,
                filename,
                mode,
                *hist_fmin,
                *hist_fstep,
                *hist_fmax,
            ),
            HistAction::CCladeCounts { id, filename } => self.hist_cclade_counts(*id, filename),
            HistAction::CCladeFitnessHistogram {
                id,
                relative,
                filename,
                mode,
                hist_fmin,
                hist_fstep,
                hist_fmax,
            } => self.hist_cclade_fitness(
                *id,
                *relative,
                filename,
                mode,
                *hist_fmin,
                *hist_fstep,
                *hist_fmax,
            ),
            HistAction::GenomicSiteEntropy => {
                self.hist_genomic_site_entropy();
                Ok(())
            }
            HistAction::PhenotypicPlasticity {
                filename,
                num_trials,
            } => self.hist_phenotypic_plasticity(filename, *num_trials),
            HistAction::TaskProbHistogram {
                id,
                filename,
                weighted,
            } => self.hist_task_prob_histogram(*id, filename, *weighted),
            HistAction::PlasticGenotypeSummary { id, filename } => {
                self.hist_plastic_genotype_summary(*id, filename)
            }
            HistAction::GeneticDistanceData {
                id,
                creature,
                filename,
            } => self.hist_genetic_distance(*id, creature, filename),
            HistAction::PopulationDistanceData {
                creature,
                filename,
                save_genotypes,
            } => self.hist_population_distance(creature, filename, *save_genotypes),
            HistAction::TestDominant { filename } => self.hist_test_dominant(filename),
            HistAction::CalcConsensus {
                lines_saved,
                inst_set,
            } => self.hist_calc_consensus(*lines_saved, inst_set),
            HistAction::LandscapeNoop => Ok(()),
            HistAction::LandscapeEmptyFile { filename } => {
                self.out.create_with_path(filename).map(drop)
            }
            HistAction::FullLandscape { distance } => {
                if *distance < 1 {
                    // `Feedback().Error(...)` without an abort.
                    self.action_state
                        .stdout
                        .push_str("error: landscape distance must be >= 1\n");
                }
                Ok(())
            }
            HistAction::AnalyzeLandscape { filename } => self.hist_analyze_landscape(filename),
            HistAction::MutationalNeighborhood { filename } => {
                let mut df = self.out.create_with_path(filename)?;
                df.write_comment("IMPORTANT: Mutational Neighborhood is *EXPERIMENTAL*");
                df.write_comment("Output data and format is subject to change in future releases.");
                Ok(())
            }
            HistAction::AnalyzePopulation {
                sample_prob,
                landscape,
                save_genotypes,
                filename,
            } => {
                let _ = landscape;
                self.hist_analyze_population(*sample_prob, *save_genotypes, filename)
            }
        }
    }

    /// Called once before the first update, after the event list is loaded
    /// (where the C++ constructors attach data recorders).
    pub(crate) fn hist_note_recorders(&mut self) {}

    /// Called at the end of every update, after the organism statistics are
    /// gathered and before the status line.
    pub(crate) fn hist_end_update(&mut self) {}

    /// `Driver().Feedback().Error(msg)` (printed to stdout) and
    /// `Driver().Abort(code)`.
    fn hist_abort(&mut self, msg: &str, code: i32) -> Result<(), String> {
        self.action_state
            .stdout
            .push_str(&format!("error: {msg}\n"));
        self.action_state.exit_code = Some(code);
        Err(msg.to_string())
    }

    /// `ArbiterForRole("genotype")->Begin()` / `Next()`.
    fn hist_genotypes(&self) -> Vec<GroupRef> {
        self.systematics
            .arbiter_for_role("genotype")
            .map(|r| self.systematics.iter_groups(r))
            .unwrap_or_default()
    }

    fn hist_genome(&self, g: GroupRef) -> Genome {
        Genome::from_string(&self.systematics.property(g, "genome").string_value())
    }

    /// The test CPU world over the world's own random number generator, or
    /// over a fresh `AvidaRNG(0)` (`seed_zero`).
    fn hist_with_test_world<R>(
        &mut self,
        seed_zero: bool,
        f: impl FnOnce(&mut TestWorld<'_>, &mut avida_apto::OutputManager) -> Result<R, String>,
    ) -> Result<R, String> {
        let mut local_rng = AvidaRng::new(0);
        let rng = if seed_zero {
            &mut local_rng
        } else {
            &mut self.rng
        };
        let result = {
            let mut tw = make_test_world(
                &self.cfg,
                &mut self.env,
                rng,
                &self.hw,
                &mut self.stats,
                &mut self.host,
                &*self.group_attack_fn,
                self.test_sterilize,
                &self.pop,
            );
            f(&mut tw, &mut self.out)
        };
        self.flush_host();
        result
    }

    /// `cActionPrint{,Parasite,Host}DepthHistogram::Process`
    fn hist_depth(&mut self, kind: DepthKind, filename: &str) -> Result<(), String> {
        let parasitic = |w: &World, g: GroupRef| {
            let t = w
                .systematics
                .property(g, "src_transmission_type")
                .int_value();
            t == TransmissionType::Horizontal.as_int() || t == TransmissionType::Vertical.as_int()
        };
        let groups: Vec<(i32, i32)> = self
            .hist_genotypes()
            .into_iter()
            .filter(|&g| kind == DepthKind::All || parasitic(self, g))
            .map(|g| {
                (
                    self.systematics.depth(g).unwrap_or(0),
                    self.systematics.num_units(g).unwrap_or(0),
                )
            })
            .collect();
        let mut min = i32::MAX;
        let mut max = 0;
        for &(d, _) in &groups {
            min = min.min(d);
            max = max.max(d);
        }
        if kind == DepthKind::Parasite && min == i32::MAX {
            min = 0;
        }
        if max < min {
            return Err(format!(
                "depth histogram of no genotypes into '{filename}' \
                 (C++: array of size max - min + 1 < 0)"
            ));
        }
        let mut n = vec![0i32; (max - min + 1) as usize];
        for (d, units) in groups {
            n[(d - min) as usize] += units;
        }
        let update = self.stats.update();
        let df = self.out.static_with_path(filename)?;
        df.write(update, "Update");
        df.write(min, "Minimum");
        df.write(max, "Maximum");
        for v in n {
            df.write_anonymous(v);
        }
        df.endl();
        Ok(())
    }

    /// `cActionPrintGenotypeAbundanceHistogram::Process`
    fn hist_genotype_abundance(&mut self, filename: &str) -> Result<(), String> {
        let units: Vec<i32> = self
            .hist_genotypes()
            .into_iter()
            .map(|g| self.systematics.num_units(g).unwrap_or(0))
            .collect();
        let first = *units
            .first()
            .ok_or("PrintGenotypeAbundanceHistogram: no genotypes (C++: NULL dereference)")?;
        let mut hist = vec![0i32; first.max(0) as usize];
        for u in units {
            let slot = usize::try_from(u - 1)
                .ok()
                .and_then(|i| hist.get_mut(i))
                .ok_or(
                    "PrintGenotypeAbundanceHistogram: a genotype outnumbers the first one \
                     (C++: write outside the array)",
                )?;
            *slot += 1;
        }
        let update = self.stats.update();
        let df = self.out.static_with_path(filename)?;
        df.write(update, "Update");
        for v in hist {
            df.write(v, "");
        }
        df.endl();
        Ok(())
    }

    /// `cActionPrintLineageCounts::Process`
    fn hist_lineage_counts(&mut self, id: usize, filename: &str) -> Result<(), String> {
        let update = self.stats.update();
        let generation = self.stats.sum_generation.average();
        let mut counts: AptoMap<i32, i32> = AptoMap::new();
        for &cell in &self.pop.live_org_list {
            if let Some(org) = self.pop.orgs.get(cell as usize).and_then(|o| o.as_ref()) {
                *counts.get_with_default(&org.state.lineage_label, 0) += 1;
            }
        }
        let labels = self
            .actions2
            .hist
            .lineage_labels
            .entry(id)
            .or_insert_with(|| counts.keys().copied().collect())
            .clone();
        let df = self.out.static_with_path(filename)?;
        df.write(update, "Update");
        df.write(generation, "Generation");
        for label in labels {
            let count = counts.find(&label).copied().unwrap_or(0);
            df.write(count, &format!("Lineage Label {label}"));
        }
        df.endl();
        Ok(())
    }

    /// `cActionPrintDominantGroupGenotypes::Process`
    fn hist_dominant_group_genotypes(&mut self, filename: &str) -> Result<(), String> {
        let num_groups = self.pop.groups.groups.values().filter(|&&s| s > 0).count();
        let genotypes = self.hist_genotypes();
        let update = self.stats.update();
        // (genome, file name, last birth cell, group id, forager type)
        let mut jobs: Vec<(Genome, String, i32, i32, i32)> = Vec::new();
        let mut checked: Vec<i32> = Vec::new();
        let mut it = genotypes.into_iter();
        let mut i = 0usize;
        while i < num_groups {
            let Some(bg) = it.next() else { break };
            let prop = |n: &str| self.systematics.property(bg, n);
            if prop("threshold").bool_value() || i == 0 {
                let group_id = prop("last_group_id").int_value();
                let birth_cell = prop("last_birth_cell").int_value();
                let forager = prop("last_forager_type").int_value();
                if i != 0 && checked.contains(&group_id) {
                    // `already_used`: `i--`, then `continue`.
                    continue;
                }
                checked.push(group_id);
                let name = if filename.is_empty() {
                    format!(
                        "archive/grp{group_id}_ft{forager}_{}.org",
                        prop("name").string_value()
                    )
                } else {
                    format!("{filename}grp{group_id}_ft{forager}")
                };
                jobs.push((self.hist_genome(bg), name, birth_cell, group_id, forager));
            }
            i += 1;
        }
        // Each genome is printed with its own test CPU on a context with a
        // fresh `AvidaRNG(0)`.
        for (genome, name, birth_cell, group_id, forager) in jobs {
            self.hist_with_test_world(true, |tw, out| {
                let mut df = out.create_with_path(&name)?;
                let mut cpu = TestCpu::new(tw)?;
                cpu.print_genome(
                    tw, &genome, &mut df, &name, update, true, birth_cell, group_id, forager,
                )
            })?;
        }
        Ok(())
    }

    /// The occupied cells' organisms as `MakeHistogram` entries, with their
    /// coalescence clade labels.
    fn hist_fit_entries(&self) -> Vec<(i32, FitEntry)> {
        let genotype_role = self.systematics.arbiter_for_role("genotype");
        let mut out = Vec::new();
        for (cell, slot) in self.pop.orgs.iter().enumerate() {
            let Some(org) = slot.as_ref() else { continue };
            let Some(g) = self.systematics.genotype_of(&self.pop.membership[cell]) else {
                continue;
            };
            let parents = self.systematics.property(g, "parents").string_value();
            let parent_fitness = if parents.is_empty() {
                None
            } else {
                // `cStringList(parents, ',').Pop().AsInt()`
                let first = parents.split(',').next().unwrap_or("");
                let parent = genotype_role.and_then(|r| self.systematics.group(r, as_int(first)));
                Some(parent.map(|p| str_as_double(self, p, "fitness")))
            };
            out.push((
                org.state.cclade_id,
                FitEntry {
                    genome: self.hist_genome(g),
                    inputs: self.pop.cells[cell].inputs.clone(),
                    fitness: org.state.phenotype.fitness,
                    merit: org.state.phenotype.merit,
                    parent_fitness,
                    parents,
                },
            ));
        }
        out
    }

    /// `cActionPrintDetailedFitnessData::Process`
    fn hist_detailed_fitness(
        &mut self,
        save_max: i32,
        print_histo: i32,
        hist_fmax: f64,
        hist_fstep: f64,
        filenames: &[String; 3],
    ) -> Result<(), String> {
        let update = self.stats.update();
        let generation = self.stats.sum_generation.average();
        let mut orgs: Vec<(GroupRef, Genome, Merit)> = Vec::new();
        for (cell, slot) in self.pop.orgs.iter().enumerate() {
            let Some(org) = slot.as_ref() else { continue };
            let g = self
                .systematics
                .genotype_of(&self.pop.membership[cell])
                .ok_or("PrintDetailedFitnessData: organism without a genotype")?;
            orgs.push((g, self.hist_genome(g), org.state.phenotype.merit));
        }
        let bins = if print_histo != 0 {
            (hist_fmax / hist_fstep) as i32 + 1
        } else {
            0
        };
        let mut histo = vec![0i32; bins.max(0) as usize];
        let mut histo_test = vec![0i32; bins.max(0) as usize];
        let mut n = 0;
        let mut nhist_tot = 0;
        let mut nhist_tot_test = 0;
        let mut fave = 0.0;
        let mut fave_test = 0.0;
        let mut max_fitness = -1.0;
        let mut max_f: Option<usize> = None;
        // All tests, then (with `save_max`) the genome print, share one CPU.
        let names: Vec<(bool, String, i32)> = orgs
            .iter()
            .map(|(g, genome, _)| {
                (
                    self.systematics.property(*g, "threshold").bool_value(),
                    self.systematics.property(*g, "name").string_value(),
                    genome.representation().size(),
                )
            })
            .collect();
        let results = self.hist_with_test_world(false, |tw, _| {
            let mut cpu = TestCpu::new(tw)?;
            let mut results = Vec::with_capacity(orgs.len());
            for (_, genome, merit) in &orgs {
                let mut info = CpuTestInfo::default();
                cpu.test_genome(tw, &mut info, genome)?;
                let ph = info
                    .test_phenotype(0)
                    .ok_or("PrintDetailedFitnessData: no test phenotype")?;
                let f = if info.is_viable() {
                    merit.calc_fitness(ph.gestation_time)
                } else {
                    0.0
                };
                results.push((f, info.colony_fitness(), ph.copy_true));
            }
            Ok(results)
        })?;
        for (i, &(f, f_test, copy_true)) in results.iter().enumerate() {
            if f_test > max_fitness && copy_true {
                max_fitness = f_test;
                max_f = Some(i);
            }
            fave += f;
            fave_test += f_test;
            n += 1;
            if print_histo != 0 && f < hist_fmax {
                let b = (f / hist_fstep) as i32;
                *usize::try_from(b)
                    .ok()
                    .and_then(|b| histo.get_mut(b))
                    .ok_or("PrintDetailedFitnessData: fitness outside the histogram")? += 1;
                nhist_tot += 1;
            }
            if print_histo != 0 && f_test < hist_fmax {
                let b = (f_test / hist_fstep) as i32;
                *usize::try_from(b)
                    .ok()
                    .and_then(|b| histo_test.get_mut(b))
                    .ok_or("PrintDetailedFitnessData: fitness outside the histogram")? += 1;
                nhist_tot_test += 1;
            }
        }
        let max_i = max_f.ok_or(
            "PrintDetailedFitnessData: no organism copies true \
             (C++: NULL dereference of the maximum-fitness genotype)",
        )?;
        let (threshold, name, size) = &names[max_i];
        let max_f_name = if *threshold {
            name.clone()
        } else {
            format!("{size:03}-no_name-u{update}")
        };
        let nf = f64::from(n);
        {
            let df = self.out.static_with_path(&filenames[0])?;
            df.write(update, "Update");
            df.write(generation, "Generation");
            df.write(fave / nf, "Average Fitness");
            df.write(fave_test / nf, "Average Test Fitness");
            df.write(n, "Organism Total");
            df.write(max_fitness, "Maximum Fitness");
            df.write(max_f_name.as_str(), "Maxfit genotype name");
            df.endl();
        }
        if save_max != 0 {
            let filename = format!("archive/{max_f_name}");
            let genome = orgs[max_i].1.clone();
            self.hist_with_test_world(false, |tw, out| {
                let mut df = out.create_with_path(&filename)?;
                let mut cpu = TestCpu::new(tw)?;
                cpu.print_genome(tw, &genome, &mut df, &filename, -1, false, 0, -1, -1)
            })?;
        }
        if print_histo != 0 {
            for (file, h, tot) in [
                (&filenames[1], &histo, nhist_tot),
                (&filenames[2], &histo_test, nhist_tot_test),
            ] {
                let df = self.out.static_with_path(file)?;
                df.write(update, "Update");
                df.write(generation, "Generation");
                df.write(fave / nf, "Average Fitness");
                for &v in h {
                    df.write_anonymous(f64::from(v) / f64::from(tot));
                }
                df.endl();
            }
        }
        Ok(())
    }

    /// `cActionPrintLogFitnessHistogram::Process` /
    /// `cActionPrintRelativeFitnessHistogram::Process`
    #[allow(clippy::too_many_arguments)]
    fn hist_fitness_histogram(
        &mut self,
        id: usize,
        relative: bool,
        filename: &str,
        mode: &str,
        min: f64,
        step: f64,
        initial_max: f64,
    ) -> Result<(), String> {
        let mut max = *self
            .actions2
            .hist
            .hist_fmax
            .get(&id)
            .unwrap_or(&initial_max);
        if !relative {
            let params = format!(
                "Parameters: {filename}, {mode}, {}:{}:{}",
                fmt_double(min),
                fmt_double(step),
                fmt_double(max)
            );
            if (mode != "ACTUAL" && mode != "CURRENT" && mode != "TESTCPU") || min > max {
                eprintln!("cActionPrintFitnessHistogram: Please check arguments.  Abort.");
                eprintln!("{params}");
                return Ok(());
            }
            eprintln!("{params}");
        }
        let update = self.stats.update();
        let generation = self.stats.sum_generation.average();
        let entries: Vec<FitEntry> = self.hist_fit_entries().into_iter().map(|e| e.1).collect();
        let outcome = self.hist_with_test_world(false, |tw, _| {
            make_histogram(tw, &entries, min, step, &mut max, mode, relative)
        })?;
        self.actions2.hist.hist_fmax.insert(id, max);
        let histogram = match outcome {
            HistOutcome::Done(h) => h,
            HistOutcome::Abort(msg, code) => return self.hist_abort(&msg, code),
        };
        let df = self.out.static_with_path(filename)?;
        df.write(update, "Update");
        df.write(generation, "Generation");
        for (k, &v) in histogram.iter().enumerate() {
            df.write(v, &bin_label(k as i32, min, step, max, relative));
        }
        df.endl();
        Ok(())
    }

    /// `cActionPrintCCladeCounts::Process`
    fn hist_cclade_counts(&mut self, id: usize, filename: &str) -> Result<(), String> {
        if self.cfg.track_cclades == 0 {
            return self.hist_abort(
                "PrintCCladeCount requires coalescence clade tracking to be enabled.",
                INVALID_CONFIG,
            );
        }
        let mut counts: AptoMap<i32, i32> = AptoMap::new();
        let mut clade_ids: BTreeSet<i32> = BTreeSet::new();
        for org in self.pop.orgs.iter().flatten() {
            let cclade = org.state.cclade_id;
            clade_ids.insert(cclade);
            *counts.get_with_default(&cclade, 0) += 1;
        }
        let update = self.stats.update();
        let mut text = String::new();
        if self.actions2.hist.started.insert(id) {
            text.push_str("# Each line is formatted as follows:\n");
            text.push_str("#   update number_cclades ccladeID0 ccladeID0_count ccladeID1\n\n");
        }
        text.push_str(&format!("{update} {} ", clade_ids.len()));
        for c in &clade_ids {
            let count = counts.find(c).copied().unwrap_or(0);
            text.push_str(&format!("{c} {count} "));
        }
        text.push('\n');
        self.hist_static_raw(filename, &text)
    }

    /// Raw text written to a static file through `OFStream()`, ending in
    /// `endl` (flushed).
    fn hist_static_raw(&mut self, filename: &str, text: &str) -> Result<(), String> {
        let df = self.out.static_with_path(filename)?;
        df.ofstream()
            .write_all(text.as_bytes())
            .map_err(|e| e.to_string())?;
        df.flush();
        Ok(())
    }

    /// `cActionPrintCCladeFitnessHistogram::Process` /
    /// `cActionPrintCCladeRelativeFitnessHistogram::Process`
    #[allow(clippy::too_many_arguments)]
    fn hist_cclade_fitness(
        &mut self,
        id: usize,
        relative: bool,
        filename: &str,
        mode: &str,
        min: f64,
        step: f64,
        initial_max: f64,
    ) -> Result<(), String> {
        let action = if relative {
            "PrintCCladeRelativeFitnessHistogram"
        } else {
            "PrintCCladeFitnessHistogram"
        };
        if self.cfg.track_cclades == 0 {
            return self.hist_abort(
                &format!("{action} requires coalescence clade tracking to be enabled."),
                INVALID_CONFIG,
            );
        }
        let mut max = *self
            .actions2
            .hist
            .hist_fmax
            .get(&id)
            .unwrap_or(&initial_max);
        if (mode != "ACTUAL" && mode != "CURRENT" && mode != "TESTCPU") || min > max {
            let msg = if relative {
                "PrintCCladeRelativeFitness: check parameters"
            } else {
                "PrintCCladeFitnessHistogram: Check parameters."
            };
            return self.hist_abort(msg, INVALID_CONFIG);
        }
        let update = self.stats.update();
        let mut clades: BTreeMap<i32, Vec<FitEntry>> = BTreeMap::new();
        for (cclade, e) in self.hist_fit_entries() {
            clades.entry(cclade).or_default().push(e);
        }
        let num_clades = clades.len();
        let mut first_run = !self.actions2.hist.started.contains(&id);
        let outcome = self.hist_with_test_world(false, |tw, out| {
            let df = out.static_with_path(filename)?;
            let mut text = String::new();
            for (i, (clade, entries)) in clades.iter().enumerate() {
                let hist = match make_histogram(tw, entries, min, step, &mut max, mode, relative)? {
                    HistOutcome::Done(h) => h,
                    abort => {
                        write_flush(df, &text)?;
                        return Ok(abort);
                    }
                };
                if first_run {
                    first_run = false;
                    text.push_str("# PrintCCladeFitnessHistogram\n# Bins: ");
                    for k in 0..hist.len() {
                        text.push(' ');
                        text.push_str(&bin_label(k as i32, min, step, max, relative));
                    }
                    text.push_str("\n\n");
                }
                if i == 0 {
                    text.push_str(&format!("{update} {num_clades} "));
                }
                text.push_str(&format!("{clade} ["));
                for v in &hist {
                    text.push_str(&format!(" {v}"));
                }
                text.push_str(" ] ");
            }
            text.push('\n');
            write_flush(df, &text)?;
            Ok(HistOutcome::Done(Vec::new()))
        })?;
        self.actions2.hist.hist_fmax.insert(id, max);
        if !first_run {
            self.actions2.hist.started.insert(id);
        }
        match outcome {
            HistOutcome::Done(_) => Ok(()),
            HistOutcome::Abort(msg, code) => self.hist_abort(&msg, code),
        }
    }

    /// `cActionPrintGenomicSiteEntropy::Process`. In a run the population's
    /// genomes are aligned and per-site entropies computed, but nothing is
    /// written (the C++ never opens `m_filename`); only an empty population
    /// is reported. QUIRK: the C++ counts instructions at
    /// `Instruction(ch).GetOp()` of the genome *characters* and reads the
    /// uninitialised `m_use_gap`, writing outside its arrays; none of it is
    /// observable here.
    fn hist_genomic_site_entropy(&mut self) {
        if self.pop.orgs.iter().all(Option::is_none) {
            // `StdIOFeedback::Notify`
            self.action_state
                .stdout
                .push_str("cActionPrintGenomicSiteEntropy: No sequences available.  Abort.\n");
        }
    }

    /// `cPhenPlastUtil::...(ctx, world, bg)`: the genotype's attached
    /// `cPhenPlastSummary`, made by `TestPlasticity` (a `cPhenPlastGenotype`
    /// of `GENOTYPE_PHENPLAST_CALC` trials) the first time it is asked for.
    fn hist_phenplast_summary(&mut self, g: GroupRef) -> Result<PhenPlastSummary, String> {
        let gid = self.systematics.group_id(g).unwrap_or(-1);
        if let Some(s) = self.host.phenplast.get(&gid) {
            return Ok(s.clone());
        }
        let genome = self.hist_genome(g);
        let trials = self.cfg.genotype_phenplast_calc;
        let summary = self.hist_with_test_world(false, |tw, _| {
            let mut info = CpuTestInfo::default();
            PhenPlastGenotype::new(tw, &genome, trials, &mut info)
                .map(|pp| pp.summary())
                .map_err(|e| e.to_string())
        })?;
        self.host.phenplast.insert(gid, summary.clone());
        Ok(summary)
    }

    /// `cActionPrintPhenotypicPlasticity::Process` (run mode). QUIRK: the
    /// file is `m_filename` itself; the `-<update>.dat` path is built and
    /// never used.
    fn hist_phenotypic_plasticity(
        &mut self,
        filename: &str,
        num_trials: i32,
    ) -> Result<(), String> {
        let mut text = String::new();
        text.push_str("# Phenotypic Plasticity\n# Format: \n# genotype id\n");
        text.push_str("# parent genotype id\n# phenotypic varient number\n");
        text.push_str("# varient frequency\n# fitness\n# merit\n# gestation time\n");
        for k in 0..self.env.num_tasks() {
            text.push_str(&format!("# task.{k}\n"));
        }
        for k in 0..self.env.input_size() {
            text.push_str(&format!("# env_input.{k}\n"));
        }
        text.push('\n');
        let genotypes: Vec<(i32, String, Genome)> = self
            .hist_genotypes()
            .into_iter()
            .map(|g| {
                (
                    self.systematics.group_id(g).unwrap_or(-1),
                    self.systematics.property(g, "parents").string_value(),
                    self.hist_genome(g),
                )
            })
            .collect();
        let mut df = self.out.create_with_path(filename)?;
        let result = self.hist_with_test_world(false, |tw, _| {
            // One `cCPUTestInfo` for the whole call.
            let mut info = CpuTestInfo::default();
            for (gid, parents, genome) in &genotypes {
                let pp = PhenPlastGenotype::new(tw, genome, num_trials, &mut info)
                    .map_err(|e| e.to_string())?;
                for (k, p) in pp.unique.iter().enumerate() {
                    let ph = &p.phenotype;
                    text.push_str(&format!(
                        "{gid} {parents} {k} {} {} {} {} ",
                        fmt_double(p.frequency()),
                        fmt_double(ph.fitness),
                        fmt_double(ph.merit.get_double()),
                        ph.gestation_time
                    ));
                    for t in &ph.last_task_count {
                        text.push_str(&format!("{t} "));
                    }
                    for e in &p.env_inputs {
                        text.push_str(&format!("{e} "));
                    }
                    text.push('\n');
                }
            }
            Ok(())
        });
        write_flush(&mut df, &text)?;
        result
    }

    /// `cActionPrintTaskProbHistogram::Process` (run mode)
    fn hist_task_prob_histogram(
        &mut self,
        id: usize,
        filename: &str,
        weighted: bool,
    ) -> Result<(), String> {
        let num_tasks = self.env.num_tasks().max(0) as usize;
        let mut bins = vec![[0i32; 22]; num_tasks];
        let mut text = String::new();
        if self.actions2.hist.started.insert(id) {
            text.push_str("# Task Probability Histogram\n");
            text.push_str(
                "#format update task_id [0] (0,0.5] (0.5,0.10] ... (0.90,0.95], (0.95, 1.0], [1.0]\n\n",
            );
        }
        // The file is opened (and the header written) before the genotypes
        // are tested.
        self.out.static_with_path(filename)?;
        for g in self.hist_genotypes() {
            let weight = if weighted {
                self.systematics.num_units(g).unwrap_or(0)
            } else {
                1
            };
            let summary = self.hist_phenplast_summary(g)?;
            for (k, &p) in summary.task_probabilities.iter().enumerate() {
                let bin = if p < 1.0 {
                    (p * 100.0 / 5.0).ceil() as i32
                } else {
                    21
                };
                let slot = bins
                    .get_mut(k)
                    .and_then(|row| usize::try_from(bin).ok().and_then(|b| row.get_mut(b)))
                    .ok_or("PrintTaskProbHistogram: bin outside the matrix")?;
                *slot += weight;
            }
        }
        let update = self.stats.update();
        for (t, row) in bins.iter().enumerate() {
            text.push_str(&format!("{update} {t} "));
            let cells: Vec<String> = row.iter().map(i32::to_string).collect();
            text.push_str(&cells.join(" "));
            text.push('\n');
        }
        self.hist_static_raw(filename, &text)
    }

    /// `cActionPrintPlasticGenotypeSummary::Process` (run mode). QUIRK: the
    /// entropy arrays are sized by `num_genotypes`, which stays 0 in run
    /// mode, so the C++ writes through a null array as soon as a genotype
    /// is plastic; that case fails here. Otherwise the genotype count is
    /// printed as 0 and both medians as -1.
    fn hist_plastic_genotype_summary(&mut self, id: usize, filename: &str) -> Result<(), String> {
        let mut text = String::new();
        if self.actions2.hist.started.insert(id) {
            text.push_str("# Plastic Genotype Sumary\n");
            text.push_str("#format  update num_genotypes num_plastic_genotypes num_gen_taskplast num_orgs num_plastic_orgs num_org_taskplast median_phenplast_entropy median_taskplast_entropy\n");
            text.push_str("# update\n");
            text.push_str("# num_genotypes              Number of genotypes in the population.\n");
            text.push_str(
                "# num_plastic_genotypes      Number of genotypes that show any plasticity.\n",
            );
            text.push_str(
                "# num_gen_taskplast          Number of genotypes that show task plasticity.\n",
            );
            text.push_str("# num_orgs                   Number of organisms in the population.\n");
            text.push_str(
                "# num_plastic_orgs           Number of organisms that show any plasticity.\n",
            );
            text.push_str(
                "# num_org_taskplast          Number of organisms that show task plasticity.\n",
            );
            text.push_str("# median_phenplast_entropy   Median entropy of plastic genotypes.\n");
            text.push_str(
                "# median_taskplast_entropy   Median entropy of task-plastic genotypes.\n\n",
            );
        }
        self.out.static_with_path(filename)?;
        let mut num_orgs = 0;
        for g in self.hist_genotypes() {
            num_orgs += self.systematics.num_units(g).unwrap_or(0);
            let summary = self.hist_phenplast_summary(g)?;
            if summary.num_phenotypes > 1 {
                let _ = has_plastic_tasks(&summary.task_probabilities);
                return Err("PrintPlasticGenotypeSummary: a plastic genotype in a run \
                            (C++: write into an array of size 0)"
                    .to_string());
            }
        }
        let update = self.stats.update();
        text.push_str(&format!(
            "{update} 0 0 0 {num_orgs} 0 0 {} {}\n",
            fmt_double(-1.0),
            fmt_double(-1.0)
        ));
        self.hist_static_raw(filename, &text)
    }

    /// `cActionPrintGeneticDistanceData::Process`
    fn hist_genetic_distance(
        &mut self,
        id: usize,
        creature: &str,
        filename: &str,
    ) -> Result<(), String> {
        if !self.actions2.hist.references.contains_key(&id) {
            let genome = self.load_genome(creature, "", false)?.ok_or_else(|| {
                format!(
                    "PrintGeneticDistanceData: cannot load '{creature}' \
                     (C++: NULL dereference in the constructor)"
                )
            })?;
            self.actions2.hist.references.insert(id, genome);
        }
        let r_seq = self.actions2.hist.references[&id].representation().clone();
        let genotypes = self.hist_genotypes();
        let best = *genotypes
            .first()
            .ok_or("PrintGeneticDistanceData: no genotypes (C++: NULL dereference)")?;
        let best_seq = self.hist_genome(best).representation().clone();
        let mut hamming_m1 = 0.0;
        let mut hamming_m2 = 0.0;
        let mut count = 0;
        for g in genotypes {
            let seq = self.hist_genome(g).representation().clone();
            let dist = InstructionSequence::find_hamming_distance(&r_seq, &seq, 0);
            hamming_m1 += f64::from(dist);
            hamming_m2 += f64::from(dist.wrapping_mul(dist));
            count += self.systematics.num_units(g).unwrap_or(0);
        }
        let count = f64::from(count);
        hamming_m1 /= count;
        hamming_m2 /= count;
        let hamming_best = f64::from(InstructionSequence::find_hamming_distance(
            &r_seq, &best_seq, 0,
        ));
        let update = self.stats.update();
        let df = self.out.static_with_path(filename)?;
        df.write(update, "Update");
        df.write(hamming_m1, "Average Hamming Distance");
        df.write(
            ((hamming_m2 - hamming_m1 * hamming_m1) / count).sqrt(),
            "Standard Error",
        );
        df.write(hamming_best, "Best Genotype Hamming Distance");
        df.endl();
        Ok(())
    }

    /// `cActionPrintPopulationDistanceData::Process`. QUIRK: the file is
    /// created from `m_filename`, not from the `pop_distance-<update>.dat`
    /// default built next to it (an empty name makes the C++ crash), and
    /// each genotype's `fitness` property is undefined (read as 0).
    fn hist_population_distance(
        &mut self,
        creature: &str,
        filename: &str,
        save_genotypes: i32,
    ) -> Result<(), String> {
        if filename.is_empty() {
            return Err(
                "unsupported: PrintPopulationDistanceData without a file name \
                 (C++: Output::File::CreateWithPath(\"\") crashes)"
                    .to_string(),
            );
        }
        let mut df = self.out.create_with_path(filename)?;
        let Some(reference) = self.load_genome(creature, "", false)? else {
            return Ok(());
        };
        let r_seq = reference.representation().clone();
        let mut sum_fitness = 0.0;
        let mut sum_num_organisms = 0;
        for g in self.hist_genotypes() {
            let genome = self.hist_genome(g);
            let seq = genome.representation();
            let num_orgs = self.systematics.num_units(g).unwrap_or(0);
            let fitness = str_as_double(self, g, "fitness");
            let name = self.systematics.property(g, "name").string_value();
            sum_fitness += fitness * f64::from(num_orgs);
            sum_num_organisms += num_orgs;
            df.write(name.as_str(), "Genotype Name");
            df.write(fitness, "Fitness");
            df.write(num_orgs, "Abundance");
            df.write(
                InstructionSequence::find_hamming_distance(&r_seq, seq, 0),
                "Hamming distance to reference",
            );
            df.write(
                InstructionSequence::find_edit_distance(&r_seq, seq),
                "Levenstein distance to reference",
            );
            df.write(genome.as_string(), "Genome");
            if save_genotypes != 0 {
                let path = format!("archive/{name}.org");
                self.hist_with_test_world(false, |tw, out| {
                    let mut gdf = out.create_with_path(&path)?;
                    let mut cpu = TestCpu::new(tw)?;
                    cpu.print_genome(tw, &genome, &mut gdf, &path, -1, false, 0, -1, -1)
                })?;
            }
            df.endl();
        }
        // QUIRK: `Stringf("...%d\n", sum_fitness / sum_num_organisms)`
        // passes a double to `%d`; on the reference platform (arm64,
        // variadic arguments in 8-byte stack slots) the low 32 bits of the
        // double are printed.
        let ave = sum_fitness / f64::from(sum_num_organisms);
        df.write_raw(&format!(
            "# ave fitness from Test CPU's: {}\n",
            ave.to_bits() as u32 as i32
        ));
        df.flush();
        Ok(())
    }

    /// `cActionTestDominant::Process`
    fn hist_test_dominant(&mut self, filename: &str) -> Result<(), String> {
        let bg = *self
            .hist_genotypes()
            .first()
            .ok_or("TestDominant: no genotypes (C++: NULL dereference)")?;
        let genome = self.hist_genome(bg);
        let length = genome.representation().size();
        let ph = self.hist_with_test_world(false, |tw, _| {
            let mut cpu = TestCpu::new(tw)?;
            let mut info = CpuTestInfo::default();
            cpu.test_genome(tw, &mut info, &genome)?;
            info.colony_organism()
                .map(|o| o.state.phenotype.clone())
                .ok_or_else(|| "TestDominant: no colony organism".to_string())
        })?;
        let update = self.stats.update();
        let df = self.out.static_with_path(filename)?;
        df.write(update, "Update");
        df.write(ph.merit.get_double(), "Merit");
        df.write(ph.gestation_time, "Gestation Time");
        df.write(ph.fitness, "Fitness");
        df.write(
            1.0 / (0.1 + f64::from(ph.gestation_time)),
            "Reproduction Rate",
        );
        df.write(length, "Genome Length");
        df.write(ph.copied_size, "Copied Size");
        df.write(ph.executed_size, "Executed Size");
        df.endl();
        Ok(())
    }

    /// `cWorld::GetAnalyze()`: the first call builds the `cAnalyze`, whose
    /// `cAnalyzeJobQueue` takes a seed from the world's generator
    /// (`GetRandom().GetInt(GetRandom().MaxSeed())`).
    fn hist_get_analyze(&mut self) {
        if !self.actions2.hist.analyze_built {
            self.actions2.hist.analyze_built = true;
            let max = self.rng.max_seed();
            let _ = self.rng.get_int(max);
        }
    }

    /// `cActionAnalyzeLandscape::Process` in a run: no genotypes are
    /// loaded, the (empty) job queue runs, and the static file gets its
    /// comment but no rows.
    fn hist_analyze_landscape(&mut self, filename: &str) -> Result<(), String> {
        self.hist_get_analyze();
        let df = self.out.static_with_path(filename)?;
        df.write_comment("Landscape analysis.  Distance results are grouped by update/depth.");
        Ok(())
    }

    /// `cActionAnalyzePopulation::Process` in a run. QUIRK: the file is
    /// created from `m_filename`, not from the `pop-analysis-<update>.dat`
    /// default built next to it (an empty name makes the C++ crash), and the
    /// landscape jobs are queued but the queue is never executed, so every
    /// row ends where the landscape statistics would start.
    fn hist_analyze_population(
        &mut self,
        sample_prob: f64,
        save_genotypes: i32,
        filename: &str,
    ) -> Result<(), String> {
        const VERBOSE_DETAILS: i32 = 3;
        if self.cfg.verbosity >= VERBOSE_DETAILS {
            self.action_state
                .stdout
                .push_str("Analyzing Population...\n");
        }
        let update = self.stats.update();
        self.hist_get_analyze();
        let skip_prob = 1.0 - sample_prob;
        // `cAnalyzeJobQueue` without worker threads runs each job as it is
        // added, inside `AddJob`'s lock; `GetSeedForJob` takes the same
        // non-recursive mutex again.
        let single_threaded = avida_analyze::worker_count(self.cfg.max_concurrency) <= 1;
        let mut sampled = Vec::new();
        for cell in 0..self.pop.orgs.len() {
            if self.pop.orgs[cell].is_none() {
                continue;
            }
            if self.rng.p(skip_prob) {
                continue;
            }
            if single_threaded {
                return Err("unsupported: AnalyzePopulation with one analyze thread \
                     (C++: cAnalyzeJobQueue::AddJob deadlocks on its own mutex in \
                     GetSeedForJob when MAX_CONCURRENCY is 1)"
                    .to_string());
            }
            sampled.push(cell);
        }
        if filename.is_empty() {
            return Err("unsupported: AnalyzePopulation without a file name \
                 (C++: Output::File::CreateWithPath(\"\") crashes)"
                .to_string());
        }
        let mut df = self.out.create_with_path(filename)?;
        for cell in sampled {
            let g = self
                .systematics
                .genotype_of(&self.pop.membership[cell])
                .ok_or("AnalyzePopulation: organism without a genotype")?;
            let (genome, fitness, lineage, neutral) = {
                let org = self.pop.orgs[cell]
                    .as_ref()
                    .ok_or("AnalyzePopulation: the organism is gone")?;
                (
                    org.state.genome().clone(),
                    org.state.phenotype.fitness,
                    org.state.lineage_label,
                    org.state.phenotype.neutral_metric,
                )
            };
            let size = genome.representation().size();
            let name = if self.systematics.property(g, "threshold").bool_value() {
                self.systematics.property(g, "name").string_value()
            } else {
                format!("{size:03}-no_name-u{update}-c{cell}")
            };
            let test_fitness = self.genome_test_metrics(g).map_or(0.0, |m| m.fitness);
            df.write(cell as i32, "Cell ID");
            df.write(name.as_str(), "Organism Name");
            df.write(size, "Genome Length");
            df.write(test_fitness, "Fitness (test-cpu)");
            df.write(fitness, "Fitness (actual)");
            df.write(lineage, "Lineage Label");
            df.write(neutral, "Neutral Metric");
            df.endl();
            if save_genotypes != 0 {
                let path = format!("archive/{name}.org");
                self.hist_with_test_world(false, |tw, out| {
                    let mut gdf = out.create_with_path(&path)?;
                    let mut cpu = TestCpu::new(tw)?;
                    cpu.print_genome(tw, &genome, &mut gdf, &path, -1, false, 0, -1, -1)
                })?;
            }
        }
        Ok(())
    }

    /// `cActionCalcConsensus::Process`.
    ///
    /// QUIRK: the genotype filter compares `Apto::StrAs(instset)` with the
    /// `cString` set name through their `const char*` conversions, i.e. as
    /// pointers, so every genotype is skipped: the instruction histograms
    /// stay empty, the consensus genome is empty, the abundance, variance
    /// and entropy files get only their line ends, and the distances are
    /// those of the population to the empty genome. The C++ then deletes its
    /// test CPU a second time and aborts after the files are written; the
    /// run fails here at the same point.
    fn hist_calc_consensus(&mut self, lines_saved: i32, inst_set: &str) -> Result<(), String> {
        let hw_type = self
            .hw
            .registry()
            .get_inst_set(inst_set)
            .ok_or_else(|| format!("CalcConsensus: unknown instruction set '{inst_set}'"))?
            .hardware_type();
        let update = self.stats.update();
        let genotypes = self.hist_genotypes();
        // Every histogram is empty: position 0 has mode -1 and count ==
        // total, so the consensus loop stops at once with no entropy.
        let con_seq = InstructionSequence::with_size(0);
        let total_entropy = 0.0;
        for f in [
            "consensus.dat",
            "consensus-abundance.dat",
            "consensus-var.dat",
            "consensus-entropy.dat",
        ] {
            self.out.static_with_path(f)?;
        }
        if lines_saved > 0 {
            for f in [
                "consensus-abundance.dat",
                "consensus-var.dat",
                "consensus-entropy.dat",
            ] {
                self.out.static_with_path(f)?.endl();
            }
        }
        let mut distance_sum = DoubleSum::new();
        for &g in &genotypes {
            let num_organisms = f64::from(self.systematics.num_units(g).unwrap_or(0));
            let seq = self.hist_genome(g).representation().clone();
            let d = f64::from(InstructionSequence::find_edit_distance(&con_seq, &seq));
            distance_sum.add_weighted(d, num_organisms);
        }
        let best = *genotypes
            .first()
            .ok_or("CalcConsensus: no genotypes (C++: NULL dereference)")?;
        let best_seq = self.hist_genome(best).representation().clone();
        let best_dist = InstructionSequence::find_edit_distance(&con_seq, &best_seq);
        let ave_dist = distance_sum.average();
        let var_dist = distance_sum.variance();
        let con_size = con_seq.size();
        let complexity_base = f64::from(con_size) - total_entropy;
        let con_name = format!("archive/{con_size:03}-consensus-u{update}.gen");
        let mg = Genome::with_inst_set(hw_type, inst_set, con_seq);
        let ph = self.hist_with_test_world(false, |tw, out| {
            let mut cpu = TestCpu::new(tw)?;
            let mut gdf = out.create_with_path(&con_name)?;
            cpu.print_genome(tw, &mg, &mut gdf, &con_name, -1, false, 0, -1, -1)?;
            let mut info = CpuTestInfo::default();
            cpu.test_genome(tw, &mut info, &mg)?;
            info.colony_organism()
                .map(|o| o.state.phenotype.clone())
                .ok_or_else(|| "CalcConsensus: no colony organism".to_string())
        })?;
        let df = self.out.static_with_path("consensus.dat")?;
        df.write(update, "Update");
        df.write(ph.merit.get_double(), "Merit");
        df.write(ph.gestation_time, "Gestation Time");
        df.write(ph.fitness, "Fitness");
        df.write(
            1.0 / (0.1 + f64::from(ph.gestation_time)),
            "Reproduction Rate",
        );
        df.write(con_size, "Length");
        df.write(ph.copied_size, "Copied Size");
        df.write(ph.executed_size, "Executed Size");
        df.write(0, "Get Births");
        df.write(0, "Breed True");
        df.write(0, "Breed In");
        df.write(0, "Abundance");
        df.write(-1, "Tree Depth");
        df.write(-1, "Genotype ID");
        df.write(0, "Age (in updates)");
        df.write(best_dist, "Best Distance");
        df.write(ave_dist, "Average Distance");
        df.write(var_dist, "Var Distance");
        df.write(total_entropy, "Total Entropy");
        df.write(complexity_base, "Complexity");
        df.endl();
        Err("CalcConsensus: the C++ deletes its test CPU twice and aborts".to_string())
    }
}

/// Raw text through `OFStream()`, flushed.
fn write_flush(df: &mut avida_apto::DataFile, text: &str) -> Result<(), String> {
    df.ofstream()
        .write_all(text.as_bytes())
        .map_err(|e| e.to_string())?;
    df.flush();
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    const CTX: ParseContext<'static> = ParseContext {
        default_inst_set: "heads_default",
    };

    fn p(name: &str, args: &str) -> HistAction {
        let mut fb = Feedback::new();
        parse(name, args, &CTX, &mut fb).unwrap().unwrap()
    }

    #[test]
    fn every_listed_name_parses() {
        for n in HIST_ACTIONS {
            assert!(parse(n, "", &CTX, &mut Feedback::new()).is_some(), "{n}");
            assert!(crate::action_library::is_registered(n), "{n}");
        }
    }

    #[test]
    fn defaults() {
        assert_eq!(
            p("PrintHostDepthHistogram", ""),
            HistAction::DepthHistogram {
                kind: DepthKind::Host,
                filename: "depth_host_histogram.dat".into()
            }
        );
        match p("PrintRelativeFitnessHistogram", "f.dat actual") {
            HistAction::FitnessHistogram {
                relative,
                mode,
                hist_fmin,
                hist_fstep,
                hist_fmax,
                ..
            } => {
                assert!(relative);
                assert_eq!(mode, "ACTUAL");
                assert_eq!((hist_fmin, hist_fstep, hist_fmax), (0.0, 0.1, 2.0));
            }
            other => panic!("{other:?}"),
        }
        assert_eq!(
            p("CalcConsensus", " 5 "),
            HistAction::CalcConsensus {
                lines_saved: 5,
                inst_set: "heads_default".into()
            }
        );
    }

    #[test]
    fn bin_labels() {
        assert_eq!(bin_label(0, -3.0, 0.5, 12.0, false), "Inviable");
        assert_eq!(
            bin_label(1, -3.0, 0.5, 12.0, false),
            "[<-3.000000, -3.000000)"
        );
        assert_eq!(
            bin_label(2, -3.0, 0.5, 12.0, false),
            "(-3.000000, -2.500000]"
        );
        assert_eq!(bin_label(32, -3.0, 0.5, 12.0, false), "[>12.000000]");
        // The relative labels count one bin fewer.
        assert_eq!(bin_label(21, 0.0, 0.1, 2.0, true), "[>2.000000]");
    }
}
