//! Port of `Avida::Systematics::GenotypeArbiter`
//! (avida-core/source/systematics/GenotypeArbiter.cc,
//! avida-core/include/private/avida/private/systematics/GenotypeArbiter.h).
//!
//! The arbiter owns every genotype (an arena indexed by [`GroupHandle`])
//! and the three kinds of `Apto::List<GenotypePtr, SparseVector>` the C++
//! keeps them in: the genome hash buckets, the per-size active lists and the
//! historic list. Their segment layout is reproduced exactly
//! ([`crate::sparse_list`]), because the order of those lists reaches the
//! output (dominant genotype, `.spop` historic rows, update sweeps).
//!
//! A genotype is freed from the arena at the point where the C++
//! `removeGenotype` deletes its list handle; after that no list or
//! reference count holds it, and a handle to it reads as `None`.
//!
//! World dependencies are parameters: the update number (`PerformUpdate`),
//! `THRESHOLD` and `DISABLE_GENOTYPE_CLASSIFICATION` (constructor), the
//! environment's action-trigger names (constructor), and the data manager
//! (provided values are read with [`GenotypeArbiter::provided_value`]).
//! Listeners become an event log ([`GenotypeArbiter::drain_events`]).

use std::collections::HashMap;

use avida_apto::DataFile;
use avida_config::string_util::{as_int, pop};
use avida_genome::{Genome, InstructionSequence};

use crate::arena::{Arena, GroupHandle};
use crate::genotype::Genotype;
use crate::group_data::{GenomeTestMetrics, SexualAncestry};
use crate::property::PropertyValue;
use crate::sparse_list::{Cursor, ListArena, ListId};
use crate::stats::DoubleSum;
use crate::unit::{ClassificationHints, GestationInfo, UnitInfo};

/// `GenotypeArbiter::HASH_SIZE`.
pub const HASH_SIZE: usize = 3203;

/// `GenotypeArbiter::EVENT_ADD_THRESHOLD`.
pub const EVENT_ADD_THRESHOLD: i32 = 0;
/// `GenotypeArbiter::EVENT_REMOVE_THRESHOLD`.
pub const EVENT_REMOVE_THRESHOLD: i32 = 1;

/// One `Arbiter::notifyListeners(g, t)` call.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SystematicsEvent {
    /// The group (may be freed by the time the event is read).
    pub group: GroupHandle,
    /// The group's id at the time of the event.
    pub group_id: i32,
    /// `EVENT_ADD_THRESHOLD` or `EVENT_REMOVE_THRESHOLD`.
    pub event: i32,
}

/// The statistics the arbiter provides to the data manager
/// (`setupProvidedData`), as last computed.
///
/// QUIRK: in C++ only `m_tot_genotypes` and `m_coalescent_depth` are
/// initialised; the rest (including the `m_num_threshold` /
/// `m_tot_threshold` counters that are incremented in place) start as
/// whatever the allocation held. The arbiter is a large object (3203 inline
/// lists), so on the reference platform it comes from fresh, zeroed pages;
/// they start at 0 here.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct GenotypeStats {
    /// `total`: `m_tot_genotypes`
    pub tot_genotypes: i32,
    /// `current`: `m_num_genotypes`
    pub num_genotypes: i32,
    /// `ancestral`: `m_num_historic_genotypes`
    pub num_historic_genotypes: i32,
    /// `current_threshold`: `m_num_threshold`
    pub num_threshold: i32,
    /// `total_threshold`: `m_tot_threshold`
    pub tot_threshold: i32,
    /// `coalescent_depth`: `m_coalescent_depth`
    pub coalescent_depth: i32,
    /// `ave_age`
    pub ave_age: f64,
    /// `ave_abundance`
    pub ave_abundance: f64,
    /// `ave_depth`
    pub ave_depth: f64,
    /// `ave_size`
    pub ave_size: f64,
    /// `ave_threshold_age`
    pub ave_threshold_age: f64,
    /// `stderr_age`
    pub stderr_age: f64,
    /// `stderr_abundance`
    pub stderr_abundance: f64,
    /// `stderr_depth`
    pub stderr_depth: f64,
    /// `stderr_size`
    pub stderr_size: f64,
    /// `stderr_threshold_age`
    pub stderr_threshold_age: f64,
    /// `var_age`
    pub var_age: f64,
    /// `var_abundance`
    pub var_abundance: f64,
    /// `var_depth`
    pub var_depth: f64,
    /// `var_size`
    pub var_size: f64,
    /// `var_threshold_age`
    pub var_threshold_age: f64,
    /// `entropy`
    pub entropy: f64,
    /// `dominant_id`: `m_dom_id`
    pub dom_id: i32,
}

/// The provided data names (after `systematics.<role>.`) with their
/// descriptions, in `setupProvidedData` order.
pub const GENOTYPE_PROVIDED: [(&str, &str); 23] = [
    ("total", "Total Number of Genotypes"),
    ("current", "Number of Current Genotypes"),
    ("ancestral", "Number of Ancestral Genotypes"),
    ("total_threshold", "Total Number of Threshold Genotypes"),
    ("current_threshold", "Number of Current Threshold Genotypes"),
    ("coalescent_depth", "Coalescent Depth"),
    ("ave_age", "Average Age"),
    ("ave_abundance", "Average Abundance"),
    ("ave_depth", "Average Depth"),
    ("ave_size", "Average "),
    ("ave_threshold_age", "Average Threshold Age"),
    ("stderr_age", "Age Standard Error"),
    ("stderr_abundance", "Abundance Standard Error"),
    ("stderr_depth", "Depth Standard Error"),
    ("stderr_size", "Size Standard Error"),
    ("stderr_threshold_age", "Threshold Age Standard Error"),
    ("var_age", "Age Variance"),
    ("var_abundance", "Abundance Variance"),
    ("var_depth", "Depth Variance"),
    ("var_size", "Size Variance"),
    ("var_threshold_age", "Threshold Age Variance"),
    ("entropy", "Genotypic Entropy"),
    ("dominant_id", "Dominant Genotype ID"),
];

