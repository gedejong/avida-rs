//! Event actions as data: the argument parsing half of the `cAction*`
//! classes (avida-core/source/actions/*.cc at 47f13dadb).
//!
//! In C++ every action is a class whose constructor parses the argument
//! string and whose `Process` method reaches into the world. Here the
//! constructor becomes [`parse`], which yields a plain [`Action`] value, and
//! `Process` is a `match` in the driver, so the world can be borrowed mutably
//! while an action runs.
//!
//! An action name that is not listed here is an error at event-file load
//! time ("unsupported action"), never a silent no-op.

use avida_config::feedback::Feedback;
use avida_config::init_file::InitFile;
use avida_config::string_util::{as_double, as_int, pop_word, trim};
use avida_genome::{ArgContainer, ArgSchema, ArgType};

use crate::event_list::parse_event_line;

/// `STATS_OUT_FILE(METHOD, DEFAULT)` table of PrintActions.cc: action name
/// (also the `cStats` method name) and default file name.
pub const STATS_OUT_FILE_ACTIONS: &[(&str, &str)] = &[
    ("PrintAverageData", "average.dat"),
    ("PrintDemeAverageData", "deme_average.dat"),
    ("PrintErrorData", "error.dat"),
    ("PrintVarianceData", "variance.dat"),
    ("PrintCountData", "count.dat"),
    ("PrintMessageData", "message.dat"),
    ("PrintMessageLog", "message_log.dat"),
    ("PrintRetMessageLog", "retmessage_log.dat"),
    ("PrintInterruptData", "interrupt.dat"),
    ("PrintTotalsData", "totals.dat"),
    ("PrintTasksData", "tasks.dat"),
    ("PrintThreadsData", "threads.dat"),
    ("PrintHostTasksData", "host_tasks.dat"),
    ("PrintParasiteTasksData", "parasite_tasks.dat"),
    ("PrintTasksExeData", "tasks_exe.dat"),
    ("PrintNewTasksData", "newtasks.dat"),
    ("PrintNewReactionData", "newreactions.dat"),
    ("PrintNewTasksDataPlus", "newtasksplus.dat"),
    ("PrintTasksQualData", "tasks_quality.dat"),
    ("PrintReactionData", "reactions.dat"),
    ("PrintReactionExeData", "reactions_exe.dat"),
    ("PrintCurrentReactionData", "cur_reactions.dat"),
    ("PrintReactionRewardData", "reaction_reward.dat"),
    ("PrintCurrentReactionRewardData", "cur_reaction_reward.dat"),
    ("PrintTimeData", "time.dat"),
    ("PrintExtendedTimeData", "xtime.dat"),
    ("PrintMutationRateData", "mutation_rates.dat"),
    ("PrintDivideMutData", "divide_mut.dat"),
    ("PrintParasiteData", "parasite.dat"),
    ("PrintPreyAverageData", "prey_average.dat"),
    ("PrintPredatorAverageData", "predator_average.dat"),
    ("PrintTopPredatorAverageData", "top_pred_average.dat"),
    ("PrintPreyErrorData", "prey_error.dat"),
    ("PrintPredatorErrorData", "predator_error.dat"),
    ("PrintTopPredatorErrorData", "top_pred_error.dat"),
    ("PrintPreyVarianceData", "prey_variance.dat"),
    ("PrintPredatorVarianceData", "predator_variance.dat"),
    ("PrintTopPredatorVarianceData", "top_pred_variance.dat"),
    ("PrintSenseData", "sense.dat"),
    ("PrintSenseExeData", "sense_exe.dat"),
    ("PrintInternalTasksData", "in_tasks.dat"),
    ("PrintInternalTasksQualData", "in_tasks_quality.dat"),
    ("PrintSleepData", "sleep.dat"),
    ("PrintCompetitionData", "competition.dat"),
    ("PrintDemeReplicationData", "deme_repl.dat"),
    ("PrintDemeGermlineSequestration", "deme_germ.dat"),
    ("PrintDemeOrgGermlineSequestration", "deme_org_germ.dat"),
    ("PrintDemeGLSFounders", "deme_gls_founders.dat"),
    (
        "PrintDemeReactionDiversityReplicationData",
        "deme_rx_repl.dat",
    ),
    ("PrintDemeGermResourcesData", "deme_germ_res.dat"),
    ("PrintWinningDeme", "deme_winners.dat"),
    (
        "PrintDemeTreatableReplicationData",
        "deme_repl_treatable.dat",
    ),
    (
        "PrintDemeUntreatableReplicationData",
        "deme_repl_untreatable.dat",
    ),
    ("PrintDemeTreatableCount", "deme_treatable.dat"),
    ("PrintDemeCompetitionData", "deme_compete.dat"),
    ("PrintDemeNetworkData", "deme_network.dat"),
    ("PrintDemeNetworkTopology", "deme_network_topology.dat"),
    ("PrintDemeFoundersData", "deme_founders.dat"),
    ("PrintPerDemeTasksData", "per_deme_tasks.dat"),
    ("PrintPerDemeTasksExeData", "per_deme_tasks_exe.dat"),
    ("PrintAvgDemeTasksExeData", "avg_deme_tasks_exe.dat"),
    (
        "PrintAvgTreatableDemeTasksExeData",
        "avg_treatable_deme_tasks_exe.dat",
    ),
    (
        "PrintAvgUntreatableDemeTasksExeData",
        "avg_untreatable_deme_tasks_exe.dat",
    ),
    ("PrintPerDemeReactionData", "per_deme_reactions.dat"),
    // `PrintDemeTasksData` is registered for `cActionPrintDemeTasks` first
    // (pop_print_actions.rs); the `STATS_OUT_FILE` registration is refused.
    ("PrintDemeTasksExeData", "deme_tasks_exe.dat"),
    ("PrintDemesTasksData", "demes_tasks.dat"),
    ("PrintDemesReactionsData", "demes_reactions.dat"),
    ("PrintDemesFitnessData", "demes_fitness.dat"),
    ("PrintDemeReactionData", "deme_reactions.dat"),
    ("PrintDemeOrgTasksData", "deme_org_tasks.dat"),
    ("PrintDemeOrgTasksExeData", "deme_org_tasks_exe.dat"),
    ("PrintDemeOrgReactionData", "deme_org_reactions.dat"),
    ("PrintDemeCurrentTaskExeData", "deme_cur_task_exe.dat"),
    (
        "PrintDemeMigrationSuicidePoints",
        "deme_mig_suicide_points.dat",
    ),
    ("PrintMultiProcessData", "multiprocess.dat"),
    ("PrintProfilingData", "profiling.dat"),
    ("PrintOrganismLocation", "location.dat"),
    ("PrintCurrentTaskCounts", "curr_task_counts.dat"),
    ("PrintGermlineData", "germline.dat"),
    ("PrintPredicatedMessages", "messages.dat"),
    ("PrintCellData", "cell_data.dat"),
    ("PrintConsensusData", "consensus.dat"),
    ("PrintSimpleConsensusData", "simple_consensus.dat"),
    ("PrintCurrentOpinions", "opinions.dat"),
    ("PrintOpinionsSetPerDeme", "opinions_set.dat"),
    (
        "PrintPerDemeGenPerFounderData",
        "deme_gen_between_founders.dat",
    ),
    ("PrintSynchronizationData", "sync.dat"),
    ("PrintDetailedSynchronizationData", "sync-detail.dat"),
    ("PrintCellVisitsData", "visits.dat"),
    ("PrintFlowRateTuples", "flow_rate_tuples.dat"),
    ("PrintDynamicMaxMinData", "maxmin.dat"),
    ("PrintNumOrgsKilledData", "orgs_killed.dat"),
    ("PrintMigrationData", "migration.dat"),
    ("PrintAgePolyethismData", "age_polyethism.dat"),
    (
        "PrintIntrinsicTaskSwitchingCostData",
        "intrinsic_task_switching_cost.dat",
    ),
    ("PrintDenData", "den_data.dat"),
    ("PrintMaleAverageData", "male_average.dat"),
    ("PrintFemaleAverageData", "female_average.dat"),
    ("PrintMaleErrorData", "male_error.dat"),
    ("PrintFemaleErrorData", "female_error.dat"),
    ("PrintMaleVarianceData", "male_variance.dat"),
    ("PrintFemaleVarianceData", "female_variance.dat"),
    ("PrintReputationData", "reputation.dat"),
    ("PrintShadedAltruists", "shadedaltruists.dat"),
    ("PrintDirectReciprocityData", "reciprocity.dat"),
    ("PrintStringMatchData", "stringmatch.dat"),
    ("PrintKaboom", "kabooms.dat"),
    ("PrintQuorum", "threshold.dat"),
    ("PrintGroupsFormedData", "groupformation.dat"),
    ("PrintGroupIds", "groupids.dat"),
    ("PrintTargets", "targets.dat"),
    ("PrintMimicDisplays", "mimics.dat"),
    ("PrintTopPredTargets", "top_pred_targets.dat"),
    ("PrintToleranceInstructionData", "toleranceinstruction.dat"),
    ("PrintToleranceData", "tolerance.dat"),
    ("PrintAttacks", "attacks.dat"),
    ("PrintHGTData", "hgt.dat"),
];

