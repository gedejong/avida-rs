//! Phylogenetic tree statistics of a batch: `PRINT_TREE_STATS` and
//! `PRINT_CUMULATIVE_STEMMINESS` (`cAnalyzeTreeStats_CumulativeStemminess`)
//! and `PRINT_GAMMA` (`cAnalyzeTreeStats_Gamma`), analyze/ at 47f13dadb.

use std::collections::HashMap;
use std::io::Write;

use avida_apto::fmt_double;
use avida_config::string_util as su;

use crate::analyze::Analyze;
use crate::error::{AnalyzeError, Result};
use crate::genotype::AnalyzeGenotype;
use crate::host::AnalyzeHost;
use crate::verbose::{VERBOSE_DETAILS, VERBOSE_ON};

/// `cAGLData`, with its constructor's defaults.
#[derive(Debug, Clone)]
struct AglData {
    id: i32,
    parent_pos: i32,
    offspring_count: i32,
    anc_branch_dist: i32,
    anc_branch_id: i32,
    anc_branch_pos: i32,
    off_branch_dist_acc: i32,
    cumulative_stemminess: f64,
    traversal_visited: bool,
    offspring_positions: Vec<usize>,
}

impl Default for AglData {
    fn default() -> Self {
        AglData {
            id: -1,
            parent_pos: -1,
            offspring_count: -1,
            anc_branch_dist: -1,
            anc_branch_id: -1,
            anc_branch_pos: -1,
            off_branch_dist_acc: -1,
            cumulative_stemminess: -1.0,
            traversal_visited: false,
            offspring_positions: Vec::new(),
        }
    }
}

