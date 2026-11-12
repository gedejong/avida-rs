//! The action library as analyze mode sees it (`cActionLibrary`,
//! avida-core/source/actions/*.cc at 47f13dadb, run through
//! `cAnalyzeCommandAction`), and the actions analyze mode runs itself:
//! `VERBOSE` / `SetVerbose` and the landscape actions of
//! LandscapeActions.cc that the test CPU supports.
//!
//! Every other registered action goes to [`AnalyzeHost::run_action`],
//! whose default fails loudly.

use std::io::Write;

use avida_apto::DataFile;
use avida_config::string_util as su;
use avida_config::Feedback;
use avida_hardware::test_cpu::TestWorld;

use crate::analyze::Analyze;
use crate::error::{AnalyzeError, Result};
use crate::host::{AnalyzeHost, CpuTestOptions, TestContext};
use crate::landscape::Landscape;
use crate::mutational_neighborhood::MutationalNeighborhood;
use crate::verbose::{self, VERBOSE_DETAILS, VERBOSE_ON, VERBOSE_SILENT};

/// Every name registered with `cActionLibrary` (`Register<...>("Name")` in
/// DriverActions.cc, EnvironmentActions.cc, LandscapeActions.cc,
/// PopulationActions.cc, PrintActions.cc and SaveLoadActions.cc, in that
/// order; commented-out registrations excluded).
static ACTION_NAMES: &[&str] = &[
    "Exit",
    "ExitAveLineageLabelGreater",
    "ExitAveLineageLabelLess",
    "ExitAveGeneration",
    "ExitElapsedTime",
    "ExitDemeReplications",
    "ExitDemeResources",
    "Pause",
    "SetRandomSeed",
    "SetFracDemeTreatable",
    "DelayedDemeEvent",
    "DelayedDemeEventsPerSlots",
    "InjectResource",
    "InjectScaledResource",
    "OutflowScaledResource",
    "SetResource",
    "SetDemeResource",
    "ZeroResources",
    "SetCellResource",
    "InjectCellResource",
    "MergeResourceAcrossDemes",
    "ChangeEnvironment",
    "SetGradientResource",
    "SetGradientPlatInflow",
    "SetGradientPlatOutflow",
    "SetGradientConeInflow",
    "SetGradientConeOutflow",
    "SetGradientInflow",
    "SetGradPlatVarInflow",
    "SetPredatoryResource",
    "SetProbabilisticResource",
    "DecInflow",
    "IncInflow",
    "SetReactionValue",
    "SetReactionValueMult",
    "SetReactionInst",
    "SetReactionMinTaskCount",
    "SetReactionMaxTaskCount",
    "SetReactionTask",
    "SetResourceInflow",
    "SetPeriodicResourceInflow",
    "SetResourceOutflow",
    "SetDemeResourceInflow",
    "SetDemeResourceOutflow",
    "SetEnvironmentInputs",
    "SetEnvironmentRandomMask",
    "SetSeasonalResource",
    "SetSeasonalResource1Kyears_1To_1",
    "SetSeasonalResource10Kyears_1To_1",
    "SetPeriodicResource",
    "SetNumInstBefore0Energy",
    "SetTaskArgInt",
    "SetTaskArgDouble",
    "SetTaskArgString",
    "SetOptimizeMinMax",
    "SetDemeIOGrid",
    "SetMigrationMatrix",
    "AlterMigrationConnection",
    "SetConfig",
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
    "Inject",
    "InjectRandom",
    "InjectWholeGenomeDuplications",
    "InjectWholeGenomeDuplicationsNopCPrepend",
    "InjectAllRandomRepro",
    "InjectAll",
    "InjectRange",
    "InjectSequence",
    "InjectWellMixed",
    "InjectSequenceWDivMutRate",
    "InjectDemes",
    "InjectModuloDemes",
    "InjectDemesFromNest",
    "InjectDemesRandom",
    "InjectGroup",
    "InjectParasite",
    "InjectParasiteSequence",
    "InjectParasitePair",
    "KillInstLimit",
    "KillInstPair",
    "KillProb",
    "ClearParasites",
    "ApplyBottleneck",
    "KillDominantGenotype",
    "KillProbSequence",
    "AttackDen",
    "RaidDen",
    "KillFractionInSequence",
    "KillFractionInSequence_PopLimit",
    "ToggleRewardInstruction",
    "ToggleFitnessValley",
    "KillRate",
    "KillRectangle",
    "SerialTransfer",
    "ReplaceFromGermline",
    "SetMigrationRate",
    "SetMutProb",
    "ModMutProb",
    "ZeroMuts",
    "TrackAllMessages",
    "CompeteDemes",
    "ReplicateDemes",
    "DivideDemes",
    "ResetDemes",
    "CopyDeme",
    "ReplicateDeme",
    "MixPopulation",
    "DecayPoints",
    "UpdateDemeParasiteMemoryScores",
    "Flash",
    "SetRedundancy",
    "SetProbFail",
    "CompeteDemes_AttackKillAndEnergyConserve",
    "AssignRandomCellData",
    "MeasureDemeNetworks",
    "DistributeData",
    "DistributeDataEfficiently",
    "DistributeDataCheaply",
    "CompeteDemesByNetwork",
    "IteratedConsensus",
    "CountOpinions",
    "CountMultipleOpinions",
    "DemeReactionDiversity",
    "DemeBalanceTwoTasks",
    "Synchronization",
    "Desynchronization",
    "PhenotypeMatch",
    "UnitFitness",
    "NewTrial",
    "CompeteOrganisms",
    "SeverGridCol",
    "SeverGridRow",
    "JoinGridCol",
    "JoinGridRow",
    "ConnectCells",
    "DisconnectCells",
    "SwapCells",
    "SwapRandomCells",
    "CompeteDemesByTaskCount",
    "CompeteDemesByTaskCountAndEfficiency",
    "CompeteDemesByEnergyDistribution",
    "CompeteDemesByMerit",
    "Pred_DemeEventMoveCenter",
    "Pred_DemeEventMoveBetweenTargets",
    "Pred_DemeEventNUniqueIndividualsMovedIntoTarget",
    "DemeResourceThresholdPredicate",
    "KillNBelowResourceThreshold",
    "KillWithinRadiusBelowResourceThreshold",
    "KillWithinRadiusMeanBelowResourceThreshold",
    "KillWithinRadiusBelowResourceThresholdTestAll",
    "KillDemePercent",
    "KillDemesHighestParasiteLoad",
    "ReplicateDemesHighestBirthCount",
    "ReplicateDemesHighestFecundity",
    "KillMeanBelowThresholdPaintable",
    "DiffuseHGTGenomeFragments",
    "AvidianConjugation",
    "PrintMiniTraces",
    "PrintMicroTraces",
    "LoadMiniTraceQ",
    "PrintReproData",
    "PrintTopNavTrace",
    "PrintNavTrace",
    "FlushTopNavTrace",
    "RemovePredators",
    "PrintDebug",
    "PrintAverageData",
    "PrintDemeAverageData",
    "PrintFlowRateTuples",
    "PrintErrorData",
    "PrintVarianceData",
    "PrintCountData",
    "PrintMessageData",
    "PrintMessageLog",
    "PrintRetMessageLog",
    "PrintInterruptData",
    "PrintTotalsData",
    "PrintThreadsData",
    "PrintTasksData",
    "PrintSoloTaskSnapshot",
    "PrintHostTasksData",
    "PrintParasiteTasksData",
    "PrintTasksExeData",
    "PrintNewTasksData",
    "PrintNewReactionData",
    "PrintNewTasksDataPlus",
    "PrintTasksQualData",
    "PrintResourceData",
    "PrintSpatialResources",
    "PrintResourceLocData",
    "PrintResWallLocData",
    "PrintReactionData",
    "PrintReactionExeData",
    "PrintCurrentReactionData",
    "PrintReactionRewardData",
    "PrintCurrentReactionRewardData",
    "PrintTimeData",
    "PrintExtendedTimeData",
    "PrintMutationRateData",
    "PrintDivideMutData",
    "PrintParasiteData",
    "PrintNumDivides",
    "PrintPreyAverageData",
    "PrintPredatorAverageData",
    "PrintTopPredatorAverageData",
    "PrintPreyErrorData",
    "PrintPredatorErrorData",
    "PrintTopPredatorErrorData",
    "PrintPreyVarianceData",
    "PrintPredatorVarianceData",
    "PrintTopPredatorVarianceData",
    "PrintPreyInstructionData",
    "PrintPredatorInstructionData",
    "PrintTopPredatorInstructionData",
    "PrintPreyFromSensorInstructionData",
    "PrintPredatorFromSensorInstructionData",
    "PrintTopPredatorFromSensorInstructionData",
    "PrintGroupAttackData",
    "PrintKilledPreyFTData",
    "PrintAttacks",
    "PrintFromMessageInstructionData",
    "PrintMaleInstructionData",
    "PrintFemaleInstructionData",
    "PrintSenseData",
    "PrintSenseExeData",
    "PrintInstructionData",
    "PrintInternalTasksData",
    "PrintInternalTasksQualData",
    "PrintSleepData",
    "PrintCompetitionData",
    "PrintDynamicMaxMinData",
    "PrintMaleAverageData",
    "PrintFemaleAverageData",
    "PrintMaleErrorData",
    "PrintFemaleErrorData",
    "PrintMaleVarianceData",
    "PrintFemaleVarianceData",
    "PrintCellVisitsData",
    "PrintPhenotypeData",
    "PrintParasitePhenotypeData",
    "PrintHostPhenotypeData",
    "PrintPhenotypeStatus",
    "PrintDemeTestamentStats",
    "PrintCurrentMeanDemeDensity",
    "PrintPredicatedMessages",
    "PrintCellData",
    "PrintConsensusData",
    "PrintSimpleConsensusData",
    "PrintCurrentOpinions",
    "PrintOpinionsSetPerDeme",
    "PrintSynchronizationData",
    "PrintCurrentMeanDemeDensity",
    "PrintPredicatedMessages",
    "PrintCellData",
    "PrintConsensusData",
    "PrintSimpleConsensusData",
    "PrintCurrentOpinions",
    "PrintOpinionsSetPerDeme",
    "PrintSynchronizationData",
    "PrintDetailedSynchronizationData",
    "PrintDonationStats",
    "PrintKaboom",
    "PrintQuorum",
    "PrintDemeAllStats",
    "PrintDemeStats",
    "PrintDemesTotalAvgEnergy",
    "PrintDemeEnergySharingStats",
    "PrintDemeEnergyDistributionStats",
    "PrintDemeDonorStats",
    "PrintDemeSpacialEnergyStats",
    "PrintDemeSpacialSleepStats",
    "PrintDemeResourceStats",
    "PrintDemeGlobalResources",
    "PrintDemeReplicationData",
    "PrintDemeGermlineSequestration",
    "PrintDemeOrgGermlineSequestration",
    "PrintDemeGLSFounders",
    "PrintDemeReactionDiversityReplicationData",
    "PrintDemeGermResourcesData",
    "PrintWinningDeme",
    "PrintDemeTreatableReplicationData",
    "PrintDemeUntreatableReplicationData",
    "PrintDemeTreatableCount",
    "PrintDemeFitnessData",
    "PrintDemeLifeFitnessData",
    "PrintDemeTasksData",
    "PrintDemeCompetitionData",
    "PrintDemeNetworkData",
    "PrintDemeNetworkTopology",
    "PrintDemeFoundersData",
    "PrintGermlineData",
    "SaveDemeFounders",
    "PrintPerDemeTasksData",
    "PrintPerDemeTasksExeData",
    "PrintAvgDemeTasksExeData",
    "PrintAvgTreatableDemeTasksExeData",
    "PrintAvgUntreatableDemeTasksExeData",
    "PrintPerDemeReactionData",
    "PrintDemeTasksData",
    "PrintDemeTasksExeData",
    "PrintDemeReactionData",
    "PrintDemeOrgTasksData",
    "PrintDemeOrgTasksExeData",
    "PrintDemeOrgReactionData",
    "PrintDemeCurrentTaskExeData",
    "PrintCurrentTaskCounts",
    "PrintPerDemeGenPerFounderData",
    "PrintDemeMigrationSuicidePoints",
    "PrintDemesTasksData",
    "PrintDemesReactionsData",
    "PrintDemesMeritsData",
    "PrintDemesFitnessData",
    "PrintMultiProcessData",
    "PrintProfilingData",
    "PrintOrganismLocation",
    "PrintOrgLocData",
    "PrintPreyFlockingData",
    "PrintOrgGuardData",
    "PrintAgePolyethismData",
    "PrintIntrinsicTaskSwitchingCostData",
    "PrintDenData",
    "PrintCCladeCounts",
    "PrintCCladeFitnessHistogram",
    "PrintCCladeRelativeFitnessHistogram",
    "PrintData",
    "PrintInstructionAbundanceHistogram",
    "PrintDepthHistogram",
    "PrintParasiteDepthHistogram",
    "PrintHostDepthHistogram",
    "Echo",
    "PrintGenotypeAbundanceHistogram",
    "PrintLineageCounts",
    "PrintDominantGenotype",
    "PrintDominantGroupGenotypes",
    "PrintDominantForagerGenotypes",
    "PrintDetailedFitnessData",
    "PrintLogFitnessHistogram",
    "PrintRelativeFitnessHistogram",
    "PrintGeneticDistanceData",
    "PrintPopulationDistanceData",
    "PrintPhenotypicPlasticity",
    "PrintTaskProbHistogram",
    "PrintPlasticGenotypeSummary",
    "TestDominant",
    "PrintTaskSnapshot",
    "PrintViableTasksData",
    "PrintAveNumTasks",
    "PrintGenomicSiteEntropy",
    "DumpClassificationIDGrid",
    "DumpFitnessGrid",
    "DumpGenotypeColorGrid",
    "DumpPhenotypeIDGrid",
    "DumpIDGrid",
    "DumpVitalityGrid",
    "DumpTargetGrid",
    "DumpMaxResGrid",
    "DumpTaskGrid",
    "DumpLastTaskGrid",
    "DumpHostTaskGrid",
    "DumpParasiteTaskGrid",
    "DumpHostTaskGridComma",
    "DumpParasiteTaskGridComma",
    "DumpParasiteVirulenceGrid",
    "PrintMigrationMatrix",
    "DumpOffspringMigrationCounts",
    "DumpParasiteMigrationCounts",
    "DumpReactionGrid",
    "PrintLastReactionCountGrid",
    "PrintCurrReactionCountGrid",
    "DumpDonorGrid",
    "DumpReceiverGrid",
    "DumpEnergyGrid",
    "DumpExecutionRatioGrid",
    "DumpCellDataGrid",
    "DumpSleepGrid",
    "DumpGenomeLengthGrid",
    "DumpGenotypeGrid",
    "DumpParasiteGenotypeGrid",
    "DumpHostGenotypeList",
    "DumpParasiteGenotypeList",
    "PrintNumOrgsKilledData",
    "PrintMigrationData",
    "PrintReputationData",
    "PrintDirectReciprocityData",
    "PrintStringMatchData",
    "PrintShadedAltruists",
    "PrintGroupsFormedData",
    "PrintGroupIds",
    "PrintGroupTolerance",
    "PrintGroupMTTolerance",
    "PrintToleranceInstructionData",
    "PrintToleranceData",
    "PrintTargets",
    "PrintMimicDisplays",
    "PrintTopPredTargets",
    "PrintHGTData",
    "SetVerbose",
    "VERBOSE",
    "PrintNumOrgsInDeme",
    "CalcConsensus",
    "PrintEditDistance",
    "PrintMatingTypeHistogram",
    "PrintMatingDisplayData",
    "PrintFemaleMatePreferenceData",
    "PrintBirthChamberMatingTypeHistogram",
    "PrintSuccessfulMates",
    "PrintBirthChamber",
    "PrintDominantData",
    "LoadParasiteGenotypeList",
    "LoadHostGenotypeList",
    "LoadPopulation",
    "SavePopulation",
    "LoadGermlines",
    "SaveGermlines",
    "LoadBirthCounts",
    "LoadParasiteMemoryScores",
    "LoadStructuredSystematicsGroup",
    "SaveStructuredSystematicsGroup",
    "SaveFlameData",
];

