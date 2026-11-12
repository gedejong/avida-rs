//! Port of `cHardwareTransSMT` (avida-core/source/cpu/cHardwareTransSMT.{h,cc}
//! at 47f13dadb), hardware type 2: the stack-based, multi-memory-space,
//! multi-threaded CPU used for SMT parasites. The instruction handlers and
//! the library table are in [`crate::hardware_transsmt_insts`].
//!
//! Layout: four stacks (three per thread, one global), a growable array of
//! memory spaces addressed by nop labels (memory 0 is the genome plus one
//! extra instruction), and threads that each own four heads. Parasites are
//! threads whose `owner` is the infecting unit; hosts run their own code in
//! thread 0.
//!
//! # Heads and memory spaces
//!
//! A C++ head caches a pointer to the memory it last adjusted into
//! (`m_memory`); `Set(const cHeadCPU&)` copies the position and memory
//! space without updating that cache. Every memory access in this
//! hardware happens right after an `Adjust()` of the head involved (the IP
//! at the start of each step, `Inst-Read`/`Inst-Write` before reading or
//! writing), and after an adjust the cache is the head's (normalised)
//! memory space. The port therefore resolves a head's memory from its
//! memory space, and computes the size an adjust needs from the memory
//! space the C++ `fullAdjust` would switch to.
//!
//! # Parasites
//!
//! [`HardwareTransSmt::parasite_infect_host`] ports `ParasiteInfectHost`
//! for the population side; the infecting unit is a [`ParasiteUnit`],
//! which `cOrganism::ParasiteInfectHost` also records in
//! [`OrgState::parasites`]. `Inject` hands the code to
//! `OrgInterface::inject_parasite` with the hardware itself (the target
//! may be the injecting organism) and `ThreadGetOwner()`: the running
//! thread's owner, `None` when the organism itself is the parent. The test
//! CPU answers `false`, as `cTestCPUInterface` does. `internalReset`
//! ends with `ClearParasites()`.
//!
//! # Undefined behaviour in the C++
//!
//! * `SingleProcess` dereferences the owner of thread 1 whenever there is
//!   more than one thread. Threads made by `Thread-Create` in a host that
//!   is not infected have a NULL owner, so the reference binary segfaults
//!   on the step after a successful `Thread-Create` (`MAX_CPU_THREADS` of
//!   2 or more). The port returns `Err` at that point.
//! * A skipped thread 0 (`INJECT_IS_VIRULENT`) with no other thread steps
//!   past the thread array; the port returns `Err`.
//! * `cLocalThread::advance_ip` of a newly created thread is
//!   uninitialised; it is read only when the current thread changes during
//!   an instruction (`Thread-Cancel`/`Thread-Kill` of the running thread).
//!   The port starts it `false`.
//! * `Inst-Write` on a memory space of size 1 copies from index -1; the
//!   port skips that copy.

use crate::code_label::CodeLabel;
use crate::cpu_stack::CpuStack;
use crate::hardware::{
    divide_check_viable, divide_do_mutations, divide_test_fitness_measures, implicit_repro_due,
    Hardware, HardwareBase, Viability,
};
use crate::hardware_transsmt_insts::{smt_functions, SmtInstFn};
use crate::head_cpu::HeadCpu;
use crate::nhardware::*;
use crate::org_interface::OrgInterface;
use crate::organism::{do_output, print_status, transmission, OrgSource, OrgState};
use avida_config::config::Config;
use avida_environment::ContextPhenotype;
use avida_genome::{CpuMemory, Genome, InstSet, Instruction, InstructionSequence};
use std::any::Any;
use std::collections::BTreeMap;
use std::sync::Arc;

/// `NUM_LOCAL_STACKS`
pub const NUM_LOCAL_STACKS: i32 = 3;
/// `NUM_GLOBAL_STACKS`
pub const NUM_GLOBAL_STACKS: i32 = 1;
/// `NUM_STACKS`
pub const NUM_STACKS: i32 = NUM_LOCAL_STACKS + NUM_GLOBAL_STACKS;
/// `NUM_NOPS`
pub const NUM_NOPS: i32 = 4;
/// `MAX_MEMSPACE_LABEL`
pub const MAX_MEMSPACE_LABEL: i32 = 3;
/// `MAX_THREAD_LABEL`
pub const MAX_THREAD_LABEL: i32 = 3;
/// `STACK_AX`
pub const STACK_AX: i32 = 0;
/// `STACK_BX`
pub const STACK_BX: i32 = 1;
/// `STACK_CX`
pub const STACK_CX: i32 = 2;
/// `STACK_DX`
pub const STACK_DX: i32 = 3;
/// `MIN_INJECT_SIZE` (avida/core/Definitions.h)
pub const MIN_INJECT_SIZE: i32 = 8;
/// `INJECT_METHOD_SPLIT` (avida/core/Definitions.h)
pub const INJECT_METHOD_SPLIT: i32 = 1;

/// The parts of a parasite `Systematics::Unit` (a `cParasite`) that the
/// hardware reads: its genome, its source and its virulence.
///
/// The unit is shared (`Arc`) between the host's thread that runs it and
/// the host's parasite list ([`OrgState::parasites`]), as the C++
/// `Systematics::UnitPtr` is. `tag` is the population's part of the unit
/// (its systematics groups); dropping the last reference drops the tag,
/// which is where the population sees the C++ `~Unit()`.
pub struct ParasiteUnit {
    /// The `UnitGenome()` instruction sequence.
    pub genome: InstructionSequence,
    /// `UnitSource()`: `arguments` holds the label of the memory space and
    /// thread the parasite occupies.
    pub source: OrgSource,
    /// `cParasite::GetVirulence()`
    pub virulence: f64,
    /// Population-side data, opaque to the hardware.
    pub tag: Option<Box<dyn Any + Send + Sync>>,
}

impl std::fmt::Debug for ParasiteUnit {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ParasiteUnit")
            .field("genome", &self.genome.as_string())
            .field("source", &self.source)
            .field("virulence", &self.virulence)
            .finish_non_exhaustive()
    }
}

/// `cHardwareTransSMT::cLocalThread`.
#[derive(Debug, Clone)]
pub struct SmtThread {
    /// `heads`
    pub heads: [HeadCpu; NUM_HEADS],
    /// `cur_head`
    pub cur_head: u8,
    /// `local_stacks`
    pub local_stacks: [CpuStack; NUM_LOCAL_STACKS as usize],
    /// `advance_ip`
    pub advance_ip: bool,
    /// `skipExecution`
    pub skip_execution: bool,
    /// `read_label`
    pub read_label: CodeLabel,
    /// `next_label`
    pub next_label: CodeLabel,
    /// `running`
    pub running: bool,
    /// `context_phenotype`
    pub context_phenotype: ContextPhenotype,
    /// `owner`: the infecting parasite, `None` for the host's own threads.
    pub owner: Option<Arc<ParasiteUnit>>,
}

