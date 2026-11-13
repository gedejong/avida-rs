//! Port of `cOrganism` (avida-core/source/main/cOrganism.{h,cc} at
//! 47f13dadb), split as ARCHITECTURE.md prescribes:
//!
//! * [`OrgState`]: everything `cOrganism` holds except the hardware and the
//!   interface pointer, with the `cOrganism` methods that only touch that
//!   state (the interface, where C++ used `m_interface`, is passed in);
//! * [`Organism`]: state + hardware, with the `cOrganism` methods that
//!   touch the hardware (`HardwareReset`, `PrintStatus`, `DoOutput` with
//!   its bonus instructions, ...).
//!
//! Hardware instruction handlers run with `(&mut self /* hardware */,
//! org: &mut OrgState, world: &mut dyn OrgInterface)`; the organism-level
//! operations they trigger are the free functions [`do_output`] and
//! [`activate_divide`], which return what the hardware must do afterwards.

use crate::hardware::Hardware;
use crate::hardware_transsmt::ParasiteUnit;
use crate::nhardware::*;
use crate::org_interface::{with_neighbor_state, OrgInterface, OrgMessage, WorldServices};
use crate::task_adapter::{OrgTaskEvaluator, OrgTaskWorld};
use avida_apto::fmt::fmt_g;
use avida_config::config::Config;
use avida_environment::{ContextPhenotype, MutationRates};
use avida_genome::{Genome, InstSet, Instruction};
use avida_phenotype::{Phenotype, PhenotypeSizes};
use avida_tasks::{IoBuffer, TaskContext};
use std::collections::{BTreeSet, VecDeque};
use std::sync::Arc;

/// `Systematics::TransmissionType` as its C++ integer (UNKNOWN 0,
/// DIVISION 1, DUPLICATION 2, VERTICAL 3, HORIZONTAL 4).
pub mod transmission {
    /// `UNKNOWN`
    pub const UNKNOWN: i32 = 0;
    /// `DIVISION`
    pub const DIVISION: i32 = 1;
    /// `DUPLICATION`
    pub const DUPLICATION: i32 = 2;
    /// `VERTICAL`
    pub const VERTICAL: i32 = 3;
    /// `HORIZONTAL`
    pub const HORIZONTAL: i32 = 4;
}

/// The identity of one `cOrganism` object: the port's stand-in for the
/// `cOrganism*` the C++ compares and stores (avatar lists, trace queues,
/// look-sensor targets, a held victim).
///
/// `cOrganism::GetID()` cannot serve: `m_id` is `GetTotCreatures()` at
/// construction, and the two offspring of a sexual birth are both built
/// before `RecordBirth` counts either, so they share one id. A uid is drawn
/// from a world counter when the organism is built
/// ([`WorldServices::new_org_uid`]) and never reused; [`OrgUid::NONE`] (0)
/// marks an organism that never enters a population (test CPU).
/// Everything the C++ prints or compares as `GetID()` keeps using the id.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct OrgUid(pub u64);

impl OrgUid {
    /// The uid of an organism outside any population.
    pub const NONE: OrgUid = OrgUid(0);
}

/// `Systematics::Source` (the systematics crate has the full type; this
/// mirror keeps the hardware crate independent of it).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct OrgSource {
    /// `transmission_type` (see [`transmission`]).
    pub transmission_type: i32,
    /// `arguments`
    pub arguments: String,
    /// `external`
    pub external: bool,
}

impl OrgSource {
    /// `Source(t, a, e)`.
    pub fn new(transmission_type: i32, arguments: &str, external: bool) -> Self {
        OrgSource {
            transmission_type,
            arguments: arguments.to_string(),
            external,
        }
    }
}

/// The `cOrganism` fields task evaluation reads or writes through
/// `ctx.GetOrganism()`, copied out so the organism's buffers and phenotype
/// can be borrowed by the task context at the same time.
#[derive(Debug, Clone, Default)]
pub struct OrgTaskSnapshot {
    pub forage_target: i32,
    pub gradient_movement: f64,
    /// `GetOpinion()` (`(opinion, update)`), `None` without opinions.
    pub opinion: Option<(i32, i32)>,
    pub killed_event: bool,
    pub lyse_display: bool,
    pub kaboom_executed: bool,
    pub kaboom_executed2: bool,
    /// `(tag, count)` of `m_string_map[...].prod_string`.
    pub strings_produced: Vec<(i32, i32)>,
    /// `(tag, count)` of `m_string_map[...].on_hand`.
    pub strings_on_hand: Vec<(i32, i32)>,
    /// `m_tag`
    pub tag: (i32, i32),
    pub lineage_label: i32,
    pub num_donations: i32,
    /// `m_world->GetStats().GetUpdate()`
    pub update: i32,
    /// `UpdateTag(tag, bits)` calls, applied after the test (they draw from
    /// the world RNG, which the environment holds during evaluation).
    pub pending_tags: Vec<(i32, i32)>,
    /// `SetReputation(rep)` (= `SetOpinion(rep)`) calls, applied after.
    pub pending_opinions: Vec<i32>,
}

