//! Phylogeny measurements: `PrintPhylogenyMetrics` and `SavePhylogeny`.
//!
//! Extensions beyond the C++ program (neither exists in avida-core). They
//! only read the genotype tracker: no random draw, no write to the
//! population or the systematics, so every other output of a run is
//! unchanged by them. The metric definitions, their conventions and the
//! treatment of living internal nodes are in
//! `avida_systematics::phylometrics`; this module builds the snapshot from
//! the tracker and writes the rows.
//!
//! A snapshot node is a genotype of the `genotype` arbiter (the active size
//! lists and the historic list: every living genotype and every extinct one
//! that still has descendants), with its first parent (`parents()[0]`, as
//! `updateCoalescent` follows), `num_units()` as abundance, `update_born()`
//! as origination time and the tracker's depth. A genotype is living when
//! `num_units() > 0`; one kept alive only by active references (a birth
//! chamber entry) is not.

use std::collections::{BTreeMap, HashMap, VecDeque};

use avida_systematics::phylometrics::{alife_csv, compute, PhyloMetrics, PhyloNode};

use crate::world::World;

/// The registered action names this module parses.
pub const PHYLO_ACTIONS: &[&str] = &["PrintPhylogenyMetrics", "SavePhylogeny"];

/// A parsed action of this module.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PhyloAction {
    /// `PrintPhylogenyMetrics [string fname="phylogeny.dat"]`
    Metrics { filename: String },
    /// `SavePhylogeny [string fname="phylogeny_tree.csv"]`
    Save { filename: String },
}

/// The columns of a `PrintPhylogenyMetrics` row that a front end charts.
/// Values that the file writes as `nan` are `nan` here.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PhyloRow {
    /// Update of the row.
    pub update: i32,
    /// Living genotypes.
    pub living: i32,
    /// Phylogenetic diversity (edges below the MRCA).
    pub pd: i32,
    /// Mean pairwise distance between living genotypes.
    pub pair_mean: f64,
    /// Mean evolutionary distinctiveness.
    pub ed_mean: f64,
    /// Shannon diversity (bits).
    pub shannon: f64,
    /// Tracker depth of the MRCA (-1: none).
    pub mrca_depth: i32,
    /// MRCA changes so far.
    pub mrca_changes: i32,
}

/// What a `PrintPhylogenyMetrics` file remembers between rows.
#[derive(Debug, Default, Clone)]
struct MrcaTrack {
    previous: Option<i32>,
    changes: u64,
    /// The last [`crate::modes::HISTORY_CAP`] rows written, for display.
    history: VecDeque<PhyloRow>,
}

/// State kept between calls, per metrics file.
#[derive(Debug, Default)]
pub struct PhyloState {
    files: BTreeMap<String, MrcaTrack>,
}

impl PhyloState {
    /// The rows each metrics file has written so far (the last
    /// [`crate::modes::HISTORY_CAP`]), oldest first, ordered by file name.
    /// Reads only.
    pub fn history(&self) -> Vec<(String, Vec<PhyloRow>)> {
        self.files
            .iter()
            .map(|(name, t)| (name.clone(), t.history.iter().copied().collect()))
            .collect()
    }
}

/// The constructor of the action `name` applied to `args`; `None` for any
/// other name.
pub fn parse(name: &str, args: &str) -> Option<PhyloAction> {
    let word = args.split_whitespace().next();
    match name {
        "PrintPhylogenyMetrics" => Some(PhyloAction::Metrics {
            filename: word.unwrap_or("phylogeny.dat").to_string(),
        }),
        "SavePhylogeny" => Some(PhyloAction::Save {
            filename: word.unwrap_or("phylogeny_tree.csv").to_string(),
        }),
        _ => None,
    }
}

const HEADER: &[&str] = &[
    "Phylogeny metrics of the genotype tree (extension, not in the C++ program).",
    "Sources: Dolson et al. 2020, Interpreting the Tape of Life (Artif Life 26(1)); the Empirical library's",
    "emp::Systematics (GetMRCA, GetPairwiseDistances, GetEvolutionaryDistinctiveness, SackinIndex, CollessLikeIndex).",
    "Taxa are genotypes. The tree is the genotype tracker's tree: living genotypes and their ancestors; a genotype",
    "is living when it has organisms; only the first parent of a genotype is followed.",
    "The tree is rooted at the MRCA of the living genotypes (the topmost living or branching node); pre-MRCA taxa",
    "are excluded. Without a single root or a living genotype, MRCA columns are -1 and the metrics are nan.",
    "Edge length is 1 except for evolutionary distinctiveness, which uses origination-update differences.",
    "nodes = nodes of the tree below and including the MRCA; PD = edges in that tree (nodes - 1), which counts",
    "no pre-MRCA ancestor; the Empirical code returns ancestors + living - 1 over the whole tree.",
    "Pairwise distance: edges on the path, over all unordered pairs of living genotypes (unweighted).",
    "Evolutionary distinctiveness: as GetEvolutionaryDistinctiveness with time = this update (the branch above a",
    "genotype is measured from now); mean, sum and sample variance over living genotypes. Living ancestors restart",
    "the divisor. Variances are sample variances (n - 1) as emp::Variance; nan below two values.",
    "Sackin = sum over living genotypes of (branching nodes strictly between it and the MRCA + 1); the MRCA itself",
    "counts 1 (the Empirical code would walk past it). Colless-like: Empirical RecursiveCollessStep (log(children +",
    "e) weights, median absolute deviation), not Mir et al. 2018; children are summed in ascending id order.",
    "Shannon diversity in bits of the living genotypes weighted by abundance. Mean depth: edges below the MRCA.",
    "mrca_changes: samples at which the MRCA id differs from the previous sample's (the first row counts 0);",
    "several changes between two rows count once. Lineage: edges from the founder to the most abundant genotype",
    "(lowest id on ties). Phenotypic volatility is not reported: the tracker keeps no task phenotype of extinct",
    "ancestors.",
];

