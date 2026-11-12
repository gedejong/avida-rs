//! Port of `cHardwareGP8` (avida-core/source/cpu/cHardwareGP8.{h,cc} at
//! 47f13dadb), hardware type 4: the BCR's gene-expression design
//! ([`crate::hardware_bcr`]) with eight registers, operands that may be
//! immediate values, hardware units instead of behavioural classes (an
//! instruction whose unit another thread used this cycle puts its thread to
//! sleep until the next one), and movement, rotation and eating queued as
//! actions run once at the end of the cycle.

use crate::hardware::{implicit_repro_due, Hardware, HardwareBase, HeadSite};
use crate::hardware_bcr::{
    full_reset, gene_trace, mini_status_common, print_status_common, record_nav_trace, GeneCore,
    GeneHw, H_IP,
};
use crate::hardware_bcr_insts_env::move_any;
use crate::hardware_gp8_insts::{gp8_handler_table, gp8_imm_table, gp8_unit_table, Gp8InstFn};
use crate::head_cpu::HeadCpu;
use crate::nhardware::*;
use crate::org_interface::OrgInterface;
use crate::organism::OrgState;
use avida_config::config::Config;
use avida_genome::{CpuMemory, InstSet, Instruction};
use std::any::Any;
use std::sync::Arc;

/// `cHardwareGP8::NUM_REGISTERS` (= heads = nops = memory spaces).
pub const GP8_NUM_REGISTERS: usize = 8;

/// `aEAT`
pub const A_EAT: u8 = 0;
/// `aMOVE`
pub const A_MOVE: u8 = 1;
/// `aROTATE`
pub const A_ROTATE: u8 = 2;

/// `cHardwareGP8`.
pub struct HardwareGp8 {
    pub(crate) base: HardwareBase,
    pub(crate) core: GeneCore,
    pub(crate) functions: &'static [Option<Gp8InstFn>],
    /// `m_hw_units`
    pub(crate) units: &'static [u32],
    /// `m_juv_enabled`
    pub(crate) juv_enabled: bool,
    /// `m_hw_reset`: set by every `internalReset`, ends the cycle.
    pub(crate) hw_reset: bool,
    /// `m_hw_busy`
    pub(crate) hw_busy: u32,
    /// `m_hw_queue`
    pub(crate) hw_queue: [u8; 3],
    /// `m_hw_queued`
    pub(crate) hw_queued: usize,
    /// `m_hw_queue_eat`
    pub(crate) hw_queue_eat: bool,
    /// `m_hw_queue_move`
    pub(crate) hw_queue_move: bool,
    /// `m_hw_queue_rotate`
    pub(crate) hw_queue_rotate: bool,
    /// `m_hw_queue_rotate_reverse`
    pub(crate) hw_queue_rotate_reverse: bool,
    /// `m_hw_queue_rotate_num:3`
    pub(crate) hw_queue_rotate_num: u32,
    /// `m_hw_queue_eat_threads`
    pub(crate) hw_queue_eat_threads: Vec<usize>,
    /// `m_action_side_effect_queue`: the threads a bonus instruction runs
    /// on while the queued eat is output.
    pub(crate) side_effect_queue: Option<Vec<usize>>,
}

impl std::fmt::Debug for HardwareGp8 {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("HardwareGp8")
            .field("genes", &self.core.genes.len())
            .field("threads", &self.core.threads.len())
            .field("cur_thread", &self.core.cur_thread)
            .finish()
    }
}

/// The mini-trace file header of `cHardwareGP8::SetupMiniTraceFileHeader`:
/// the BCR's with the three queue columns after "Current Update".
pub fn gp8_mini_header(
    update: i32,
    org_id: i32,
    gen_id: i32,
    genotype: &str,
    genome_size: i32,
) -> Vec<String> {
    let mut v = crate::hardware_bcr::bcr_mini_header(update, org_id, gen_id, genotype, genome_size);
    if let Some(i) = v.iter().position(|s| s == "Current Update") {
        for (k, s) in ["Queued Eat", "Queued Move", "Queued Rotate (Number)"]
            .iter()
            .enumerate()
        {
            v.insert(i + 1 + k, (*s).to_string());
        }
    }
    v
}

