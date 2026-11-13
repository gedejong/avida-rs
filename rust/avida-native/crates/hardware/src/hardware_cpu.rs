//! Port of `cHardwareCPU` (avida-core/source/cpu/cHardwareCPU.{h,cc} at
//! 47f13dadb), hardware type 0, the "heads" CPU: the execution core,
//! label search, threads, allocation and divide. The instruction handlers
//! are in [`crate::hardware_cpu_insts`].
//!
//! The promoter model (`PROMOTERS_ENABLED`, `CONSTITUTIVE_REGULATION`) is
//! here and in [`crate::hardware_cpu_insts_misc`].

use crate::code_label::CodeLabel;
use crate::cpu_stack::CpuStack;
use crate::hardware::{
    divide_check_viable, divide_do_exact_mutations, divide_do_mutations,
    divide_test_fitness_measures, implicit_repro_due, Hardware, HardwareBase, Viability,
};
use crate::hardware_cpu_insts::{handler_table, InstFn};
use crate::head_cpu::HeadCpu;
use crate::nhardware::*;
use crate::org_interface::OrgInterface;
use crate::organism::{do_output, print_status, OrgState};
use avida_config::config::Config;
use avida_genome::{CpuMemory, Genome, InstSet, Instruction, InstructionSequence};
use std::any::Any;
use std::sync::Arc;

/// `cHardwareCPU::NUM_REGISTERS`
pub const NUM_REGISTERS: usize = 3;
/// `cHardwareCPU::NUM_NOPS`
pub const NUM_NOPS: i32 = 3;
/// `REG_AX`
pub const REG_AX: usize = 0;
/// `REG_BX`
pub const REG_BX: usize = 1;
/// `REG_CX`
pub const REG_CX: usize = 2;

/// The C++ constructor's `internalReset()` looks up the `promoter`
/// instruction when `PROMOTERS_ENABLED` is set; `cInstSet::GetInst` prints
/// `Error: Unknown instruction 'promoter'.  Exiting...` and exits with
/// status 1 when the set lacks it. That message is the `Err`.
pub fn check_inst_set(cfg: &Config, is: &InstSet) -> Result<(), String> {
    if cfg.promoters_enabled != 0 {
        is.get_inst("promoter")?;
    }
    Ok(())
}

/// `cHardwareCPU::cPromoter`
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct CpuPromoter {
    /// `m_pos`: position within the genome.
    pub pos: i32,
    /// `m_bit_code`
    pub bit_code: i32,
    /// `m_regulation`
    pub regulation: i32,
}

impl CpuPromoter {
    /// `GetRegulatedBitCode()`
    pub fn regulated_bit_code(&self) -> i32 {
        self.bit_code ^ self.regulation
    }
}

/// `cHardwareCPU::cLocalThread`.
#[derive(Debug, Clone, Default)]
pub struct LocalThread {
    pub(crate) id: i32,
    pub(crate) promoter_inst_executed: i32,
    pub(crate) message_trigger_type: i32,
    /// `reg`
    pub reg: [i32; NUM_REGISTERS],
    /// `heads`
    pub heads: [HeadCpu; NUM_HEADS],
    /// `stack`
    pub stack: CpuStack,
    /// `cur_stack` (0 local, 1 global).
    pub cur_stack: u8,
    /// `cur_head`
    pub cur_head: u8,
    /// `read_label`
    pub read_label: CodeLabel,
    /// `next_label`
    pub next_label: CodeLabel,
}

impl LocalThread {
    /// `cLocalThread()` (`Reset(NULL, -1)`).
    fn new_default() -> Self {
        let mut t = LocalThread::default();
        t.reset(-1);
        t
    }

    /// `Reset(in_hardware, in_id)`.
    pub fn reset(&mut self, id: i32) {
        self.id = id;
        self.reg = [0; NUM_REGISTERS];
        for h in self.heads.iter_mut() {
            *h = HeadCpu::default();
        }
        self.stack.clear();
        self.cur_stack = 0;
        self.cur_head = HEAD_IP as u8;
        self.read_label.clear();
        self.next_label.clear();
        self.promoter_inst_executed = 0;
        self.message_trigger_type = -1;
    }

    /// `operator=`: copies id, registers, heads, stack and message trigger
    /// type only (not the current stack/head, labels or promoter count).
    pub fn assign(&mut self, other: &LocalThread) {
        self.id = other.id;
        self.reg = other.reg;
        for (h, o) in self.heads.iter_mut().zip(other.heads.iter()) {
            h.assign(o);
        }
        self.stack = other.stack;
        self.message_trigger_type = other.message_trigger_type;
    }

    /// `GetID()`
    pub fn id(&self) -> i32 {
        self.id
    }

    /// `getMessageTriggerType()`
    pub fn message_trigger_type(&self) -> i32 {
        self.message_trigger_type
    }
}

/// `Apto::Array<cLocalThread>::Resize(n)`.
///
/// QUIRK: Apto's `Basic` storage reallocates on every size change: new
/// default-constructed threads, then `operator=` copies of the old ones.
/// `cLocalThread::operator=` does not copy `cur_stack`, `cur_head`,
/// `read_label`, `next_label` or the promoter count, so every fork and
/// kill resets those fields of all surviving threads.
fn apto_resize_threads(threads: &mut Vec<LocalThread>, new_size: usize) -> bool {
    if new_size == threads.len() {
        return false;
    }
    let mut new_threads: Vec<LocalThread> =
        (0..new_size).map(|_| LocalThread::new_default()).collect();
    for (n, o) in new_threads.iter_mut().zip(threads.iter()) {
        n.assign(o);
    }
    *threads = new_threads;
    true
}

/// `cHardwareCPU`.
pub struct HardwareCpu {
    pub(crate) base: HardwareBase,
    pub(crate) functions: &'static [Option<InstFn>],
    pub(crate) memory: CpuMemory,
    pub(crate) global_stack: CpuStack,
    pub(crate) threads: Vec<LocalThread>,
    pub(crate) thread_id_chart: i32,
    pub(crate) cur_thread: usize,
    /// Bumped whenever the thread array is reallocated (see
    /// [`HardwareCpu::single_process`]).
    pub(crate) thread_realloc: u64,

    pub(crate) mal_active: bool,
    pub(crate) advance_ip: bool,
    pub(crate) executedmatchstrings: bool,
    pub(crate) spec_die: bool,
    pub(crate) thread_slicing_parallel: bool,
    pub(crate) no_cpu_cycle_time: bool,
    pub(crate) slip_read_head: bool,

    pub(crate) epigenetic_state: bool,
    pub(crate) epigenetic_saved_reg: [i32; NUM_REGISTERS],
    pub(crate) epigenetic_saved_stack: CpuStack,

    /// `m_last_cell_data`
    pub(crate) last_cell_data: (bool, i32),
    /// `m_flash_info`
    pub(crate) flash_info: (u32, u32),
    /// `m_cycle_counter`
    pub(crate) cycle_counter: u32,

    /// `m_promoters_enabled` (`PROMOTERS_ENABLED` at construction).
    pub(crate) promoters_enabled: bool,
    /// `m_constitutive_regulation` (`CONSTITUTIVE_REGULATION` at
    /// construction; not tied to `PROMOTERS_ENABLED`).
    pub(crate) constitutive_regulation: bool,
    /// `m_promoter_index`: the promoter execution last started at (-1:
    /// none).
    pub(crate) promoter_index: i32,
    /// `m_promoter_offset`: the bit window `IsActivePromoter` tests.
    pub(crate) promoter_offset: i32,
    /// `m_promoters`
    pub(crate) promoters: Vec<CpuPromoter>,
    /// A condition under which `internalReset()` never returns in C++
    /// (`Numberate` with `INST_CODE_LENGTH < 1`); the next
    /// `SingleProcess` reports it.
    pub(crate) reset_error: Option<String>,
}

