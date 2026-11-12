//! Port of `cCPUMemory` (avida-core/source/cpu/cCPUMemory.{h,cc}): an
//! `InstructionSequence` with one flag byte per allocated site.
//!
//! The flag array always tracks the allocated instruction array (not just
//! the active size), exactly like the C++ `m_flag_array`, so flags of sites
//! that fall out of and back into the active range behave identically.

use crate::instruction::Instruction;
use crate::sequence::{replace_circular_impl, rotate_impl, InstructionSequence, SequenceVirtuals};
use std::ops::{Deref, Index, IndexMut};

/// `MASK_COPIED`
pub const MASK_COPIED: u8 = 0x01;
/// `MASK_MUTATED`
pub const MASK_MUTATED: u8 = 0x02;
/// `MASK_EXECUTED`
pub const MASK_EXECUTED: u8 = 0x04;
/// `MASK_POINTMUT`
pub const MASK_POINTMUT: u8 = 0x08;
/// `MASK_COPYMUT`
pub const MASK_COPYMUT: u8 = 0x10;
/// `MASK_INJECTED`
pub const MASK_INJECTED: u8 = 0x20;
/// `MASK_UNUSED1`
pub const MASK_UNUSED1: u8 = 0x40;
/// `MASK_UNUSED2`
pub const MASK_UNUSED2: u8 = 0x80;

/// `cCPUMemory`. Read-only `InstructionSequence` methods (`size`, `get`,
/// `as_string`, `crop`, `find_inst`, ...) are reachable through `Deref`;
/// every mutating method is overridden here so flags stay in sync.
#[derive(Debug)]
pub struct CpuMemory {
    seq: InstructionSequence,
    /// `m_flag_array`
    flags: Vec<u8>,
}

impl Default for CpuMemory {
    /// `explicit cCPUMemory(int size = 1)`.
    fn default() -> Self {
        CpuMemory::new(1)
    }
}

impl Clone for CpuMemory {
    /// `cCPUMemory(const cCPUMemory&)`: arrays become exactly the active
    /// size; the active flags are copied.
    fn clone(&self) -> Self {
        let n = self.seq.size() as usize;
        CpuMemory {
            seq: self.seq.clone(),
            flags: self.flags[..n].to_vec(),
        }
    }
}

impl CpuMemory {
    /// `explicit cCPUMemory(int size = 1)`: `size` zero opcodes, flags clear.
    pub fn new(size: i32) -> Self {
        let size = size.max(0);
        CpuMemory {
            seq: InstructionSequence::with_size(size),
            flags: vec![0; size as usize],
        }
    }

    /// `cCPUMemory(const InstructionSequence&)`: copy with clear flags.
    pub fn from_sequence(genome: &InstructionSequence) -> Self {
        CpuMemory {
            seq: genome.clone(),
            flags: vec![0; genome.size() as usize],
        }
    }

    /// `cCPUMemory(const Apto::String&)`.
    ///
    /// QUIRK: the flag array is sized by the string length, not the number
    /// of parsed instructions; it is brought back in line by the next
    /// capacity adjustment (all extra flags are 0, so this is invisible).
    pub fn from_string(s: &str) -> Self {
        CpuMemory {
            seq: InstructionSequence::from_string(s),
            flags: vec![0; s.len()],
        }
    }

    /// The memory viewed as a plain sequence (`const InstructionSequence&`).
    pub fn as_sequence(&self) -> &InstructionSequence {
        &self.seq
    }

    /// Copy of the active part as a plain sequence.
    pub fn to_sequence(&self) -> InstructionSequence {
        self.seq.clone()
    }

    // ---- Flags -------------------------------------------------------------

    /// Raw flag byte of a site.
    #[inline]
    pub fn flags(&self, pos: i32) -> u8 {
        self.flags[pos as usize]
    }

    /// `m_flag_array[pos] |= mask`.
    #[inline]
    pub fn set_flags(&mut self, pos: i32, mask: u8) {
        self.flags[pos as usize] |= mask;
    }

    /// `m_flag_array[pos] &= ~mask`.
    #[inline]
    pub fn clear_flag_mask(&mut self, pos: i32, mask: u8) {
        self.flags[pos as usize] &= !mask;
    }

