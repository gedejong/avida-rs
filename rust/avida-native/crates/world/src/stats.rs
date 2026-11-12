//! Port of `cStats` (avida-core/source/main/cStats.{h,cc} at 47f13dadb).
//!
//! Fields are public and keep their C++ names. `m_world` look-ups are
//! replaced by constructor arguments or method parameters. Ported so far:
//! construction, `ProcessUpdate`, the birth/death/task/reaction counters and
//! the data files `average.dat`, `count.dat`, `tasks.dat`, `tasks_exe.dat`,
//! `tasks_quality.dat`, `resource.dat` and `time.dat`. The other `Print*`
//! methods and the recording methods that feed them are in
//! [`crate::stats_print`], whose module doc lists the printers still missing.
//!
//! The `m_data_manager` entries behind `PrintDataFile` are in
//! [`crate::stats_data`]. Not ported yet: the data-manager provider
//! interface (`PROVIDE`).

use std::collections::BTreeMap;
use std::collections::VecDeque;

use avida_apto::Accumulator;
use avida_apto::OutputManager;
use avida_phenotype::PhenotypeStats;

use crate::stat_tools::{DoubleSum, RunningAverage, RunningStats};
use crate::stats_print::{ConsensusRecord, FlowRateTuple, InstCircumstances, MessageLogEntry};
use crate::topology::geometry;

/// C++: `cStats`
#[derive(Debug, Clone)]
pub struct Stats {
    // Time scales.
    pub m_update: i32,
    pub avida_time: f64,
    pub last_update: i32,

    // Genotype counts (set by the systematics arbiter).
    pub m_num_genotypes: i32,
    pub m_threshold_genotypes: i32,

    // Organism sums, refilled every update by the population.
    pub sum_merit: DoubleSum,
    pub sum_mem_size: DoubleSum,
    pub sum_creature_age: DoubleSum,
    pub sum_generation: DoubleSum,
    pub sum_neutral_metric: DoubleSum,
    pub sum_lineage_label: DoubleSum,
    pub sum_copy_mut_rate: RunningStats,
    pub sum_log_copy_mut_rate: RunningStats,
    pub sum_div_mut_rate: RunningStats,
    pub sum_log_div_mut_rate: RunningStats,
    pub sum_gestation: DoubleSum,
    pub sum_fitness: DoubleSum,
    pub sum_repro_rate: DoubleSum,
    pub rave_true_replication_rate: RunningAverage,
    pub sum_copy_size: DoubleSum,
    pub sum_exe_size: DoubleSum,

    // Extremes.
    pub max_viable_fitness: f64,
    pub max_fitness: f64,
    pub max_merit: f64,
    pub max_gestation_time: i32,
    pub max_genome_length: i32,
    pub min_fitness: f64,
    pub min_merit: f64,
    pub min_gestation_time: i32,
    pub min_genome_length: i32,

    // Counts.
    pub num_births: i32,
    pub cumulative_births: i32,
    pub num_deaths: i32,
    pub num_breed_in: i32,
    pub num_breed_true: i32,
    pub num_breed_true_creatures: i32,
    pub num_creatures: i32,
    pub num_executed: i32,
    pub num_parasites: i32,
    pub num_no_birth_creatures: i32,
    pub num_single_thread_creatures: i32,
    pub num_multi_thread_creatures: i32,
    pub m_num_threads: i32,
    pub num_modified: i32,
    pub tot_organisms: i32,
    pub tot_executed: i64,
    pub num_resamplings: i32,
    pub num_failed_resamplings: i32,
    pub num_migrations: i32,
    pub m_num_successful_mates: i32,
    pub m_spec_total: i32,
    pub m_spec_num: i32,
    pub m_spec_waste: i32,
    pub sense_size: i32,

    // Tasks.
    pub task_cur_count: Vec<i32>,
    pub task_last_count: Vec<i32>,
    pub task_test_count: Vec<i32>,
    pub m_collect_env_test_stats: bool,
    pub tasks_host_current: Vec<i32>,
    pub tasks_host_last: Vec<i32>,
    pub tasks_parasite_current: Vec<i32>,
    pub tasks_parasite_last: Vec<i32>,
    pub task_cur_quality: Vec<f64>,
    pub task_last_quality: Vec<f64>,
    pub task_cur_max_quality: Vec<f64>,
    pub task_last_max_quality: Vec<f64>,
    pub task_exe_count: Vec<i32>,
    pub new_task_count: Vec<i32>,
    pub prev_task_count: Vec<i32>,
    pub cur_task_count: Vec<i32>,
    pub new_reaction_count: Vec<i32>,
    pub task_internal_cur_count: Vec<i32>,
    pub task_internal_last_count: Vec<i32>,
    pub task_internal_cur_quality: Vec<f64>,
    pub task_internal_last_quality: Vec<f64>,
    pub task_internal_cur_max_quality: Vec<f64>,
    pub task_internal_last_max_quality: Vec<f64>,

    // Reactions.
    pub m_reaction_cur_count: Vec<i32>,
    pub m_reaction_last_count: Vec<i32>,
    pub m_reaction_cur_add_reward: Vec<f64>,
    pub m_reaction_last_add_reward: Vec<f64>,
    pub m_reaction_exe_count: Vec<i32>,

    // Senses.
    pub sense_last_count: Vec<i32>,
    pub sense_last_exe_count: Vec<i32>,

    // Resources.
    pub resource_count: Vec<f64>,
    pub resource_geometry: Vec<i32>,
    pub spatial_res_count: Vec<Vec<f64>>,
    pub m_resource_print_thresh: f64,

