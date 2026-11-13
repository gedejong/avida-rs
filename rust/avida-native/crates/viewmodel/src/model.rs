//! The snapshot data types. Everything here is plain owned data (`Clone`,
//! `Send`, `Sync`); large per-cell arrays sit behind `Arc` so a frame can be
//! handed to a UI thread or kept in a history without copying them.
//!
//! The C++ viewers (`avida-core/source/viewer/` and the ncurses
//! `targets/avida-viewer`) read the same quantities straight from
//! `cPopulation`, `cOrganism`, `cPhenotype` and `cHardwareBase`; this layer
//! copies them out once per frame instead.

use std::sync::Arc;

use crate::ancestry::Family;
use crate::legend::Scale;

/// A per-cell quantity the map can show.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum LayerId {
    /// Genotype of the occupant. Carried by [`MapFrame::genotype_id`]
    /// rather than by a float layer, and rendered with a categorical
    /// palette.
    Genotype,
    /// `cPhenotype::GetFitness()`
    Fitness,
    /// `cPhenotype::GetMerit().GetDouble()`
    Merit,
    /// `cPhenotype::GetGestationTime()`
    Gestation,
    /// `cPhenotype::GetAge()` (updates lived)
    Age,
    /// `cPhenotype::GetGeneration()`
    Generation,
    /// `cPhenotype::GetGenomeLength()`
    GenomeLen,
    /// `cOrganism::GetLineageLabel()`
    Lineage,
    /// The family the organism belongs to in the phylogeny of the living
    /// population and its place within that family
    /// ([`crate::ancestry::Layout::value`]); categorical, the families
    /// are in [`MapFrame::families`].
    Ancestry,
    /// `cPhenotype::ParentTrue()` as 0/1: the organism is an exact copy of
    /// its parent.
    BreedTrue,
    /// `cPhenotype::IsModified()` as 0/1.
    Modified,
    /// `cHardwareBase::GetNumThreads()`
    Threads,
    /// Number of distinct tasks with `GetLastTaskCount()[k] > 0`.
    TaskCount,
    /// `cPhenotype::GetLastTaskCount()[k]` for task `k`.
    Task(usize),
    /// Amount of resource `r` in the cell (`cResourceCount::
    /// GetFrozenCellResVal`); defined for empty cells as well.
    Resource(usize),
}

impl LayerId {
    /// The fixed per-organism layers, in display order (without the
    /// world-dependent [`LayerId::Task`] and [`LayerId::Resource`]).
    pub const FIXED: [LayerId; 13] = [
        LayerId::Genotype,
        LayerId::Fitness,
        LayerId::Merit,
        LayerId::Gestation,
        LayerId::Age,
        LayerId::Generation,
        LayerId::GenomeLen,
        LayerId::Lineage,
        LayerId::BreedTrue,
        LayerId::Modified,
        LayerId::Threads,
        LayerId::TaskCount,
        LayerId::Ancestry,
    ];

    /// A short identifier (`fitness`, `task[3]`, `resource[0]`). Use
    /// [`EnvView::layer_label`] for a name that spells out tasks and
    /// resources.
    pub fn key(self) -> String {
        match self {
            LayerId::Genotype => "genotype".into(),
            LayerId::Fitness => "fitness".into(),
            LayerId::Merit => "merit".into(),
            LayerId::Gestation => "gestation".into(),
            LayerId::Age => "age".into(),
            LayerId::Generation => "generation".into(),
            LayerId::GenomeLen => "genome_len".into(),
            LayerId::Lineage => "lineage".into(),
            LayerId::Ancestry => "ancestry".into(),
            LayerId::BreedTrue => "breed_true".into(),
            LayerId::Modified => "modified".into(),
            LayerId::Threads => "threads".into(),
            LayerId::TaskCount => "task_count".into(),
            LayerId::Task(k) => format!("task[{k}]"),
            LayerId::Resource(r) => format!("resource[{r}]"),
        }
    }

    /// True for the layers that show classes, not magnitudes: they have
    /// no value range, no scale and their own colours.
    pub fn is_categorical(self) -> bool {
        matches!(self, LayerId::Genotype | LayerId::Ancestry)
    }

    /// True for the 0/1 layers.
    pub fn is_boolean(self) -> bool {
        matches!(self, LayerId::BreedTrue | LayerId::Modified)
    }

