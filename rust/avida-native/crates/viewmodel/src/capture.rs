//! Copying the world out into a [`Frame`].
//!
//! # Determinism
//!
//! Every function here takes `&World` and calls only methods that take
//! `&self` on the world's parts, and none of those has interior
//! mutability: capturing never changes the run. In particular it does not
//! touch the RNG, the scheduler, any test CPU, `World::genome_test_metrics`
//! (which runs a test CPU and fills a cache), `World::fatal` (a `RefCell`),
//! `ResourceLib::initial_resource_levels` (a `OnceCell`), or the resource
//! accessors that bring resource counts up to date (`get_resources`,
//! `get_cell_resources`, `get_spatial_res`, `World::update_res_stats`).
//!
//! The price of the last rule: resource amounts are the *frozen* values,
//! as of the last time the run itself updated them (`GetFrozenCellResVal`,
//! `ReadResources`). Global resources update lazily in the C++ and here, so
//! between updates they can lag by the inflow and decay of up to one update
//! that no organism or print action has asked for yet.

use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use avida_genome::{CpuMemory, Genome};
use avida_hardware::hardware_bcr::HardwareBcr;
use avida_hardware::hardware_cpu::HardwareCpu;
use avida_hardware::hardware_experimental::HardwareExperimental;
use avida_hardware::hardware_gp8::HardwareGp8;
use avida_hardware::hardware_transsmt::{HardwareTransSmt, SmtThread};
use avida_hardware::nhardware::{HEAD_IP, HEAD_WRITE, STACK_SIZE};
use avida_hardware::{Hardware, Organism};
use avida_systematics::unit::GroupRef;
use avida_tasks::IoBuffer;
use avida_world::world::World;

use crate::ancestry::{layout, Family, Node};
use crate::model::{
    CpuView, EnvView, Frame, GenotypeCells, GenotypeRow, GenotypeView, HeadView, Header, LayerId,
    MapFrame, MemCell, MemSpaceView, OrgProbe, OrganismView, PhenotypeView, ReactionView,
    ResourceView, Subscription, TaskView, ThreadView, UpdateProgress, UpdateRow,
};
use crate::palette::CATEGORICAL_SLOTS;

/// Genotype id of an occupied cell whose organism has no genotype (not seen
/// in ordinary runs); -1 marks an empty cell.
pub const UNCLASSIFIED: i32 = -2;

/// Longest ancestry [`Frame::lineage`] lists.
pub const LINEAGE_MAX: usize = 256;

/// Copies what `sub` asks for out of the world, between updates. Takes
/// `&World` and never changes the run (see the module documentation).
///
/// A caller that may capture inside an update (one that drives the run
/// with a `Stepper` and pauses between time slices) uses
/// [`capture_with_progress`] so the frame says so.
pub fn capture(w: &World, sub: &Subscription) -> Frame {
    capture_with_progress(w, sub, UpdateProgress::default())
}

/// [`capture`] for a world at `progress` within its update cycle, which
/// only the caller knows (`Stepper::in_update`, `Stepper::slice_progress`).
pub fn capture_with_progress(w: &World, sub: &Subscription, progress: UpdateProgress) -> Frame {
    let ranked = ranked_genotypes(w);
    let map = if sub.layers.is_empty() {
        None
    } else {
        Some(map_frame(w, &sub.layers, &ranked))
    };
    let dominant = w.systematics.dominant_genotype().map(|g| {
        let rank = ranked.iter().position(|&r| r == g).unwrap_or(0);
        genotype_view(w, rank, g)
    });
    let genotypes = ranked
        .iter()
        .take(sub.genotypes_top_n)
        .enumerate()
        .map(|(rank, &g)| genotype_row(w, rank, g))
        .collect();
    let inspect = sub.inspect_cell.and_then(|c| organism_view(w, c));
    let lineage = sub
        .lineage_of
        .map(|g| lineage(w, &ranked, g))
        .unwrap_or_default();
    Frame {
        update: w.stats.update(),
        header: header(w, progress),
        map,
        dominant,
        genotypes,
        inspect,
        env: env_view(w),
        inst_names: inst_names(w),
        layers: all_layers(w),
        lineage,
    }
}

