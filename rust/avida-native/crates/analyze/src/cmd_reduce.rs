//! Reduction, sampling and lineage commands of `cAnalyze`: `FILTER`,
//! `FIND_*`, `SAMPLE_*`, `KEEP_TOP`, `TRUNCATE_LINEAGE`, `ALIGN`, and the
//! `PopGenotype` selector they share.

use std::collections::VecDeque;
use std::io::Write;

use avida_config::string_util as su;

use crate::analyze::Analyze;
use crate::cmd_output::compare_flex_stat;
use crate::data_entry::{flex_compare, get_data_command};
use crate::error::{AnalyzeError, Result};
use crate::flex::FlexVar;
use crate::genotype::{cstring_insert, pop_mutation, AnalyzeGenotype};
use crate::host::AnalyzeHost;
use crate::verbose::VERBOSE_ON;

/// `tListPlus::PopMax(fun)`: the first genotype with the largest value
/// (strict `>` while scanning).
fn pop_max<V: PartialOrd + Copy>(
    list: &mut Vec<AnalyzeGenotype>,
    f: impl Fn(&AnalyzeGenotype) -> V,
) -> Option<AnalyzeGenotype> {
    if list.is_empty() {
        return None;
    }
    let mut best = 0;
    let mut max_val = f(&list[0]);
    for (i, g) in list.iter().enumerate() {
        let v = f(g);
        if v > max_val {
            max_val = v;
            best = i;
        }
    }
    Some(list.remove(best))
}

impl Analyze {
    /// `PopGenotype(gen_desc, batch_id)`: remove and return the genotype
    /// `gen_desc` names (`num_cpus`, `total_cpus`, `merit`, `fitness`,
    /// `length` maximum, a numeric id, or `random`).
    pub(crate) fn pop_genotype(
        &mut self,
        gen_desc: &str,
        batch_id: i32,
    ) -> Result<Option<AnalyzeGenotype>> {
        let batch_id = if batch_id == -1 {
            self.cur_batch
        } else {
            batch_id
        };
        let bi = self.batch_index(batch_id)?;
        let desc = gen_desc.to_ascii_lowercase();
        let list = &mut self.batches[bi].list;
        Ok(match desc.as_str() {
            "num_cpus" => pop_max(list, |g| g.num_cpus),
            "total_cpus" => pop_max(list, |g| g.total_cpus),
            "merit" => pop_max(list, |g| g.merit),
            "fitness" => pop_max(list, |g| g.fitness),
            "length" => pop_max(list, |g| g.length),
            _ if su::is_numeric_at(&desc, 0) => {
                let id = su::as_int(&desc);
                list.iter()
                    .position(|g| g.id_num == id)
                    .map(|p| list.remove(p))
            }
            "random" => {
                let pos = self.random.get_uint(list.len() as u32) as usize;
                if pos < list.len() {
                    Some(list.remove(pos))
                } else {
                    None
                }
            }
            _ => {
                writeln!(self.out, "  Error: unknown type {desc}")?;
                self.out.flush()?;
                return Err(AnalyzeError::Exit(1));
            }
        })
    }