impl std::fmt::Debug for HardwareCpu {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("HardwareCpu")
            .field("memory", &self.memory.as_string())
            .field("threads", &self.threads)
            .field("cur_thread", &self.cur_thread)
            .finish()
    }
}

impl HardwareCpu {
    /// `cHardwareCPU(ctx, world, organism, inst_set)`: memory from the
    /// organism's genome, then `Reset(ctx)` (whose `HardwareReset` runs
    /// without an interface) and a second `internalReset()`.
    pub fn new(cfg: &Config, org: &mut OrgState, inst_set: Arc<InstSet>) -> Self {
        let memory = CpuMemory::from_sequence(org.initial_genome.representation());
        let mut hw = HardwareCpu {
            base: HardwareBase::new(cfg, inst_set),
            functions: handler_table(),
            memory,
            global_stack: CpuStack::default(),
            threads: vec![LocalThread::new_default()],
            thread_id_chart: 0,
            cur_thread: 0,
            thread_realloc: 0,
            mal_active: false,
            advance_ip: false,
            executedmatchstrings: false,
            spec_die: false,
            thread_slicing_parallel: cfg.thread_slicing_method == 1,
            no_cpu_cycle_time: cfg.no_cpu_cycle_time != 0,
            slip_read_head: cfg.slip_copy_mode == 0,
            epigenetic_state: false,
            epigenetic_saved_reg: [0; NUM_REGISTERS],
            epigenetic_saved_stack: CpuStack::default(),
            last_cell_data: (false, 0),
            flash_info: (0, 0),
            cycle_counter: 0,
            promoters_enabled: cfg.promoters_enabled != 0,
            constitutive_regulation: cfg.constitutive_regulation != 0,
            promoter_index: -1,
            promoter_offset: 0,
            promoters: Vec::new(),
            reset_error: None,
        };
        // Reset(ctx) without an interface: HardwareReset only touches the
        // organism's opinion and display.
        org.hardware_reset_state(cfg);
        hw.base.reset_costs(cfg);
        hw.internal_reset(cfg);
        hw.internal_reset(cfg);
        hw
    }

    // ------------------------------------------------------------------
    // Small accessors
    // ------------------------------------------------------------------

    /// `m_cur_thread` clamped into the thread array (an index past the end
    /// is undefined in the C++).
    #[inline]
    pub(crate) fn ct(&self) -> usize {
        if self.cur_thread < self.threads.len() {
            self.cur_thread
        } else {
            0
        }
    }

    /// `GetRegister(id)` of the current thread.
    #[inline]
    pub(crate) fn reg(&self, r: usize) -> i32 {
        self.threads[self.ct()].reg[r]
    }

    /// `GetRegister(id) = v`.
    #[inline]
    pub(crate) fn set_reg(&mut self, r: usize, v: i32) {
        let ct = self.ct();
        self.threads[ct].reg[r] = v;
    }

    /// `getHead(id)` (a copy).
    #[inline]
    pub(crate) fn get_head(&self, h: usize) -> HeadCpu {
        self.threads[self.ct()].heads[h]
    }

    /// `getHead(id)`, mutable.
    #[inline]
    pub(crate) fn head_ref(&mut self, h: usize) -> &mut HeadCpu {
        let ct = self.ct();
        &mut self.threads[ct].heads[h]
    }

    /// `getIP()` (a copy).
    #[inline]
    pub(crate) fn ip_head(&self) -> HeadCpu {
        self.get_head(HEAD_IP)
    }

    /// `getHead(h).Advance()`.
    #[inline]
    pub(crate) fn head_advance(&mut self, h: usize) {
        let size = self.memory.size();
        self.head_ref(h).advance(size, 1);
    }

    /// `getHead(h).Set(pos)` (adjusting).
    #[inline]
    pub(crate) fn head_set_pos(&mut self, h: usize, pos: i32) {
        let size = self.memory.size();
        self.head_ref(h).set(pos, 0, size, 1);
    }

    /// `getHead(h).Set(other)` (no adjust).
    #[inline]
    pub(crate) fn head_set_from(&mut self, h: usize, other: &HeadCpu) {
        self.head_ref(h).set_from(other);
    }

    /// `getHead(h).Jump(n)`.
    #[inline]
    pub(crate) fn head_jump(&mut self, h: usize, n: i32) {
        let size = self.memory.size();
        self.head_ref(h).jump(n, size, 1);
    }

    /// `getHead(h).Adjust()`.
    #[inline]
    pub(crate) fn head_adjust(&mut self, h: usize) {
        let size = self.memory.size();
        self.head_ref(h).adjust(size, 1);
    }

    /// `getIP().Advance()`.
    #[inline]
    pub(crate) fn ip_advance(&mut self) {
        self.head_advance(HEAD_IP);
    }

    /// `getIP().SetFlagExecuted()`.
    #[inline]
    pub(crate) fn ip_set_flag_executed(&mut self) {
        let ip = self.ip_head();
        ip.set_flag_executed(&mut self.memory);
    }

    /// `cHeadCPU(this, pos)`: a head at `pos`, adjusted unless 0.
    #[inline]
    pub(crate) fn make_head(&self, pos: i32) -> HeadCpu {
        HeadCpu::new(pos, 0, self.memory.size(), 1)
    }

    /// `GetActiveHead()` id.
    #[inline]
    pub(crate) fn active_head_id(&self) -> usize {
        (self.threads[self.ct()].cur_head as usize).min(NUM_HEADS - 1)
    }

    /// `GetLabel()`
    #[inline]
    pub(crate) fn label(&self) -> &CodeLabel {
        &self.threads[self.ct()].next_label
    }

    /// `GetLabel()`, mutable.
    #[inline]
    pub(crate) fn label_mut(&mut self) -> &mut CodeLabel {
        let ct = self.ct();
        &mut self.threads[ct].next_label
    }

    /// `GetReadLabel()`
    #[inline]
    pub(crate) fn read_label_ref(&self) -> &CodeLabel {
        &self.threads[self.ct()].read_label
    }

    /// The memory.
    pub fn memory_ref(&self) -> &CpuMemory {
        &self.memory
    }

    /// The threads.
    pub fn threads(&self) -> &[LocalThread] {
        &self.threads
    }

    /// `GetActiveStack()`
    pub fn active_stack(&self) -> i32 {
        i32::from(self.threads[self.ct()].cur_stack)
    }

    /// `GetThread(i)`
    pub fn thread(&self, i: usize) -> Option<&LocalThread> {
        self.threads.get(i)
    }

    // ------------------------------------------------------------------
    // Stack
    // ------------------------------------------------------------------

    /// `StackPush(value)`
    #[inline]
    pub(crate) fn stack_push(&mut self, value: i32) {
        let ct = self.ct();
        if self.threads[ct].cur_stack == 0 {
            self.threads[ct].stack.push(value);
        } else {
            self.global_stack.push(value);
        }
    }

    /// `StackPop()`
    #[inline]
    pub(crate) fn stack_pop(&mut self) -> i32 {
        let ct = self.ct();
        if self.threads[ct].cur_stack == 0 {
            self.threads[ct].stack.pop()
        } else {
            self.global_stack.pop()
        }
    }

