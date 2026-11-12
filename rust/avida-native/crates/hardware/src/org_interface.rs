//! Port of `cOrgInterface` (avida-core/source/main/cOrgInterface.h) and of
//! the world services the hardware and organism layers reach through
//! `m_world->` and `cAvidaContext`.
//!
//! Translation rules (see ARCHITECTURE.md):
//!
//! * every `cOrgInterface` virtual is a method here, in snake_case, with
//!   plain data in and out. Pointer results become ids (`Option<i32>`
//!   for a possibly-NULL organism or cell) or closures
//!   ([`OrgInterface::with_neighbor`], [`OrgInterface::with_organism`]);
//! * `cOrganism*` arguments that always denote the organism the interface
//!   belongs to become `&mut OrgState` / `&OrgState` (the running organism
//!   is borrowed separately from the world);
//! * `cAvidaContext&` arguments are dropped: the RNG is
//!   [`WorldServices::rng`], the test/analyze/fault flags are methods;
//! * `const Apto::Array<double>&` results are returned by value.
//!
//! Pointer accessors with no plain-data equivalent are not methods:
//! `GetCell()`, `GetCell(int)`, `GetDeme()` and `GetResourceCount()`
//! (their uses in the hardware go through the dedicated methods below,
//! e.g. [`OrgInterface::deme_inc_num_switching_penalties`]).
//!
//! No method has a default body: [`crate::test_cpu::TestCpuInterface`]
//! spells out the `cTestCPUInterface` behaviour of each.

use crate::hardware::Hardware;
use crate::hardware_manager::HardwareManager;
use crate::hardware_transsmt::ParasiteUnit;
use crate::organism::{OrgState, OrgUid, Organism};
use crate::task_adapter::{Env, OutputHost};
use avida_apto::AvidaRng;
use avida_config::config::Config;
use avida_genome::{Genome, InstructionSequence};
use avida_phenotype::{PhenotypeDeme, PhenotypeStats};
use std::sync::Arc;

/// `cOrgMessage` (avida-core/source/main/cOrgMessage.h): the payload of
/// the messaging instructions.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct OrgMessage {
    /// `m_data`
    pub data: u32,
    /// `m_label`
    pub label: u32,
    /// Sender's cell id (`GetSenderCellID()`).
    pub sender_cell: i32,
    /// Receiver's cell id (`GetReceiverCellID()`), -1 until delivered.
    pub receiver_cell: i32,
    /// Sender's organism id.
    pub sender_org_id: i32,
    /// `m_messageType`
    pub message_type: i32,
    /// `m_transCellID` (`GetTransCellID()`): -1, or the avatar cell the
    /// message last went to under `NEURAL_NETWORKING` with `USE_AVATARS 2`
    /// (`cPopulationInterface::SendMessage(msg, cellid)`).
    pub trans_cell: i32,
}

/// A delivery the population made to the organism that is running, i.e.
/// to the sender itself (a cell connected to itself, `ALARM_SELF`). The
/// world cannot reach that organism while it executes, so it queues the
/// delivery and the hardware applies it as soon as the interface call
/// that made it returns ([`OrgInterface::take_self_deliveries`],
/// `hardware_cpu_insts_msg::deliver_to_self`); C++ applies it inside that
/// call, before anything else the instruction does.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SelfDelivery {
    /// `cOrganism::ReceiveMessage(msg)`
    Message(OrgMessage),
    /// `cOrganism::ReceiveFlash()`
    Flash,
    /// `cOrganism::moveIPtoAlarmLabel(jump_label)`
    Alarm(i32),
}

/// What `GetNeighbor()` hands out ([`OrgInterface::with_neighbor`]): the
/// organism in the faced cell.
///
/// In a torus one cell wide or one cell high every cell is connected to
/// itself, so an organism can face its own cell. The C++ then hands out
/// the running organism itself as its neighbour, and an instruction that
/// acts on the faced organism acts on itself (donor and recipient are one
/// organism, every read sees the writes made before it). The port keeps
/// the running organism out of the population while it executes, so the
/// accessor cannot lend it: it answers [`Faced::SelfOrg`], and the caller
/// operates on the `org` it holds, in the C++ order of operations
/// ([`with_neighbor_state`] does this for state-only access).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Faced {
    /// The faced cell is empty (C++ NULL); the closure was not called.
    Empty,
    /// The faced cell holds the running organism itself; the closure was
    /// not called.
    SelfOrg,
    /// Another organism; the closure ran on it.
    Other,
}

impl Faced {
    /// `GetNeighbor() != NULL`
    pub fn is_some(self) -> bool {
        self != Faced::Empty
    }
}

/// `GetNeighbor()` followed by reads or writes of its state: `f` runs on
/// the state of the faced organism, which is `org` (the running organism)
/// when it faces its own cell. `None` when the faced cell is empty.
pub fn with_neighbor_state<T>(
    world: &mut dyn OrgInterface,
    org: &mut OrgState,
    f: impl FnOnce(&mut OrgState) -> T,
) -> Option<T> {
    let mut f = Some(f);
    let mut out = None;
    let faced = world.with_neighbor(&mut |o| {
        if let Some(f) = f.take() {
            out = Some(f(&mut o.state));
        }
    });
    match faced {
        Faced::Empty => None,
        Faced::Other => out,
        Faced::SelfOrg => f.map(|f| f(org)),
    }
}

/// Result of running an offspring genome through a test CPU for
/// `cHardwareBase::Divide_TestFitnessMeasures`.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct OffspringTest {
    /// `test_info.GetGenotypeFitness()`
    pub genotype_fitness: f64,
    /// `test_info.GetMaxDepth()`
    pub max_depth: i32,
    /// `test_info.IsViable()`
    pub is_viable: bool,
    /// `test_info.GetTestPhenotype().GetLastTaskCount()`
    pub last_task_count: Vec<i32>,
}

