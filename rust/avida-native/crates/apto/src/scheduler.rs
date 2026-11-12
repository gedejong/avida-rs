//! Priority schedulers: port of `Apto::PriorityScheduler` and the four
//! implementations in `libs/apto/include/apto/scheduler/` and
//! `libs/apto/src/scheduler/`.
//!
//! The selection order of every scheduler is bit-exact with the C++ (verified
//! against reference sequences produced by `tools/ref_harness.cc` linked
//! against the real `libapto.a`, see `testdata/scheduler_ref.txt`).
//!
//! The C++ asserts (compiled out in the Release build, `-DNDEBUG`) guard some
//! states that are undefined behaviour in the reference binary: calling
//! `Next()` on a scheduler with no entries, a round-robin scheduler with no
//! active entry (infinite loop), or floating-point drift that walks a search
//! off the end of an array. Those cases return `-1` (or the nearest valid
//! entry) here and are marked `// UB in C++:` at the site.

use std::hint::select_unpredictable;

use crate::rng::AvidaRng;

/// Port of `Apto::PriorityScheduler` (apto/core/PriorityScheduler.h).
pub trait PriorityScheduler {
    /// `AdjustPriority(int entry_id, double priority)`.
    fn adjust_priority(&mut self, entry_id: i32, priority: f64);
    /// `Next()`: id of the next entry to execute, or `-1` if none.
    fn next(&mut self) -> i32;
    /// `EntryLimit()`: maximum number of accepted entries; `-1` (the base
    /// class default) means no limit.
    fn entry_limit(&self) -> i32 {
        -1
    }
}

// ---------------------------------------------------------------------------
// Util::Priority
// ---------------------------------------------------------------------------

/// `sizeof(unsigned int) * 8` in `Util::Priority`.
const MAX_BITS: i32 = 32;

/// C `frexp` as implemented by the platform libm: `x = mant * 2^exp` with
/// `0.5 <= |mant| < 1`; zero, infinities and NaN return `(x, 0)`.
fn frexp(x: f64) -> (f64, i32) {
    if x == 0.0 || !x.is_finite() {
        return (x, 0);
    }
    let bits = x.to_bits();
    let biased = ((bits >> 52) & 0x7ff) as i32;
    if biased == 0 {
        // Subnormal: scale into the normal range first (exact).
        let (m, e) = frexp(x * f64::from_bits(0x4350_0000_0000_0000)); // 2^54
        return (m, e - 54);
    }
    let mant = f64::from_bits((bits & !(0x7ff_u64 << 52)) | (1022_u64 << 52));
    (mant, biased - 1022)
}

/// Port of `Apto::Scheduler::Util::Priority` (apto/scheduler/Util.h): splits a
/// priority into its binary digits. Values below 1 count as 0; values with
/// more than 32 significant bits keep only the top 32 (`m_offset`).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Priority {
    bits: i32,
    base: u32,
    offset: i32,
}

impl Priority {
    /// `Priority::Priority(double value)`.
    pub fn new(value: f64) -> Self {
        let mut value = value;
        // Do not allow negative priorities. If less than 1, set to 0.
        if value < 1.0 {
            value = 0.0;
        }
        let (mant, bits) = frexp(value);
        let offset = if bits > MAX_BITS { bits - MAX_BITS } else { 0 };
        // `exp.mult[i] = pow(2.0, i - 1)`.
        let mult = 2.0_f64.powf(f64::from(bits - offset - 1));
        // `static_cast<unsigned int>`; for inf (frexp exponent 0) arm64
        // saturates to UINT_MAX exactly like Rust's `as`.
        let base = (mant * mult * 2.0) as u32;
        Priority { bits, base, offset }
    }

    /// `Priority::Bit(int bit_num)`.
    #[inline]
    pub fn bit(&self, bit_num: i32) -> i32 {
        if bit_num >= self.offset && bit_num < self.bits {
            ((self.base >> (bit_num - self.offset)) & 0x1) as i32
        } else {
            0
        }
    }

    /// `Priority::NumBits()`.
    #[inline]
    pub fn num_bits(&self) -> i32 {
        self.bits
    }
}

// ---------------------------------------------------------------------------
// RoundRobin
// ---------------------------------------------------------------------------

/// Port of `Apto::Scheduler::RoundRobin` (apto/scheduler/RoundRobin.h,
/// src/scheduler/RoundRobin.cc): priority-agnostic round robin over all
/// entries with non-zero priority.
#[derive(Debug, Clone)]
pub struct RoundRobin {
    last_id: i32,
    active: Vec<bool>,
    /// Not in the C++; lets `next` detect the no-active-entry case.
    num_active: usize,
}

impl RoundRobin {
    /// `RoundRobin(int num_entries)`.
    pub fn new(num_entries: usize) -> Self {
        RoundRobin {
            last_id: 0,
            active: vec![false; num_entries],
            num_active: 0,
        }
    }
}

impl PriorityScheduler for RoundRobin {
    fn adjust_priority(&mut self, entry_id: i32, priority: f64) {
        let slot = &mut self.active[entry_id as usize];
        let now_active = priority != 0.0;
        match (*slot, now_active) {
            (false, true) => self.num_active += 1,
            (true, false) => self.num_active -= 1,
            _ => {}
        }
        *slot = now_active;
    }