    /// `StackFlip()`
    pub(crate) fn stack_flip(&mut self) {
        let ct = self.ct();
        if self.threads[ct].cur_stack == 0 {
            self.threads[ct].stack.flip();
        } else {
            self.global_stack.flip();
        }
    }

    /// `StackClear()`
    pub(crate) fn stack_clear(&mut self) {
        let ct = self.ct();
        if self.threads[ct].cur_stack == 0 {
            self.threads[ct].stack.clear();
        } else {
            self.global_stack.clear();
        }
    }

    /// `SwitchStack()`
    pub(crate) fn switch_stack(&mut self) {
        let ct = self.ct();
        let t = &mut self.threads[ct];
        t.cur_stack += 1;
        if t.cur_stack > 1 {
            t.cur_stack = 0;
        }
    }

    // ------------------------------------------------------------------
    // Resets
    // ------------------------------------------------------------------

    /// `internalReset()`.
    pub(crate) fn internal_reset(&mut self, cfg: &Config) {
        self.global_stack.clear();
        if apto_resize_threads(&mut self.threads, 1) {
            self.thread_realloc += 1;
        }
        self.threads[0].reset(0);
        self.thread_id_chart = 1;
        self.cur_thread = 0;
        if self.epigenetic_state {
            self.threads[0].reg = self.epigenetic_saved_reg;
            self.threads[0].stack = self.epigenetic_saved_stack;
        }
        self.mal_active = false;
        self.executedmatchstrings = false;

        // Promoter model (the live setting, as in C++; the instruction set
        // was checked for `promoter` by `check_inst_set`).
        if cfg.promoters_enabled != 0 {
            self.promoter_index = -1;
            self.promoter_offset = 0;
            self.promoters.clear();
            if let Ok(promoter_inst) = self.base.inst_set.get_inst("promoter") {
                for i in 0..self.memory.size() {
                    if self.memory[i] != promoter_inst {
                        continue;
                    }
                    match crate::hardware_cpu_insts_misc::numberate(
                        self,
                        cfg.inst_code_length,
                        i - 1,
                        -1,
                        cfg.promoter_code_size,
                    ) {
                        Ok(code) => self.promoters.push(CpuPromoter {
                            pos: i,
                            bit_code: code,
                            regulation: 0,
                        }),
                        Err(e) => {
                            self.reset_error.get_or_insert(e);
                            break;
                        }
                    }
                }
            }
        }

        self.last_cell_data = (false, 0);
        self.flash_info = (0, 0);
        self.cycle_counter = 0;
    }

    /// `internalResetOnFailedDivide()`.
    pub(crate) fn internal_reset_on_failed_divide(&mut self, cfg: &Config) {
        self.internal_reset(cfg);
        self.mal_active = true;
        self.advance_ip = false;
    }

    /// `cHardwareBase::Reset(ctx)` with the interface.
    pub(crate) fn full_reset(&mut self, org: &mut OrgState, world: &mut dyn OrgInterface) {
        crate::organism::hardware_reset(org, self, world);
        let cfg = world.config();
        self.base.reset_costs(cfg);
        self.internal_reset(cfg);
    }

    // ------------------------------------------------------------------
    // Labels
    // ------------------------------------------------------------------

    /// `ReadLabel(max_size)`: read the nops following the IP into the
    /// current label, leaving the IP on the last one.
    pub(crate) fn read_label(&mut self, max_label_exe_size: i32) {
        self.read_label_max(max_label_exe_size, CodeLabel::MAX_LENGTH as i32);
    }

    /// `ReadLabel(max_size)` with an explicit limit.
    pub(crate) fn read_label_max(&mut self, max_label_exe_size: i32, max_size: i32) {
        let ct = self.ct();
        let mut count = 0;
        self.threads[ct].next_label.clear();
        let is = &self.base.inst_set;
        let size = self.memory.size();
        loop {
            let ip = self.threads[ct].heads[HEAD_IP];
            if !(is.is_nop(ip.get_next_inst(&self.memory, is)) && count < max_size) {
                break;
            }
            count += 1;
            self.threads[ct].heads[HEAD_IP].advance(size, 1);
            let ip = self.threads[ct].heads[HEAD_IP];
            let m = is.nop_mod(ip.get_inst(&self.memory));
            self.threads[ct].next_label.add_nop(m);
            if self.threads[ct].next_label.get_size() <= max_label_exe_size {
                ip.set_flag_executed(&mut self.memory);
            }
        }
    }

    /// `ReadInst(in_inst)`: extend or clear the read label.
    pub(crate) fn read_inst(&mut self, in_inst: i32) {
        let is_nop = self.base.inst_set.is_nop(Instruction::new(in_inst));
        let ct = self.ct();
        if is_nop {
            self.threads[ct].read_label.add_nop(in_inst);
        } else {
            self.threads[ct].read_label.clear();
        }
    }

    /// `FindLabel(direction)`: search for the complement of the current
    /// label; the result is still at the IP if not found.
    pub(crate) fn find_label(&self, direction: i32) -> HeadCpu {
        let inst_ptr = self.ip_head();
        let mut search_head = inst_ptr;
        let search_label = self.label();
        if search_label.get_size() == 0 {
            return inst_ptr;
        }
        let found_pos = if direction < 0 {
            self.find_label_backward(search_label, inst_ptr.position() - search_label.get_size())
        } else if direction > 0 {
            self.find_label_forward(search_label, inst_ptr.position())
        } else {
            self.find_label_forward(search_label, 0)
        };
        if found_pos >= 0 {
            search_head.set(found_pos - 1, 0, self.memory.size(), 1);
        }
        search_head
    }

    #[inline]
    fn mem_inst(&self, pos: i32) -> Instruction {
        crate::head_cpu::inst_at(&self.memory, pos)
    }

    /// `FindLabel_Forward(search_label, memory, pos)`.
    pub(crate) fn find_label_forward(&self, search_label: &CodeLabel, mut pos: i32) -> i32 {
        let is = &self.base.inst_set;
        let size = self.memory.size();
        let search_start = pos;
        let label_size = search_label.get_size();
        let mut found_label = false;

        pos += label_size;
        while pos < size {
            if is.is_nop(self.mem_inst(pos)) {
                let mut start_pos = pos;
                let mut end_pos = pos + 1;
                while start_pos > search_start && is.is_nop(self.mem_inst(start_pos - 1)) {
                    start_pos -= 1;
                }
                while end_pos < size && is.is_nop(self.mem_inst(end_pos)) {
                    end_pos += 1;
                }
                let test_size = end_pos - start_pos;
                let max_offset = test_size - label_size + 1;
                let mut offset = start_pos;
                while offset < start_pos + max_offset {
                    let mut matches = 0;
                    while matches < label_size {
                        let inst = self.mem_inst(offset + matches);
                        if !is.is_nop(inst) || search_label.get(matches) != is.nop_mod(inst) {
                            break;
                        }
                        matches += 1;
                    }
                    if matches == label_size {
                        found_label = true;
                        break;
                    }
                    offset += 1;
                }
                if found_label {
                    pos = label_size + offset;
                    break;
                }
                pos = end_pos;
            }
            pos += label_size;
        }
        if !found_label {
            pos = -1;
        }
        pos
    }

