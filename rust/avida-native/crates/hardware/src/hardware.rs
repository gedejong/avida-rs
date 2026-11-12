//! Port of `cHardwareBase` (avida-core/source/cpu/cHardwareBase.{h,cc} at
//! 47f13dadb): the [`Hardware`] trait every CPU type implements, the shared
//! state ([`HardwareBase`]: instruction costs, extended memory, tracer) and
//! the hardware-independent mutation and divide helpers.
//!
//! The C++ base class reaches its organism through `m_organism` and the
//! world through `m_world`; here both are passed in (`org: &mut OrgState`,
//! `world: &mut dyn OrgInterface`), as ARCHITECTURE.md prescribes.

use crate::head_cpu::HeadCpu;
use crate::nhardware::*;
use crate::org_interface::OrgInterface;
use crate::organism::OrgState;
use crate::tracer::HardwareTracerPtr;
use avida_apto::AvidaRng;
use avida_config::config::Config;
use avida_genome::{CpuMemory, InstSet, Instruction, InstructionSequence};
use std::any::Any;
use std::sync::Arc;

/// Where one head of a thread points, for inspection: the memory space id
/// (the ids [`Hardware::memory_spaces`] reports) and the position in it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct HeadSite {
    /// The memory space.
    pub mem_space: i32,
    /// The position in it.
    pub pos: i32,
}

/// A hardware type (`cHardwareBase` virtuals).
///
/// Methods with a body are the `cHardwareBase` defaults.
pub trait Hardware: Send {
    /// Downcast support (`InheritState` casts its argument).
    fn as_any(&self) -> &dyn Any;
    /// Mutable downcast support.
    fn as_any_mut(&mut self) -> &mut dyn Any;

    /// Shared `cHardwareBase` state.
    fn base(&self) -> &HardwareBase;
    /// Shared `cHardwareBase` state, mutable.
    fn base_mut(&mut self) -> &mut HardwareBase;

    /// `GetType()`
    fn hw_type(&self) -> i32;
    /// `SupportsSpeculative()`
    fn supports_speculative(&self) -> bool;
    /// `GetInstSet()`
    fn inst_set(&self) -> &Arc<InstSet> {
        &self.base().inst_set
    }

    /// `Reset(ctx)`: `m_organism->HardwareReset(ctx)`, cost tables,
    /// `internalReset()`.
    fn reset(&mut self, org: &mut OrgState, world: &mut dyn OrgInterface);

    /// `SingleProcess(ctx, speculative)`: `Ok(false)` when the organism
    /// should stop (died, or a speculative step was rejected). `Err` only
    /// for an unported feature reached at run time.
    fn single_process(
        &mut self,
        org: &mut OrgState,
        world: &mut dyn OrgInterface,
        speculative: bool,
    ) -> Result<bool, String>;

    /// `ProcessBonusInst(ctx, inst)`.
    fn process_bonus_inst(
        &mut self,
        org: &mut OrgState,
        world: &mut dyn OrgInterface,
        inst: Instruction,
    ) -> Result<(), String>;

    /// `PrintStatus(fp)`.
    fn print_status(&self, org: &OrgState, world: &mut dyn OrgInterface) -> String;
    /// `PrintMiniTraceStatus(ctx, fp)`.
    fn print_mini_trace_status(
        &self,
        _org: &mut OrgState,
        _world: &mut dyn OrgInterface,
    ) -> String {
        String::new()
    }
    /// `PrintMiniTraceSuccess(fp, exec_success)`.
    fn print_mini_trace_success(&self, _exec_success: i32) -> String {
        String::new()
    }

    /// `SetTrace(tracer)`.
    fn set_trace(&mut self, tracer: Option<HardwareTracerPtr>) {
        self.base_mut().tracer = tracer;
    }

    // --------  Stack Manipulation  --------
    /// `GetStack(depth, stack_id, in_thread)`.
    fn get_stack(&self, depth: i32, stack_id: i32, in_thread: i32) -> i32;
    /// `GetCurStack(in_thread)`.
    fn get_cur_stack(&self, _in_thread: i32) -> i32 {
        -1
    }
    /// `GetNumStacks()`.
    fn num_stacks(&self) -> i32;

    // --------  Head Manipulation  --------
    /// `GetHead(head_id)` of the current thread.
    fn head(&self, head_id: usize) -> &HeadCpu;
    /// `GetHead(head_id)` of the current thread, mutable.
    fn head_mut(&mut self, head_id: usize) -> &mut HeadCpu;
    /// `GetHead(head_id, thread)`.
    fn head_in_thread(&self, head_id: usize, thread: usize) -> &HeadCpu;
    /// `GetNumHeads()`.
    fn num_heads(&self) -> i32;
    /// `IP()`.
    fn ip(&self) -> &HeadCpu {
        self.head(HEAD_IP)
    }
    /// Every head of `thread` in `GetHead` order (IP first), for read-only
    /// inspection. Hardware without `cHeadCPU`s (BCR, GP8: `GetNumHeads()`
    /// is 0) overrides this with its own heads.
    fn thread_heads(&self, thread: usize) -> Vec<HeadSite> {
        (0..self.num_heads().max(0) as usize)
            .map(|h| {
                let head = self.head_in_thread(h, thread);
                HeadSite {
                    mem_space: head.mem_space(),
                    pos: head.position(),
                }
            })
            .collect()
    }

    // --------  Memory Manipulation  --------
    /// `GetMemory()`.
    fn memory(&self) -> &CpuMemory;
    /// `GetMemory()`, mutable.
    fn memory_mut(&mut self) -> &mut CpuMemory;
    /// `GetMemSize()`.
    fn mem_size(&self) -> i32 {
        self.memory().size()
    }
    /// `GetNumMemSpaces()`.
    fn num_mem_spaces(&self) -> i32 {
        1
    }
    /// Every memory space in use as `(space id, memory)`, for read-only
    /// inspection (the debugger). Hardware with one memory returns just
    /// space 0.
    fn memory_spaces(&self) -> Vec<(i32, &CpuMemory)> {
        vec![(0, self.memory())]
    }
    /// What the hardware knows about memory space `id` (one of
    /// [`Hardware::memory_spaces`]), as a short label; `None` leaves the
    /// naming to the caller.
    fn memory_space_label(&self, _id: i32) -> Option<String> {
        None
    }
    /// `GetExtendedMemory()`.
    fn ext_mem(&self) -> &[i32] {
        &self.base().ext_mem
    }
    /// `SetupExtendedMemory(ext_mem)`.
    fn setup_extended_memory(&mut self, ext_mem: Vec<i32>) {
        self.base_mut().ext_mem = ext_mem;
    }

    // --------  Register Manipulation  --------
    /// `GetRegister(reg_id)` of the current thread.
    fn register(&self, reg_id: usize) -> i32;
    /// `GetNumRegisters()`.
    fn num_registers(&self) -> i32;
    /// Register `reg_id` of `thread`; `None` when the hardware only reads
    /// the current thread's registers and `thread` is another one.
    fn register_in_thread(&self, reg_id: usize, thread: usize) -> Option<i32> {
        (thread as i32 == self.cur_thread()).then(|| self.register(reg_id))
    }

    // --------  Thread Manipulation  --------
    /// `GetNumThreads()`.
    fn num_threads(&self) -> i32;
    /// `GetCurThread()`.
    fn cur_thread(&self) -> i32;
    /// `InterruptThread(interruptType)`.
    fn interrupt_thread(
        &mut self,
        _org: &mut OrgState,
        _world: &mut dyn OrgInterface,
        _interrupt_type: i32,
    ) -> Result<bool, String> {
        Ok(false)
    }
    /// `GetThreadMessageTriggerType(index)`.
    fn thread_message_trigger_type(&self, _index: i32) -> i32 {
        -1
    }
    /// A short description of the state of `thread` for the debugger (for
    /// example `waiting for BX > 3 into CX`); empty when there is nothing
    /// to say.
    fn thread_status(&self, thread: usize) -> String {
        match self.thread_message_trigger_type(thread as i32) {
            t if t >= 0 => format!("message trigger {t}"),
            _ => String::new(),
        }
    }