/// The statistics row of the current update (also part of every
/// [`Frame`] header), for front ends that keep a time series without
/// capturing whole frames.
pub fn update_row(w: &World) -> UpdateRow {
    let s = &w.stats;
    UpdateRow {
        update: s.update(),
        orgs: w.pop.num_organisms,
        genotypes: s.m_num_genotypes,
        threshold_genotypes: s.m_threshold_genotypes,
        ave_fitness: s.sum_fitness.average(),
        ave_merit: s.sum_merit.average(),
        ave_gestation: s.sum_gestation.average(),
        ave_generation: s.sum_generation.average(),
        ave_age: s.sum_creature_age.average(),
        ave_mem_size: s.sum_mem_size.average(),
        max_fitness: s.max_fitness,
        max_merit: s.max_merit,
        births: s.num_births,
        deaths: s.num_deaths,
        breed_true: s.num_breed_true,
        executed: s.num_executed,
        threads: s.m_num_threads,
        tot_organisms: s.tot_organisms,
        task_last_count: s.task_last_count.clone(),
        resources: resource_totals(w),
    }
}

/// Every layer the world has: the fixed ones, one per task, one per
/// resource.
pub fn all_layers(w: &World) -> Vec<LayerId> {
    let num_tasks = w.env.num_tasks().max(0) as usize;
    let num_res = w.pop.resource_count.size();
    LayerId::FIXED
        .iter()
        .copied()
        .chain((0..num_tasks).map(LayerId::Task))
        .chain((0..num_res).map(LayerId::Resource))
        .collect()
}

impl Subscription {
    /// Every layer, every genotype, no organism.
    pub fn everything(w: &World) -> Subscription {
        Subscription {
            layers: all_layers(w),
            inspect_cell: None,
            genotypes_top_n: usize::MAX,
            lineage_of: None,
        }
    }
}

/// The world's genotypes in `Begin()` / `Next()` order: most abundant
/// first, as the print actions and the C++ viewers list them.
fn ranked_genotypes(w: &World) -> Vec<GroupRef> {
    match w.systematics.arbiter_for_role("genotype") {
        Some(role) => w.systematics.iter_groups(role),
        None => Vec::new(),
    }
}

fn header(w: &World, progress: UpdateProgress) -> Header {
    let update = w.stats.update();
    Header {
        world_x: w.pop.world_x,
        world_y: w.pop.world_y,
        random_seed: w.cfg.random_seed,
        inst_set: w
            .hw
            .registry()
            .shared_inst_set(0)
            .map(|is| is.inst_set_name().to_string())
            .unwrap_or_default(),
        done: w.done,
        progress,
        last_finished_update: if progress.in_update {
            update - 1
        } else {
            update
        },
        stats: update_row(w),
    }
}

/// The instruction names of instruction set 0, by opcode.
fn inst_names(w: &World) -> Arc<[String]> {
    match w.hw.registry().shared_inst_set(0) {
        Some(is) => (0..is.size().max(0))
            .map(|i| is.name(i).to_string())
            .collect(),
        None => Arc::from(Vec::new()),
    }
}

/// `genotype_id` back through its parents, at most [`LINEAGE_MAX`]
/// genotypes. Extinct ancestors stay in the systematics manager (their
/// descendants hold references to them), so the chain normally reaches
/// the founder.
fn lineage(w: &World, ranked: &[GroupRef], genotype_id: i32) -> Vec<GenotypeRow> {
    let Some(role) = w.systematics.arbiter_for_role("genotype") else {
        return Vec::new();
    };
    let mut out: Vec<GenotypeRow> = Vec::new();
    let mut id = genotype_id;
    while out.len() < LINEAGE_MAX {
        let Some(g) = w.systematics.group(role, id) else {
            break;
        };
        let rank = ranked
            .iter()
            .position(|&r| r == g)
            .unwrap_or(GenotypeRow::NOT_RANKED);
        let row = genotype_row(w, rank, g);
        let parent = row.parent_id;
        out.push(row);
        if parent < 0 || out.iter().any(|r| r.id == parent) {
            break;
        }
        id = parent;
    }
    out
}

/// The first id in a `parents` property (`"12"` or `"12,40"`), -1 if none.
fn first_parent(parents: &str) -> i32 {
    parents
        .split(',')
        .next()
        .and_then(|p| p.trim().parse().ok())
        .unwrap_or(-1)
}

