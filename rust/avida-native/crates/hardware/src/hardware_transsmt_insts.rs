//! The `cHardwareTransSMT` instruction library (`initInstLib`) and the
//! instruction handlers (`Inst_*`, cHardwareTransSMT.cc:1236-1831 at
//! 47f13dadb).

use crate::code_label::CodeLabel;
use crate::hardware_cpu_insts_res::{at, set_at};
use crate::hardware_transsmt::*;
use crate::nhardware::*;
use crate::org_interface::OrgInterface;
use crate::organism::OrgState;
use avida_genome::inst_lib::{inst_flag, BehavClass, InstructionClass};
use avida_genome::{InstLib, InstLibEntry, Instruction, InstructionSequence};
use std::sync::{Arc, OnceLock};

/// A `cHardwareTransSMT` instruction handler.
pub(crate) type SmtInstFn =
    fn(&mut HardwareTransSmt, &mut OrgState, &mut dyn OrgInterface) -> Result<bool, String>;

type R = Result<bool, String>;
type H = HardwareTransSmt;

/// One `s_f_array` entry.
struct SmtInstDef {
    name: &'static str,
    class: InstructionClass,
    flags: u32,
    f: SmtInstFn,
}

const fn op(name: &'static str, f: SmtInstFn) -> SmtInstDef {
    SmtInstDef {
        name,
        class: InstructionClass::Other,
        flags: 0,
        f,
    }
}

const fn nop(name: &'static str, flags: u32) -> SmtInstDef {
    SmtInstDef {
        name,
        class: InstructionClass::Nop,
        flags,
        f: H::inst_nop,
    }
}

/// `s_f_array`, in library order; `NULL` is last (the null instruction)
/// and `Nop-A` the default.
static SMT_INST_DEFS: [SmtInstDef; 50] = [
    nop("Nop-A", inst_flag::NOP),
    nop("Nop-B", inst_flag::NOP),
    nop("Nop-C", inst_flag::NOP),
    nop("Nop-D", inst_flag::NOP),
    nop("Nop-X", 0),
    op("Val-Shift-R", H::inst_shift_r),
    op("Val-Shift-L", H::inst_shift_l),
    op("Val-Nand", H::inst_val_nand),
    op("Val-Add", H::inst_val_add),
    op("Val-Sub", H::inst_val_sub),
    op("Val-Mult", H::inst_val_mult),
    op("Val-Div", H::inst_val_div),
    op("Val-Mod", H::inst_val_mod),
    op("Val-Inc", H::inst_val_inc),
    op("Val-Dec", H::inst_val_dec),
    op("SetMemory", H::inst_set_memory),
    op("Divide", H::inst_divide),
    op("Inst-Read", H::inst_head_read),
    op("Inst-Write", H::inst_head_write),
    op("If-Equal", H::inst_if_equal),
    op("If-Not-Equal", H::inst_if_not_equal),
    op("If-Less", H::inst_if_less),
    op("If-Greater", H::inst_if_greater),
    op("Head-Push", H::inst_head_push),
    op("Head-Pop", H::inst_head_pop),
    op("Head-Move", H::inst_head_move),
    op("Search", H::inst_search),
    op("Push-Next", H::inst_push_next),
    op("Push-Prev", H::inst_push_previous),
    op("Push-Comp", H::inst_push_complement),
    op("Val-Delete", H::inst_val_delete),
    op("Val-Copy", H::inst_val_copy),
    op("IO", H::inst_io),
    op("Thread-Create", H::inst_thread_create),
    op("Thread-Cancel", H::inst_thread_cancel),
    op("Thread-Kill", H::inst_thread_kill),
    op("Inject", H::inst_inject),
    op("Apoptosis", H::inst_apoptosis),
    op("Rotate-Left", H::inst_rotate_left),
    op("Rotate-Right", H::inst_rotate_right),
    op("Call-Flow", H::inst_call_flow),
    op("Call-Label", H::inst_call_label),
    op("Return", H::inst_return),
    op("If-Greater-Equal", H::inst_if_greater_equal),
    op("Divide-Erase", H::inst_divide_erase),
    op("Divide-Sex-Erase", H::inst_divide_sex_erase),
    op("Divide-Sex", H::inst_divide_sex),
    op("Divide-Asex-Wait", H::inst_divide_asex_wait),
    op("Collect-Unit", H::inst_collect_unit),
    nop("NULL", 0),
];

