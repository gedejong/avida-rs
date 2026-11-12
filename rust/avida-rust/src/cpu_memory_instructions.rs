//! CPU instruction handlers that edit genome memory or redirect control flow:
//! copy/compare, head read/write/copy (with copy mutations), label jumps,
//! throw/goto searches, thread fork/kill, and allocate/divide entry points.
//!
//! The handler logic is written against the [`Machine`] trait rather than raw
//! FFI calls. In production the trait is implemented by [`FfiMachine`], which
//! forwards to `cHardwareFFI.cc`; the unit tests use an in-memory mock. Every
//! handler preserves the C++ call order of RNG draws and head adjustments, since
//! both are observable in consistency-test output.
#![allow(clippy::missing_safety_doc)]

use std::ffi::{c_char, c_int, c_void, CStr};

const REG_AX: c_int = 0;
const REG_BX: c_int = 1;
const REG_CX: c_int = 2;

const HEAD_IP: c_int = 0;
const HEAD_READ: c_int = 1;
const HEAD_WRITE: c_int = 2;

/// Memory flag masks, matching `avd_hw_get_memory_flags`.
const FLAG_COPIED: c_int = 0x01;
const FLAG_MUTATED: c_int = 0x02;
const FLAG_EXECUTED: c_int = 0x04;
const FLAG_COPY_MUT: c_int = 0x10;

/// Copy-mutation kinds, matching `avd_org_test_copy` / `avd_org_get_copy_prob`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CopyMut {
    Point = 0,
    Insert = 1,
    Delete = 2,
    Uniform = 3,
    Slip = 4,
}

// nHardware.h fault codes.
const FAULT_TYPE_FORK_TH: c_int = 0;
const FAULT_TYPE_KILL_TH: c_int = 0;
const FAULT_TYPE_ERROR: c_int = 1;
const FAULT_LOC_JUMP: c_int = 2;
const FAULT_LOC_THREAD_FORK: c_int = 5;
const FAULT_LOC_THREAD_KILL: c_int = 6;

/// `cHeadCPU::fullAdjust`: map an arbitrary position onto `0..mem_size`.
///
/// Negative positions and empty memory clamp to 0 (the documented C++ quirk:
/// heads loop forward but not backward). Positions less than twice the memory
/// size wrap by subtraction, larger ones by modulo.
pub fn adjust_position(pos: c_int, mem_size: c_int) -> c_int {
    if pos >= 0 && pos < mem_size {
        pos
    } else if mem_size == 0 || pos < 0 {
        0
    } else if (pos as i64) < 2 * (mem_size as i64) {
        pos - mem_size
    } else {
        pos % mem_size
    }
}

/// `AvidaTools::Mod`: remainder normalised into `0..base`.
fn modulo(value: c_int, base: c_int) -> c_int {
    if base == 0 {
        return 0;
    }
    let rem = value.wrapping_rem(base);
    if rem < 0 {
        rem + base
    } else {
        rem
    }
}

/// Which `Divide_Main*` routine performs a division.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DivideKind {
    /// `Divide_Main`.
    Plain = 0,
    /// `Divide_MainRS`: resample reverted offspring.
    Resample = 1,
    /// `Divide_Main1RS`: at most one divide mutation, resampling.
    Resample1 = 2,
    /// `Divide_Main2RS`: at most two divide mutations, resampling.
    Resample2 = 3,
}

/// Register guard shared by the `...if=0` / `...if!=0` instruction variants.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Condition {
    Always,
    /// Run only when BX is non-zero.
    IfNot0,
    /// Run only when BX is zero.
    If0,
}

impl Condition {
    fn holds<M: Machine>(self, m: &mut M) -> bool {
        match self {
            Condition::Always => true,
            Condition::IfNot0 => m.reg(REG_BX) != 0,
            Condition::If0 => m.reg(REG_BX) == 0,
        }
    }

    fn from_code(code: c_int) -> Option<Self> {
        match code {
            0 => Some(Condition::Always),
            1 => Some(Condition::IfNot0),
            2 => Some(Condition::If0),
            _ => None,
        }
    }
}

/// The slice of `cHardwareCPU` state these handlers need.
pub trait Machine {
    fn mem_size(&mut self) -> c_int;
    fn mem_inst(&mut self, pos: c_int) -> c_int;
    fn set_mem_inst(&mut self, pos: c_int, op: c_int);
    fn mem_set_flags(&mut self, pos: c_int, mask: c_int);
    fn mem_clear_flags(&mut self, pos: c_int, mask: c_int);
    fn mem_insert(&mut self, pos: c_int, op: c_int);
    fn mem_remove(&mut self, pos: c_int);

    fn head_pos(&mut self, head: c_int) -> c_int;
    /// `cHeadCPU::Set(pos)`: stores the position and adjusts it into range.
    fn set_head_pos(&mut self, head: c_int, pos: c_int);
    fn cur_head(&mut self) -> c_int;
    /// `FindModifiedHead`: consumes a trailing nop if present.
    fn find_modified_head(&mut self, default_head: c_int) -> c_int;

    fn reg(&mut self, reg: c_int) -> c_int;
    fn set_reg(&mut self, reg: c_int, value: c_int);
    fn stack_push(&mut self, value: c_int);
    fn stack_pop(&mut self) -> c_int;

    /// `ReadLabel`: read the nop label after the IP, advancing the IP over it.
    fn read_label(&mut self);
    fn label_size(&mut self) -> c_int;
    fn label_nop(&mut self, idx: c_int) -> c_int;
    /// Rotate the current label to its complement, then `FindLabel(direction)`.
    /// Returns the found position, or -1.
    fn search_complement_label(&mut self, direction: c_int) -> c_int;

    fn next_inst_is_nop(&mut self) -> bool;
    fn is_nop(&mut self, op: c_int) -> bool;
    fn nop_mod(&mut self, op: c_int) -> c_int;
    fn inst_set_size(&mut self) -> c_int;
    fn random_inst(&mut self) -> c_int;
    fn check_no_mut_list(&mut self, pos: c_int) -> bool;
    /// `ReadInst`: track the instruction in the read label.
    fn read_inst(&mut self, op: c_int);

    fn test_copy(&mut self, kind: CopyMut) -> bool;
    fn copy_prob(&mut self, kind: CopyMut) -> f64;
    fn random_p(&mut self, prob: f64) -> bool;
    fn random_int(&mut self, max: c_int) -> c_int;
    fn uniform_copy_mutation(&mut self, head: c_int);
    fn slip_mutation(&mut self, pos: c_int);
    fn slip_read_head(&mut self) -> bool;

    fn fork_thread(&mut self) -> bool;
    fn kill_thread(&mut self) -> bool;
    fn set_advance_ip(&mut self, advance: bool);
    fn fault(&mut self, loc: c_int, fault_type: c_int, desc: &CStr);

    fn allocate_main(&mut self, size: c_int) -> bool;
    fn divide_main(
        &mut self,
        div_point: c_int,
        extra_lines: c_int,
        mut_multiplier: f64,
        kind: DivideKind,
    ) -> bool;
    /// `AdjustHeads`: re-adjust every head of every thread.
    fn adjust_all_heads(&mut self);
    /// Opcode of the named instruction (the error opcode when absent).
    fn find_inst_op(&mut self, name: &CStr) -> c_int;
    /// `GetLabel().AsInt(NUM_NOPS)`.
    fn label_as_int(&mut self) -> c_int;

    fn set_divide_sex(&mut self, divide_sex: bool);
    fn set_cross_num(&mut self, cross_num: c_int);
    fn set_mate_select_id(&mut self, select_id: c_int);
    fn born_parent_group(&mut self) -> bool;

    fn rbin(&mut self, idx: c_int) -> f64;
    fn add_to_rbin(&mut self, idx: c_int, amount: f64);

    /// Position a temporary `cHeadCPU(hw, pos)` would settle on.
    fn adjusted(&mut self, pos: c_int) -> c_int {
        let size = self.mem_size();
        adjust_position(pos, size)
    }
    /// `cHeadCPU::Adjust` on a thread head. Returns the adjusted position.
    fn adjust_head(&mut self, head: c_int) -> c_int {
        let pos = self.head_pos(head);
        self.set_head_pos(head, pos);
        self.head_pos(head)
    }
    /// `cHeadCPU::Advance` on a thread head.
    fn advance_head(&mut self, head: c_int) {
        let pos = self.head_pos(head);
        self.set_head_pos(head, pos.wrapping_add(1));
    }
}

// ---------------------------------------------------------------------------
// Handler logic
// ---------------------------------------------------------------------------

/// Inst_IfGrX / Inst_IfEquX: compare BX against a constant chosen by an
/// optional nop (none: 1, nop-A: -1, nop-B: 2, nop-C: 4). Returns true when the
/// next instruction must be skipped.
pub fn if_x<M: Machine>(m: &mut M, greater: bool) -> bool {
    let mut value = 1;
    if m.next_inst_is_nop() {
        m.advance_head(HEAD_IP);
        let ip = m.head_pos(HEAD_IP);
        let op = m.mem_inst(ip);
        value = match m.nop_mod(op) {
            REG_AX => -1,
            REG_BX => 2,
            REG_CX => 4,
            _ => 1,
        };
        m.mem_set_flags(ip, FLAG_EXECUTED);
    }
    let bx = m.reg(REG_BX);
    if greater {
        bx <= value
    } else {
        bx != value
    }
}

/// Inst_JumpF / Inst_JumpB: jump the active head to the complement label, or
/// by BX when there is no label. Returns false (after faulting) when the
/// complement label does not exist.
pub fn jump_label<M: Machine>(m: &mut M, direction: c_int) -> bool {
    m.read_label();
    if m.label_size() == 0 {
        let head = m.cur_head();
        let pos = m.head_pos(head);
        let bx = m.reg(REG_BX);
        m.set_head_pos(head, pos.wrapping_add(bx));
        return true;
    }
    let found = m.search_complement_label(direction);
    if found != -1 {
        let head = m.cur_head();
        m.set_head_pos(head, found);
        return true;
    }
    let desc = if direction > 0 {
        c"jump-f: No complement label"
    } else {
        c"jump-b: No complement label"
    };
    m.fault(FAULT_LOC_JUMP, FAULT_TYPE_ERROR, desc);
    false
}

/// Inst_Call: push the IP position, then jump the IP like `jump-f`.
pub fn call<M: Machine>(m: &mut M) -> bool {
    let location = m.head_pos(HEAD_IP);
    m.stack_push(location);
    m.read_label();
    if m.label_size() == 0 {
        let pos = m.head_pos(HEAD_IP);
        let bx = m.reg(REG_BX);
        m.set_head_pos(HEAD_IP, pos.wrapping_add(bx));
        return true;
    }
    let found = m.search_complement_label(1);
    if found != -1 {
        m.set_head_pos(HEAD_IP, found);
        return true;
    }
    m.fault(
        FAULT_LOC_JUMP,
        FAULT_TYPE_ERROR,
        c"call: no complement label",
    );
    false
}