/// `cOrganism` without its hardware and interface.
#[derive(Debug, Clone)]
pub struct OrgState {
    /// `m_id`
    pub id: i32,
    /// The object identity (`this`), see [`OrgUid`].
    pub uid: OrgUid,
    /// `m_phenotype`
    pub phenotype: Phenotype,
    /// The array sizes the phenotype was built with.
    pub phenotype_sizes: PhenotypeSizes,
    /// `m_src`
    pub src: OrgSource,
    /// `m_initial_genome` (`UnitGenome()` / `GetGenome()`).
    pub initial_genome: Genome,
    /// `m_mut_rates`
    pub mut_rates: MutationRates,
    /// `m_lineage_label`
    pub lineage_label: i32,
    /// `cclade_id`
    pub cclade_id: i32,
    /// `m_org_list_index`
    pub org_list_index: i32,
    /// `m_display`
    pub display: bool,
    /// `m_org_display`
    pub org_display: Option<crate::org_sensor::OrgDisplay>,
    /// `m_queued_display_data`: what the look sensor last saw
    /// (`cOrgSensor::SetPotentialDisplayData`), swapped in by
    /// `UpdateOrgDisplay`.
    pub queued_display: Option<crate::org_sensor::OrgDisplay>,
    /// `m_lyse_display`
    pub lyse_display: bool,
    /// `m_offspring_genome`
    pub offspring_genome: Genome,
    /// `m_input_pointer`
    pub input_pointer: i32,
    /// `m_input_buf`
    pub input_buf: IoBuffer,
    /// `m_output_buf`
    pub output_buf: IoBuffer,
    /// `m_received_messages`
    pub received_messages: IoBuffer,
    /// `m_cur_sg`
    pub cur_sg: i32,
    /// `m_sent_value`
    pub sent_value: i32,
    /// `m_sent_active`
    pub sent_active: bool,
    /// `m_test_receive_pos`
    pub test_receive_pos: i32,
    /// `m_gradient_movement`
    pub gradient_movement: f64,
    /// `m_pher_drop`
    pub pher_drop: bool,
    /// `frac_energy_donating`
    pub frac_energy_donating: f64,
    /// `m_max_executed`
    pub max_executed: i32,
    /// `m_is_running`
    pub is_running: bool,
    /// `m_is_sleeping`
    pub is_sleeping: bool,
    /// `m_is_dead`
    pub is_dead: bool,
    /// `killed_event`
    pub killed_event: bool,
    /// `m_opinion->opinion_list` (`(opinion, update)`, oldest first);
    /// `None` until `InitOpinions()`.
    pub opinions: Option<Vec<(i32, i32)>>,
    /// `m_self_raw_materials`
    pub self_raw_materials: i32,
    /// `m_other_raw_materials`
    pub other_raw_materials: i32,
    /// `m_num_donate`
    pub num_donate: i32,
    /// `m_num_donate_received`
    pub num_donate_received: i32,
    /// `m_amount_donate_received`
    pub amount_donate_received: i32,
    /// `m_num_reciprocate`
    pub num_reciprocate: i32,
    /// `m_failed_reputation_increases`
    pub failed_reputation_increases: i32,
    /// `m_tag`
    pub tag: (i32, i32),
    /// `m_northerly`
    pub northerly: i32,
    /// `m_easterly`
    pub easterly: i32,
    /// `m_forage_target`
    pub forage_target: i32,
    /// `m_show_ft`
    pub show_ft: i32,
    /// `m_has_set_ft`
    pub has_set_ft: bool,
    /// `m_teach`
    pub teach: bool,
    /// `m_parent_teacher`
    pub parent_teacher: bool,
    /// `m_parent_ft`
    pub parent_ft: i32,
    /// `m_parent_group`
    pub parent_group: i32,
    /// `m_p_merit`
    pub p_merit: f64,
    /// `m_beggar`
    pub beggar: bool,
    /// `m_para_donate`
    pub para_donate: f64,
    /// `m_guard`
    pub guard: bool,
    /// `m_num_guard`
    pub num_guard: i32,
    /// `m_num_deposits`
    pub num_deposits: i32,
    /// `m_amount_deposited`
    pub amount_deposited: f64,
    /// `m_num_point_mut`
    pub num_point_mut: i32,
    /// `m_av_in_index`
    pub av_in_index: i32,
    /// `m_av_out_index`
    pub av_out_index: i32,
    /// `m_repair`
    pub repair: i32,
    /// `m_string_map` as `(tag, prod_string, on_hand)` sorted by tag;
    /// `None` while the C++ pointer is NULL (before `InitStringMap`).
    /// `received_string` is never read in C++ and is not kept.
    pub string_map: Option<Vec<(i32, i32, i32)>>,
    /// `m_parent_multi_threaded`
    pub parent_multi_threaded: bool,
    /// `m_msg->received` (`cMessagingSupport`): messages not yet retrieved.
    pub msg_received: VecDeque<OrgMessage>,
    /// `m_msg->sent`: the last `MESSAGE_SEND_BUFFER_SIZE` messages sent.
    pub msg_sent: VecDeque<OrgMessage>,
    /// `donor_list`: ids of the organisms that donated to this one.
    pub donor_list: BTreeSet<i32>,
    /// `donating_lineages`: lineage labels of those donors.
    pub donating_lineages: BTreeSet<i32>,
    /// `m_neighborhood`: the snapshot taken by `LoadNeighborhood`, `None`
    /// until then (`loaded == false`).
    pub neighborhood: Option<BTreeSet<i32>>,
    /// `m_parasites`: every parasite that infected this organism since its
    /// hardware was last reset, in infection order (`GetParasites()`).
    pub parasites: Vec<Arc<ParasiteUnit>>,
    /// The organism's budget, allocation carry and random stream in
    /// `EXECUTION_MODE 1` (not in the C++; untouched in the exact mode).
    /// They live on the organism so that they move with it.
    pub parallel: crate::parallel::ParallelOrgState,
}

/// Inputs of `cOrganism::cOrganism` beyond the world.
pub struct OrgInit<'a> {
    pub genome: &'a Genome,
    pub parent_generation: i32,
    pub src: OrgSource,
}