/// `cHardwareTransSMT::GetInstLib()`.
pub fn transsmt_inst_lib() -> Arc<InstLib> {
    static LIB: OnceLock<Arc<InstLib>> = OnceLock::new();
    LIB.get_or_init(|| {
        let entries = SMT_INST_DEFS
            .iter()
            .map(|d| InstLibEntry::new(d.name, d.class, d.flags, "", BehavClass::None))
            .collect();
        // `s_n_array`: Nop-A..D modify stacks AX..DX.
        let nops = vec![
            ("Nop-A".to_string(), STACK_AX),
            ("Nop-B".to_string(), STACK_BX),
            ("Nop-C".to_string(), STACK_CX),
            ("Nop-D".to_string(), STACK_DX),
        ];
        let null_inst = SMT_INST_DEFS.len() as i32 - 1;
        Arc::new(InstLib::new(entries, nops, 0, null_inst))
    })
    .clone()
}

/// The handler table, indexed by library function index.
pub(crate) fn smt_functions() -> &'static [SmtInstFn] {
    static FUNCS: OnceLock<Vec<SmtInstFn>> = OnceLock::new();
    FUNCS.get_or_init(|| SMT_INST_DEFS.iter().map(|d| d.f).collect())
}

/// `ceil(log(num_resources) / log(num_nops))` of `FindModifiedResource`.
fn max_label_length(num_resources: i32, num_nops: i32) -> i32 {
    (f64::from(num_resources).ln() / f64::from(num_nops).ln()).ceil() as i32
}

impl HardwareTransSmt {
    fn top(&mut self, id: i32) -> i32 {
        self.stack(id).top()
    }

    fn push(&mut self, id: i32, value: i32) {
        self.stack(id).push(value);
    }

    fn pop(&mut self, id: i32) -> i32 {
        self.stack(id).pop()
    }

    /// `ReadLabel(max_size)` with the configured `MAX_LABEL_EXE_SIZE`.
    fn read_label_cfg(&mut self, world: &dyn OrgInterface, max_size: i32) {
        let m = world.config().max_label_exe_size;
        self.read_label(max_size, m);
    }

    /// The four-nop (Nop-B) prefix parasites put on memory-space labels
    /// when `PARASITE_MEM_SPACES` is set.
    fn add_parasite_label_prefix(&mut self, org: &OrgState, world: &dyn OrgInterface) {
        if self.owner_is_horizontal(org) && world.config().parasite_mem_spaces != 0 {
            for _ in 0..4 {
                self.label_mut().add_nop(1);
            }
        }
    }

    /// `Inst_Nop` (also `Nop-X` and `NULL`).
    pub(crate) fn inst_nop(&mut self, _: &mut OrgState, _: &mut dyn OrgInterface) -> R {
        Ok(true)
    }

    // ----------------------------------------------------------------
    // Arithmetic
    // ----------------------------------------------------------------

    /// `Inst_ShiftR`: ?BX? = ?src? (popped) >> 1.
    pub(crate) fn inst_shift_r(&mut self, _: &mut OrgState, _: &mut dyn OrgInterface) -> R {
        let dst = self.find_modified_stack(STACK_BX);
        let src = self.find_modified_stack(dst);
        let value = self.pop(src) >> 1;
        self.push(dst, value);
        Ok(true)
    }

    /// `Inst_ShiftL`: ?BX? = ?src? (popped) << 1.
    pub(crate) fn inst_shift_l(&mut self, _: &mut OrgState, _: &mut dyn OrgInterface) -> R {
        let dst = self.find_modified_stack(STACK_BX);
        let src = self.find_modified_stack(dst);
        let value = self.pop(src).wrapping_shl(1);
        self.push(dst, value);
        Ok(true)
    }

