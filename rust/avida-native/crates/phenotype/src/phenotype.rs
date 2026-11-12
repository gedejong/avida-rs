//! Port of `cPhenotype` (avida-core/source/main/cPhenotype.{h,cc}, following
//! the pre-FFI bodies at commit ef5c6d948).
//!
//! Fields are public and keep their C++ names; the C++ accessors that only
//! read or write one field are not reproduced. World services the C++ reaches
//! through `m_world` are passed in: the config, the current update, the RNG
//! and the two small traits below for the statistics and deme hooks.
//!
//! Differences that cannot matter for defined C++ behaviour:
//! * scalars the C++ constructor leaves uninitialised start at zero/false;
//! * `Clone` copies every field, while C++ `operator=` skips some
//!   (`last_cpu_cycles_used`, the trial arrays, birth ids, `born_parent_group`
//!   and a few more) and self-assigns `first_reaction_execs`.

use std::collections::VecDeque;

use avida_apto::rng::AvidaRng;
use avida_config::string_util::format_ostream_double;
use avida_config::Config;
use avida_environment::{ContextPhenotype, ReactionResult};
use avida_tasks::TaskStates;

use crate::merit::Merit;

pub const DIVIDE_METHOD_OFFSPRING: i32 = 0;
pub const DIVIDE_METHOD_SPLIT: i32 = 1;
pub const DIVIDE_METHOD_BIRTH: i32 = 2;
pub const GENERATION_INC_OFFSPRING: i32 = 0;
pub const GENERATION_INC_BOTH: i32 = 1;
pub const MATING_TYPE_JUVENILE: i32 = -1;
pub const MATING_TYPE_FEMALE: i32 = 0;
pub const MATING_TYPE_MALE: i32 = 1;
pub const MATE_PREFERENCE_RANDOM: i32 = 0;
pub const BASE_MERIT_CONST: i32 = 0;
pub const BASE_MERIT_COPIED_SIZE: i32 = 1;
pub const BASE_MERIT_EXE_SIZE: i32 = 2;
pub const BASE_MERIT_FULL_SIZE: i32 = 3;
pub const BASE_MERIT_LEAST_SIZE: i32 = 4;
pub const BASE_MERIT_SQRT_LEAST_SIZE: i32 = 5;
pub const BASE_MERIT_NUM_BONUS_INST: i32 = 6;
pub const BASE_MERIT_GESTATION_TIME: i32 = 7;
pub const ENERGY_LEVEL_LOW: i32 = 0;
pub const ENERGY_LEVEL_MEDIUM: i32 = 1;
pub const ENERGY_LEVEL_HIGH: i32 = 2;

/// Array sizes the C++ constructor reads from the environment and stats.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct PhenotypeSizes {
    /// `GetEnvironment().GetNumTasks()`
    pub num_tasks: usize,
    /// `GetEnvironment().GetResourceLib().GetSize()`
    pub num_resources: usize,
    /// `GetEnvironment().GetReactionLib().GetSize()`
    pub num_reactions: usize,
    /// `GetStats().GetSenseSize()`
    pub sense_size: usize,
}

/// The `cStats` calls `cPhenotype` makes.
pub trait PhenotypeStats {
    /// `SumEnergyTestamentAcceptedByOrganisms().Add(value)`
    fn add_energy_testament_accepted(&mut self, value: f64);
    /// `AgeTaskEvent(org_id, task_id, org_age)`
    fn age_task_event(&mut self, org_id: i32, task_id: usize, org_age: i32);
    /// `AddNewTaskCount(task_num)`
    fn add_new_task_count(&mut self, task_num: usize);
    /// `AddOtherTaskCounts(task_num, prev_tasks, cur_tasks)`
    fn add_other_task_counts(&mut self, task_num: usize, prev_tasks: i32, cur_tasks: i32);
    /// `AddNewReactionCount(reaction_num)`
    fn add_new_reaction_count(&mut self, reaction_num: usize);
    /// `AddTaskSwitchTime(t1, t2, time)`
    fn add_task_switch_time(&mut self, t1: i32, t2: i32, time: i32);
}

/// The `cDeme` calls `cPhenotype::TestOutput` makes.
pub trait PhenotypeDeme {
    /// `GetHeritableDemeMerit().GetDouble()`
    fn heritable_deme_merit(&self) -> f64;
    /// `UpdateHeritableDemeMerit(value)`
    fn update_heritable_deme_merit(&mut self, value: f64);
    /// `AddCurTask(task_num)`
    fn add_cur_task(&mut self, task_num: usize);
    /// `AddCurReaction(reaction_num)`
    fn add_cur_reaction(&mut self, reaction_num: usize);
}

/// C++: `cPhenotype`.
#[derive(Debug, Clone, Default)]
pub struct Phenotype {
    pub initialized: bool,

    // 1. Values calculated at the last divide (of self or offspring).
    pub merit: Merit,
    pub execution_ratio: f64,
    pub energy_store: f64,
    pub genome_length: i32,
    pub bonus_instruction_count: i32,
    pub copied_size: i32,
    pub executed_size: i32,
    pub gestation_time: i32,
    pub gestation_start: i32,
    pub fitness: f64,
    pub div_type: f64,

    // 2. "In progress" variables, updated as the organism operates.
    pub cur_bonus: f64,
    pub cur_energy_bonus: f64,
    pub energy_tobe_applied: f64,
    pub energy_testament: f64,
    pub energy_received_buffer: f64,
    pub total_energy_donated: f64,
    pub total_energy_received: f64,
    pub total_energy_applied: f64,
    pub num_energy_requests: i32,
    pub num_energy_donations: i32,
    pub num_energy_receptions: i32,
    pub num_energy_applications: i32,
    pub cur_num_errors: i32,
    pub cur_num_donates: i32,
    pub cur_task_count: Vec<i32>,
    pub cur_para_tasks: Vec<i32>,
    pub cur_host_tasks: Vec<i32>,
    pub cur_internal_task_count: Vec<i32>,
    pub eff_task_count: Vec<i32>,
    pub cur_task_quality: Vec<f64>,
    pub cur_task_value: Vec<f64>,
    pub cur_internal_task_quality: Vec<f64>,
    pub cur_rbins_total: Vec<f64>,
    pub cur_rbins_avail: Vec<f64>,
    pub cur_collect_spec_counts: Vec<i32>,
    pub cur_reaction_count: Vec<i32>,
    pub first_reaction_cycles: Vec<i32>,
    pub first_reaction_execs: Vec<i32>,
    pub cur_stolen_reaction_count: Vec<i32>,
    pub cur_reaction_add_reward: Vec<f64>,
    pub cur_inst_count: Vec<i32>,
    pub cur_from_sensor_count: Vec<i32>,
    pub cur_group_attack_count: Vec<Vec<i32>>,
    pub cur_top_pred_group_attack_count: Vec<Vec<i32>>,
    pub cur_killed_targets: Vec<i32>,
    pub cur_attacks: i32,
    pub cur_kills: i32,
    pub cur_sense_count: Vec<i32>,
    pub sensed_resources: Vec<f64>,
    pub cur_task_time: Vec<f64>,
    /// `m_task_states`
    pub task_states: TaskStates,
    pub cur_trial_fitnesses: Vec<f64>,
    pub cur_trial_bonuses: Vec<f64>,
    pub cur_trial_times_used: Vec<i32>,
    pub cur_from_message_count: Vec<i32>,
    pub trial_time_used: i32,
    pub trial_cpu_cycles_used: i32,
    /// `m_tolerance_immigrants` (front = most recent `Push`).
    pub tolerance_immigrants: VecDeque<i32>,
    pub tolerance_offspring_own: VecDeque<i32>,
    pub tolerance_offspring_others: VecDeque<i32>,
    /// `m_intolerances`: (update, intolerance) caches.
    pub intolerances: Vec<(i32, i32)>,
    pub last_child_germline_propensity: f64,
    pub mating_type: i32,
    pub mate_preference: i32,
    pub cur_mating_display_a: i32,
    pub cur_mating_display_b: i32,
    /// `m_reaction_result`: scratch object reused between `TestOutput` calls.
    pub reaction_result: Option<ReactionResult>,

    // 3. Status of the "in progress" variables at the last divide.
    pub last_merit_base: f64,
    pub last_bonus: f64,
    pub last_energy_bonus: f64,
    pub last_num_errors: i32,
    pub last_num_donates: i32,
    pub last_task_count: Vec<i32>,
    pub last_para_tasks: Vec<i32>,
    pub last_host_tasks: Vec<i32>,
    pub last_internal_task_count: Vec<i32>,
    pub last_task_quality: Vec<f64>,
    pub last_task_value: Vec<f64>,
    pub last_internal_task_quality: Vec<f64>,
    pub last_rbins_total: Vec<f64>,
    pub last_rbins_avail: Vec<f64>,
    pub last_collect_spec_counts: Vec<i32>,
    pub last_reaction_count: Vec<i32>,
    pub last_reaction_add_reward: Vec<f64>,
    pub last_inst_count: Vec<i32>,
    pub last_from_sensor_count: Vec<i32>,
    pub last_sense_count: Vec<i32>,
    pub last_group_attack_count: Vec<Vec<i32>>,
    pub last_top_pred_group_attack_count: Vec<Vec<i32>>,
    pub last_killed_targets: Vec<i32>,
    pub last_attacks: i32,
    pub last_kills: i32,
    pub last_from_message_count: Vec<i32>,
    pub last_fitness: f64,
    pub last_cpu_cycles_used: i32,
    pub cur_child_germline_propensity: f64,
    pub last_mating_display_a: i32,
    pub last_mating_display_b: i32,

    // 4. Records from this organism's life.
    pub num_divides_failed: i32,
    pub num_divides: i32,
    pub generation: i32,
    pub cpu_cycles_used: i32,
    pub time_used: i32,
    pub num_execs: i32,
    pub age: i32,
    pub fault_desc: String,
    pub neutral_metric: f64,
    pub life_fitness: f64,
    pub exec_time_born: i32,
    pub gmu_exec_time_born: f64,
    pub birth_update: i32,
    pub birth_cell_id: i32,
    pub av_birth_cell_id: i32,
    pub birth_group_id: i32,
    pub birth_forager_type: i32,
    /// `testCPU_inst_count`
    pub test_cpu_inst_count: Vec<i32>,
    pub last_task_id: i32,
    pub num_new_unique_reactions: i32,
    pub res_consumed: f64,
    pub is_germ_cell: bool,
    pub last_task_time: i32,
    /// Not in the C++: the CPU cycles this organism used over its whole life
    /// (`cpu_cycles_used` restarts at every divide), for debuggers. It
    /// follows `IncCPUCyclesUsed`/`DecCPUCyclesUsed`, plus one per time slice
    /// of hardware that counts no cycles (TransSMT); nothing reads it.
    pub lifetime_cpu_cycles: u64,