    // --------  Mutation  --------
    /// `PointMutate(ctx, override_mut_rate)`.
    fn point_mutate(
        &mut self,
        org: &OrgState,
        world: &mut dyn OrgInterface,
        override_mut_rate: f64,
    ) -> i32 {
        let inst_set = self.inst_set().clone();
        point_mutate(&inst_set, org, world, self.memory_mut(), override_mut_rate)
    }

    // --------  State Transfer  --------
    /// `InheritState(in_hardware)`.
    fn inherit_state(&mut self, _parent: &dyn Hardware) {}

    // --------  Alarm  --------
    /// `Jump_To_Alarm_Label(jump_label)`: move the IP to the next
    /// `alarm-label-high` (1) / `-low` (0) unless the organism sleeps. An
    /// instruction set without the label exits the C++ run (the `Err`).
    fn jump_to_alarm_label(&mut self, _org: &OrgState, _jump_label: i32) -> Result<bool, String> {
        Ok(false)
    }

    // -------- Synchronization --------
    /// `ReceiveFlash()`: the base version calls `exit(1)`.
    fn receive_flash(&mut self) -> Result<(), String> {
        Err("cHardwareBase::ReceiveFlash must be overridden (C++ exits with status 1)".to_string())
    }

    // -------- HGT --------
    /// `GetGenomeFragment(downstream)`: copy `downstream` instructions
    /// from the read head on.
    fn get_genome_fragment(&self, downstream: u32) -> InstructionSequence {
        let mut tmp = *self.head(HEAD_READ);
        let mem = self.memory();
        let mut fragment = InstructionSequence::new();
        let mut d = downstream;
        while d > 0 {
            fragment.append(tmp.get_inst(mem));
            d -= 1;
            tmp.advance(mem.size(), self.num_mem_spaces());
        }
        fragment
    }

    /// `InsertGenomeFragment(fragment)`: insert at the write head.
    fn insert_genome_fragment(&mut self, fragment: &InstructionSequence) {
        let pos = self.head(HEAD_WRITE).position();
        let nms = self.num_mem_spaces();
        let mem = self.memory_mut();
        if pos >= 0 && pos <= mem.size() {
            mem.insert_seq(pos, fragment);
        }
        let size = self.memory().size();
        self.head_mut(HEAD_WRITE).adjust(size, nms);
    }

    // --------  Traces  --------
    /// `SetMicroTrace()`.
    fn set_micro_trace(&mut self) {
        self.base_mut().microtrace = true;
    }
    /// `GetMicroTrace()`.
    fn micro_trace(&self) -> &[u8] {
        &self.base().microtracer
    }
    /// `SetTopNavTrace(b)`.
    fn set_top_nav_trace(&mut self, b: bool) {
        self.base_mut().topnavtrace = b;
    }
    /// `IsTopNavTrace()`.
    fn is_top_nav_trace(&self) -> bool {
        self.base().topnavtrace
    }
    /// `SetReproTrace(b)`.
    fn set_repro_trace(&mut self, b: bool) {
        self.base_mut().reprotrace = b;
    }
    /// `IsReproTrace()`.
    fn is_repro_trace(&self) -> bool {
        self.base().reprotrace
    }
    /// `GetNavTraceLoc()` / `GetNavTraceFacing()` / `GetNavTraceUpdate()`.
    fn nav_trace(&self) -> (&[i32], &[i32], &[i32]) {
        let b = self.base();
        (&b.navtraceloc, &b.navtracefacing, &b.navtraceupdate)
    }
}

/// `cHardwareBase` data members.
#[derive(Clone)]
pub struct HardwareBase {
    /// `m_inst_set`
    pub inst_set: Arc<InstSet>,
    /// `m_tracer`
    pub tracer: Option<HardwareTracerPtr>,
    /// `m_microtracer`
    pub microtracer: Vec<u8>,
    /// `m_navtraceloc`
    pub navtraceloc: Vec<i32>,
    /// `m_navtracefacing`
    pub navtracefacing: Vec<i32>,
    /// `m_navtraceupdate`
    pub navtraceupdate: Vec<i32>,
    /// `m_minitrace`
    pub minitrace: bool,
    /// `m_microtrace`
    pub microtrace: bool,
    /// `m_topnavtrace`
    pub topnavtrace: bool,
    /// `m_reprotrace`
    pub reprotrace: bool,

    /// `m_inst_cost`
    pub inst_cost: i32,
    /// `m_female_cost`
    pub female_cost: i32,
    /// `m_inst_ft_cost`
    pub inst_ft_cost: Vec<i32>,
    /// `m_inst_energy_cost`
    pub inst_energy_cost: Vec<f64>,
    /// `m_inst_res_cost`
    pub inst_res_cost: Vec<f64>,
    /// `m_inst_fem_res_cost`
    pub inst_fem_res_cost: Vec<f64>,
    /// `m_inst_bonus_cost`
    pub inst_bonus_cost: Vec<f64>,
    /// `m_thread_inst_cost`
    pub thread_inst_cost: Vec<i32>,
    /// `m_thread_inst_post_cost`
    pub thread_inst_post_cost: Vec<i32>,
    /// `m_active_thread_costs`
    pub active_thread_costs: Vec<i32>,
    /// `m_active_thread_post_costs`
    pub active_thread_post_costs: Vec<i32>,

    /// `m_has_any_costs`
    pub has_any_costs: bool,
    /// `m_has_costs`
    pub has_costs: bool,
    /// `m_has_ft_costs`
    pub has_ft_costs: bool,
    /// `m_has_energy_costs`
    pub has_energy_costs: bool,
    /// `m_has_res_costs`
    pub has_res_costs: bool,
    /// `m_has_fem_res_costs`
    pub has_fem_res_costs: bool,
    /// `m_has_female_costs`
    pub has_female_costs: bool,
    /// `m_has_choosy_female_costs`
    pub has_choosy_female_costs: bool,
    /// `m_has_post_costs`
    pub has_post_costs: bool,
    /// `m_has_bonus_costs`
    pub has_bonus_costs: bool,

    /// `m_task_switching_cost`
    pub task_switching_cost: i32,
    /// `m_ext_mem`
    pub ext_mem: Vec<i32>,
    /// `m_implicit_repro_active`
    pub implicit_repro_active: bool,
}

impl std::fmt::Debug for HardwareBase {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("HardwareBase")
            .field("inst_set", &self.inst_set.inst_set_name())
            .field("has_any_costs", &self.has_any_costs)
            .field("ext_mem", &self.ext_mem)
            .finish()
    }
}

impl HardwareBase {
    /// `cHardwareBase(world, organism, inst_set)`.
    pub fn new(cfg: &Config, inst_set: Arc<InstSet>) -> Self {
        let has_costs = inst_set.has_costs();
        let has_ft_costs = inst_set.has_ft_costs();
        let has_energy_costs = inst_set.has_energy_costs();
        let has_res_costs = inst_set.has_res_costs();
        let has_fem_res_costs = inst_set.has_fem_res_costs();
        let has_female_costs = inst_set.has_female_costs();
        let has_choosy_female_costs = inst_set.has_choosy_female_costs();
        let has_post_costs = inst_set.has_post_costs();
        let has_bonus_costs = inst_set.has_bonus_costs();
        let switch_cost = cfg.task_switch_penalty;
        let has_any_costs = has_costs
            | has_ft_costs
            | has_energy_costs
            | has_res_costs
            | has_fem_res_costs
            | (switch_cost != 0)
            | has_female_costs
            | has_choosy_female_costs
            | has_post_costs
            | has_bonus_costs;
        let implicit_repro_active = cfg.implicit_repro_time != 0
            || cfg.implicit_repro_cpu_cycles != 0
            || cfg.implicit_repro_bonus != 0
            || cfg.implicit_repro_end != 0
            || cfg.implicit_repro_energy != 0.0;
        HardwareBase {
            inst_set,
            tracer: None,
            microtracer: Vec::new(),
            navtraceloc: Vec::new(),
            navtracefacing: Vec::new(),
            navtraceupdate: Vec::new(),
            minitrace: false,
            microtrace: false,
            topnavtrace: false,
            reprotrace: false,
            inst_cost: 0,
            female_cost: 0,
            inst_ft_cost: Vec::new(),
            inst_energy_cost: Vec::new(),
            inst_res_cost: Vec::new(),
            inst_fem_res_cost: Vec::new(),
            inst_bonus_cost: Vec::new(),
            thread_inst_cost: Vec::new(),
            thread_inst_post_cost: Vec::new(),
            active_thread_costs: Vec::new(),
            active_thread_post_costs: Vec::new(),
            has_any_costs,
            has_costs,
            has_ft_costs,
            has_energy_costs,
            has_res_costs,
            has_fem_res_costs,
            has_female_costs,
            has_choosy_female_costs,
            has_post_costs,
            has_bonus_costs,
            task_switching_cost: 0,
            ext_mem: Vec::new(),
            implicit_repro_active,
        }
    }

