//! Text snapshots of `to_text(capture(..))` for `heads_default_100u` at
//! updates 0, 10 and 100, plus direct checks of what the frames contain.
//!
//! The expected text lives in `tests/snapshots/frame_u<update>.txt`. To
//! regenerate it after an intended change, run
//! `VIEWMODEL_BLESS=1 cargo test -p avida-viewmodel --test snapshots`
//! and review the diff.

mod common;

use std::collections::BTreeMap;
use std::fs;
use std::path::PathBuf;

use avida_viewmodel::{
    all_layers, capture, mem_flags, to_text, update_row, Frame, LayerId, Subscription,
};

use common::{append_events, build_world, fresh_config, run_with_stops};

const TEST: &str = "heads_default_100u";
const UPDATES: [i32; 3] = [0, 10, 100];

/// Cell 0 is where `Inject default-classic.org` puts the ancestor; the
/// lineage traced is the newest genotype's, which has the longest one.
fn subscription(w: &avida_world::world::World) -> Subscription {
    let newest = w
        .systematics
        .arbiter_for_role("genotype")
        .map(|role| w.systematics.iter_groups(role))
        .unwrap_or_default()
        .iter()
        .filter_map(|&g| w.systematics.group_id(g))
        .max();
    Subscription {
        layers: all_layers(w),
        inspect_cell: Some(0),
        genotypes_top_n: 8,
        lineage_of: newest,
    }
}

fn frames() -> BTreeMap<i32, Frame> {
    let dir = fresh_config(TEST, "snapshots");
    append_events(&dir, "u 0 Exit\nu 10 Exit\n");
    let mut w = build_world(&dir);
    let mut frames = BTreeMap::new();
    let mut sink = Vec::new();
    run_with_stops(&mut w, 100, &mut sink, |w| {
        let row = update_row(w);
        let frame = capture(w, &subscription(w));
        assert_eq!(frame.header.stats, row);
        frames.insert(w.stats.update(), frame);
    });
    frames
}

fn snapshot_path(update: i32) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/snapshots")
        .join(format!("frame_u{update}.txt"))
}

#[test]
fn frames_match_text_snapshots_and_are_consistent() {
    let frames = frames();
    assert_eq!(frames.keys().copied().collect::<Vec<_>>(), UPDATES.to_vec());

    let bless = std::env::var_os("VIEWMODEL_BLESS").is_some();
    for (&update, frame) in &frames {
        let text = to_text(frame);
        let path = snapshot_path(update);
        if bless {
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            fs::write(&path, &text).unwrap();
            continue;
        }
        let expected = fs::read_to_string(&path)
            .unwrap_or_else(|e| panic!("{}: {e} (run with VIEWMODEL_BLESS=1)", path.display()));
        assert_eq!(text, expected, "snapshot {} differs", path.display());
    }

    check_update_0(&frames[&0]);
    for frame in frames.values() {
        check_invariants(frame);
    }
    // The ancestor needs about 13 updates for its first offspring; by
    // update 100 its line has spread and split into several genotypes.
    assert_eq!(frames[&10].header.stats.orgs, 1);
    assert!(frames[&100].header.stats.orgs > 10);
    assert!(frames[&100].genotypes.len() > 1);
}

/// After update 0 (the stats start at update -1, when `u begin Inject`
/// places the ancestor; update 0 is its first time slice): one organism in
/// cell 0, which has allocated its offspring's memory but not divided.
fn check_update_0(f: &Frame) {
    assert_eq!(f.update, 0);
    assert_eq!(f.header.world_x * f.header.world_y, 3600);
    assert_eq!(f.header.inst_set, "heads_default");
    // Captured between updates.
    assert!(!f.header.progress.in_update);
    assert_eq!(f.header.last_finished_update, 0);
    assert_eq!(f.inst_names.len(), 26);
    assert_eq!(f.inst_names[0], "nop-A");
    // The founder's lineage is itself.
    assert_eq!(f.lineage.len(), 1);
    assert_eq!(f.lineage[0].parent_id, -1);
    let map = f.map.as_ref().unwrap();
    assert_eq!(map.occupied(), 1);
    assert!(map.genotype_id[0] >= 0);
    assert_eq!(map.top_genotypes.len(), 1);
    assert_eq!(map.top_genotypes[0], map.genotype_id[0]);
    assert_eq!(f.genotypes.len(), 1);
    let dom = f.dominant.as_ref().unwrap();
    assert_eq!(dom.row.id, map.genotype_id[0]);
    assert_eq!(dom.row.abundance, 1);
    assert_eq!(dom.row.genome_len, 100);

    let o = f.inspect.as_ref().unwrap();
    assert_eq!((o.cell, o.x, o.y), (0, 0, 0));
    assert_eq!(o.genotype_id, dom.row.id);
    assert_eq!(o.genotype_name, dom.row.name);
    assert_eq!(o.phenotype.birth_update, -1);
    assert_eq!(o.phenotype.num_divides, 0);
    let cpu = &o.cpu;
    assert_eq!(cpu.inst_names.len(), 26);
    assert_eq!(cpu.inst_name(0), "h-alloc");
    // `h-alloc` has run: the parent's genome followed by blank offspring
    // memory (`nop-A`, symbol `a`).
    assert!(cpu.memory.len() > 100);
    let mem = cpu.genome_string();
    assert_eq!(&mem[..100], dom.genome);
    assert!(mem[100..].bytes().all(|b| b == b'a'));
    // Only the parent's part has executed instructions.
    let executed = |m: &avida_viewmodel::MemCell| m.flags & mem_flags::EXECUTED != 0;
    assert!(executed(&cpu.memory[0]));
    assert!(!cpu.memory[100..].iter().any(executed));
    assert!(o.phenotype.cpu_cycles_used > 0);
    assert_eq!(cpu.threads.len(), 1);
    let t = &cpu.threads[0];
    assert_eq!(t.regs.len(), 3);
    assert_eq!(t.heads.len(), 4);
    assert_eq!(t.stacks.len(), 2);
    assert!(t.stacks.iter().all(|s| s.len() == 10));
    // The write head stands at the start of the offspring memory.
    assert_eq!(t.heads[2].pos, 100);
}