impl OrgState {
    /// The member initialisers of `cOrganism::cOrganism` (before the
    /// hardware exists and before `initialize`).
    pub fn new(
        world: &dyn WorldServices,
        genome: &Genome,
        parent_generation: i32,
        src: OrgSource,
        num_nops: i32,
    ) -> Self {
        let cfg = world.config();
        let env = world.environment();
        let sizes = PhenotypeSizes {
            num_tasks: env.num_tasks().max(0) as usize,
            num_resources: env.resource_lib().size().max(0) as usize,
            num_reactions: env.reaction_lib().size().max(0) as usize,
            sense_size: world.sense_size(),
        };
        let phenotype = Phenotype::new(cfg, &sizes, parent_generation, num_nops);
        OrgState {
            id: world.tot_creatures(),
            uid: OrgUid::NONE,
            phenotype,
            phenotype_sizes: sizes,
            src,
            initial_genome: genome.clone(),
            mut_rates: MutationRates::default(),
            lineage_label: -1,
            cclade_id: -1,
            org_list_index: -1,
            display: false,
            org_display: None,
            queued_display: None,
            lyse_display: false,
            offspring_genome: Genome::default(),
            input_pointer: 0,
            input_buf: IoBuffer::new(env.input_size().max(0) as usize),
            output_buf: IoBuffer::new(env.output_size().max(0) as usize),
            received_messages: IoBuffer::new(RECEIVED_MESSAGES_SIZE as usize),
            cur_sg: 0,
            sent_value: 0,
            sent_active: false,
            test_receive_pos: 0,
            gradient_movement: 0.0,
            pher_drop: false,
            frac_energy_donating: cfg.energy_sharing_pct,
            max_executed: -1,
            is_running: false,
            is_sleeping: false,
            is_dead: false,
            killed_event: false,
            opinions: None,
            self_raw_materials: cfg.raw_material_amount,
            other_raw_materials: 0,
            num_donate: 0,
            num_donate_received: 0,
            amount_donate_received: 0,
            num_reciprocate: 0,
            failed_reputation_increases: 0,
            tag: (-1, 0),
            northerly: 0,
            easterly: 0,
            forage_target: -1,
            show_ft: -1,
            has_set_ft: false,
            teach: false,
            parent_teacher: false,
            parent_ft: -1,
            parent_group: cfg.default_group,
            p_merit: 0.0,
            beggar: false,
            para_donate: cfg.parasite_virulence,
            guard: false,
            num_guard: 0,
            num_deposits: 0,
            amount_deposited: 0.0,
            num_point_mut: 0,
            av_in_index: -1,
            av_out_index: -1,
            repair: 0,
            string_map: None,
            parent_multi_threaded: false,
            msg_received: VecDeque::new(),
            msg_sent: VecDeque::new(),
            donor_list: BTreeSet::new(),
            donating_lineages: BTreeSet::new(),
            neighborhood: None,
            parasites: Vec::new(),
            parallel: crate::parallel::ParallelOrgState::default(),
        }
    }

    /// `GetNumParasites()`
    pub fn num_parasites(&self) -> i32 {
        self.parasites.len() as i32
    }

    /// `ClearParasites()` (`m_parasites.Resize(0)`): the C++ array deletes
    /// its old elements with `delete[]`, last to first. A parasite whose
    /// thread still runs keeps that reference.
    pub fn clear_parasites(&mut self) {
        while let Some(p) = self.parasites.pop() {
            drop(p);
        }
    }

    /// `cOrganism::initialize(ctx)` after the hardware was created.
    pub fn initialize(
        &mut self,
        world: &mut dyn WorldServices,
        inst_set_size: i32,
        inst_set_name: &str,
    ) {
        self.phenotype
            .set_inst_set_size(inst_set_size.max(0) as usize);
        self.initial_genome.set_property("instset", inst_set_name);
        let ga = world.group_attack_inst_count(inst_set_name);
        self.phenotype.set_group_attack_inst_set_size(ga);

        let death_method = world.config().death_method;
        let age_limit = world.config().age_limit;
        let age_deviation = world.config().age_deviation;
        if death_method > DEATH_METHOD_OFF {
            self.max_executed = age_limit;
            if age_deviation > 0 {
                let r = world.rng().get_rand_normal();
                self.max_executed = self
                    .max_executed
                    .wrapping_add((r * f64::from(age_deviation)) as i32);
            }
            if death_method == DEATH_METHOD_MULTIPLE {
                self.max_executed = self
                    .max_executed
                    .wrapping_mul(self.initial_genome.representation().size());
            }
            // m_max_executed must be positive or an organism will not die!
            if self.max_executed < 1 {
                self.max_executed = 1;
            }
        }

        self.repair = world.config().point_mut_repair_start;

        if world.config().randomize_raw_material_amount != 0 {
            let raw_mat = world.config().raw_material_amount;
            self.self_raw_materials = world
                .rng()
                .get_uint_range(0, raw_mat.wrapping_add(1) as u32)
                as i32;
        }
    }

    /// `GetGenome()` / `UnitGenome()`.
    pub fn genome(&self) -> &Genome {
        &self.initial_genome
    }

    /// `GetID()`
    pub fn id(&self) -> i32 {
        self.id
    }

    /// `GetMaxExecuted()`
    pub fn max_executed(&self) -> i32 {
        self.max_executed
    }

    /// `IsDead()`
    pub fn is_dead(&self) -> bool {
        self.is_dead
    }

    /// `SetRunning(b)`
    pub fn set_running(&mut self, running: bool) {
        self.is_running = running;
    }

    /// `IsRunning()`
    pub fn is_running(&self) -> bool {
        self.is_running
    }

    /// `Die(ctx)`: `m_interface->Die(ctx); m_is_dead = true;`.
    pub fn die(&mut self, world: &mut dyn OrgInterface) {
        world.die(self);
        self.is_dead = true;
    }

    /// `Fault(fault_loc, fault_type, fault_desc)`: counts an error.
    pub fn fault(&mut self, _fault_loc: i32, _fault_type: i32, _fault_desc: &str) {
        self.phenotype.cur_num_errors = self.phenotype.cur_num_errors.wrapping_add(1);
    }