    /// The part of `Reset(ctx)` after `m_organism->HardwareReset(ctx)` and
    /// before `internalReset()`: clear the traces and rebuild the cost
    /// tables.
    pub fn reset_costs(&mut self, cfg: &Config) {
        self.microtracer.clear();
        self.navtraceloc.clear();
        self.navtracefacing.clear();
        self.navtraceupdate.clear();
        self.inst_cost = 0;
        self.resize_cost_arrays(cfg.max_cpu_threads);
        self.female_cost = 0;

        let is = &self.inst_set;
        let n = is.size();
        let insts = || (0..n).map(Instruction::new);
        if self.has_ft_costs {
            self.inst_ft_cost = insts().map(|i| is.ft_cost(i)).collect();
        }
        if self.has_energy_costs {
            self.inst_energy_cost = insts().map(|i| f64::from(is.energy_cost(i))).collect();
        }
        if self.has_res_costs {
            self.inst_res_cost = insts().map(|i| is.res_cost(i)).collect();
        }
        if self.has_fem_res_costs {
            self.inst_fem_res_cost = insts().map(|i| is.fem_res_cost(i)).collect();
        }
        if self.has_costs {
            self.thread_inst_cost = insts().map(|i| is.cost(i)).collect();
        }
        if self.has_post_costs {
            self.thread_inst_post_cost = insts().map(|i| is.post_cost(i)).collect();
        }
        if self.has_bonus_costs {
            self.inst_bonus_cost = insts().map(|i| is.bonus_cost(i)).collect();
        }
    }

    /// `ResizeCostArrays(new_size)`.
    pub fn resize_cost_arrays(&mut self, new_size: i32) {
        let n = new_size.max(0) as usize;
        self.active_thread_costs = vec![0; n];
        self.active_thread_post_costs = vec![0; n];
    }

    /// The first-time cost reset of the divide methods
    /// (`m_inst_ft_cost[i] = m_inst_set->GetFTCost(Instruction(i))` over
    /// the current array).
    pub fn reset_ft_costs(&mut self) {
        let is = &self.inst_set;
        for (i, c) in self.inst_ft_cost.iter_mut().enumerate() {
            *c = is.ft_cost(Instruction::new(i as i32));
        }
    }

    /// `IncrementTaskSwitchingCost(cost)` (cHardwareCPU).
    pub fn increment_task_switching_cost(&mut self, cost: i32) {
        self.task_switching_cost = self.task_switching_cost.wrapping_add(cost);
    }

    /// `SingleProcess_PayPreCosts(ctx, cur_inst, thread_id)`.
    pub fn pay_pre_costs(
        &mut self,
        org: &mut OrgState,
        world: &mut dyn OrgInterface,
        cur_inst: Instruction,
        thread_id: usize,
    ) -> bool {
        let op = cur_inst.op() as usize;
        let energy_enabled = world.config().energy_enabled;
        if energy_enabled {
            let cost = self.inst_energy_cost.get(op).copied().unwrap_or(0.0);
            let energy_req = cost * (org.phenotype.merit.get_double() / 100.0);
            if energy_req > 0.0 {
                if org.phenotype.energy_store >= energy_req {
                    if let Some(c) = self.inst_energy_cost.get_mut(op) {
                        *c = 0.0;
                    }
                    org.phenotype.reduce_energy(world.config(), energy_req);
                    if self.inst_set.should_sleep(cur_inst) {
                        // `m_organism->SetSleeping(true)`
                        org.is_sleeping = true;
                        world.begin_sleep();
                    }
                } else {
                    org.phenotype.to_die = true;
                    return false;
                }
            }
        }

        if self.task_switching_cost > 0 {
            self.task_switching_cost -= 1;
            world.deme_inc_num_switching_penalties(1);
            return false;
        }

        if self.has_ft_costs {
            if let Some(c) = self.inst_ft_cost.get_mut(op) {
                if *c > 0 {
                    *c -= 1;
                    return false;
                }
            }
        }

        if self.has_res_costs || self.has_fem_res_costs {
            let res_cost = self.inst_set.res_cost(cur_inst);
            let fem_res_cost = if org.phenotype.mating_type == MATING_TYPE_FEMALE {
                self.inst_set.fem_res_cost(cur_inst)
            } else {
                0.0
            };
            let res_req = res_cost + fem_res_cost;
            // C++ fetches GetResources(ctx) here and ignores it.
            let _ = world.get_resources(org);
            let resource = world.config().collect_specific_resource;
            if resource < 0 {
                println!("Instruction resource costs require use of COLLECT_SPECIFIC_RESOURCE and USE_RESOURCE_BINS");
            }
            let res_stored = org.rbin(resource);
            if res_stored < res_req {
                return false;
            }
        }

        if self.has_bonus_costs && org.phenotype.cur_bonus < self.inst_set.bonus_cost(cur_inst) {
            return false;
        }

        let mut per_use_cost = 0;
        if self.has_costs {
            per_use_cost = self.thread_inst_cost.get(op).copied().unwrap_or(0);
        }
        let mut add_female_costs = false;
        if self.has_female_costs
            && org.phenotype.mating_type == MATING_TYPE_FEMALE
            && self.inst_set.female_cost(cur_inst) != 0
        {
            add_female_costs = true;
            per_use_cost += self.inst_set.female_cost(cur_inst);
        }
        let mut add_choosy_female_costs = false;
        if self.has_choosy_female_costs
            && org.phenotype.mating_type == MATING_TYPE_FEMALE
            && org.phenotype.mate_preference != MATE_PREFERENCE_RANDOM
            && self.inst_set.choosy_female_cost(cur_inst) != 0
        {
            add_choosy_female_costs = true;
            per_use_cost += self.inst_set.choosy_female_cost(cur_inst);
        }
        if self.has_costs | add_female_costs | add_choosy_female_costs | self.has_post_costs {
            let Some(post) = self.active_thread_post_costs.get_mut(thread_id) else {
                return true;
            };
            if *post > 1 {
                *post -= 1;
                return false;
            }
            if *post == 1 {
                *post = 0;
            }
            let Some(active) = self.active_thread_costs.get_mut(thread_id) else {
                return true;
            };
            if *active > 1 {
                *active -= 1;
                return false;
            }
            if *active == 0 && per_use_cost > 1 {
                *active = per_use_cost - 1;
                return false;
            }
            if *active == 1 {
                *active = 0;
            }
        }

        if energy_enabled {
            if let Some(c) = self.inst_energy_cost.get_mut(op) {
                *c = f64::from(self.inst_set.energy_cost(cur_inst));
            }
        }
        true
    }

    /// `IsPayingActiveCost(ctx, thread_id)`.
    pub fn is_paying_active_cost(&self, thread_id: usize) -> bool {
        self.active_thread_post_costs
            .get(thread_id)
            .is_some_and(|c| *c > 1)
    }