fn check_invariants(f: &Frame) {
    let map = f.map.as_ref().unwrap();
    let n = map.num_cells();
    assert_eq!(n, (map.w * map.h) as usize);
    assert_eq!(map.occupied() as i32, f.header.stats.orgs);
    assert_eq!(f.layers, {
        let mut l = LayerId::FIXED.to_vec();
        l.extend((0..f.env.tasks.len()).map(LayerId::Task));
        l.extend((0..f.env.resources.len()).map(LayerId::Resource));
        l
    });
    if let Some(o) = &f.inspect {
        assert_eq!(o.cpu.inst_names, f.inst_names);
    }
    // Where each genotype lives agrees with the cells.
    assert!(map.genotype_cells.windows(2).all(|p| p[0].id < p[1].id));
    assert_eq!(
        map.genotype_cells
            .iter()
            .map(|c| c.cells as usize)
            .sum::<usize>(),
        map.occupied()
    );
    for c in map.genotype_cells.iter() {
        let first = map.genotype_id.iter().position(|&g| g == c.id);
        assert_eq!(first, Some(c.first_cell as usize), "genotype {}", c.id);
    }
    assert_eq!(map.top_counts.len(), map.top_genotypes.len());
    for (g, &n) in map.top_genotypes.iter().zip(map.top_counts.iter()) {
        assert_eq!(n, map.cells_of(*g));
    }
    // The lineage runs from the asked-for genotype back to the founder,
    // one level of depth at a time.
    assert!(!f.lineage.is_empty());
    for pair in f.lineage.windows(2) {
        assert_eq!(pair[0].parent_id, pair[1].id);
        assert_eq!(pair[0].depth, pair[1].depth + 1);
    }
    let root = f.lineage.last().unwrap();
    assert_eq!((root.parent_id, root.depth), (-1, 0));
    for row in &f.lineage {
        if row.rank != avida_viewmodel::GenotypeRow::NOT_RANKED {
            assert!(row.abundance > 0);
        }
    }
    assert_eq!(map.task_bits.len(), n * map.words_per_cell);
    assert_eq!(map.num_tasks, f.env.tasks.len());
    // Every subscribed layer but Genotype is present; per-organism layers
    // are NaN exactly in the empty cells.
    assert_eq!(map.layers.len(), all_layers_len(f) - 1);
    for (id, values) in &map.layers {
        assert_eq!(values.len(), n);
        if id.is_per_organism() {
            for (c, v) in values.iter().enumerate() {
                assert_eq!(v.is_nan(), map.genotype_id[c] == -1, "{id:?} cell {c}");
            }
        }
    }
    // Task bits agree with the task layers and with the task counts.
    for k in 0..map.num_tasks {
        let layer = map.layer(LayerId::Task(k)).unwrap();
        let done = (0..n).filter(|&c| map.task_done(c, k)).count();
        assert_eq!(done, layer.iter().filter(|v| **v > 0.0).count());
    }
    let counts = map.layer(LayerId::TaskCount).unwrap();
    for (c, &v) in counts.iter().enumerate() {
        if !v.is_nan() {
            let bits = (0..map.num_tasks).filter(|&k| map.task_done(c, k)).count();
            assert_eq!(v as usize, bits);
        }
    }
    // Genotype rows are in abundance order and add up to the population.
    let rows = &f.genotypes;
    assert!(rows.windows(2).all(|p| p[0].abundance >= p[1].abundance));
    if let Some(d) = &f.dominant {
        assert_eq!(rows[0].id, d.row.id);
    }
    if rows.len() < 8 {
        assert_eq!(
            rows.iter().map(|r| r.abundance).sum::<i32>(),
            f.header.stats.orgs
        );
    }
    // Cells of each listed genotype match its abundance.
    for r in rows {
        let cells = map.genotype_id.iter().filter(|&&g| g == r.id).count();
        assert_eq!(cells as i32, r.abundance, "genotype {}", r.id);
    }
}

fn all_layers_len(f: &Frame) -> usize {
    LayerId::FIXED.len() + f.env.tasks.len() + f.env.resources.len()
}

#[test]
fn an_empty_subscription_captures_only_header_and_environment() {
    let dir = fresh_config(TEST, "empty_subscription");
    append_events(&dir, "u 0 Exit\n");
    let mut w = build_world(&dir);
    let mut sink = Vec::new();
    let mut seen = false;
    run_with_stops(&mut w, 0, &mut sink, |w| {
        let f = capture(w, &Subscription::default());
        assert!(f.map.is_none());
        assert!(f.genotypes.is_empty());
        assert!(f.inspect.is_none());
        assert!(f.dominant.is_some());
        assert_eq!(f.env.tasks.len(), 9);
        assert_eq!(f.env.reactions.len(), 9);
        assert_eq!(f.env.reactions[1].name, "NAND");
        assert_eq!(f.env.tasks[f.env.reactions[1].task.unwrap()].name, "nand");
        // An empty or out-of-range cell inspects nothing.
        for cell in [1, -1, 1_000_000] {
            let sub = Subscription {
                inspect_cell: Some(cell),
                ..Subscription::default()
            };
            assert!(capture(w, &sub).inspect.is_none());
        }
        seen = true;
    });
    assert!(seen);
}
