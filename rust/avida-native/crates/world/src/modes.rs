//! `PrintMODESData`: the MODES open-endedness metrics (change, novelty,
//! complexity, ecology) of Dolson, Vostinar, Wiser and Ofria, "The MODES
//! Toolbox: Measurements of Open-Ended Dynamics in Evolving Systems",
//! Artificial Life 25(1), 2019, as the reference code of that paper computes
//! them (the Empirical library's `OEETracker`, `CoalescenceFilter` and
//! `Skeletonize`, driven from the Avida fork's `cWorld`).
//!
//! **This action is an extension beyond the C++ program** (the C++ binary of
//! the reference commit has no `PrintMODESData`), and an observer: it never
//! changes what the run does (see "Observation only").
//!
//! ```text
//! u 0:1:end PrintMODESData <file> <resolution> <t> [nearest|any]
//! ```
//!
//! `file` is written to `data/`; `resolution` is the spacing of the samples
//! in generations of the population's average generation (`cStats`
//! `GetGeneration`); `t` is the persistence filter in generations, a positive
//! multiple of the resolution. The action is put in the events file to fire
//! every update and samples only when the average generation crosses a
//! multiple of the resolution. Instances with different arguments coexist
//! (each keeps its own snapshots; the skeletons are shared).
//!
//! # Definitions
//!
//! * **Taxon** = genotype (the tracker's), **component** = its skeleton.
//! * **Skeleton** (MODES 3.1.2, `emp::Skeletonize`): the fitness `f` of the
//!   genome on a test CPU; for each site `i` in turn the site alone replaced
//!   by a NULL instruction (`cInstSet::ActivateNullInst`: does nothing,
//!   mutation never produces it), fitness `f_i`; the site is kept iff
//!   `f_i < f`, strictly. The skeleton is the concatenation of the kept
//!   instruction symbols, **positions are not kept** (the reference
//!   behaviour, kept for comparability). Complexity of a skeleton is the
//!   number of kept instructions.
//! * **Persistence filter** (MODES 3.1.1, `CoalescenceFilter`): the snapshot
//!   of the living genotypes (id, organism count) from `t` generations ago;
//!   a genotype of it is persistent when it is alive now or has a living
//!   descendant now. `nearest` (the default) is the reference code's lookup:
//!   for each living genotype now walk up (first parent) to the first
//!   ancestor-or-self that was alive at the snapshot, and only that one is
//!   persistent. `any` is the paper's wording: every genotype of the
//!   snapshot with a living descendant. They differ when a snapshot genotype
//!   has a snapshot descendant that survived (the reference then leaves the
//!   ancestor out unless it survives itself or by another line).
//! * `F` = skeleton -> sum of snapshot counts of the persistent genotypes
//!   with that skeleton. `F'` = the previous sample's `F`; `S` = every
//!   skeleton of every earlier `F`. **change** = `|F \ F'|`, **novelty** =
//!   `|F \ S|` (then `S := S u F`; exact set, no Bloom filter),
//!   **complexity** = the longest skeleton in `F` (0 if empty),
//!   **ecology** = Shannon entropy (log2) of the counts of `F` (0 if empty).
//!
//! # Where this departs from the reference code
//!
//! * No row until a snapshot `t` generations old exists (the reference
//!   writes zeros).
//! * One sample per crossing of a multiple of the resolution, also when the
//!   average generation skips several in one update (the reference drops the
//!   sample when its rounded generation skips one). Samples of one jump take
//!   the same snapshot of the population.
//! * Skeletons are computed at filter time for the persistent genotypes
//!   only, not for every living genotype at every snapshot (exactly the same
//!   result, far fewer test runs), and cached per genotype id.
//! * The test CPU runs at the resource levels of an unexploited world, see
//!   below; the reference runs it at `initial=` levels, where a task paid
//!   from a limited resource whose `initial` is 0 earns nothing, so no site
//!   needed only for that task is informative.
//! * Complexity counts instructions, not characters of the symbol string
//!   (they differ for instruction sets of more than 62 instructions).
//! * The first parent only is followed in `nearest` mode (Empirical taxa have
//!   one parent); `any` follows all parents of a sexual genotype.
//!
//! # Test CPU resources
//!
//! The default test CPU (`RES_INITIAL`) gives a limited resource its
//! `initial=` level, 0 unless the environment sets it, and
//! `cEnvironment::DoProcesses` pays nothing from a resource at 0. In a world
//! whose tasks are paid from a per-cell resource (`geometry=grid`,
//! `inflow=..:outflow=..`, `frac=..:max=..`) every task would then be worth
//! nothing in the test CPU. The faithful Avida mechanism for testing at other
//! levels is `cCPUTestInfo::SetResourceOptions(RES_CONSTANT, history,
//! update, 0)` with a `cResourceHistory`, which the analyze mode uses for its
//! resource files. This action builds one entry from the environment: for
//! each resource the level of an unexploited cell,
//! `max(initial, inflow per cell / outflow)`, the inflow per cell being the
//! resource's inflow over the cells of its inflow box (`inflow` itself for a
//! global resource). That is the level an organism finds in a fresh patch of
//! the resource's region (1 unit in the 160 x 160 "continents" environment:
//! 0.05 in per cell, 5 percent out). Constant levels: the test organism does
//! not deplete them. Not modelled: diffusion, gradient resources (their
//! `initial` is used), instructions that read the cell's own resources
//! (`collect`, `get-res`: the test CPU's cell resources are 0).
//!
//! # Observation only
//!
//! The test CPU runs in a detached world: its own RNG (re-seeded for every
//! genotype, so a skeleton depends on the genome and the environment only),
//! null statistics, its own `std::rand` state and services, no population,
//! and a private copy of the instruction sets in which the NULL instruction
//! is activated (the sets the run uses are not edited: instruction numbering,
//! mutation and every output file stay as they are). The environment is the
//! run's own and is only read by the test CPU; the one task that writes it
//! (`xor-max`, which repoints its reaction) is refused when the action
//! starts. The test results are cached here, not on the genotype (nothing
//! else's `genome_test_metrics` is touched).

