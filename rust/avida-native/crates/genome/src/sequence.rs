//! Port of `Avida::InstructionSequence`
//! (avida-core/include/public/avida/core/InstructionSequence.h,
//! avida-core/source/core/InstructionSequence.cc).
//!
//! Positions, sizes and offsets are `i32` like the C++ `int`s, so callers
//! can transliterate head arithmetic directly.
//!
//! Capacity model: the C++ keeps an allocated `Apto::Array` (`m_seq`) that
//! can be larger than the active size, grows/shrinks it with fixed factors
//! (`adjustCapacity`), and never clears the slack. `cCPUMemory::ResizeOld`
//! exposes that slack, so the backing array and its exact length are
//! modelled here too (`seq.len()` is the C++ `m_seq.GetSize()`).

use crate::instruction::Instruction;
use std::cmp::Ordering;
use std::ops::{Add, Index, IndexMut};

/// `MEMORY_INCREASE_MINIMUM`
pub(crate) const MEMORY_INCREASE_MINIMUM: i32 = 5;
/// `MEMORY_INCREASE_FACTOR`
pub(crate) const MEMORY_INCREASE_FACTOR: f64 = 1.5;
/// `MEMORY_SHRINK_TEST_FACTOR`
pub(crate) const MEMORY_SHRINK_TEST_FACTOR: f64 = 4.0;

/// `Avida::InstructionSequence`: a series of opcodes forming a genome.
#[derive(Debug, Default)]
pub struct InstructionSequence {
    /// `m_seq`: the allocated array; its length is the C++ array size.
    pub(crate) seq: Vec<Instruction>,
    /// `m_active_size`
    pub(crate) active_size: i32,
}

impl Clone for InstructionSequence {
    /// `InstructionSequence(const InstructionSequence&)`: the copy's array
    /// is exactly the active size (slack is not copied).
    fn clone(&self) -> Self {
        InstructionSequence {
            seq: self.as_slice().to_vec(),
            active_size: self.active_size,
        }
    }

    /// The same copy into this sequence's allocation.
    fn clone_from(&mut self, source: &Self) {
        self.seq.clear();
        self.seq.extend_from_slice(source.as_slice());
        self.active_size = source.active_size;
    }
}

/// `cInstSet`-independent "new array size" rule of `adjustCapacity`.
/// Returns `Some(new_array_size)` when the allocated array must change.
pub(crate) fn capacity_change(new_size: i32, array_size: i32) -> Option<i32> {
    if new_size > array_size || (new_size as f64) * MEMORY_SHRINK_TEST_FACTOR < array_size as f64 {
        let mut new_array_size = ((new_size as f64) * MEMORY_INCREASE_FACTOR) as i32;
        let new_array_min = new_size + MEMORY_INCREASE_MINIMUM;
        if new_array_min > new_array_size {
            new_array_size = new_array_min;
        }
        Some(new_array_size)
    } else {
        None
    }
}

impl InstructionSequence {
    /// `InstructionSequence()`.
    pub fn new() -> Self {
        InstructionSequence {
            seq: Vec::new(),
            active_size: 0,
        }
    }

    /// `explicit InstructionSequence(int size)`: `size` copies of opcode 0.
    pub fn with_size(size: i32) -> Self {
        let size = size.max(0);
        InstructionSequence {
            seq: vec![Instruction::default(); size as usize],
            active_size: size,
        }
    }

    /// Build from a slice of instructions (array size == length).
    pub fn from_instructions(insts: &[Instruction]) -> Self {
        InstructionSequence {
            seq: insts.to_vec(),
            active_size: insts.len() as i32,
        }
    }

    /// `explicit InstructionSequence(const Apto::String& str)`: parse the
    /// symbol string produced by [`as_string`](Self::as_string).
    ///
    /// Unrecognised characters are skipped. QUIRK: `_` (opcode 255) is
    /// skipped too, so it does not round-trip; a prefix character (`+ - ~ ?`)
    /// at the very end is paired with the string's NUL terminator and
    /// skipped.
    pub fn from_string(s: &str) -> Self {
        let b = s.as_bytes();
        // m_seq.ResizeClear(str.GetSize())
        let mut seq = vec![Instruction::default(); b.len()];
        let mut size = 0usize;
        let mut i = 0usize;
        while i < b.len() {
            if b[i] == b'_' {
                i += 1;
                continue;
            }
            match b[i] {
                b'+' | b'-' | b'~' | b'?' => {
                    let second = if i + 1 < b.len() { b[i + 1] } else { 0 };
                    if !seq[size].set_symbol_bytes(&[b[i], second]) {
                        i += 1;
                        continue;
                    }
                    i += 1;
                }
                _ => {
                    if !seq[size].set_symbol_bytes(&[b[i]]) {
                        i += 1;
                        continue;
                    }
                }
            }
            size += 1;
            i += 1;
        }
        // m_seq.Resize(size): exact-size array.
        seq.truncate(size);
        InstructionSequence {
            seq,
            active_size: size as i32,
        }
    }