    /// `FindLabel_Backward(search_label, memory, pos)`.
    pub(crate) fn find_label_backward(&self, search_label: &CodeLabel, mut pos: i32) -> i32 {
        let is = &self.base.inst_set;
        let search_start = pos;
        let label_size = search_label.get_size();
        let mut found_label = false;

        pos -= label_size;
        while pos >= 0 {
            if is.is_nop(self.mem_inst(pos)) {
                let mut start_pos = pos;
                let mut end_pos = pos + 1;
                while start_pos > 0 && is.is_nop(self.mem_inst(start_pos - 1)) {
                    start_pos -= 1;
                }
                while end_pos < search_start && is.is_nop(self.mem_inst(end_pos)) {
                    end_pos += 1;
                }
                let test_size = end_pos - start_pos;
                let max_offset = test_size - label_size + 1;
                let mut offset = start_pos;
                while offset < start_pos + max_offset {
                    let mut matches = 0;
                    while matches < label_size {
                        let inst = self.mem_inst(offset + matches);
                        if !is.is_nop(inst) || search_label.get(matches) != is.nop_mod(inst) {
                            break;
                        }
                        matches += 1;
                    }
                    if matches == label_size {
                        found_label = true;
                        break;
                    }
                    offset += 1;
                }
                if found_label {
                    pos = end_pos;
                    break;
                }
                pos = start_pos - 1;
            }
            pos -= label_size;
        }
        if !found_label {
            pos = -1;
        }
        pos
    }

    /// `FindLabelInMemory(label, search_head)` over `mem`.
    pub(crate) fn find_label_in_memory(
        is: &InstSet,
        mem: &CpuMemory,
        label: &CodeLabel,
        search_head: &mut HeadCpu,
    ) {
        let size = mem.size();
        let nop_at = |h: &HeadCpu, off: i32| {
            let inst = h.get_inst_offset(mem, off);
            is.is_nop(inst)
        };
        while search_head.in_memory(size) {
            if !nop_at(search_head, 0) {
                search_head.abs_jump(label.get_size());
                continue;
            }
            while !search_head.at_front() && nop_at(search_head, -1) {
                search_head.abs_jump(-1);
            }
            let mut sz = 0;
            let mut label_match = true;
            loop {
                if sz < label.get_size() {
                    let inst = search_head.get_inst(mem);
                    if !is.is_nop(inst) || label.get(sz) != is.nop_mod(inst) {
                        label_match = false;
                    }
                }
                search_head.abs_jump(1);
                sz += 1;
                if !(search_head.in_memory(size) && nop_at(search_head, 0)) {
                    break;
                }
            }
            if sz != label.get_size() {
                continue;
            }
            if label_match {
                return;
            }
        }
        search_head.abs_set(-1);
    }

    /// `AdjustHeads()`.
    pub(crate) fn adjust_heads(&mut self) {
        let size = self.memory.size();
        for t in self.threads.iter_mut() {
            for h in t.heads.iter_mut() {
                h.adjust(size, 1);
            }
        }
    }

    // ------------------------------------------------------------------
    // Modifiers
    // ------------------------------------------------------------------

    /// `FindModifiedRegister(default_register)`.
    #[inline]
    pub(crate) fn find_modified_register(&mut self, default_register: usize) -> usize {
        let ct = self.ct();
        let is = &self.base.inst_set;
        let ip = self.threads[ct].heads[HEAD_IP];
        if is.is_nop(ip.get_next_inst(&self.memory, is)) {
            let size = self.memory.size();
            self.threads[ct].heads[HEAD_IP].advance(size, 1);
            let ip = self.threads[ct].heads[HEAD_IP];
            let r = is.nop_mod(ip.get_inst(&self.memory));
            ip.set_flag_executed(&mut self.memory);
            return (r.max(0) as usize).min(NUM_REGISTERS - 1);
        }
        default_register
    }

    /// `FindModifiedNextRegister(default_register)`.
    pub(crate) fn find_modified_next_register(&mut self, default_register: usize) -> usize {
        let ct = self.ct();
        let is = &self.base.inst_set;
        let ip = self.threads[ct].heads[HEAD_IP];
        if is.is_nop(ip.get_next_inst(&self.memory, is)) {
            self.find_modified_register(default_register)
        } else {
            (default_register + 1) % NUM_REGISTERS
        }
    }

    /// `FindModifiedHead(default_head)`.
    #[inline]
    pub(crate) fn find_modified_head(&mut self, default_head: usize) -> usize {
        let ct = self.ct();
        let is = &self.base.inst_set;
        let ip = self.threads[ct].heads[HEAD_IP];
        if is.is_nop(ip.get_next_inst(&self.memory, is)) {
            let size = self.memory.size();
            self.threads[ct].heads[HEAD_IP].advance(size, 1);
            let ip = self.threads[ct].heads[HEAD_IP];
            let h = is.nop_mod(ip.get_inst(&self.memory));
            ip.set_flag_executed(&mut self.memory);
            return (h.max(0) as usize).min(NUM_HEADS - 1);
        }
        default_head
    }

    /// `FindNextRegister(base_reg)`.
    #[inline]
    pub(crate) fn find_next_register(base_reg: usize) -> usize {
        (base_reg + 1) % NUM_REGISTERS
    }

    // ------------------------------------------------------------------
    // Threads
    // ------------------------------------------------------------------

    /// `ForkThread()`.
    pub(crate) fn fork_thread(&mut self, max_cpu_threads: i32) -> bool {
        let num_threads = self.threads.len();
        if num_threads as i32 == max_cpu_threads {
            return false;
        }
        if apto_resize_threads(&mut self.threads, num_threads + 1) {
            self.thread_realloc += 1;
        }
        let src = self.threads[self.ct()].clone();
        self.threads[num_threads].assign(&src);
        let mut new_id = 0;
        while new_id < 31 && (self.thread_id_chart >> new_id) & 1 != 0 {
            new_id += 1;
        }
        self.threads[num_threads].id = new_id;
        self.thread_id_chart |= 1i32.wrapping_shl(new_id as u32);
        true
    }

    /// `ThreadPrev()`.
    pub(crate) fn thread_prev(&mut self) {
        if self.cur_thread == 0 {
            self.cur_thread = self.threads.len().saturating_sub(1);
        } else {
            self.cur_thread -= 1;
        }
    }

    /// `KillThread()`.
    pub(crate) fn kill_thread(&mut self) -> bool {
        if self.threads.len() == 1 {
            return false;
        }
        let kill_thread = self.cur_thread;
        self.thread_prev();
        if let Some(t) = self.threads.get(kill_thread) {
            self.thread_id_chart ^= 1i32.wrapping_shl(t.id as u32);
        }
        let last_thread = self.threads.len() - 1;
        if last_thread != kill_thread && kill_thread < self.threads.len() {
            let src = self.threads[last_thread].clone();
            self.threads[kill_thread].assign(&src);
        }
        let n = self.threads.len() - 1;
        if apto_resize_threads(&mut self.threads, n) {
            self.thread_realloc += 1;
        }
        if self.cur_thread > kill_thread {
            self.cur_thread -= 1;
        }
        true
    }

    // ------------------------------------------------------------------
    // Allocation and divide
    // ------------------------------------------------------------------