/// `cActionLibrary::GetInstance().Supports(name)`: the factory keys are
/// lower-cased, so the match ignores case.
pub fn is_registered_action(name: &str) -> bool {
    ACTION_NAMES.iter().any(|n| n.eq_ignore_ascii_case(name))
}

/// The registered actions analyze mode runs without the host.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum NativeAction {
    Verbose,
    AnalyzeLandscape,
    PrecalcLandscape,
    FullLandscape,
    DumpLandscape,
    DeletionLandscape,
    InsertionLandscape,
    RandomLandscape,
    SampleLandscape,
    MutationalNeighborhood,
    PredictWLandscape,
    PredictNuLandscape,
    HillClimb,
    PairTestLandscape,
    AnalyzePopulation,
}

/// Which native action `name` is (case-insensitive, as the factory).
pub(crate) fn native_action(name: &str) -> Option<NativeAction> {
    let n = name.to_ascii_lowercase();
    Some(match n.as_str() {
        "verbose" | "setverbose" => NativeAction::Verbose,
        "analyzelandscape" => NativeAction::AnalyzeLandscape,
        "precalclandscape" => NativeAction::PrecalcLandscape,
        "fulllandscape" => NativeAction::FullLandscape,
        "dumplandscape" => NativeAction::DumpLandscape,
        "deletionlandscape" => NativeAction::DeletionLandscape,
        "insertionlandscape" => NativeAction::InsertionLandscape,
        "randomlandscape" => NativeAction::RandomLandscape,
        "samplelandscape" => NativeAction::SampleLandscape,
        "mutationalneighborhood" => NativeAction::MutationalNeighborhood,
        "predictwlandscape" => NativeAction::PredictWLandscape,
        "predictnulandscape" => NativeAction::PredictNuLandscape,
        "hillclimb" => NativeAction::HillClimb,
        "pairtestlandscape" => NativeAction::PairTestLandscape,
        "analyzepopulation" => NativeAction::AnalyzePopulation,
        _ => return None,
    })
}