use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet, VecDeque};
use std::hash::Hash;
use std::sync::Arc;

use avida_apto::AvidaRng;
use avida_environment::{geometry, Resource, ResourceHistory};
use avida_genome::{Genome, InstSet, Instruction};
use avida_hardware::test_cpu::{CpuTestInfo, TestCpu, RES_CONSTANT, RES_INITIAL};
use avida_hardware::HardwareManager;

use crate::world::World;

/// The names this module parses.
pub const MODES_ACTIONS: &[&str] = &["PrintMODESData"];

/// Is `name` an action of this module (a port-only extension, unknown to the
/// C++ action library)?
pub fn is_extension(name: &str) -> bool {
    MODES_ACTIONS.iter().any(|n| n.eq_ignore_ascii_case(name))
}

/// Which ancestor lookup the persistence filter uses (see the module docs).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum FilterMode {
    /// The reference code: the nearest ancestor-or-self alive at the
    /// snapshot.
    Nearest,
    /// The paper: every snapshot genotype with a living descendant.
    Any,
}

impl FilterMode {
    /// The word the event line uses.
    pub fn name(self) -> &'static str {
        match self {
            FilterMode::Nearest => "nearest",
            FilterMode::Any => "any",
        }
    }
}

/// How many rows of each instance (and of each phylogeny file) the world
/// keeps in memory for the front ends; the oldest are dropped first. The
/// data files themselves are never truncated.
pub const HISTORY_CAP: usize = 2_000;

/// Appends `row`, dropping the oldest rows beyond `cap`.
pub(crate) fn push_capped<T>(rows: &mut VecDeque<T>, row: T, cap: usize) {
    while rows.len() >= cap.max(1) {
        rows.pop_front();
    }
    rows.push_back(row);
}

/// One row of a `PrintMODESData` file, as written.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ModesRow {
    /// Update at which the row was written.
    pub update: i32,
    /// Average generation then.
    pub generation: f64,
    /// Update of the snapshot that was evaluated.
    pub snapshot_update: i32,
    /// Change.
    pub change: i64,
    /// Novelty.
    pub novelty: i64,
    /// Complexity (instructions in the longest skeleton).
    pub complexity: i64,
    /// Ecology (bits).
    pub ecology: f64,
    /// Persistent genotypes.
    pub persistent: i64,
    /// Distinct persistent skeletons.
    pub skeletons: i64,
    /// Living genotypes in the snapshot.
    pub living: i64,
    /// Cumulative novelty.
    pub cumulative_novelty: i64,
}