/// The organism in `cell`: its id, CPU cycles and divides. Reads three
/// fields; cheap enough to call after every instruction.
pub fn probe(w: &World, cell: i32) -> Option<OrgProbe> {
    let org = w.pop.orgs.get(usize::try_from(cell).ok()?)?.as_deref()?;
    Some(OrgProbe {
        cell,
        org_id: org.state.id,
        cpu_cycles: org.state.phenotype.lifetime_cpu_cycles,
        num_divides: org.state.phenotype.num_divides,
    })
}

/// Where the instruction pointer of each thread of the organism in `cell`
/// stands, as `(memory space, position)` pairs (the breakpoint sites of
/// every space).
pub fn ip_sites(w: &World, cell: i32) -> Vec<(i32, i32)> {
    let Some(org) = usize::try_from(cell)
        .ok()
        .and_then(|c| w.pop.orgs.get(c))
        .and_then(|o| o.as_deref())
    else {
        return Vec::new();
    };
    let hw = org.hardware.as_ref();
    (0..hw.num_threads().max(0) as usize)
        .filter_map(|t| hw.thread_heads(t).first().copied())
        .map(|h| (h.mem_space, h.pos))
        .collect()
}

/// The instruction pointer of each thread of the organism in `cell`, as
/// memory positions (heads in another memory space are left out).
pub fn ip_positions(w: &World, cell: i32) -> Vec<i32> {
    let Some(org) = usize::try_from(cell)
        .ok()
        .and_then(|c| w.pop.orgs.get(c))
        .and_then(|o| o.as_deref())
    else {
        return Vec::new();
    };
    let hw = org.hardware.as_ref();
    (0..hw.num_threads().max(0) as usize)
        .filter_map(|t| hw.thread_heads(t).first().copied())
        .filter(|h| h.mem_space == 0)
        .map(|h| h.pos)
        .collect()
}

/// The cell of the living organism with id `org_id` (a scan of the
/// population).
pub fn find_organism(w: &World, org_id: i32) -> Option<i32> {
    w.pop
        .orgs
        .iter()
        .position(|o| o.as_deref().is_some_and(|o| o.state.id == org_id))
        .map(|c| c as i32)
}

/// The newest living organism whose id is at least `min_id` (organism ids
/// count the organisms ever created, so an offspring born after a moment
/// has an id at or above `cStats::GetTotCreatures()` at that moment).
pub fn newest_organism_since(w: &World, min_id: i32) -> Option<OrgProbe> {
    w.pop
        .orgs
        .iter()
        .enumerate()
        .filter_map(|(c, o)| {
            let o = o.as_deref()?;
            (o.state.id >= min_id).then_some((c, o.state.id))
        })
        .max_by_key(|&(_, id)| id)
        .and_then(|(c, _)| probe(w, c as i32))
}

/// Global amount of each resource, or the sum over cells for a spatial one.
fn resource_totals(w: &World) -> Vec<f64> {
    let rc = &w.pop.resource_count;
    let global = rc.read_resources();
    let num_cells = w.pop.cells.len() as i32;
    (0..rc.size())
        .map(|r| {
            if rc.is_spatial_resource(r) {
                (0..num_cells)
                    .map(|c| rc.get_frozen_cell_res_val(c, r))
                    .sum()
            } else {
                global.get(r).copied().unwrap_or(0.0)
            }
        })
        .collect()
}

/// The genotype id of each cell's occupant.
fn cell_genotype_id(w: &World, role: Option<usize>, cell: usize) -> i32 {
    if w.pop.orgs[cell].is_none() {
        return -1;
    }
    role.and_then(|r| w.pop.membership.get(cell)?.group(r))
        .and_then(|g| w.systematics.group_id(g))
        .unwrap_or(UNCLASSIFIED)
}