    // ---- Accessors ---------------------------------------------------------

    /// `GetSize()`.
    #[inline]
    pub fn size(&self) -> i32 {
        self.active_size
    }

    /// True when `GetSize() == 0`.
    #[inline]
    pub fn is_empty(&self) -> bool {
        self.active_size == 0
    }

    /// Size of the allocated array (`m_seq.GetSize()`), >= `size()`.
    #[inline]
    pub fn array_size(&self) -> i32 {
        self.seq.len() as i32
    }

    /// `operator[](int)` (read).
    #[inline]
    pub fn get(&self, idx: i32) -> Instruction {
        self.seq[idx as usize]
    }

    /// `operator[](int) = inst`.
    #[inline]
    pub fn set(&mut self, idx: i32, inst: Instruction) {
        self.seq[idx as usize] = inst;
    }

    /// The active instructions.
    #[inline]
    pub fn as_slice(&self) -> &[Instruction] {
        &self.seq[..self.active_size as usize]
    }

    /// Mutable view of the active instructions.
    #[inline]
    pub fn as_mut_slice(&mut self) -> &mut [Instruction] {
        let n = self.active_size as usize;
        &mut self.seq[..n]
    }

    /// Iterate over the active instructions.
    pub fn iter(&self) -> std::slice::Iter<'_, Instruction> {
        self.as_slice().iter()
    }

    /// `AsString()`: the concatenated instruction symbols.
    pub fn as_string(&self) -> String {
        let mut out = String::with_capacity(self.active_size as usize);
        for inst in self.as_slice() {
            inst.push_symbol(&mut out);
        }
        out
    }

    // ---- Capacity helpers (protected in C++) -------------------------------

    /// `InstructionSequence::adjustCapacity(int new_size)`.
    pub(crate) fn adjust_capacity(&mut self, new_size: i32) {
        // Make sure we're really changing the size...
        if new_size == self.active_size {
            return;
        }
        let array_size = self.seq.len() as i32;
        if let Some(new_array_size) = capacity_change(new_size, array_size) {
            // Apto::Array::Resize keeps the common prefix and value-initialises
            // new elements.
            self.seq
                .resize(new_array_size.max(0) as usize, Instruction::default());
        }
        self.active_size = new_size;
    }

    /// `InstructionSequence::prepareInsert(int pos, int num_sites)`.
    pub(crate) fn prepare_insert(&mut self, pos: i32, num_sites: i32) {
        let old_size = self.active_size;
        let new_size = self.active_size + num_sites;
        self.adjust_capacity(new_size);
        let mut i = old_size - 1;
        while i >= pos {
            self.seq[(i + num_sites) as usize] = self.seq[i as usize];
            i -= 1;
        }
    }

    // ---- Manipulation ------------------------------------------------------

    /// `Resize(int new_size)`: sites beyond the old size become opcode 0.
    pub fn resize(&mut self, new_size: i32) {
        let old_size = self.active_size;
        self.adjust_capacity(new_size);
        for i in old_size.max(0)..new_size {
            self.seq[i as usize].set_op(0);
        }
    }

    /// `Copy(int to, int from)`.
    pub fn copy(&mut self, to: i32, from: i32) {
        self.seq[to as usize] = self.seq[from as usize];
    }

    /// `Insert(int pos, const Instruction&)`.
    pub fn insert(&mut self, pos: i32, inst: Instruction) {
        self.prepare_insert(pos, 1);
        self.seq[pos as usize] = inst;
    }

    /// `Insert(int pos, const InstructionSequence&)`.
    pub fn insert_seq(&mut self, pos: i32, seq: &InstructionSequence) {
        self.prepare_insert(pos, seq.size());
        for i in 0..seq.size() {
            self.seq[(i + pos) as usize] = seq.get(i);
        }
    }

    /// `Remove(int pos, int num_sites = 1)`.
    pub fn remove(&mut self, pos: i32, num_sites: i32) {
        let new_size = self.active_size - num_sites;
        for i in pos..new_size {
            self.seq[i as usize] = self.seq[(i + num_sites) as usize];
        }
        self.adjust_capacity(new_size);
    }

    /// `Replace(int pos, int num_sites, const InstructionSequence&)`.
    pub fn replace(&mut self, pos: i32, num_sites: i32, seq: &InstructionSequence) {
        let size_change = seq.size() - num_sites;
        // First, get the size right
        if size_change > 0 {
            self.prepare_insert(pos, size_change);
        } else if size_change < 0 {
            self.remove(pos, -size_change);
        }
        // Now just copy everything over!
        for i in 0..seq.size() {
            self.seq[(i + pos) as usize] = seq.get(i);
        }
    }

    /// `Replace(const InstructionSequence& g, int begin, int end)`: replace
    /// `[begin, end)` with `g`, wrapping around the end of the sequence when
    /// `end < begin`; all size changes are made at the end.
    pub fn replace_circular(&mut self, g: &InstructionSequence, begin: i32, end: i32) {
        replace_circular_impl(self, g, begin, end);
    }

    /// `Rotate(int n)`: move the last `n` instructions to the front
    /// (negative `n` moves the first `-n` to the end).
    pub fn rotate(&mut self, n: i32) {
        rotate_impl(self, n);
    }

    /// `operator=(const InstructionSequence&)`: the array becomes exactly
    /// the other sequence's active size.
    pub fn assign(&mut self, other: &InstructionSequence) {
        self.active_size = other.active_size;
        // m_seq.ResizeClear(m_active_size)
        self.seq.clear();
        self.seq.extend_from_slice(other.as_slice());
    }

    // ---- Utility methods ---------------------------------------------------

    /// `Append(const Instruction&)`.
    pub fn append(&mut self, inst: Instruction) {
        let n = self.size();
        self.insert(n, inst);
    }

    /// `Append(const InstructionSequence&)`.
    pub fn append_seq(&mut self, seq: &InstructionSequence) {
        let n = self.size();
        self.insert_seq(n, seq);
    }

    /// `FindInst(inst, start_index = 0)`: first position >= `start_index`
    /// holding `inst`, or -1.
    pub fn find_inst(&self, inst: Instruction, start_index: i32) -> i32 {
        let mut i = start_index;
        while i < self.active_size {
            if self.seq[i as usize] == inst {
                return i;
            }
            i += 1;
        }
        -1
    }

    /// `CountInst(inst)`.
    pub fn count_inst(&self, inst: Instruction) -> i32 {
        self.as_slice().iter().filter(|i| **i == inst).count() as i32
    }

    /// `HasInst(inst)`.
    pub fn has_inst(&self, inst: Instruction) -> bool {
        self.find_inst(inst, 0) >= 0
    }

    /// `MinDistBetween(inst)`: minimal circular distance between two copies
    /// of `inst`, 0 if it occurs at most once.
    pub fn min_dist_between(&self, inst: Instruction) -> i32 {
        let mut start_index = 0;
        let mut min_dist = self.active_size;

        while start_index < self.active_size {
            let first_instance = self.find_inst(inst, start_index);
            if first_instance == -1 && start_index == 0 {
                // no instance of inst
                return 0;
            } else if first_instance == -1 {
                // no more instances
                return min_dist;
            }

            start_index = first_instance + 1;
            let mut second_instance = self.find_inst(inst, start_index);

            if second_instance == -1 {
                // search from beginning
                second_instance = self.find_inst(inst, 0);
            }

            if first_instance != second_instance {
                min_dist = (first_instance - second_instance)
                    .abs()
                    .min(second_instance + self.active_size - first_instance)
                    .min(min_dist);
            } else {
                return 0;
            }
        }
        min_dist
    }

    // ---- Creation methods --------------------------------------------------

    /// `Crop(start, end)`: copy of `[start, end)`.
    pub fn crop(&self, start: i32, end: i32) -> InstructionSequence {
        let out_length = end - start;
        let mut out = InstructionSequence::with_size(out_length);
        for i in 0..out_length {
            out.seq[i as usize] = self.seq[(i + start) as usize];
        }
        out
    }

    /// `Cut(start, end)`: copy without `[start, end)`.
    pub fn cut(&self, start: i32, end: i32) -> InstructionSequence {
        let cut_length = end - start;
        let out_length = self.active_size - cut_length;
        let mut out = InstructionSequence::with_size(out_length);
        for i in 0..start {
            out.seq[i as usize] = self.seq[i as usize];
        }
        for i in start..out_length {
            out.seq[i as usize] = self.seq[(i + cut_length) as usize];
        }
        out
    }

    /// `static Join(lhs, rhs)` (also `operator+`).
    pub fn join(lhs: &InstructionSequence, rhs: &InstructionSequence) -> InstructionSequence {
        let length1 = lhs.size();
        let length2 = rhs.size();
        let mut out = InstructionSequence::with_size(length1 + length2);
        for i in 0..length1 {
            out.seq[i as usize] = lhs.get(i);
        }
        for i in 0..length2 {
            out.seq[(i + length1) as usize] = rhs.get(i);
        }
        out
    }

    // ---- Genetic distance methods -----------------------------------------

    /// `static FindOverlap(seq1, seq2, offset = 0)`.
    pub fn find_overlap(
        seq1: &InstructionSequence,
        seq2: &InstructionSequence,
        offset: i32,
    ) -> i32 {
        if offset > 0 {
            return (seq1.size() - offset).min(seq2.size());
        }
        (seq2.size() + offset).min(seq1.size())
    }

    /// `static FindHammingDistance(seq1, seq2, offset = 0)`: mismatches in
    /// the overlap plus every site protruding past it.
    pub fn find_hamming_distance(
        seq1: &InstructionSequence,
        seq2: &InstructionSequence,
        offset: i32,
    ) -> i32 {
        let start1 = if offset < 0 { 0 } else { offset };
        let start2 = if offset > 0 { 0 } else { -offset };
        let overlap = Self::find_overlap(seq1, seq2, offset);

        let mut hamming_distance = seq1.size() + seq2.size() - 2 * overlap;
        for i in 0..overlap {
            if seq1.get(start1 + i) != seq2.get(start2 + i) {
                hamming_distance += 1;
            }
        }
        hamming_distance
    }

    /// `static FindBestOffset(seq1, seq2)`.
    pub fn find_best_offset(seq1: &InstructionSequence, seq2: &InstructionSequence) -> i32 {
        let size1 = seq1.size();
        let size2 = seq2.size();

        let mut best_offset = 0;
        let mut cur_distance = Self::find_hamming_distance(seq1, seq2, 0);
        let mut best_distance = cur_distance;

        // Check positive offsets...
        let mut i = 1;
        while i < size1 || i < size2 {
            if size1 + size2 - 2 * Self::find_overlap(seq1, seq2, i) > best_distance {
                break;
            }
            cur_distance = Self::find_hamming_distance(seq1, seq2, i);
            if cur_distance < best_distance {
                best_distance = cur_distance;
                best_offset = i;
            }
            i += 1;
        }

        // Check negative offsets...
        let mut i = 1;
        while i < size1 || i < size2 {
            if size1 + size2 - 2 * Self::find_overlap(seq1, seq2, -i) > best_distance {
                break;
            }
            cur_distance = Self::find_hamming_distance(seq1, seq2, -i);
            if cur_distance < best_distance {
                best_distance = cur_distance;
                best_offset = -i;
            }
            i += 1;
        }

        best_offset
    }

    /// `static FindSlidingDistance(seq1, seq2)`.
    pub fn find_sliding_distance(seq1: &InstructionSequence, seq2: &InstructionSequence) -> i32 {
        let offset = Self::find_best_offset(seq1, seq2);
        Self::find_hamming_distance(seq1, seq2, offset)
    }

    /// `static FindEditDistance(seq1, seq2)`.
    ///
    /// QUIRK: the common prefix and suffix are stripped first and may
    /// overlap (e.g. `ab` vs `abab`); the C++ then returns
    /// `abs(test_size1 - test_size2)` of the possibly negative remainders.
    pub fn find_edit_distance(seq1: &InstructionSequence, seq2: &InstructionSequence) -> i32 {
        let size1 = seq1.size();
        let size2 = seq2.size();
        let min_size = if size1 < size2 { size1 } else { size2 };

        // If either size is zero, return the other one!
        if min_size == 0 {
            return if size1 > size2 { size1 } else { size2 };
        }

        // Count how many direct matches we have at the front and end.
        let mut match_front = 0;
        let mut match_end = 0;
        while match_front < min_size && seq1.get(match_front) == seq2.get(match_front) {
            match_front += 1;
        }
        while match_end < min_size
            && seq1.get(size1 - match_end - 1) == seq2.get(size2 - match_end - 1)
        {
            match_end += 1;
        }

        let test_size1 = size1 - match_front - match_end;
        let test_size2 = size2 - match_front - match_end;

        if test_size1 <= 0 || test_size2 <= 0 {
            return (test_size1 - test_size2).abs();
        }

        let ts1 = test_size1 as usize;
        let mut cur_row = vec![0i32; ts1];
        let mut prev_row: Vec<i32> = (0..ts1).map(|i| i as i32 + 1).collect();

        for i in 0..test_size2 {
            if seq1.get(match_front) == seq2.get(match_front + i) {
                cur_row[0] = i;
            } else {
                cur_row[0] = if i < prev_row[0] {
                    i + 1
                } else {
                    prev_row[0] + 1
                };
            }

            for j in 1..ts1 {
                if seq1.get(match_front + j as i32) == seq2.get(match_front + i) {
                    cur_row[j] = prev_row[j - 1];
                } else {
                    cur_row[j] = if prev_row[j] < prev_row[j - 1] {
                        prev_row[j]
                    } else {
                        prev_row[j - 1]
                    };
                    if cur_row[j - 1] < cur_row[j] {
                        cur_row[j] = cur_row[j - 1];
                    }
                    cur_row[j] += 1;
                }
            }
            std::mem::swap(&mut cur_row, &mut prev_row);
        }

        prev_row[ts1 - 1]
    }
}