/// What one `PrintMODESData` instance has written so far (the last
/// [`HISTORY_CAP`] rows), for display.
#[derive(Debug, Clone, PartialEq)]
pub struct ModesHistory {
    /// Output file.
    pub filename: String,
    /// Sample spacing, in generations.
    pub resolution: i64,
    /// Persistence filter length `t`, in generations.
    pub filter: i64,
    /// Ancestor lookup.
    pub mode: FilterMode,
    /// Rows written, oldest first.
    pub rows: Vec<ModesRow>,
}

/// A parsed `PrintMODESData`.
#[derive(Debug, Clone, PartialEq)]
pub struct ModesAction {
    /// Output file under `data/`.
    pub filename: String,
    /// Sample spacing, in generations.
    pub resolution: i64,
    /// Persistence filter length, in generations.
    pub filter: i64,
    /// Ancestor lookup.
    pub mode: FilterMode,
    /// Identifies the instance (file, resolution, filter, mode).
    key: Arc<str>,
}

/// The constructor of `name` (`None` when it is not an action of this
/// module). `Err` is the loud argument error.
pub fn parse(name: &str, args: &str) -> Option<Result<ModesAction, String>> {
    if !is_extension(name) {
        return None;
    }
    Some(parse_args(args))
}

fn parse_args(args: &str) -> Result<ModesAction, String> {
    const USAGE: &str =
        "Arguments: <string file> <int resolution [generations]> <int filter t [generations]> \
         [nearest|any]";
    let words: Vec<&str> = args.split_whitespace().collect();
    if words.len() < 3 || words.len() > 4 {
        return Err(format!(
            "PrintMODESData needs 3 or 4 arguments, got {} ({USAGE})",
            words.len()
        ));
    }
    let int = |what: &str, w: &str| -> Result<i64, String> {
        w.parse::<i64>()
            .map_err(|_| format!("PrintMODESData: {what} '{w}' is not an integer ({USAGE})"))
    };
    let resolution = int("resolution", words[1])?;
    let filter = int("filter t", words[2])?;
    if resolution < 1 {
        return Err(format!(
            "PrintMODESData: the resolution must be at least 1 generation, got {resolution}"
        ));
    }
    if filter < resolution || filter % resolution != 0 {
        return Err(format!(
            "PrintMODESData: the filter t must be a positive multiple of the resolution \
             ({resolution}), got {filter}"
        ));
    }
    let mode = match words.get(3).map(|w| w.to_ascii_lowercase()) {
        None => FilterMode::Nearest,
        Some(w) if w == "nearest" => FilterMode::Nearest,
        Some(w) if w == "any" => FilterMode::Any,
        Some(w) => {
            return Err(format!(
                "PrintMODESData: the filter mode must be 'nearest' or 'any', got '{w}'"
            ))
        }
    };
    let filename = words[0].to_string();
    let key: Arc<str> = format!(
        "{filename}\u{1}{resolution}\u{1}{filter}\u{1}{}",
        mode.name()
    )
    .into();
    Ok(ModesAction {
        filename,
        resolution,
        filter,
        mode,
        key,
    })
}

// ----------------------------------------------------------------------
// Pure core: sampling, persistence, metrics
// ----------------------------------------------------------------------

/// How many multiples of `resolution` the generation passed since the
/// sample index `last` (the highest index sampled so far): returns the new
/// index and the number of crossings. Generation 0 is not a sample; a
/// generation that goes down samples nothing until it passes the old index
/// again.
pub fn crossings(generation: f64, resolution: i64, last: i64) -> (i64, i64) {
    if generation.is_nan() || generation < 0.0 || resolution < 1 {
        return (last, 0);
    }
    let idx = (generation / resolution as f64).floor() as i64;
    if idx > last {
        (idx, idx - last)
    } else {
        (last, 0)
    }
}

/// The living genotypes of one moment: `(genotype id, organisms)`.
pub type Taxa = Arc<Vec<(i32, i32)>>;

/// A snapshot of the population.
#[derive(Debug, Clone)]
pub struct Snapshot {
    /// Update at which it was taken.
    pub update: i32,
    /// The living genotypes with their organism counts.
    pub taxa: Taxa,
}

/// The last `k + 1` snapshots; the oldest is the one `k` samples (so `t`
/// generations) ago once the window is full.
#[derive(Debug, Clone)]
pub struct Window {
    k: usize,
    queue: VecDeque<Snapshot>,
}