    /// True for layers whose value belongs to the occupant (NaN in empty
    /// cells); false for resources, which every cell has.
    pub fn is_per_organism(self) -> bool {
        !matches!(self, LayerId::Resource(_))
    }

    /// The scale a front end should start with: logarithmic for fitness and
    /// merit, which span orders of magnitude once tasks are rewarded.
    pub fn default_scale(self) -> Scale {
        match self {
            LayerId::Fitness | LayerId::Merit => Scale::Log,
            _ => Scale::Linear,
        }
    }
}

/// What a front end wants in the next [`Frame`]. The header and the
/// environment summary are always captured.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Subscription {
    /// Map layers to capture. The map (and its genotype ids and task bits)
    /// is captured only when this is non-empty.
    pub layers: Vec<LayerId>,
    /// The cell whose organism (phenotype and CPU state) to copy out.
    pub inspect_cell: Option<i32>,
    /// How many genotypes, most abundant first, to list in
    /// [`Frame::genotypes`] (`usize::MAX` for all of them).
    pub genotypes_top_n: usize,
    /// The genotype whose ancestry to trace into [`Frame::lineage`].
    pub lineage_of: Option<i32>,
}

/// Where the run stands inside its update cycle when a frame is captured.
///
/// Between updates every part of a frame describes the update that just
/// ended. Inside an update (a front end paused or stepped a few time
/// slices) the per-cell and per-organism parts (map, inspected organism,
/// genotype abundances) are mid-update, while the statistics row still
/// belongs to the last finished update: `cStats` averages are computed in
/// `ProcessPostUpdate`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct UpdateProgress {
    /// An update has begun and not yet ended.
    pub in_update: bool,
    /// Time slices of the update in progress already run (0 between
    /// updates).
    pub slices_done: i32,
    /// `UD_size` of the update in progress (0 between updates).
    pub slices_total: i32,
}

impl UpdateProgress {
    /// Inside an update, `done` of `total` slices run.
    pub fn inside(done: i32, total: i32) -> UpdateProgress {
        UpdateProgress {
            in_update: true,
            slices_done: done,
            slices_total: total,
        }
    }

    /// The fraction of the update in progress already run, `None` between
    /// updates.
    pub fn fraction(&self) -> Option<f64> {
        (self.in_update && self.slices_total > 0)
            .then(|| (f64::from(self.slices_done) / f64::from(self.slices_total)).clamp(0.0, 1.0))
    }
}

/// One snapshot of the world, taken between updates.
#[derive(Clone, Debug, PartialEq)]
pub struct Frame {
    /// `cStats::GetUpdate()` at capture time.
    pub update: i32,
    /// World geometry and population statistics.
    pub header: Header,
    /// Per-cell layers, when [`Subscription::layers`] is non-empty.
    pub map: Option<MapFrame>,
    /// The dominant genotype as `PrintDominantData` finds it.
    pub dominant: Option<GenotypeView>,
    /// The most abundant genotypes, at most [`Subscription::genotypes_top_n`].
    pub genotypes: Vec<GenotypeRow>,
    /// The organism in [`Subscription::inspect_cell`], if that cell is
    /// occupied.
    pub inspect: Option<OrganismView>,
    /// Resources, reactions and tasks of the environment.
    pub env: EnvView,
    /// Instruction names of instruction set 0, indexed by opcode (what
    /// genome listings need without inspecting an organism).
    pub inst_names: Arc<[String]>,
    /// Every layer the world has ([`crate::all_layers`]): the fixed ones,
    /// one per task, one per resource.
    pub layers: Vec<LayerId>,
    /// The ancestry of [`Subscription::lineage_of`]: that genotype first,
    /// then its parent, and so on back to the founder (at most
    /// [`crate::capture::LINEAGE_MAX`] genotypes). Empty when nothing was
    /// asked for or the genotype is unknown.
    pub lineage: Vec<GenotypeRow>,
}

/// World-level facts and the statistics row of the current update.
#[derive(Clone, Debug, PartialEq)]
pub struct Header {
    /// `WORLD_X`
    pub world_x: i32,
    /// `WORLD_Y`
    pub world_y: i32,
    /// `RANDOM_SEED` as configured (the world RNG may have been reseeded by
    /// a `SetRandomSeed` action since).
    pub random_seed: i32,
    /// Name of instruction set 0 (the default set organisms are injected
    /// with).
    pub inst_set: String,
    /// `Avida2Driver::m_done`: the run has reached `Exit` or died out.
    pub done: bool,
    /// Whether the frame was captured inside an update, and how far.
    pub progress: UpdateProgress,
    /// The last update that has ended (`-1` before the first): the update
    /// [`Header::stats`] describes. Equal to [`Frame::update`] between
    /// updates, one less inside one.
    pub last_finished_update: i32,
    /// The statistics a time-series plot would record for this update.
    pub stats: UpdateRow,
}

