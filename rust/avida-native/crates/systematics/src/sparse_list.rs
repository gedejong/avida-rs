//! Port of `Apto::List<T, Apto::SparseVector>` with its `EntryHandle`s
//! (libs/apto/include/apto/core/ListStorage.h `SparseVector`, List.h).
//!
//! The genotype arbiter keeps every genotype in such lists (the per-size
//! active lists, the genome hash buckets and the historic list), and the
//! order in which they are iterated is visible in output files. The list is a
//! chain of 16-entry segments; inside a segment the logical order runs from
//! the highest index down to 0. `Push` adds at the logical front, `PushRear`
//! at the logical back, and removing an entry shifts its segment or packs it
//! into the following one. All of that is transliterated here, segment by
//! segment, because the C++ historic sweep removes entries while an iterator
//! walks the list, and what the iterator then visits depends on the segment
//! layout.
//!
//! Lists live in an arena ([`ListArena`]) so that an entry handle can move
//! between lists of the same owner, as `Genotype::m_handle` does.
//!
//! Freed segments: when the segment an iterator stands on is deleted, the C++
//! iterator keeps reading the freed memory. The reference platform (macOS 26,
//! arm64) zeroes small blocks on free apart from allocator metadata in bytes
//! 0 and 8 (checked with a harness on the reference machine), so the freed
//! segment reads as `next == NULL` with null entries. A `Next()` from such a
//! segment therefore either ends the iteration (position 0) or yields a null
//! entry that the C++ caller dereferences and crashes on. [`ListArena::next`]
//! and [`ListArena::get`] model both as "no element".

/// `SparseVector::SEGMENT_SIZE`.
pub const SEGMENT_SIZE: usize = 16;

/// One list owned by a [`ListArena`].
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct ListId(u32);

/// `SparseVector<T>::EntryHandle*`: lets its owner remove one entry without a
/// search.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct EntryHandle(u32);

type SegId = usize;

/// `SparseVector<T>::ListSegment`.
#[derive(Clone, Debug)]
struct Segment<T: Copy> {
    list: ListId,
    used: usize,
    entries: [Option<T>; SEGMENT_SIZE],
    prev: Option<SegId>,
    next: Option<SegId>,
    handles: [Option<EntryHandle>; SEGMENT_SIZE],
    /// Bumped whenever the segment is freed, so iterators can tell that the
    /// segment they stand on was deleted.
    generation: u32,
    live: bool,
}

/// `EntryHandle` state: `m_seg` and `m_entry`.
#[derive(Clone, Debug)]
struct HandleSlot<T: Copy> {
    seg: Option<SegId>,
    entry: Option<T>,
    live: bool,
}

/// The `SparseVector` members `m_size`, `m_segs`, `m_head_seg`, `m_tail_seg`.
#[derive(Clone, Debug, Default)]
struct ListHeader {
    size: i32,
    segs: i32,
    head: Option<SegId>,
    tail: Option<SegId>,
}

/// `SparseVector<T>::Iterator` (`m_cur`, `m_pos`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Cursor {
    cur: Option<(SegId, u32)>,
    pos: i32,
}

impl Default for Cursor {
    /// An iterator over an empty list (`Begin()` with no segments).
    fn default() -> Self {
        Cursor { cur: None, pos: -1 }
    }
}

/// Arena of `Apto::List<T, SparseVector>` lists sharing one handle space.
#[derive(Clone, Debug)]
pub struct ListArena<T: Copy + PartialEq> {
    segs: Vec<Segment<T>>,
    free_segs: Vec<SegId>,
    handles: Vec<HandleSlot<T>>,
    free_handles: Vec<u32>,
    lists: Vec<ListHeader>,
}

impl<T: Copy + PartialEq> Default for ListArena<T> {
    fn default() -> Self {
        Self::new()
    }
}

impl<T: Copy + PartialEq> ListArena<T> {
    /// An arena without lists.
    pub fn new() -> Self {
        ListArena {
            segs: Vec::new(),
            free_segs: Vec::new(),
            handles: Vec::new(),
            free_handles: Vec::new(),
            lists: Vec::new(),
        }
    }

