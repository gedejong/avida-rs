//! Port of `cHardwareExperimental` (avida-core/source/cpu/
//! cHardwareExperimental.{h,cc} at 47f13dadb), hardware type 3: eight
//! registers whose values carry provenance (`DataValue`), eight heads,
//! threads that can wait on register conditions, label and nop-sequence
//! search, the promoter model, and the environment instructions of the
//! predator/prey and avatar experiments. The instruction handlers are in
//! [`crate::hardware_experimental_insts`] and
//! [`crate::hardware_experimental_insts_env`]; the instruction library
//! table is [`crate::hardware_experimental_lib`].
//!
//! The C++ thread array is an `Apto::Array<cLocalThread,
//! Apto::ManagedPointer>`: every element is a separate allocation, so
//! growing or shrinking it keeps the surviving threads in place (unlike the
//! heads CPU's `Basic` storage). A `Vec` that is only pushed to and
//! truncated has the same semantics.

use crate::code_label::CodeLabel;
use crate::hardware::{
    divide_check_viable, divide_do_mutations, divide_test_fitness_measures, implicit_repro_due,
    Hardware, HardwareBase, Viability,
};
use crate::hardware_experimental_insts::{exp_handler_table, ExpInstFn};
use crate::head_cpu::HeadCpu;
use crate::nhardware::*;
use crate::org_interface::OrgInterface;
use crate::org_sensor::OrgSensor;
use crate::organism::{do_output, ostream_double, print_status, OrgState};
use avida_config::config::Config;
use avida_genome::{CpuMemory, Genome, InstSet, Instruction, InstructionSequence};
use std::any::Any;
use std::sync::Arc;

/// `cHardwareExperimental::NUM_REGISTERS`
pub const NUM_REGISTERS: usize = 8;
/// `cHardwareExperimental::NUM_HEADS` (`max(nHardware::NUM_HEADS,
/// NUM_REGISTERS)`).
pub const NUM_HEADS_EXP: usize = if NUM_HEADS >= NUM_REGISTERS {
    NUM_HEADS
} else {
    NUM_REGISTERS
};
/// `cHardwareExperimental::NUM_NOPS`
pub const NUM_NOPS: i32 = NUM_REGISTERS as i32;

/// `rAX`
pub const R_AX: usize = 0;
/// `rBX`
pub const R_BX: usize = 1;
/// `rCX`
pub const R_CX: usize = 2;
/// `rDX`
pub const R_DX: usize = 3;

/// Mask of the 13-bit `originated` / `oldest_component` bit-fields.
const MASK13: u32 = 0x1FFF;

/// `cHardwareExperimental::DataValue`: a register or stack value and where
/// it came from. `originated` and `oldest_component` are 13-bit fields (the
/// cycle count modulo 8192).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct DataValue {
    /// `value`
    pub value: i32,
    /// `originated:13`
    pub originated: u32,
    /// `from_env:1`
    pub from_env: bool,
    /// `from_sensor:1`
    pub from_sensor: bool,
    /// `from_message:1`
    pub from_message: bool,
    /// `oldest_component:13`
    pub oldest_component: u32,
    /// `env_component:1`
    pub env_component: bool,
    /// `sensor_component:1`
    pub sensor_component: bool,
    /// `message_component:1`
    pub message_component: bool,
}

impl DataValue {
    /// `Clear()`
    #[inline]
    pub fn clear(&mut self) {
        *self = DataValue::default();
    }

    /// `operator=`.
    ///
    /// QUIRK: copies only `value`, `originated`, `from_env`,
    /// `oldest_component` and `env_component`; the sensor and message
    /// flags of the destination are kept. (The copy constructor copies
    /// everything; that is Rust's `Copy`.)
    #[inline]
    pub fn assign(&mut self, i: &DataValue) {
        self.value = i.value;
        self.originated = i.originated;
        self.from_env = i.from_env;
        self.oldest_component = i.oldest_component;
        self.env_component = i.env_component;
    }
}

/// `cHardwareExperimental::Stack`: a ring of `DataValue`s whose size is the
/// instruction set's stack size.
#[derive(Debug, Clone, Default)]
pub struct ExpStack {
    sp: i32,
    stack: Vec<DataValue>,
}

impl ExpStack {
    #[inline]
    fn sz(&self) -> i32 {
        self.stack.len() as i32
    }

    /// `Push(value)` (through `DataValue::operator=`). A zero-size stack
    /// (undefined in C++) drops the value.
    pub fn push(&mut self, value: &DataValue) {
        self.sp -= 1;
        if self.sp < 0 {
            self.sp = self.sz() - 1;
        }
        if let Some(slot) = self.stack.get_mut(self.sp.max(0) as usize) {
            slot.assign(value);
        }
    }

    /// `Pop()`: a full copy of the top, which is then cleared.
    pub fn pop(&mut self) -> DataValue {
        let idx = self.sp.max(0) as usize;
        let v = self.stack.get(idx).copied().unwrap_or_default();
        if let Some(slot) = self.stack.get_mut(idx) {
            slot.clear();
        }
        self.sp += 1;
        if self.sp == self.sz() {
            self.sp = 0;
        }
        v
    }

    /// `Peek()`
    pub fn peek(&self) -> DataValue {
        self.stack
            .get(self.sp.max(0) as usize)
            .copied()
            .unwrap_or_default()
    }

    /// `Peek()`, mutable.
    pub fn peek_mut(&mut self) -> Option<&mut DataValue> {
        self.stack.get_mut(self.sp.max(0) as usize)
    }

    /// `Get(d)`. Out-of-range depths (undefined in C++, reachable when the
    /// stack is smaller than `nHardware::STACK_SIZE` in `PrintStatus`) read
    /// as a cleared value.
    pub fn get(&self, d: i32) -> DataValue {
        let p = d + self.sp;
        let idx = if p >= self.sz() { p - self.sz() } else { p };
        if idx < 0 {
            return DataValue::default();
        }
        self.stack.get(idx as usize).copied().unwrap_or_default()
    }

    /// `Clear(sz)`. QUIRK: the stack pointer is kept.
    pub fn clear(&mut self, sz: i32) {
        self.stack = vec![DataValue::default(); sz.max(0) as usize];
    }

    /// `operator=`: the stack pointer, then a cleared array of the same size
    /// filled through `DataValue::operator=` (so sensor and message flags
    /// are not copied).
    pub fn assign(&mut self, is: &ExpStack) {
        self.sp = is.sp;
        self.clear(is.sz());
        for (d, s) in self.stack.iter_mut().zip(is.stack.iter()) {
            d.assign(s);
        }
    }
}