/// One row of population statistics, as `cStats` holds them after
/// `ProcessPostUpdate` (the per-update counters `births`, `deaths`,
/// `breed_true` and `executed` cover the update that just finished).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct UpdateRow {
    /// `GetUpdate()`
    pub update: i32,
    /// `cPopulation::GetNumOrganisms()`
    pub orgs: i32,
    /// `m_num_genotypes` (`systematics.genotype.current`)
    pub genotypes: i32,
    /// `m_threshold_genotypes`
    pub threshold_genotypes: i32,
    /// `sum_fitness.Average()`
    pub ave_fitness: f64,
    /// `sum_merit.Average()`
    pub ave_merit: f64,
    /// `sum_gestation.Average()`
    pub ave_gestation: f64,
    /// `sum_generation.Average()`
    pub ave_generation: f64,
    /// `sum_creature_age.Average()`
    pub ave_age: f64,
    /// `sum_mem_size.Average()` (memory size, i.e. genome length while not
    /// copying)
    pub ave_mem_size: f64,
    /// `max_fitness`
    pub max_fitness: f64,
    /// `max_merit`
    pub max_merit: f64,
    /// `num_births`
    pub births: i32,
    /// `num_deaths`
    pub deaths: i32,
    /// `num_breed_true`
    pub breed_true: i32,
    /// `num_executed`: instructions executed in the update.
    pub executed: i32,
    /// `m_num_threads`: threads summed over all organisms.
    pub threads: i32,
    /// `tot_organisms`: organisms ever placed.
    pub tot_organisms: i32,
    /// `GetTaskLastCount(k)`: organisms that performed task `k` in their
    /// last gestation.
    pub task_last_count: Vec<i32>,
    /// Total amount of each resource: the global amount for a global
    /// resource, the sum over cells for a spatial one (both without
    /// bringing the resource counts up to date, see `capture`).
    pub resources: Vec<f64>,
}

/// Per-cell data for the map. Cell `c` is at `x = c % w`, `y = c / w`
/// (`cPopulationCell` ids).
#[derive(Clone, Debug, PartialEq)]
pub struct MapFrame {
    /// `WORLD_X`
    pub w: i32,
    /// `WORLD_Y`
    pub h: i32,
    /// Genotype id of each cell's occupant; -1 for an empty cell,
    /// [`crate::capture::UNCLASSIFIED`] for an occupant without a genotype.
    pub genotype_id: Arc<[i32]>,
    /// Ids of the most abundant genotypes, most abundant first; at most
    /// [`crate::palette::CATEGORICAL_SLOTS`]. These get the categorical
    /// colours, all other genotypes grey.
    pub top_genotypes: Arc<[i32]>,
    /// The subscribed float layers, in subscription order (without
    /// [`LayerId::Genotype`], and without task or resource ids out of
    /// range). NaN marks an empty cell in per-organism layers.
    pub layers: Vec<(LayerId, Arc<[f32]>)>,
    /// Number of tasks in the environment.
    pub num_tasks: usize,
    /// For each cell, `words_per_cell` words of a bit set: bit `k` is set
    /// when the occupant's `GetLastTaskCount()[k] > 0`.
    pub task_bits: Arc<[u64]>,
    /// Words of [`MapFrame::task_bits`] per cell (`ceil(num_tasks / 64)`,
    /// at least 1).
    pub words_per_cell: usize,
    /// Cells held by each of [`MapFrame::top_genotypes`], in that order.
    pub top_counts: Arc<[u32]>,
    /// Every genotype with an organism on the map, by ascending id, with
    /// its first cell (in id order) and its number of cells.
    pub genotype_cells: Arc<[GenotypeCells]>,
    /// The families of [`LayerId::Ancestry`], largest first; empty when
    /// that layer was not captured.
    pub families: Arc<[Family]>,
}

/// Where one genotype lives on the map.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct GenotypeCells {
    /// Genotype id ([`crate::capture::UNCLASSIFIED`] for occupants without
    /// one).
    pub id: i32,
    /// The lowest cell id holding it.
    pub first_cell: i32,
    /// Cells holding it.
    pub cells: u32,
}

