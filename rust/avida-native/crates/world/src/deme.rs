//! Port of `cDeme` (avida-core/source/main/cDeme.{h,cc} at 47f13dadb) and
//! `cGermline` (cGermline.h): a block of consecutive cells the population
//! manages as a unit, with its own counters, merit, germline and deme
//! resources.
//!
//! What needs the cells' organisms (`KillAll`, `UpdateStats`,
//! `UpdateShannon`, the germline sequestration statistics) lives on
//! [`crate::world::World`] in [`crate::pop_demes`]; this file holds the
//! per-deme state and the methods that only touch it.
//!
//! Cell events, event slots and the deme and movement predicates are in
//! [`crate::deme_events`], which also explains why the deme network
//! (`cDemeNetwork`) only ever aborts the run.
//!
//! Deme input/output (`DoDemeInput`, `DoDemeOutput`) needs no state: no
//! code calls `cPopulationCell::SetCanInput` or `SetCanOutput`, so
//! `GetNextDemeInput` always returns -1, both calls return before touching
//! the buffers, and `m_cur_bonus` never leaves DEFAULT_BONUS, which is
//! what `CompeteDemesByMerit` in [`crate::actions_deme2`] relies on. The
//! message predicates and the pheromone helpers are not ported.

use std::collections::BTreeSet;

use avida_apto::AvidaRng;
use avida_config::Config;
use avida_genome::Genome;
use avida_phenotype::phenotype::PhenotypeDeme;
use avida_phenotype::Merit;
use avida_resources::{ResourceCount, ResourceEnv};

use crate::deme_events::DemeEvents;
use crate::stat_tools::DoubleSum;

/// C++: `cDeme`
#[derive(Debug, Clone)]
pub struct Deme {
    /// `_id`
    pub id: i32,
    /// `cell_ids`
    pub cell_ids: Vec<i32>,
    /// `width`
    pub width: i32,

    /// `replicateDeme`
    pub replicate_deme: bool,
    /// `treatable`
    pub treatable: bool,
    /// `treatment_ages`
    pub treatment_ages: BTreeSet<i32>,

    /// `cur_birth_count`
    pub cur_birth_count: i32,
    /// `last_birth_count`
    pub last_birth_count: i32,
    /// `cur_org_count`
    pub cur_org_count: i32,
    /// `last_org_count`
    pub last_org_count: i32,
    /// `injected_count`
    pub injected_count: i32,
    /// `birth_count_perslot`
    pub birth_count_perslot: i32,
    /// `_age`
    pub age: i32,
    /// `generation`
    pub generation: i32,
    /// `parasite_memory_score`
    pub parasite_memory_score: f64,
    /// `time_used`
    pub time_used: i32,
    /// `gestation_time`
    pub gestation_time: i32,
    /// `cur_normalized_time_used`
    pub cur_normalized_time_used: f64,
    /// `last_normalized_time_used`
    pub last_normalized_time_used: f64,
    /// `MSG_sendFailed`, `MSG_dropped`, `MSG_SuccessfullySent`
    pub msg_send_failed: u32,
    pub msg_dropped: u32,
    pub msg_successfully_sent: u32,
    /// `energyInjectedIntoOrganisms`
    pub energy_injected_into_organisms: f64,
    /// `energyRemainingInDemeAtReplication`
    pub energy_remaining_in_deme_at_replication: f64,
    /// `total_energy_testament`
    pub total_energy_testament: f64,
    /// `eventsTotal` and the event kill counters.
    pub events_total: i32,
    pub events_killed: u32,
    pub events_killed_this_slot: u32,
    pub event_kill_attempts: u32,
    pub event_kill_attempts_this_slot: u32,
    pub consecutive_successful_event_periods: u32,
    /// `sleeping_count`
    pub sleeping_count: i32,

    /// `total_energy_donated`, `total_energy_received`,
    /// `total_energy_applied`
    pub total_energy_donated: f64,
    pub total_energy_received: f64,
    pub total_energy_applied: f64,