    // 5. Status flags (updated at each divide).
    pub to_die: bool,
    pub to_delete: bool,
    pub make_random_resource: bool,
    pub is_injected: bool,
    pub is_clone: bool,
    pub is_donor_cur: bool,
    pub is_donor_last: bool,
    pub is_donor_rand: bool,
    pub is_donor_rand_last: bool,
    pub is_donor_null: bool,
    pub is_donor_null_last: bool,
    pub is_donor_kin: bool,
    pub is_donor_kin_last: bool,
    pub is_donor_edit: bool,
    pub is_donor_edit_last: bool,
    pub is_donor_gbg: bool,
    pub is_donor_gbg_last: bool,
    pub is_donor_truegb: bool,
    pub is_donor_truegb_last: bool,
    pub is_donor_threshgb: bool,
    pub is_donor_threshgb_last: bool,
    pub is_donor_quanta_threshgb: bool,
    pub is_donor_quanta_threshgb_last: bool,
    pub is_donor_shadedgb: bool,
    pub is_donor_shadedgb_last: bool,
    pub is_donor_locus: Vec<bool>,
    pub is_donor_locus_last: Vec<bool>,
    pub is_energy_requestor: bool,
    pub is_energy_donor: bool,
    pub is_energy_receiver: bool,
    pub has_used_donated_energy: bool,
    pub has_open_energy_request: bool,
    pub num_thresh_gb_donations: i32,
    pub num_thresh_gb_donations_last: i32,
    pub num_quanta_thresh_gb_donations: i32,
    pub num_quanta_thresh_gb_donations_last: i32,
    pub num_shaded_gb_donations: i32,
    pub num_shaded_gb_donations_last: i32,
    pub num_donations_locus: i32,
    pub num_donations_locus_last: i32,
    pub is_receiver: bool,
    pub is_receiver_last: bool,
    pub is_receiver_rand: bool,
    pub is_receiver_kin: bool,
    pub is_receiver_kin_last: bool,
    pub is_receiver_edit: bool,
    pub is_receiver_edit_last: bool,
    pub is_receiver_gbg: bool,
    pub is_receiver_truegb: bool,
    pub is_receiver_truegb_last: bool,
    pub is_receiver_threshgb: bool,
    pub is_receiver_threshgb_last: bool,
    pub is_receiver_quanta_threshgb: bool,
    pub is_receiver_quanta_threshgb_last: bool,
    pub is_receiver_shadedgb: bool,
    pub is_receiver_shadedgb_last: bool,
    pub is_receiver_gb_same_locus: bool,
    pub is_receiver_gb_same_locus_last: bool,
    pub is_modifier: bool,
    pub is_modified: bool,
    pub is_fertile: bool,
    pub is_mutated: bool,
    pub is_multi_thread: bool,
    pub parent_true: bool,
    pub parent_sex: bool,
    pub parent_cross_num: i32,
    pub born_parent_group: bool,
    pub kaboom_executed: bool,
    pub kaboom_executed2: bool,

    // 6. Child information.
    pub copy_true: bool,
    pub divide_sex: bool,
    pub mate_select_id: i32,
    pub cross_num: i32,
    pub child_fertile: bool,
    pub last_child_fertile: bool,
    pub child_copied_size: i32,

    // 7. Information that is set once (when the organism was born).
    pub permanent_germline_propensity: f64,
}

impl Phenotype {
    /// C++: `cPhenotype::cPhenotype(cWorld*, int parent_generation, int num_nops)`
    pub fn new(
        cfg: &Config,
        sizes: &PhenotypeSizes,
        parent_generation: i32,
        num_nops: i32,
    ) -> Self {
        let nt = sizes.num_tasks;
        let nres = sizes.num_resources;
        let nreact = sizes.num_reactions;
        let mut p = Phenotype {
            initialized: false,
            energy_store: 0.0,
            cur_task_count: vec![0; nt],
            cur_para_tasks: vec![0; nt],
            cur_host_tasks: vec![0; nt],
            cur_internal_task_count: vec![0; nt],
            eff_task_count: vec![0; nt],
            cur_task_quality: vec![0.0; nt],
            cur_task_value: vec![0.0; nt],
            cur_internal_task_quality: vec![0.0; nt],
            cur_rbins_total: vec![0.0; nres],
            cur_rbins_avail: vec![0.0; nres],
            cur_reaction_count: vec![0; nreact],
            first_reaction_cycles: vec![0; nreact],
            first_reaction_execs: vec![0; nreact],
            cur_stolen_reaction_count: vec![0; nreact],
            cur_reaction_add_reward: vec![0.0; nreact],
            cur_sense_count: vec![0; sizes.sense_size],
            sensed_resources: vec![0.0; nres],
            cur_task_time: vec![0.0; nt],
            intolerances: vec![(0, 0); if cfg.tolerance_variations > 0 { 1 } else { 3 }],
            mating_type: MATING_TYPE_JUVENILE,
            mate_preference: MATE_PREFERENCE_RANDOM,
            last_task_count: vec![0; nt],
            last_para_tasks: vec![0; nt],
            last_host_tasks: vec![0; nt],
            last_internal_task_count: vec![0; nt],
            last_task_quality: vec![0.0; nt],
            last_task_value: vec![0.0; nt],
            last_internal_task_quality: vec![0.0; nt],
            last_rbins_total: vec![0.0; nres],
            last_rbins_avail: vec![0.0; nres],
            last_reaction_count: vec![0; nreact],
            last_reaction_add_reward: vec![0.0; nreact],
            last_sense_count: vec![0; sizes.sense_size],
            birth_forager_type: -1,
            last_task_id: -1,
            is_germ_cell: cfg.demes_orgs_start_in_germ != 0,
            ..Default::default()
        };

        if parent_generation >= 0 {
            p.generation = parent_generation;
            if cfg.generation_inc_method != GENERATION_INC_BOTH {
                p.generation += 1;
            }
        }

        let num_resources = nres as f64;
        if num_resources <= 0.0 || num_nops <= 0 {
            return p;
        }
        let nn = f64::from(num_nops);
        let most_nops_needed = (num_resources.ln() / nn.ln()).ceil();
        let n = ((nn.powf(most_nops_needed + 1.0) - 1.0) / (nn - 1.0)) as i32;
        p.cur_collect_spec_counts = vec![0; n.max(0) as usize];
        p
    }

    /// C++: `cPhenotype::SetInstSetSize`
    pub fn set_inst_set_size(&mut self, inst_set_size: usize) {
        self.cur_inst_count.resize(inst_set_size, 0);
        self.cur_from_sensor_count.resize(inst_set_size, 0);
        self.cur_from_message_count.resize(inst_set_size, 0);
        self.last_inst_count.resize(inst_set_size, 0);
        self.last_from_sensor_count.resize(inst_set_size, 0);
        self.last_from_message_count.resize(inst_set_size, 0);
    }

    /// C++: `cPhenotype::SetGroupAttackInstSetSize`
    pub fn set_group_attack_inst_set_size(&mut self, num_group_attack_inst: usize) {
        for v in [
            &mut self.last_group_attack_count,
            &mut self.last_top_pred_group_attack_count,
            &mut self.cur_group_attack_count,
            &mut self.cur_top_pred_group_attack_count,
        ] {
            v.resize(num_group_attack_inst, Vec::new());
            for row in v.iter_mut() {
                row.resize(20, 0);
            }
        }
    }

    fn clear_cur_group_attack(&mut self) {
        for r in 0..self.cur_group_attack_count.len() {
            self.cur_group_attack_count[r].fill(0);
            self.cur_top_pred_group_attack_count[r].fill(0);
        }
    }

    fn clear_tolerance(&mut self) {
        self.tolerance_immigrants.clear();
        self.tolerance_offspring_own.clear();
        self.tolerance_offspring_others.clear();
        self.intolerances.fill((-1, -1));
    }

    fn clear_trials(&mut self) {
        self.cur_trial_fitnesses.clear();
        self.cur_trial_bonuses.clear();
        self.cur_trial_times_used.clear();
        self.trial_time_used = 0;
        self.trial_cpu_cycles_used = 0;
    }

    /// The `cur_*` counters every setup/reset path zeroes after the task and
    /// resource-bin arrays: collect specs through the attack counters.
    fn clear_cur_reaction_and_inst_counts(&mut self) {
        self.cur_collect_spec_counts.fill(0);
        self.cur_reaction_count.fill(0);
        self.first_reaction_cycles.fill(-1);
        self.first_reaction_execs.fill(-1);
        self.cur_stolen_reaction_count.fill(0);
        self.cur_reaction_add_reward.fill(0.0);
        self.cur_inst_count.fill(0);
        self.cur_from_sensor_count.fill(0);
        self.cur_from_message_count.fill(0);
        self.clear_cur_group_attack();
        self.cur_killed_targets.fill(0);
        self.cur_attacks = 0;
        self.cur_kills = 0;
    }

    /// The `last_* = cur_*` block shared by `DivideReset`, `TestDivideReset`
    /// and `NewTrial`.
    fn copy_cur_to_last(&mut self, cur_merit_base: f64) {
        self.last_merit_base = cur_merit_base;
        self.last_bonus = self.cur_bonus;
        self.last_cpu_cycles_used = self.cpu_cycles_used;
        self.last_num_errors = self.cur_num_errors;
        self.last_num_donates = self.cur_num_donates;
        self.last_task_count.clone_from(&self.cur_task_count);
        self.last_host_tasks.clone_from(&self.cur_host_tasks);
        self.last_para_tasks.clone_from(&self.cur_para_tasks);
        self.last_internal_task_count
            .clone_from(&self.cur_internal_task_count);
        self.last_task_quality.clone_from(&self.cur_task_quality);
        self.last_task_value.clone_from(&self.cur_task_value);
        self.last_internal_task_quality
            .clone_from(&self.cur_internal_task_quality);
        self.last_rbins_total.clone_from(&self.cur_rbins_total);
        self.last_rbins_avail.clone_from(&self.cur_rbins_avail);
        // `last_collect_spec_counts` is left alone: the reference copies
        // `cur_collect_spec_counts.GetSize()` ints into
        // `last_collect_spec_counts.GetData()` (`avd_pheno_copy_int_array`),
        // and that array is constructed empty and never resized, so its data
        // pointer is null and the copy returns without writing. It stays
        // empty for life, and analyze-mode `r_spec.N` prints -1.
        self.last_reaction_count
            .clone_from(&self.cur_reaction_count);
        self.last_reaction_add_reward
            .clone_from(&self.cur_reaction_add_reward);
        self.last_inst_count.clone_from(&self.cur_inst_count);
        self.last_from_sensor_count
            .clone_from(&self.cur_from_sensor_count);
        self.last_from_message_count
            .clone_from(&self.cur_from_message_count);
        self.last_group_attack_count
            .clone_from(&self.cur_group_attack_count);
        self.last_killed_targets
            .clone_from(&self.cur_killed_targets);
        self.last_attacks = self.cur_attacks;
        self.last_kills = self.cur_kills;
        self.last_top_pred_group_attack_count
            .clone_from(&self.cur_top_pred_group_attack_count);
        self.last_sense_count.clone_from(&self.cur_sense_count);
    }

    /// The donation counters and donor/receiver flags rolled from "current"
    /// to "last" by `DivideReset`, `TestDivideReset` and `NewTrial`.
    fn roll_donation_state(&mut self) {
        self.num_thresh_gb_donations_last = self.num_thresh_gb_donations;
        self.num_thresh_gb_donations = 0;
        self.num_quanta_thresh_gb_donations_last = self.num_quanta_thresh_gb_donations;
        self.num_quanta_thresh_gb_donations = 0;
        self.num_shaded_gb_donations_last = self.num_shaded_gb_donations;
        self.num_shaded_gb_donations = 0;
        self.num_donations_locus_last = self.num_donations_locus;
        self.num_donations_locus = 0;
    }