    /// `SingleProcess_PayPostResCosts(ctx, cur_inst)`.
    pub fn pay_post_res_costs(
        &mut self,
        org: &mut OrgState,
        world: &mut dyn OrgInterface,
        cur_inst: Instruction,
    ) {
        if self.has_res_costs || self.has_fem_res_costs {
            let res_cost = self.inst_set.res_cost(cur_inst);
            let fem_res_cost = if org.phenotype.mating_type == MATING_TYPE_FEMALE {
                self.inst_set.fem_res_cost(cur_inst)
            } else {
                0.0
            };
            let res_req = res_cost + fem_res_cost;
            let _ = world.get_resources(org);
            let resource = world.config().collect_specific_resource;
            if resource < 0 {
                println!("Instruction resource costs require use of COLLECT_SPECIFIC_RESOURCE and USE_RESOURCE_BINS");
            }
            org.add_to_rbin(resource, -res_req);
        }
        if self.has_bonus_costs {
            org.phenotype.cur_bonus -= self.inst_set.bonus_cost(cur_inst);
        }
    }

    /// `SingleProcess_SetPostCPUCosts(ctx, cur_inst, thread_id)`.
    pub fn set_post_cpu_costs(&mut self, cur_inst: Instruction, thread_id: usize) {
        if self.has_post_costs {
            let per_use_post_cost = self
                .thread_inst_post_cost
                .get(cur_inst.op() as usize)
                .copied()
                .unwrap_or(0);
            if let Some(post) = self.active_thread_post_costs.get_mut(thread_id) {
                if *post == 0 && per_use_post_cost > 1 {
                    *post = per_use_post_cost;
                }
            }
        }
    }

    /// `RecordMicroTrace(cur_inst)`.
    pub fn record_micro_trace(&mut self, cur_inst: Instruction) {
        let sym = cur_inst.symbol();
        self.microtracer
            .push(sym.as_bytes().first().copied().unwrap_or(0));
    }
}

// ---------------------------------------------------------------------------
// Sequences the mutation helpers work on
// ---------------------------------------------------------------------------

/// The `InstructionSequence` operations the mutation helpers use; both the
/// offspring genome and the hardware memory (`cCPUMemory`, which overrides
/// them to keep its flags) implement it.
pub trait MutSeq {
    /// `GetSize()`
    fn len_i32(&self) -> i32;
    /// `seq[i]`, out-of-range reads give [`crate::head_cpu::OUT_OF_RANGE_INST`].
    fn at(&self, i: i32) -> Instruction;
    /// `seq[i] = inst`, out-of-range writes dropped.
    fn put(&mut self, i: i32, inst: Instruction);
    /// `Resize(n)`
    fn resize_to(&mut self, n: i32);
    /// `Insert(pos, inst)` (guarded).
    fn insert_at(&mut self, pos: i32, inst: Instruction);
    /// `Remove(pos)` (guarded).
    fn remove_at(&mut self, pos: i32);
    /// A plain copy (`InstructionSequence(genome)`).
    fn to_seq(&self) -> InstructionSequence;
    /// `AsString()`
    fn as_text(&self) -> String;
}

impl MutSeq for InstructionSequence {
    fn len_i32(&self) -> i32 {
        self.size()
    }
    fn at(&self, i: i32) -> Instruction {
        crate::head_cpu::inst_at(self, i)
    }
    fn put(&mut self, i: i32, inst: Instruction) {
        crate::head_cpu::set_inst_at(self, i, inst);
    }
    fn resize_to(&mut self, n: i32) {
        self.resize(n.max(0));
    }
    fn insert_at(&mut self, pos: i32, inst: Instruction) {
        if pos >= 0 && pos <= self.size() {
            self.insert(pos, inst);
        }
    }
    fn remove_at(&mut self, pos: i32) {
        if pos >= 0 && pos < self.size() {
            self.remove(pos, 1);
        }
    }
    fn to_seq(&self) -> InstructionSequence {
        self.clone()
    }
    fn as_text(&self) -> String {
        self.as_string()
    }
}

impl MutSeq for CpuMemory {
    fn len_i32(&self) -> i32 {
        self.size()
    }
    fn at(&self, i: i32) -> Instruction {
        crate::head_cpu::inst_at(self, i)
    }
    fn put(&mut self, i: i32, inst: Instruction) {
        if i >= 0 && i < self.size() {
            self[i] = inst;
        }
    }
    fn resize_to(&mut self, n: i32) {
        self.resize(n.max(0));
    }
    fn insert_at(&mut self, pos: i32, inst: Instruction) {
        if pos >= 0 && pos <= self.size() {
            self.insert(pos, inst);
        }
    }
    fn remove_at(&mut self, pos: i32) {
        if pos >= 0 && pos < self.size() {
            self.remove(pos, 1);
        }
    }
    fn to_seq(&self) -> InstructionSequence {
        self.to_sequence()
    }
    fn as_text(&self) -> String {
        self.as_string()
    }
}

/// `MAX_GENOME_SIZE` clamped to `MAX_GENOME_LENGTH` (0 means the limit).
fn max_genome_size(cfg: &Config) -> i32 {
    let m = cfg.max_genome_size;
    if m == 0 || m > MAX_GENOME_LENGTH {
        MAX_GENOME_LENGTH
    } else {
        m
    }
}

/// `MIN_GENOME_SIZE` clamped to `MIN_GENOME_LENGTH` (0 means the limit).
fn min_genome_size(cfg: &Config) -> i32 {
    let m = cfg.min_genome_size;
    if m == 0 || m < MIN_GENOME_LENGTH {
        MIN_GENOME_LENGTH
    } else {
        m
    }
}

/// `GetRandBinomial(n, p)` as the C++ `int` it is assigned to.
fn binomial(rng: &mut AvidaRng, n: i32, p: f64) -> i32 {
    rng.get_rand_binomial(f64::from(n), p) as i32
}

/// `GetUInt(n)` as an index (`n <= 0` is undefined in C++: 0 is used).
fn uint_index(rng: &mut AvidaRng, n: i32) -> i32 {
    if n <= 0 {
        return 0;
    }
    rng.get_uint(n as u32) as i32
}

/// `Apto::QSort` of the insertion sites, then insert in reverse order.
fn insert_sorted_sites<S: MutSeq + ?Sized>(
    seq: &mut S,
    inst_set: &InstSet,
    rng: &mut AvidaRng,
    num_mut: i32,
) {
    let mut mut_sites: Vec<i32> = (0..num_mut)
        .map(|_| uint_index(rng, seq.len_i32() + 1))
        .collect();
    mut_sites.sort_unstable();
    for site in mut_sites.into_iter().rev() {
        let inst = inst_set.random_inst(rng);
        seq.insert_at(site, inst);
    }
}

/// `cHardwareBase::doUniformMutation(ctx, genome)`.
pub fn do_uniform_mutation<S: MutSeq + ?Sized>(
    inst_set: &InstSet,
    cfg: &Config,
    rng: &mut AvidaRng,
    genome: &mut S,
) -> bool {
    let n = inst_set.size();
    let mutv = rng.get_uint((n * 2 + 1) as u32) as i32;
    if mutv < n {
        let site = uint_index(rng, genome.len_i32());
        genome.put(site, Instruction::new(mutv));
    } else if mutv == n {
        if genome.len_i32() == min_genome_size(cfg) {
            return false;
        }
        let site = uint_index(rng, genome.len_i32());
        genome.remove_at(site);
    } else {
        if genome.len_i32() == max_genome_size(cfg) {
            return false;
        }
        let site = uint_index(rng, genome.len_i32() + 1);
        genome.insert_at(site, Instruction::new(mutv - n - 1));
    }
    true
}

/// `NO_MUT_INSTS` contains the first character of `inst`'s symbol
/// (`cHardwareCPU::checkNoMutList`).
pub fn in_no_mut_list(cfg: &Config, inst: Instruction) -> bool {
    let sym = inst.symbol();
    let Some(c) = sym.as_bytes().first().copied() else {
        return false;
    };
    cfg.no_mut_insts.as_bytes().contains(&c)
}