    /// The operands of the binary value instructions: `dst = ?BX?`,
    /// `op1 = ?BX?`, `op2 = ?next(op1)?`.
    fn binary_operands(&mut self) -> (i32, i32, i32) {
        let dst = self.find_modified_stack(STACK_BX);
        let op1 = self.find_modified_stack(STACK_BX);
        let op2 = self.find_modified_next_stack(op1);
        (dst, op1, op2)
    }

    /// `Inst_Val_Nand`
    pub(crate) fn inst_val_nand(&mut self, _: &mut OrgState, _: &mut dyn OrgInterface) -> R {
        let (dst, op1, op2) = self.binary_operands();
        let v = !(self.top(op1) & self.top(op2));
        self.push(dst, v);
        Ok(true)
    }

    /// `Inst_Val_Add`
    pub(crate) fn inst_val_add(&mut self, _: &mut OrgState, _: &mut dyn OrgInterface) -> R {
        let (dst, op1, op2) = self.binary_operands();
        let v = self.top(op1).wrapping_add(self.top(op2));
        self.push(dst, v);
        Ok(true)
    }

    /// `Inst_Val_Sub`
    pub(crate) fn inst_val_sub(&mut self, _: &mut OrgState, _: &mut dyn OrgInterface) -> R {
        let (dst, op1, op2) = self.binary_operands();
        let v = self.top(op1).wrapping_sub(self.top(op2));
        self.push(dst, v);
        Ok(true)
    }

    /// `Inst_Val_Mult`
    pub(crate) fn inst_val_mult(&mut self, _: &mut OrgState, _: &mut dyn OrgInterface) -> R {
        let (dst, op1, op2) = self.binary_operands();
        let v = self.top(op1).wrapping_mul(self.top(op2));
        self.push(dst, v);
        Ok(true)
    }

    /// `Inst_Val_Div`: faults (and fails) on division by zero; `INT_MIN / -1`
    /// faults but succeeds without pushing.
    pub(crate) fn inst_val_div(&mut self, org: &mut OrgState, _: &mut dyn OrgInterface) -> R {
        let (dst, op1, op2) = self.binary_operands();
        let b = self.top(op2);
        if b != 0 {
            let a = self.top(op1);
            if -i32::MAX > a && b == -1 {
                org.fault(FAULT_LOC_MATH, FAULT_TYPE_ERROR, "div: Float exception");
            } else {
                self.push(dst, a / b);
            }
        } else {
            org.fault(FAULT_LOC_MATH, FAULT_TYPE_ERROR, "div: dividing by 0");
            return Ok(false);
        }
        Ok(true)
    }

    /// `Inst_Val_Mod`: `x % -1` pushes 0; modding by zero faults and fails.
    pub(crate) fn inst_val_mod(&mut self, org: &mut OrgState, _: &mut dyn OrgInterface) -> R {
        let (dst, op1, op2) = self.binary_operands();
        let b = self.top(op2);
        if b != 0 {
            if b == -1 {
                self.push(dst, 0);
            } else {
                let a = self.top(op1);
                self.push(dst, a % b);
            }
        } else {
            org.fault(FAULT_LOC_MATH, FAULT_TYPE_ERROR, "mod: modding by 0");
            return Ok(false);
        }
        Ok(true)
    }

    /// `Inst_Val_Inc`
    pub(crate) fn inst_val_inc(&mut self, _: &mut OrgState, _: &mut dyn OrgInterface) -> R {
        let dst = self.find_modified_stack(STACK_BX);
        let src = self.find_modified_stack(dst);
        let value = self.pop(src).wrapping_add(1);
        self.push(dst, value);
        Ok(true)
    }

    /// `Inst_Val_Dec`
    pub(crate) fn inst_val_dec(&mut self, _: &mut OrgState, _: &mut dyn OrgInterface) -> R {
        let dst = self.find_modified_stack(STACK_BX);
        let src = self.find_modified_stack(dst);
        let value = self.pop(src).wrapping_sub(1);
        self.push(dst, value);
        Ok(true)
    }

    // ----------------------------------------------------------------
    // Memory, heads and replication
    // ----------------------------------------------------------------