    /// Creates an empty list (`SparseVector()`).
    pub fn new_list(&mut self) -> ListId {
        self.lists.push(ListHeader::default());
        ListId((self.lists.len() - 1) as u32)
    }

    fn hdr(&self, l: ListId) -> &ListHeader {
        &self.lists[l.0 as usize]
    }

    fn hdr_mut(&mut self, l: ListId) -> &mut ListHeader {
        &mut self.lists[l.0 as usize]
    }

    /// `GetSize()`.
    pub fn len(&self, l: ListId) -> i32 {
        self.hdr(l).size
    }

    /// `GetSize() == 0`.
    pub fn is_empty(&self, l: ListId) -> bool {
        self.hdr(l).size == 0
    }

    /// `ListSegment(list, value, next, prev)`: one used entry, no handle.
    fn alloc_seg(
        &mut self,
        list: ListId,
        value: T,
        next: Option<SegId>,
        prev: Option<SegId>,
    ) -> SegId {
        let mut entries = [None; SEGMENT_SIZE];
        entries[0] = Some(value);
        if let Some(id) = self.free_segs.pop() {
            let s = &mut self.segs[id];
            s.list = list;
            s.used = 1;
            s.entries = entries;
            s.prev = prev;
            s.next = next;
            s.handles = [None; SEGMENT_SIZE];
            s.live = true;
            id
        } else {
            self.segs.push(Segment {
                list,
                used: 1,
                entries,
                prev,
                next,
                handles: [None; SEGMENT_SIZE],
                generation: 0,
                live: true,
            });
            self.segs.len() - 1
        }
    }

    /// `delete cur`.
    fn free_seg(&mut self, id: SegId) {
        let s = &mut self.segs[id];
        s.live = false;
        s.generation = s.generation.wrapping_add(1);
        s.entries = [None; SEGMENT_SIZE];
        s.handles = [None; SEGMENT_SIZE];
        self.free_segs.push(id);
    }

    /// `new EntryHandle(seg, value)`.
    fn new_handle(&mut self, seg: SegId, value: T) -> EntryHandle {
        let slot = HandleSlot {
            seg: Some(seg),
            entry: Some(value),
            live: true,
        };
        if let Some(i) = self.free_handles.pop() {
            self.handles[i as usize] = slot;
            EntryHandle(i)
        } else {
            self.handles.push(slot);
            EntryHandle((self.handles.len() - 1) as u32)
        }
    }

    /// `delete handle` (`~EntryHandle`): detaches it from its segment.
    pub fn delete_handle(&mut self, h: EntryHandle) {
        let idx = h.0 as usize;
        if idx >= self.handles.len() || !self.handles[idx].live {
            return;
        }
        if let Some(seg) = self.handles[idx].seg {
            let s = &mut self.segs[seg];
            for i in 0..s.used {
                if s.handles[i] == Some(h) {
                    s.handles[i] = None;
                    break;
                }
            }
        }
        self.handles[idx] = HandleSlot {
            seg: None,
            entry: None,
            live: false,
        };
        self.free_handles.push(h.0);
    }

    /// `EntryHandle::IsValid()`.
    pub fn handle_is_valid(&self, h: EntryHandle) -> bool {
        self.handles
            .get(h.0 as usize)
            .is_some_and(|s| s.live && s.seg.is_some())
    }

    /// `EntryHandle::Remove()`: removes the entry from its list (searching
    /// its segment from index 0 for the value) and detaches the handle.
    pub fn handle_remove(&mut self, h: EntryHandle) {
        let idx = h.0 as usize;
        if idx >= self.handles.len() || !self.handles[idx].live {
            return;
        }
        let Some(seg) = self.handles[idx].seg else {
            return;
        };
        if let Some(entry) = self.handles[idx].entry {
            self.remove_from_seg(entry, seg, 0);
        }
        self.handles[idx].seg = None;
    }