/// `cHardwareBase::doUniformCopyMutation(ctx, head)`.
pub fn do_uniform_copy_mutation(
    inst_set: &InstSet,
    cfg: &Config,
    rng: &mut AvidaRng,
    head: &HeadCpu,
    mem: &mut CpuMemory,
) {
    let n = inst_set.size();
    let mutv = rng.get_uint((n * 2 + 1) as u32) as i32;
    if !in_no_mut_list(cfg, head.get_inst(mem)) {
        if mutv < n {
            head.set_inst(mem, Instruction::new(mutv));
        } else if mutv == n {
            head.remove_inst(mem);
        } else {
            head.insert_inst(mem, Instruction::new(mutv - n - 1));
        }
    }
}

/// The scrambled-order index walk of the slip and translocation fills.
fn scrambled_index(copied_so_far: &[bool], mut copy_index: i32) -> i32 {
    let mut test = 0usize;
    let mut passed = copy_index;
    while passed >= 0 {
        if copied_so_far.get(test).copied().unwrap_or(false) {
            copy_index += 1;
        } else {
            passed -= 1;
        }
        test += 1;
        if test > copied_so_far.len() + 1 {
            break;
        }
    }
    copy_index
}

/// `cHardwareBase::doSlipMutation(ctx, genome, from)`. An unknown
/// `SLIP_FILL_MODE` aborts the C++ run (`INVALID_CONFIG`): `Err`.
pub fn do_slip_mutation<S: MutSeq + ?Sized>(
    inst_set: &InstSet,
    cfg: &Config,
    verbosity: i32,
    rng: &mut AvidaRng,
    genome: &mut S,
    from: i32,
) -> Result<(), String> {
    let genome_copy = genome.to_seq();
    let size = genome_copy.size();
    let mut from = from;
    if from < 0 {
        from = rng.get_int(size + 1);
    }
    let to = if from == 0 {
        rng.get_int(size)
    } else {
        rng.get_int(size + 1)
    };

    let mut insertion_length = from - to;
    genome.resize_to(genome.len_i32() + insertion_length);

    if insertion_length > 0 {
        let mut copied_so_far = vec![false; insertion_length as usize];
        for i in 0..insertion_length {
            match cfg.slip_fill_mode {
                0 => genome.put(from + i, crate::head_cpu::inst_at(&genome_copy, to + i)),
                1 => genome.put(from + i, inst_set.get_inst("nop-X")?),
                2 => {
                    let inst = inst_set.random_inst(rng);
                    genome.put(from + i, inst);
                }
                3 => {
                    let copy_index =
                        scrambled_index(&copied_so_far, rng.get_int(insertion_length - i));
                    // QUIRK: reads the genome being filled, not the copy.
                    let v = genome.at(to + copy_index);
                    genome.put(from + i, v);
                    if let Some(c) = copied_so_far.get_mut(copy_index as usize) {
                        *c = true;
                    }
                }
                4 => genome.put(from + i, inst_set.get_inst("nop-C")?),
                _ => return Err("Unknown SLIP_FILL_MODE".to_string()),
            }
        }
    }

    if insertion_length < 0 {
        insertion_length = 0;
    }
    let mut i = insertion_length;
    while i < size - to {
        genome.put(from + i, crate::head_cpu::inst_at(&genome_copy, to + i));
        i += 1;
    }

    if verbosity >= VERBOSE_DETAILS {
        println!("SLIP MUTATION from {} to {}", from, to);
        println!("Parent: {}", genome_copy.as_string());
        println!("Offspring: {}", genome.as_text());
    }
    Ok(())
}

/// `cHardwareBase::doTransMutation(ctx, genome, from)`. An unknown
/// `TRANS_FILL_MODE` is an `Err` (C++ abort).
pub fn do_trans_mutation<S: MutSeq + ?Sized>(
    cfg: &Config,
    rng: &mut AvidaRng,
    genome: &mut S,
    from: i32,
) -> Result<(), String> {
    let genome_copy = genome.to_seq();
    let size = genome_copy.size();
    let mut from = from;
    if from < 0 {
        from = rng.get_int(size + 1);
    }
    let to = if from == 0 {
        rng.get_int(size)
    } else {
        rng.get_int(size + 1)
    };

    let insertion_length = from - to;
    genome.resize_to(genome.len_i32() + insertion_length);

    let ins_loc = rng.get_int(size + 1);

    if insertion_length > 0 {
        match cfg.trans_fill_mode {
            0 => {
                for i in 0..insertion_length {
                    genome.put(ins_loc + i, crate::head_cpu::inst_at(&genome_copy, to + i));
                }
            }
            1 => {
                let mut copied_so_far = vec![false; insertion_length as usize];
                for i in 0..insertion_length {
                    let copy_index =
                        scrambled_index(&copied_so_far, rng.get_int(insertion_length - i));
                    let v = genome.at(to + copy_index);
                    genome.put(ins_loc + i, v);
                    if let Some(c) = copied_so_far.get_mut(copy_index as usize) {
                        *c = true;
                    }
                }
            }
            _ => return Err("Unknown TRANS_FILL_MODE".to_string()),
        }
    }

    if insertion_length < 0 {
        let mut i = ins_loc;
        while i < genome.len_i32() {
            genome.put(
                i,
                crate::head_cpu::inst_at(&genome_copy, i - insertion_length),
            );
            i += 1;
        }
    } else {
        for i in ins_loc..size {
            genome.put(
                i + insertion_length,
                crate::head_cpu::inst_at(&genome_copy, i),
            );
        }
    }
    Ok(())
}

/// `cHardwareBase::doLGTMutation(ctx, genome)`.
///
/// QUIRK: `LGT_FILL_MODE` 1 falls through into the `default:` error case in
/// the C++ (no `break`), so it aborts like an unknown mode: both are `Err`.
pub fn do_lgt_mutation(
    org: &OrgState,
    world: &mut dyn OrgInterface,
    genome: &mut InstructionSequence,
) -> Result<(), String> {
    let size = genome.size();
    let rng = world.rng();
    let from = rng.get_int(size + 1);
    let to = if from == 0 {
        rng.get_int(size)
    } else {
        rng.get_int(size + 1)
    };
    let insertion_length = from - to;

    if insertion_length > 0 {
        let region = world.config().lgt_source_region;
        match world.get_lgt_fragment(region, &org.initial_genome) {
            Some(ins_seq) => {
                let genome_copy = genome.clone();
                genome.resize(genome.size() + ins_seq.size());
                let ins_loc = world.rng().get_int(genome_copy.size() + 1);
                match world.config().lgt_fill_mode {
                    0 => {
                        for i in 0..ins_seq.size() {
                            genome.put(i + ins_loc, ins_seq.get(i));
                        }
                    }
                    _ => return Err("Unknown LGT_FILL_MODE".to_string()),
                }
                for i in ins_loc..genome_copy.size() {
                    genome.put(i + ins_seq.size(), genome_copy.get(i));
                }
            }
            None => {
                let cfg = world.config().clone();
                do_trans_mutation(&cfg, world.rng(), genome, -1)?;
            }
        }
    } else if insertion_length < 0 {
        let genome_copy = genome.clone();
        genome.resize(genome.size() + insertion_length);
        let mut i = 0;
        while to + i < genome_copy.size() - to {
            genome.put(from + i, genome_copy.get(to + i));
            i += 1;
        }
    }
    Ok(())
}

