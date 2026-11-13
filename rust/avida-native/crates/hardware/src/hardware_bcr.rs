//! Port of `cHardwareBCR` (avida-core/source/cpu/cHardwareBCR.{h,cc} at
//! 47f13dadb), hardware type 5, and of the machinery it shares with
//! `cHardwareGP8` (type 4, [`crate::hardware_gp8`]): genomes whose
//! promoter/terminator-delimited genes are copied into separate memories,
//! one thread per gene, extra memory spaces for offspring, twelve (GP8:
//! eight) registers whose values carry a 15-bit provenance stamp, threads
//! that wait on register conditions, and micro-op scheduling.
//!
//! The C++ of the two classes is one design written twice: the gene set-up,
//! heads, label and nop-sequence searches, thread creation, register
//! writes, waiting threads and `Divide_Main` are textually identical. Here
//! they live once, in [`GeneCore`] (the register count is a field), and the
//! instruction handlers that are identical in both are generic over
//! [`GeneHw`].
//!
//! The thread and memory arrays are `Apto::Array<_, ManagedPointer>` in
//! C++ (separately allocated elements), so a thread keeps its identity
//! while threads are added; `Vec` indices give the same semantics. Every
//! `internalReset` frees all threads, which leaves the reference
//! `SingleProcess` holds to the running thread's IP dangling; see
//! [`HardwareBcr`]'s `single_process`.

use crate::code_label::CodeLabel;
use crate::hardware::{
    divide_check_viable, divide_do_mutations, divide_test_fitness_measures, implicit_repro_due,
    Hardware, HardwareBase, HeadSite, Viability,
};
use crate::hardware_bcr_insts::{bcr_handler_table, BcrInstFn};
use crate::head_cpu::{inst_at, HeadCpu};
use crate::nhardware::*;
use crate::org_interface::OrgInterface;
use crate::org_sensor::{LookInit, OrgSensor};
use crate::organism::{do_output, ostream_double, print_status, OrgState};
use avida_config::config::Config;
use avida_genome::{CpuMemory, Genome, InstSet, Instruction, InstructionSequence};
use std::any::Any;
use std::sync::Arc;

/// `cHardwareBCR::NUM_REGISTERS` (= `NUM_HEADS` = `NUM_NOPS` =
/// `MAX_MEM_SPACES`).
pub const BCR_NUM_REGISTERS: usize = 12;
/// The largest register count of the gene-expression hardware; register
/// and head arrays are this long, of which the first `n` are used.
pub const MAX_REGS: usize = 12;

/// `rAX`
pub const R_AX: usize = 0;
/// `rBX`
pub const R_BX: usize = 1;
/// `rCX`
pub const R_CX: usize = 2;
/// `rDX`
pub const R_DX: usize = 3;
/// `rLX` (BCR)
pub const R_LX: usize = 11;

/// `hIP`
pub const H_IP: usize = 0;
/// `hREAD`
pub const H_READ: usize = 1;
/// `hWRITE`
pub const H_WRITE: usize = 2;
/// `hFLOW`
pub const H_FLOW: usize = 3;

/// The inspection id of gene `g`'s memory (`GeneHead::is_gene`): gene
/// memories are numbered from here so they do not clash with the ids of
/// `m_mem_array` (at most [`MAX_REGS`] spaces).
pub const GENE_SPACE_BASE: i32 = 16;

/// Mask of the 15-bit `originated` / `oldest_component` bit-fields.
pub const MASK15: u32 = 0x7FFF;

/// `DataValue`: a register or stack value and where it came from. Both
/// stamps are 15-bit fields (the cycle count modulo 32768). Assignment
/// copies every field, so this is plain `Copy`.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct DataValue {
    /// `value`
    pub value: i32,
    /// `originated:15`
    pub originated: u32,
    /// `from_env:1`
    pub from_env: bool,
    /// `oldest_component:15`
    pub oldest_component: u32,
    /// `env_component:1`
    pub env_component: bool,
}

impl DataValue {
    /// `Clear()`
    #[inline]
    pub fn clear(&mut self) {
        *self = DataValue::default();
    }
}

/// `Stack`: a ring of `DataValue`s whose size is the instruction set's
/// stack size.
#[derive(Debug, Clone, Default)]
pub struct DvStack {
    sp: i32,
    stack: Vec<DataValue>,
}

impl DvStack {
    #[inline]
    fn sz(&self) -> i32 {
        self.stack.len() as i32
    }

    /// `Push(value)`. A zero-size stack (an out-of-range write in C++)
    /// drops the value.
    pub fn push(&mut self, value: DataValue) {
        self.sp -= 1;
        if self.sp < 0 {
            self.sp = self.sz() - 1;
        }
        if let Some(slot) = self.stack.get_mut(self.sp.max(0) as usize) {
            *slot = value;
        }
    }

    /// `Pop()`: the top, which is then cleared.
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

    /// `Get(d)`; out-of-range depths read as a cleared value.
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
}

/// `Head`: a position in one of the memories (a gene's or a memory
/// space). Operations that need the memory are [`GeneCore`] methods.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct GeneHead {
    /// `m_pos`
    pub pos: i32,
    /// `m_ms:31`
    pub ms: u32,
    /// `m_is_gene:1`
    pub is_gene: bool,
}

impl GeneHead {
    /// `Head(hw, pos, ms, is_gene)` (no adjustment).
    pub const fn new(pos: i32, ms: u32, is_gene: bool) -> Self {
        GeneHead { pos, ms, is_gene }
    }
}

/// `Gene`: a gene's memory and label.
#[derive(Debug, Clone)]
pub struct Gene {
    /// `memory`
    pub memory: CpuMemory,
    /// `label`
    pub label: CodeLabel,
}

impl Default for Gene {
    /// A default-constructed gene: `cCPUMemory()` holds one instruction
    /// (opcode 0) with clear flags.
    fn default() -> Self {
        Gene {
            memory: CpuMemory::new(1),
            label: CodeLabel::new(),
        }
    }
}

/// `Thread`.
#[derive(Debug, Clone, Default)]
pub struct GeneThread {
    /// `thread_label`
    pub thread_label: CodeLabel,
    /// `reg` (the first `n` are used)
    pub reg: [DataValue; MAX_REGS],
    /// `heads` (the first `n` are used)
    pub heads: [GeneHead; MAX_REGS],
    /// `stack`
    pub stack: DvStack,
    /// `cur_stack:1`
    pub cur_stack: u8,
    /// `reading_label:1`
    pub reading_label: bool,
    /// `reading_seq:1`
    pub reading_seq: bool,
    /// `running:1`
    pub running: bool,
    /// `active:1`
    pub active: bool,
    /// `wait_greater:1`
    pub wait_greater: bool,
    /// `wait_equal:1`
    pub wait_equal: bool,
    /// `wait_less:1`
    pub wait_less: bool,
    /// `wait_reg` (signed bit-field; -1 = woken next cycle)
    pub wait_reg: i32,
    /// `wait_dst` (unsigned bit-field)
    pub wait_dst: u32,
    /// `wait_value`
    pub wait_value: i32,
    /// `read_label`
    pub read_label: CodeLabel,
    /// `read_seq`
    pub read_seq: CodeLabel,
    /// `next_label`
    pub next_label: CodeLabel,
    /// `sensor_session`
    pub sensor_session: LookInit,
}