    /// `Allocate_Main(ctx, allocated_size)`.
    pub(crate) fn allocate_main(
        &mut self,
        org: &mut OrgState,
        world: &mut dyn OrgInterface,
        allocated_size: i32,
    ) -> bool {
        let cfg = world.config();
        if cfg.require_allocate != 0 && self.mal_active {
            org.fault(FAULT_LOC_ALLOC, FAULT_TYPE_ERROR, "Allocate already active");
            return false;
        }
        if allocated_size < 1 {
            org.fault(
                FAULT_LOC_ALLOC,
                FAULT_TYPE_ERROR,
                &format!("Allocate of {} too small", allocated_size),
            );
            return false;
        }
        let old_size = self.memory.size();
        let new_size = old_size.wrapping_add(allocated_size);
        if !(MIN_GENOME_LENGTH..=MAX_GENOME_LENGTH).contains(&new_size) {
            org.fault(
                FAULT_LOC_ALLOC,
                FAULT_TYPE_ERROR,
                &format!("Invalid post-allocate size ({})", new_size),
            );
            return false;
        }
        let max_alloc_size = (f64::from(old_size) * cfg.offspring_size_range) as i32;
        if allocated_size > max_alloc_size {
            org.fault(
                FAULT_LOC_ALLOC,
                FAULT_TYPE_ERROR,
                &format!(
                    "Allocate too large ({} > {})",
                    allocated_size, max_alloc_size
                ),
            );
            return false;
        }
        let max_old_size = (f64::from(allocated_size) * cfg.offspring_size_range) as i32;
        if old_size > max_old_size {
            org.fault(
                FAULT_LOC_ALLOC,
                FAULT_TYPE_ERROR,
                &format!("Allocate too small ({} > {})", old_size, max_old_size),
            );
            return false;
        }
        match cfg.alloc_method {
            ALLOC_METHOD_NECRO => {
                // Allocate_Necro always succeeds.
                self.memory.resize_old(new_size);
            }
            ALLOC_METHOD_RANDOM => {
                self.memory.resize(new_size);
                for i in old_size..new_size {
                    let inst = self.base.inst_set.random_inst(world.rng());
                    self.memory[i] = inst;
                }
            }
            ALLOC_METHOD_DEFAULT => {
                self.memory.resize(new_size);
            }
            _ => {}
        }
        self.mal_active = true;
        true
    }

    /// `calcCopiedSize(parent_size, child_size)`.
    pub(crate) fn calc_copied_size(&self, parent_size: i32, child_size: i32) -> i32 {
        let end = parent_size.wrapping_add(child_size).min(self.memory.size());
        (parent_size.max(0)..end)
            .filter(|i| self.memory.flag_copied(*i))
            .count() as i32
    }

    /// `cHardwareBase::Divide_CheckViable` with this hardware's
    /// `calcCopiedSize` and `internalResetOnFailedDivide`.
    pub(crate) fn check_viable(
        &mut self,
        org: &mut OrgState,
        world: &mut dyn OrgInterface,
        parent_size: i32,
        child_size: i32,
        using_repro: bool,
    ) -> bool {
        let copied = if using_repro {
            None
        } else {
            Some(self.calc_copied_size(parent_size, child_size))
        };
        match divide_check_viable(org, world, &self.memory, parent_size, child_size, copied) {
            Viability::Viable => true,
            Viability::NotViable => false,
            Viability::NotViableReset => {
                self.internal_reset_on_failed_divide(world.config());
                false
            }
        }
    }

    /// The offspring genome cut from memory (`m_memory.Crop(...)` with the
    /// inst set name property).
    fn crop_offspring(&self, div_point: i32, child_size: i32) -> Genome {
        let seq: InstructionSequence = self.memory.crop(div_point, div_point + child_size);
        Genome::with_inst_set(
            HARDWARE_TYPE_CPU_ORIGINAL,
            self.base.inst_set.inst_set_name(),
            seq,
        )
    }

    /// `cOrganism::DoOutput` plus the bonus instructions it triggers.
    pub(crate) fn org_do_output(
        &mut self,
        org: &mut OrgState,
        world: &mut dyn OrgInterface,
        value: Option<i32>,
        on_divide: bool,
    ) -> Result<(), String> {
        let is = self.base.inst_set.clone();
        let effects = do_output(
            org,
            &self.base.ext_mem,
            &is,
            world,
            value,
            on_divide,
            false,
            None,
        )?;
        for inst in effects.bonus_insts {
            self.process_bonus_inst(org, world, inst)?;
        }
        Ok(())
    }

    /// `cOrganism::ActivateDivide(ctx)`: the last output test (and its bonus
    /// instructions), then `m_interface->Divide`. Returns whether the
    /// parent is still alive.
    pub(crate) fn activate_divide(
        &mut self,
        org: &mut OrgState,
        world: &mut dyn OrgInterface,
    ) -> Result<bool, String> {
        self.org_do_output(org, world, None, true)?;
        let offspring = org.offspring_genome.clone();
        let split_success = self.print_mini_trace_success(1);
        Ok(crate::mini_trace::divide_lending_traces(
            self,
            split_success,
            org,
            world,
            &offspring,
        ))
    }

    /// The common tail of the divide methods: first-time costs, malloc
    /// flag, split-mode IP, activation, and the parent's reset.
    fn divide_finish(
        &mut self,
        org: &mut OrgState,
        world: &mut dyn OrgInterface,
        inherit_parent_state: bool,
    ) -> Result<(), String> {
        let divide_method = world.config().divide_method;
        if divide_method != DIVIDE_METHOD_OFFSPRING {
            self.base.reset_ft_costs();
        }
        self.mal_active = false;
        if divide_method == DIVIDE_METHOD_SPLIT {
            self.advance_ip = false;
        }
        let parent_alive = self.activate_divide(org, world)?;
        if parent_alive {
            let epi = world.config().epigenetic_method;
            if inherit_parent_state
                && (epi == EPIGENETIC_METHOD_PARENT || epi == EPIGENETIC_METHOD_BOTH)
            {
                self.inherit_state_from_self();
            }
            if world.config().divide_method == DIVIDE_METHOD_SPLIT {
                self.full_reset(org, world);
            }
            self.memory.clear_flags();
        }
        Ok(())
    }

    /// `Divide_Main(ctx, div_point, extra_lines, mut_multiplier)`.
    pub(crate) fn divide_main(
        &mut self,
        org: &mut OrgState,
        world: &mut dyn OrgInterface,
        div_point: i32,
        extra_lines: i32,
        mut_multiplier: f64,
    ) -> Result<bool, String> {
        let child_size = self.memory.size() - div_point - extra_lines;
        if !self.check_viable(org, world, div_point, child_size, false) {
            return Ok(false);
        }
        let offspring = self.crop_offspring(div_point, child_size);
        if world.config().require_exact_copy != 0
            && org.initial_genome.representation() != offspring.representation()
        {
            return Ok(false);
        }
        org.offspring_genome = offspring;
        self.memory.resize(div_point);
        let is = self.base.inst_set.clone();
        divide_do_mutations(&is, org, world, &mut self.memory, mut_multiplier, i32::MAX)?;
        divide_test_fitness_measures(org, world, true);
        self.divide_finish(org, world, true)?;
        Ok(true)
    }