/// `largs.PopWord()` if words remain.
fn pop_arg(args: &mut String) -> Option<String> {
    if args.is_empty() {
        None
    } else {
        Some(su::pop_word(args))
    }
}

fn pop_int(args: &mut String, default: i32) -> i32 {
    pop_arg(args).map(|w| su::as_int(&w)).unwrap_or(default)
}

fn pop_string(args: &mut String, default: &str) -> String {
    pop_arg(args).unwrap_or_else(|| default.to_string())
}

impl Analyze {
    // -- Driver feedback (Avida2Driver::StdIOFeedback, printf to stdout) ----

    /// `ctx.Driver().Feedback().Notify(msg)`.
    ///
    /// QUIRK: the C++ passes the message as a `printf` format; the messages
    /// used here contain no `%`.
    pub(crate) fn driver_notify(&mut self, msg: &str) -> Result<()> {
        writeln!(self.out, "{msg}")?;
        Ok(())
    }

    /// `ctx.Driver().Feedback().Error(msg)`.
    pub(crate) fn driver_error(&mut self, msg: &str) -> Result<()> {
        writeln!(self.out, "error: {msg}")?;
        Ok(())
    }

    /// `ctx.Driver().Feedback().Warning(msg)`.
    pub(crate) fn driver_warning(&mut self, msg: &str) -> Result<()> {
        writeln!(self.out, "warning: {msg}")?;
        Ok(())
    }

