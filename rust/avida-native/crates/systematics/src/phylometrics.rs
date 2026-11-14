//! Phylogeny-based metrics over a plain snapshot of a genotype tree.
//!
//! An extension beyond the C++ program (nothing here is read by, or changes,
//! the simulation). The definitions follow the Empirical library's
//! `emp::Systematics` (`Systematics.hpp`: `GetMRCA`, `GetPhylogeneticDiversity`,
//! `GetPairwiseDistances`, `GetEvolutionaryDistinctiveness`, `SackinIndex`,
//! `GetBranchesToRoot`, `CollessLikeIndex`, `RecursiveCollessStep`,
//! `CalcDiversity`) and Dolson et al. 2020, "Interpreting the Tape of Life"
//! (Artificial Life 26(1)). Where the two differ the code wins and the
//! difference is stated at the metric.
//!
//! The functions are pure and know nothing of the world: a snapshot is a
//! vector of [`PhyloNode`]. Every metric is O(nodes) or O(nodes log nodes)
//! and none recurses: the pairwise distance sum comes from the edge
//! contribution identity, its sum of squares from a subtree dynamic
//! programme, and the walks the reference does per taxon (evolutionary
//! distinctiveness, branches to the root) become top-down passes.
//!
//! Conventions
//!
//! * A *living* node has `abundance > 0`. The tree is the snapshot pruned to
//!   living nodes and their ancestors (Empirical's `active_taxa` plus
//!   `ancestor_taxa`), rooted at the MRCA: the topmost node on any living
//!   lineage that is living or has two or more children (`GetMRCA`).
//!   Everything above it is excluded. Without exactly one root, or without a
//!   living node, no MRCA exists and the metrics that need one are NaN.
//! * A living node may have living descendants (an *internal* living node).
//!   Pairwise distance: it is a taxon like any other, at distance 0 from its
//!   own position (as `GetPairwiseDistances`, which pairs a living taxon with
//!   the subtrees below it). Evolutionary distinctiveness: it is a split
//!   point whose divisor restarts at its living descendants plus itself
//!   (`GetNumOrgs() > 0` branch). Sackin: it adds a taxon, and counts as a
//!   branching point for the taxa below only if it has two or more children
//!   (as `GetBranchesToRoot`, which tests `GetNumOff() > 1` and not
//!   aliveness). Colless-like: aliveness is ignored, only the shape counts
//!   (`RecursiveCollessStep`). Phylogenetic diversity: a node like any
//!   other. Shannon diversity: abundance weighted, internal or not.
//! * Edge lengths are 1, except evolutionary distinctiveness, which uses
//!   origination-update differences and the time of the call.
//! * Variances are the sample variance (divisor n - 1), as `emp::Variance`;
//!   NaN for fewer than two values.

use std::f64::consts::E;

/// One node of a snapshot.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PhyloNode {
    /// Taxon id (any unique integer; children are visited in ascending id
    /// order wherever floating-point summation order matters).
    pub id: i32,
    /// Index of the parent in the snapshot; `None` for a root.
    pub parent: Option<usize>,
    /// Living organisms of the taxon (0 for an extinct ancestor).
    pub abundance: i32,
    /// Origination update.
    pub origin: i32,
    /// The tracker's depth of the taxon (reported for the MRCA only).
    pub depth: i32,
    /// Update at which the taxon went extinct, -1 when unknown or living.
    pub destroyed: i32,
}

/// The metrics of one snapshot. Fields that need an MRCA are NaN / 0 when
/// there is none (`mrca` is `None`).
#[derive(Clone, Debug)]
pub struct PhyloMetrics {
    /// Living taxa.
    pub taxa: usize,
    /// Nodes of the whole pruned tree, pre-MRCA ancestors included.
    pub pruned_nodes: usize,
    /// Nodes from the MRCA down (the MRCA included).
    pub nodes: usize,
    /// Snapshot index of the MRCA.
    pub mrca: Option<usize>,
    /// Phylogenetic diversity: edges of the tree from the MRCA down.
    pub pd: usize,
    /// Unordered pairs of living taxa.
    pub pair_count: u64,
    /// Sum of pairwise distances (edges) over those pairs.
    pub pair_sum: u128,
    /// Sum of squared pairwise distances.
    pub pair_sumsq: u128,
    pub pair_mean: f64,
    pub pair_var: f64,
    /// Evolutionary distinctiveness over living taxa: sum, mean, variance.
    pub ed_sum: f64,
    pub ed_mean: f64,
    pub ed_var: f64,
    /// Sackin index.
    pub sackin: u64,
    /// Colless-like index as Empirical computes it.
    pub colless: f64,
    /// Shannon diversity of living taxa, abundance weighted, in bits.
    pub shannon: f64,
    /// Mean depth of living taxa below the MRCA (edges).
    pub mean_depth: f64,
    /// Snapshot index of the most abundant taxon (lowest id on ties).
    pub top: Option<usize>,
    /// Edges from the root of the snapshot to `top` along the parent chain.
    pub lineage_length: usize,
}