    fn roll_donor_receiver_flags(&mut self) {
        self.is_donor_last = self.is_donor_cur;
        self.is_donor_cur = false;
        self.is_donor_rand_last = self.is_donor_rand;
        self.is_donor_rand = false;
        self.is_donor_null_last = self.is_donor_null;
        self.is_donor_null = false;
        self.is_donor_kin_last = self.is_donor_kin;
        self.is_donor_kin = false;
        self.is_donor_edit_last = self.is_donor_edit;
        self.is_donor_edit = false;
        self.is_donor_gbg_last = self.is_donor_gbg;
        self.is_donor_gbg = false;
        self.is_donor_truegb_last = self.is_donor_truegb;
        self.is_donor_truegb = false;
        self.is_donor_threshgb_last = self.is_donor_threshgb;
        self.is_donor_threshgb = false;
        self.is_donor_quanta_threshgb_last = self.is_donor_quanta_threshgb;
        self.is_donor_quanta_threshgb = false;
        self.is_donor_shadedgb_last = self.is_donor_shadedgb;
        self.is_donor_shadedgb = false;
        self.is_donor_locus_last.clone_from(&self.is_donor_locus);
        self.is_donor_locus.fill(false);

        self.is_receiver_last = self.is_receiver;
        self.is_receiver = false;
        self.is_receiver_rand = false;
        self.is_receiver_kin_last = self.is_receiver_kin;
        self.is_receiver_kin = false;
        self.is_receiver_edit_last = self.is_receiver_edit;
        self.is_receiver_edit = false;
        self.is_receiver_gbg = false;
        self.is_receiver_truegb_last = self.is_receiver_truegb;
        self.is_receiver_truegb = false;
        self.is_receiver_threshgb_last = self.is_receiver_threshgb;
        self.is_receiver_threshgb = false;
        self.is_receiver_quanta_threshgb_last = self.is_receiver_quanta_threshgb;
        self.is_receiver_quanta_threshgb = false;
        self.is_receiver_shadedgb_last = self.is_receiver_shadedgb;
        self.is_receiver_shadedgb = false;
        self.is_receiver_gb_same_locus_last = self.is_receiver_gb_same_locus;
        self.is_receiver_gb_same_locus = false;
    }

    /// C++: `cPhenotype::SetupOffspring`. `genome_size` is `_genome.GetSize()`.
    pub fn setup_offspring(
        &mut self,
        cfg: &Config,
        parent: &Phenotype,
        genome_size: i32,
        rng: &mut AvidaRng,
    ) {
        self.merit = parent.merit;
        self.execution_ratio = if cfg.inherit_exe_rate == 0 {
            1.0
        } else {
            parent.execution_ratio
        };
        self.energy_store = self.energy_store.min(cfg.energy_cap);
        self.energy_tobe_applied = 0.0;
        self.energy_testament = 0.0;
        self.energy_received_buffer = 0.0;
        self.genome_length = genome_size;
        self.copied_size = parent.child_copied_size;
        self.executed_size = parent.executed_size;
        self.gestation_time = parent.gestation_time;
        self.gestation_start = 0;
        self.cpu_cycles_used = 0;
        self.fitness = parent.fitness;
        self.div_type = parent.div_type;

        self.cur_bonus = cfg.default_bonus;
        self.cur_energy_bonus = 0.0;
        self.cur_num_errors = 0;
        self.cur_num_donates = 0;
        self.cur_task_count.fill(0);
        self.cur_internal_task_count.fill(0);
        self.eff_task_count.fill(0);
        self.cur_host_tasks.fill(0);
        self.cur_para_tasks.fill(0);
        self.cur_task_quality.fill(0.0);
        self.cur_task_value.fill(0.0);
        self.cur_internal_task_quality.fill(0.0);
        self.cur_rbins_total.fill(0.0);
        self.cur_rbins_avail.fill(0.0);
        if cfg.split_on_divide {
            for i in 0..self.cur_rbins_avail.len() {
                self.cur_rbins_avail[i] = parent.cur_rbins_avail[i];
            }
        }
        if cfg.resource_given_at_birth > 0.0 {
            let resource = cfg.collect_specific_resource as usize;
            self.cur_rbins_avail[resource] += cfg.resource_given_at_birth;
        }
        self.clear_cur_reaction_and_inst_counts();
        self.cur_sense_count.fill(0);
        self.cur_task_time.fill(0.0);
        for j in 0..self.sensed_resources.len() {
            self.sensed_resources[j] = parent.sensed_resources[j];
        }
        self.clear_trials();
        self.clear_tolerance();
        self.cur_child_germline_propensity = cfg.demes_default_germline_propensity;
        self.mating_type = MATING_TYPE_JUVENILE;
        self.mate_preference = MATE_PREFERENCE_RANDOM;
        self.cur_mating_display_a = 0;
        self.cur_mating_display_b = 0;
        self.last_mating_display_a = 0;
        self.last_mating_display_b = 0;

        self.last_merit_base = parent.last_merit_base;
        self.last_bonus = parent.last_bonus;
        self.last_cpu_cycles_used = parent.last_cpu_cycles_used;
        self.last_num_errors = parent.last_num_errors;
        self.last_num_donates = parent.last_num_donates;
        self.last_task_count.clone_from(&parent.last_task_count);
        self.last_host_tasks.clone_from(&parent.last_host_tasks);
        self.last_para_tasks.clone_from(&parent.last_para_tasks);
        self.last_internal_task_count
            .clone_from(&parent.last_internal_task_count);
        self.last_task_quality.clone_from(&parent.last_task_quality);
        self.last_task_value.clone_from(&parent.last_task_value);
        self.last_internal_task_quality
            .clone_from(&parent.last_internal_task_quality);
        self.last_rbins_total.clone_from(&parent.last_rbins_total);
        self.last_rbins_avail.clone_from(&parent.last_rbins_avail);
        self.last_collect_spec_counts
            .clone_from(&parent.last_collect_spec_counts);
        self.last_reaction_count
            .clone_from(&parent.last_reaction_count);
        self.last_reaction_add_reward
            .clone_from(&parent.last_reaction_add_reward);
        self.last_inst_count.clone_from(&parent.last_inst_count);
        self.last_from_sensor_count
            .clone_from(&parent.last_from_sensor_count);
        self.last_group_attack_count
            .clone_from(&parent.last_group_attack_count);
        self.last_top_pred_group_attack_count
            .clone_from(&parent.last_top_pred_group_attack_count);
        self.last_killed_targets
            .clone_from(&parent.last_killed_targets);
        self.last_attacks = parent.last_attacks;
        self.last_kills = parent.last_kills;
        self.last_sense_count.clone_from(&parent.last_sense_count);
        self.last_fitness = calc_fitness(
            cfg,
            self.last_merit_base,
            self.last_bonus,
            self.gestation_time,
            self.last_cpu_cycles_used,
        );
        self.last_child_germline_propensity = parent.last_child_germline_propensity;
        self.last_from_message_count
            .clone_from(&parent.last_from_message_count);

        self.num_divides = 0;
        self.num_divides_failed = 0;
        self.generation = parent.generation;
        if cfg.generation_inc_method != GENERATION_INC_BOTH {
            self.generation += 1;
        }
        self.cpu_cycles_used = 0;
        self.time_used = 0;
        self.num_execs = 0;
        self.age = 0;
        self.fault_desc.clear();
        self.neutral_metric = parent.neutral_metric + rng.get_rand_normal();
        self.life_fitness = self.fitness;
        self.exec_time_born = parent.exec_time_born;
        self.birth_update = parent.birth_update;
        self.num_new_unique_reactions = 0;
        self.last_task_id = -1;
        self.res_consumed = 0.0;
        self.is_germ_cell = parent.is_germ_cell;
        self.last_task_time = 0;

        self.num_thresh_gb_donations = 0;
        self.num_thresh_gb_donations_last = parent.num_thresh_gb_donations_last;
        self.num_quanta_thresh_gb_donations = 0;
        // QUIRK: C++ reads the parent's `num_thresh_gb_donations_last` here.
        self.num_quanta_thresh_gb_donations_last = parent.num_thresh_gb_donations_last;
        self.num_shaded_gb_donations = 0;
        self.num_shaded_gb_donations_last = parent.num_shaded_gb_donations_last;
        self.num_donations_locus = 0;
        self.num_donations_locus_last = parent.num_donations_locus_last;

        self.is_injected = false;
        self.is_clone = false;
        self.is_donor_cur = false;
        self.is_donor_last = parent.is_donor_last;
        self.is_donor_rand = false;
        self.is_donor_rand_last = parent.is_donor_rand_last;
        self.is_donor_null = false;
        self.is_donor_null_last = parent.is_donor_null_last;
        self.is_donor_kin = false;
        self.is_donor_kin_last = parent.is_donor_kin_last;
        self.is_donor_edit = false;
        self.is_donor_edit_last = parent.is_donor_edit_last;
        self.is_donor_gbg = false;
        self.is_donor_gbg_last = parent.is_donor_gbg_last;
        self.is_donor_truegb = false;
        self.is_donor_truegb_last = parent.is_donor_truegb_last;
        self.is_donor_threshgb = false;
        self.is_donor_threshgb_last = parent.is_donor_threshgb_last;
        self.is_donor_quanta_threshgb = false;
        self.is_donor_quanta_threshgb_last = parent.is_donor_quanta_threshgb_last;
        self.is_donor_shadedgb = false;
        self.is_donor_shadedgb_last = parent.is_donor_shadedgb_last;
        self.is_donor_locus.fill(false);
        self.is_donor_locus_last
            .clone_from(&parent.is_donor_locus_last);

        self.is_receiver = false;
        self.is_receiver_last = parent.is_receiver_last;
        self.is_receiver_rand = false;
        self.is_receiver_kin = false;
        self.is_receiver_kin_last = parent.is_receiver_kin_last;
        self.is_receiver_edit = false;
        self.is_receiver_edit_last = parent.is_receiver_edit_last;
        self.is_receiver_gbg = false;
        self.is_receiver_truegb = false;
        self.is_receiver_truegb_last = parent.is_receiver_truegb_last;
        self.is_receiver_threshgb = false;
        self.is_receiver_threshgb_last = parent.is_receiver_threshgb_last;
        self.is_receiver_quanta_threshgb = false;
        self.is_receiver_quanta_threshgb_last = parent.is_receiver_quanta_threshgb_last;
        self.is_receiver_shadedgb = false;
        self.is_receiver_shadedgb_last = parent.is_receiver_shadedgb_last;
        self.is_receiver_gb_same_locus = false;
        // QUIRK: C++ copies the parent's current flag, not its `_last`.
        self.is_receiver_gb_same_locus_last = parent.is_receiver_gb_same_locus;

        self.is_modifier = false;
        self.is_modified = false;
        self.is_fertile = parent.last_child_fertile;
        self.is_mutated = false;
        self.kaboom_executed = false;
        self.kaboom_executed2 = false;
        self.is_multi_thread = if cfg.inherit_multithread != 0 {
            parent.is_multi_thread
        } else {
            false
        };
        self.parent_true = parent.copy_true;
        self.parent_sex = parent.divide_sex;
        self.parent_cross_num = parent.cross_num;
        self.make_random_resource = false;
        self.to_die = false;
        self.to_delete = false;

        self.is_energy_requestor = false;
        self.is_energy_donor = false;
        self.is_energy_receiver = false;
        self.has_used_donated_energy = false;
        self.has_open_energy_request = false;
        self.total_energy_donated = 0.0;
        self.total_energy_received = 0.0;
        self.total_energy_applied = 0.0;

        self.copy_true = false;
        self.divide_sex = false;
        self.mate_select_id = -1;
        self.cross_num = 0;
        self.last_child_fertile = self.is_fertile;
        self.child_fertile = true;
        self.child_copied_size = 0;

        self.permanent_germline_propensity = parent.last_child_germline_propensity;

        self.initialized = true;
    }