    /// `FlagCopied(pos)`
    #[inline]
    pub fn flag_copied(&self, pos: i32) -> bool {
        self.flags(pos) & MASK_COPIED != 0
    }
    /// `FlagMutated(pos)`
    #[inline]
    pub fn flag_mutated(&self, pos: i32) -> bool {
        self.flags(pos) & MASK_MUTATED != 0
    }
    /// `FlagExecuted(pos)`
    #[inline]
    pub fn flag_executed(&self, pos: i32) -> bool {
        self.flags(pos) & MASK_EXECUTED != 0
    }
    /// `FlagPointMut(pos)`
    #[inline]
    pub fn flag_point_mut(&self, pos: i32) -> bool {
        self.flags(pos) & MASK_POINTMUT != 0
    }
    /// `FlagCopyMut(pos)`
    #[inline]
    pub fn flag_copy_mut(&self, pos: i32) -> bool {
        self.flags(pos) & MASK_COPYMUT != 0
    }
    /// `FlagInjected(pos)`
    #[inline]
    pub fn flag_injected(&self, pos: i32) -> bool {
        self.flags(pos) & MASK_INJECTED != 0
    }

    /// `SetFlagCopied(pos)`
    #[inline]
    pub fn set_flag_copied(&mut self, pos: i32) {
        self.set_flags(pos, MASK_COPIED);
    }
    /// `SetFlagMutated(pos)`
    #[inline]
    pub fn set_flag_mutated(&mut self, pos: i32) {
        self.set_flags(pos, MASK_MUTATED);
    }
    /// `SetFlagExecuted(pos)`
    #[inline]
    pub fn set_flag_executed(&mut self, pos: i32) {
        self.set_flags(pos, MASK_EXECUTED);
    }
    /// `SetFlagPointMut(pos)`
    #[inline]
    pub fn set_flag_point_mut(&mut self, pos: i32) {
        self.set_flags(pos, MASK_POINTMUT);
    }
    /// `SetFlagCopyMut(pos)`
    #[inline]
    pub fn set_flag_copy_mut(&mut self, pos: i32) {
        self.set_flags(pos, MASK_COPYMUT);
    }
    /// `SetFlagInjected(pos)`
    #[inline]
    pub fn set_flag_injected(&mut self, pos: i32) {
        self.set_flags(pos, MASK_INJECTED);
    }

    /// `ClearFlagCopied(pos)`
    #[inline]
    pub fn clear_flag_copied(&mut self, pos: i32) {
        self.clear_flag_mask(pos, MASK_COPIED);
    }
    /// `ClearFlagMutated(pos)`
    #[inline]
    pub fn clear_flag_mutated(&mut self, pos: i32) {
        self.clear_flag_mask(pos, MASK_MUTATED);
    }
    /// `ClearFlagExecuted(pos)`
    #[inline]
    pub fn clear_flag_executed(&mut self, pos: i32) {
        self.clear_flag_mask(pos, MASK_EXECUTED);
    }
    /// `ClearFlagPointMut(pos)`
    #[inline]
    pub fn clear_flag_point_mut(&mut self, pos: i32) {
        self.clear_flag_mask(pos, MASK_POINTMUT);
    }
    /// `ClearFlagCopyMut(pos)`
    #[inline]
    pub fn clear_flag_copy_mut(&mut self, pos: i32) {
        self.clear_flag_mask(pos, MASK_COPYMUT);
    }
    /// `ClearFlagInjected(pos)`
    #[inline]
    pub fn clear_flag_injected(&mut self, pos: i32) {
        self.clear_flag_mask(pos, MASK_INJECTED);
    }

    // ---- Capacity ----------------------------------------------------------

    /// `cCPUMemory::adjustCapacity(int new_size)`.
    fn adjust_capacity(&mut self, new_size: i32) {
        self.seq.adjust_capacity(new_size);
        if self.seq.seq.len() != self.flags.len() {
            self.flags.resize(self.seq.seq.len(), 0);
        }
    }