    // -- cAnalyzeJobQueue ----------------------------------------------------
    //
    // The jobs run through `Analyze::run_job_units` (jobs.rs): one at a time
    // as `singleThreadedJobExecution` runs them, or on worker threads with
    // the same result. The C++ worker threads draw their seeds in a racy
    // order and set the analyze-mode flag of their context; the
    // single-threaded order and context are used for any worker count.

    /// `cAnalyzeJobQueue::Start()` / `tAnalyzeJobBatch::RunBatch()`.
    pub(crate) fn job_queue_start(&mut self, host: &dyn AnalyzeHost) -> Result<()> {
        if Self::verbosity(host) >= VERBOSE_DETAILS {
            self.driver_notify("waking worker threads...")?;
        }
        Ok(())
    }

    /// `cAnalyzeJobQueue::Execute()`.
    pub(crate) fn job_queue_execute(&mut self, host: &dyn AnalyzeHost) -> Result<()> {
        self.job_queue_start(host)?;
        if Self::verbosity(host) >= VERBOSE_DETAILS {
            self.driver_notify("job queue complete")?;
        }
        Ok(())
    }

    // -- Native actions -----------------------------------------------------

    /// The "Doing X of batch N" / "Doing X..." notice the landscape actions
    /// print before working.
    fn landscape_notice(&mut self, host: &dyn AnalyzeHost, on: &str, normal: &str) -> Result<()> {
        let v = Self::verbosity(host);
        if v >= VERBOSE_ON {
            let msg = format!("{on}{}", self.cur_batch);
            self.driver_notify(&msg)
        } else if v > VERBOSE_SILENT {
            self.driver_notify(normal)
        } else {
            Ok(())
        }
    }