impl World {
    /// The tracker's tree as a snapshot, with the handle-to-index map.
    fn phylo_snapshot(&self) -> Result<Vec<PhyloNode>, String> {
        let arb = self
            .systematics
            .genotypes()
            .ok_or("phylogeny actions need the genotype arbiter")?;
        if arb.disable_class() {
            return Err("phylogeny actions need genotype classification \
                 (DISABLE_GENOTYPE_CLASSIFICATION is set)"
                .to_string());
        }
        // Every genotype the arbiter holds: the active size lists (size 0
        // included) and the historic list. `iter_groups` (`Begin()`/`Next()`)
        // stops at size 1 and skips the historic ancestors.
        let mut handles = Vec::new();
        for size in 0..arb.active_list_count() as i32 {
            handles.extend(arb.active_list(size));
        }
        handles.extend(arb.historic());
        let index: HashMap<_, _> = handles.iter().enumerate().map(|(i, &h)| (h, i)).collect();
        Ok(handles
            .iter()
            .filter_map(|&h| arb.genotype(h))
            .map(|g| PhyloNode {
                id: g.id(),
                parent: g.parents().first().and_then(|p| index.get(p).copied()),
                abundance: g.num_units(),
                origin: g.update_born(),
                depth: g.depth(),
                destroyed: g.update_deactivated(),
            })
            .collect())
    }

    pub(crate) fn run_phylo_action(&mut self, action: &PhyloAction) -> Result<(), String> {
        match action {
            PhyloAction::Metrics { filename } => self.phylo_metrics(filename),
            PhyloAction::Save { filename } => self.phylo_save(filename),
        }
    }

    fn phylo_metrics(&mut self, filename: &str) -> Result<(), String> {
        let nodes = self.phylo_snapshot()?;
        let update = self.stats.update();
        let m = compute(&nodes, update);
        let mrca_id = m.mrca.map(|i| nodes[i].id);
        let mrca_depth = m.mrca.map_or(-1, |i| nodes[i].depth);
        let track = self
            .actions2
            .phylo
            .files
            .entry(filename.to_string())
            .or_default();
        if let Some(id) = mrca_id {
            if track.previous.is_some_and(|p| p != id) {
                track.changes += 1;
            }
            track.previous = Some(id);
        }
        let changes = track.changes;
        crate::modes::push_capped(
            &mut track.history,
            PhyloRow {
                update,
                living: m.taxa as i32,
                pd: m.pd as i32,
                pair_mean: m.pair_mean,
                ed_mean: m.ed_mean,
                shannon: m.shannon,
                mrca_depth,
                mrca_changes: changes as i32,
            },
            crate::modes::HISTORY_CAP,
        );
        let top_id = m.top.map_or(-1, |i| nodes[i].id);
        let df = self.out.static_with_path(filename)?;
        for line in HEADER {
            df.write_comment(line);
        }
        write_row(
            df,
            update,
            &m,
            mrca_id.unwrap_or(-1),
            mrca_depth,
            changes,
            top_id,
        );
        df.endl();
        Ok(())
    }

    fn phylo_save(&mut self, filename: &str) -> Result<(), String> {
        let nodes = self.phylo_snapshot()?;
        let csv = alife_csv(&nodes);
        let mut df = self.out.create_with_path(filename)?;
        df.ofstream()
            .write_all(csv.as_bytes())
            .map_err(|e| format!("SavePhylogeny: cannot write '{filename}': {e}"))?;
        df.flush();
        Ok(())
    }
}

fn write_row(
    df: &mut avida_apto::datafile::DataFile,
    update: i32,
    m: &PhyloMetrics,
    mrca_id: i32,
    mrca_depth: i32,
    changes: u64,
    top_id: i32,
) {
    df.write(update, "Update");
    df.write(m.taxa as i32, "living genotypes");
    df.write(
        m.nodes as i32,
        "nodes in the tree below and including the MRCA",
    );
    df.write(mrca_id, "MRCA genotype id (-1: none)");
    df.write(mrca_depth, "MRCA depth (tracker)");
    df.write(changes as i32, "MRCA changes so far");
    df.write(m.pd as i32, "phylogenetic diversity (edges)");
    df.write(m.pair_sum as i64, "pairwise distance: sum");
    df.write(m.pair_mean, "pairwise distance: mean");
    df.write(m.pair_var, "pairwise distance: variance");
    df.write(m.ed_mean, "evolutionary distinctiveness: mean");
    df.write(m.ed_sum, "evolutionary distinctiveness: sum");
    df.write(m.ed_var, "evolutionary distinctiveness: variance");
    df.write(m.sackin as i64, "Sackin index");
    df.write(m.colless, "Colless-like index");
    df.write(m.shannon, "Shannon diversity (bits)");
    df.write(m.mean_depth, "mean depth below the MRCA");
    df.write(
        m.lineage_length as i32,
        "lineage length of the most abundant genotype",
    );
    df.write(top_id, "id of the most abundant genotype");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_takes_the_first_word() {
        assert_eq!(
            parse("PrintPhylogenyMetrics", ""),
            Some(PhyloAction::Metrics {
                filename: "phylogeny.dat".into()
            })
        );
        assert_eq!(
            parse("SavePhylogeny", "t.csv extra"),
            Some(PhyloAction::Save {
                filename: "t.csv".into()
            })
        );
        assert_eq!(parse("PrintDominantData", ""), None);
    }
}