/// The state `cHardwareBCR` and `cHardwareGP8` share.
#[derive(Debug, Clone)]
pub struct GeneCore {
    /// `NUM_REGISTERS` (also heads, nops and memory spaces).
    pub n: usize,
    /// `m_inst_set`
    pub is: Arc<InstSet>,
    /// `m_genes`
    pub genes: Vec<Gene>,
    /// `m_mem_array`
    pub mem_array: Vec<CpuMemory>,
    /// `m_mem_ids` (`char`)
    pub mem_ids: [i8; MAX_REGS],
    /// `m_global_stack`
    pub global_stack: DvStack,
    /// `m_threads`
    pub threads: Vec<GeneThread>,
    /// `m_cur_thread`
    pub cur_thread: usize,
    /// `m_sensor`
    pub sensor: OrgSensor,
    /// `m_sensor_sessions`
    pub sensor_sessions: Vec<LookInit>,
    /// `m_cycle_count`
    pub cycle_count: u32,
    /// `m_last_output`
    pub last_output: u32,
    /// `m_cur_uop:8`
    pub cur_uop: u32,
    /// `m_cur_offspring:5`
    pub cur_offspring: i32,
    /// `m_use_avatar:3`
    pub use_avatar: i32,
    /// `m_advance_ip`
    pub advance_ip: bool,
    /// `m_spec_stall`
    pub spec_stall: bool,
    /// `m_spec_die`
    pub spec_die: bool,
    /// `m_no_cpu_cycle_time`
    pub no_cpu_cycle_time: bool,
    /// `m_slip_read_head`
    pub slip_read_head: bool,
    /// `m_waiting_threads`
    pub waiting_threads: u32,
    /// `m_running_threads`
    pub running_threads: u32,
    /// The width of the two thread counters: `0xF` for the BCR's 4-bit
    /// fields, `u32::MAX` for the GP8's plain `unsigned int`s.
    pub counter_mask: u32,
    /// `m_placeholder_head` (what `GetHead` / `IP` return).
    pub placeholder_head: HeadCpu,
    /// `m_imm_methods` (GP8): the immediate value of each instruction-library
    /// entry, by library index; empty for the BCR.
    pub imm: &'static [Option<Imm>],
    /// A zero-length memory read where a head names a memory that does not
    /// exist (never reached by the C++, which would read freed memory).
    empty: CpuMemory,
}

impl GeneCore {
    /// The constructor-time state: memory space 0 is the genome.
    pub fn new(
        n: usize,
        cfg: &Config,
        genome: &InstructionSequence,
        is: Arc<InstSet>,
        counter_mask: u32,
        imm: &'static [Option<Imm>],
    ) -> Self {
        GeneCore {
            n,
            is,
            genes: Vec::new(),
            mem_array: vec![CpuMemory::from_sequence(genome)],
            mem_ids: [-1; MAX_REGS],
            global_stack: DvStack::default(),
            threads: Vec::new(),
            cur_thread: 0,
            sensor: OrgSensor::new(cfg.use_avatars),
            sensor_sessions: vec![LookInit::default(); n],
            cycle_count: 0,
            last_output: 0,
            cur_uop: 0,
            cur_offspring: 0,
            use_avatar: cfg.use_avatars,
            advance_ip: false,
            spec_stall: false,
            spec_die: false,
            no_cpu_cycle_time: cfg.no_cpu_cycle_time != 0,
            slip_read_head: cfg.slip_copy_mode == 0,
            waiting_threads: 0,
            running_threads: 0,
            counter_mask,
            placeholder_head: HeadCpu::default(),
            imm,
            empty: CpuMemory::new(0),
        }
    }

    // ------------------------------------------------------------------
    // Instruction-set predicates (guarded: out-of-range reads give the
    // error instruction, which is no nop, label, promoter, ...)
    // ------------------------------------------------------------------

    #[inline]
    fn valid(&self, inst: Instruction) -> bool {
        inst.op() >= 0 && inst.op() < self.is.size()
    }

    /// `IsNop(inst)`
    #[inline]
    pub fn is_nop(&self, inst: Instruction) -> bool {
        self.is.is_nop(inst)
    }

    /// `IsLabel(inst)`
    #[inline]
    pub fn is_label(&self, inst: Instruction) -> bool {
        self.valid(inst) && self.is.is_label(inst)
    }

    /// `IsPromoter(inst)`
    #[inline]
    pub fn is_promoter(&self, inst: Instruction) -> bool {
        self.valid(inst) && self.is.is_promoter(inst)
    }

    /// `IsTerminator(inst)`
    #[inline]
    pub fn is_terminator(&self, inst: Instruction) -> bool {
        self.valid(inst) && self.is.is_terminator(inst)
    }

    /// `IsImmediateValue(inst)`
    #[inline]
    pub fn is_immediate_value(&self, inst: Instruction) -> bool {
        self.valid(inst) && self.is.is_immediate_value(inst)
    }

    /// `GetNopMod(inst)` (an instruction known to be a nop).
    #[inline]
    pub fn nop_mod(&self, inst: Instruction) -> i32 {
        self.is.nop_mod(inst)
    }

    /// `GetName(inst)`; empty for an instruction outside the set.
    pub fn inst_name(&self, inst: Instruction) -> &str {
        if self.valid(inst) {
            self.is.name_of(inst)
        } else {
            ""
        }
    }

    // ------------------------------------------------------------------
    // Heads
    // ------------------------------------------------------------------

    /// `Head::GetMemory()`
    pub fn mem(&self, h: &GeneHead) -> &CpuMemory {
        let m = if h.is_gene {
            self.genes.get(h.ms as usize).map(|g| &g.memory)
        } else {
            self.mem_array.get(h.ms as usize)
        };
        m.unwrap_or(&self.empty)
    }

    /// `Head::GetMemory()`, mutable.
    pub fn mem_mut(&mut self, h: &GeneHead) -> &mut CpuMemory {
        let m = if h.is_gene {
            self.genes.get_mut(h.ms as usize).map(|g| &mut g.memory)
        } else {
            self.mem_array.get_mut(h.ms as usize)
        };
        m.unwrap_or(&mut self.empty)
    }

    /// `Head::Adjust()`
    pub fn h_adjust(&self, h: &mut GeneHead) {
        let mem_size = self.mem(h).size();
        if h.pos >= 0 && h.pos < mem_size {
            return;
        }
        if mem_size == 0 || h.pos < 0 {
            h.pos = 0;
            return;
        }
        if h.pos < mem_size.wrapping_mul(2) {
            h.pos -= mem_size;
        } else {
            h.pos %= mem_size;
        }
    }

    /// `Head::Advance()` (also `operator++`).
    #[inline]
    pub fn h_advance(&self, h: &mut GeneHead) {
        h.pos = h.pos.wrapping_add(1);
        self.h_adjust(h);
    }

    /// `operator--`
    #[inline]
    pub fn h_retreat(&self, h: &mut GeneHead) {
        h.pos = h.pos.wrapping_sub(1);
        self.h_adjust(h);
    }

    /// `Head::Jump(jump)`
    #[inline]
    pub fn h_jump(&self, h: &mut GeneHead, jump: i32) {
        h.pos = h.pos.wrapping_add(jump);
        self.h_adjust(h);
    }

    /// `Head::SetPosition(pos)`
    #[inline]
    pub fn h_set_position(&self, h: &mut GeneHead, pos: i32) {
        h.pos = pos;
        self.h_adjust(h);
    }

    /// `Head::GetInst()`
    #[inline]
    pub fn h_inst(&self, h: &GeneHead) -> Instruction {
        inst_at(self.mem(h).as_sequence(), h.pos)
    }

    /// `Head::AtEnd()`
    #[inline]
    pub fn h_at_end(&self, h: &GeneHead) -> bool {
        h.pos.wrapping_add(1) == self.mem(h).size()
    }

    /// `Head::NextInst()`
    #[inline]
    pub fn h_next_inst(&self, h: &GeneHead) -> Instruction {
        if self.h_at_end(h) {
            self.is.inst_error()
        } else {
            inst_at(self.mem(h).as_sequence(), h.pos.wrapping_add(1))
        }
    }

    /// `Head::SetInst(value)`
    pub fn h_set_inst(&mut self, h: &GeneHead, inst: Instruction) {
        let pos = h.pos;
        let m = self.mem_mut(h);
        if pos >= 0 && pos < m.size() {
            m[pos] = inst;
        }
    }

    /// `Head::InsertInst(inst)`
    pub fn h_insert_inst(&mut self, h: &GeneHead, inst: Instruction) {
        let pos = h.pos;
        let m = self.mem_mut(h);
        if pos >= 0 && pos <= m.size() {
            m.insert(pos, inst);
        }
    }

    /// `Head::RemoveInst()`
    pub fn h_remove_inst(&mut self, h: &GeneHead) {
        let pos = h.pos;
        let m = self.mem_mut(h);
        if pos >= 0 && pos < m.size() {
            m.remove(pos, 1);
        }
    }