    /// C++: `cPhenotype::SetupInject`. `update` is `GetStats().GetUpdate()`.
    pub fn setup_inject(&mut self, cfg: &Config, genome_size: i32, update: i32) {
        self.genome_length = genome_size;
        self.merit = Merit::from(f64::from(genome_size));
        self.copied_size = genome_size;
        self.executed_size = genome_size;
        self.energy_store = cfg.energy_given_on_inject.min(cfg.energy_cap);
        self.energy_tobe_applied = 0.0;
        self.energy_testament = 0.0;
        self.energy_received_buffer = 0.0;
        self.execution_ratio = 1.0;
        self.gestation_time = 0;
        self.gestation_start = 0;
        self.fitness = 0.0;
        self.div_type = 1.0;

        self.cur_bonus = cfg.default_bonus;
        self.cur_energy_bonus = 0.0;
        self.cur_num_errors = 0;
        self.cur_num_donates = 0;
        self.cur_task_count.fill(0);
        self.cur_para_tasks.fill(0);
        self.cur_host_tasks.fill(0);
        self.cur_internal_task_count.fill(0);
        self.eff_task_count.fill(0);
        self.cur_task_quality.fill(0.0);
        self.cur_task_value.fill(0.0);
        self.cur_internal_task_quality.fill(0.0);
        self.cur_rbins_total.fill(0.0);
        if cfg.resource_given_on_inject > 0.0 {
            let resource = cfg.collect_specific_resource as usize;
            self.cur_rbins_avail[resource] = cfg.resource_given_on_inject;
        } else {
            self.cur_rbins_avail.fill(0.0);
        }
        self.clear_cur_reaction_and_inst_counts();
        self.sensed_resources.fill(0.0);
        self.cur_sense_count.fill(0);
        self.cur_task_time.fill(0.0);
        self.clear_trials();
        self.clear_tolerance();
        self.cur_child_germline_propensity = cfg.demes_default_germline_propensity;
        self.mating_type = MATING_TYPE_JUVENILE;
        self.mate_preference = MATE_PREFERENCE_RANDOM;

        self.last_merit_base = f64::from(genome_size);
        self.last_bonus = 1.0;
        self.last_cpu_cycles_used = 0;
        self.last_num_errors = 0;
        self.last_num_donates = 0;
        self.last_task_count.fill(0);
        self.last_host_tasks.fill(0);
        self.last_para_tasks.fill(0);
        self.last_internal_task_count.fill(0);
        self.last_task_quality.fill(0.0);
        self.last_task_value.fill(0.0);
        self.last_internal_task_quality.fill(0.0);
        self.last_rbins_total.fill(0.0);
        self.last_rbins_avail.fill(0.0);
        self.last_collect_spec_counts.fill(0);
        self.last_reaction_count.fill(0);
        self.last_reaction_add_reward.fill(0.0);
        self.last_inst_count.fill(0);
        self.last_from_sensor_count.fill(0);
        self.last_from_message_count.fill(0);
        for r in 0..self.last_group_attack_count.len() {
            self.last_group_attack_count[r].fill(0);
            self.last_top_pred_group_attack_count[r].fill(0);
        }
        self.last_killed_targets.fill(0);
        self.last_attacks = 0;
        self.last_kills = 0;
        self.last_sense_count.fill(0);
        self.last_child_germline_propensity = cfg.demes_default_germline_propensity;

        self.num_divides = 0;
        self.num_divides_failed = 0;
        self.generation = 0;
        self.cpu_cycles_used = 0;
        self.time_used = 0;
        self.num_execs = 0;
        self.age = 0;
        self.fault_desc.clear();
        self.neutral_metric = 0.0;
        self.life_fitness = 0.0;
        self.exec_time_born = 0;
        self.birth_update = update;

        self.num_thresh_gb_donations = 0;
        self.num_thresh_gb_donations_last = 0;
        self.num_quanta_thresh_gb_donations = 0;
        self.num_quanta_thresh_gb_donations_last = 0;
        self.num_shaded_gb_donations = 0;
        self.num_shaded_gb_donations_last = 0;
        self.num_donations_locus = 0;
        self.num_donations_locus_last = 0;

        self.is_injected = true;
        self.is_clone = false;
        self.is_donor_last = false;
        self.is_donor_cur = false;
        self.is_donor_rand = false;
        self.is_donor_rand_last = false;
        self.is_donor_null = false;
        self.is_donor_null_last = false;
        self.is_donor_kin = false;
        self.is_donor_kin_last = false;
        self.is_donor_edit = false;
        self.is_donor_edit_last = false;
        self.is_donor_gbg = false;
        self.is_donor_gbg_last = false;
        self.is_donor_truegb = false;
        self.is_donor_truegb_last = false;
        self.is_donor_threshgb = false;
        self.is_donor_threshgb_last = false;
        self.is_donor_quanta_threshgb = false;
        self.is_donor_quanta_threshgb_last = false;
        self.is_donor_shadedgb = false;
        self.is_donor_shadedgb_last = false;
        self.is_donor_locus.fill(false);
        self.is_donor_locus_last.fill(false);

        self.is_receiver = false;
        self.is_receiver_last = false;
        self.is_receiver_rand = false;
        self.is_receiver_kin = false;
        self.is_receiver_kin_last = false;
        self.is_receiver_edit = false;
        self.is_receiver_edit_last = false;
        self.is_receiver_gbg = false;
        self.is_receiver_truegb = false;
        self.is_receiver_truegb_last = false;
        self.is_receiver_threshgb = false;
        self.is_receiver_threshgb_last = false;
        self.is_receiver_quanta_threshgb = false;
        self.is_receiver_quanta_threshgb_last = false;
        self.is_receiver_shadedgb = false;
        self.is_receiver_shadedgb_last = false;
        self.is_receiver_gb_same_locus = false;
        self.is_receiver_gb_same_locus_last = false;

        self.is_modifier = false;
        self.is_modified = false;
        self.is_fertile = true;
        self.is_mutated = false;
        self.is_multi_thread = false;
        self.parent_true = true;
        self.parent_sex = false;
        self.parent_cross_num = 0;
        self.make_random_resource = false;
        self.to_die = false;
        self.to_delete = false;
        self.kaboom_executed = false;
        self.kaboom_executed2 = false;

        self.is_energy_requestor = false;
        self.is_energy_donor = false;
        self.is_energy_receiver = false;
        self.has_used_donated_energy = false;
        self.has_open_energy_request = false;
        self.total_energy_donated = 0.0;
        self.total_energy_received = 0.0;
        self.total_energy_applied = 0.0;

        self.copy_true = false;
        self.divide_sex = false;
        self.mate_select_id = -1;
        self.cross_num = 0;
        self.child_fertile = true;
        self.last_child_fertile = true;
        self.child_copied_size = 0;

        self.permanent_germline_propensity = cfg.demes_default_germline_propensity;

        self.initialized = true;
    }

    /// The merit recomputation that opens `ResetMerit`, the divide resets and
    /// `NewTrial`. Returns `cur_merit_base`.
    fn recompute_merit(&mut self, cfg: &Config, honour_inherit_merit: bool) -> f64 {
        let cur_merit_base = f64::from(self.calc_size_merit(cfg));
        let merit_default_bonus = f64::from(cfg.merit_default_bonus);
        if merit_default_bonus != 0.0 {
            self.cur_bonus = merit_default_bonus;
        }
        self.merit = Merit::from(cur_merit_base * self.cur_bonus);
        if honour_inherit_merit && cfg.inherit_merit == 0 {
            self.merit = Merit::from(cur_merit_base);
        }
        cur_merit_base
    }

    /// C++: `cPhenotype::ResetMerit`
    pub fn reset_merit(&mut self, cfg: &Config) {
        self.recompute_merit(cfg, true);
    }

    /// C++: `cPhenotype::DivideReset`
    pub fn divide_reset(
        &mut self,
        cfg: &Config,
        genome_size: i32,
        update: i32,
        rng: &mut AvidaRng,
        stats: &mut dyn PhenotypeStats,
    ) {
        let cur_merit_base = self.recompute_merit(cfg, true);

        self.set_energy(cfg, self.energy_store + self.cur_energy_bonus);
        stats.add_energy_testament_accepted(self.energy_testament);
        self.energy_testament = 0.0;
        self.energy_received_buffer = 0.0;

        self.genome_length = genome_size;
        self.gestation_time = self.time_used - self.gestation_start;
        self.gestation_start = self.time_used;
        self.fitness = calc_fitness(
            cfg,
            cur_merit_base,
            self.cur_bonus,
            self.gestation_time,
            self.cpu_cycles_used,
        );

        self.copy_cur_to_last(cur_merit_base);
        self.last_child_germline_propensity = self.cur_child_germline_propensity;
        self.last_mating_display_a = self.cur_mating_display_a;
        self.last_mating_display_b = self.cur_mating_display_b;

        self.cur_bonus = cfg.default_bonus;
        self.cpu_cycles_used = 0;
        self.cur_energy_bonus = 0.0;
        self.cur_num_errors = 0;
        self.cur_num_donates = 0;
        self.cur_task_count.fill(0);
        self.cur_host_tasks.fill(0);
        self.cur_mating_display_a = 0;
        self.cur_mating_display_b = 0;

        if cfg.divide_method == DIVIDE_METHOD_SPLIT {
            self.last_para_tasks.clone_from(&self.cur_para_tasks);
            self.cur_para_tasks.fill(0);
        }
        self.cur_internal_task_count.fill(0);
        self.eff_task_count.fill(0);
        self.cur_task_quality.fill(0.0);
        self.cur_task_value.fill(0.0);
        self.cur_internal_task_quality.fill(0.0);
        if cfg.split_on_divide {
            for v in self.cur_rbins_avail.iter_mut() {
                *v /= 2.0;
            }
        } else if cfg.divide_method != 0 {
            self.cur_rbins_avail.fill(0.0);
            self.cur_rbins_total.fill(0.0);
            if cfg.resource_given_at_birth > 0.0 {
                let resource = cfg.collect_specific_resource as usize;
                self.cur_rbins_avail[resource] += cfg.resource_given_at_birth;
            }
        }
        self.clear_cur_reaction_and_inst_counts();
        self.cur_sense_count.fill(0);
        self.cur_task_time.fill(0.0);
        self.cur_child_germline_propensity = cfg.demes_default_germline_propensity;

        self.num_divides += 1;
        self.num_execs = 0;
        self.age = 0;
        self.fault_desc.clear();
        self.life_fitness = self.fitness;
        self.exec_time_born += self.gestation_time;
        self.birth_update = update;
        self.num_new_unique_reactions = 0;
        self.last_task_id = -1;
        self.res_consumed = 0.0;
        self.last_task_time = 0;

        self.roll_donation_state();
        self.is_clone = false;
        self.roll_donor_receiver_flags();

        self.last_child_fertile = self.child_fertile;
        self.child_fertile = true;

        if cfg.divide_method == DIVIDE_METHOD_SPLIT || cfg.divide_method == DIVIDE_METHOD_BIRTH {
            self.gestation_start = 0;
            self.cpu_cycles_used = 0;
            self.time_used = 0;
            self.neutral_metric += rng.get_rand_normal();
        }
        if cfg.divide_method == DIVIDE_METHOD_SPLIT {
            self.clear_tolerance();
        }
        if cfg.generation_inc_method == GENERATION_INC_BOTH {
            self.generation += 1;
        }

        self.task_states.clear();
    }