/// What `cOrgSensor` reads of an organism it sights
/// ([`OrgInterface::seen_org`]).
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct SeenOrg {
    /// `GetID()`
    pub id: i32,
    /// The organism itself (the `cOrganism*`).
    pub uid: OrgUid,
    /// `GetOrgInterface().GetCellID()`
    pub cell_id: i32,
    /// `GetOrgInterface().GetAVCellID()`
    pub av_cell_id: i32,
    /// `GetOrgInterface().GetFacedDir()`
    pub faced_dir: i32,
    /// `GetOrgInterface().GetAVFacing()`
    pub av_facing: i32,
    /// `GetPhenotype().GetCurBonus()`
    pub cur_bonus: f64,
    /// `GetOpinion().first` when `HasOpinion()`
    pub opinion: Option<i32>,
    /// `GetForageTarget()`
    pub forage_target: i32,
    /// `GetShowForageTarget()`
    pub show_ft: i32,
    /// `IsDead()`
    pub is_dead: bool,
    /// `IsDisplaying()`
    pub is_displaying: bool,
    /// `GetOrgDisplayData()`
    pub display: Option<crate::org_sensor::OrgDisplay>,
}

/// The world state `cOrganism::doOutput` hands to `cPhenotype::TestOutput`
/// at one time, borrowed disjointly from the world: the environment and
/// RNG are mutated while the task evaluator consults the host.
pub struct OutputServices<'a> {
    /// `m_world->GetConfig()`
    pub config: &'a Config,
    /// `m_world->GetEnvironment()` (mutable because `xor-max` may repoint a
    /// reaction's resource during evaluation).
    pub environment: &'a mut Env,
    /// `ctx.GetRandom()`
    pub rng: &'a mut AvidaRng,
    /// `ctx.GetTestMode()`
    pub test_mode: bool,
    /// `m_world->GetStats().GetUpdate()`
    pub update: i32,
    /// `m_world->GetStats()` as `cPhenotype` uses it.
    pub stats: &'a mut dyn PhenotypeStats,
    /// The organism's deme, if any.
    pub deme: Option<&'a mut dyn PhenotypeDeme>,
    /// World side of `cTaskContext` (cell, deme, population, stats).
    pub host: &'a mut dyn OutputHost,
}

/// World services needed to build an organism (`cOrganism::cOrganism`) and
/// its hardware, before any interface is attached.
pub trait WorldServices {
    /// `m_world->GetConfig()`
    fn config(&self) -> &Config;
    /// `m_world->GetEnvironment()`
    fn environment(&self) -> &Env;
    /// `ctx.GetRandom()` (one RNG per world).
    fn rng(&mut self) -> &mut AvidaRng;
    /// `m_world->GetHardwareManager()`
    fn hardware_manager(&self) -> &HardwareManager;
    /// `m_world->GetStats().GetUpdate()`
    fn update(&self) -> i32;
    /// `m_world->GetStats().GetTotCreatures()` (the next organism id).
    fn tot_creatures(&self) -> i32;
    /// `m_world->GetStats().GetSenseSize()`
    fn sense_size(&self) -> usize;
    /// `m_world->GetStats().GetGroupAttackInsts(inst_set).GetSize()`
    fn group_attack_inst_count(&self, inst_set: &str) -> usize;
    /// `ctx.GetTestMode()`
    fn test_mode(&self) -> bool;
    /// `ctx.GetAnalyzeMode()`
    fn analyze_mode(&self) -> bool;
    /// `ctx.OrgFaultReporting()`
    fn org_fault_reporting(&self) -> bool;
    /// `m_world->GetVerbosity()`
    fn verbosity(&self) -> i32;
    /// A fresh [`OrgUid`] for an organism being built (`new cOrganism`).
    /// Worlds without a population (the test CPU) leave it
    /// [`OrgUid::NONE`]: nothing there looks organisms up.
    fn new_org_uid(&mut self) -> OrgUid {
        OrgUid::NONE
    }
}

/// `cOrgInterface` plus the world services hardware instructions use.
pub trait OrgInterface: WorldServices {
    // ------------------------------------------------------------------
    // World services beyond construction
    // ------------------------------------------------------------------

    /// Split borrow for `cOrganism::doOutput` (see [`OutputServices`]).
    fn output_services(&mut self) -> OutputServices<'_>;
    /// `m_world->GetTestSterilize()`
    fn test_sterilize(&self) -> bool;
    /// `cOrganism::GetTestFitness(ctx)`: the test-CPU fitness of the
    /// organism's genotype (`Systematics::GenomeTestMetrics`).
    fn genotype_test_fitness(&mut self, org: &OrgState) -> f64;
    /// `m_organism->SystematicsGroup("genotype")` is set (false for a test
    /// CPU organism, which is never classified).
    fn has_genotype(&mut self) -> bool;
    /// `SexualAncestry::GetPhyloDistance`: from this organism's genotype to
    /// that of the organism in `cell` (0 same genotype ... 5 unrelated).
    /// `Err` where the C++ dereferences a missing group.
    fn phylo_distance_to(&mut self, cell: i32) -> Result<i32, String>;
    /// `m_world->GetHardwareManager().CreateTestCPU(ctx)->TestGenome(ctx,
    /// test_info, genome)` with random inputs, as
    /// `Divide_TestFitnessMeasures` runs it for `parent`'s offspring.
    fn test_offspring(&mut self, parent: &OrgState, genome: &Genome) -> OffspringTest;
    /// `m_world->GetStats().IncResamplings()`
    fn stats_inc_resamplings(&mut self);
    /// `m_world->GetStats().IncFailedResamplings()`
    fn stats_inc_failed_resamplings(&mut self);
    /// `m_world->GetStats().AddNewTaskCount(task)`
    fn stats_add_new_task_count(&mut self, task: i32);
    /// `GetDeme()->IncNumSwitchingPenalties(n)` when the organism has a deme.
    fn deme_inc_num_switching_penalties(&mut self, n: i32);
    /// `GetDeme()->DecSleepingCount()` when the organism has a deme.
    fn deme_dec_sleeping_count(&mut self);
    /// `GetDeme()->IncSleepingCount()` when the organism has a deme.
    fn deme_inc_sleeping_count(&mut self);
    /// The living organism `uid` other than the running one (a held
    /// `cOrganism*`). Returns false when absent.
    fn with_organism(&mut self, uid: OrgUid, f: &mut dyn FnMut(&mut Organism)) -> bool;
    /// The organism in cell `cell_id` other than the running one
    /// (`GetCell(cell_id).GetOrganism()`). Returns false when there is none.
    /// Organism ids are not unique (the two offspring of a sexual birth are
    /// built before either is counted, so they share one), so a held
    /// `cOrganism*` is better kept as a cell.
    fn with_organism_in_cell(&mut self, cell_id: i32, f: &mut dyn FnMut(&mut Organism)) -> bool;