impl Window {
    /// A window for a filter of `k` resolutions.
    pub fn new(k: usize) -> Self {
        Window {
            k,
            queue: VecDeque::new(),
        }
    }

    /// Adds a snapshot; the snapshot `k` samples before it, once there is
    /// one (older ones are dropped).
    pub fn push(&mut self, s: Snapshot) -> Option<&Snapshot> {
        self.queue.push_back(s);
        while self.queue.len() > self.k + 1 {
            self.queue.pop_front();
        }
        if self.queue.len() == self.k + 1 {
            self.queue.front()
        } else {
            None
        }
    }

    /// The snapshots held.
    pub fn snapshots(&self) -> impl Iterator<Item = &Snapshot> {
        self.queue.iter()
    }
}

/// The persistent genotypes: those of `snapshot` (id -> count) that are
/// alive now (`living`, listed as nodes) or have a living descendant, as
/// `id -> node`. `parents` lists a node's parents (empty at a root).
pub fn persistent_taxa<N: Copy + Eq + Hash>(
    snapshot: &HashMap<i32, i32>,
    living: &[N],
    id_of: impl Fn(N) -> i32,
    parents: impl Fn(N) -> Vec<N>,
    mode: FilterMode,
) -> BTreeMap<i32, N> {
    let mut out = BTreeMap::new();
    match mode {
        FilterMode::Nearest => {
            // `emp::Systematics::GetCanopyExtantRoots`: from each living
            // taxon up to the first ancestor-or-self alive at the snapshot.
            // The answers of nodes on the way are kept, so that every node of
            // the union of the lineages is visited once.
            let mut memo: HashMap<N, Option<N>> = HashMap::new();
            let mut path: Vec<N> = Vec::new();
            for &start in living {
                path.clear();
                let mut cur = start;
                let found = loop {
                    if let Some(&r) = memo.get(&cur) {
                        break r;
                    }
                    if snapshot.contains_key(&id_of(cur)) {
                        memo.insert(cur, Some(cur));
                        break Some(cur);
                    }
                    path.push(cur);
                    match parents(cur).first() {
                        Some(&p) => cur = p,
                        None => break None,
                    }
                };
                for &n in &path {
                    memo.insert(n, found);
                }
                if let Some(n) = found {
                    out.insert(id_of(n), n);
                }
            }
        }
        FilterMode::Any => {
            let mut seen: HashSet<N> = HashSet::new();
            let mut stack: Vec<N> = living.to_vec();
            while let Some(n) = stack.pop() {
                if !seen.insert(n) {
                    continue;
                }
                if snapshot.contains_key(&id_of(n)) {
                    out.insert(id_of(n), n);
                }
                stack.extend(parents(n));
            }
        }
    }
    out
}

/// `F`: skeleton -> (summed snapshot count, length).
pub type Fmap = BTreeMap<String, (i64, usize)>;

/// The four metrics of one sample.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Metrics {
    /// `|F \ F'|`
    pub change: usize,
    /// `|F \ S|`
    pub novelty: usize,
    /// The longest skeleton of `F`.
    pub complexity: usize,
    /// Shannon entropy of the counts of `F`, in bits.
    pub ecology: f64,
}

/// What the metrics remember between samples: `F'` and `S`.
#[derive(Debug, Clone, Default)]
pub struct Accumulator {
    prev: BTreeSet<String>,
    seen: HashSet<String>,
    /// Novelty summed over the samples so far.
    pub cumulative_novelty: u64,
}

impl Accumulator {
    /// An accumulator before the first sample.
    pub fn new() -> Self {
        Self::default()
    }

    /// The metrics of the next sample `f`, which becomes `F'` and joins `S`.
    pub fn step(&mut self, f: &Fmap) -> Metrics {
        let mut change = 0;
        let mut novelty = 0;
        let mut complexity = 0;
        for (skel, &(_, len)) in f {
            if !self.prev.contains(skel) {
                change += 1;
            }
            if self.seen.insert(skel.clone()) {
                novelty += 1;
            }
            complexity = complexity.max(len);
        }
        let total: f64 = f.values().map(|&(c, _)| c as f64).sum();
        let mut ecology = 0.0;
        if total > 0.0 {
            for &(c, _) in f.values() {
                let p = c as f64 / total;
                ecology -= p * p.log2();
            }
        }
        self.prev = f.keys().cloned().collect();
        self.cumulative_novelty += novelty as u64;
        Metrics {
            change,
            novelty,
            complexity,
            ecology,
        }
    }
}