/// `cHardwareBase::Divide_DoMutations(ctx, mut_multiplier, maxmut)`:
/// mutate `org.offspring_genome` and the parent `memory`. Returns the
/// number of counted mutations.
pub fn divide_do_mutations(
    inst_set: &InstSet,
    org: &mut OrgState,
    world: &mut dyn OrgInterface,
    memory: &mut CpuMemory,
    mut_multiplier: f64,
    maxmut: i32,
) -> Result<i32, String> {
    // The config is cloned only on the rare paths that need all of it
    // alongside the RNG (slip, translocation and uniform mutations).
    let verbosity = world.verbosity();
    let max_genome_size = max_genome_size(world.config());
    let min_genome_size = min_genome_size(world.config());
    let enable_hgt = world.config().enable_hgt;

    let mut total = 0;
    org.phenotype.div_type = mut_multiplier;
    let mr = org.mut_rates;

    // Slip
    if mr.test_divide_slip(world.rng()) {
        let g = org.offspring_genome.representation_mut();
        let cfg = world.config().clone();
        do_slip_mutation(inst_set, &cfg, verbosity, world.rng(), g, -1)?;
    }
    let n = mr.num_divide_poisson_slip(world.rng());
    for _ in 0..n {
        let g = org.offspring_genome.representation_mut();
        let cfg = world.config().clone();
        do_slip_mutation(inst_set, &cfg, verbosity, world.rng(), g, -1)?;
    }
    if mr.divide.slip_prob > 0.0 {
        let size = org.offspring_genome.representation().size();
        let num_mut = binomial(world.rng(), size, mr.divide.slip_prob / mut_multiplier);
        for _ in 0..num_mut {
            let g = org.offspring_genome.representation_mut();
            let cfg = world.config().clone();
            do_slip_mutation(inst_set, &cfg, verbosity, world.rng(), g, -1)?;
        }
    }

    // Translocation
    if mr.test_divide_trans(world.rng()) {
        do_trans_mutation(
            &world.config().clone(),
            world.rng(),
            org.offspring_genome.representation_mut(),
            -1,
        )?;
    }
    let n = mr.num_divide_poisson_trans(world.rng());
    for _ in 0..n {
        do_trans_mutation(
            &world.config().clone(),
            world.rng(),
            org.offspring_genome.representation_mut(),
            -1,
        )?;
    }
    if mr.divide.trans_prob > 0.0 {
        let size = org.offspring_genome.representation().size();
        let num_mut = binomial(world.rng(), size, mr.divide.trans_prob / mut_multiplier);
        for _ in 0..num_mut {
            do_trans_mutation(
                &world.config().clone(),
                world.rng(),
                org.offspring_genome.representation_mut(),
                -1,
            )?;
        }
    }

    // Lateral gene transfer
    if mr.test_divide_lgt(world.rng()) {
        let mut g = org.offspring_genome.representation().clone();
        do_lgt_mutation(org, world, &mut g)?;
        org.offspring_genome.set_representation(g);
    }
    let n = mr.num_divide_poisson_lgt(world.rng());
    for _ in 0..n {
        let mut g = org.offspring_genome.representation().clone();
        do_lgt_mutation(org, world, &mut g)?;
        org.offspring_genome.set_representation(g);
    }
    if mr.divide.lgt_prob > 0.0 {
        let size = org.offspring_genome.representation().size();
        let num_mut = binomial(world.rng(), size, mr.divide.lgt_prob / mut_multiplier);
        for _ in 0..num_mut {
            let mut g = org.offspring_genome.representation().clone();
            do_lgt_mutation(org, world, &mut g)?;
            org.offspring_genome.set_representation(g);
        }
    }

    // HGT
    if enable_hgt != 0 {
        world.do_hgt_mutation(org);
    }

    let og = org.offspring_genome.representation_mut();

    // Divide mutations
    if mr.test_divide_mut(world.rng()) && total < maxmut {
        let line = uint_index(world.rng(), og.size());
        let inst = inst_set.random_inst(world.rng());
        og.put(line, inst);
        total += 1;
    }
    let n = mr.num_divide_poisson_mut(world.rng());
    for _ in 0..n {
        if total >= maxmut {
            break;
        }
        let line = uint_index(world.rng(), og.size());
        let inst = inst_set.random_inst(world.rng());
        og.put(line, inst);
        total += 1;
    }

    // Divide insertions
    if mr.test_divide_ins(world.rng()) && og.size() < max_genome_size && total < maxmut {
        let line = uint_index(world.rng(), og.size() + 1);
        let inst = inst_set.random_inst(world.rng());
        og.insert_at(line, inst);
        total += 1;
    }
    let n = mr.num_divide_poisson_ins(world.rng());
    for _ in 0..n {
        if og.size() >= max_genome_size || total >= maxmut {
            break;
        }
        let line = uint_index(world.rng(), og.size() + 1);
        let inst = inst_set.random_inst(world.rng());
        og.insert_at(line, inst);
        total += 1;
    }

    // Divide deletions
    if mr.test_divide_del(world.rng()) && og.size() > min_genome_size && total < maxmut {
        let line = uint_index(world.rng(), og.size());
        og.remove_at(line);
        total += 1;
    }
    let n = mr.num_divide_poisson_del(world.rng());
    for _ in 0..n {
        if og.size() <= min_genome_size || total >= maxmut {
            break;
        }
        let line = uint_index(world.rng(), og.size());
        og.remove_at(line);
        total += 1;
    }

    // Divide uniform
    if mr.test_divide_uniform(world.rng())
        && total < maxmut
        && do_uniform_mutation(inst_set, &world.config().clone(), world.rng(), og)
    {
        total += 1;
    }

    // Per-site substitutions
    if mr.divide.mut_prob > 0.0 && total < maxmut {
        let num_mut = binomial(world.rng(), og.size(), mr.divide.mut_prob / mut_multiplier);
        if num_mut > 0 && total < maxmut {
            let mut i = 0;
            while i < num_mut && total < maxmut {
                let site = uint_index(world.rng(), og.size());
                let inst = inst_set.random_inst(world.rng());
                og.put(site, inst);
                total += 1;
                i += 1;
            }
        }
    }

    // Per-site insertions
    if mr.divide.ins_prob > 0.0 && total < maxmut {
        let mut num_mut = binomial(world.rng(), og.size(), mr.divide.ins_prob);
        if num_mut + og.size() > max_genome_size {
            num_mut = max_genome_size - og.size();
        }
        if num_mut > 0 {
            insert_sorted_sites(og, inst_set, world.rng(), num_mut);
            total += num_mut;
        }
    }

    // Per-site deletions
    if mr.divide.del_prob > 0.0 && total < maxmut {
        let mut num_mut = binomial(world.rng(), og.size(), mr.divide.del_prob);
        if og.size() - num_mut < min_genome_size {
            num_mut = og.size() - min_genome_size;
        }
        for _ in 0..num_mut.max(0) {
            let site = uint_index(world.rng(), og.size());
            og.remove_at(site);
        }
        total += num_mut;
    }

    // Per-site uniform
    if mr.divide.uniform_prob > 0.0 && total < maxmut {
        let num_mut = binomial(
            world.rng(),
            og.size(),
            mr.divide.uniform_prob / mut_multiplier,
        );
        if num_mut > 0 && total < maxmut {
            let mut i = 0;
            while i < num_mut && total < maxmut {
                if do_uniform_mutation(inst_set, &world.config().clone(), world.rng(), og) {
                    total += 1;
                }
                i += 1;
            }
        }
    }

    // Parent substitutions
    if mr.divide.parent_mut_prob > 0.0 && total < maxmut {
        let num_mut = binomial(world.rng(), memory.size(), mr.divide.parent_mut_prob);
        if num_mut > 0 {
            let mut i = 0;
            while i < num_mut && total < maxmut {
                let site = uint_index(world.rng(), memory.size());
                let inst = inst_set.random_inst(world.rng());
                memory.put(site, inst);
                total += 1;
                i += 1;
            }
        }
    }

    // Parent insertions
    if mr.divide.parent_ins_prob > 0.0 && total < maxmut {
        let mut num_mut = binomial(world.rng(), memory.size(), mr.divide.parent_ins_prob);
        if num_mut + memory.size() > max_genome_size {
            num_mut = max_genome_size - memory.size();
        }
        if num_mut > 0 {
            insert_sorted_sites(memory, inst_set, world.rng(), num_mut);
            total += num_mut;
        }
    }

    // Parent deletions
    if mr.divide.parent_del_prob > 0.0 && total < maxmut {
        let mut num_mut = binomial(world.rng(), memory.size(), mr.divide.parent_del_prob);
        if memory.size() - num_mut < min_genome_size {
            num_mut = memory.size() - min_genome_size;
        }
        for _ in 0..num_mut.max(0) {
            let site = uint_index(world.rng(), memory.size());
            memory.remove_at(site);
        }
        total += num_mut;
    }

    Ok(total)
}

