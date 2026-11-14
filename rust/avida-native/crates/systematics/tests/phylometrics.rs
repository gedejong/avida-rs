//! Cross-check of `avida_systematics::phylometrics` against brute-force
//! implementations written from the definitions, and (for the MRCA,
//! evolutionary distinctiveness, branches to the root and the Colless-like
//! index) as line-by-line transcriptions of the Empirical reference code in
//! `Systematics.hpp`. The brute force is deliberately naive and recursive; the
//! tree sizes here keep it within the stack.

use avida_systematics::phylometrics::{alife_csv, compute, pruned, PhyloNode};

// ---------------------------------------------------------------------------
// Random trees
// ---------------------------------------------------------------------------

struct Rng(u64);
impl Rng {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }
    fn below(&mut self, n: usize) -> usize {
        (self.next() % n as u64) as usize
    }
    fn chance(&mut self, p: f64) -> bool {
        ((self.next() >> 11) as f64 / (1u64 << 53) as f64) < p
    }
}

#[derive(Clone, Copy, Debug)]
enum Shape {
    Chainy,
    Bushy,
    Mixed,
}

/// A random tree (parents before children, then shuffled). `living`: how
/// many nodes are alive (clamped); `prefix`: a dead unifurcation chain above
/// the first real node; `root_alive`: the root is alive.
fn random_tree(
    rng: &mut Rng,
    n: usize,
    shape: Shape,
    living: usize,
    prefix: usize,
    root_alive: bool,
    shuffle: bool,
) -> (Vec<PhyloNode>, i32) {
    let total = n + prefix;
    let mut parent: Vec<Option<usize>> = vec![None; total];
    let mut origin = vec![0i32; total];
    for i in 1..total {
        let p = if i <= prefix {
            i - 1
        } else {
            match shape {
                Shape::Chainy => {
                    if rng.chance(0.92) {
                        i - 1
                    } else {
                        rng.below(i)
                    }
                }
                Shape::Bushy => rng.below(i),
                Shape::Mixed => {
                    if rng.chance(0.5) {
                        i - 1
                    } else {
                        rng.below(i)
                    }
                }
            }
        };
        parent[i] = Some(p);
        origin[i] = origin[p] + rng.below(6) as i32;
    }
    let mut alive = vec![false; total];
    let living = living.min(total);
    let mut placed = 0;
    if root_alive && total > 0 {
        alive[0] = true;
        placed = 1;
    }
    let mut guard = 0;
    while placed < living && guard < 100_000 {
        guard += 1;
        let i = rng.below(total);
        if !alive[i] {
            alive[i] = true;
            placed += 1;
        }
    }
    let now = origin.iter().copied().max().unwrap_or(0) + rng.below(5) as i32;
    // Unique ids, not in index order.
    let mut ids: Vec<i32> = (0..total as i32).map(|i| i * 3 + 7).collect();
    for i in (1..total).rev() {
        ids.swap(i, rng.below(i + 1));
    }
    let mut nodes: Vec<PhyloNode> = (0..total)
        .map(|i| PhyloNode {
            id: ids[i],
            parent: parent[i],
            abundance: if alive[i] {
                1 + rng.below(40) as i32
            } else {
                0
            },
            origin: origin[i],
            depth: 0,
            destroyed: -1,
        })
        .collect();
    if shuffle {
        let mut perm: Vec<usize> = (0..total).collect();
        for i in (1..total).rev() {
            perm.swap(i, rng.below(i + 1));
        }
        // perm[new] = old; inverse maps old -> new.
        let mut inv = vec![0; total];
        for (new, &old) in perm.iter().enumerate() {
            inv[old] = new;
        }
        nodes = perm
            .iter()
            .map(|&old| {
                let mut x = nodes[old].clone();
                x.parent = x.parent.map(|p| inv[p]);
                x
            })
            .collect();
    }
    (nodes, now)
}

// ---------------------------------------------------------------------------
// Brute force
// ---------------------------------------------------------------------------