/// A parsed event action. Field names follow the C++ members (`m_` dropped).
#[derive(Debug, Clone, PartialEq)]
pub enum Action {
    /// Any `STATS_OUT_FILE` action: `cStats::<method>(filename)`.
    PrintStats {
        method: &'static str,
        filename: String,
    },
    /// `cActionPrintResourceData`
    PrintResourceData { filename: String, maps: String },
    /// `cActionPrintData`
    PrintData { filename: String, format: String },
    /// `cActionPrintInstructionData`
    PrintInstructionData { filename: String, inst_set: String },
    /// `cActionPrintDominantData`
    PrintDominantData { filename: String },
    /// `cActionPrintDominantGenotype` (empty filename: `archive/<name>.org`)
    PrintDominantGenotype { filename: String },
    /// `cActionInject`
    Inject {
        filename: String,
        cell_id: i32,
        merit: f64,
        lineage_label: i32,
        neutral_metric: f64,
        trace_filename: String,
    },
    /// `cActionInjectAll`
    InjectAll {
        filename: String,
        instset: String,
        merit: f64,
        lineage_label: i32,
        neutral_metric: f64,
    },
    /// `cActionInjectRange`
    InjectRange {
        filename: String,
        cell_start: i32,
        cell_end: i32,
        merit: f64,
        lineage_label: i32,
        neutral_metric: f64,
    },
    /// `cActionInjectSequence`
    InjectSequence {
        sequence: String,
        cell_start: i32,
        cell_end: i32,
        merit: f64,
        lineage_label: i32,
        neutral_metric: f64,
    },
    /// `cActionKillProb`
    KillProb { killprob: f64 },
    /// `cActionZeroMuts`
    ZeroMuts,
    /// `cActionSavePopulation`
    SavePopulation {
        filename: String,
        save_historic: bool,
        save_group_info: bool,
        save_avatars: bool,
        save_rebirth: bool,
    },
    /// `cActionLoadPopulation`
    LoadPopulation {
        filename: String,
        update: i32,
        cellid_offset: i32,
        lineage_offset: i32,
        load_groups: bool,
        load_birth_cells: bool,
        load_avatars: bool,
        load_rebirth: bool,
        load_parent_dat: bool,
        load_traceq: i32,
    },
    /// `cActionSetRandomSeed`
    SetRandomSeed { seed: i32 },
    /// `cActionExit`
    Exit,
    /// `cActionInjectAllRandomRepro`
    InjectAllRandomRepro {
        length: i32,
        merit: f64,
        lineage_label: i32,
        neutral_metric: f64,
        sex: i32,
    },
    /// `cActionInjectWholeGenomeDuplications` (`nopc_prepend == false`) and
    /// `cActionInjectWholeGenomeDuplicationsNopCPrepend`
    InjectWholeGenomeDuplications { nopc_prepend: bool },
    /// `cActionToggleRewardInstruction`
    ToggleRewardInstruction,
    /// `cActionToggleFitnessValley`
    ToggleFitnessValley,
    /// `cActionPrintCurrReactionCountGrid`
    PrintCurrReactionCountGrid { filename: String },
    /// `cActionPrintInstructionAbundanceHistogram`
    PrintInstructionAbundanceHistogram { filename: String, inst_set: String },
    /// `cActionDumpMaxResGrid` (empty filename:
    /// `grid_dumps/max_res_grid.<update>.dat`)
    DumpMaxResGrid { filename: String },
    /// `cActionPrintMatingTypeHistogram`
    PrintMatingTypeHistogram { filename: String },
    /// `cActionPrintBirthChamberMatingTypeHistogram`
    PrintBirthChamberMatingTypeHistogram { filename: String, hw_type: i32 },
    /// `cActionPrintMatingDisplayData`
    PrintMatingDisplayData { filename: String },
    /// `cActionPrintFemaleMatePreferenceData`
    PrintFemaleMatePreferenceData { filename: String },
    /// `cActionPrintBirthChamber` (empty filename:
    /// `birth_chamber/bc-<update>.dat`)
    PrintBirthChamber { filename: String, hw_type: i32 },
    /// `cActionPrintGroupTolerance` (`by_mating_type == false`) and
    /// `cActionPrintGroupMTTolerance`
    PrintGroupTolerance {
        filename: String,
        by_mating_type: bool,
    },
    /// A deme action (deme_actions.rs).
    Deme(crate::deme_actions::DemeAction),
    /// A deme competition action (deme_compete.rs).
    Compete(crate::deme_compete::CompeteAction),
    /// A parasite action (parasites.rs).
    Parasite(crate::parasites::ParasiteAction),
    /// A predator/prey or avatar action (pred_prey_actions.rs).
    PredPrey(crate::pred_prey_actions::PredPreyAction),
    /// A deme or donation statistics printer (pop_print_actions.rs).
    PopPrint(crate::pop_print_actions::PopPrintAction),
    /// A `POP_OUT_FILE` or deme summary printer (actions_stats2.rs).
    Stats2(crate::actions_stats2::Stats2Action),
    /// A grid dump or other printer (actions_print2.rs).
    Print2(crate::actions_print2::Print2Action),
    /// A histogram, genotype-analysis or landscape action (actions_hist.rs).
    Hist(crate::actions_hist::HistAction),
    /// An organism or cell population action (actions_pop2.rs).
    Pop2(crate::actions_pop2::Pop2Action),
    /// A deme population action (actions_deme2.rs).
    Deme2(crate::actions_deme2::Deme2Action),
    /// An environment, driver or save/load action (actions_env.rs).
    Env(crate::actions_env::EnvAction),
    /// A mini or micro trace action (mini_trace.rs).
    MiniTrace(crate::mini_trace::MiniTraceAction),
    /// An action of the third porting round (actions3.rs).
    A3(crate::actions3::Action3),
}