/// `GenotypeArbiter::GenotypeIterator`: walks the active lists from the
/// best size down to size 1.
#[derive(Clone, Copy, Debug)]
pub struct GenotypeCursor {
    sz_i: i32,
    it: Cursor,
}

/// `GenotypeArbiter::hashGenome`: `sum((op + 3) * i) % 3203` in `unsigned
/// int` arithmetic.
pub fn hash_genome(genome: &InstructionSequence) -> usize {
    let mut total: u32 = 0;
    for (i, inst) in genome.iter().enumerate() {
        let term = (inst.op() as u32).wrapping_add(3).wrapping_mul(i as u32);
        total = total.wrapping_add(term);
    }
    (total % HASH_SIZE as u32) as usize
}

/// A pending `removeGenotype` call whose parents are being released.
struct RemoveFrame {
    genotype: GroupHandle,
    parents: Vec<GroupHandle>,
    next: usize,
}

/// Port of `Systematics::GenotypeArbiter`.
#[derive(Clone, Debug)]
pub struct GenotypeArbiter {
    role: String,
    threshold: i32,
    disable_class: bool,

    lists: ListArena<GroupHandle>,
    active_hash: Vec<ListId>,
    active_sz: Vec<ListId>,
    historic: ListId,
    genotypes: Arena<Genotype>,

    coalescent: Option<GroupHandle>,
    best: i32,
    next_id: i32,
    sz_count: Vec<i32>,
    cur_update: i32,

    stats: GenotypeStats,

    env_action_average: Vec<String>,
    env_action_count: Vec<String>,

    record_events: bool,
    events: Vec<SystematicsEvent>,
}

impl GenotypeArbiter {
    /// `GenotypeArbiter(world, role, threshold, disable_class)`.
    /// `trigger_names` are the environment's action trigger ids in
    /// `GetActionTriggerIDs()` iteration order.
    pub fn new(role: &str, threshold: i32, disable_class: bool, trigger_names: &[String]) -> Self {
        let mut lists = ListArena::new();
        let active_hash = (0..HASH_SIZE).map(|_| lists.new_list()).collect();
        let active_sz = vec![lists.new_list()];
        let historic = lists.new_list();
        GenotypeArbiter {
            role: role.to_string(),
            threshold,
            disable_class,
            lists,
            active_hash,
            active_sz,
            historic,
            genotypes: Arena::default(),
            coalescent: None,
            best: 0,
            next_id: 1,
            sz_count: Vec::new(),
            cur_update: -1,
            stats: GenotypeStats {
                coalescent_depth: -1,
                ..GenotypeStats::default()
            },
            env_action_average: trigger_names
                .iter()
                .map(|t| format!("environment.triggers.{t}.average"))
                .collect(),
            env_action_count: trigger_names
                .iter()
                .map(|t| format!("environment.triggers.{t}.count"))
                .collect(),
            record_events: false,
            events: Vec::new(),
        }
    }

    // ------------------------------------------------------------------
    // Accessors
    // ------------------------------------------------------------------

    /// `Role()`.
    pub fn role(&self) -> &str {
        &self.role
    }

    /// `m_threshold` (the `THRESHOLD` setting).
    pub fn threshold(&self) -> i32 {
        self.threshold
    }

    /// `m_disable_class` (`DISABLE_GENOTYPE_CLASSIFICATION`).
    pub fn disable_class(&self) -> bool {
        self.disable_class
    }

    /// The genotype behind a handle (`None` once it was pruned).
    pub fn genotype(&self, g: GroupHandle) -> Option<&Genotype> {
        self.genotypes.get(g)
    }

    /// Mutable access, for the public setters (`SetLastBirthCell` etc.).
    pub fn genotype_mut(&mut self, g: GroupHandle) -> Option<&mut Genotype> {
        self.genotypes.get_mut(g)
    }

    /// Number of genotypes alive in the arena (active, historic, or
    /// detached but still referenced).
    pub fn num_genotypes_alive(&self) -> usize {
        self.genotypes.len()
    }

    /// `m_cur_update`.
    pub fn current_update(&self) -> i32 {
        self.cur_update
    }

    /// `m_best`: the largest active size.
    pub fn best_size(&self) -> i32 {
        self.best
    }

    /// `m_next_id`.
    pub fn next_id(&self) -> i32 {
        self.next_id
    }

    /// The statistics (`UpdateProvidedValues` and the running counters).
    pub fn stats(&self) -> &GenotypeStats {
        &self.stats
    }

    /// `m_coalescent`.
    pub fn coalescent(&self) -> Option<GroupHandle> {
        self.coalescent.filter(|&c| self.genotypes.contains(c))
    }

    /// `EnvironmentActionTriggerAverageIDs()`.
    pub fn trigger_average_ids(&self) -> &[String] {
        &self.env_action_average
    }

    /// `EnvironmentActionTriggerCountIDs()`.
    pub fn trigger_count_ids(&self) -> &[String] {
        &self.env_action_count
    }

    /// `m_active_sz[size]` in list order (empty past the end).
    pub fn active_list(&self, size: i32) -> Vec<GroupHandle> {
        self.sz_list(size)
            .map(|l| self.lists.to_vec(l))
            .unwrap_or_default()
    }