    fn next(&mut self) -> i32 {
        // UB in C++: with no active entry the C++ loops forever.
        if self.num_active == 0 {
            return -1;
        }
        let size = self.active.len() as i32;
        // Grab the next ID...
        self.last_id += 1;
        if self.last_id == size {
            self.last_id = 0;
        }
        // Make sure we actually have an active ID...
        while !self.active[self.last_id as usize] {
            self.last_id += 1;
            if self.last_id == size {
                self.last_id = 0;
            }
        }
        self.last_id
    }

    fn entry_limit(&self) -> i32 {
        self.active.len() as i32
    }
}

// ---------------------------------------------------------------------------
// Integrated
// ---------------------------------------------------------------------------

/// `Apto::Scheduler::Integrated::Node`. The C++ `Node*` links become indices
/// into the node array; a node's index equals its `node_id`.
#[derive(Debug, Clone)]
struct IntegratedNode {
    /// Per entry: 0 if not in the list, else the id of the next included
    /// entry (-1 for the last).
    active_array: Vec<i32>,
    first_entry: i32,
    active_entry: i32,
    size: i32,
    process_size: i32,
    process_count: i32,
    execute: bool,
    /// Next lower-priority node.
    next: Option<usize>,
    /// Next higher-priority node.
    prev: Option<usize>,
}

impl IntegratedNode {
    fn new(entry_count: usize) -> Self {
        IntegratedNode {
            active_array: vec![0; entry_count],
            first_entry: -1,
            active_entry: -1,
            size: 0,
            process_size: 1,
            process_count: 0,
            execute: true,
            next: None,
            prev: None,
        }
    }

    /// `Integrated::Node::Insert`.
    fn insert(&mut self, item_id: i32) {
        let item = item_id as usize;
        // If this item is already active in this node, ignore this call...
        if self.active_array[item] != 0 {
            return;
        }
        if self.first_entry == -1 || item_id < self.first_entry {
            self.active_array[item] = self.first_entry;
            self.first_entry = item_id;
        } else {
            let prev = self.find_prev_active(item_id);
            self.active_array[item] = self.active_array[prev];
            self.active_array[prev] = item_id;
        }
        self.size += 1;
    }

    /// `Integrated::Node::Remove`.
    fn remove(&mut self, item_id: i32) {
        let item = item_id as usize;
        // If this item is already inactive, ignore this call...
        if self.active_array[item] == 0 {
            return;
        }
        if self.first_entry == item_id {
            self.first_entry = self.active_array[item];
        } else {
            let prev = self.find_prev_active(item_id);
            self.active_array[prev] = self.active_array[item];
        }
        self.active_array[item] = 0;
        self.size -= 1;
    }

    /// The predecessor scan shared by `Insert` and `Remove`. The C++ asserts
    /// `prev_item >= 0`; it holds because `first_entry < item_id` is active.
    fn find_prev_active(&self, item_id: i32) -> usize {
        let mut prev_item = item_id - 1;
        while prev_item >= 0 {
            if self.active_array[prev_item as usize] != 0 {
                break;
            }
            prev_item -= 1;
        }
        debug_assert!(prev_item >= 0, "Integrated: predecessor not found");
        prev_item.max(0) as usize
    }
}

/// `Integrated::Node::Next` on node `id`: execute everything on this node's
/// list, then alternate with the next (lower) node until it returns -1.
fn integrated_node_next(nodes: &mut [Option<IntegratedNode>], id: usize) -> i32 {
    let node = nodes[id].as_mut().expect("Integrated: linked node exists");

    // Alternate between this node's Process and the next's.
    if !node.execute {
        let next = node.next;
        let mut next_id = -1;
        if let Some(next) = next {
            next_id = integrated_node_next(nodes, next);
        }
        if next_id == -1 {
            let node = nodes[id].as_mut().expect("Integrated: linked node exists");
            node.execute = true;
            node.process_count = 0;
            node.active_entry = -1;
        }
        return next_id;
    }

    let len = node.active_array.len() as i32;
    if node.active_entry == -1 {
        // If we were at the end of the list, start over...
        node.active_entry = node.first_entry;
    } else if node.active_array[node.active_entry as usize] == 0 {
        // If this entry no longer exists, hunt for the next active entry manually...
        while node.active_entry < len && node.active_array[node.active_entry as usize] == 0 {
            node.active_entry += 1;
        }
        if node.active_entry == len {
            node.active_entry = -1;
        }
    } else {
        // Otherwise, if the entry does exist, we can just look the next one up.
        node.active_entry = node.active_array[node.active_entry as usize];
    }

    // If we have now hit the end of this list, move on to the next node.
    if node.active_entry == -1 {
        node.process_count = node.process_count.wrapping_add(1);
        if node.process_count >= node.process_size {
            node.execute = false;
        }
    }
    node.active_entry
}

/// Port of `Apto::Scheduler::Integrated` (apto/scheduler/Integrated.h,
/// src/scheduler/Integrated.cc): deterministic scheduling proportional to
/// priority, built from the binary digits of each priority.
#[derive(Debug, Clone)]
pub struct Integrated {
    node_array: Vec<Option<IntegratedNode>>,
    priority_chart: Vec<f64>,
}