    /// `DoInput(value)`: add to the input buffer (`cPhenotype::TestInput`
    /// does nothing).
    pub fn do_input(&mut self, value: i32) {
        self.input_buf.add(value);
    }

    /// `GetNextInput()`: `m_interface->GetInputAt(m_input_pointer)`.
    pub fn get_next_input(&mut self, world: &mut dyn OrgInterface) -> i32 {
        let mut p = self.input_pointer;
        let v = world.get_input_at(&mut p);
        self.input_pointer = p;
        v
    }

    /// `InitOpinions()`.
    fn init_opinions(&mut self) -> &mut Vec<(i32, i32)> {
        self.opinions.get_or_insert_with(Vec::new)
    }

    /// `HasOpinion()`.
    pub fn has_opinion(&mut self) -> bool {
        !self.init_opinions().is_empty()
    }

    /// `GetOpinion()`: the newest `(opinion, update)`. The C++ reads past
    /// an empty list (undefined); `(0, 0)` is returned instead.
    pub fn get_opinion(&mut self) -> (i32, i32) {
        self.init_opinions().last().copied().unwrap_or((0, 0))
    }

    /// `ClearOpinion()`.
    pub fn clear_opinion(&mut self) {
        self.init_opinions().clear();
    }

    /// `SetOpinion(opinion)`. An `OPINION_BUFFER_SIZE` of 0 aborts the C++
    /// run (`INVALID_CONFIG`); it is reported as an error here.
    pub fn set_opinion(&mut self, cfg: &Config, update: i32, opinion: i32) -> Result<(), String> {
        self.set_opinion_bsize(cfg.opinion_buffer_size, update, opinion)
    }

    /// [`Self::set_opinion`] with `OPINION_BUFFER_SIZE` passed in, for
    /// callers that hold the world mutably (a neighbour's reputation).
    pub fn set_opinion_bsize(
        &mut self,
        bsize: i32,
        update: i32,
        opinion: i32,
    ) -> Result<(), String> {
        if bsize == 0 {
            return Err("OPINION_BUFFER_SIZE is set to an invalid value.".to_string());
        }
        let list = self.init_opinions();
        if bsize > 0 || bsize == -1 {
            list.push((opinion, update));
            while bsize != -1 && list.len() as i32 > bsize {
                list.remove(0);
            }
        }
        Ok(())
    }

    /// `GetRBins()`
    pub fn rbins(&self) -> &[f64] {
        &self.phenotype.cur_rbins_avail
    }

    /// `GetRBin(index)`; 0 outside the array (undefined in C++).
    pub fn rbin(&self, index: i32) -> f64 {
        self.phenotype
            .cur_rbins_avail
            .get(index as usize)
            .copied()
            .unwrap_or(0.0)
    }

    /// `GetRBinsTotal()`
    pub fn rbins_total(&self) -> f64 {
        let mut total = 0.0;
        for v in &self.phenotype.cur_rbins_avail {
            total += *v;
        }
        total
    }

    /// `AddToRBin(index, value)`.
    pub fn add_to_rbin(&mut self, index: i32, value: f64) {
        if let Some(v) = self.phenotype.cur_rbins_avail.get_mut(index as usize) {
            *v += value;
        }
        if value > 0.0 {
            if let Some(v) = self.phenotype.cur_rbins_total.get_mut(index as usize) {
                *v += value;
            }
        }
    }

    /// The state half of `HardwareReset(ctx)` (the extended-memory setup is
    /// done by [`Organism::hardware_reset`]): clear the opinion unless
    /// `INHERIT_OPINION`, drop the display data.
    pub fn hardware_reset_state(&mut self, cfg: &Config) {
        if cfg.inherit_opinion == 0 {
            self.clear_opinion();
        }
        self.org_display = None;
        self.queued_display = None;
        self.display = false;
    }

    /// `NotifyDeath(ctx)`.
    pub fn notify_death(&mut self, world: &mut dyn OrgInterface) {
        if self.is_sleeping {
            self.is_sleeping = false;
            world.deme_dec_sleeping_count();
        }
        let use_bins = world.config().use_resource_bins;
        let return_stored = world.config().return_stored_on_death;
        if use_bins && return_stored {
            let bins = self.phenotype.cur_rbins_avail.clone();
            if world.config().use_avatars != 0 {
                world.update_av_resources(self, &bins, 0);
            } else {
                world.update_resources(self, &bins);
            }
        }
        if world.config().use_form_groups != 0 && self.has_opinion() {
            let g = self.get_opinion().0;
            world.leave_group(self, g);
        }
    }