    /// `cur_task_exe_count`, `cur_reaction_count` and their `last_`
    /// counterparts.
    pub cur_task_exe_count: Vec<i32>,
    pub cur_reaction_count: Vec<i32>,
    pub last_task_exe_count: Vec<i32>,
    pub last_reaction_count: Vec<i32>,
    /// `cur_org_task_count` ... `last_org_reaction_count`
    pub cur_org_task_count: Vec<i32>,
    pub cur_org_task_exe_count: Vec<i32>,
    pub cur_org_reaction_count: Vec<i32>,
    pub last_org_task_count: Vec<i32>,
    pub last_org_task_exe_count: Vec<i32>,
    pub last_org_reaction_count: Vec<i32>,

    /// `avg_founder_generation`
    pub avg_founder_generation: f64,
    /// `generations_per_lifetime`
    pub generations_per_lifetime: f64,

    /// `_germline` (`cGermline::m_germline`)
    pub germline: Vec<Genome>,

    /// `deme_resource_count`
    pub resource_count: ResourceCount,
    /// `energy_res_ids`
    pub energy_res_ids: Vec<usize>,

    /// `m_germline_genotype_id`
    pub germline_genotype_id: i32,
    /// `m_founder_genotype_ids`
    pub founder_genotype_ids: Vec<i32>,
    /// `m_founder_phenotypes`, of which only the generation and the
    /// permanent germline propensity are read.
    pub founder_generations: Vec<i32>,
    pub founder_germline_propensities: Vec<f64>,

    /// `_current_merit`
    pub current_merit: Merit,
    /// `_next_merit`
    pub next_merit: Merit,

    /// `points`, `migrations_out`, `migrations_in`, `suicides`
    pub points: f64,
    pub migrations_out: u32,
    pub migrations_in: u32,
    pub suicides: u32,

    /// `m_total_res_consumed`
    pub total_res_consumed: f64,
    /// `m_switch_penalties`
    pub switch_penalties: i32,
    /// `m_shannon_matrix`
    pub shannon_matrix: Vec<Vec<f64>>,
    /// `m_num_active`
    pub num_active: i32,
    /// `m_num_reproductives`
    pub num_reproductives: i32,

    /// `cell_events`, `event_slot_end_points`, `deme_pred_list` and
    /// `movement_pred_list`
    pub events: DemeEvents,
}

/// The values `DivideReset(ctx, parent_deme, ...)` reads from its parent,
/// taken before the reset (the parent may be the deme itself; none of
/// these is changed before the C++ reads it).
#[derive(Debug, Clone)]
pub struct DivideParent {
    avg_founder_generation: f64,
    generation: i32,
    time_used: i32,
    normalized_time_used: f64,
    cur_task_exe_count: Vec<i32>,
    cur_reaction_count: Vec<i32>,
    cur_org_task_count: Vec<i32>,
    cur_org_task_exe_count: Vec<i32>,
    cur_org_reaction_count: Vec<i32>,
    last_org_count: i32,
    birth_count: i32,
}