impl Integrated {
    /// `Integrated(int entry_count)`.
    pub fn new(entry_count: usize) -> Self {
        Integrated {
            node_array: Vec::new(),
            priority_chart: vec![0.0; entry_count],
        }
    }

    /// `Integrated::insertNode`.
    fn insert_node(&mut self, node_id: usize) {
        debug_assert!(node_id >= self.node_array.len() || self.node_array[node_id].is_none());
        let mut new_node = IntegratedNode::new(self.priority_chart.len());
        if node_id >= self.node_array.len() {
            self.resize_nodes(node_id);
        }

        // Find the node to mark as the 'prev'.
        for prev_id in node_id + 1..self.node_array.len() {
            if let Some(prev_node) = self.node_array[prev_id].as_mut() {
                new_node.prev = Some(prev_id);
                prev_node.next = Some(node_id);
                // QUIRK: C `1 << n` with n >= 32 is UB; the hardware (arm64
                // LSLV, x86 SHL) masks the count to 5 bits, as wrapping_shl does.
                prev_node.process_size = 1_i32.wrapping_shl((prev_id - node_id - 1) as u32);
                break;
            }
        }

        // And find the node to mark as the 'next'.
        for next_id in (0..node_id).rev() {
            if let Some(next_node) = self.node_array[next_id].as_mut() {
                new_node.next = Some(next_id);
                next_node.prev = Some(node_id);
                new_node.process_size = 1_i32.wrapping_shl((node_id - next_id - 1) as u32);
                break;
            }
        }

        self.node_array[node_id] = Some(new_node);
    }

    /// `Integrated::removeNode`.
    fn remove_node(&mut self, node_id: usize) {
        let old_node = self.node_array[node_id]
            .take()
            .expect("Integrated: removing an existing node");
        let next_node = old_node.next;
        let prev_node = old_node.prev;

        if let Some(next) = next_node {
            if let Some(n) = self.node_array[next].as_mut() {
                n.prev = prev_node;
            }
        }
        if let Some(prev) = prev_node {
            if let Some(p) = self.node_array[prev].as_mut() {
                p.next = next_node;
                p.process_size = old_node
                    .process_size
                    .wrapping_mul(p.process_size)
                    .wrapping_mul(2);
            }
        }

        if node_id == self.node_array.len() - 1 {
            match old_node.next {
                None => self.resize_nodes(0),
                Some(next) => self.resize_nodes(next),
            }
        }
    }

    /// `Integrated::resizeNodes(int new_max)`: size becomes `new_max + 1`.
    fn resize_nodes(&mut self, new_max: usize) {
        self.node_array.resize_with(new_max + 1, || None);
    }
}

impl PriorityScheduler for Integrated {
    /// `Integrated::AdjustPriority`. Panics if `entry_id` is out of range
    /// (an out-of-bounds write in C++).
    fn adjust_priority(&mut self, entry_id: i32, priority: f64) {
        let old_priority = self.priority_chart[entry_id as usize];
        if old_priority == priority {
            return;
        }
        self.priority_chart[entry_id as usize] = priority;

        // Re-adjust the lists.
        let old_p_comp = Priority::new(old_priority);
        let new_p_comp = Priority::new(priority);

        let priority_magnitude = old_p_comp.num_bits().max(new_p_comp.num_bits());
        for i in 0..priority_magnitude {
            let old_bit = old_p_comp.bit(i) != 0;
            let new_bit = new_p_comp.bit(i) != 0;
            let idx = i as usize;

            if old_bit && !new_bit {
                // Remove the item from this node...
                let node = self.node_array[idx]
                    .as_mut()
                    .expect("Integrated: node for a set bit exists");
                node.remove(entry_id);
                if node.size == 0 {
                    self.remove_node(idx);
                }
            }

            if !old_bit && new_bit {
                // Add the item from this node...
                if idx >= self.node_array.len() || self.node_array[idx].is_none() {
                    self.insert_node(idx);
                }
                self.node_array[idx]
                    .as_mut()
                    .expect("Integrated: node just inserted")
                    .insert(entry_id);
            }
        }
    }

    fn next(&mut self) -> i32 {
        // UB in C++: an empty node array fails `assert(m_node_array.GetSize() > 0)`.
        if self.node_array.is_empty() {
            return -1;
        }
        let last_id = self.node_array.len() - 1;

        // Make sure there are entries in the scheduler!
        if self.node_array[last_id].is_none() {
            return -1;
        }

        let mut next_id = -1;
        while next_id < 0 {
            next_id = integrated_node_next(&mut self.node_array, last_id);
        }
        next_id
    }

    fn entry_limit(&self) -> i32 {
        self.priority_chart.len() as i32
    }
}

// ---------------------------------------------------------------------------
// Probabilistic
// ---------------------------------------------------------------------------

/// Port of `Apto::Scheduler::Probabilistic::WeightedIndex`
/// (apto/scheduler/Probabilistic.h, src/scheduler/Probabilistic.cc): an
/// implicit binary tree of item weights with cached subtree sums.
#[derive(Debug, Clone)]
pub struct WeightedIndex {
    item_weight: Vec<f64>,
    subtree_weight: Vec<f64>,
}

impl WeightedIndex {
    /// `WeightedIndex(int size)`.
    pub fn new(size: usize) -> Self {
        WeightedIndex {
            item_weight: vec![0.0; size],
            subtree_weight: vec![0.0; size],
        }
    }