    fn create_file(host: &mut dyn AnalyzeHost, path: &str) -> Result<DataFile> {
        host.output().create_with_path(path).map_err(|e| {
            AnalyzeError::fatal(format!(
                "{e} (C++: File::CreateWithPath returns NULL, then dereferenced)"
            ))
        })
    }

    /// The genomes of the current batch, in order.
    fn current_genomes(&mut self) -> Result<Vec<avida_genome::Genome>> {
        Ok(self.cur()?.list.iter().map(|g| g.genome.clone()).collect())
    }

    /// `cAction::Process(ctx)` for an action analyze mode runs itself.
    /// `_feedback` is the creation feedback; none of these constructors
    /// report anything.
    pub(crate) fn run_native_action(
        &mut self,
        host: &mut dyn AnalyzeHost,
        action: NativeAction,
        args: &str,
        _feedback: &mut Feedback,
    ) -> Result<()> {
        let mut largs = args.to_string();
        let analyze_mode = host.analyze_mode();
        match action {
            NativeAction::Verbose => {
                verbose::set_verbose(host.config_mut(), args, &mut *self.out)?;
                Ok(())
            }
            NativeAction::AnalyzeLandscape => {
                let filename = pop_string(&mut largs, "land-analyze.dat");
                let trials = pop_int(&mut largs, 1000);
                let min_found = pop_int(&mut largs, 0);
                let max_trials = pop_int(&mut largs, 0);
                let mut max_dist = pop_int(&mut largs, 10);
                if max_dist <= 0 {
                    max_dist = 10;
                }
                self.action_analyze_landscape(
                    host,
                    analyze_mode,
                    &filename,
                    trials,
                    min_found,
                    max_trials,
                    max_dist,
                )
            }
            NativeAction::PrecalcLandscape => {
                // cAnalyze::PopCommonCPUTestParameters(world, args, info)
                // with the default (NULL) resource history and offset 0.
                let use_resources = pop_int(&mut largs, 0);
                let update = pop_int(&mut largs, -1);
                let mut use_random_inputs = pop_arg(&mut largs)
                    .map(|w| su::as_int(&w) == 1)
                    .unwrap_or(false);
                let manual = self.pop_manual_inputs(host, &mut largs)?;
                if manual.is_some() {
                    use_random_inputs = false;
                }
                let opts = CpuTestOptions {
                    use_resources,
                    update,
                    use_random_inputs,
                    manual_inputs: manual,
                    resources: None,
                    resource_time_spent_offset: 0,
                    state_grid: 0,
                };
                if !analyze_mode {
                    return Ok(());
                }
                self.landscape_notice(
                    host,
                    "Precalculating landscape for batch ",
                    "Precalculating landscape...",
                )?;
                let bi = self.batch_index(self.cur_batch)?;
                // cAnalyzeGenotype::CalcLandscape, one job per genotype.
                let mut units = Vec::with_capacity(self.batches[bi].list.len());
                for (gi, g) in self.batches[bi].list.iter_mut().enumerate() {
                    g.cpu_test_info = opts.clone();
                    let mut land = g
                        .land
                        .take()
                        .unwrap_or_else(|| Box::new(Landscape::new(&g.genome)));
                    land.set_cpu_test_info(&opts);
                    land.set_distance(1);
                    units.push(((gi, land), 1));
                }
                self.run_job_units(
                    host,
                    units,
                    &|(_, land), world, seeds| {
                        let mut rng = seeds.next_rng()?;
                        world.with_job_world(&mut rng, &mut |w| land.process(w))
                    },
                    &|_| true,
                    &mut |a, _, (gi, land)| {
                        a.batches[bi].list[gi].land = Some(land);
                        Ok(())
                    },
                )?;
                self.job_queue_start(host)
            }
            NativeAction::FullLandscape => {
                let sfilename = pop_string(&mut largs, "land-full.dat");
                let dist = pop_int(&mut largs, 1);
                let efilename = pop_string(&mut largs, "");
                let cfilename = pop_string(&mut largs, "");
                if dist < 1 {
                    return self.driver_error("landscape distance must be >= 1");
                }
                if !analyze_mode {
                    return Ok(());
                }
                self.landscape_notice(host, "Full Landscaping batch ", "Full Landscapping...")?;
                let lands = self.landscape_jobs(host, Some(dist), None, |l, w| l.process(w))?;
                self.job_queue_execute(host)?;
                let mut sf = Self::create_file(host, &sfilename)?;
                let mut ef = if efilename.is_empty() {
                    None
                } else {
                    Some(Self::create_file(host, &efilename)?)
                };
                let mut cf = if cfilename.is_empty() {
                    None
                } else {
                    Some(Self::create_file(host, &cfilename)?)
                };
                for land in &lands {
                    land.print_stats(&mut sf, -1);
                    if let Some(ef) = ef.as_mut() {
                        land.print_entropy(ef, host.inst_sets())?;
                    }
                    if let Some(cf) = cf.as_mut() {
                        land.print_site_count(cf);
                    }
                }
                Ok(())
            }
            NativeAction::DumpLandscape => {
                let filename = pop_string(&mut largs, "land-dump.dat");
                let mut sdf = Self::create_file(host, &filename)?;
                if !analyze_mode {
                    return Ok(());
                }
                self.landscape_notice(host, "Dumping Landscape of batch ", "Dumping Landscape...")?;
                let bi = self.batch_index(self.cur_batch)?;
                let items: Vec<_> = self.batches[bi]
                    .list
                    .iter()
                    .map(|g| (format!("{}.land", g.name), g.genome.clone()))
                    .collect();
                for (gfn, genome) in items {
                    let mut gdf = Self::create_file(host, &gfn)?;
                    let mut land = Landscape::new(&genome);
                    host.with_test_world(TestContext::Default, &mut |w| {
                        land.process_dump(w, &mut gdf)
                    })?;
                    land.print_stats(&mut sdf, -1);
                }
                Ok(())
            }
            NativeAction::DeletionLandscape | NativeAction::InsertionLandscape => {
                let deletion = action == NativeAction::DeletionLandscape;
                let default = if deletion {
                    "land-del.dat"
                } else {
                    "land-ins.dat"
                };
                let sfilename = pop_string(&mut largs, default);
                let dist = pop_int(&mut largs, 1);
                let cfilename = pop_string(&mut largs, "");
                if !analyze_mode {
                    return Ok(());
                }
                let lands = if deletion {
                    self.landscape_notice(
                        host,
                        "Deletion Landscaping batch ",
                        "Deletion Landscapping...",
                    )?;
                    self.landscape_jobs(host, Some(dist), None, |l, w| l.process_delete(w))?
                } else {
                    self.landscape_notice(
                        host,
                        "Insertion Landscaping batch ",
                        "Insertion Landscapping...",
                    )?;
                    self.landscape_jobs(host, Some(dist), None, |l, w| l.process_insert(w))?
                };
                self.job_queue_execute(host)?;
                let mut sf = Self::create_file(host, &sfilename)?;
                let mut cf = if cfilename.is_empty() {
                    None
                } else {
                    Some(Self::create_file(host, &cfilename)?)
                };
                for land in &lands {
                    land.print_stats(&mut sf, -1);
                    if let Some(cf) = cf.as_mut() {
                        land.print_site_count(cf);
                    }
                }
                Ok(())
            }
            NativeAction::RandomLandscape | NativeAction::SampleLandscape => {
                let random = action == NativeAction::RandomLandscape;
                let filename = pop_string(
                    &mut largs,
                    if random {
                        "land-random.dat"
                    } else {
                        "land-sample.dat"
                    },
                );
                let dist = if random { pop_int(&mut largs, 1) } else { 0 };
                let trials = pop_int(&mut largs, 0);
                if !analyze_mode {
                    return Ok(());
                }
                let lands = if random {
                    self.landscape_notice(
                        host,
                        "Random Landscaping batch ",
                        "Random Landscapping...",
                    )?;
                    self.landscape_jobs(host, Some(dist), Some(trials), |l, w| l.random_process(w))?
                } else {
                    self.landscape_notice(
                        host,
                        "Sample Landscaping batch ",
                        "Sample Landscapping...",
                    )?;
                    // cActionSampleLandscape never calls SetDistance.
                    self.landscape_jobs(host, None, Some(trials), |l, w| l.sample_process(w))?
                };
                self.job_queue_execute(host)?;
                let mut df = Self::create_file(host, &filename)?;
                for land in &lands {
                    land.print_stats(&mut df, -1);
                }
                Ok(())
            }
            NativeAction::MutationalNeighborhood => {
                let filename = pop_string(&mut largs, "mut-neighborhood.dat");
                let target = pop_int(&mut largs, -1);
                self.action_mutational_neighborhood(host, analyze_mode, &filename, target)
            }
            NativeAction::PredictWLandscape | NativeAction::PredictNuLandscape => {
                // cActionPredictWLandscape / cActionPredictNuLandscape: the
                // file is created before the analyze-mode check, and each
                // genotype runs in the action's own context (not a job).
                let nu = action == NativeAction::PredictNuLandscape;
                let filename = pop_string(&mut largs, "land-predict.dat");
                let mut df = Self::create_file(host, &filename)?;
                if !analyze_mode {
                    return Ok(());
                }
                if nu {
                    self.landscape_notice(
                        host,
                        "Predicting Nu Landscape on batch ",
                        "Predicting Nu Landscape...",
                    )?;
                } else {
                    self.landscape_notice(
                        host,
                        "Predicting W Landscape on batch ",
                        "Predicting W Landscape...",
                    )?;
                }
                for genome in self.current_genomes()? {
                    let mut land = Landscape::new(&genome);
                    host.with_test_world(TestContext::Default, &mut |w| {
                        if nu {
                            land.predict_nu_process(w, &mut df, -1)
                        } else {
                            land.predict_w_process(w, &mut df, -1)
                        }
                    })?;
                }
                Ok(())
            }
            NativeAction::HillClimb => {
                let filename = pop_string(&mut largs, "hillclimb.dat");
                if !analyze_mode {
                    return Ok(());
                }
                self.landscape_notice(
                    host,
                    "Calculating Hill Climb on batch ",
                    "Calculating Hill Climb...",
                )?;
                let _df = Self::create_file(host, &filename)?;
                if self.cur()?.list.is_empty() {
                    return Ok(());
                }
                // cLandscape::HillClimb calls Reset(cur_genome), which sets
                // distance to 0, and then Process(ctx): Process_Body with
                // cur_distance 0 runs to max_line = size + 1, so it reads
                // base_seq[size] and writes mod_genome[size], one past the
                // end of both sequences.
                Err(AnalyzeError::fatal(
                    "HillClimb: cLandscape::HillClimb resets the distance to 0, so \
                     cLandscape::Process_Body reads and writes the site one past the end \
                     of the genome (C++ undefined behaviour)",
                ))
            }
            NativeAction::PairTestLandscape => {
                let filename = pop_string(&mut largs, "land-pairs.dat");
                let sample_size = pop_int(&mut largs, 0);
                if !analyze_mode {
                    return Ok(());
                }
                self.landscape_notice(
                    host,
                    "Pair Testing Landscape on batch ",
                    "Pair Testing Landscape...",
                )?;
                let lands = if sample_size != 0 {
                    self.landscape_jobs(host, None, Some(sample_size), |l, w| l.test_pairs(w))?
                } else {
                    self.landscape_jobs(host, None, None, |l, w| l.test_all_pairs(w))?
                };
                self.job_queue_execute(host)?;
                let mut df = Self::create_file(host, &filename)?;
                for land in &lands {
                    land.print_stats(&mut df, -1);
                }
                Ok(())
            }
            NativeAction::AnalyzePopulation => {
                // The constructor only parses its arguments (sample
                // probability, landscape flag, save flag, file name).
                if analyze_mode {
                    self.driver_warning(
                        "AnalyzePopulation not currently supported in Analyze Mode.",
                    )
                } else {
                    Err(AnalyzeError::unsupported(
                        "AnalyzePopulation",
                        "cActionAnalyzePopulation outside analyze mode (samples the live population)",
                    ))
                }
            }
        }
    }