/// Inst_Throw: scan forward (circularly) for a `catch` whose trailing nops are
/// a prefix-compatible match of the throw label; on success park the IP on it.
///
/// When `cond` does not hold the instruction fails without reading its label.
pub fn throw<M: Machine>(m: &mut M, catch_op: c_int, cond: Condition) -> bool {
    if !cond.holds(m) {
        return false;
    }
    m.read_label();
    let label_size = m.label_size();
    let start = m.head_pos(HEAD_IP);
    let mut pos = m.adjusted(start.wrapping_add(1));

    while pos != start {
        if m.mem_inst(pos) == catch_op {
            let catch_pos = pos;
            pos = m.adjusted(pos + 1);

            let mut matched = true;
            let mut size_matched = 0;
            loop {
                let op = m.mem_inst(pos);
                if !(matched && m.is_nop(op) && size_matched < label_size) {
                    break;
                }
                if m.label_nop(size_matched) != m.nop_mod(op) {
                    matched = false;
                }
                pos = m.adjusted(pos + 1);
                size_matched += 1;
            }

            if matched {
                m.set_head_pos(HEAD_IP, catch_pos);
                m.set_advance_ip(false);
                return true;
            }

            // If we advanced past nops during testing, retreat.
            let op = m.mem_inst(pos);
            if !m.is_nop(op) {
                pos = m.adjusted(pos - 1);
            }
        }
        pos = m.adjusted(pos + 1);
    }
    false
}

/// Inst_Goto: scan forward (circularly) for a `label` instruction followed by
/// exactly the goto's nop label; on success park the IP on it.
///
/// When `cond` does not hold the instruction fails without reading its label.
pub fn goto<M: Machine>(m: &mut M, label_op: c_int, cond: Condition) -> bool {
    if !cond.holds(m) {
        return false;
    }
    m.read_label();
    let label_size = m.label_size();
    let start = m.head_pos(HEAD_IP);
    let mut pos = m.adjusted(start.wrapping_add(1));

    while pos != start {
        if m.mem_inst(pos) == label_op {
            let label_pos = pos;
            pos = m.adjusted(pos + 1);

            let mut size_matched = 0;
            while size_matched < label_size {
                let op = m.mem_inst(pos);
                if !m.is_nop(op) || m.label_nop(size_matched) != m.nop_mod(op) {
                    break;
                }
                size_matched += 1;
                pos = m.adjusted(pos + 1);
            }

            let op = m.mem_inst(pos);
            let at_nop = m.is_nop(op);
            if size_matched == label_size && !at_nop {
                m.set_head_pos(HEAD_IP, label_pos);
                m.set_advance_ip(false);
                return true;
            }
            if !at_nop {
                pos = m.adjusted(pos - 1);
            }
        }
        pos = m.adjusted(pos + 1);
    }
    false
}

/// Write `op` at `to`, or a random instruction when a copy mutation fires and
/// the instruction at `guard_pos` is not on the no-mutation list.
fn write_with_copy_mut<M: Machine>(m: &mut M, to: c_int, op: c_int, guard_pos: c_int) {
    if m.test_copy(CopyMut::Point) && !m.check_no_mut_list(guard_pos) {
        let random = m.random_inst();
        m.set_mem_inst(to, random);
        m.mem_set_flags(to, FLAG_MUTATED | FLAG_COPY_MUT);
    } else {
        m.set_mem_inst(to, op);
        m.mem_clear_flags(to, FLAG_MUTATED | FLAG_COPY_MUT);
    }
    m.mem_set_flags(to, FLAG_COPIED);
}

/// Positions of the `from` (BX) and `to` (AX + BX) temporary heads.
fn copy_operands<M: Machine>(m: &mut M) -> (c_int, c_int) {
    let bx = m.reg(REG_BX);
    let ax = m.reg(REG_AX);
    let from = m.adjusted(bx);
    let to = m.adjusted(ax.wrapping_add(bx));
    (from, to)
}

/// Inst_Copy: copy the instruction at BX to AX + BX.
pub fn copy<M: Machine>(m: &mut M) {
    let (from, to) = copy_operands(m);
    let op = m.mem_inst(from);
    write_with_copy_mut(m, to, op, from);
}

/// Inst_WriteInst: write the instruction numbered by `src_reg` to AX + BX.
pub fn write_inst<M: Machine>(m: &mut M, src_reg: c_int) {
    let (_, to) = copy_operands(m);
    let src = m.reg(src_reg);
    let set_size = m.inst_set_size();
    write_with_copy_mut(m, to, modulo(src, set_size), to);
}

/// Inst_StackWriteInst: pop an instruction number and write it to AX + ?BX?.
pub fn stack_write_inst<M: Machine>(m: &mut M, dst_reg: c_int) {
    let ax = m.reg(REG_AX);
    let dst = m.reg(dst_reg);
    let to = m.adjusted(ax.wrapping_add(dst));
    let popped = m.stack_pop();
    let set_size = m.inst_set_size();
    write_with_copy_mut(m, to, modulo(popped, set_size), to);
}

/// Inst_Compare: store the opcode difference between BX and AX + BX. A copy
/// mutation may corrupt the destination first.
pub fn compare<M: Machine>(m: &mut M, dst_reg: c_int) {
    let (from, to) = copy_operands(m);
    if m.test_copy(CopyMut::Point) && !m.check_no_mut_list(from) {
        let random = m.random_inst();
        m.set_mem_inst(to, random);
        m.mem_set_flags(to, FLAG_MUTATED | FLAG_COPY_MUT);
    }
    let diff = m.mem_inst(from).wrapping_sub(m.mem_inst(to));
    m.set_reg(dst_reg, diff);
}

/// Inst_IfNCpy: skip the next instruction when BX and AX + BX hold the same
/// instruction; a copy mutation inverts the test. Returns true to skip.
pub fn if_n_cpy<M: Machine>(m: &mut M) -> bool {
    let (from, to) = copy_operands(m);
    let mutated = m.test_copy(CopyMut::Point);
    let same = m.mem_inst(from) == m.mem_inst(to);
    if mutated {
        !same
    } else {
        same
    }
}

/// Inst_HeadRead: read the instruction under ?read-head? into BX.
pub fn head_read<M: Machine>(m: &mut M) {
    let head = m.find_modified_head(HEAD_READ);
    let pos = m.adjust_head(head);

    let read_inst = if m.test_copy(CopyMut::Point) {
        m.random_inst()
    } else {
        m.mem_inst(pos)
    };
    m.set_reg(REG_BX, read_inst);
    m.read_inst(read_inst);

    if m.slip_read_head() && m.test_copy(CopyMut::Slip) {
        let size = m.mem_size();
        let target = m.random_int(size);
        m.set_head_pos(head, target);
    }
    m.advance_head(head);
}

/// Insertion, deletion, uniform and slip copy mutations applied at the write
/// head after an instruction has been written. `draw` decides whether each
/// kind fires, so the plain and error-correcting copies share this sequence.
fn post_write_mutations<M: Machine>(
    m: &mut M,
    write_head: c_int,
    read_head: Option<c_int>,
    draw: &mut dyn FnMut(&mut M, CopyMut) -> bool,
) {
    if draw(m, CopyMut::Insert) {
        let pos = m.head_pos(write_head);
        let random = m.random_inst();
        m.mem_insert(pos, random);
    }
    if draw(m, CopyMut::Delete) {
        let pos = m.head_pos(write_head);
        m.mem_remove(pos);
    }
    if draw(m, CopyMut::Uniform) {
        m.uniform_copy_mutation(write_head);
    }
    match read_head {
        // h-copy: the slip test is always drawn; its target depends on config.
        Some(read_head) => {
            if draw(m, CopyMut::Slip) {
                if m.slip_read_head() {
                    let size = m.mem_size();
                    let target = m.random_int(size);
                    m.set_head_pos(read_head, target);
                } else {
                    let pos = m.head_pos(write_head);
                    m.slip_mutation(pos);
                }
            }
        }
        // h-write: the slip test is only drawn when slips apply to the write head.
        None => {
            if !m.slip_read_head() && draw(m, CopyMut::Slip) {
                let pos = m.head_pos(write_head);
                m.slip_mutation(pos);
            }
        }
    }
}

/// Inst_HeadWrite: write the instruction numbered by BX under ?write-head?.
pub fn head_write<M: Machine>(m: &mut M) {
    let head = m.find_modified_head(HEAD_WRITE);
    let pos = m.adjust_head(head);

    let mut value = m.reg(REG_BX);
    if value < 0 || value >= m.inst_set_size() {
        value = 0;
    }
    m.set_mem_inst(pos, value);
    m.mem_set_flags(pos, FLAG_COPIED);

    post_write_mutations(m, head, None, &mut |m, kind| m.test_copy(kind));
    m.advance_head(head);
}

/// Inst_HeadCopy and the error-correcting Inst_HeadCopy2..10: copy the
/// instruction under the read head to the write head and advance both.
///
/// `reduction` is `None` for plain `h-copy` (organism mutation-rate tests) and
/// `Some(r)` for the error-correcting variants, which draw each mutation with
/// probability `rate / r` directly from the context RNG.
pub fn head_copy<M: Machine>(m: &mut M, reduction: Option<f64>) {
    let read_pos = m.adjust_head(HEAD_READ);
    let write_pos = m.adjust_head(HEAD_WRITE);

    let mut draw = move |m: &mut M, kind: CopyMut| match reduction {
        None => m.test_copy(kind),
        Some(r) => {
            let prob = m.copy_prob(kind) / r;
            m.random_p(prob)
        }
    };

    let mut read_inst = m.mem_inst(read_pos);
    m.read_inst(read_inst);

    if draw(m, CopyMut::Point) && !m.check_no_mut_list(read_pos) {
        read_inst = m.random_inst();
        m.mem_set_flags(write_pos, FLAG_MUTATED | FLAG_COPY_MUT);
    }
    m.set_mem_inst(write_pos, read_inst);
    m.mem_set_flags(write_pos, FLAG_COPIED);

    post_write_mutations(m, HEAD_WRITE, Some(HEAD_READ), &mut draw);

    m.advance_head(HEAD_READ);
    m.advance_head(HEAD_WRITE);
}

/// Inst_HeadCopy_ifResource: `h-copy` gated on the organism holding one unit of
/// the resource whose index equals the opcode under the read head.
pub fn head_copy_if_resource<M: Machine>(m: &mut M) {
    let pos = m.adjust_head(HEAD_READ);
    let resource = m.mem_inst(pos);
    if resource < 52 && m.rbin(resource) >= 1.0 {
        m.add_to_rbin(resource, -1.0);
        head_copy(m, None);
    }
}

/// Inst_ForkThread: advance the IP and fork; fault when the fork fails.
pub fn fork_thread<M: Machine>(m: &mut M) {
    m.advance_head(HEAD_IP);
    if !m.fork_thread() {
        m.fault(FAULT_LOC_THREAD_FORK, FAULT_TYPE_FORK_TH, c"");
    }
}

/// Inst_ForkThreadLabel: fork a thread that starts after the complement label,
/// falling back to a plain fork when there is no label.
///
/// When `cond` does not hold the label is still consumed and the instruction
/// fails.
pub fn fork_thread_label<M: Machine>(m: &mut M, cond: Condition) -> bool {
    if !cond.holds(m) {
        m.read_label();
        return false;
    }
    m.read_label();
    if m.label_size() == 0 {
        fork_thread(m);
        return true;
    }
    let found = m.search_complement_label(1);
    let save_pos = m.head_pos(HEAD_IP);
    if found != save_pos {
        m.set_head_pos(HEAD_IP, found.wrapping_add(1));
        if !m.fork_thread() {
            m.fault(FAULT_LOC_THREAD_FORK, FAULT_TYPE_FORK_TH, c"");
        }
        m.set_head_pos(HEAD_IP, save_pos);
    }
    true
}

/// Inst_KillThread: kill the current thread; fault when it is the last one.
pub fn kill_thread<M: Machine>(m: &mut M) {
    if m.kill_thread() {
        m.set_advance_ip(false);
    } else {
        m.fault(FAULT_LOC_THREAD_KILL, FAULT_TYPE_KILL_TH, c"");
    }
}