/// Empirical's view of the pruned tree.
struct Emp {
    id: Vec<i32>,
    parent: Vec<Option<usize>>,
    num_orgs: Vec<i32>,
    origin: Vec<i32>,
    /// kept children, ascending id (Empirical's `GetOffspring()`)
    off: Vec<Vec<usize>>,
    total_off: Vec<usize>,
    active: Vec<usize>,
    kept: Vec<bool>,
}

impl Emp {
    fn new(nodes: &[PhyloNode]) -> Emp {
        let n = nodes.len();
        let mut kept = vec![false; n];
        for i in 0..n {
            if nodes[i].abundance > 0 {
                let mut x = Some(i);
                while let Some(y) = x {
                    kept[y] = true;
                    x = nodes[y].parent;
                }
            }
        }
        let mut off = vec![vec![]; n];
        for i in 0..n {
            if kept[i] {
                if let Some(p) = nodes[i].parent {
                    off[p].push(i);
                }
            }
        }
        for o in off.iter_mut() {
            o.sort_by_key(|&c| nodes[c].id);
        }
        // total_offspring: extant (living) strict descendants.
        let mut total_off = vec![0usize; n];
        for i in 0..n {
            if nodes[i].abundance > 0 {
                let mut x = nodes[i].parent;
                while let Some(y) = x {
                    total_off[y] += 1;
                    x = nodes[y].parent;
                }
            }
        }
        Emp {
            id: nodes.iter().map(|x| x.id).collect(),
            parent: nodes.iter().map(|x| x.parent).collect(),
            num_orgs: nodes.iter().map(|x| x.abundance).collect(),
            origin: nodes.iter().map(|x| x.origin).collect(),
            off,
            total_off,
            active: (0..n).filter(|&i| nodes[i].abundance > 0).collect(),
            kept,
        }
    }

    fn num_roots(&self) -> usize {
        (0..self.parent.len())
            .filter(|&i| self.kept[i] && self.parent[i].is_none())
            .count()
    }

    /// `GetMRCA`.
    fn get_mrca(&self) -> Option<usize> {
        if self.num_roots() != 1 || self.active.is_empty() {
            return None;
        }
        let mut candidate = *self
            .active
            .iter()
            .find(|&&x| self.off[x].len() != 1)
            .unwrap();
        let mut test_taxon = self.parent[candidate];
        while let Some(t) = test_taxon {
            if self.off[t].len() > 1 || self.num_orgs[t] > 0 {
                candidate = t;
            }
            test_taxon = self.parent[t];
        }
        Some(candidate)
    }

    /// `GetEvolutionaryDistinctiveness(tax, time)`; -1 when unconnected.
    fn evolutionary_distinctiveness(&self, tax: usize, mut time: f64) -> f64 {
        let mut depth = 0.0;
        let mut total = 0.0;
        let mut divisor = (self.total_off[tax] + 1) as f64;
        let mrca = self.get_mrca();
        if Some(tax) == mrca {
            return 0.0;
        }
        let mut test_taxon = self.parent[tax];
        assert!(time >= self.origin[tax] as f64);
        while let Some(t) = test_taxon {
            depth += time - self.origin[t] as f64;
            time = self.origin[t] as f64;
            if Some(t) == mrca {
                total += depth / divisor;
                return total;
            } else if self.num_orgs[t] > 0 {
                total += depth / divisor;
                depth = 0.0;
                divisor = (self.total_off[t] + 1) as f64;
            } else if self.off[t].len() > 1 {
                total += depth / divisor;
                depth = 0.0;
                divisor = self.total_off[t] as f64;
            }
            test_taxon = self.parent[t];
        }
        -1.0
    }

    /// `GetBranchesToRoot`, except that the MRCA itself is 0 (pre-MRCA taxa
    /// are excluded by definition).
    fn branches_to_root(&self, tax: usize) -> u64 {
        let mrca = self.get_mrca();
        if Some(tax) == mrca {
            return 0;
        }
        let mut depth = 0;
        let mut test_taxon = self.parent[tax];
        while let Some(t) = test_taxon {
            if Some(t) == mrca {
                return depth;
            } else if self.off[t].len() > 1 {
                depth += 1;
            }
            test_taxon = self.parent[t];
        }
        depth
    }