    /// C++: `cPhenotype::TestDivideReset`
    pub fn test_divide_reset(&mut self, cfg: &Config, genome_size: i32, update: i32) {
        let cur_merit_base = self.recompute_merit(cfg, true);

        self.genome_length = genome_size;
        self.gestation_time = self.time_used - self.gestation_start;
        self.gestation_start = self.time_used;
        self.fitness = calc_fitness(
            cfg,
            cur_merit_base,
            self.cur_bonus,
            self.gestation_time,
            self.cpu_cycles_used,
        );

        self.copy_cur_to_last(cur_merit_base);
        self.last_child_germline_propensity = self.cur_child_germline_propensity;

        self.cur_bonus = cfg.default_bonus;
        self.cpu_cycles_used = 0;
        self.cur_num_errors = 0;
        self.cur_num_donates = 0;
        self.cur_task_count.fill(0);
        self.cur_host_tasks.fill(0);
        if cfg.divide_method == DIVIDE_METHOD_SPLIT {
            self.last_para_tasks.clone_from(&self.cur_para_tasks);
            self.cur_para_tasks.fill(0);
        }
        self.cur_internal_task_count.fill(0);
        self.eff_task_count.fill(0);
        self.cur_task_quality.fill(0.0);
        self.cur_task_value.fill(0.0);
        self.cur_internal_task_quality.fill(0.0);
        self.cur_rbins_total.fill(0.0);
        if cfg.resource_given_on_inject > 0.0 {
            let resource = cfg.collect_specific_resource as usize;
            self.cur_rbins_avail[resource] = cfg.resource_given_on_inject;
        } else {
            self.cur_rbins_avail.fill(0.0);
        }
        self.clear_cur_reaction_and_inst_counts();
        self.cur_sense_count.fill(0);
        self.cur_task_time.fill(0.0);
        self.sensed_resources.fill(-1.0);
        self.clear_trials();
        self.clear_tolerance();
        self.cur_child_germline_propensity = cfg.demes_default_germline_propensity;

        self.num_divides += 1;
        self.generation += 1;
        self.life_fitness = self.fitness;
        self.exec_time_born += self.gestation_time;
        self.birth_update = update;
        self.num_new_unique_reactions = 0;
        self.last_task_id = -1;
        self.res_consumed = 0.0;
        self.last_task_time = 0;

        self.roll_donation_state();
        self.is_clone = false;
        self.roll_donor_receiver_flags();
    }

    /// C++: `cPhenotype::SetupClone`
    pub fn setup_clone(
        &mut self,
        cfg: &Config,
        clone: &Phenotype,
        update: i32,
        rng: &mut AvidaRng,
    ) {
        self.merit = clone.merit;
        self.energy_store = clone.energy_store;
        self.energy_tobe_applied = 0.0;
        self.energy_testament = 0.0;
        self.energy_received_buffer = 0.0;
        self.execution_ratio = if cfg.inherit_exe_rate == 0 {
            1.0
        } else {
            clone.execution_ratio
        };
        self.genome_length = clone.genome_length;
        self.copied_size = clone.copied_size;
        self.executed_size = clone.executed_size;
        self.gestation_time = clone.gestation_time;
        self.gestation_start = 0;
        self.fitness = clone.fitness;
        self.div_type = clone.div_type;

        self.cur_bonus = cfg.default_bonus;
        self.cpu_cycles_used = 0;
        self.cur_num_errors = 0;
        self.cur_num_donates = 0;
        self.cur_task_count.fill(0);
        self.cur_host_tasks.fill(0);
        self.cur_para_tasks.fill(0);
        self.cur_internal_task_count.fill(0);
        self.eff_task_count.fill(0);
        self.cur_rbins_total.fill(0.0);
        self.cur_rbins_avail.fill(0.0);
        self.clear_cur_reaction_and_inst_counts();
        self.cur_sense_count.fill(0);
        self.cur_task_time.fill(0.0);
        for j in 0..self.sensed_resources.len() {
            self.sensed_resources[j] = clone.sensed_resources[j];
        }
        self.clear_trials();
        self.clear_tolerance();
        self.cur_child_germline_propensity = cfg.demes_default_germline_propensity;
        self.mating_type = MATING_TYPE_JUVENILE;
        self.mate_preference = MATE_PREFERENCE_RANDOM;

        self.last_merit_base = clone.last_merit_base;
        self.last_bonus = clone.last_bonus;
        self.last_cpu_cycles_used = clone.last_cpu_cycles_used;
        self.last_num_errors = clone.last_num_errors;
        self.last_num_donates = clone.last_num_donates;
        self.last_task_count.clone_from(&clone.last_task_count);
        self.last_host_tasks.clone_from(&clone.last_host_tasks);
        self.last_para_tasks.clone_from(&clone.last_para_tasks);
        self.last_internal_task_count
            .clone_from(&clone.last_internal_task_count);
        self.last_rbins_total.clone_from(&clone.last_rbins_total);
        self.last_rbins_avail.clone_from(&clone.last_rbins_avail);
        self.last_collect_spec_counts
            .clone_from(&clone.last_collect_spec_counts);
        self.last_reaction_count
            .clone_from(&clone.last_reaction_count);
        self.last_reaction_add_reward
            .clone_from(&clone.last_reaction_add_reward);
        self.last_inst_count.clone_from(&clone.last_inst_count);
        self.last_from_sensor_count
            .clone_from(&clone.last_from_sensor_count);
        self.last_from_message_count
            .clone_from(&clone.last_from_message_count);
        self.last_group_attack_count
            .clone_from(&clone.last_group_attack_count);
        self.last_top_pred_group_attack_count
            .clone_from(&clone.last_top_pred_group_attack_count);
        self.last_killed_targets
            .clone_from(&clone.last_killed_targets);
        self.last_attacks = clone.last_attacks;
        self.last_kills = clone.last_kills;
        self.last_sense_count.clone_from(&clone.last_sense_count);
        self.last_fitness = calc_fitness(
            cfg,
            self.last_merit_base,
            self.last_bonus,
            self.gestation_time,
            self.last_cpu_cycles_used,
        );
        self.last_child_germline_propensity = clone.last_child_germline_propensity;

        self.num_divides = 0;
        self.num_divides_failed = 0;
        self.generation = clone.generation;
        if cfg.generation_inc_method != GENERATION_INC_BOTH {
            self.generation += 1;
        }
        self.cpu_cycles_used = 0;
        self.time_used = 0;
        self.num_execs = 0;
        self.age = 0;
        self.fault_desc.clear();
        self.neutral_metric = clone.neutral_metric + rng.get_rand_normal();
        self.life_fitness = self.fitness;
        self.exec_time_born = 0;
        self.birth_update = update;
        self.num_new_unique_reactions = clone.num_new_unique_reactions;
        self.last_task_id = clone.last_task_id;
        self.res_consumed = clone.res_consumed;
        self.is_germ_cell = clone.is_germ_cell;
        self.last_task_time = clone.last_task_time;

        self.num_thresh_gb_donations_last = clone.num_thresh_gb_donations_last;
        self.num_thresh_gb_donations = clone.num_thresh_gb_donations;
        self.num_quanta_thresh_gb_donations_last = clone.num_quanta_thresh_gb_donations_last;
        self.num_quanta_thresh_gb_donations = clone.num_quanta_thresh_gb_donations;
        self.num_shaded_gb_donations_last = clone.num_shaded_gb_donations_last;
        self.num_shaded_gb_donations = clone.num_shaded_gb_donations;
        self.num_donations_locus = clone.num_donations_locus;
        self.num_donations_locus_last = clone.num_donations_locus_last;

        self.is_injected = false;
        self.is_clone = true;
        self.is_donor_last = clone.is_donor_last;
        self.is_donor_cur = clone.is_donor_cur;
        self.is_donor_rand_last = clone.is_donor_rand_last;
        self.is_donor_rand = clone.is_donor_rand;
        self.is_donor_null_last = clone.is_donor_null_last;
        self.is_donor_null = clone.is_donor_null;
        self.is_donor_kin_last = clone.is_donor_kin_last;
        self.is_donor_kin = clone.is_donor_kin;
        self.is_donor_edit_last = clone.is_donor_edit_last;
        self.is_donor_edit = clone.is_donor_edit;
        self.is_donor_gbg_last = clone.is_donor_gbg_last;
        self.is_donor_gbg = clone.is_donor_gbg;
        self.is_donor_truegb_last = clone.is_donor_truegb_last;
        self.is_donor_truegb = clone.is_donor_truegb;
        self.is_donor_threshgb_last = clone.is_donor_threshgb_last;
        self.is_donor_threshgb = clone.is_donor_threshgb;
        self.is_donor_quanta_threshgb_last = clone.is_donor_quanta_threshgb_last;
        self.is_donor_quanta_threshgb = clone.is_donor_quanta_threshgb;
        self.is_donor_shadedgb_last = clone.is_donor_shadedgb_last;
        self.is_donor_shadedgb = clone.is_donor_shadedgb;
        self.is_donor_locus_last
            .clone_from(&clone.is_donor_locus_last);
        self.is_donor_locus.clone_from(&clone.is_donor_locus);

        self.is_receiver = clone.is_receiver;
        self.is_receiver_last = clone.is_receiver_last;
        self.is_receiver_rand = clone.is_receiver_rand;
        self.is_receiver_kin = clone.is_receiver_kin;
        self.is_receiver_kin_last = clone.is_receiver_kin_last;
        self.is_receiver_edit = clone.is_receiver_edit;
        self.is_receiver_edit_last = clone.is_receiver_edit_last;
        self.is_receiver_gbg = clone.is_receiver_gbg;
        self.is_receiver_truegb = clone.is_receiver_truegb;
        self.is_receiver_truegb_last = clone.is_receiver_truegb_last;
        self.is_receiver_threshgb = clone.is_receiver_threshgb;
        self.is_receiver_threshgb_last = clone.is_receiver_threshgb_last;
        self.is_receiver_quanta_threshgb = clone.is_receiver_quanta_threshgb;
        self.is_receiver_quanta_threshgb_last = clone.is_receiver_quanta_threshgb_last;
        self.is_receiver_shadedgb = clone.is_receiver_shadedgb;
        self.is_receiver_shadedgb_last = clone.is_receiver_shadedgb_last;
        self.is_receiver_gb_same_locus = clone.is_receiver_gb_same_locus;

        self.is_modifier = false;
        self.is_modified = false;
        self.is_fertile = clone.last_child_fertile;
        self.is_mutated = false;
        self.is_multi_thread = clone.is_multi_thread;
        self.parent_true = clone.copy_true;
        self.parent_sex = clone.divide_sex;
        self.parent_cross_num = clone.cross_num;
        self.make_random_resource = false;
        self.to_die = false;
        self.to_delete = false;
        self.is_energy_requestor = false;
        self.is_energy_donor = false;
        self.is_energy_receiver = false;
        self.has_used_donated_energy = false;
        self.has_open_energy_request = false;
        self.kaboom_executed = false;
        self.kaboom_executed2 = false;

        self.copy_true = false;
        self.divide_sex = false;
        self.mate_select_id = 0;
        self.cross_num = 0;
        self.last_child_fertile = self.is_fertile;
        self.child_fertile = true;
        self.child_copied_size = 0;

        self.permanent_germline_propensity = clone.permanent_germline_propensity;

        self.initialized = true;
    }