/// Inst_Allocate: allocate BX more memory; AX receives the old size.
pub fn allocate<M: Machine>(m: &mut M) -> bool {
    let size = m.mem_size();
    let requested = m.reg(REG_BX);
    if m.allocate_main(requested) {
        m.set_reg(REG_AX, size);
        true
    } else {
        false
    }
}

/// Inst_MaxAlloc / Inst_MaxAllocMoveWriteHead: allocate as much as the
/// offspring size range and genome length limit allow.
pub fn max_alloc<M: Machine>(
    m: &mut M,
    offspring_size_range: f64,
    max_genome_length: c_int,
    move_write_head: bool,
) -> bool {
    let cur_size = m.mem_size();
    let by_range = (offspring_size_range * f64::from(cur_size)) as c_int;
    let alloc_size = by_range.min(max_genome_length - cur_size);
    if m.allocate_main(alloc_size) {
        m.set_reg(REG_AX, cur_size);
        if move_write_head {
            m.set_head_pos(HEAD_WRITE, cur_size);
        }
        true
    } else {
        false
    }
}

/// Division entry points that take their divide point from machine state.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DivideMode {
    /// Inst_Divide: divide at AX.
    AtAx,
    /// Inst_DivideRS: divide at AX, resampling reverted offspring.
    AtAxResample,
    /// Inst_CDivide: divide at half the memory size.
    Half,
    /// Inst_CAlloc: not a divide; allocate the current memory size again.
    DoubleAlloc,
}

pub fn divide<M: Machine>(m: &mut M, mode: DivideMode) -> bool {
    match mode {
        DivideMode::AtAx => {
            let point = m.reg(REG_AX);
            m.divide_main(point, 0, 1.0, DivideKind::Plain)
        }
        DivideMode::AtAxResample => {
            let point = m.reg(REG_AX);
            m.divide_main(point, 0, 1.0, DivideKind::Resample)
        }
        DivideMode::Half => {
            let point = m.mem_size() / 2;
            m.divide_main(point, 0, 1.0, DivideKind::Plain)
        }
        DivideMode::DoubleAlloc => {
            let size = m.mem_size();
            m.allocate_main(size)
        }
    }
}

/// Inst_HeadDivide and its mutation-multiplier / resampling variants: divide
/// at the read head, with the offspring ending at the write head.
///
/// With a tolerance window configured, a successful plain divide reports in BX
/// whether the offspring was born into the parent's group.
pub fn head_divide<M: Machine>(
    m: &mut M,
    kind: DivideKind,
    mut_multiplier: f64,
    tolerance_window: bool,
) -> bool {
    m.adjust_all_heads();
    let divide_pos = m.head_pos(HEAD_READ);
    let mem_size = m.mem_size();
    let mut child_end = m.head_pos(HEAD_WRITE);
    if child_end == 0 {
        child_end = mem_size;
    }
    let extra_lines = mem_size - child_end;
    let divided = m.divide_main(divide_pos, extra_lines, mut_multiplier, kind);
    m.adjust_all_heads();

    if kind == DivideKind::Plain && tolerance_window && divided {
        let born_in_group = m.born_parent_group();
        m.set_reg(REG_BX, c_int::from(born_in_group));
    }
    divided
}

/// Reproduction-mode flags set before a head divide.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SexMode {
    /// Inst_HeadDivideSex.
    Sex,
    /// Inst_HeadDivideAsex.
    Asex,
    /// Inst_HeadDivideAsexWait: waits in the birth chamber without crossover.
    AsexWait,
    /// Inst_HeadDivideMateSelect: sexual, with the trailing label as mate ID.
    MateSelect,
}

pub fn head_divide_sex<M: Machine>(m: &mut M, mode: SexMode, tolerance_window: bool) -> bool {
    let (divide_sex, cross_num) = match mode {
        SexMode::Sex => (true, 1),
        SexMode::Asex => (false, 0),
        SexMode::AsexWait => (true, 0),
        SexMode::MateSelect => {
            m.read_label();
            let select_id = m.label_as_int();
            m.set_mate_select_id(select_id);
            (true, 1)
        }
    };
    m.set_divide_sex(divide_sex);
    m.set_cross_num(cross_num);
    head_divide(m, DivideKind::Plain, 1.0, tolerance_window)
}

/// `cHardwareCPU::Find`: circular search from the IP for an opcode.
///
/// Faithful to the original's quirk: the instruction directly after the IP is
/// never examined, and the IP's own position is returned when nothing matches.
fn find_from_ip<M: Machine>(m: &mut M, op: c_int) -> c_int {
    let current = m.head_pos(HEAD_IP);
    let mut pos = m.adjusted(current + 1);
    while pos != current {
        pos = m.adjusted(pos + 1);
        if m.mem_inst(pos) == op {
            break;
        }
    }
    pos
}

/// `Else_TopHalf`: the if-condition failed, so jump to the `else` clause, or
/// failing that to the `endif`, or failing that skip one instruction.
pub fn else_top_half<M: Machine>(m: &mut M) {
    let else_op = m.find_inst_op(c"else");
    let else_pos = find_from_ip(m, else_op);
    let endif_op = m.find_inst_op(c"endif");
    let endif_pos = find_from_ip(m, endif_op);

    let ip = m.head_pos(HEAD_IP);
    if else_pos != ip {
        m.set_head_pos(HEAD_IP, else_pos);
    } else if endif_pos != ip {
        m.set_head_pos(HEAD_IP, endif_pos);
    } else {
        m.advance_head(HEAD_IP);
    }
}

/// Comparison performed by the structured `if-...-end` instructions.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum IfEnd {
    /// Inst_IfLessEnd: body runs when x < y.
    Less,
    /// Inst_IfNotEqualEnd: body runs when x != y.
    NotEqual,
    /// Inst_IfGrtEquEnd: body runs when x >= y.
    GreaterEqual,
}

/// Inst_IfLessEnd / Inst_IfNotEqualEnd / Inst_IfGrtEquEnd.
pub fn if_end<M: Machine>(m: &mut M, cmp: IfEnd, x_reg: c_int, y_reg: c_int) {
    let x = m.reg(x_reg);
    let y = m.reg(y_reg);
    let holds = match cmp {
        IfEnd::Less => x < y,
        IfEnd::NotEqual => x != y,
        IfEnd::GreaterEqual => x >= y,
    };
    if !holds {
        else_top_half(m);
    }
}

/// Inst_Else: reached only by falling out of an if-body, so jump to the
/// `endif`, or skip one instruction when there is none.
pub fn else_<M: Machine>(m: &mut M) {
    let endif_op = m.find_inst_op(c"endif");
    let endif_pos = find_from_ip(m, endif_op);
    if endif_pos != m.head_pos(HEAD_IP) {
        m.set_head_pos(HEAD_IP, endif_pos);
    } else {
        m.advance_head(HEAD_IP);
    }
}

// ---------------------------------------------------------------------------
// FFI-backed machine
// ---------------------------------------------------------------------------

unsafe extern "C" {
    fn avd_hw_get_memory_size(hw: *mut c_void) -> c_int;
    fn avd_hw_get_memory_inst(hw: *mut c_void, idx: c_int) -> c_int;
    fn avd_hw_set_memory_inst(hw: *mut c_void, idx: c_int, inst_id: c_int);
    fn avd_hw_memory_set_flags(hw: *mut c_void, idx: c_int, mask: c_int);
    fn avd_hw_memory_clear_flags(hw: *mut c_void, idx: c_int, mask: c_int);
    fn avd_hw_memory_insert(hw: *mut c_void, idx: c_int, inst_id: c_int);
    fn avd_hw_memory_remove(hw: *mut c_void, idx: c_int);
    fn avd_hw_get_head_position(hw: *mut c_void, head_id: c_int) -> c_int;
    fn avd_hw_set_head_position(hw: *mut c_void, head_id: c_int, pos: c_int);
    fn avd_hw_adjust_head(hw: *mut c_void, head_id: c_int) -> c_int;
    fn avd_hw_advance_head(hw: *mut c_void, head_id: c_int);
    fn avd_hw_get_cur_head(hw: *mut c_void) -> c_int;
    fn avd_hw_find_modified_head(hw: *mut c_void, default_head: c_int) -> c_int;
    fn avd_hw_get_register(hw: *mut c_void, reg_id: c_int) -> c_int;
    fn avd_hw_set_register(hw: *mut c_void, reg_id: c_int, value: c_int);
    fn avd_hw_stack_push(hw: *mut c_void, value: c_int);
    fn avd_hw_stack_pop(hw: *mut c_void) -> c_int;
    fn avd_hw_read_label(hw: *mut c_void);
    fn avd_hw_get_label_size(hw: *mut c_void) -> c_int;
    fn avd_hw_get_label_nop(hw: *mut c_void, idx: c_int) -> c_int;
    fn avd_hw_search_label(hw: *mut c_void, direction: c_int) -> c_int;
    fn avd_hw_is_next_inst_nop(hw: *mut c_void) -> c_int;
    fn avd_hw_is_nop(hw: *mut c_void, inst_id: c_int) -> c_int;
    fn avd_hw_get_nop_mod(hw: *mut c_void, inst_id: c_int) -> c_int;
    fn avd_hw_get_inst_set_size(hw: *mut c_void) -> c_int;
    fn avd_hw_get_random_inst(hw: *mut c_void, ctx: *mut c_void) -> c_int;
    fn avd_hw_check_no_mut_list(hw: *mut c_void, idx: c_int) -> c_int;
    fn avd_hw_read_inst(hw: *mut c_void, inst_id: c_int);
    fn avd_hw_get_organism(hw: *mut c_void) -> *mut c_void;
    fn avd_org_test_copy(org: *mut c_void, ctx: *mut c_void, kind: c_int) -> c_int;
    fn avd_org_get_copy_prob(org: *mut c_void, kind: c_int) -> f64;
    fn avd_ctx_random_p(ctx: *mut c_void, prob: f64) -> c_int;
    fn avd_ctx_random_get_int(ctx: *mut c_void, max: c_int) -> c_int;
    fn avd_hw_do_uniform_copy_mutation(hw: *mut c_void, ctx: *mut c_void, head_id: c_int);
    fn avd_hw_do_slip_mutation(hw: *mut c_void, ctx: *mut c_void, pos: c_int);
    fn avd_hw_get_slip_read_head(hw: *mut c_void) -> c_int;
    fn avd_hw_fork_thread(hw: *mut c_void) -> c_int;
    fn avd_hw_kill_thread(hw: *mut c_void) -> c_int;
    fn avd_hw_set_advance_ip(hw: *mut c_void, advance: c_int);
    fn avd_hw_fault(hw: *mut c_void, loc: c_int, fault_type: c_int, desc: *const c_char);
    fn avd_hw_allocate_main(hw: *mut c_void, ctx: *mut c_void, size: c_int) -> c_int;
    fn avd_hw_divide_main(
        hw: *mut c_void,
        ctx: *mut c_void,
        div_point: c_int,
        extra_lines: c_int,
        mut_multiplier: f64,
        kind: c_int,
    ) -> c_int;
    fn avd_hw_adjust_heads(hw: *mut c_void);
    fn avd_hw_find_inst_op(hw: *mut c_void, name: *const c_char) -> c_int;
    fn avd_hw_get_label_as_int(hw: *mut c_void, mode: c_int) -> c_int;
    fn avd_org_set_divide_sex(org: *mut c_void, divide_sex: c_int);
    fn avd_org_set_cross_num(org: *mut c_void, cross_num: c_int);
    fn avd_org_set_mate_select_id(org: *mut c_void, select_id: c_int);
    fn avd_org_born_parent_group(org: *mut c_void) -> c_int;
    fn avd_org_get_rbin(org: *mut c_void, index: c_int) -> f64;
    fn avd_org_add_to_rbin(org: *mut c_void, index: c_int, amount: f64);
}