    /// `CommandFilter`: `FILTER stat relation value [batch]`.
    pub(crate) fn command_filter(
        &mut self,
        host: &mut dyn AnalyzeHost,
        mut cur_string: String,
    ) -> Result<()> {
        let num_args = su::count_num_words(&cur_string);
        let stat_name = su::pop_word(&mut cur_string);
        let relation = su::pop_word(&mut cur_string);
        let test_value = su::pop_word(&mut cur_string);
        let stat_command = get_data_command(&stat_name).ok();

        let mut error_found = false;
        if !(3..=4).contains(&num_args) {
            writeln!(self.err, "Error: Incorrect argument count.")?;
            error_found = true;
        }
        if stat_command.is_none() {
            writeln!(self.err, "Error: Unknown stat '{stat_name}'")?;
            error_found = true;
        }
        // rel_ok[0] = less ok; [1] = same ok; [2] = greater ok.
        let rel_ok = match relation.as_str() {
            "==" => [false, true, false],
            "!=" => [true, false, true],
            "<" => [true, false, false],
            ">" => [false, false, true],
            "<=" => [true, true, false],
            ">=" => [false, true, true],
            _ => {
                writeln!(self.err, "Error: Unknown relation '{relation}'")?;
                error_found = true;
                [false; 3]
            }
        };
        if error_found {
            writeln!(
                self.err,
                "Format: FILTER [stat] [relation] [value] [batch=current]"
            )?;
            return Err(self.exit_with("Example: FILTER fitness >= 10.0"));
        }
        let stat_command = stat_command.expect("checked above");

        if Self::verbosity(host) >= VERBOSE_ON {
            writeln!(
                self.out,
                "Filtering batch {} to genotypes where {stat_name} {relation} {test_value}",
                self.cur_batch
            )?;
        }
        let bi = self.batch_index(self.cur_batch)?;
        let test = FlexVar::Str(test_value);
        let old = std::mem::take(&mut self.batches[bi].list);
        let mut kept = Vec::with_capacity(old.len());
        for mut g in old {
            let value = stat_command.value(&mut g, host)?;
            let compare = 1 + compare_flex_stat(&value, &test, flex_compare::MAX);
            if rel_ok[compare as usize] {
                kept.push(g);
            }
        }
        self.batches[bi].list = kept;
        self.batches[bi].clear_flags();
        Ok(())
    }

    /// `FindGenotype`: keep only the named genotypes, in the order named.
    pub(crate) fn find_genotype(
        &mut self,
        host: &mut dyn AnalyzeHost,
        mut cur_string: String,
    ) -> Result<()> {
        if cur_string.is_empty() {
            cur_string = "num_cpus".to_string();
        }
        let verbose = Self::verbosity(host) >= VERBOSE_ON;
        if verbose {
            write!(self.out, "Reducing batch {} to genotypes: ", self.cur_batch)?;
        }
        let mut found = Vec::new();
        while su::count_num_words(&cur_string) > 0 {
            let gen_desc = su::pop_word(&mut cur_string);
            if verbose {
                write!(self.out, "{gen_desc} ")?;
            }
            match self.pop_genotype(&gen_desc, self.cur_batch)? {
                Some(g) => found.push(g),
                None => writeln!(self.err, "  Warning: genotype not found!")?,
            }
        }
        writeln!(self.out)?;
        let b = self.cur()?;
        b.list = found;
        b.clear_flags();
        Ok(())
    }

    /// `FindOrganism`: sample organisms (`random` without, `randomWR` with
    /// replacement) and keep the genotypes that were hit.
    pub(crate) fn find_organism(
        &mut self,
        host: &mut dyn AnalyzeHost,
        mut cur_string: String,
    ) -> Result<()> {
        if cur_string.is_empty() {
            writeln!(
                self.err,
                "Error: At least one argument is required in FIND_ORGANISM."
            )?;
            writeln!(self.err, " (perhaps you want FIND_GENOTYPE?)")?;
            return Ok(());
        }
        let verbose = Self::verbosity(host) >= VERBOSE_ON;
        if verbose {
            writeln!(self.out, "Reducing batch {} to organisms: ", self.cur_batch)?;
        }
        let bi = self.batch_index(self.cur_batch)?;
        let mut new_counts = vec![0i32; self.batches[bi].list.len()];

        while su::count_num_words(&cur_string) > 0 {
            let org_desc = su::pop_word(&mut cur_string);
            if verbose {
                write!(self.out, "{org_desc} ")?;
            }
            let with_replacement = match org_desc.as_str() {
                "random" => false,
                "randomWR" => true,
                _ => continue,
            };
            loop {
                let list = &self.batches[bi].list;
                let num_orgs = list.iter().fold(0i32, |a, g| a.wrapping_add(g.num_cpus));
                if !find_summed_value_can_succeed(list, num_orgs) {
                    return Err(AnalyzeError::fatal(
                        "FIND_ORGANISM: no viable organism can ever be drawn; the C++ loops forever",
                    ));
                }
                let org_chosen = self.random.get_uint(num_orgs as u32) as i32;
                let pos = find_summed_value(list, org_chosen).ok_or_else(|| {
                    AnalyzeError::fatal(
                        "FIND_ORGANISM: FindSummedValue ran off the list (C++ NULL dereference)",
                    )
                })?;
                let g = &mut self.batches[bi].list[pos];
                if g.num_cpus != 0 && g.viable {
                    if !with_replacement {
                        g.num_cpus -= 1;
                    }
                    new_counts[pos] += 1;
                    writeln!(self.out, "Found genotype {pos}")?;
                    break;
                }
            }
        }

        let old = std::mem::take(&mut self.batches[bi].list);
        let mut kept = Vec::new();
        for (pos, mut g) in old.into_iter().enumerate() {
            g.num_cpus = new_counts[pos];
            if g.num_cpus == 0 {
                continue;
            }
            writeln!(
                self.out,
                "Genotype {pos} has {} organisms.",
                new_counts[pos]
            )?;
            kept.push(g);
        }
        self.batches[bi].list = kept;
        self.batches[bi].clear_flags();
        Ok(())
    }