    /// First half of `cPhenotype::TestOutput`: hand out the cached
    /// `cReactionResult` (creating it on first use). The caller points its
    /// task context at `self.task_states`, runs `Environment::test_output`
    /// with `self.eff_task_count` and `self.cur_reaction_count`, and passes
    /// the outcome to [`Phenotype::apply_test_output`].
    pub fn take_reaction_result(&mut self, sizes: &PhenotypeSizes) -> ReactionResult {
        self.reaction_result.take().unwrap_or_else(|| {
            ReactionResult::new(sizes.num_resources, sizes.num_tasks, sizes.num_reactions)
        })
    }

    /// Second half of `cPhenotype::TestOutput`, from the `found` test on.
    /// `res_change` has `res_in.GetSize()` elements. `org_id` is
    /// `taskctx.GetOrganism()->GetID()`; `deme` is its deme, if any.
    #[allow(clippy::too_many_arguments)]
    pub fn apply_test_output(
        &mut self,
        cfg: &Config,
        sizes: &PhenotypeSizes,
        cur_update_time: i32,
        org_id: i32,
        found: bool,
        mut result: ReactionResult,
        res_change: &mut [f64],
        insts_triggered: &mut Vec<String>,
        is_parasite: bool,
        mut context_phenotype: Option<&mut ContextPhenotype>,
        stats: &mut dyn PhenotypeStats,
        deme: Option<&mut dyn PhenotypeDeme>,
    ) -> bool {
        let num_resources = sizes.num_resources;
        let num_tasks = sizes.num_tasks;
        let num_reactions = sizes.num_reactions;
        let task_refractory_period = cfg.task_refractory_period;

        if !found {
            result.invalidate();
            self.reaction_result = Some(result);
            res_change.fill(0.0);
            return false;
        }

        for i in 0..num_tasks {
            let refract_factor = if task_refractory_period == 0.0 {
                1.0
            } else {
                // `(update - time) - period * 0.5`: the product is exact, so
                // contraction cannot change the result.
                1.0 - (1.0
                    / (1.0
                        + ((f64::from(cur_update_time) - self.cur_task_time[i])
                            - task_refractory_period * 0.5)
                            .exp()))
            };

            if result.task_done(i) {
                self.cur_task_count[i] += 1;
                self.eff_task_count[i] += 1;
                if is_parasite {
                    self.cur_para_tasks[i] += 1;
                } else {
                    self.cur_host_tasks[i] += 1;
                }
                if let Some(cp) = context_phenotype.as_deref_mut() {
                    cp.cur_task_count[i] += 1;
                }
                if !result.used_env_resource() {
                    self.cur_internal_task_count[i] += 1;
                }
                if cfg.age_poly_tracking {
                    stats.age_task_event(org_id, i, self.time_used);
                }
            }

            if result.task_quality(i) > 0.0 {
                // `x += a * b` is contracted by the reference compiler.
                self.cur_task_quality[i] = result
                    .task_quality(i)
                    .mul_add(refract_factor, self.cur_task_quality[i]);
                if !result.used_env_resource() {
                    self.cur_internal_task_quality[i] = result
                        .task_quality(i)
                        .mul_add(refract_factor, self.cur_internal_task_quality[i]);
                }
            }

            self.cur_task_value[i] = result.task_value(i);
            self.cur_task_time[i] = f64::from(cur_update_time);
        }

        for i in 0..num_tasks {
            if result.task_done(i) && self.last_task_count[i] == 0 {
                stats.add_new_task_count(i);
                let mut prev_num_tasks = 0;
                let mut cur_num_tasks = 0;
                for j in 0..num_tasks {
                    if self.last_task_count[j] > 0 {
                        prev_num_tasks += 1;
                    }
                    if self.cur_task_count[j] > 0 {
                        cur_num_tasks += 1;
                    }
                }
                stats.add_other_task_counts(i, prev_num_tasks, cur_num_tasks);
            }
        }

        for i in 0..num_reactions {
            self.cur_reaction_add_reward[i] += result.reaction_add_bonus(i);
            if result.reaction_triggered(i) && self.last_reaction_count[i] == 0 {
                stats.add_new_reaction_count(i);
            }
            if result.reaction_triggered(i) {
                if let Some(cp) = context_phenotype.as_deref_mut() {
                    cp.cur_reaction_count[i] += 1;
                }
                let i_id = i as i32;
                match cfg.task_switch_penalty_type {
                    0 => {}
                    1 => {
                        // "learning" cost
                        let n_react = self.cur_reaction_count[i] - 1;
                        if n_react < cfg.learning_count {
                            self.num_new_unique_reactions += cfg.learning_count - n_react;
                        }
                    }
                    2 => {
                        // "retooling" cost
                        if self.last_task_id == -1 {
                            self.last_task_id = i_id;
                            self.last_task_time = self.time_used;
                        } else {
                            let cur_time_used = self.time_used - self.last_task_time;
                            self.last_task_time = self.time_used;
                            stats.add_task_switch_time(self.last_task_id, i_id, cur_time_used);
                            if self.last_task_id != i_id {
                                self.num_new_unique_reactions += 1;
                                self.last_task_id = i_id;
                            }
                        }
                    }
                    3 => {
                        // centrifuge
                        let distance = (i_id - self.last_task_id).abs();
                        self.num_new_unique_reactions += distance;
                        self.last_task_id = i_id;
                    }
                    // C++ asserts; release builds do nothing.
                    _ => {}
                }
            }
        }

        self.cur_bonus *= result.mult_bonus_total();
        self.cur_bonus += result.add_bonus_total();

        self.cur_child_germline_propensity += result.add_germline_total();
        self.cur_child_germline_propensity *= result.mult_germline_total();

        if let Some(deme) = deme {
            if result.active_deme() {
                let mut deme_bonus = deme.heritable_deme_merit();
                deme_bonus *= result.mult_deme_bonus_total();
                deme_bonus += result.add_deme_bonus_total();
                deme.update_heritable_deme_merit(deme_bonus);
            }
            for i in 0..num_tasks {
                if result.task_done(i) {
                    deme.add_cur_task(i);
                }
            }
            for i in 0..num_reactions {
                if result.reaction_triggered(i) {
                    deme.add_cur_reaction(i);
                }
            }
        }

        self.cur_energy_bonus += result.add_energy_total();

        for (i, change) in res_change.iter_mut().enumerate() {
            *change = result.produced(i) - result.consumed(i);
            self.res_consumed += result.consumed(i);
        }

        if !result.used_env_resource() {
            for i in 0..num_resources {
                let rbin_diff = result.internal_consumed(i) - result.internal_produced(i);
                self.cur_rbins_avail[i] -= rbin_diff;
                if rbin_diff != 0.0 {
                    self.cur_rbins_total[i] += rbin_diff;
                }
            }
        }

        insts_triggered.clear();
        insts_triggered.extend(result.inst_array().iter().cloned());

        for j in 0..res_change.len() {
            if result.detected(j) != -1.0 {
                self.sensed_resources[j] = result.detected(j);
            }
        }

        if result.is_random_resource() {
            self.make_random_resource = true;
        }
        if result.lethal() {
            self.to_die = true;
        }
        if result.sterilize() {
            self.is_fertile = false;
        }

        result.invalidate();
        self.reaction_result = Some(result);
        true
    }

    /// C++: `cPhenotype::Sterilize`
    pub fn sterilize(&mut self) {
        self.is_fertile = false;
    }

    /// C++: `cPhenotype::PrintStatus`
    pub fn print_status(&self, cfg: &Config) -> String {
        let d = format_ostream_double;
        let mut out = format!(
            "  MeritBase:{} Bonus:{} Errors:{} Donates:{}\n",
            self.calc_size_merit(cfg),
            d(self.cur_bonus),
            self.cur_num_errors,
            self.cur_num_donates
        );
        out.push_str("  Task Count (Quality):");
        for i in 0..self.cur_task_count.len() {
            out.push_str(&format!(
                " {} ({})",
                self.cur_task_count[i],
                d(self.cur_task_quality[i])
            ));
        }
        out.push('\n');
        if cfg.use_resource_bins {
            out.push_str("  Used-Internal-Resources Task Count (Quality):");
            for i in 0..self.cur_internal_task_count.len() {
                out.push_str(&format!(
                    " {} ({})",
                    self.cur_internal_task_count[i],
                    d(self.cur_internal_task_quality[i])
                ));
            }
            out.push('\n');
            out.push_str("  Available Internal Resource Bin Contents (Total Ever Collected):");
            for i in 0..self.cur_rbins_avail.len() {
                out.push_str(&format!(
                    " {} ({})",
                    d(self.cur_rbins_avail[i]),
                    d(self.cur_rbins_total[i])
                ));
            }
            out.push('\n');
        }
        out
    }

    /// C++: `cPhenotype::CalcSizeMerit`
    pub fn calc_size_merit(&self, cfg: &Config) -> i32 {
        match cfg.base_merit_method {
            BASE_MERIT_COPIED_SIZE => self.copied_size,
            BASE_MERIT_EXE_SIZE => self.executed_size,
            BASE_MERIT_FULL_SIZE => self.genome_length,
            BASE_MERIT_LEAST_SIZE => self
                .genome_length
                .min(self.copied_size)
                .min(self.executed_size),
            BASE_MERIT_SQRT_LEAST_SIZE => {
                let least = self
                    .genome_length
                    .min(self.copied_size)
                    .min(self.executed_size);
                f64::from(least).sqrt() as i32
            }
            BASE_MERIT_NUM_BONUS_INST => {
                if cfg.fitness_valley != 0
                    && self.bonus_instruction_count >= cfg.fitness_valley_start
                    && self.bonus_instruction_count <= cfg.fitness_valley_stop
                {
                    return 1;
                }
                if cfg.merit_bonus_effect > 0 {
                    1 + self.bonus_instruction_count
                } else if cfg.merit_bonus_effect < 0 {
                    self.genome_length - (self.bonus_instruction_count - 1)
                } else {
                    1
                }
            }
            BASE_MERIT_GESTATION_TIME => self.cpu_cycles_used,
            _ => cfg.base_const_merit,
        }
    }

    /// C++: `cPhenotype::GetCurMeritBase`
    pub fn cur_merit_base(&self, cfg: &Config) -> f64 {
        f64::from(self.calc_size_merit(cfg))
    }

    /// C++: `cPhenotype::CalcCurrentMerit`
    pub fn calc_current_merit(&self, cfg: &Config) -> f64 {
        f64::from(self.calc_size_merit(cfg)) * self.cur_bonus
    }

    /// C++: `cPhenotype::CalcFitnessRatio`
    pub fn calc_fitness_ratio(&self, cfg: &Config) -> f64 {
        let merit_base = f64::from(self.calc_size_merit(cfg));
        let cur_fitness = merit_base * self.cur_bonus / f64::from(self.time_used);
        cur_fitness / self.last_fitness
    }

    /// C++: `cPhenotype::CalcID`
    pub fn calc_id(&self) -> i32 {
        let mut phen_id: i32 = 0;
        for (i, &count) in self.last_task_count.iter().enumerate() {
            if count > 0 {
                phen_id = phen_id.wrapping_add(1_i32.wrapping_shl(i as u32));
            }
        }
        phen_id
    }

    /// C++: `cPhenotype::GetLastMerit`
    pub fn last_merit(&self) -> f64 {
        self.last_merit_base * self.last_bonus
    }