    /// Replaces `*handle` the way `Push`/`PushRear` do: `delete *handle`.
    fn take_handle(&mut self, handle: &mut Option<&mut Option<EntryHandle>>) {
        if let Some(h) = handle.as_deref_mut() {
            if let Some(old) = h.take() {
                self.delete_handle(old);
            }
        }
    }

    /// `GetFirst()` (the logically first entry), `None` when empty.
    pub fn first(&self, l: ListId) -> Option<T> {
        let head = self.hdr(l).head?;
        let s = &self.segs[head];
        s.entries[s.used - 1]
    }

    /// `Push(value, handle)`: insert at the logical front.
    pub fn push(&mut self, l: ListId, value: T, mut handle: Option<&mut Option<EntryHandle>>) {
        self.take_handle(&mut handle);
        let head = self.hdr(l).head;
        match head {
            Some(hs) if self.segs[hs].used < SEGMENT_SIZE => {
                let idx = self.segs[hs].used;
                self.segs[hs].used += 1;
                self.segs[hs].entries[idx] = Some(value);
                if let Some(slot) = handle {
                    let nh = self.new_handle(hs, value);
                    self.segs[hs].handles[idx] = Some(nh);
                    *slot = Some(nh);
                } else {
                    self.segs[hs].handles[idx] = None;
                }
            }
            _ => {
                let ns = self.alloc_seg(l, value, head, None);
                let hdr = self.hdr_mut(l);
                hdr.head = Some(ns);
                hdr.segs += 1;
                if let Some(next) = head {
                    self.segs[next].prev = Some(ns);
                } else {
                    self.hdr_mut(l).tail = Some(ns);
                }
                if let Some(slot) = handle {
                    let nh = self.new_handle(ns, value);
                    self.segs[ns].handles[0] = Some(nh);
                    *slot = Some(nh);
                }
            }
        }
        self.hdr_mut(l).size += 1;
    }

    /// `PushRear(value, handle)`: insert at the logical back.
    pub fn push_rear(&mut self, l: ListId, value: T, mut handle: Option<&mut Option<EntryHandle>>) {
        self.take_handle(&mut handle);
        let tail = self.hdr(l).tail;
        match tail {
            Some(ts) if self.segs[ts].used < SEGMENT_SIZE => {
                let s = &mut self.segs[ts];
                for i in (1..=s.used).rev() {
                    s.entries[i] = s.entries[i - 1];
                }
                for i in (1..=s.used).rev() {
                    s.handles[i] = s.handles[i - 1];
                }
                s.entries[0] = Some(value);
                s.used += 1;
                if let Some(slot) = handle {
                    let nh = self.new_handle(ts, value);
                    self.segs[ts].handles[0] = Some(nh);
                    *slot = Some(nh);
                } else {
                    self.segs[ts].handles[0] = None;
                }
            }
            Some(ts) => {
                // Tail segment full.
                let ns = self.alloc_seg(l, value, None, Some(ts));
                self.segs[ts].next = Some(ns);
                let hdr = self.hdr_mut(l);
                hdr.segs += 1;
                hdr.tail = Some(ns);
                if let Some(slot) = handle {
                    let nh = self.new_handle(ns, value);
                    self.segs[ns].handles[0] = Some(nh);
                    *slot = Some(nh);
                }
            }
            None => {
                let ns = self.alloc_seg(l, value, None, None);
                let hdr = self.hdr_mut(l);
                hdr.segs += 1;
                hdr.tail = Some(ns);
                hdr.head = Some(ns);
                if let Some(slot) = handle {
                    let nh = self.new_handle(ns, value);
                    self.segs[ns].handles[0] = Some(nh);
                    *slot = Some(nh);
                }
            }
        }
        self.hdr_mut(l).size += 1;
    }

    /// `Remove(value)`: the first match found scanning segments from the
    /// head and, inside each, from index 0 (the logically last entry).
    pub fn remove(&mut self, l: ListId, value: T) -> bool {
        let mut cur = self.hdr(l).head;
        while let Some(c) = cur {
            let next = self.segs[c].next;
            if self.remove_from_seg(value, c, 0) {
                return true;
            }
            cur = next;
        }
        false
    }