    /// `cOrganism::Divide_CheckViable(ctx)`.
    pub fn divide_check_viable(&mut self, world: &mut dyn OrgInterface) -> bool {
        let cfg = world.config();
        if self.phenotype.cur_bonus < cfg.required_bonus {
            return false;
        }

        let required_task = cfg.required_task;
        let immunity_task = cfg.immunity_task;
        if cfg.required_pred_habitat != -1 || cfg.required_prey_habitat != -1 {
            let (habitat_required, required_value) = if self.forage_target <= -2 {
                (cfg.required_pred_habitat, cfg.required_pred_habitat_value)
            } else {
                (cfg.required_prey_habitat, cfg.required_prey_habitat_value)
            };
            if habitat_required != -1 {
                let use_avatars = cfg.use_avatars != 0;
                let habitats: Vec<i32> = world
                    .environment()
                    .resource_lib()
                    .resources()
                    .iter()
                    .map(|r| r.habitat)
                    .collect();
                let mut has_req_res = false;
                for (i, h) in habitats.iter().enumerate() {
                    if *h == habitat_required {
                        let resource_count = if !use_avatars {
                            world.get_resource_val(self, i as i32)
                        } else {
                            world.get_av_resource_val(self, i as i32, 0)
                        };
                        if resource_count >= required_value {
                            has_req_res = true;
                            break;
                        }
                    }
                }
                if !has_req_res {
                    return false;
                }
            }
        }
        let cfg = world.config();
        let task_count = |i: i32| -> i32 {
            self.phenotype
                .cur_task_count
                .get(i as usize)
                .copied()
                .unwrap_or(0)
        };
        if required_task != -1
            && task_count(required_task) == 0
            && (immunity_task == -1 || task_count(immunity_task) == 0)
        {
            self.fault(
                FAULT_LOC_DIVIDE,
                FAULT_TYPE_ERROR,
                &format!("Lacks required task ({})", required_task),
            );
            return false;
        }

        let required_reaction = cfg.required_reaction;
        let immunity_reaction = cfg.immunity_reaction;
        let single_reaction = cfg.require_single_reaction;
        let max_task_count = cfg.max_unique_task_count;
        let reaction_count = |v: &Vec<i32>, i: i32| v.get(i as usize).copied().unwrap_or(0);
        if single_reaction == 0
            && required_reaction != -1
            && reaction_count(&self.phenotype.cur_reaction_count, required_reaction) == 0
            && reaction_count(&self.phenotype.cur_stolen_reaction_count, required_reaction) == 0
            && (immunity_reaction == -1
                || reaction_count(&self.phenotype.cur_reaction_count, immunity_reaction) == 0)
        {
            self.fault(
                FAULT_LOC_DIVIDE,
                FAULT_TYPE_ERROR,
                &format!("Lacks required reaction ({})", required_reaction),
            );
            return false;
        }

        if max_task_count > 0 {
            let mut task_limit = max_task_count;
            for c in &self.phenotype.cur_task_count {
                if *c > 0 {
                    task_limit -= 1;
                }
            }
            if task_limit < 0 {
                self.fault(
                    FAULT_LOC_DIVIDE,
                    FAULT_TYPE_ERROR,
                    "Organism performs more than MAX_TASK_COUNT tasks",
                );
                return false;
            }
        }

        if single_reaction != 0 {
            let mut to_fail = single_reaction;
            for c in &self.phenotype.cur_reaction_count {
                if to_fail == 0 {
                    break;
                }
                if *c > 0 {
                    to_fail -= 1;
                }
            }
            if to_fail != 0 {
                for c in &self.phenotype.cur_stolen_reaction_count {
                    if to_fail == 0 {
                        break;
                    }
                    if *c > 0 {
                        to_fail -= 1;
                    }
                }
            }
            if to_fail != 0 {
                self.fault(
                    FAULT_LOC_DIVIDE,
                    FAULT_TYPE_ERROR,
                    "Lacks sufficient reactions required for divide",
                );
                return false;
            }
        }

        let cfg = world.config();
        let required_resource = cfg.required_resource;
        let required_resource_level = cfg.required_resource_level;
        if required_resource != -1 && required_resource_level > 0.0 {
            let resource_level = self.rbin(required_resource);
            if resource_level < required_resource_level {
                return false;
            }
            self.add_to_rbin(required_resource, -required_resource_level);
        }

        if !self.phenotype.is_fertile {
            self.fault(FAULT_LOC_DIVIDE, FAULT_TYPE_ERROR, "Infertile organism");
            return false;
        }

        let cfg = world.config();
        let cur_merit_base = f64::from(self.phenotype.calc_size_merit(cfg));
        let merit_default_bonus = cfg.merit_default_bonus;
        let mut cur_bonus = self.phenotype.cur_bonus;
        if merit_default_bonus != 0 {
            cur_bonus = f64::from(merit_default_bonus);
        }
        let mut off_merit = cur_merit_base * cur_bonus;
        if cfg.inherit_merit == 0 {
            off_merit = cur_merit_base;
        }
        if off_merit == 0.0 {
            return false;
        }
        true
    }

    /// The task-evaluation view of this organism's state.
    pub fn task_snapshot(&mut self, update: i32) -> OrgTaskSnapshot {
        let opinion = if self.has_opinion() {
            Some(self.get_opinion())
        } else {
            None
        };
        OrgTaskSnapshot {
            forage_target: self.forage_target,
            gradient_movement: self.gradient_movement,
            opinion,
            killed_event: self.killed_event,
            lyse_display: self.lyse_display,
            kaboom_executed: self.phenotype.kaboom_executed,
            kaboom_executed2: self.phenotype.kaboom_executed2,
            strings_produced: self
                .string_map
                .iter()
                .flatten()
                .map(|(t, p, _)| (*t, *p))
                .collect(),
            strings_on_hand: self
                .string_map
                .iter()
                .flatten()
                .map(|(t, _, h)| (*t, *h))
                .collect(),
            tag: self.tag,
            lineage_label: self.lineage_label,
            num_donations: self.num_donate,
            update,
            pending_tags: Vec::new(),
            pending_opinions: Vec::new(),
        }
    }

    /// Write back what task evaluation changed (see [`OrgTaskSnapshot`]).
    pub fn apply_task_snapshot(
        &mut self,
        snap: OrgTaskSnapshot,
        world: &mut dyn OrgInterface,
    ) -> Result<(), String> {
        self.lineage_label = snap.lineage_label;
        // `consume-public-good` may pick the organism itself
        // (`ClearKaboomExecuted`).
        self.phenotype.kaboom_executed = snap.kaboom_executed;
        for (tag, bits) in snap.pending_tags {
            self.update_tag(world, tag, bits);
        }
        for op in snap.pending_opinions {
            let update = world.update();
            self.set_opinion(world.config(), update, op)?;
        }
        Ok(())
    }