fn org_value(org: &Organism, layer: LayerId) -> f32 {
    let ph = &org.state.phenotype;
    match layer {
        LayerId::Genotype | LayerId::Ancestry | LayerId::Resource(_) => f32::NAN,
        LayerId::Fitness => ph.fitness as f32,
        LayerId::Merit => ph.merit.get_double() as f32,
        LayerId::Gestation => ph.gestation_time as f32,
        LayerId::Age => ph.age as f32,
        LayerId::Generation => ph.generation as f32,
        LayerId::GenomeLen => ph.genome_length as f32,
        LayerId::Lineage => org.state.lineage_label as f32,
        LayerId::BreedTrue => f32::from(u8::from(ph.parent_true)),
        LayerId::Modified => f32::from(u8::from(ph.is_modified)),
        LayerId::Threads => org.hardware.num_threads() as f32,
        LayerId::TaskCount => ph.last_task_count.iter().filter(|&&c| c > 0).count() as f32,
        LayerId::Task(k) => ph.last_task_count.get(k).map_or(0.0, |&c| c as f32),
    }
}

fn map_frame(w: &World, layers: &[LayerId], ranked: &[GroupRef]) -> MapFrame {
    let n = w.pop.cells.len();
    let role = w.systematics.arbiter_for_role("genotype");
    let num_tasks = w.env.num_tasks().max(0) as usize;
    let words_per_cell = num_tasks.div_ceil(64).max(1);
    let rc = &w.pop.resource_count;

    let genotype_id: Vec<i32> = (0..n).map(|c| cell_genotype_id(w, role, c)).collect();

    let mut task_bits = vec![0u64; n * words_per_cell];
    for (c, slot) in w.pop.orgs.iter().enumerate().take(n) {
        let Some(org) = slot.as_deref() else {
            continue;
        };
        for (k, &count) in org
            .state
            .phenotype
            .last_task_count
            .iter()
            .enumerate()
            .take(num_tasks)
        {
            if count > 0 {
                task_bits[c * words_per_cell + k / 64] |= 1u64 << (k % 64);
            }
        }
    }

    let mut where_: HashMap<i32, (i32, u32)> = HashMap::new();
    for (c, &g) in genotype_id.iter().enumerate() {
        if g != -1 {
            where_.entry(g).or_insert((c as i32, 0)).1 += 1;
        }
    }
    let mut genotype_cells: Vec<GenotypeCells> = where_
        .into_iter()
        .map(|(id, (first_cell, cells))| GenotypeCells {
            id,
            first_cell,
            cells,
        })
        .collect();
    genotype_cells.sort_unstable_by_key(|c| c.id);

    let mut out_layers: Vec<(LayerId, Arc<[f32]>)> = Vec::new();
    let mut families: Vec<Family> = Vec::new();
    for &layer in layers {
        let valid = match layer {
            LayerId::Genotype => false,
            LayerId::Task(k) => k < num_tasks,
            LayerId::Resource(r) => r < rc.size(),
            _ => true,
        };
        if !valid || out_layers.iter().any(|(l, _)| *l == layer) {
            continue;
        }
        let values: Vec<f32> = match layer {
            LayerId::Resource(r) => (0..n)
                .map(|c| rc.get_frozen_cell_res_val(c as i32, r) as f32)
                .collect(),
            LayerId::Ancestry => {
                let tree = layout(&phylogeny(w, role, &genotype_cells));
                let values = genotype_id.iter().map(|&g| tree.value(g)).collect();
                families = tree.families;
                values
            }
            _ => w
                .pop
                .orgs
                .iter()
                .take(n)
                .map(|slot| slot.as_deref().map_or(f32::NAN, |o| org_value(o, layer)))
                .collect(),
        };
        out_layers.push((layer, values.into()));
    }

    let top_genotypes: Vec<i32> = ranked
        .iter()
        .filter_map(|&g| w.systematics.group_id(g))
        .take(CATEGORICAL_SLOTS)
        .collect();

    let top_counts: Vec<u32> = top_genotypes
        .iter()
        .map(|g| {
            genotype_cells
                .binary_search_by_key(g, |c| c.id)
                .map_or(0, |i| genotype_cells[i].cells)
        })
        .collect();

    MapFrame {
        w: w.pop.world_x,
        h: w.pop.world_y,
        genotype_id: genotype_id.into(),
        top_genotypes: top_genotypes.into(),
        layers: out_layers,
        num_tasks,
        task_bits: task_bits.into(),
        words_per_cell,
        top_counts: top_counts.into(),
        genotype_cells: genotype_cells.into(),
        families: families.into(),
    }
}