    /// `cActionMutationalNeighborhood::Process`: one
    /// `cMutationalNeighborhood` per genotype of the current batch, then one
    /// row per genotype (`update` = its tree depth), in batch order.
    ///
    /// The single-threaded job queue runs each job as it is added: the
    /// action's `Process` job (`ProcessInitialize`), then the per-site jobs
    /// `ProcessInitialize` adds (each with its own seed), then
    /// `ProcessInitialize`'s `jobqueue.Start()`.
    ///
    /// The reference binary (FFI-migrated, MAX_CONCURRENCY 1) never gets
    /// here: `cAnalyzeJobQueue::AddJob` holds its non-recursive mutex while
    /// the single-threaded path calls `GetSeedForJob`, which locks it again,
    /// so it deadlocks on the first job; and with worker threads it crashes
    /// in `AvidaArray<sStep>::ResizeClear`, whose `std::vector::assign`
    /// copies a default `Genome` with a NULL representation. This port
    /// follows the original source and is checked against the test's
    /// `expected/` output.
    fn action_mutational_neighborhood(
        &mut self,
        host: &mut dyn AnalyzeHost,
        analyze_mode: bool,
        filename: &str,
        target: i32,
    ) -> Result<()> {
        let mut batch: Vec<(MutationalNeighborhood, i32)> = Vec::new();
        if analyze_mode {
            self.landscape_notice(
                host,
                "Calculating Mutational Neighborhood for batch ",
                "Calculating Mutational Neighborhood...",
            )?;
            let bi = self.batch_index(self.cur_batch)?;
            let items: Vec<_> = self.batches[bi]
                .list
                .iter()
                .map(|g| (g.genome.clone(), g.depth))
                .collect();
            // One unit per genotype: the action's Process job and the
            // per-site jobs it adds. A genotype whose instruction set is
            // unknown stops the action after the genotypes before it ran.
            let mut units = Vec::with_capacity(items.len());
            let mut bad = None;
            for (genome, depth) in items {
                match MutationalNeighborhood::new(host.inst_sets(), &genome, target) {
                    Ok(mutn) => {
                        let jobs = 1 + mutn.base_genome_size().max(0) as usize;
                        units.push(((mutn, depth), jobs));
                    }
                    Err(e) => {
                        bad = Some(e);
                        break;
                    }
                }
            }
            self.run_job_units(
                host,
                units,
                &|(mutn, _), world, seeds| {
                    // jobqueue.AddJob(Process): ProcessInitialize ...
                    let mut rng = seeds.next_rng()?;
                    world.with_job_world(&mut rng, &mut |w| mutn.process(w))?;
                    // ... which adds one Process job per site ...
                    for _ in 0..mutn.base_genome_size() {
                        let mut rng = seeds.next_rng()?;
                        world.with_job_world(&mut rng, &mut |w| mutn.process(w))?;
                    }
                    Ok(())
                },
                &|_| true,
                &mut |a, host, unit| {
                    // ... and then calls jobqueue.Start().
                    a.job_queue_start(host)?;
                    batch.push(unit);
                    Ok(())
                },
            )?;
            if let Some(e) = bad {
                return Err(e);
            }
            self.job_queue_execute(host)?;
        }

        let mut df = Self::create_file(host, filename)?;
        df.write_comment("IMPORTANT: Mutational Neighborhood is *EXPERIMENTAL*");
        df.write_comment("Output data and format is subject to change in future releases.");
        for (mutn, depth) in &batch {
            mutn.print_stats(&mut df, *depth)?;
        }
        Ok(())
    }