    /// `RecursiveCollessStep` -> (n_values, total).
    fn colless_step(&self, mut curr: usize) -> (Vec<f64>, f64) {
        let mut n_values: Vec<f64> = vec![];
        let mut total = 0.0;
        while self.off[curr].len() == 1 {
            curr = self.off[curr][0];
        }
        if self.off[curr].is_empty() {
            n_values.push(0.0);
            return (n_values, total);
        }
        for &off in &self.off[curr] {
            let (nv, tot) = self.colless_step(off);
            n_values.push(nv.iter().sum::<f64>() + (self.off[off].len() as f64 + 1f64.exp()).ln());
            total += tot;
        }
        let mut sorted = n_values.clone();
        sorted.sort_by(|a, b| a.partial_cmp(b).unwrap());
        let med = if sorted.len() % 2 == 1 {
            sorted[sorted.len() / 2]
        } else {
            (sorted[sorted.len() / 2 - 1] + sorted[sorted.len() / 2]) / 2.0
        };
        let sum_diffs = n_values.iter().fold(0.0, |s, n| s + (n - med).abs());
        total += sum_diffs / n_values.len() as f64;
        (n_values, total)
    }

    fn is_below(&self, x: usize, anc: usize) -> bool {
        let mut y = Some(x);
        while let Some(z) = y {
            if z == anc {
                return true;
            }
            y = self.parent[z];
        }
        false
    }

    /// Path length between two nodes (edges), by ancestor marking.
    fn distance(&self, a: usize, b: usize) -> u64 {
        let mut dist_a = std::collections::HashMap::new();
        let mut x = Some(a);
        let mut d = 0u64;
        while let Some(y) = x {
            dist_a.insert(y, d);
            d += 1;
            x = self.parent[y];
        }
        let mut x = Some(b);
        let mut d = 0u64;
        while let Some(y) = x {
            if let Some(&da) = dist_a.get(&y) {
                return da + d;
            }
            d += 1;
            x = self.parent[y];
        }
        panic!("unconnected");
    }
}

fn close(a: f64, b: f64) -> bool {
    if a.is_nan() || b.is_nan() {
        return a.is_nan() && b.is_nan();
    }
    (a - b).abs() <= 1e-9 * (1.0 + a.abs().max(b.abs()))
}

fn sample_var(v: &[f64]) -> f64 {
    if v.len() < 2 {
        return f64::NAN;
    }
    let mean = v.iter().sum::<f64>() / v.len() as f64;
    v.iter().map(|x| (mean - x).powi(2)).sum::<f64>() / (v.len() - 1) as f64
}