/// The snapshot as adjacency arrays. `order` is a breadth-first order from
/// the roots (parents before children), used while building.
struct Tree {
    /// Kept (living or with living descendants) children, ascending id.
    kc_start: Vec<usize>,
    kc: Vec<usize>,
    kept: Vec<bool>,
    /// Living nodes in the node's subtree, itself included.
    live_sub: Vec<u64>,
}

impl Tree {
    fn build(nodes: &[PhyloNode]) -> Tree {
        let n = nodes.len();
        let mut kids: Vec<usize> = (0..n).filter(|&i| nodes[i].parent.is_some()).collect();
        kids.sort_by_key(|&i| (nodes[i].parent, nodes[i].id));
        let mut start = vec![0usize; n + 1];
        for &i in &kids {
            if let Some(p) = nodes[i].parent {
                start[p + 1] += 1;
            }
        }
        for i in 0..n {
            start[i + 1] += start[i];
        }
        // `kids` is sorted by parent, so it is already the CSR payload.
        let mut roots: Vec<usize> = (0..n).filter(|&i| nodes[i].parent.is_none()).collect();
        roots.sort_by_key(|&i| nodes[i].id);
        let mut order = roots;
        let mut head = 0;
        while head < order.len() {
            let x = order[head];
            head += 1;
            order.extend_from_slice(&kids[start[x]..start[x + 1]]);
        }
        let mut live_sub = vec![0u64; n];
        for &x in order.iter().rev() {
            if nodes[x].abundance > 0 {
                live_sub[x] += 1;
            }
            if let Some(p) = nodes[x].parent {
                live_sub[p] += live_sub[x];
            }
        }
        let kept: Vec<bool> = live_sub.iter().map(|&l| l > 0).collect();
        let mut kc_start = vec![0usize; n + 1];
        let mut kc = Vec::new();
        for x in 0..n {
            if kept[x] {
                kc.extend(
                    kids[start[x]..start[x + 1]]
                        .iter()
                        .copied()
                        .filter(|&c| kept[c]),
                );
            }
            kc_start[x + 1] = kc.len();
        }
        Tree {
            kc_start,
            kc,
            kept,
            live_sub,
        }
    }

    fn children(&self, x: usize) -> &[usize] {
        &self.kc[self.kc_start[x]..self.kc_start[x + 1]]
    }

    fn numoff(&self, x: usize) -> usize {
        self.kc_start[x + 1] - self.kc_start[x]
    }
}

/// Which snapshot nodes belong to the pruned tree (living, or with a living
/// descendant).
pub fn pruned(nodes: &[PhyloNode]) -> Vec<bool> {
    Tree::build(nodes).kept
}

/// The sample variance of `v` (two-pass, as `emp::Variance`); NaN below two.
fn sample_variance(v: &[f64]) -> f64 {
    if v.len() < 2 {
        return f64::NAN;
    }
    let mean = v.iter().sum::<f64>() / v.len() as f64;
    let ss: f64 = v.iter().map(|x| (mean - x) * (mean - x)).sum();
    ss / (v.len() - 1) as f64
}

