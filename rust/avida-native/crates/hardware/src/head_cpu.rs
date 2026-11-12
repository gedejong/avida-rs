//! Port of `cHeadCPU` (avida-core/source/cpu/cHeadCPU.{h,cc}).
//!
//! The C++ head keeps a pointer to its hardware and to the memory space it
//! is in. Here a head is plain data (`position`, `mem_space`, the cached
//! memory-space id) and every operation that needs the memory takes it, or
//! its size, explicitly. All hardware ported so far has one memory space,
//! so `fullAdjust`'s memory-space lookup reduces to normalising
//! `mem_space` against `num_mem_spaces`.

use avida_genome::{CpuMemory, InstSet, Instruction, InstructionSequence};

/// Out-of-range memory reads are undefined behaviour in the C++ (an
/// unchecked `Apto::Array` index). The port reads this instead and lists it
/// as a UB substitute in the crate documentation.
pub const OUT_OF_RANGE_INST: Instruction = Instruction::ERROR;

/// `memory[pos]` with the out-of-range substitute.
pub fn inst_at(seq: &InstructionSequence, pos: i32) -> Instruction {
    if pos >= 0 && pos < seq.size() {
        seq.get(pos)
    } else {
        OUT_OF_RANGE_INST
    }
}

/// `memory[pos] = inst`; an out-of-range write (undefined in C++) is dropped.
pub fn set_inst_at(seq: &mut InstructionSequence, pos: i32, inst: Instruction) {
    if pos >= 0 && pos < seq.size() {
        seq.set(pos, inst);
    }
}

/// `cHeadCPU`: a position in one of the hardware's memory spaces.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HeadCpu {
    position: i32,
    mem_space: i32,
    cached_ms: i32,
}

impl Default for HeadCpu {
    /// `cHeadCPU(hw, 0, 0)` with a hardware: position 0 in memory space 0,
    /// the memory cached.
    fn default() -> Self {
        HeadCpu {
            position: 0,
            mem_space: 0,
            cached_ms: 0,
        }
    }
}

impl HeadCpu {
    /// `cHeadCPU(hw, pos, ms)` for hardware with `num_mem_spaces` memory
    /// spaces whose memory `ms` has `mem_size` instructions.
    pub fn new(pos: i32, ms: i32, mem_size: i32, num_mem_spaces: i32) -> Self {
        let mut h = HeadCpu {
            position: pos,
            mem_space: ms,
            cached_ms: -1,
        };
        if pos != 0 || ms != 0 {
            h.adjust(mem_size, num_mem_spaces);
        } else {
            h.cached_ms = 0;
        }
        h
    }

    /// `fullAdjust(mem_size)`.
    pub fn full_adjust(&mut self, mem_size: i32, num_mem_spaces: i32) {
        if self.mem_space != self.cached_ms {
            if self.mem_space >= num_mem_spaces && num_mem_spaces > 0 {
                self.mem_space %= num_mem_spaces;
            }
            self.cached_ms = self.mem_space;
        }
        if self.position >= 0 && self.position < mem_size {
            return;
        }
        if mem_size == 0 || self.position < 0 {
            self.position = 0;
            return;
        }
        if self.position < mem_size.wrapping_mul(2) {
            self.position -= mem_size;
        } else {
            self.position %= mem_size;
        }
    }

    /// `Adjust()`.
    #[inline]
    pub fn adjust(&mut self, mem_size: i32, num_mem_spaces: i32) {
        if self.mem_space != self.cached_ms || self.position < 0 || self.position >= mem_size {
            self.full_adjust(mem_size, num_mem_spaces);
        }
    }

    /// `Reset(hw, ms)`.
    pub fn reset(&mut self, ms: i32, mem_size: i32, num_mem_spaces: i32) {
        self.position = 0;
        self.mem_space = ms;
        self.adjust(mem_size, num_mem_spaces);
    }

    /// `GetMemSpace()`.
    #[inline]
    pub fn mem_space(&self) -> i32 {
        self.mem_space
    }