impl SmtThread {
    /// `cLocalThread()` (`Reset(NULL, 0)`): heads at position 0 of memory
    /// space 0. `advance_ip` is uninitialised in the C++ (see the module
    /// documentation).
    fn new_default() -> Self {
        SmtThread {
            heads: [HeadCpu::default(); NUM_HEADS],
            cur_head: HEAD_IP as u8,
            local_stacks: [CpuStack::default(); NUM_LOCAL_STACKS as usize],
            advance_ip: false,
            skip_execution: false,
            read_label: CodeLabel::new(),
            next_label: CodeLabel::new(),
            running: true,
            context_phenotype: ContextPhenotype::default(),
            owner: None,
        }
    }

    /// `Reset(in_hardware, mem_space)`: `advance_ip` and the context
    /// phenotype are kept.
    fn reset(&mut self, mems: &[CpuMemory], mem_space: i32) {
        for s in self.local_stacks.iter_mut() {
            s.clear();
        }
        for h in self.heads.iter_mut() {
            head_reset(mems, h, mem_space);
        }
        self.cur_head = HEAD_IP as u8;
        self.read_label.clear();
        self.next_label.clear();
        self.running = true;
        self.owner = None;
        self.skip_execution = false;
    }
}

// ---------------------------------------------------------------------------
// Heads over several memory spaces
// ---------------------------------------------------------------------------

/// `NormalizeMemSpace(mem_space)` for `n` memory spaces (`mem_space` is
/// asserted non-negative in the C++).
pub(crate) fn ms_index(n: usize, mem_space: i32) -> usize {
    let n = n.max(1) as i32;
    let ms = if mem_space >= n {
        mem_space % n
    } else {
        mem_space
    };
    ms.max(0) as usize
}

/// The memory a head reads and writes (its cached `m_memory`).
pub(crate) fn head_memory<'a>(mems: &'a [CpuMemory], h: &HeadCpu) -> &'a CpuMemory {
    &mems[ms_index(mems.len(), h.mem_space())]
}

/// `Adjust()`: the size is that of the memory space `fullAdjust` would
/// select.
pub(crate) fn head_adjust(mems: &[CpuMemory], h: &mut HeadCpu) {
    let size = head_memory(mems, h).size();
    h.adjust(size, mems.len() as i32);
}

/// `Advance()` / `operator++`.
pub(crate) fn head_advance(mems: &[CpuMemory], h: &mut HeadCpu) {
    let size = head_memory(mems, h).size();
    h.advance(size, mems.len() as i32);
}

/// `Set(pos, ms)`.
pub(crate) fn head_set(mems: &[CpuMemory], h: &mut HeadCpu, pos: i32, ms: i32) {
    let size = mems[ms_index(mems.len(), ms)].size();
    h.set(pos, ms, size, mems.len() as i32);
}

/// `Reset(hw, ms)`.
pub(crate) fn head_reset(mems: &[CpuMemory], h: &mut HeadCpu, ms: i32) {
    let size = mems[ms_index(mems.len(), ms)].size();
    h.reset(ms, size, mems.len() as i32);
}

/// `cHardwareTransSMT`.
pub struct HardwareTransSmt {
    pub(crate) base: HardwareBase,
    pub(crate) functions: &'static [SmtInstFn],
    /// `m_global_stacks`
    pub(crate) global_stacks: [CpuStack; NUM_GLOBAL_STACKS as usize],
    /// `m_mem_array`
    pub(crate) mem_array: Vec<CpuMemory>,
    /// `m_mem_lbls`: label (`AsInt(NUM_NOPS)`) -> memory space.
    pub(crate) mem_lbls: BTreeMap<i32, i32>,
    /// `m_threads`
    pub(crate) threads: Vec<SmtThread>,
    /// `m_thread_lbls`: label -> thread.
    pub(crate) thread_lbls: BTreeMap<i32, i32>,
    /// `m_cur_thread`
    pub(crate) cur_thread: i32,
    /// `m_cur_child`
    pub(crate) cur_child: i32,
}

impl std::fmt::Debug for HardwareTransSmt {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let mems: Vec<String> = self.mem_array.iter().map(|m| m.as_string()).collect();
        f.debug_struct("HardwareTransSmt")
            .field("mem_array", &mems)
            .field("threads", &self.threads)
            .field("cur_thread", &self.cur_thread)
            .finish()
    }
}

impl HardwareTransSmt {
    /// `cHardwareTransSMT(ctx, world, organism, inst_set)`: memory 0 is the
    /// genome plus one default instruction, then `Reset(ctx)` (whose
    /// `HardwareReset` runs without an interface).
    pub fn new(cfg: &Config, org: &mut OrgState, inst_set: Arc<InstSet>) -> Self {
        let mut mem0 = CpuMemory::from_sequence(org.initial_genome.representation());
        let size = mem0.size();
        mem0.resize(size + 1);
        mem0[size] = Instruction::new(0);
        let mut hw = HardwareTransSmt {
            base: HardwareBase::new(cfg, inst_set),
            functions: smt_functions(),
            global_stacks: [CpuStack::default(); NUM_GLOBAL_STACKS as usize],
            mem_array: vec![mem0],
            mem_lbls: BTreeMap::new(),
            threads: Vec::new(),
            thread_lbls: BTreeMap::new(),
            cur_thread: 0,
            cur_child: 0,
        };
        org.hardware_reset_state(cfg);
        hw.base.reset_costs(cfg);
        hw.internal_reset(org);
        hw
    }

    // ------------------------------------------------------------------
    // Accessors
    // ------------------------------------------------------------------

    /// `m_cur_thread` as an index (clamped; an index past the end only
    /// occurs where the port returns `Err`).
    #[inline]
    pub(crate) fn ct(&self) -> usize {
        let c = self.cur_thread.max(0) as usize;
        if c < self.threads.len() {
            c
        } else {
            0
        }
    }

    /// The threads (for inspection).
    pub fn threads(&self) -> &[SmtThread] {
        &self.threads
    }

    /// The memory spaces (for inspection).
    pub fn mem_array(&self) -> &[CpuMemory] {
        &self.mem_array
    }

    /// `Stack(stack_id)` of the current thread.
    pub(crate) fn stack(&mut self, stack_id: i32) -> &mut CpuStack {
        let id = if !(0..NUM_STACKS).contains(&stack_id) {
            0
        } else {
            stack_id
        };
        if id < NUM_LOCAL_STACKS {
            let ct = self.ct();
            &mut self.threads[ct].local_stacks[id as usize]
        } else {
            &mut self.global_stacks[(id % NUM_LOCAL_STACKS) as usize]
        }
    }

    /// `Stack(stack_id, in_thread)` (const).
    fn stack_in(&self, stack_id: i32, in_thread: i32) -> &CpuStack {
        let id = if !(0..NUM_STACKS).contains(&stack_id) {
            0
        } else {
            stack_id
        };
        let t = if in_thread < 0 || in_thread >= self.threads.len() as i32 {
            self.ct()
        } else {
            in_thread as usize
        };
        if id < NUM_LOCAL_STACKS {
            &self.threads[t].local_stacks[id as usize]
        } else {
            &self.global_stacks[(id % NUM_LOCAL_STACKS) as usize]
        }
    }