/// `cHardwareBase::Divide_DoExactMutations(ctx, mut_multiplier, pointmut)`.
pub fn divide_do_exact_mutations(
    inst_set: &InstSet,
    org: &mut OrgState,
    world: &mut dyn OrgInterface,
    mut_multiplier: f64,
    pointmut: i32,
) -> i32 {
    let maxmut = pointmut;
    let mut total = 0;
    org.phenotype.div_type = mut_multiplier;
    let div_mut_prob = org.mut_rates.divide.mut_prob;
    let g = org.offspring_genome.representation_mut();
    if total < maxmut {
        let line = uint_index(world.rng(), g.size());
        let inst = inst_set.random_inst(world.rng());
        g.put(line, inst);
        total += 1;
    }
    if div_mut_prob > 0.0 && total < maxmut {
        let num_mut = pointmut;
        if num_mut > 0 && total < maxmut {
            let mut i = 0;
            while i < num_mut && total < maxmut {
                let site = uint_index(world.rng(), g.size());
                let inst = inst_set.random_inst(world.rng());
                g.put(site, inst);
                total += 1;
                eprintln!("Resampling here {}", total);
                i += 1;
            }
        }
    }
    total
}

/// `cHardwareBase::Divide_TestFitnessMeasures(ctx)` (`variant1 = false`) and
/// `Divide_TestFitnessMeasures1(ctx)` (`variant1 = true`, which also
/// sterilizes non-viable offspring when `STERILIZE_UNSTABLE > 1`).
///
/// QUIRK: the revert/sterilize probabilities go through the `bool`
/// `cOrganism::GetRevert*` / `GetSterilize*` getters, so any non-zero
/// setting is probability 1.
pub fn divide_test_fitness_measures(
    org: &mut OrgState,
    world: &mut dyn OrgInterface,
    variant1: bool,
) -> bool {
    org.phenotype.copy_true = org.offspring_genome == org.initial_genome;
    org.phenotype.child_fertile = true;

    if !world.test_on_divide() {
        return false;
    }
    if org.phenotype.copy_true {
        return false;
    }

    let parent_fitness = world.genotype_test_fitness(org);
    let parenttasks = org.phenotype.cur_task_count.clone();
    let cfg = world.config().clone();
    let neut_min = parent_fitness * (1.0 - cfg.neutral_min);
    let neut_max = parent_fitness * (1.0 + cfg.neutral_max);

    let test = world.test_offspring(org, &org.offspring_genome);
    let child_fitness = test.genotype_fitness;

    let pb = |v: f64| if v != 0.0 { 1.0 } else { 0.0 };

    let mut revert = false;
    let mut sterilize = false;

    if variant1 {
        if cfg.sterilize_unstable > 0 && test.max_depth > 0 {
            sterilize = true;
        }
        if cfg.sterilize_unstable > 1 && !test.is_viable {
            sterilize = true;
        }
    } else if cfg.sterilize_unstable != 0 && test.max_depth > 0 {
        // `GetSterilizeUnstable() == true` compares the int with 1.
        if cfg.sterilize_unstable == 1 {
            sterilize = true;
        }
    }

    let rng = world.rng();
    if child_fitness == 0.0 {
        if rng.p(pb(cfg.revert_fatal)) {
            revert = true;
        }
        if rng.p(pb(cfg.sterilize_fatal)) {
            sterilize = true;
        }
    } else if child_fitness < neut_min {
        if rng.p(pb(cfg.revert_detrimental)) {
            revert = true;
        }
        if rng.p(pb(cfg.sterilize_detrimental)) {
            sterilize = true;
        }
    } else if child_fitness <= neut_max {
        if rng.p(pb(cfg.revert_neutral)) {
            revert = true;
        }
        if rng.p(pb(cfg.sterilize_neutral)) {
            sterilize = true;
        }
    } else {
        if rng.p(pb(cfg.revert_beneficial)) {
            revert = true;
        }
        if rng.p(pb(cfg.sterilize_beneficial)) {
            sterilize = true;
        }
    }

    let mut r_or_s = 0;
    if rng.p(pb(cfg.revert_taskloss)) {
        r_or_s = 1;
    } else if rng.p(pb(cfg.sterilize_taskloss)) {
        r_or_s = 2;
    }
    if r_or_s != 0 {
        let childtasks = &test.last_task_count;
        let mut del = false;
        let mut added = false;
        for (i, c) in childtasks.iter().enumerate() {
            let p = parenttasks.get(i).copied().unwrap_or(0);
            if *c > p {
                added = true;
                break;
            } else if *c < p {
                del = true;
            }
        }
        if r_or_s == 1 {
            revert = del & !added;
        } else {
            sterilize = del & !added;
        }
    }

    if pb(cfg.revert_equals) != 0.0 && world.rng().p(pb(cfg.revert_equals)) {
        let child_tasks = &test.last_task_count;
        if let Some(last) = child_tasks.last() {
            if *last >= 1 {
                revert = true;
                world.stats_add_new_task_count(child_tasks.len() as i32 - 1);
            }
        }
    }

    if revert {
        org.offspring_genome = org.initial_genome.clone();
    }
    if sterilize {
        org.phenotype.child_fertile = false;
    }
    !sterilize && revert
}

/// `cHardwareBase::PointMutate(ctx, override_mut_rate)` on `memory`.
pub fn point_mutate(
    inst_set: &InstSet,
    org: &OrgState,
    world: &mut dyn OrgInterface,
    memory: &mut CpuMemory,
    override_mut_rate: f64,
) -> i32 {
    let max_genome_size = world.config().max_genome_size;
    let min_genome_size = world.config().min_genome_size;
    let mr = &org.mut_rates;
    let mut total = 0;

    if mr.point.mut_prob > 0.0 || override_mut_rate > 0.0 {
        let rate = if override_mut_rate > 0.0 {
            override_mut_rate
        } else {
            mr.point.mut_prob
        };
        let num_mut = binomial(world.rng(), memory.size(), rate);
        for _ in 0..num_mut.max(0) {
            let site = uint_index(world.rng(), memory.size());
            let inst = inst_set.random_inst(world.rng());
            memory.put(site, inst);
            total += 1;
        }
    }

    if mr.point.ins_prob > 0.0 {
        let mut num_mut = binomial(world.rng(), memory.size(), mr.point.ins_prob);
        if num_mut + memory.size() > max_genome_size {
            num_mut = max_genome_size - memory.size();
        }
        if num_mut > 0 {
            insert_sorted_sites(memory, inst_set, world.rng(), num_mut);
            total += num_mut;
        }
    }

    if mr.point.del_prob > 0.0 {
        let mut num_mut = binomial(world.rng(), memory.size(), mr.point.del_prob);
        if memory.size() - num_mut < min_genome_size {
            num_mut = memory.size() - min_genome_size;
        }
        for _ in 0..num_mut.max(0) {
            let site = uint_index(world.rng(), memory.size());
            memory.remove_at(site);
        }
        total += num_mut;
    }
    total
}

/// The condition of `cHardwareBase::checkImplicitRepro(ctx, exec_last_inst)`
/// (the hardware then runs its `Inst_Repro`).
pub fn implicit_repro_due(org: &OrgState, cfg: &Config, exec_last_inst: bool) -> bool {
    if org.is_dead {
        return false;
    }
    let p = &org.phenotype;
    (cfg.implicit_repro_time != 0 && p.time_used >= cfg.implicit_repro_time)
        || (cfg.implicit_repro_cpu_cycles != 0
            && p.cpu_cycles_used >= cfg.implicit_repro_cpu_cycles)
        || (cfg.implicit_repro_bonus != 0 && p.cur_bonus >= f64::from(cfg.implicit_repro_bonus))
        || (cfg.implicit_repro_end != 0 && exec_last_inst)
        || (cfg.implicit_repro_energy != 0.0 && p.energy_store >= cfg.implicit_repro_energy)
}