    /// `cCPUMemory::prepareInsert(int pos, int num_sites)`.
    fn prepare_insert(&mut self, pos: i32, num_sites: i32) {
        let old_size = self.seq.active_size;
        let new_size = old_size + num_sites;
        self.adjust_capacity(new_size);
        let mut i = old_size - 1;
        while i >= pos {
            self.seq.seq[(i + num_sites) as usize] = self.seq.seq[i as usize];
            i -= 1;
        }
        let mut i = old_size - 1;
        while i >= pos {
            self.flags[(i + num_sites) as usize] = self.flags[i as usize];
            i -= 1;
        }
    }

    // ---- Manipulation ------------------------------------------------------

    /// `Clear()`: zero opcodes and flags of the active sites.
    pub fn clear(&mut self) {
        for i in 0..self.seq.active_size as usize {
            self.seq.seq[i].set_op(0);
            self.flags[i] = 0;
        }
    }

    /// `ClearFlags()`: zero every flag byte of the allocated array.
    pub fn clear_flags(&mut self) {
        self.flags.iter_mut().for_each(|f| *f = 0);
    }

    /// `Reset(int new_size)`: resize and clear the contents.
    pub fn reset(&mut self, new_size: i32) {
        self.adjust_capacity(new_size);
        self.clear();
    }

    /// `Resize(int new_size)`: new sites get opcode 0 and clear flags.
    pub fn resize(&mut self, new_size: i32) {
        let old_size = self.seq.active_size;
        self.adjust_capacity(new_size);
        for i in old_size.max(0)..new_size {
            self.seq.seq[i as usize].set_op(0);
            self.flags[i as usize] = 0;
        }
    }

    /// `ResizeOld(int new_size)`: new sites keep whatever opcodes the
    /// allocated array still holds (0 if freshly allocated); their flags are
    /// cleared.
    pub fn resize_old(&mut self, new_size: i32) {
        let old_size = self.seq.active_size;
        self.adjust_capacity(new_size);
        for i in old_size.max(0)..new_size {
            self.flags[i as usize] = 0;
        }
    }

    /// `Copy(int to, int from)`: opcode and flags; positions may lie
    /// anywhere in the allocated array.
    pub fn copy(&mut self, to: i32, from: i32) {
        self.seq.seq[to as usize] = self.seq.seq[from as usize];
        self.flags[to as usize] = self.flags[from as usize];
    }

    /// `Insert(int pos, const Instruction&)`: the new site has clear flags.
    pub fn insert(&mut self, pos: i32, inst: Instruction) {
        self.prepare_insert(pos, 1);
        self.seq.seq[pos as usize] = inst;
        self.flags[pos as usize] = 0;
    }

    /// `Insert(int pos, const InstructionSequence&)`.
    pub fn insert_seq(&mut self, pos: i32, genome: &InstructionSequence) {
        self.prepare_insert(pos, genome.size());
        for i in 0..genome.size() {
            self.seq.seq[(i + pos) as usize] = genome.get(i);
            self.flags[(i + pos) as usize] = 0;
        }
    }

    /// `Remove(int pos, int num_sites = 1)`: later sites (and their flags)
    /// shift down.
    pub fn remove(&mut self, pos: i32, num_sites: i32) {
        let new_size = self.seq.active_size - num_sites;
        for i in pos..new_size {
            self.seq.seq[i as usize] = self.seq.seq[(i + num_sites) as usize];
            self.flags[i as usize] = self.flags[(i + num_sites) as usize];
        }
        self.adjust_capacity(new_size);
    }

    /// `Replace(int pos, int num_sites, const InstructionSequence&)`: the
    /// replaced sites get clear flags.
    pub fn replace(&mut self, pos: i32, num_sites: i32, genome: &InstructionSequence) {
        let size_change = genome.size() - num_sites;
        if size_change > 0 {
            self.prepare_insert(pos, size_change);
        } else if size_change < 0 {
            self.remove(pos, -size_change);
        }
        for i in 0..genome.size() {
            self.seq.seq[(i + pos) as usize] = genome.get(i);
            self.flags[(i + pos) as usize] = 0;
        }
    }

    /// `InstructionSequence::Replace(g, begin, end)` dispatching to the
    /// `cCPUMemory` overrides.
    pub fn replace_circular(&mut self, g: &InstructionSequence, begin: i32, end: i32) {
        replace_circular_impl(self, g, begin, end);
    }

