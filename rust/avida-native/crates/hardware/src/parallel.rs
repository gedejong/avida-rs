//! The organism side of `EXECUTION_MODE 1`, the parallel execution mode
//! (nothing in the C++; see ARCHITECTURE.md, "Parallel execution mode").
//!
//! In the parallel phase of an update every organism with instructions
//! left runs a burst of speculative steps
//! (`single_process(.., speculative = true)`) on a worker thread, against a
//! [`LocalInterface`]: the read-only world data of [`LocalWorld`] and the
//! organism's own random stream, nothing else. A step stops before its
//! instruction (`Ok(false)`, the instruction not executed) when that
//! instruction is not organism-local: when the instruction set marks it
//! `STALL`, as for the C++ speculative execution, or, while
//! [`HardwareBase::parallel_burst`] is set, when its handler is not in the
//! hardware type's allow-list ([`CPU_PARALLEL_METHODS`] for the heads CPU,
//! the only hardware type with one). The world then runs that one
//! instruction through the exact code path.
//!
//! The allow-list was made by reading every handler on it: each touches
//! only the organism's state, the configuration (`world.config()`), the
//! random number generator (`world.rng()`, here the organism's own
//! stream) and `world.verbosity()`. Whatever else a step asks of a
//! [`LocalInterface`] is recorded ([`LocalInterface::denied_call`]) and
//! turned into a run error by the caller, so a handler wrongly on the list
//! stops the run instead of computing something different.
//!
//! [`HardwareBase::parallel_burst`]: crate::hardware::HardwareBase::parallel_burst

use std::cell::Cell;
use std::sync::{Arc, OnceLock};

use avida_apto::AvidaRng;
use avida_config::config::Config;
use avida_genome::{Genome, InstructionSequence};

use crate::cpu_inst_lib::CPU_INST_DEFS;
use crate::hardware::Hardware;
use crate::hardware_manager::HardwareManager;
use crate::hardware_transsmt::ParasiteUnit;
use crate::org_interface::{
    Faced, OffspringTest, OrgInterface, OrgMessage, OutputServices, SeenOrg, SelfDelivery,
    TestamentTarget, WorldServices,
};
use crate::organism::{OrgState, OrgUid, Organism};
use crate::task_adapter::Env;

/// The heads-CPU (`cHardwareCPU`) handlers an organism may run in the
/// parallel phase. Each was read for what it reaches through the
/// interface: nothing but `config()`, `rng()` and `verbosity()` (the last
/// only for the slip-mutation report, which the world keeps out of the
/// parallel phase at `VERBOSITY` 3 and above). Handlers the instruction
/// library marks `STALL` stop a speculative step anyway.
pub static CPU_PARALLEL_METHODS: &[&str] = &[
    // No-ops and labels
    "Inst_Nop",
    "Inst_EndIf",
    "Inst_NandTreatable",
    "Inst_Catch",
    "Inst_Label",
    "Inst_Transposon",
    // Conditionals (the `if-p-*` ones draw from the organism's stream)
    "Inst_If0",
    "Inst_IfNot0",
    "Inst_If0_defaultAX",
    "Inst_IfNot0_defaultAX",
    "Inst_IfEqu",
    "Inst_IfNEqu",
    "Inst_IfGr0",
    "Inst_IfGr",
    "Inst_IfGrEqu0",
    "Inst_IfGrEqu",
    "Inst_IfLess0",
    "Inst_IfLess",
    "Inst_IfLsEqu0",
    "Inst_IfLsEqu",
    "Inst_IfANotEqB",
    "Inst_IfBNotEqC",
    "Inst_IfANotEqC",
    "Inst_IfBit1",
    "Inst_IfGrX",
    "Inst_IfEquX",
    "Inst_IfGerm",
    "Inst_IfSoma",
    "Inst_IfP0p125",
    "Inst_IfP0p25",
    "Inst_IfP0p50",
    "Inst_IfP0p75",
    "Inst_IfLessEnd",
    "Inst_IfNotEqualEnd",
    "Inst_IfGrtEquEnd",
    "Inst_Else",
    // Flow control
    "Inst_JumpF",
    "Inst_JumpB",
    "Inst_Call",
    "Inst_Return",
    "Inst_Throw",
    "Inst_ThrowIf0",
    "Inst_ThrowIfNot0",
    "Inst_Goto",
    "Inst_GotoIf0",
    "Inst_GotoIfNot0",
    // Stacks and registers
    "Inst_Pop",
    "Inst_Push",
    "Inst_SwitchStack",
    "Inst_FlipStack",
    "Inst_Swap",
    "Inst_SwapAB",
    "Inst_SwapBC",
    "Inst_SwapAC",
    "Inst_CopyReg",
    "Inst_CopyRegAB",
    "Inst_CopyRegAC",
    "Inst_CopyRegBA",
    "Inst_CopyRegBC",
    "Inst_CopyRegCA",
    "Inst_CopyRegCB",
    "Inst_Reset",
    "Inst_PopA",
    "Inst_PopB",
    "Inst_PopC",
    "Inst_PushA",
    "Inst_PushB",
    "Inst_PushC",
    // Arithmetic and logic
    "Inst_ShiftR",
    "Inst_ShiftL",
    "Inst_Bit1",
    "Inst_SetNum",
    "Inst_ValGrey",
    "Inst_ValDir",
    "Inst_ValAddP",
    "Inst_ValFib",
    "Inst_ValPolyC",
    "Inst_Inc",
    "Inst_Dec",
    "Inst_Zero",
    "Inst_One",
    "Inst_All1s",
    "Inst_Neg",
    "Inst_Square",
    "Inst_Sqrt",
    "Inst_Not",
    "Inst_Add",
    "Inst_Sub",
    "Inst_Mult",
    "Inst_Div",
    "Inst_Mod",
    "Inst_Nand",
    "Inst_Or",
    "Inst_Nor",
    "Inst_And",
    "Inst_Order",
    "Inst_Xor",
    "Inst_Setbit",
    "Inst_Clearbit",
    "Inst_MaskSignBit",
    "Inst_MaskOffLower16Bits",
    "Inst_MaskOffLower16Bits_defaultAX",
    "Inst_MaskOffLower15Bits",
    "Inst_MaskOffLower15Bits_defaultAX",
    "Inst_MaskOffLower14Bits",
    "Inst_MaskOffLower14Bits_defaultAX",
    "Inst_MaskOffLower13Bits",
    "Inst_MaskOffLower13Bits_defaultAX",
    "Inst_MaskOffLower12Bits",
    "Inst_MaskOffLower12Bits_defaultAX",
    "Inst_MaskOffLower8Bits",
    "Inst_MaskOffLower8Bits_defaultAX",
    "Inst_MaskOffLower4Bits",
    "Inst_MaskOffLower4Bits_defaultAX",
    // Memory, copying and allocation (copy mutations draw from the
    // organism's stream)
    "Inst_Copy",
    "Inst_ReadInst",
    "Inst_WriteInst",
    "Inst_StackReadInst",
    "Inst_StackWriteInst",
    "Inst_Compare",
    "Inst_IfNCpy",
    "Inst_Allocate",
    "Inst_CAlloc",
    "Inst_MaxAlloc",
    "Inst_MaxAllocMoveWriteHead",
    "Inst_SearchF",
    "Inst_SearchB",
    "Inst_MemSize",
    "Inst_SetCopyMut",
    "Inst_ModCopyMut",
    "Inst_GetID",
    // Threads
    "Inst_ForkThread",
    "Inst_ForkThreadLabel",
    "Inst_ForkThreadLabelIfNot0",
    "Inst_ForkThreadLabelIf0",
    "Inst_KillThread",
    "Inst_ThreadID",
    // Heads
    "Inst_HeadRead",
    "Inst_HeadWrite",
    "Inst_HeadCopy",
    "Inst_HeadCopy2",
    "Inst_HeadCopy3",
    "Inst_HeadCopy4",
    "Inst_HeadCopy5",
    "Inst_HeadCopy6",
    "Inst_HeadCopy7",
    "Inst_HeadCopy8",
    "Inst_HeadCopy9",
    "Inst_HeadCopy10",
    "Inst_HeadSearch",
    "Inst_HeadSearchDirect",
    "Inst_HeadPush",
    "Inst_HeadPop",
    "Inst_SetHead",
    "Inst_AdvanceHead",
    "Inst_MoveHead",
    "Inst_JumpHead",
    "Inst_GetHead",
    "Inst_IfLabel",
    "Inst_IfLabelDirect",
    "Inst_IfLabel2",
    "Inst_SetFlow",
];