/// Compare everything for one tree.
fn check(nodes: &[PhyloNode], now: i32, label: &str) {
    let m = compute(nodes, now);
    let e = Emp::new(nodes);
    assert_eq!(m.taxa, e.active.len(), "{label}: taxa");
    assert_eq!(
        m.pruned_nodes,
        e.kept.iter().filter(|&&k| k).count(),
        "{label}: pruned"
    );
    // Shannon, from the definition.
    let tot: f64 = e.active.iter().map(|&i| e.num_orgs[i] as f64).sum();
    let h: f64 = e
        .active
        .iter()
        .map(|&i| {
            let p = e.num_orgs[i] as f64 / tot;
            -p * p.log2()
        })
        .sum();
    assert!(
        close(m.shannon, if e.active.is_empty() { 0.0 } else { h }),
        "{label}: shannon"
    );

    let mrca = e.get_mrca();
    assert_eq!(m.mrca, mrca, "{label}: mrca");
    let Some(mrca) = mrca else {
        assert!(m.pair_mean.is_nan() && m.colless.is_nan());
        return;
    };
    // PD = edges below the MRCA.
    let below: Vec<usize> = (0..nodes.len())
        .filter(|&i| e.kept[i] && e.is_below(i, mrca))
        .collect();
    assert_eq!(m.nodes, below.len(), "{label}: nodes");
    assert_eq!(m.pd, below.len() - 1, "{label}: pd");
    // Pairwise distances over all unordered pairs.
    let mut dists: Vec<f64> = vec![];
    let mut isum: u128 = 0;
    let mut isq: u128 = 0;
    for (a, &i) in e.active.iter().enumerate() {
        for &j in &e.active[a + 1..] {
            let d = e.distance(i, j);
            dists.push(d as f64);
            isum += d as u128;
            isq += (d * d) as u128;
        }
    }
    assert_eq!(m.pair_count as usize, dists.len(), "{label}: pairs");
    assert_eq!(m.pair_sum, isum, "{label}: pair sum");
    assert_eq!(m.pair_sumsq, isq, "{label}: pair sumsq");
    let bmean = if dists.is_empty() {
        f64::NAN
    } else {
        isum as f64 / dists.len() as f64
    };
    assert!(
        close(m.pair_mean, bmean),
        "{label}: pair mean {} vs {}",
        m.pair_mean,
        bmean
    );
    assert!(
        close(m.pair_var, sample_var(&dists)),
        "{label}: pair var {} vs {}",
        m.pair_var,
        sample_var(&dists)
    );
    // Evolutionary distinctiveness.
    let eds: Vec<f64> = e
        .active
        .iter()
        .map(|&i| e.evolutionary_distinctiveness(i, now as f64))
        .collect();
    assert!(eds.iter().all(|&x| x >= 0.0), "{label}: ED connected");
    let esum: f64 = eds.iter().sum();
    assert!(
        close(m.ed_sum, esum),
        "{label}: ed sum {} vs {}",
        m.ed_sum,
        esum
    );
    assert!(
        close(m.ed_mean, esum / eds.len() as f64),
        "{label}: ed mean"
    );
    assert!(close(m.ed_var, sample_var(&eds)), "{label}: ed var");
    // Sackin.
    let sackin: u64 = e.active.iter().map(|&i| e.branches_to_root(i) + 1).sum();
    assert_eq!(m.sackin, sackin, "{label}: sackin");
    // Colless-like.
    let (_, colless) = e.colless_step(mrca);
    assert!(
        close(m.colless, colless),
        "{label}: colless {} vs {}",
        m.colless,
        colless
    );
    // Mean depth below the MRCA.
    let md: f64 = e
        .active
        .iter()
        .map(|&i| e.distance(i, mrca) as f64)
        .sum::<f64>()
        / e.active.len() as f64;
    assert!(close(m.mean_depth, md), "{label}: mean depth");
    // Lineage of the most abundant taxon.
    let top = *e
        .active
        .iter()
        .max_by_key(|&&i| (e.num_orgs[i], std::cmp::Reverse(e.id[i])))
        .unwrap();
    assert_eq!(m.top, Some(top));
    let mut len = 0;
    let mut x = top;
    while let Some(p) = e.parent[x] {
        len += 1;
        x = p;
    }
    assert_eq!(m.lineage_length, len);
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

fn node(id: i32, parent: Option<usize>, abundance: i32, origin: i32) -> PhyloNode {
    PhyloNode {
        id,
        parent,
        abundance,
        origin,
        depth: 0,
        destroyed: -1,
    }
}

#[test]
fn two_living_leaves_under_a_dead_root() {
    // R(0, dead) -> A(10, 1 org), B(20, 1 org); now = 100.
    let nodes = vec![
        node(1, None, 0, 0),
        node(2, Some(0), 1, 10),
        node(3, Some(0), 1, 20),
    ];
    let m = compute(&nodes, 100);
    assert_eq!((m.taxa, m.nodes, m.pd), (2, 3, 2));
    assert_eq!((m.pair_count, m.pair_sum, m.pair_sumsq), (1, 2, 4));
    assert_eq!(m.pair_mean, 2.0);
    assert!(m.pair_var.is_nan());
    assert_eq!(m.ed_sum, 200.0);
    assert_eq!(m.sackin, 2);
    assert_eq!(m.colless, 0.0);
    assert_eq!(m.shannon, 1.0);
    check(&nodes, 100, "two leaves");
}

#[test]
fn living_internal_nodes_follow_the_reference() {
    // R(0, alive) -> A(10, alive) -> B(30, alive); now = 100.
    let nodes = vec![
        node(1, None, 1, 0),
        node(2, Some(0), 1, 10),
        node(3, Some(1), 1, 30),
    ];
    let m = compute(&nodes, 100);
    assert_eq!(m.mrca, Some(0));
    assert_eq!((m.taxa, m.nodes, m.pd), (3, 3, 2));
    // distances R-A 1, A-B 1, R-B 2
    assert_eq!((m.pair_count, m.pair_sum, m.pair_sumsq), (3, 4, 6));
    // ED: R 0; A (100-0)/2 = 50; B (100-10)/1 + (10-0)/2 = 95.
    assert_eq!(m.ed_sum, 145.0);
    // Sackin: each taxon 0 branch points + 1.
    assert_eq!(m.sackin, 3);
    assert_eq!(m.colless, 0.0);
    check(&nodes, 100, "living chain");

    // A living node that also branches: R(alive) -> A(alive) -> {B, C}.
    let nodes = vec![
        node(1, None, 2, 0),
        node(2, Some(0), 1, 10),
        node(3, Some(1), 1, 30),
        node(4, Some(1), 3, 40),
    ];
    check(&nodes, 100, "living branching");
    let m = compute(&nodes, 100);
    // B: branches to root = 1 (A, two children); R: 0.
    assert_eq!(m.sackin, 1 + 1 + 2 + 2);
}

#[test]
fn single_living_taxon_and_pre_mrca_exclusion() {
    // dead chain 1 -> 2 -> 3 -> 4(alive): the MRCA is the living taxon.
    let nodes = vec![
        node(1, None, 0, 0),
        node(2, Some(0), 0, 5),
        node(3, Some(1), 0, 9),
        node(4, Some(2), 7, 12),
    ];
    let m = compute(&nodes, 20);
    assert_eq!(m.mrca, Some(3));
    assert_eq!((m.taxa, m.nodes, m.pd, m.pruned_nodes), (1, 1, 0, 4));
    assert_eq!(m.pair_count, 0);
    assert!(m.pair_mean.is_nan() && m.pair_var.is_nan() && m.ed_var.is_nan());
    assert_eq!(m.ed_sum, 0.0);
    assert_eq!(m.sackin, 1);
    assert_eq!(m.colless, 0.0);
    assert_eq!(m.shannon, 0.0);
    assert_eq!(m.lineage_length, 3);
    check(&nodes, 20, "single");
}

#[test]
fn no_mrca_for_a_forest_and_nothing_alive() {
    let nodes = vec![node(1, None, 1, 0), node(2, None, 1, 0)];
    let m = compute(&nodes, 5);
    assert_eq!((m.taxa, m.mrca), (2, None));
    assert!(m.colless.is_nan());
    let m = compute(&[node(1, None, 0, 0)], 5);
    assert_eq!((m.taxa, m.mrca, m.pruned_nodes), (0, None, 0));
    assert!(compute(&[], 0).mrca.is_none());
}

#[test]
fn random_trees_match_brute_force() {
    let mut rng = Rng(0x9E3779B97F4A7C15);
    let mut count = 0;
    for trial in 0..4000 {
        let n = 1 + rng.below(if trial % 10 == 0 { 120 } else { 40 });
        let shape = [Shape::Chainy, Shape::Bushy, Shape::Mixed][rng.below(3)];
        let living = match rng.below(5) {
            0 => 1,
            1 => 2,
            2 => 3,
            _ => 1 + rng.below(n.max(1)),
        };
        let prefix = if rng.chance(0.4) { rng.below(6) } else { 0 };
        let root_alive = rng.chance(0.15);
        let shuffle = rng.chance(0.5);
        let (nodes, now) = random_tree(&mut rng, n, shape, living, prefix, root_alive, shuffle);
        check(
            &nodes,
            now,
            &format!("trial {trial} {shape:?} n={n} living={living}"),
        );
        count += 1;
    }
    assert_eq!(count, 4000);
}

#[test]
fn long_chains_and_caterpillars() {
    let mut rng = Rng(12345);
    for &(n, living) in &[(1500usize, 1usize), (1500, 2), (1500, 40), (900, 150)] {
        let (nodes, now) = random_tree(&mut rng, n, Shape::Chainy, living, 30, false, true);
        check(&nodes, now, &format!("chain n={n} living={living}"));
    }
    // A pure chain with every node alive (all internal living).
    let nodes: Vec<PhyloNode> = (0..800)
        .map(|i| {
            node(
                i,
                if i == 0 { None } else { Some(i as usize - 1) },
                1,
                i * 2,
            )
        })
        .collect();
    check(&nodes, 2000, "all-living chain");
}

#[test]
fn deep_caterpillar_does_not_overflow_the_stack() {
    // 60,000 branch points in a row: any per-node recursion would overflow.
    let depth = 60_000usize;
    let mut nodes = vec![node(0, None, 0, 0)];
    for i in 0..depth {
        let spine = nodes.len() - 1;
        let id = nodes.len() as i32;
        nodes.push(node(id, Some(spine), 0, i as i32 + 1)); // next spine node
        let id = nodes.len() as i32;
        nodes.push(node(id, Some(spine), 1, i as i32 + 1)); // a living leaf
    }
    let last = nodes.len() - 2;
    nodes[last].abundance = 1;
    let m = compute(&nodes, depth as i32 + 10);
    assert_eq!(m.taxa, depth + 1);
    // every leaf has one organism: the lowest id wins, the first leaf (id 2)
    assert_eq!(m.lineage_length, 1);
    assert!(m.colless.is_finite() && m.ed_sum.is_finite());
    assert_eq!(
        m.pair_count as u128 * 2,
        (depth as u128 + 1) * depth as u128
    );
}

#[test]
fn pruning_and_alife_csv() {
    // 0 dead root -> 1 living, 0 -> 2 dead -> 3 dead (no living below: pruned).
    let mut nodes = vec![
        node(10, None, 0, 0),
        node(11, Some(0), 4, 3),
        node(12, Some(0), 0, 2),
        node(13, Some(2), 0, 6),
    ];
    nodes[2].destroyed = 40;
    assert_eq!(pruned(&nodes), vec![true, true, false, false]);
    let csv = alife_csv(&nodes);
    assert_eq!(
        csv,
        "id,ancestor_list,origin_time,destruction_time,num_orgs,depth\n\
         10,[none],0,,0,0\n11,[10],3,inf,4,0\n"
    );
    // Dead nodes that carry a living descendant keep their destruction time.
    nodes[3].abundance = 1;
    let csv = alife_csv(&nodes);
    assert!(csv.contains("12,[10],2,40,0,0\n"));
    assert!(csv.contains("13,[12],6,inf,1,0\n"));
}

/// Time on a large tree (run with `--ignored --nocapture`, release build).
#[test]
#[ignore]
fn large_tree_timing() {
    let mut rng = Rng(777);
    for &(n, living, shape) in &[
        (200_000usize, 20_000usize, Shape::Mixed),
        (200_000, 20_000, Shape::Chainy),
        (100_000, 20_000, Shape::Bushy),
    ] {
        let (nodes, now) = random_tree(&mut rng, n, shape, living, 50, false, true);
        let t = std::time::Instant::now();
        let m = compute(&nodes, now);
        let dt = t.elapsed();
        println!(
            "{shape:?} nodes={} pruned={} taxa={} tree={} pairs={} pair_mean={:.3} sackin={} colless={:.3}: {:?}",
            nodes.len(),
            m.pruned_nodes,
            m.taxa,
            m.nodes,
            m.pair_count,
            m.pair_mean,
            m.sackin,
            m.colless,
            dt
        );
    }
}