/// [`Machine`] backed by a live `cHardwareCPU` through `cHardwareFFI.cc`.
///
/// Invariant: `hw` points to the `cHardwareCPU` currently executing an
/// instruction, and `ctx` is either the `cAvidaContext` of that execution or
/// null for handlers that never touch the RNG, mutations, allocation or
/// division. The C++ shims null-check both pointers.
struct FfiMachine {
    hw: *mut c_void,
    ctx: *mut c_void,
    /// Organism owning `hw`, looked up on first use. An organism never changes
    /// hardware mid-instruction, so caching it saves an FFI call per mutation
    /// test on the `h-copy` hot path.
    org: Option<*mut c_void>,
}

impl FfiMachine {
    fn org(&mut self) -> *mut c_void {
        let hw = self.hw;
        // SAFETY: `hw` is a live hardware pointer (struct invariant).
        *self
            .org
            .get_or_insert_with(|| unsafe { avd_hw_get_organism(hw) })
    }
}

// SAFETY (whole impl): every call forwards `self.hw` / `self.ctx`, which the
// struct invariant guarantees are valid for the duration of the handler, to a
// C++ shim that performs its own null and bounds checks.
#[allow(clippy::undocumented_unsafe_blocks)]
impl Machine for FfiMachine {
    fn mem_size(&mut self) -> c_int {
        unsafe { avd_hw_get_memory_size(self.hw) }
    }
    fn mem_inst(&mut self, pos: c_int) -> c_int {
        unsafe { avd_hw_get_memory_inst(self.hw, pos) }
    }
    fn set_mem_inst(&mut self, pos: c_int, op: c_int) {
        unsafe { avd_hw_set_memory_inst(self.hw, pos, op) }
    }
    fn mem_set_flags(&mut self, pos: c_int, mask: c_int) {
        unsafe { avd_hw_memory_set_flags(self.hw, pos, mask) }
    }
    fn mem_clear_flags(&mut self, pos: c_int, mask: c_int) {
        unsafe { avd_hw_memory_clear_flags(self.hw, pos, mask) }
    }
    fn mem_insert(&mut self, pos: c_int, op: c_int) {
        unsafe { avd_hw_memory_insert(self.hw, pos, op) }
    }
    fn mem_remove(&mut self, pos: c_int) {
        unsafe { avd_hw_memory_remove(self.hw, pos) }
    }
    fn head_pos(&mut self, head: c_int) -> c_int {
        unsafe { avd_hw_get_head_position(self.hw, head) }
    }
    fn set_head_pos(&mut self, head: c_int, pos: c_int) {
        unsafe { avd_hw_set_head_position(self.hw, head, pos) }
    }
    // Single-call overrides of the default get-then-set implementations.
    fn adjust_head(&mut self, head: c_int) -> c_int {
        unsafe { avd_hw_adjust_head(self.hw, head) }
    }
    fn advance_head(&mut self, head: c_int) {
        unsafe { avd_hw_advance_head(self.hw, head) }
    }
    fn cur_head(&mut self) -> c_int {
        unsafe { avd_hw_get_cur_head(self.hw) }
    }
    fn find_modified_head(&mut self, default_head: c_int) -> c_int {
        unsafe { avd_hw_find_modified_head(self.hw, default_head) }
    }
    fn reg(&mut self, reg: c_int) -> c_int {
        unsafe { avd_hw_get_register(self.hw, reg) }
    }
    fn set_reg(&mut self, reg: c_int, value: c_int) {
        unsafe { avd_hw_set_register(self.hw, reg, value) }
    }
    fn stack_push(&mut self, value: c_int) {
        unsafe { avd_hw_stack_push(self.hw, value) }
    }
    fn stack_pop(&mut self) -> c_int {
        unsafe { avd_hw_stack_pop(self.hw) }
    }
    fn read_label(&mut self) {
        unsafe { avd_hw_read_label(self.hw) }
    }
    fn label_size(&mut self) -> c_int {
        unsafe { avd_hw_get_label_size(self.hw) }
    }
    fn label_nop(&mut self, idx: c_int) -> c_int {
        unsafe { avd_hw_get_label_nop(self.hw, idx) }
    }
    fn search_complement_label(&mut self, direction: c_int) -> c_int {
        unsafe { avd_hw_search_label(self.hw, direction) }
    }
    fn next_inst_is_nop(&mut self) -> bool {
        unsafe { avd_hw_is_next_inst_nop(self.hw) != 0 }
    }
    fn is_nop(&mut self, op: c_int) -> bool {
        unsafe { avd_hw_is_nop(self.hw, op) != 0 }
    }
    fn nop_mod(&mut self, op: c_int) -> c_int {
        unsafe { avd_hw_get_nop_mod(self.hw, op) }
    }
    fn inst_set_size(&mut self) -> c_int {
        unsafe { avd_hw_get_inst_set_size(self.hw) }
    }
    fn random_inst(&mut self) -> c_int {
        unsafe { avd_hw_get_random_inst(self.hw, self.ctx) }
    }
    fn check_no_mut_list(&mut self, pos: c_int) -> bool {
        unsafe { avd_hw_check_no_mut_list(self.hw, pos) != 0 }
    }
    fn read_inst(&mut self, op: c_int) {
        unsafe { avd_hw_read_inst(self.hw, op) }
    }
    fn test_copy(&mut self, kind: CopyMut) -> bool {
        unsafe { avd_org_test_copy(self.org(), self.ctx, kind as c_int) != 0 }
    }
    fn copy_prob(&mut self, kind: CopyMut) -> f64 {
        unsafe { avd_org_get_copy_prob(self.org(), kind as c_int) }
    }
    fn random_p(&mut self, prob: f64) -> bool {
        unsafe { avd_ctx_random_p(self.ctx, prob) != 0 }
    }
    fn random_int(&mut self, max: c_int) -> c_int {
        unsafe { avd_ctx_random_get_int(self.ctx, max) }
    }
    fn uniform_copy_mutation(&mut self, head: c_int) {
        unsafe { avd_hw_do_uniform_copy_mutation(self.hw, self.ctx, head) }
    }
    fn slip_mutation(&mut self, pos: c_int) {
        unsafe { avd_hw_do_slip_mutation(self.hw, self.ctx, pos) }
    }
    fn slip_read_head(&mut self) -> bool {
        unsafe { avd_hw_get_slip_read_head(self.hw) != 0 }
    }
    fn fork_thread(&mut self) -> bool {
        unsafe { avd_hw_fork_thread(self.hw) != 0 }
    }
    fn kill_thread(&mut self) -> bool {
        unsafe { avd_hw_kill_thread(self.hw) != 0 }
    }
    fn set_advance_ip(&mut self, advance: bool) {
        unsafe { avd_hw_set_advance_ip(self.hw, c_int::from(advance)) }
    }
    fn fault(&mut self, loc: c_int, fault_type: c_int, desc: &CStr) {
        unsafe { avd_hw_fault(self.hw, loc, fault_type, desc.as_ptr()) }
    }
    fn allocate_main(&mut self, size: c_int) -> bool {
        unsafe { avd_hw_allocate_main(self.hw, self.ctx, size) != 0 }
    }
    fn divide_main(
        &mut self,
        div_point: c_int,
        extra_lines: c_int,
        mut_multiplier: f64,
        kind: DivideKind,
    ) -> bool {
        unsafe {
            avd_hw_divide_main(
                self.hw,
                self.ctx,
                div_point,
                extra_lines,
                mut_multiplier,
                kind as c_int,
            ) != 0
        }
    }
    fn adjust_all_heads(&mut self) {
        unsafe { avd_hw_adjust_heads(self.hw) }
    }
    fn find_inst_op(&mut self, name: &CStr) -> c_int {
        unsafe { avd_hw_find_inst_op(self.hw, name.as_ptr()) }
    }
    fn label_as_int(&mut self) -> c_int {
        unsafe { avd_hw_get_label_as_int(self.hw, 0) }
    }
    fn set_divide_sex(&mut self, divide_sex: bool) {
        unsafe { avd_org_set_divide_sex(self.org(), c_int::from(divide_sex)) }
    }
    fn set_cross_num(&mut self, cross_num: c_int) {
        unsafe { avd_org_set_cross_num(self.org(), cross_num) }
    }
    fn set_mate_select_id(&mut self, select_id: c_int) {
        unsafe { avd_org_set_mate_select_id(self.org(), select_id) }
    }
    fn born_parent_group(&mut self) -> bool {
        unsafe { avd_org_born_parent_group(self.org()) != 0 }
    }
    fn rbin(&mut self, idx: c_int) -> f64 {
        unsafe { avd_org_get_rbin(self.org(), idx) }
    }
    fn add_to_rbin(&mut self, idx: c_int, amount: f64) {
        unsafe { avd_org_add_to_rbin(self.org(), idx, amount) }
    }
}

fn machine(hw: *mut c_void, ctx: *mut c_void) -> FfiMachine {
    FfiMachine { hw, ctx, org: None }
}

// ---------------------------------------------------------------------------
// Exported entry points (called from cHardwareCPU.cc)
// ---------------------------------------------------------------------------
// Safety contract for all exports: `hw` must be the live `cHardwareCPU`
// executing the instruction and `ctx` (where taken) its `cAvidaContext`.

/// Inst_IfGrX (`greater != 0`) / Inst_IfEquX. Returns 1 to skip the next instruction.
#[no_mangle]
pub unsafe extern "C" fn avd_cpu_inst_if_x(hw: *mut c_void, greater: c_int) -> c_int {
    c_int::from(if_x(&mut machine(hw, std::ptr::null_mut()), greater != 0))
}

/// Inst_JumpF (`direction = 1`) / Inst_JumpB (`direction = -1`). Returns the handler result.
#[no_mangle]
pub unsafe extern "C" fn avd_cpu_inst_jump_label(hw: *mut c_void, direction: c_int) -> c_int {
    c_int::from(jump_label(
        &mut machine(hw, std::ptr::null_mut()),
        direction,
    ))
}

/// Inst_Call. Returns the handler result.
#[no_mangle]
pub unsafe extern "C" fn avd_cpu_inst_call(hw: *mut c_void) -> c_int {
    c_int::from(call(&mut machine(hw, std::ptr::null_mut())))
}

/// Inst_Throw. `catch_op` is the opcode of `catch`. Returns the handler result.
#[no_mangle]
pub unsafe extern "C" fn avd_cpu_inst_throw(
    hw: *mut c_void,
    catch_op: c_int,
    cond: c_int,
) -> c_int {
    let Some(cond) = Condition::from_code(cond) else {
        return 0;
    };
    c_int::from(throw(
        &mut machine(hw, std::ptr::null_mut()),
        catch_op,
        cond,
    ))
}

/// Inst_Goto. `label_op` is the opcode of `label`. Returns the handler result.
#[no_mangle]
pub unsafe extern "C" fn avd_cpu_inst_goto(hw: *mut c_void, label_op: c_int, cond: c_int) -> c_int {
    let Some(cond) = Condition::from_code(cond) else {
        return 0;
    };
    c_int::from(goto(&mut machine(hw, std::ptr::null_mut()), label_op, cond))
}

