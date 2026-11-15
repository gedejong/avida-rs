//! A synthetic run: a recording of hand-made frames, for design reviews
//! and measurements that need a rich world without simulating one (families
//! that grow, tasks that spread, genome lengths and fitness that depend on
//! them). Played back it exercises the same paths as a recorded run.

use std::path::Path;
use std::sync::Arc;

use avida_session::recording::{Meta, RecordingWriter};
use avida_session::{Frame, LayerId, UpdateRow};
use avida_viewmodel::ancestry::{layout, Node};
use avida_viewmodel::{
    EnvView, GenotypeCells, GenotypeRow, Header, MapFrame, ReactionView, TaskView, UpdateProgress,
};

/// The size and length of the synthetic run.
#[derive(Clone, Debug)]
pub struct Synth {
    pub w: i32,
    pub h: i32,
    /// Genotypes over the whole run.
    pub genotypes: usize,
    /// Frames recorded.
    pub frames: usize,
    /// Updates between frames.
    pub every: i32,
    /// The layers recorded (`None`: all).
    pub layers: Option<Vec<LayerId>>,
}

impl Default for Synth {
    fn default() -> Self {
        Synth {
            w: 60,
            h: 60,
            genotypes: 48,
            frames: 40,
            every: 10,
            layers: None,
        }
    }
}

const TASKS: [&str; 9] = [
    "Not", "Nand", "And", "Orn", "Or", "Andn", "Nor", "Xor", "Equ",
];

/// A deterministic hash to a number in `0..u64::MAX`.
fn mix(a: u64, b: u64) -> u64 {
    let mut x = a
        .wrapping_mul(0x9e37_79b9_7f4a_7c15)
        .wrapping_add(b.wrapping_mul(0xbf58_476d_1ce4_e5b9))
        .wrapping_add(0x94d0_49bb_1331_11eb);
    x ^= x >> 30;
    x = x.wrapping_mul(0xbf58_476d_1ce4_e5b9);
    x ^= x >> 27;
    x = x.wrapping_mul(0x94d0_49bb_1331_11eb);
    x ^ (x >> 31)
}

fn unit(a: u64, b: u64) -> f64 {
    (mix(a, b) >> 11) as f64 / (1u64 << 53) as f64
}

struct Geno {
    id: i32,
    parent: i32,
    born: i32,
    len: i32,
    own_tasks: u32,
    seed: (f64, f64),
}

fn genotypes(spec: &Synth) -> Vec<Geno> {
    let total = spec.frames as i32 * spec.every;
    let mut v: Vec<Geno> = Vec::new();
    for g in 0..spec.genotypes {
        let founder = g < 3;
        let parent = if founder {
            -1
        } else {
            // Mostly a recent genotype, sometimes an old one.
            let back = 1 + (unit(g as u64, 1) * unit(g as u64, 2) * g as f64) as usize;
            (g - back.min(g)) as i32 + 1
        };
        let born = if founder {
            0
        } else {
            (f64::from(total) * 0.8 * (g as f64 / spec.genotypes as f64)) as i32
        };
        let base_len = if founder {
            [50, 80, 100][g]
        } else {
            v[(parent - 1) as usize].len
        };
        let len = (base_len + (unit(g as u64, 3) * 21.0) as i32 - 10).clamp(30, 200);
        let own = (mix(g as u64, 4) & 0x1ff) as u32 & (mix(g as u64, 5) & 0x1ff) as u32;
        v.push(Geno {
            id: g as i32 + 1,
            parent,
            born,
            len,
            own_tasks: own,
            seed: (
                unit(g as u64, 6) * f64::from(spec.w),
                unit(g as u64, 7) * f64::from(spec.h),
            ),
        });
    }
    v
}

/// The tasks genotype `g` performs at update `u`: its own, and its
/// parent's, each appearing some updates after the genotype is born.
fn tasks_of(all: &[Geno], g: usize, u: i32) -> u32 {
    let geno = &all[g];
    let mut bits = 0u32;
    for k in 0..9 {
        let own = geno.own_tasks >> k & 1 == 1;
        if own && u >= geno.born + 12 * k {
            bits |= 1 << k;
        }
    }
    if geno.parent > 0 {
        bits |= tasks_of(all, (geno.parent - 1) as usize, u) & !(1 << 8);
    }
    bits
}