impl Deme {
    /// `cDeme()` followed by `Setup(id, in_cells, in_width, world)` and
    /// `SetDemeResourceCount(cResourceCount(num_deme_res))`.
    pub fn new(
        id: i32,
        cell_ids: Vec<i32>,
        in_width: i32,
        num_tasks: usize,
        num_reactions: usize,
        num_deme_res: usize,
    ) -> Self {
        // If width is negative, set it to the full number of cells.
        let width = if in_width < 1 {
            cell_ids.len() as i32
        } else {
            in_width
        };
        Deme {
            id,
            cell_ids,
            width,
            replicate_deme: false,
            treatable: false,
            treatment_ages: BTreeSet::new(),
            cur_birth_count: 0,
            last_birth_count: 0,
            cur_org_count: 0,
            last_org_count: 0,
            injected_count: 0,
            birth_count_perslot: 0,
            age: 0,
            generation: 0,
            parasite_memory_score: 0.0,
            time_used: 0,
            gestation_time: 0,
            cur_normalized_time_used: 0.0,
            last_normalized_time_used: 0.0,
            msg_send_failed: 0,
            msg_dropped: 0,
            msg_successfully_sent: 0,
            energy_injected_into_organisms: 0.0,
            energy_remaining_in_deme_at_replication: 0.0,
            total_energy_testament: 0.0,
            events_total: 0,
            events_killed: 0,
            events_killed_this_slot: 0,
            event_kill_attempts: 0,
            event_kill_attempts_this_slot: 0,
            consecutive_successful_event_periods: 0,
            sleeping_count: 0,
            total_energy_donated: 0.0,
            total_energy_received: 0.0,
            total_energy_applied: 0.0,
            cur_task_exe_count: vec![0; num_tasks],
            cur_reaction_count: vec![0; num_reactions],
            last_task_exe_count: vec![0; num_tasks],
            last_reaction_count: vec![0; num_reactions],
            cur_org_task_count: vec![0; num_tasks],
            cur_org_task_exe_count: vec![0; num_tasks],
            cur_org_reaction_count: vec![0; num_reactions],
            last_org_task_count: vec![0; num_tasks],
            last_org_task_exe_count: vec![0; num_tasks],
            last_org_reaction_count: vec![0; num_reactions],
            avg_founder_generation: 0.0,
            generations_per_lifetime: 0.0,
            germline: Vec::new(),
            resource_count: ResourceCount::new(num_deme_res),
            energy_res_ids: Vec::new(),
            germline_genotype_id: 0,
            founder_genotype_ids: Vec::new(),
            founder_generations: Vec::new(),
            founder_germline_propensities: Vec::new(),
            current_merit: Merit::new(1.0),
            next_merit: Merit::new(1.0),
            points: 0.0,
            migrations_out: 0,
            migrations_in: 0,
            suicides: 0,
            total_res_consumed: 0.0,
            switch_penalties: 0,
            shannon_matrix: Vec::new(),
            num_active: 0,
            num_reproductives: 0,
            events: DemeEvents::default(),
        }
    }

    /// `GetSize()`
    pub fn size(&self) -> i32 {
        self.cell_ids.len() as i32
    }

    /// `GetCellID(pos)`
    pub fn cell_id(&self, pos: i32) -> i32 {
        self.cell_ids[pos as usize]
    }

    /// `GetHeight()`
    pub fn height(&self) -> i32 {
        self.size() / self.width
    }

    /// `GetCellID(x, y)`
    pub fn cell_id_xy(&self, x: i32, y: i32) -> i32 {
        self.cell_ids[(y * self.width + x) as usize]
    }

    /// `GetCellPosition(cellid)`: assumes monotonically increasing ids.
    pub fn cell_position(&self, cell_id: i32) -> (i32, i32) {
        (cell_id % self.width, (cell_id % self.size()) / self.width)
    }

    /// `GetRelativeCellID(absolute_cell_id)`: assumes equal-sized demes.
    pub fn relative_cell_id(&self, absolute_cell_id: i32) -> i32 {
        absolute_cell_id % self.size()
    }

    /// `GetDensity()`
    pub fn density(&self) -> f64 {
        f64::from(self.cur_org_count) / f64::from(self.size())
    }

    /// `IsEmpty()`
    pub fn is_empty(&self) -> bool {
        self.cur_org_count == 0
    }

    /// `IsFull()`
    pub fn is_full(&self) -> bool {
        self.cur_org_count == self.size()
    }

    /// `IncBirthCount()`
    pub fn inc_birth_count(&mut self) {
        self.cur_birth_count += 1;
        self.birth_count_perslot += 1;
    }

    /// `HasDemeMerit()`
    pub fn has_deme_merit(&self) -> bool {
        self.current_merit.get_double() != 1.0
    }

    /// `IncTimeUsed(merit)`: `time_used++` and `1/merit/cur_org_count` more
    /// normalized time (infinite for an empty deme, as in C++).
    pub fn inc_time_used(&mut self, merit: f64) {
        self.time_used += 1;
        self.cur_normalized_time_used += 1.0 / merit / f64::from(self.cur_org_count);
    }