    // Names.
    pub task_names: Vec<String>,
    pub reaction_names: Vec<String>,
    pub resource_names: Vec<String>,

    // Energy testament.
    pub energy_testament_accepted_by_organisms: DoubleSum,
    /// The other `cStats` energy-testament sums (written, never printed).
    pub energy: crate::energy::EnergyStats,

    // Task/age and task-switch tracking.
    pub reaction_age_map: BTreeMap<i32, DoubleSum>,
    pub intrinsic_task_switch_time: BTreeMap<(i32, i32), DoubleSum>,

    // ---- Fields read by the printers in `stats_print.rs` ----

    // Instruction counts, keyed by instruction-set name (`Apto::Map`).
    pub m_is_inst_names_map: BTreeMap<String, Vec<String>>,
    pub m_is_from_message_inst_map: BTreeMap<String, Vec<Accumulator<i32>>>,
    pub m_is_tolerance_exe_counts: Vec<(i32, i32)>,
    pub m_is_tolerance_exe_insts: Vec<InstCircumstances>,
    pub m_is_prey_exe_inst_map: BTreeMap<String, Vec<Accumulator<i32>>>,
    pub m_is_pred_exe_inst_map: BTreeMap<String, Vec<Accumulator<i32>>>,
    pub m_is_prey_fail_exe_inst_map: BTreeMap<String, Vec<Accumulator<i32>>>,
    pub m_is_pred_fail_exe_inst_map: BTreeMap<String, Vec<Accumulator<i32>>>,
    pub m_is_tpred_exe_inst_map: BTreeMap<String, Vec<Accumulator<i32>>>,
    pub m_is_prey_from_sensor_inst_map: BTreeMap<String, Vec<Accumulator<i32>>>,
    pub m_is_pred_from_sensor_inst_map: BTreeMap<String, Vec<Accumulator<i32>>>,
    pub m_is_tpred_from_sensor_inst_map: BTreeMap<String, Vec<Accumulator<i32>>>,
    pub m_group_attack_names: BTreeMap<String, Vec<String>>,
    pub m_group_attack_exe_map: BTreeMap<String, BTreeMap<String, Vec<Accumulator<i32>>>>,
    pub m_is_male_exe_inst_map: BTreeMap<String, Vec<Accumulator<i32>>>,
    pub m_is_female_exe_inst_map: BTreeMap<String, Vec<Accumulator<i32>>>,

    // Kaboom, quorum and division-of-labour stats.
    pub num_kabooms: i32,
    pub num_kabooms_pre: i32,
    pub num_kabooms_post: i32,
    pub num_kaboom_kills: i32,
    pub num_sa_kin: i32,
    pub num_sa_notkin: i32,
    pub num_nsa_kin: i32,
    pub num_nsa_notkin: i32,
    pub sum_perc_lyse: f64,
    pub sum_cpu_cycles: f64,
    pub hd_list: Vec<i32>,
    pub num_stop_explode: i32,
    pub ave_threshold_ub: i32,
    pub num_quorum: i32,
    pub juv_killed: i32,
    pub num_guard_fail: i32,

    // Sense instruction names.
    pub sense_names: Vec<String>,

    // Competition stats.
    pub avg_trial_fitnesses: Vec<f64>,
    pub avg_competition_fitness: f64,
    pub min_competition_fitness: f64,
    pub max_competition_fitness: f64,
    pub avg_competition_copied_fitness: f64,
    pub min_competition_copied_fitness: f64,
    pub max_competition_copied_fitness: f64,
    pub num_orgs_replicated: i32,

    // Deme stats.
    pub sum_deme_age: Accumulator<i32>,
    pub sum_deme_birth_count: Accumulator<i32>,
    pub sum_deme_last_birth_count: Accumulator<i32>,
    pub sum_deme_org_count: Accumulator<i32>,
    pub sum_deme_last_org_count: Accumulator<i32>,
    pub sum_deme_generation: Accumulator<i32>,
    pub sum_deme_gestation_time: Accumulator<i32>,
    pub sum_deme_normalized_time_used: DoubleSum,
    pub sum_deme_merit: DoubleSum,
    pub sum_deme_generations_per_lifetime: DoubleSum,
    pub m_num_occupied_demes: i32,
    pub sum_deme_events_killed: Accumulator<i32>,
    pub sum_deme_events_kill_attempts: Accumulator<i32>,
    pub flow_rate_tuples: BTreeMap<i32, FlowRateTuple>,

    // Organism kill stats.
    pub sum_orgs_killed: Accumulator<i32>,
    pub sum_unoccupied_cell_kill_attempts: Accumulator<i32>,
    pub sum_cells_scanned_at_kill: Accumulator<i32>,

    // Pred-prey stats.
    pub sum_prey_fitness: DoubleSum,
    pub sum_prey_gestation: DoubleSum,
    pub sum_prey_merit: DoubleSum,
    pub sum_prey_creature_age: DoubleSum,
    pub sum_prey_generation: DoubleSum,
    pub sum_prey_size: DoubleSum,
    pub sum_pred_fitness: DoubleSum,
    pub sum_pred_gestation: DoubleSum,
    pub sum_pred_merit: DoubleSum,
    pub sum_pred_creature_age: DoubleSum,
    pub sum_pred_generation: DoubleSum,
    pub sum_pred_size: DoubleSum,
    pub sum_tpred_fitness: DoubleSum,
    pub sum_tpred_gestation: DoubleSum,
    pub sum_tpred_merit: DoubleSum,
    pub sum_tpred_creature_age: DoubleSum,
    pub sum_tpred_generation: DoubleSum,
    pub sum_tpred_size: DoubleSum,
    pub sum_attacks: DoubleSum,
    pub sum_kills: DoubleSum,
    pub prey_entropy: f64,
    pub pred_entropy: f64,
    pub tpred_entropy: f64,