    /// `FindLineage`: keep the ancestry of one genotype, oldest first.
    pub(crate) fn find_lineage(
        &mut self,
        host: &mut dyn AnalyzeHost,
        mut cur_string: String,
    ) -> Result<()> {
        let mut lin_type = "num_cpus".to_string();
        if su::count_num_words(&cur_string) > 0 {
            lin_type = su::pop_word(&mut cur_string);
        }
        if Self::verbosity(host) >= VERBOSE_ON {
            writeln!(
                self.out,
                "Reducing batch {} to {lin_type} lineage ",
                self.cur_batch
            )?;
        } else {
            writeln!(self.out, "Performing lineage scan...")?;
        }
        let Some(found_gen) = self.pop_genotype(&lin_type, self.cur_batch)? else {
            writeln!(
                self.err,
                "  Warning: Genotype {lin_type} not found.  Lineage scan aborted."
            )?;
            return Ok(());
        };
        let mut next_id = found_gen.parent_id;
        let mut found_list = VecDeque::from([found_gen]);
        let bi = self.batch_index(self.cur_batch)?;
        let list = &mut self.batches[bi].list;
        while let Some(p) = list.iter().position(|g| g.id_num == next_id) {
            let g = list.remove(p);
            next_id = g.parent_id;
            found_list.push_front(g);
        }
        let total_removed = list.len();
        let total_kept = found_list.len();
        *list = found_list.into_iter().collect();
        if Self::verbosity(host) >= VERBOSE_ON {
            writeln!(
                self.out,
                "  Lineage has {total_kept} genotypes; {total_removed} were removed."
            )?;
        }
        self.batches[bi].is_lineage = true;
        self.batches[bi].is_aligned = false;
        Ok(())
    }