    /// `Head::SetFlagExecuted()`
    pub fn h_set_flag_executed(&mut self, h: &GeneHead) {
        let pos = h.pos;
        let m = self.mem_mut(h);
        if pos >= 0 && pos < m.size() {
            m.set_flag_executed(pos);
        }
    }

    /// `Head::SetFlagCopied()`
    pub fn h_set_flag_copied(&mut self, h: &GeneHead) {
        let pos = h.pos;
        let m = self.mem_mut(h);
        if pos >= 0 && pos < m.size() {
            m.set_flag_copied(pos);
        }
    }

    /// `Head::SetFlagMutated()`
    pub fn h_set_flag_mutated(&mut self, h: &GeneHead) {
        let pos = h.pos;
        let m = self.mem_mut(h);
        if pos >= 0 && pos < m.size() {
            m.set_flag_mutated(pos);
        }
    }

    /// `Head::SetFlagCopyMut()`
    pub fn h_set_flag_copy_mut(&mut self, h: &GeneHead) {
        let pos = h.pos;
        let m = self.mem_mut(h);
        if pos >= 0 && pos < m.size() {
            m.set_flag_copy_mut(pos);
        }
    }

    /// The inspection id of the memory a head is in: the index into
    /// `m_mem_array`, or [`GENE_SPACE_BASE`] plus the gene index.
    pub fn space_id(h: &GeneHead) -> i32 {
        if h.is_gene {
            GENE_SPACE_BASE.saturating_add(h.ms.min(i32::MAX as u32) as i32)
        } else {
            h.ms.min(i32::MAX as u32) as i32
        }
    }

    /// Every memory for inspection as `(id, memory)`: the `m_mem_array`
    /// spaces in use, then each gene.
    pub fn inspect_spaces(&self) -> Vec<(i32, &CpuMemory)> {
        let mut v: Vec<(i32, &CpuMemory)> = (0..self.mem_array.len())
            .filter(|&i| i == 0 || self.mem_ids.iter().any(|&m| m >= 0 && m as usize == i))
            .map(|i| (i as i32, &self.mem_array[i]))
            .collect();
        v.extend(
            self.genes
                .iter()
                .enumerate()
                .map(|(g, gene)| (GENE_SPACE_BASE + g as i32, &gene.memory)),
        );
        v
    }

    /// A label for an inspection memory id.
    pub fn space_label(&self, id: i32) -> Option<String> {
        if id >= GENE_SPACE_BASE {
            let g = (id - GENE_SPACE_BASE) as usize;
            let gene = self.genes.get(g)?;
            let l = gene.label.as_string();
            return Some(if l.is_empty() {
                format!("gene {g}")
            } else {
                format!("gene {g}, label {l}")
            });
        }
        if id == 0 {
            return Some("genome".to_string());
        }
        // `Inst_SetMemory` names a space by a register letter.
        let slot = self
            .mem_ids
            .iter()
            .position(|&m| m >= 0 && i32::from(m) == id)?;
        Some(format!("memory {}", char::from(b'A' + (slot % 26) as u8)))
    }

    /// The heads of thread `t` for inspection.
    pub fn thread_sites(&self, t: usize) -> Vec<HeadSite> {
        self.threads.get(t).map_or_else(Vec::new, |th| {
            th.heads
                .iter()
                .take(self.n)
                .map(|h| HeadSite {
                    mem_space: Self::space_id(h),
                    pos: h.pos,
                })
                .collect()
        })
    }

    /// A short description of thread `t` for the debugger.
    pub fn thread_status_text(&self, t: usize) -> String {
        let Some(th) = self.threads.get(t) else {
            return String::new();
        };
        let reg = |r: i32| format!("{}X", char::from(b'A' + (r.rem_euclid(26)) as u8));
        let mut parts: Vec<String> = Vec::new();
        if !th.thread_label.as_string().is_empty() {
            parts.push(format!("label {}", th.thread_label.as_string()));
        }
        if !th.running {
            parts.push("stopped".to_string());
        } else if th.active {
            parts.push("running".to_string());
        } else if th.wait_reg < 0 {
            parts.push("waking next cycle".to_string());
        } else {
            let op = match (th.wait_greater, th.wait_equal, th.wait_less) {
                (true, false, false) => ">",
                (false, true, false) => "=",
                (false, false, true) => "<",
                (true, true, false) => ">=",
                (false, true, true) => "<=",
                (true, false, true) => "!=",
                _ => "any",
            };
            parts.push(format!(
                "waiting for {} {} {} into {}",
                reg(th.wait_reg),
                op,
                th.wait_value,
                reg(th.wait_dst as i32)
            ));
        }
        if th.reading_label {
            parts.push("reading a label".to_string());
        }
        if th.reading_seq {
            parts.push("reading a sequence".to_string());
        }
        parts.join(", ")
    }

    /// `m_cur_thread` clamped into the thread array (the C++ leaves it one
    /// past the end after a cycle; reads there are undefined).
    #[inline]
    pub fn ct(&self) -> usize {
        if self.cur_thread < self.threads.len() {
            self.cur_thread
        } else {
            0
        }
    }

    /// `getHead(h)` (a copy).
    #[inline]
    pub fn head(&self, h: usize) -> GeneHead {
        self.threads[self.ct()].heads[h % MAX_REGS]
    }

    /// `getHead(h)`, mutable.
    #[inline]
    pub fn head_mut(&mut self, h: usize) -> &mut GeneHead {
        let ct = self.ct();
        &mut self.threads[ct].heads[h % MAX_REGS]
    }

    /// `getIP()` (a copy).
    #[inline]
    pub fn ip(&self) -> GeneHead {
        self.head(H_IP)
    }

    /// `getHead(h).Advance()`
    pub fn head_advance(&mut self, h: usize) {
        let mut hd = self.head(h);
        self.h_advance(&mut hd);
        *self.head_mut(h) = hd;
    }

    /// `getHead(h).Adjust()`
    pub fn head_adjust(&mut self, h: usize) {
        let mut hd = self.head(h);
        self.h_adjust(&mut hd);
        *self.head_mut(h) = hd;
    }

    /// `getIP().Advance()`
    #[inline]
    pub fn ip_advance(&mut self) {
        self.head_advance(H_IP);
    }

    /// `getIP().SetFlagExecuted()`
    #[inline]
    pub fn ip_set_flag_executed(&mut self) {
        let ip = self.ip();
        self.h_set_flag_executed(&ip);
    }

    // ------------------------------------------------------------------
    // Registers
    // ------------------------------------------------------------------

    /// `m_threads[m_cur_thread].reg[r]` (a copy).
    #[inline]
    pub fn regv(&self, r: usize) -> DataValue {
        self.threads[self.ct()].reg[r % MAX_REGS]
    }

    /// `getRegister(r)`
    #[inline]
    pub fn reg(&self, r: usize) -> i32 {
        self.regv(r).value
    }

    /// `getRegister(ctx, r)` (GP8): register `r`, or for `r` past the last
    /// register the value of the immediate-value instruction whose library
    /// index it is (`rand` draws).
    pub fn reg_ctx(&self, w: &mut dyn OrgInterface, r: i32) -> i32 {
        match usize::try_from(r) {
            Ok(r) if r < self.n => self.reg(r),
            _ => imm_value(self.imm, r, w),
        }
    }

    /// `getRegisterData(ctx, r)` (GP8): as [`Self::reg_ctx`]; an immediate
    /// value is a cleared `DataValue` stamped with this cycle.
    pub fn regv_ctx(&self, w: &mut dyn OrgInterface, r: i32) -> DataValue {
        match usize::try_from(r) {
            Ok(r) if r < self.n => self.regv(r),
            _ => DataValue {
                value: imm_value(self.imm, r, w),
                originated: self.stamp(),
                ..DataValue::default()
            },
        }
    }

    /// The cycle count as a 15-bit stamp.
    #[inline]
    fn stamp(&self) -> u32 {
        self.cycle_count & MASK15
    }

    /// `setRegister(reg_num, value, from_env)`. A register number past the
    /// last register is ignored (the GP8 guard; the BCR never passes one).
    pub fn set_reg(&mut self, reg_num: usize, value: i32, from_env: bool) {
        if reg_num >= self.n {
            return;
        }
        let ct = self.ct();
        let cc = self.stamp();
        let dest = &mut self.threads[ct].reg[reg_num];
        dest.value = value;
        dest.from_env = from_env;
        dest.originated = cc;
        dest.oldest_component = cc;
        dest.env_component = from_env;
        if self.waiting_threads != 0 {
            self.check_waiting_threads(ct, reg_num);
        }
    }