    // Mating-type (male/female) stats.
    pub sum_male_fitness: DoubleSum,
    pub sum_male_gestation: DoubleSum,
    pub sum_male_merit: DoubleSum,
    pub sum_male_creature_age: DoubleSum,
    pub sum_male_generation: DoubleSum,
    pub sum_male_size: DoubleSum,
    pub sum_female_fitness: DoubleSum,
    pub sum_female_gestation: DoubleSum,
    pub sum_female_merit: DoubleSum,
    pub sum_female_creature_age: DoubleSum,
    pub sum_female_generation: DoubleSum,
    pub sum_female_size: DoubleSum,

    // Message logs.
    pub m_message_log: Vec<MessageLogEntry>,
    pub m_retmessage_log: Vec<MessageLogEntry>,

    // Germlines and deme founders.
    pub m_germline_generation: DoubleSum,
    pub m_gls_deme_founders: BTreeMap<(i32, i32), Vec<(i32, String)>>,
    pub m_deme_founders: BTreeMap<i32, Vec<i32>>,

    // Deme competition.
    pub m_deme_fitness: Vec<f64>,

    // Synchronization (update -> deme id -> deme-relative cell ids).
    pub m_flash_count: i32,
    pub m_flash_times: BTreeMap<i32, BTreeMap<i32, Vec<i32>>>,

    // Consensus.
    pub m_consensi: Vec<ConsensusRecord>,

    // Division of labour (deme reaction diversity at replication).
    pub m_switching: VecDeque<f64>,
    pub m_deme_diversity: VecDeque<f64>,
    pub m_shannon_div: VecDeque<f64>,
    pub m_shannon_div_norm: VecDeque<f64>,
    pub m_num_orgs_perf_reaction: VecDeque<f64>,
    pub m_percent_reproductives: VecDeque<f64>,

    // Reputation and string matching.
    pub m_donate_to_donor: i32,
    pub m_donate_to_facing: i32,
    pub m_string_bits_matched: BTreeMap<String, DoubleSum>,
    pub m_perfect_match: DoubleSum,
    pub m_perfect_match_org: DoubleSum,
    pub m_tags: BTreeMap<i32, i32>,

    // Deme networks, HGT, multiprocess and profiling.
    pub m_network_stats: BTreeMap<String, DoubleSum>,
    pub m_hgt_metabolized: DoubleSum,
    pub m_hgt_inserted: DoubleSum,
    pub m_outgoing: DoubleSum,
    pub m_incoming: DoubleSum,
    pub m_profiling: BTreeMap<String, DoubleSum>,
}