    /// `UpdateTag(new_tag, bits)`.
    pub fn update_tag(&mut self, world: &mut dyn OrgInterface, new_tag: i32, bits: i32) {
        let rand_int = world.rng().get_uint_range(0, 2);
        // The C++ draws `rand_int` before testing, whichever branch runs.
        let replace = self.tag.0 == -1
            || self.tag.0 == new_tag
            || self.tag.1 < bits
            || (self.tag.1 == bits && rand_int != 0);
        if replace {
            self.tag = (new_tag, bits);
        }
    }

    /// Mutation-rate shorthands (`cOrganism::TestCopyMut` etc.).
    pub fn mut_rates(&self) -> &MutationRates {
        &self.mut_rates
    }

    /// `PrintFinalStatus(fp, time_used, time_allocated)` given the
    /// hardware's memory string (empty memory reported as death).
    pub fn print_final_status(
        &self,
        cfg: &Config,
        hardware: &dyn Hardware,
        time_used: i32,
        time_allocated: i32,
    ) -> String {
        let mut s = String::new();
        s.push_str("---------------------------\n");
        s.push_str(&self.phenotype.print_status(cfg));
        s.push('\n');
        if time_used == time_allocated {
            s.push_str("\n# TIMEOUT: No offspring produced.\n");
        } else if hardware.memory().size() == 0 {
            s.push_str("\n# ORGANISM DEATH: No offspring produced.\n");
        } else {
            s.push('\n');
            s.push_str("# Final Memory: ");
            s.push_str(&hardware.memory().as_string());
            s.push('\n');
            s.push_str("# Offspring Memory: ");
            // An organism that died (e.g. `die`) before any divide has no
            // offspring representation; the C++ dereferences the NULL
            // pointer here and crashes. The port prints the empty sequence.
            s.push_str(&self.offspring_genome.representation().as_string());
            s.push('\n');
        }
        s
    }
}

/// `cOrganism`: state plus hardware.
pub struct Organism {
    pub state: OrgState,
    pub hardware: Box<dyn Hardware>,
}

impl std::fmt::Debug for Organism {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Organism")
            .field("id", &self.state.id)
            .field("uid", &self.state.uid)
            .field("hw_type", &self.hardware.hw_type())
            .finish()
    }
}

impl Organism {
    /// `cOrganism::cOrganism(world, ctx, genome, parent_generation, src)`:
    /// the member initialisers, `m_id = stats.GetTotCreatures()`, the
    /// hardware (`cHardwareManager::Create`) and `initialize(ctx)`.
    ///
    /// Errors when the genome names an unknown instruction set or one of a
    /// different hardware type (the C++ asserts and returns NULL hardware).
    pub fn new(
        world: &mut dyn WorldServices,
        genome: &Genome,
        parent_generation: i32,
        src: OrgSource,
    ) -> Result<Organism, String> {
        let inst_set = world.hardware_manager().inst_set_for_genome(genome)?;
        let mut state = OrgState::new(world, genome, parent_generation, src, inst_set.num_nops());
        state.uid = world.new_org_uid();
        let hardware =
            world
                .hardware_manager()
                .create(world.config(), &mut state, inst_set.clone())?;
        let size = inst_set.size();
        let name = inst_set.inst_set_name().to_string();
        state.initialize(world, size, &name);
        Ok(Organism { state, hardware })
    }

    /// `SetOrgInterface(ctx, interface)`: the interface is supplied per
    /// call in the port, so attaching one only runs `HardwareReset(ctx)`.
    pub fn set_org_interface(&mut self, world: &mut dyn OrgInterface) {
        self.hardware_reset(world);
    }

    /// `HardwareReset(ctx)`.
    pub fn hardware_reset(&mut self, world: &mut dyn OrgInterface) {
        hardware_reset(&mut self.state, self.hardware.as_mut(), world);
    }

    /// `PrintStatus(fp)`.
    pub fn print_status(&self, world: &mut dyn OrgInterface) -> String {
        print_status(&self.state, self.hardware.as_ref(), world)
    }

    /// `PrintFinalStatus(fp, time_used, time_allocated)`.
    pub fn print_final_status(&self, cfg: &Config, time_used: i32, time_allocated: i32) -> String {
        self.state
            .print_final_status(cfg, self.hardware.as_ref(), time_used, time_allocated)
    }

    /// `NotifyDeath(ctx)`.
    pub fn notify_death(&mut self, world: &mut dyn OrgInterface) {
        self.state.notify_death(world);
    }

    /// `GetHardware().SingleProcess(ctx, speculative)`.
    pub fn single_process(
        &mut self,
        world: &mut dyn OrgInterface,
        speculative: bool,
    ) -> Result<bool, String> {
        self.hardware
            .single_process(&mut self.state, world, speculative)
    }

    /// `IsInterrupted()`.
    pub fn is_interrupted(&self) -> bool {
        is_interrupted(self.hardware.as_ref())
    }
}

/// `cOrganism::IsInterrupted()` given the hardware.
pub fn is_interrupted(hardware: &dyn Hardware) -> bool {
    (0..hardware.num_threads()).any(|k| hardware.thread_message_trigger_type(k) != -1)
}

/// `cOrganism::HardwareReset(ctx)`.
pub fn hardware_reset(
    org: &mut OrgState,
    hardware: &mut dyn Hardware,
    world: &mut dyn OrgInterface,
) {
    if world.environment().num_state_grids() > 0 {
        org.cur_sg = world.get_state_grid_id();
        let sg = world.environment().state_grid(org.cur_sg.max(0) as usize);
        let mut sg_state = vec![0; (3 + sg.num_states()).max(3) as usize];
        sg_state[0] = sg.initial_x();
        sg_state[1] = sg.initial_y();
        sg_state[2] = sg.initial_facing();
        hardware.setup_extended_memory(sg_state);
    }
    org.hardware_reset_state(world.config());
}