/// World facts the C++ constructors read while parsing.
#[derive(Debug, Clone, Copy)]
pub struct ParseContext<'a> {
    /// `GetHardwareManager().GetDefaultInstSet().GetInstSetName()`
    pub default_inst_set: &'a str,
}

/// Pops `largs.PopWord()` only `if (largs.GetSize())`.
struct Args(String);

impl Args {
    fn word(&mut self) -> Option<String> {
        if self.0.is_empty() {
            None
        } else {
            Some(pop_word(&mut self.0))
        }
    }
    fn int(&mut self, def: i32) -> i32 {
        self.word().map_or(def, |w| as_int(&w))
    }
    fn double(&mut self, def: f64) -> f64 {
        self.word().map_or(def, |w| as_double(&w))
    }
    fn string(&mut self, def: &str) -> String {
        self.word().unwrap_or_else(|| def.to_string())
    }
}

/// Actions with their own constructor (everything outside the
/// `STATS_OUT_FILE` table).
const OTHER_ACTIONS: &[&str] = &[
    "PrintResourceData",
    "PrintData",
    "PrintInstructionData",
    "PrintDominantData",
    "PrintDominantGenotype",
    "Inject",
    "InjectAll",
    "InjectRange",
    "InjectSequence",
    "KillProb",
    "ZeroMuts",
    "SavePopulation",
    "LoadPopulation",
    "SetRandomSeed",
    "Exit",
    "InjectAllRandomRepro",
    "InjectWholeGenomeDuplications",
    "InjectWholeGenomeDuplicationsNopCPrepend",
    "ToggleRewardInstruction",
    "ToggleFitnessValley",
    "PrintCurrReactionCountGrid",
    "PrintInstructionAbundanceHistogram",
    "DumpMaxResGrid",
    "PrintMatingTypeHistogram",
    "PrintBirthChamberMatingTypeHistogram",
    "PrintMatingDisplayData",
    "PrintFemaleMatePreferenceData",
    "PrintBirthChamber",
    "PrintGroupTolerance",
    "PrintGroupMTTolerance",
];