impl Stats {
    /// C++: `cStats::cStats(cWorld*)`. `task_names` are the task
    /// descriptions (`env.GetTask(i).GetDesc()`), `reaction_names` come from
    /// `env.GetReactionName(i)`, `num_resources` is `world->GetNumResources()`
    /// and `res_for_deme_rep` is `RES_FOR_DEME_REP`.
    pub fn new(
        task_names: Vec<String>,
        reaction_names: Vec<String>,
        num_resources: usize,
        res_for_deme_rep: f64,
    ) -> Self {
        let num_tasks = task_names.len();
        let num_reactions = reaction_names.len();
        Stats {
            m_update: -1,
            avida_time: 0.0,
            last_update: 0,
            m_num_genotypes: 0,
            // Left uninitialised by the C++ constructor.
            m_threshold_genotypes: 0,
            sum_merit: DoubleSum::new(),
            sum_mem_size: DoubleSum::new(),
            sum_creature_age: DoubleSum::new(),
            sum_generation: DoubleSum::new(),
            sum_neutral_metric: DoubleSum::new(),
            sum_lineage_label: DoubleSum::new(),
            sum_copy_mut_rate: RunningStats::new(),
            sum_log_copy_mut_rate: RunningStats::new(),
            sum_div_mut_rate: RunningStats::new(),
            sum_log_div_mut_rate: RunningStats::new(),
            sum_gestation: DoubleSum::new(),
            sum_fitness: DoubleSum::new(),
            sum_repro_rate: DoubleSum::new(),
            rave_true_replication_rate: RunningAverage::new(500),
            sum_copy_size: DoubleSum::new(),
            sum_exe_size: DoubleSum::new(),
            max_viable_fitness: 0.0,
            max_fitness: 0.0,
            max_merit: 0.0,
            max_gestation_time: 0,
            max_genome_length: 0,
            min_fitness: f64::from(f32::MAX),
            min_merit: f64::from(f32::MAX),
            min_gestation_time: i32::MAX,
            min_genome_length: i32::MAX,
            num_births: 0,
            cumulative_births: 0,
            num_deaths: 0,
            num_breed_in: 0,
            num_breed_true: 0,
            num_breed_true_creatures: 0,
            num_creatures: 0,
            num_executed: 0,
            num_parasites: 0,
            num_no_birth_creatures: 0,
            num_single_thread_creatures: 0,
            num_multi_thread_creatures: 0,
            m_num_threads: 0,
            num_modified: 0,
            tot_organisms: 0,
            tot_executed: 0,
            num_resamplings: 0,
            num_failed_resamplings: 0,
            num_migrations: 0,
            m_num_successful_mates: 0,
            m_spec_total: 0,
            m_spec_num: 0,
            m_spec_waste: 0,
            sense_size: 0,
            task_cur_count: vec![0; num_tasks],
            task_last_count: vec![0; num_tasks],
            task_test_count: vec![0; num_tasks],
            m_collect_env_test_stats: false,
            tasks_host_current: vec![0; num_tasks],
            tasks_host_last: vec![0; num_tasks],
            tasks_parasite_current: vec![0; num_tasks],
            tasks_parasite_last: vec![0; num_tasks],
            task_cur_quality: vec![0.0; num_tasks],
            task_last_quality: vec![0.0; num_tasks],
            task_cur_max_quality: vec![0.0; num_tasks],
            task_last_max_quality: vec![0.0; num_tasks],
            task_exe_count: vec![0; num_tasks],
            new_task_count: vec![0; num_tasks],
            prev_task_count: vec![0; num_tasks],
            cur_task_count: vec![0; num_tasks],
            new_reaction_count: vec![0; num_reactions],
            task_internal_cur_count: vec![0; num_tasks],
            task_internal_last_count: vec![0; num_tasks],
            task_internal_cur_quality: vec![0.0; num_tasks],
            task_internal_last_quality: vec![0.0; num_tasks],
            task_internal_cur_max_quality: vec![0.0; num_tasks],
            task_internal_last_max_quality: vec![0.0; num_tasks],
            m_reaction_cur_count: vec![0; num_reactions],
            m_reaction_last_count: vec![0; num_reactions],
            m_reaction_cur_add_reward: vec![0.0; num_reactions],
            m_reaction_last_add_reward: vec![0.0; num_reactions],
            m_reaction_exe_count: vec![0; num_reactions],
            sense_last_count: Vec::new(),
            sense_last_exe_count: Vec::new(),
            resource_count: vec![0.0; num_resources],
            resource_geometry: vec![geometry::GLOBAL; num_resources],
            spatial_res_count: Vec::new(),
            m_resource_print_thresh: res_for_deme_rep,
            task_names,
            reaction_names,
            resource_names: vec![String::new(); num_resources],
            energy_testament_accepted_by_organisms: DoubleSum::new(),
            energy: crate::energy::EnergyStats::default(),
            reaction_age_map: BTreeMap::new(),
            intrinsic_task_switch_time: BTreeMap::new(),

            // ---- Fields read by the printers in `stats_print.rs` ----
            // QUIRK: the C++ constructor leaves num_kabooms_pre/post,
            // num_sa_kin, num_sa_notkin, num_nsa_kin, num_nsa_notkin,
            // sum_perc_lyse, sum_cpu_cycles, num_stop_explode,
            // ave_threshold_ub, num_quorum, tpred_entropy,
            // m_num_occupied_demes and m_flash_count uninitialised; they
            // start at zero here.
            m_is_inst_names_map: BTreeMap::new(),
            m_is_from_message_inst_map: BTreeMap::new(),
            m_is_tolerance_exe_counts: Vec::new(),
            m_is_tolerance_exe_insts: Vec::new(),
            m_is_prey_exe_inst_map: BTreeMap::new(),
            m_is_pred_exe_inst_map: BTreeMap::new(),
            m_is_prey_fail_exe_inst_map: BTreeMap::new(),
            m_is_pred_fail_exe_inst_map: BTreeMap::new(),
            m_is_tpred_exe_inst_map: BTreeMap::new(),
            m_is_prey_from_sensor_inst_map: BTreeMap::new(),
            m_is_pred_from_sensor_inst_map: BTreeMap::new(),
            m_is_tpred_from_sensor_inst_map: BTreeMap::new(),
            m_group_attack_names: BTreeMap::new(),
            m_group_attack_exe_map: BTreeMap::new(),
            m_is_male_exe_inst_map: BTreeMap::new(),
            m_is_female_exe_inst_map: BTreeMap::new(),
            num_kabooms: 0,
            num_kabooms_pre: 0,
            num_kabooms_post: 0,
            num_kaboom_kills: 0,
            num_sa_kin: 0,
            num_sa_notkin: 0,
            num_nsa_kin: 0,
            num_nsa_notkin: 0,
            sum_perc_lyse: 0.0,
            sum_cpu_cycles: 0.0,
            hd_list: Vec::new(),
            num_stop_explode: 0,
            ave_threshold_ub: 0,
            num_quorum: 0,
            juv_killed: 0,
            num_guard_fail: 0,
            sense_names: Vec::new(),
            avg_trial_fitnesses: Vec::new(),
            avg_competition_fitness: 0.0,
            min_competition_fitness: 0.0,
            max_competition_fitness: 0.0,
            avg_competition_copied_fitness: 0.0,
            min_competition_copied_fitness: 0.0,
            max_competition_copied_fitness: 0.0,
            num_orgs_replicated: 0,
            sum_deme_age: Accumulator::new(),
            sum_deme_birth_count: Accumulator::new(),
            sum_deme_last_birth_count: Accumulator::new(),
            sum_deme_org_count: Accumulator::new(),
            sum_deme_last_org_count: Accumulator::new(),
            sum_deme_generation: Accumulator::new(),
            sum_deme_gestation_time: Accumulator::new(),
            sum_deme_normalized_time_used: DoubleSum::new(),
            sum_deme_merit: DoubleSum::new(),
            sum_deme_generations_per_lifetime: DoubleSum::new(),
            m_num_occupied_demes: 0,
            sum_deme_events_killed: Accumulator::new(),
            sum_deme_events_kill_attempts: Accumulator::new(),
            flow_rate_tuples: BTreeMap::new(),
            sum_orgs_killed: Accumulator::new(),
            sum_unoccupied_cell_kill_attempts: Accumulator::new(),
            sum_cells_scanned_at_kill: Accumulator::new(),
            sum_prey_fitness: DoubleSum::new(),
            sum_prey_gestation: DoubleSum::new(),
            sum_prey_merit: DoubleSum::new(),
            sum_prey_creature_age: DoubleSum::new(),
            sum_prey_generation: DoubleSum::new(),
            sum_prey_size: DoubleSum::new(),
            sum_pred_fitness: DoubleSum::new(),
            sum_pred_gestation: DoubleSum::new(),
            sum_pred_merit: DoubleSum::new(),
            sum_pred_creature_age: DoubleSum::new(),
            sum_pred_generation: DoubleSum::new(),
            sum_pred_size: DoubleSum::new(),
            sum_tpred_fitness: DoubleSum::new(),
            sum_tpred_gestation: DoubleSum::new(),
            sum_tpred_merit: DoubleSum::new(),
            sum_tpred_creature_age: DoubleSum::new(),
            sum_tpred_generation: DoubleSum::new(),
            sum_tpred_size: DoubleSum::new(),
            sum_attacks: DoubleSum::new(),
            sum_kills: DoubleSum::new(),
            prey_entropy: 0.0,
            pred_entropy: 0.0,
            tpred_entropy: 0.0,
            sum_male_fitness: DoubleSum::new(),
            sum_male_gestation: DoubleSum::new(),
            sum_male_merit: DoubleSum::new(),
            sum_male_creature_age: DoubleSum::new(),
            sum_male_generation: DoubleSum::new(),
            sum_male_size: DoubleSum::new(),
            sum_female_fitness: DoubleSum::new(),
            sum_female_gestation: DoubleSum::new(),
            sum_female_merit: DoubleSum::new(),
            sum_female_creature_age: DoubleSum::new(),
            sum_female_generation: DoubleSum::new(),
            sum_female_size: DoubleSum::new(),
            m_message_log: Vec::new(),
            m_retmessage_log: Vec::new(),
            m_germline_generation: DoubleSum::new(),
            m_gls_deme_founders: BTreeMap::new(),
            m_deme_founders: BTreeMap::new(),
            m_deme_fitness: Vec::new(),
            m_flash_count: 0,
            m_flash_times: BTreeMap::new(),
            m_consensi: Vec::new(),
            m_switching: VecDeque::new(),
            m_deme_diversity: VecDeque::new(),
            m_shannon_div: VecDeque::new(),
            m_shannon_div_norm: VecDeque::new(),
            m_num_orgs_perf_reaction: VecDeque::new(),
            m_percent_reproductives: VecDeque::new(),
            m_donate_to_donor: 0,
            m_donate_to_facing: 0,
            m_string_bits_matched: BTreeMap::new(),
            m_perfect_match: DoubleSum::new(),
            m_perfect_match_org: DoubleSum::new(),
            m_tags: BTreeMap::new(),
            m_network_stats: BTreeMap::new(),
            m_hgt_metabolized: DoubleSum::new(),
            m_hgt_inserted: DoubleSum::new(),
            m_outgoing: DoubleSum::new(),
            m_incoming: DoubleSum::new(),
            m_profiling: BTreeMap::new(),
        }
    }

