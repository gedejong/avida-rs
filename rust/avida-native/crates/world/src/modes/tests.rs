//! Unit tests of the pure core of `PrintMODESData` on hand-built lineages.

use super::*;

fn snap(entries: &[(i32, i32)]) -> HashMap<i32, i32> {
    entries.iter().copied().collect()
}

/// `persistent_taxa` over a lineage given as `child -> parents`.
fn persistent(
    snapshot: &[(i32, i32)],
    living: &[i32],
    parents: &[(i32, &[i32])],
    mode: FilterMode,
) -> Vec<i32> {
    let table: HashMap<i32, Vec<i32>> = parents.iter().map(|&(c, p)| (c, p.to_vec())).collect();
    persistent_taxa(
        &snap(snapshot),
        living,
        |n| n,
        |n| table.get(&n).cloned().unwrap_or_default(),
        mode,
    )
    .into_keys()
    .collect()
}

fn skel(s: &str) -> Skeleton {
    Arc::new((s.to_string(), s.len()))
}

fn f_of(entries: &[(&str, i64)]) -> Fmap {
    entries
        .iter()
        .map(|&(s, c)| (s.to_string(), (c, s.len())))
        .collect()
}

#[test]
fn arguments_are_validated_loudly() {
    let ok = parse("PrintMODESData", "modes.dat 100 500")
        .unwrap()
        .unwrap();
    assert_eq!(
        (ok.filename.as_str(), ok.resolution, ok.filter, ok.mode),
        ("modes.dat", 100, 500, FilterMode::Nearest)
    );
    let any = parse("printmodesdata", "m.dat 10 10 ANY").unwrap().unwrap();
    assert_eq!(any.mode, FilterMode::Any);
    assert!(parse("PrintAverageData", "x").is_none());
    for bad in [
        "",
        "modes.dat",
        "modes.dat 100",
        "modes.dat 100 250",
        "modes.dat 100 50",
        "modes.dat 0 0",
        "modes.dat -10 -20",
        "modes.dat ten 100",
        "modes.dat 100 abc",
        "modes.dat 100 500 sideways",
        "modes.dat 100 500 any extra",
        "modes.dat 1.5 3",
    ] {
        let e = parse("PrintMODESData", bad).unwrap().unwrap_err();
        assert!(e.starts_with("PrintMODESData"), "{bad:?}: {e}");
    }
    // Different arguments are different instances; equal arguments are one.
    let a = parse("PrintMODESData", "a.dat 10 20").unwrap().unwrap();
    let b = parse("PrintMODESData", "a.dat 10 30").unwrap().unwrap();
    let c = parse("PrintMODESData", "a.dat 10 20").unwrap().unwrap();
    assert_ne!(a.key, b.key);
    assert_eq!(a.key, c.key);
}

#[test]
fn one_sample_per_crossing() {
    // Smooth progress: one sample on each multiple.
    assert_eq!(crossings(0.0, 50, 0), (0, 0));
    assert_eq!(crossings(49.999, 50, 0), (0, 0));
    assert_eq!(crossings(50.0, 50, 0), (1, 1));
    assert_eq!(crossings(75.0, 50, 1), (1, 0));
    // A jump over two multiples (3 to 5) is two crossings, where the
    // reference code would drop the sample for generation 4 * 50.
    assert_eq!(crossings(251.0, 50, 3), (5, 2));
    // The generation going down samples nothing until it passes the old index.
    assert_eq!(crossings(120.0, 50, 5), (5, 0));
    assert_eq!(crossings(250.0, 50, 5), (5, 0));
    assert_eq!(crossings(300.0, 50, 5), (6, 1));
    assert_eq!(crossings(f64::NAN, 50, 5), (5, 0));
    assert_eq!(crossings(-3.0, 50, 0), (0, 0));
}

#[test]
fn window_yields_the_snapshot_k_samples_back() {
    let mut w = Window::new(2);
    let mk = |u| Snapshot {
        update: u,
        taxa: Arc::new(vec![]),
    };
    assert!(w.push(mk(10)).is_none());
    assert!(w.push(mk(20)).is_none());
    assert_eq!(w.push(mk(30)).map(|s| s.update), Some(10));
    assert_eq!(w.push(mk(40)).map(|s| s.update), Some(20));
    assert_eq!(w.snapshots().count(), 3);
    // k = 0: the snapshot is evaluated at once.
    let mut w0 = Window::new(0);
    assert_eq!(w0.push(mk(5)).map(|s| s.update), Some(5));
}