/// `cOrganism::PrintStatus(fp)`.
pub fn print_status(
    org: &OrgState,
    hardware: &dyn Hardware,
    world: &mut dyn OrgInterface,
) -> String {
    let mut s = String::new();
    s.push_str("---------------------------\n");
    s.push_str(&format!("U:{}\n", world.update()));
    if hardware.hw_type() != HARDWARE_TYPE_CPU_GP8 {
        s.push_str(&hardware.print_status(org, world));
    }
    s.push_str(&org.phenotype.print_status(world.config()));
    s.push('\n');

    s.push_str("Input (env):");
    for i in 0..org.input_buf.capacity() {
        let mut j = i;
        let v = world.get_input_at(&mut j);
        s.push_str(&format!(" 0x{:08x}", v as u32));
    }
    s.push('\n');

    s.push_str("Input (buf):");
    for i in 0..org.input_buf.num_stored() {
        s.push_str(&format!(" 0x{:08x}", org.input_buf.get(i) as u32));
    }
    s.push('\n');

    s.push_str("Output:     ");
    for i in 0..org.output_buf.num_stored() {
        s.push_str(&format!(" 0x{:08x}", org.output_buf.get(i) as u32));
    }
    s.push('\n');
    s
}

/// What the hardware must do after [`do_output`]: run each triggered
/// instruction as a bonus instruction (`ProcessBonusInst`).
#[derive(Debug, Default, Clone)]
pub struct OutputEffects {
    /// `insts_triggered` resolved in the organism's instruction set.
    pub bonus_insts: Vec<Instruction>,
}

/// `cOrganism::DoOutput(ctx, value)` (`value = None` is the
/// `DoOutput(ctx, on_divide)` form): add the value to the output buffer
/// and run `doOutput`, or `doAVOutput` with `USE_AVATARS`. `ext_mem` is the
/// hardware's extended memory.
///
/// The triggered bonus instructions are returned for the hardware to
/// execute (`m_hardware->ProcessBonusInst`), which needs the hardware the
/// caller holds.
#[allow(clippy::too_many_arguments)]
pub fn do_output(
    org: &mut OrgState,
    ext_mem: &[i32],
    inst_set: &InstSet,
    world: &mut dyn OrgInterface,
    value: Option<i32>,
    on_divide: bool,
    is_parasite: bool,
    context_phenotype: Option<&mut ContextPhenotype>,
) -> Result<OutputEffects, String> {
    if let Some(v) = value {
        org.output_buf.add(v);
    }
    let av = world.config().use_avatars != 0;
    do_output_inner(
        org,
        ext_mem,
        inst_set,
        world,
        on_divide,
        is_parasite,
        context_phenotype,
        av,
    )
}