    /// `InstructionSequence::Rotate(n)` dispatching to
    /// `cCPUMemory::operator=(const InstructionSequence&)` (all flags clear).
    pub fn rotate(&mut self, n: i32) {
        rotate_impl(self, n);
    }

    /// `Append(const Instruction&)`.
    pub fn append(&mut self, inst: Instruction) {
        let n = self.seq.size();
        self.insert(n, inst);
    }

    /// `Append(const InstructionSequence&)`.
    pub fn append_seq(&mut self, seq: &InstructionSequence) {
        let n = self.seq.size();
        self.insert_seq(n, seq);
    }

    /// `operator=(const cCPUMemory&)`: contents and flags; capacity follows
    /// `adjustCapacity` (unlike `clone`).
    pub fn assign_memory(&mut self, other: &CpuMemory) {
        self.adjust_capacity(other.seq.active_size);
        for i in 0..self.seq.active_size as usize {
            self.seq.seq[i] = other.seq.seq[i];
            self.flags[i] = other.flags[i];
        }
    }

    /// `operator=(const InstructionSequence&)`: contents, flags cleared.
    pub fn assign(&mut self, other: &InstructionSequence) {
        self.adjust_capacity(other.size());
        for i in 0..self.seq.active_size {
            self.seq.seq[i as usize] = other.get(i);
            self.flags[i as usize] = 0;
        }
    }
}

impl SequenceVirtuals for CpuMemory {
    fn base_seq(&self) -> &InstructionSequence {
        &self.seq
    }
    fn v_insert_seq(&mut self, pos: i32, seq: &InstructionSequence) {
        self.insert_seq(pos, seq);
    }
    fn v_replace(&mut self, pos: i32, num_sites: i32, seq: &InstructionSequence) {
        self.replace(pos, num_sites, seq);
    }
    fn v_remove(&mut self, pos: i32, num_sites: i32) {
        self.remove(pos, num_sites);
    }
    fn v_assign(&mut self, seq: &InstructionSequence) {
        self.assign(seq);
    }
}

impl Deref for CpuMemory {
    type Target = InstructionSequence;
    fn deref(&self) -> &InstructionSequence {
        &self.seq
    }
}

impl Index<i32> for CpuMemory {
    type Output = Instruction;
    #[inline]
    fn index(&self, idx: i32) -> &Instruction {
        &self.seq[idx]
    }
}

impl IndexMut<i32> for CpuMemory {
    /// `m_memory[pos] = inst`: flags are untouched.
    #[inline]
    fn index_mut(&mut self, idx: i32) -> &mut Instruction {
        &mut self.seq[idx]
    }
}

impl PartialEq for CpuMemory {
    /// `InstructionSequence::operator==`: flags are not compared.
    fn eq(&self, other: &Self) -> bool {
        self.seq == other.seq
    }
}