    // ------------------------------------------------------------------
    // cOrgInterface: location
    // ------------------------------------------------------------------

    /// `GetLiveOrgList()` as organism ids.
    fn live_org_ids(&self) -> Vec<i32>;
    /// `GetLiveOrgList()` as the organisms themselves (the running one
    /// included while it lives).
    fn live_org_uids(&self) -> Vec<OrgUid> {
        Vec::new()
    }
    /// `m_world->GetPopulation().GetLiveOrgList()` with each organism's cell
    /// and `GetForageTarget()`: `None` for the running organism, whose state
    /// the caller holds. `Err` where the population cannot be walked (a
    /// test CPU inside a population run).
    fn live_org_forage_targets(&self) -> Result<Vec<(OrgUid, i32, Option<i32>)>, String>;
    /// The first organism of `GetLiveOrgList()` whose `GetID()` is `id`
    /// (the C++ id searches stop at the first match; siblings of a sexual
    /// birth share an id).
    fn find_live_org(&self, id: i32) -> Option<OrgUid>;
    /// `GetCellID()`
    fn get_cell_id(&self) -> i32;
    /// `GetDemeID()`
    fn get_deme_id(&self) -> i32;
    /// `SetCellID(id)`
    fn set_cell_id(&mut self, id: i32);
    /// `SetDemeID(id)`
    fn set_deme_id(&mut self, id: i32);
    /// `GetCellXPosition()`
    fn get_cell_x_position(&self) -> i32;
    /// `GetCellYPosition()`
    fn get_cell_y_position(&self) -> i32;

    // ---- cell data ----
    /// `GetCellData()`
    fn get_cell_data(&self) -> i32;
    /// `GetCellDataOrgID()`
    fn get_cell_data_org_id(&self) -> i32;
    /// `GetCellDataUpdate()`
    fn get_cell_data_update(&self) -> i32;
    /// `GetCellDataTerritory()`
    fn get_cell_data_territory(&self) -> i32;
    /// `GetCellDataForagerType()`
    fn get_cell_data_forager_type(&self) -> i32;
    /// `SetCellData(data)` from the running organism `org`:
    /// `cPopulationCell::SetCellData` also records the opinion and forage
    /// target of the cell's occupant, `org` itself.
    fn set_cell_data(&mut self, org: &mut OrgState, data: i32);
    /// `GetFacedCellData()`
    fn get_faced_cell_data(&self) -> i32;
    /// `GetFacedCellDataOrgID()`
    fn get_faced_cell_data_org_id(&self) -> i32;
    /// `GetFacedCellDataUpdate()`
    fn get_faced_cell_data_update(&self) -> i32;
    /// `GetFacedCellDataTerritory()`
    fn get_faced_cell_data_territory(&self) -> i32;

    // ---- navigation bookkeeping ----
    /// `GetPrevSeenCellID()`
    fn get_prev_seen_cell_id(&self) -> i32;
    /// `GetPrevTaskCellID()`
    fn get_prev_task_cell_id(&self) -> i32;
    /// `GetNumTaskCellsReached()`
    fn get_num_task_cells_reached(&self) -> i32;
    /// `AddReachedTaskCell()`
    fn add_reached_task_cell(&mut self);
    /// `SetPrevSeenCellID(id)`
    fn set_prev_seen_cell_id(&mut self, id: i32);
    /// `SetPrevTaskCellID(id)`
    fn set_prev_task_cell_id(&mut self, id: i32);

    // ------------------------------------------------------------------
    // cOrgInterface: reproduction and neighbourhood
    // ------------------------------------------------------------------

    /// `Divide(ctx, parent, offspring_genome)`: activate the offspring.
    /// Returns whether the parent is still alive.
    fn divide(&mut self, parent: &mut OrgState, offspring_genome: &Genome) -> bool;
    /// [`OrgInterface::divide`] with the parent's hardware traces lent to
    /// the population for the duration (`ActivateOffspring` acts on them:
    /// `UpdateQs`, `TestForMiniTrace`; see [`crate::mini_trace`]), and the
    /// rest of the parent's hardware readable as `parent_hw`
    /// (`parent_organism->GetHardware()`: `InheritState`, `GetMemory`).
    fn divide_with_traces(
        &mut self,
        parent: &mut OrgState,
        parent_hw: &dyn Hardware,
        offspring_genome: &Genome,
        traces: &mut crate::mini_trace::LentTraces,
    ) -> bool;
    /// `GetNeighbor()`: run `f` on the faced organism and answer
    /// [`Faced::Other`]; [`Faced::Empty`] when the faced cell is empty (C++
    /// NULL) and [`Faced::SelfOrg`] when it holds the running organism
    /// itself (a cell connected to itself), in both cases without calling
    /// `f`.
    fn with_neighbor(&mut self, f: &mut dyn FnMut(&mut Organism)) -> Faced;
    /// `IsNeighborCellOccupied()`
    fn is_neighbor_cell_occupied(&self) -> bool;
    /// `GetNumNeighbors()`
    fn get_num_neighbors(&self) -> i32;
    /// `GetNeighborhoodCellIDs(list)`
    fn get_neighborhood_cell_ids(&self) -> Vec<i32>;
    /// `GetAVNeighborhoodCellIDs(list, av_num)`
    fn get_av_neighborhood_cell_ids(&self, av_num: i32) -> Vec<i32>;
    /// `GetFacing()`
    fn get_facing(&self) -> i32;
    /// `GetFacedCellID()`
    fn get_faced_cell_id(&self) -> i32;
    /// `GetFacedDir()`
    fn get_faced_dir(&self) -> i32;
    /// `GetNeighborCellContents()`
    fn get_neighbor_cell_contents(&self) -> i32;
    /// `Rotate(ctx, direction)`
    fn rotate(&mut self, direction: i32);
    /// `GetLGTFragment(ctx, region, dest_genome, seq)`: the fragment, or
    /// `None` when selection fails.
    fn get_lgt_fragment(
        &mut self,
        region: i32,
        dest_genome: &Genome,
    ) -> Option<InstructionSequence>;

    // ------------------------------------------------------------------
    // cOrgInterface: inputs and resources
    // ------------------------------------------------------------------