impl MapFrame {
    /// `w * h`
    pub fn num_cells(&self) -> usize {
        self.genotype_id.len()
    }

    /// The values of one float layer, if it was captured.
    pub fn layer(&self, id: LayerId) -> Option<&[f32]> {
        self.layers
            .iter()
            .find(|(l, _)| *l == id)
            .map(|(_, v)| &v[..])
    }

    /// Whether the occupant of `cell` performed task `task` in its last
    /// gestation.
    pub fn task_done(&self, cell: usize, task: usize) -> bool {
        if task >= self.num_tasks {
            return false;
        }
        let word = cell * self.words_per_cell + task / 64;
        self.task_bits
            .get(word)
            .is_some_and(|w| w & (1u64 << (task % 64)) != 0)
    }

    /// The categorical colour slot of a genotype: its index in
    /// [`MapFrame::top_genotypes`].
    pub fn genotype_slot(&self, genotype_id: i32) -> Option<usize> {
        if genotype_id < 0 {
            return None;
        }
        self.top_genotypes.iter().position(|&g| g == genotype_id)
    }

    /// The family a [`LayerId::Ancestry`] value belongs to.
    pub fn family_of_value(&self, value: f32) -> Option<Family> {
        let (slot, _) = crate::ancestry::decode(value)?;
        let slot = slot?;
        self.families.iter().copied().find(|f| f.slot == slot)
    }

    /// Number of occupied cells.
    pub fn occupied(&self) -> usize {
        self.genotype_id.iter().filter(|&&g| g != -1).count()
    }

    /// Where genotype `genotype_id` lives, if it has an organism.
    pub fn genotype_cells_of(&self, genotype_id: i32) -> Option<GenotypeCells> {
        self.genotype_cells
            .binary_search_by_key(&genotype_id, |c| c.id)
            .ok()
            .map(|i| self.genotype_cells[i])
    }

    /// Cells holding genotype `genotype_id`.
    pub fn cells_of(&self, genotype_id: i32) -> u32 {
        self.genotype_cells_of(genotype_id).map_or(0, |c| c.cells)
    }

    /// The first cell (in id order) holding genotype `genotype_id`.
    pub fn first_cell_of(&self, genotype_id: i32) -> Option<i32> {
        self.genotype_cells_of(genotype_id).map(|c| c.first_cell)
    }
}

/// One genotype in the abundance-ordered list.
#[derive(Clone, Debug, PartialEq)]
pub struct GenotypeRow {
    /// Position in the abundance order (0 is the dominant genotype);
    /// [`GenotypeRow::NOT_RANKED`] for an extinct genotype listed in
    /// [`Frame::lineage`].
    pub rank: usize,
    /// `ID()`
    pub id: i32,
    /// The first id of the `parents` property (the only one in an asexual
    /// run); -1 for an injected genotype.
    pub parent_id: i32,
    /// `name` property (`%03d-aaaaa`).
    pub name: String,
    /// `NumUnits()`: living organisms of this genotype.
    pub abundance: i32,
    /// `Depth()` in the phylogeny.
    pub depth: i32,
    /// `update_born` property.
    pub update_born: i32,
    /// Length of the genome.
    pub genome_len: i32,
    /// `threshold` property: the genotype has reached `THRESHOLD`.
    pub threshold: bool,
    /// `ave_fitness` property.
    pub ave_fitness: f64,
    /// `ave_metabolic_rate` property (average merit).
    pub ave_merit: f64,
    /// `ave_gestation_time` property.
    pub ave_gestation: f64,
    /// `last_births` property.
    pub last_births: i32,
}

impl GenotypeRow {
    /// [`GenotypeRow::rank`] of a genotype outside the abundance order.
    pub const NOT_RANKED: usize = usize::MAX;

    /// Whether the genotype still has living organisms.
    pub fn is_alive(&self) -> bool {
        self.abundance > 0
    }
}

/// The dominant genotype with the fields `PrintDominantData` prints.
#[derive(Clone, Debug, PartialEq)]
pub struct GenotypeView {
    /// The list fields.
    pub row: GenotypeRow,
    /// The genome as instruction symbols.
    pub genome: String,
    /// `ave_copy_size` property.
    pub ave_copy_size: f64,
    /// `ave_exe_size` property.
    pub ave_exe_size: f64,
    /// `ave_repro_rate` property.
    pub ave_repro_rate: f64,
    /// `max_fitness` property.
    pub max_fitness: f64,
    /// `last_breed_true` property.
    pub last_breed_true: i32,
    /// `last_breed_in` property.
    pub last_breed_in: i32,
    /// `parents` property (comma-separated ids).
    pub parents: String,
}