/// `cAnalyzeTreeStats_CumulativeStemminess::AnalyzeBatchTree`: the average
/// cumulative stemminess of the inner nodes of the n-furcating subtree
/// (0 when the analysis stops early). Progress goes to `out`, errors (at
/// `VERBOSE_ON`) to `err`.
fn average_stemminess(
    genotypes: &[AnalyzeGenotype],
    verbose: bool,
    out: &mut dyn Write,
    err: &mut dyn Write,
) -> Result<f64> {
    let num_gens = genotypes.len();
    let say = |out: &mut dyn Write, msg: &str| -> Result<()> {
        if verbose {
            writeln!(out, "{msg}")?;
        }
        Ok(())
    };
    say(out, &format!("Number of genotypes: {num_gens}"))?;
    say(out, "Scanning genotypes...")?;
    // `Apto::Map::Set`: a repeated id maps to its last position.
    let mut id_hash: HashMap<i32, usize> = HashMap::new();
    for (pos, g) in genotypes.iter().enumerate() {
        id_hash.insert(g.id_num, pos);
    }
    let mut agl: Vec<AglData> = genotypes
        .iter()
        .map(|g| AglData {
            id: g.id_num,
            ..AglData::default()
        })
        .collect();

    say(out, "Assembling tree...")?;
    for pos in 0..num_gens {
        let parent_id = genotypes[pos].parent_id;
        if parent_id != -1 {
            match id_hash.get(&parent_id) {
                Some(&pp) => {
                    agl[pos].parent_pos = pp as i32;
                    agl[pp].offspring_positions.push(pos);
                }
                None => {
                    if verbose {
                        writeln!(
                            err,
                            "Error: the parent of a non-root tree node is missing - "
                        )?;
                    }
                    return Ok(0.0);
                }
            }
        }
    }
    say(out, "Loading offspring counts...")?;
    for a in agl.iter_mut() {
        a.offspring_count = a.offspring_positions.len() as i32;
    }

    say(out, "Finding branch points...")?;
    loop {
        let mut found = false;
        let mut progressed = false;
        for pos in 0..num_gens {
            if agl[pos].anc_branch_dist > -1 {
                continue;
            }
            found = true;
            let parent_pos = agl[pos].parent_pos;
            if parent_pos == -1 {
                agl[pos].anc_branch_dist = 0;
                progressed = true;
            } else {
                let p = &agl[parent_pos as usize];
                let (p_count, p_dist, p_id, p_bid, p_bpos) = (
                    p.offspring_count,
                    p.anc_branch_dist,
                    p.id,
                    p.anc_branch_id,
                    p.anc_branch_pos,
                );
                if p_count > 1 {
                    agl[pos].anc_branch_dist = 1;
                    agl[pos].anc_branch_id = p_id;
                    agl[pos].anc_branch_pos = parent_pos;
                    progressed = true;
                } else if p_dist > -1 {
                    agl[pos].anc_branch_dist = p_dist + 1;
                    agl[pos].anc_branch_id = p_bid;
                    agl[pos].anc_branch_pos = p_bpos;
                    progressed = true;
                }
            }
        }
        if !found {
            break;
        }
        if !progressed {
            return Err(AnalyzeError::fatal(
                "tree stats: the parent links form a cycle (the C++ loops forever)",
            ));
        }
    }

    say(out, "Building n-furcating subtree...")?;
    let branch_tree_size = agl.iter().filter(|a| a.offspring_count > 1).count();
    say(
        out,
        &format!("Number of n-furcating nodes: {branch_tree_size}"),
    )?;
    if branch_tree_size == 0 {
        if verbose {
            writeln!(err, "Error: no branches found in tree - ")?;
        }
        return Ok(0.0);
    }
    let mut agl2: Vec<AglData> = Vec::with_capacity(branch_tree_size);
    let mut id_hash_2: HashMap<i32, usize> = HashMap::new();
    for a in agl.iter().filter(|a| a.offspring_count > 1) {
        id_hash_2.insert(a.id, agl2.len());
        agl2.push(AglData {
            id: a.id,
            anc_branch_dist: a.anc_branch_dist,
            anc_branch_id: a.anc_branch_id,
            ..AglData::default()
        });
    }
    for pos in 0..branch_tree_size {
        // `Apto::Map::Get(key, value)` leaves the value alone when missing.
        if let Some(&p) = id_hash_2.get(&agl2[pos].anc_branch_id) {
            agl2[pos].anc_branch_pos = p as i32;
        }
        let abp = agl2[pos].anc_branch_pos;
        if abp >= 0 {
            agl2[abp as usize].offspring_positions.push(pos);
        }
    }

    say(out, "Finding root of n-furcating subtree...")?;
    let mut root = None;
    for (pos, a) in agl2.iter_mut().enumerate() {
        a.traversal_visited = false;
        if a.anc_branch_dist == 0 || a.anc_branch_id == -1 || a.anc_branch_pos == -1 {
            root = Some(pos);
            if !(a.anc_branch_id == -1 && a.anc_branch_pos == -1) {
                if verbose {
                    writeln!(
                        err,
                        "Error: while looking for root of subtree, found inconsistencies - "
                    )?;
                    writeln!(err, " root ancestor-branch-distance: {}", a.anc_branch_dist)?;
                    writeln!(err, " root ancestor-branch-id: {}", a.anc_branch_id)?;
                    // QUIRK: the "pos" line prints the branch id again.
                    writeln!(err, " root ancestor-branch-pos: {}", a.anc_branch_id)?;
                }
                return Ok(0.0);
            }
        }
    }

    say(out, "Accumulating branch distances...")?;
    let Some(root) = root else {
        if verbose {
            writeln!(err, "Error: couldn't find root of subtree - ")?;
        }
        return Ok(0.0);
    };
    // `tList` Push / Pop work at the front: a stack.
    let mut dfs_stack = vec![root];
    while let Some(node) = dfs_stack.pop() {
        if !agl2[node].traversal_visited {
            dfs_stack.push(node);
            agl2[node].off_branch_dist_acc = 0;
            for i in 0..agl2[node].offspring_positions.len() {
                let pos = agl2[node].offspring_positions[i];
                if !agl2[pos].traversal_visited {
                    dfs_stack.push(pos);
                }
            }
            agl2[node].traversal_visited = true;
        } else {
            let abp = agl2[node].anc_branch_pos;
            if abp >= 0 {
                let (dist, acc) = (agl2[node].anc_branch_dist, agl2[node].off_branch_dist_acc);
                let p = &mut agl2[abp as usize];
                p.off_branch_dist_acc = p.off_branch_dist_acc.wrapping_add(dist);
                p.off_branch_dist_acc = p.off_branch_dist_acc.wrapping_add(acc);
            }
        }
    }

    say(out, "Computing cumulative stemminesses...")?;
    for a in agl2.iter_mut() {
        a.cumulative_stemminess = if a.anc_branch_dist == 0 {
            0.0
        } else {
            f64::from(a.anc_branch_dist)
                / (f64::from(a.off_branch_dist_acc) + f64::from(a.anc_branch_dist))
        };
    }

    say(out, "Computing average cumulative stemminess...")?;
    let mut sum = 0.0f64;
    let mut inner_nodes = 0i32;
    if branch_tree_size > 1 {
        for a in &agl2 {
            let not_leaf = a.off_branch_dist_acc > 0;
            let not_root = a.anc_branch_id > 0;
            if not_leaf && not_root {
                sum += a.cumulative_stemminess;
                inner_nodes += 1;
            }
        }
    }
    Ok(if inner_nodes > 0 {
        sum / f64::from(inner_nodes)
    } else {
        0.0
    })
}