    /// C++: `GetUpdate`
    pub fn update(&self) -> i32 {
        self.m_update
    }

    /// C++: `IncCurrentUpdate`
    pub fn inc_current_update(&mut self) {
        self.m_update += 1;
    }

    /// C++: `SetCurrentUpdate`
    pub fn set_current_update(&mut self, new_update: i32) {
        self.m_update = new_update;
    }

    /// C++: `ZeroTasks`
    pub fn zero_tasks(&mut self) {
        self.task_cur_count.fill(0);
        self.task_last_count.fill(0);
        self.task_test_count.fill(0);
        self.tasks_host_current.fill(0);
        self.tasks_host_last.fill(0);
        self.tasks_parasite_current.fill(0);
        self.tasks_parasite_last.fill(0);
        self.task_cur_quality.fill(0.0);
        self.task_last_quality.fill(0.0);
        self.task_last_max_quality.fill(0.0);
        self.task_cur_max_quality.fill(0.0);
        self.task_internal_cur_count.fill(0);
        self.task_internal_cur_quality.fill(0.0);
        self.task_internal_cur_max_quality.fill(0.0);
        self.task_internal_last_count.fill(0);
        self.task_internal_last_quality.fill(0.0);
        self.task_internal_last_max_quality.fill(0.0);
    }

    /// C++: `ZeroReactions`
    pub fn zero_reactions(&mut self) {
        self.m_reaction_cur_count.fill(0);
        self.m_reaction_last_count.fill(0);
        self.m_reaction_cur_add_reward.fill(0.0);
        self.m_reaction_last_add_reward.fill(0.0);
    }

    /// C++: `RecordBirth`, without its leading birth-interrupt check: the
    /// caller runs `EventList::check_birth_interrupt_queue(tot_organisms)`
    /// and `process_interrupt` first, since that needs the whole world.
    pub fn record_birth(&mut self, breed_true: bool) {
        self.tot_organisms += 1;
        self.num_births += 1;
        self.cumulative_births += 1;
        if breed_true {
            self.num_breed_true += 1;
        } else {
            self.num_breed_in += 1;
        }
    }

    /// C++: `RecordDeath`
    pub fn record_death(&mut self) {
        self.num_deaths += 1;
    }

    /// C++: `IncExecuted`
    pub fn inc_executed(&mut self) {
        self.num_executed += 1;
    }

