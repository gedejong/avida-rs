//! Capturing must not change the run: `heads_default_100u` run straight
//! through, and run again with a stop between every pair of updates at which
//! every layer, every genotype and many organisms are captured, rendered
//! and printed, must write byte-identical data files (timestamp lines
//! aside) and the same status lines.

mod common;

use std::fs;

use avida_viewmodel::{
    all_layers, capture, find_organism, ip_positions, newest_organism_since, probe, render_rgba,
    to_text, update_row, Histogram, LayerId, Palette, Subscription,
};
use avida_world::world::World;

use common::{append_events, build_world, files_under, fresh_config, normalized, run_with_stops};

const TEST: &str = "heads_default_100u";
const LAST_UPDATE: i32 = 100;

/// Everything a front end could ask for at one stop.
fn capture_everything(w: &World, update_index: usize) -> usize {
    let mut work = 0;
    let inspect_cell = w.pop.live_org_list.first().copied();
    let sub = Subscription {
        layers: all_layers(w),
        inspect_cell,
        genotypes_top_n: usize::MAX,
        // The newest genotype has the longest ancestry.
        lineage_of: w
            .systematics
            .dominant_genotype()
            .and_then(|g| w.systematics.group_id(g)),
    };
    let frame = capture(w, &sub);
    // The per-slice probes the session uses.
    if let Some(cell) = inspect_cell {
        let p = probe(w, cell).expect("a living organism");
        work += ip_positions(w, cell).len();
        assert_eq!(find_organism(w, p.org_id), Some(cell));
        work += newest_organism_since(w, 0).map_or(0, |n| n.cell as usize);
    }
    work += frame.lineage.len();
    work += to_text(&frame).len();
    let map = frame.map.as_ref().expect("map subscribed");
    for layer in std::iter::once(LayerId::Genotype).chain(map.layers.iter().map(|(l, _)| *l)) {
        for palette in [Palette::Viridis, Palette::Cividis, Palette::OkabeIto] {
            work += render_rgba(map, layer, &palette, layer.default_scale()).len();
        }
        if let Some(v) = map.layer(layer) {
            work += Histogram::new(v, 16, layer.default_scale()).counts.len();
        }
    }
    work += update_row(w).task_last_count.len();

    // Every organism's CPU on every tenth stop.
    if update_index.is_multiple_of(10) {
        for cell in 0..w.pop.cells.len() as i32 {
            let f = capture(
                w,
                &Subscription {
                    layers: Vec::new(),
                    inspect_cell: Some(cell),
                    genotypes_top_n: 0,
                    lineage_of: None,
                },
            );
            if let Some(o) = f.inspect {
                work += o.cpu.memory.len();
            }
        }
    }
    work
}

#[test]
fn capturing_between_updates_leaves_the_run_unchanged() {
    // Run A: straight through, as the `avida` binary runs it.
    let plain = fresh_config(TEST, "determinism_plain");
    let mut out_plain = Vec::new();
    {
        let mut w = build_world(&plain);
        w.run(&mut out_plain).unwrap_or_else(|e| panic!("run: {e}"));
        w.out.flush_all();
        assert_eq!(w.stats.update(), LAST_UPDATE);
    }

    // Run B: an `Exit` at every update, capture at every stop, resume.
    let probed = fresh_config(TEST, "determinism_capture");
    append_events(&probed, "u 0:1:end Exit\n");
    let mut out_probed = Vec::new();
    let mut stops = Vec::new();
    let mut work = 0;
    {
        let mut w = build_world(&probed);
        run_with_stops(&mut w, LAST_UPDATE, &mut out_probed, |w| {
            work += capture_everything(w, stops.len());
            stops.push(w.stats.update());
        });
    }
    assert!(work > 0);
    assert_eq!(stops, (0..=LAST_UPDATE).collect::<Vec<_>>());

    let (da, db) = (plain.join("data"), probed.join("data"));
    let mut fa = Vec::new();
    let mut fb = Vec::new();
    files_under(&da, &da, &mut fa);
    files_under(&db, &db, &mut fb);
    assert!(fa.len() >= 10, "expected the run's data files, got {fa:?}");
    assert_eq!(fa, fb, "different sets of data files");
    let differing: Vec<String> = fa
        .iter()
        .filter(|f| normalized(&da.join(f)) != normalized(&db.join(f)))
        .map(|f| f.display().to_string())
        .collect();
    assert!(differing.is_empty(), "files differ: {differing:?}");
    // The timestamp filter must not hide everything.
    assert!(fs::read(da.join("average.dat")).unwrap().len() > 1000);

    assert_eq!(
        String::from_utf8_lossy(&out_plain),
        String::from_utf8_lossy(&out_probed),
        "status lines differ"
    );
}