    /// `removeFromSeg(value, cur, entry_idx)`.
    fn remove_from_seg(&mut self, value: T, cur: SegId, entry_idx: usize) -> bool {
        let list = self.segs[cur].list;
        let mut idx = entry_idx;
        while idx < self.segs[cur].used {
            if self.segs[cur].entries[idx] != Some(value) {
                idx += 1;
                continue;
            }
            self.segs[cur].used -= 1;
            let used = self.segs[cur].used;
            let next = self.segs[cur].next;
            let prev = self.segs[cur].prev;
            if used == 0 {
                // Last entry in this segment: remove the segment.
                if let Some(p) = prev {
                    self.segs[p].next = next;
                }
                if let Some(n) = next {
                    self.segs[n].prev = prev;
                }
                let hdr = self.hdr_mut(list);
                if hdr.head == Some(cur) {
                    hdr.head = next;
                }
                if hdr.tail == Some(cur) {
                    hdr.tail = prev;
                }
                hdr.segs -= 1;
                if let Some(h) = self.segs[cur].handles[0] {
                    self.handles[h.0 as usize].seg = None;
                }
                self.free_seg(cur);
            } else if next.is_some_and(|n| used + self.segs[n].used <= SEGMENT_SIZE) {
                // Pack the remaining entries of this segment onto the next.
                let n = next.unwrap_or_default();
                let nu = self.segs[n].used;
                let src = self.segs[cur].clone();
                for j in 0..idx.min(used) {
                    self.segs[n].entries[nu + j] = src.entries[j];
                }
                for j in idx..used {
                    self.segs[n].entries[nu + j] = src.entries[j + 1];
                }
                for j in 0..idx.min(used) {
                    let h = src.handles[j];
                    self.segs[n].handles[nu + j] = h;
                    if let Some(h) = h {
                        self.handles[h.0 as usize].seg = Some(n);
                    }
                }
                if let Some(h) = src.handles[idx] {
                    self.handles[h.0 as usize].seg = None;
                }
                for j in idx..used {
                    let h = src.handles[j + 1];
                    self.segs[n].handles[nu + j] = h;
                    if let Some(h) = h {
                        self.handles[h.0 as usize].seg = Some(n);
                    }
                }
                self.segs[n].used += used;
                if let Some(p) = prev {
                    self.segs[p].next = next;
                }
                self.segs[n].prev = prev;
                let hdr = self.hdr_mut(list);
                if hdr.head == Some(cur) {
                    hdr.head = next;
                }
                hdr.segs -= 1;
                self.free_seg(cur);
            } else {
                // Shift the remaining entries of this segment.
                let s = &mut self.segs[cur];
                for j in idx..used {
                    s.entries[j] = s.entries[j + 1];
                }
                let removed_handle = s.handles[idx];
                for j in idx..used {
                    s.handles[j] = s.handles[j + 1];
                }
                // QUIRK: `handles[used]` keeps a stale copy, as in C++; it
                // is outside `[0, used)` and never read.
                s.entries[used] = None;
                if let Some(h) = removed_handle {
                    self.handles[h.0 as usize].seg = None;
                }
            }
            self.hdr_mut(list).size -= 1;
            return true;
        }
        false
    }

    /// `Begin()`: an iterator standing before the first entry.
    pub fn begin(&self, l: ListId) -> Cursor {
        match self.hdr(l).head {
            Some(h) => Cursor {
                cur: Some((h, self.segs[h].generation)),
                pos: self.segs[h].used as i32,
            },
            None => Cursor { cur: None, pos: -1 },
        }
    }

    fn seg_is_current(&self, (seg, generation): (SegId, u32)) -> bool {
        let s = &self.segs[seg];
        s.live && s.generation == generation
    }

