//! The open-endedness rows of a frame are the rows of the data files, and
//! asking for them changes nothing in the run.

mod common;

use std::fs;

use avida_viewmodel::{capture, Subscription};

/// The data rows of a `.dat` file: whitespace-separated numbers, comment
/// lines left out.
fn rows(path: &std::path::Path) -> Vec<Vec<f64>> {
    fs::read_to_string(path)
        .unwrap_or_else(|e| panic!("{}: {e}", path.display()))
        .lines()
        .filter(|l| !l.trim().is_empty() && !l.starts_with('#'))
        .map(|l| {
            l.split_whitespace()
                .map(|w| w.parse::<f64>().unwrap_or(f64::NAN))
                .collect()
        })
        .collect()
}

#[test]
fn a_frame_carries_the_rows_the_actions_wrote() {
    let dir = common::fresh_config("heads_default_100u", "open_ended");
    common::append_events(
        &dir,
        "u 0:1:end PrintMODESData modes.dat 1 2\n\
         u 0:5:end PrintPhylogenyMetrics phylogeny.dat\n\
         u 100 Exit\n",
    );
    let mut world = common::build_world(&dir);
    // Before anything has fired, a frame has no rows.
    assert!(capture(&world, &Subscription::default())
        .open_ended
        .is_empty());
    let mut out = Vec::new();
    common::run_with_stops(&mut world, 100, &mut out, |_| {});
    let frame = capture(&world, &Subscription::default());
    let oe = &frame.open_ended;

    assert_eq!(oe.modes.len(), 1);
    let m = &oe.modes[0];
    assert_eq!(
        (m.filename.as_str(), m.resolution, m.filter),
        ("modes.dat", 1, 2)
    );
    let file = rows(&dir.join("data/modes.dat"));
    assert!(!file.is_empty(), "the run wrote MODES rows");
    assert_eq!(m.rows.len(), file.len());
    for (r, f) in m.rows.iter().zip(&file) {
        // Columns: update, average generation, snapshot update, change,
        // novelty, complexity, ecology, persistent, skeletons, living,
        // cumulative novelty.
        assert_eq!(f64::from(r.update), f[0]);
        assert!((r.generation - f[1]).abs() <= 1e-4 * f[1].abs().max(1.0));
        assert_eq!(f64::from(r.snapshot_update), f[2]);
        assert_eq!(
            (r.change as f64, r.novelty as f64, r.complexity as f64),
            (f[3], f[4], f[5])
        );
        assert!((r.ecology - f[6]).abs() <= 1e-4);
        assert_eq!(
            (
                r.persistent as f64,
                r.skeletons as f64,
                r.living as f64,
                r.cumulative_novelty as f64
            ),
            (f[7], f[8], f[9], f[10])
        );
    }

    assert_eq!(oe.phylogeny.len(), 1);
    let p = &oe.phylogeny[0];
    assert_eq!(p.filename, "phylogeny.dat");
    let file = rows(&dir.join("data/phylogeny.dat"));
    assert_eq!(p.rows.len(), file.len());
    assert!(p.rows.len() >= 20);
    for (r, f) in p.rows.iter().zip(&file) {
        // Columns: update, living genotypes, nodes, MRCA id, MRCA depth,
        // MRCA changes, PD, pairwise sum, pairwise mean, variance, ED mean...
        assert_eq!(f64::from(r.update), f[0]);
        assert_eq!(f64::from(r.living), f[1]);
        assert_eq!(f64::from(r.mrca_depth), f[4]);
        assert_eq!(f64::from(r.mrca_changes), f[5]);
        assert_eq!(f64::from(r.pd), f[6]);
    }
}