#[test]
fn persistence_filter_cases() {
    // Snapshot: 1 (root, extinct since), 2 (child of 1), 3 (alive then and
    // now), 4 (extinct, descendant 6 also extinct), 8 (extinct, child 9 of
    // it alive now). Born after the snapshot: 5 (child of 2), 7 (child of 3),
    // 9 (child of 8).
    let snapshot = [(1, 4), (2, 3), (3, 2), (4, 1), (8, 6)];
    let parents: &[(i32, &[i32])] = &[
        (2, &[1]),
        (3, &[]),
        (4, &[]),
        (5, &[2]),
        (6, &[4]),
        (7, &[3]),
        (8, &[]),
        (9, &[8]),
    ];
    let living = [3, 5, 7, 9];
    // The reference lookup: the nearest snapshot ancestor-or-self. 3 is alive
    // at the snapshot and still alive; 2 is extinct with a living descendant
    // (5); 8 likewise (9); 4 is extinct without one; 1 is extinct and only
    // reached through 2, which the lookup stops at.
    assert_eq!(
        persistent(&snapshot, &living, parents, FilterMode::Nearest),
        vec![2, 3, 8]
    );
    // The paper's wording: 1 has a living descendant too.
    assert_eq!(
        persistent(&snapshot, &living, parents, FilterMode::Any),
        vec![1, 2, 3, 8]
    );
    // Nothing alive: nothing persists.
    assert!(persistent(&snapshot, &[], parents, FilterMode::Nearest).is_empty());
    assert!(persistent(&snapshot, &[], parents, FilterMode::Any).is_empty());
}

#[test]
fn nearest_stops_at_the_first_snapshot_ancestor() {
    // 1 -> 2 -> 3 -> 4, all alive at the snapshot, 4 alive now: the lookup
    // finds 4 itself only.
    let snapshot = [(1, 1), (2, 1), (3, 1), (4, 1)];
    let parents: &[(i32, &[i32])] = &[(2, &[1]), (3, &[2]), (4, &[3])];
    assert_eq!(
        persistent(&snapshot, &[4], parents, FilterMode::Nearest),
        vec![4]
    );
    assert_eq!(
        persistent(&snapshot, &[4], parents, FilterMode::Any),
        vec![1, 2, 3, 4]
    );
    // A long lineage after the snapshot, shared by two living tips: both
    // resolve to the one snapshot ancestor (memoised walk).
    let snapshot = [(1, 7)];
    let mut table: Vec<(i32, Vec<i32>)> = Vec::new();
    for i in 2..200 {
        table.push((i, vec![i - 1]));
    }
    table.push((300, vec![199]));
    table.push((301, vec![199]));
    let parents: Vec<(i32, &[i32])> = table.iter().map(|(c, p)| (*c, p.as_slice())).collect();
    assert_eq!(
        persistent(&snapshot, &[300, 301, 150], &parents, FilterMode::Nearest),
        vec![1]
    );
}

#[test]
fn two_genotypes_share_a_skeleton() {
    let counts = snap(&[(2, 3), (3, 2), (8, 6), (4, 1)]);
    let skeletons: HashMap<i32, Skeleton> = [
        (2, skel("abc")),
        (3, skel("abc")),
        (8, skel("xy")),
        (4, skel("zzzz")),
    ]
    .into_iter()
    .collect();
    // 4 is not persistent and 7 has no count at the snapshot.
    let f = collect_f([2, 3, 8, 7], &counts, &skeletons);
    assert_eq!(f.len(), 2);
    assert_eq!(f["abc"], (5, 3));
    assert_eq!(f["xy"], (6, 2));
}

#[test]
fn metrics_on_the_first_sample_and_after() {
    let mut acc = Accumulator::new();
    // The first evaluated sample: F' and S are empty, so change and novelty
    // are |F|.
    let m1 = acc.step(&f_of(&[("ab", 2), ("c", 2)]));
    assert_eq!((m1.change, m1.novelty, m1.complexity), (2, 2, 2));
    assert!((m1.ecology - 1.0).abs() < 1e-12);
    // The same F: nothing changes, nothing is new.
    let m2 = acc.step(&f_of(&[("ab", 9), ("c", 1)]));
    assert_eq!((m2.change, m2.novelty), (0, 0));
    // One new skeleton "dddd" (longest), one gone: change and novelty 1.
    let m3 = acc.step(&f_of(&[("ab", 1), ("dddd", 1)]));
    assert_eq!((m3.change, m3.novelty, m3.complexity), (1, 1, 4));
    // A return to an earlier state: "c" was in F two samples ago but not in
    // F', so it counts as change but not as novelty.
    let m4 = acc.step(&f_of(&[("ab", 1), ("c", 1)]));
    assert_eq!((m4.change, m4.novelty), (1, 0));
    // Novelty is never counted twice: "dddd" comes back and is not new.
    let m5 = acc.step(&f_of(&[("dddd", 1)]));
    assert_eq!((m5.change, m5.novelty), (1, 0));
    assert_eq!(acc.cumulative_novelty, 3);
}