/// The genotypes on the map and their ancestors, for
/// [`LayerId::Ancestry`]. Extinct ancestors stay in the systematics
/// manager while they have living descendants.
fn phylogeny(w: &World, role: Option<usize>, on_map: &[GenotypeCells]) -> Vec<Node> {
    let mut nodes: Vec<Node> = Vec::with_capacity(on_map.len() * 2);
    let mut seen: HashSet<i32> = on_map.iter().map(|g| g.id).collect();
    let parent_of = |id: i32| {
        role.and_then(|r| w.systematics.group(r, id))
            .map_or(-1, |g| {
                first_parent(&w.systematics.property(g, "parents").string_value())
            })
    };
    for g in on_map {
        let mut parent = parent_of(g.id);
        nodes.push(Node {
            id: g.id,
            parent,
            organisms: g.cells,
        });
        while parent >= 0 && seen.insert(parent) {
            let id = parent;
            parent = parent_of(id);
            nodes.push(Node {
                id,
                parent,
                organisms: 0,
            });
        }
    }
    nodes
}

/// A genotype's genome, parsed from its `genome` property the way
/// `PrintDominantData` reads it.
fn genome_of(w: &World, g: GroupRef) -> Genome {
    Genome::from_string(&w.systematics.property(g, "genome").string_value())
}

fn genotype_row(w: &World, rank: usize, g: GroupRef) -> GenotypeRow {
    let sys = &w.systematics;
    let d = |name: &str| sys.property(g, name).double_value();
    let i = |name: &str| sys.property(g, name).int_value();
    GenotypeRow {
        rank,
        id: sys.group_id(g).unwrap_or(-1),
        parent_id: first_parent(&sys.property(g, "parents").string_value()),
        name: sys.property(g, "name").string_value(),
        abundance: sys.num_units(g).unwrap_or(0),
        depth: sys.depth(g).unwrap_or(0),
        update_born: i("update_born"),
        genome_len: genome_of(w, g).representation().size(),
        threshold: sys.property(g, "threshold").bool_value(),
        ave_fitness: d("ave_fitness"),
        ave_merit: d("ave_metabolic_rate"),
        ave_gestation: d("ave_gestation_time"),
        last_births: i("last_births"),
    }
}

/// The dominant genotype, read as `cActionPrintDominantData::Process` reads
/// it (`World::print_dominant_data`).
fn genotype_view(w: &World, rank: usize, g: GroupRef) -> GenotypeView {
    let sys = &w.systematics;
    let d = |name: &str| sys.property(g, name).double_value();
    let i = |name: &str| sys.property(g, name).int_value();
    GenotypeView {
        row: genotype_row(w, rank, g),
        genome: genome_of(w, g).representation().as_string(),
        ave_copy_size: d("ave_copy_size"),
        ave_exe_size: d("ave_exe_size"),
        ave_repro_rate: d("ave_repro_rate"),
        max_fitness: d("max_fitness"),
        last_breed_true: i("last_breed_true"),
        last_breed_in: i("last_breed_in"),
        parents: sys.property(g, "parents").string_value(),
    }
}

fn organism_view(w: &World, cell: i32) -> Option<OrganismView> {
    let c = usize::try_from(cell).ok()?;
    let org = w.pop.orgs.get(c)?.as_deref()?;
    let pc = w.pop.cells.get(c)?;
    let role = w.systematics.arbiter_for_role("genotype");
    let g = role.and_then(|r| w.pop.membership.get(c)?.group(r));
    let st = &org.state;
    let ph = &st.phenotype;
    let buf = |b: &IoBuffer| (0..b.num_stored()).map(|i| b.get(i)).collect();
    Some(OrganismView {
        cell,
        x: pc.x,
        y: pc.y,
        org_id: st.id,
        genotype_id: g.and_then(|g| w.systematics.group_id(g)).unwrap_or(-1),
        genotype_name: g
            .map(|g| w.systematics.property(g, "name").string_value())
            .unwrap_or_default(),
        lineage_label: st.lineage_label,
        faced_cell: pc.cell_faced(),
        phenotype: PhenotypeView {
            merit: ph.merit.get_double(),
            fitness: ph.fitness,
            cur_bonus: ph.cur_bonus,
            gestation_time: ph.gestation_time,
            age: ph.age,
            generation: ph.generation,
            genome_length: ph.genome_length,
            copied_size: ph.copied_size,
            executed_size: ph.executed_size,
            cpu_cycles_used: ph.cpu_cycles_used,
            lifetime_cpu_cycles: ph.lifetime_cpu_cycles,
            time_used: ph.time_used,
            num_divides: ph.num_divides,
            birth_update: ph.birth_update,
            parent_true: ph.parent_true,
            is_modified: ph.is_modified,
            is_multi_thread: ph.is_multi_thread,
            to_die: ph.to_die,
            cur_task_count: ph.cur_task_count.clone(),
            last_task_count: ph.last_task_count.clone(),
            cur_reaction_count: ph.cur_reaction_count.clone(),
            last_reaction_count: ph.last_reaction_count.clone(),
        },
        cell_inputs: pc.inputs.clone(),
        input_buf: buf(&st.input_buf),
        output_buf: buf(&st.output_buf),
        cpu: cpu_view(org.hardware.as_ref()),
    })
}