/// `cHardwareExperimental::cLocalThread`.
#[derive(Debug, Clone, Default)]
pub struct LocalThread {
    pub(crate) id: i32,
    pub(crate) promoter_inst_executed: i32,
    pub(crate) execurate: u32,
    /// `reg`
    pub reg: [DataValue; NUM_REGISTERS],
    /// `heads`
    pub heads: [HeadCpu; NUM_HEADS_EXP],
    /// `stack`
    pub stack: ExpStack,
    /// `cur_stack` (0 local, 1 global).
    pub cur_stack: u8,
    /// `cur_head`
    pub cur_head: u8,
    /// `reading_label:1`
    pub reading_label: bool,
    /// `reading_seq:1`
    pub reading_seq: bool,
    /// `active:1`
    pub active: bool,
    /// `wait_greater:1`
    pub wait_greater: bool,
    /// `wait_equal:1`
    pub wait_equal: bool,
    /// `wait_less:1`
    pub wait_less: bool,
    /// `wait_reg:4`
    pub wait_reg: u32,
    /// `wait_dst:4`
    pub wait_dst: u32,
    /// `wait_value`
    pub wait_value: i32,
    /// `read_label`
    pub read_label: CodeLabel,
    /// `read_seq`
    pub read_seq: CodeLabel,
    /// `next_label`
    pub next_label: CodeLabel,
}

impl LocalThread {
    /// `operator=`: every field; registers and stack entries through
    /// `DataValue::operator=`, heads through `cHeadCPU::operator=`.
    pub fn assign(&mut self, t: &LocalThread) {
        self.id = t.id;
        self.promoter_inst_executed = t.promoter_inst_executed;
        self.execurate = t.execurate;
        for (d, s) in self.reg.iter_mut().zip(t.reg.iter()) {
            d.assign(s);
        }
        for (d, s) in self.heads.iter_mut().zip(t.heads.iter()) {
            d.assign(s);
        }
        self.stack.assign(&t.stack);
        self.cur_stack = t.cur_stack;
        self.cur_head = t.cur_head;
        self.reading_label = t.reading_label;
        self.reading_seq = t.reading_seq;
        self.active = t.active;
        self.wait_greater = t.wait_greater;
        self.wait_equal = t.wait_equal;
        self.wait_less = t.wait_less;
        self.wait_reg = t.wait_reg;
        self.wait_dst = t.wait_dst;
        self.wait_value = t.wait_value;
        self.read_label = t.read_label.clone();
        self.read_seq = t.read_seq.clone();
        self.next_label = t.next_label.clone();
    }

    /// `Reset(hardware, id)`.
    ///
    /// QUIRK: `read_seq` and the wait fields are not reset.
    pub fn reset(&mut self, id: i32, stack_size: i32, mem_size: i32) {
        self.id = id;
        for r in self.reg.iter_mut() {
            r.clear();
        }
        for h in self.heads.iter_mut() {
            h.reset(0, mem_size, 1);
        }
        self.stack.clear(stack_size);
        self.cur_stack = 0;
        self.cur_head = HEAD_IP as u8;
        self.reading_label = false;
        self.reading_seq = false;
        self.active = true;
        self.read_label.clear();
        self.next_label.clear();
        self.execurate = 0;
        self.promoter_inst_executed = 0;
    }

    /// `GetID()`
    pub fn id(&self) -> i32 {
        self.id
    }

    /// `GetExecurate()`
    pub fn execurate(&self) -> u32 {
        self.execurate
    }

    /// `UpdateExecurate(code_len, inst_code)`
    pub fn update_execurate(&mut self, code_len: i32, inst_code: u32) {
        self.execurate = self.execurate.wrapping_shl(code_len as u32);
        if !(0..32).contains(&code_len) {
            // `<<=` by the full width is undefined in C++; x86 masks the
            // count, which `wrapping_shl` reproduces.
        }
        self.execurate |= inst_code;
    }
}

/// `cHardwareExperimental::cPromoter`.
#[derive(Debug, Clone, Copy, Default)]
pub struct Promoter {
    /// `pos`
    pub pos: i32,
    /// `bit_code`
    pub bit_code: i32,
    /// `regulation`
    pub regulation: i32,
}

impl Promoter {
    /// `GetRegulatedBitCode()`
    pub fn regulated_bit_code(&self) -> i32 {
        self.bit_code ^ self.regulation
    }
}

/// `cHardwareExperimental`.
pub struct HardwareExperimental {
    pub(crate) base: HardwareBase,
    pub(crate) functions: &'static [Option<ExpInstFn>],
    pub(crate) memory: CpuMemory,
    pub(crate) global_stack: ExpStack,
    pub(crate) threads: Vec<LocalThread>,
    pub(crate) thread_id_chart: i32,
    pub(crate) cur_thread: usize,
    pub(crate) use_avatar: i32,
    pub(crate) sensor: OrgSensor,
    pub(crate) from_sensor: bool,
    pub(crate) from_message: bool,
    pub(crate) cycle_count: u32,
    pub(crate) last_output: u32,

    pub(crate) mal_active: bool,
    pub(crate) advance_ip: bool,
    pub(crate) executedmatchstrings: bool,
    pub(crate) spec_die: bool,
    pub(crate) thread_slicing_parallel: bool,
    pub(crate) no_cpu_cycle_time: bool,
    pub(crate) promoters_enabled: bool,
    pub(crate) constitutive_regulation: bool,
    pub(crate) no_active_promoter_halt: bool,
    pub(crate) slip_read_head: bool,
    pub(crate) io_expire: bool,
    /// `m_waiting_threads:4`
    pub(crate) waiting_threads: u32,

    pub(crate) promoter_index: i32,
    pub(crate) promoter_offset: i32,
    pub(crate) promoters: Vec<Promoter>,

    /// `m_last_cell_data`
    pub(crate) last_cell_data: (bool, i32),
}

impl std::fmt::Debug for HardwareExperimental {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("HardwareExperimental")
            .field("memory", &self.memory.as_string())
            .field("threads", &self.threads.len())
            .field("cur_thread", &self.cur_thread)
            .finish()
    }
}

impl HardwareExperimental {
    /// `cHardwareExperimental(ctx, world, organism, inst_set)`: memory from
    /// the organism's genome, then `Reset(ctx)` (whose `HardwareReset`
    /// runs without an interface).
    pub fn new(cfg: &Config, org: &mut OrgState, inst_set: Arc<InstSet>) -> Self {
        let memory = CpuMemory::from_sequence(org.initial_genome.representation());
        let promoters_enabled = cfg.promoters_enabled != 0;
        let mut hw = HardwareExperimental {
            base: HardwareBase::new(cfg, inst_set),
            functions: exp_handler_table(),
            memory,
            global_stack: ExpStack::default(),
            threads: Vec::new(),
            thread_id_chart: 0,
            cur_thread: 0,
            use_avatar: cfg.use_avatars,
            sensor: OrgSensor::new(cfg.use_avatars),
            from_sensor: false,
            from_message: false,
            cycle_count: 0,
            last_output: 0,
            mal_active: false,
            advance_ip: false,
            executedmatchstrings: false,
            spec_die: false,
            thread_slicing_parallel: cfg.thread_slicing_method == 1,
            no_cpu_cycle_time: cfg.no_cpu_cycle_time != 0,
            promoters_enabled,
            constitutive_regulation: promoters_enabled && cfg.constitutive_regulation != 0,
            no_active_promoter_halt: promoters_enabled && cfg.no_active_promoter_effect == 2,
            slip_read_head: cfg.slip_copy_mode == 0,
            io_expire: false,
            waiting_threads: 0,
            promoter_index: 0,
            promoter_offset: 0,
            promoters: Vec::new(),
            last_cell_data: (false, 0),
        };
        org.hardware_reset_state(cfg);
        hw.base.reset_costs(cfg);
        hw.internal_reset(cfg);
        hw
    }