/// The registered spelling of an action. `cActionLibrary` lower-cases keys on
/// registration and on lookup, so event files may spell names in any case
/// (`exit`, `injectsequence`).
pub fn canonical_name(name: &str) -> Option<&'static str> {
    STATS_OUT_FILE_ACTIONS
        .iter()
        .map(|(n, _)| *n)
        .chain(OTHER_ACTIONS.iter().copied())
        .chain(crate::deme_actions::DEME_ACTIONS.iter().copied())
        .chain(crate::deme_compete::COMPETE_ACTIONS.iter().copied())
        .chain(crate::parasites::PARASITE_ACTIONS.iter().copied())
        .chain(crate::pred_prey_actions::PRED_PREY_ACTIONS.iter().copied())
        .chain(crate::pop_print_actions::POP_PRINT_ACTIONS.iter().copied())
        .chain(crate::actions_stats2::STATS2_ACTIONS.iter().copied())
        .chain(crate::actions_print2::PRINT2_ACTIONS.iter().copied())
        .chain(crate::actions_hist::HIST_ACTIONS.iter().copied())
        .chain(crate::actions_pop2::POP2_ACTIONS.iter().copied())
        .chain(crate::actions_deme2::DEME2_ACTIONS.iter().copied())
        .chain(crate::actions_env::ENV_ACTIONS.iter().copied())
        .chain(crate::mini_trace::MINI_TRACE_ACTIONS.iter().copied())
        .chain(crate::actions3::ACTIONS3.iter().copied())
        .find(|n| n.eq_ignore_ascii_case(name))
}

/// Marks an action constructor's `Err` text as what the C++ prints to `cerr`
/// before `exit(1)` while the event file loads: no `error:` prefix, no
/// further feedback, exit status 1 (see [`cerr_exit`]).
pub const CERR_EXIT_MARK: &str = "\u{1}cerr-exit-1\u{1}";

/// The `Err` of an action constructor that does `cerr << line << endl;
/// exit(1);`.
pub fn cerr_exit(lines: &str) -> String {
    format!("{CERR_EXIT_MARK}{lines}\n")
}

/// Is `name` an action this port can parse?
pub fn is_supported(name: &str) -> bool {
    canonical_name(name).is_some()
}

/// Does the C++ action library know `name` (`cActionLibrary::Supports`)?
/// An event naming an action it does not know fails to load with
/// `error: unable to load event '<name>'`, which is the reference
/// behaviour rather than a gap in this port.
pub fn is_cpp_action(name: &str) -> bool {
    crate::action_library::is_registered(name)
}

/// What the C++ prints when `cEventList::LoadEventFile` fails on
/// `event_file`: the `cerr` text of the failing line and the feedback the
/// action constructors before it produced. `None` when the C++ would load
/// the file. A run whose event file the C++ cannot load ends at set-up, so
/// that failure is the reference behaviour even when this port also lacks
/// a setting or instruction the run would have needed.
pub fn cpp_event_file_failure(
    event_file: &str,
    defs: &[(String, String)],
    working_dir: &str,
    ctx: &ParseContext<'_>,
) -> Option<(String, String)> {
    let file = InitFile::with_mappings(event_file, defs, working_dir);
    if !file.was_opened() {
        return Some((String::new(), String::new()));
    }
    let mut feedback = Feedback::new();
    for line_id in 0..file.num_lines() {
        let line = file.get_line(line_id as i32);
        let spec = match parse_event_line(&line) {
            Ok(spec) => spec,
            Err(msg) => return Some((format!("{msg}\n"), feedback.render())),
        };
        if !is_cpp_action(&spec.name) {
            let cerr = format!("error: unable to load event '{}'\n", spec.name);
            return Some((cerr, feedback.render()));
        }
        // Only the constructor's feedback matters here, unless it exits.
        if let Err(msg) = parse(&spec.name, &spec.args, ctx, &mut feedback) {
            if msg.starts_with(CERR_EXIT_MARK) {
                return Some((msg, String::new()));
            }
        }
    }
    None
}