/// Inst_Copy.
#[no_mangle]
pub unsafe extern "C" fn avd_cpu_inst_copy(hw: *mut c_void, ctx: *mut c_void) {
    copy(&mut machine(hw, ctx));
}

/// Inst_WriteInst. `src_reg` is the nop-modified source register.
#[no_mangle]
pub unsafe extern "C" fn avd_cpu_inst_write_inst(
    hw: *mut c_void,
    ctx: *mut c_void,
    src_reg: c_int,
) {
    write_inst(&mut machine(hw, ctx), src_reg);
}

/// Inst_StackWriteInst. `dst_reg` is the nop-modified offset register.
#[no_mangle]
pub unsafe extern "C" fn avd_cpu_inst_stack_write_inst(
    hw: *mut c_void,
    ctx: *mut c_void,
    dst_reg: c_int,
) {
    stack_write_inst(&mut machine(hw, ctx), dst_reg);
}

/// Inst_Compare. `dst_reg` is the nop-modified result register.
#[no_mangle]
pub unsafe extern "C" fn avd_cpu_inst_compare(hw: *mut c_void, ctx: *mut c_void, dst_reg: c_int) {
    compare(&mut machine(hw, ctx), dst_reg);
}

/// Inst_IfNCpy. Returns 1 to skip the next instruction.
#[no_mangle]
pub unsafe extern "C" fn avd_cpu_inst_if_n_cpy(hw: *mut c_void, ctx: *mut c_void) -> c_int {
    c_int::from(if_n_cpy(&mut machine(hw, ctx)))
}

/// Inst_HeadRead.
#[no_mangle]
pub unsafe extern "C" fn avd_cpu_inst_head_read(hw: *mut c_void, ctx: *mut c_void) {
    head_read(&mut machine(hw, ctx));
}

/// Inst_HeadWrite.
#[no_mangle]
pub unsafe extern "C" fn avd_cpu_inst_head_write(hw: *mut c_void, ctx: *mut c_void) {
    head_write(&mut machine(hw, ctx));
}

/// Inst_HeadCopy (`reduction <= 0`) and HeadCopy_ErrorCorrect (`reduction > 0`).
#[no_mangle]
pub unsafe extern "C" fn avd_cpu_inst_head_copy(hw: *mut c_void, ctx: *mut c_void, reduction: f64) {
    let reduction = (reduction > 0.0).then_some(reduction);
    head_copy(&mut machine(hw, ctx), reduction);
}

/// Inst_HeadCopy_ifResource.
#[no_mangle]
pub unsafe extern "C" fn avd_cpu_inst_head_copy_if_resource(hw: *mut c_void, ctx: *mut c_void) {
    head_copy_if_resource(&mut machine(hw, ctx));
}

/// Inst_ForkThread.
#[no_mangle]
pub unsafe extern "C" fn avd_cpu_inst_fork_thread(hw: *mut c_void) {
    fork_thread(&mut machine(hw, std::ptr::null_mut()));
}

/// Inst_ForkThreadLabel and its `=0` / `!=0` variants. Returns the handler result.
#[no_mangle]
pub unsafe extern "C" fn avd_cpu_inst_fork_thread_label(hw: *mut c_void, cond: c_int) -> c_int {
    let Some(cond) = Condition::from_code(cond) else {
        return 0;
    };
    c_int::from(fork_thread_label(
        &mut machine(hw, std::ptr::null_mut()),
        cond,
    ))
}

/// Inst_KillThread.
#[no_mangle]
pub unsafe extern "C" fn avd_cpu_inst_kill_thread(hw: *mut c_void) {
    kill_thread(&mut machine(hw, std::ptr::null_mut()));
}

/// Inst_Allocate. Returns the handler result.
#[no_mangle]
pub unsafe extern "C" fn avd_cpu_inst_allocate(hw: *mut c_void, ctx: *mut c_void) -> c_int {
    c_int::from(allocate(&mut machine(hw, ctx)))
}

/// Inst_MaxAlloc / Inst_MaxAllocMoveWriteHead. Returns the handler result.
#[no_mangle]
pub unsafe extern "C" fn avd_cpu_inst_max_alloc(
    hw: *mut c_void,
    ctx: *mut c_void,
    offspring_size_range: f64,
    max_genome_length: c_int,
    move_write_head: c_int,
) -> c_int {
    c_int::from(max_alloc(
        &mut machine(hw, ctx),
        offspring_size_range,
        max_genome_length,
        move_write_head != 0,
    ))
}

/// Inst_Divide (0), Inst_DivideRS (1), Inst_CDivide (2), Inst_CAlloc (3).
/// Returns the handler result; unknown modes return 0.
#[no_mangle]
pub unsafe extern "C" fn avd_cpu_inst_divide(
    hw: *mut c_void,
    ctx: *mut c_void,
    mode: c_int,
) -> c_int {
    let mode = match mode {
        0 => DivideMode::AtAx,
        1 => DivideMode::AtAxResample,
        2 => DivideMode::Half,
        3 => DivideMode::DoubleAlloc,
        _ => return 0,
    };
    c_int::from(divide(&mut machine(hw, ctx), mode))
}

/// Inst_HeadDivide family. `kind`: 0 plain, 1 RS, 2 1RS, 3 2RS.
/// Returns the handler result; unknown kinds return 0.
#[no_mangle]
pub unsafe extern "C" fn avd_cpu_inst_head_divide(
    hw: *mut c_void,
    ctx: *mut c_void,
    kind: c_int,
    mut_multiplier: f64,
    tolerance_window: c_int,
) -> c_int {
    let kind = match kind {
        0 => DivideKind::Plain,
        1 => DivideKind::Resample,
        2 => DivideKind::Resample1,
        3 => DivideKind::Resample2,
        _ => return 0,
    };
    c_int::from(head_divide(
        &mut machine(hw, ctx),
        kind,
        mut_multiplier,
        tolerance_window != 0,
    ))
}

/// Inst_HeadDivideSex (0), Asex (1), AsexWait (2), MateSelect (3).
/// Returns the handler result; unknown modes return 0.
#[no_mangle]
pub unsafe extern "C" fn avd_cpu_inst_head_divide_sex(
    hw: *mut c_void,
    ctx: *mut c_void,
    mode: c_int,
    tolerance_window: c_int,
) -> c_int {
    let mode = match mode {
        0 => SexMode::Sex,
        1 => SexMode::Asex,
        2 => SexMode::AsexWait,
        3 => SexMode::MateSelect,
        _ => return 0,
    };
    c_int::from(head_divide_sex(
        &mut machine(hw, ctx),
        mode,
        tolerance_window != 0,
    ))
}

/// Inst_IfLessEnd (0), Inst_IfNotEqualEnd (1), Inst_IfGrtEquEnd (2).
/// `x_reg` / `y_reg` are the nop-modified register and its successor.
#[no_mangle]
pub unsafe extern "C" fn avd_cpu_inst_if_end(
    hw: *mut c_void,
    cmp: c_int,
    x_reg: c_int,
    y_reg: c_int,
) {
    let cmp = match cmp {
        0 => IfEnd::Less,
        1 => IfEnd::NotEqual,
        2 => IfEnd::GreaterEqual,
        _ => return,
    };
    if_end(&mut machine(hw, std::ptr::null_mut()), cmp, x_reg, y_reg);
}