/// Outcome of [`divide_check_viable`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Viability {
    /// The divide may proceed.
    Viable,
    /// The divide fails.
    NotViable,
    /// The divide fails and `DIVIDE_FAILURE_RESETS` asks the hardware to
    /// run `internalResetOnFailedDivide()`.
    NotViableReset,
}

/// `cHardwareBase::Divide_CheckViable(ctx, parent_size, child_size,
/// using_repro)`. `copied_size` is the hardware's
/// `calcCopiedSize(parent_size, child_size)` (`None` with `using_repro`).
pub fn divide_check_viable(
    org: &mut OrgState,
    world: &mut dyn OrgInterface,
    memory: &CpuMemory,
    parent_size: i32,
    child_size: i32,
    copied_size: Option<i32>,
) -> Viability {
    let fault_reporting = world.org_fault_reporting();
    let fault = |org: &mut OrgState, msg: String| {
        if fault_reporting {
            org.fault(FAULT_LOC_DIVIDE, FAULT_TYPE_ERROR, &msg);
        }
    };
    let cfg = world.config();
    let genome_size = org.initial_genome.representation().size();

    let juv_age = cfg.juv_period;
    let parent_age = org.phenotype.time_used;
    if parent_age < juv_age {
        fault(
            org,
            format!("Org is juvenile ({} < {})", parent_age, juv_age),
        );
        return Viability::NotViable;
    }
    let min_age = cfg.min_cycles;
    if parent_age < min_age {
        fault(org, format!("Org too young ({} < {})", parent_age, min_age));
        return Viability::NotViable;
    }

    let size_range = cfg.offspring_size_range;
    let min_size = MIN_GENOME_LENGTH.max((f64::from(genome_size) / size_range) as i32);
    let max_size = MAX_GENOME_LENGTH.min((f64::from(genome_size) * size_range) as i32);
    if child_size < min_size || child_size > max_size {
        fault(org, format!("Invalid offspring length ({})", child_size));
        return Viability::NotViable;
    }
    if parent_size < min_size || parent_size > max_size {
        fault(org, format!("Invalid post-divide length ({})", parent_size));
        return Viability::NotViable;
    }

    let max_genome_size = cfg.max_genome_size;
    let min_genome_size = cfg.min_genome_size;
    if (min_genome_size != 0 && child_size < min_genome_size)
        || (max_genome_size != 0 && child_size > max_genome_size)
    {
        fault(
            org,
            format!("Invalid absolute offspring length ({})", child_size),
        );
        return Viability::NotViable;
    }
    if (min_genome_size != 0 && parent_size < min_genome_size)
        || (max_genome_size != 0 && parent_size > max_genome_size)
    {
        fault(
            org,
            format!("Invalid absolute post-divide length ({})", parent_size),
        );
        return Viability::NotViable;
    }

    let executed_size = calc_executed_size(memory, parent_size);
    let min_exe_lines = (f64::from(parent_size) * cfg.min_exe_lines) as i32;
    if executed_size < min_exe_lines {
        fault(
            org,
            format!(
                "Too few executed lines ({} < {})",
                executed_size, min_exe_lines
            ),
        );
        return Viability::NotViable;
    }

    let mut copied = parent_size;
    if let Some(c) = copied_size {
        copied = c;
        let min_copied = (f64::from(child_size) * cfg.min_copied_lines) as i32;
        if copied < min_copied {
            fault(
                org,
                format!("Too few copied commands ({} < {})", copied, min_copied),
            );
            return Viability::NotViable;
        }
    }

    let (use_form_groups, default_group) = (cfg.use_form_groups, cfg.default_group);
    let divide_failure_resets = cfg.divide_failure_resets;
    if use_form_groups != 0 && !world.has_opinion(org) {
        if default_group != -1 {
            world.set_opinion(default_group, org);
        } else {
            return Viability::NotViable;
        }
    }

    if !org.divide_check_viable(world) {
        if divide_failure_resets != 0 {
            return Viability::NotViableReset;
        }
        return Viability::NotViable;
    }

    // cPhenotype::SetLinesExecuted / SetLinesCopied (cPhenotype.h:601-602):
    // the copied count goes to the *child's* copied size, the executed count
    // to the parent's own executed size.
    org.phenotype.executed_size = executed_size;
    org.phenotype.child_copied_size = copied;

    if world.test_sterilize() && !org.phenotype.is_injected {
        let cfg = world.config().clone();
        let merit_base = f64::from(org.phenotype.calc_size_merit(&cfg));
        let cur_fitness = merit_base * org.phenotype.cur_bonus / f64::from(org.phenotype.time_used);
        let fitness_ratio = cur_fitness / org.phenotype.last_fitness;
        let pb = |v: f64| if v != 0.0 { 1.0 } else { 0.0 };
        let rng = world.rng();
        let mut sterilize = false;
        if fitness_ratio < FITNESS_NEUTRAL_MIN {
            if rng.p(pb(cfg.sterilize_detrimental)) {
                sterilize = true;
            }
        } else if fitness_ratio <= FITNESS_NEUTRAL_MAX {
            if rng.p(pb(cfg.sterilize_neutral)) {
                sterilize = true;
            }
        } else if rng.p(pb(cfg.sterilize_beneficial)) {
            sterilize = true;
        }
        if rng.p(pb(cfg.sterilize_taskloss)) {
            let childtasks = &org.phenotype.cur_task_count;
            let parenttasks = &org.phenotype.last_task_count;
            let mut del = false;
            let mut added = false;
            for (i, c) in childtasks.iter().enumerate() {
                let p = parenttasks.get(i).copied().unwrap_or(0);
                if *c > p {
                    added = true;
                    break;
                } else if *c < p {
                    del = true;
                }
            }
            sterilize = del & !added;
        }
        if sterilize {
            org.phenotype.is_fertile = false;
            return Viability::NotViable;
        }
    }
    Viability::Viable
}

/// The outcome of [`div_guard`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DivGuard {
    /// `AVD_CPU_DIV_OK`: compute the quotient or remainder.
    Ok,
    /// `AVD_CPU_DIV_ZERO`
    Zero,
    /// `AVD_CPU_DIV_OVERFLOW`
    Overflow,
}

/// `avd_cpu_div_guard(op1, op2, 0-INT_MAX)`, the guard the reference binary
/// puts in front of `Inst_Div` and `Inst_Mod` of the heads CPU and of the
/// experimental, GP8 and BCR hardware (TransSMT keeps the pre-Rust test).
/// The C++ passes `0-INT_MAX`, not `INT_MIN`, as the overflow numerator, so
/// `-INT_MAX / -1` is the "overflow" (where the pre-Rust code divided) and
/// `INT_MIN / -1` is divided (where the pre-Rust code faulted).
pub fn div_guard(op1: i32, op2: i32) -> DivGuard {
    if op2 == 0 {
        DivGuard::Zero
    } else if op1 == -i32::MAX && op2 == -1 {
        DivGuard::Overflow
    } else {
        DivGuard::Ok
    }
}

/// `cHardwareBase::calcExecutedSize(parent_size)`.
pub fn calc_executed_size(memory: &CpuMemory, parent_size: i32) -> i32 {
    (0..parent_size.min(memory.size()))
        .filter(|i| memory.flag_executed(*i))
        .count() as i32
}

/// `Inst_DoubleEnergyUsage` / `Inst_HalveEnergyUsage` /
/// `Inst_DefaultEnergyUsage`: `which` is 0, 1, 2 respectively.
pub fn energy_usage_inst(org: &mut OrgState, world: &mut dyn OrgInterface, which: i32) -> bool {
    match which {
        0 => org.phenotype.double_energy_usage(),
        1 => org.phenotype.halve_energy_usage(),
        _ => org.phenotype.default_energy_usage(),
    }
    let e = org.phenotype.energy_store * org.phenotype.execution_ratio;
    let new_merit = org.phenotype.convert_energy_to_merit(world.config(), e);
    world.update_merit(org, new_merit);
    true
}