    /// `GetHead(head_id)` of the current thread, copied.
    #[inline]
    pub(crate) fn get_head(&self, head_id: usize) -> HeadCpu {
        self.threads[self.ct()].heads[head_id]
    }

    /// `IP()` of the current thread, copied.
    #[inline]
    pub(crate) fn ip(&self) -> HeadCpu {
        self.get_head(HEAD_IP)
    }

    /// `IP().GetInst()`.
    pub(crate) fn ip_inst(&self) -> Instruction {
        let ip = self.ip();
        ip.get_inst(head_memory(&self.mem_array, &ip))
    }

    /// `m_inst_set->IsNop(IP().GetNextInst())`.
    pub(crate) fn ip_next_is_nop(&self) -> bool {
        let ip = self.ip();
        let is = &self.base.inst_set;
        is.is_nop(ip.get_next_inst(head_memory(&self.mem_array, &ip), is))
    }

    /// `IP().Advance()`.
    pub(crate) fn ip_advance(&mut self) {
        self.head_advance(HEAD_IP);
    }

    /// `GetHead(head_id).Advance()`.
    pub(crate) fn head_advance(&mut self, head_id: usize) {
        let ct = self.ct();
        head_advance(&self.mem_array, &mut self.threads[ct].heads[head_id]);
    }

    /// `GetHead(head_id).Adjust()`.
    pub(crate) fn head_adjust(&mut self, head_id: usize) {
        let ct = self.ct();
        head_adjust(&self.mem_array, &mut self.threads[ct].heads[head_id]);
    }

    /// `GetHead(head_id).Set(pos, ms)`.
    pub(crate) fn head_set(&mut self, head_id: usize, pos: i32, ms: i32) {
        let ct = self.ct();
        head_set(
            &self.mem_array,
            &mut self.threads[ct].heads[head_id],
            pos,
            ms,
        );
    }

    /// `GetHead(head_id).Reset(this, ms)` for every head of the current
    /// thread.
    pub(crate) fn reset_current_heads(&mut self, ms_of: impl Fn(&Self) -> i32) {
        let ct = self.ct();
        for x in 0..NUM_HEADS {
            let ms = ms_of(self);
            head_reset(&self.mem_array, &mut self.threads[ct].heads[x], ms);
        }
    }

    /// `IP().SetFlagExecuted()`.
    pub(crate) fn ip_set_flag_executed(&mut self) {
        let ip = self.ip();
        let i = ms_index(self.mem_array.len(), ip.mem_space());
        ip.set_flag_executed(&mut self.mem_array[i]);
    }

    /// `AdvanceIP()` of the current thread.
    pub(crate) fn set_advance_ip(&mut self, v: bool) {
        let ct = self.ct();
        self.threads[ct].advance_ip = v;
    }

    /// `GetLabel()` (the current thread's `next_label`).
    pub(crate) fn label(&self) -> &CodeLabel {
        &self.threads[self.ct()].next_label
    }

    /// `GetLabel()`, mutable.
    pub(crate) fn label_mut(&mut self) -> &mut CodeLabel {
        let ct = self.ct();
        &mut self.threads[ct].next_label
    }

    /// Whether `ThreadGetOwner()->UnitSource().transmission_type` is
    /// `HORIZONTAL` (the owner, or the organism itself for host threads).
    pub(crate) fn owner_is_horizontal(&self, org: &OrgState) -> bool {
        match &self.threads[self.ct()].owner {
            Some(u) => u.source.transmission_type == transmission::HORIZONTAL,
            None => org.src.transmission_type == transmission::HORIZONTAL,
        }
    }

    /// `ThreadGetOwner()` of thread `thread`: `None` for the host itself.
    pub fn thread_owner(&self, thread: usize) -> Option<&Arc<ParasiteUnit>> {
        self.threads.get(thread).and_then(|t| t.owner.as_ref())
    }

    // ------------------------------------------------------------------
    // Reset
    // ------------------------------------------------------------------

    /// `internalReset()`: one memory space, one thread (reset into memory
    /// 0), all stacks cleared, then `m_organism->ClearParasites()`. The
    /// threads let go of their owners first (the dropped threads in order,
    /// then thread 0), so a parasite's last reference is normally the one
    /// in the parasite list.
    pub(crate) fn internal_reset(&mut self, org: &mut OrgState) {
        self.mem_array.truncate(1);
        self.mem_lbls.clear();
        // `m_threads.Resize(1)`: the managed array deletes the extra
        // threads first to last, as `truncate` drops them.
        self.threads.truncate(1);
        if self.threads.is_empty() {
            self.threads.push(SmtThread::new_default());
        }
        self.thread_lbls.clear();
        let mems = &self.mem_array;
        self.threads[0].reset(mems, 0);
        self.cur_thread = 0;
        for i in 0..NUM_STACKS {
            self.stack(i).clear();
        }
        org.clear_parasites();
    }

    /// `cHardwareBase::Reset(ctx)` with the interface.
    pub(crate) fn full_reset(&mut self, org: &mut OrgState, world: &mut dyn OrgInterface) {
        crate::organism::hardware_reset(org, self, world);
        self.base.reset_costs(world.config());
        self.internal_reset(org);
    }

    // ------------------------------------------------------------------
    // Labels
    // ------------------------------------------------------------------

    /// `ReadLabel(max_size)`: read the nops following the IP into the
    /// current label, leaving the IP on the last one.
    pub(crate) fn read_label(&mut self, max_size: i32, max_label_exe_size: i32) {
        let ct = self.ct();
        self.threads[ct].next_label.clear();
        let mut count = 0;
        while self.ip_next_is_nop() && count < max_size {
            count += 1;
            self.ip_advance();
            let m = self.base.inst_set.nop_mod(self.ip_inst());
            self.threads[ct].next_label.add_nop(m);
            if self.threads[ct].next_label.get_size() <= max_label_exe_size {
                self.ip_set_flag_executed();
            }
        }
    }

    /// `ReadInst(in_inst)`: a nop opcode extends the read label (with the
    /// opcode itself, as in the C++), anything else clears it.
    pub(crate) fn read_inst(&mut self, in_inst: i32) {
        let ct = self.ct();
        if self.base.inst_set.is_nop(Instruction::new(in_inst)) {
            self.threads[ct].read_label.add_nop(in_inst);
        } else {
            self.threads[ct].read_label.clear();
        }
    }