#[test]
fn ecology_and_complexity_edge_cases() {
    let mut acc = Accumulator::new();
    let empty = acc.step(&Fmap::new());
    assert_eq!(
        (empty.change, empty.novelty, empty.complexity, empty.ecology),
        (0, 0, 0, 0.0)
    );
    // One skeleton: no entropy.
    let one = acc.step(&f_of(&[("abc", 17)]));
    assert_eq!(one.ecology, 0.0);
    // Eight equally abundant skeletons: 3 bits.
    let eight: Vec<(String, i64)> = (0..8).map(|i| (format!("s{i}"), 5)).collect();
    let f: Fmap = eight
        .into_iter()
        .map(|(s, c)| (s.clone(), (c, s.len())))
        .collect();
    assert!((acc.step(&f).ecology - 3.0).abs() < 1e-12);
    // 1:3 -> -(1/4 log2 1/4 + 3/4 log2 3/4).
    let m = acc.step(&f_of(&[("p", 1), ("q", 3)]));
    let expect = -(0.25f64 * 0.25f64.log2() + 0.75 * 0.75f64.log2());
    assert!((m.ecology - expect).abs() < 1e-12);
    // After an empty F every skeleton is change again, but not novelty.
    acc.step(&Fmap::new());
    let back = acc.step(&f_of(&[("p", 1)]));
    assert_eq!((back.change, back.novelty), (1, 0));
}

#[test]
fn kept_sites_are_the_strict_losers() {
    // Fitness 10; sites: 0 neutral, 1 lethal, 2 beneficial to knock out,
    // 3 mildly informative.
    let table = [10.0, 10.0, 0.0, 12.0, 9.5];
    let mut calls = 0;
    let kept = kept_sites(4, &mut |site| {
        calls += 1;
        Ok(match site {
            None => table[0],
            Some(i) => table[i + 1],
        })
    })
    .unwrap();
    assert_eq!(kept, vec![1, 3]);
    // L + 1 test runs.
    assert_eq!(calls, 5);
    // An error stops the skeleton.
    let e = kept_sites(3, &mut |s| match s {
        Some(1) => Err("boom".to_string()),
        _ => Ok(1.0),
    });
    assert_eq!(e, Err("boom".to_string()));
}

#[test]
fn unexploited_levels_follow_inflow_and_outflow() {
    let mut grid = Resource::new("g", 0);
    grid.geometry = geometry::GRID;
    grid.inflow = 1280.0;
    grid.outflow = 0.05;
    grid.inflow_x1 = 0;
    grid.inflow_x2 = 159;
    grid.inflow_y1 = 0;
    grid.inflow_y2 = 159;
    let mut region = grid.clone();
    // A quarter of the world: 4 times the inflow per cell.
    region.inflow_x2 = 79;
    region.inflow_y2 = 79;
    let mut global = Resource::new("o", 1);
    global.inflow = 10.0;
    global.outflow = 0.1;
    let mut seeded = Resource::new("s", 2);
    seeded.initial = 50.0;
    let mut unbounded = Resource::new("u", 3);
    unbounded.inflow = 3.0;
    let levels = unexploited_levels(&[grid.clone(), region, global, seeded, unbounded], 160, 160);
    assert!((levels[0] - 1.0).abs() < 1e-12);
    assert!((levels[1] - 4.0).abs() < 1e-12);
    assert!((levels[2] - 100.0).abs() < 1e-12);
    assert_eq!(levels[3], 50.0);
    // No outflow: nothing to settle at, `initial` (0) it is.
    assert_eq!(levels[4], 0.0);
    // An inflow box left at its default covers the world.
    let mut unset = grid;
    unset.inflow_x1 = -99;
    assert!((unexploited_levels(&[unset], 160, 160)[0] - 1.0).abs() < 1e-12);
}

#[test]
fn the_row_history_keeps_the_newest_rows() {
    let mut q = VecDeque::new();
    for i in 0..10 {
        push_capped(&mut q, i, 4);
    }
    assert_eq!(q.iter().copied().collect::<Vec<_>>(), vec![6, 7, 8, 9]);
    // A cap of zero still keeps the newest row.
    push_capped(&mut q, 10, 0);
    assert_eq!(q.len(), 1);
    assert_eq!(HISTORY_CAP, 2_000);
}