    /// C++: `ProcessUpdate`. `ave_time_slice` is `AVE_TIME_SLICE`.
    pub fn process_update(&mut self, ave_time_slice: i32) {
        if self.sum_merit.count() > 0.0 && self.sum_merit.average() > 0.0 {
            let delta = f64::from(self.m_update - self.last_update) / self.sum_merit.average();
            self.avida_time += delta;
            self.rave_true_replication_rate.add(
                f64::from(self.num_births)
                    / (delta * f64::from(ave_time_slice) * f64::from(self.num_creatures)),
            );
        }
        self.last_update = self.m_update;

        self.num_births = 0;
        self.num_deaths = 0;
        self.num_breed_true = 0;

        self.tot_executed += i64::from(self.num_executed);
        self.num_executed = 0;

        self.task_cur_count.fill(0);
        self.task_last_count.fill(0);
        self.task_test_count.fill(0);
        self.task_cur_quality.fill(0.0);
        self.task_last_quality.fill(0.0);
        self.task_cur_max_quality.fill(0.0);
        self.task_last_max_quality.fill(0.0);
        self.task_exe_count.fill(0);

        self.task_internal_cur_count.fill(0);
        self.task_internal_last_count.fill(0);
        self.task_internal_cur_quality.fill(0.0);
        self.task_internal_last_quality.fill(0.0);
        self.task_internal_cur_max_quality.fill(0.0);
        self.task_internal_last_max_quality.fill(0.0);

        self.sense_last_count.fill(0);
        self.sense_last_exe_count.fill(0);

        self.m_reaction_cur_count.fill(0);
        self.m_reaction_last_count.fill(0);
        self.m_reaction_cur_add_reward.fill(0.0);
        self.m_reaction_last_add_reward.fill(0.0);
        self.m_reaction_exe_count.fill(0);

        self.max_fitness = 0.0;

        self.num_resamplings = 0;
        self.num_failed_resamplings = 0;

        self.m_spec_total = 0;
        self.m_spec_num = 0;
        self.m_spec_waste = 0;

        self.num_migrations = 0;
        self.m_num_successful_mates = 0;
    }

    // ---- task and reaction counters filled by the population ----

    pub fn add_cur_task(&mut self, task_num: usize) {
        self.task_cur_count[task_num] += 1;
    }

    pub fn add_cur_task_quality(&mut self, task_num: usize, quality: f64) {
        self.task_cur_quality[task_num] += quality;
        if quality > self.task_cur_max_quality[task_num] {
            self.task_cur_max_quality[task_num] = quality;
        }
    }

    pub fn add_last_task(&mut self, task_num: usize) {
        self.task_last_count[task_num] += 1;
    }

    pub fn add_last_task_quality(&mut self, task_num: usize, quality: f64) {
        self.task_last_quality[task_num] += quality;
        if quality > self.task_last_max_quality[task_num] {
            self.task_last_max_quality[task_num] = quality;
        }
    }

    pub fn inc_task_exe_count(&mut self, task_num: usize, task_count: i32) {
        self.task_exe_count[task_num] += task_count;
    }

    pub fn add_cur_internal_task(&mut self, task_num: usize) {
        self.task_internal_cur_count[task_num] += 1;
    }

    pub fn add_cur_internal_task_quality(&mut self, task_num: usize, quality: f64) {
        self.task_internal_cur_quality[task_num] += quality;
        if quality > self.task_internal_cur_max_quality[task_num] {
            self.task_internal_cur_max_quality[task_num] = quality;
        }
    }

    pub fn add_last_internal_task(&mut self, task_num: usize) {
        self.task_internal_last_count[task_num] += 1;
    }

    pub fn add_last_internal_task_quality(&mut self, task_num: usize, quality: f64) {
        self.task_internal_last_quality[task_num] += quality;
        if quality > self.task_internal_last_max_quality[task_num] {
            self.task_internal_last_max_quality[task_num] = quality;
        }
    }

    pub fn add_cur_reaction(&mut self, reaction: usize) {
        self.m_reaction_cur_count[reaction] += 1;
    }

    pub fn add_last_reaction(&mut self, reaction: usize) {
        self.m_reaction_last_count[reaction] += 1;
    }

    pub fn add_cur_reaction_add_reward(&mut self, reaction: usize, reward: f64) {
        self.m_reaction_cur_add_reward[reaction] += reward;
    }

    pub fn add_last_reaction_add_reward(&mut self, reaction: usize, reward: f64) {
        self.m_reaction_last_add_reward[reaction] += reward;
    }

    pub fn inc_reaction_exe_count(&mut self, reaction: usize, count: i32) {
        self.m_reaction_exe_count[reaction] += count;
    }

    // ---- data files ----