/// The CPU state through the `Hardware` trait. The trait exposes registers
/// and labels of the current thread only, so for the heads, experimental
/// and TransSMT CPUs they are read from each thread after a downcast
/// (TransSMT's registers are the tops of its stacks).
fn cpu_view(hw: &dyn Hardware) -> CpuView {
    let is = hw.inst_set();
    let n_inst = is.size().max(0);
    let inst_names: Arc<[String]> = (0..n_inst).map(|i| is.name(i).to_string()).collect();
    let cells = |mem: &CpuMemory| -> Vec<MemCell> {
        (0..mem.size())
            .map(|p| {
                let inst = mem[p];
                let op = inst.op();
                MemCell {
                    op: op as u8,
                    sym: inst.symbol().chars().last().unwrap_or('?'),
                    name_idx: if op < n_inst { op as u16 } else { u16::MAX },
                    flags: mem.flags(p),
                }
            })
            .collect()
    };
    let memory = cells(hw.memory());

    let num_threads = hw.num_threads().max(0) as usize;
    let cur_thread = usize::try_from(hw.cur_thread())
        .ok()
        .filter(|&t| t < num_threads)
        .unwrap_or(0);
    let cpu = hw.as_any().downcast_ref::<HardwareCpu>();
    let exp = hw.as_any().downcast_ref::<HardwareExperimental>();
    let smt = hw.as_any().downcast_ref::<HardwareTransSmt>();
    // The gene-expression hardware (BCR, GP8) keeps its threads in a shared
    // core.
    let gene = hw
        .as_any()
        .downcast_ref::<HardwareBcr>()
        .map(HardwareBcr::core_ref)
        .or_else(|| {
            hw.as_any()
                .downcast_ref::<HardwareGp8>()
                .map(HardwareGp8::core_ref)
        });
    let num_stacks = hw.num_stacks().max(0);
    let num_regs = hw.num_registers().max(0) as usize;
    let mut num_heads = 0;
    let threads: Vec<ThreadView> = (0..num_threads)
        .map(|t| {
            let heads: Vec<HeadView> = hw
                .thread_heads(t)
                .into_iter()
                .map(|h| HeadView {
                    pos: h.pos,
                    mem_space: h.mem_space,
                })
                .collect();
            num_heads = num_heads.max(heads.len());
            let stacks = (0..num_stacks)
                .map(|s| {
                    (0..STACK_SIZE)
                        .map(|d| hw.get_stack(d, s, t as i32))
                        .collect()
                })
                .collect();
            let (id, regs, read_label, next_label) = if let Some(lt) = cpu.and_then(|c| c.thread(t))
            {
                (
                    lt.id(),
                    lt.reg.to_vec(),
                    lt.read_label.as_string(),
                    lt.next_label.as_string(),
                )
            } else if let Some(lt) = exp.and_then(|e| e.threads().get(t)) {
                (
                    lt.id(),
                    lt.reg.iter().map(|r| r.value).collect(),
                    lt.read_label.as_string(),
                    lt.next_label.as_string(),
                )
            } else if let Some(lt) = smt.and_then(|s| s.threads().get(t)) {
                // `GetRegister(r)` is the top of stack `r` of the thread.
                (
                    t as i32,
                    (0..num_regs)
                        .map(|r| hw.get_stack(0, r as i32, t as i32))
                        .collect(),
                    lt.read_label.as_string(),
                    lt.next_label.as_string(),
                )
            } else if let Some(lt) = gene.and_then(|g| g.threads.get(t)) {
                (
                    t as i32,
                    (0..num_regs)
                        .filter_map(|r| hw.register_in_thread(r, t))
                        .collect(),
                    lt.read_label.as_string(),
                    lt.next_label.as_string(),
                )
            } else {
                (
                    t as i32,
                    (0..num_regs)
                        .map_while(|r| hw.register_in_thread(r, t))
                        .collect(),
                    String::new(),
                    String::new(),
                )
            };
            ThreadView {
                id,
                regs,
                heads,
                stacks,
                active_stack: hw.get_cur_stack(t as i32),
                read_label,
                next_label,
                status: hw.thread_status(t),
            }
        })
        .collect();

    // Memory spaces. TransSMT names a space after the threads that use it:
    // a thread owned by a parasite makes its space a parasite's code, a
    // host thread's write head makes it an offspring's.
    let smt_label = |id: i32| -> Option<&'static str> {
        let s = smt?;
        let in_use = |pick: fn(&SmtThread) -> i32, owned: bool| {
            s.threads()
                .iter()
                .any(|t| t.owner.is_some() == owned && pick(t) == id)
        };
        if id == 0 {
            Some("genome")
        } else if in_use(|t| t.heads[HEAD_IP].mem_space(), true) {
            Some("parasite")
        } else if in_use(|t| t.heads[HEAD_WRITE].mem_space(), false) {
            Some("offspring")
        } else {
            None
        }
    };
    let spaces = hw
        .memory_spaces()
        .into_iter()
        .map(|(id, mem)| MemSpaceView {
            id,
            label: hw
                .memory_space_label(id)
                .or_else(|| smt_label(id).map(str::to_string))
                .unwrap_or_else(|| {
                    if id == 0 {
                        "genome".to_string()
                    } else {
                        format!("space {id}")
                    }
                }),
            memory: if id == 0 { memory.clone() } else { cells(mem) },
        })
        .collect();

    CpuView {
        inst_set: is.inst_set_name().to_string(),
        hw_type: hw.hw_type(),
        memory,
        spaces,
        inst_names,
        threads,
        head_names: (0..num_heads).map(head_name).collect(),
        cur_thread,
    }
}