impl Analyze {
    /// `CommandPrintTreeStats` (`tree_stats`) and
    /// `CommandPrintCumulativeStemminess` (`stemminess`): the average
    /// cumulative stemminess of the batch's tree.
    pub(crate) fn command_print_stemminess(
        &mut self,
        host: &mut dyn AnalyzeHost,
        mut cur_string: String,
        tree_stats: bool,
    ) -> Result<()> {
        let verbose = Self::verbosity(host) >= VERBOSE_ON;
        let (on, off, default) = if tree_stats {
            (
                "Printing tree stats for batch ",
                "Printing tree stats...",
                "tree_stats.dat",
            )
        } else {
            (
                "Printing cumulative stemmines for batch ",
                "Printing cumulative stemmines...",
                "cumulative_stemminess.dat",
            )
        };
        if verbose {
            writeln!(self.out, "{on}{}", self.cur_batch)?;
        } else {
            writeln!(self.out, "{off}")?;
        }
        let mut filename = default.to_string();
        if !cur_string.is_empty() {
            filename = su::pop_word(&mut cur_string);
        }
        let bi = self.batch_index(self.cur_batch)?;
        static_raw(
            host,
            &filename,
            "# Legend:\n# 1: Average cumulative stemminess\n\n",
        )?;
        let avg = average_stemminess(
            &self.batches[bi].list,
            verbose,
            &mut self.out,
            &mut self.err,
        )?;
        static_raw(host, &filename, &format!("{}\n", fmt_double(avg)))
    }