/// Compute every metric of the snapshot. `now` is the time evolutionary
/// distinctiveness measures to (the current update).
pub fn compute(nodes: &[PhyloNode], now: i32) -> PhyloMetrics {
    let n = nodes.len();
    let t = Tree::build(nodes);
    let alive = |x: usize| nodes[x].abundance > 0;

    let mut living: Vec<usize> = (0..n).filter(|&x| alive(x)).collect();
    living.sort_by_key(|&x| nodes[x].id);
    let taxa = living.len();
    let pruned_nodes = t.kept.iter().filter(|&&k| k).count();

    // Shannon diversity (CalcDiversity: -sum p log2 p over active taxa).
    let total: f64 = living.iter().map(|&x| nodes[x].abundance as f64).sum();
    let mut shannon = 0.0;
    for &x in &living {
        let p = nodes[x].abundance as f64 / total;
        shannon -= p * p.log2();
    }
    if taxa == 0 {
        shannon = 0.0;
    }

    // Most abundant taxon and its lineage length.
    let top = living
        .iter()
        .copied()
        .max_by_key(|&x| (nodes[x].abundance, std::cmp::Reverse(nodes[x].id)));
    let mut lineage_length = 0;
    if let Some(mut x) = top {
        while let Some(p) = nodes[x].parent {
            lineage_length += 1;
            x = p;
        }
    }

    let mut m = PhyloMetrics {
        taxa,
        pruned_nodes,
        nodes: 0,
        mrca: None,
        pd: 0,
        pair_count: 0,
        pair_sum: 0,
        pair_sumsq: 0,
        pair_mean: f64::NAN,
        pair_var: f64::NAN,
        ed_sum: f64::NAN,
        ed_mean: f64::NAN,
        ed_var: f64::NAN,
        sackin: 0,
        colless: f64::NAN,
        shannon,
        mean_depth: f64::NAN,
        top,
        lineage_length,
    };

    // The MRCA: from the single root, descend through dead unifurcations.
    let roots: Vec<usize> = (0..n)
        .filter(|&x| nodes[x].parent.is_none() && t.kept[x])
        .collect();
    if roots.len() != 1 || taxa == 0 {
        return m;
    }
    let mut mrca = roots[0];
    while !alive(mrca) && t.numoff(mrca) == 1 {
        mrca = t.children(mrca)[0];
    }
    m.mrca = Some(mrca);

    // Breadth-first order of the tree below the MRCA, with relative depths.
    let mut sub = vec![mrca];
    let mut head = 0;
    while head < sub.len() {
        let x = sub[head];
        head += 1;
        sub.extend_from_slice(t.children(x));
    }
    let mut depth = vec![0u32; n];
    for &x in &sub {
        for &c in t.children(x) {
            depth[c] = depth[x] + 1;
        }
    }
    m.nodes = sub.len();
    m.pd = sub.len() - 1;
    let nl = t.live_sub[mrca];
    debug_assert_eq!(nl as usize, taxa);

    m.mean_depth = living.iter().map(|&x| depth[x] as f64).sum::<f64>() / taxa as f64;

    // Pairwise distances. Sum: each edge to a child c separates live(c) taxa
    // from the other nl - live(c), so it lies on live(c)*(nl - live(c)) paths.
    let mut sum: u128 = 0;
    for &x in sub.iter().skip(1) {
        let l = t.live_sub[x] as u128;
        sum += l * (nl as u128 - l);
    }
    // Sum of squares: a subtree programme carrying (count, sum of depths,
    // sum of squared depths) of the living taxa below each node; pairs are
    // charged at their lowest common ancestor v, where
    // d = Du + Dw - 2 Dv and d^2 expands over the two groups being merged.
    let mut grp: Vec<(i128, i128, i128)> = vec![(0, 0, 0); n];
    let mut sumsq: i128 = 0;
    for &v in sub.iter().rev() {
        let dv = depth[v] as i128;
        let (mut c, mut s1, mut s2) = if alive(v) {
            (1i128, dv, dv * dv)
        } else {
            (0, 0, 0)
        };
        for &ch in t.children(v) {
            let (cb, s1b, s2b) = grp[ch];
            let cnt = c * cb;
            let sum_u = cb * s1 + c * s1b;
            let sumsq_u = cb * s2 + c * s2b + 2 * s1 * s1b;
            sumsq += sumsq_u - 4 * dv * sum_u + 4 * dv * dv * cnt;
            c += cb;
            s1 += s1b;
            s2 += s2b;
        }
        grp[v] = (c, s1, s2);
    }
    let pairs = (nl as u128) * (nl as u128 - 1) / 2;
    m.pair_count = pairs as u64;
    m.pair_sum = sum;
    m.pair_sumsq = sumsq as u128;
    if pairs >= 1 {
        m.pair_mean = sum as f64 / pairs as f64;
    }
    if pairs >= 2 {
        // (sumsq - sum^2 / N) / (N - 1), kept exact until the last division.
        let num = pairs as i128 * sumsq - (sum as i128) * (sum as i128);
        m.pair_var = num as f64 / (pairs as f64 * (pairs - 1) as f64);
    }

    // Evolutionary distinctiveness (GetEvolutionaryDistinctiveness). The walk
    // from a taxon splits the path at every ancestor that is living or
    // branching; the segment above a split point s has divisor
    // live_sub(s) (living descendants + itself for a living s, living
    // descendants for a dead branch point) and a length that telescopes to
    // origin(s) - origin(next split or MRCA). The first segment runs from
    // `now`, not from the taxon's own origination. g[s] is the sum over the
    // segments above s.
    let mut up = vec![usize::MAX; n];
    let mut g = vec![0.0f64; n];
    let split = |x: usize| alive(x) || t.numoff(x) > 1;
    for &x in &sub {
        for &c in t.children(x) {
            up[c] = if x == mrca || split(x) { x } else { up[x] };
        }
        if x != mrca && split(x) {
            let p = up[x];
            g[x] = (nodes[x].origin as f64 - nodes[p].origin as f64) / t.live_sub[x] as f64 + g[p];
        }
    }
    let mut ed = Vec::with_capacity(taxa);
    for &x in &sub {
        if !alive(x) {
            continue;
        }
        if x == mrca {
            ed.push(0.0);
        } else {
            let p = up[x];
            ed.push((now as f64 - nodes[p].origin as f64) / t.live_sub[x] as f64 + g[p]);
        }
    }
    m.ed_sum = ed.iter().sum();
    m.ed_mean = m.ed_sum / taxa as f64;
    m.ed_var = sample_variance(&ed);

    // Sackin (SackinIndex, GetBranchesToRoot): per living taxon, the
    // ancestors strictly between it and the MRCA with two or more children,
    // plus one. For the MRCA itself the reference walks past the MRCA to
    // the root and counts branch points above it; pre-MRCA taxa are
    // excluded here, so it contributes 1.
    let mut br = vec![0u64; n];
    for &x in &sub {
        for &c in t.children(x) {
            br[c] = br[x] + u64::from(x != mrca && t.numoff(x) > 1);
        }
    }
    m.sackin = living.iter().map(|&x| br[x] + 1).sum();

    // Colless-like index (RecursiveCollessStep). res[x] = (sum of the
    // node's n_values, total) of the step started at x, which first skips
    // down unifurcations.
    let mut res: Vec<(f64, f64)> = vec![(0.0, 0.0); n];
    let mut nvals: Vec<f64> = Vec::new();
    for &x in sub.iter().rev() {
        let k = t.numoff(x);
        if k == 0 {
            res[x] = (0.0, 0.0);
        } else if k == 1 {
            res[x] = res[t.children(x)[0]];
        } else {
            nvals.clear();
            let mut total = 0.0;
            for &c in t.children(x) {
                let (s, tot) = res[c];
                nvals.push(s + (t.numoff(c) as f64 + E).ln());
                total += tot;
            }
            let mut sorted = nvals.clone();
            sorted.sort_by(|a, b| a.total_cmp(b));
            let med = if sorted.len() % 2 == 1 {
                sorted[sorted.len() / 2]
            } else {
                (sorted[sorted.len() / 2 - 1] + sorted[sorted.len() / 2]) / 2.0
            };
            let mut sum_diffs = 0.0;
            let mut s = 0.0;
            for &v in &nvals {
                sum_diffs += (v - med).abs();
                s += v;
            }
            total += sum_diffs / nvals.len() as f64;
            res[x] = (s, total);
        }
    }
    m.colless = res[mrca].1;
    m
}