    /// `GetInputAt(input_pointer)`: advances the pointer.
    fn get_input_at(&mut self, input_pointer: &mut i32) -> i32;
    /// `ResetInputs(ctx)`
    fn reset_inputs(&mut self);
    /// `GetInputs()`
    fn get_inputs(&self) -> Vec<i32>;
    // The reads and writes of the population's resource count below take
    // the running organism `org`: they bring the resources up to date
    // (`cResourceCount::DoUpdates`), and a damaging gradient updated there
    // injures whoever stands in its plateau, the running organism included
    // (`cPopulation::InjureOrg` lowers its merit, bonus and resource bins in
    // the middle of the instruction).
    /// `GetResources(ctx)`
    fn get_resources(&mut self, org: &mut OrgState) -> Vec<f64>;
    /// `GetResourceVal(ctx, res_id)`
    fn get_resource_val(&mut self, org: &mut OrgState, res_id: i32) -> f64;
    /// `GetFacedCellResources(ctx)`
    fn get_faced_cell_resources(&mut self, org: &mut OrgState) -> Vec<f64>;
    /// `GetFacedResourceVal(ctx, res_id)`
    fn get_faced_resource_val(&mut self, org: &mut OrgState, res_id: i32) -> f64;
    /// `GetDemeResources(deme_id, ctx)`
    fn get_deme_resources(&mut self, deme_id: i32) -> Vec<f64>;
    /// `GetCellResources(cell_id, ctx)`
    fn get_cell_resources(&mut self, org: &mut OrgState, cell_id: i32) -> Vec<f64>;
    /// `GetFrozenCellResVal(ctx, cell_id, res_id)`
    fn get_frozen_cell_res_val(&mut self, cell_id: i32, res_id: i32) -> f64;
    /// `GetCellResVal(ctx, cell_id, res_id)`
    fn get_cell_res_val(&mut self, org: &mut OrgState, cell_id: i32, res_id: i32) -> f64;
    /// `GetFrozenResources(ctx, cell_id)`
    fn get_frozen_resources(&mut self, cell_id: i32) -> Vec<f64>;
    /// `GetCellIdLists()`
    fn get_cell_id_lists(&self) -> Vec<Vec<i32>>;
    /// `GetCurrPeakX(ctx, res_id)`
    fn get_curr_peak_x(&mut self, org: &mut OrgState, res_id: i32) -> i32;
    /// `GetCurrPeakY(ctx, res_id)`
    fn get_curr_peak_y(&mut self, org: &mut OrgState, res_id: i32) -> i32;
    /// `GetFrozenPeakX(ctx, res_id)`
    fn get_frozen_peak_x(&mut self, res_id: i32) -> i32;
    /// `GetFrozenPeakY(ctx, res_id)`
    fn get_frozen_peak_y(&mut self, res_id: i32) -> i32;
    /// `TriggerDoUpdates(ctx)`
    fn trigger_do_updates(&mut self, org: &mut OrgState);
    /// `UpdateResources(ctx, res_change)`
    fn update_resources(&mut self, org: &mut OrgState, res_change: &[f64]);
    /// `UpdateRandomResources(ctx, res_change)`
    fn update_random_resources(&mut self, res_change: &[f64]);
    /// `UpdateDemeResources(ctx, res_change)`
    fn update_deme_resources(&mut self, res_change: &[f64]);

    // ------------------------------------------------------------------
    // cOrgInterface: death, merit, messaging
    // ------------------------------------------------------------------

    /// `Die(ctx)`: remove the organism from its cell.
    fn die(&mut self, org: &mut OrgState);
    /// `KillCellID(target, ctx)`. `org` is the running organism, which
    /// `target` may hold (a cell that is its own neighbour): its kill sets
    /// `SetToDelete()` on that state at once, as the C++ does.
    fn kill_cell_id(&mut self, org: &mut OrgState, target: i32);
    /// `Kaboom(distance, ctx)`
    fn kaboom(&mut self, org: &mut OrgState, distance: i32);
    /// `Kaboom(distance, ctx, effect)`
    fn kaboom_effect(&mut self, org: &mut OrgState, distance: i32, effect: f64);
    /// `SpawnDeme(ctx)`. `org` is the organism the interface belongs to
    /// (the spawned clone may be copied from it).
    fn spawn_deme(&mut self, org: &mut OrgState);
    /// The deme loop of `Inst_ApplyPointMutationsGroupRandom` /
    /// `Inst_ApplyPointMutationsGroupGS`: from a random position of deme
    /// `deme_id`, the first occupied cell whose organism passes `germline`
    /// (`IsGermline()` equal to it, when given) gets
    /// `GetHardware().PointMutate(ctx, prob)` and `IncPointMutations`.
    /// `org` / `hw` are the running organism (it may pick itself).
    fn point_mutate_deme_member(
        &mut self,
        org: &mut OrgState,
        hw: &mut dyn Hardware,
        deme_id: i32,
        prob: f64,
        germline: Option<bool>,
    );
    /// `std::cout << text` from inside an organism's step (population runs
    /// order it before the update's status line).
    fn cout(&mut self, text: &str);
    /// `ReceiveValue()`: the first sent value among the neighbours, turning
    /// to each; `org` is the running organism, a neighbour of itself when
    /// its cell is connected to itself.
    fn receive_value(&mut self, org: &mut OrgState) -> i32;
    /// `InjectParasite(host, parent, label, injected_code)`. `host_hw` is
    /// the host's hardware (the target may be the host itself); `parent`
    /// is `ThreadGetOwner()`, `None` when that is the host.
    fn inject_parasite(
        &mut self,
        host: &mut OrgState,
        host_hw: &mut dyn Hardware,
        parent: Option<Arc<ParasiteUnit>>,
        label: &str,
        injected_code: &InstructionSequence,
    ) -> bool;
    /// `UpdateMerit(ctx, new_merit)` for this organism.
    fn update_merit(&mut self, org: &mut OrgState, new_merit: f64) -> bool;
    /// `TestOnDivide()`
    fn test_on_divide(&self) -> bool;
    /// `SendMessage(msg)`
    fn send_message(&mut self, msg: &mut OrgMessage) -> bool;
    /// `BroadcastMessage(msg, depth)`
    fn broadcast_message(&mut self, msg: &mut OrgMessage, depth: i32) -> bool;
    /// `BcastAlarm(jump_label, bcast_range)`
    fn bcast_alarm(&mut self, jump_label: i32, bcast_range: i32) -> bool;
    /// `DivideOrgTestamentAmongDeme(value)`
    fn divide_org_testament_among_deme(&mut self, value: f64);
    /// `SendFlash()`
    fn send_flash(&mut self);
    /// The deliveries the last messaging call (`send_message`,
    /// `broadcast_message`, `bcast_alarm`, `send_flash`) made to the
    /// running organism itself, in order (see [`SelfDelivery`]). Only a
    /// population makes any.
    fn take_self_deliveries(&mut self) -> Vec<SelfDelivery> {
        Vec::new()
    }
    /// `GetStateGridID(ctx)`
    fn get_state_grid_id(&mut self) -> i32;
    /// `RotateToGreatestReputation()`. `org` is the organism the interface
    /// belongs to (it may be its own neighbour on a narrow torus).
    fn rotate_to_greatest_reputation(&mut self, org: &mut OrgState);
    /// `RotateToGreatestReputationWithDifferentTag(tag)`
    fn rotate_to_greatest_reputation_with_different_tag(&mut self, org: &mut OrgState, tag: i32);
    /// `RotateToGreatestReputationWithDifferentLineage(line)`
    fn rotate_to_greatest_reputation_with_different_lineage(
        &mut self,
        org: &mut OrgState,
        line: i32,
    );