    /// `Inst_SetMemory`: put the flow head at the start of the memory space
    /// named by the following label (memory 0 without a label), creating
    /// the space if needed.
    pub(crate) fn inst_set_memory(
        &mut self,
        org: &mut OrgState,
        world: &mut dyn OrgInterface,
    ) -> R {
        self.read_label_cfg(world, MAX_MEMSPACE_LABEL);
        self.add_parasite_label_prefix(org, world);
        if self.label().get_size() == 0 {
            self.head_set(HEAD_FLOW, 0, 0);
        } else {
            let label = self.label().clone();
            let mem_space_used = self.find_memory_space_label(&label);
            if mem_space_used == -1 {
                return Ok(false);
            }
            self.head_set(HEAD_FLOW, 0, mem_space_used);
        }
        Ok(true)
    }

    /// `Inst_Divide`
    pub(crate) fn inst_divide(&mut self, org: &mut OrgState, world: &mut dyn OrgInterface) -> R {
        self.divide_main(org, world, 1.0)
    }

    /// `Inst_HeadRead`: push the opcode under ?READ? (or, after a copy
    /// mutation, a random one) onto ?AX? and advance the head.
    pub(crate) fn inst_head_read(&mut self, org: &mut OrgState, world: &mut dyn OrgInterface) -> R {
        let head_id = self.find_modified_head(HEAD_READ);
        let dst = self.find_modified_stack(STACK_AX);
        self.head_adjust(head_id);

        let mut to_mutate = true;
        if world.config().parasite_no_copy_mut != 0 && self.owner_is_horizontal(org) {
            to_mutate = false;
        }
        let copy_mut = org.mut_rates.test_copy_mut(world.rng());
        let read_inst = if copy_mut && to_mutate {
            self.base.inst_set.random_inst(world.rng()).op()
        } else {
            let h = self.get_head(head_id);
            h.get_inst(head_memory(&self.mem_array, &h)).op()
        };
        self.push(dst, read_inst);
        self.read_inst(read_inst);
        self.head_advance(head_id);
        Ok(true)
    }

    /// `Inst_HeadWrite`: write ?AX? (popped; 0 if not an opcode) at ?WRITE?,
    /// growing the memory space by one when the head is on its last site.
    pub(crate) fn inst_head_write(&mut self, _: &mut OrgState, _: &mut dyn OrgInterface) -> R {
        let head_id = self.find_modified_head(HEAD_WRITE);
        let src = self.find_modified_stack(STACK_AX);
        let ct = self.ct();
        let active = self.threads[ct].heads[head_id];
        let mem_space_used = ms_index(self.mem_array.len(), active.mem_space());
        {
            let mem = &mut self.mem_array[mem_space_used];
            if active.position() >= mem.size() - 1 {
                let n = mem.size() + 1;
                mem.resize(n);
                // UB substitute: from a memory of size 1 the C++ copies
                // from index -1.
                if n >= 2 {
                    mem.copy(n - 1, n - 2);
                }
            }
        }
        self.head_adjust(head_id);

        let mut value = self.pop(src);
        if value < 0 || value >= self.base.inst_set.size() {
            value = 0;
        }
        let h = self.threads[ct].heads[head_id];
        let i = ms_index(self.mem_array.len(), h.mem_space());
        h.set_inst(&mut self.mem_array[i], Instruction::new(value));
        h.set_flag_copied(&mut self.mem_array[i]);
        self.head_advance(head_id);
        Ok(true)
    }

    /// The operands of the conditionals: `op1 = ?AX?`, `op2 = ?next(op1)?`.
    fn compare_operands(&mut self) -> (i32, i32) {
        let op1 = self.find_modified_stack(STACK_AX);
        let op2 = self.find_modified_next_stack(op1);
        (self.top(op1), self.top(op2))
    }

    /// `Inst_IfEqual`
    pub(crate) fn inst_if_equal(&mut self, _: &mut OrgState, _: &mut dyn OrgInterface) -> R {
        let (a, b) = self.compare_operands();
        if a != b {
            self.ip_advance();
        }
        Ok(true)
    }

    /// `Inst_IfNotEqual`
    pub(crate) fn inst_if_not_equal(&mut self, _: &mut OrgState, _: &mut dyn OrgInterface) -> R {
        let (a, b) = self.compare_operands();
        if a == b {
            self.ip_advance();
        }
        Ok(true)
    }