/// Inst_Else.
#[no_mangle]
pub unsafe extern "C" fn avd_cpu_inst_else(hw: *mut c_void) {
    else_(&mut machine(hw, std::ptr::null_mut()));
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::VecDeque;

    // Mock instruction set: opcodes 0..=2 are nop-A/B/C with matching nop mods.
    const NOP_A: c_int = 0;
    const NOP_B: c_int = 1;
    const NOP_C: c_int = 2;
    const INST_SET_SIZE: c_int = 26;
    const CATCH: c_int = 20;
    const LABEL: c_int = 21;
    const OTHER: c_int = 10;
    const ELSE: c_int = 22;
    const ENDIF: c_int = 23;
    const ERROR_OP: c_int = 255;

    #[derive(Default)]
    struct Mock {
        mem: Vec<c_int>,
        flags: Vec<c_int>,
        heads: [c_int; 4],
        cur_head: c_int,
        regs: [c_int; 3],
        stack: Vec<c_int>,
        label: Vec<c_int>,
        read_label: Vec<c_int>,
        /// Scripted results for `test_copy`, keyed by kind; missing → false.
        copy_tests: Vec<(CopyMut, bool)>,
        copy_test_log: Vec<CopyMut>,
        copy_probs: [f64; 5],
        random_p_log: Vec<f64>,
        random_p_results: VecDeque<bool>,
        random_insts: VecDeque<c_int>,
        random_ints: VecDeque<c_int>,
        no_mut_ops: Vec<c_int>,
        slip_read_head: bool,
        uniform_calls: Vec<c_int>,
        slip_calls: Vec<c_int>,
        fork_ok: bool,
        fork_ips: Vec<c_int>,
        kill_ok: bool,
        advance_ip: Option<bool>,
        faults: Vec<(c_int, c_int, String)>,
        alloc_ok: bool,
        alloc_calls: Vec<c_int>,
        divide_ok: bool,
        divide_calls: Vec<(c_int, c_int, f64, DivideKind)>,
        adjust_all_calls: usize,
        has_structured_if: bool,
        divide_sex: Option<bool>,
        cross_num: Option<c_int>,
        mate_select_id: Option<c_int>,
        born_parent_group: bool,
        rbins: Vec<f64>,
        complement_label_pos: c_int,
        search_dirs: Vec<c_int>,
    }

    impl Mock {
        fn new(mem: &[c_int]) -> Self {
            Mock {
                mem: mem.to_vec(),
                flags: vec![0; mem.len()],
                complement_label_pos: -1,
                fork_ok: true,
                kill_ok: true,
                alloc_ok: true,
                divide_ok: true,
                has_structured_if: true,
                ..Default::default()
            }
        }
        fn fire(mut self, kind: CopyMut) -> Self {
            self.copy_tests.push((kind, true));
            self
        }
        fn idx(pos: c_int) -> usize {
            usize::try_from(pos).unwrap_or(usize::MAX)
        }
    }

    impl Machine for Mock {
        fn mem_size(&mut self) -> c_int {
            self.mem.len() as c_int
        }
        fn mem_inst(&mut self, pos: c_int) -> c_int {
            self.mem.get(Self::idx(pos)).copied().unwrap_or(-1)
        }
        fn set_mem_inst(&mut self, pos: c_int, op: c_int) {
            if let Some(slot) = self.mem.get_mut(Self::idx(pos)) {
                *slot = op;
            }
        }
        fn mem_set_flags(&mut self, pos: c_int, mask: c_int) {
            if let Some(slot) = self.flags.get_mut(Self::idx(pos)) {
                *slot |= mask;
            }
        }
        fn mem_clear_flags(&mut self, pos: c_int, mask: c_int) {
            if let Some(slot) = self.flags.get_mut(Self::idx(pos)) {
                *slot &= !mask;
            }
        }
        fn mem_insert(&mut self, pos: c_int, op: c_int) {
            self.mem.insert(Self::idx(pos), op);
            self.flags.insert(Self::idx(pos), 0);
        }
        fn mem_remove(&mut self, pos: c_int) {
            self.mem.remove(Self::idx(pos));
            self.flags.remove(Self::idx(pos));
        }
        fn head_pos(&mut self, head: c_int) -> c_int {
            self.heads[Self::idx(head)]
        }
        fn set_head_pos(&mut self, head: c_int, pos: c_int) {
            let size = self.mem_size();
            self.heads[Self::idx(head)] = adjust_position(pos, size);
        }
        fn cur_head(&mut self) -> c_int {
            self.cur_head
        }
        fn find_modified_head(&mut self, default_head: c_int) -> c_int {
            default_head
        }
        fn reg(&mut self, reg: c_int) -> c_int {
            self.regs[Self::idx(reg)]
        }
        fn set_reg(&mut self, reg: c_int, value: c_int) {
            self.regs[Self::idx(reg)] = value;
        }
        fn stack_push(&mut self, value: c_int) {
            self.stack.push(value);
        }
        fn stack_pop(&mut self) -> c_int {
            self.stack.pop().unwrap_or(0)
        }
        fn read_label(&mut self) {
            // Like ReadLabel: consume the nops following the IP.
            self.label.clear();
            loop {
                let ip = self.heads[0];
                let next = ip + 1;
                if next >= self.mem_size() || !self.is_nop(self.mem[Self::idx(next)]) {
                    break;
                }
                self.label.push(self.mem[Self::idx(next)]);
                self.heads[0] = next;
            }
        }
        fn label_size(&mut self) -> c_int {
            self.label.len() as c_int
        }
        fn label_nop(&mut self, idx: c_int) -> c_int {
            self.label.get(Self::idx(idx)).copied().unwrap_or(-1)
        }
        fn search_complement_label(&mut self, direction: c_int) -> c_int {
            self.search_dirs.push(direction);
            self.complement_label_pos
        }
        fn next_inst_is_nop(&mut self) -> bool {
            let next = self.heads[0] + 1;
            next < self.mem_size() && self.is_nop(self.mem[Self::idx(next)])
        }
        fn is_nop(&mut self, op: c_int) -> bool {
            (NOP_A..=NOP_C).contains(&op)
        }
        fn nop_mod(&mut self, op: c_int) -> c_int {
            op
        }
        fn inst_set_size(&mut self) -> c_int {
            INST_SET_SIZE
        }
        fn random_inst(&mut self) -> c_int {
            self.random_insts.pop_front().unwrap_or(25)
        }
        fn check_no_mut_list(&mut self, pos: c_int) -> bool {
            let op = self.mem_inst(pos);
            self.no_mut_ops.contains(&op)
        }
        fn read_inst(&mut self, op: c_int) {
            if self.is_nop(op) {
                self.read_label.push(op);
            } else {
                self.read_label.clear();
            }
        }
        fn test_copy(&mut self, kind: CopyMut) -> bool {
            self.copy_test_log.push(kind);
            self.copy_tests.iter().any(|&(k, fire)| k == kind && fire)
        }
        fn copy_prob(&mut self, kind: CopyMut) -> f64 {
            self.copy_probs[kind as usize]
        }
        fn random_p(&mut self, prob: f64) -> bool {
            self.random_p_log.push(prob);
            self.random_p_results.pop_front().unwrap_or(false)
        }
        fn random_int(&mut self, _max: c_int) -> c_int {
            self.random_ints.pop_front().unwrap_or(0)
        }
        fn uniform_copy_mutation(&mut self, head: c_int) {
            self.uniform_calls.push(head);
        }
        fn slip_mutation(&mut self, pos: c_int) {
            self.slip_calls.push(pos);
        }
        fn slip_read_head(&mut self) -> bool {
            self.slip_read_head
        }
        fn fork_thread(&mut self) -> bool {
            self.fork_ips.push(self.heads[0]);
            self.fork_ok
        }
        fn kill_thread(&mut self) -> bool {
            self.kill_ok
        }
        fn set_advance_ip(&mut self, advance: bool) {
            self.advance_ip = Some(advance);
        }
        fn fault(&mut self, loc: c_int, fault_type: c_int, desc: &CStr) {
            self.faults
                .push((loc, fault_type, desc.to_string_lossy().into_owned()));
        }
        fn allocate_main(&mut self, size: c_int) -> bool {
            self.alloc_calls.push(size);
            self.alloc_ok
        }
        fn divide_main(
            &mut self,
            div_point: c_int,
            extra_lines: c_int,
            mut_multiplier: f64,
            kind: DivideKind,
        ) -> bool {
            self.divide_calls
                .push((div_point, extra_lines, mut_multiplier, kind));
            self.divide_ok
        }
        fn adjust_all_heads(&mut self) {
            self.adjust_all_calls += 1;
            let size = self.mem_size();
            for head in &mut self.heads {
                *head = adjust_position(*head, size);
            }
        }
        fn find_inst_op(&mut self, name: &CStr) -> c_int {
            match name.to_bytes() {
                b"else" if self.has_structured_if => ELSE,
                b"endif" if self.has_structured_if => ENDIF,
                _ => ERROR_OP,
            }
        }
        fn label_as_int(&mut self) -> c_int {
            // Base-3 reading of the label, like cCodeLabel::AsInt(3).
            self.label.iter().fold(0, |acc, nop| acc * 3 + nop)
        }
        fn set_divide_sex(&mut self, divide_sex: bool) {
            self.divide_sex = Some(divide_sex);
        }
        fn set_cross_num(&mut self, cross_num: c_int) {
            self.cross_num = Some(cross_num);
        }
        fn set_mate_select_id(&mut self, select_id: c_int) {
            self.mate_select_id = Some(select_id);
        }
        fn born_parent_group(&mut self) -> bool {
            self.born_parent_group
        }
        fn rbin(&mut self, idx: c_int) -> f64 {
            self.rbins.get(Self::idx(idx)).copied().unwrap_or(0.0)
        }
        fn add_to_rbin(&mut self, idx: c_int, amount: f64) {
            if let Some(slot) = self.rbins.get_mut(Self::idx(idx)) {
                *slot += amount;
            }
        }
    }

    #[test]
    fn adjust_position_matches_full_adjust() {
        assert_eq!(adjust_position(0, 10), 0);
        assert_eq!(adjust_position(9, 10), 9);
        assert_eq!(adjust_position(10, 10), 0);
        assert_eq!(adjust_position(19, 10), 9);
        assert_eq!(adjust_position(20, 10), 0);
        assert_eq!(adjust_position(47, 10), 7);
        // Backward motion does not wrap: it clamps to the start.
        assert_eq!(adjust_position(-1, 10), 0);
        assert_eq!(adjust_position(-37, 10), 0);
        assert_eq!(adjust_position(5, 0), 0);
        assert_eq!(adjust_position(c_int::MAX, 10), c_int::MAX % 10);
    }

    #[test]
    fn modulo_normalises_negative_values() {
        assert_eq!(modulo(27, 26), 1);
        assert_eq!(modulo(-1, 26), 25);
        assert_eq!(modulo(0, 26), 0);
        assert_eq!(modulo(5, 0), 0);
        assert_eq!(modulo(c_int::MIN, -1), 0);
    }

    #[test]
    fn if_x_uses_default_and_nop_selected_constants() {
        // No nop: compare against 1.
        let mut m = Mock::new(&[OTHER, OTHER]);
        m.regs[1] = 2;
        assert!(!if_x(&mut m, true));
        m.regs[1] = 1;
        assert!(if_x(&mut m, true));
        assert!(!if_x(&mut m, false));
        assert_eq!(m.heads[0], 0);

        // nop-A: -1; nop-B: 2; nop-C: 4. The nop is consumed and marked executed.
        for (nop, value) in [(NOP_A, -1), (NOP_B, 2), (NOP_C, 4)] {
            let mut m = Mock::new(&[OTHER, nop, OTHER]);
            m.regs[1] = value;
            assert!(!if_x(&mut m, false));
            assert_eq!(m.heads[0], 1);
            assert_eq!(m.flags[1], FLAG_EXECUTED);

            let mut m = Mock::new(&[OTHER, nop, OTHER]);
            m.regs[1] = value + 1;
            assert!(!if_x(&mut m, true));
            assert!(if_x(&mut Mock::new(&[OTHER, nop, OTHER]), false));
        }
    }

    #[test]
    fn jump_without_label_moves_active_head_by_bx() {
        let mut m = Mock::new(&[OTHER; 8]);
        m.cur_head = 3;
        m.heads[3] = 6;
        m.regs[1] = 5;
        assert!(jump_label(&mut m, 1));
        assert_eq!(m.heads[3], 3); // 11 wraps to 3
        assert!(m.search_dirs.is_empty());
        assert!(m.faults.is_empty());
    }

    #[test]
    fn jump_with_label_sets_head_or_faults() {
        let mut m = Mock::new(&[OTHER, NOP_A, OTHER, OTHER]);
        m.complement_label_pos = 3;
        assert!(jump_label(&mut m, -1));
        assert_eq!(m.heads[0], 3);
        assert_eq!(m.search_dirs, vec![-1]);

        for (direction, text) in [
            (1, "jump-f: No complement label"),
            (-1, "jump-b: No complement label"),
        ] {
            let mut m = Mock::new(&[OTHER, NOP_A, OTHER, OTHER]);
            assert!(!jump_label(&mut m, direction));
            assert_eq!(
                m.faults,
                vec![(FAULT_LOC_JUMP, FAULT_TYPE_ERROR, text.to_string())]
            );
            assert_eq!(m.heads[0], 1); // only the label was consumed
        }
    }

    #[test]
    fn call_pushes_return_address_before_reading_label() {
        let mut m = Mock::new(&[OTHER, OTHER, NOP_B, OTHER, OTHER]);
        m.heads[0] = 1;
        m.complement_label_pos = 4;
        assert!(call(&mut m));
        assert_eq!(m.stack, vec![1]);
        assert_eq!(m.heads[0], 4);

        let mut m = Mock::new(&[OTHER; 5]);
        m.heads[0] = 1;
        m.regs[1] = 2;
        assert!(call(&mut m));
        assert_eq!(m.stack, vec![1]);
        assert_eq!(m.heads[0], 3);

        let mut m = Mock::new(&[OTHER, NOP_B, OTHER]);
        assert!(!call(&mut m));
        assert_eq!(m.faults.len(), 1);
        assert_eq!(m.faults[0].2, "call: no complement label");
    }

    #[test]
    fn throw_finds_catch_with_matching_prefix() {
        // throw nop-A nop-B ... catch nop-C (mismatch) ... catch nop-A (prefix ok)
        let mem = [
            OTHER, NOP_A, NOP_B, OTHER, CATCH, NOP_C, OTHER, CATCH, NOP_A, OTHER,
        ];
        let mut m = Mock::new(&mem);
        assert!(throw(&mut m, CATCH, Condition::Always));
        assert_eq!(m.heads[0], 7);
        assert_eq!(m.advance_ip, Some(false));
    }

    #[test]
    fn throw_accepts_bare_catch_and_wraps_around() {
        // The catch sits before the throw; the search wraps past the end.
        let mem = [CATCH, OTHER, OTHER, OTHER, NOP_A, OTHER];
        let mut m = Mock::new(&mem);
        m.heads[0] = 3;
        assert!(throw(&mut m, CATCH, Condition::Always));
        assert_eq!(m.heads[0], 0);
    }

    #[test]
    fn throw_without_catch_fails_and_leaves_advance_flag() {
        let mut m = Mock::new(&[OTHER, NOP_A, OTHER, CATCH, NOP_B, OTHER]);
        assert!(!throw(&mut m, CATCH, Condition::Always));
        assert_eq!(m.advance_ip, None);
        assert_eq!(m.heads[0], 1);
    }

    #[test]
    fn goto_requires_exact_label_match() {
        // goto nop-A ; label nop-A nop-B (too long) ; label nop-A (exact)
        let mem = [
            OTHER, NOP_A, OTHER, LABEL, NOP_A, NOP_B, OTHER, LABEL, NOP_A, OTHER,
        ];
        let mut m = Mock::new(&mem);
        assert!(goto(&mut m, LABEL, Condition::Always));
        assert_eq!(m.heads[0], 7);
        assert_eq!(m.advance_ip, Some(false));

        // A shorter label than requested does not match.
        let mut m = Mock::new(&[OTHER, NOP_A, NOP_B, OTHER, LABEL, NOP_A, OTHER]);
        assert!(!goto(&mut m, LABEL, Condition::Always));
        assert_eq!(m.advance_ip, None);
    }

    #[test]
    fn copy_without_mutation_copies_and_marks_copied() {
        let mut m = Mock::new(&[5, 6, 7, 8, 9, 10]);
        m.flags[4] = FLAG_MUTATED | FLAG_COPY_MUT;
        m.regs[1] = 1; // from
        m.regs[0] = 3; // offset → to = 4
        copy(&mut m);
        assert_eq!(m.mem, vec![5, 6, 7, 8, 6, 10]);
        assert_eq!(m.flags[4], FLAG_COPIED);
        assert_eq!(m.copy_test_log, vec![CopyMut::Point]);
    }

    #[test]
    fn copy_with_mutation_writes_random_inst_unless_protected() {
        let mut m = Mock::new(&[5, 6, 7, 8, 9, 10]).fire(CopyMut::Point);
        m.random_insts.push_back(17);
        m.regs[1] = 1;
        m.regs[0] = 3;
        copy(&mut m);
        assert_eq!(m.mem[4], 17);
        assert_eq!(m.flags[4], FLAG_MUTATED | FLAG_COPY_MUT | FLAG_COPIED);

        // Source instruction on the no-mutation list: plain copy.
        let mut m = Mock::new(&[5, 6, 7, 8, 9, 10]).fire(CopyMut::Point);
        m.no_mut_ops = vec![6];
        m.regs[1] = 1;
        m.regs[0] = 3;
        copy(&mut m);
        assert_eq!(m.mem[4], 6);
        assert_eq!(m.flags[4], FLAG_COPIED);
    }

    #[test]
    fn write_inst_wraps_operands() {
        let mut m = Mock::new(&[5, 6, 7, 8]);
        m.regs[1] = 3;
        m.regs[0] = 2; // to = 5 → wraps to 1
        m.regs[2] = -1; // value → 25
        write_inst(&mut m, REG_CX);
        assert_eq!(m.mem, vec![5, 25, 7, 8]);
        assert_eq!(m.flags[1], FLAG_COPIED);

        // The no-mutation guard looks at the destination, not the source.
        let mut m = Mock::new(&[5, 6, 7, 8]).fire(CopyMut::Point);
        m.no_mut_ops = vec![6];
        m.regs[0] = 1;
        m.regs[2] = 3;
        write_inst(&mut m, REG_CX);
        assert_eq!(m.mem[1], 3);
    }

    #[test]
    fn stack_write_inst_pops_value() {
        let mut m = Mock::new(&[5, 6, 7, 8]);
        m.stack.push(30); // → 4
        m.regs[0] = 1;
        m.regs[1] = 1;
        stack_write_inst(&mut m, REG_BX);
        assert_eq!(m.mem, vec![5, 6, 4, 8]);
        assert!(m.stack.is_empty());
    }

    #[test]
    fn compare_stores_opcode_difference() {
        let mut m = Mock::new(&[5, 6, 7, 8, 9, 20]);
        m.regs[1] = 1;
        m.regs[0] = 4;
        compare(&mut m, REG_CX);
        assert_eq!(m.regs[2], 6 - 20);
        assert_eq!(m.mem[5], 20);

        let mut m = Mock::new(&[5, 6, 7, 8, 9, 20]).fire(CopyMut::Point);
        m.random_insts.push_back(1);
        m.regs[1] = 1;
        m.regs[0] = 4;
        compare(&mut m, REG_CX);
        assert_eq!(m.regs[2], 5);
        assert_eq!(m.flags[5], FLAG_MUTATED | FLAG_COPY_MUT);
    }

    #[test]
    fn if_n_cpy_inverts_on_mutation() {
        let setup = |mem: &[c_int], fire: bool| {
            let mut m = Mock::new(mem);
            if fire {
                m = m.fire(CopyMut::Point);
            }
            m.regs[1] = 0;
            m.regs[0] = 1;
            m
        };
        assert!(if_n_cpy(&mut setup(&[7, 7], false)));
        assert!(!if_n_cpy(&mut setup(&[7, 8], false)));
        assert!(!if_n_cpy(&mut setup(&[7, 7], true)));
        assert!(if_n_cpy(&mut setup(&[7, 8], true)));
    }

    #[test]
    fn head_read_reads_tracks_label_and_advances() {
        let mut m = Mock::new(&[OTHER, NOP_B, OTHER]);
        m.heads[1] = 1;
        head_read(&mut m);
        assert_eq!(m.regs[1], NOP_B);
        assert_eq!(m.read_label, vec![NOP_B]);
        assert_eq!(m.heads[1], 2);
        // Slip is not even drawn unless slips apply to the read head.
        assert_eq!(m.copy_test_log, vec![CopyMut::Point]);

        let mut m = Mock::new(&[OTHER, NOP_B, OTHER]).fire(CopyMut::Point);
        m.random_insts.push_back(12);
        m.heads[1] = 1;
        head_read(&mut m);
        assert_eq!(m.regs[1], 12);
        assert!(m.read_label.is_empty());
    }

    #[test]
    fn head_read_slip_relocates_read_head() {
        let mut m = Mock::new(&[OTHER; 6]).fire(CopyMut::Slip);
        m.slip_read_head = true;
        m.random_ints.push_back(3);
        head_read(&mut m);
        assert_eq!(m.heads[1], 4);
    }

    #[test]
    fn head_write_clamps_value_and_draws_mutations_in_order() {
        let mut m = Mock::new(&[5, 6, 7]);
        m.heads[2] = 1;
        m.regs[1] = 99; // out of range → 0
        head_write(&mut m);
        assert_eq!(m.mem, vec![5, 0, 7]);
        assert_eq!(m.flags[1], FLAG_COPIED);
        assert_eq!(m.heads[2], 2);
        assert_eq!(
            m.copy_test_log,
            vec![
                CopyMut::Insert,
                CopyMut::Delete,
                CopyMut::Uniform,
                CopyMut::Slip
            ]
        );

        // With slips on the read head, h-write never draws the slip test.
        let mut m = Mock::new(&[5, 6, 7]);
        m.slip_read_head = true;
        head_write(&mut m);
        assert_eq!(
            m.copy_test_log,
            vec![CopyMut::Insert, CopyMut::Delete, CopyMut::Uniform]
        );
    }

    #[test]
    fn head_write_applies_insert_delete_uniform_slip() {
        let mut m = Mock::new(&[5, 6, 7]).fire(CopyMut::Insert);
        m.random_insts.push_back(13);
        m.heads[2] = 1;
        m.regs[1] = 9;
        head_write(&mut m);
        assert_eq!(m.mem, vec![5, 13, 9, 7]);

        let mut m = Mock::new(&[5, 6, 7]).fire(CopyMut::Delete);
        m.heads[2] = 1;
        m.regs[1] = 9;
        head_write(&mut m);
        assert_eq!(m.mem, vec![5, 7]);
        assert_eq!(m.heads[2], 0); // 2 is now out of range and wraps

        let mut m = Mock::new(&[5, 6, 7])
            .fire(CopyMut::Uniform)
            .fire(CopyMut::Slip);
        m.heads[2] = 1;
        head_write(&mut m);
        assert_eq!(m.uniform_calls, vec![HEAD_WRITE]);
        assert_eq!(m.slip_calls, vec![1]);
    }

    #[test]
    fn head_copy_copies_and_advances_both_heads() {
        let mut m = Mock::new(&[NOP_C, 6, 7, 8]);
        m.heads[1] = 0;
        m.heads[2] = 3;
        head_copy(&mut m, None);
        assert_eq!(m.mem, vec![NOP_C, 6, 7, NOP_C]);
        assert_eq!(m.flags[3], FLAG_COPIED);
        assert_eq!(m.heads[1], 1);
        assert_eq!(m.heads[2], 0);
        assert_eq!(m.read_label, vec![NOP_C]);
        assert_eq!(
            m.copy_test_log,
            vec![
                CopyMut::Point,
                CopyMut::Insert,
                CopyMut::Delete,
                CopyMut::Uniform,
                CopyMut::Slip
            ]
        );
    }

    #[test]
    fn head_copy_point_mutation_respects_no_mut_list() {
        let mut m = Mock::new(&[9, 6, 7, 8]).fire(CopyMut::Point);
        m.random_insts.push_back(14);
        m.heads[2] = 2;
        head_copy(&mut m, None);
        assert_eq!(m.mem[2], 14);
        assert_eq!(m.flags[2], FLAG_MUTATED | FLAG_COPY_MUT | FLAG_COPIED);
        // The read label tracks the instruction actually read, not the mutant.
        assert!(m.read_label.is_empty());

        let mut m = Mock::new(&[9, 6, 7, 8]).fire(CopyMut::Point);
        m.no_mut_ops = vec![9];
        m.heads[2] = 2;
        head_copy(&mut m, None);
        assert_eq!(m.mem[2], 9);
        assert_eq!(m.flags[2], FLAG_COPIED);
    }

    #[test]
    fn head_copy_slip_targets_depend_on_config() {
        let mut m = Mock::new(&[9, 6, 7, 8]).fire(CopyMut::Slip);
        m.heads[2] = 2;
        head_copy(&mut m, None);
        assert_eq!(m.slip_calls, vec![2]);

        let mut m = Mock::new(&[9, 6, 7, 8]).fire(CopyMut::Slip);
        m.slip_read_head = true;
        m.random_ints.push_back(2);
        m.heads[2] = 2;
        head_copy(&mut m, None);
        assert!(m.slip_calls.is_empty());
        assert_eq!(m.heads[1], 3);
    }

    #[test]
    fn error_correcting_head_copy_scales_probabilities() {
        let mut m = Mock::new(&[9, 6, 7, 8]);
        m.copy_probs = [0.8, 0.4, 0.2, 0.1, 0.05];
        m.random_p_results = VecDeque::from([true]);
        m.random_insts.push_back(15);
        m.heads[2] = 1;
        head_copy(&mut m, Some(4.0));
        assert_eq!(m.random_p_log, vec![0.2, 0.1, 0.05, 0.025, 0.0125]);
        assert!(m.copy_test_log.is_empty());
        assert_eq!(m.mem[1], 15);
    }

    #[test]
    fn head_copy_if_resource_consumes_one_unit() {
        let mut m = Mock::new(&[3, 6, 7]);
        m.rbins = vec![0.0, 0.0, 0.0, 2.0];
        m.heads[2] = 1;
        head_copy_if_resource(&mut m);
        assert_eq!(m.rbins[3], 1.0);
        assert_eq!(m.mem[1], 3);
        assert_eq!(m.heads[1], 1);

        // Not enough resource: nothing is copied and no head moves.
        let mut m = Mock::new(&[3, 6, 7]);
        m.rbins = vec![0.0, 0.0, 0.0, 0.5];
        m.heads[2] = 1;
        head_copy_if_resource(&mut m);
        assert_eq!(m.mem, vec![3, 6, 7]);
        assert_eq!(m.heads, [0, 0, 1, 0]);
    }

    #[test]
    fn fork_thread_advances_ip_and_faults_on_failure() {
        let mut m = Mock::new(&[OTHER; 4]);
        fork_thread(&mut m);
        assert_eq!(m.fork_ips, vec![1]);
        assert!(m.faults.is_empty());

        let mut m = Mock::new(&[OTHER; 4]);
        m.fork_ok = false;
        fork_thread(&mut m);
        assert_eq!(
            m.faults,
            vec![(FAULT_LOC_THREAD_FORK, FAULT_TYPE_FORK_TH, String::new())]
        );
    }

    #[test]
    fn fork_thread_label_forks_after_label_and_restores_ip() {
        let mut m = Mock::new(&[OTHER, NOP_A, OTHER, OTHER, OTHER, OTHER]);
        m.complement_label_pos = 3;
        assert!(fork_thread_label(&mut m, Condition::Always));
        assert_eq!(m.fork_ips, vec![4]);
        assert_eq!(m.heads[0], 1);

        // Label found at the IP itself: no fork.
        let mut m = Mock::new(&[OTHER, NOP_A, OTHER, OTHER]);
        m.complement_label_pos = 1;
        assert!(fork_thread_label(&mut m, Condition::Always));
        assert!(m.fork_ips.is_empty());

        // No label: plain fork.
        let mut m = Mock::new(&[OTHER; 4]);
        assert!(fork_thread_label(&mut m, Condition::Always));
        assert_eq!(m.fork_ips, vec![1]);
        assert!(m.search_dirs.is_empty());
    }

    #[test]
    fn kill_thread_holds_ip_or_faults() {
        let mut m = Mock::new(&[OTHER]);
        kill_thread(&mut m);
        assert_eq!(m.advance_ip, Some(false));
        assert!(m.faults.is_empty());

        let mut m = Mock::new(&[OTHER]);
        m.kill_ok = false;
        kill_thread(&mut m);
        assert_eq!(m.advance_ip, None);
        assert_eq!(
            m.faults,
            vec![(FAULT_LOC_THREAD_KILL, FAULT_TYPE_KILL_TH, String::new())]
        );
    }

    #[test]
    fn allocate_reports_old_size_only_on_success() {
        let mut m = Mock::new(&[OTHER; 5]);
        m.regs[1] = 7;
        assert!(allocate(&mut m));
        assert_eq!(m.alloc_calls, vec![7]);
        assert_eq!(m.regs[0], 5);

        let mut m = Mock::new(&[OTHER; 5]);
        m.alloc_ok = false;
        m.regs[0] = -3;
        assert!(!allocate(&mut m));
        assert_eq!(m.regs[0], -3);
    }

    #[test]
    fn max_alloc_is_bounded_by_range_and_genome_limit() {
        let mut m = Mock::new(&[OTHER; 10]);
        assert!(max_alloc(&mut m, 2.0, 2048, false));
        assert_eq!(m.alloc_calls, vec![20]);
        assert_eq!(m.regs[0], 10);
        assert_eq!(m.heads[2], 0);

        let mut m = Mock::new(&[OTHER; 10]);
        assert!(max_alloc(&mut m, 2.0, 25, true));
        assert_eq!(m.alloc_calls, vec![15]);
        // The mock does not grow memory, so the write head wraps back to 0.
        assert_eq!(m.heads[2], 0);

        let mut m = Mock::new(&[OTHER; 10]);
        m.alloc_ok = false;
        assert!(!max_alloc(&mut m, 1.5, 2048, true));
        assert_eq!(m.alloc_calls, vec![15]);
        assert_eq!(m.regs[0], 0);
    }

    #[test]
    fn divide_modes_select_point_and_variant() {
        let mut m = Mock::new(&[OTHER; 9]);
        m.regs[0] = 6;
        assert!(divide(&mut m, DivideMode::AtAx));
        assert!(divide(&mut m, DivideMode::AtAxResample));
        assert!(divide(&mut m, DivideMode::Half));
        assert_eq!(
            m.divide_calls,
            vec![
                (6, 0, 1.0, DivideKind::Plain),
                (6, 0, 1.0, DivideKind::Resample),
                (4, 0, 1.0, DivideKind::Plain)
            ]
        );
        assert!(divide(&mut m, DivideMode::DoubleAlloc));
        assert_eq!(m.alloc_calls, vec![9]);
    }

    #[test]
    fn conditional_variants_gate_on_bx() {
        let mem = [OTHER, NOP_A, OTHER, CATCH, NOP_A, LABEL, NOP_A, OTHER];

        // Guard fails: nothing happens, and throw/goto do not read the label.
        let mut m = Mock::new(&mem);
        assert!(!throw(&mut m, CATCH, Condition::IfNot0));
        assert!(!goto(&mut m, LABEL, Condition::IfNot0));
        assert_eq!(m.heads[0], 0);
        m.regs[1] = 7;
        assert!(!throw(&mut m, CATCH, Condition::If0));
        assert!(!goto(&mut m, LABEL, Condition::If0));
        assert_eq!(m.heads[0], 0);

        // Guard holds.
        let mut m = Mock::new(&mem);
        assert!(throw(&mut m, CATCH, Condition::If0));
        assert_eq!(m.heads[0], 3);
        let mut m = Mock::new(&mem);
        m.regs[1] = 7;
        assert!(goto(&mut m, LABEL, Condition::IfNot0));
        assert_eq!(m.heads[0], 5);

        // A failed conditional fork still consumes its label.
        let mut m = Mock::new(&mem);
        assert!(!fork_thread_label(&mut m, Condition::IfNot0));
        assert_eq!(m.heads[0], 1);
        assert!(m.fork_ips.is_empty());
        let mut m = Mock::new(&mem);
        m.complement_label_pos = 5;
        assert!(fork_thread_label(&mut m, Condition::If0));
        assert_eq!(m.fork_ips, vec![6]);

        assert_eq!(Condition::from_code(0), Some(Condition::Always));
        assert_eq!(Condition::from_code(1), Some(Condition::IfNot0));
        assert_eq!(Condition::from_code(2), Some(Condition::If0));
        assert_eq!(Condition::from_code(3), None);
    }

    #[test]
    fn head_divide_computes_divide_point_and_extra_lines() {
        let mut m = Mock::new(&[OTHER; 20]);
        m.heads[1] = 8;
        m.heads[2] = 17;
        assert!(head_divide(&mut m, DivideKind::Plain, 2.0, false));
        assert_eq!(m.divide_calls, vec![(8, 3, 2.0, DivideKind::Plain)]);
        assert_eq!(m.adjust_all_calls, 2);

        // A write head that wrapped to 0 means the offspring fills memory.
        let mut m = Mock::new(&[OTHER; 20]);
        m.heads[1] = 10;
        assert!(head_divide(&mut m, DivideKind::Resample2, 1.0, false));
        assert_eq!(m.divide_calls, vec![(10, 0, 1.0, DivideKind::Resample2)]);
    }

    #[test]
    fn head_divide_reports_group_only_for_successful_plain_divides() {
        let run = |kind, divide_ok, tolerance| {
            let mut m = Mock::new(&[OTHER; 20]);
            m.regs[1] = 99;
            m.born_parent_group = true;
            m.divide_ok = divide_ok;
            let divided = head_divide(&mut m, kind, 1.0, tolerance);
            (divided, m.regs[1])
        };
        assert_eq!(run(DivideKind::Plain, true, true), (true, 1));
        assert_eq!(run(DivideKind::Plain, true, false), (true, 99));
        assert_eq!(run(DivideKind::Plain, false, true), (false, 99));
        assert_eq!(run(DivideKind::Resample, true, true), (true, 99));
    }

    #[test]
    fn head_divide_sex_sets_reproduction_flags() {
        for (mode, sex, cross) in [
            (SexMode::Sex, true, 1),
            (SexMode::Asex, false, 0),
            (SexMode::AsexWait, true, 0),
        ] {
            let mut m = Mock::new(&[OTHER; 6]);
            assert!(head_divide_sex(&mut m, mode, false));
            assert_eq!(m.divide_sex, Some(sex));
            assert_eq!(m.cross_num, Some(cross));
            assert_eq!(m.mate_select_id, None);
            assert_eq!(m.divide_calls.len(), 1);
        }

        let mut m = Mock::new(&[OTHER, NOP_B, NOP_C, OTHER]);
        assert!(head_divide_sex(&mut m, SexMode::MateSelect, false));
        assert_eq!(m.mate_select_id, Some(5)); // B, C read base 3
        assert_eq!(m.divide_sex, Some(true));
        assert_eq!(m.cross_num, Some(1));
        assert_eq!(m.heads[0], 2);
    }

    #[test]
    fn find_from_ip_skips_adjacent_instruction_and_falls_back_to_ip() {
        // The instruction right after the IP is never examined.
        let mut m = Mock::new(&[OTHER, ELSE, OTHER, ELSE, OTHER]);
        assert_eq!(find_from_ip(&mut m, ELSE), 3);
        // Only the adjacent match exists: the lap ends at the IP before reaching it.
        let mut m = Mock::new(&[OTHER, ELSE, OTHER, OTHER]);
        assert_eq!(find_from_ip(&mut m, ELSE), 0);
        // No match at all: the IP position comes back.
        let mut m = Mock::new(&[OTHER; 4]);
        m.heads[0] = 2;
        assert_eq!(find_from_ip(&mut m, ELSE), 2);
        // The IP's own instruction counts as a match after a full lap.
        let mut m = Mock::new(&[ELSE, OTHER, OTHER]);
        assert_eq!(find_from_ip(&mut m, ELSE), 0);
    }

    #[test]
    fn if_end_runs_body_or_jumps_to_else_endif() {
        // if (ip 0) body body else alt endif
        let mem = [OTHER, OTHER, OTHER, ELSE, OTHER, ENDIF, OTHER];
        let run = |cmp, x, y| {
            let mut m = Mock::new(&mem);
            m.regs[1] = x;
            m.regs[2] = y;
            if_end(&mut m, cmp, REG_BX, REG_CX);
            m.heads[0]
        };
        assert_eq!(run(IfEnd::Less, 1, 2), 0);
        assert_eq!(run(IfEnd::Less, 2, 2), 3);
        assert_eq!(run(IfEnd::NotEqual, 1, 2), 0);
        assert_eq!(run(IfEnd::NotEqual, 2, 2), 3);
        assert_eq!(run(IfEnd::GreaterEqual, 2, 2), 0);
        assert_eq!(run(IfEnd::GreaterEqual, 1, 2), 3);
    }

    #[test]
    fn else_top_half_falls_back_to_endif_then_single_skip() {
        let mut m = Mock::new(&[OTHER, OTHER, OTHER, ENDIF, OTHER]);
        else_top_half(&mut m);
        assert_eq!(m.heads[0], 3);

        let mut m = Mock::new(&[OTHER; 5]);
        else_top_half(&mut m);
        assert_eq!(m.heads[0], 1);

        // Instruction set without else/endif behaves like "not found".
        let mut m = Mock::new(&[OTHER, OTHER, ELSE, ENDIF]);
        m.has_structured_if = false;
        else_top_half(&mut m);
        assert_eq!(m.heads[0], 1);
    }

    #[test]
    fn else_jumps_to_endif_or_skips_one() {
        let mut m = Mock::new(&[ELSE, OTHER, OTHER, ENDIF, OTHER]);
        else_(&mut m);
        assert_eq!(m.heads[0], 3);

        let mut m = Mock::new(&[ELSE, OTHER, OTHER]);
        else_(&mut m);
        assert_eq!(m.heads[0], 1);
    }
}