    /// `Divide_MainRS`, `Divide_Main1RS`, `Divide_Main2RS` (`variant` 0, 1,
    /// 2): divide with resampling of reverted offspring.
    pub(crate) fn divide_main_rs(
        &mut self,
        org: &mut OrgState,
        world: &mut dyn OrgInterface,
        div_point: i32,
        extra_lines: i32,
        mut_multiplier: f64,
        variant: i32,
    ) -> Result<bool, String> {
        let child_size = self.memory.size() - div_point - extra_lines;
        if !self.check_viable(org, world, div_point, child_size, false) {
            return Ok(false);
        }
        org.offspring_genome = self.crop_offspring(div_point, child_size);
        self.memory.resize(div_point);
        let is = self.base.inst_set.clone();

        let mut fit_test = false;
        match variant {
            0 => {
                let mut total = 0;
                for i in 0..=100 {
                    let mutations;
                    if i == 0 {
                        mutations = divide_do_mutations(
                            &is,
                            org,
                            world,
                            &mut self.memory,
                            mut_multiplier,
                            i32::MAX,
                        )?;
                        total = mutations;
                    } else {
                        mutations = divide_do_mutations(
                            &is,
                            org,
                            world,
                            &mut self.memory,
                            mut_multiplier,
                            i32::MAX,
                        )?;
                        world.stats_inc_resamplings();
                    }
                    fit_test = divide_test_fitness_measures(org, world, true);
                    if !fit_test && mutations >= total {
                        break;
                    }
                }
            }
            1 => {
                let total =
                    divide_do_mutations(&is, org, world, &mut self.memory, mut_multiplier, 1)?;
                let mut mutations = total;
                for i in 0..100 {
                    if i > 0 {
                        mutations = divide_do_exact_mutations(&is, org, world, mut_multiplier, 1);
                        world.stats_inc_resamplings();
                    }
                    fit_test = divide_test_fitness_measures(org, world, true);
                    if !fit_test && mutations >= total {
                        break;
                    }
                }
            }
            _ => {
                let mut total = 0;
                let mut mutations = 0;
                for i in 0..100 {
                    if i == 0 {
                        mutations = divide_do_mutations(
                            &is,
                            org,
                            world,
                            &mut self.memory,
                            mut_multiplier,
                            2,
                        )?;
                        total = mutations;
                    } else {
                        divide_do_exact_mutations(&is, org, world, mut_multiplier, mutations);
                        world.stats_inc_resamplings();
                    }
                    fit_test = divide_test_fitness_measures(org, world, false);
                    if !fit_test && mutations >= total {
                        break;
                    }
                }
            }
        }
        if fit_test {
            org.phenotype.child_fertile = false;
            world.stats_inc_failed_resamplings();
        }
        self.divide_finish(org, world, false)?;
        Ok(true)
    }

    /// `InheritState(*this)` (epigenetic parent state).
    fn inherit_state_from_self(&mut self) {
        self.epigenetic_state = true;
        let ct = self.ct();
        self.epigenetic_saved_reg = self.threads[ct].reg;
        self.epigenetic_saved_stack = self.threads[ct].stack;
    }

    // ------------------------------------------------------------------
    // Execution
    // ------------------------------------------------------------------

    /// `SingleProcess_ExecuteInst(ctx, cur_inst)`.
    pub(crate) fn execute_inst(
        &mut self,
        org: &mut OrgState,
        world: &mut dyn OrgInterface,
        cur_inst: Instruction,
    ) -> Result<bool, String> {
        let op = cur_inst.op();
        if op >= self.base.inst_set.size() {
            // UB substitute: an opcode outside the set (only reachable via
            // the opcode-255 quirks) executes as a failed no-op.
            return Ok(false);
        }
        let inst_idx = self.base.inst_set.lib_function_index(cur_inst);
        if let Some(c) = org.phenotype.cur_inst_count.get_mut(op as usize) {
            *c = c.wrapping_add(1);
        }
        // Every instruction of the set has a handler: `HardwareManager::create`
        // refuses a set with an unported one (`check_inst_set_ported`).
        let Some(f) = self
            .functions
            .get(inst_idx.max(0) as usize)
            .copied()
            .flatten()
        else {
            return Ok(false);
        };
        let exec_success = f(self, org, world)?;

        if world.config().task_switch_penalty_type != 0
            && org.phenotype.num_new_unique_reactions != 0
        {
            let cost = org
                .phenotype
                .num_new_unique_reactions
                .wrapping_mul(world.config().task_switch_penalty);
            self.base.increment_task_switching_cost(cost);
            org.phenotype.num_new_unique_reactions = 0;
        }

        if !exec_success {
            if let Some(c) = org.phenotype.cur_inst_count.get_mut(op as usize) {
                *c = c.wrapping_sub(1);
            }
        }
        Ok(exec_success)
    }

    /// `m_tracer->TraceHardware(ctx, *this, bonus)`.
    fn trace(&self, org: &OrgState, world: &mut dyn OrgInterface, bonus: bool) {
        let Some(tracer) = self.base.tracer.clone() else {
            return;
        };
        let mut guard = match tracer.lock() {
            Ok(g) => g,
            Err(p) => p.into_inner(),
        };
        let me: &HardwareCpu = self;
        guard.trace_hardware(
            &mut || print_status(org, me, world),
            &mut String::new,
            &mut |_| String::new(),
            bonus,
            false,
            -2,
        );
    }

    /// `Inst_Repro` for implicit reproduction.
    fn check_implicit_repro(
        &mut self,
        org: &mut OrgState,
        world: &mut dyn OrgInterface,
        exec_last_inst: bool,
    ) -> Result<(), String> {
        if self.base.implicit_repro_active
            && implicit_repro_due(org, world.config(), exec_last_inst)
        {
            self.inst_repro(org, world)?;
        }
        Ok(())
    }
}