    /// `m_active_sz.GetSize()`.
    pub fn active_list_count(&self) -> usize {
        self.active_sz.len()
    }

    /// `m_historic` in list order.
    pub fn historic(&self) -> Vec<GroupHandle> {
        self.lists.to_vec(self.historic)
    }

    /// `m_active_hash[bucket]` in list order.
    pub fn hash_bucket(&self, bucket: usize) -> Vec<GroupHandle> {
        self.active_hash
            .get(bucket)
            .map(|&l| self.lists.to_vec(l))
            .unwrap_or_default()
    }

    fn sz_list(&self, size: i32) -> Option<ListId> {
        usize::try_from(size)
            .ok()
            .and_then(|i| self.active_sz.get(i).copied())
    }

    fn id_of(&self, g: GroupHandle) -> i32 {
        self.genotypes.get(g).map_or(-1, |x| x.id)
    }

    fn num_units(&self, g: GroupHandle) -> i32 {
        self.genotypes.get(g).map_or(0, |x| x.num_organisms)
    }

    fn a_refs(&self, g: GroupHandle) -> i32 {
        self.genotypes.get(g).map_or(0, |x| x.a_refs)
    }

    // ------------------------------------------------------------------
    // Listeners
    // ------------------------------------------------------------------

    /// Turns event recording on or off (the stand-in for
    /// `AttachListener` / `DetachListener`). QUIRK: the C++ notifies the
    /// listeners of an `Apto::Set<Listener*>` in pointer-hash order; here
    /// there is one ordered log.
    pub fn set_event_recording(&mut self, on: bool) {
        self.record_events = on;
    }

    /// Takes the recorded events, oldest first.
    pub fn drain_events(&mut self) -> Vec<SystematicsEvent> {
        std::mem::take(&mut self.events)
    }

    fn notify_listeners(&mut self, g: GroupHandle, event: i32) {
        if self.record_events {
            let group_id = self.id_of(g);
            self.events.push(SystematicsEvent {
                group: g,
                group_id,
                event,
            });
        }
    }

    // ------------------------------------------------------------------
    // Classification
    // ------------------------------------------------------------------

    /// `Arbiter::ClassifyNewUnit(u, hints)`: classification without
    /// parents (injected units).
    pub fn classify_new_unit(
        &mut self,
        u: &UnitInfo<'_>,
        hints: Option<&ClassificationHints>,
    ) -> GroupHandle {
        self.classify_new_unit_with_parents(u, None, hints)
    }

    /// The private `ClassifyNewUnit(u, parents, hints)`: find the hinted
    /// genotype (`hints["id"]`, active lists first, then historic), else a
    /// matching genotype in the genome's hash bucket, else create one.
    pub fn classify_new_unit_with_parents(
        &mut self,
        u: &UnitInfo<'_>,
        parents: Option<&[GroupHandle]>,
        hints: Option<&ClassificationHints>,
    ) -> GroupHandle {
        let list_num = hash_genome(u.genome.representation());
        let mut found: Option<GroupHandle> = None;

        if let Some(gid_str) = hints.and_then(|h| h.get("id")) {
            let gid = as_int(gid_str);

            // Search all lists attempting to locate the referenced genotype by ID
            'lists: for i in 0..self.active_sz.len() {
                let mut it = self.lists.begin(self.active_sz[i]);
                while self.lists.next(&mut it) {
                    let Some(g) = self.lists.get(&it) else {
                        break;
                    };
                    if self.id_of(g) == gid {
                        found = Some(g);
                        self.notify_new_unit(g, u);
                        break 'lists;
                    }
                }
            }

            if found.is_none() {
                let mut it = self.lists.begin(self.historic);
                while self.lists.next(&mut it) {
                    let Some(g) = self.lists.get(&it) else {
                        break;
                    };
                    if self.id_of(g) != gid {
                        continue;
                    }
                    found = Some(g);
                    self.reactivate_from_historic(g, u);
                    // The C++ keeps scanning the historic list after this,
                    // but ids are unique, so nothing else can match.
                    break;
                }
            }
        }

        // No hints or unable to locate hinted genome, search for a matching genotype
        if found.is_none() {
            let mut it = self.lists.begin(self.active_hash[list_num]);
            while self.lists.next(&mut it) {
                let Some(g) = self.lists.get(&it) else {
                    break;
                };
                if self.genotypes.get(g).is_some_and(|x| x.matches(u)) {
                    found = Some(g);
                    self.notify_new_unit(g, u);
                    break;
                }
            }
        }

        if let Some(g) = found {
            return g;
        }