    /// `Size()`.
    pub fn size(&self) -> usize {
        self.item_weight.len()
    }

    /// Weight of one entry (`m_item_weight[id]`).
    pub fn weight(&self, entry_id: usize) -> f64 {
        self.item_weight[entry_id]
    }

    /// `TotalWeight()`. UB in C++ for a zero-size index; 0 here.
    pub fn total_weight(&self) -> f64 {
        self.subtree_weight.first().copied().unwrap_or(0.0)
    }

    /// `SetWeight(int entry_id, double weight)`.
    pub fn set_weight(&mut self, entry_id: usize, weight: f64) {
        let size = self.item_weight.len();
        let mut entry_id = entry_id;
        self.item_weight[entry_id] = weight;
        loop {
            let left_id = 2 * entry_id + 1;
            let right_id = 2 * entry_id + 2;
            let left_subtree = if left_id >= size {
                0.0
            } else {
                self.subtree_weight[left_id]
            };
            let right_subtree = if right_id >= size {
                0.0
            } else {
                self.subtree_weight[right_id]
            };
            self.subtree_weight[entry_id] =
                self.item_weight[entry_id] + left_subtree + right_subtree;
            if entry_id == 0 {
                break;
            }
            entry_id = (entry_id - 1) / 2;
        }
    }

    /// `FindPosition(double position)` (the recursive `findPosition` from the
    /// root, written as a loop).
    ///
    /// The left/right choice is made without a branch (it is a coin flip the
    /// branch predictor cannot learn): the right-hand position is always
    /// computed and then selected, which is the same subtraction the C++
    /// makes when it goes right.
    ///
    /// The weights the next level compares against (the child's item
    /// weight and its left subtree's weight) are loaded for both children
    /// before the choice is known and then selected, so the descent waits
    /// on the floating-point compare alone and not on a load as well. The
    /// arithmetic is the C++'s, in the same order. An index past the end is
    /// clamped to the last entry; such a value is never used, because the
    /// level that would use it stops first (the `>= size` checks).
    pub fn find_position(&self, position: f64) -> i32 {
        let size = self.item_weight.len();
        let last = size.saturating_sub(1);
        let mut position = position;
        let mut root_id = 0usize;
        // `m_item_weight[root_id]` and `m_subtree_weight[left_id]`.
        let mut item = self.item_weight[0];
        let mut left_subtree = self.subtree_weight[1.min(last)];
        loop {
            // First, see if we should just return this node.
            if position < item {
                return root_id as i32;
            }
            // If not, then see if we should search in the left subtree...
            position -= item;
            let left_id = 2 * root_id + 1;
            // UB in C++: rounding can leave `position` past the last child, and
            // the C++ (asserts off) then reads beyond the array. Stop at the
            // current node instead.
            if left_id >= size {
                return root_id as i32;
            }
            let right_id = left_id + 1;
            let left_item = self.item_weight[left_id];
            let right_item = self.item_weight[right_id.min(last)];
            let left_left_subtree = self.subtree_weight[(2 * left_id + 1).min(last)];
            let right_left_subtree = self.subtree_weight[(2 * right_id + 1).min(last)];
            // Otherwise we must look in the right subtree...
            let go_left = position < left_subtree;
            let right_position = position - left_subtree;
            position = select_unpredictable(go_left, position, right_position);
            let next_id = left_id + usize::from(!go_left);
            if next_id >= size {
                return root_id as i32; // UB in C++, as above.
            }
            root_id = next_id;
            item = select_unpredictable(go_left, left_item, right_item);
            left_subtree = select_unpredictable(go_left, left_left_subtree, right_left_subtree);
        }
    }
}

/// Port of `Apto::Scheduler::Probabilistic`: picks entries at random with
/// probability proportional to priority, using its own RNG.
#[derive(Debug, Clone)]
pub struct Probabilistic {
    rng: AvidaRng,
    index: WeightedIndex,
}

impl Probabilistic {
    /// `Probabilistic(int num_entries, SmartPtr<Random> rng)`.
    pub fn new(num_entries: usize, rng: AvidaRng) -> Self {
        Probabilistic {
            rng,
            index: WeightedIndex::new(num_entries),
        }
    }

    /// As `cPopulation::BuildTimeSlicer` (SLICE_PROB_MERIT) builds it: the
    /// scheduler RNG is seeded with `world.GetInt(0x7FFFFFFF)`.
    pub fn from_world_rng(num_entries: usize, world_rng: &mut AvidaRng) -> Self {
        let seed = world_rng.get_int(0x7FFF_FFFF);
        Self::new(num_entries, AvidaRng::new(seed))
    }

    /// The scheduler's own RNG.
    pub fn rng(&self) -> &AvidaRng {
        &self.rng
    }

    /// The weighted index (read-only).
    pub fn index(&self) -> &WeightedIndex {
        &self.index
    }
}

impl PriorityScheduler for Probabilistic {
    fn adjust_priority(&mut self, entry_id: i32, priority: f64) {
        self.index.set_weight(entry_id as usize, priority);
    }

    fn next(&mut self) -> i32 {
        if self.index.total_weight() == 0.0 {
            return -1;
        }
        let total = self.index.total_weight();
        self.index.find_position(self.rng.get_double_max(total))
    }