    /// `UpdateParasiteMemoryScore(decay)`; `parasite_load` is
    /// `GetParasiteLoad()`.
    pub fn update_parasite_memory_score(&mut self, parasite_load: f64, decay: f64) {
        self.parasite_memory_score += parasite_load;
        self.parasite_memory_score *= decay;
    }

    /// `GetSlotFlowRate()`
    pub fn slot_flow_rate(&self) -> i32 {
        self.events.slot_flow_rate(self.age)
    }

    /// `MinNumTimesReactionPerformed()`
    pub fn min_num_times_reaction_performed(&self) -> i32 {
        let mut min = 100_000_000;
        for &c in &self.cur_reaction_count {
            if c < min {
                min = c;
            }
        }
        min
    }

    /// `ProcessPreUpdate()`
    pub fn process_pre_update(&mut self, update: i32) {
        self.resource_count.set_spatial_update(update);
    }

    /// `Reset(ctx, resetResources, deme_energy)`
    pub fn reset(
        &mut self,
        cfg: &Config,
        rng: &mut AvidaRng,
        res_env: &mut dyn ResourceEnv,
        reset_resources: bool,
        deme_energy: f64,
    ) {
        let mut additional_resource = 0.0;
        // Handle energy model
        if cfg.energy_enabled {
            self.total_energy_testament = 0.0;
            if cfg.energy_passed_on_deme_replication_method == 1 {
                // split deme energy evenly between cell in deme
                additional_resource = deme_energy;
            }
        }

        // Handle stat and resource resets
        self.age = 0;
        self.time_used = 0;
        self.cur_birth_count = 0;
        self.cur_normalized_time_used = 0.0;
        self.parasite_memory_score = 0.0;
        self.injected_count = 0;
        self.birth_count_perslot = 0;
        self.events_total = 0;
        self.events_killed = 0;
        self.events_killed_this_slot = 0;
        self.event_kill_attempts = 0;
        self.event_kill_attempts_this_slot = 0;
        self.sleeping_count = 0;
        self.msg_send_failed = 0;
        self.msg_dropped = 0;
        self.msg_successfully_sent = 0;
        self.total_res_consumed = 0.0;
        self.switch_penalties = 0;
        self.num_active = 0;
        self.shannon_matrix.clear();
        self.num_reproductives = 0;

        self.consecutive_successful_event_periods = 0;
        self.replicate_deme = false;
        self.total_energy_donated = 0.0;
        self.total_energy_received = 0.0;
        self.total_energy_applied = 0.0;

        self.cur_task_exe_count.fill(0);
        self.cur_reaction_count.fill(0);
        // The reference binary includes d19201d03 ("Fix missing array
        // initialization in cDeme::Reset()"), which postdates 47f13dadb:
        // the `last_` counters `DivideReset` has just copied from the
        // parent, and the per-organism counters, are zeroed too.
        self.last_task_exe_count.fill(0);
        self.last_reaction_count.fill(0);
        self.cur_org_task_count.fill(0);
        self.cur_org_task_exe_count.fill(0);
        self.cur_org_reaction_count.fill(0);
        self.last_org_task_count.fill(0);
        self.last_org_task_exe_count.fill(0);
        self.last_org_reaction_count.fill(0);

        //reset remaining deme and movement predicates
        self.events.reset_predicates();

        if reset_resources {
            self.resource_count
                .reinitialize_resources(rng, res_env, additional_resource);
        }
    }

    /// What [`Deme::divide_reset`] reads from this deme as the parent.
    pub fn divide_parent(&self) -> DivideParent {
        DivideParent {
            avg_founder_generation: self.avg_founder_generation,
            generation: self.generation,
            time_used: self.time_used,
            normalized_time_used: self.cur_normalized_time_used,
            cur_task_exe_count: self.cur_task_exe_count.clone(),
            cur_reaction_count: self.cur_reaction_count.clone(),
            cur_org_task_count: self.cur_org_task_count.clone(),
            cur_org_task_exe_count: self.cur_org_task_exe_count.clone(),
            cur_org_reaction_count: self.cur_org_reaction_count.clone(),
            last_org_count: self.last_org_count,
            birth_count: self.cur_birth_count,
        }
    }