    // ---- network ----
    /// `CreateLinkByFacing(weight)`
    fn create_link_by_facing(&mut self, weight: f64);
    /// `CreateLinkByXY(x, y, weight)`
    fn create_link_by_xy(&mut self, x: i32, y: i32, weight: f64);
    /// `CreateLinkByIndex(idx, weight)`
    fn create_link_by_index(&mut self, idx: i32, weight: f64);
    /// `NetworkBroadcast(msg)`
    fn network_broadcast(&mut self, msg: &mut OrgMessage) -> bool;
    /// `NetworkUnicast(msg)`
    fn network_unicast(&mut self, msg: &mut OrgMessage) -> bool;
    /// `NetworkRotate(x)`
    fn network_rotate(&mut self, x: i32) -> bool;
    /// `NetworkSelect(x)`
    fn network_select(&mut self, x: i32) -> bool;

    // ---- deme IO ----
    /// `GetNextDemeInput(ctx)`
    fn get_next_deme_input(&mut self) -> i32;
    /// `DoDemeInput(value)`
    fn do_deme_input(&mut self, value: i32);
    /// `DoDemeOutput(ctx, value)`
    fn do_deme_output(&mut self, value: i32);

    // ---- HGT ----
    /// `DoHGTDonation(ctx)`
    fn do_hgt_donation(&mut self, org: &mut OrgState);
    /// `DoHGTConjugation(ctx)`
    fn do_hgt_conjugation(&mut self, org: &mut OrgState);
    /// `DoHGTMutation(ctx, offspring)` of the dividing `org`, whose
    /// `offspring_genome` is the offspring. `org` is also where the world
    /// reads the parent's own genome (`HGT_SOURCE` 1, or conjugation from a
    /// cell that is its own neighbour).
    fn do_hgt_mutation(&mut self, org: &mut OrgState);
    /// `ReceiveHGTDonation(fragment)`
    fn receive_hgt_donation(&mut self, fragment: &InstructionSequence);

    // ---- movement and live-org bookkeeping ----
    /// `Move(ctx, src_id, dest_id)`
    fn move_org(&mut self, org: &mut OrgState, src_id: i32, dest_id: i32) -> bool;
    /// `AddLiveOrg()`
    fn add_live_org(&mut self);
    /// `RemoveLiveOrg()`
    fn remove_live_org(&mut self);

    // ---- opinions ----
    /// `HasOpinion(org)`
    fn has_opinion(&self, org: &OrgState) -> bool;
    /// `SetOpinion(opinion, org)`
    fn set_opinion(&mut self, opinion: i32, org: &mut OrgState);
    /// `ClearOpinion(org)`
    fn clear_opinion(&mut self, org: &mut OrgState);

    // ---- groups ----
    // `org` (or `parent`) below is the calling organism, which is out of
    // its cell while it runs: the population reads its mating type and
    // tolerances, and it may be a member of the group being summed.
    /// `JoinGroup(group_id)`
    fn join_group(&mut self, org: &mut OrgState, group_id: i32);
    /// `MakeGroup()`
    fn make_group(&mut self, org: &mut OrgState);
    /// `LeaveGroup(group_id)`
    fn leave_group(&mut self, org: &mut OrgState, group_id: i32);
    /// `NumberOfOrganismsInGroup(group_id)`
    fn number_of_organisms_in_group(&mut self, group_id: i32) -> i32;
    /// `NumberGroupFemales(group_id)`
    fn number_group_females(&mut self, group_id: i32) -> i32;
    /// `NumberGroupMales(group_id)`
    fn number_group_males(&mut self, group_id: i32) -> i32;
    /// `NumberGroupJuvs(group_id)`
    fn number_group_juvs(&mut self, group_id: i32) -> i32;
    /// `ChangeGroupMatingTypes(org, group_id, old_type, new_type)`
    fn change_group_mating_types(
        &mut self,
        org: &mut OrgState,
        group_id: i32,
        old_type: i32,
        new_type: i32,
    );

    // ---- tolerance ----
    /// `IncTolerance(tolerance_type, ctx)`
    fn inc_tolerance(&mut self, org: &mut OrgState, tolerance_type: i32) -> i32;
    /// `DecTolerance(tolerance_type, ctx)`
    fn dec_tolerance(&mut self, org: &mut OrgState, tolerance_type: i32) -> i32;
    /// `CalcGroupToleranceImmigrants(target_group_id, mating_type)`
    fn calc_group_tolerance_immigrants(
        &mut self,
        org: &mut OrgState,
        target_group_id: i32,
        mating_type: i32,
    ) -> i32;
    /// `CalcGroupToleranceOffspring(parent)`
    fn calc_group_tolerance_offspring(&mut self, parent: &mut OrgState) -> i32;
    /// `CalcGroupOddsImmigrants(group_id, mating_type)`
    fn calc_group_odds_immigrants(
        &mut self,
        org: &mut OrgState,
        group_id: i32,
        mating_type: i32,
    ) -> f64;
    /// `CalcGroupOddsOffspring(parent)`
    fn calc_group_odds_offspring_parent(&mut self, parent: &mut OrgState) -> f64;
    /// `CalcGroupOddsOffspring(group_id)`
    fn calc_group_odds_offspring(&mut self, org: &mut OrgState, group_id: i32) -> f64;
    /// `AttemptImmigrateGroup(ctx, group_id, org)`
    fn attempt_immigrate_group(&mut self, group_id: i32, org: &mut OrgState) -> bool;
    /// `PushToleranceInstExe(tol_inst, ctx)`
    fn push_tolerance_inst_exe(&mut self, org: &mut OrgState, tol_inst: i32);
    /// `GetGroupIntolerances(group_id, tol_num, mating_type)` (read).
    fn get_group_intolerances(&mut self, group_id: i32, tol_num: i32, mating_type: i32) -> i32;
    /// `GetGroupIntolerances(...) = value` (the C++ returns `int&`).
    fn set_group_intolerances(&mut self, group_id: i32, tol_num: i32, mating_type: i32, value: i32);