    fn entry_limit(&self) -> i32 {
        self.index.size() as i32
    }
}

// ---------------------------------------------------------------------------
// ProbabilisticIntegrated
// ---------------------------------------------------------------------------

/// A C++ `Node*` link: node index plus the serial number of the node object it
/// pointed to, so a link to a node that was since deleted (a dangling pointer
/// in C++) is recognised and never written through.
type Link = Option<(usize, u64)>;

/// `Apto::Scheduler::ProbabilisticIntegrated::Node`.
#[derive(Debug, Clone)]
struct PiNode {
    /// Per entry: position in `active_entries`, or -1.
    active_index: Vec<i32>,
    active_entries: Vec<i32>,
    serial: u64,
    next: Link,
    prev: Link,
}

impl PiNode {
    fn new(entry_count: usize, serial: u64) -> Self {
        PiNode {
            active_index: vec![-1; entry_count],
            active_entries: Vec::with_capacity(entry_count),
            serial,
            next: None,
            prev: None,
        }
    }

    fn size(&self) -> i32 {
        self.active_entries.len() as i32
    }

    /// `ProbabilisticIntegrated::Node::Insert`.
    fn insert(&mut self, item_id: i32) {
        let item = item_id as usize;
        if self.active_index[item] >= 0 {
            return;
        }
        let new_idx = self.active_entries.len() as i32;
        self.active_entries.push(item_id);
        self.active_index[item] = new_idx;
    }

    /// `ProbabilisticIntegrated::Node::Remove` (swap with last, then pop).
    fn remove(&mut self, item_id: i32) {
        let item = item_id as usize;
        if self.active_index[item] == -1 {
            return;
        }
        let cur_idx = self.active_index[item] as usize;
        let last = self.active_entries.len() - 1;
        self.active_entries.swap(cur_idx, last);
        let moved = self.active_entries[cur_idx] as usize;
        self.active_index[moved] = cur_idx as i32;
        self.active_entries.pop();
        self.active_index[item] = -1;
    }
}

/// Port of `Apto::Scheduler::ProbabilisticIntegrated`
/// (apto/scheduler/ProbabilisticIntegrated.h, src/scheduler/ProbabilisticIntegrated.cc):
/// picks a priority bit level at random weighted by `2^i * count`, then an
/// entry within that level by position.
#[derive(Debug, Clone)]
pub struct ProbabilisticIntegrated {
    rng: AvidaRng,
    node_array: Vec<Option<PiNode>>,
    node_weight: Vec<f64>,
    priority_chart: Vec<f64>,
    total_weight: f64,
    next_serial: u64,
}

impl ProbabilisticIntegrated {
    /// `ProbabilisticIntegrated(int entry_count, SmartPtr<Random> rng)`.
    pub fn new(entry_count: usize, rng: AvidaRng) -> Self {
        ProbabilisticIntegrated {
            rng,
            node_array: Vec::new(),
            node_weight: Vec::new(),
            priority_chart: vec![0.0; entry_count],
            total_weight: 0.0,
            next_serial: 0,
        }
    }

    /// As `cPopulation::BuildTimeSlicer` (SLICE_PROB_INTEGRATED_MERIT) builds
    /// it: the scheduler RNG is seeded with `world.GetInt(world.MaxSeed())`.
    pub fn from_world_rng(entry_count: usize, world_rng: &mut AvidaRng) -> Self {
        let max_seed = world_rng.max_seed();
        let seed = world_rng.get_int(max_seed);
        Self::new(entry_count, AvidaRng::new(seed))
    }

    /// The scheduler's own RNG.
    pub fn rng(&self) -> &AvidaRng {
        &self.rng
    }

    /// `m_total_weight`.
    pub fn total_weight(&self) -> f64 {
        self.total_weight
    }

    fn node_mut(&mut self, link: Link) -> Option<&mut PiNode> {
        let (idx, serial) = link?;
        self.node_array
            .get_mut(idx)
            .and_then(Option::as_mut)
            .filter(|n| n.serial == serial)
    }

    fn live_link(&self, idx: usize) -> Link {
        match &self.node_array[idx] {
            Some(n) if n.size() > 0 => Some((idx, n.serial)),
            _ => None,
        }
    }

    /// `ProbabilisticIntegrated::insertNode`. An existing empty node is reused
    /// with whatever links it still has; links not found below are left as
    /// they were (QUIRK: possibly stale, as in C++).
    fn insert_node(&mut self, node_id: usize) {
        if node_id >= self.node_array.len() {
            self.resize_nodes(node_id);
        }
        if self.node_array[node_id].is_none() {
            let serial = self.next_serial;
            self.next_serial += 1;
            self.node_array[node_id] = Some(PiNode::new(self.priority_chart.len(), serial));
        }
        let new_link = self.node_array[node_id]
            .as_ref()
            .map(|n| (node_id, n.serial));

        // Find the node to mark as the 'prev'.
        for prev_id in node_id + 1..self.node_array.len() {
            let prev_link = self.live_link(prev_id);
            if prev_link.is_some() {
                if let Some(new_node) = self.node_array[node_id].as_mut() {
                    new_node.prev = prev_link;
                }
                if let Some(prev_node) = self.node_array[prev_id].as_mut() {
                    prev_node.next = new_link;
                }
                break;
            }
        }

        // And find the node to mark as the 'next'.
        for next_id in (0..node_id).rev() {
            let next_link = self.live_link(next_id);
            if next_link.is_some() {
                if let Some(new_node) = self.node_array[node_id].as_mut() {
                    new_node.next = next_link;
                }
                if let Some(next_node) = self.node_array[next_id].as_mut() {
                    next_node.prev = new_link;
                }
                break;
            }
        }
    }