/// The name of head `i` as `nHardware` and the BCR / GP8 enums give it:
/// `hIP`, `hREAD`, `hWRITE`, `hFLOW`, `hFLOW2`, `hFLOW3`, ...
pub fn head_name(i: usize) -> String {
    match i {
        0 => "IP".to_string(),
        1 => "Read".to_string(),
        2 => "Write".to_string(),
        3 => "Flow".to_string(),
        _ => format!("Flow{}", i - 2),
    }
}

fn env_view(w: &World) -> EnvView {
    let rc = &w.pop.resource_count;
    let totals = resource_totals(w);
    let resources = (0..rc.size())
        .map(|r| {
            let name = rc.get_res_name(r).to_string();
            ResourceView {
                geometry: rc.get_resource_geometry(r),
                spatial: rc.is_spatial_resource(r),
                quantity: totals[r],
                inflow: rc.get_inflow(&name).ok(),
                outflow: rc.get_decay(&name).ok().map(|d| 1.0 - d),
                name,
            }
        })
        .collect();
    let lib = w.env.reaction_lib();
    let reactions = lib
        .reactions()
        .iter()
        .enumerate()
        .map(|(i, r)| ReactionView {
            name: r.name().to_string(),
            task: r.task.and_then(|t| usize::try_from(t).ok()),
            value: w.env.reaction_value(i as i32),
            last_count: w.stats.m_reaction_last_count.get(i).copied().unwrap_or(0),
        })
        .collect();
    let tasks = w
        .env
        .tasklib()
        .0
        .tasks()
        .iter()
        .enumerate()
        .map(|(k, t)| TaskView {
            name: t.name().to_string(),
            desc: t.desc().to_string(),
            last_count: w.stats.task_last_count.get(k).copied().unwrap_or(0),
        })
        .collect();
    EnvView {
        resources,
        reactions,
        tasks,
    }
}