    /// `Inst_IfLess`
    pub(crate) fn inst_if_less(&mut self, _: &mut OrgState, _: &mut dyn OrgInterface) -> R {
        let (a, b) = self.compare_operands();
        if a >= b {
            self.ip_advance();
        }
        Ok(true)
    }

    /// `Inst_IfGreater`
    pub(crate) fn inst_if_greater(&mut self, _: &mut OrgState, _: &mut dyn OrgInterface) -> R {
        let (a, b) = self.compare_operands();
        if a <= b {
            self.ip_advance();
        }
        Ok(true)
    }

    /// `Inst_IfGreaterEqual`.
    ///
    /// QUIRK: the next instruction is skipped when ?op1? is *greater*, so
    /// it runs when ?op1? <= ?op2?.
    pub(crate) fn inst_if_greater_equal(
        &mut self,
        _: &mut OrgState,
        _: &mut dyn OrgInterface,
    ) -> R {
        let (a, b) = self.compare_operands();
        if a > b {
            self.ip_advance();
        }
        Ok(true)
    }

    /// `Inst_HeadPush`: push the position of ?IP?.
    pub(crate) fn inst_head_push(&mut self, _: &mut OrgState, _: &mut dyn OrgInterface) -> R {
        let head_used = self.find_modified_head(HEAD_IP);
        let dst = self.find_modified_stack(STACK_BX);
        let pos = self.get_head(head_used).position();
        self.push(dst, pos);
        Ok(true)
    }

    /// `Inst_HeadPop`: move ?IP? to the popped position in its memory
    /// space.
    pub(crate) fn inst_head_pop(&mut self, _: &mut OrgState, _: &mut dyn OrgInterface) -> R {
        let head_used = self.find_modified_head(HEAD_IP);
        let src = self.find_modified_stack(STACK_BX);
        let pos = self.pop(src);
        let ms = self.get_head(head_used).mem_space();
        self.head_set(head_used, pos, ms);
        Ok(true)
    }

    /// `Inst_HeadMove`: ?IP? jumps to the flow head (`Set(const cHeadCPU&)`,
    /// no adjust); the flow head itself advances.
    pub(crate) fn inst_head_move(&mut self, _: &mut OrgState, _: &mut dyn OrgInterface) -> R {
        let head_used = self.find_modified_head(HEAD_IP);
        if head_used != HEAD_FLOW {
            let flow = self.get_head(HEAD_FLOW);
            let ct = self.ct();
            self.threads[ct].heads[head_used].set_from(&flow);
            if head_used == HEAD_IP {
                self.set_advance_ip(false);
            }
        } else {
            self.head_advance(HEAD_FLOW);
        }
        Ok(true)
    }

    /// `Inst_Search`: find the complement (rotated by 2) of the following
    /// label from the start of the IP's memory; on a hit the flow head goes
    /// to its last nop and BX/AX get the distance and label size, else the
    /// flow head goes just past the IP and BX gets 0.
    pub(crate) fn inst_search(&mut self, _: &mut OrgState, world: &mut dyn OrgInterface) -> R {
        self.read_label_cfg(world, CodeLabel::MAX_LENGTH as i32);
        self.label_mut().rotate(2, NUM_NOPS);
        let found_pos = self.find_label_from_start();
        let ip = self.ip();
        if found_pos.position() - ip.position() == 0 {
            self.head_set(HEAD_FLOW, ip.position() + 1, ip.mem_space());
            self.push(STACK_BX, 0);
        } else {
            let label_size = self.label().get_size();
            let search_size = found_pos.position() - ip.position() + label_size + 1;
            self.push(STACK_BX, search_size);
            self.push(STACK_AX, label_size);
            let ct = self.ct();
            self.threads[ct].heads[HEAD_FLOW].set_from(&found_pos);
        }
        Ok(true)
    }

    // ----------------------------------------------------------------
    // Stacks
    // ----------------------------------------------------------------

