//! `STATS_OUT_FILE` actions: `cStats::<method>(filename)` by action name.
//!
//! Generated from `actions::STATS_OUT_FILE_ACTIONS` and the printers in
//! `stats.rs` / `stats_print.rs`. A method that needs population state this
//! port does not gather yet returns `None`; the caller reports it as
//! unsupported.

use avida_apto::OutputManager;

use crate::stats::Stats;

/// Runs the `cStats` printer named `method`, or returns `None` when it is
/// not available as a plain `(filename)` printer.
pub fn print_stats(
    stats: &mut Stats,
    out: &mut OutputManager,
    method: &str,
    filename: &str,
) -> Option<Result<(), String>> {
    Some(match method {
        "PrintAverageData" => stats.print_average_data(out, filename),
        "PrintDemeAverageData" => stats.print_deme_average_data(out, filename),
        "PrintErrorData" => stats.print_error_data(out, filename),
        "PrintVarianceData" => stats.print_variance_data(out, filename),
        "PrintCountData" => stats.print_count_data(out, filename),
        "PrintMessageLog" => stats.print_message_log(out, filename),
        "PrintRetMessageLog" => stats.print_ret_message_log(out, filename),
        "PrintTotalsData" => stats.print_totals_data(out, filename),
        "PrintTasksData" => stats.print_tasks_data(out, filename),
        "PrintHostTasksData" => stats.print_host_tasks_data(out, filename),
        "PrintParasiteTasksData" => stats.print_parasite_tasks_data(out, filename),
        "PrintTasksExeData" => stats.print_tasks_exe_data(out, filename),
        "PrintNewTasksData" => stats.print_new_tasks_data(out, filename),
        "PrintNewReactionData" => stats.print_new_reaction_data(out, filename),
        "PrintNewTasksDataPlus" => stats.print_new_tasks_data_plus(out, filename),
        "PrintTasksQualData" => stats.print_tasks_qual_data(out, filename),
        "PrintReactionData" => stats.print_reaction_data(out, filename),
        "PrintReactionExeData" => stats.print_reaction_exe_data(out, filename),
        "PrintCurrentReactionData" => stats.print_current_reaction_data(out, filename),
        "PrintReactionRewardData" => stats.print_reaction_reward_data(out, filename),
        "PrintCurrentReactionRewardData" => stats.print_current_reaction_reward_data(out, filename),
        "PrintTimeData" => stats.print_time_data(out, filename),
        "PrintExtendedTimeData" => stats.print_extended_time_data(out, filename),
        "PrintMutationRateData" => stats.print_mutation_rate_data(out, filename),
        "PrintDivideMutData" => stats.print_divide_mut_data(out, filename),
        "PrintParasiteData" => stats.print_parasite_data(out, filename),
        "PrintPreyAverageData" => stats.print_prey_average_data(out, filename),
        "PrintPredatorAverageData" => stats.print_predator_average_data(out, filename),
        "PrintTopPredatorAverageData" => stats.print_top_predator_average_data(out, filename),
        "PrintPreyErrorData" => stats.print_prey_error_data(out, filename),
        "PrintPredatorErrorData" => stats.print_predator_error_data(out, filename),
        "PrintTopPredatorErrorData" => stats.print_top_predator_error_data(out, filename),
        "PrintPreyVarianceData" => stats.print_prey_variance_data(out, filename),
        "PrintPredatorVarianceData" => stats.print_predator_variance_data(out, filename),
        "PrintTopPredatorVarianceData" => stats.print_top_predator_variance_data(out, filename),
        "PrintSenseData" => stats.print_sense_data(out, filename),
        "PrintSenseExeData" => stats.print_sense_exe_data(out, filename),
        "PrintInternalTasksData" => stats.print_internal_tasks_data(out, filename),
        "PrintInternalTasksQualData" => stats.print_internal_tasks_qual_data(out, filename),
        "PrintCompetitionData" => stats.print_competition_data(out, filename),
        "PrintDemeGLSFounders" => stats.print_deme_gls_founders(out, filename),
        "PrintDemeReactionDiversityReplicationData" => {
            stats.print_deme_reaction_diversity_replication_data(out, filename)
        }
        "PrintDemeCompetitionData" => stats.print_deme_competition_data(out, filename),
        "PrintDemeNetworkData" => stats.print_deme_network_data(out, filename),
        "PrintDemeFoundersData" => stats.print_deme_founders_data(out, filename),
        "PrintDemesFitnessData" => stats.print_demes_fitness_data(out, filename),
        "PrintMultiProcessData" => stats.print_multi_process_data(out, filename),
        "PrintProfilingData" => stats.print_profiling_data(out, filename),
        "PrintGermlineData" => stats.print_germline_data(out, filename),
        "PrintConsensusData" => stats.print_consensus_data(out, filename),
        "PrintSimpleConsensusData" => stats.print_simple_consensus_data(out, filename),
        "PrintSynchronizationData" => stats.print_synchronization_data(out, filename),
        "PrintDetailedSynchronizationData" => {
            stats.print_detailed_synchronization_data(out, filename)
        }
        "PrintFlowRateTuples" => stats.print_flow_rate_tuples(out, filename),
        "PrintNumOrgsKilledData" => stats.print_num_orgs_killed_data(out, filename),
        "PrintMigrationData" => stats.print_migration_data(out, filename),
        "PrintAgePolyethismData" => stats.print_age_polyethism_data(out, filename),
        "PrintIntrinsicTaskSwitchingCostData" => {
            stats.print_intrinsic_task_switching_cost_data(out, filename)
        }
        "PrintMaleAverageData" => stats.print_male_average_data(out, filename),
        "PrintFemaleAverageData" => stats.print_female_average_data(out, filename),
        "PrintMaleErrorData" => stats.print_male_error_data(out, filename),
        "PrintFemaleErrorData" => stats.print_female_error_data(out, filename),
        "PrintMaleVarianceData" => stats.print_male_variance_data(out, filename),
        "PrintFemaleVarianceData" => stats.print_female_variance_data(out, filename),
        "PrintKaboom" => stats.print_kaboom(out, filename),
        "PrintQuorum" => stats.print_quorum(out, filename),
        "PrintToleranceInstructionData" => stats.print_tolerance_instruction_data(out, filename),
        "PrintAttacks" => stats.print_attacks(out, filename),
        "PrintHGTData" => stats.print_hgt_data(out, filename),
        _ => return None,
    })
}