/// The pruned tree (living nodes and their ancestors, pre-MRCA ancestors
/// included) in the ALife standard phylogeny format: a CSV with the columns
/// `id`, `ancestor_list`, `origin_time`, `destruction_time`, `num_orgs`,
/// `depth`. A root has ancestor list `[none]`; a taxon that is still living
/// has destruction time `inf`, an extinct one with an unknown time leaves it
/// empty. Rows are in ascending id order, so the file does not depend on
/// the order of the snapshot.
pub fn alife_csv(nodes: &[PhyloNode]) -> String {
    let t = Tree::build(nodes);
    let mut idx: Vec<usize> = (0..nodes.len()).filter(|&x| t.kept[x]).collect();
    idx.sort_by_key(|&x| nodes[x].id);
    let mut out = String::from("id,ancestor_list,origin_time,destruction_time,num_orgs,depth\n");
    for x in idx {
        let nd = &nodes[x];
        let anc = match nd.parent {
            Some(p) => format!("[{}]", nodes[p].id),
            None => "[none]".to_string(),
        };
        let dest = if nd.abundance > 0 {
            "inf".to_string()
        } else if nd.destroyed >= 0 {
            nd.destroyed.to_string()
        } else {
            String::new()
        };
        out.push_str(&format!(
            "{},{},{},{},{},{}\n",
            nd.id, anc, nd.origin, dest, nd.abundance, nd.depth
        ));
    }
    out
}