    /// `DivideReset(ctx, parent_deme, resetResources, deme_energy)`
    pub fn divide_reset(
        &mut self,
        parent: &DivideParent,
        cfg: &Config,
        rng: &mut AvidaRng,
        res_env: &mut dyn ResourceEnv,
        reset_resources: bool,
        deme_energy: f64,
    ) {
        // the parent might be us, so save this value...
        let old_avg_founder_generation = parent.avg_founder_generation;

        // update our average founder generation
        let mut gen = DoubleSum::new();
        for &g in &self.founder_generations {
            gen.add(f64::from(g));
        }
        self.avg_founder_generation = gen.average();

        // update our generations per lifetime based on current founders and
        // parents generation
        self.generations_per_lifetime = self.avg_founder_generation - old_avg_founder_generation;

        // Save statistics according to parent before reset.
        self.generation = parent.generation + 1;
        self.gestation_time = parent.time_used;
        self.last_normalized_time_used = parent.normalized_time_used;

        self.last_task_exe_count
            .clone_from(&parent.cur_task_exe_count);
        self.last_reaction_count
            .clone_from(&parent.cur_reaction_count);

        self.last_org_task_count
            .clone_from(&parent.cur_org_task_count);
        self.last_org_task_exe_count
            .clone_from(&parent.cur_org_task_exe_count);
        self.last_org_reaction_count
            .clone_from(&parent.cur_org_reaction_count);

        // Org count was updated upon KillAll()....
        self.last_org_count = parent.last_org_count;
        self.last_birth_count = parent.birth_count;

        self.reset(cfg, rng, res_env, reset_resources, deme_energy);
    }

    /// `UpdateDemeMerit()`: the parent rotates its heritable merit in.
    pub fn update_deme_merit(&mut self) {
        self.current_merit = self.next_merit;
        self.next_merit = Merit::new(1.0);
    }

    /// `UpdateDemeMerit(source)`; `source_heritable` is
    /// `source.GetHeritableDemeMerit()`.
    pub fn update_deme_merit_from(&mut self, source_heritable: Merit) {
        self.current_merit = source_heritable;
        self.next_merit = Merit::new(1.0);
    }

    /// `ClearShannonInformationStats()`
    pub fn clear_shannon_information_stats(&mut self) {
        self.total_res_consumed = 0.0;
        self.switch_penalties = 0;
        self.num_active = 0;
        self.shannon_matrix.clear();
        self.num_reproductives = 0;
    }

    /// `GetShannonMutualInformation()`
    pub fn shannon_mutual_information(&self) -> f64 {
        // exit if 0 or 1 organism did stuff.
        if self.num_active == 0 || self.num_active == 1 {
            return 0.0;
        }
        let num_org = self.shannon_matrix.len();
        let num_task = self.shannon_matrix[0].len();
        let active = f64::from(self.num_active);
        let mut ptask = vec![0.0; num_task];
        for (j, p) in ptask.iter_mut().enumerate() {
            for row in &self.shannon_matrix {
                *p += row[j];
            }
            *p /= active;
        }

        let mut shannon_sum = 0.0;
        let pi = 1.0 / active;
        for row in self.shannon_matrix.iter().take(num_org) {
            for (j, &pj) in ptask.iter().enumerate() {
                let pij = row[j] / active;
                if pi != 0.0 && pj != 0.0 && pij != 0.0 {
                    shannon_sum += pij * (pij / (pi * pj)).ln();
                }
            }
        }
        shannon_sum
    }

    /// `GetPercentReproductives()`
    pub fn percent_reproductives(&self) -> f64 {
        (f64::from(self.num_reproductives)
            / (f64::from(self.injected_count) + f64::from(self.cur_birth_count)))
            * 100.0
    }