    /// `setRegister(reg_num, value, src)`
    pub fn set_reg_src(&mut self, reg_num: usize, value: i32, src: &DataValue) {
        if reg_num >= self.n {
            return;
        }
        let ct = self.ct();
        let cc = self.stamp();
        let dest = &mut self.threads[ct].reg[reg_num];
        dest.value = value;
        dest.from_env = false;
        dest.originated = cc;
        dest.oldest_component = src.oldest_component;
        dest.env_component = src.env_component;
        if self.waiting_threads != 0 {
            self.check_waiting_threads(ct, reg_num);
        }
    }

    /// `setRegister(reg_num, value, op1, op2)`
    pub fn set_reg_ops(&mut self, reg_num: usize, value: i32, op1: &DataValue, op2: &DataValue) {
        if reg_num >= self.n {
            return;
        }
        let ct = self.ct();
        let cc = self.stamp();
        let dest = &mut self.threads[ct].reg[reg_num];
        dest.value = value;
        dest.from_env = false;
        dest.originated = cc;
        dest.oldest_component = op1.oldest_component.min(op2.oldest_component);
        dest.env_component = op1.env_component || op2.env_component;
        if self.waiting_threads != 0 {
            self.check_waiting_threads(ct, reg_num);
        }
    }

    /// `checkWaitingThreads(cur_thread, reg_num)`
    pub fn check_waiting_threads(&mut self, cur_thread: usize, reg_num: usize) {
        for i in 0..self.threads.len() {
            let t = &self.threads[i];
            if i != cur_thread && !t.active && t.wait_reg == reg_num as i32 {
                let wait_value = t.wait_value;
                let src = self.threads[cur_thread].reg[reg_num % MAX_REGS];
                let check_value = src.value;
                if (t.wait_greater && check_value > wait_value)
                    || (t.wait_equal && check_value == wait_value)
                    || (t.wait_less && check_value < wait_value)
                {
                    self.threads[i].active = true;
                    self.waiting_threads = self.waiting_threads.wrapping_sub(1) & self.counter_mask;
                    let cc = self.stamp();
                    let dst = self.threads[i].wait_dst as usize % MAX_REGS;
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
    pub fn stack_pop(&mut self) -> DataValue {
        let ct = self.ct();
        if self.threads[ct].cur_stack == 0 {
            self.threads[ct].stack.pop()
        } else {
            self.global_stack.pop()
        }
    }

    /// `getStack(stack_id)`
    pub fn stack_mut(&mut self, stack_id: i32) -> &mut DvStack {
        let ct = self.ct();
        if stack_id == 0 {
            &mut self.threads[ct].stack
        } else {
            &mut self.global_stack
        }
    }

    /// `switchStack()`
    pub fn switch_stack(&mut self) {
        let ct = self.ct();
        let t = &mut self.threads[ct];
        t.cur_stack = t.cur_stack.wrapping_add(1);
        if t.cur_stack > 1 {
            t.cur_stack = 0;
        }
    }

    /// `GetStack(depth, stack_id, in_thread)`
    pub fn get_stack(&self, depth: i32, stack_id: i32, in_thread: i32) -> i32 {
        let t = if in_thread >= self.threads.len() as i32 || in_thread < 0 {
            self.ct()
        } else {
            in_thread as usize
        };
        if self.threads.is_empty() {
            return 0;
        }
        let stack_id = if stack_id == -1 {
            i32::from(self.threads[t].cur_stack)
        } else {
            stack_id
        };
        if stack_id == 0 {
            self.threads[t].stack.get(depth).value
        } else if stack_id == 1 {
            self.global_stack.get(depth).value
        } else {
            0
        }
    }

    // ------------------------------------------------------------------
    // Labels
    // ------------------------------------------------------------------

    /// `readLabel(head, label, max_size)`: read the nops after `head` into
    /// `label`, leaving `head` on the last one.
    pub fn read_label_at(
        &mut self,
        head: &mut GeneHead,
        label: &mut CodeLabel,
        max_size: i32,
        max_label_exe_size: i32,
    ) {
        let mut count = 0;
        label.clear();
        while self.is_nop(self.h_next_inst(head)) && count < max_size {
            count += 1;
            self.h_advance(head);
            label.add_nop(self.nop_mod(self.h_inst(head)));
            if label.get_size() <= max_label_exe_size {
                self.h_set_flag_executed(head);
            }
        }
    }

    /// `readLabel(getIP(), GetLabel())`
    pub fn read_label_ip(&mut self, max_label_exe_size: i32) {
        let ct = self.ct();
        let mut ip = self.threads[ct].heads[H_IP];
        let mut label = std::mem::take(&mut self.threads[ct].next_label);
        self.read_label_at(
            &mut ip,
            &mut label,
            CodeLabel::MAX_LENGTH as i32,
            max_label_exe_size,
        );
        self.threads[ct].heads[H_IP] = ip;
        self.threads[ct].next_label = label;
    }

    /// `GetLabel()` (a copy).
    pub fn label(&self) -> CodeLabel {
        self.threads[self.ct()].next_label.clone()
    }

    /// `GetLabel()`, mutable.
    pub fn label_mut(&mut self) -> &mut CodeLabel {
        let ct = self.ct();
        &mut self.threads[ct].next_label
    }

    /// `ReadInst(in_inst)`: extend or clear the read label and nop
    /// sequence of the current thread.
    pub fn read_inst(&mut self, in_inst: Instruction) {
        let is_nop = self.is_nop(in_inst);
        let is_label = self.is_label(in_inst);
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

    /// `FindLabelStart(head, default_pos, mark_executed)`: the new head.
    pub fn find_label_start(
        &mut self,
        head: GeneHead,
        default_pos: GeneHead,
        mark_executed: bool,
        mles: i32,
    ) -> GeneHead {
        let search_label = self.label();
        let n = search_label.get_size();
        if n == 0 {
            return default_pos;
        }
        let mut head = head;
        let size = self.mem(&head).size();
        let at = |s: &Self, p: i32| inst_at(s.mem(&head).as_sequence(), p);
        let mut pos = 0;
        while pos < size {
            if self.is_label(at(self, pos)) {
                pos += 1;
                let mut size_matched = 0;
                while size_matched < n && pos < size {
                    let inst = at(self, pos);
                    if !self.is_nop(inst) || search_label.get(size_matched) != self.nop_mod(inst) {
                        break;
                    }
                    size_matched += 1;
                    pos += 1;
                }
                if size_matched == n {
                    if mark_executed {
                        size_matched += 1;
                        let start = pos - size_matched;
                        let max = mles.wrapping_add(1);
                        let mut i = 0;
                        while i < size_matched && i < max {
                            let p = start + i;
                            let m = self.mem_mut(&head);
                            if p >= 0 && p < m.size() {
                                m.set_flag_executed(p);
                            }
                            i += 1;
                        }
                    }
                    self.h_set_position(&mut head, pos - 1);
                    return head;
                }
                continue;
            }
            pos += 1;
        }
        default_pos
    }

    /// `FindNopSequenceStart(head, default_pos, mark_executed)`.
    pub fn find_nop_sequence_start(
        &mut self,
        head: GeneHead,
        default_pos: GeneHead,
        mark_executed: bool,
        mles: i32,
    ) -> GeneHead {
        let search_label = self.label();
        let n = search_label.get_size();
        if n == 0 {
            return default_pos;
        }
        let mut head = head;
        let size = self.mem(&head).size();
        let at = |s: &Self, p: i32| inst_at(s.mem(&head).as_sequence(), p);
        let mut pos = 0;
        while pos < size {
            if self.is_nop(at(self, pos)) {
                let mut size_matched = 0;
                while size_matched < n && pos < size {
                    let inst = at(self, pos);
                    if !self.is_nop(inst) || search_label.get(size_matched) != self.nop_mod(inst) {
                        break;
                    }
                    size_matched += 1;
                    pos += 1;
                }
                if size_matched == n {
                    if mark_executed {
                        let start = pos - size_matched;
                        let mut i = 0;
                        while i < size_matched && i < mles {
                            let p = start + i;
                            let m = self.mem_mut(&head);
                            if p >= 0 && p < m.size() {
                                m.set_flag_executed(p);
                            }
                            i += 1;
                        }
                    }
                    self.h_set_position(&mut head, pos - 1);
                    return head;
                }
            }
            pos += 1;
        }
        default_pos
    }

    /// `FindLabelForward(head, default_pos, mark_executed)`.
    pub fn find_label_forward(
        &mut self,
        head: GeneHead,
        default_pos: GeneHead,
        mark_executed: bool,
        mles: i32,
    ) -> GeneHead {
        let search_label = self.label();
        let n = search_label.get_size();
        if n == 0 {
            return default_pos;
        }
        let mut head = head;
        self.h_adjust(&mut head);
        let mut pos = head;
        self.h_advance(&mut pos);
        while pos.pos != head.pos {
            if self.is_label(self.h_inst(&pos)) {
                let label_start = pos.pos;
                self.h_advance(&mut pos);
                let mut size_matched = 0;
                while size_matched < n && pos.pos != head.pos {
                    let inst = self.h_inst(&pos);
                    if !self.is_nop(inst) || search_label.get(size_matched) != self.nop_mod(inst) {
                        break;
                    }
                    size_matched += 1;
                    self.h_advance(&mut pos);
                }
                if size_matched == n {
                    self.h_retreat(&mut pos);
                    let found_pos = pos.pos;
                    if mark_executed {
                        self.h_set_position(&mut pos, label_start);
                        let max = mles.wrapping_add(1);
                        let mut i = 0;
                        while i < size_matched && i < max {
                            self.h_set_flag_executed(&pos);
                            i += 1;
                            self.h_advance(&mut pos);
                        }
                    }
                    self.h_set_position(&mut head, found_pos);
                    return head;
                }
                continue;
            }
            self.h_advance(&mut pos);
        }
        default_pos
    }

    /// `FindLabelBackward(head, default_pos, mark_executed)`.
    ///
    /// QUIRK: the search loop runs while the scan head differs from
    /// `head`, and it starts at `head`, so it never runs: a non-empty label
    /// always gives `default_pos`.
    pub fn find_label_backward(&mut self, _head: GeneHead, default_pos: GeneHead) -> GeneHead {
        default_pos
    }

    /// `FindNopSequenceForward(head, default_pos, mark_executed)`.
    pub fn find_nop_sequence_forward(
        &mut self,
        head: GeneHead,
        default_pos: GeneHead,
        mark_executed: bool,
        mles: i32,
    ) -> GeneHead {
        let search_label = self.label();
        let n = search_label.get_size();
        if n == 0 {
            return default_pos;
        }
        let mut head = head;
        self.h_adjust(&mut head);
        let mut pos = head;
        self.h_advance(&mut pos);
        while pos.pos != head.pos {
            if self.is_nop(self.h_inst(&pos)) {
                let label_start = pos.pos;
                let mut size_matched = 0;
                while size_matched < n && pos.pos != head.pos {
                    let inst = self.h_inst(&pos);
                    if !self.is_nop(inst) || search_label.get(size_matched) != self.nop_mod(inst) {
                        break;
                    }
                    size_matched += 1;
                    self.h_advance(&mut pos);
                }
                if size_matched == n {
                    self.h_retreat(&mut pos);
                    let found_pos = pos.pos;
                    if mark_executed {
                        self.h_set_position(&mut pos, label_start);
                        let mut i = 0;
                        while i < size_matched && i < mles {
                            self.h_set_flag_executed(&pos);
                            i += 1;
                            self.h_advance(&mut pos);
                        }
                    }
                    self.h_set_position(&mut head, found_pos);
                    return head;
                }
            }
            if pos.pos == head.pos {
                break;
            }
            self.h_advance(&mut pos);
        }
        default_pos
    }

    /// `FindNopSequenceBackward(head, default_pos, mark_executed)`.
    ///
    /// QUIRK: as [`Self::find_label_backward`], the loop never runs.
    pub fn find_nop_sequence_backward(
        &mut self,
        _head: GeneHead,
        default_pos: GeneHead,
    ) -> GeneHead {
        default_pos
    }

    // ------------------------------------------------------------------
    // Modifiers
    // ------------------------------------------------------------------

    /// `FindModifiedRegister(default_register[, accept_immediate])`: the
    /// following nop's register (or, with `accept_immediate`, an
    /// immediate-value instruction's library index), else the default.
    pub fn fmr_ex(&mut self, default_register: i32, accept_immediate: bool) -> i32 {
        let inst = self.h_next_inst(&self.ip());
        if self.is_nop(inst) {
            self.ip_advance();
            let r = self.nop_mod(inst);
            self.ip_set_flag_executed();
            return r;
        }
        if accept_immediate && self.is_immediate_value(inst) {
            self.ip_advance();
            let r = self.is.lib_function_index(inst);
            self.ip_set_flag_executed();
            return r;
        }
        default_register
    }

    /// `FindModifiedRegister(default_register)` for a valid default.
    #[inline]
    pub fn fmr(&mut self, default_register: usize) -> usize {
        self.fmr_ex(default_register as i32, false).max(0) as usize
    }

    /// `FindModifiedNextRegister(default_register[, accept_immediate])`.
    pub fn fmnr_ex(&mut self, default_register: i32, accept_immediate: bool) -> i32 {
        let inst = self.h_next_inst(&self.ip());
        if self.is_nop(inst) {
            self.ip_advance();
            let r = self.nop_mod(inst);
            self.ip_set_flag_executed();
            return r;
        }
        if accept_immediate && self.is_immediate_value(inst) {
            self.ip_advance();
            let r = self.is.lib_function_index(inst);
            self.ip_set_flag_executed();
            return r;
        }
        (default_register + 1).rem_euclid(self.n as i32)
    }

    /// `FindModifiedNextRegister(default_register)`.
    #[inline]
    pub fn fmnr(&mut self, default_register: usize) -> usize {
        self.fmnr_ex(default_register as i32, false).max(0) as usize
    }

    /// `FindModifiedHead(default_head)`.
    #[inline]
    pub fn fmh(&mut self, default_head: usize) -> usize {
        self.fmr(default_head)
    }

    /// `FindNextRegister(base_reg)`.
    #[inline]
    pub fn next_reg(&self, base_reg: usize) -> usize {
        (base_reg + 1) % self.n
    }

    /// `FindUpstreamModifiedRegister(offset, default_register)`.
    pub fn find_upstream_modified_register(&mut self, offset: i32, default_register: i32) -> i32 {
        let mut location = self.ip();
        self.h_jump(&mut location, -offset - 1);
        let inst = self.h_inst(&location);
        if self.is_nop(inst) {
            let r = self.nop_mod(inst);
            self.h_set_flag_executed(&location);
            return r;
        }
        default_register
    }

    // ------------------------------------------------------------------
    // Threads and genes
    // ------------------------------------------------------------------

    /// `Thread::Reset(hardware, start_pos)`.
    ///
    /// QUIRK: `thread_label`, `read_seq` and the wait fields are kept.
    pub fn thread_reset(&mut self, t: usize, start_pos: GeneHead) {
        let ss = self.is.stack_size();
        let n = self.n;
        let th = &mut self.threads[t];
        for r in th.reg.iter_mut().take(n) {
            r.clear();
        }
        th.heads[H_READ] = GeneHead::new(0, 0, false);
        th.heads[H_WRITE] = GeneHead::new(0, 0, false);
        th.heads[H_IP] = start_pos;
        for h in th.heads.iter_mut().take(n).skip(H_FLOW) {
            *h = start_pos;
        }
        th.stack.clear(ss);
        th.cur_stack = 0;
        th.reading_label = false;
        th.reading_seq = false;
        th.running = true;
        th.active = true;
        th.read_label.clear();
        th.next_label.clear();
        th.sensor_session = LookInit::default();
    }

    /// `threadCreate(thread_label, start_pos)`: restart (or leave running)
    /// the thread with this label, or add a new one.
    pub fn thread_create(
        &mut self,
        org: &mut OrgState,
        thread_label: &CodeLabel,
        start_pos: GeneHead,
    ) {
        if thread_label.get_size() > 0 {
            for t in 0..self.threads.len() {
                if self.threads[t].thread_label == *thread_label {
                    if !self.threads[t].running {
                        self.thread_reset(t, start_pos);
                        self.running_threads =
                            self.running_threads.wrapping_add(1) & self.counter_mask;
                    }
                    return;
                }
            }
        }
        let thread_id = self.threads.len();
        self.threads.push(GeneThread::default());
        self.threads[thread_id].thread_label = thread_label.clone();
        self.thread_reset(thread_id, start_pos);
        self.running_threads = self.running_threads.wrapping_add(1) & self.counter_mask;
        org.phenotype.is_multi_thread = true;
    }

    /// `setupGenes()`: one gene (and thread) per promoter; without any, the
    /// whole genome is gene 0.
    ///
    /// A promoter followed by a terminator gives a one-instruction gene
    /// (opcode 0): the gene's `cCPUMemory` is default-constructed with one
    /// instruction, so the C++ "ignore zero length genes" branch never runs.
    pub fn setup_genes(&mut self, org: &mut OrgState, mles: i32) {
        let mut cur_promoter = GeneHead::new(0, 0, false);
        loop {
            if self.is_promoter(self.h_inst(&cur_promoter)) {
                self.h_set_flag_executed(&cur_promoter);
                let gene_id = self.genes.len();
                self.genes.push(Gene::default());

                let mut gene_content_start = cur_promoter;
                let mut label = CodeLabel::new();
                self.read_label_at(
                    &mut gene_content_start,
                    &mut label,
                    CodeLabel::MAX_LENGTH as i32,
                    mles,
                );
                self.genes[gene_id].label = label.clone();

                let mut seghead = gene_content_start;
                self.h_advance(&mut seghead);
                let mut gene_idx = 0;
                while !self.is_terminator(self.h_inst(&seghead)) && seghead != gene_content_start {
                    let inst = self.h_inst(&seghead);
                    let gene = &mut self.genes[gene_id].memory;
                    if gene.size() <= gene_idx {
                        gene.resize(gene.size() + 1);
                    }
                    gene[gene_idx] = inst;
                    self.h_set_flag_executed(&seghead);
                    gene_idx += 1;
                    self.h_advance(&mut seghead);
                }
                let thread_start = GeneHead::new(0, gene_id as u32, true);
                self.thread_create(org, &label, thread_start);
            }
            self.h_advance(&mut cur_promoter);
            if self.h_at_end(&cur_promoter) {
                break;
            }
        }

        if self.genes.is_empty() {
            let mut g = Gene::default();
            g.memory.assign_memory(&self.mem_array[0]);
            self.genes.push(g);
            for i in 0..self.mem_array[0].size() {
                self.mem_array[0].set_flag_executed(i);
            }
            let label = CodeLabel::new();
            self.thread_create(org, &label, GeneHead::new(0, 0, true));
        }
    }

    /// `internalReset()` (without the GP8's `m_hw_reset`; the caller resizes
    /// the cost arrays to the new thread count).
    pub fn internal_reset(&mut self, org: &mut OrgState, cfg: &Config) {
        self.spec_stall = false;
        self.cycle_count = 0;
        self.last_output = 0;
        self.sensor.reset(cfg.use_avatars);
        for s in self.sensor_sessions.iter_mut() {
            *s = LookInit::default();
        }
        let ss = self.is.stack_size();
        self.global_stack.clear(ss);
        self.threads.clear();
        self.waiting_threads = 0;
        self.running_threads = 0;
        self.mem_array.truncate(1);
        self.mem_ids = [-1; MAX_REGS];
        self.mem_ids[0] = 0;
        self.genes.clear();
        self.setup_genes(org, cfg.max_label_exe_size);
    }

    /// `calcCopiedSize(parent_size, child_size)`: copied sites of the
    /// offspring's memory space.
    pub fn calc_copied_size(&self) -> i32 {
        let Some(m) = self.mem_array.get(self.cur_offspring.max(0) as usize) else {
            return 0;
        };
        (0..m.size()).filter(|&i| m.flag_copied(i)).count() as i32
    }

    /// The trailing nop modifiers after the IP (up to `n`), as letters,
    /// or `NoMods` (the mini-trace column).
    pub fn trailing_mods(&self) -> String {
        let ip = self.ip();
        let memory = self.mem(&ip);
        let mut pos = ip.pos;
        let mut mods = String::new();
        for _ in 0..self.n {
            pos += 1;
            if pos >= memory.size() {
                pos = 0;
            }
            let inst = inst_at(memory.as_sequence(), pos);
            if self.is_nop(inst) {
                mods.push((self.nop_mod(inst) as u8).wrapping_add(b'A') as char);
            } else {
                break;
            }
        }
        if mods.is_empty() {
            "NoMods".to_string()
        } else {
            mods
        }
    }
}

/// The immediate-value methods of the GP8 (`Val_*`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Imm {
    /// `Val_Zero`
    Zero,
    /// `Val_One`
    One,
    /// `Val_MaxInt`
    MaxInt,
    /// `Val_Rand`: a random magnitude below `INT_MAX` with a random sign.
    Rand,
}

/// `(this->*(m_imm_methods[r]))(ctx)`. Only immediate-value instructions
/// are ever looked up (`FindModifiedRegister` hands out no other library
/// index past the registers); anything else reads as 0.
fn imm_value(imm: &[Option<Imm>], r: i32, w: &mut dyn OrgInterface) -> i32 {
    match usize::try_from(r)
        .ok()
        .and_then(|r| imm.get(r).copied().flatten())
    {
        Some(Imm::Zero) | None => 0,
        Some(Imm::One) => 1,
        Some(Imm::MaxInt) => i32::MAX,
        Some(Imm::Rand) => {
            let randsign = if w.rng().get_uint_range(0, 2) != 0 {
                -1
            } else {
                1
            };
            w.rng().get_int(i32::MAX).wrapping_mul(randsign)
        }
    }
}

/// The hardware-specific parts the shared handlers need.
pub(crate) trait GeneHw: Hardware + Sized {
    /// Whether this is the GP8: operands may be immediate-value
    /// instructions (`FindModifiedRegister(reg, true)`, `getRegister(ctx,
    /// reg)`), and the few places where the GP8 text otherwise differs from
    /// the BCR's (noted at each use) take the GP8 branch.
    const IMM: bool;
    /// The C++ class, for `unsupported` messages.
    const CLASS: &'static str;
    /// The shared state.
    fn core(&self) -> &GeneCore;
    /// The shared state, mutable.
    fn core_mut(&mut self) -> &mut GeneCore;
    /// `internalReset()`.
    fn internal_reset(&mut self, org: &mut OrgState, cfg: &Config);
    /// This hardware's type id.
    fn type_id(&self) -> i32;
}

/// `Reset(ctx)`: `cOrganism::HardwareReset`, the cost tables, then
/// `internalReset()`.
pub(crate) fn full_reset<H: GeneHw>(hw: &mut H, org: &mut OrgState, world: &mut dyn OrgInterface) {
    crate::organism::hardware_reset(org, hw, world);
    let cfg = world.config();
    hw.base_mut().reset_costs(cfg);
    hw.internal_reset(org, cfg);
}

/// `cOrganism::DoOutput` plus the bonus instructions it triggers.
pub(crate) fn org_do_output<H: GeneHw>(
    hw: &mut H,
    org: &mut OrgState,
    world: &mut dyn OrgInterface,
    value: Option<i32>,
    on_divide: bool,
) -> Result<(), String> {
    let is = hw.base().inst_set.clone();
    let effects = do_output(
        org,
        &hw.base().ext_mem,
        &is,
        world,
        value,
        on_divide,
        false,
        None,
    )?;
    for inst in effects.bonus_insts {
        hw.process_bonus_inst(org, world, inst)?;
    }
    Ok(())
}

/// `cOrganism::ActivateDivide(ctx)`: the last output test, then
/// `m_interface->Divide`. Returns whether the parent is still alive.
pub(crate) fn activate_divide<H: GeneHw>(
    hw: &mut H,
    org: &mut OrgState,
    world: &mut dyn OrgInterface,
) -> Result<bool, String> {
    org_do_output(hw, org, world, None, true)?;
    let offspring = org.offspring_genome.clone();
    let split_success = hw.print_mini_trace_success(1);
    Ok(crate::mini_trace::divide_lending_traces(
        hw,
        split_success,
        org,
        world,
        &offspring,
    ))
}

/// The offspring genome (`InstructionSequence` plus this set's name).
pub(crate) fn make_offspring<H: GeneHw>(hw: &H, seq: InstructionSequence) -> Genome {
    Genome::with_inst_set(hw.type_id(), hw.base().inst_set.inst_set_name(), seq)
}

/// `cHardwareBase::Divide_CheckViable` over memory space 0 with this
/// hardware's `calcCopiedSize` (`using_repro`: no copied-size test) and
/// `internalResetOnFailedDivide`.
pub(crate) fn check_viable<H: GeneHw>(
    hw: &mut H,
    org: &mut OrgState,
    world: &mut dyn OrgInterface,
    parent_size: i32,
    child_size: i32,
    using_repro: bool,
) -> bool {
    let copied = if using_repro {
        None
    } else {
        Some(hw.core().calc_copied_size())
    };
    let v = divide_check_viable(
        org,
        world,
        &hw.core().mem_array[0],
        parent_size,
        child_size,
        copied,
    );
    match v {
        Viability::Viable => true,
        Viability::NotViable => false,
        Viability::NotViableReset => {
            hw.internal_reset(org, world.config());
            false
        }
    }
}

/// `Divide_Main(ctx, mem_space_used, write_head_pos, mut_multiplier)`.
pub(crate) fn divide_main<H: GeneHw>(
    hw: &mut H,
    org: &mut OrgState,
    world: &mut dyn OrgInterface,
    mem_space_used: i32,
    write_head_pos: i32,
    mut_multiplier: f64,
) -> Result<bool, String> {
    if mem_space_used < 0 || hw.core().mem_array.len() as i32 <= mem_space_used {
        return Ok(false);
    }
    // `m_cur_offspring` is a signed 5-bit field.
    hw.core_mut().cur_offspring = (mem_space_used << 27) >> 27;
    let parent_size = hw.core().mem_array[0].size();
    if !check_viable(hw, org, world, parent_size, write_head_pos, false) {
        return Ok(false);
    }
    let ms = mem_space_used as usize;
    hw.core_mut().mem_array[ms].resize(write_head_pos);
    let seq = hw.core().mem_array[ms].to_sequence();
    org.offspring_genome = make_offspring(hw, seq);

    let is = hw.base().inst_set.clone();
    divide_do_mutations(
        &is,
        org,
        world,
        &mut hw.core_mut().mem_array[0],
        mut_multiplier,
        i32::MAX,
    )?;
    divide_test_fitness_measures(org, world, false);
    hw.base_mut().reset_ft_costs();

    let parent_alive = activate_divide(hw, org, world)?;
    // `m_mem_array[mem_space_used] = InstructionSequence("a")`
    if let Some(m) = hw.core_mut().mem_array.get_mut(ms) {
        m.assign(&InstructionSequence::from_string("a"));
    }
    if parent_alive {
        match world.config().divide_method {
            DIVIDE_METHOD_SPLIT => full_reset(hw, org, world),
            DIVIDE_METHOD_BIRTH => {
                let core = hw.core_mut();
                let n = core.n;
                for x in 0..n {
                    *core.head_mut(x) = GeneHead::new(0, 0, false);
                }
                for x in 0..n {
                    core.set_reg(x, 0, false);
                }
                if world.config().inherit_merit == 0 {
                    org.phenotype.reset_merit(world.config());
                }
            }
            _ => {}
        }
        hw.core_mut().advance_ip = false;
    }
    Ok(true)
}

/// `m_tracer->TraceHardware(ctx, *this, bonus, mini, exec_success)`;
/// `header` builds the mini-trace file header comments.
pub(crate) fn gene_trace<H: GeneHw>(
    hw: &H,
    org: &mut OrgState,
    world: &mut dyn OrgInterface,
    bonus: bool,
    mini: bool,
    exec_success: i32,
    header: fn(i32, i32, i32, &str, i32) -> Vec<String>,
) {
    let Some(tracer) = hw.base().tracer.clone() else {
        return;
    };
    let mut guard = match tracer.lock() {
        Ok(g) => g,
        Err(p) => p.into_inner(),
    };
    if mini {
        if let Some((gen_id, genotype)) = guard.mini_header_due() {
            let h = header(
                world.update(),
                org.id,
                gen_id,
                &genotype,
                org.genome().representation().size(),
            );
            guard.write_mini_header(&h);
        }
    }
    let world_cell = std::cell::RefCell::new(world);
    // The mini-trace status reads the faced cell's resources, which may
    // injure the organism (crate::org_interface), so it gets it mutably.
    let org_cell = std::cell::RefCell::new(org);
    guard.trace_hardware(
        &mut || print_status(&org_cell.borrow(), hw, &mut **world_cell.borrow_mut()),
        &mut || {
            hw.print_mini_trace_status(&mut org_cell.borrow_mut(), &mut **world_cell.borrow_mut())
        },
        &mut |e| hw.print_mini_trace_success(e),
        bonus,
        mini,
        exec_success,
    );
}

/// The mini-trace status columns the two hardware types share, from the
/// update through the trailing nops; `regs` are the register values,
/// `extra` the GP8 queue columns (written before the registers).
pub(crate) fn mini_status_common(
    core: &GeneCore,
    org: &mut OrgState,
    world: &mut dyn OrgInterface,
    extra: &str,
) -> String {
    use std::fmt::Write;
    let mut s = String::new();
    let _ = write!(
        s,
        "{} {} {} ",
        core.cycle_count,
        core.cur_uop,
        world.update()
    );
    s.push_str(extra);
    let t = &core.threads[core.ct()];
    for r in t.reg.iter().take(core.n) {
        let _ = write!(s, "{} ({}) ", r.value, r.originated);
    }
    let _ = write!(
        s,
        "{} {} {} {} {} {} ",
        core.cur_thread,
        core.ip().pos,
        core.head(H_READ).pos,
        core.head(H_WRITE).pos,
        core.head(H_FLOW).pos,
        core.last_output
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
    let ua = core.use_avatar;
    let _ = write!(s, "{} ", world.get_cell_id());
    if ua != 0 {
        let _ = write!(s, "{} ", world.get_av_cell_id(0));
    }
    if ua == 0 {
        let _ = write!(s, "{} ", world.get_faced_dir());
        let _ = write!(s, "{} ", i32::from(world.is_neighbor_cell_occupied()));
    } else {
        let _ = write!(s, "{} ", world.get_av_facing(0));
        let _ = write!(s, "{} ", i32::from(world.faced_has_av(0)));
    }
    let habitats: Vec<i32> = world
        .environment()
        .resource_lib()
        .resources()
        .iter()
        .map(|r| r.habitat)
        .collect();
    let levels = if ua == 0 {
        world.get_faced_cell_resources(org)
    } else {
        world.get_av_faced_resources(org, 0)
    };
    let mut wall = 0;
    let mut hill = 0;
    for (i, level) in levels.iter().enumerate() {
        let h = habitats.get(i).copied().unwrap_or(0);
        if h == 2 && *level > 0.0 {
            wall = 1;
        }
        if h == 1 && *level > 0.0 {
            hill = 1;
        }
        if hill == 1 && wall == 1 {
            break;
        }
    }
    let _ = write!(s, "{} {} ", hill, wall);
    let ip_inst = core.h_inst(&core.ip());
    let _ = write!(s, "{} ", core.inst_name(ip_inst));
    let _ = write!(s, "{} ", core.trailing_mods());
    s
}

/// The `PrintStatus` text the two hardware types share; `reg_value` is
/// what the first column of each register shows.
pub(crate) fn print_status_common(core: &GeneCore, org: &OrgState) -> String {
    use std::fmt::Write;
    let mut s = String::new();
    let ip = core.ip();
    let _ = writeln!(
        s,
        "CPU CYCLE:{}.{} THREAD:{}  IP:{} ({})",
        org.phenotype.cpu_cycles_used,
        core.cur_uop,
        core.cur_thread,
        ip.pos,
        core.inst_name(core.h_inst(&ip))
    );
    let t = &core.threads[core.ct()];
    for (i, r) in t.reg.iter().take(core.n).enumerate() {
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
        core.head(H_READ).pos,
        core.head(H_WRITE).pos,
        core.head(H_FLOW).pos,
        t.read_label.as_string(),
        core.last_output
    );
    for stack_id in 0..2 {
        let mark = if i32::from(t.cur_stack) == stack_id {
            '*'
        } else {
            ' '
        };
        let _ = write!(s, "{} Stack {}:", mark, stack_id);
        for i in 0..core.is.stack_size() {
            let _ = write!(s, " Ox{:08x}", core.get_stack(i, stack_id, 0) as u32);
        }
        s.push('\n');
    }
    for (i, mem) in core.mem_array.iter().enumerate() {
        let _ = writeln!(s, "  Mem {} ({}): {}", i, mem.size(), mem.as_string());
    }
    for (i, g) in core.genes.iter().enumerate() {
        let _ = writeln!(
            s,
            "  Gene {} ({}): {}",
            i,
            g.memory.size(),
            g.memory.as_string()
        );
    }
    s
}

/// `cHardwareBase::RecordNavTrace(use_avatar)`.
pub(crate) fn record_nav_trace(
    core: &GeneCore,
    base: &mut HardwareBase,
    world: &mut dyn OrgInterface,
) {
    let use_avatar = core.use_avatar != 0;
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
    base.navtraceloc.push(loc);
    base.navtracefacing.push(facing);
    base.navtraceupdate.push(world.update());
}

// ----------------------------------------------------------------------
// cHardwareBCR
// ----------------------------------------------------------------------

/// `cHardwareBCR`.
pub struct HardwareBcr {
    pub(crate) base: HardwareBase,
    pub(crate) core: GeneCore,
    pub(crate) functions: &'static [Option<BcrInstFn>],
    /// `m_behav_class_used[3]`
    pub(crate) behav_class_used: [bool; 3],
}

impl std::fmt::Debug for HardwareBcr {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("HardwareBcr")
            .field("genes", &self.core.genes.len())
            .field("threads", &self.core.threads.len())
            .field("cur_thread", &self.core.cur_thread)
            .finish()
    }
}

/// The mini-trace file header of `cHardwareBCR::SetupMiniTraceFileHeader`:
/// the experimental hardware's with a "MicroOp" column after "CPU Cycle".
pub fn bcr_mini_header(
    update: i32,
    org_id: i32,
    gen_id: i32,
    genotype: &str,
    genome_size: i32,
) -> Vec<String> {
    let mut v =
        crate::mini_trace::experimental_header(update, org_id, gen_id, genotype, genome_size);
    if let Some(i) = v.iter().position(|s| s == "CPU Cycle") {
        v.insert(i + 1, "MicroOp".to_string());
    }
    v
}

impl HardwareBcr {
    /// `cHardwareBCR(ctx, world, organism, inst_set)`: memory space 0 from
    /// the organism's genome, then `Reset(ctx)` (whose `HardwareReset` runs
    /// without an interface).
    pub fn new(cfg: &Config, org: &mut OrgState, inst_set: Arc<InstSet>) -> Self {
        let seq = org.initial_genome.representation().clone();
        let mut hw = HardwareBcr {
            base: HardwareBase::new(cfg, inst_set.clone()),
            core: GeneCore::new(BCR_NUM_REGISTERS, cfg, &seq, inst_set, 0xF, &[]),
            functions: bcr_handler_table(),
            behav_class_used: [false; 3],
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
    pub(crate) fn execute_inst(
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
        gene_trace(self, org, world, bonus, mini, exec_success, bcr_mini_header);
    }

    /// `CheckImplicitRepro(ctx)`.
    fn check_implicit_repro(
        &mut self,
        org: &mut OrgState,
        world: &mut dyn OrgInterface,
    ) -> Result<(), String> {
        if self.base.implicit_repro_active && implicit_repro_due(org, world.config(), false) {
            crate::hardware_bcr_insts::inst_repro(self, org, world)?;
        }
        Ok(())
    }
}

impl GeneHw for HardwareBcr {
    const IMM: bool = false;
    const CLASS: &'static str = "cHardwareBCR";
    fn core(&self) -> &GeneCore {
        &self.core
    }
    fn core_mut(&mut self) -> &mut GeneCore {
        &mut self.core
    }
    fn internal_reset(&mut self, org: &mut OrgState, cfg: &Config) {
        self.core.internal_reset(org, cfg);
        self.base.resize_cost_arrays(self.core.threads.len() as i32);
    }
    fn type_id(&self) -> i32 {
        HARDWARE_TYPE_CPU_BCR
    }
}

impl Hardware for HardwareBcr {
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
        HARDWARE_TYPE_CPU_BCR
    }
    fn supports_speculative(&self) -> bool {
        true
    }

    fn reset(&mut self, org: &mut OrgState, world: &mut dyn OrgInterface) {
        full_reset(self, org, world);
    }

    /// `SingleProcess(ctx, speculative)`: `GetUOpsPerCycle()` rounds over
    /// the running, active threads. An instruction whose behavioural class
    /// (input, action, copy) was already used this cycle puts its thread to
    /// sleep until the next cycle.
    ///
    /// The C++ holds a reference to the executing thread's IP across the
    /// instruction. A failed divide under `DIVIDE_FAILURE_RESETS` frees
    /// every thread and the reference dangles; the port then advances the
    /// IP of the new thread with the same index, if any (the allocator
    /// hands the freed block back to the next thread of that size).
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
            self.behav_class_used = [false; 3];
            self.core.cur_uop = 0;
            self.core.cur_thread = 0;
        }

        let uop_ratio = self.base.inst_set.uops_per_cycle();
        if uop_ratio > 255 {
            return Err("unsupported: UOPS_PER_CYCLE above 255 (C++: the 8-bit m_cur_uop never reaches it and cHardwareBCR::SingleProcess loops forever)".to_string());
        }
        while (self.core.cur_uop as i32) < uop_ratio {
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
                let behav_class = if valid {
                    let idx = self.base.inst_set.lib_function_index(cur_inst);
                    self.base.inst_set.inst_lib().get(idx).behav_class() as usize
                } else {
                    3
                };
                if behav_class < 3 && self.behav_class_used[behav_class] {
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
                    }
                    if org.phenotype.to_delete {
                        // The C++ traces here (mini only); see the
                        // experimental hardware for why the call is skipped.
                        break;
                    }
                    if self.core.advance_ip && ct < self.core.threads.len() {
                        let mut ip = self.core.threads[ct].heads[H_IP];
                        self.core.h_advance(&mut ip);
                        self.core.threads[ct].heads[H_IP] = ip;
                    }
                    org.phenotype.inc_time_used(addl_time_cost);
                    if behav_class < 3 {
                        self.behav_class_used[behav_class] = true;
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
                            record_nav_trace(&self.core, &mut self.base, world);
                        }
                    }
                }
                if org.phenotype.to_delete {
                    break;
                }
                self.core.cur_thread += 1;
            }
            if org.phenotype.to_delete {
                break;
            }
            self.core.cur_uop = (self.core.cur_uop + 1) & 0xFF;
        }

        // A gradient habitat's `SetToDie` of this organism (see
        // `OrgInterface::take_habitat_to_die`).
        if world.take_habitat_to_die() {
            org.phenotype.to_die = true;
        }
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
        self.check_implicit_repro(org, world)?;
        Ok(!self.core.spec_die && !self.core.spec_stall)
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
    fn print_status(&self, org: &OrgState, _world: &mut dyn OrgInterface) -> String {
        print_status_common(&self.core, org)
    }

    /// `PrintMiniTraceStatus(ctx, fp)`.
    fn print_mini_trace_status(&self, org: &mut OrgState, world: &mut dyn OrgInterface) -> String {
        mini_status_common(&self.core, org, world, "")
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
        BCR_NUM_REGISTERS as i32
    }

    fn num_threads(&self) -> i32 {
        self.core.threads.len() as i32
    }

    fn cur_thread(&self) -> i32 {
        self.core.cur_thread as i32
    }
}