impl HardwareGp8 {
    /// `cHardwareGP8(ctx, world, organism, inst_set)`.
    pub fn new(cfg: &Config, org: &mut OrgState, inst_set: Arc<InstSet>) -> Self {
        let seq = org.initial_genome.representation().clone();
        let mut hw = HardwareGp8 {
            base: HardwareBase::new(cfg, inst_set.clone()),
            core: GeneCore::new(
                GP8_NUM_REGISTERS,
                cfg,
                &seq,
                inst_set,
                u32::MAX,
                gp8_imm_table(),
            ),
            functions: gp8_handler_table(),
            units: gp8_unit_table(),
            juv_enabled: cfg.juv_period > 0,
            hw_reset: false,
            hw_busy: 0,
            hw_queue: [0; 3],
            hw_queued: 0,
            hw_queue_eat: false,
            hw_queue_move: false,
            hw_queue_rotate: false,
            hw_queue_rotate_reverse: false,
            hw_queue_rotate_num: 0,
            hw_queue_eat_threads: Vec::new(),
            side_effect_queue: None,
        };
        org.hardware_reset_state(cfg);
        hw.base.reset_costs(cfg);
        GeneHw::internal_reset(&mut hw, org, cfg);
        hw
    }

    /// The shared state.
    pub fn core_ref(&self) -> &GeneCore {
        &self.core
    }

    /// `SingleProcess_ExecuteInst(ctx, cur_inst)`.
    fn execute_inst(
        &mut self,
        org: &mut OrgState,
        world: &mut dyn OrgInterface,
        cur_inst: Instruction,
    ) -> Result<bool, String> {
        let op = cur_inst.op();
        if op < 0 || op >= self.base.inst_set.size() {
            // UB substitute: an opcode outside the set fails as a no-op.
            return Ok(false);
        }
        let inst_idx = self.base.inst_set.lib_function_index(cur_inst);
        self.core.ip_set_flag_executed();
        if let Some(c) = org.phenotype.cur_inst_count.get_mut(op as usize) {
            *c = c.wrapping_add(1);
        }
        let f = self
            .functions
            .get(inst_idx.max(0) as usize)
            .copied()
            .flatten()
            .ok_or_else(|| {
                format!(
                    "unsupported: instruction '{}' (C++: cHardwareGP8.cc)",
                    self.base.inst_set.name_of(cur_inst)
                )
            })?;
        let exec_success = f(self, org, world)?;
        if !exec_success {
            if let Some(c) = org.phenotype.cur_inst_count.get_mut(op as usize) {
                *c = c.wrapping_sub(1);
            }
        }
        Ok(exec_success)
    }

    fn trace(
        &self,
        org: &mut OrgState,
        world: &mut dyn OrgInterface,
        bonus: bool,
        mini: bool,
        exec_success: i32,
    ) {
        gene_trace(self, org, world, bonus, mini, exec_success, gp8_mini_header);
    }

    /// The hardware units the instruction at `inst` needs.
    fn inst_units(&self, inst: Instruction) -> u32 {
        if inst.op() < 0 || inst.op() >= self.base.inst_set.size() {
            return 0;
        }
        let idx = self.base.inst_set.lib_function_index(inst);
        self.units.get(idx.max(0) as usize).copied().unwrap_or(0)
    }

    /// The queued actions, in the order queued, until the organism is
    /// deleted or the hardware resets.
    fn run_queue(
        &mut self,
        org: &mut OrgState,
        world: &mut dyn OrgInterface,
    ) -> Result<(), String> {
        let mut hw_action = 0;
        while hw_action < self.hw_queued && !org.phenotype.to_delete && !self.hw_reset {
            match self.hw_queue.get(hw_action).copied() {
                Some(A_EAT) => {
                    self.side_effect_queue = Some(self.hw_queue_eat_threads.clone());
                    crate::hardware_bcr::org_do_output(self, org, world, Some(0), false)?;
                    self.hw_queue_eat_threads.clear();
                    self.side_effect_queue = None;
                }
                Some(A_MOVE) => {
                    move_any(self, org, world)?;
                }
                Some(A_ROTATE) => {
                    let dir = if self.hw_queue_rotate_reverse { -1 } else { 1 };
                    for _ in 0..self.hw_queue_rotate_num {
                        world.rotate(dir);
                    }
                }
                _ => {}
            }
            hw_action += 1;
        }
        Ok(())
    }
}