    /// `Inst_PushNext`: pop ?AX?, push onto the next stack.
    pub(crate) fn inst_push_next(&mut self, _: &mut OrgState, _: &mut dyn OrgInterface) -> R {
        let src = self.find_modified_stack(STACK_AX);
        let dst = self.find_modified_next_stack(src);
        let v = self.pop(src);
        self.push(dst, v);
        Ok(true)
    }

    /// `Inst_PushPrevious`: pop ?BX?, push onto the previous stack.
    pub(crate) fn inst_push_previous(&mut self, _: &mut OrgState, _: &mut dyn OrgInterface) -> R {
        let src = self.find_modified_stack(STACK_BX);
        let dst = self.find_modified_previous_stack(src);
        let v = self.pop(src);
        self.push(dst, v);
        Ok(true)
    }

    /// `Inst_PushComplement`: pop ?BX?, push onto the "complement" stack
    /// (the previous one, see `find_modified_complement_stack`).
    pub(crate) fn inst_push_complement(&mut self, _: &mut OrgState, _: &mut dyn OrgInterface) -> R {
        let src = self.find_modified_stack(STACK_BX);
        let dst = self.find_modified_complement_stack(src);
        let v = self.pop(src);
        self.push(dst, v);
        Ok(true)
    }

    /// `Inst_ValDelete`
    pub(crate) fn inst_val_delete(&mut self, _: &mut OrgState, _: &mut dyn OrgInterface) -> R {
        let stack_used = self.find_modified_stack(STACK_BX);
        self.pop(stack_used);
        Ok(true)
    }

    /// `Inst_ValCopy`
    pub(crate) fn inst_val_copy(&mut self, _: &mut OrgState, _: &mut dyn OrgInterface) -> R {
        let dst = self.find_modified_stack(STACK_BX);
        let src = self.find_modified_stack(dst);
        let v = self.top(src);
        self.push(dst, v);
        Ok(true)
    }

    /// `Inst_IO`: output the top of ?src? (not popped; as a parasite's
    /// output for parasite threads), then push the next input onto ?BX?.
    pub(crate) fn inst_io(&mut self, org: &mut OrgState, world: &mut dyn OrgInterface) -> R {
        let dst = self.find_modified_stack(STACK_BX);
        let src = self.find_modified_stack(dst);
        let value_out = self.top(src);
        let is_parasite = self.owner_is_horizontal(org);
        self.org_do_output(org, world, Some(value_out), false, is_parasite)?;
        let value_in = org.get_next_input(world);
        self.push(dst, value_in);
        org.do_input(value_in);
        Ok(true)
    }

    // ----------------------------------------------------------------
    // Threads
    // ----------------------------------------------------------------

    /// `Inst_ThreadCreate`.
    ///
    /// QUIRK: `ThreadCreate` returns -1 on failure, which converts to
    /// `true`; the instruction therefore never faults and always marks the
    /// organism multi-threaded.
    pub(crate) fn inst_thread_create(
        &mut self,
        org: &mut OrgState,
        world: &mut dyn OrgInterface,
    ) -> R {
        self.read_label_cfg(world, MAX_THREAD_LABEL);
        let label = self.label().clone();
        let ms = self.get_head(HEAD_FLOW).mem_space();
        let max_threads = world.config().max_cpu_threads;
        let success = self.thread_create(&label, ms, max_threads) != 0;
        if !success {
            org.fault(FAULT_LOC_THREAD_FORK, FAULT_TYPE_WARNING, "");
        }
        if success {
            org.phenotype.is_multi_thread = true;
        }
        Ok(success)
    }

    /// `Inst_ThreadCancel`: stop the current thread (never thread 0).
    pub(crate) fn inst_thread_cancel(&mut self, org: &mut OrgState, _: &mut dyn OrgInterface) -> R {
        let cur = self.cur_thread;
        let success = self.thread_kill(cur);
        if !success {
            org.fault(FAULT_LOC_THREAD_KILL, FAULT_TYPE_WARNING, "");
        }
        if self.threads.len() == 1 {
            org.phenotype.is_multi_thread = false;
        }
        Ok(success)
    }