impl PartialEq<InstructionSequence> for CpuMemory {
    fn eq(&self, other: &InstructionSequence) -> bool {
        &self.seq == other
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn s(x: &str) -> InstructionSequence {
        InstructionSequence::from_string(x)
    }

    fn flags_of(m: &CpuMemory) -> Vec<u8> {
        (0..m.size()).map(|i| m.flags(i)).collect()
    }

    #[test]
    fn constructors() {
        let d = CpuMemory::default();
        assert_eq!(
            (d.size(), d.as_string(), d.flags(0)),
            (1, "a".to_string(), 0)
        );
        let m = CpuMemory::from_string("ab_c");
        assert_eq!(m.size(), 3);
        assert_eq!(m.flags.len(), 4);
        let mut m2 = m.clone();
        assert_eq!(m2.flags.len(), 3);
        m2.resize(3);
        assert_eq!(m2.flags.len(), 3);
        let mut m3 = CpuMemory::from_string("ab_c");
        m3.resize(4); // 4 > 3 -> array 9, flags follow
        assert_eq!((m3.array_size(), m3.flags.len()), (9, 9));
    }

    #[test]
    fn flags_follow_insert_remove() {
        let mut m = CpuMemory::from_sequence(&s("abcdef"));
        m.set_flag_copied(1);
        m.set_flag_executed(4);
        m.set_flags(5, MASK_MUTATED | MASK_INJECTED);
        m.insert(2, Instruction::new(25));
        assert_eq!(m.as_string(), "abzcdef");
        assert_eq!(flags_of(&m), vec![0, 1, 0, 0, 0, 4, 0x22]);
        m.remove(0, 2);
        assert_eq!(m.as_string(), "zcdef");
        assert_eq!(flags_of(&m), vec![0, 0, 0, 4, 0x22]);
        m.insert_seq(1, &s("XY"));
        assert_eq!(flags_of(&m), vec![0, 0, 0, 0, 0, 4, 0x22]);
        assert!(m.flag_executed(5) && m.flag_mutated(6) && m.flag_injected(6));
        m.clear_flag_injected(6);
        assert_eq!(m.flags(6), MASK_MUTATED);
        m.copy(0, 6);
        assert_eq!((m[0], m.flags(0)), (Instruction::new(5), MASK_MUTATED));
        m[0] = Instruction::new(1);
        assert_eq!(m.flags(0), MASK_MUTATED);
    }

    #[test]
    fn replace_clears_flags_of_replaced_sites() {
        let mut m = CpuMemory::from_sequence(&s("abcdef"));
        for i in 0..6 {
            m.set_flag_copied(i);
        }
        m.replace(1, 2, &s("XYZ"));
        assert_eq!(m.as_string(), "aXYZdef");
        assert_eq!(flags_of(&m), vec![1, 0, 0, 0, 1, 1, 1]);
        m.replace(0, 4, &s("q"));
        assert_eq!(m.as_string(), "qdef");
        assert_eq!(flags_of(&m), vec![0, 1, 1, 1]);
        m.replace_circular(&s("12"), 3, 1);
        assert_eq!(m.as_string(), "2de1");
        assert_eq!(flags_of(&m), vec![0, 1, 1, 0]);
    }

    #[test]
    fn resize_old_exposes_stale_sites() {
        let mut m = CpuMemory::from_sequence(&s("abcdefgh"));
        m.set_flag_copied(6);
        m.resize(5); // 20 < 8? no: array stays 8
        assert_eq!(m.array_size(), 8);
        m.resize_old(8);
        assert_eq!(m.as_string(), "abcdefgh");
        assert_eq!(m.flags(6), 0);
        m.resize(5);
        m.resize(8);
        assert_eq!(m.as_string(), "abcdeaaa");
        // Growing past the array keeps the slack prefix and zero-fills.
        let mut m = CpuMemory::from_sequence(&s("abcd"));
        m.resize(2); // 8 < 4? no
        m.resize_old(6); // 6 > 4 -> array max(9, 11) = 11
        assert_eq!(m.array_size(), 11);
        assert_eq!(m.as_string(), "abcdaa");
    }

    #[test]
    fn reset_clear_assign_rotate() {
        let mut m = CpuMemory::from_sequence(&s("abcdef"));
        m.set_flag_copied(0);
        m.reset(3);
        assert_eq!(
            (m.as_string(), flags_of(&m)),
            ("aaa".to_string(), vec![0, 0, 0])
        );

        let mut src = CpuMemory::from_sequence(&s("xyz"));
        src.set_flag_executed(2);
        m.assign_memory(&src);
        assert_eq!(flags_of(&m), vec![0, 0, 4]);
        m.assign(&s("pqrs"));
        assert_eq!(
            (m.as_string(), flags_of(&m)),
            ("pqrs".to_string(), vec![0, 0, 0, 0])
        );

        m.set_flag_copied(3);
        m.rotate(1);
        assert_eq!(m.as_string(), "spqr");
        assert_eq!(flags_of(&m), vec![0, 0, 0, 0]);

        m.set_flag_copied(1);
        m.clear_flags();
        assert_eq!(flags_of(&m), vec![0, 0, 0, 0]);
        m.set_flag_copied(1);
        m.clear();
        assert_eq!(
            (m.as_string(), flags_of(&m)),
            ("aaaa".to_string(), vec![0, 0, 0, 0])
        );
        m.append(Instruction::new(2));
        m.append_seq(&s("dd"));
        assert_eq!(m.as_string(), "aaaacdd");
        assert_eq!(m, s("aaaacdd"));
    }
}