    // ------------------------------------------------------------------
    // Small accessors
    // ------------------------------------------------------------------

    /// `m_cur_thread` clamped into the thread array.
    #[inline]
    pub(crate) fn ct(&self) -> usize {
        if self.cur_thread < self.threads.len() {
            self.cur_thread
        } else {
            0
        }
    }

    /// `GetRegister(id)`.
    #[inline]
    pub(crate) fn reg(&self, r: usize) -> i32 {
        self.threads[self.ct()].reg[r % NUM_REGISTERS].value
    }

    /// `m_threads[m_cur_thread].reg[r]` (a copy).
    #[inline]
    pub(crate) fn regv(&self, r: usize) -> DataValue {
        self.threads[self.ct()].reg[r % NUM_REGISTERS]
    }

    /// `m_memory.GetSize()`
    #[inline]
    pub(crate) fn mem_size_i(&self) -> i32 {
        self.memory.size()
    }

    /// `getHead(h)` (a copy).
    #[inline]
    pub(crate) fn get_head(&self, h: usize) -> HeadCpu {
        self.threads[self.ct()].heads[h % NUM_HEADS_EXP]
    }

    /// `getHead(h)`, mutable.
    #[inline]
    pub(crate) fn head_ref(&mut self, h: usize) -> &mut HeadCpu {
        let ct = self.ct();
        &mut self.threads[ct].heads[h % NUM_HEADS_EXP]
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

    /// `getHead(h).Set(pos)`.
    #[inline]
    pub(crate) fn head_set_pos(&mut self, h: usize, pos: i32) {
        let size = self.memory.size();
        self.head_ref(h).set(pos, 0, size, 1);
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

    /// `GetLabel()`
    #[inline]
    pub(crate) fn label(&self) -> &CodeLabel {
        &self.threads[self.ct()].next_label
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

    /// `m_inst_set->IsNop(getIP().GetNextInst())`.
    #[inline]
    pub(crate) fn next_is_nop(&self) -> bool {
        let is = &self.base.inst_set;
        is.is_nop(self.ip_head().get_next_inst(&self.memory, is))
    }

    // ------------------------------------------------------------------
    // Registers
    // ------------------------------------------------------------------

    /// `setInternalValue(reg_num, value, from_env, from_sensor,
    /// from_message)`.
    pub(crate) fn set_iv(
        &mut self,
        reg_num: usize,
        value: i32,
        from_env: bool,
        from_sensor: bool,
        from_message: bool,
    ) {
        let ct = self.ct();
        let cc = self.cycle_count & MASK13;
        let dest = &mut self.threads[ct].reg[reg_num % NUM_REGISTERS];
        dest.value = value;
        dest.from_env = from_env;
        dest.originated = cc;
        dest.oldest_component = cc;
        dest.env_component = from_env;
        dest.from_sensor = from_sensor;
        dest.from_message = from_message;
        dest.sensor_component = from_sensor;
        dest.message_component = from_message;
        if self.waiting_threads != 0 {
            self.check_waiting_threads(ct, reg_num % NUM_REGISTERS);
        }
    }

    /// `setInternalValue(reg_num, value)` (all flags false).
    #[inline]
    pub(crate) fn set_ivp(&mut self, reg_num: usize, value: i32) {
        self.set_iv(reg_num, value, false, false, false);
    }

    /// `setInternalValue(reg_num, value, src)`.
    pub(crate) fn set_iv_src(&mut self, reg_num: usize, value: i32, src: &DataValue) {
        let ct = self.ct();
        let cc = self.cycle_count & MASK13;
        let dest = &mut self.threads[ct].reg[reg_num % NUM_REGISTERS];
        dest.value = value;
        dest.from_env = false;
        dest.from_sensor = false;
        dest.from_message = false;
        dest.originated = cc;
        dest.oldest_component = src.oldest_component;
        dest.env_component = src.env_component;
        dest.sensor_component = src.sensor_component;
        dest.message_component = src.message_component;
        if self.waiting_threads != 0 {
            self.check_waiting_threads(ct, reg_num % NUM_REGISTERS);
        }
    }

    /// `setInternalValue(reg_num, value, op1, op2)`.
    pub(crate) fn set_iv_ops(
        &mut self,
        reg_num: usize,
        value: i32,
        op1: &DataValue,
        op2: &DataValue,
    ) {
        let ct = self.ct();
        let cc = self.cycle_count & MASK13;
        let dest = &mut self.threads[ct].reg[reg_num % NUM_REGISTERS];
        dest.value = value;
        dest.from_env = false;
        dest.from_sensor = false;
        dest.from_message = false;
        dest.originated = cc;
        dest.oldest_component = op1.oldest_component.min(op2.oldest_component);
        dest.env_component = op1.env_component || op2.env_component;
        dest.sensor_component = op1.sensor_component || op2.sensor_component;
        dest.message_component = op1.message_component || op2.message_component;
        if self.waiting_threads != 0 {
            self.check_waiting_threads(ct, reg_num % NUM_REGISTERS);
        }
    }

    /// `checkWaitingThreads(cur_thread, reg_num)`.
    pub(crate) fn check_waiting_threads(&mut self, cur_thread: usize, reg_num: usize) {
        for i in 0..self.threads.len() {
            let t = &self.threads[i];
            if i != cur_thread && !t.active && t.wait_reg as usize == reg_num {
                let wait_value = t.wait_value;
                let src = self.threads[cur_thread].reg[reg_num];
                let check_value = src.value;
                let t = &self.threads[i];
                if (t.wait_greater && check_value > wait_value)
                    || (t.wait_equal && check_value == wait_value)
                    || (t.wait_less && check_value < wait_value)
                {
                    self.threads[i].active = true;
                    self.waiting_threads = self.waiting_threads.wrapping_sub(1) & 0xF;
                    let cc = self.cycle_count & MASK13;
                    let dst = self.threads[i].wait_dst as usize % NUM_REGISTERS;
                    let dest = &mut self.threads[i].reg[dst];
                    dest.value = check_value;
                    dest.from_env = false;
                    dest.originated = cc;
                    dest.oldest_component = src.oldest_component;
                    dest.env_component = src.env_component;
                    if self.waiting_threads != 0 {
                        self.check_waiting_threads(i, dst);
                    }
                }
            }
        }
    }

    // ------------------------------------------------------------------
    // Stacks
    // ------------------------------------------------------------------

    /// `stackPop()`
    pub(crate) fn stack_pop(&mut self) -> DataValue {
        let ct = self.ct();
        if self.threads[ct].cur_stack == 0 {
            self.threads[ct].stack.pop()
        } else {
            self.global_stack.pop()
        }
    }

    /// `getStack(stack_id)`
    pub(crate) fn stack_mut(&mut self, stack_id: i32) -> &mut ExpStack {
        let ct = self.ct();
        if stack_id == 0 {
            &mut self.threads[ct].stack
        } else {
            &mut self.global_stack
        }
    }

    /// `switchStack()`
    pub(crate) fn switch_stack(&mut self) {
        let ct = self.ct();
        let t = &mut self.threads[ct];
        t.cur_stack = t.cur_stack.wrapping_add(1);
        if t.cur_stack > 1 {
            t.cur_stack = 0;
        }
    }

    // ------------------------------------------------------------------
    // Reset
    // ------------------------------------------------------------------

    /// `internalReset()`.
    pub(crate) fn internal_reset(&mut self, cfg: &Config) {
        self.cycle_count = 0;
        self.last_output = 0;
        let stack_size = self.base.inst_set.stack_size();
        self.global_stack.clear(stack_size);

        if self.threads.is_empty() {
            self.threads.push(LocalThread::default());
        } else {
            self.threads.truncate(1);
        }
        let mem_size = self.memory.size();
        self.threads[0].reset(0, stack_size, mem_size);
        self.threads[0].active = true;
        self.thread_id_chart = 1;
        self.cur_thread = 0;
        self.waiting_threads = 0;

        self.mal_active = false;
        self.executedmatchstrings = false;

        if self.promoters_enabled {
            self.promoter_index = -1;
            self.promoter_offset = 0;
            self.promoters.clear();
            for i in 0..self.memory.size() {
                if self.base.inst_set.is_promoter(self.memory[i]) {
                    let code =
                        self.numberate(i - 1, -1, cfg.promoter_code_size, cfg.inst_code_length);
                    self.promoters.push(Promoter {
                        pos: i,
                        bit_code: code,
                        regulation: 0,
                    });
                }
            }
        }

        self.io_expire = cfg.io_expire;
        self.use_avatar = cfg.use_avatars;
        self.sensor.reset(cfg.use_avatars);
    }

    /// `Reset(ctx)`: `cOrganism::HardwareReset`, the cost tables, then
    /// `internalReset()`.
    pub(crate) fn full_reset(&mut self, org: &mut OrgState, world: &mut dyn OrgInterface) {
        crate::organism::hardware_reset(org, self, world);
        let cfg = world.config();
        self.base.reset_costs(cfg);
        self.internal_reset(cfg);
    }

    /// `internalResetOnFailedDivide()` (= `internalReset()`).
    pub(crate) fn internal_reset_on_failed_divide(&mut self, world: &dyn OrgInterface) {
        self.internal_reset(world.config());
    }

    // ------------------------------------------------------------------
    // Labels
    // ------------------------------------------------------------------

    /// `ReadLabel(max_size)`.
    pub(crate) fn read_label(&mut self, max_label_exe_size: i32, max_size: i32) {
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

    /// `ReadLabel()` with the default `cCodeLabel::MAX_LENGTH`.
    #[inline]
    pub(crate) fn rl(&mut self, world: &dyn OrgInterface) {
        let m = world.config().max_label_exe_size;
        self.read_label(m, CodeLabel::MAX_LENGTH as i32);
    }

    /// `ReadInst(in_inst)`: extend or clear the read label and the read
    /// nop sequence.
    pub(crate) fn read_inst(&mut self, in_inst: Instruction) {
        let is = &self.base.inst_set;
        let is_nop = is.is_nop(in_inst);
        let is_label = is.is_label(in_inst);
        let ct = self.ct();
        let t = &mut self.threads[ct];
        if is_label {
            t.read_label.clear();
            t.reading_label = true;
        } else if t.reading_label && is_nop {
            t.read_label.add_nop(in_inst.op());
        } else {
            t.read_label.clear();
            t.reading_label = false;
        }

        if !t.reading_seq && is_nop {
            t.read_seq.add_nop(in_inst.op());
            t.reading_seq = true;
        } else if t.reading_seq && is_nop {
            t.read_seq.add_nop(in_inst.op());
        } else {
            t.read_seq.clear();
            t.reading_seq = false;
        }
    }

    #[inline]
    fn mem_inst(&self, pos: i32) -> Instruction {
        crate::head_cpu::inst_at(&self.memory, pos)
    }

    /// `FindLabelStart(mark_executed)`.
    pub(crate) fn find_label_start(
        &mut self,
        mark_executed: bool,
        max_label_exe_size: i32,
    ) -> HeadCpu {
        let ip = self.ip_head();
        let search_label = self.label().clone();
        let n = search_label.get_size();
        if n == 0 {
            return ip;
        }
        let is = self.base.inst_set.clone();
        let size = self.memory.size();
        let mut pos = 0;
        while pos < size {
            if is.is_label(self.mem_inst(pos)) {
                pos += 1;
                let mut size_matched = 0;
                while size_matched < n && pos < size {
                    let inst = self.mem_inst(pos);
                    if !is.is_nop(inst) || search_label.get(size_matched) != is.nop_mod(inst) {
                        break;
                    }
                    size_matched += 1;
                    pos += 1;
                }
                if size_matched == n {
                    if mark_executed {
                        size_matched += 1;
                        let start = pos - size_matched;
                        let max = max_label_exe_size + 1;
                        let mut i = 0;
                        while i < size_matched && i < max {
                            self.memory.set_flag_executed(start + i);
                            i += 1;
                        }
                    }
                    return HeadCpu::new(pos - 1, ip.mem_space(), size, 1);
                }
                continue;
            }
            pos += 1;
        }
        ip
    }

    /// `FindNopSequenceStart(mark_executed)`.
    pub(crate) fn find_nop_sequence_start(
        &mut self,
        mark_executed: bool,
        max_label_exe_size: i32,
    ) -> HeadCpu {
        let ip = self.ip_head();
        let search_label = self.label().clone();
        let n = search_label.get_size();
        if n == 0 {
            return ip;
        }
        let is = self.base.inst_set.clone();
        let size = self.memory.size();
        let mut pos = 0;
        while pos < size {
            if is.is_nop(self.mem_inst(pos)) {
                let mut size_matched = 0;
                while size_matched < n && pos < size {
                    let inst = self.mem_inst(pos);
                    if !is.is_nop(inst) || search_label.get(size_matched) != is.nop_mod(inst) {
                        break;
                    }
                    size_matched += 1;
                    pos += 1;
                }
                if size_matched == n {
                    if mark_executed {
                        let start = pos - size_matched;
                        let max = max_label_exe_size;
                        let mut i = 0;
                        while i < size_matched && i < max {
                            self.memory.set_flag_executed(start + i);
                            i += 1;
                        }
                    }
                    return HeadCpu::new(pos - 1, ip.mem_space(), size, 1);
                }
            }
            pos += 1;
        }
        ip
    }

    /// `FindLabelForward(mark_executed)`.
    pub(crate) fn find_label_forward(
        &mut self,
        mark_executed: bool,
        max_label_exe_size: i32,
    ) -> HeadCpu {
        let ip = self.ip_head();
        let search_label = self.label().clone();
        let n = search_label.get_size();
        if n == 0 {
            return ip;
        }
        let is = self.base.inst_set.clone();
        let size = self.memory.size();
        let mut pos = ip;
        pos.advance(size, 1);
        while pos.position() != ip.position() {
            if is.is_label(pos.get_inst(&self.memory)) {
                let label_start = pos.position();
                pos.advance(size, 1);
                let mut size_matched = 0;
                while size_matched < n && pos.position() != ip.position() {
                    let inst = pos.get_inst(&self.memory);
                    if !is.is_nop(inst) || search_label.get(size_matched) != is.nop_mod(inst) {
                        break;
                    }
                    size_matched += 1;
                    pos.advance(size, 1);
                }
                if size_matched == n {
                    pos.retreat(size, 1);
                    let found_pos = pos.position();
                    if mark_executed {
                        pos.set(label_start, 0, size, 1);
                        let max = max_label_exe_size + 1;
                        let mut i = 0;
                        while i < size_matched && i < max {
                            pos.set_flag_executed(&mut self.memory);
                            i += 1;
                            pos.advance(size, 1);
                        }
                    }
                    return HeadCpu::new(found_pos, ip.mem_space(), size, 1);
                }
                continue;
            }
            pos.advance(size, 1);
        }
        ip
    }

    /// `FindLabelBackward(mark_executed)`.
    ///
    /// QUIRK: the search loop runs while the scan head differs from the IP,
    /// and the scan head starts at the IP, so the loop never runs and the
    /// IP is returned.
    pub(crate) fn find_label_backward(&mut self) -> HeadCpu {
        self.ip_head()
    }

    /// `FindNopSequenceForward(mark_executed)`.
    pub(crate) fn find_nop_sequence_forward(
        &mut self,
        mark_executed: bool,
        max_label_exe_size: i32,
    ) -> HeadCpu {
        let ip = self.ip_head();
        let search_label = self.label().clone();
        let n = search_label.get_size();
        if n == 0 {
            return ip;
        }
        let is = self.base.inst_set.clone();
        let size = self.memory.size();
        let mut pos = ip;
        pos.advance(size, 1);
        while pos.position() != ip.position() {
            if is.is_nop(pos.get_inst(&self.memory)) {
                let label_start = pos.position();
                let mut size_matched = 0;
                while size_matched < n && pos.position() != ip.position() {
                    let inst = pos.get_inst(&self.memory);
                    if !is.is_nop(inst) || search_label.get(size_matched) != is.nop_mod(inst) {
                        break;
                    }
                    size_matched += 1;
                    pos.advance(size, 1);
                }
                if size_matched == n {
                    pos.retreat(size, 1);
                    let found_pos = pos.position();
                    if mark_executed {
                        pos.set(label_start, 0, size, 1);
                        let max = max_label_exe_size;
                        let mut i = 0;
                        while i < size_matched && i < max {
                            pos.set_flag_executed(&mut self.memory);
                            i += 1;
                            pos.advance(size, 1);
                        }
                    }
                    return HeadCpu::new(found_pos, ip.mem_space(), size, 1);
                }
            }
            if pos.position() == ip.position() {
                break;
            }
            pos.advance(size, 1);
        }
        ip
    }

    /// `FindNopSequenceBackward(mark_executed)`.
    ///
    /// QUIRK: as [`Self::find_label_backward`], the loop never runs.
    pub(crate) fn find_nop_sequence_backward(&mut self) -> HeadCpu {
        self.ip_head()
    }

    /// `AdjustHeads()`
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
    pub(crate) fn find_modified_register(&mut self, default_register: usize) -> usize {
        if self.next_is_nop() {
            self.ip_advance();
            let m = self
                .base
                .inst_set
                .nop_mod(self.ip_head().get_inst(&self.memory));
            self.ip_set_flag_executed();
            return m.max(0) as usize;
        }
        default_register
    }

    /// `FindModifiedNextRegister(default_register)`.
    pub(crate) fn find_modified_next_register(&mut self, default_register: usize) -> usize {
        if self.next_is_nop() {
            self.ip_advance();
            let m = self
                .base
                .inst_set
                .nop_mod(self.ip_head().get_inst(&self.memory));
            self.ip_set_flag_executed();
            return m.max(0) as usize;
        }
        (default_register + 1) % NUM_REGISTERS
    }

    /// `FindModifiedHead(default_head)`.
    pub(crate) fn find_modified_head(&mut self, default_head: usize) -> usize {
        self.find_modified_register(default_head)
    }

    /// `FindNextRegister(base_reg)`.
    #[inline]
    pub(crate) fn find_next_register(base_reg: usize) -> usize {
        (base_reg + 1) % NUM_REGISTERS
    }

    // ------------------------------------------------------------------
    // Threads
    // ------------------------------------------------------------------

    /// `ThreadPrev()`
    pub(crate) fn thread_prev(&mut self) {
        if self.cur_thread == 0 {
            self.cur_thread = self.threads.len().saturating_sub(1);
        } else {
            self.cur_thread -= 1;
        }
    }

    /// `ForkThread()`.
    pub(crate) fn fork_thread(&mut self, max_cpu_threads: i32) -> bool {
        let num_threads = self.threads.len();
        if num_threads as i32 == max_cpu_threads {
            return false;
        }
        self.threads.push(LocalThread::default());
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

    /// `ThreadCreate(start_pos)`.
    pub(crate) fn thread_create(&mut self, start_pos: HeadCpu, max_cpu_threads: i32) -> bool {
        let thread_id = self.threads.len();
        if thread_id as i32 == max_cpu_threads {
            return false;
        }
        self.threads.push(LocalThread::default());
        let mut new_id = 0;
        while new_id < 31 && (self.thread_id_chart >> new_id) & 1 != 0 {
            new_id += 1;
        }
        self.thread_id_chart |= 1i32.wrapping_shl(new_id as u32);
        let stack_size = self.base.inst_set.stack_size();
        let mem_size = self.memory.size();
        self.threads[thread_id].reset(new_id, stack_size, mem_size);
        self.threads[thread_id].heads[HEAD_IP].assign(&start_pos);
        true
    }

    /// `ExitThread()`.
    pub(crate) fn exit_thread(&mut self) -> bool {
        let n = self.threads.len();
        if n == 1 || self.waiting_threads as usize == n - 1 {
            return false;
        }
        let kill_thread = self.cur_thread;
        self.thread_prev();
        let kid = self.threads[kill_thread].id;
        self.thread_id_chart ^= 1i32.wrapping_shl(kid as u32);
        let last_thread = n - 1;
        if last_thread != kill_thread {
            let last = self.threads[last_thread].clone();
            self.threads[kill_thread].assign(&last);
        }
        self.threads.truncate(n - 1);
        if self.cur_thread > kill_thread {
            self.cur_thread -= 1;
        }
        true
    }

    // ------------------------------------------------------------------
    // Allocate and divide
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
                self.internal_reset_on_failed_divide(world);
                false
            }
        }
    }

    /// The offspring genome (`InstructionSequence` plus this set's name).
    pub(crate) fn make_offspring(&self, seq: InstructionSequence) -> Genome {
        Genome::with_inst_set(
            HARDWARE_TYPE_CPU_EXPERIMENTAL,
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

    /// `cOrganism::ActivateDivide(ctx)`: the last output test, then
    /// `m_interface->Divide`. Returns whether the parent is still alive.
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
        let seq = self.memory.crop(div_point, div_point + child_size);
        org.offspring_genome = self.make_offspring(seq);
        self.memory.resize(div_point);
        let is = self.base.inst_set.clone();
        divide_do_mutations(&is, org, world, &mut self.memory, mut_multiplier, i32::MAX)?;
        divide_test_fitness_measures(org, world, false);
        self.base.reset_ft_costs();
        self.mal_active = false;
        let split = world.config().divide_method == DIVIDE_METHOD_SPLIT;
        if split {
            self.advance_ip = false;
        }
        let parent_alive = self.activate_divide(org, world)?;
        if parent_alive {
            if world.config().divide_method == DIVIDE_METHOD_SPLIT {
                self.full_reset(org, world);
            }
            self.memory.clear_flags();
        }
        Ok(true)
    }

    // ------------------------------------------------------------------
    // Promoters
    // ------------------------------------------------------------------

    /// `Numberate(pos, dir, num_bits)`.
    pub(crate) fn numberate(
        &self,
        pos: i32,
        dir: i32,
        num_bits: i32,
        inst_code_length: i32,
    ) -> i32 {
        let size = self.memory.size();
        if size == 0 {
            // Division by zero in C++.
            return 0;
        }
        let mut code_size = 0;
        let mut code: u32 = 0;
        let num_bits = if num_bits == 0 { 32 } else { num_bits };
        let mut j = pos.wrapping_add(size);
        j %= size;
        while code_size < num_bits {
            let mut inst_code = self.base.inst_set.instruction_code(self.mem_inst(j)) as u32;
            let mut code_on = 0;
            while code_size < num_bits && code_on < inst_code_length {
                if dir < 0 {
                    code >>= 1;
                    code = code.wrapping_add(
                        1u32.wrapping_shl((num_bits - 1) as u32)
                            .wrapping_mul(inst_code & 1),
                    );
                    inst_code >>= 1;
                } else {
                    code = code.wrapping_shl(1);
                    code = code
                        .wrapping_add(inst_code.wrapping_shr((inst_code_length - 1) as u32) & 1);
                    inst_code = inst_code.wrapping_shl(1);
                }
                code_size += 1;
                code_on += 1;
            }
            if inst_code_length <= 0 {
                // C++ loops forever; stop instead.
                break;
            }
            j = j.wrapping_add(size + dir);
            j %= size;
        }
        code as i32
    }

    /// `Inst_SenseRegulate` (also the constitutive regulation step).
    pub(crate) fn sense_regulate(&mut self, org: &mut OrgState, world: &mut dyn OrgInterface) {
        let mut bits: u32 = 0;
        let res_count = world.get_resources(org);
        let code_size = world.config().promoter_code_size;
        for i in 0..code_size {
            if res_count.is_empty() {
                break;
            }
            let b = (i as usize) % res_count.len();
            bits = bits.wrapping_shl(1);
            bits = bits.wrapping_add(u32::from(res_count[b] != 0.0));
        }
        for p in self.promoters.iter_mut() {
            p.regulation = bits as i32;
        }
    }

    /// `PromoterTerminate(ctx)`.
    pub(crate) fn promoter_terminate(
        &mut self,
        org: &mut OrgState,
        world: &mut dyn OrgInterface,
    ) -> Result<(), String> {
        let ct = self.ct();
        if world.config().termination_resets != 0 {
            let id = self.threads[ct].id;
            let stack_size = self.base.inst_set.stack_size();
            let mem_size = self.memory.size();
            self.threads[ct].reset(id, stack_size, mem_size);
            world.reset_inputs();
            org.input_buf.clear();
        }
        self.threads[ct].promoter_inst_executed = 0;
        self.advance_ip = false;
        let promoter_reg_used = R_DX;

        let start_offset = self.promoter_offset;
        let mut start_index = self.promoter_index;
        let mut no_promoter_found = true;
        if !self.promoters.is_empty() {
            let cfg = world.config();
            let promoter_exe_length = cfg.promoter_exe_length;
            let promoter_code_size = cfg.promoter_code_size;
            let promoter_exe_threshold = cfg.promoter_exe_threshold;
            loop {
                self.promoter_index += 1;
                if self.promoter_index == self.promoters.len() as i32 {
                    self.promoter_index = 0;
                    self.promoter_offset += promoter_exe_length;
                    if self.promoter_offset + promoter_exe_length > promoter_code_size {
                        self.promoter_offset = 0;
                    }
                }
                let mut count = 0;
                let code = self.promoters[self.promoter_index as usize].regulated_bit_code() as u32;
                for i in 0..promoter_exe_length {
                    let mut offset = self.promoter_offset + i;
                    if offset >= promoter_code_size {
                        offset -= promoter_code_size;
                    }
                    let state = code.wrapping_shr(offset as u32) as i32;
                    count += state & 1;
                }
                if count >= promoter_exe_threshold {
                    no_promoter_found = false;
                    break;
                }
                if start_offset == self.promoter_offset && start_index == self.promoter_index {
                    break;
                }
                if start_index == -1 {
                    start_index = 0;
                }
            }
        }

        if no_promoter_found {
            match world.config().no_active_promoter_effect {
                0 | 2 => {
                    self.promoter_index = -1;
                    self.head_set_pos(HEAD_IP, 0);
                    self.set_ivp(promoter_reg_used, 0);
                }
                1 => {
                    org.die(world);
                }
                v => {
                    println!("Unrecognized NO_ACTIVE_PROMOTER_EFFECT setting: {}", v);
                    return Err(
                        "Unrecognized NO_ACTIVE_PROMOTER_EFFECT setting (C++ exits with status 1)"
                            .to_string(),
                    );
                }
            }
        } else {
            let p = self.promoters[self.promoter_index as usize];
            self.head_set_pos(HEAD_IP, p.pos + 1);
            if world.config().promoter_to_register != 0 {
                self.set_ivp(promoter_reg_used, p.bit_code);
            }
        }
        Ok(())
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
            // UB substitute: an opcode outside the set executes as a failed
            // no-op.
            return Ok(false);
        }
        let inst_idx = self.base.inst_set.lib_function_index(cur_inst);
        self.ip_set_flag_executed();
        if let Some(c) = org.phenotype.cur_inst_count.get_mut(op as usize) {
            *c = c.wrapping_add(1);
        }
        self.from_sensor = false;
        self.from_message = false;
        let f = self
            .functions
            .get(inst_idx.max(0) as usize)
            .copied()
            .flatten()
            .ok_or_else(|| {
                format!(
                    "unsupported: instruction '{}' (C++: cHardwareExperimental.cc)",
                    self.base.inst_set.name_of(cur_inst)
                )
            })?;
        let exec_success = f(self, org, world)?;

        if exec_success {
            let code_len = world.config().inst_code_length;
            let code = self.base.inst_set.instruction_code(cur_inst) as u32;
            let ct = self.ct();
            self.threads[ct].update_execurate(code_len, code);
            if self.from_sensor {
                if let Some(c) = org.phenotype.cur_from_sensor_count.get_mut(op as usize) {
                    *c = c.wrapping_add(1);
                }
            }
            if self.from_message {
                if let Some(c) = org.phenotype.cur_from_message_count.get_mut(op as usize) {
                    *c = c.wrapping_add(1);
                }
            }
        } else if let Some(c) = org.phenotype.cur_inst_count.get_mut(op as usize) {
            *c = c.wrapping_sub(1);
        }
        Ok(exec_success)
    }

    /// `m_tracer->TraceHardware(ctx, *this, bonus, mini, exec_success)`.
    fn trace(
        &self,
        org: &mut OrgState,
        world: &mut dyn OrgInterface,
        bonus: bool,
        mini: bool,
        exec_success: i32,
    ) {
        let Some(tracer) = self.base.tracer.clone() else {
            return;
        };
        let mut guard = match tracer.lock() {
            Ok(g) => g,
            Err(p) => p.into_inner(),
        };
        if mini {
            // `TraceHardware` opens a new mini trace file with
            // `SetupMiniTraceFileHeader(*m_file, genotype id, genotype)`.
            if let Some((gen_id, genotype)) = guard.mini_header_due() {
                let header = crate::mini_trace::experimental_header(
                    world.update(),
                    org.id,
                    gen_id,
                    &genotype,
                    org.genome().representation().size(),
                );
                guard.write_mini_header(&header);
            }
        }
        let me: &HardwareExperimental = self;
        let world_cell = std::cell::RefCell::new(world);
        // The mini-trace status reads the faced cell's resources, which may
        // injure the organism (crate::org_interface), so it gets it mutably.
        let org_cell = std::cell::RefCell::new(org);
        guard.trace_hardware(
            &mut || print_status(&org_cell.borrow(), me, &mut **world_cell.borrow_mut()),
            &mut || {
                me.mini_trace_status(&mut org_cell.borrow_mut(), &mut **world_cell.borrow_mut())
            },
            &mut |e| me.print_mini_trace_success(e),
            bonus,
            mini,
            exec_success,
        );
    }

    /// `PrintMiniTraceStatus(ctx, fp)`.
    fn mini_trace_status(&self, org: &mut OrgState, world: &mut dyn OrgInterface) -> String {
        use std::fmt::Write;
        let mut s = String::new();
        let t = &self.threads[self.ct()];
        let _ = write!(s, "{} {} ", self.cycle_count, world.update());
        for r in t.reg.iter() {
            let _ = write!(s, "{} ({}) ", r.value, r.originated);
        }
        let _ = write!(
            s,
            "{} {} {} {} {} {} ",
            self.cur_thread,
            self.ip_head().position(),
            self.get_head(HEAD_READ).position(),
            self.get_head(HEAD_WRITE).position(),
            self.get_head(HEAD_FLOW).position(),
            self.last_output
        );
        let _ = write!(
            s,
            "{} {} {} ",
            ostream_double(org.phenotype.merit.get_double()),
            ostream_double(org.phenotype.cur_bonus),
            org.forage_target
        );
        let opinion = match &org.opinions {
            Some(l) if !l.is_empty() => l[l.len() - 1].0,
            _ => -99,
        };
        let _ = write!(s, "{} ", opinion);
        let _ = write!(s, "{} ", world.get_cell_id());
        if self.use_avatar != 0 {
            let _ = write!(s, "{} ", world.get_av_cell_id(0));
        }
        if self.use_avatar == 0 {
            let _ = write!(s, "{} ", world.get_faced_dir());
        } else {
            let _ = write!(s, "{} ", world.get_av_facing(0));
        }
        if self.use_avatar == 0 {
            let _ = write!(s, "{} ", i32::from(world.is_neighbor_cell_occupied()));
        } else {
            let _ = write!(s, "{} ", i32::from(world.faced_has_av(0)));
        }
        let habitats: Vec<i32> = {
            let lib = world.environment().resource_lib();
            lib.resources().iter().map(|r| r.habitat).collect()
        };
        if self.use_avatar == 0 {
            let _ = world.get_faced_cell_resources(org);
        } else {
            let _ = world.get_av_faced_resources(org, 0);
        }
        let mut wall = 0;
        let mut hill = 0;
        for (i, h) in habitats.iter().enumerate() {
            if *h == 2 {
                let level = if self.use_avatar == 0 {
                    world.get_faced_resource_val(org, i as i32)
                } else {
                    world.get_av_faced_resource_val(org, i as i32, 0)
                };
                if level > 0.0 {
                    wall = 1;
                }
            } else if *h == 1 {
                let level = if self.use_avatar == 0 {
                    world.get_faced_resource_val(org, i as i32)
                } else {
                    world.get_av_faced_resource_val(org, i as i32, 0)
                };
                if level > 0.0 {
                    hill = 1;
                }
            }
            if hill == 1 && wall == 1 {
                break;
            }
        }
        let _ = write!(s, "{} {} ", hill, wall);
        let ip_inst = self.ip_head().get_inst(&self.memory);
        let is = &self.base.inst_set;
        let name = if ip_inst.op() < is.size() {
            is.name_of(ip_inst)
        } else {
            ""
        };
        let _ = write!(s, "{} ", name);
        let mut pos = self.ip_head().position();
        let mut mods = String::new();
        for _ in 0..NUM_REGISTERS {
            pos += 1;
            if pos >= self.memory.size() {
                pos = 0;
            }
            let inst = self.mem_inst(pos);
            if is.is_nop(inst) {
                mods.push((is.nop_mod(inst) as u8).wrapping_add(b'A') as char);
            } else {
                break;
            }
        }
        if mods.is_empty() {
            s.push_str("NoMods ");
        } else {
            let _ = write!(s, "{} ", mods);
        }
        s
    }

    /// `cHardwareBase::RecordNavTrace(use_avatar)`.
    fn record_nav_trace(&mut self, world: &mut dyn OrgInterface) {
        let use_avatar = self.use_avatar != 0;
        let loc = if use_avatar {
            world.get_av_cell_id(0)
        } else {
            world.get_cell_id()
        };
        let facing = if use_avatar {
            world.get_av_facing(0)
        } else {
            world.get_faced_dir()
        };
        self.base.navtraceloc.push(loc);
        self.base.navtracefacing.push(facing);
        self.base.navtraceupdate.push(world.update());
    }

    /// `CheckImplicitRepro(ctx)`.
    fn check_implicit_repro(
        &mut self,
        org: &mut OrgState,
        world: &mut dyn OrgInterface,
    ) -> Result<(), String> {
        if self.base.implicit_repro_active && implicit_repro_due(org, world.config(), false) {
            crate::hardware_experimental_insts::inst_repro(self, org, world)?;
        }
        Ok(())
    }
}

impl Hardware for HardwareExperimental {
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
        HARDWARE_TYPE_CPU_EXPERIMENTAL
    }
    fn supports_speculative(&self) -> bool {
        true
    }

    fn reset(&mut self, org: &mut OrgState, world: &mut dyn OrgInterface) {
        self.full_reset(org, world);
    }

    /// `SingleProcess(ctx, speculative)`.
    ///
    /// The C++ holds a reference to the IP of the thread that was current
    /// when the instruction started. Threads are separate allocations, so
    /// the reference survives forks and the copying of `ExitThread`; it
    /// only dangles when that thread's slot was removed (a reset to one
    /// thread from another thread), where the port skips the advance.
    fn single_process(
        &mut self,
        org: &mut OrgState,
        world: &mut dyn OrgInterface,
        speculative: bool,
    ) -> Result<bool, String> {
        debug_assert!(!speculative || !self.thread_slicing_parallel);
        org.set_running(true);

        if !speculative && self.spec_die {
            org.die(world);
            org.set_running(false);
            return Ok(false);
        }

        if org.phenotype.cpu_cycles_used == 0 && self.promoters_enabled {
            self.promoter_terminate(org, world)?;
        }

        self.cycle_count = self.cycle_count.wrapping_add(1);
        org.phenotype.inc_cpu_cycles_used();
        if !self.no_cpu_cycle_time {
            org.phenotype.inc_time_used(1);
        }

        let num_inst_exec = if world.config().thread_slicing_method == 1 {
            self.threads.len()
        } else {
            1
        };

        let mut i = 0;
        while i < num_inst_exec {
            let last_thread = self.cur_thread;
            self.cur_thread += 1;
            if self.cur_thread >= self.threads.len() {
                self.cur_thread = 0;
            }
            if !self.threads[self.cur_thread].active {
                if num_inst_exec != 1 {
                    i += 1;
                }
                continue;
            }

            self.advance_ip = true;
            let ip_thread = self.cur_thread;
            let size = self.memory.size();
            self.threads[ip_thread].heads[HEAD_IP].adjust(size, 1);

            if self.base.tracer.is_some() {
                self.trace(org, world, false, false, -2);
            }

            let cur_inst = self.threads[ip_thread].heads[HEAD_IP].get_inst(&self.memory);
            let valid = cur_inst.op() < self.base.inst_set.size();

            if speculative
                && (self.spec_die || (valid && self.base.inst_set.should_stall(cur_inst)))
            {
                self.cur_thread = last_thread;
                org.phenotype.dec_cpu_cycles_used();
                if !self.no_cpu_cycle_time {
                    org.phenotype.inc_time_used(-1);
                }
                org.set_running(false);
                return Ok(false);
            }

            if self.base.tracer.is_some() {
                self.trace(org, world, false, true, -2);
            }

            let mut exec = true;
            let mut exec_success = 0;
            let on_pause = self.base.is_paying_active_cost(self.cur_thread);
            if self.base.has_any_costs && valid {
                exec = self
                    .base
                    .pay_pre_costs(org, world, cur_inst, self.cur_thread);
            }
            if !exec {
                exec_success = -1;
            }

            if self.promoters_enabled {
                if self.constitutive_regulation {
                    self.sense_regulate(org, world);
                }
                if self.no_active_promoter_halt && self.promoter_index == -1 {
                    exec = false;
                }
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
                if self.promoters_enabled {
                    let ct = self.ct();
                    self.threads[ct].promoter_inst_executed += 1;
                }
                if exec && self.execute_inst(org, world, cur_inst)? {
                    self.base.pay_post_res_costs(org, world, cur_inst);
                    let ct = self.cur_thread;
                    self.base.set_post_cpu_costs(cur_inst, ct);
                    exec_success = 1;
                }
                if org.phenotype.to_delete {
                    // The C++ traces here (`TraceHardware(.., true,
                    // exec_success)`), but the call never writes anything:
                    // `KillOrganism` ends a mini trace (`UpdateQs` ->
                    // `DeleteMiniTrace`) before it sets `ToDelete`, and other
                    // tracers ignore mini calls. Here the population cannot
                    // reach the running hardware, so a killed organism keeps
                    // its mini tracer until the step ends; skipping the call
                    // keeps it from writing.
                    break;
                }
                if self.advance_ip && ip_thread < self.threads.len() {
                    let size = self.memory.size();
                    self.threads[ip_thread].heads[HEAD_IP].advance(size, 1);
                }
                org.phenotype.inc_time_used(addl_time_cost);

                if self.promoters_enabled {
                    let processivity = world.config().promoter_processivity;
                    if world.rng().p(1.0 - processivity) {
                        self.promoter_terminate(org, world)?;
                    }
                    let max = world.config().promoter_inst_max;
                    if max != 0 && self.threads[self.ct()].promoter_inst_executed >= max {
                        self.promoter_terminate(org, world)?;
                    }
                }
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
                        self.record_nav_trace(world);
                    }
                }
            }
            i += 1;
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

        org.set_running(false);
        self.check_implicit_repro(org, world)?;
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
            self.trace(org, world, true, false, -2);
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
        let is = &self.base.inst_set;
        let name = if ip_inst.op() < is.size() {
            is.name_of(ip_inst)
        } else {
            ""
        };
        let _ = writeln!(
            s,
            "CPU CYCLE:{} THREAD:{}  IP:{} ({})",
            org.phenotype.cpu_cycles_used,
            self.cur_thread,
            ip.position(),
            name
        );
        let t = &self.threads[self.ct()];
        for (i, r) in t.reg.iter().enumerate() {
            let _ = write!(
                s,
                "{}X:{} [0x{:x}] ({} {} {} {})  ",
                (b'A' + i as u8) as char,
                r.value,
                r.value as u32,
                i32::from(r.from_env),
                i32::from(r.env_component),
                r.originated,
                r.oldest_component
            );
        }
        if org.phenotype.time_used != org.phenotype.cpu_cycles_used {
            let _ = write!(s, "  EnergyUsed:{}", org.phenotype.time_used);
        }
        s.push('\n');
        let _ = writeln!(
            s,
            "  R-Head:{} W-Head:{} F-Head:{}  RL:{}   Ex:{}",
            self.get_head(HEAD_READ).position(),
            self.get_head(HEAD_WRITE).position(),
            self.get_head(HEAD_FLOW).position(),
            self.read_label_ref().as_string(),
            self.last_output
        );
        for stack_id in 0..2 {
            let mark = if i32::from(t.cur_stack) == stack_id {
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
        if world.config().promoters_enabled != 0 {
            let _ = write!(
                s,
                "Promoters: index={} offset={} exe_inst={}",
                self.promoter_index, self.promoter_offset, t.promoter_inst_executed
            );
            for p in &self.promoters {
                let _ = write!(s, "{}:Ox{:08x} ", p.pos, p.regulated_bit_code() as u32);
            }
            s.push('\n');
            s.push('\n');
        }
        s
    }

    fn print_mini_trace_status(&self, org: &mut OrgState, world: &mut dyn OrgInterface) -> String {
        self.mini_trace_status(org, world)
    }

    /// `PrintMiniTraceSuccess(fp, exec_success)`.
    fn print_mini_trace_success(&self, exec_success: i32) -> String {
        format!("{}\n", exec_success)
    }

    /// `GetStack(depth, stack_id, in_thread)`.
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
        if stack_id == 0 {
            self.threads[in_thread].stack.get(depth).value
        } else if stack_id == 1 {
            self.global_stack.get(depth).value
        } else {
            0
        }
    }

    fn get_cur_stack(&self, in_thread: i32) -> i32 {
        let t = if in_thread >= self.threads.len() as i32 || in_thread < 0 {
            self.ct()
        } else {
            in_thread as usize
        };
        i32::from(self.threads[t].cur_stack)
    }

    fn num_stacks(&self) -> i32 {
        2
    }

    fn head(&self, head_id: usize) -> &HeadCpu {
        &self.threads[self.ct()].heads[head_id]
    }

    fn head_mut(&mut self, head_id: usize) -> &mut HeadCpu {
        let ct = self.ct();
        &mut self.threads[ct].heads[head_id]
    }

    fn head_in_thread(&self, head_id: usize, thread: usize) -> &HeadCpu {
        &self.threads[thread].heads[head_id]
    }

    fn num_heads(&self) -> i32 {
        NUM_HEADS_EXP as i32
    }

    fn memory(&self) -> &CpuMemory {
        &self.memory
    }

    fn memory_mut(&mut self) -> &mut CpuMemory {
        &mut self.memory
    }

    fn register(&self, reg_id: usize) -> i32 {
        self.reg(reg_id)
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
}