/// `emp::Skeletonize`: the sites of a genome of `len` sites whose knockout
/// strictly lowers the fitness. `fitness(None)` is the genome's own,
/// `fitness(Some(i))` that with site `i` replaced by the NULL instruction.
pub fn kept_sites(
    len: usize,
    fitness: &mut dyn FnMut(Option<usize>) -> Result<f64, String>,
) -> Result<Vec<usize>, String> {
    let base = fitness(None)?;
    let mut kept = Vec::new();
    for i in 0..len {
        if fitness(Some(i))? < base {
            kept.push(i);
        }
    }
    Ok(kept)
}

/// A skeleton: the symbols of the kept instructions and their number.
pub type Skeleton = Arc<(String, usize)>;

/// `F` of the persistent genotypes `ids` with the snapshot's `counts`.
pub fn collect_f(
    ids: impl IntoIterator<Item = i32>,
    counts: &HashMap<i32, i32>,
    skeletons: &HashMap<i32, Skeleton>,
) -> Fmap {
    let mut f = Fmap::new();
    for id in ids {
        let (Some(&count), Some(skel)) = (counts.get(&id), skeletons.get(&id)) else {
            continue;
        };
        let e = f.entry(skel.0.clone()).or_insert((0, skel.1));
        e.0 += i64::from(count);
    }
    f
}

// ----------------------------------------------------------------------
// The test CPU side
// ----------------------------------------------------------------------

/// `max(initial, inflow per cell / outflow)` for each resource (see the
/// module docs).
pub fn unexploited_levels(resources: &[Resource], world_x: i32, world_y: i32) -> Vec<f64> {
    resources
        .iter()
        .map(|r| {
            let mut steady = 0.0;
            if r.inflow > 0.0 && r.outflow > 0.0 {
                let spatial = r.geometry != geometry::GLOBAL && !r.deme_resource;
                let cells = if spatial {
                    let (x1, x2, y1, y2) = (r.inflow_x1, r.inflow_x2, r.inflow_y1, r.inflow_y2);
                    let (w, h) = if x1 < 0 || y1 < 0 || x2 < x1 || y2 < y1 {
                        (world_x.max(1), world_y.max(1))
                    } else {
                        (x2 - x1 + 1, y2 - y1 + 1)
                    };
                    f64::from(w) * f64::from(h)
                } else {
                    1.0
                };
                steady = r.inflow / cells / r.outflow;
            }
            r.initial.max(steady)
        })
        .collect()
}

/// The NULL instruction of a set that `ActivateNullInst` has run on.
pub(crate) fn null_of(set: &InstSet) -> Option<Instruction> {
    let null = set.inst_lib().inst_null();
    (0..set.size())
        .map(Instruction::new)
        .find(|&i| set.lib_function_index(i) == null)
}

/// How the test CPU gets its resources.
#[derive(Debug, Clone)]
pub enum TestLevels {
    /// The default test CPU: `initial=` levels (the reference code).
    Initial,
    /// `RES_CONSTANT` with this history.
    Constant(Arc<ResourceHistory>),
}

impl World {
    /// `PrintMODESData`, once per event firing.
    pub(crate) fn run_modes_action(&mut self, a: &ModesAction) -> Result<(), String> {
        // The no-op path: one division and a comparison.
        let gen = self.stats.sum_generation.average();
        let last = self
            .actions2
            .modes
            .instances
            .get(&a.key)
            .map_or(0, |i| i.last_idx);
        let (new_last, n) = crossings(gen, a.resolution, last);
        if n == 0 && self.actions2.modes.checked {
            return Ok(());
        }
        let mut st = std::mem::take(&mut self.actions2.modes);
        let r = self.modes_step(&mut st, a, gen, new_last, n);
        self.actions2.modes = st;
        r
    }