/// Whether the heads-CPU library entry `lib_fun_id` (an index into
/// [`CPU_INST_DEFS`], `cInstSet::GetLibFunctionIndex`) may run in the
/// parallel phase.
#[inline]
pub fn cpu_parallel_ok(lib_fun_id: i32) -> bool {
    static TABLE: OnceLock<Vec<bool>> = OnceLock::new();
    let table = TABLE.get_or_init(|| {
        CPU_INST_DEFS
            .iter()
            .map(|d| CPU_PARALLEL_METHODS.contains(&d.method))
            .collect()
    });
    usize::try_from(lib_fun_id)
        .ok()
        .and_then(|i| table.get(i))
        .copied()
        .unwrap_or(false)
}

/// What the parallel mode keeps on each organism. Nothing here is read in
/// the exact mode, and nothing here exists in the C++.
#[derive(Debug, Clone, Default)]
pub struct ParallelOrgState {
    /// Instructions the organism may still execute in the current update.
    pub budget: u32,
    /// The budget it was granted at update time [`ParallelOrgState::t0`]
    /// (the update's start, its birth, or its last change of rate), so that
    /// its own clock is `t0 + (1 - t0) * (granted - budget) / granted`.
    pub granted: u32,
    /// The update time (0 to 1) at which `granted` was granted.
    pub t0: f64,
    /// The scheduling weight the budget was computed with (its
    /// `AdjustSchedule` priority, or 1 under `SLICING_METHOD 0`).
    pub weight: f64,
    /// The fractional part of its merit-proportional share that the
    /// budget allocation carries to the next update.
    pub carry: f64,
    /// The organism's own random stream ([`org_stream`]), made on first
    /// use: mutations drawn in the parallel phase come from here, never
    /// from the world's generator.
    pub rng: Option<Box<AvidaRng>>,
    /// The sequential phase in which the organism last ran its one
    /// instruction (the world's counter), so that an organism that moved
    /// into another waiting cell does not run twice in one phase.
    pub stamp: u64,
}

/// `Apto::RNG::AvidaRNG`'s largest effective seed.
const MAX_SEED: u64 = 161_803_398;

/// SplitMix64's finaliser: a bijective 64-bit mixer.
fn mix64(mut z: u64) -> u64 {
    z = (z ^ (z >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
    z ^ (z >> 31)
}

/// A random stream of a run with seed `run_seed`, one per `key`: an
/// `AvidaRng` seeded from a hash of the two, advanced by up to 63 more
/// draws (also from the hash) so that two keys of one run share a stream
/// about 64 times less often than the generator's 1.6e8 seeds alone
/// allow.
pub fn stream(run_seed: i32, key: u64) -> AvidaRng {
    let h = mix64(mix64(u64::from(run_seed as u32) ^ 0x6a09_e667_f3bc_c909) ^ key);
    let mut rng = AvidaRng::new((h % MAX_SEED) as i32);
    for _ in 0..(h >> 58) {
        rng.get_double();
    }
    rng
}

/// The random stream of the organism `uid` (organism uids start at 1).
pub fn org_stream(run_seed: i32, uid: OrgUid) -> AvidaRng {
    stream(run_seed, uid.0)
}

/// What one burst ([`run_burst`]) did.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct BurstOutcome {
    /// Instructions executed (each took one unit of the budget).
    pub executed: u32,
    /// The organism stopped before an instruction the world has to run
    /// (or cannot run bursts at all) with budget left.
    pub needs_world: bool,
    /// A run error: a hardware `Err`, or a request the [`LocalInterface`]
    /// refused.
    pub error: Option<String>,
}

/// The parallel phase of one organism: speculative steps against a
/// [`LocalInterface`] until its budget is spent or its next instruction
/// is not organism-local. An organism that cannot run bursts
/// ([`Hardware::parallel_capable`], or `force_world`) runs nothing here and
/// is handed to the world. `cell_id` is its cell, `run_seed` the seed its
/// random stream is made from on first use.
///
/// A step that ran its instruction but answers `Ok(false)` (the organism
/// reached its maximum age or must die: `spec_die`) counts against the
/// budget and hands the organism to the world, whose non-speculative step
/// then carries out the death, as in the exact mode.
pub fn run_burst(
    org: &mut Organism,
    world: &LocalWorld<'_>,
    cell_id: i32,
    run_seed: i32,
    force_world: bool,
) -> BurstOutcome {
    let mut out = BurstOutcome::default();
    let cap = org.state.parallel.budget;
    if cap == 0 {
        return out;
    }
    if force_world || !org.hardware.parallel_capable() {
        out.needs_world = true;
        return out;
    }
    let mut rng = match org.state.parallel.rng.take() {
        Some(rng) => rng,
        None => Box::new(org_stream(run_seed, org.state.uid)),
    };
    org.hardware.base_mut().parallel_burst = true;
    {
        let mut iface = LocalInterface::new(world, &mut rng, cell_id);
        while out.executed < cap {
            let cycles = org.state.phenotype.cpu_cycles_used;
            let r = org
                .hardware
                .single_process(&mut org.state, &mut iface, true);
            let executed = org.state.phenotype.cpu_cycles_used != cycles;
            if executed {
                out.executed += 1;
                org.state.parallel.budget -= 1;
            }
            if let Some(what) = iface.denied_call() {
                out.error = Some(format!(
                    "internal: an instruction of the organism in cell {cell_id} asked for \
                     {what} in the parallel phase of EXECUTION_MODE 1 (its handler must not \
                     be on the parallel allow-list)"
                ));
                break;
            }
            match r {
                Ok(true) if executed => {}
                Ok(_) => {
                    out.needs_world = out.executed < cap;
                    break;
                }
                Err(e) => {
                    out.error = Some(e);
                    break;
                }
            }
        }
    }
    org.hardware.base_mut().parallel_burst = false;
    org.state.parallel.rng = Some(rng);
    out
}

/// The world data every worker reads during a parallel phase; none of it
/// changes until the phase ends.
pub struct LocalWorld<'a> {
    /// `m_world->GetConfig()`
    pub config: &'a Config,
    /// `m_world->GetEnvironment()`
    pub environment: &'a Env,
    /// `m_world->GetHardwareManager()`
    pub hardware_manager: &'a HardwareManager,
    /// `m_world->GetStats().GetUpdate()`
    pub update: i32,
    /// `m_world->GetStats().GetTotCreatures()`
    pub tot_creatures: i32,
    /// `m_world->GetStats().GetSenseSize()`
    pub sense_size: usize,
}