    fn calc_tolerance(
        list: &mut VecDeque<i32>,
        cache: &mut (i32, i32),
        cfg: &Config,
        cur_update: i32,
    ) -> i32 {
        let tolerance_max = cfg.max_tolerance;
        if cache.0 == cur_update {
            return tolerance_max - cache.1;
        }
        let update_window = cfg.tolerance_window;
        while list
            .back()
            .is_some_and(|&last| last < cur_update - update_window)
        {
            list.pop_back();
        }
        while list.len() as i32 > tolerance_max {
            list.pop_back();
        }
        let tolerance = tolerance_max - list.len() as i32;
        cache.0 = cur_update;
        cache.1 = list.len() as i32;
        tolerance
    }

    /// C++: `cPhenotype::CalcToleranceImmigrants`
    pub fn calc_tolerance_immigrants(&mut self, cfg: &Config, cur_update: i32) -> i32 {
        Self::calc_tolerance(
            &mut self.tolerance_immigrants,
            &mut self.intolerances[0],
            cfg,
            cur_update,
        )
    }

    /// C++: `cPhenotype::CalcToleranceOffspringOwn`
    pub fn calc_tolerance_offspring_own(&mut self, cfg: &Config, cur_update: i32) -> i32 {
        if cfg.tolerance_variations > 0 {
            return cfg.max_tolerance;
        }
        Self::calc_tolerance(
            &mut self.tolerance_offspring_own,
            &mut self.intolerances[1],
            cfg,
            cur_update,
        )
    }

    /// C++: `cPhenotype::CalcToleranceOffspringOthers`
    pub fn calc_tolerance_offspring_others(&mut self, cfg: &Config, cur_update: i32) -> i32 {
        if cfg.tolerance_variations > 0 {
            return cfg.max_tolerance;
        }
        Self::calc_tolerance(
            &mut self.tolerance_offspring_others,
            &mut self.intolerances[2],
            cfg,
            cur_update,
        )
    }

    /// C++: `cPhenotype::IncAttackedPreyFTData`. `target_list` is
    /// `GetEnvironment().GetAttackPreyFTList()`.
    pub fn inc_attacked_prey_ft_data(&mut self, target_list: &[i32], target_ft: i32) {
        if self.cur_killed_targets.is_empty() {
            self.cur_killed_targets = vec![0; target_list.len()];
        }
        let target_ft = target_ft.max(-3);
        let mut this_index = target_ft;
        for (i, &t) in target_list.iter().enumerate() {
            if t == target_ft {
                this_index = i as i32;
                break;
            }
        }
        self.cur_killed_targets[this_index as usize] += 1;
    }

    /// C++: `cPhenotype::ReduceEnergy`
    pub fn reduce_energy(&mut self, cfg: &Config, cost: f64) {
        self.set_energy(cfg, self.energy_store - cost);
    }

    /// C++: `cPhenotype::SetEnergy`
    pub fn set_energy(&mut self, cfg: &Config, value: f64) {
        self.energy_store = cmax(0.0, cmin(value, cfg.energy_cap));
    }

    /// C++: `cPhenotype::DoubleEnergyUsage`
    pub fn double_energy_usage(&mut self) {
        self.execution_ratio *= 2.0;
    }

    /// C++: `cPhenotype::HalveEnergyUsage`
    pub fn halve_energy_usage(&mut self) {
        self.execution_ratio *= 0.5;
    }

    /// C++: `cPhenotype::DefaultEnergyUsage`
    pub fn default_energy_usage(&mut self) {
        self.execution_ratio = 1.0;
    }

    /// C++: `cPhenotype::DivideFailed`
    pub fn divide_failed(&mut self) {
        self.num_divides_failed += 1;
    }

    /// C++: `cPhenotype::RefreshEnergy`. An unknown `APPLY_ENERGY_METHOD`
    /// makes C++ print to `cerr` and `exit(-1)`; that is the `Err`.
    pub fn refresh_energy(
        &mut self,
        cfg: &Config,
        stats: &mut dyn PhenotypeStats,
    ) -> Result<(), String> {
        if self.cur_energy_bonus > 0.0 {
            match cfg.apply_energy_method {
                0 | 2 => self.energy_tobe_applied += self.cur_energy_bonus,
                1 => {
                    self.set_energy(cfg, self.energy_store + self.cur_energy_bonus);
                    stats.add_energy_testament_accepted(self.energy_testament);
                    self.energy_testament = 0.0;
                }
                other => return Err(format!("Unknown APPLY_ENERGY_METHOD value {other}")),
            }
            self.cur_energy_bonus = 0.0;
        }
        Ok(())
    }

    /// C++: `cPhenotype::ApplyToEnergyStore`
    pub fn apply_to_energy_store(&mut self, cfg: &Config, stats: &mut dyn PhenotypeStats) {
        self.set_energy(cfg, self.energy_store + self.energy_tobe_applied);
        stats.add_energy_testament_accepted(self.energy_testament);
        self.energy_testament = 0.0;
        self.energy_tobe_applied = 0.0;
    }

    /// C++: `cPhenotype::EnergyTestament`
    pub fn energy_testament(&mut self, value: f64) {
        self.energy_tobe_applied += value;
        self.energy_testament += value;
    }

    /// C++: `cPhenotype::ApplyDonatedEnergy`
    pub fn apply_donated_energy(&mut self, cfg: &Config) {
        let energy_cap = cfg.energy_cap;
        if self.energy_store + self.energy_received_buffer >= energy_cap {
            self.total_energy_applied += energy_cap - self.energy_store;
            self.set_energy(
                cfg,
                self.energy_store + (energy_cap - self.energy_received_buffer),
            );
        } else {
            self.total_energy_applied += self.energy_received_buffer;
            self.set_energy(cfg, self.energy_store + self.energy_received_buffer);
        }
        self.num_energy_applications += 1;
        self.has_used_donated_energy = true;
        self.energy_received_buffer = 0.0;
    }

    /// C++: `cPhenotype::ReceiveDonatedEnergy`
    pub fn receive_donated_energy(&mut self, donation: f64) {
        self.energy_received_buffer += donation;
        self.total_energy_received += donation;
        self.is_energy_receiver = true;
        self.num_energy_receptions += 1;
    }

    /// C++: `cPhenotype::ExtractParentEnergy`
    pub fn extract_parent_energy(
        &mut self,
        cfg: &Config,
        stats: &mut dyn PhenotypeStats,
    ) -> Result<f64, String> {
        let energy_given_at_birth = cfg.energy_given_at_birth;
        let frac_parent_energy_given_at_birth = cfg.frac_parent_energy_given_to_org_at_birth;
        let frac_energy_decay_at_birth = cfg.frac_energy_decay_at_org_birth;
        let energy_cap = cfg.energy_cap;

        if cfg.apply_energy_method == 0 {
            self.refresh_energy(cfg, stats)?;
            self.apply_to_energy_store(cfg, stats);
        }

        self.reduce_energy(cfg, self.energy_store * frac_energy_decay_at_birth);

        // `a * b + c` is contracted by the reference compiler.
        let child_energy = cmax(
            0.0,
            cmin(
                self.energy_store
                    .mul_add(frac_parent_energy_given_at_birth, energy_given_at_birth),
                energy_cap,
            ),
        );
        // 2*energy_given_at_birth: 1 in child_energy and 1 for the parent
        // (the doubling is exact, so contraction is immaterial).
        self.reduce_energy(cfg, child_energy - 2.0 * energy_given_at_birth);

        let parent_merit = Merit::from(
            self.convert_energy_to_merit(cfg, self.energy_store * self.execution_ratio),
        );
        if parent_merit.get_double() > 0.0 {
            self.merit = parent_merit;
        } else {
            self.to_die = true;
        }
        Ok(child_energy)
    }

    /// C++: `cPhenotype::NewTrial`
    pub fn new_trial(&mut self, cfg: &Config) {
        if self.trial_cpu_cycles_used == 0 {
            return;
        }

        self.fitness = calc_fitness(
            cfg,
            self.cur_merit_base(cfg),
            self.cur_bonus,
            self.trial_time_used,
            self.trial_cpu_cycles_used,
        );
        self.cur_trial_fitnesses.push(self.fitness);
        self.cur_trial_bonuses.push(self.cur_bonus);
        self.cur_trial_times_used.push(self.trial_time_used);

        let cur_merit_base = self.recompute_merit(cfg, false);

        // C++ adds the bonus and then overwrites the store.
        self.energy_store = cfg.energy_given_at_birth;
        self.cur_energy_bonus = 0.0;

        self.gestation_time = self.time_used - self.gestation_start;
        self.gestation_start = self.time_used;

        self.copy_cur_to_last(cur_merit_base);

        self.cur_bonus = cfg.default_bonus;
        self.cpu_cycles_used = 0;
        self.cur_energy_bonus = 0.0;
        self.cur_num_errors = 0;
        self.cur_num_donates = 0;
        self.cur_task_count.fill(0);
        self.cur_host_tasks.fill(0);
        self.cur_para_tasks.fill(0);
        self.cur_internal_task_count.fill(0);
        self.eff_task_count.fill(0);
        self.cur_task_quality.fill(0.0);
        self.cur_internal_task_quality.fill(0.0);
        self.cur_task_value.fill(0.0);
        self.cur_rbins_total.fill(0.0);
        self.cur_rbins_avail.fill(0.0);
        self.clear_cur_reaction_and_inst_counts();
        self.cur_sense_count.fill(0);

        self.trial_time_used = 0;
        self.trial_cpu_cycles_used = 0;
        self.clear_tolerance();

        self.num_divides += 1;
        self.num_execs = 0;
        self.age = 0;
        self.fault_desc.clear();
        self.life_fitness = self.fitness;

        self.roll_donation_state();
        self.roll_donor_receiver_flags();

        self.is_energy_requestor = false;
        self.is_energy_donor = false;
        self.is_energy_receiver = false;
    }

    /// C++: `cPhenotype::TrialDivideReset`
    pub fn trial_divide_reset(
        &mut self,
        cfg: &Config,
        genome_size: i32,
        rng: &mut AvidaRng,
        stats: &mut dyn PhenotypeStats,
    ) {
        self.recompute_merit(cfg, false);

        self.set_energy(cfg, self.energy_store + self.cur_energy_bonus);
        stats.add_energy_testament_accepted(self.energy_testament);
        self.energy_testament = 0.0;

        self.genome_length = genome_size;
        self.gestation_start = self.time_used;

        self.cur_trial_fitnesses.clear();
        self.cur_trial_bonuses.clear();
        self.cur_trial_times_used.clear();

        self.last_child_fertile = self.child_fertile;
        self.child_fertile = true;

        if cfg.divide_method == DIVIDE_METHOD_SPLIT || cfg.divide_method == DIVIDE_METHOD_BIRTH {
            self.gestation_start = 0;
            self.cpu_cycles_used = 0;
            self.time_used = 0;
            self.num_execs = 0;
            self.neutral_metric += rng.get_rand_normal();
        }
        if cfg.divide_method == DIVIDE_METHOD_SPLIT {
            self.clear_tolerance();
        }
        if cfg.generation_inc_method == GENERATION_INC_BOTH {
            self.generation += 1;
        }
    }

    /// C++: `cPhenotype::Compare`
    pub fn compare(lhs: &Phenotype, rhs: &Phenotype) -> i32 {
        if lhs.merit < rhs.merit {
            return -1;
        } else if lhs.merit > rhs.merit {
            return 1;
        }
        if lhs.gestation_time < rhs.gestation_time {
            return -1;
        } else if lhs.gestation_time > rhs.gestation_time {
            return 1;
        }
        for k in 0..lhs.last_task_count.len() {
            if lhs.last_task_count[k] < rhs.last_task_count[k] {
                return -1;
            } else if lhs.last_task_count[k] > rhs.last_task_count[k] {
                return 1;
            }
        }
        0
    }