    /// C++: `PrintAverageData`
    pub fn print_average_data(
        &self,
        out: &mut OutputManager,
        filename: &str,
    ) -> Result<(), String> {
        let df = out.static_with_path(filename)?;
        df.write_comment("Avida Average Data");
        df.write_time_stamp();

        df.write(self.m_update, "Update");
        df.write(self.sum_merit.average(), "Merit");
        df.write(self.sum_gestation.average(), "Gestation Time");
        df.write(self.sum_fitness.average(), "Fitness");
        df.write(self.sum_repro_rate.average(), "Repro Rate?");
        df.write(0, "(deprecated) Size");
        df.write(self.sum_copy_size.average(), "Copied Size");
        df.write(self.sum_exe_size.average(), "Executed Size");
        df.write(0, "(deprecated) Abundance");

        let mut ave_births = 0.0;
        let mut ave_breed_true = 0.0;
        if self.num_creatures > 0 {
            let d_num_creatures = f64::from(self.num_creatures);
            ave_births = f64::from(self.num_births) / d_num_creatures;
            ave_breed_true = f64::from(self.num_breed_true) / d_num_creatures;
        }
        df.write(
            ave_births,
            "Proportion of organisms that gave birth in this update",
        );
        df.write(ave_breed_true, "Proportion of Breed True Organisms");

        df.write(0, "(deprecated) Genotype Depth");
        df.write(self.sum_generation.average(), "Generation");
        df.write(self.sum_neutral_metric.average(), "Neutral Metric");
        df.write(self.sum_lineage_label.average(), "Lineage Label");
        df.write(
            self.rave_true_replication_rate.average(),
            "True Replication Rate (based on births/update, time-averaged)",
        );
        df.endl();
        Ok(())
    }

    /// C++: `PrintCountData`
    pub fn print_count_data(&self, out: &mut OutputManager, filename: &str) -> Result<(), String> {
        let df = out.static_with_path(filename)?;
        df.write_comment("Avida count data");
        df.write_time_stamp();

        df.write(self.m_update, "update");
        df.write(self.num_executed, "number of insts executed this update");
        df.write(self.num_creatures, "number of organisms");
        df.write(self.m_num_genotypes, "number of different genotypes");
        df.write(
            self.m_threshold_genotypes,
            "number of different threshold genotypes",
        );
        df.write(0, "(deprecated) number of different species");
        df.write(0, "(deprecated) number of different threshold species");
        df.write(0, "(deprecated) number of different lineages");
        df.write(self.num_births, "number of births in this update");
        df.write(self.num_deaths, "number of deaths in this update");
        df.write(self.num_breed_true, "number of breed true");
        df.write(
            self.num_breed_true_creatures,
            "number of breed true organisms?",
        );
        df.write(self.num_no_birth_creatures, "number of no-birth organisms");
        df.write(
            self.num_single_thread_creatures,
            "number of single-threaded organisms",
        );
        df.write(
            self.num_multi_thread_creatures,
            "number of multi-threaded organisms",
        );
        df.write(self.num_modified, "number of modified organisms");
        df.endl();
        Ok(())
    }

    /// C++: `PrintTasksData`. The file name `tasksq.dat` is a flag that also
    /// prints `taskquality.dat`.
    pub fn print_tasks_data(&self, out: &mut OutputManager, filename: &str) -> Result<(), String> {
        let mut file = filename;
        if filename == "tasksq.dat" {
            file = "tasks.dat";
            self.print_tasks_qual_data(out, "taskquality.dat")?;
        }
        let df = out.static_with_path(file)?;
        df.write_comment("Avida tasks data");
        df.write_time_stamp();
        df.write_comment("First column gives the current update, next columns give the number");
        df.write_comment(
            "of organisms that have the particular task as a component of their merit",
        );

        df.write(self.m_update, "Update");
        for i in 0..self.task_last_count.len() {
            df.write(self.task_last_count[i], &self.task_names[i]);
        }
        df.endl();
        Ok(())
    }

    /// C++: `PrintTasksExeData`
    pub fn print_tasks_exe_data(
        &self,
        out: &mut OutputManager,
        filename: &str,
    ) -> Result<(), String> {
        let df = out.static_with_path(filename)?;
        df.write_comment("Avida tasks execution data");
        df.write_time_stamp();
        df.write_comment(
            "First column gives the current update, all further columns give the number",
        );
        df.write_comment("of times the particular task has been executed this update.");

        df.write(self.m_update, "Update");
        for i in 0..self.task_exe_count.len() {
            df.write(self.task_exe_count[i], &self.task_names[i]);
        }
        df.endl();
        Ok(())
    }

    /// C++: `PrintTasksQualData`
    pub fn print_tasks_qual_data(
        &self,
        out: &mut OutputManager,
        filename: &str,
    ) -> Result<(), String> {
        let df = out.static_with_path(filename)?;
        df.write_comment("Avida tasks quality data");
        df.write_time_stamp();
        df.write_comment(
            "First column gives the current update, rest give average and max task quality",
        );
        df.write(self.m_update, "Update");
        for i in 0..self.task_last_count.len() {
            let mut qual = 0.0;
            if self.task_last_count[i] > 0 {
                qual = self.task_last_quality[i] / f64::from(self.task_last_count[i]);
            }
            df.write(qual, &format!("{} Average", self.task_names[i]));
            df.write(
                self.task_last_max_quality[i],
                &format!("{} Max", self.task_names[i]),
            );
        }
        df.endl();
        Ok(())
    }

    /// C++: `PrintResourceData`. The per-resource spatial maps the C++
    /// prints when `maps == "1"` are not ported; that case is an error.
    pub fn print_resource_data(
        &self,
        out: &mut OutputManager,
        filename: &str,
        maps: &str,
        world_x: i32,
    ) -> Result<(), String> {
        {
            let df = out.static_with_path(filename)?;
            df.write_comment("Avida resource data");
            df.write_time_stamp();
            df.write_comment(
                "First column gives the current update, all further columns give the quantity",
            );
            df.write_comment("of the particular resource at that update.");

            df.write(self.m_update, "Update");
        }

        // Check for spatial resources if they exist total up the resource in
        // each cell and print that total.  Also call the routine to print the
        // individual maps for each spatial resource
        for i in 0..self.resource_count.len() {
            if self.resource_geometry[i] != geometry::GLOBAL
                && self.resource_geometry[i] != geometry::PARTIAL
            {
                let mut sum_spa_resource = 0.0;
                for &v in &self.spatial_res_count[i] {
                    sum_spa_resource += v;
                }
                out.static_with_path(filename)?
                    .write(sum_spa_resource, &self.resource_names[i]);
                if maps == "1" {
                    self.print_spatial_res_data(out, filename, i, world_x)?;
                }
            } else {
                out.static_with_path(filename)?
                    .write(self.resource_count[i], &self.resource_names[i]);
            }
        }
        out.static_with_path(filename)?.endl();
        Ok(())
    }