    fn modes_check(&self) -> Result<(), String> {
        let arb = self
            .systematics
            .genotypes()
            .ok_or("PrintMODESData needs the genotype tracker (no genotype arbiter)")?;
        if arb.disable_class() {
            return Err(
                "PrintMODESData needs genotype classification (DISABLE_GENOTYPE_CLASSIFICATION 0)"
                    .to_string(),
            );
        }
        let lib = &self.env.tasklib().0;
        if (0..lib.get_size()).any(|i| {
            lib.get_task(i)
                .is_some_and(|t| t.kind() == avida_tasks::TaskKind::XorMax)
        }) {
            return Err(
                "unsupported: PrintMODESData with the xor-max task (the test CPU would repoint \
                 the run's reaction; C++: cEnvironment::ChangeResource from Task_XorMax)"
                    .to_string(),
            );
        }
        Ok(())
    }

    fn modes_step(
        &mut self,
        st: &mut ModesState,
        a: &ModesAction,
        gen: f64,
        new_last: i64,
        n: i64,
    ) -> Result<(), String> {
        if !st.checked {
            self.modes_check()?;
            st.checked = true;
        }
        if n == 0 {
            st.instances
                .entry(a.key.clone())
                .or_insert_with(|| Instance::new(a));
            return Ok(());
        }
        // The living genotypes now.
        let taxa: Taxa = {
            let arb = self
                .systematics
                .genotypes()
                .ok_or("PrintMODESData: the genotype tracker disappeared")?;
            let mut v = Vec::new();
            for size in 1..=arb.best_size() {
                for h in arb.active_list(size) {
                    if let Some(g) = arb.genotype(h) {
                        if g.num_units() > 0 {
                            v.push((g.id(), g.num_units()));
                        }
                    }
                }
            }
            v.sort_unstable();
            Arc::new(v)
        };
        let update = self.stats.update();
        let mut inst = st
            .instances
            .remove(&a.key)
            .unwrap_or_else(|| Instance::new(a));
        inst.last_idx = new_last;
        let mut result = Ok(());
        for _ in 0..n {
            let front = inst
                .window
                .push(Snapshot {
                    update,
                    taxa: Arc::clone(&taxa),
                })
                .cloned();
            if let Some(front) = front {
                if let Err(e) = self.modes_evaluate(st, a, &mut inst, &front, update, gen) {
                    result = Err(e);
                    break;
                }
            }
        }
        st.instances.insert(a.key.clone(), inst);
        // Skeletons that no snapshot can ask for again.
        let mut needed: HashSet<i32> = HashSet::new();
        for i in st.instances.values() {
            for s in i.window.snapshots() {
                needed.extend(s.taxa.iter().map(|&(id, _)| id));
            }
        }
        st.cache.retain(|id, _| needed.contains(id));
        result
    }