    /// C++: `cPhenotype::GetDiscreteEnergyLevel`
    pub fn discrete_energy_level(&self, cfg: &Config) -> i32 {
        let max_energy = cfg.energy_cap;
        let high_pct = cfg.energy_thresh_high;
        let low_pct = cfg.energy_thresh_low;
        if self.energy_store < low_pct * max_energy {
            ENERGY_LEVEL_LOW
        } else if self.energy_store >= low_pct * max_energy
            && self.energy_store <= high_pct * max_energy
        {
            ENERGY_LEVEL_MEDIUM
        } else if self.energy_store > high_pct * max_energy {
            ENERGY_LEVEL_HIGH
        } else {
            -1
        }
    }

    /// C++: `cPhenotype::ConvertEnergyToMerit`
    pub fn convert_energy_to_merit(&self, cfg: &Config, energy: f64) -> f64 {
        let fix_metabolic_rate = cfg.fix_metabolic_rate;
        if fix_metabolic_rate > 0.0 {
            return 100.0 * fix_metabolic_rate;
        }
        100.0 * energy / f64::from(cfg.num_cycles_exc_before_0_energy)
    }

    /// C++: `cPhenotype::GetResourcesConsumed` (reads and resets).
    pub fn take_resources_consumed(&mut self) -> f64 {
        std::mem::replace(&mut self.res_consumed, 0.0)
    }

    /// C++: `cPhenotype::SetLastParasiteTaskCount`
    pub fn set_last_parasite_task_count(&mut self, old_para_phenotype: &[i32]) {
        for (i, &v) in old_para_phenotype.iter().enumerate() {
            self.last_para_tasks[i] = v;
        }
    }

    /// C++: `cPhenotype::GetCumulativeReactionCount`
    pub fn cumulative_reaction_count(&self, cfg: &Config) -> Vec<i32> {
        if cfg.divide_method == 0 {
            (0..self.cur_reaction_count.len())
                .map(|i| self.cur_reaction_count[i] + self.last_reaction_count[i])
                .collect()
        } else {
            self.cur_reaction_count.clone()
        }
    }

    /// C++: `cPhenotype::UpdateParasiteTasks`
    pub fn update_parasite_tasks(&mut self) {
        self.last_para_tasks.clone_from(&self.cur_para_tasks);
        self.cur_para_tasks.fill(0);
    }

    /// C++: `SetFirstReactionCycle`
    pub fn set_first_reaction_cycle(&mut self, idx: usize) {
        if self.first_reaction_cycles[idx] < 0 {
            self.first_reaction_cycles[idx] = self.time_used;
        }
    }

    /// C++: `SetFirstReactionExec`
    pub fn set_first_reaction_exec(&mut self, idx: usize) {
        if self.first_reaction_execs[idx] < 0 {
            self.first_reaction_execs[idx] = self.num_execs;
        }
    }

    /// C++: `IncCPUCyclesUsed`
    pub fn inc_cpu_cycles_used(&mut self) {
        self.cpu_cycles_used += 1;
        self.trial_cpu_cycles_used += 1;
        self.lifetime_cpu_cycles += 1;
    }

    /// C++: `DecCPUCyclesUsed`
    pub fn dec_cpu_cycles_used(&mut self) {
        self.cpu_cycles_used -= 1;
        self.trial_cpu_cycles_used -= 1;
        self.lifetime_cpu_cycles = self.lifetime_cpu_cycles.saturating_sub(1);
    }

    /// C++: `IncTimeUsed`
    pub fn inc_time_used(&mut self, i: i32) {
        self.time_used += i;
        self.trial_time_used += i;
    }

    /// C++: `IsDonorPosition`
    pub fn is_donor_position(&self, pos: usize) -> bool {
        self.is_donor_locus.get(pos).copied().unwrap_or(false)
    }

    /// C++: `IsDonorPositionLast`
    pub fn is_donor_position_last(&self, pos: usize) -> bool {
        self.is_donor_locus_last.get(pos).copied().unwrap_or(false)
    }

    /// C++: `SetIsDonorPosition`
    pub fn set_is_donor_position(&mut self, pos: usize) {
        self.is_donor_cur = true;
        if self.is_donor_locus.len() <= pos {
            self.is_donor_locus.resize(pos + 1, false);
        }
        self.is_donor_locus[pos] = true;
    }
}

/// `std::min` (returns `a` unless `b < a`), kept for NaN parity.
fn cmin(a: f64, b: f64) -> f64 {
    if b < a {
        b
    } else {
        a
    }
}

/// `std::max` (returns `a` unless `a < b`).
fn cmax(a: f64, b: f64) -> f64 {
    if a < b {
        b
    } else {
        a
    }
}

/// C++: `cPhenotype::CalcFitness`. An unknown `FITNESS_METHOD` prints
/// "Unknown FITNESS_METHOD!" and exits with status 1, as C++ does.
pub fn calc_fitness(
    cfg: &Config,
    merit_base: f64,
    bonus: f64,
    gestation_time: i32,
    _cpu_cycles: i32,
) -> f64 {
    let gest = f64::from(gestation_time);
    match cfg.fitness_method {
        0 => merit_base * bonus / gest,
        1 => {
            // `1 + bonus * c2` is contracted by the reference compiler.
            let converted_bonus = (bonus - cfg.default_bonus) * cfg.fitness_coeff_2
                / bonus.mul_add(cfg.fitness_coeff_2, 1.0);
            merit_base * (converted_bonus * cfg.fitness_coeff_1.ln()).exp() / gest
        }
        2 => {
            let net_bonus = bonus + -cfg.default_bonus;
            net_bonus / (net_bonus + 1.0) * (gest * (1.0 - cfg.fitness_coeff_1).ln()).exp()
        }
        _ => {
            println!("Unknown FITNESS_METHOD!");
            std::process::exit(1);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Default)]
    struct NullStats {
        new_tasks: Vec<usize>,
    }

    impl PhenotypeStats for NullStats {
        fn add_energy_testament_accepted(&mut self, _value: f64) {}
        fn age_task_event(&mut self, _org_id: i32, _task_id: usize, _org_age: i32) {}
        fn add_new_task_count(&mut self, task_num: usize) {
            self.new_tasks.push(task_num);
        }
        fn add_other_task_counts(&mut self, _t: usize, _p: i32, _c: i32) {}
        fn add_new_reaction_count(&mut self, _reaction_num: usize) {}
        fn add_task_switch_time(&mut self, _t1: i32, _t2: i32, _time: i32) {}
    }

    fn sizes() -> PhenotypeSizes {
        PhenotypeSizes {
            num_tasks: 9,
            num_resources: 0,
            num_reactions: 9,
            sense_size: 0,
        }
    }

    fn injected(cfg: &Config, len: i32) -> Phenotype {
        let mut p = Phenotype::new(cfg, &sizes(), -1, 3);
        p.set_inst_set_size(26);
        p.setup_inject(cfg, len, 0);
        p
    }

    #[test]
    fn inject_then_divide_matches_default_run_numbers() {
        // The default ancestor: length 100, copies itself in 389 cycles while
        // executing 97 distinct instructions.
        let cfg = Config::default();
        let mut rng = AvidaRng::new(1);
        let mut stats = NullStats::default();
        let mut p = injected(&cfg, 100);
        assert!(p.is_injected && p.is_fertile && p.parent_true);
        assert_eq!(p.merit.get_double(), 100.0);
        assert_eq!(p.generation, 0);

        p.executed_size = 97;
        p.child_copied_size = 100;
        p.copy_true = true;
        for _ in 0..389 {
            p.inc_cpu_cycles_used();
            p.inc_time_used(1);
        }
        p.divide_reset(&cfg, 100, 5, &mut rng, &mut stats);
        assert_eq!(p.merit.get_double(), 97.0);
        assert_eq!(p.gestation_time, 389);
        assert!((p.fitness - 97.0 / 389.0).abs() < 1e-15);
        // DIVIDE_METHOD defaults to SPLIT, which also resets the parent's clock.
        assert_eq!(cfg.divide_method, DIVIDE_METHOD_SPLIT);
        assert_eq!((p.num_divides, p.cpu_cycles_used, p.time_used), (1, 0, 0));
        assert_eq!(p.gestation_start, 0);
        assert_eq!(p.birth_update, 5);

        let mut child = Phenotype::new(&cfg, &sizes(), 0, 3);
        child.set_inst_set_size(26);
        child.setup_offspring(&cfg, &p, 100, &mut rng);
        assert_eq!(child.generation, 1);
        assert_eq!(child.copied_size, 100);
        assert_eq!(child.merit.get_double(), 97.0);
        assert_eq!(child.last_fitness, p.fitness);
        assert!(child.parent_true && !child.is_injected);
        assert_ne!(child.neutral_metric, 0.0);
    }

    #[test]
    fn applying_a_reaction_result_updates_counts_and_bonus() {
        let cfg = Config::default();
        let mut stats = NullStats::default();
        let s = sizes();
        let mut p = injected(&cfg, 100);
        let mut result = p.take_reaction_result(&s);
        result.mark_task(0, 1.0, 1.0);
        result.mark_reaction(0);
        result.mult_bonus(2.0);
        let mut insts = Vec::new();
        let done = p.apply_test_output(
            &cfg,
            &s,
            3,
            1,
            true,
            result,
            &mut [],
            &mut insts,
            false,
            None,
            &mut stats,
            None,
        );
        assert!(done);
        assert_eq!(p.cur_task_count[0], 1);
        assert_eq!(p.cur_host_tasks[0], 1);
        assert_eq!(p.cur_task_quality[0], 1.0);
        assert_eq!(p.cur_bonus, 2.0);
        assert_eq!(p.cur_task_time[0], 3.0);
        assert_eq!(stats.new_tasks, [0]);
        assert!(p.reaction_result.is_some());

        let result = p.take_reaction_result(&s);
        assert!(!p.apply_test_output(
            &cfg,
            &s,
            3,
            1,
            false,
            result,
            &mut [],
            &mut insts,
            false,
            None,
            &mut stats,
            None,
        ));
    }

    #[test]
    fn size_merit_methods() {
        let mut cfg = Config::default();
        let mut p = injected(&cfg, 100);
        p.copied_size = 80;
        p.executed_size = 64;
        let expect = [(0, 100), (1, 80), (2, 64), (3, 100), (4, 64), (5, 8)];
        for (method, want) in expect {
            cfg.base_merit_method = method;
            assert_eq!(p.calc_size_merit(&cfg), want, "method {method}");
        }
    }

    #[test]
    fn tolerance_window_drops_old_entries() {
        let cfg = Config::default();
        let mut p = injected(&cfg, 100);
        let max = cfg.max_tolerance;
        p.tolerance_immigrants.push_front(1);
        p.tolerance_immigrants.push_front(1000);
        let t = p.calc_tolerance_immigrants(&cfg, 1000);
        let kept = if 1 < 1000 - cfg.tolerance_window {
            1
        } else {
            2
        };
        assert_eq!(t, max - kept);
        // Cached for the same update.
        p.tolerance_immigrants.clear();
        assert_eq!(p.calc_tolerance_immigrants(&cfg, 1000), max - kept);
    }
}