/// `cOrganism::doOutput(ctx, input_buffer, output_buffer, on_divide,
/// is_parasite, context_phenotype)` on the organism's own buffers.
///
/// With `av` this is `cOrganism::doAVOutput`: "avatar output has to be
/// seperate from doOutput to ensure avatars, not the true orgs, are
/// triggering reactions". Neighbours are the avatars in the cells avatar 0
/// faces while it turns, resources are those of avatar 0's cell (no deme
/// resources), and the change goes back to that cell.
#[allow(clippy::too_many_arguments)]
fn do_output_inner(
    org: &mut OrgState,
    ext_mem: &[i32],
    inst_set: &InstSet,
    world: &mut dyn OrgInterface,
    on_divide: bool,
    is_parasite: bool,
    context_phenotype: Option<&mut ContextPhenotype>,
    av: bool,
) -> Result<OutputEffects, String> {
    // doOutput reads the resources first; doAVOutput after turning the
    // avatar (which can draw from the RNG).
    let mut pre_counts = None;
    if !av {
        let deme_id = world.get_deme_id();
        let global_resource_count = world.get_resources(org);
        let deme_resource_count = world.get_deme_resources(deme_id);
        pre_counts = Some((global_resource_count, deme_resource_count));
    }
    let cell_id_lists = world.get_cell_id_lists();

    let mut other_input_list: Vec<IoBuffer> = Vec::new();
    let mut other_output_list: Vec<IoBuffer> = Vec::new();
    for (wanted, list, input) in [
        (
            world.environment().use_neighbor_input(),
            &mut other_input_list,
            true,
        ),
        (
            world.environment().use_neighbor_output(),
            &mut other_output_list,
            false,
        ),
    ] {
        if !wanted {
            continue;
        }
        let pick = |o: &OrgState| {
            if input {
                o.input_buf.clone()
            } else {
                o.output_buf.clone()
            }
        };
        if av {
            let num_neighbors = world.get_av_num_neighbors(0);
            for _ in 0..num_neighbors {
                world.rotate(1);
                for uid in world.get_faced_avs(0) {
                    if uid == org.uid {
                        list.push(pick(org));
                        continue;
                    }
                    world.with_organism(uid, &mut |n: &mut Organism| list.push(pick(&n.state)));
                }
            }
        } else {
            let num_neighbors = world.get_num_neighbors();
            for _ in 0..num_neighbors {
                world.rotate(1);
                // `&cur_neighbor->m_input_buf`: the organism's own buffer
                // where it faces its own cell (unchanged until the tasks
                // read it).
                if let Some(buf) = with_neighbor_state(world, org, |n| pick(n)) {
                    list.push(buf);
                }
            }
        }
    }

    let (n_global, mut res_count, mut res_change) = match pre_counts {
        Some((global_resource_count, deme_resource_count)) => {
            let n_global = global_resource_count.len();
            // globalAndDeme_resource_count = global + deme (Apto::Array
            // operator+ concatenates).
            let mut res_count: Vec<f64> = global_resource_count;
            res_count.extend_from_slice(&deme_resource_count);
            let res_change = vec![0.0; res_count.len()];
            (n_global, res_count, res_change)
        }
        None => {
            let n_res = world.environment().resource_lib().size().max(0) as usize;
            let res_count = world.get_av_resources(org, 0);
            (res_count.len(), res_count, vec![0.0; n_res])
        }
    };

    let cell_id = if av {
        world.get_av_cell_id(0)
    } else {
        world.get_cell_id()
    };
    for (i, list) in cell_id_lists.iter().enumerate() {
        if !list.is_empty() && !list.contains(&cell_id) {
            if let Some(v) = res_count.get_mut(i) {
                *v = 0.0;
            }
        }
    }

    let save_received = world.config().save_received;
    let update = world.update();
    let mut snapshot = org.task_snapshot(update);
    let mut insts_triggered: Vec<String> = Vec::new();

    let task_completed;
    {
        let other_in: Vec<&IoBuffer> = other_input_list.iter().collect();
        let other_out: Vec<&IoBuffer> = other_output_list.iter().collect();
        let received = if save_received {
            Some(&org.received_messages)
        } else {
            None
        };
        let phen = &mut org.phenotype;
        let sizes = org.phenotype_sizes;
        let mut result = phen.take_reaction_result(&sizes);
        let rbins: Vec<f64> = phen.cur_rbins_avail.clone();
        let mut services = world.output_services();
        let resource_names: Vec<String> = services
            .environment
            .resource_lib()
            .resources()
            .iter()
            .map(|r| r.name().to_string())
            .collect();
        let mut ctx = TaskContext::new(
            &org.input_buf,
            &org.output_buf,
            &other_in,
            &other_out,
            ext_mem,
            on_divide,
            received,
        );
        ctx.task_states = Some(&mut phen.task_states);
        let mut evaluator = OrgTaskEvaluator {
            ctx,
            world: OrgTaskWorld {
                host: services.host,
                org: &mut snapshot,
                resource_change: None,
            },
            stolen_reaction_count: &phen.cur_stolen_reaction_count,
            first_reactions: Vec::new(),
            resource_names,
            config: services.config,
            stats: &mut *services.stats,
            plasticity: services.plasticity.take(),
        };
        let mut context_phenotype = context_phenotype;
        let found = services.environment.test_output(
            services.rng,
            services.test_mode,
            services.config,
            &mut result,
            &mut evaluator,
            &phen.eff_task_count,
            &mut phen.cur_reaction_count,
            &res_count,
            &rbins,
            is_parasite,
            context_phenotype.as_deref_mut(),
        );
        let first_reactions = std::mem::take(&mut evaluator.first_reactions);
        drop(evaluator);
        for r in first_reactions {
            phen.set_first_reaction_cycle(r.max(0) as usize);
            phen.set_first_reaction_exec(r.max(0) as usize);
        }
        task_completed = phen.apply_test_output(
            services.config,
            &sizes,
            services.update,
            org.id,
            found,
            result,
            &mut res_change,
            &mut insts_triggered,
            is_parasite,
            context_phenotype,
            services.stats,
            services.deme,
        );
    }
    org.apply_task_snapshot(snapshot, world)?;

    if world.config().merit_inc_apply_immediate {
        let cur_merit = org.phenotype.calc_current_merit(world.config());
        if org.phenotype.merit.get_double() < cur_merit {
            world.update_merit(org, cur_merit);
        }
    }

    let global_res_change: Vec<f64> = res_change[..n_global.min(res_change.len())].to_vec();
    let deme_res_change: Vec<f64> = res_change[n_global.min(res_change.len())..].to_vec();

    let energy_enabled = world.config().energy_enabled;
    let apply_energy_method = world.config().apply_energy_method;
    if energy_enabled && apply_energy_method == 1 && task_completed {
        apply_stored_energy_merit(org, world, "doOutput")?;
        if org.phenotype.merit.get_double() == 0.0 {
            org.phenotype.to_die = true;
        }
    }
    if av {
        world.update_av_resources(org, &res_change, 0);
    } else {
        if org.phenotype.make_random_resource {
            world.update_random_resources(org, &global_res_change);
        } else {
            world.update_resources(org, &global_res_change);
        }
        world.update_deme_resources(&deme_res_change);
    }

    let mut effects = OutputEffects::default();
    for name in &insts_triggered {
        effects.bonus_insts.push(inst_set.get_inst(name)?);
    }
    Ok(effects)
}

/// `RefreshEnergy(); ApplyToEnergyStore();` and then
/// `UpdateMerit(ctx, ConvertEnergyToMerit(GetStoredEnergy() *
/// GetEnergyUsageRatio()))`, as `cOrganism::doOutput` (`APPLY_ENERGY_METHOD`
/// 1, after a completed task) and `cHardwareCPU::Inst_Sleep` (method 2) do.
/// A merit of 0 makes `cPopulation::UpdateMerit` kill the organism and then
/// dereference its emptied cell; that is the `Err`.
pub fn apply_stored_energy_merit(
    org: &mut OrgState,
    world: &mut dyn OrgInterface,
    caller: &str,
) -> Result<(), String> {
    {
        let svc = world.output_services();
        org.phenotype.refresh_energy(svc.config, svc.stats)?;
        org.phenotype.apply_to_energy_store(svc.config, svc.stats);
    }
    let p = &org.phenotype;
    let new_merit = p.convert_energy_to_merit(world.config(), p.energy_store * p.execution_ratio);
    if new_merit <= 0.0 {
        return Err(format!(
            "unsupported: {caller} converting the stored energy to a merit of 0 \
             (C++: cPopulation::UpdateMerit kills the organism, then dereferences its emptied cell)"
        ));
    }
    world.update_merit(org, new_merit);
    Ok(())
}

/// `%g` of a double, as `ostream <<` prints it (used by status printers).
pub fn ostream_double(v: f64) -> String {
    fmt_g(v, 6)
}