    /// `ProbabilisticIntegrated::removeNode`. QUIRK: unlike `Integrated`, the
    /// emptied node stays in the array (with its links) unless it is the last
    /// one, in which case the array is cut back to its `next` node.
    fn remove_node(&mut self, node_id: usize) {
        let (next_link, prev_link) = {
            let old_node = self.node_array[node_id]
                .as_ref()
                .expect("ProbabilisticIntegrated: removing an existing node");
            (old_node.next, old_node.prev)
        };

        // UB in C++ when a link dangles (its node was deleted): the write goes
        // to freed memory. Here it is skipped.
        if let Some(next_node) = self.node_mut(next_link) {
            next_node.prev = prev_link;
        }
        if let Some(prev_node) = self.node_mut(prev_link) {
            prev_node.next = next_link;
        }

        if node_id == self.node_array.len() - 1 {
            match next_link {
                None => self.resize_nodes(0),
                // `old_node->next->node_id`: the index is kept in the link.
                Some((next_id, _)) => self.resize_nodes(next_id),
            }
        }
    }

    /// `ProbabilisticIntegrated::resizeNodes(int new_max)`.
    fn resize_nodes(&mut self, new_max: usize) {
        self.node_array.resize_with(new_max + 1, || None);
        self.node_weight.resize(new_max + 1, 0.0);
    }
}

impl PriorityScheduler for ProbabilisticIntegrated {
    /// `ProbabilisticIntegrated::AdjustPriority`. Panics if `entry_id` is out
    /// of range (an out-of-bounds write in C++).
    fn adjust_priority(&mut self, entry_id: i32, priority: f64) {
        let old_priority = self.priority_chart[entry_id as usize];
        if old_priority == priority {
            return;
        }
        self.priority_chart[entry_id as usize] = priority;

        let old_p_comp = Priority::new(old_priority);
        let new_p_comp = Priority::new(priority);

        let priority_magnitude = old_p_comp.num_bits().max(new_p_comp.num_bits());
        for i in 0..priority_magnitude {
            let old_bit = old_p_comp.bit(i) != 0;
            let new_bit = new_p_comp.bit(i) != 0;
            let idx = i as usize;

            if old_bit && !new_bit {
                // Remove the item from this node...
                self.total_weight -= self.node_weight[idx];
                let node = self.node_array[idx]
                    .as_mut()
                    .expect("ProbabilisticIntegrated: node for a set bit exists");
                node.remove(entry_id);
                let size = node.size();
                if size == 0 {
                    self.node_weight[idx] = 0.0;
                    self.remove_node(idx);
                } else {
                    self.node_weight[idx] = 2.0_f64.powf(f64::from(i)) * f64::from(size);
                    self.total_weight += self.node_weight[idx];
                }
            } else if !old_bit && new_bit {
                // Add the item from this node...
                let needs_node = match self.node_array.get(idx) {
                    None | Some(None) => true,
                    Some(Some(n)) => n.size() == 0,
                };
                if needs_node {
                    self.insert_node(idx);
                }
                self.total_weight -= self.node_weight[idx];
                let node = self.node_array[idx]
                    .as_mut()
                    .expect("ProbabilisticIntegrated: node just inserted");
                node.insert(entry_id);
                let size = node.size();
                self.node_weight[idx] = 2.0_f64.powf(f64::from(i)) * f64::from(size);
                self.total_weight += self.node_weight[idx];
            }
        }
    }

    fn next(&mut self) -> i32 {
        let mut position = self.rng.get_double_max(self.total_weight);

        // UB in C++: `assert(m_node_array.GetSize() > 0)`.
        if self.node_array.is_empty() {
            return -1;
        }

        let mut node_id = self.node_array.len() as i32 - 1;
        while position > self.node_weight[node_id as usize] {
            position -= self.node_weight[node_id as usize];
            node_id -= 1;
            if node_id < 0 {
                // UB in C++: rounding drift between m_total_weight and the
                // node weights walks past node 0 (reads m_node_weight[-1]).
                // Use the last entry of the lowest non-empty node.
                return self
                    .node_array
                    .iter()
                    .flatten()
                    .find(|n| n.size() > 0)
                    .and_then(|n| n.active_entries.last().copied())
                    .unwrap_or(-1);
            }
        }

        let idx = node_id as usize;
        match &self.node_array[idx] {
            Some(cur_node) if cur_node.size() > 0 => {
                // `floor(((position / w) * (Size() - 1)) + 0.5)`; the reference
                // build fuses the multiply-add into one `fmadd` (clang
                // `-ffp-contract=on`, arm64), hence `mul_add`.
                let relative_position = (position / self.node_weight[idx])
                    .mul_add(f64::from(cur_node.size() - 1), 0.5)
                    .floor() as i32;
                cur_node.active_entries[relative_position as usize]
            }
            _ => {
                // UB in C++: only reachable with position == 0.0 exactly on an
                // empty or missing top node (0/0 index or NULL dereference).
                // Use the first entry of the highest non-empty node below.
                self.node_array[..idx]
                    .iter()
                    .rev()
                    .flatten()
                    .find(|n| n.size() > 0)
                    .map(|n| n.active_entries[0])
                    .unwrap_or(-1)
            }
        }
    }