    /// The organism half of `UpdateShannon(cell)`: `curr_react` is the
    /// occupant's `GetCumulativeReactionCount()`, `repro` its
    /// `GetNumDivides() - GetNumDivideFailed()`, `track` is
    /// `DEMES_TRACK_SHANNON_INFO`.
    pub fn update_shannon(&mut self, curr_react: &[i32], repro: i32, track: i32) {
        let mut org_react_count = 0;
        let mut org_row = vec![0.0; curr_react.len()];
        // track number of reproductives
        if repro > 0 {
            self.num_reproductives += 1;
        }
        for (j, &c) in curr_react.iter().enumerate() {
            // we are tracking repro as a task
            if track == 2 && j == 0 {
                org_react_count += repro;
                org_row[j] = f64::from(repro);
            } else {
                org_react_count += c;
                org_row[j] = f64::from(c);
            }
        }
        if org_react_count > 0 {
            // normalize the data for the current organism.
            for v in &mut org_row {
                if *v != 0.0 {
                    *v /= f64::from(org_react_count);
                }
            }
            self.num_active += 1;
            self.shannon_matrix.push(org_row);
        }
    }

    /// `ReplaceGermline(germline)`
    pub fn replace_germline(&mut self, germline: &[Genome]) {
        self.germline = germline.to_vec();
    }
}

impl PhenotypeDeme for Deme {
    fn heritable_deme_merit(&self) -> f64 {
        self.next_merit.get_double()
    }
    fn update_heritable_deme_merit(&mut self, value: f64) {
        self.next_merit = Merit::new(value);
    }
    fn add_cur_task(&mut self, task_num: usize) {
        self.cur_task_exe_count[task_num] += 1;
    }
    fn add_cur_reaction(&mut self, reaction_num: usize) {
        self.cur_reaction_count[reaction_num] += 1;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn geometry_and_counters() {
        let mut d = Deme::new(2, (20..30).collect(), 5, 3, 2, 0);
        assert_eq!((d.size(), d.height()), (10, 2));
        assert_eq!(d.cell_id_xy(2, 1), 27);
        assert_eq!(d.cell_position(27), (2, 1));
        assert_eq!(d.relative_cell_id(27), 7);
        assert!(d.is_empty());
        d.cur_org_count = 10;
        assert!(d.is_full());
        d.inc_time_used(2.0);
        assert_eq!(d.cur_normalized_time_used, 0.05);
        assert!(!d.has_deme_merit());
        d.update_heritable_deme_merit(3.0);
        d.update_deme_merit();
        assert!(d.has_deme_merit());
        assert_eq!(d.next_merit.get_double(), 1.0);
    }

    #[test]
    fn divide_reset_copies_parent_statistics() {
        let cfg = Config::default();
        let mut rng = AvidaRng::new(1);
        let mut env = avida_resources::RecordingEnv::default();
        let mut parent = Deme::new(0, (0..4).collect(), 2, 1, 1, 0);
        parent.generation = 4;
        parent.time_used = 17;
        parent.cur_birth_count = 6;
        parent.last_org_count = 3;
        parent.cur_reaction_count = vec![5];
        let snap = parent.divide_parent();
        let mut child = Deme::new(1, (4..8).collect(), 2, 1, 1, 0);
        child.age = 9;
        child.divide_reset(&snap, &cfg, &mut rng, &mut env, true, 0.0);
        assert_eq!(child.generation, 5);
        assert_eq!(child.gestation_time, 17);
        assert_eq!(child.last_birth_count, 6);
        assert_eq!(child.last_org_count, 3);
        // copied, then zeroed by `Reset` (d19201d03)
        assert_eq!(child.last_reaction_count, vec![0]);
        assert_eq!(child.age, 0);
    }

    #[test]
    fn shannon_information() {
        let mut d = Deme::new(0, (0..4).collect(), 2, 0, 2, 0);
        d.update_shannon(&[2, 0], 1, 1);
        d.update_shannon(&[0, 3], 0, 1);
        d.update_shannon(&[0, 0], 0, 1);
        assert_eq!(d.num_active, 2);
        assert_eq!(d.num_reproductives, 1);
        assert!((d.shannon_mutual_information() - std::f64::consts::LN_2).abs() < 1e-12);
    }
}