/// One organism, copied out of its cell.
#[derive(Clone, Debug, PartialEq)]
pub struct OrganismView {
    /// Cell id.
    pub cell: i32,
    /// Cell x.
    pub x: i32,
    /// Cell y.
    pub y: i32,
    /// `cOrganism::GetID()`
    pub org_id: i32,
    /// Genotype id (-1 if unclassified).
    pub genotype_id: i32,
    /// Genotype name (empty if unclassified).
    pub genotype_name: String,
    /// `GetLineageLabel()`
    pub lineage_label: i32,
    /// The cell this one faces (`cPopulationCell::GetCellFaced`), if any.
    pub faced_cell: Option<i32>,
    /// The phenotype summary.
    pub phenotype: PhenotypeView,
    /// The cell's input values (`cPopulationCell::m_inputs`).
    pub cell_inputs: Vec<i32>,
    /// `m_input_buf`, newest first.
    pub input_buf: Vec<i32>,
    /// `m_output_buf`, newest first.
    pub output_buf: Vec<i32>,
    /// The virtual CPU.
    pub cpu: CpuView,
}

/// The `cPhenotype` fields a viewer shows.
#[derive(Clone, Debug, PartialEq)]
pub struct PhenotypeView {
    /// `GetMerit().GetDouble()`
    pub merit: f64,
    /// `GetFitness()`
    pub fitness: f64,
    /// `GetCurBonus()`
    pub cur_bonus: f64,
    /// `GetGestationTime()`
    pub gestation_time: i32,
    /// `GetAge()`
    pub age: i32,
    /// `GetGeneration()`
    pub generation: i32,
    /// `GetGenomeLength()`
    pub genome_length: i32,
    /// `GetCopiedSize()`
    pub copied_size: i32,
    /// `GetExecutedSize()`
    pub executed_size: i32,
    /// `GetCPUCyclesUsed()`: restarts at every divide.
    pub cpu_cycles_used: i32,
    /// CPU cycles over the organism's whole life (not in the C++): one per
    /// instruction executed (per time slice on TransSMT hardware, which
    /// counts no cycles), never restarting.
    pub lifetime_cpu_cycles: u64,
    /// `GetTimeUsed()`
    pub time_used: i32,
    /// `GetNumDivides()`
    pub num_divides: i32,
    /// `GetBirthUpdate()`
    pub birth_update: i32,
    /// `ParentTrue()`
    pub parent_true: bool,
    /// `IsModified()`
    pub is_modified: bool,
    /// `IsMultiThread()`
    pub is_multi_thread: bool,
    /// `GetToDie()`
    pub to_die: bool,
    /// `GetCurTaskCount()`: tasks done so far in this gestation.
    pub cur_task_count: Vec<i32>,
    /// `GetLastTaskCount()`: tasks done in the last gestation.
    pub last_task_count: Vec<i32>,
    /// `GetCurReactionCount()`
    pub cur_reaction_count: Vec<i32>,
    /// `GetLastReactionCount()`
    pub last_reaction_count: Vec<i32>,
}

/// A few numbers about one organism, cheap enough to read after every time
/// slice (the session's run-until and follow-offspring checks).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct OrgProbe {
    /// The cell it lives in.
    pub cell: i32,
    /// `cOrganism::GetID()`
    pub org_id: i32,
    /// CPU cycles over the organism's whole life
    /// ([`PhenotypeView::lifetime_cpu_cycles`]): grows by one per
    /// instruction it executes and never restarts.
    pub cpu_cycles: u64,
    /// `GetNumDivides()`
    pub num_divides: i32,
}

/// Instruction-set memory flags (`cCPUMemory`).
pub mod mem_flags {
    pub use avida_genome::cpu_memory::{
        MASK_COPIED as COPIED, MASK_COPYMUT as COPY_MUT, MASK_EXECUTED as EXECUTED,
        MASK_INJECTED as INJECTED, MASK_MUTATED as MUTATED, MASK_POINTMUT as POINT_MUT,
    };
}