    // ---- predator/prey ----
    /// `TryWriteGroupAttackBits(raw_bits)`
    fn try_write_group_attack_bits(&mut self, raw_bits: u8);
    /// `TryWriteGroupAttackString(string)`
    fn try_write_group_attack_string(&mut self, s: &str);
    /// `DecNumPreyOrganisms()`
    fn dec_num_prey_organisms(&mut self);
    /// `DecNumPredOrganisms()`
    fn dec_num_pred_organisms(&mut self);
    /// `DecNumTopPredOrganisms()`
    fn dec_num_top_pred_organisms(&mut self);
    /// `IncNumPreyOrganisms()`
    fn inc_num_prey_organisms(&mut self);
    /// `IncNumPredOrganisms()`
    fn inc_num_pred_organisms(&mut self);
    /// `IncNumTopPredOrganisms()`
    fn inc_num_top_pred_organisms(&mut self);
    /// `AttackFacedOrg(ctx, loser)`: `KillOrganism` of the cell `loser`,
    /// which may hold the running organism `org` (see
    /// [`OrgInterface::kill_cell_id`]).
    fn attack_faced_org(&mut self, org: &mut OrgState, loser: i32);
    /// `TryWriteBirthLocData(org->GetOrgIndex())` of the organism `org`
    /// itself (every C++ caller passes the organism's own index into the
    /// live list).
    fn try_write_birth_loc_data(&mut self, org: &OrgState);
    /// `InjectPreyClone(ctx, m_organism->SystematicsGroup("genotype")->ID())`
    /// from the running organism `org` (the clone may replace it).
    fn inject_prey_clone(&mut self, org: &mut OrgState);
    /// `KillRandPred(ctx, org)`
    fn kill_rand_pred(&mut self, org: &mut OrgState);
    /// `KillRandPrey(ctx, org)`
    fn kill_rand_prey(&mut self, org: &mut OrgState);
    /// `TryWriteLookData(string)`
    fn try_write_look_data(&mut self, s: &str);
    /// `TryWriteLookOutput(string)`
    fn try_write_look_output(&mut self, s: &str);
    /// `TryWriteLookEXOutput(string)`
    fn try_write_look_ex_output(&mut self, s: &str);
    /// `GetFormedGroupArray()`
    fn get_formed_group_array(&mut self) -> Vec<i32>;

    // ---- avatars ----
    /// `HasOutputAV(av_num)`
    fn has_output_av(&self, av_num: i32) -> bool;
    /// `FacedHasOutputAV(av_num)`
    fn faced_has_output_av(&self, av_num: i32) -> bool;
    /// `FacedHasAV(av_num)`
    fn faced_has_av(&self, av_num: i32) -> bool;
    /// `FacedHasPredAV(av_num)`
    fn faced_has_pred_av(&self, av_num: i32) -> bool;
    /// `FacedHasPreyAV(av_num)`
    fn faced_has_prey_av(&self, av_num: i32) -> bool;
    /// `AddIOAV(ctx, av_cell_id, av_facing, input, output)`
    fn add_io_av(&mut self, av_cell_id: i32, av_facing: i32, input: bool, output: bool);
    /// `AddPredPreyAV(ctx, av_cell_id)` for the organism `org` (its
    /// `IsPreyFT()` picks the avatar list).
    fn add_pred_prey_av(&mut self, org: &OrgState, av_cell_id: i32);
    /// `SwitchPredPrey(ctx, av_num)`
    fn switch_pred_prey(&mut self, av_num: i32);
    /// `RemoveAllAV()`
    fn remove_all_av(&mut self);
    /// `GetAVFacing(av_num)`
    fn get_av_facing(&self, av_num: i32) -> i32;
    /// `GetAVCellID(av_num)`
    fn get_av_cell_id(&self, av_num: i32) -> i32;
    /// `GetAVFacedCellID(av_num)`
    fn get_av_faced_cell_id(&self, av_num: i32) -> i32;
    /// `GetAVNumNeighbors(av_num)`
    fn get_av_num_neighbors(&self, av_num: i32) -> i32;
    /// `GetAVFacedData(av_num)`
    fn get_av_faced_data(&self, av_num: i32) -> i32;
    /// `GetAVFacedDataOrgID(av_num)`
    fn get_av_faced_data_org_id(&self, av_num: i32) -> i32;
    /// `GetAVFacedDataUpdate(av_num)`
    fn get_av_faced_data_update(&self, av_num: i32) -> i32;
    /// `GetAVFacedDataTerritory(av_num)`
    fn get_av_faced_data_territory(&self, av_num: i32) -> i32;
    /// `GetAVData(av_num)`
    fn get_av_data(&self, av_num: i32) -> i32;
    /// `GetAVDataOrgID(av_num)`
    fn get_av_data_org_id(&self, av_num: i32) -> i32;
    /// `GetAVDataUpdate(av_num)`
    fn get_av_data_update(&self, av_num: i32) -> i32;
    /// `GetAVDataTerritory(av_num)`
    fn get_av_data_territory(&self, av_num: i32) -> i32;
    /// `FindAV(input, output, av_num)`
    fn find_av(&self, input: bool, output: bool, av_num: i32) -> i32;
    /// `SetAVFacing(ctx, av_facing, av_num)`
    fn set_av_facing(&mut self, av_facing: i32, av_num: i32);
    /// `SetAVCellID(ctx, av_cell_id, av_num)`
    fn set_av_cell_id(&mut self, av_cell_id: i32, av_num: i32) -> bool;
    /// `SetAVFacedCellID(ctx, av_num)`
    fn set_av_faced_cell_id(&mut self, av_num: i32);
    /// `SetAVCellData(new_data, org_id, av_num)` from the running organism
    /// `org`, the occupant `cPopulationCell::SetCellData` reads when the
    /// avatar is in its own cell.
    fn set_av_cell_data(&mut self, org: &mut OrgState, new_data: i32, org_id: i32, av_num: i32);
    /// `MoveAV(ctx, av_num)`; `org` is the organism owning the avatar
    /// (`MoveOrganisms` may kill or injure it).
    fn move_av(&mut self, org: &mut OrgState, av_num: i32) -> bool;
    /// `RotateAV(ctx, increment, av_num)`
    fn rotate_av(&mut self, increment: i32, av_num: i32) -> bool;
    /// `GetRandFacedAV(ctx, av_num)`
    fn get_rand_faced_av(&mut self, av_num: i32) -> Option<OrgUid>;
    /// `GetRandFacedPredAV(av_num)`
    fn get_rand_faced_pred_av(&mut self, av_num: i32) -> Option<OrgUid>;
    /// `GetRandFacedPreyAV(av_num)`
    fn get_rand_faced_prey_av(&mut self, av_num: i32) -> Option<OrgUid>;
    /// `GetFacedAVs(av_num)`
    fn get_faced_avs(&mut self, av_num: i32) -> Vec<OrgUid>;
    /// `GetCellAVs(av_cell_id, av_num)`
    fn get_cell_avs(&mut self, av_cell_id: i32, av_num: i32) -> Vec<OrgUid>;
    /// `GetFacedPreyAVs(av_num)`
    fn get_faced_prey_avs(&mut self, av_num: i32) -> Vec<OrgUid>;
    /// `GetAVResources(ctx, av_num)`
    fn get_av_resources(&mut self, org: &mut OrgState, av_num: i32) -> Vec<f64>;
    /// `GetAVResourceVal(ctx, res_id, av_num)`
    fn get_av_resource_val(&mut self, org: &mut OrgState, res_id: i32, av_num: i32) -> f64;
    /// `GetAVFacedResources(ctx, av_num)`
    fn get_av_faced_resources(&mut self, org: &mut OrgState, av_num: i32) -> Vec<f64>;
    /// `GetAVFacedResourceVal(ctx, res_id, av_num)`
    fn get_av_faced_resource_val(&mut self, org: &mut OrgState, res_id: i32, av_num: i32) -> f64;
    /// `UpdateAVResources(ctx, res_change, av_num)`
    fn update_av_resources(&mut self, org: &mut OrgState, res_change: &[f64], av_num: i32);