    /// `CommandPrintGamma end_time [file [lineage_file]]`: the Pybus-Harvey
    /// gamma statistic of the batch's tree, with furcations timed at the
    /// parent's birth.
    ///
    /// Parents are linked as `EstablishLinks` does: a genotype whose
    /// parent id is not in the batch is linked to the previous genotype's
    /// parent (the C++ keeps the stale index).
    pub(crate) fn command_print_gamma(
        &mut self,
        host: &mut dyn AnalyzeHost,
        mut cur_string: String,
    ) -> Result<()> {
        let verbosity = Self::verbosity(host);
        if verbosity >= VERBOSE_ON {
            writeln!(
                self.out,
                "Printing Pybus-Harvey gamma statistic for batch {}",
                self.cur_batch
            )?;
        } else {
            writeln!(self.out, "Printing Pybus-Harvey gamma statistic...")?;
        }
        let end_time = if cur_string.is_empty() {
            -1
        } else {
            su::as_int(&su::pop_word(&mut cur_string))
        };
        if end_time < 0 {
            writeln!(
                self.out,
                "Error: end_time (argument 1) must be specified as nonzero."
            )?;
            return Ok(());
        }
        let mut filename = "gamma.dat".to_string();
        if !cur_string.is_empty() {
            filename = su::pop_word(&mut cur_string);
        }
        let mut ltt_filename = String::new();
        if !cur_string.is_empty() {
            ltt_filename = su::pop_word(&mut cur_string);
        }
        static_raw(
            host,
            &filename,
            "# Legend:\n# 1: Pybus-Harvey gamma statistic\n\n",
        )?;
        let bi = self.batch_index(self.cur_batch)?;
        let list = &self.batches[bi].list;
        let details = verbosity >= VERBOSE_DETAILS;

        // EstablishLinks: children in batch order.
        let mut mapping: HashMap<i32, usize> = HashMap::new();
        for (i, g) in list.iter().enumerate() {
            mapping.insert(g.id_num, i);
        }
        let mut children: Vec<Vec<usize>> = vec![Vec::new(); list.len()];
        let mut parent_index: i32 = -1;
        for (i, g) in list.iter().enumerate() {
            if g.parent_id >= 0 {
                if let Some(&p) = mapping.get(&g.parent_id) {
                    parent_index = p as i32;
                }
                if parent_index < 0 {
                    return Err(AnalyzeError::fatal(
                        "PRINT_GAMMA: the first genotype with a parent id has no parent in the batch (C++ indexes lineage[-1])",
                    ));
                }
                children[parent_index as usize].push(i);
            }
        }
        // FindFurcations / FindFurcationTimes (parent's birth).
        let mut times: Vec<i32> = Vec::new();
        let mut furcation_parents: Vec<usize> = Vec::new();
        for (i, kids) in children.iter().enumerate() {
            for j in 1..kids.len() {
                if details {
                    writeln!(
                        self.out,
                        "Furcation(parent_id={}, elder_child_id={}, younger_child_id={})",
                        list[i].id_num,
                        list[kids[j - 1]].id_num,
                        list[kids[j]].id_num
                    )?;
                }
                furcation_parents.push(i);
            }
        }
        for &p in &furcation_parents {
            let t = list[p].update_born;
            if details {
                writeln!(
                    self.out,
                    "FurcationTime(parent_id={}) = {t}",
                    list[p].id_num
                )?;
            }
            times.push(t);
        }
        times.sort_unstable();
        if details {
            for (i, t) in times.iter().enumerate() {
                writeln!(
                    self.out,
                    "SortedFurcationTime(num_lineages={}) = {t}",
                    i + 2
                )?;
            }
        }
        let Some(&last) = times.last() else {
            return Err(AnalyzeError::fatal(
                "PRINT_GAMMA of a tree without furcations (C++ indexes the furcation times at -1)",
            ));
        };
        if end_time < last && verbosity >= VERBOSE_ON {
            writeln!(
                self.err,
                "Error: Lineage end time comes before last furcation."
            )?;
        }
        let size = times.len();
        let mut dists = vec![0i32; size];
        for i in 0..size - 1 {
            dists[i] = times[i + 1].wrapping_sub(times[i]);
        }
        dists[size - 1] = end_time.wrapping_sub(last);
        if details {
            for (i, d) in dists.iter().enumerate() {
                writeln!(
                    self.out,
                    "g_{} = InternodeDistance(num_lineages={}) = {d}",
                    i + 2,
                    i + 2
                )?;
            }
        }

        // CalculateGamma.
        let n = size as i32 + 1;
        if n <= 2 {
            if verbosity >= VERBOSE_ON {
                writeln!(
                    self.err,
                    "Error: not enough genotypes in batch to calculate gamma - "
                )?;
            }
            return Err(AnalyzeError::fatal(
                "PRINT_GAMMA with a single furcation: the C++ prints its uninitialised m_gamma",
            ));
        }
        let g = |j: i32| dists[(j - 2) as usize];
        // `int * int` converted to `unsigned long long` (modulo 2^64).
        let kg = |k: i32| u64::from_ne_bytes(i64::from(k.wrapping_mul(g(k))).to_ne_bytes());
        let mut t_sum: u64 = 0;
        for j in 2..=n {
            t_sum = t_sum.wrapping_add(kg(j));
        }
        let mut si = vec![0u64; n as usize];
        for k in 2..n {
            si[k as usize] = kg(k).wrapping_add(si[(k - 1) as usize]);
        }
        let mut so: u64 = 0;
        for i in 2..n {
            so = so.wrapping_add(si[i as usize]);
        }
        let nf = f64::from(n);
        let (so_f, t_f) = (so as f64, t_sum as f64);
        // `(1./(n-2.)) * so - T/2.` is fused on the reference build.
        let numerator = (1.0 / (nf - 2.0)).mul_add(so_f, -(t_f / 2.0));
        let denominator = t_f * (1.0 / (12.0 * (nf - 2.0))).sqrt();
        let gamma = numerator / denominator;
        if details {
            for i in 2..=n {
                writeln!(self.out, "{i}*g_{i} = {}", i.wrapping_mul(g(i)))?;
            }
            for i in 2..n {
                writeln!(self.out, "sum(k=2..{i}, k*g_k) = {}", si[i as usize])?;
            }
            writeln!(self.out, "double_sum = {so}")?;
            writeln!(self.out, "T = {t_sum}")?;
            writeln!(self.out, "gamma_numerator = {}", fmt_double(numerator))?;
            writeln!(self.out, "gamma_denominator = {}", fmt_double(denominator))?;
            writeln!(self.out, "gamma = {}", fmt_double(gamma))?;
        }
        static_raw(host, &filename, &format!("{}\n", fmt_double(gamma)))?;
        if !ltt_filename.is_empty() {
            let mut text = String::from("# Legend:\n# 1: num_lineages\n# 2: furcation_time\n\n");
            for (i, t) in times.iter().enumerate() {
                text.push_str(&format!("{} {t}\n", i + 2));
            }
            static_raw(host, &ltt_filename, &text)?;
        }
        Ok(())
    }
}

/// Append raw text to the static file at `path`.
fn static_raw(host: &mut dyn AnalyzeHost, path: &str, text: &str) -> Result<()> {
    let df = crate::cmd_population::static_file(host, path)?;
    df.ofstream().write_all(text.as_bytes())?;
    df.flush();
    Ok(())
}