/// One site of CPU memory.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MemCell {
    /// The opcode (`Instruction::GetOp()`).
    pub op: u8,
    /// The last character of the instruction symbol (`a`-`z`, `A`-`Z`,
    /// `0`-`9`, `_` for the error instruction); opcodes 62 and above also
    /// carry a prefix, see [`MemCell::symbol`].
    pub sym: char,
    /// Index into [`CpuView::inst_names`]; `u16::MAX` when the opcode is
    /// not in the instruction set.
    pub name_idx: u16,
    /// [`mem_flags`] bits.
    pub flags: u8,
}

impl MemCell {
    /// The full instruction symbol (`Instruction::GetSymbol()`).
    pub fn symbol(&self) -> String {
        avida_genome::Instruction::new(i32::from(self.op)).symbol()
    }
}

/// One CPU head.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct HeadView {
    /// `GetPosition()`
    pub pos: i32,
    /// `GetMemSpace()`
    pub mem_space: i32,
}

/// One CPU thread.
#[derive(Clone, Debug, PartialEq)]
pub struct ThreadView {
    /// `cLocalThread::GetID()` (the thread index when the hardware does not
    /// expose ids).
    pub id: i32,
    /// Registers (`AX`, `BX`, `CX`, ...). Empty when the hardware type does
    /// not expose the registers of threads other than the current one.
    pub regs: Vec<i32>,
    /// Heads in the hardware's order: IP, read, write, flow, and for the
    /// gene-expression hardware (BCR, GP8) further flow heads. Named by
    /// [`CpuView::head_names`].
    pub heads: Vec<HeadView>,
    /// Each stack, top first (`GetStack(depth, stack_id, thread)` for depth
    /// 0..10): the thread's local stack, then the global stack.
    pub stacks: Vec<Vec<i32>>,
    /// `GetCurStack(thread)`: index into [`ThreadView::stacks`].
    pub active_stack: i32,
    /// `read_label` as nop letters (`A` = nop-A).
    pub read_label: String,
    /// `next_label` as nop letters.
    pub next_label: String,
    /// What a person debugging wants to know about the thread's state, such
    /// as `running`, `stopped`, `waiting for BX > 3 into CX` or
    /// `message trigger 2`; empty when there is nothing to say.
    pub status: String,
}

/// One memory space of a CPU: the host genome (space 0), an offspring or
/// write space, or the code of an injected parasite.
#[derive(Clone, Debug, PartialEq)]
pub struct MemSpaceView {
    /// The space number a head's `mem_space` refers to.
    pub id: i32,
    /// What the space holds: `genome`, `parasite`, `offspring`, `memory`,
    /// `memory B` and `gene 2, label AB` for the gene-expression hardware,
    /// or `space N` when the hardware cannot say.
    pub label: String,
    /// The sites, with the executed, copied and mutated flags.
    pub memory: Vec<MemCell>,
}

impl MemSpaceView {
    /// A heading such as `Memory space 2 (parasite)`.
    pub fn title(&self) -> String {
        format!("Memory space {} ({})", self.id, self.label)
    }
}

/// The virtual CPU of an organism.
#[derive(Clone, Debug, PartialEq)]
pub struct CpuView {
    /// `GetInstSet().GetInstSetName()`
    pub inst_set: String,
    /// `GetType()`
    pub hw_type: i32,
    /// `GetMemory()`, one entry per site (memory space 0).
    pub memory: Vec<MemCell>,
    /// Every memory space in use, space 0 first (a copy of
    /// [`CpuView::memory`] among them). One entry for hardware with a
    /// single memory.
    pub spaces: Vec<MemSpaceView>,
    /// Instruction names, indexed by opcode.
    pub inst_names: Arc<[String]>,
    /// The threads.
    pub threads: Vec<ThreadView>,
    /// Names of the heads as the C++ enums name them (`IP`, `Read`,
    /// `Write`, `Flow`, `Flow2`, ...), one per head of a thread.
    pub head_names: Vec<String>,
    /// `GetCurThread()`, clamped into `threads`.
    pub cur_thread: usize,
}

impl CpuView {
    /// Whether one CPU cycle of this hardware runs several micro-operations
    /// (`UOPS_PER_CYCLE` rounds over the running threads): BCR
    /// (`HARDWARE_TYPE` 5) and GP8 (4). A debugger step, which is one
    /// cycle, can then move an instruction pointer more than one position;
    /// on the other hardware a cycle runs one instruction.
    pub fn cycle_runs_micro_ops(&self) -> bool {
        matches!(self.hw_type, 4 | 5)
    }