    /// C++: `PrintTimeData`
    pub fn print_time_data(&self, out: &mut OutputManager, filename: &str) -> Result<(), String> {
        let df = out.static_with_path(filename)?;
        df.write_comment("Avida time data");
        df.write_time_stamp();

        df.write(self.m_update, "update");
        df.write(self.avida_time, "avida time");
        df.write(self.sum_generation.average(), "average generation");
        df.write(self.num_executed, "num_executed?");
        df.endl();
        Ok(())
    }

    /// C++: `PrintExtendedTimeData`
    pub fn print_extended_time_data(
        &self,
        out: &mut OutputManager,
        filename: &str,
    ) -> Result<(), String> {
        let df = out.static_with_path(filename)?;
        df.write_time_stamp();
        df.write(self.m_update, "update");
        df.write(self.avida_time, "avida time");
        df.write(self.num_executed, "num_executed");
        df.write(self.tot_organisms, "num_organisms");
        df.endl();
        Ok(())
    }
}

impl PhenotypeStats for Stats {
    fn add_energy_testament_accepted(&mut self, value: f64) {
        self.energy_testament_accepted_by_organisms.add(value);
    }

    fn age_task_event(&mut self, _org_id: i32, task_id: usize, org_age: i32) {
        self.reaction_age_map
            .entry(task_id as i32)
            .or_default()
            .add(f64::from(org_age));
    }

    fn add_new_task_count(&mut self, task_num: usize) {
        self.new_task_count[task_num] += 1;
    }

    fn add_other_task_counts(&mut self, task_num: usize, prev_tasks: i32, cur_tasks: i32) {
        self.prev_task_count[task_num] += prev_tasks;
        self.cur_task_count[task_num] += cur_tasks;
    }

    fn add_new_reaction_count(&mut self, reaction_num: usize) {
        self.new_reaction_count[reaction_num] += 1;
    }

    fn add_task_switch_time(&mut self, t1: i32, t2: i32, time: i32) {
        self.intrinsic_task_switch_time
            .entry((t1, t2))
            .or_default()
            .add(f64::from(time));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn data_lines(path: &std::path::Path) -> Vec<String> {
        std::fs::read_to_string(path)
            .unwrap()
            .lines()
            .filter(|l| !l.starts_with('#') && !l.trim().is_empty())
            .map(str::to_string)
            .collect()
    }

    #[test]
    fn default_run_rows_match_the_reference_format() {
        let dir = std::env::temp_dir().join(format!("avida-world-stats-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let mut out = OutputManager::new(dir.to_str().unwrap());

        let names: Vec<String> = ["Not", "Nand", "And"]
            .iter()
            .map(|s| s.to_string())
            .collect();
        let mut stats = Stats::new(names.clone(), names, 0, 0.0);
        // Update 0 of heads_default_100u: one injected organism.
        stats.inc_current_update();
        stats.num_creatures = 1;
        stats.num_executed = 30;
        stats.m_num_genotypes = 1;
        stats.m_threshold_genotypes = 1;
        stats.num_breed_true_creatures = 1;
        stats.num_no_birth_creatures = 1;
        stats.num_single_thread_creatures = 1;
        stats.sum_merit.add(100.0);
        stats.sum_gestation.add(0.0);
        stats.sum_fitness.add(0.0);
        stats.sum_copy_size.add(100.0);
        stats.sum_exe_size.add(100.0);
        stats.sum_generation.add(0.0);

        stats.print_average_data(&mut out, "average.dat").unwrap();
        stats.print_count_data(&mut out, "count.dat").unwrap();
        stats.print_tasks_data(&mut out, "tasks.dat").unwrap();
        stats.print_time_data(&mut out, "time.dat").unwrap();
        stats
            .print_resource_data(&mut out, "resource.dat", "", 1)
            .unwrap();
        out.flush_all();

        assert_eq!(
            data_lines(&dir.join("average.dat")),
            ["0 100 0 0 0 0 100 100 0 0 0 0 0 0 0 0 "]
        );
        assert_eq!(
            data_lines(&dir.join("count.dat")),
            ["0 30 1 1 1 0 0 0 0 0 0 1 1 1 0 0 "]
        );
        assert_eq!(data_lines(&dir.join("tasks.dat")), ["0 0 0 0 "]);
        assert_eq!(data_lines(&dir.join("time.dat")), ["0 0 0 30 "]);
        assert_eq!(data_lines(&dir.join("resource.dat")), ["0 "]);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn process_update_advances_avida_time() {
        let mut stats = Stats::new(Vec::new(), Vec::new(), 0, 0.0);
        stats.m_update = 1;
        stats.sum_merit.add(50.0);
        stats.num_births = 3;
        stats.num_creatures = 2;
        stats.num_executed = 60;
        stats.process_update(30);
        assert_eq!(stats.avida_time, 1.0 / 50.0);
        assert_eq!((stats.num_births, stats.num_executed), (0, 0));
        assert_eq!(stats.tot_executed, 60);
        assert_eq!(stats.last_update, 1);
    }
}