    fn entry_limit(&self) -> i32 {
        self.priority_chart.len() as i32
    }
}

// ---------------------------------------------------------------------------
// Factory
// ---------------------------------------------------------------------------

/// `eTIMESLICE` values (avida-core/include/public/avida/core/Definitions.h).
pub mod slicing_method {
    pub const SLICE_CONSTANT: i32 = 0;
    pub const SLICE_PROB_MERIT: i32 = 1;
    pub const SLICE_INTEGRATED_MERIT: i32 = 2;
    pub const SLICE_DEME_PROB_MERIT: i32 = 3;
    pub const SLICE_PROB_DEMESIZE_PROB_MERIT: i32 = 4;
    pub const SLICE_PROB_INTEGRATED_MERIT: i32 = 5;
}

/// Port of `cPopulation::BuildTimeSlicer` (avida-core/source/main/cPopulation.cc):
/// builds the scheduler selected by `SLICING_METHOD`, drawing the seed of a
/// probabilistic scheduler's own RNG from the world RNG exactly as the C++
/// does. Methods 3 and 4 are commented out in the C++ and fall through to its
/// error message, returned here as `Err`.
pub fn build_time_slicer(
    slicing_method: i32,
    num_cells: usize,
    world_rng: &mut AvidaRng,
) -> Result<Box<dyn PriorityScheduler>, String> {
    use slicing_method::*;
    match slicing_method {
        SLICE_CONSTANT => Ok(Box::new(RoundRobin::new(num_cells))),
        SLICE_INTEGRATED_MERIT => Ok(Box::new(Integrated::new(num_cells))),
        SLICE_PROB_MERIT => Ok(Box::new(Probabilistic::from_world_rng(
            num_cells, world_rng,
        ))),
        SLICE_PROB_INTEGRATED_MERIT => Ok(Box::new(ProbabilisticIntegrated::from_world_rng(
            num_cells, world_rng,
        ))),
        _ => Err("error: requested time slicer not found.".to_string()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const REF: &str = include_str!("../testdata/scheduler_ref.txt");

    fn reference(name: &str) -> Vec<i32> {
        let line = REF
            .lines()
            .find(|l| l.split_whitespace().next() == Some(name))
            .unwrap_or_else(|| panic!("no reference line {name}"));
        line.split_whitespace()
            .skip(1)
            .map(|t| t.parse().expect("integer"))
            .collect()
    }

    /// Mirror of `DrawPriority` in tools/ref_harness.cc.
    fn draw_priority(drv: &mut AvidaRng, chart: &[f64], id: usize) -> f64 {
        match drv.get_int(8) {
            0 => 0.0,
            1 => drv.get_double(),
            2 => f64::from(drv.get_int_range(1, 20)),
            3 => drv.get_double_range(1.0, 1000.0),
            4 => f64::from(drv.get_int_range(1, 100_000)),
            5 => {
                let e = 2.0_f64.powf(f64::from(drv.get_int_range(0, 40)));
                e * drv.get_double_range(1.0, 2.0)
            }
            6 => drv.get_double_range(1e9, 1e13),
            _ => chart[id],
        }
    }

    /// Mirror of `RunScenario` in tools/ref_harness.cc.
    fn run_scenario(
        sched: &mut dyn PriorityScheduler,
        seed: i32,
        n: usize,
        steps: usize,
    ) -> Vec<i32> {
        let mut drv = AvidaRng::new(seed);
        let mut chart = vec![0.0; n];
        let mut live = 0;
        let mut out = Vec::new();
        for _ in 0..steps {
            if live == 0 || drv.get_int(10) < 3 {
                let id = drv.get_int(n as i32) as usize;
                let p = draw_priority(&mut drv, &chart, id);
                if chart[id] >= 1.0 {
                    live -= 1;
                }
                if p >= 1.0 {
                    live += 1;
                }
                chart[id] = p;
                sched.adjust_priority(id as i32, p);
            } else {
                out.push(sched.next());
            }
        }
        out
    }

    #[test]
    fn round_robin_matches_cpp() {
        let mut s = RoundRobin::new(20);
        assert_eq!(
            run_scenario(&mut s, 1, 20, 3000),
            reference("roundrobin_s1_n20")
        );
    }

    #[test]
    fn integrated_matches_cpp() {
        let mut s = Integrated::new(50);
        assert_eq!(
            run_scenario(&mut s, 1, 50, 6000),
            reference("integrated_s1_n50")
        );
        let mut s = Integrated::new(7);
        assert_eq!(
            run_scenario(&mut s, 2, 7, 6000),
            reference("integrated_s2_n7")
        );
    }

    #[test]
    fn probabilistic_matches_cpp() {
        let mut world = AvidaRng::new(101);
        let mut s = Probabilistic::from_world_rng(50, &mut world);
        assert_eq!(
            run_scenario(&mut s, 3, 50, 6000),
            reference("probabilistic_w101_s3_n50")
        );
    }

    #[test]
    fn probabilistic_integrated_matches_cpp() {
        let mut world = AvidaRng::new(202);
        let mut s = ProbabilisticIntegrated::from_world_rng(50, &mut world);
        assert_eq!(
            run_scenario(&mut s, 4, 50, 6000),
            reference("probint_w202_s4_n50")
        );
        let mut world = AvidaRng::new(303);
        let mut s = ProbabilisticIntegrated::from_world_rng(9, &mut world);
        assert_eq!(
            run_scenario(&mut s, 5, 9, 6000),
            reference("probint_w303_s5_n9")
        );
    }

    #[test]
    fn priority_bits() {
        let p = Priority::new(0.75);
        assert_eq!(p.num_bits(), 0);
        let p = Priority::new(5.9); // truncated to 5 = 0b101
        assert_eq!(p.num_bits(), 3);
        assert_eq!((p.bit(0), p.bit(1), p.bit(2), p.bit(3)), (1, 0, 1, 0));
        // 2^40 + 2^8: only the top 32 bits survive (offset 9), so bit 8 is lost.
        let p = Priority::new(2f64.powi(40) + 256.0);
        assert_eq!(p.num_bits(), 41);
        assert_eq!(p.bit(40), 1);
        assert_eq!(p.bit(8), 0);
        assert_eq!(Priority::new(f64::INFINITY).num_bits(), 0);
        assert_eq!(Priority::new(f64::NAN).num_bits(), 0);
        assert_eq!(frexp(1e300), (0.746_610_894_802_575_1, 997));
        assert_eq!(frexp(f64::MIN_POSITIVE / 4.0), (0.5, -1023));
    }

    #[test]
    fn integrated_is_proportional_and_deterministic() {
        // Priorities 1, 2, 4: integrated scheduling gives exactly 1:2:4 shares.
        let mut s = Integrated::new(3);
        s.adjust_priority(0, 1.0);
        s.adjust_priority(1, 2.0);
        s.adjust_priority(2, 4.0);
        let mut counts = [0; 3];
        for _ in 0..700 {
            counts[s.next() as usize] += 1;
        }
        assert_eq!(counts, [100, 200, 400]);
    }

    #[test]
    fn empty_schedulers_return_minus_one() {
        let mut rng = AvidaRng::new(1);
        for method in [0, 1, 2, 5] {
            let mut s = build_time_slicer(method, 4, &mut rng).expect("known method");
            assert_eq!(s.entry_limit(), 4);
            if method != 2 && method != 5 {
                assert_eq!(s.next(), -1);
            }
            s.adjust_priority(2, 3.0);
            assert_eq!(s.next(), 2);
            s.adjust_priority(2, 0.0);
            assert_eq!(s.next(), -1);
        }
        assert_eq!(
            build_time_slicer(3, 4, &mut rng).err(),
            Some("error: requested time slicer not found.".to_string())
        );
    }

    /// `findPosition` one level at a time, each load made once the level
    /// is known: what [`WeightedIndex::find_position`] must agree with.
    fn find_position_plain(w: &WeightedIndex, position: f64) -> i32 {
        let size = w.item_weight.len();
        let mut position = position;
        let mut root_id = 0usize;
        loop {
            let item = w.item_weight[root_id];
            if position < item {
                return root_id as i32;
            }
            position -= item;
            let left_id = 2 * root_id + 1;
            if left_id >= size {
                return root_id as i32;
            }
            let left_subtree = w.subtree_weight[left_id];
            if position < left_subtree {
                root_id = left_id;
            } else {
                position -= left_subtree;
                if left_id + 1 >= size {
                    return root_id as i32;
                }
                root_id = left_id + 1;
            }
        }
    }

    #[test]
    fn find_position_matches_plain_descent() {
        let mut rng = AvidaRng::new(11);
        for size in [1usize, 2, 3, 4, 5, 6, 7, 8, 31, 32, 33, 100, 3600] {
            let mut w = WeightedIndex::new(size);
            for id in 0..size {
                // Some zero weights, as for empty cells.
                let weight = if rng.get_int(4) == 0 {
                    0.0
                } else {
                    rng.get_double() * 10.0
                };
                w.set_weight(id, weight);
            }
            let total = w.total_weight();
            for _ in 0..2000 {
                let p = rng.get_double_max(total);
                assert_eq!(w.find_position(p), find_position_plain(&w, p), "{size}");
            }
            // At and past the total: the C++ UB cases.
            for p in [total, total * 1.5, f64::INFINITY] {
                assert_eq!(w.find_position(p), find_position_plain(&w, p), "{size}");
            }
        }
    }

    #[test]
    fn build_time_slicer_draws_seed_like_cpp() {
        let mut a = AvidaRng::new(7);
        let mut b = AvidaRng::new(7);
        let s = Probabilistic::from_world_rng(3, &mut a);
        let expect = AvidaRng::new(b.get_int(0x7FFF_FFFF)).original_seed();
        assert_eq!(s.rng().original_seed(), expect);
        let s = ProbabilisticIntegrated::from_world_rng(3, &mut a);
        let expect = AvidaRng::new(b.get_int(161_803_398)).original_seed();
        assert_eq!(s.rng().original_seed(), expect);
    }
}