/// The [`OrgInterface`] of an organism in the parallel phase: the
/// read-only [`LocalWorld`], the organism's own random stream and its cell
/// id. Every other request is refused: it is recorded
/// ([`LocalInterface::denied_call`]) and answered with a neutral value,
/// and the caller stops the run.
pub struct LocalInterface<'a> {
    world: &'a LocalWorld<'a>,
    rng: &'a mut AvidaRng,
    cell_id: i32,
    denied: Cell<Option<&'static str>>,
}

impl<'a> LocalInterface<'a> {
    /// The interface of the organism in `cell_id`, drawing from `rng`.
    pub fn new(world: &'a LocalWorld<'a>, rng: &'a mut AvidaRng, cell_id: i32) -> Self {
        LocalInterface {
            world,
            rng,
            cell_id,
            denied: Cell::new(None),
        }
    }

    /// The first request this interface refused, if any.
    pub fn denied_call(&self) -> Option<&'static str> {
        self.denied.get()
    }

    fn deny(&self, what: &'static str) {
        if self.denied.get().is_none() {
            self.denied.set(Some(what));
        }
    }

    fn denied(&self, what: &'static str) -> String {
        self.deny(what);
        format!("{what} is not organism-local (parallel phase)")
    }
}

impl WorldServices for LocalInterface<'_> {
    fn config(&self) -> &Config {
        self.world.config
    }
    fn environment(&self) -> &Env {
        self.world.environment
    }
    fn rng(&mut self) -> &mut AvidaRng {
        self.rng
    }
    fn hardware_manager(&self) -> &HardwareManager {
        self.world.hardware_manager
    }
    fn update(&self) -> i32 {
        self.world.update
    }
    fn tot_creatures(&self) -> i32 {
        self.world.tot_creatures
    }
    fn sense_size(&self) -> usize {
        self.world.sense_size
    }
    fn group_attack_inst_count(&self, _inst_set: &str) -> usize {
        self.deny("group_attack_inst_count");
        0
    }
    fn test_mode(&self) -> bool {
        false
    }
    fn analyze_mode(&self) -> bool {
        false
    }
    fn org_fault_reporting(&self) -> bool {
        false
    }
    fn verbosity(&self) -> i32 {
        self.world.config.verbosity
    }
    fn new_org_uid(&mut self) -> OrgUid {
        self.deny("new_org_uid");
        OrgUid::NONE
    }
}