    /// The row of one sample: the persistent genotypes of `front`, their
    /// skeletons, the four metrics.
    fn modes_evaluate(
        &mut self,
        st: &mut ModesState,
        a: &ModesAction,
        inst: &mut Instance,
        front: &Snapshot,
        update: i32,
        gen: f64,
    ) -> Result<(), String> {
        let counts: HashMap<i32, i32> = front.taxa.iter().copied().collect();
        let (persistent, genomes): (BTreeMap<i32, _>, HashMap<i32, Genome>) = {
            let arb = self
                .systematics
                .genotypes()
                .ok_or("PrintMODESData: the genotype tracker disappeared")?;
            let mut living = Vec::new();
            for size in 1..=arb.best_size() {
                for h in arb.active_list(size) {
                    if arb.genotype(h).is_some_and(|g| g.num_units() > 0) {
                        living.push(h);
                    }
                }
            }
            let id_of = |h| arb.genotype(h).map_or(-1, |g| g.id());
            let parents = |h| {
                arb.genotype(h)
                    .map(|g| g.parents().to_vec())
                    .unwrap_or_default()
            };
            let persistent = persistent_taxa(&counts, &living, id_of, parents, a.mode);
            let genomes = persistent
                .iter()
                .filter(|(id, _)| !st.cache.contains_key(id))
                .filter_map(|(&id, &h)| arb.genotype(h).map(|g| (id, g.genome().clone())))
                .collect();
            (persistent, genomes)
        };
        if !genomes.is_empty() {
            let levels = self.modes_levels();
            let hw = self.modes_private_hw();
            let mut ids: Vec<i32> = genomes.keys().copied().collect();
            ids.sort_unstable();
            for id in ids {
                let skel = self.modes_skeleton(&hw, &levels, &genomes[&id])?;
                st.skeletons_computed += 1;
                st.cache.insert(id, skel);
            }
        }
        let f = collect_f(persistent.keys().copied(), &counts, &st.cache);
        let m = inst.acc.step(&f);
        let n_persistent = persistent.len() as i64;
        let n_skeletons = f.len() as i64;
        let n_living = front.taxa.len() as i64;
        let k = a.filter / a.resolution;

        let df = self.out.static_with_path(&a.filename)?;
        df.write_comment("MODES open-endedness metrics (change, novelty, complexity, ecology).");
        df.write_comment(
            "Definitions: Dolson, Vostinar, Wiser, Ofria, 'The MODES Toolbox', Artificial Life \
             25(1), 2019; code: Empirical OEETracker / CoalescenceFilter / Skeletonize.",
        );
        df.write_comment(
            "An extension of this port (the C++ program has no PrintMODESData); \
             an observer, it does not change the run.",
        );
        df.write_comment(&format!(
            "Parameters: resolution {} generations, persistence filter t = {} generations \
             ({} samples), filter mode {} ({}).",
            a.resolution,
            a.filter,
            k,
            a.mode.name(),
            match a.mode {
                FilterMode::Nearest =>
                    "nearest ancestor-or-self alive at the snapshot, as the reference code",
                FilterMode::Any => "every snapshot genotype with a living descendant, as the paper",
            }
        ));
        df.write_comment(
            "Sampling: one sample whenever the average generation crosses a multiple of the \
             resolution; a row is written once a snapshot t generations old exists.",
        );
        df.write_comment(
            "Taxon = genotype; component = skeleton = the symbols of the instructions whose \
             replacement by NULL strictly lowers test-CPU fitness, positions not kept; \
             complexity = number of kept instructions.",
        );
        df.write_comment(
            "Test CPU: default test environment, but resources held constant at the level of an \
             unexploited cell, max(initial, inflow per cell / outflow).",
        );
        df.write_comment(
            "change = |F minus previous F|, novelty = |F minus every earlier F|, ecology = \
             Shannon entropy (bits) of the snapshot counts per skeleton, F over persistent \
             genotypes.",
        );
        let row = ModesRow {
            update,
            generation: gen,
            snapshot_update: front.update,
            change: m.change as i64,
            novelty: m.novelty as i64,
            complexity: m.complexity as i64,
            ecology: m.ecology,
            persistent: n_persistent,
            skeletons: n_skeletons,
            living: n_living,
            cumulative_novelty: inst.acc.cumulative_novelty as i64,
        };
        push_capped(&mut inst.history, row, HISTORY_CAP);
        df.write_time_stamp();
        df.write(update, "Update");
        df.write(gen, "Average generation");
        df.write(front.update, "Update of the snapshot evaluated");
        df.write(m.change as i64, "Change");
        df.write(m.novelty as i64, "Novelty");
        df.write(m.complexity as i64, "Complexity");
        df.write(m.ecology, "Ecology (bits)");
        df.write(n_persistent, "Persistent genotypes");
        df.write(n_skeletons, "Distinct persistent skeletons");
        df.write(n_living, "Living genotypes in the snapshot");
        df.write(inst.acc.cumulative_novelty as i64, "Cumulative novelty");
        df.endl();
        Ok(())
    }

    /// The `RES_CONSTANT` history of unexploited levels for the current
    /// environment.
    pub fn modes_levels(&self) -> TestLevels {
        let lib = self.env.resource_lib();
        let levels = unexploited_levels(lib.resources(), self.cfg.world_x, self.cfg.world_y);
        let mut h = ResourceHistory::new();
        h.add_entry(0, &levels);
        TestLevels::Constant(Arc::new(h))
    }

    /// A private copy of the instruction sets with the NULL instruction
    /// activated.
    pub fn modes_private_hw(&self) -> HardwareManager {
        let mut reg = self.hw.registry().duplicate();
        let names: Vec<String> = reg
            .inst_sets()
            .iter()
            .map(|s| s.inst_set_name().to_string())
            .collect();
        for n in names {
            if let Some(s) = reg.get_inst_set_mut(&n) {
                s.activate_null_inst();
            }
        }
        HardwareManager::from_registry(reg)
    }