    /// `GetPosition()`.
    #[inline]
    pub fn position(&self) -> i32 {
        self.position
    }

    /// `GetFullLocation()`.
    pub fn full_location(&self) -> i32 {
        (self.position & 0xFFFFFF) | self.mem_space.wrapping_shl(24)
    }

    /// `Set(pos, ms)`.
    pub fn set(&mut self, pos: i32, ms: i32, mem_size: i32, num_mem_spaces: i32) {
        self.position = pos;
        self.mem_space = ms;
        self.adjust(mem_size, num_mem_spaces);
    }

    /// `SetFullLocation(loc)`.
    pub fn set_full_location(&mut self, loc: i32, mem_size: i32, num_mem_spaces: i32) {
        self.position = loc & 0xFFFFFF;
        self.mem_space = loc >> 24;
        self.adjust(mem_size, num_mem_spaces);
    }

    /// `Set(const cHeadCPU&)`: copies position and memory space without
    /// adjusting.
    pub fn set_from(&mut self, other: &HeadCpu) {
        self.position = other.position;
        self.mem_space = other.mem_space;
    }

    /// `operator=`: copies position and memory space (the cache is kept).
    pub fn assign(&mut self, other: &HeadCpu) {
        self.position = other.position;
        self.mem_space = other.mem_space;
    }

    /// `AbsSet(pos)`.
    #[inline]
    pub fn abs_set(&mut self, pos: i32) {
        self.position = pos;
    }

    /// `Jump(n)`.
    pub fn jump(&mut self, jump: i32, mem_size: i32, num_mem_spaces: i32) {
        self.position = self.position.wrapping_add(jump);
        self.adjust(mem_size, num_mem_spaces);
    }

    /// `AbsJump(n)`.
    #[inline]
    pub fn abs_jump(&mut self, jump: i32) {
        self.position = self.position.wrapping_add(jump);
    }

    /// `LoopJump(n)`. A zero-size memory (a C++ division by zero) leaves the
    /// position at the jump target.
    pub fn loop_jump(&mut self, jump: i32, mem_size: i32) {
        self.position = self.position.wrapping_add(jump);
        if mem_size == 0 {
            return;
        }
        self.position %= mem_size;
        if self.position <= 0 {
            self.position += mem_size;
        }
    }

    /// `Advance()` / `operator++`.
    #[inline]
    pub fn advance(&mut self, mem_size: i32, num_mem_spaces: i32) {
        self.position = self.position.wrapping_add(1);
        self.adjust(mem_size, num_mem_spaces);
    }

    /// `Retreat()` / `operator--`.
    #[inline]
    pub fn retreat(&mut self, mem_size: i32, num_mem_spaces: i32) {
        self.position = self.position.wrapping_sub(1);
        self.adjust(mem_size, num_mem_spaces);
    }

    /// `GetInst()`.
    #[inline]
    pub fn get_inst(&self, mem: &CpuMemory) -> Instruction {
        inst_at(mem, self.position)
    }

    /// `GetInst(offset)`.
    #[inline]
    pub fn get_inst_offset(&self, mem: &CpuMemory, offset: i32) -> Instruction {
        inst_at(mem, self.position.wrapping_add(offset))
    }

    /// `GetPrevInst()`.
    pub fn get_prev_inst(&self, mem: &CpuMemory) -> Instruction {
        if self.at_front() {
            inst_at(mem, mem.size() - 1)
        } else {
            inst_at(mem, self.position - 1)
        }
    }

    /// `GetNextInst()`: the error instruction past the end of memory.
    pub fn get_next_inst(&self, mem: &CpuMemory, inst_set: &InstSet) -> Instruction {
        if self.at_end(mem.size()) {
            inst_set.inst_error()
        } else {
            inst_at(mem, self.position + 1)
        }
    }

    /// `SetInst(inst)`: flags are untouched; an out-of-range write
    /// (undefined in C++) is dropped.
    pub fn set_inst(&self, mem: &mut CpuMemory, inst: Instruction) {
        if self.in_memory(mem.size()) {
            mem[self.position] = inst;
        }
    }