    /// `FindSexLineage`: like `FindLineage` but following both parents,
    /// with `rec_region_size` (default) or `genome_size` choosing the
    /// "mother".
    pub(crate) fn find_sex_lineage(
        &mut self,
        host: &mut dyn AnalyzeHost,
        mut cur_string: String,
    ) -> Result<()> {
        let mut lin_type = "num_cpus".to_string();
        if su::count_num_words(&cur_string) > 0 {
            lin_type = su::pop_word(&mut cur_string);
        }
        let mut parent_method = "rec_region_size".to_string();
        if su::count_num_words(&cur_string) > 0 {
            parent_method = su::pop_word(&mut cur_string);
        }
        if Self::verbosity(host) >= VERBOSE_ON {
            writeln!(
                self.out,
                "Reducing batch {} to {lin_type} sexual lineage  using {parent_method} criteria.",
                self.cur_batch
            )?;
        } else {
            writeln!(self.out, "Performing sexual lineage scan...")?;
        }
        let Some(found_gen) = self.pop_genotype(&lin_type, self.cur_batch)? else {
            writeln!(
                self.err,
                "  Warning: Genotype {lin_type} not found.  Sexual lineage scan aborted."
            )?;
            return Ok(());
        };
        let by_size = parent_method == "genome_size";
        let mut next_id1 = found_gen.parent_id;
        let mut next_id2 = found_gen.parent2_id;
        let mut found_list = VecDeque::from([found_gen]);
        let bi = self.batch_index(self.cur_batch)?;
        let list = &mut self.batches[bi].list;

        // (length, parent_id, parent2_id) of a genotype; the scan only reads
        // these through its mom/dad pointers.
        type Snap = (i32, i32, i32);
        let snap = |g: &AnalyzeGenotype| -> Snap { (g.length, g.parent_id, g.parent2_id) };

        let mut found_m = true;
        let mut found_d = true;
        while found_m && found_d {
            found_m = false;
            found_d = false;
            // Father first: in the batch, then among those already found.
            let mut found_dad: Option<Snap> = None;
            if let Some(p) = list.iter().position(|g| g.id_num == next_id2) {
                let g = list.remove(p);
                found_dad = Some(snap(&g));
                found_list.push_front(g);
                found_d = true;
            }
            if !found_d {
                if let Some(g) = found_list.iter().find(|g| g.id_num == next_id2) {
                    found_dad = Some(snap(g));
                    found_d = true;
                }
            }
            // Then the mother, possibly swapping roles by genome size.
            let mut mom: Option<Snap> = None;
            if let Some(p) = list.iter().position(|g| g.id_num == next_id1) {
                let g = list.remove(p);
                mom = Some(snap(&g));
                found_list.push_front(g);
            } else if let Some(g) = found_list.iter().find(|g| g.id_num == next_id1) {
                mom = Some(snap(g));
            }
            if let Some(mut m) = mom {
                if by_size {
                    let d = found_dad.ok_or_else(|| {
                        AnalyzeError::fatal(
                            "FIND_SEX_LINEAGE: father not found (C++ NULL dereference)",
                        )
                    })?;
                    if m.0 < d.0 {
                        m = d;
                    }
                }
                next_id1 = m.1;
                next_id2 = m.2;
                found_m = true;
            }
        }

        let total_removed = list.len();
        let total_kept = found_list.len();
        *list = found_list.into_iter().collect();
        if Self::verbosity(host) >= VERBOSE_ON {
            writeln!(
                self.out,
                "  Sexual lineage has {total_kept} genotypes; {total_removed} were removed."
            )?;
        }
        self.batches[bi].clear_flags();
        Ok(())
    }

    /// `FindClade`: keep a genotype and all of its descendants.
    pub(crate) fn find_clade(
        &mut self,
        host: &mut dyn AnalyzeHost,
        mut cur_string: String,
    ) -> Result<()> {
        if cur_string.is_empty() {
            writeln!(
                self.err,
                "  Warning: No clade specified for FIND_CLADE.  Aborting."
            )?;
            return Ok(());
        }
        let clade_type = su::pop_word(&mut cur_string);
        if Self::verbosity(host) >= VERBOSE_ON {
            writeln!(
                self.out,
                "Reducing batch {} to clade {clade_type}.",
                self.cur_batch
            )?;
        } else {
            writeln!(self.out, "Performing clade scan...")?;
        }
        let Some(found_gen) = self.pop_genotype(&clade_type, self.cur_batch)? else {
            writeln!(
                self.err,
                "  Warning: Ancestral genotype {clade_type} not found.  Clade scan aborted."
            )?;
            return Ok(());
        };
        let bi = self.batch_index(self.cur_batch)?;
        let list = &mut self.batches[bi].list;
        let mut found_list: VecDeque<AnalyzeGenotype> = VecDeque::new();
        let mut scan_list: VecDeque<AnalyzeGenotype> = VecDeque::from([found_gen]);
        while let Some(g) = scan_list.pop_front() {
            let parent_id = g.id_num;
            found_list.push_front(g);
            let old = std::mem::take(list);
            for c in old {
                if c.parent_id == parent_id {
                    scan_list.push_front(c);
                } else {
                    list.push(c);
                }
            }
        }
        let total_removed = list.len();
        let total_kept = found_list.len();
        *list = found_list.into_iter().collect();
        if Self::verbosity(host) >= VERBOSE_ON {
            writeln!(
                self.out,
                "  Clade has {total_kept} genotypes; {total_removed} were removed."
            )?;
        }
        self.batches[bi].clear_flags();
        Ok(())
    }