    /// `FindLabel(0)`: the IP's memory searched from the start for the
    /// current label; the head on the last nop of the match, or a copy of
    /// the IP when the label is empty or not found. (`FindLabel` with a
    /// non-zero direction and the label-argument overload are not reachable
    /// from any instruction.)
    pub(crate) fn find_label_from_start(&self) -> HeadCpu {
        let ip = self.ip();
        let mut search_head = ip;
        let label = self.label();
        if label.get_size() == 0 {
            return ip;
        }
        let found_pos = self.find_label_forward(label, head_memory(&self.mem_array, &ip), 0);
        if found_pos > 0 {
            head_set(
                &self.mem_array,
                &mut search_head,
                found_pos - 1,
                ip.mem_space(),
            );
        }
        search_head
    }

    /// `FindLabel_Forward(search_label, search_genome, pos)`.
    fn find_label_forward(
        &self,
        search_label: &CodeLabel,
        genome: &CpuMemory,
        mut pos: i32,
    ) -> i32 {
        let is = &self.base.inst_set;
        let size = genome.size();
        let is_nop = |p: i32| is.is_nop(genome.get(p));
        let search_start = pos;
        let label_size = search_label.get_size();
        let mut found_label = false;
        pos += label_size;
        while pos < size {
            if is_nop(pos) {
                let mut start_pos = pos;
                let mut end_pos = pos + 1;
                while start_pos > search_start && is_nop(start_pos - 1) {
                    start_pos -= 1;
                }
                while end_pos < size && is_nop(end_pos) {
                    end_pos += 1;
                }
                let test_size = end_pos - start_pos;
                let max_offset = test_size - label_size + 1;
                let mut offset = start_pos;
                while offset < start_pos + max_offset {
                    let mut matches = 0;
                    while matches < label_size {
                        if search_label.get(matches) != is.nop_mod(genome.get(offset + matches)) {
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

    // ------------------------------------------------------------------
    // Memory spaces and threads
    // ------------------------------------------------------------------

    /// `FindMemorySpaceLabel(label, -1)`: memory 0 for an empty label, else
    /// the space registered for the label, created (size 1) if new.
    pub(crate) fn find_memory_space_label(&mut self, label: &CodeLabel) -> i32 {
        if label.get_size() == 0 {
            return 0;
        }
        let hash_key = label.as_int(NUM_NOPS);
        if let Some(ms) = self.mem_lbls.get(&hash_key) {
            return *ms;
        }
        let mem_space = self.mem_array.len() as i32;
        self.mem_array.push(CpuMemory::default());
        self.mem_lbls.insert(hash_key, mem_space);
        mem_space
    }

    /// `MemorySpaceExists(label)`: true for an empty label.
    pub(crate) fn memory_space_exists(&self, label: &CodeLabel) -> bool {
        label.get_size() == 0 || self.mem_lbls.contains_key(&label.as_int(NUM_NOPS))
    }

    /// `ThreadCreate(label, mem_space)`: the new (or restarted) thread id,
    /// or -1.
    pub(crate) fn thread_create(
        &mut self,
        label: &CodeLabel,
        mem_space: i32,
        max_cpu_threads: i32,
    ) -> i32 {
        let thread_id = self.threads.len() as i32;
        if thread_id == max_cpu_threads || label.get_size() == 0 {
            return -1;
        }
        let hash_key = label.as_int(NUM_NOPS);
        if let Some(&id) = self.thread_lbls.get(&hash_key) {
            let t = id as usize;
            if self.threads[t].running {
                return -1;
            }
            let mems = &self.mem_array;
            self.threads[t].reset(mems, mem_space);
            return id;
        }
        self.threads.push(SmtThread::new_default());
        self.thread_lbls.insert(hash_key, thread_id);
        let t = thread_id as usize;
        let mems = &self.mem_array;
        self.threads[t].reset(mems, mem_space);
        let owner = self.threads[self.ct()].owner.clone();
        self.threads[t].owner = owner;
        thread_id
    }

    /// `ThreadKill(thread_id)`: threads are only marked as not running.
    pub(crate) fn thread_kill(&mut self, thread_id: i32) -> bool {
        if thread_id < 1 || thread_id >= self.threads.len() as i32 {
            return false;
        }
        if self.cur_thread == thread_id {
            self.thread_next();
        }
        self.threads[thread_id as usize].running = false;
        true
    }

    /// `FindThreadLabel(label)`: 0 for an empty label, -1 if unknown.
    pub(crate) fn find_thread_label(&self, label: &CodeLabel) -> i32 {
        if label.get_size() == 0 {
            return 0;
        }
        self.thread_lbls
            .get(&label.as_int(NUM_NOPS))
            .copied()
            .unwrap_or(-1)
    }

    /// `ThreadNext()`.
    pub(crate) fn thread_next(&mut self) {
        self.cur_thread += 1;
        if self.cur_thread >= self.threads.len() as i32 {
            self.cur_thread = 0;
        }
    }

    /// `ParasiteInfectHost(bu)`: give the parasite its own memory space
    /// and thread, both named by the label in its source arguments. Fails
    /// if the label is empty or its memory space is in use, if the thread
    /// exists and runs, or at the thread cap. `cOrganism::ParasiteInfectHost`
    /// then records the parasite.
    pub fn parasite_infect_host(&mut self, cfg: &Config, unit: Arc<ParasiteUnit>) -> bool {
        let mut label = CodeLabel::new();
        label.read_string(&unit.source.arguments);
        if label.get_size() == 0 || self.memory_space_exists(&label) {
            return false;
        }
        let mut thread_id = self.threads.len() as i32;
        let hash_key = label.as_int(NUM_NOPS);
        if let Some(&id) = self.thread_lbls.get(&hash_key) {
            thread_id = id;
            if self.threads[id as usize].running {
                return false;
            }
        } else {
            if thread_id == cfg.max_cpu_threads {
                return false;
            }
            self.threads.push(SmtThread::new_default());
            self.thread_lbls.insert(hash_key, thread_id);
        }
        let mem_space = self.find_memory_space_label(&label);
        self.mem_array[mem_space as usize].assign(&unit.genome);
        let t = thread_id as usize;
        let mems = &self.mem_array;
        self.threads[t].reset(mems, mem_space);
        self.threads[t].owner = Some(unit);
        if cfg.inject_is_virulent != 0 {
            self.threads[0].skip_execution = true;
        }
        true
    }

    // ------------------------------------------------------------------
    // Modifier helpers
    // ------------------------------------------------------------------

    /// The nop after the IP, if any: advance onto it, mark it executed and
    /// return its modifier.
    fn take_nop_modifier(&mut self) -> Option<i32> {
        if !self.ip_next_is_nop() {
            return None;
        }
        self.ip_advance();
        let m = self.base.inst_set.nop_mod(self.ip_inst());
        self.ip_set_flag_executed();
        Some(m)
    }

    /// `FindModifiedStack(default_stack)`.
    pub(crate) fn find_modified_stack(&mut self, default_stack: i32) -> i32 {
        self.take_nop_modifier().unwrap_or(default_stack)
    }

    /// `FindModifiedNextStack(default_stack)`.
    pub(crate) fn find_modified_next_stack(&mut self, default_stack: i32) -> i32 {
        self.take_nop_modifier()
            .unwrap_or((default_stack + 1) % NUM_STACKS)
    }

    /// `FindModifiedPreviousStack(default_stack)`.
    pub(crate) fn find_modified_previous_stack(&mut self, default_stack: i32) -> i32 {
        self.take_nop_modifier()
            .unwrap_or((default_stack + NUM_STACKS - 1) % NUM_STACKS)
    }

    /// `FindModifiedComplementStack(default_stack)`.
    ///
    /// QUIRK: without a nop the C++ falls back to `FindPreviousStack`, not
    /// `FindComplementStack`.
    pub(crate) fn find_modified_complement_stack(&mut self, default_stack: i32) -> i32 {
        self.take_nop_modifier()
            .unwrap_or((default_stack + NUM_STACKS - 1) % NUM_STACKS)
    }

    /// `FindModifiedHead(default_head)`.
    pub(crate) fn find_modified_head(&mut self, default_head: usize) -> usize {
        if self.ip_next_is_nop() {
            self.ip_advance();
            let nop_head = self.base.inst_set.nop_mod(self.ip_inst());
            let head = if (0..NUM_HEADS as i32).contains(&nop_head) {
                nop_head as usize
            } else {
                default_head
            };
            self.ip_set_flag_executed();
            return head;
        }
        default_head
    }

    // ------------------------------------------------------------------
    // Organism calls
    // ------------------------------------------------------------------

    /// `cOrganism::DoOutput` with the current thread's context phenotype,
    /// then the bonus instructions it triggers.
    pub(crate) fn org_do_output(
        &mut self,
        org: &mut OrgState,
        world: &mut dyn OrgInterface,
        value: Option<i32>,
        on_divide: bool,
        is_parasite: bool,
    ) -> Result<(), String> {
        let is = self.base.inst_set.clone();
        let ct = self.ct();
        let mut cp = std::mem::take(&mut self.threads[ct].context_phenotype);
        let effects = do_output(
            org,
            &self.base.ext_mem,
            &is,
            world,
            value,
            on_divide,
            is_parasite,
            Some(&mut cp),
        );
        self.threads[ct].context_phenotype = cp;
        for inst in effects?.bonus_insts {
            self.process_bonus_inst(org, world, inst)?;
        }
        Ok(())
    }

    /// `cOrganism::ActivateDivide(ctx, &context_phenotype)`: the last
    /// output test, then `m_interface->Divide`. Returns whether the parent
    /// is still alive.
    fn activate_divide(
        &mut self,
        org: &mut OrgState,
        world: &mut dyn OrgInterface,
    ) -> Result<bool, String> {
        self.org_do_output(org, world, None, true, false)?;
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

    // ------------------------------------------------------------------
    // Divide and inject
    // ------------------------------------------------------------------

    /// `calcCopiedSize`: copied sites of the child memory space.
    fn calc_copied_size(&self) -> i32 {
        let Some(memory) = self.mem_array.get(self.cur_child.max(0) as usize) else {
            return 0;
        };
        (0..memory.size())
            .filter(|i| memory.flag_copied(*i))
            .count() as i32
    }

    /// `Divide_Main(ctx, mut_multiplier)`: the write head's memory space
    /// (not memory 0) up to the write head becomes the offspring.
    pub(crate) fn divide_main(
        &mut self,
        org: &mut OrgState,
        world: &mut dyn OrgInterface,
        mut_multiplier: f64,
    ) -> Result<bool, String> {
        let wh = self.get_head(HEAD_WRITE);
        let mem_space_used = wh.mem_space();
        let write_head_pos = wh.position();
        if self.mem_array.len() as i32 <= mem_space_used || mem_space_used == 0 {
            return Ok(false);
        }
        self.cur_child = mem_space_used;
        let copied = self.calc_copied_size();
        let parent_size = self.mem_array[0].size() - 1;
        match divide_check_viable(
            org,
            world,
            &self.mem_array[0],
            parent_size,
            write_head_pos,
            Some(copied),
        ) {
            Viability::Viable => {}
            Viability::NotViable => return Ok(false),
            Viability::NotViableReset => {
                // internalResetOnFailedDivide()
                self.internal_reset(org);
                self.set_advance_ip(false);
                return Ok(false);
            }
        }

        let child = mem_space_used as usize;
        self.mem_array[child].resize(write_head_pos);
        let is = self.base.inst_set.clone();
        org.offspring_genome = Genome::with_inst_set(
            HARDWARE_TYPE_CPU_TRANSSMT,
            is.inst_set_name(),
            self.mem_array[child].to_sequence(),
        );
        divide_do_mutations(
            &is,
            org,
            world,
            &mut self.mem_array[0],
            mut_multiplier,
            i32::MAX,
        )?;
        divide_test_fitness_measures(org, world, false);
        self.base.reset_ft_costs();

        let parent_alive = self.activate_divide(org, world)?;
        self.mem_array[child].assign(&InstructionSequence::from_string("a"));

        if parent_alive {
            match world.config().divide_method {
                DIVIDE_METHOD_SPLIT => self.full_reset(org, world),
                DIVIDE_METHOD_BIRTH => {
                    self.reset_current_heads(|_| 0);
                    for x in 0..NUM_LOCAL_STACKS {
                        self.stack(x).clear();
                    }
                    if world.config().inherit_merit == 0 {
                        org.phenotype.reset_merit(world.config());
                    }
                }
                _ => {}
            }
            self.set_advance_ip(false);
            self.mem_array[0].clear_flags();
        }
        Ok(true)
    }

    /// `InjectParasite(ctx, mut_multiplier)`: send the write head's memory
    /// space up to the write head to the organism interface.
    pub(crate) fn inject_parasite(
        &mut self,
        org: &mut OrgState,
        world: &mut dyn OrgInterface,
        mut_multiplier: f64,
    ) -> Result<bool, String> {
        let wh = self.get_head(HEAD_WRITE);
        let end_pos = wh.position();
        let mem_space_used = ms_index(self.mem_array.len(), wh.mem_space());
        if end_pos <= 0 {
            org.fault(
                FAULT_LOC_INJECT,
                FAULT_TYPE_ERROR,
                "inject: no code to inject",
            );
            return Ok(false);
        }
        if end_pos < MIN_INJECT_SIZE {
            org.fault(
                FAULT_LOC_INJECT,
                FAULT_TYPE_ERROR,
                "inject: new size too small",
            );
            return Ok(false);
        }

        org.phenotype.update_parasite_tasks();
        if world.config().inject_resets_tasks != 0 {
            org.phenotype.last_task_count = org.phenotype.cur_task_count.clone();
        }

        // QUIRK: the write head may be in memory 0, which is then cut down
        // to the injected code (and replaced by "a" below).
        self.mem_array[mem_space_used].resize(end_pos);
        let mut injected_code = self.mem_array[mem_space_used].clone();
        self.inject_do_mutations(org, world, mut_multiplier, &mut injected_code);

        let mut inject_signal = false;
        if injected_code.size() > 0 {
            // `ThreadGetOwner()`: `None` stands for the organism itself.
            let parent = self.threads[self.ct()].owner.clone();
            let label = self.label().as_string();
            inject_signal =
                world.inject_parasite(org, self, parent, &label, injected_code.as_sequence());
        }

        self.mem_array[mem_space_used].assign(&InstructionSequence::from_string("a"));

        if world.config().inject_method == INJECT_METHOD_SPLIT {
            self.reset_current_heads(|hw| hw.ip().mem_space());
            for x in 0..NUM_LOCAL_STACKS {
                self.stack(x).clear();
            }
        }
        self.set_advance_ip(false);
        Ok(inject_signal)
    }

    /// `Inject_DoMutations(ctx, mut_multiplier, injected_code)`: inject
    /// substitutions, insertions and deletions on the code, then the
    /// parent mutations on memory 0 (with the raw `MAX_GENOME_SIZE` /
    /// `MIN_GENOME_SIZE`, unlike `Divide_DoMutations`).
    fn inject_do_mutations(
        &mut self,
        org: &mut OrgState,
        world: &mut dyn OrgInterface,
        mut_multiplier: f64,
        code: &mut CpuMemory,
    ) {
        let is = self.base.inst_set.clone();
        org.phenotype.div_type = mut_multiplier;
        let mr = org.mut_rates;

        let num_mut = binomial(world, code.size(), mr.inject.mut_prob / mut_multiplier);
        for _ in 0..num_mut {
            let site = uint_index(world, code.size());
            let inst = is.random_inst(world.rng());
            code[site] = inst;
        }

        let mut num_mut = binomial(world, code.size(), mr.inject.ins_prob);
        if num_mut + code.size() > MAX_GENOME_LENGTH {
            num_mut = MAX_GENOME_LENGTH - code.size();
        }
        if num_mut > 0 {
            insert_sorted(&is, world, code, num_mut);
        }

        let mut num_mut = binomial(world, code.size(), mr.inject.del_prob);
        if code.size() - num_mut < MIN_GENOME_LENGTH {
            num_mut = code.size() - MIN_GENOME_LENGTH;
        }
        for _ in 0..num_mut {
            let site = uint_index(world, code.size());
            code.remove(site, 1);
        }

        let max_genome_size = world.config().max_genome_size;
        let min_genome_size = world.config().min_genome_size;
        let memory = &mut self.mem_array[0];

        if mr.divide.parent_mut_prob > 0.0 {
            let num_mut = binomial(world, memory.size(), mr.divide.parent_mut_prob);
            for _ in 0..num_mut {
                let site = uint_index(world, memory.size());
                memory[site] = is.random_inst(world.rng());
            }
        }
        if mr.divide.parent_ins_prob > 0.0 {
            let mut num_mut = binomial(world, memory.size(), mr.divide.parent_ins_prob);
            if num_mut + memory.size() > max_genome_size {
                num_mut = max_genome_size - memory.size();
            }
            if num_mut > 0 {
                insert_sorted(&is, world, memory, num_mut);
            }
        }
        if mr.divide.parent_del_prob > 0.0 {
            let mut num_mut = binomial(world, memory.size(), mr.divide.parent_del_prob);
            if memory.size() - num_mut < min_genome_size {
                num_mut = memory.size() - min_genome_size;
            }
            for _ in 0..num_mut {
                let site = uint_index(world, memory.size());
                memory.remove(site, 1);
            }
        }
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
        if op < 0 || op >= self.base.inst_set.size() {
            return Err(format!(
                "opcode {} outside instruction set '{}' (C++: cHardwareTransSMT.cc:SingleProcess_ExecuteInst reads past m_functions)",
                op,
                self.base.inst_set.inst_set_name()
            ));
        }
        let inst_idx = self.base.inst_set.lib_function_index(cur_inst);
        self.ip_set_flag_executed();
        if let Some(c) = org.phenotype.cur_inst_count.get_mut(op as usize) {
            *c = c.wrapping_add(1);
        }
        let f = *self
            .functions
            .get(inst_idx.max(0) as usize)
            .ok_or_else(|| format!("instruction library index {} out of range", inst_idx))?;
        let exec_success = f(self, org, world)?;
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
        let me: &HardwareTransSmt = self;
        guard.trace_hardware(
            &mut || print_status(org, me, world),
            &mut String::new,
            &mut |_| String::new(),
            bonus,
            false,
            -2,
        );
    }
}

/// `GetRandBinomial(n, p)` as the C++ `int` it is assigned to.
fn binomial(world: &mut dyn OrgInterface, n: i32, p: f64) -> i32 {
    world.rng().get_rand_binomial(f64::from(n), p) as i32
}

/// `GetUInt(n)` as an index (`n <= 0` is undefined in C++: 0 is used).
fn uint_index(world: &mut dyn OrgInterface, n: i32) -> i32 {
    if n <= 0 {
        return 0;
    }
    world.rng().get_uint(n as u32) as i32
}

/// The insertion step of `Inject_DoMutations`: `num_mut` sites drawn with
/// `GetUInt(size + 1)`, `Apto::QSort`ed, then a random instruction inserted
/// at each in reverse order.
fn insert_sorted(is: &InstSet, world: &mut dyn OrgInterface, code: &mut CpuMemory, num_mut: i32) {
    let mut sites: Vec<i32> = (0..num_mut)
        .map(|_| uint_index(world, code.size() + 1))
        .collect();
    sites.sort_unstable();
    for site in sites.into_iter().rev() {
        let inst = is.random_inst(world.rng());
        code.insert(site, inst);
    }
}

impl Hardware for HardwareTransSmt {
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
        HARDWARE_TYPE_CPU_TRANSSMT
    }
    fn supports_speculative(&self) -> bool {
        false
    }

    fn reset(&mut self, org: &mut OrgState, world: &mut dyn OrgInterface) {
        self.full_reset(org, world);
    }

    /// `SingleProcess(ctx, speculative)`: one instruction (one per thread
    /// with `THREAD_SLICING_METHOD` 1). With a parasite virulence other than
    /// -1 and `THREAD_SLICING_METHOD` 0, each step draws which of threads 0
    /// and 1 runs. Time used advances by one per call; CPU cycles do not.
    fn single_process(
        &mut self,
        org: &mut OrgState,
        world: &mut dyn OrgInterface,
        speculative: bool,
    ) -> Result<bool, String> {
        if speculative {
            return Ok(false);
        }
        org.set_running(true);
        org.phenotype.inc_time_used(1);
        // Debugger count only (no CPU cycles here, as in the C++).
        org.phenotype.lifetime_cpu_cycles += 1;

        let num_inst_exec = if world.config().thread_slicing_method == 1 {
            self.threads.len()
        } else {
            1
        };

        for _ in 0..num_inst_exec {
            self.cur_thread += 1;
            let parasite_virulence = if self.threads.len() > 1 {
                let owner = self.threads[1].owner.as_ref().ok_or_else(|| {
                    "C++ segfault: cHardwareTransSMT::SingleProcess dereferences the NULL owner of thread 1 (a Thread-Create thread of an uninfected organism)".to_string()
                })?;
                if owner.source.transmission_type == transmission::HORIZONTAL {
                    if world.config().virulence_source == 2 {
                        org.para_donate
                    } else {
                        owner.virulence
                    }
                } else {
                    world.config().parasite_virulence
                }
            } else {
                world.config().parasite_virulence
            };

            if parasite_virulence != -1.0 && world.config().thread_slicing_method == 0 {
                let prob_thread = world.rng().get_double();
                self.cur_thread = 0;
                if prob_thread < parasite_virulence {
                    self.cur_thread = 1;
                }
            }

            if self.cur_thread >= self.threads.len() as i32 {
                self.cur_thread = 0;
            }
            if self.threads[self.cur_thread as usize].skip_execution {
                self.cur_thread += 1;
                if self.cur_thread >= self.threads.len() as i32 {
                    return Err("C++ undefined behaviour: cHardwareTransSMT::SingleProcess skips thread 0 with no other thread (m_threads read past the end)".to_string());
                }
            }

            let ct = self.cur_thread as usize;
            if !self.threads[ct].running {
                continue;
            }

            self.threads[ct].advance_ip = true;
            head_adjust(&self.mem_array, &mut self.threads[ct].heads[HEAD_IP]);

            if self.base.tracer.is_some() {
                self.trace(org, world, false);
            }

            let cur_inst = self.ip_inst();
            let mut exec = self.base.pay_pre_costs(org, world, cur_inst, ct);
            if exec {
                let prob_fail = self.base.inst_set.prob_fail(cur_inst);
                if prob_fail > 0.0 {
                    exec = !world.rng().p(prob_fail);
                }
                if exec && self.execute_inst(org, world, cur_inst)? {
                    self.base.pay_post_res_costs(org, world, cur_inst);
                    let t = self.cur_thread.max(0) as usize;
                    self.base.set_post_cpu_costs(cur_inst, t);
                }
                let ct = self.ct();
                if self.threads[ct].advance_ip {
                    head_advance(&self.mem_array, &mut self.threads[ct].heads[HEAD_IP]);
                }
            }
        }

        let max_executed = org.max_executed;
        if (max_executed > 0 && org.phenotype.time_used >= max_executed) || org.phenotype.to_die {
            org.die(world);
        }
        org.set_running(false);

        if self.base.implicit_repro_active && implicit_repro_due(org, world.config(), false) {
            return Err("This hardware type does not have a =repro= instruction. IMPLICIT_REPRO conditions cannot be used! (C++: cHardwareBase::Inst_Repro exits with status 1)".to_string());
        }
        Ok(true)
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
    fn print_status(&self, org: &OrgState, _world: &mut dyn OrgInterface) -> String {
        use std::fmt::Write;
        let mut s = String::new();
        let ip = self.ip();
        let ip_inst = ip.get_inst(head_memory(&self.mem_array, &ip));
        let name = if ip_inst.op() >= 0 && ip_inst.op() < self.base.inst_set.size() {
            self.base.inst_set.name_of(ip_inst).to_string()
        } else {
            String::new()
        };
        let _ = writeln!(
            s,
            "{} THREAD: {}, {}   IP:({}, {}) ({})",
            org.phenotype.time_used,
            self.cur_thread,
            self.threads.len(),
            ip.mem_space(),
            ip.position(),
            name
        );
        for (id, letter) in ['A', 'B', 'C', 'D'].into_iter().enumerate() {
            let v = self.stack_in(id as i32, -1).top();
            let _ = write!(s, "{}X:{} [0x{:x}]  ", letter, v, v as u32);
        }
        s.push('\n');
        let r = self.get_head(HEAD_READ);
        let w = self.get_head(HEAD_WRITE);
        let f = self.get_head(HEAD_FLOW);
        let _ = writeln!(
            s,
            "  R-Head:({}, {})  W-Head:({}, {})  F-Head:({},  {})  RL:{}   ",
            r.mem_space(),
            r.position(),
            w.mem_space(),
            w.position(),
            f.mem_space(),
            f.position(),
            self.threads[self.ct()].read_label.as_string()
        );
        for (i, mem) in self.mem_array.iter().enumerate() {
            let _ = writeln!(s, "  Mem {} ({}): {}", i, mem.size(), mem.as_string());
        }
        s
    }

    fn get_stack(&self, depth: i32, stack_id: i32, in_thread: i32) -> i32 {
        // `stack_id > NUM_STACKS` as in the C++; `Stack()` maps NUM_STACKS
        // itself to 0.
        let stack_id = if !(0..=NUM_STACKS).contains(&stack_id) {
            0
        } else {
            stack_id
        };
        let in_thread = if in_thread == -1 {
            self.cur_thread
        } else {
            in_thread
        };
        self.stack_in(stack_id, in_thread).get(depth)
    }

    fn num_stacks(&self) -> i32 {
        NUM_STACKS
    }

    fn head(&self, head_id: usize) -> &HeadCpu {
        &self.threads[self.ct()].heads[head_id.min(NUM_HEADS - 1)]
    }

    fn head_mut(&mut self, head_id: usize) -> &mut HeadCpu {
        let ct = self.ct();
        &mut self.threads[ct].heads[head_id.min(NUM_HEADS - 1)]
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

    /// `GetMemory()`: memory space 0.
    fn memory(&self) -> &CpuMemory {
        &self.mem_array[0]
    }

    fn memory_mut(&mut self) -> &mut CpuMemory {
        &mut self.mem_array[0]
    }

    fn num_mem_spaces(&self) -> i32 {
        self.mem_array.len() as i32
    }

    fn memory_spaces(&self) -> Vec<(i32, &CpuMemory)> {
        self.mem_array
            .iter()
            .enumerate()
            .map(|(i, m)| (i as i32, m))
            .collect()
    }

    /// `GetRegister(reg_id)`: the top of stack `reg_id`.
    fn register(&self, reg_id: usize) -> i32 {
        self.stack_in(reg_id.min(i32::MAX as usize) as i32, -1)
            .peek()
    }

    fn num_registers(&self) -> i32 {
        NUM_STACKS
    }

    fn num_threads(&self) -> i32 {
        self.threads.len() as i32
    }

    fn cur_thread(&self) -> i32 {
        self.cur_thread
    }

    /// `cHardwareBase::GetGenomeFragment`: the read head's memory space.
    fn get_genome_fragment(&self, downstream: u32) -> InstructionSequence {
        let mut tmp = self.get_head(HEAD_READ);
        let mut fragment = InstructionSequence::new();
        for _ in 0..downstream {
            fragment.append(tmp.get_inst(head_memory(&self.mem_array, &tmp)));
            head_advance(&self.mem_array, &mut tmp);
        }
        fragment
    }

    /// `cHardwareBase::InsertGenomeFragment`: into the write head's memory
    /// space.
    fn insert_genome_fragment(&mut self, fragment: &InstructionSequence) {
        let ct = self.ct();
        let wh = self.threads[ct].heads[HEAD_WRITE];
        let i = ms_index(self.mem_array.len(), wh.mem_space());
        let pos = wh.position();
        let mem = &mut self.mem_array[i];
        if pos >= 0 && pos <= mem.size() {
            mem.insert_seq(pos, fragment);
        }
        head_adjust(&self.mem_array, &mut self.threads[ct].heads[HEAD_WRITE]);
    }
}

/// `ParasiteInfectHost` is reached only from the population (not ported),
/// so these checks are against the C++ source, not the reference binary.
#[cfg(test)]
mod tests {
    use super::*;
    use crate::hardware_manager::HardwareManager;
    use crate::org_interface::WorldServices;
    use crate::task_adapter::{Env, TaskLibRegistry};
    use avida_apto::AvidaRng;
    use avida_genome::{Feedback, InstCodeConfig};

    struct World {
        cfg: Config,
        env: Env,
        rng: AvidaRng,
        hwm: HardwareManager,
    }

    impl WorldServices for World {
        fn config(&self) -> &Config {
            &self.cfg
        }
        fn environment(&self) -> &Env {
            &self.env
        }
        fn rng(&mut self) -> &mut AvidaRng {
            &mut self.rng
        }
        fn hardware_manager(&self) -> &HardwareManager {
            &self.hwm
        }
        fn update(&self) -> i32 {
            0
        }
        fn tot_creatures(&self) -> i32 {
            0
        }
        fn sense_size(&self) -> usize {
            0
        }
        fn group_attack_inst_count(&self, _inst_set: &str) -> usize {
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
            1
        }
    }

    /// A host running `host` with all 50 library instructions in library
    /// order (letters a, b, ... as in the default TransSMT set).
    fn host(cfg: Config, host: &str) -> (World, HardwareTransSmt) {
        let (world, _, hw) = host_org(cfg, host);
        (world, hw)
    }

    /// [`host`] with the organism's state.
    fn host_org(cfg: Config, host: &str) -> (World, OrgState, HardwareTransSmt) {
        let mut lines = vec!["INSTSET transsmt:hw_type=2".to_string()];
        lines.extend(
            crate::hardware_transsmt_insts::transsmt_inst_lib()
                .entries()
                .iter()
                .filter(|e| e.name() != "NULL")
                .map(|e| format!("INST {}", e.name())),
        );
        let mut hwm = HardwareManager::new();
        let code_cfg = InstCodeConfig {
            inst_code_length: 3,
            inst_code_default_type: 0,
        };
        assert!(hwm
            .load_inst_sets(&lines, code_cfg, &mut Feedback::new())
            .unwrap());
        let env = Env::new(&cfg, TaskLibRegistry::new());
        let world = World {
            cfg,
            env,
            rng: AvidaRng::new(1),
            hwm,
        };
        let genome = Genome::with_inst_set(
            HARDWARE_TYPE_CPU_TRANSSMT,
            "transsmt",
            InstructionSequence::from_string(host),
        );
        let mut org = OrgState::new(&world, &genome, 0, OrgSource::default(), NUM_NOPS);
        let is = world.hwm.inst_set_for_genome(&genome).unwrap();
        let hw = HardwareTransSmt::new(&world.cfg, &mut org, is);
        (world, org, hw)
    }

    fn unit(code: &str, label: &str) -> Arc<ParasiteUnit> {
        Arc::new(ParasiteUnit {
            genome: InstructionSequence::from_string(code),
            source: OrgSource::new(transmission::HORIZONTAL, label, false),
            virulence: 0.5,
            tag: None,
        })
    }

    #[test]
    fn infection_adds_an_owned_thread_in_a_new_memory_space() {
        let mut cfg = Config::new();
        cfg.max_cpu_threads = 2;
        let (world, mut hw) = host(cfg, "zcdBoaddyab");
        assert_eq!(hw.threads().len(), 1);
        let para = unit("zcdBoaycccFzqrwcsGaddyab", "BC");
        assert!(hw.parasite_infect_host(&world.cfg, para.clone()));
        assert_eq!(hw.threads().len(), 2);
        assert_eq!(hw.mem_array().len(), 2);
        assert_eq!(
            hw.mem_array()[1].as_string(),
            para.genome.as_string(),
            "the parasite is copied into its labelled memory space"
        );
        let t = &hw.threads()[1];
        assert!(t.running);
        assert!(Arc::ptr_eq(hw.thread_owner(1).unwrap(), &para));
        for h in &t.heads {
            assert_eq!((h.mem_space(), h.position()), (1, 0));
        }
        assert!(hw.thread_owner(0).is_none());
        assert!(!hw.threads()[0].skip_execution);
    }

    #[test]
    fn infection_fails_like_cpp() {
        let mut cfg = Config::new();
        cfg.max_cpu_threads = 2;
        let (world, mut hw) = host(cfg, "zcdBoaddyab");
        // Empty label.
        assert!(!hw.parasite_infect_host(&world.cfg, unit("zcd", "")));
        assert!(hw.parasite_infect_host(&world.cfg, unit("zcd", "BC")));
        // Memory space in use.
        assert!(!hw.parasite_infect_host(&world.cfg, unit("zcd", "BC")));
        // Thread cap (MAX_CPU_THREADS 2).
        assert!(!hw.parasite_infect_host(&world.cfg, unit("zcd", "CD")));
        assert_eq!(hw.threads().len(), 2);
        assert_eq!(hw.mem_array().len(), 2);
    }

    #[test]
    fn reset_drops_threads_then_the_parasite_list_last_to_first() {
        let mut cfg = Config::new();
        cfg.max_cpu_threads = 3;
        let (world, mut org, mut hw) = host_org(cfg, "zcdBoaddyab");
        let (p1, p2) = (unit("zcd", "B"), unit("zce", "C"));
        for p in [&p1, &p2] {
            assert!(hw.parasite_infect_host(&world.cfg, Arc::clone(p)));
            org.parasites.push(Arc::clone(p));
        }
        assert_eq!(org.num_parasites(), 2);
        assert_eq!(Arc::strong_count(&p1), 3);
        hw.internal_reset(&mut org);
        assert_eq!(hw.threads().len(), 1);
        assert!(org.parasites.is_empty());
        assert_eq!((Arc::strong_count(&p1), Arc::strong_count(&p2)), (1, 1));
    }

    #[test]
    fn virulent_infection_skips_the_host_thread() {
        let mut cfg = Config::new();
        cfg.max_cpu_threads = 3;
        cfg.inject_is_virulent = 1;
        let (world, mut hw) = host(cfg, "zcdBoaddyab");
        assert!(hw.parasite_infect_host(&world.cfg, unit("zcd", "B")));
        assert!(hw.threads()[0].skip_execution);
    }
}