/// Every [`OrgInterface`] method but the cell id is refused (see the
/// module docs). `output_services` cannot answer at all: it is reached
/// only through `cOrganism::DoOutput`, whose instructions are not on any
/// allow-list.
impl OrgInterface for LocalInterface<'_> {
    fn output_services(&mut self) -> OutputServices<'_> {
        panic!(
            "parallel phase: an instruction asked for the task output services; \
             its handler must not be on the parallel allow-list"
        )
    }
    fn get_cell_id(&self) -> i32 {
        self.cell_id
    }
    fn take_self_deliveries(&mut self) -> Vec<SelfDelivery> {
        Vec::new()
    }
    fn test_sterilize(&self) -> bool {
        self.deny("test_sterilize");
        Default::default()
    }
    fn genotype_test_fitness(&mut self, _org: &OrgState) -> f64 {
        self.deny("genotype_test_fitness");
        Default::default()
    }
    fn has_genotype(&mut self) -> bool {
        self.deny("has_genotype");
        Default::default()
    }
    fn phylo_distance_to(&mut self, _cell: i32) -> Result<i32, String> {
        Err(self.denied("phylo_distance_to"))
    }
    fn test_offspring(&mut self, _parent: &OrgState, _genome: &Genome) -> OffspringTest {
        self.deny("test_offspring");
        Default::default()
    }
    fn stats_inc_resamplings(&mut self) {
        self.deny("stats_inc_resamplings");
    }
    fn stats_inc_failed_resamplings(&mut self) {
        self.deny("stats_inc_failed_resamplings");
    }
    fn stats_add_new_task_count(&mut self, _task: i32) {
        self.deny("stats_add_new_task_count");
    }
    fn deme_inc_num_switching_penalties(&mut self, _n: i32) {
        self.deny("deme_inc_num_switching_penalties");
    }
    fn deme_dec_sleeping_count(&mut self) {
        self.deny("deme_dec_sleeping_count");
    }
    fn deme_inc_sleeping_count(&mut self) {
        self.deny("deme_inc_sleeping_count");
    }
    fn with_organism(&mut self, _uid: OrgUid, _f: &mut dyn FnMut(&mut Organism)) -> bool {
        self.deny("with_organism");
        Default::default()
    }
    fn with_organism_in_cell(&mut self, _cell_id: i32, _f: &mut dyn FnMut(&mut Organism)) -> bool {
        self.deny("with_organism_in_cell");
        Default::default()
    }
    fn live_org_ids(&self) -> Vec<i32> {
        self.deny("live_org_ids");
        Default::default()
    }
    fn live_org_uids(&self) -> Vec<OrgUid> {
        self.deny("live_org_uids");
        Default::default()
    }
    fn live_org_forage_targets(&self) -> Result<Vec<(OrgUid, i32, Option<i32>)>, String> {
        Err(self.denied("live_org_forage_targets"))
    }
    fn find_live_org(&self, _id: i32) -> Option<OrgUid> {
        self.deny("find_live_org");
        Default::default()
    }
    fn get_deme_id(&self) -> i32 {
        self.deny("get_deme_id");
        Default::default()
    }
    fn set_cell_id(&mut self, _id: i32) {
        self.deny("set_cell_id");
    }
    fn set_deme_id(&mut self, _id: i32) {
        self.deny("set_deme_id");
    }
    fn get_cell_x_position(&self) -> i32 {
        self.deny("get_cell_x_position");
        Default::default()
    }
    fn get_cell_y_position(&self) -> i32 {
        self.deny("get_cell_y_position");
        Default::default()
    }
    fn get_cell_data(&self) -> i32 {
        self.deny("get_cell_data");
        Default::default()
    }
    fn get_cell_data_org_id(&self) -> i32 {
        self.deny("get_cell_data_org_id");
        Default::default()
    }
    fn get_cell_data_update(&self) -> i32 {
        self.deny("get_cell_data_update");
        Default::default()
    }
    fn get_cell_data_territory(&self) -> i32 {
        self.deny("get_cell_data_territory");
        Default::default()
    }
    fn get_cell_data_forager_type(&self) -> i32 {
        self.deny("get_cell_data_forager_type");
        Default::default()
    }
    fn set_cell_data(&mut self, _org: &mut OrgState, _data: i32) {
        self.deny("set_cell_data");
    }
    fn get_faced_cell_data(&self) -> i32 {
        self.deny("get_faced_cell_data");
        Default::default()
    }
    fn get_faced_cell_data_org_id(&self) -> i32 {
        self.deny("get_faced_cell_data_org_id");
        Default::default()
    }
    fn get_faced_cell_data_update(&self) -> i32 {
        self.deny("get_faced_cell_data_update");
        Default::default()
    }
    fn get_faced_cell_data_territory(&self) -> i32 {
        self.deny("get_faced_cell_data_territory");
        Default::default()
    }
    fn get_prev_seen_cell_id(&self) -> i32 {
        self.deny("get_prev_seen_cell_id");
        Default::default()
    }
    fn get_prev_task_cell_id(&self) -> i32 {
        self.deny("get_prev_task_cell_id");
        Default::default()
    }
    fn get_num_task_cells_reached(&self) -> i32 {
        self.deny("get_num_task_cells_reached");
        Default::default()
    }
    fn add_reached_task_cell(&mut self) {
        self.deny("add_reached_task_cell");
    }
    fn set_prev_seen_cell_id(&mut self, _id: i32) {
        self.deny("set_prev_seen_cell_id");
    }
    fn set_prev_task_cell_id(&mut self, _id: i32) {
        self.deny("set_prev_task_cell_id");
    }
    fn divide(&mut self, _parent: &mut OrgState, _offspring_genome: &Genome) -> bool {
        self.deny("divide");
        Default::default()
    }
    fn divide_with_traces(
        &mut self,
        _parent: &mut OrgState,
        _parent_hw: &dyn Hardware,
        _offspring_genome: &Genome,
        _traces: &mut crate::mini_trace::LentTraces,
    ) -> bool {
        self.deny("divide_with_traces");
        Default::default()
    }
    fn with_neighbor(&mut self, _f: &mut dyn FnMut(&mut Organism)) -> Faced {
        self.deny("with_neighbor");
        Faced::Empty
    }
    fn is_neighbor_cell_occupied(&self) -> bool {
        self.deny("is_neighbor_cell_occupied");
        Default::default()
    }
    fn get_num_neighbors(&self) -> i32 {
        self.deny("get_num_neighbors");
        Default::default()
    }
    fn get_neighborhood_cell_ids(&self) -> Vec<i32> {
        self.deny("get_neighborhood_cell_ids");
        Default::default()
    }
    fn get_av_neighborhood_cell_ids(&self, _av_num: i32) -> Vec<i32> {
        self.deny("get_av_neighborhood_cell_ids");
        Default::default()
    }
    fn get_facing(&self) -> i32 {
        self.deny("get_facing");
        Default::default()
    }
    fn get_faced_cell_id(&self) -> i32 {
        self.deny("get_faced_cell_id");
        Default::default()
    }
    fn get_faced_dir(&self) -> i32 {
        self.deny("get_faced_dir");
        Default::default()
    }
    fn get_neighbor_cell_contents(&self) -> i32 {
        self.deny("get_neighbor_cell_contents");
        Default::default()
    }
    fn rotate(&mut self, _direction: i32) {
        self.deny("rotate");
    }
    fn get_lgt_fragment(
        &mut self,
        _region: i32,
        _dest_genome: &Genome,
    ) -> Option<InstructionSequence> {
        self.deny("get_lgt_fragment");
        Default::default()
    }
    fn get_input_at(&mut self, _input_pointer: &mut i32) -> i32 {
        self.deny("get_input_at");
        Default::default()
    }
    fn reset_inputs(&mut self) {
        self.deny("reset_inputs");
    }
    fn get_inputs(&self) -> Vec<i32> {
        self.deny("get_inputs");
        Default::default()
    }
    fn get_resources(&mut self, _org: &mut OrgState) -> Vec<f64> {
        self.deny("get_resources");
        Default::default()
    }
    fn get_resource_val(&mut self, _org: &mut OrgState, _res_id: i32) -> f64 {
        self.deny("get_resource_val");
        Default::default()
    }
    fn get_faced_cell_resources(&mut self, _org: &mut OrgState) -> Vec<f64> {
        self.deny("get_faced_cell_resources");
        Default::default()
    }
    fn get_faced_resource_val(&mut self, _org: &mut OrgState, _res_id: i32) -> f64 {
        self.deny("get_faced_resource_val");
        Default::default()
    }
    fn get_deme_resources(&mut self, _deme_id: i32) -> Vec<f64> {
        self.deny("get_deme_resources");
        Default::default()
    }
    fn get_cell_resources(&mut self, _org: &mut OrgState, _cell_id: i32) -> Vec<f64> {
        self.deny("get_cell_resources");
        Default::default()
    }
    fn get_frozen_cell_res_val(&mut self, _cell_id: i32, _res_id: i32) -> f64 {
        self.deny("get_frozen_cell_res_val");
        Default::default()
    }
    fn get_cell_res_val(&mut self, _org: &mut OrgState, _cell_id: i32, _res_id: i32) -> f64 {
        self.deny("get_cell_res_val");
        Default::default()
    }
    fn get_frozen_resources(&mut self, _cell_id: i32) -> Vec<f64> {
        self.deny("get_frozen_resources");
        Default::default()
    }
    fn get_cell_id_lists(&self) -> Vec<Vec<i32>> {
        self.deny("get_cell_id_lists");
        Default::default()
    }
    fn get_curr_peak_x(&mut self, _org: &mut OrgState, _res_id: i32) -> i32 {
        self.deny("get_curr_peak_x");
        Default::default()
    }
    fn get_curr_peak_y(&mut self, _org: &mut OrgState, _res_id: i32) -> i32 {
        self.deny("get_curr_peak_y");
        Default::default()
    }
    fn get_frozen_peak_x(&mut self, _res_id: i32) -> i32 {
        self.deny("get_frozen_peak_x");
        Default::default()
    }
    fn get_frozen_peak_y(&mut self, _res_id: i32) -> i32 {
        self.deny("get_frozen_peak_y");
        Default::default()
    }
    fn trigger_do_updates(&mut self, _org: &mut OrgState) {
        self.deny("trigger_do_updates");
    }
    fn update_resources(&mut self, _org: &mut OrgState, _res_change: &[f64]) {
        self.deny("update_resources");
    }
    fn update_random_resources(&mut self, _org: &mut OrgState, _res_change: &[f64]) {
        self.deny("update_random_resources");
    }
    fn update_deme_resources(&mut self, _res_change: &[f64]) {
        self.deny("update_deme_resources");
    }
    fn die(&mut self, _org: &mut OrgState) {
        self.deny("die");
    }
    fn kill_cell_id(&mut self, _org: &mut OrgState, _target: i32) {
        self.deny("kill_cell_id");
    }
    fn kaboom(&mut self, _org: &mut OrgState, _distance: i32) {
        self.deny("kaboom");
    }
    fn kaboom_effect(&mut self, _org: &mut OrgState, _distance: i32, _effect: f64) {
        self.deny("kaboom_effect");
    }
    fn spawn_deme(&mut self, _org: &mut OrgState) {
        self.deny("spawn_deme");
    }
    fn point_mutate_deme_member(
        &mut self,
        _org: &mut OrgState,
        _hw: &mut dyn Hardware,
        _deme_id: i32,
        _prob: f64,
        _germline: Option<bool>,
    ) {
        self.deny("point_mutate_deme_member");
    }
    fn cout(&mut self, _text: &str) {
        self.deny("cout");
    }
    fn receive_value(&mut self, _org: &mut OrgState) -> i32 {
        self.deny("receive_value");
        Default::default()
    }
    fn inject_parasite(
        &mut self,
        _host: &mut OrgState,
        _host_hw: &mut dyn Hardware,
        _parent: Option<Arc<ParasiteUnit>>,
        _label: &str,
        _injected_code: &InstructionSequence,
    ) -> bool {
        self.deny("inject_parasite");
        Default::default()
    }
    fn update_merit(&mut self, _org: &mut OrgState, _new_merit: f64) -> bool {
        self.deny("update_merit");
        Default::default()
    }
    fn test_on_divide(&self) -> bool {
        self.deny("test_on_divide");
        Default::default()
    }
    fn send_message(&mut self, _msg: &mut OrgMessage) -> bool {
        self.deny("send_message");
        Default::default()
    }
    fn broadcast_message(&mut self, _msg: &mut OrgMessage, _depth: i32) -> bool {
        self.deny("broadcast_message");
        Default::default()
    }
    fn bcast_alarm(&mut self, _jump_label: i32, _bcast_range: i32) -> bool {
        self.deny("bcast_alarm");
        Default::default()
    }
    fn divide_org_testament_among_deme(&mut self, _value: f64) {
        self.deny("divide_org_testament_among_deme");
    }
    fn send_flash(&mut self) {
        self.deny("send_flash");
    }
    fn get_state_grid_id(&mut self) -> i32 {
        self.deny("get_state_grid_id");
        Default::default()
    }
    fn rotate_to_greatest_reputation(&mut self, _org: &mut OrgState) {
        self.deny("rotate_to_greatest_reputation");
    }
    fn rotate_to_greatest_reputation_with_different_tag(&mut self, _org: &mut OrgState, _tag: i32) {
        self.deny("rotate_to_greatest_reputation_with_different_tag");
    }
    fn rotate_to_greatest_reputation_with_different_lineage(
        &mut self,
        _org: &mut OrgState,
        _line: i32,
    ) {
        self.deny("rotate_to_greatest_reputation_with_different_lineage");
    }
    fn create_link_by_facing(&mut self, _weight: f64) {
        self.deny("create_link_by_facing");
    }
    fn create_link_by_xy(&mut self, _x: i32, _y: i32, _weight: f64) {
        self.deny("create_link_by_xy");
    }
    fn create_link_by_index(&mut self, _idx: i32, _weight: f64) {
        self.deny("create_link_by_index");
    }
    fn network_broadcast(&mut self, _msg: &mut OrgMessage) -> bool {
        self.deny("network_broadcast");
        Default::default()
    }
    fn network_unicast(&mut self, _msg: &mut OrgMessage) -> bool {
        self.deny("network_unicast");
        Default::default()
    }
    fn network_rotate(&mut self, _x: i32) -> bool {
        self.deny("network_rotate");
        Default::default()
    }
    fn network_select(&mut self, _x: i32) -> bool {
        self.deny("network_select");
        Default::default()
    }
    fn get_next_deme_input(&mut self) -> i32 {
        self.deny("get_next_deme_input");
        Default::default()
    }
    fn do_deme_input(&mut self, _value: i32) {
        self.deny("do_deme_input");
    }
    fn do_deme_output(&mut self, _value: i32) {
        self.deny("do_deme_output");
    }
    fn do_hgt_mutation(&mut self, _org: &mut OrgState) {
        self.deny("do_hgt_mutation");
    }
    fn receive_hgt_donation(&mut self, _fragment: &InstructionSequence) {
        self.deny("receive_hgt_donation");
    }
    fn move_org(&mut self, _org: &mut OrgState, _src_id: i32, _dest_id: i32) -> bool {
        self.deny("move_org");
        Default::default()
    }
    fn has_opinion(&self, _org: &OrgState) -> bool {
        self.deny("has_opinion");
        Default::default()
    }
    fn set_opinion(&mut self, _opinion: i32, _org: &mut OrgState) {
        self.deny("set_opinion");
    }
    fn clear_opinion(&mut self, _org: &mut OrgState) {
        self.deny("clear_opinion");
    }
    fn join_group(&mut self, _org: &mut OrgState, _group_id: i32) {
        self.deny("join_group");
    }
    fn make_group(&mut self, _org: &mut OrgState) {
        self.deny("make_group");
    }
    fn leave_group(&mut self, _org: &mut OrgState, _group_id: i32) {
        self.deny("leave_group");
    }
    fn number_of_organisms_in_group(&mut self, _group_id: i32) -> i32 {
        self.deny("number_of_organisms_in_group");
        Default::default()
    }
    fn number_group_females(&mut self, _group_id: i32) -> i32 {
        self.deny("number_group_females");
        Default::default()
    }
    fn number_group_males(&mut self, _group_id: i32) -> i32 {
        self.deny("number_group_males");
        Default::default()
    }
    fn number_group_juvs(&mut self, _group_id: i32) -> i32 {
        self.deny("number_group_juvs");
        Default::default()
    }
    fn change_group_mating_types(
        &mut self,
        _org: &mut OrgState,
        _group_id: i32,
        _old_type: i32,
        _new_type: i32,
    ) {
        self.deny("change_group_mating_types");
    }
    fn inc_tolerance(&mut self, _org: &mut OrgState, _tolerance_type: i32) -> i32 {
        self.deny("inc_tolerance");
        Default::default()
    }
    fn dec_tolerance(&mut self, _org: &mut OrgState, _tolerance_type: i32) -> i32 {
        self.deny("dec_tolerance");
        Default::default()
    }
    fn calc_group_tolerance_immigrants(
        &mut self,
        _org: &mut OrgState,
        _target_group_id: i32,
        _mating_type: i32,
    ) -> i32 {
        self.deny("calc_group_tolerance_immigrants");
        Default::default()
    }
    fn calc_group_tolerance_offspring(&mut self, _parent: &mut OrgState) -> i32 {
        self.deny("calc_group_tolerance_offspring");
        Default::default()
    }
    fn calc_group_odds_immigrants(
        &mut self,
        _org: &mut OrgState,
        _group_id: i32,
        _mating_type: i32,
    ) -> f64 {
        self.deny("calc_group_odds_immigrants");
        Default::default()
    }
    fn calc_group_odds_offspring_parent(&mut self, _parent: &mut OrgState) -> f64 {
        self.deny("calc_group_odds_offspring_parent");
        Default::default()
    }
    fn calc_group_odds_offspring(&mut self, _org: &mut OrgState, _group_id: i32) -> f64 {
        self.deny("calc_group_odds_offspring");
        Default::default()
    }
    fn attempt_immigrate_group(&mut self, _group_id: i32, _org: &mut OrgState) -> bool {
        self.deny("attempt_immigrate_group");
        Default::default()
    }
    fn push_tolerance_inst_exe(&mut self, _org: &mut OrgState, _tol_inst: i32) {
        self.deny("push_tolerance_inst_exe");
    }
    fn get_group_intolerances(&mut self, _group_id: i32, _tol_num: i32, _mating_type: i32) -> i32 {
        self.deny("get_group_intolerances");
        Default::default()
    }
    fn set_group_intolerances(
        &mut self,
        _group_id: i32,
        _tol_num: i32,
        _mating_type: i32,
        _value: i32,
    ) {
        self.deny("set_group_intolerances");
    }
    fn try_write_group_attack_bits(&mut self, _raw_bits: u8) {
        self.deny("try_write_group_attack_bits");
    }
    fn try_write_group_attack_string(&mut self, _s: &str) {
        self.deny("try_write_group_attack_string");
    }
    fn dec_num_prey_organisms(&mut self) {
        self.deny("dec_num_prey_organisms");
    }
    fn dec_num_pred_organisms(&mut self) {
        self.deny("dec_num_pred_organisms");
    }
    fn dec_num_top_pred_organisms(&mut self) {
        self.deny("dec_num_top_pred_organisms");
    }
    fn inc_num_prey_organisms(&mut self) {
        self.deny("inc_num_prey_organisms");
    }
    fn inc_num_pred_organisms(&mut self) {
        self.deny("inc_num_pred_organisms");
    }
    fn inc_num_top_pred_organisms(&mut self) {
        self.deny("inc_num_top_pred_organisms");
    }
    fn attack_faced_org(&mut self, _org: &mut OrgState, _loser: i32) {
        self.deny("attack_faced_org");
    }
    fn try_write_birth_loc_data(&mut self, _org: &OrgState) {
        self.deny("try_write_birth_loc_data");
    }
    fn inject_prey_clone(&mut self, _org: &mut OrgState) {
        self.deny("inject_prey_clone");
    }
    fn kill_rand_pred(&mut self, _org: &mut OrgState) {
        self.deny("kill_rand_pred");
    }
    fn kill_rand_prey(&mut self, _org: &mut OrgState) {
        self.deny("kill_rand_prey");
    }
    fn try_write_look_data(&mut self, _s: &str) {
        self.deny("try_write_look_data");
    }
    fn try_write_look_output(&mut self, _s: &str) {
        self.deny("try_write_look_output");
    }
    fn try_write_look_ex_output(&mut self, _s: &str) {
        self.deny("try_write_look_ex_output");
    }
    fn get_formed_group_array(&mut self) -> Vec<i32> {
        self.deny("get_formed_group_array");
        Default::default()
    }
    fn has_output_av(&self, _av_num: i32) -> bool {
        self.deny("has_output_av");
        Default::default()
    }
    fn faced_has_output_av(&self, _av_num: i32) -> bool {
        self.deny("faced_has_output_av");
        Default::default()
    }
    fn faced_has_av(&self, _av_num: i32) -> bool {
        self.deny("faced_has_av");
        Default::default()
    }
    fn faced_has_pred_av(&self, _av_num: i32) -> bool {
        self.deny("faced_has_pred_av");
        Default::default()
    }
    fn faced_has_prey_av(&self, _av_num: i32) -> bool {
        self.deny("faced_has_prey_av");
        Default::default()
    }
    fn add_io_av(&mut self, _av_cell_id: i32, _av_facing: i32, _input: bool, _output: bool) {
        self.deny("add_io_av");
    }
    fn add_pred_prey_av(&mut self, _org: &OrgState, _av_cell_id: i32) {
        self.deny("add_pred_prey_av");
    }
    fn switch_pred_prey(&mut self, _av_num: i32) {
        self.deny("switch_pred_prey");
    }
    fn remove_all_av(&mut self) {
        self.deny("remove_all_av");
    }
    fn get_av_facing(&self, _av_num: i32) -> i32 {
        self.deny("get_av_facing");
        Default::default()
    }
    fn get_av_cell_id(&self, _av_num: i32) -> i32 {
        self.deny("get_av_cell_id");
        Default::default()
    }
    fn get_av_faced_cell_id(&self, _av_num: i32) -> i32 {
        self.deny("get_av_faced_cell_id");
        Default::default()
    }
    fn get_av_num_neighbors(&self, _av_num: i32) -> i32 {
        self.deny("get_av_num_neighbors");
        Default::default()
    }
    fn get_av_faced_data(&self, _av_num: i32) -> i32 {
        self.deny("get_av_faced_data");
        Default::default()
    }
    fn get_av_faced_data_org_id(&self, _av_num: i32) -> i32 {
        self.deny("get_av_faced_data_org_id");
        Default::default()
    }
    fn get_av_faced_data_update(&self, _av_num: i32) -> i32 {
        self.deny("get_av_faced_data_update");
        Default::default()
    }
    fn get_av_faced_data_territory(&self, _av_num: i32) -> i32 {
        self.deny("get_av_faced_data_territory");
        Default::default()
    }
    fn get_av_data(&self, _av_num: i32) -> i32 {
        self.deny("get_av_data");
        Default::default()
    }
    fn get_av_data_org_id(&self, _av_num: i32) -> i32 {
        self.deny("get_av_data_org_id");
        Default::default()
    }
    fn get_av_data_update(&self, _av_num: i32) -> i32 {
        self.deny("get_av_data_update");
        Default::default()
    }
    fn get_av_data_territory(&self, _av_num: i32) -> i32 {
        self.deny("get_av_data_territory");
        Default::default()
    }
    fn find_av(&self, _input: bool, _output: bool, _av_num: i32) -> i32 {
        self.deny("find_av");
        Default::default()
    }
    fn set_av_facing(&mut self, _av_facing: i32, _av_num: i32) {
        self.deny("set_av_facing");
    }
    fn set_av_cell_id(&mut self, _av_cell_id: i32, _av_num: i32) -> bool {
        self.deny("set_av_cell_id");
        Default::default()
    }
    fn set_av_faced_cell_id(&mut self, _av_num: i32) {
        self.deny("set_av_faced_cell_id");
    }
    fn set_av_cell_data(
        &mut self,
        _org: &mut OrgState,
        _new_data: i32,
        _org_id: i32,
        _av_num: i32,
    ) {
        self.deny("set_av_cell_data");
    }
    fn move_av(&mut self, _org: &mut OrgState, _av_num: i32) -> bool {
        self.deny("move_av");
        Default::default()
    }
    fn rotate_av(&mut self, _increment: i32, _av_num: i32) -> bool {
        self.deny("rotate_av");
        Default::default()
    }
    fn get_rand_faced_av(&mut self, _av_num: i32) -> Option<OrgUid> {
        self.deny("get_rand_faced_av");
        Default::default()
    }
    fn get_rand_faced_pred_av(&mut self, _av_num: i32) -> Option<OrgUid> {
        self.deny("get_rand_faced_pred_av");
        Default::default()
    }
    fn get_rand_faced_prey_av(&mut self, _av_num: i32) -> Option<OrgUid> {
        self.deny("get_rand_faced_prey_av");
        Default::default()
    }
    fn get_faced_avs(&mut self, _av_num: i32) -> Vec<OrgUid> {
        self.deny("get_faced_avs");
        Default::default()
    }
    fn get_cell_avs(&mut self, _av_cell_id: i32, _av_num: i32) -> Vec<OrgUid> {
        self.deny("get_cell_avs");
        Default::default()
    }
    fn get_faced_prey_avs(&mut self, _av_num: i32) -> Vec<OrgUid> {
        self.deny("get_faced_prey_avs");
        Default::default()
    }
    fn get_av_resources(&mut self, _org: &mut OrgState, _av_num: i32) -> Vec<f64> {
        self.deny("get_av_resources");
        Default::default()
    }
    fn get_av_resource_val(&mut self, _org: &mut OrgState, _res_id: i32, _av_num: i32) -> f64 {
        self.deny("get_av_resource_val");
        Default::default()
    }
    fn get_av_faced_resources(&mut self, _org: &mut OrgState, _av_num: i32) -> Vec<f64> {
        self.deny("get_av_faced_resources");
        Default::default()
    }
    fn get_av_faced_resource_val(
        &mut self,
        _org: &mut OrgState,
        _res_id: i32,
        _av_num: i32,
    ) -> f64 {
        self.deny("get_av_faced_resource_val");
        Default::default()
    }
    fn update_av_resources(&mut self, _org: &mut OrgState, _res_change: &[f64], _av_num: i32) {
        self.deny("update_av_resources");
    }
    fn seen_org(&mut self, _uid: OrgUid) -> Option<SeenOrg> {
        self.deny("seen_org");
        Default::default()
    }
    fn cell_av_counts(&self, _cell_id: i32) -> (i32, i32) {
        self.deny("cell_av_counts");
        Default::default()
    }
    fn cell_rand_av(&mut self, _cell_id: i32) -> Option<OrgUid> {
        self.deny("cell_rand_av");
        Default::default()
    }
    fn cell_first_pred_av(&self, _cell_id: i32) -> Option<OrgUid> {
        self.deny("cell_first_pred_av");
        Default::default()
    }
    fn cell_first_prey_av(&self, _cell_id: i32) -> Option<OrgUid> {
        self.deny("cell_first_prey_av");
        Default::default()
    }
    fn cell_prey_avs(&self, _cell_id: i32) -> Vec<OrgUid> {
        self.deny("cell_prey_avs");
        Default::default()
    }
    fn cell_neighborhood_ids(&self, _cell_id: i32) -> Vec<i32> {
        self.deny("cell_neighborhood_ids");
        Default::default()
    }
    fn resource_used_bounds(&self, _res_id: i32) -> (i32, i32, i32, i32) {
        self.deny("resource_used_bounds");
        Default::default()
    }
    fn group_attack_inst_index(&self, _inst_set: &str, _inst: &str) -> Option<usize> {
        self.deny("group_attack_inst_index");
        Default::default()
    }
    fn begin_sleep(&mut self) {
        self.deny("begin_sleep");
    }
    fn end_sleep(&mut self) {
        self.deny("end_sleep");
    }
    fn cell_deme_id(&self, _cell_id: i32) -> i32 {
        self.deny("cell_deme_id");
        Default::default()
    }
    fn get_deme_cell_resources(&mut self, _deme_id: i32, _cell_id: i32) -> Vec<f64> {
        self.deny("get_deme_cell_resources");
        Default::default()
    }
    fn deme_resource_amount(&mut self, _deme_id: i32, _res_index: i32) -> f64 {
        self.deny("deme_resource_amount");
        Default::default()
    }
    fn population_resource_amount(&mut self, _org: &mut OrgState, _res_index: i32) -> f64 {
        self.deny("population_resource_amount");
        Default::default()
    }
    fn deme_add_pheromone(&mut self, _deme_id: i32, _cell_id: i32, _value: f64) {
        self.deny("deme_add_pheromone");
    }
    fn deme_cell_position(&self, _deme_id: i32, _cell_id: i32) -> (i32, i32) {
        self.deny("deme_cell_position");
        Default::default()
    }
    fn update_cell_resources(&mut self, _org: &mut OrgState, _res_change: &[f64], _cell_id: i32) {
        self.deny("update_cell_resources");
    }
    fn stats_move(&mut self, _org: &OrgState) {
        self.deny("stats_move");
    }
    fn population_cell_data(&self, _cell_id: i32) -> i32 {
        self.deny("population_cell_data");
        Default::default()
    }
    fn cell_organism_id(&self, _cell_id: i32) -> i32 {
        self.deny("cell_organism_id");
        Default::default()
    }
    fn cell_organism_uid(&self, _cell_id: i32) -> Option<OrgUid> {
        self.deny("cell_organism_uid");
        Default::default()
    }
    fn update_cell_merit(&mut self, _org: &mut OrgState, _cell_id: i32, _new_merit: f64) -> bool {
        self.deny("update_cell_merit");
        Default::default()
    }
    fn deme_increase_energy_donated(&mut self, _deme_id: i32, _amount: f64) {
        self.deny("deme_increase_energy_donated");
    }
    fn deme_increase_energy_received(&mut self, _deme_id: i32, _amount: f64) {
        self.deny("deme_increase_energy_received");
    }
    fn turn_cell_to_face(&mut self, _cell_id: i32, _target_cell_id: i32) {
        self.deny("turn_cell_to_face");
    }
    fn stats_inc_donate_to_donor(&mut self) {
        self.deny("stats_inc_donate_to_donor");
    }
    fn stats_log_ret_message(&mut self, _msg: &OrgMessage) {
        self.deny("stats_log_ret_message");
    }
    fn stats_sent_flash(&mut self, _org: &OrgState) {
        self.deny("stats_sent_flash");
    }
    fn deme_kill_cell_event(&mut self, _org: &mut OrgState, _deme_id: i32, _event_id: i32) -> bool {
        self.deny("deme_kill_cell_event");
        Default::default()
    }
    fn stats_creature_age_mean_stddev(&self) -> (f64, f64) {
        self.deny("stats_creature_age_mean_stddev");
        Default::default()
    }
    fn kill_group_member(&mut self, _group_id: i32, _org: &mut OrgState) {
        self.deny("kill_group_member");
    }
    fn deme_add_resources_consumed(&mut self, _deme_id: i32, _amount: f64) {
        self.deny("deme_add_resources_consumed");
    }
    fn deme_replicate(&mut self, _deme_id: i32) {
        self.deny("deme_replicate");
    }
    fn deme_record_suicide(&mut self, _deme_id: i32, _points: i32) {
        self.deny("deme_record_suicide");
    }
    fn deme_increase_total_energy_testament(&mut self, _deme_id: i32, _amount: f64) {
        self.deny("deme_increase_total_energy_testament");
    }
    fn stats_add_energy_testament(&mut self, _target: TestamentTarget, _amount: f64) {
        self.deny("stats_add_energy_testament");
    }
    fn stats_inc_dont_explode(&mut self) {
        self.deny("stats_inc_dont_explode");
    }
    fn stats_lyse(&mut self, _percent_prob: f64, _cpu_cycles: i32) {
        self.deny("stats_lyse");
    }
    fn stats_inc_quorum(&mut self, _org_ratio: i32) {
        self.deny("stats_inc_quorum");
    }
    fn stats_inc_guard_fail(&mut self) {
        self.deny("stats_inc_guard_fail");
    }
    fn stats_num_prey_creatures(&self) -> Result<i32, String> {
        Err(self.denied("stats_num_prey_creatures"))
    }
    fn stats_num_total_pred_creatures(&self) -> Result<i32, String> {
        Err(self.denied("stats_num_total_pred_creatures"))
    }
    fn stats_num_pred_creatures(&self) -> Result<i32, String> {
        Err(self.denied("stats_num_pred_creatures"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lib_index(name: &str) -> i32 {
        CPU_INST_DEFS
            .iter()
            .position(|d| d.name == name)
            .unwrap_or_else(|| panic!("no instruction {name}")) as i32
    }

    #[test]
    fn allow_list_names_real_handlers() {
        for m in CPU_PARALLEL_METHODS {
            assert!(
                CPU_INST_DEFS.iter().any(|d| d.method == *m),
                "{m} is not a heads-CPU handler"
            );
        }
        let mut sorted = CPU_PARALLEL_METHODS.to_vec();
        sorted.sort_unstable();
        sorted.dedup();
        assert_eq!(sorted.len(), CPU_PARALLEL_METHODS.len(), "duplicates");
    }

    #[test]
    fn world_instructions_are_not_organism_local() {
        // The default instruction set: everything runs in bursts except
        // the input/output and the divide.
        for name in [
            "nop-A", "nop-B", "nop-C", "if-n-equ", "if-less", "if-label", "mov-head", "jmp-head",
            "get-head", "set-flow", "shift-r", "shift-l", "inc", "dec", "push", "pop", "swap-stk",
            "swap", "add", "sub", "nand", "h-copy", "h-alloc", "h-search",
        ] {
            assert!(cpu_parallel_ok(lib_index(name)), "{name}");
        }
        for name in [
            "IO",
            "h-divide",
            "divide",
            "get",
            "rotate-l",
            "move",
            "sense",
            "donate-rnd",
        ] {
            assert!(!cpu_parallel_ok(lib_index(name)), "{name}");
        }
        assert!(!cpu_parallel_ok(-1));
        assert!(!cpu_parallel_ok(CPU_INST_DEFS.len() as i32));
    }

    #[test]
    fn streams_depend_on_seed_and_key_only() {
        let draws = |mut r: AvidaRng| (0..8).map(|_| r.get_uint(1 << 30)).collect::<Vec<_>>();
        assert_eq!(draws(stream(42, 7)), draws(stream(42, 7)));
        assert_ne!(draws(stream(42, 7)), draws(stream(42, 8)));
        assert_ne!(draws(stream(42, 7)), draws(stream(43, 7)));
        assert_eq!(draws(org_stream(5, OrgUid(3))), draws(stream(5, 3)));
    }
}