    /// `FindLastCommonAncestor`: reduce the batch to the last genotype all
    /// others descend from.
    ///
    /// The C++ keeps the parent/child links in the genotypes across
    /// commands; they are rebuilt from scratch here (identical unless a
    /// previous call stopped on its "more than one genotype" error and the
    /// batch was edited in between).
    pub(crate) fn find_last_common_ancestor(
        &mut self,
        host: &mut dyn AnalyzeHost,
        _args: String,
    ) -> Result<()> {
        writeln!(
            self.out,
            "Finding last common ancestor of batch {}",
            self.cur_batch
        )?;
        let verbose = Self::verbosity(host) >= VERBOSE_ON;
        if verbose {
            writeln!(self.out, "  Connecting genotypes to parents. ")?;
        }
        let bi = self.batch_index(self.cur_batch)?;
        let list = &self.batches[bi].list;
        let n = list.len();
        let mut parent: Vec<Option<usize>> = vec![None; n];
        let mut children: Vec<Vec<usize>> = vec![Vec::new(); n];
        for c in 0..n {
            if let Some(p) = (0..n).find(|&p| list[c].parent_id == list[p].id_num) {
                parent[c] = Some(p);
                if !children[p].contains(&c) {
                    children[p].push(c);
                }
            }
        }
        if verbose {
            writeln!(self.out, "  Finding earliest genotype. ")?;
        }
        let mut lca: Option<usize> = None;
        for t in 0..n {
            if parent[t].is_none() {
                if let Some(l) = lca {
                    writeln!(
                        self.out,
                        "Error: More than one genotype does not have a parent. "
                    )?;
                    writeln!(self.out, "Genotype 1: {}", list[t].id_num)?;
                    writeln!(self.out, "Genotype 2: {}", list[l].id_num)?;
                    return Ok(());
                }
                lca = Some(t);
            }
        }
        if verbose {
            writeln!(self.out, "  Following children to last common ancestor. ")?;
        }
        let mut lca = lca.ok_or_else(|| {
            AnalyzeError::fatal(
                "FIND_LAST_COMMON_ANCESTOR: no genotype without a parent (C++ NULL dereference)",
            )
        })?;
        while children[lca].len() == 1 {
            lca = children[lca].remove(0);
        }
        let keep = self.batches[bi].list.swap_remove(lca);
        self.batches[bi].list = vec![keep];
        Ok(())
    }

    /// `SampleOrganisms`: keep a random subset of the organisms.
    pub(crate) fn sample_organisms(
        &mut self,
        host: &mut dyn AnalyzeHost,
        mut cur_string: String,
    ) -> Result<()> {
        let fraction = su::as_double(&su::pop_word(&mut cur_string));
        let bi = self.batch_index(self.cur_batch)?;
        let init_genotypes = self.batches[bi].list.len();
        let mut test_viable = 0.0;
        if !cur_string.is_empty() {
            test_viable = su::as_double(&su::pop_word(&mut cur_string));
        }
        let verbose = Self::verbosity(host) >= VERBOSE_ON;
        if verbose {
            writeln!(
                self.out,
                "Sampling {} organisms from batch {}.",
                avida_apto::fmt_double(fraction),
                self.cur_batch
            )?;
        } else {
            writeln!(self.out, "Sampling Organisms...")?;
        }

        let list = &mut self.batches[bi].list;
        let mut org_count: i32 = 0;
        for g in list.iter_mut() {
            if test_viable == 1.0 && !g.viable {
                g.num_cpus = 0;
            }
            org_count = org_count.wrapping_add(g.num_cpus);
        }
        let mut org_array: Vec<usize> = Vec::with_capacity(org_count.max(0) as usize);
        for (i, g) in list.iter_mut().enumerate() {
            for _ in 0..g.num_cpus {
                org_array.push(i);
            }
            g.num_cpus = 0;
        }

        let mut new_org_count = fraction as i32;
        if fraction < 1.0 {
            new_org_count = (fraction * org_count as f64) as i32;
        }
        if new_org_count > org_count {
            writeln!(
                self.err,
                "Warning: Trying to sample {new_org_count}organisms from a population of {org_count}"
            )?;
            new_org_count = org_count;
        }
        if new_org_count < 0 {
            return Err(AnalyzeError::fatal(format!(
                "SAMPLE_ORGANISMS: negative sample size {new_org_count} (C++ allocates a negative array)"
            )));
        }
        let mut keep_ids = vec![0i32; new_org_count as usize];
        self.random.choose(org_count, &mut keep_ids);
        let list = &mut self.batches[bi].list;
        for &k in &keep_ids {
            let gi = org_array[k as usize];
            list[gi].num_cpus += 1;
        }
        list.retain(|g| g.num_cpus != 0);

        let num_genotypes = list.len();
        if verbose {
            writeln!(
                self.out,
                "  Removed {} organisms ({} genotypes); {new_org_count} orgs ({num_genotypes} gens) remaining.",
                org_count - new_org_count,
                init_genotypes as i64 - num_genotypes as i64
            )?;
        }
        self.batches[bi].clear_flags();
        Ok(())
    }