impl Hardware for HardwareCpu {
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
        HARDWARE_TYPE_CPU_ORIGINAL
    }
    fn supports_speculative(&self) -> bool {
        true
    }

    /// Everything a speculative step does besides its instruction stays
    /// in the organism unless one of these is on: parallel thread slicing
    /// (several instructions per step, which speculation cannot cut),
    /// promoters and constitutive regulation (`promoter_terminate`,
    /// `sense_regulate`), implicit reproduction (a divide after any
    /// instruction), resource costs and the task-switching penalty (both
    /// reach the world in `pay_pre_costs`), and any trace.
    fn parallel_capable(&self) -> bool {
        let b = &self.base;
        !self.thread_slicing_parallel
            && !self.promoters_enabled
            && !self.constitutive_regulation
            && !b.implicit_repro_active
            && !b.has_res_costs
            && !b.has_fem_res_costs
            && b.task_switching_cost == 0
            && b.tracer.is_none()
            && !b.minitrace
            && !b.microtrace
            && !b.topnavtrace
            && !b.reprotrace
    }

    fn reset(&mut self, org: &mut OrgState, world: &mut dyn OrgInterface) {
        self.full_reset(org, world);
    }

    /// `SingleProcess(ctx, speculative)`.
    ///
    /// UB substitute: the C++ keeps a reference to the IP of the thread
    /// slot that was current before the instruction ran and advances it
    /// afterwards. An instruction that reallocates the thread array (fork,
    /// kill, a reset with several threads) leaves that reference dangling,
    /// and the advance lands in freed memory. The port drops the advance in
    /// that case.
    fn single_process(
        &mut self,
        org: &mut OrgState,
        world: &mut dyn OrgInterface,
        speculative: bool,
    ) -> Result<bool, String> {
        if let Some(e) = self.reset_error.take() {
            return Err(e);
        }
        let last_ip_pos = self.ip_head().position();
        org.set_running(true);

        if !speculative && self.spec_die {
            org.die(world);
            org.set_running(false);
            return Ok(false);
        }

        // First instruction: start at a promoter, when enabled.
        if org.phenotype.cpu_cycles_used == 0 && self.promoters_enabled {
            self.promoter_terminate(org, world)?;
        }

        org.phenotype.inc_cpu_cycles_used();
        if world.config().no_cpu_cycle_time == 0 {
            org.phenotype.inc_time_used(1);
        }

        let mut num_threads = self.threads.len() as i32;
        let mut num_inst_exec = if self.thread_slicing_parallel {
            num_threads
        } else {
            1
        };

        let mut i = 0;
        while i < num_inst_exec {
            let last_thread = self.cur_thread;
            self.cur_thread += 1;
            if self.cur_thread as i32 >= num_threads {
                self.cur_thread = 0;
            }
            let ip_thread = self.ct();

            self.advance_ip = true;
            let size = self.memory.size();
            self.threads[ip_thread].heads[HEAD_IP].adjust(size, 1);

            if self.base.tracer.is_some() {
                self.trace(org, world, false);
            }

            let cur_inst = self.threads[ip_thread].heads[HEAD_IP].get_inst(&self.memory);
            let valid = cur_inst.op() < self.base.inst_set.size();

            if speculative
                && (self.spec_die
                    || (valid
                        && (if self.base.parallel_burst {
                            let idx = self.base.inst_set.lib_function_index(cur_inst);
                            // The input and output instructions are marked
                            // `STALL` and run in a burst when they are
                            // organism-local in this run.
                            let io = self.base.burst_io && crate::parallel::cpu_io_ok(idx);
                            // A divide runs in the burst when nothing it
                            // would leave to the world could be undone by
                            // what a step does after the instruction: it
                            // costs no extra time, and the organism neither
                            // dies of age nor is to die (the exact order
                            // resets the clock first, `ActivateOffspring`
                            // runs after the burst here).
                            let divide = self.base.burst_divide
                                && crate::parallel::cpu_divide_ok(idx)
                                && (org.max_executed <= 0
                                    || org.phenotype.time_used < org.max_executed)
                                && !org.phenotype.to_die
                                && org.mut_rates.divide.lgt_prob == 0.0
                                && org.mut_rates.divide.divide_lgt_prob == 0.0
                                && org.mut_rates.divide.divide_poisson_lgt_mean == 0.0
                                && self.base.inst_set.addl_time_cost(cur_inst) == 0;
                            !(io || divide
                                || (!self.base.inst_set.should_stall(cur_inst)
                                    && crate::parallel::cpu_parallel_ok(idx)))
                        } else {
                            self.base.inst_set.should_stall(cur_inst)
                        })))
            {
                self.cur_thread = last_thread;
                org.phenotype.dec_cpu_cycles_used();
                if !self.no_cpu_cycle_time {
                    org.phenotype.inc_time_used(-1);
                }
                org.set_running(false);
                return Ok(false);
            }

            let mut exec = true;
            if self.base.has_any_costs && valid {
                exec = self.base.pay_pre_costs(org, world, cur_inst, self.ct());
            }

            // Constitutive regulation applied here.
            if self.constitutive_regulation {
                self.sense_regulate(org, world)?;
            }

            // No active promoter under NO_ACTIVE_PROMOTER_EFFECT 2: nothing
            // executes.
            if self.promoters_enabled
                && world.config().no_active_promoter_effect == 2
                && self.promoter_index == -1
            {
                exec = false;
            }

            if exec {
                let time_cost = if valid {
                    self.base.inst_set.addl_time_cost(cur_inst)
                } else {
                    0
                };
                if valid {
                    let prob_fail = self.base.inst_set.prob_fail(cur_inst);
                    if prob_fail > 0.0 {
                        exec = !world.rng().p(prob_fail);
                    }
                }

                self.ip_set_flag_executed();

                // Count before executing (in case it is a terminator).
                if self.promoters_enabled {
                    let ct = self.ct();
                    let t = &mut self.threads[ct];
                    t.promoter_inst_executed = t.promoter_inst_executed.wrapping_add(1);
                }

                let realloc_before = self.thread_realloc;
                if exec && self.execute_inst(org, world, cur_inst)? {
                    self.base.pay_post_res_costs(org, world, cur_inst);
                    let ct = self.ct();
                    self.base.set_post_cpu_costs(cur_inst, ct);
                }

                if org.phenotype.to_delete {
                    break;
                }

                if self.advance_ip
                    && self.thread_realloc == realloc_before
                    && ip_thread < self.threads.len()
                {
                    let size = self.memory.size();
                    self.threads[ip_thread].heads[HEAD_IP].advance(size, 1);
                }

                org.phenotype.inc_time_used(time_cost);

                // The promoter model may force termination after an
                // instruction (one RNG draw per instruction even at a
                // processivity of 1).
                if self.promoters_enabled {
                    let processivity = world.config().promoter_processivity;
                    if world.rng().p(1.0 - processivity) {
                        self.promoter_terminate(org, world)?;
                    }
                    let inst_max = world.config().promoter_inst_max;
                    if inst_max != 0 && self.threads[self.ct()].promoter_inst_executed >= inst_max {
                        self.promoter_terminate(org, world)?;
                    }
                }

                let len = self.threads.len() as i32;
                if num_threads == len + 1 {
                    num_threads -= 1;
                    num_inst_exec -= 1;
                } else if num_threads > len && len == 1 {
                    num_threads = 1;
                    num_inst_exec = 0;
                } else if num_threads > len {
                    eprintln!(
                        "{} {} {} {}",
                        cur_inst.op(),
                        cur_inst.symbol(),
                        num_threads,
                        len
                    );
                    org.fault(FAULT_LOC_DEFAULT, FAULT_TYPE_ERROR, "");
                    eprintln!("Error in thread handling");
                    return Err("Error in thread handling (C++ exits with status -1)".to_string());
                }
            }
            i += 1;
        }

        // A gradient habitat's `SetToDie` of this organism (see
        // `OrgInterface::take_habitat_to_die`).
        if world.take_habitat_to_die() {
            org.phenotype.to_die = true;
        }
        let max_executed = org.max_executed;
        if (max_executed > 0 && org.phenotype.time_used >= max_executed) || org.phenotype.to_die {
            if speculative {
                self.spec_die = true;
            } else {
                org.die(world);
            }
        }
        if !speculative && org.phenotype.to_delete {
            self.spec_die = true;
        }

        let cur_ip_pos = self.ip_head().position();
        self.check_implicit_repro(org, world, last_ip_pos > cur_ip_pos)?;

        org.set_running(false);
        Ok(!self.spec_die)
    }

    /// `ProcessBonusInst(ctx, inst)`.
    fn process_bonus_inst(
        &mut self,
        org: &mut OrgState,
        world: &mut dyn OrgInterface,
        inst: Instruction,
    ) -> Result<(), String> {
        let prev_run_state = org.is_running;
        org.set_running(true);
        if self.base.tracer.is_some() {
            self.trace(org, world, true);
        }
        self.execute_inst(org, world, inst)?;
        org.set_running(prev_run_state);
        Ok(())
    }

    /// `PrintStatus(fp)`.
    fn print_status(&self, org: &OrgState, world: &mut dyn OrgInterface) -> String {
        use std::fmt::Write;
        let mut s = String::new();
        let ip = self.ip_head();
        let ip_inst = ip.get_inst(&self.memory);
        let name = if ip_inst.op() < self.base.inst_set.size() {
            self.base.inst_set.name_of(ip_inst).to_string()
        } else {
            String::new()
        };
        let _ = writeln!(
            s,
            "{} IP:{} ({})",
            org.phenotype.cpu_cycles_used,
            ip.position(),
            name
        );
        for i in 0..NUM_REGISTERS {
            let r = self.reg(i);
            let _ = write!(
                s,
                "{}X:{} [0x{:x}]  ",
                (b'A' + i as u8) as char,
                r,
                r as u32
            );
        }
        if crate::organism::is_interrupted(self) {
            s.push_str("  Interrupted");
        }
        if org.phenotype.time_used != org.phenotype.cpu_cycles_used {
            let _ = write!(s, "  EnergyUsed:{}", org.phenotype.time_used);
        }
        s.push('\n');
        let _ = writeln!(
            s,
            "  R-Head:{} W-Head:{} F-Head:{}  RL:{}   ",
            self.get_head(HEAD_READ).position(),
            self.get_head(HEAD_WRITE).position(),
            self.get_head(HEAD_FLOW).position(),
            self.read_label_ref().as_string()
        );
        let cur_stack = self.threads[self.ct()].cur_stack;
        for stack_id in 0..2 {
            let mark = if i32::from(cur_stack) == stack_id {
                '*'
            } else {
                ' '
            };
            let _ = write!(s, "{} Stack {}:", mark, stack_id);
            for i in 0..STACK_SIZE {
                let _ = write!(s, " Ox{:08x}", self.get_stack(i, stack_id, 0) as u32);
            }
            s.push('\n');
        }
        let _ = writeln!(
            s,
            "  Mem ({}):  {}",
            self.memory.size(),
            self.memory.as_string()
        );
        let ext = &self.base.ext_mem;
        if !ext.is_empty() {
            let _ = write!(s, "  Ext Mem: {}", ext[0]);
            for v in &ext[1..] {
                let _ = write!(s, ", {}", v);
            }
            s.push('\n');
        }
        if world.config().promoters_enabled != 0 {
            let _ = write!(
                s,
                "  Promoters: index={} offset={} exe_inst={}",
                self.promoter_index,
                self.promoter_offset,
                self.threads[self.ct()].promoter_inst_executed
            );
            for p in &self.promoters {
                let _ = write!(s, " {}:Ox{:08x} ", p.pos, p.regulated_bit_code() as u32);
            }
            s.push('\n');
        }
        s
    }

    fn get_stack(&self, depth: i32, stack_id: i32, in_thread: i32) -> i32 {
        let in_thread = if in_thread >= self.threads.len() as i32 || in_thread < 0 {
            self.ct()
        } else {
            in_thread as usize
        };
        let stack_id = if stack_id == -1 {
            i32::from(self.threads[in_thread].cur_stack)
        } else {
            stack_id
        };
        match stack_id {
            0 => self.threads[in_thread].stack.get(depth),
            1 => self.global_stack.get(depth),
            _ => 0,
        }
    }

    fn get_cur_stack(&self, in_thread: i32) -> i32 {
        let in_thread = if in_thread >= self.threads.len() as i32 || in_thread < 0 {
            self.ct()
        } else {
            in_thread as usize
        };
        i32::from(self.threads[in_thread].cur_stack)
    }

    fn num_stacks(&self) -> i32 {
        2
    }

    fn head(&self, head_id: usize) -> &HeadCpu {
        &self.threads[self.ct()].heads[head_id.min(NUM_HEADS - 1)]
    }

    fn head_mut(&mut self, head_id: usize) -> &mut HeadCpu {
        self.head_ref(head_id.min(NUM_HEADS - 1))
    }

    fn head_in_thread(&self, head_id: usize, thread: usize) -> &HeadCpu {
        let t = if thread < self.threads.len() {
            thread
        } else {
            self.ct()
        };
        &self.threads[t].heads[head_id.min(NUM_HEADS - 1)]
    }

    fn num_heads(&self) -> i32 {
        NUM_HEADS as i32
    }

    fn memory(&self) -> &CpuMemory {
        &self.memory
    }

    fn memory_mut(&mut self) -> &mut CpuMemory {
        &mut self.memory
    }

    fn register(&self, reg_id: usize) -> i32 {
        self.reg(reg_id.min(NUM_REGISTERS - 1))
    }

    fn num_registers(&self) -> i32 {
        NUM_REGISTERS as i32
    }

    fn num_threads(&self) -> i32 {
        self.threads.len() as i32
    }

    fn cur_thread(&self) -> i32 {
        self.cur_thread as i32
    }

    fn interrupt_thread(
        &mut self,
        org: &mut OrgState,
        world: &mut dyn OrgInterface,
        interrupt_type: i32,
    ) -> Result<bool, String> {
        self.interrupt_thread_impl(org, world, interrupt_type)
    }

    fn jump_to_alarm_label(&mut self, org: &OrgState, jump_label: i32) -> Result<bool, String> {
        if org.is_sleeping {
            return Ok(false);
        }
        self.jump_to_alarm_label_impl(jump_label)
    }

    /// `ReceiveFlash()`: note that, and when (`m_cycle_counter`), a flash
    /// arrived.
    fn receive_flash(&mut self) -> Result<(), String> {
        self.flash_info = (1, self.cycle_counter);
        Ok(())
    }

    fn thread_message_trigger_type(&self, index: i32) -> i32 {
        self.threads
            .get(index.max(0) as usize)
            .map(|t| t.message_trigger_type)
            .unwrap_or(-1)
    }

    /// `InheritState(in_hardware)`: take the registers and stack of the
    /// parent's current thread as the epigenetic state.
    fn inherit_state(&mut self, parent: &dyn Hardware) {
        let Some(p) = parent.as_any().downcast_ref::<HardwareCpu>() else {
            return;
        };
        self.epigenetic_state = true;
        let pt = &p.threads[p.ct()];
        self.epigenetic_saved_reg = pt.reg;
        self.epigenetic_saved_stack = pt.stack;
        let ct = self.ct();
        self.threads[ct].reg = self.epigenetic_saved_reg;
        self.threads[ct].stack = self.epigenetic_saved_stack;
    }
}