    /// The memory as instruction symbols.
    pub fn genome_string(&self) -> String {
        self.memory.iter().map(MemCell::symbol).collect()
    }

    /// The memory space with this id.
    pub fn space(&self, id: i32) -> Option<&MemSpaceView> {
        self.spaces.iter().find(|s| s.id == id)
    }

    /// The name of the instruction at memory site `pos`.
    pub fn inst_name(&self, pos: usize) -> &str {
        self.cell_name(self.memory.get(pos))
    }

    /// The name of the instruction in a memory cell of any space.
    pub fn cell_name(&self, cell: Option<&MemCell>) -> &str {
        cell.and_then(|m| self.inst_names.get(usize::from(m.name_idx)))
            .map_or("", String::as_str)
    }
}

/// A resource of the environment.
#[derive(Clone, Debug, PartialEq)]
pub struct ResourceView {
    /// `cResource::GetName()`
    pub name: String,
    /// `GetResourceGeometry(r)` (`nGeometry`: 0 global, 1 grid, 2 torus,
    /// 5 partial, ...).
    pub geometry: i32,
    /// Whether the resource is spatial (has a per-cell grid).
    pub spatial: bool,
    /// Global amount, or the sum over cells for a spatial resource.
    pub quantity: f64,
    /// `cResourceCount::GetInflow(name)`
    pub inflow: Option<f64>,
    /// `1 - cResourceCount::GetDecay(name)`
    pub outflow: Option<f64>,
}

/// A reaction of the environment.
#[derive(Clone, Debug, PartialEq)]
pub struct ReactionView {
    /// `cReaction::GetName()`
    pub name: String,
    /// Index of the task that triggers it.
    pub task: Option<usize>,
    /// `cEnvironment::GetReactionValue(id)` (first process).
    pub value: f64,
    /// `cStats::GetReactionLastCount()[id]`: organisms whose last gestation
    /// triggered it.
    pub last_count: i32,
}

/// A task of the environment.
#[derive(Clone, Debug, PartialEq)]
pub struct TaskView {
    /// `cTaskEntry::GetName()` (the environment keyword, e.g. `nand`).
    pub name: String,
    /// `cTaskEntry::GetDesc()` (e.g. `Nand`).
    pub desc: String,
    /// `cStats::GetTaskLastCount(k)`
    pub last_count: i32,
}

/// The environment summary.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct EnvView {
    /// Resources in library order.
    pub resources: Vec<ResourceView>,
    /// Reactions in library order.
    pub reactions: Vec<ReactionView>,
    /// Tasks in library order.
    pub tasks: Vec<TaskView>,
}

impl EnvView {
    /// A display name for a layer, spelling out task and resource names.
    pub fn layer_label(&self, id: LayerId) -> String {
        match id {
            LayerId::Task(k) => match self.tasks.get(k) {
                Some(t) => format!("task {}", t.name),
                None => id.key(),
            },
            LayerId::Resource(r) => match self.resources.get(r) {
                Some(res) => format!("resource {}", res.name),
                None => id.key(),
            },
            _ => id.key(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cell(op: u8) -> MemCell {
        MemCell {
            op,
            sym: 'a',
            name_idx: u16::from(op),
            flags: 0,
        }
    }

    #[test]
    fn spaces_are_found_by_id_and_name_their_instructions() {
        let cpu = CpuView {
            inst_set: "x".into(),
            hw_type: 2,
            memory: vec![cell(0)],
            spaces: vec![
                MemSpaceView {
                    id: 0,
                    label: "genome".into(),
                    memory: vec![cell(0)],
                },
                MemSpaceView {
                    id: 2,
                    label: "parasite".into(),
                    memory: vec![cell(1), cell(0)],
                },
            ],
            inst_names: Arc::from(vec!["nop-A".to_string(), "h-copy".to_string()]),
            threads: Vec::new(),
            head_names: Vec::new(),
            cur_thread: 0,
        };
        assert!(cpu.space(1).is_none());
        let p = cpu.space(2).unwrap();
        assert_eq!(p.title(), "Memory space 2 (parasite)");
        assert_eq!(cpu.cell_name(p.memory.first()), "h-copy");
        assert_eq!(cpu.cell_name(None), "");
        assert_eq!(cpu.inst_name(0), "nop-A");
    }
}