    /// `InsertInst(inst)`.
    pub fn insert_inst(&self, mem: &mut CpuMemory, inst: Instruction) {
        if self.position >= 0 && self.position <= mem.size() {
            mem.insert(self.position, inst);
        }
    }

    /// `RemoveInst()`.
    pub fn remove_inst(&self, mem: &mut CpuMemory) {
        if self.in_memory(mem.size()) {
            mem.remove(self.position, 1);
        }
    }

    /// `SetFlagCopied()`.
    pub fn set_flag_copied(&self, mem: &mut CpuMemory) {
        if self.in_memory(mem.size()) {
            mem.set_flag_copied(self.position);
        }
    }

    /// `SetFlagMutated()`.
    pub fn set_flag_mutated(&self, mem: &mut CpuMemory) {
        if self.in_memory(mem.size()) {
            mem.set_flag_mutated(self.position);
        }
    }

    /// `SetFlagExecuted()`.
    pub fn set_flag_executed(&self, mem: &mut CpuMemory) {
        if self.in_memory(mem.size()) {
            mem.set_flag_executed(self.position);
        }
    }

    /// `SetFlagPointMut()`.
    pub fn set_flag_point_mut(&self, mem: &mut CpuMemory) {
        if self.in_memory(mem.size()) {
            mem.set_flag_point_mut(self.position);
        }
    }

    /// `SetFlagCopyMut()`.
    pub fn set_flag_copy_mut(&self, mem: &mut CpuMemory) {
        if self.in_memory(mem.size()) {
            mem.set_flag_copy_mut(self.position);
        }
    }

    /// `ClearFlagMutated()`.
    pub fn clear_flag_mutated(&self, mem: &mut CpuMemory) {
        if self.in_memory(mem.size()) {
            mem.clear_flag_mutated(self.position);
        }
    }

    /// `ClearFlagCopyMut()`.
    pub fn clear_flag_copy_mut(&self, mem: &mut CpuMemory) {
        if self.in_memory(mem.size()) {
            mem.clear_flag_copy_mut(self.position);
        }
    }

    /// `operator-`.
    pub fn diff(&self, other: &HeadCpu) -> i32 {
        self.position.wrapping_sub(other.position)
    }

    /// `operator==` (same hardware implied).
    pub fn same_location(&self, other: &HeadCpu) -> bool {
        self.position == other.position && self.mem_space == other.mem_space
    }

    /// `AtFront()`.
    #[inline]
    pub fn at_front(&self) -> bool {
        self.position == 0
    }

    /// `AtEnd()`.
    #[inline]
    pub fn at_end(&self, mem_size: i32) -> bool {
        self.position.wrapping_add(1) == mem_size
    }

    /// `InMemory()`.
    #[inline]
    pub fn in_memory(&self, mem_size: i32) -> bool {
        self.position >= 0 && self.position < mem_size
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn full_adjust_wraps_like_cpp() {
        let mut h = HeadCpu::default();
        h.abs_set(15);
        h.adjust(10, 1);
        assert_eq!(h.position(), 5);
        h.abs_set(35);
        h.adjust(10, 1);
        assert_eq!(h.position(), 5);
        h.abs_set(-3);
        h.adjust(10, 1);
        assert_eq!(h.position(), 0);
        h.abs_set(4);
        h.adjust(0, 1);
        assert_eq!(h.position(), 0);
    }

    #[test]
    fn loop_jump_matches_cpp() {
        let mut h = HeadCpu::default();
        h.abs_set(3);
        h.loop_jump(-3, 10);
        // 0 % 10 = 0 -> <= 0 -> += 10
        assert_eq!(h.position(), 10);
        h.abs_set(3);
        h.loop_jump(12, 10);
        assert_eq!(h.position(), 5);
    }

    #[test]
    fn full_location_round_trip() {
        let mut h = HeadCpu::default();
        h.set_full_location(7, 20, 1);
        assert_eq!(h.position(), 7);
        assert_eq!(h.full_location(), 7);
    }
}
