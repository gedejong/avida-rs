# Actions reference

An *action* is something an event can do: inject an organism, print a data file, change a resource, end the run. You schedule actions in the [events file](events-file.md), and many of them can also be used in [analyze mode](analyze-mode.md).

This page lists all 423 distinct action names that the program knows, with the argument signatures that `avida -e` prints. The list was generated from the action table in `crates/world/src/action_library.rs`, and the argument text is the program's own. The C++ program registers 432 names, nine of them twice, which is why the count is 423.

## Reading the argument column

- `<type name>` is a required argument; `[type name=default]` is optional with its default.
- `No Arguments`, `No arguments.` and `none` mean the action takes none.
- Action names are not case-sensitive in an events file.
- Every action on this page is known to the port. Where a path of an action is not ported, the program stops with an `unsupported:` message instead of running with different behaviour. The one action that always stops is `PrintDemeEnergySharingStats`; the others stop only in narrow cases. See [features that stop with unsupported](unsupported.md).

## The categories


- [Driver actions](#driver-actions) (9)
- [Environment actions](#environment-actions) (50)
- [Landscape actions](#landscape-actions) (14)
- [Population actions](#population-actions) (105)
- [Print actions](#print-actions) (234)
- [Save and load actions](#save-and-load-actions) (11)

For the actions most experiments use, with examples, see [the events file page](events-file.md#the-most-used-actions).

## Driver actions

Control the run itself: ending it, pausing it and seeding the random number generator.

| Action | Arguments |
|---|---|
| `Exit` | `No Arguments` |
| `ExitAveGeneration` | `<double generation>` |
| `ExitAveLineageLabelGreater` | `<double threshold>` |
| `ExitAveLineageLabelLess` | `<double threshold>` |
| `ExitDemeReplications` | `<int number of deme replications>` |
| `ExitDemeResources` | `<int number of deme resources>` |
| `ExitElapsedTime` | `<int elapsed time [seconds]>` |
| `Pause` | `No Arguments` |
| `SetRandomSeed` | `<int rng seed>` |

## Environment actions

Change resources, reactions and the environment during a run.

| Action | Arguments |
|---|---|
| `AlterMigrationConnection` | `<int from_deme> <int to_deme> <double alter_amount>` |
| `ChangeEnvironment` | `<string env_string>` |
| `DecInflow` | `<string resource_name> <double inflow_change>` |
| `DelayedDemeEvent` | `<int x1> <int y1> <int x2> <int y2> <int delay> <int duraion> <bool static_position> <int total_events>` |
| `DelayedDemeEventsPerSlots` | `<int x1> <int y1> <int x2> <int y2> <int delay> <int duraion> <bool static_position> <int total_slots_per_deme> <int total_events_per_slot_max> <int total_events_per_slot_min> <int tolal_event_flow_levels>` |
| `IncInflow` | `<string resource_name> <double inflow_change>` |
| `InjectCellResource` | `<int cell_id> <string res_name> <double res_count>` |
| `InjectResource` | `<string res_name> <double res_count>` |
| `InjectScaledResource` | `<string res_name> <double res_count>` |
| `MergeResourceAcrossDemes` | `<string deme_res_name> <string global_res_name>` |
| `OutflowScaledResource` | `<string res_name> <double res_percent>` |
| `SetCellResource` | `<int cell_id> <string res_name> <double res_count>` |
| `SetConfig` | `<string config_var> <string value>` |
| `SetDemeIOGrid` | `<Input/Output>:<cell id list>` |
| `SetDemeResource` | `<string res_name> <double res_count>` |
| `SetDemeResourceInflow` | `<int deme id> <string resource_name> <double inflow>` |
| `SetDemeResourceOutflow` | `<int deme id> <string resource_name> <double outflow>` |
| `SetEnvironmentInputs` | `<int input_1> <int input_2> <int input_3> ` |
| `SetEnvironmentRandomMask` | `<int mask>` |
| `SetFracDemeTreatable` | `<double factionTreatable>` |
| `SetGradientConeInflow` | `<string resource_name> <double inflow>` |
| `SetGradientConeOutflow` | `<string resource_name> <double outflow>` |
| `SetGradientInflow` | `<string resource_name> <double inflow>` |
| `SetGradientPlatInflow` | `<string resource_name> <double inflow>` |
| `SetGradientPlatOutflow` | `<string resource_name> <double outflow>` |
| `SetGradientResource` | `<string env_string>` |
| `SetGradPlatVarInflow` | `<string resource_name> <double mean> <double variance>` |
| `SetMigrationMatrix` | `<string filename>` |
| `SetNumInstBefore0Energy` | `<int new_value>` |
| `SetOptimizeMinMax` | `No Arguments` |
| `SetPeriodicResource` | `<string reaction_name> <string amplitude> <string pi/frequence> <phaseShift*pi> <string initial_Y>` |
| `SetPeriodicResourceInflow` | `<string resource_name> <int period>` |
| `SetPredatoryResource` | `<string resource_name> <double kill_odds> <int guarded_juvs_per_adult> <double detection_prob>` |
| `SetProbabilisticResource` | `<string resource_name> <double initial> <double inflow> <double outflow> <double lambda> <double theta> <int x> <int y> <int num>` |
| `SetReactionInst` | `<string reaction_name> <string inst>` |
| `SetReactionMaxTaskCount` | `<string reaction_name> <int max_count>` |
| `SetReactionMinTaskCount` | `<string reaction_name> <int min_count>` |
| `SetReactionTask` | `<string reaction_name> <string task_name>` |
| `SetReactionValue` | `<string reaction_name> <double value>` |
| `SetReactionValueMult` | `<string reaction_name> <double value>` |
| `SetResource` | `<string res_name> <double res_count>` |
| `SetResourceInflow` | `<string resource_name> <double inflow>` |
| `SetResourceOutflow` | `<string resource_name> <double outflow>` |
| `SetSeasonalResource` | `<string reaction_name>` |
| `SetSeasonalResource10Kyears_1To_1` | `<string reaction_name> <double scale>` |
| `SetSeasonalResource1Kyears_1To_1` | `<string reaction_name> <double scale>` |
| `SetTaskArgDouble` | `<int task> <int arg> <double value>` |
| `SetTaskArgInt` | `<int task> <int arg> <int value>` |
| `SetTaskArgString` | `<int task> <int arg> <string value>` |
| `ZeroResources` | `none` |

## Landscape actions

Measure fitness landscapes: the effect of every one-step mutation of the dominant genotype or of the whole population.

| Action | Arguments |
|---|---|
| `AnalyzeLandscape` | `[filename='land-analyze.dat'] [int trials=1000] [int min_found=0] [int max_trials=0] [int max_dist=10]` |
| `AnalyzePopulation` | `[double sample_prob=1] [int landscape=0] [int save_genotype=0] [string filename='']` |
| `DeletionLandscape` | `[string filename='land-del.dat'] [int distance=1] [string sitecount_file='']` |
| `DumpLandscape` | `[string filename='land-dump.dat']` |
| `FullLandscape` | `[string filename='land-full.dat'] [int distance=1] [string entropy_file=''] [string sitecount_file='']` |
| `HillClimb` | `[string filename='hillclimb.dat']` |
| `InsertionLandscape` | `[string filename='land-ins.dat'] [int distance=1] [string sitecount_file='']` |
| `MutationalNeighborhood` | `[string fname='mut-neighborhood.dat'] [int target=-1]` |
| `PairTestLandscape` | `[string filename=''] [int sample_size=0]` |
| `PrecalcLandscape` | `No Arguments` |
| `PredictNuLandscape` | `[string filename='land-predict.dat']` |
| `PredictWLandscape` | `[string filename='land-predict.dat']` |
| `RandomLandscape` | `[string filename='land-random.dat'] [int distance=1] [int trials=0]` |
| `SampleLandscape` | `[string filename='land-sample.dat'] [int trials=0]` |

## Population actions

Inject organisms, kill them, run demes, change mutation rates and reshape the world.

| Action | Arguments |
|---|---|
| `ApplyBottleneck` | `[int = 0]` |
| `AssignRandomCellData` | `[num_cells=deme_size]` |
| `AttackDen` | `[double probability=0.0] [int juvs_per_adult=1]` |
| `AvidianConjugation` | `(prob. of donation)` |
| `ClearParasites` | `[int cell_start=0] [int cell_end=-1]` |
| `CompeteDemes` | `[int type=1]` |
| `CompeteDemes_AttackKillAndEnergyConserve` | `No Arguments` |
| `CompeteDemesByEnergyDistribution` | `Competes demes according to the distribution of energy among the organisms` |
| `CompeteDemesByMerit` | `Compete demes according to the each deme's current merit` |
| `CompeteDemesByNetwork` | `No arguments.` |
| `CompeteDemesByTaskCount` | `Competes demes according to the number of times a given task has been completed within that deme` |
| `CompeteDemesByTaskCountAndEfficiency` | `Competes demes according to the number of times a given task has been completed within that deme and the efficiency with which it was done` |
| `CompeteOrganisms` | `[int type=0] [int parents_survive=0] [double scaled_time=1.0] [int dynamic_scaling=0]` |
| `ConnectCells` | `<int cellA_x> <int cellA_y> <int cellB_x> <int cellB_y>` |
| `CopyDeme` | `<int src_id> <int dest_id>` |
| `CountMultipleOpinions` | `[int opinion_count=0]` |
| `CountOpinions` | `[int desired_opinion=0 [int multiplicity=1 [int side=0]]]` |
| `DecayPoints` | `No Arguments` |
| `DemeBalanceTwoTasks` | `Two arguments: min and max for not` |
| `DemeReactionDiversity` | `No arguments.` |
| `DemeResourceThresholdPredicate` | `cString resourceName, cString comparisonOperator, double threasholdValue` |
| `Desynchronization` | `No Arguments` |
| `DiffuseHGTGenomeFragments` | `<none>` |
| `DisconnectCells` | `<int cellA_x> <int cellA_y> <int cellB_x> <int cellB_y>` |
| `DistributeData` | `No arguments.` |
| `DistributeDataCheaply` | `No arguments.` |
| `DistributeDataEfficiently` | `No arguments.` |
| `DivideDemes` | `No arguments (yet!)` |
| `Flash` | `No arguments` |
| `FlushTopNavTrace` | `''` |
| `Inject` | `<string fname> [int cell_id=0] [double merit=-1] [int lineage_label=0] [double neutral_metric=0] [string trace_filename]` |
| `InjectAll` | `<string filename> [instset=] [double merit=-1] [int lineage_label=0] [double neutral_metric=0]` |
| `InjectAllRandomRepro` | `<int length> [double merit=-1] [int lineage_label=0] [double neutral_metric=0]` |
| `InjectDemes` | `<string fname> [double merit=-1] [int lineage_label=0] [double neutral_metric=0]` |
| `InjectDemesFromNest` | `[int num_orgs=1] [int nest_cellid=0] [double merit=-1] [int lineage_label=0] [double neutral_metric=0]` |
| `InjectDemesRandom` | `[int num_orgs=1] [double merit=-1] [int lineage_label=0] [double neutral_metric=0]` |
| `InjectGroup` | `<string fname> [int cell_id=0] [int group_id=-1] [int forager_type=-1] [bool trace=false] [double merit=-1] [int lineage_label=0] [double neutral_metric=0]` |
| `InjectModuloDemes` | `<string fname> [int mod_num = 1] [double merit=-1] [int lineage_label=0] [double neutral_metric=0]` |
| `InjectParasite` | `<string filename> <string label> [int cell_start=0] [int cell_end=-1] [int cell_stride=1] [int only_if_parasites_extinct=0]` |
| `InjectParasitePair` | `<string filename_genome> <string filename_parasite> <string label> [int cell_start=0] [int cell_end=-1] [double merit=-1] [int lineage_label=0] [double neutral_metric=0]` |
| `InjectParasiteSequence` | `<string sequence> [int cell_start=0] [int cell_end=-1]` |
| `InjectRandom` | `<int length> [int cell_id=0] [double merit=-1] [int lineage_label=0] [double neutral_metric=0]` |
| `InjectRange` | `<string fname> [int cell_start=0] [int cell_end=-1] [double merit=-1] [int lineage_label=0] [double neutral_metric=0]` |
| `InjectSequence` | `<string sequence> [int cell_start=0] [int cell_end=-1] [double merit=-1] [int lineage_label=0] [double neutral_metric=0]` |
| `InjectSequenceWDivMutRate` | `<string sequence> [int cell_start=0] [int cell_end=-1] [double div_mut_rate=0] [double merit=-1] [int lineage_label=0] [double neutral_metric=0]` |
| `InjectWellMixed` | `<string fname> [int cell_num=0] [double merit=-1] [int lineage_label=0] [double neutral_metric=0]` |
| `InjectWholeGenomeDuplications` | `No arguments` |
| `InjectWholeGenomeDuplicationsNopCPrepend` | `No arguments` |
| `IteratedConsensus` | `[int compete_period=100 [int replace_number=1 [int kill=1 [int restrict_range=1]]]]` |
| `JoinGridCol` | `[int col_id=-1] [int min_row=0] [int max_row=-1]` |
| `JoinGridRow` | `[int row_id=-1] [int min_col=0] [int max_col=-1]` |
| `KillDemePercent` | `[double pctkills=0.0]` |
| `KillDemesHighestParasiteLoad` | `[double killprob=0.01] [int killmax = intmax]` |
| `KillDominantGenotype` | |
| `KillFractionInSequence` | `[double fraction=0.01]` |
| `KillFractionInSequence_PopLimit` | `[double fraction=0.01]` |
| `KillInstLimit` | `[double probability=0.9] [cString inst=nand] [int limit=5]` |
| `KillInstPair` | `[double probability=0.9] [cString inst1=nand] [cString inst2=nor] [int limit=1]` |
| `KillMeanBelowThresholdPaintable` | `[int numradii=0, int radius=0, string resource name, double threshold=0, string adversity resource name]` |
| `KillNBelowResourceThreshold` | `[int numkills=0, string resource name, double threshold=0]` |
| `KillProb` | `[double probability=0.9]` |
| `KillProbSequence` | `<string sequence> [double probability=0.9]` |
| `KillRate` | `[double probability=0.9]` |
| `KillRectangle` | `[int x1=0] [int y1=0] [int x2=0] [int y2=0]` |
| `KillWithinRadiusBelowResourceThreshold` | `[int numradii=0, int radius=0, string resource name, double threshold=0, double killdensity=1]` |
| `KillWithinRadiusBelowResourceThresholdTestAll` | `[int numradii=0, int radius=0, string resource name, double threshold=0, double killdensity=1]` |
| `KillWithinRadiusMeanBelowResourceThreshold` | `[int numradii=0, int radius=0, string resource name, double threshold=0, double killdensity=1]` |
| `LoadMiniTraceQ` | `<cString fname> [int orgs_per=1] [boolean print_genomes=1] [boolean print_reacs=0]` |
| `MeasureDemeNetworks` | `No arguments.` |
| `MixPopulation` | `No arguments.` |
| `ModMutProb` | `[string mut_type='COPY_MUT'] [double prob=0.0] [int start_cell=-1] [int end_cell=-1]` |
| `NewTrial` | `No Arguments` |
| `PhenotypeMatch` | `string file-name` |
| `Pred_DemeEventMoveBetweenTargets` | `[int times=1]` |
| `Pred_DemeEventMoveCenter` | `[int times=1]` |
| `Pred_DemeEventNUniqueIndividualsMovedIntoTarget` | `[int numorgs=1]` |
| `PrintMicroTraces` | `[boolean random=0] [boolean rand_prey=0] [boolean rand_pred=0] [int next_prey=0] [int next_pred=0] [boolean save_dominants=0] [boolean save_groups=0] [boolean save_foragers=0] [int orgs_per=1] [int max_samples=0] [boolean print_genomes=0] [boolean print_reacs=0]` |
| `PrintMiniTraces` | `[boolean random=0] [boolean save_dominants=0] [boolean save_groups=0] [boolean save_foragers=0] [int orgs_per=1] [int max_samples=0] [boolean print_genomes=1] [boolean print_reacs = 0]` |
| `PrintNavTrace` | `''` |
| `PrintReproData` | `''` |
| `PrintTopNavTrace` | `''` |
| `RaidDen` | `[double loss=0.0] [int res_id=1] [double units_per_adult=1.0]` |
| `RemovePredators` | `''` |
| `ReplaceFromGermline` | `[double p(kill)=0.0 [int update_cell_data=0]]` |
| `ReplicateDeme` | `<int src_id>` |
| `ReplicateDemes` | `[string trigger=full_deme]` |
| `ReplicateDemesHighestBirthCount` | `[double replprob=0.01] [int replmax = intmax]` |
| `ReplicateDemesHighestFecundity` | `[double replprob=0.01] [int replmax = intmax]` |
| `ResetDemes` | `No Arguments` |
| `SerialTransfer` | `[int transfer_size=1] [int ignore_deads=1]` |
| `SetMigrationRate` | `[double rate=0.0]` |
| `SetMutProb` | `[string mut_type='COPY_MUT'] [double prob=0.0] [int start_cell=-1] [int end_cell=-1]` |
| `SetProbFail` | `<char instruction> <int prob_fail>` |
| `SetRedundancy` | `<char instruction> <int redundancy>` |
| `SeverGridCol` | `[int col_id=-1] [int min_row=0] [int max_row=-1]` |
| `SeverGridRow` | `[int row_id=-1] [int min_col=0] [int max_col=-1]` |
| `SwapCells` | `<int cell_id1> <int cell_id2>` |
| `SwapRandomCells` | `<int number swaps>` |
| `Synchronization` | `No Arguments` |
| `ToggleFitnessValley` | `No Arguments` |
| `ToggleRewardInstruction` | `No Arguments` |
| `TrackAllMessages` | `No Arguments` |
| `UnitFitness` | `No arguments.` |
| `UpdateDemeParasiteMemoryScores` | `<double decay_rate>` |
| `ZeroMuts` | `No Arguments` |

## Print actions

Write data files and grids. Most take an optional file name; the default file name is in the arguments column.

| Action | Arguments |
|---|---|
| `CalcConsensus` | `[int lines_saved=0]` |
| `DumpCellDataGrid` | `[string fname='']` |
| `DumpClassificationIDGrid` | `[string fname_prefix='']` |
| `DumpDonorGrid` | `[string fname='']` |
| `DumpEnergyGrid` | `[string fname='']` |
| `DumpExecutionRatioGrid` | `[string fname='']` |
| `DumpFitnessGrid` | `[string fname='']` |
| `DumpGenomeLengthGrid` | `[string fname='']` |
| `DumpGenotypeColorGrid` | `[int num_colors=12] [string fname='']` |
| `DumpGenotypeGrid` | `[string fname='']` |
| `DumpHostGenotypeList` | `[string fname='']` |
| `DumpHostTaskGrid` | `[string fname='']` |
| `DumpHostTaskGridComma` | `[string fname='']` |
| `DumpIDGrid` | `[string fname='']` |
| `DumpLastTaskGrid` | `[string fname='']` |
| `DumpMaxResGrid` | `[string fname='']` |
| `DumpOffspringMigrationCounts` | `[string fname='']` |
| `DumpParasiteGenotypeGrid` | `[string fname='']` |
| `DumpParasiteGenotypeList` | `[string fname='']` |
| `DumpParasiteMigrationCounts` | `[string fname='']` |
| `DumpParasiteTaskGrid` | `[string fname='']` |
| `DumpParasiteTaskGridComma` | `[string fname='']` |
| `DumpParasiteVirulenceGrid` | `[string fname='']` |
| `DumpPhenotypeIDGrid` | `[string fname='']` |
| `DumpReactionGrid` | `[string fname='']` |
| `DumpReceiverGrid` | `[string fname='']` |
| `DumpSleepGrid` | `[string fname='']` |
| `DumpTargetGrid` | `[string fname='']` |
| `DumpTaskGrid` | `[string fname='']` |
| `DumpVitalityGrid` | `[string fname='']` |
| `Echo` | `<cString message>` |
| `PrintAgePolyethismData` | `[string fname="age_polyethism.dat"]` |
| `PrintAttacks` | `[string fname="attacks.dat"]` |
| `PrintAveNumTasks` | `[string fname='']` |
| `PrintAverageData` | `[string fname="average.dat"]` |
| `PrintAvgDemeTasksExeData` | `[string fname="avg_deme_tasks_exe.dat"]` |
| `PrintAvgTreatableDemeTasksExeData` | `[string fname="avg_treatable_deme_tasks_exe.dat"]` |
| `PrintAvgUntreatableDemeTasksExeData` | `[string fname="avg_untreatable_deme_tasks_exe.dat"]` |
| `PrintBirthChamber` | `[string fname="birth_chamber/bc-XXXX.dat"] [int hwtype=0]` |
| `PrintBirthChamberMatingTypeHistogram` | `[string fname="birth_chamber_mating_type_histogram.dat"] [int hwtype=0]` |
| `PrintCCladeCounts` | `[filename = "cclade_count.dat"]` |
| `PrintCCladeFitnessHistogram` | `[filename] [fit_mode] [hist_min] [hist_step] [hist_max]` |
| `PrintCCladeRelativeFitnessHistogram` | `[filename] [fit_mode] [hist_min] [hist_step] [hist_max]` |
| `PrintCellData` | `[string fname="cell_data.dat"]` |
| `PrintCellVisitsData` | `[string fname="visits.dat"]` |
| `PrintCompetitionData` | `[string fname="competition.dat"]` |
| `PrintConsensusData` | `[string fname="consensus.dat"]` |
| `PrintCountData` | `[string fname="count.dat"]` |
| `PrintCurrentMeanDemeDensity` | `[string fname="deme_currentMeanDensity.dat"]` |
| `PrintCurrentOpinions` | `[string fname="opinions.dat"]` |
| `PrintCurrentReactionData` | `[string fname="cur_reactions.dat"]` |
| `PrintCurrentReactionRewardData` | `[string fname="cur_reaction_reward.dat"]` |
| `PrintCurrentTaskCounts` | `[string fname="curr_task_counts.dat"]` |
| `PrintCurrReactionCountGrid` | `[string fname='']` |
| `PrintData` | `<cString fname> <cString format>` |
| `PrintDebug` | `No Arguments` |
| `PrintDemeAllStats` | `No Arguments` |
| `PrintDemeAverageData` | `[string fname="deme_average.dat"]` |
| `PrintDemeCompetitionData` | `[string fname="deme_compete.dat"]` |
| `PrintDemeCurrentTaskExeData` | `[string fname="deme_cur_task_exe.dat"]` |
| `PrintDemeDonorStats` | `No Arguments` |
| `PrintDemeEnergyDistributionStats` | `No Arguments` |
| `PrintDemeEnergySharingStats` | `No Arguments` |
| `PrintDemeFitnessData` | `No Arguments` |
| `PrintDemeFoundersData` | `[string fname="deme_founders.dat"]` |
| `PrintDemeGermlineSequestration` | `[string fname="deme_germ.dat"]` |
| `PrintDemeGermResourcesData` | `[string fname="deme_germ_res.dat"]` |
| `PrintDemeGlobalResources` | `No Arguments` |
| `PrintDemeGLSFounders` | `[string fname="deme_gls_founders.dat"]` |
| `PrintDemeLifeFitnessData` | `No Arguments` |
| `PrintDemeMigrationSuicidePoints` | `[string fname="deme_mig_suicide_points.dat"]` |
| `PrintDemeNetworkData` | `[string fname="deme_network.dat"]` |
| `PrintDemeNetworkTopology` | `[string fname="deme_network_topology.dat"]` |
| `PrintDemeOrgGermlineSequestration` | `[string fname="deme_org_germ.dat"]` |
| `PrintDemeOrgReactionData` | `[string fname="deme_org_reactions.dat"]` |
| `PrintDemeOrgTasksData` | `[string fname="deme_org_tasks.dat"]` |
| `PrintDemeOrgTasksExeData` | `[string fname="deme_org_tasks_exe.dat"]` |
| `PrintDemeReactionData` | `[string fname="deme_reactions.dat"]` |
| `PrintDemeReactionDiversityReplicationData` | `[string fname="deme_rx_repl.dat"]` |
| `PrintDemeReplicationData` | `[string fname="deme_repl.dat"]` |
| `PrintDemeResourceStats` | `No Arguments` |
| `PrintDemesFitnessData` | `[string fname="demes_fitness.dat"]` |
| `PrintDemesMeritsData` | `No Arguments` |
| `PrintDemeSpacialEnergyStats` | `No Arguments` |
| `PrintDemeSpacialSleepStats` | `No Arguments` |
| `PrintDemesReactionsData` | `[string fname="demes_reactions.dat"]` |
| `PrintDemesTasksData` | `[string fname="demes_tasks.dat"]` |
| `PrintDemeStats` | `No Arguments` |
| `PrintDemesTotalAvgEnergy` | `No Arguments` |
| `PrintDemeTasksData` | `No Arguments` |
| `PrintDemeTasksExeData` | `[string fname="deme_tasks_exe.dat"]` |
| `PrintDemeTestamentStats` | `[string fname="deme_testament.dat"]` |
| `PrintDemeTreatableCount` | `[string fname="deme_treatable.dat"]` |
| `PrintDemeTreatableReplicationData` | `[string fname="deme_repl_treatable.dat"]` |
| `PrintDemeUntreatableReplicationData` | `[string fname="deme_repl_untreatable.dat"]` |
| `PrintDenData` | `[string fname="den_data.dat"]` |
| `PrintDepthHistogram` | `[string fname="depth_histogram.dat"]` |
| `PrintDetailedFitnessData` | `[int save_max_f_genotype=0] [int print_fitness_histo=0] [double hist_fmax=1] [double hist_fstep=0.1] [string datafn="fitness.dat"] [string histofn="fitness_histos.dat"] [string histotestfn="fitness_histos_testCPU.dat"]` |
| `PrintDetailedSynchronizationData` | `[string fname="sync-detail.dat"]` |
| `PrintDirectReciprocityData` | `[string fname="reciprocity.dat"]` |
| `PrintDivideMutData` | `[string fname="divide_mut.dat"]` |
| `PrintDominantData` | `[string fname="dominant.dat"]` |
| `PrintDominantForagerGenotypes` | `[string fname='']` |
| `PrintDominantGenotype` | `[string fname='']` |
| `PrintDominantGroupGenotypes` | `[string fname='']` |
| `PrintDonationStats` | `No Arguments` |
| `PrintDynamicMaxMinData` | `[string fname="maxmin.dat"]` |
| `PrintEditDistance` | `[sample size [filename]]` |
| `PrintErrorData` | `[string fname="error.dat"]` |
| `PrintExtendedTimeData` | `[string fname="xtime.dat"]` |
| `PrintFemaleAverageData` | `[string fname="female_average.dat"]` |
| `PrintFemaleErrorData` | `[string fname="female_error.dat"]` |
| `PrintFemaleInstructionData` | `[string fname="female_instruction-${inst_set}.dat"] [string inst_set]` |
| `PrintFemaleMatePreferenceData` | `[string fname="female_mate_preference_data.dat"]` |
| `PrintFemaleVarianceData` | `[string fname="female_variance.dat"]` |
| `PrintFlowRateTuples` | `[string fname="flow_rate_tuples.dat"]` |
| `PrintFromMessageInstructionData` | `[string fname="from_msg_instruction-${inst_set}.dat"] [string inst_set]` |
| `PrintGeneticDistanceData` | `<string ref_creature_file> [string fname='genetic_distance.dat']` |
| `PrintGenomicSiteEntropy` | `[filename = "GenomicSiteEntropyData.dat"]` |
| `PrintGenotypeAbundanceHistogram` | `[string fname="genotype_abundance_histogram.dat"]` |
| `PrintGermlineData` | `[string fname="germline.dat"]` |
| `PrintGroupAttackData` | `[string fname="group_attacks-${inst_set}.dat"] [string inst_set]` |
| `PrintGroupIds` | `[string fname="groupids.dat"]` |
| `PrintGroupMTTolerance` | `[string fname="groupMTtolerance.dat"]` |
| `PrintGroupsFormedData` | `[string fname="groupformation.dat"]` |
| `PrintGroupTolerance` | `[string fname="grouptolerance.dat"]` |
| `PrintHGTData` | `[string fname="hgt.dat"]` |
| `PrintHostDepthHistogram` | `[string fname="depth_host_histogram.dat"]` |
| `PrintHostPhenotypeData` | `[string fname="host_phenotype_count.dat"]` |
| `PrintHostTasksData` | `[string fname="host_tasks.dat"]` |
| `PrintInstructionAbundanceHistogram` | `[string fname="instruction_histogram-${inst_set}.dat"] [string inst_set]` |
| `PrintInstructionData` | `[string fname="instruction-${inst_set}.dat"] [string inst_set]` |
| `PrintInternalTasksData` | `[string fname="in_tasks.dat"]` |
| `PrintInternalTasksQualData` | `[string fname="in_tasks_quality.dat"]` |
| `PrintInterruptData` | `[string fname="interrupt.dat"]` |
| `PrintIntrinsicTaskSwitchingCostData` | `[string fname="intrinsic_task_switching_cost.dat"]` |
| `PrintKaboom` | `[string fname="kabooms.dat"]` |
| `PrintKilledPreyFTData` | `[string fname="killed_prey.dat"]` |
| `PrintLastReactionCountGrid` | `[string fname='']` |
| `PrintLineageCounts` | `[string fname='lineage_counts.dat']` (the header is only correct if all lineages are present before this action first runs) |
| `PrintLogFitnessHistogram` | `Parameters: <filename> <mode> <min> <step> <max>` |
| `PrintMaleAverageData` | `[string fname="male_average.dat"]` |
| `PrintMaleErrorData` | `[string fname="male_error.dat"]` |
| `PrintMaleInstructionData` | `[string fname="male_instruction-${inst_set}.dat"] [string inst_set]` |
| `PrintMaleVarianceData` | `[string fname="male_variance.dat"]` |
| `PrintMatingDisplayData` | `[string fname="mating_display_data.dat"]` |
| `PrintMatingTypeHistogram` | `[string fname="mating_type_histogram.dat"]` |
| `PrintMessageData` | `[string fname="message.dat"]` |
| `PrintMessageLog` | `[string fname="message_log.dat"]` |
| `PrintMigrationData` | `[string fname="migration.dat"]` |
| `PrintMigrationMatrix` | `[string fname='']` |
| `PrintMimicDisplays` | `[string fname="mimics.dat"]` |
| `PrintMultiProcessData` | `[string fname="multiprocess.dat"]` |
| `PrintMutationRateData` | `[string fname="mutation_rates.dat"]` |
| `PrintNewReactionData` | `[string fname="newreactions.dat"]` |
| `PrintNewTasksData` | `[string fname="newtasks.dat"]` |
| `PrintNewTasksDataPlus` | `[string fname="newtasksplus.dat"]` |
| `PrintNumDivides` | `[string fname='']` |
| `PrintNumOrgsInDeme` | `No Arguments` |
| `PrintNumOrgsKilledData` | `[string fname="orgs_killed.dat"]` |
| `PrintOpinionsSetPerDeme` | `[string fname="opinions_set.dat"]` |
| `PrintOrganismLocation` | `[string fname="location.dat"]` |
| `PrintOrgGuardData` | `[string fname='']` |
| `PrintOrgLocData` | `[string fname='']` |
| `PrintParasiteData` | `[string fname="parasite.dat"]` |
| `PrintParasiteDepthHistogram` | `[string fname="depth_parasite_histogram.dat"]` |
| `PrintParasitePhenotypeData` | `[string fname="parasite_phenotype_count.dat"]` |
| `PrintParasiteTasksData` | `[string fname="parasite_tasks.dat"]` |
| `PrintPerDemeGenPerFounderData` | `[string fname="deme_gen_between_founders.dat"]` |
| `PrintPerDemeReactionData` | `[string fname="per_deme_reactions.dat"]` |
| `PrintPerDemeTasksData` | `[string fname="per_deme_tasks.dat"]` |
| `PrintPerDemeTasksExeData` | `[string fname="per_deme_tasks_exe.dat"]` |
| `PrintPhenotypeData` | `[string fname="phenotype_count.dat"]` |
| `PrintPhenotypeStatus` | `[string fname="phenotype_status.dat"]` |
| `PrintPhenotypicPlasticity` | `[string filename='phenplast'] [int num_trials=1000]` |
| `PrintPlasticGenotypeSummary` | `[string filename='genotype_plsticity.dat']` |
| `PrintPopulationDistanceData` | `<string creature> [string fname=""] [int save_genotypes=0]` |
| `PrintPredatorAverageData` | `[string fname="predator_average.dat"]` |
| `PrintPredatorErrorData` | `[string fname="predator_error.dat"]` |
| `PrintPredatorFromSensorInstructionData` | `[string fname="predator_from_sensor_exec-${inst_set}.dat"] [string inst_set]` |
| `PrintPredatorInstructionData` | `[string fname="predator_instruction-${inst_set}.dat"] [string inst_set]` |
| `PrintPredatorVarianceData` | `[string fname="predator_variance.dat"]` |
| `PrintPredicatedMessages` | `[string fname="messages.dat"]` |
| `PrintPreyAverageData` | `[string fname="prey_average.dat"]` |
| `PrintPreyErrorData` | `[string fname="prey_error.dat"]` |
| `PrintPreyFlockingData` | `[string fname='']` |
| `PrintPreyFromSensorInstructionData` | `[string fname="prey_from_sensor_exec-${inst_set}.dat"] [string inst_set]` |
| `PrintPreyInstructionData` | `[string fname="prey_instruction-${inst_set}.dat"] [string inst_set]` |
| `PrintPreyVarianceData` | `[string fname="prey_variance.dat"]` |
| `PrintProfilingData` | `[string fname="profiling.dat"]` |
| `PrintQuorum` | `[string fname="threshold.dat"]` |
| `PrintReactionData` | `[string fname="reactions.dat"]` |
| `PrintReactionExeData` | `[string fname="reactions_exe.dat"]` |
| `PrintReactionRewardData` | `[string fname="reaction_reward.dat"]` |
| `PrintRelativeFitnessHistogram` | `[filename] [fit_mode] [hist_min] [hist_step] [hist_max]` |
| `PrintReputationData` | `[string fname="reputation.dat"]` |
| `PrintResourceData` | `[string fname="resource.dat"]` |
| `PrintResourceLocData` | `[string fname="resourceloc.dat"]` |
| `PrintResWallLocData` | `[string fname="reswallloc.dat"]` |
| `PrintRetMessageLog` | `[string fname="retmessage_log.dat"]` |
| `PrintSenseData` | `[string fname="sense.dat"]` |
| `PrintSenseExeData` | `[string fname="sense_exe.dat"]` |
| `PrintShadedAltruists` | `[string fname="shadedaltruists.dat"]` |
| `PrintSimpleConsensusData` | `[string fname="simple_consensus.dat"]` |
| `PrintSleepData` | `[string fname="sleep.dat"]` |
| `PrintSoloTaskSnapshot` | `[string fname="tasks-snap.dat"]` |
| `PrintSpatialResources` | `[string fname="resource.dat"]` |
| `PrintStringMatchData` | `[string fname="stringmatch.dat"]` |
| `PrintSuccessfulMates` | `[string fname="mates/mates-XXXX.dat"]` |
| `PrintSynchronizationData` | `[string fname="sync.dat"]` |
| `PrintTargets` | `[string fname="targets.dat"]` |
| `PrintTaskProbHistogram` | `[filename=pp_histogram.dat] [weightbycpus=0]` |
| `PrintTasksData` | `[string fname="tasks.dat"]` |
| `PrintTasksExeData` | `[string fname="tasks_exe.dat"]` |
| `PrintTaskSnapshot` | `[string fname='']` |
| `PrintTasksQualData` | `[string fname="tasks_quality.dat"]` |
| `PrintThreadsData` | `[string fname="threads.dat"]` |
| `PrintTimeData` | `[string fname="time.dat"]` |
| `PrintToleranceData` | `[string fname="tolerance.dat"]` |
| `PrintToleranceInstructionData` | `[string fname="toleranceinstruction.dat"]` |
| `PrintTopPredatorAverageData` | `[string fname="top_pred_average.dat"]` |
| `PrintTopPredatorErrorData` | `[string fname="top_pred_error.dat"]` |
| `PrintTopPredatorFromSensorInstructionData` | `[string fname="top_pred_from_sensor_exec-${inst_set}.dat"] [string inst_set]` |
| `PrintTopPredatorInstructionData` | `[string fname="top_pred_instruction-${inst_set}.dat"] [string inst_set]` |
| `PrintTopPredatorVarianceData` | `[string fname="top_pred_variance.dat"]` |
| `PrintTopPredTargets` | `[string fname="top_pred_targets.dat"]` |
| `PrintTotalsData` | `[string fname="totals.dat"]` |
| `PrintVarianceData` | `[string fname="variance.dat"]` |
| `PrintViableTasksData` | `[string fname='viable_tasks.dat']` |
| `PrintWinningDeme` | `[string fname="deme_winners.dat"]` |
| `SaveDemeFounders` | `[string fname='']` |
| `SetVerbose` | `[string verbosity='']` |
| `TestDominant` | `[string fname='dom-test.dat']` |
| `VERBOSE` | `[string verbosity='']` |

## Save and load actions

Write and read populations, germlines and other large data sets.

| Action | Arguments |
|---|---|
| `LoadBirthCounts` | `[string filename='detailgermlines']` |
| `LoadGermlines` | `[string filename='detailgermlines']` |
| `LoadHostGenotypeList` | `[string filename='flame']` |
| `LoadParasiteGenotypeList` | `[string filename='flame']` |
| `LoadParasiteMemoryScores` | `[string filename='detailgermlines']` |
| `LoadPopulation` | `<cString fname> [int update=-1] [int cellid_offset=0] [int lineage_offset=0] [bool load_groups=0] [bool load_birth_cells=0] [bool load_avatars] [bool load_rebirth] [bool load_parent_dat] [int load_traceq]` |
| `LoadStructuredSystematicsGroup` | `<string fname> <string role>` |
| `SaveFlameData` | `[string filename='flame']` |
| `SaveGermlines` | `[string filename='detailgermlines']` |
| `SavePopulation` | `[string filename='detail'] [boolean save_historic=1] [boolean save_groups=0] [boolean save_avatars=0] [boolean save_rebirth=0]` |
| `SaveStructuredSystematicsGroup` | `[string filename=''] <string role>` |