    /// One landscape per genotype of the current batch (`SetDistance` and
    /// `SetTrials` when given, as each action calls them), each
    /// processed by `f` as an analyze job.
    fn landscape_jobs(
        &mut self,
        host: &mut dyn AnalyzeHost,
        dist: Option<i32>,
        trials: Option<i32>,
        f: fn(&mut Landscape, &mut TestWorld<'_>) -> Result<()>,
    ) -> Result<Vec<Landscape>> {
        let genomes = self.current_genomes()?;
        let units: Vec<(Landscape, usize)> = genomes
            .iter()
            .map(|genome| {
                let mut land = Landscape::new(genome);
                if let Some(d) = dist {
                    land.set_distance(d);
                }
                if let Some(t) = trials {
                    land.set_trials(t);
                }
                (land, 1)
            })
            .collect();
        let mut lands = Vec::with_capacity(units.len());
        self.run_job_units(
            host,
            units,
            &|land, world, seeds| {
                let mut rng = seeds.next_rng()?;
                world.with_job_world(&mut rng, &mut |w| f(land, w))
            },
            &|_| true,
            &mut |_, _, land| {
                lands.push(land);
                Ok(())
            },
        )?;
        Ok(lands)
    }

    /// `cActionAnalyzeLandscape::Process`.
    #[allow(clippy::too_many_arguments)]
    fn action_analyze_landscape(
        &mut self,
        host: &mut dyn AnalyzeHost,
        analyze_mode: bool,
        filename: &str,
        trials: i32,
        min_found: i32,
        max_trials: i32,
        max_dist: i32,
    ) -> Result<()> {
        let update = -1;
        let mut batches: Vec<std::collections::VecDeque<Landscape>> =
            (0..max_dist).map(|_| Default::default()).collect();
        let mut depths = Vec::new();
        if analyze_mode {
            self.landscape_notice(
                host,
                "Performing landscape analysis on batch ",
                "Performing landscape analysis...",
            )?;
            let bi = self.batch_index(self.cur_batch)?;
            let items: Vec<_> = self.batches[bi]
                .list
                .iter()
                .map(|g| (g.genome.clone(), g.depth))
                .collect();
            // LoadGenome(batches, genome): one job per distance, from the
            // largest down.
            let mut units = Vec::new();
            for (genome, depth) in items {
                for dist in (1..=max_dist).rev() {
                    let mut land = Landscape::new(&genome);
                    land.set_distance(dist);
                    land.set_trials(trials);
                    if dist != 1 {
                        land.set_min_found(min_found);
                        land.set_max_trials(max_trials);
                    }
                    units.push(((land, dist), 1));
                }
                depths.push(depth);
            }
            self.run_job_units(
                host,
                units,
                &|(land, dist), world, seeds| {
                    let mut rng = seeds.next_rng()?;
                    if *dist == 1 {
                        world.with_job_world(&mut rng, &mut |w| land.process(w))
                    } else {
                        world.with_job_world(&mut rng, &mut |w| land.random_process(w))
                    }
                },
                &|_| true,
                &mut |_, _, (land, dist)| {
                    batches[(dist - 1) as usize].push_back(land);
                    Ok(())
                },
            )?;
        }
        self.job_queue_execute(host)?;

        if !analyze_mode {
            let df = host.output().static_with_path(filename).map_err(|e| {
                AnalyzeError::fatal(format!("{e} (C++: File::StaticWithPath returns NULL)"))
            })?;
            df.write_comment("Landscape analysis.  Distance results are grouped by update/depth.");
            return Ok(());
        }
        let mut df = Self::create_file(host, filename)?;
        df.write_comment("Landscape analysis.  Distance results are grouped by update/depth.");
        for &depth in &depths {
            for dist in 1..=max_dist {
                let land = batches[(dist - 1) as usize]
                    .pop_front()
                    .ok_or_else(|| AnalyzeError::fatal("landscape batch exhausted"))?;
                df.write(update, "update");
                df.write(depth, "tree depth");
                df.write(dist, "distance");
                df.write(land.prob_dead(), "fractional mutations lethal");
                df.write(land.prob_neg(), "fractional mutations detrimental");
                df.write(land.prob_neut(), "fractional mutations neutral");
                df.write(land.prob_pos(), "fractional mutations beneficial");
                df.write(land.num_trials(), "number of trials");
                df.write(land.num_found(), "number found");
                df.write(land.ave_fitness(), "average fitness");
                df.write(land.ave_sqr_fitness(), "average sqr fitness");
                df.endl();
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn table_has_every_registration() {
        assert_eq!(ACTION_NAMES.len(), 432);
        assert!(is_registered_action("dumplandscape"));
        assert!(is_registered_action("VERBOSE"));
        assert!(is_registered_action("PrintAverageData"));
        assert!(!is_registered_action("NoSuchAction"));
        assert_eq!(
            native_action("FullLandscape"),
            Some(NativeAction::FullLandscape)
        );
        assert_eq!(native_action("hillclimb"), Some(NativeAction::HillClimb));
        assert_eq!(
            native_action("mutationalNeighborhood"),
            Some(NativeAction::MutationalNeighborhood)
        );
        assert_eq!(
            native_action("PAIRTESTLANDSCAPE"),
            Some(NativeAction::PairTestLandscape)
        );
    }
}