/// `unsupported` (a set-up path this port lacks), unless the C++ would not
/// get that far because it cannot load the event file: then the text the
/// C++ prints, with `feedback` the set-up feedback accumulated so far.
pub fn prefer_event_file_failure(
    unsupported: String,
    event_file: &str,
    defs: &[(String, String)],
    working_dir: &str,
    default_inst_set: &str,
    feedback: &str,
) -> String {
    let ctx = ParseContext { default_inst_set };
    match cpp_event_file_failure(event_file, defs, working_dir, &ctx) {
        Some((cerr, _)) if cerr.starts_with(CERR_EXIT_MARK) => cerr,
        Some((cerr, action_feedback)) => {
            format!("{cerr}{feedback}{action_feedback}error: unable to load event file\n")
        }
        None => unsupported,
    }
}

/// The constructor of `cAction<name>` applied to `args`.
///
/// Schema-based actions report argument errors into `feedback` and, as in
/// C++, keep their member defaults when the argument load fails. `Err` is
/// returned for an action this port does not know and for the one
/// constructor that aborts (`SetRandomSeed` without a seed).
pub fn parse(
    name: &str,
    args: &str,
    ctx: &ParseContext<'_>,
    feedback: &mut Feedback,
) -> Result<Action, String> {
    let Some(name) = canonical_name(name) else {
        return Err(format!("unsupported action '{name}'"));
    };
    if let Some((method, default)) = STATS_OUT_FILE_ACTIONS.iter().find(|(n, _)| *n == name) {
        // `if (largs == "") m_filename = DEFAULT; else m_filename = largs.PopWord();`
        let mut largs = args.to_string();
        let filename = if largs.is_empty() {
            (*default).to_string()
        } else {
            pop_word(&mut largs)
        };
        return Ok(Action::PrintStats { method, filename });
    }

    if let Some(d) = crate::deme_actions::parse(name, args, feedback) {
        return d.map(Action::Deme);
    }
    if let Some(c) = crate::deme_compete::parse(name, args) {
        return Ok(Action::Compete(c));
    }
    if let Some(p) = crate::parasites::parse(name, args) {
        return Ok(Action::Parasite(p));
    }
    if let Some(p) = crate::pred_prey_actions::parse(name, args, ctx.default_inst_set, feedback) {
        return Ok(Action::PredPrey(p));
    }
    if let Some(p) = crate::pop_print_actions::parse(name, args) {
        return Ok(Action::PopPrint(p));
    }
    if let Some(r) = crate::actions_stats2::parse(name, args, ctx, feedback) {
        return r.map(Action::Stats2);
    }
    if let Some(r) = crate::actions_print2::parse(name, args, ctx, feedback) {
        return r.map(Action::Print2);
    }
    if let Some(r) = crate::actions_hist::parse(name, args, ctx, feedback) {
        return r.map(Action::Hist);
    }
    if let Some(r) = crate::actions_pop2::parse(name, args, ctx, feedback) {
        return r.map(Action::Pop2);
    }
    if let Some(r) = crate::actions_deme2::parse(name, args, ctx, feedback) {
        return r.map(Action::Deme2);
    }
    if let Some(r) = crate::actions_env::parse(name, args, ctx, feedback) {
        return r.map(Action::Env);
    }
    if let Some(m) = crate::mini_trace::parse(name, args, feedback) {
        return Ok(Action::MiniTrace(m));
    }
    if let Some(r) = crate::actions3::parse(name, args, ctx, feedback) {
        return r.map(Action::A3);
    }

    let mut a = Args(args.to_string());
    Ok(match name {
        "PrintResourceData" => Action::PrintResourceData {
            filename: a.string("resource.dat"),
            maps: a.string("1"),
        },
        "PrintData" => Action::PrintData {
            filename: pop_word(&mut a.0),
            format: pop_word(&mut a.0),
        },
        "PrintInstructionData" => {
            trim(&mut a.0);
            // An empty argument string yields "instruction.dat"; an explicit
            // empty first word cannot occur after the trim.
            let mut filename = a.string("instruction.dat");
            let inst_set = a.string(ctx.default_inst_set);
            if filename.is_empty() {
                filename = format!("instruction-{inst_set}.dat");
            }
            Action::PrintInstructionData { filename, inst_set }
        }
        "PrintDominantData" => {
            trim(&mut a.0);
            Action::PrintDominantData {
                filename: a.string("dominant.dat"),
            }
        }
        "PrintDominantGenotype" => Action::PrintDominantGenotype {
            filename: a.string(""),
        },
        "Inject" => Action::Inject {
            filename: a.string(""),
            cell_id: a.int(0),
            merit: a.double(-1.0),
            lineage_label: a.int(0),
            neutral_metric: a.double(0.0),
            trace_filename: a.string(""),
        },
        "InjectAll" => {
            let mut schema = ArgSchema::new(':', '=', false);
            schema.add_required("filename", 0, ArgType::String);
            schema.add_string("instset", 1, "");
            schema.add_double("merit", 0, -1.0);
            schema.add_double("neutral_metric", 1, 0.0);
            schema.add_int("lineage_label", 0, 0);
            match ArgContainer::load(args, &schema, feedback) {
                Some(c) => Action::InjectAll {
                    filename: c.get_string(0).to_string(),
                    instset: c.get_string(1).to_string(),
                    merit: c.get_double(0),
                    lineage_label: c.get_int(0),
                    neutral_metric: c.get_double(1),
                },
                None => Action::InjectAll {
                    filename: String::new(),
                    instset: String::new(),
                    merit: -1.0,
                    lineage_label: 0,
                    neutral_metric: 0.0,
                },
            }
        }
        "InjectRange" | "InjectSequence" => {
            // InjectSequence pops its first word unconditionally; on an empty
            // string that is the same empty word.
            let first = a.string("");
            let cell_start = a.int(0);
            let mut cell_end = a.int(-1);
            let merit = a.double(-1.0);
            let lineage_label = a.int(0);
            let neutral_metric = a.double(0.0);
            if cell_end == -1 {
                cell_end = cell_start.wrapping_add(1);
            }
            if name == "InjectRange" {
                Action::InjectRange {
                    filename: first,
                    cell_start,
                    cell_end,
                    merit,
                    lineage_label,
                    neutral_metric,
                }
            } else {
                Action::InjectSequence {
                    sequence: first,
                    cell_start,
                    cell_end,
                    merit,
                    lineage_label,
                    neutral_metric,
                }
            }
        }
        "KillProb" => Action::KillProb {
            killprob: a.double(0.9),
        },
        "ZeroMuts" => Action::ZeroMuts,
        "SavePopulation" => {
            let mut schema = ArgSchema::new(':', '=', false);
            schema.add_string("filename", 0, "detail");
            schema.add_int_range_default("save_historic", 0, 0, 1, 1);
            schema.add_int_range_default("save_groups", 1, 0, 1, 0);
            schema.add_int_range_default("save_avatars", 2, 0, 1, 0);
            schema.add_int_range_default("save_rebirth", 3, 0, 1, 0);
            match ArgContainer::load(args, &schema, feedback) {
                Some(c) => Action::SavePopulation {
                    filename: c.get_string(0).to_string(),
                    save_historic: c.get_int(0) != 0,
                    save_group_info: c.get_int(1) != 0,
                    save_avatars: c.get_int(2) != 0,
                    save_rebirth: c.get_int(3) != 0,
                },
                None => Action::SavePopulation {
                    filename: String::new(),
                    save_historic: true,
                    save_group_info: false,
                    save_avatars: false,
                    save_rebirth: false,
                },
            }
        }
        "LoadPopulation" => Action::LoadPopulation {
            filename: a.string(""),
            update: a.int(-1),
            cellid_offset: a.int(0),
            lineage_offset: a.int(0),
            load_groups: a.int(0) != 0,
            load_birth_cells: a.int(0) != 0,
            load_avatars: a.int(0) != 0,
            load_rebirth: a.int(0) != 0,
            load_parent_dat: a.int(0) != 0,
            load_traceq: a.int(0),
        },
        "SetRandomSeed" => match a.word() {
            Some(w) => Action::SetRandomSeed { seed: as_int(&w) },
            None => return Err("SetRandomSeed event requires an integer seed.".to_string()),
        },
        "Exit" => Action::Exit,
        "InjectAllRandomRepro" => {
            // `m_length = largs.PopWord().AsInt();` pops unconditionally.
            let length = as_int(&pop_word(&mut a.0));
            Action::InjectAllRandomRepro {
                length,
                merit: a.double(-1.0),
                lineage_label: a.int(0),
                neutral_metric: a.double(0.0),
                sex: a.int(0),
            }
        }
        "InjectWholeGenomeDuplications" => Action::InjectWholeGenomeDuplications {
            nopc_prepend: false,
        },
        "InjectWholeGenomeDuplicationsNopCPrepend" => {
            Action::InjectWholeGenomeDuplications { nopc_prepend: true }
        }
        "ToggleRewardInstruction" => Action::ToggleRewardInstruction,
        "ToggleFitnessValley" => Action::ToggleFitnessValley,
        "PrintCurrReactionCountGrid" => Action::PrintCurrReactionCountGrid {
            filename: a.string("curr-reaction-count.dat"),
        },
        "PrintInstructionAbundanceHistogram" => {
            trim(&mut a.0);
            let mut filename = a.string("");
            let inst_set = match a.word() {
                Some(w) => w,
                None => {
                    if filename.is_empty() {
                        filename = "instruction_histogram.dat".to_string();
                    }
                    ctx.default_inst_set.to_string()
                }
            };
            if filename.is_empty() {
                filename = format!("instruction_histogram-{inst_set}.dat");
            }
            Action::PrintInstructionAbundanceHistogram { filename, inst_set }
        }
        "DumpMaxResGrid" => Action::DumpMaxResGrid {
            filename: a.string(""),
        },
        "PrintMatingTypeHistogram" => {
            trim(&mut a.0);
            Action::PrintMatingTypeHistogram {
                filename: a.string("mating_type_histogram.dat"),
            }
        }
        "PrintBirthChamberMatingTypeHistogram" => {
            trim(&mut a.0);
            Action::PrintBirthChamberMatingTypeHistogram {
                filename: a.string("birth_chamber_mating_type_histogram.dat"),
                hw_type: a.int(0),
            }
        }
        "PrintMatingDisplayData" => {
            trim(&mut a.0);
            Action::PrintMatingDisplayData {
                filename: a.string("mating_display_data.dat"),
            }
        }
        "PrintFemaleMatePreferenceData" => {
            trim(&mut a.0);
            Action::PrintFemaleMatePreferenceData {
                filename: a.string("female_mate_preference_data.dat"),
            }
        }
        "PrintBirthChamber" => {
            trim(&mut a.0);
            Action::PrintBirthChamber {
                filename: a.string(""),
                hw_type: a.int(0),
            }
        }
        "PrintGroupTolerance" | "PrintGroupMTTolerance" => {
            // `if (largs == "") m_filename = DEFAULT; else m_filename = largs.PopWord();`
            let by_mating_type = name == "PrintGroupMTTolerance";
            let mut largs = args.to_string();
            let filename = if largs.is_empty() {
                if by_mating_type {
                    "groupMTtolerance.dat".to_string()
                } else {
                    "grouptolerance.dat".to_string()
                }
            } else {
                pop_word(&mut largs)
            };
            Action::PrintGroupTolerance {
                filename,
                by_mating_type,
            }
        }
        _ => unreachable!("canonical_name and parse disagree on '{name}'"),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const CTX: ParseContext<'static> = ParseContext {
        default_inst_set: "heads_default",
    };

    fn p(name: &str, args: &str) -> Action {
        let mut fb = Feedback::new();
        let a = parse(name, args, &CTX, &mut fb).unwrap();
        assert_eq!(fb.num_errors(), 0, "{}", fb.render());
        a
    }

    #[test]
    fn stats_actions_default_and_explicit_filename() {
        assert_eq!(
            p("PrintAverageData", ""),
            Action::PrintStats {
                method: "PrintAverageData",
                filename: "average.dat".into()
            }
        );
        assert_eq!(
            p("PrintTasksQualData", " q.dat extra"),
            Action::PrintStats {
                method: "PrintTasksQualData",
                filename: "q.dat".into()
            }
        );
        // 118 in PrintActions.cc, less `PrintDemeTasksData`, whose
        // registration is refused.
        assert_eq!(STATS_OUT_FILE_ACTIONS.len(), 117);
    }

    #[test]
    fn inject_defaults_and_positional_args() {
        assert_eq!(
            p("Inject", "default-classic.org"),
            Action::Inject {
                filename: "default-classic.org".into(),
                cell_id: 0,
                merit: -1.0,
                lineage_label: 0,
                neutral_metric: 0.0,
                trace_filename: String::new(),
            }
        );
        assert_eq!(
            p("Inject", "a.org 12 3.5 2 0.25 t.trace"),
            Action::Inject {
                filename: "a.org".into(),
                cell_id: 12,
                merit: 3.5,
                lineage_label: 2,
                neutral_metric: 0.25,
                trace_filename: "t.trace".into(),
            }
        );
    }

    #[test]
    fn inject_range_end_defaults_to_start_plus_one() {
        match p("InjectRange", "a.org 7") {
            Action::InjectRange {
                cell_start,
                cell_end,
                ..
            } => assert_eq!((cell_start, cell_end), (7, 8)),
            other => panic!("{other:?}"),
        }
        match p("InjectSequence", "abc 0 100") {
            Action::InjectSequence {
                sequence, cell_end, ..
            } => {
                assert_eq!(sequence, "abc");
                assert_eq!(cell_end, 100);
            }
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn schema_actions() {
        assert_eq!(
            p("SavePopulation", ""),
            Action::SavePopulation {
                filename: "detail".into(),
                save_historic: true,
                save_group_info: false,
                save_avatars: false,
                save_rebirth: false,
            }
        );
        assert_eq!(
            p("SavePopulation", "filename=x:save_historic=0:save_groups=1"),
            Action::SavePopulation {
                filename: "x".into(),
                save_historic: false,
                save_group_info: true,
                save_avatars: false,
                save_rebirth: false,
            }
        );
        assert_eq!(
            p("InjectAll", "filename=default-classic.org"),
            Action::InjectAll {
                filename: "default-classic.org".into(),
                instset: String::new(),
                merit: -1.0,
                lineage_label: 0,
                neutral_metric: 0.0,
            }
        );
        // Missing required argument: reported, members keep their defaults.
        let mut fb = Feedback::new();
        let a = parse("InjectAll", "", &CTX, &mut fb).unwrap();
        assert!(fb.num_errors() > 0);
        assert!(matches!(a, Action::InjectAll { ref filename, .. } if filename.is_empty()));
    }

    #[test]
    fn print_actions_with_special_defaults() {
        assert_eq!(
            p("PrintResourceData", ""),
            Action::PrintResourceData {
                filename: "resource.dat".into(),
                maps: "1".into()
            }
        );
        assert_eq!(
            p("PrintInstructionData", " "),
            Action::PrintInstructionData {
                filename: "instruction.dat".into(),
                inst_set: "heads_default".into()
            }
        );
        assert_eq!(
            p("PrintInstructionData", "i.dat other"),
            Action::PrintInstructionData {
                filename: "i.dat".into(),
                inst_set: "other".into()
            }
        );
        assert_eq!(
            p("PrintDominantGenotype", ""),
            Action::PrintDominantGenotype {
                filename: String::new()
            }
        );
        assert_eq!(p("KillProb", ""), Action::KillProb { killprob: 0.9 });
    }

    #[test]
    fn inject_all_random_repro_args() {
        assert_eq!(
            p("InjectAllRandomRepro", "40"),
            Action::InjectAllRandomRepro {
                length: 40,
                merit: -1.0,
                lineage_label: 0,
                neutral_metric: 0.0,
                sex: 0,
            }
        );
        assert_eq!(
            p("InjectAllRandomRepro", "40 100 0 0 1"),
            Action::InjectAllRandomRepro {
                length: 40,
                merit: 100.0,
                lineage_label: 0,
                neutral_metric: 0.0,
                sex: 1,
            }
        );
        // `PopWord().AsInt()` of an empty argument string is 0.
        assert!(matches!(
            p("InjectAllRandomRepro", ""),
            Action::InjectAllRandomRepro { length: 0, .. }
        ));
    }

    #[test]
    fn argumentless_actions() {
        assert_eq!(
            p("InjectWholeGenomeDuplications", ""),
            Action::InjectWholeGenomeDuplications {
                nopc_prepend: false
            }
        );
        assert_eq!(
            p("InjectWholeGenomeDuplicationsNopCPrepend", "ignored"),
            Action::InjectWholeGenomeDuplications { nopc_prepend: true }
        );
        assert_eq!(
            p("ToggleRewardInstruction", ""),
            Action::ToggleRewardInstruction
        );
        assert_eq!(p("togglefitnessvalley", ""), Action::ToggleFitnessValley);
    }

    #[test]
    fn grid_and_histogram_print_args() {
        assert_eq!(
            p("PrintCurrReactionCountGrid", ""),
            Action::PrintCurrReactionCountGrid {
                filename: "curr-reaction-count.dat".into()
            }
        );
        assert_eq!(
            p("PrintCurrReactionCountGrid", "g.dat"),
            Action::PrintCurrReactionCountGrid {
                filename: "g.dat".into()
            }
        );
        assert_eq!(
            p("PrintInstructionAbundanceHistogram", "  "),
            Action::PrintInstructionAbundanceHistogram {
                filename: "instruction_histogram.dat".into(),
                inst_set: "heads_default".into(),
            }
        );
        assert_eq!(
            p("PrintInstructionAbundanceHistogram", "h.dat"),
            Action::PrintInstructionAbundanceHistogram {
                filename: "h.dat".into(),
                inst_set: "heads_default".into(),
            }
        );
        assert_eq!(
            p("PrintInstructionAbundanceHistogram", "h.dat other"),
            Action::PrintInstructionAbundanceHistogram {
                filename: "h.dat".into(),
                inst_set: "other".into(),
            }
        );
        assert_eq!(
            p("DumpMaxResGrid", ""),
            Action::DumpMaxResGrid {
                filename: String::new()
            }
        );
    }

    #[test]
    fn mating_print_args() {
        assert_eq!(
            p("PrintMatingTypeHistogram", ""),
            Action::PrintMatingTypeHistogram {
                filename: "mating_type_histogram.dat".into()
            }
        );
        assert_eq!(
            p("PrintBirthChamberMatingTypeHistogram", ""),
            Action::PrintBirthChamberMatingTypeHistogram {
                filename: "birth_chamber_mating_type_histogram.dat".into(),
                hw_type: 0,
            }
        );
        assert_eq!(
            p("PrintBirthChamberMatingTypeHistogram", " bc.dat 2 "),
            Action::PrintBirthChamberMatingTypeHistogram {
                filename: "bc.dat".into(),
                hw_type: 2,
            }
        );
        assert_eq!(
            p("PrintMatingDisplayData", ""),
            Action::PrintMatingDisplayData {
                filename: "mating_display_data.dat".into()
            }
        );
        assert_eq!(
            p("PrintFemaleMatePreferenceData", "f.dat"),
            Action::PrintFemaleMatePreferenceData {
                filename: "f.dat".into()
            }
        );
        assert_eq!(
            p("PrintBirthChamber", ""),
            Action::PrintBirthChamber {
                filename: String::new(),
                hw_type: 0,
            }
        );
    }

    #[test]
    fn cpp_library_knows_more_than_this_port() {
        assert!(is_cpp_action("PrintTopNavTrace") && is_supported("PrintNavTrace"));
        assert!(!is_cpp_action("PrintStatsData") && !is_supported("PrintStatsData"));
        // Everything this port parses is a C++ action.
        for (n, _) in STATS_OUT_FILE_ACTIONS {
            assert!(is_cpp_action(n), "{n}");
        }
        for n in OTHER_ACTIONS {
            assert!(is_cpp_action(n), "{n}");
        }
    }

    #[test]
    fn unknown_and_aborting_actions_fail() {
        let mut fb = Feedback::new();
        assert!(parse("NoSuchAction", "", &CTX, &mut fb).is_err());
        assert!(parse("SetRandomSeed", "", &CTX, &mut fb).is_err());
        assert_eq!(p("SetRandomSeed", "42"), Action::SetRandomSeed { seed: 42 });
        assert!(is_supported("Exit") && is_supported("PrintTimeData"));
        assert!(!is_supported("NoSuchAction"));
        assert_eq!(p("exit", ""), Action::Exit);
        assert!(matches!(
            p("injectsequence", "abc"),
            Action::InjectSequence { .. }
        ));
        // Every listed name must reach a parse arm.
        for n in OTHER_ACTIONS {
            let mut fb = Feedback::new();
            let _ = parse(n, "x", &CTX, &mut fb);
        }
    }
}