    /// `Inst_ThreadKill`: stop the thread named by the following label.
    pub(crate) fn inst_thread_kill(
        &mut self,
        org: &mut OrgState,
        world: &mut dyn OrgInterface,
    ) -> R {
        self.read_label_cfg(world, MAX_THREAD_LABEL);
        let label = self.label().clone();
        let id = self.find_thread_label(&label);
        let success = self.thread_kill(id);
        if !success {
            org.fault(FAULT_LOC_THREAD_KILL, FAULT_TYPE_WARNING, "");
        }
        if self.threads.len() == 1 {
            org.phenotype.is_multi_thread = false;
        }
        Ok(success)
    }

    // ----------------------------------------------------------------
    // Parasites and the environment
    // ----------------------------------------------------------------

    /// `Inst_Inject`: inject the write head's memory space into the
    /// neighbour named by the following label.
    pub(crate) fn inst_inject(&mut self, org: &mut OrgState, world: &mut dyn OrgInterface) -> R {
        self.read_label_cfg(world, MAX_MEMSPACE_LABEL);
        self.add_parasite_label_prefix(org, world);
        self.inject_parasite(org, world, 1.0)
    }

    /// `Inst_Apoptosis`
    pub(crate) fn inst_apoptosis(&mut self, org: &mut OrgState, world: &mut dyn OrgInterface) -> R {
        org.die(world);
        Ok(true)
    }

    /// `Inst_RotateLeft`: fails without neighbours.
    pub(crate) fn inst_rotate_left(&mut self, _: &mut OrgState, world: &mut dyn OrgInterface) -> R {
        if world.get_num_neighbors() == 0 {
            return Ok(false);
        }
        world.rotate(1);
        Ok(true)
    }

    /// `Inst_RotateRight`: fails without neighbours.
    pub(crate) fn inst_rotate_right(
        &mut self,
        _: &mut OrgState,
        world: &mut dyn OrgInterface,
    ) -> R {
        if world.get_num_neighbors() == 0 {
            return Ok(false);
        }
        world.rotate(-1);
        Ok(true)
    }

    /// The return address `(IP position & 0xFFFF) | (IP memory space << 8)`.
    ///
    /// QUIRK: the memory space overlaps bits 8-15 of the position.
    fn return_address(&self) -> i32 {
        let ip = self.ip();
        (ip.position() & 0xFFFF) | ip.mem_space().wrapping_shl(8)
    }

    /// `Inst_CallFlow`: push the return address on ?AX? and jump the IP to
    /// the flow head (the IP then advances past it).
    pub(crate) fn inst_call_flow(&mut self, _: &mut OrgState, _: &mut dyn OrgInterface) -> R {
        let dst = self.find_modified_stack(STACK_AX);
        let ra = self.return_address();
        self.push(dst, ra);
        let flow = self.get_head(HEAD_FLOW);
        self.head_set(HEAD_IP, flow.position(), flow.mem_space());
        Ok(true)
    }

    /// `Inst_CallLabel`: push the return address on ?AX? and jump the IP to
    /// the start of the memory space named by the following label (created
    /// if new); fails without a label.
    pub(crate) fn inst_call_label(&mut self, _: &mut OrgState, world: &mut dyn OrgInterface) -> R {
        let dst = self.find_modified_stack(STACK_AX);
        let ra = self.return_address();
        self.push(dst, ra);
        self.read_label_cfg(world, MAX_MEMSPACE_LABEL);
        if self.label().get_size() != 0 {
            let label = self.label().clone();
            let mem_space_used = self.find_memory_space_label(&label);
            if mem_space_used > -1 {
                self.head_set(HEAD_IP, 0, mem_space_used);
                return Ok(true);
            }
        }
        Ok(false)
    }

    /// `Inst_Return`: pop ?AX? as a return address.
    pub(crate) fn inst_return(&mut self, _: &mut OrgState, _: &mut dyn OrgInterface) -> R {
        let src = self.find_modified_stack(STACK_AX);
        let ra = self.pop(src) as u32;
        let location = (ra & 0xFFFF) as i32;
        let mem_space = ms_index(self.mem_array.len(), (ra >> 8) as i32) as i32;
        self.head_set(HEAD_IP, location, mem_space);
        Ok(true)
    }