impl HardwareCpu {
    /// `InterruptThread(interruptType)`.
    pub(crate) fn interrupt_thread_impl(
        &mut self,
        org: &mut OrgState,
        world: &mut dyn OrgInterface,
        interrupt_type: i32,
    ) -> Result<bool, String> {
        let mut interrupt_msg_type = -1;
        let handler_name = match interrupt_type {
            MSG_INTERRUPT => {
                // PeekAtNextMessageType(): front() of an empty deque is
                // undefined in C++; ReceiveMessage always pushes first.
                let Some(m) = org.msg_received.front() else {
                    return Err(
                        "message interrupt with no received message (C++: undefined deque::front)"
                            .to_string(),
                    );
                };
                interrupt_msg_type = m.message_type;
                format!("msg-handler-type{interrupt_msg_type}")
            }
            MOVE_INTERRUPT => "moved-handler".to_string(),
            _ => {
                eprintln!("Unknown intrerrupt type {}  Exitting.\n", interrupt_type);
                return Err(format!(
                    "Unknown interrupt type {} (C++ exits with status -1)",
                    interrupt_type
                ));
            }
        };
        let label_inst = self.base.inst_set.get_inst(&handler_name)?;
        let mut search_head = self.ip_head();
        let size = self.memory.size();
        let start_pos = search_head.position();
        search_head.advance(size, 1);
        while start_pos != search_head.position() {
            if search_head.get_inst(&self.memory) == label_inst {
                search_head.advance(size, 1);
                break;
            }
            search_head.advance(size, 1);
        }
        if start_pos == search_head.position() {
            return Ok(false);
        }
        if self.fork_thread(world.config().max_cpu_threads) {
            let num_threads = self.threads.len() - 1;
            self.threads[num_threads].message_trigger_type = interrupt_msg_type;
            for h in 0..NUM_HEADS {
                self.threads[num_threads].heads[h].set(search_head.position(), 0, size, 1);
            }
            if interrupt_type == MSG_INTERRUPT {
                self.msg_interrupt_retrieve(org, world, num_threads)?;
            }
            return Ok(true);
        }
        Ok(false)
    }
}