/// Whether [`print_stats`] has a printer for `method`.
pub fn has_printer(method: &str) -> bool {
    matches!(
        method,
        "PrintAverageData"
            | "PrintDemeAverageData"
            | "PrintErrorData"
            | "PrintVarianceData"
            | "PrintCountData"
            | "PrintMessageLog"
            | "PrintRetMessageLog"
            | "PrintTotalsData"
            | "PrintTasksData"
            | "PrintHostTasksData"
            | "PrintParasiteTasksData"
            | "PrintTasksExeData"
            | "PrintNewTasksData"
            | "PrintNewReactionData"
            | "PrintNewTasksDataPlus"
            | "PrintTasksQualData"
            | "PrintReactionData"
            | "PrintReactionExeData"
            | "PrintCurrentReactionData"
            | "PrintReactionRewardData"
            | "PrintCurrentReactionRewardData"
            | "PrintTimeData"
            | "PrintExtendedTimeData"
            | "PrintMutationRateData"
            | "PrintDivideMutData"
            | "PrintParasiteData"
            | "PrintPreyAverageData"
            | "PrintPredatorAverageData"
            | "PrintTopPredatorAverageData"
            | "PrintPreyErrorData"
            | "PrintPredatorErrorData"
            | "PrintTopPredatorErrorData"
            | "PrintPreyVarianceData"
            | "PrintPredatorVarianceData"
            | "PrintTopPredatorVarianceData"
            | "PrintSenseData"
            | "PrintSenseExeData"
            | "PrintInternalTasksData"
            | "PrintInternalTasksQualData"
            | "PrintCompetitionData"
            | "PrintDemeGLSFounders"
            | "PrintDemeReactionDiversityReplicationData"
            | "PrintDemeCompetitionData"
            | "PrintDemeNetworkData"
            | "PrintDemeFoundersData"
            | "PrintDemesFitnessData"
            | "PrintMultiProcessData"
            | "PrintProfilingData"
            | "PrintGermlineData"
            | "PrintConsensusData"
            | "PrintSimpleConsensusData"
            | "PrintSynchronizationData"
            | "PrintDetailedSynchronizationData"
            | "PrintFlowRateTuples"
            | "PrintNumOrgsKilledData"
            | "PrintMigrationData"
            | "PrintAgePolyethismData"
            | "PrintIntrinsicTaskSwitchingCostData"
            | "PrintMaleAverageData"
            | "PrintFemaleAverageData"
            | "PrintMaleErrorData"
            | "PrintFemaleErrorData"
            | "PrintMaleVarianceData"
            | "PrintFemaleVarianceData"
            | "PrintKaboom"
            | "PrintQuorum"
            | "PrintToleranceInstructionData"
            | "PrintAttacks"
            | "PrintHGTData"
    )
}