// ---- Virtual dispatch for the shared Rotate / Replace(g, begin, end) ------

/// The virtual methods `InstructionSequence::Rotate` and
/// `InstructionSequence::Replace(g, begin, end)` dispatch to; `cCPUMemory`
/// overrides them, so the shared algorithms are written against this trait.
pub(crate) trait SequenceVirtuals {
    fn base_seq(&self) -> &InstructionSequence;
    fn v_insert_seq(&mut self, pos: i32, seq: &InstructionSequence);
    fn v_replace(&mut self, pos: i32, num_sites: i32, seq: &InstructionSequence);
    fn v_remove(&mut self, pos: i32, num_sites: i32);
    fn v_assign(&mut self, seq: &InstructionSequence);
}

impl SequenceVirtuals for InstructionSequence {
    fn base_seq(&self) -> &InstructionSequence {
        self
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

/// `InstructionSequence::Replace(const InstructionSequence& g, int begin, int end)`.
pub(crate) fn replace_circular_impl<T: SequenceVirtuals>(
    this: &mut T,
    g: &InstructionSequence,
    begin: i32,
    end: i32,
) {
    match begin.cmp(&end) {
        // we're actually doing an insertion...
        Ordering::Equal => this.v_insert_seq(begin, g),
        // no wrap-around
        Ordering::Less => this.v_replace(begin, end - begin, g),
        Ordering::Greater => {
            // replacement wraps around the end: [begin, size) and [0, end).
            let active = this.base_seq().size();
            let tail_size = (active - begin).min(g.size());
            let tail = g.crop(0, tail_size);
            this.v_replace(begin, active - begin, &tail);

            if tail_size != g.size() {
                let head = g.crop(tail_size, g.size());
                this.v_replace(0, end, &head);
            } else if end > 0 {
                this.v_remove(0, end);
            }
        }
    }
}

/// `InstructionSequence::Rotate(int n)`.
pub(crate) fn rotate_impl<T: SequenceVirtuals>(this: &mut T, n: i32) {
    if n == 0 {
        return;
    }
    let active = this.base_seq().size();
    if n > 0 {
        // forward
        let mut head = this.base_seq().crop(active - n, active);
        let tail = this.base_seq().crop(0, active - n);
        head.append_seq(&tail);
        this.v_assign(&head);
    } else {
        // backward (`assert(false)` in C++, but compiled out in release)
        let head = this.base_seq().crop(0, -n);
        let mut tail = this.base_seq().crop(-n, active);
        tail.append_seq(&head);
        this.v_assign(&tail);
    }
}

// ---- Operators -------------------------------------------------------------

impl PartialEq for InstructionSequence {
    /// `operator==(const GeneticRepresentation&)`: same active contents.
    fn eq(&self, other: &Self) -> bool {
        self.as_slice() == other.as_slice()
    }
}

impl Eq for InstructionSequence {}

impl PartialOrd for InstructionSequence {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for InstructionSequence {
    /// `operator<`: compares `AsString()` byte-wise.
    fn cmp(&self, other: &Self) -> Ordering {
        self.as_string().cmp(&other.as_string())
    }
}

impl Index<i32> for InstructionSequence {
    type Output = Instruction;
    #[inline]
    fn index(&self, idx: i32) -> &Instruction {
        &self.seq[idx as usize]
    }
}

impl IndexMut<i32> for InstructionSequence {
    #[inline]
    fn index_mut(&mut self, idx: i32) -> &mut Instruction {
        &mut self.seq[idx as usize]
    }
}

impl Add for &InstructionSequence {
    type Output = InstructionSequence;
    /// `operator+(lhs, rhs)` == `Join`.
    fn add(self, rhs: &InstructionSequence) -> InstructionSequence {
        InstructionSequence::join(self, rhs)
    }
}

impl From<&str> for InstructionSequence {
    fn from(s: &str) -> Self {
        InstructionSequence::from_string(s)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn s(x: &str) -> InstructionSequence {
        InstructionSequence::from_string(x)
    }

    #[test]
    fn parse_and_print() {
        let g = s("rucavccccutycasvab");
        assert_eq!(g.size(), 18);
        assert_eq!(g.as_string(), "rucavccccutycasvab");
        assert_eq!(g.array_size(), 18);
        let g = s("a+b-C~0?a_Z");
        assert_eq!(g.size(), 6);
        assert_eq!(g.as_string(), "a+b-C~0?aZ");
        assert_eq!(g[1].op(), 63);
        assert_eq!(g[2].op(), 124 + 28);
        // junk is skipped; trailing prefix pairs with NUL and is skipped.
        let g = s("a b*c+");
        assert_eq!(g.as_string(), "abc");
        assert_eq!(g.array_size(), 3);
        // '+' followed by an invalid char: the '+' alone is skipped.
        assert_eq!(s("+_b").as_string(), "b");
        assert_eq!(s("+*b").as_string(), "b");
    }

    #[test]
    fn capacity_rules() {
        let mut g = s("abc");
        g.resize(5); // 5 > 3 -> array max(7, 10) = 10
        assert_eq!(g.array_size(), 10);
        assert_eq!(g.as_string(), "abcaa");
        g.resize(4); // 16 < 10? no -> keep
        assert_eq!(g.array_size(), 10);
        g.resize(2); // 8 < 10 -> shrink to max(3, 7) = 7
        assert_eq!(g.array_size(), 7);
        assert_eq!(g.as_string(), "ab");
        g.resize(0); // 0 < 7 -> max(0, 5)
        assert_eq!(g.array_size(), 5);
        assert_eq!(g.size(), 0);
        assert_eq!(capacity_change(100, 100), None);
        assert_eq!(capacity_change(101, 100), Some(151));
        assert_eq!(capacity_change(24, 100), Some(36));
        assert_eq!(capacity_change(25, 100), None);
    }

    #[test]
    fn insert_remove_replace() {
        let mut g = s("abcdef");
        g.insert(0, Instruction::new(25));
        assert_eq!(g.as_string(), "zabcdef");
        g.insert(7, Instruction::new(24));
        assert_eq!(g.as_string(), "zabcdefy");
        g.insert_seq(3, &s("XY"));
        assert_eq!(g.as_string(), "zabXYcdefy");
        g.remove(0, 1);
        assert_eq!(g.as_string(), "abXYcdefy");
        g.remove(2, 2);
        assert_eq!(g.as_string(), "abcdefy");
        g.replace(1, 2, &s("QRS"));
        assert_eq!(g.as_string(), "aQRSdefy");
        g.replace(1, 3, &s("b"));
        assert_eq!(g.as_string(), "abdefy");
        g.replace(0, 0, &s(""));
        assert_eq!(g.as_string(), "abdefy");
        g.copy(0, 5);
        assert_eq!(g.as_string(), "ybdefy");
        g.append(Instruction::new(0));
        g.append_seq(&s("zz"));
        assert_eq!(g.as_string(), "ybdefyazz");
    }

    #[test]
    fn replace_circular_and_rotate() {
        let mut g = s("abcdefgh");
        g.replace_circular(&s("XY"), 2, 2);
        assert_eq!(g.as_string(), "abXYcdefgh");

        let mut g = s("abcdefgh");
        g.replace_circular(&s("XY"), 2, 5);
        assert_eq!(g.as_string(), "abXYfgh");

        // wrap: replace [6, 8) + [0, 2) with "1234"
        let mut g = s("abcdefgh");
        g.replace_circular(&s("1234"), 6, 2);
        assert_eq!(g.as_string(), "34cdef12");

        // wrap where the fragment is shorter than the tail: removes [0, end)
        let mut g = s("abcdefgh");
        g.replace_circular(&s("1"), 6, 2);
        assert_eq!(g.as_string(), "cdef1");

        let mut g = s("abcdefgh");
        g.rotate(3);
        assert_eq!(g.as_string(), "fghabcde");
        g.rotate(-3);
        assert_eq!(g.as_string(), "abcdefgh");
        g.rotate(0);
        assert_eq!(g.as_string(), "abcdefgh");
    }

    #[test]
    fn crop_cut_join() {
        let g = s("abcdef");
        assert_eq!(g.crop(1, 4).as_string(), "bcd");
        assert_eq!(g.cut(1, 4).as_string(), "aef");
        assert_eq!((&g + &s("xy")).as_string(), "abcdefxy");
        assert!(s("ab") < s("b"));
        assert_eq!(s("ab"), s("ab"));
        assert_ne!(s("ab"), s("abc"));
        // Equality ignores array slack.
        let mut h = s("abcdefgh");
        h.resize(2);
        assert_eq!(h, s("ab"));
        assert_eq!(h.clone().array_size(), 2);
    }

    #[test]
    fn find_count_mindist() {
        let g = s("abcabxa");
        let a = Instruction::new(0);
        assert_eq!(g.find_inst(a, 0), 0);
        assert_eq!(g.find_inst(a, 1), 3);
        assert_eq!(g.find_inst(Instruction::new(25), 0), -1);
        assert_eq!(g.count_inst(a), 3);
        assert!(g.has_inst(Instruction::new(23)));
        // a at 0, 3, 6 in size 7: circular distance 6 -> 0 is 1.
        assert_eq!(g.min_dist_between(a), 1);
        assert_eq!(s("abcd").min_dist_between(a), 0);
        assert_eq!(s("bcd").min_dist_between(a), 0);
        assert_eq!(s("abcabc").min_dist_between(a), 3);
    }

    #[test]
    fn distances() {
        let a = s("abcdef");
        let b = s("abcxef");
        assert_eq!(InstructionSequence::find_hamming_distance(&a, &b, 0), 1);
        assert_eq!(InstructionSequence::find_overlap(&a, &s("bcdef"), 1), 5);
        // 'a' protrudes past the overlap.
        assert_eq!(
            InstructionSequence::find_hamming_distance(&a, &s("bcdef"), 1),
            1
        );
        assert_eq!(
            InstructionSequence::find_hamming_distance(&a, &s("bcdef"), 0),
            6
        );
        assert_eq!(InstructionSequence::find_best_offset(&a, &s("bcdef")), 1);
        assert_eq!(
            InstructionSequence::find_sliding_distance(&a, &s("bcdef")),
            1
        );
        assert_eq!(InstructionSequence::find_best_offset(&s("bcdef"), &a), -1);
        assert_eq!(
            InstructionSequence::find_hamming_distance(&s("abc"), &s("abcde"), 0),
            2
        );

        assert_eq!(InstructionSequence::find_edit_distance(&a, &b), 1);
        assert_eq!(InstructionSequence::find_edit_distance(&a, &s("")), 6);
        assert_eq!(
            InstructionSequence::find_edit_distance(&s("kitten"), &s("sitting")),
            3
        );
        assert_eq!(
            InstructionSequence::find_edit_distance(&s("ab"), &s("abab")),
            2
        );
        assert_eq!(InstructionSequence::find_edit_distance(&a, &s("abcdef")), 0);
        assert_eq!(
            InstructionSequence::find_edit_distance(&s("abXdef"), &s("abYYdef")),
            2
        );
    }
}