    /// `Iterator::Next()`: true where the C++ returns a non-NULL pointer.
    /// The pointed-to entry is then read with [`ListArena::get`], which can
    /// still be `None` when the list was changed under the iterator (see the
    /// module docs).
    pub fn next(&self, c: &mut Cursor) -> bool {
        let Some(cur) = c.cur else {
            return false;
        };
        if c.pos > 0 {
            c.pos -= 1;
            return true;
        }
        // A freed segment reads as zeroed memory: its `next` is NULL.
        let next = if self.seg_is_current(cur) {
            self.segs[cur.0].next
        } else {
            None
        };
        match next {
            Some(n) => {
                c.cur = Some((n, self.segs[n].generation));
                c.pos = self.segs[n].used as i32 - 1;
                true
            }
            None => {
                c.cur = None;
                false
            }
        }
    }

    /// `Iterator::Get()`.
    pub fn get(&self, c: &Cursor) -> Option<T> {
        let cur = c.cur?;
        if !self.seg_is_current(cur) {
            return None;
        }
        let s = &self.segs[cur.0];
        if c.pos >= 0 && (c.pos as usize) < s.used {
            s.entries[c.pos as usize]
        } else {
            None
        }
    }

    /// Read-only iteration in logical order (`while (it.Next()) it.Get()`).
    pub fn iter(&self, l: ListId) -> ListIter<'_, T> {
        ListIter {
            arena: self,
            cursor: self.begin(l),
        }
    }

    /// The list's entries in logical order.
    pub fn to_vec(&self, l: ListId) -> Vec<T> {
        self.iter(l).collect()
    }
}

/// Read-only iterator over one list.
pub struct ListIter<'a, T: Copy + PartialEq> {
    arena: &'a ListArena<T>,
    cursor: Cursor,
}