        // No matching genotype (hinted or otherwise), so create a new one
        let parent_info: Vec<(GroupHandle, i32, i32)> = if self.disable_class {
            Vec::new()
        } else {
            parents
                .unwrap_or(&[])
                .iter()
                .filter_map(|&p| self.genotypes.get(p).map(|x| (p, x.id, x.depth)))
                .collect()
        };
        let id = self.next_id;
        self.next_id = self.next_id.wrapping_add(1);
        let genotype = Genotype::new_founder(
            id,
            u,
            self.cur_update,
            &parent_info,
            self.env_action_count.len(),
        );
        let g = self.genotypes.insert(genotype);
        for &(p, _, _) in &parent_info {
            if let Some(x) = self.genotypes.get_mut(p) {
                x.p_refs = x.p_refs.wrapping_add(1);
            }
        }
        self.lists.push(self.active_hash[list_num], g, None);
        let n = self.num_units(g);
        self.push_active(n, g, false);
        self.stats.tot_genotypes = self.stats.tot_genotypes.wrapping_add(1);
        if n > self.best {
            self.best = n;
            self.make_threshold(g);
        }
        g
    }

    /// The historic branch of the hinted search: move the genotype back to
    /// the active lists and add the unit.
    fn reactivate_from_historic(&mut self, g: GroupHandle, u: &UnitInfo<'_>) {
        let bucket = match self.genotypes.get(g) {
            Some(x) => hash_genome(x.genome.representation()),
            None => return,
        };
        self.lists.push(self.active_hash[bucket], g, None);
        if let Some(h) = self.genotypes.get(g).and_then(|x| x.handle) {
            self.lists.handle_remove(h); // Remove from historic list
        }
        let n = self.num_units(g);
        self.push_active(n, g, false);
        if let Some(x) = self.genotypes.get_mut(g) {
            x.reactivate();
        }
        self.notify_new_unit(g, u);
        self.stats.tot_genotypes = self.stats.tot_genotypes.wrapping_add(1);
        // Unreachable in practice (NotifyNewUnit already raised m_best), kept
        // as in the C++.
        if self.num_units(g) > self.best {
            self.best = self.num_units(g);
            self.make_threshold(g);
        }
    }

    /// `SetThreshold()`, `SetName(nameGenotype(size))`, the threshold
    /// counters and the `EVENT_ADD_THRESHOLD` notification.
    fn make_threshold(&mut self, g: GroupHandle) {
        let size = match self.genotypes.get(g) {
            Some(x) => x.genome.representation().size(),
            None => return,
        };
        let name = self.name_genotype(size);
        if let Some(x) = self.genotypes.get_mut(g) {
            x.threshold = true;
            x.name = name;
        }
        self.stats.num_threshold = self.stats.num_threshold.wrapping_add(1);
        self.stats.tot_threshold = self.stats.tot_threshold.wrapping_add(1);
        self.notify_listeners(g, EVENT_ADD_THRESHOLD);
    }

    /// `resizeActiveList(size)` then `m_active_sz[size].Push` (front) or
    /// `PushRear`, storing the entry handle in the genotype.
    fn push_active(&mut self, size: i32, g: GroupHandle, front: bool) {
        self.resize_active_list(size);
        let Some(l) = self.sz_list(size) else {
            // Negative sizes index before the C++ array; not reachable
            // through the reference-counting API.
            return;
        };
        let Some(x) = self.genotypes.get_mut(g) else {
            return;
        };
        if front {
            self.lists.push(l, g, Some(&mut x.handle));
        } else {
            self.lists.push_rear(l, g, Some(&mut x.handle));
        }
    }

    /// `resizeActiveList(size)`: the size lists only grow.
    fn resize_active_list(&mut self, size: i32) {
        if size < 0 {
            return;
        }
        while self.active_sz.len() <= size as usize {
            let l = self.lists.new_list();
            self.active_sz.push(l);
        }
    }

    /// `Genotype::NotifyNewUnit(u)`.
    fn notify_new_unit(&mut self, g: GroupHandle, u: &UnitInfo<'_>) {
        let Some(x) = self.genotypes.get_mut(g) else {
            return;
        };
        x.notify_new_unit_counts(u);
        let n = x.num_organisms;
        self.adjust_genotype(g, n.wrapping_sub(1), n);
        if let Some(x) = self.genotypes.get_mut(g) {
            x.a_refs = x.a_refs.wrapping_add(1);
        }
    }

    /// `Genotype::ClassifyNewUnit(u, parents)`: a unit born from a parent
    /// in genotype `g` joins `g` when it matches, otherwise the arbiter
    /// classifies it with `parents`.
    pub fn group_classify_new_unit(
        &mut self,
        g: GroupHandle,
        u: &UnitInfo<'_>,
        parents: Option<&[GroupHandle]>,
    ) -> GroupHandle {
        let Some(x) = self.genotypes.get_mut(g) else {
            return self.classify_new_unit_with_parents(u, parents, None);
        };
        x.births.inc();
        if x.matches(u) {
            x.breed_true.inc();
            x.total_organisms = x.total_organisms.wrapping_add(1);
            x.num_organisms = x.num_organisms.wrapping_add(1);
            let n = x.num_organisms;
            self.adjust_genotype(g, n.wrapping_sub(1), n);
            if let Some(x) = self.genotypes.get_mut(g) {
                x.a_refs = x.a_refs.wrapping_add(1);
            }
            return g;
        }
        x.breed_out.inc();
        self.classify_new_unit_with_parents(u, parents, None)
    }

    /// `Genotype::HandleUnitGestation(u)`.
    pub fn handle_unit_gestation(&mut self, g: GroupHandle, info: &GestationInfo) {
        if let Some(x) = self.genotypes.get_mut(g) {
            x.handle_unit_gestation(info);
        }
    }

    /// `Genotype::RemoveUnit()`: a member unit died.
    pub fn remove_unit(&mut self, g: GroupHandle) {
        let Some(x) = self.genotypes.get_mut(g) else {
            return;
        };
        x.deaths.inc();
        x.a_refs = x.a_refs.wrapping_sub(1);
        x.num_organisms = x.num_organisms.wrapping_sub(1);
        let n = x.num_organisms;
        self.adjust_genotype(g, n.wrapping_add(1), n);
    }

    /// `Group::AddActiveReference()`.
    pub fn add_active_reference(&mut self, g: GroupHandle) {
        if let Some(x) = self.genotypes.get_mut(g) {
            x.a_refs = x.a_refs.wrapping_add(1);
        }
    }

    /// `Genotype::RemoveActiveReference()`: the last active reference
    /// shrinks the genotype to size 0 (and removes it if nothing else holds
    /// it).
    pub fn remove_active_reference(&mut self, g: GroupHandle) {
        let Some(x) = self.genotypes.get_mut(g) else {
            return;
        };
        x.a_refs = x.a_refs.wrapping_sub(1);
        if x.a_refs == 0 {
            let n = x.num_organisms;
            self.adjust_genotype(g, n, 0);
        }
    }

    /// `Group::AddPassiveReference()`.
    pub fn add_passive_reference(&mut self, g: GroupHandle) {
        if let Some(x) = self.genotypes.get_mut(g) {
            x.p_refs = x.p_refs.wrapping_add(1);
        }
    }

    /// `Group::RemovePassiveReference()`: only the count changes; an
    /// unreferenced historic genotype is pruned at the next
    /// [`GenotypeArbiter::perform_update`].
    pub fn remove_passive_reference(&mut self, g: GroupHandle) {
        if let Some(x) = self.genotypes.get_mut(g) {
            x.p_refs = x.p_refs.wrapping_sub(1);
        }
    }

    /// `AdjustGenotype(genotype, old_size, new_size)`.
    fn adjust_genotype(&mut self, g: GroupHandle, old_size: i32, new_size: i32) {
        // Remove from old size list
        if let Some(h) = self.genotypes.get(g).and_then(|x| x.handle) {
            self.lists.handle_remove(h);
        }
        if self.coalescent == Some(g) {
            self.coalescent = None;
        }

        // Handle best genotype pointer
        let was_best = old_size != 0 && old_size == self.best;
        if was_best
            && self
                .sz_list(old_size)
                .is_some_and(|l| self.lists.is_empty(l))
        {
            self.best -= 1;
            while self.best > 0 {
                if self
                    .sz_list(self.best)
                    .is_some_and(|l| !self.lists.is_empty(l))
                {
                    break;
                }
                self.best -= 1;
            }
        }

        // Handle defunct genotypes
        if new_size == 0 && self.a_refs(g) == 0 {
            self.remove_genotype(g);
            return;
        }

        // Add to new size list
        if was_best && self.best == new_size {
            // Special case to keep the current best genotype as best when
            // shrinking to the same size as other genotypes
            self.push_active(new_size, g, true);
        } else {
            self.push_active(new_size, g, false);
            if new_size > self.best {
                self.best = new_size;
            }
        }

        let is_threshold = self.genotypes.get(g).is_some_and(|x| x.threshold);
        if !is_threshold && (new_size >= self.threshold || Some(g) == self.get_best()) {
            self.make_threshold(g);
        }
    }

    /// `getBest()`: the first genotype of the best size list.
    fn get_best(&self) -> Option<GroupHandle> {
        if self.best == 0 {
            return None;
        }
        self.sz_list(self.best).and_then(|l| self.lists.first(l))
    }

    /// The dominant genotype as `getBest()` defines it (none when no
    /// genotype has units).
    pub fn best_genotype(&self) -> Option<GroupHandle> {
        self.get_best()
    }

    /// `nameGenotype(size)`: `"%03d-%s"` with a five-letter base-26 count
    /// per genome size.
    fn name_genotype(&mut self, size: i32) -> String {
        let idx = size.max(0) as usize;
        if self.sz_count.len() <= idx {
            self.sz_count.resize(idx + 1, 0);
        }
        let mut num = self.sz_count[idx];
        self.sz_count[idx] = num.wrapping_add(1);
        let mut alpha = [b'a'; 5];
        for slot in alpha.iter_mut().rev() {
            *slot = (num % 26) as u8 + b'a';
            num /= 26;
        }
        format!("{:03}-{}", size, String::from_utf8_lossy(&alpha))
    }

    /// `removeGenotype(genotype)`, with the recursion over parents made
    /// iterative (same order of operations).
    fn remove_genotype(&mut self, g: GroupHandle) {
        let mut stack: Vec<RemoveFrame> = Vec::new();
        if let Some(f) = self.remove_genotype_enter(g) {
            stack.push(f);
        }
        while let Some(top) = stack.last_mut() {
            if top.next < top.parents.len() {
                let p = top.parents[top.next];
                top.next += 1;
                if let Some(x) = self.genotypes.get_mut(p) {
                    x.p_refs = x.p_refs.wrapping_sub(1);
                }
                self.update_coalescent();
                // Pre-check for active genotypes to avoid recursion costs
                if self.genotypes.contains(p) && self.a_refs(p) == 0 {
                    if let Some(f) = self.remove_genotype_enter(p) {
                        stack.push(f);
                    }
                }
            } else {
                let done = top.genotype;
                stack.pop();
                self.remove_genotype_exit(done);
            }
        }
    }

    /// The part of `removeGenotype` before the parent loop; `None` when it
    /// returns early.
    fn remove_genotype_enter(&mut self, g: GroupHandle) -> Option<RemoveFrame> {
        let x = self.genotypes.get(g)?;
        if x.a_refs != 0 {
            return None;
        }
        if x.active {
            let list_num = hash_genome(x.genome.representation());
            self.lists.remove(self.active_hash[list_num], g);
            let cur_update = self.cur_update;
            let x = self.genotypes.get_mut(g)?;
            x.deactivate(cur_update);
            self.lists.push(self.historic, g, Some(&mut x.handle));
        }
        if self.genotypes.get(g)?.threshold {
            self.stats.num_threshold = self.stats.num_threshold.wrapping_sub(1);
            self.notify_listeners(g, EVENT_REMOVE_THRESHOLD);
            self.genotypes.get_mut(g)?.threshold = false;
        }
        let x = self.genotypes.get(g)?;
        if x.p_refs != 0 {
            return None;
        }
        Some(RemoveFrame {
            genotype: g,
            parents: x.parents.clone(),
            next: 0,
        })
    }

    /// The end of `removeGenotype`: drop the historic entry and its handle.
    /// Nothing refers to the genotype any more, so it leaves the arena.
    fn remove_genotype_exit(&mut self, g: GroupHandle) {
        if let Some(h) = self.genotypes.get_mut(g).and_then(|x| x.handle.take()) {
            self.lists.handle_remove(h); // Remove from historic list
            self.lists.delete_handle(h);
        }
        self.genotypes.remove(g);
    }

    /// `updateCoalescent()`: walk first parents from the best genotype; the
    /// deepest ancestor still referenced by more than one line is the
    /// coalescent genotype.
    fn update_coalescent(&mut self) {
        if let Some(c) = self.coalescent {
            if let Some(x) = self.genotypes.get(c) {
                if x.a_refs > 0 || x.p_refs > 1 {
                    return;
                }
            }
        }
        let Some(mut test_gen) = self.get_best().filter(|_| self.best != 0) else {
            self.coalescent = None;
            self.stats.coalescent_depth = -1;
            return;
        };
        let first_parent = |s: &Self, g: GroupHandle| -> Option<GroupHandle> {
            s.genotypes.get(g).and_then(|x| x.parents.first().copied())
        };
        let mut found_gen = test_gen;
        let mut parent_gen = first_parent(self, found_gen);
        while let Some(p) = parent_gen {
            if let Some(x) = self.genotypes.get(test_gen) {
                if x.a_refs > 0 || x.p_refs > 1 {
                    found_gen = test_gen;
                }
            }
            test_gen = p;
            parent_gen = first_parent(self, test_gen);
        }
        self.coalescent = Some(found_gen);
        self.stats.coalescent_depth = self.genotypes.get(found_gen).map_or(-1, |x| x.depth);
    }

    // ------------------------------------------------------------------
    // Lookup and iteration
    // ------------------------------------------------------------------

    /// `Group(id)`: active lists from the best size down to 0, then the
    /// historic list.
    pub fn group(&self, id: i32) -> Option<GroupHandle> {
        let mut i = self.best;
        while i >= 0 {
            if let Some(l) = self.sz_list(i) {
                if let Some(g) = self.lists.iter(l).find(|&g| self.id_of(g) == id) {
                    return Some(g);
                }
            }
            i -= 1;
        }
        self.lists
            .iter(self.historic)
            .find(|&g| self.id_of(g) == id)
    }

    /// `Begin()`: a [`GenotypeCursor`] standing before the first genotype.
    pub fn begin(&self) -> GenotypeCursor {
        let it = match self.sz_list(self.best) {
            Some(l) => self.lists.begin(l),
            None => Cursor::default(),
        };
        GenotypeCursor {
            sz_i: self.best,
            it,
        }
    }

    /// `GenotypeIterator::Next()`: the rest of the current size list, then
    /// each smaller non-empty list down to size 1.
    pub fn next(&self, c: &mut GenotypeCursor) -> Option<GroupHandle> {
        if !self.lists.next(&mut c.it) {
            c.sz_i -= 1;
            while c.sz_i > 0 {
                if let Some(l) = self.sz_list(c.sz_i) {
                    if !self.lists.is_empty(l) {
                        c.it = self.lists.begin(l);
                        self.lists.next(&mut c.it);
                        break;
                    }
                }
                c.sz_i -= 1;
            }
        }
        self.lists.get(&c.it)
    }

    /// `GenotypeIterator::Get()`.
    pub fn get(&self, c: &GenotypeCursor) -> Option<GroupHandle> {
        self.lists.get(&c.it)
    }

    /// All genotypes `Begin()` / `Next()` visit, in order.
    pub fn iter_groups(&self) -> Vec<GroupHandle> {
        let mut c = self.begin();
        let mut out = Vec::new();
        while let Some(g) = self.next(&mut c) {
            out.push(g);
        }
        out
    }

    /// The dominant genotype as print actions find it:
    /// `ArbiterForRole("genotype")->Begin()->Next()`. QUIRK: when no
    /// genotype has units (`m_best == 0`) this is the first genotype of the
    /// size-0 list, i.e. one kept alive only by active references.
    pub fn dominant(&self) -> Option<GroupHandle> {
        let mut c = self.begin();
        self.next(&mut c)
    }

    // ------------------------------------------------------------------
    // Properties
    // ------------------------------------------------------------------

    /// `g->Properties().Has(name)`.
    pub fn has_property(&self, g: GroupHandle, name: &str) -> bool {
        self.genotypes
            .get(g)
            .is_some_and(|x| x.has_property(name, &self.env_action_average))
    }

    /// `g->Properties().Get(name)`.
    pub fn property(&self, g: GroupHandle, name: &str) -> PropertyValue {
        self.genotypes.get(g).map_or(PropertyValue::Null, |x| {
            x.property(name, &self.env_action_average)
        })
    }

    // ------------------------------------------------------------------
    // Update
    // ------------------------------------------------------------------

    /// `PerformUpdate(ctx, current_update)`: reset the per-update counters
    /// of threshold genotypes and prune unreferenced historic genotypes.
    pub fn perform_update(&mut self, current_update: i32) {
        // +1 since PerformUpdate happens at end of updates, but m_cur_update
        // is used during
        self.cur_update = current_update.wrapping_add(1);

        let lists: Vec<ListId> = if self.active_sz.len() < HASH_SIZE {
            self.active_sz.clone()
        } else {
            self.active_hash.clone()
        };
        for l in lists {
            for g in self.lists.to_vec(l) {
                if let Some(x) = self.genotypes.get_mut(g) {
                    if x.threshold {
                        x.update_reset();
                    }
                }
            }
        }

        let mut it = self.lists.begin(self.historic);
        while self.lists.next(&mut it) {
            // QUIRK: a removal under the iterator can free the segment it
            // stands on; the C++ then reads a null entry and crashes. Stop
            // instead (see crate::sparse_list).
            let Some(g) = self.lists.get(&it) else {
                break;
            };
            if self
                .genotypes
                .get(g)
                .is_some_and(|x| x.reference_count() == 0)
            {
                self.remove_genotype(g);
            }
        }
    }

    /// `UpdateProvidedValues(current_update)`: recompute the provided
    /// statistics over the active genotypes of size >= 1.
    pub fn update_provided_values(&mut self, current_update: i32) {
        let mut sum_age = DoubleSum::new();
        let mut sum_abundance = DoubleSum::new();
        let mut sum_depth = DoubleSum::new();
        let mut sum_size = DoubleSum::new();
        let mut sum_threshold_age = DoubleSum::new();

        // Pre-calculate the total number of units that are currently active
        // (used in entropy calculation)
        let mut tot_units: i32 = 0;
        for &l in self.active_sz.iter().skip(1) {
            for g in self.lists.iter(l) {
                tot_units = tot_units.wrapping_add(self.num_units(g));
            }
        }

        let mut entropy = 0.0;
        let mut active_count: i32 = 0;
        for &l in self.active_sz.iter().skip(1) {
            active_count = active_count.wrapping_add(self.lists.len(l));
            for g in self.lists.iter(l) {
                let Some(bg) = self.genotypes.get(g) else {
                    continue;
                };
                let abundance = bg.num_organisms;
                let age = current_update.wrapping_sub(bg.update_born);
                sum_age.add(f64::from(age), f64::from(abundance));
                sum_abundance.add1(f64::from(abundance));
                sum_depth.add(f64::from(bg.depth), f64::from(abundance));
                sum_size.add(
                    f64::from(bg.genome.representation().size()),
                    f64::from(abundance),
                );
                let p = f64::from(abundance) / f64::from(tot_units);
                let partial_ent = if abundance == tot_units {
                    0.0
                } else {
                    -(p * p.ln())
                };
                entropy += partial_ent;
                if bg.threshold {
                    sum_threshold_age.add(f64::from(age), f64::from(abundance));
                }
            }
        }

        let s = &mut self.stats;
        s.entropy = entropy;
        s.num_genotypes = active_count;
        s.num_historic_genotypes = self.lists.len(self.historic);
        s.ave_age = sum_age.average();
        s.ave_abundance = sum_abundance.average();
        s.ave_depth = sum_depth.average();
        s.ave_size = sum_size.average();
        s.ave_threshold_age = sum_threshold_age.average();
        s.stderr_age = sum_age.std_error();
        s.stderr_abundance = sum_abundance.std_error();
        s.stderr_depth = sum_depth.std_error();
        s.stderr_size = sum_size.std_error();
        s.stderr_threshold_age = sum_threshold_age.std_error();
        s.var_age = sum_age.variance();
        s.var_abundance = sum_abundance.variance();
        s.var_depth = sum_depth.variance();
        s.var_size = sum_size.variance();
        s.var_threshold_age = sum_threshold_age.variance();
        let best = self.get_best();
        self.stats.dom_id = best.map_or(-1, |g| self.id_of(g));
    }

    /// `Provides()`: the full data ids, `systematics.<role>.<name>`.
    pub fn provided_ids(&self) -> Vec<String> {
        GENOTYPE_PROVIDED
            .iter()
            .map(|(n, _)| format!("systematics.{}.{}", self.role, n))
            .collect()
    }

    /// `GetProvidedValue(data_id)`: `Int` or `Double` as wrapped by the
    /// C++ (`None` for unknown ids, where the C++ asserts).
    pub fn provided_value(&self, data_id: &str) -> Option<PropertyValue> {
        let prefix = format!("systematics.{}.", self.role);
        let name = data_id.strip_prefix(&prefix)?;
        let s = &self.stats;
        use PropertyValue::{Double, Int};
        Some(match name {
            "total" => Int(s.tot_genotypes),
            "current" => Int(s.num_genotypes),
            "ancestral" => Int(s.num_historic_genotypes),
            "total_threshold" => Int(s.tot_threshold),
            "current_threshold" => Int(s.num_threshold),
            "coalescent_depth" => Int(s.coalescent_depth),
            "ave_age" => Double(s.ave_age),
            "ave_abundance" => Double(s.ave_abundance),
            "ave_depth" => Double(s.ave_depth),
            "ave_size" => Double(s.ave_size),
            "ave_threshold_age" => Double(s.ave_threshold_age),
            "stderr_age" => Double(s.stderr_age),
            "stderr_abundance" => Double(s.stderr_abundance),
            "stderr_depth" => Double(s.stderr_depth),
            "stderr_size" => Double(s.stderr_size),
            "stderr_threshold_age" => Double(s.stderr_threshold_age),
            "var_age" => Double(s.var_age),
            "var_abundance" => Double(s.var_abundance),
            "var_depth" => Double(s.var_depth),
            "var_size" => Double(s.var_size),
            "var_threshold_age" => Double(s.var_threshold_age),
            "entropy" => Double(s.entropy),
            "dominant_id" => Int(s.dom_id),
            _ => return None,
        })
    }

    /// `DescribeProvidedValue(data_id)`.
    pub fn describe_provided_value(&self, data_id: &str) -> Option<&'static str> {
        let prefix = format!("systematics.{}.", self.role);
        let name = data_id.strip_prefix(&prefix)?;
        GENOTYPE_PROVIDED
            .iter()
            .find(|(n, _)| *n == name)
            .map(|(_, d)| *d)
    }

    // ------------------------------------------------------------------
    // Legacy save / load
    // ------------------------------------------------------------------

    /// `Genotype::LegacySave(df)` for one genotype (the row is not ended).
    pub fn save_genotype(&self, g: GroupHandle, df: &mut DataFile) -> bool {
        let Some(x) = self.genotypes.get(g) else {
            return false;
        };
        let parent_ids: Vec<i32> = x.parents.iter().map(|&p| self.id_of(p)).collect();
        x.legacy_save(df, &parent_ids);
        true
    }

    /// `LegacySave(df)`: one row per historic genotype, in historic list
    /// order (most recently deactivated or loaded first).
    pub fn legacy_save(&self, df: &mut DataFile) -> bool {
        for g in self.lists.iter(self.historic) {
            self.save_genotype(g, df);
            df.endl();
        }
        true
    }

    /// `LegacyLoad(props)`: create a historic genotype from one `.spop`
    /// line (as `cInitFile::GetLineAsDict` returns it) and push it on the
    /// front of the historic list. Fails, without changes, when a parent id
    /// is not a known genotype (the C++ dereferences NULL there).
    pub fn legacy_load(&mut self, props: &HashMap<String, String>) -> Result<GroupHandle, String> {
        let id = self.next_id;
        let (mut genotype, parent_ids) =
            Genotype::from_props(id, props, self.env_action_count.len());
        let mut parents = Vec::with_capacity(parent_ids.len());
        for pid in parent_ids {
            match self.group(pid) {
                Some(p) => parents.push(p),
                None => return Err(format!("parent genotype {pid} of genotype {id} not found")),
            }
        }
        self.next_id = self.next_id.wrapping_add(1);
        genotype.parents = parents.clone();
        let g = self.genotypes.insert(genotype);
        for p in parents {
            if let Some(x) = self.genotypes.get_mut(p) {
                x.p_refs = x.p_refs.wrapping_add(1);
            }
        }
        if let Some(x) = self.genotypes.get_mut(g) {
            self.lists.push(self.historic, g, Some(&mut x.handle));
        }
        Ok(g)
    }

    // ------------------------------------------------------------------
    // Attached data
    // ------------------------------------------------------------------

    /// The first two parents named by a genotype's `parents` property,
    /// looked up with `Group(id)`. QUIRK: the C++ dereferences a parent id
    /// that is no longer known; here it counts as absent.
    fn sa_parents(&self, g: GroupHandle) -> [Option<GroupHandle>; 2] {
        let mut out = [None, None];
        let Some(x) = self.genotypes.get(g) else {
            return out;
        };
        let mut s = x.parent_str.clone();
        let mut i = 0;
        while !s.is_empty() {
            let word = pop(&mut s, ',');
            let p = self.group(as_int(&word));
            if i < 2 {
                out[i] = p;
            }
            i += 1;
        }
        out
    }

    /// `GetData<SexualAncestry>()`, creating and attaching the record (and
    /// those of the ancestors it needs) when missing, as
    /// `SexualAncestry::SexualAncestry(g)` does.
    pub fn sexual_ancestry(&mut self, g: GroupHandle) -> Option<SexualAncestry> {
        self.genotypes.get(g)?;
        let mut stack = vec![g];
        while let Some(&top) = stack.last() {
            let Some(x) = self.genotypes.get(top) else {
                stack.pop();
                continue;
            };
            if x.data.sexual_ancestry.is_some() {
                stack.pop();
                continue;
            }
            let parents = self.sa_parents(top);
            let pending: Vec<GroupHandle> = parents
                .iter()
                .flatten()
                .copied()
                .filter(|&p| {
                    self.genotypes
                        .get(p)
                        .is_some_and(|y| y.data.sexual_ancestry.is_none())
                })
                .collect();
            if pending.is_empty() {
                let sa_of = |p: Option<GroupHandle>| {
                    p.and_then(|p| self.genotypes.get(p))
                        .and_then(|y| y.data.sexual_ancestry)
                };
                let id = self.id_of(top);
                let sa = SexualAncestry::from_parents(
                    id,
                    sa_of(parents[0]).as_ref(),
                    sa_of(parents[1]).as_ref(),
                );
                if let Some(x) = self.genotypes.get_mut(top) {
                    x.data.sexual_ancestry = Some(sa);
                }
                stack.pop();
            } else {
                // Parent 1 first, as the C++ constructor recursion does.
                stack.extend(pending.into_iter().rev());
            }
        }
        self.genotypes.get(g).and_then(|x| x.data.sexual_ancestry)
    }

    /// `GetData<SexualAncestry>()->GetPhyloDistance(other)`, creating both
    /// records as needed.
    pub fn phylo_distance(&mut self, a: GroupHandle, b: GroupHandle) -> Option<i32> {
        let sa = self.sexual_ancestry(a)?;
        let sb = self.sexual_ancestry(b)?;
        Some(sa.phylo_distance(&sb))
    }

    /// `GenomeTestMetrics::GetMetrics(world, ctx, g)`: the attached test
    /// results, computed on first use by `test` (the hardware layer's
    /// `cTestCPU::TestGenome`) from `Genome(g->Properties().Get("genome"))`.
    pub fn genome_test_metrics(
        &mut self,
        g: GroupHandle,
        test: impl FnOnce(&Genome) -> GenomeTestMetrics,
    ) -> Option<&GenomeTestMetrics> {
        let x = self.genotypes.get_mut(g)?;
        if x.data.test_metrics.is_none() {
            let genome = Genome::from_string(&x.genome.as_string());
            x.data.test_metrics = Some(test(&genome));
        }
        x.data.test_metrics.as_ref()
    }
}