    /// `Inst_Divide_Erase`: divide; on failure clear the write head's
    /// memory space to "a" and reset the current thread's heads to memory
    /// 0.
    ///
    /// QUIRK: with the write head in memory 0 the divide fails and the
    /// genome itself is replaced by "a".
    pub(crate) fn inst_divide_erase(
        &mut self,
        org: &mut OrgState,
        world: &mut dyn OrgInterface,
    ) -> R {
        if self.divide_main(org, world, 1.0)? {
            return Ok(true);
        }
        org.phenotype.divide_failed();
        let mem_space_used = self.get_head(HEAD_WRITE).mem_space();
        if self.mem_array.len() as i32 <= mem_space_used {
            return Ok(false);
        }
        self.mem_array[mem_space_used.max(0) as usize]
            .assign(&InstructionSequence::from_string("a"));
        self.reset_current_heads(|_| 0);
        Ok(false)
    }

    /// `Inst_Divide_Sex_Erase`
    pub(crate) fn inst_divide_sex_erase(
        &mut self,
        org: &mut OrgState,
        world: &mut dyn OrgInterface,
    ) -> R {
        org.phenotype.divide_sex = true;
        org.phenotype.cross_num = 1;
        self.inst_divide_erase(org, world)
    }

    /// `Inst_Divide_Sex`
    pub(crate) fn inst_divide_sex(
        &mut self,
        org: &mut OrgState,
        world: &mut dyn OrgInterface,
    ) -> R {
        org.phenotype.divide_sex = true;
        org.phenotype.cross_num = 1;
        self.inst_divide(org, world)
    }

    /// `Inst_Divide_Asex_Wait`: sexual bookkeeping without recombination.
    pub(crate) fn inst_divide_asex_wait(
        &mut self,
        org: &mut OrgState,
        world: &mut dyn OrgInterface,
    ) -> R {
        org.phenotype.divide_sex = true;
        org.phenotype.cross_num = 0;
        self.inst_divide(org, world)
    }

    /// `Inst_Collect_Unit`: `DoCollect(ctx, true, true, false, true)`.
    pub(crate) fn inst_collect_unit(
        &mut self,
        org: &mut OrgState,
        world: &mut dyn OrgInterface,
    ) -> R {
        // FindModifiedResource
        let num_resources = world.get_resources(org).len() as i32;
        if num_resources <= 0 {
            return Ok(false);
        }
        let num_nops = self.base.inst_set.num_nops();
        let max_len = max_label_length(num_resources, num_nops);
        self.read_label_cfg(world, max_len);
        let real_label_length = self.label().get_size();
        let spec_id = self.label().as_int_unique(num_nops);
        let label_int = self.label().as_int(num_nops);
        let chunk_size = 1.0 / f64::from(num_nops).powf(f64::from(real_label_length));
        let r = world.rng().get_double_max(chunk_size);
        let resource_approx = f64::from(label_int).mul_add(chunk_size, r);
        let bin_used = (resource_approx * f64::from(num_resources)).floor() as i32;
        if bin_used < 0 {
            return Ok(false);
        }

        // IncCollectSpecCount(spec_id)
        if let Some(c) = usize::try_from(spec_id)
            .ok()
            .and_then(|i| org.phenotype.cur_collect_spec_counts.get_mut(i))
        {
            *c = c.wrapping_add(1);
        }

        // DoActualCollect(ctx, bin_used, env_remove = true,
        // internal_add = true, probabilistic = false, unit = true)
        let res_count = world.get_resources(org);
        let mut res_change = vec![0.0; res_count.len()];
        let total = org.rbins_total();
        let max = world.config().max_total_stored;
        if at(&res_count, bin_used) >= 1.0 {
            set_at(&mut res_change, bin_used, -1.0);
        } else {
            return Ok(false);
        }
        let change = at(&res_change, bin_used);
        if max < 0.0 || (total + -change) <= max {
            org.add_to_rbin(bin_used, -change);
        }
        if max >= 0.0 && (total + -change) > max {
            set_at(&mut res_change, bin_used, 0.0);
        }
        world.update_resources(org, &res_change);
        Ok(true)
    }
}