impl<T: Copy + PartialEq> Iterator for ListIter<'_, T> {
    type Item = T;

    fn next(&mut self) -> Option<T> {
        if !self.arena.next(&mut self.cursor) {
            return None;
        }
        self.arena.get(&self.cursor)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn visit_removing(arena: &mut ListArena<i64>, l: ListId, remove_at: i64) -> (Vec<i64>, bool) {
        let mut seen = Vec::new();
        let mut c = arena.begin(l);
        let mut null_hit = false;
        while arena.next(&mut c) {
            match arena.get(&c) {
                Some(v) => {
                    seen.push(v);
                    if v == remove_at {
                        arena.remove(l, v);
                    }
                }
                None => {
                    null_hit = true;
                    break;
                }
            }
        }
        (seen, null_hit)
    }

    /// Mirrors the harness run on the reference machine (Apto ListStorage.h,
    /// clang -O3, `List<long, SparseVector>`):
    /// ```text
    /// a-visit: 17                       (iteration ends after the freed head segment)
    /// b-visit: 1 2 3 4 5 <NULL>         (packing frees the segment under the iterator)
    /// c-visit: 1 .. 16 100 <NULL>
    /// ```
    #[test]
    fn freed_segment_under_iterator_matches_reference_machine() {
        let mut a = ListArena::new();
        let l = a.new_list();
        for i in 1..=17 {
            a.push(l, i, None);
        }
        assert_eq!(a.to_vec(l), (1..=17).rev().collect::<Vec<_>>());
        let (seen, null_hit) = visit_removing(&mut a, l, 17);
        assert_eq!((seen, null_hit), (vec![17], false));
        assert_eq!(a.to_vec(l), (1..=16).rev().collect::<Vec<_>>());

        let mut a = ListArena::new();
        let l = a.new_list();
        for i in 1..=17 {
            a.push_rear(l, i, None);
        }
        let (seen, null_hit) = visit_removing(&mut a, l, 5);
        assert_eq!((seen, null_hit), (vec![1, 2, 3, 4, 5], true));
        let mut want: Vec<i64> = (1..=17).collect();
        want.retain(|&v| v != 5);
        assert_eq!(a.to_vec(l), want);

        let mut a = ListArena::new();
        let l = a.new_list();
        for i in 1..=16 {
            a.push_rear(l, i, None);
        }
        a.push_rear(l, 100, None);
        for i in 17..=32 {
            a.push_rear(l, i, None);
        }
        let (seen, null_hit) = visit_removing(&mut a, l, 100);
        let mut want: Vec<i64> = (1..=16).collect();
        want.push(100);
        assert_eq!((seen, null_hit), (want, true));
        assert_eq!(a.to_vec(l), (1..=32).collect::<Vec<_>>());
    }

    #[test]
    fn handles_follow_entries_across_packing() {
        let mut a = ListArena::new();
        let l = a.new_list();
        let mut hs: Vec<Option<EntryHandle>> = vec![None; 20];
        for (i, h) in hs.iter_mut().enumerate() {
            a.push_rear(l, i as i64, Some(h));
        }
        // Remove from the first segment until it packs into the second.
        for i in [3usize, 7, 0, 15, 1] {
            a.handle_remove(hs[i].unwrap_or(EntryHandle(u32::MAX)));
            assert!(!a.handle_is_valid(hs[i].unwrap_or(EntryHandle(u32::MAX))));
        }
        let mut want: Vec<i64> = (0..20).collect();
        want.retain(|v| ![3, 7, 0, 15, 1].contains(v));
        assert_eq!(a.to_vec(l), want);
        for &v in &want {
            let h = hs[v as usize].unwrap_or(EntryHandle(u32::MAX));
            assert!(a.handle_is_valid(h), "handle of {v}");
        }
        // Every remaining handle still removes its own entry.
        for &v in want.iter().rev() {
            a.handle_remove(hs[v as usize].unwrap_or(EntryHandle(u32::MAX)));
        }
        assert!(a.is_empty(l));
        assert_eq!(a.first(l), None);
    }

    /// Replays tools/sparse_list_ref.cc (random Push / PushRear / Remove /
    /// EntryHandle::Remove on the real Apto header) against
    /// testdata/sparse_list_ref.txt.
    #[test]
    fn matches_cpp_random_operations() {
        const REF: &str = include_str!("../testdata/sparse_list_ref.txt");
        let mut s: u32 = 12345;
        let mut rnd = |m: u32| {
            s = s.wrapping_mul(1_103_515_245).wrapping_add(12345);
            (s >> 8) % m
        };
        const N: usize = 70;
        let mut a = ListArena::new();
        let l = a.new_list();
        let mut h: Vec<Option<EntryHandle>> = vec![None; N];
        let mut present = [false; N];
        for (step, want) in REF.lines().enumerate() {
            let op = rnd(5);
            let v = rnd(N as u32) as usize;
            match op {
                0 if !present[v] => {
                    a.push(l, v as i64, Some(&mut h[v]));
                    present[v] = true;
                }
                1 if !present[v] => {
                    a.push_rear(l, v as i64, Some(&mut h[v]));
                    present[v] = true;
                }
                2 if !present[v] => {
                    a.push(l, v as i64, None);
                    present[v] = true;
                }
                3 if present[v] => {
                    if let Some(hv) = h[v] {
                        if a.handle_is_valid(hv) {
                            a.handle_remove(hv);
                            present[v] = false;
                        }
                    }
                }
                4 if present[v] => {
                    a.remove(l, v as i64);
                    present[v] = false;
                }
                _ => {}
            }
            let mut got = format!("{step} {op} {v}:");
            for e in a.iter(l) {
                got.push_str(&format!(" {e}"));
            }
            assert_eq!(got, want);
        }
    }

    #[test]
    fn push_front_and_handle_reuse_between_lists() {
        let mut a = ListArena::new();
        let l1 = a.new_list();
        let l2 = a.new_list();
        let mut h = None;
        a.push_rear(l1, 1, Some(&mut h));
        a.push(l1, 2, None);
        a.push_rear(l1, 3, None);
        assert_eq!(a.to_vec(l1), vec![2, 1, 3]);
        assert_eq!(a.first(l1), Some(2));
        a.handle_remove(h.unwrap_or(EntryHandle(u32::MAX)));
        assert_eq!(a.to_vec(l1), vec![2, 3]);
        a.push(l2, 1, Some(&mut h));
        assert_eq!(a.to_vec(l2), vec![1]);
        assert!(a.remove(l1, 3));
        assert!(!a.remove(l1, 3));
        assert_eq!(a.len(l1), 1);
    }
}