    // ---- what the look sensor (`cOrgSensor`) reads of the population ----
    /// What the sensor reads of the living organism `uid` (through its
    /// `cOrganism*` and interface). `None` when there is none, and for the
    /// organism being run, whose state the caller holds.
    fn seen_org(&mut self, uid: OrgUid) -> Option<SeenOrg>;
    /// `GetCell(cell_id)->GetNumPredAV()` and `GetNumPreyAV()`.
    fn cell_av_counts(&self, cell_id: i32) -> (i32, i32);
    /// `GetCell(cell_id)->GetRandAV(ctx)` (draws).
    fn cell_rand_av(&mut self, cell_id: i32) -> Option<OrgUid>;
    /// `GetCell(cell_id)->GetRandPredAV()` (the first entry).
    fn cell_first_pred_av(&self, cell_id: i32) -> Option<OrgUid>;
    /// `GetCell(cell_id)->GetRandPreyAV()` (the first entry).
    fn cell_first_prey_av(&self, cell_id: i32) -> Option<OrgUid>;
    /// `GetCell(cell_id)->GetCellOutputAVs()`: the prey avatars in the cell,
    /// in list order.
    fn cell_prey_avs(&self, _cell_id: i32) -> Vec<OrgUid> {
        Vec::new()
    }
    /// The `GetNeighborhoodCellIDs` list of any cell: its connection list,
    /// in list order.
    fn cell_neighborhood_ids(&self, _cell_id: i32) -> Vec<i32> {
        Vec::new()
    }
    /// `GetResourceCount()->GetMinUsedX/GetMinUsedY/GetMaxUsedX/GetMaxUsedY
    /// (res_id)`.
    fn resource_used_bounds(&self, res_id: i32) -> (i32, i32, i32, i32);
    /// The index of `inst` in `m_world->GetStats().GetGroupAttackInsts
    /// (inst_set)`; `None` where there is no such list (the test CPU).
    fn group_attack_inst_index(&self, _inst_set: &str, _inst: &str) -> Option<usize> {
        None
    }

    // ---- sleep ----
    /// `BeginSleep()`
    fn begin_sleep(&mut self);
    /// `EndSleep()`
    fn end_sleep(&mut self);

    // ------------------------------------------------------------------
    // Population and deme services the `Inst_*` handlers reach through
    // `m_world->GetPopulation()` or `GetDeme()` (cell, deme and resource
    // ids are plain ints as in the C++)
    // ------------------------------------------------------------------

