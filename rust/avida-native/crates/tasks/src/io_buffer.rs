//! Port of `tBuffer<int>` (avida-core/source/tools/tBuffer.h).
//!
//! A fixed-capacity circular buffer of the most recent values. Index 0 is the
//! most recently added value. Organisms use this type for their input, output
//! and received-message buffers, so the semantics (including reads of slots
//! that were never written or are stale) must match the C++ exactly.

/// `tBuffer<int>`: circular buffer with write offset and add counters.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IoBuffer {
    /// Contents; zero-initialised like `std::vector<int>(size)`.
    data: Vec<i32>,
    /// Position of the next write.
    offset: i32,
    /// Total adds since the last `clear` / `zero_num_adds`.
    total: i32,
    /// `total` at the time of the last `zero_num_adds`.
    last_total: i32,
}

impl IoBuffer {
    /// `tBuffer(const int size)`.
    pub fn new(size: usize) -> Self {
        IoBuffer {
            data: vec![0; size],
            offset: 0,
            total: 0,
            last_total: 0,
        }
    }

    /// `Clear()`: resets the counters only; stored values stay (stale reads
    /// through `get` still see them, as in C++).
    pub fn clear(&mut self) {
        self.offset = 0;
        self.total = 0;
        self.last_total = 0;
    }

    /// `ZeroNumAdds()`.
    pub fn zero_num_adds(&mut self) {
        self.last_total = self.total;
        self.total = 0;
    }

    /// `Add(const T&)`.
    ///
    /// QUIRK: on a zero-capacity buffer the C++ does `data[offset]` out of
    /// bounds and `offset %= 0` (undefined). Here the call is ignored.
    pub fn add(&mut self, value: i32) {
        let cap = self.data.len() as i32;
        if cap == 0 {
            return;
        }
        self.data[self.offset as usize] = value;
        self.total = self.total.wrapping_add(1);
        self.offset += 1;
        self.offset %= cap;
    }

    /// `Pop()`: undo the most recent add (the slot is not cleared).
    pub fn pop(&mut self) {
        self.total = self.total.wrapping_sub(1);
        self.offset -= 1;
        if self.offset < 0 {
            self.offset += self.data.len() as i32;
        }
    }

    /// `operator[](int i)`: `i == 0` is the newest value. Indices at or beyond
    /// the capacity (undefined in C++, an assert in debug builds) wrap around;
    /// an empty buffer reads as 0.
    pub fn get(&self, i: i32) -> i32 {
        let cap = self.data.len() as i32;
        if cap == 0 {
            return 0;
        }
        let mut index = self.offset - i - 1;
        if index < 0 {
            index += cap;
        }
        self.data[index.rem_euclid(cap) as usize]
    }

    /// `GetCapacity()`.
    pub fn capacity(&self) -> i32 {
        self.data.len() as i32
    }

    /// `GetTotal()`.
    pub fn total(&self) -> i32 {
        self.total
    }

    /// `GetNumStored()`.
    pub fn num_stored(&self) -> i32 {
        if self.total <= self.data.len() as i32 {
            self.total
        } else {
            self.data.len() as i32
        }
    }

    /// `GetNum()`: adds since the last `zero_num_adds`.
    pub fn num(&self) -> i32 {
        self.total.wrapping_sub(self.last_total)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn newest_first_and_wraparound() {
        let mut b = IoBuffer::new(3);
        assert_eq!(b.capacity(), 3);
        assert_eq!(b.num_stored(), 0);
        b.add(10);
        b.add(20);
        assert_eq!((b.get(0), b.get(1)), (20, 10));
        assert_eq!(b.num_stored(), 2);
        b.add(30);
        b.add(40); // overwrites 10
        assert_eq!((b.get(0), b.get(1), b.get(2)), (40, 30, 20));
        assert_eq!(b.total(), 4);
        assert_eq!(b.num_stored(), 3);
    }

    #[test]
    fn empty_reads_last_slot() {
        let mut b = IoBuffer::new(3);
        assert_eq!(b.get(0), 0);
        b.add(1);
        b.add(2);
        b.add(3); // offset back to 0
        b.clear();
        // stale data survives clear, offset 0: [0] reads data[2]
        assert_eq!(b.get(0), 3);
        assert_eq!(b.num_stored(), 0);
    }

    #[test]
    fn pop_and_counters() {
        let mut b = IoBuffer::new(2);
        b.add(5);
        b.add(6);
        b.zero_num_adds();
        // QUIRK: GetNum() is total - last_total, so right after ZeroNumAdds it
        // is minus the previous total.
        assert_eq!(b.num(), -2);
        assert_eq!(b.total(), 0);
        b.add(7);
        assert_eq!(b.num(), -1);
        b.pop();
        assert_eq!(b.total(), 0);
        assert_eq!(b.get(0), 6);
        // pop past the start wraps the offset
        let mut c = IoBuffer::new(4);
        c.pop();
        assert_eq!(c.total(), -1);
        c.add(9);
        assert_eq!(c.get(0), 9);
    }

    #[test]
    fn unwritten_slots_in_range_read_zero() {
        let mut b = IoBuffer::new(4);
        b.add(1);
        assert_eq!(b.get(1), 0);
        assert_eq!(b.get(3), 0);
    }
}