    /// The skeleton of `genome`: one test of it and one per site with the
    /// site replaced by NULL, in a detached test world.
    pub fn modes_skeleton(
        &mut self,
        hw: &HardwareManager,
        levels: &TestLevels,
        genome: &Genome,
    ) -> Result<Skeleton, String> {
        let null = hw
            .registry()
            .inst_set_for_genome(genome)
            .and_then(|(_, set)| null_of(set))
            .ok_or_else(|| {
                "PrintMODESData: the genome's instruction set is not loaded".to_string()
            })?;
        let len = usize::try_from(genome.representation().size()).unwrap_or(0);
        let World {
            cfg,
            env,
            group_attack_counts,
            test_sterilize,
            ..
        } = self;
        let mut rng = AvidaRng::new(MODES_TEST_SEED);
        let mut test_genome = genome.clone();
        let mut kept: Vec<usize> = Vec::new();
        avida_analyze::world::with_detached_test_world(
            cfg,
            env,
            hw,
            group_attack_counts,
            *test_sterilize,
            &mut rng,
            &mut |tw| {
                let mut fitness = |site: Option<usize>| -> Result<f64, String> {
                    let original = site.map(|i| {
                        let old = genome.representation().get(i as i32);
                        test_genome.representation_mut().set(i as i32, null);
                        (i, old)
                    });
                    let mut cpu = TestCpu::new(tw)?;
                    let mut info = CpuTestInfo::default();
                    if let TestLevels::Constant(h) = levels {
                        info.set_resource_options(RES_CONSTANT, Some(Arc::clone(h)), 0, 0);
                    } else {
                        info.set_resource_options(RES_INITIAL, None, 0, 0);
                    }
                    let r = cpu.test_genome(tw, &mut info, &test_genome);
                    if let Some((i, old)) = original {
                        test_genome.representation_mut().set(i as i32, old);
                    }
                    r?;
                    Ok(info.genotype_fitness())
                };
                kept = kept_sites(len, &mut fitness)?;
                Ok(())
            },
        )?;
        let mut symbols = String::new();
        for &i in &kept {
            genome
                .representation()
                .get(i as i32)
                .push_symbol(&mut symbols);
        }
        Ok(Arc::new((symbols, kept.len())))
    }
}

/// The seed of the detached test worlds' RNG (every genotype starts from it).
const MODES_TEST_SEED: i32 = 0x4d_4f_44;

/// One instance of the action (one file, resolution, filter and mode).
#[derive(Debug)]
pub struct Instance {
    last_idx: i64,
    window: Window,
    acc: Accumulator,
    /// The arguments of the action that made this instance.
    action: ModesAction,
    /// The last [`HISTORY_CAP`] rows written, for display.
    history: VecDeque<ModesRow>,
}

impl Instance {
    fn new(a: &ModesAction) -> Self {
        Instance {
            last_idx: 0,
            window: Window::new((a.filter / a.resolution) as usize),
            acc: Accumulator::new(),
            action: a.clone(),
            history: VecDeque::new(),
        }
    }
}

/// State of the `PrintMODESData` instances of a world.
#[derive(Debug, Default)]
pub struct ModesState {
    checked: bool,
    instances: HashMap<Arc<str>, Instance>,
    /// Skeleton per genotype id, shared by the instances, dropped when no
    /// snapshot holds the id any more.
    cache: HashMap<i32, Skeleton>,
    /// Skeletons computed so far (each costs `length + 1` test runs).
    pub skeletons_computed: u64,
}

impl ModesState {
    /// What each instance has written so far, ordered by file, filter,
    /// resolution and mode. Reads only.
    pub fn history(&self) -> Vec<ModesHistory> {
        let mut v: Vec<ModesHistory> = self
            .instances
            .values()
            .map(|i| ModesHistory {
                filename: i.action.filename.clone(),
                resolution: i.action.resolution,
                filter: i.action.filter,
                mode: i.action.mode,
                rows: i.history.iter().copied().collect(),
            })
            .collect();
        v.sort_by(|a, b| {
            (&a.filename, a.filter, a.resolution, a.mode.name()).cmp(&(
                &b.filename,
                b.filter,
                b.resolution,
                b.mode.name(),
            ))
        });
        v
    }

    /// Genotype skeletons cached now.
    pub fn cached_skeletons(&self) -> usize {
        self.cache.len()
    }
}

#[cfg(test)]
mod tests;