    /// `m_world->GetPopulation().GetCell(cell_id).GetDemeID()`
    fn cell_deme_id(&self, cell_id: i32) -> i32;
    /// `m_world->GetPopulation().GetDemeCellResources(deme_id, cell_id,
    /// ctx)`: the deme resource levels in the deme-relative cell of the
    /// absolute `cell_id` (indexed like the deme's `cResourceCount`).
    fn get_deme_cell_resources(&mut self, deme_id: i32, cell_id: i32) -> Vec<f64>;
    /// `GetDeme(deme_id).GetDemeResourceCount().Get(ctx, res_index)`
    /// (`res_index` counts deme resources only, in library order).
    fn deme_resource_amount(&mut self, deme_id: i32, res_index: i32) -> f64;
    /// `m_world->GetPopulation().GetResourceCount().Get(ctx, res_index)`
    /// (`res_index` counts non-deme resources only, in library order).
    fn population_resource_amount(&mut self, org: &mut OrgState, res_index: i32) -> f64;
    /// `GetDeme(deme_id).AddPheromone(cell_id, value, ctx)`
    fn deme_add_pheromone(&mut self, deme_id: i32, cell_id: i32, value: f64);
    /// `GetDeme(deme_id).GetCellPosition(cell_id)`
    fn deme_cell_position(&self, deme_id: i32, cell_id: i32) -> (i32, i32);
    /// `m_world->GetPopulation().UpdateCellResources(ctx, res_change,
    /// cell_id)`
    fn update_cell_resources(&mut self, org: &mut OrgState, res_change: &[f64], cell_id: i32);
    /// `m_world->GetStats().Move(*organism)`
    fn stats_move(&mut self, org: &OrgState);
    /// `m_world->GetPopulation().GetCell(cell_id).GetCellData()`: the raw
    /// cell data, without the expiry check `GetCellData()` makes first.
    fn population_cell_data(&self, cell_id: i32) -> i32;
    /// `m_world->GetPopulation().GetCell(cell_id).GetOrganism()->GetID()`,
    /// or -1 when the cell is empty (`IsOccupied()` false).
    fn cell_organism_id(&self, cell_id: i32) -> i32;
    /// `m_world->GetPopulation().GetCell(cell_id).GetOrganism()` (the
    /// running organism answers for its own cell), `None` when empty.
    fn cell_organism_uid(&self, cell_id: i32) -> Option<OrgUid>;
    /// `to_org->UpdateMerit(ctx, new_merit)`: `cPopulation::UpdateMerit
    /// (cell_id, new_merit)` on the organism living in `cell_id`, which is
    /// the running organism `org` when `cell_id` is its own cell.
    fn update_cell_merit(&mut self, org: &mut OrgState, cell_id: i32, new_merit: f64) -> bool;
    /// `GetDeme(deme_id).IncreaseEnergyDonated(amount)`
    fn deme_increase_energy_donated(&mut self, deme_id: i32, amount: f64);
    /// `GetDeme(deme_id).IncreaseEnergyReceived(amount)`
    fn deme_increase_energy_received(&mut self, deme_id: i32, amount: f64);
    /// `while (neighbor->GetNeighbor() != m_organism) neighbor->Rotate(ctx,
    /// 1)`: turn the organism in `cell_id` until it faces `target_cell_id`.
    /// The C++ loops forever when the two cells are not connected.
    fn turn_cell_to_face(&mut self, cell_id: i32, target_cell_id: i32);
    /// `m_world->GetStats().IncDonateToDonor()`
    fn stats_inc_donate_to_donor(&mut self);
    /// `m_world->GetStats().LogRetMessage(msg)` (`NET_LOG_RETMESSAGES`)
    fn stats_log_ret_message(&mut self, msg: &OrgMessage);
    /// `m_world->GetStats().SentFlash(*organism)`
    fn stats_sent_flash(&mut self, org: &OrgState);
    /// `GetDeme(deme_id).KillCellEvent(event_id)`: whether an event died.
    /// `org` is the running organism, whose opinion and forage target
    /// clearing its own cell's data reads.
    fn deme_kill_cell_event(&mut self, org: &mut OrgState, deme_id: i32, event_id: i32) -> bool;
    /// `(m_world->GetStats().SumCreatureAge().Ave(), .StdDeviation())`
    /// (`cOrganism::GetVitality`); `(0, 0)` for empty statistics.
    fn stats_creature_age_mean_stddev(&self) -> (f64, f64);
    /// `m_world->GetPopulation().KillGroupMember(ctx, group_id, org)`
    fn kill_group_member(&mut self, group_id: i32, org: &mut OrgState);
    /// `GetDeme(deme_id).AddResourcesConsumed(amount)`
    fn deme_add_resources_consumed(&mut self, deme_id: i32, amount: f64);
    /// `GetDeme(deme_id).ReplicateDeme()`
    fn deme_replicate(&mut self, deme_id: i32);
    /// `GetDeme(deme_id).AddNumberOfPoints(points); ...AddSuicide()`
    fn deme_record_suicide(&mut self, deme_id: i32, points: i32);
    /// `GetDeme(deme_id).IncreaseTotalEnergyTestament(amount)`
    fn deme_increase_total_energy_testament(&mut self, deme_id: i32, amount: f64);
    /// `m_world->GetStats().SumEnergyTestamentTo<target>().Add(amount)`
    fn stats_add_energy_testament(&mut self, target: TestamentTarget, amount: f64);
    /// `m_world->GetStats().IncDontExplode()`
    fn stats_inc_dont_explode(&mut self);
    /// `IncKaboom(); IncPercLyse(percent_prob); IncSumCPUs(cpu_cycles)` on
    /// `m_world->GetStats()` (`Inst_Lyse`).
    fn stats_lyse(&mut self, percent_prob: f64, cpu_cycles: i32);
    /// `IncQuorumThresholdUB(org_ratio); IncQuorumNum()` on
    /// `m_world->GetStats()` (`Inst_SenseQuorum`, `Inst_NoisyQuorum`).
    fn stats_inc_quorum(&mut self, org_ratio: i32);
    /// `m_world->GetStats().IncGuardFail()` (experimental hardware
    /// `Inst_SetGuard`, `Inst_SetGuardOnce`).
    fn stats_inc_guard_fail(&mut self);
    /// `m_world->GetStats().GetNumPreyCreatures()`; `Err` where the
    /// statistic is not available.
    fn stats_num_prey_creatures(&self) -> Result<i32, String>;
    /// `m_world->GetStats().GetNumTotalPredCreatures()`; `Err` where the
    /// statistic is not available.
    fn stats_num_total_pred_creatures(&self) -> Result<i32, String>;
    /// `m_world->GetStats().GetNumPredCreatures()` (predators only, not top
    /// predators; `cHardwareBCR` / `cHardwareGP8` `makePred`); `Err` where
    /// the statistic is not available.
    fn stats_num_pred_creatures(&self) -> Result<i32, String> {
        Err(
            "unsupported: the world statistics' predator count (C++: cStats::GetNumPredCreatures)"
                .to_string(),
        )
    }
}

/// Which `cStats::SumEnergyTestamentTo*` sum a relinquished energy amount
/// goes to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TestamentTarget {
    /// `SumEnergyTestamentToFutureDeme()`
    FutureDeme,
    /// `SumEnergyTestamentToNeighborOrganisms()`
    NeighborOrganisms,
    /// `SumEnergyTestamentToDemeOrganisms()`
    DemeOrganisms,
}