impl GeneHw for HardwareGp8 {
    const IMM: bool = true;
    const CLASS: &'static str = "cHardwareGP8";
    fn core(&self) -> &GeneCore {
        &self.core
    }
    fn core_mut(&mut self) -> &mut GeneCore {
        &mut self.core
    }
    /// `internalReset()`: as the BCR's, plus `m_hw_reset` and the side
    /// effect queue.
    fn internal_reset(&mut self, org: &mut OrgState, cfg: &Config) {
        self.hw_reset = true;
        self.core.internal_reset(org, cfg);
        self.base.resize_cost_arrays(self.core.threads.len() as i32);
        self.side_effect_queue = None;
    }
    fn type_id(&self) -> i32 {
        HARDWARE_TYPE_CPU_GP8
    }
}

impl Hardware for HardwareGp8 {
    fn as_any(&self) -> &dyn Any {
        self
    }
    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }
    fn base(&self) -> &HardwareBase {
        &self.base
    }
    fn base_mut(&mut self) -> &mut HardwareBase {
        &mut self.base
    }
    fn hw_type(&self) -> i32 {
        HARDWARE_TYPE_CPU_GP8
    }
    fn supports_speculative(&self) -> bool {
        true
    }

    fn reset(&mut self, org: &mut OrgState, world: &mut dyn OrgInterface) {
        full_reset(self, org, world);
    }

    /// `SingleProcess(ctx, speculative)`: as the BCR's, with hardware units
    /// in place of behavioural classes, the cycle ending at a hardware
    /// reset, and the queued actions run after the micro-op rounds.
    ///
    /// The C++ advances the IP through a reference to the thread that ran
    /// the instruction; after a reset (a failed divide under
    /// `DIVIDE_FAILURE_RESETS`) that thread is freed, and the port advances
    /// the new thread with the same index, if any, as for the BCR.
    fn single_process(
        &mut self,
        org: &mut OrgState,
        world: &mut dyn OrgInterface,
        speculative: bool,
    ) -> Result<bool, String> {
        if speculative && self.core.spec_stall {
            return Ok(false);
        }
        org.set_running(true);
        if !speculative && self.core.spec_die {
            org.die(world);
            org.set_running(false);
            return Ok(false);
        }

        if self.core.spec_stall {
            self.core.spec_stall = false;
        } else {
            self.hw_reset = false;
            self.core.cycle_count = self.core.cycle_count.wrapping_add(1);
            org.phenotype.inc_cpu_cycles_used();
            if !self.core.no_cpu_cycle_time {
                org.phenotype.inc_time_used(1);
            }
            for t in self.core.threads.iter_mut() {
                if !t.active && t.wait_reg == -1 {
                    t.active = true;
                }
            }
            self.hw_busy = 0;
            self.hw_queue_eat = false;
            self.hw_queue_move = false;
            self.hw_queue_rotate = false;
            self.hw_queued = 0;
            self.hw_queue_eat_threads.clear();
            self.core.cur_uop = 0;
            self.core.cur_thread = 0;
        }

        let uop_ratio = self.base.inst_set.uops_per_cycle();
        if uop_ratio > 255 {
            return Err("unsupported: UOPS_PER_CYCLE above 255 (C++: the 8-bit m_cur_uop never reaches it and cHardwareGP8::SingleProcess loops forever)".to_string());
        }
        'uops: while (self.core.cur_uop as i32) < uop_ratio {
            if self.core.cur_thread >= self.core.threads.len() {
                self.core.cur_thread = 0;
            }
            while self.core.cur_thread < self.core.threads.len() {
                let ct = self.core.cur_thread;
                if !self.core.threads[ct].running || !self.core.threads[ct].active {
                    self.core.cur_thread += 1;
                    continue;
                }
                self.core.advance_ip = true;
                let mut ip = self.core.threads[ct].heads[H_IP];
                self.core.h_adjust(&mut ip);
                self.core.threads[ct].heads[H_IP] = ip;

                if self.base.tracer.is_some() {
                    self.trace(org, world, false, false, -2);
                }
                let cur_inst = self.core.h_inst(&ip);
                let valid = cur_inst.op() >= 0 && cur_inst.op() < self.base.inst_set.size();
                if speculative
                    && (self.core.spec_die || (valid && self.base.inst_set.should_stall(cur_inst)))
                {
                    self.core.spec_stall = true;
                    org.set_running(false);
                    return Ok(false);
                }
                if self.base.tracer.is_some() {
                    self.trace(org, world, false, true, -2);
                }

                let mut exec = true;
                let mut exec_success = 0;
                let inst_hw_units = self.inst_units(cur_inst);
                if inst_hw_units & self.hw_busy != 0 {
                    self.core.threads[ct].active = false;
                    self.core.threads[ct].wait_reg = -1;
                    if self.base.tracer.is_some() {
                        self.trace(org, world, false, true, exec_success);
                    }
                    self.core.cur_thread += 1;
                    continue;
                }

                let on_pause = self.base.is_paying_active_cost(ct);
                if self.base.has_any_costs && valid {
                    exec = self.base.pay_pre_costs(org, world, cur_inst, ct);
                }
                if !exec {
                    exec_success = -1;
                }

                let mut rand_fail = false;
                if exec {
                    let addl_time_cost = if valid {
                        self.base.inst_set.addl_time_cost(cur_inst)
                    } else {
                        0
                    };
                    if valid {
                        let prob_fail = self.base.inst_set.prob_fail(cur_inst);
                        if prob_fail > 0.0 {
                            exec = !world.rng().p(prob_fail);
                            rand_fail = !exec;
                        }
                    }
                    if exec && self.execute_inst(org, world, cur_inst)? {
                        self.base.pay_post_res_costs(org, world, cur_inst);
                        self.base.set_post_cpu_costs(cur_inst, ct);
                        exec_success = 1;
                        self.hw_busy |= inst_hw_units;
                    }
                    if org.phenotype.to_delete {
                        // The C++ traces here (mini only), as the BCR.
                        break 'uops;
                    }
                    if self.core.advance_ip && ct < self.core.threads.len() {
                        let mut ip = self.core.threads[ct].heads[H_IP];
                        self.core.h_advance(&mut ip);
                        self.core.threads[ct].heads[H_IP] = ip;
                    }
                    org.phenotype.inc_time_used(addl_time_cost);
                }
                if self.base.tracer.is_some() {
                    self.trace(org, world, false, true, exec_success);
                }

                let do_record = (exec_success == -1 && !on_pause)
                    || (exec_success == 1 && !on_pause)
                    || rand_fail
                    || exec_success == 0;
                if do_record {
                    org.phenotype.num_execs = org.phenotype.num_execs.wrapping_add(1);
                    if self.base.microtrace || self.base.topnavtrace {
                        self.base.record_micro_trace(cur_inst);
                        if self.base.topnavtrace {
                            record_nav_trace(&self.core, &mut self.base, world);
                        }
                    }
                }
                if org.phenotype.to_delete || self.hw_reset {
                    break 'uops;
                }
                self.core.cur_thread += 1;
            }
            self.core.cur_uop = (self.core.cur_uop + 1) & 0xFF;
        }

        self.run_queue(org, world)?;

        let max_executed = org.max_executed;
        if (max_executed > 0 && org.phenotype.time_used >= max_executed) || org.phenotype.to_die {
            if speculative {
                self.core.spec_die = true;
            } else {
                org.die(world);
            }
        }
        if !speculative && org.phenotype.to_delete {
            self.core.spec_die = true;
        }
        org.set_running(false);
        if self.base.implicit_repro_active && implicit_repro_due(org, world.config(), false) {
            crate::hardware_gp8_insts::inst_repro(self, org, world)?;
        }
        Ok(!self.core.spec_die && !self.core.spec_stall)
    }

    /// `ProcessBonusInst(ctx, inst)`: on the running thread, or, while a
    /// queued eat is output, once on every thread that ate.
    fn process_bonus_inst(
        &mut self,
        org: &mut OrgState,
        world: &mut dyn OrgInterface,
        inst: Instruction,
    ) -> Result<(), String> {
        let prev_run_state = org.is_running;
        org.set_running(true);
        match self.side_effect_queue.clone() {
            None => {
                if self.base.tracer.is_some() {
                    self.trace(org, world, true, false, -2);
                }
                self.execute_inst(org, world, inst)?;
            }
            Some(threads) => {
                let running_thread = self.core.cur_thread;
                for t in threads {
                    self.core.cur_thread = t;
                    if self.base.tracer.is_some() {
                        self.trace(org, world, true, false, -2);
                    }
                    self.execute_inst(org, world, inst)?;
                }
                self.core.cur_thread = running_thread;
            }
        }
        org.set_running(prev_run_state);
        Ok(())
    }

    /// `PrintStatus(fp)`.
    fn print_status(&self, org: &OrgState, _world: &mut dyn OrgInterface) -> String {
        print_status_common(&self.core, org)
    }

    /// `PrintMiniTraceStatus(ctx, fp)`: the queued actions before the
    /// registers.
    fn print_mini_trace_status(&self, org: &mut OrgState, world: &mut dyn OrgInterface) -> String {
        let extra = format!(
            "{} {} {}{}{}) ",
            i32::from(self.hw_queue_eat),
            i32::from(self.hw_queue_move),
            i32::from(self.hw_queue_rotate),
            if self.hw_queue_rotate_reverse {
                " (-"
            } else {
                " ("
            },
            self.hw_queue_rotate_num
        );
        mini_status_common(&self.core, org, world, &extra)
    }

    /// `PrintMiniTraceSuccess(fp, exec_success)`.
    fn print_mini_trace_success(&self, exec_success: i32) -> String {
        format!("{}\n", exec_success)
    }

    fn get_stack(&self, depth: i32, stack_id: i32, in_thread: i32) -> i32 {
        self.core.get_stack(depth, stack_id, in_thread)
    }

    fn get_cur_stack(&self, in_thread: i32) -> i32 {
        let t = if in_thread >= self.core.threads.len() as i32 || in_thread < 0 {
            self.core.ct()
        } else {
            in_thread as usize
        };
        self.core
            .threads
            .get(t)
            .map_or(0, |t| i32::from(t.cur_stack))
    }

    fn num_stacks(&self) -> i32 {
        2
    }

    /// `GetHead(id)`: the placeholder head (the C++ has no `cHeadCPU`s).
    fn head(&self, _head_id: usize) -> &HeadCpu {
        &self.core.placeholder_head
    }

    fn head_mut(&mut self, _head_id: usize) -> &mut HeadCpu {
        &mut self.core.placeholder_head
    }

    fn head_in_thread(&self, _head_id: usize, _thread: usize) -> &HeadCpu {
        &self.core.placeholder_head
    }

    fn num_heads(&self) -> i32 {
        0
    }

    /// `GetMemory()`: memory space 0 (the genome).
    fn memory(&self) -> &CpuMemory {
        &self.core.mem_array[0]
    }

    fn memory_mut(&mut self) -> &mut CpuMemory {
        &mut self.core.mem_array[0]
    }

    fn num_mem_spaces(&self) -> i32 {
        -1
    }

    /// Space 0, every space a `mem_ids` slot names, then each gene's memory
    /// (ids from [`crate::hardware_bcr::GENE_SPACE_BASE`]); the threads run
    /// in the genes.
    fn memory_spaces(&self) -> Vec<(i32, &CpuMemory)> {
        self.core.inspect_spaces()
    }

    fn memory_space_label(&self, id: i32) -> Option<String> {
        self.core.space_label(id)
    }

    /// The heads of the thread (the C++ has no `cHeadCPU`s, so
    /// `head_in_thread` only returns a placeholder).
    fn thread_heads(&self, thread: usize) -> Vec<HeadSite> {
        self.core.thread_sites(thread)
    }

    fn register_in_thread(&self, reg_id: usize, thread: usize) -> Option<i32> {
        let t = self.core.threads.get(thread)?;
        t.reg.get(reg_id).map(|r| r.value)
    }

    fn thread_status(&self, thread: usize) -> String {
        self.core.thread_status_text(thread)
    }

    fn register(&self, reg_id: usize) -> i32 {
        if self.core.threads.is_empty() {
            return 0;
        }
        self.core.reg(reg_id)
    }

    fn num_registers(&self) -> i32 {
        GP8_NUM_REGISTERS as i32
    }

    fn num_threads(&self) -> i32 {
        self.core.threads.len() as i32
    }

    fn cur_thread(&self) -> i32 {
        self.core.cur_thread as i32
    }
}