    /// `SampleGenotypes`: keep each genotype with probability `fraction`.
    pub(crate) fn sample_genotypes(
        &mut self,
        host: &mut dyn AnalyzeHost,
        mut cur_string: String,
    ) -> Result<()> {
        let fraction = su::as_double(&su::pop_word(&mut cur_string));
        let bi = self.batch_index(self.cur_batch)?;
        let init_genotypes = self.batches[bi].list.len();
        let mut test_viable = 0.0;
        if !cur_string.is_empty() {
            test_viable = su::as_double(&su::pop_word(&mut cur_string));
        }
        let verbose = Self::verbosity(host) >= VERBOSE_ON;
        if verbose {
            writeln!(
                self.out,
                "Sampling {} genotypes from batch {}.",
                avida_apto::fmt_double(fraction),
                self.cur_batch
            )?;
        } else {
            writeln!(self.out, "Sampling Genotypes...")?;
        }
        let frac_remove = 1.0 - fraction;
        let old = std::mem::take(&mut self.batches[bi].list);
        let mut kept = Vec::with_capacity(old.len());
        for g in old {
            // The random draw happens for every genotype (it comes first).
            let drop_it = self.random.p(frac_remove) || (!g.viable && test_viable == 1.0);
            if !drop_it {
                kept.push(g);
            }
        }
        let num_genotypes = kept.len();
        self.batches[bi].list = kept;
        if verbose {
            writeln!(
                self.out,
                "  Removed {} genotypes; {num_genotypes} remaining.",
                init_genotypes - num_genotypes
            )?;
        }
        self.batches[bi].clear_flags();
        Ok(())
    }

    /// `KeepTopGenotypes`: drop genotypes from the end until `n` remain.
    pub(crate) fn keep_top_genotypes(&mut self, mut cur_string: String) -> Result<()> {
        let num_kept = su::as_int(&su::pop_word(&mut cur_string));
        let b = self.cur()?;
        let num_removed = b.list.len() as i64 - num_kept as i64;
        for _ in 0..num_removed.max(0) {
            b.list.pop();
        }
        // The lineage flag survives.
        b.is_aligned = false;
        Ok(())
    }

    /// `TruncateLineage [task [id]] [lineage-type]`: find the lineage,
    /// recalculate it, and cut it after the first genotype doing the task.
    pub(crate) fn truncate_lineage(
        &mut self,
        host: &mut dyn AnalyzeHost,
        mut cur_string: String,
    ) -> Result<()> {
        let mut type_ = "task".to_string();
        let mut arg_i = -1;
        if !cur_string.is_empty() {
            type_ = su::pop_word(&mut cur_string);
        }
        if type_ == "task" {
            if !cur_string.is_empty() {
                arg_i = su::as_int(&su::pop_word(&mut cur_string));
            }
            let env_size = host.num_tasks();
            if arg_i < 0 || arg_i >= env_size {
                arg_i = env_size - 1;
            }
        }
        let mut lin_type = "num_cpus".to_string();
        if !cur_string.is_empty() {
            lin_type = su::pop_word(&mut cur_string);
        }
        self.find_lineage(host, lin_type)?;
        self.batch_recalculate(host, String::new())?;

        if type_ == "task" {
            if Self::verbosity(host) >= VERBOSE_ON {
                writeln!(
                    self.out,
                    "Truncating batch {} based on task {arg_i} emergence...",
                    self.cur_batch
                )?;
            } else {
                writeln!(self.out, "Truncating lineage...")?;
            }
            let bi = self.batch_index(self.cur_batch)?;
            let list = &mut self.batches[bi].list;
            let mut cut = list.len();
            for (i, g) in list.iter().enumerate() {
                if g.task_count(arg_i)? != 0 {
                    cut = i + 1;
                    break;
                }
            }
            list.truncate(cut);
        }
        Ok(())
    }