fn frame(spec: &Synth, all: &[Geno], u: i32, index: usize) -> Frame {
    let (w, h) = (spec.w as usize, spec.h as usize);
    let n = w * h;
    let alive: Vec<usize> = (0..all.len()).filter(|&g| all[g].born <= u).collect();
    let tasks: Vec<u32> = (0..all.len()).map(|g| tasks_of(all, g, u)).collect();
    let fitness_of = |g: usize| {
        let t = tasks[g].count_ones();
        (0.12 + 0.045 * f64::from(t).powf(1.7)) * (100.0 / f64::from(all[g].len)).powf(0.5)
    };
    // Each cell goes to the genotype whose weighted distance is smallest.
    let weight = |g: usize| {
        let age = f64::from((u - all[g].born).max(0) + 8);
        let r = 3.0 + fitness_of(g).sqrt() * 7.0 * (age / 60.0).min(1.0);
        r * r
    };
    let weights: Vec<f64> = alive.iter().map(|&g| weight(g)).collect();
    let mut gid = vec![-1i32; n];
    let mut owner = vec![usize::MAX; n];
    for c in 0..n {
        if unit(c as u64, (u / 30) as u64 + 100) < 0.04 {
            continue;
        }
        let (x, y) = ((c % w) as f64, (c / w) as f64);
        let mut best = (f64::INFINITY, 0usize);
        for (i, &g) in alive.iter().enumerate() {
            let (dx, dy) = (x - all[g].seed.0, y - all[g].seed.1);
            let d = dx * dx + dy * dy - weights[i];
            if d < best.0 {
                best = (d, g);
            }
        }
        gid[c] = all[best.1].id;
        owner[c] = best.1;
    }

    let mut counts = vec![0u32; all.len()];
    let mut first = vec![i32::MAX; all.len()];
    for (c, &o) in owner.iter().enumerate() {
        if o != usize::MAX {
            counts[o] += 1;
            first[o] = first[o].min(c as i32);
        }
    }
    let nodes: Vec<Node> = (0..all.len())
        .filter(|&g| all[g].born <= u)
        .map(|g| Node {
            id: all[g].id,
            parent: all[g].parent,
            organisms: counts[g],
        })
        .collect();
    let tree = layout(&nodes);

    let mut fit = vec![f32::NAN; n];
    let mut merit = fit.clone();
    let mut gest = fit.clone();
    let mut age = fit.clone();
    let mut generation = fit.clone();
    let mut len = fit.clone();
    let mut lineage = fit.clone();
    let mut breed = fit.clone();
    let mut modified = fit.clone();
    let mut threads = fit.clone();
    let mut task_count = fit.clone();
    let mut anc = fit.clone();
    let mut task_layers: Vec<Vec<f32>> = vec![vec![f32::NAN; n]; 9];
    let mut task_bits = vec![0u64; n];
    for c in 0..n {
        let o = owner[c];
        if o == usize::MAX {
            continue;
        }
        let noise = 0.8 + 0.4 * unit(c as u64, u as u64 + 7);
        let f = fitness_of(o) * noise;
        let l = f64::from(all[o].len);
        let gt = 280.0 + l * 2.5 - f64::from(tasks[o].count_ones()) * 8.0;
        fit[c] = f as f32;
        gest[c] = gt as f32;
        merit[c] = (f * gt) as f32;
        age[c] = (unit(c as u64, 11) * gt / 8.0).floor() as f32;
        generation[c] =
            (1.0 + f64::from((u - all[o].born).max(0)) / 15.0 + unit(c as u64, 12) * 3.0).floor()
                as f32;
        len[c] = all[o].len as f32;
        lineage[c] = (o % 3) as f32;
        breed[c] = f32::from(u8::from(unit(c as u64, 13) < 0.8));
        modified[c] = 0.0;
        threads[c] = 1.0;
        task_count[c] = tasks[o].count_ones() as f32;
        anc[c] = tree.value(all[o].id);
        for k in 0..9 {
            let on = tasks[o] >> k & 1 == 1;
            // A fraction of the organisms has not yet done the task in
            // this gestation.
            let done = on && unit(c as u64, 20 + k) < 0.9;
            task_layers[k as usize][c] = f32::from(u8::from(done));
            if done {
                task_bits[c] |= 1 << k;
            }
        }
    }
    let mut t_count = [0i32; 9];
    for &b in &task_bits {
        for (k, c) in t_count.iter_mut().enumerate() {
            *c += i32::from(b >> k & 1 == 1);
        }
    }
    let finite = |v: &[f32]| -> Vec<f64> {
        v.iter()
            .filter(|x| x.is_finite())
            .map(|&x| f64::from(x))
            .collect()
    };
    let mean = |v: &[f32]| {
        let x = finite(v);
        x.iter().sum::<f64>() / x.len().max(1) as f64
    };
    let max = |v: &[f32]| finite(v).into_iter().fold(0.0, f64::max);
    let orgs = gid.iter().filter(|&&g| g != -1).count() as i32;
    let stats = UpdateRow {
        update: u,
        orgs,
        genotypes: alive.iter().filter(|&&g| counts[g] > 0).count() as i32,
        threshold_genotypes: 5,
        ave_fitness: mean(&fit),
        ave_merit: mean(&merit),
        ave_gestation: mean(&gest),
        ave_generation: mean(&generation),
        ave_age: mean(&age),
        ave_mem_size: mean(&len),
        max_fitness: max(&fit),
        max_merit: max(&merit),
        births: 120 + (index as i32 % 7) * 9,
        deaths: 118 + (index as i32 % 5) * 7,
        breed_true: 90,
        executed: orgs * 30,
        threads: orgs,
        tot_organisms: orgs + u * 100,
        task_last_count: t_count.to_vec(),
        resources: Vec::new(),
    };

    let mut ranked: Vec<usize> = (0..all.len()).filter(|&g| counts[g] > 0).collect();
    ranked.sort_by_key(|&g| (std::cmp::Reverse(counts[g]), all[g].id));
    let top: Vec<i32> = ranked.iter().take(7).map(|&g| all[g].id).collect();
    let genotype_cells: Vec<GenotypeCells> = (0..all.len())
        .filter(|&g| counts[g] > 0)
        .map(|g| GenotypeCells {
            id: all[g].id,
            first_cell: first[g],
            cells: counts[g],
        })
        .collect();
    let rows: Vec<GenotypeRow> = ranked
        .iter()
        .enumerate()
        .map(|(rank, &g)| GenotypeRow {
            rank,
            id: all[g].id,
            parent_id: all[g].parent,
            name: format!(
                "{:03}-{}",
                all[g].len,
                ["aaaaa", "aaaab", "aaabc", "aabcd"][g % 4]
            ),
            abundance: counts[g] as i32,
            depth: {
                let mut d = 0;
                let mut p = all[g].parent;
                while p > 0 {
                    d += 1;
                    p = all[(p - 1) as usize].parent;
                }
                d
            },
            update_born: all[g].born,
            genome_len: all[g].len,
            threshold: counts[g] > 5,
            ave_fitness: fitness_of(g),
            ave_merit: fitness_of(g) * 300.0,
            ave_gestation: 300.0,
            last_births: 4,
        })
        .collect();

    let all_layers: Vec<(LayerId, Arc<[f32]>)> = vec![
        (LayerId::Fitness, fit.into()),
        (LayerId::Merit, merit.into()),
        (LayerId::Gestation, gest.into()),
        (LayerId::Age, age.into()),
        (LayerId::Generation, generation.into()),
        (LayerId::GenomeLen, len.into()),
        (LayerId::Lineage, lineage.into()),
        (LayerId::Ancestry, anc.into()),
        (LayerId::BreedTrue, breed.into()),
        (LayerId::Modified, modified.into()),
        (LayerId::Threads, threads.into()),
        (LayerId::TaskCount, task_count.into()),
    ]
    .into_iter()
    .chain(
        task_layers
            .into_iter()
            .enumerate()
            .map(|(k, v)| (LayerId::Task(k), Arc::<[f32]>::from(v))),
    )
    .collect();
    let layers: Vec<(LayerId, Arc<[f32]>)> = match &spec.layers {
        None => all_layers,
        Some(keep) => all_layers
            .into_iter()
            .filter(|(l, _)| keep.contains(l))
            .collect(),
    };
    let mut layer_ids = avida_viewmodel::LayerId::FIXED.to_vec();
    layer_ids.extend((0..9).map(LayerId::Task));
    Frame {
        update: u,
        header: Header {
            world_x: spec.w,
            world_y: spec.h,
            random_seed: 1,
            inst_set: "heads_default".into(),
            done: false,
            progress: UpdateProgress::default(),
            last_finished_update: u,
            stats,
        },
        map: Some(MapFrame {
            w: spec.w,
            h: spec.h,
            genotype_id: gid.into(),
            top_genotypes: top.clone().into(),
            layers,
            num_tasks: 9,
            task_bits: task_bits.into(),
            words_per_cell: 1,
            top_counts: top
                .iter()
                .map(|id| counts[(*id - 1) as usize])
                .collect::<Vec<_>>()
                .into(),
            genotype_cells: genotype_cells.into(),
            families: tree.families.clone().into(),
        }),
        dominant: None,
        genotypes: rows,
        inspect: None,
        env: EnvView {
            resources: Vec::new(),
            // One reaction per task, named as the stock environment does.
            reactions: TASKS
                .iter()
                .enumerate()
                .map(|(k, n)| ReactionView {
                    name: n.to_uppercase(),
                    task: Some(k),
                    value: 1.0,
                    last_count: t_count[k],
                })
                .collect(),
            tasks: TASKS
                .iter()
                .enumerate()
                .map(|(k, n)| TaskView {
                    name: n.to_lowercase(),
                    desc: (*n).to_string(),
                    last_count: t_count[k],
                })
                .collect(),
        },
        inst_names: ["nop-A", "nop-B", "nop-C", "h-copy", "h-divide"]
            .map(String::from)
            .to_vec()
            .into(),
        layers: layer_ids,
        lineage: Vec::new(),
        open_ended: Default::default(),
    }
}

/// The frames of the synthetic run, in order.
pub fn frames(spec: &Synth) -> Vec<Frame> {
    let all = genotypes(spec);
    (0..spec.frames)
        .map(|i| frame(spec, &all, i as i32 * spec.every, i))
        .collect()
}

/// Writes the run as a recording.
pub fn write(path: &Path, spec: &Synth) {
    let frames = frames(spec);
    let meta = Meta::for_frame(
        &frames[0],
        spec.every as u32,
        100,
        Vec::new(),
        "synthetic".into(),
    );
    let mut w = RecordingWriter::create(path, &meta).unwrap();
    for f in &frames {
        w.append(f).unwrap();
    }
}