    /// `CommandAlign`: gap-align each sequence against the previous one.
    pub(crate) fn command_align(&mut self, host: &mut dyn AnalyzeHost) -> Result<()> {
        writeln!(self.out, "Aligning sequences...")?;
        let bi = self.batch_index(self.cur_batch)?;
        if !self.batches[bi].is_lineage && Self::verbosity(host) >= VERBOSE_ON {
            writeln!(
                self.err,
                "  Warning: sequences may not be a consecutive lineage."
            )?;
        }
        let list = &mut self.batches[bi].list;
        let mut sequences: Vec<String> = Vec::with_capacity(list.len());
        for i in 0..list.len() {
            sequences.push(list[i].sequence());
            if i == 0 {
                continue;
            }
            let mut num_ins = 0;
            let mut num_del = 0;
            let (_, mut diff_info) = su::edit_distance_info(&sequences[i], &sequences[i - 1], b'_');
            while !diff_info.is_empty() {
                let Some((mut_type, position)) = pop_mutation(&mut diff_info) else {
                    continue;
                };
                match mut_type {
                    b'I' => {
                        for s in sequences.iter_mut().take(i) {
                            cstring_insert(s, "_", position + num_del)?;
                        }
                        num_ins += 1;
                    }
                    b'D' => {
                        cstring_insert(&mut sequences[i], "_", position + num_ins)?;
                        num_del += 1;
                    }
                    _ => {}
                }
            }
        }
        for (g, s) in list.iter_mut().zip(sequences) {
            g.aligned_sequence = s;
        }
        self.batches[bi].is_aligned = true;
        Ok(())
    }
}

/// `tListPlus::FindSummedValue(sum, &GetNumCPUs)`: walk while the running
/// total is below `sum`, returning the element *after* the one that reached
/// it (`None` = the list sentinel, a NULL genotype in C++).
/// QUIRK: off by one; `sum == 0` returns the first genotype.
fn find_summed_value(list: &[AnalyzeGenotype], sum: i32) -> Option<usize> {
    if list.is_empty() {
        return None;
    }
    let mut total = 0i32;
    let mut idx = 0;
    while idx < list.len() && total < sum {
        total = total.wrapping_add(list[idx].num_cpus);
        idx += 1;
    }
    (idx < list.len()).then_some(idx)
}

/// Whether some draw `0..num_orgs` of `FindOrganism` ends on a viable
/// genotype with organisms left, or on the NULL sentinel (a crash); if
/// neither, the C++ retry loop never ends.
fn find_summed_value_can_succeed(list: &[AnalyzeGenotype], num_orgs: i32) -> bool {
    let draws = if num_orgs <= 0 { 1 } else { num_orgs };
    let ok = |pos: Option<usize>| match pos {
        None => true,
        Some(p) => list[p].num_cpus != 0 && list[p].viable,
    };
    if draws <= 4096 {
        return (0..draws).any(|s| ok(find_summed_value(list, s)));
    }
    // Large populations: a genotype k >= 1 is reachable when its
    // predecessor holds organisms; k == 0 through the draw 0.
    if list.is_empty() {
        return true;
    }
    if ok(Some(0)) {
        return true;
    }
    (1..=list.len()).any(|k| list[k - 1].num_cpus > 0 && ok((k < list.len()).then_some(k)))
}
