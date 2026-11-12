//! Complexity and mate-selection commands of `cAnalyze`
//! (analyze/cAnalyze.cc at 47f13dadb): `ANALYZE_COMPLEXITY_TWO_SITES`,
//! `ANALYZE_COMPLEXITY_DELTA` (with `cAnalyzeGenotype::CalcKnockouts`),
//! `COMMUNITY_COMPLEXITY`, `ANALYZE_MATE_SELECTION` and
//! `PRINT_RESOURCE_FITNESS_MAP`.
//!
//! Floating point follows the reference build (Apple clang, arm64, `-O3`,
//! `-ffp-contract=on`): clang fuses `a * b + c` written in one expression
//! (also `x += a * b` and `x -= a * b`) into one multiply-add, so those
//! sums use `mul_add` here, except where the loop vectoriser splits the
//! multiply-add again (see [`sum_products`]; checked against the
//! disassembly of the reference binary).

use std::collections::BTreeMap;
use std::io::Write;

use avida_apto::{fmt_double, AvidaRng};
use avida_config::string_util as su;
use avida_genome::{Genome, Instruction, InstructionSequence};
use avida_hardware::test_cpu::CpuTestInfo;

use crate::analyze::Analyze;
use crate::cmd_mutation::{base_ops, create_file, recalc_in_place, recalc_new, write_raw};
use crate::cmd_output::pop_directory;
use crate::cmd_population::static_file;
use crate::error::{AnalyzeError, Result};
use crate::genotype::{AnalyzeGenotype, KnockoutStats, MIN_GENOME_LENGTH};
use crate::host::{AnalyzeHost, TestContext};
use crate::landscape::inst_set_of;
use crate::verbose::VERBOSE_ON;

fn hw<T>(r: std::result::Result<T, String>) -> Result<T> {
    r.map_err(AnalyzeError::world)
}

/// `MAX_GENOME_LENGTH`.
const MAX_GENOME_LENGTH: i32 = 2048;

/// `RES_CONSTANT` of `eTestCPUResourceMethod`.
const RES_CONSTANT: i32 = 1;

/// Iterations after which a mutation-selection balance is taken not to
/// converge (the C++ `while(1)` would run on for ever).
const MAX_BALANCE_ITERATIONS: u64 = 50_000_000;

/// `v[i]` for an index the C++ does not check.
fn at<T: Copy>(v: &[T], i: i32, what: &str) -> Result<T> {
    if i < 0 || i as usize >= v.len() {
        return Err(AnalyzeError::fatal(format!(
            "{what}: index {i} out of range 0..{} (C++ reads past the array)",
            v.len()
        )));
    }
    Ok(v[i as usize])
}

/// `-sum p ln(1/p) / ln(base)` over the non-zero `p`, in mers (`base` =
/// the number of instructions) and in bits.
fn entropies(prob: impl Iterator<Item = f64>, ln_insts: f64) -> (f64, f64) {
    let (mut mers, mut bits) = (0.0, 0.0);
    for p in prob {
        if p != 0.0 {
            mers += p * (1.0 / p).ln() / ln_insts;
            bits += p * (1.0 / p).ln() / 2.0f64.ln();
        }
    }
    (mers, bits)
}

/// `acc += a[i] * b[i]` over the slices as the reference build compiles
/// the `w_avg` loops: the loop vectoriser turns the first `len & !7` terms
/// into separate multiplies and in-order adds, and only the scalar
/// remainder keeps clang's fused multiply-add.
fn sum_products(mut acc: f64, a: &[f64], b: &[f64]) -> f64 {
    let n = a.len().min(b.len());
    let (head_a, tail_a) = a[..n].split_at(n & !7);
    let (head_b, tail_b) = b[..n].split_at(n & !7);
    for (x, y) in head_a.iter().zip(head_b) {
        acc += x * y;
    }
    for (x, y) in tail_a.iter().zip(tail_b) {
        acc = x.mul_add(*y, acc);
    }
    acc
}

/// The single-site mutation-selection balance of `ANALYZE_COMPLEXITY_TWO_SITES`
/// started from the relative fitnesses.
fn balance_one_site(test_fitness: &[f64], mut_rate: f64, accuracy: f64) -> Result<Vec<f64>> {
    let n = test_fitness.len();
    let mut fitness_total = 0.0;
    for &f in test_fitness {
        fitness_total += f;
    }
    let mut prob: Vec<f64> = test_fitness.iter().map(|&f| f / fitness_total).collect();
    let mut prob_next = vec![0.0; n];
    let mut iterations: u64 = 0;
    loop {
        let mut delta_prob = 0.0f64;
        for m in 0..n {
            let w_avg = sum_products(0.0, &prob, test_fitness);
            if mut_rate != 0.0 {
                prob_next[m] = ((1.0 - mut_rate) * test_fitness[m] * prob[m]) / w_avg;
                prob_next[m] += mut_rate / n as f64;
            } else {
                prob_next[m] = (test_fitness[m] * prob[m]) / w_avg;
            }
            let d = prob_next[m] - prob[m];
            delta_prob = d.mul_add(d, delta_prob);
        }
        prob.copy_from_slice(&prob_next);
        if delta_prob < accuracy {
            return Ok(prob);
        }
        iterations += 1;
        if delta_prob.is_nan() || iterations > MAX_BALANCE_ITERATIONS {
            return Err(AnalyzeError::fatal(
                "ANALYZE_COMPLEXITY_TWO_SITES: the single-site mutation-selection balance does not converge (the C++ loops forever)",
            ));
        }
    }
}

/// The two-site balance (`prob_2s`) from the relative fitnesses of every
/// instruction pair.
fn balance_two_sites(
    test_fitness: &[Vec<f64>],
    fitness_total: f64,
    mut_rate: f64,
    accuracy: f64,
) -> Result<Vec<Vec<f64>>> {
    let n = test_fitness.len();
    let nf = n as f64;
    let mut prob: Vec<Vec<f64>> = test_fitness
        .iter()
        .map(|row| row.iter().map(|&f| f / fitness_total).collect())
        .collect();
    let mut next = vec![vec![0.0f64; n]; n];
    let mut iterations: u64 = 0;
    loop {
        let mut delta_prob = 0.0f64;
        for m1 in 0..n {
            for m2 in 0..n {
                let mut w_avg = 0.0f64;
                for (p, f) in prob.iter().zip(test_fitness) {
                    w_avg = sum_products(w_avg, p, f);
                }
                if mut_rate != 0.0 {
                    let mut v =
                        ((1.0 - mut_rate) * (1.0 - mut_rate) * test_fitness[m1][m2] * prob[m1][m2])
                            / w_avg;
                    let mut sum_term2 = 0.0;
                    for i in 0..n {
                        sum_term2 += (test_fitness[i][m2] * prob[i][m2]) / w_avg;
                    }
                    v = ((mut_rate * (1.0 - mut_rate)) / nf).mul_add(sum_term2, v);
                    let mut sum_term3 = 0.0;
                    for j in 0..n {
                        sum_term3 += (test_fitness[m1][j] * prob[m1][j]) / w_avg;
                    }
                    v = ((mut_rate * (1.0 - mut_rate)) / nf).mul_add(sum_term3, v);
                    v = (mut_rate / nf).mul_add(mut_rate / nf, v);
                    next[m1][m2] = v;
                } else {
                    next[m1][m2] = (test_fitness[m1][m2] * prob[m1][m2]) / w_avg;
                }
                let d = next[m1][m2] - prob[m1][m2];
                delta_prob = d.mul_add(d, delta_prob);
            }
        }
        for (p, q) in prob.iter_mut().zip(&next) {
            p.copy_from_slice(q);
        }
        if delta_prob < accuracy {
            return Ok(prob);
        }
        iterations += 1;
        if delta_prob.is_nan() || iterations > MAX_BALANCE_ITERATIONS {
            return Err(AnalyzeError::fatal(
                "ANALYZE_COMPLEXITY_TWO_SITES: the two-site mutation-selection balance does not converge (the C++ loops forever)",
            ));
        }
    }
}

/// `cAnalyzeGenotype::CalcKnockouts(false, check_chart)`: the single
/// knockout counts of `g` (and, with `check_chart`, the task counts of
/// each knockout), computed once and cached on the genotype. A cached
/// result without the chart is recomputed when the chart is asked for.
pub(crate) fn calc_knockouts(
    host: &mut dyn AnalyzeHost,
    g: &mut AnalyzeGenotype,
    check_chart: bool,
) -> Result<()> {
    match &g.knockouts {
        None => {}
        Some(ks) if check_chart && !ks.has_chart_info => {}
        Some(_) => return Ok(()),
    }
    let mut ks = KnockoutStats::default();
    crate::cpu::create_test_cpu(host)?;
    let base_fitness = recalc_new(host, &g.genome)?.fitness;
    let length = g.length;
    if base_fitness == 0.0 {
        ks.neut_count = length;
        g.knockouts = Some(ks);
        return Ok(());
    }
    let mut mod_genome = g.genome.clone();
    let set_name = inst_set_of(host.inst_sets(), &mod_genome)?
        .inst_set_name()
        .to_string();
    let null_inst = host.activate_null_inst(&set_name)?;
    if length < 0 || length > mod_genome.representation().size() {
        return Err(AnalyzeError::fatal(format!(
            "CalcKnockouts: length {length} of a {}-site genome (C++ indexes past the sequence)",
            mod_genome.representation().size()
        )));
    }
    if check_chart {
        ks.task_counts = vec![Vec::new(); length as usize];
        ks.has_chart_info = true;
    }
    for line in 0..length {
        let cur_inst = mod_genome.representation().get(line);
        mod_genome.representation_mut().set(line, null_inst);
        let ko = recalc_new(host, &mod_genome)?;
        if check_chart {
            ks.task_counts[line as usize] = ko.task_counts.clone();
        }
        let ko_fitness = ko.fitness;
        if ko_fitness == 0.0 {
            ks.dead_count += 1;
        } else if ko_fitness < base_fitness {
            ks.neg_count += 1;
        } else if ko_fitness == base_fitness {
            ks.neut_count += 1;
        } else if ko_fitness > base_fitness {
            ks.pos_count += 1;
        } else {
            // cerr in C++; the analyze streams are not reachable from here.
            eprintln!("error: internal: illegal state in CalcKnockouts()");
        }
        mod_genome.representation_mut().set(line, cur_inst);
    }
    g.knockouts = Some(ks);
    Ok(())
}

/// `GetKO_Complexity()`: lethal plus detrimental knockouts.
fn ko_complexity(host: &mut dyn AnalyzeHost, g: &mut AnalyzeGenotype) -> Result<i32> {
    calc_knockouts(host, g, false)?;
    let ks = g
        .knockouts
        .as_ref()
        .ok_or_else(|| AnalyzeError::fatal("knockout statistics missing after CalcKnockouts"))?;
    Ok(ks.dead_count + ks.neg_count)
}

/// `GetKO_TaskCounts()`.
fn ko_task_counts(host: &mut dyn AnalyzeHost, g: &mut AnalyzeGenotype) -> Result<Vec<Vec<i32>>> {
    calc_knockouts(host, g, true)?;
    Ok(g.knockouts
        .as_ref()
        .map(|ks| ks.task_counts.clone())
        .unwrap_or_default())
}

/// `ko[line][task]`, where the C++ indexes both unchecked.
fn ko_at(ko: &[Vec<i32>], line: i32, task: usize, what: &str) -> Result<i32> {
    let row = ko
        .get(line.max(0) as usize)
        .filter(|_| line >= 0)
        .ok_or_else(|| {
            AnalyzeError::fatal(format!(
                "{what}: knockout chart row {line} of {} (C++ reads past the array)",
                ko.len()
            ))
        })?;
    row.get(task).copied().ok_or_else(|| {
        AnalyzeError::fatal(format!(
            "{what}: knockout chart task {task} of {} (C++ reads past the array)",
            row.len()
        ))
    })
}

impl Analyze {
    // -- ANALYZE_COMPLEXITY_TWO_SITES ---------------------------------------

    /// `AnalyzeComplexityTwoSites [mut_rate [dir [use_resources
    /// [batch_frequency [accuracy]]]]]`: single-site entropies at
    /// mutation-selection balance and, for every pair of sites, the
    /// two-site balance and the sites' mutual information.
    ///
    /// QUIRK: a site whose test fitness for the current instruction is
    /// zero prints a message and is left mutated to the last instruction
    /// (its single-site entropy stays 0) for the rest of the genotype.
    pub(crate) fn analyze_complexity_two_sites(
        &mut self,
        host: &mut dyn AnalyzeHost,
        mut cur_string: String,
    ) -> Result<()> {
        writeln!(
            self.out,
            "Analyzing genome complexity (one and two sites)..."
        )?;
        let words = su::count_num_words(&cur_string);
        let verbose = Self::verbosity(host) >= VERBOSE_ON;
        if verbose {
            writeln!(self.out, "  Number of arguments passed: {words}")?;
        }
        let mut mut_rate = 0.0075;
        if words < 1 {
            if verbose {
                writeln!(
                    self.out,
                    "  - No mutation rate passed, using default mu = {}",
                    fmt_double(mut_rate)
                )?;
            }
        } else {
            mut_rate = su::as_double(&su::pop_word(&mut cur_string));
            if mut_rate < 0.0 {
                mut_rate = 0.0;
            }
            if verbose {
                writeln!(
                    self.out,
                    "  - Mutation rate passed, using mu = {}",
                    fmt_double(mut_rate)
                )?;
            }
        }
        let directory = pop_directory(&su::pop_word(&mut cur_string), "complexity_two_sites/")?;
        if verbose {
            writeln!(self.out, "  - Analysis results to directory: {directory}")?;
        }
        let mut use_resources = 0;
        if words >= 3 {
            use_resources = su::as_int(&su::pop_word(&mut cur_string));
        }
        if verbose {
            writeln!(
                self.out,
                "  - Use resorces set to: {use_resources} (0=false, true other int)"
            )?;
        }
        let mut batch_frequency = 1;
        if words >= 4 {
            batch_frequency = su::as_int(&su::pop_word(&mut cur_string));
            if batch_frequency <= 0 {
                batch_frequency = 1;
            }
        }
        if verbose {
            writeln!(self.out, "  - Batch frequency set to: {batch_frequency}")?;
        }
        let mut converg_accuracy = 1.0e-10;
        if words >= 5 {
            converg_accuracy = su::as_double(&su::pop_word(&mut cur_string));
        }
        if verbose {
            writeln!(
                self.out,
                "  - Convergence accuracy: {}",
                fmt_double(converg_accuracy)
            )?;
        }
        crate::cpu::create_test_cpu(host)?;
        let bi = self.batch_index(self.cur_batch)?;
        let mut summary = create_file(host, &format!("{directory}complexity_batch_summary.dat"))?;
        summary.write_comment("One, Two Site Entropy/Complexity Analysis");
        summary.write_time_stamp();

        let mut gi = 0;
        while gi < self.batches[bi].list.len() {
            let name = self.batches[bi].list[gi].name.clone();
            if verbose {
                writeln!(self.out, "  Analyzing complexity for {name}")?;
            }
            let mut fp_2s =
                create_file(host, &format!("{directory}{name}.twosite.complexity.dat"))?;
            fp_2s.write_comment("One, Two Site Entropy/Complexity Analysis");
            fp_2s.write_comment("NOTE: mutual information = (col 6 + col 8) - (col 9)");
            fp_2s.write_comment("NOTE: possible negative mutual information-- is this real? ");
            fp_2s.write_time_stamp();

            let mut info = CpuTestInfo::default();
            info.set_resource_options(
                use_resources,
                self.resources.clone(),
                self.batches[bi].list[gi].update_born,
                self.resource_time_spent_offset,
            );
            recalc_in_place(host, &mut self.batches[bi].list[gi], &mut info, 1)?;
            let g = &self.batches[bi].list[gi];
            let (genotype_id, genotype_fitness) = (g.id_num, g.fitness);
            let base_genome = g.genome.clone();
            let base = base_ops(&base_genome, g.length, "ANALYZE_COMPLEXITY_TWO_SITES")?;
            let max_line = base.len();
            let mut seq = base_genome.clone();
            let num_insts = inst_set_of(host.inst_sets(), &base_genome)?.size();
            let ni = num_insts.max(0) as usize;
            let ln_insts = (num_insts as f64).ln();

            let mut genome_ss_entropy_mers = 0.0;
            let mut genome_ss_entropy_bits = 0.0;
            let mut genome_ss_complexity_mers = 0.0;
            let mut genome_ss_complexity_bits = 0.0;
            let mut genome_ds_mut_info_mers = 0.0;
            let mut genome_ds_mut_info_bits = 0.0;

            // One site.
            let mut entropy_ss_mers = vec![0.0f64; max_line];
            for (line, &cur_inst) in base.iter().enumerate() {
                let mut test_fitness = vec![0.0f64; ni];
                for (mod_inst, f) in test_fitness.iter_mut().enumerate() {
                    seq.representation_mut()
                        .set(line as i32, Instruction::new(mod_inst as i32));
                    *f = recalc_new(host, &seq)?.fitness;
                }
                let cur_inst_fitness = at(&test_fitness, cur_inst, "ANALYZE_COMPLEXITY_TWO_SITES")?;
                if cur_inst_fitness > 0.0 {
                    for f in test_fitness.iter_mut() {
                        if *f > cur_inst_fitness {
                            *f = cur_inst_fitness;
                        }
                        *f /= cur_inst_fitness;
                    }
                } else {
                    writeln!(
                        self.out,
                        "Fitness of this genotype is ZERO--no information."
                    )?;
                    continue;
                }
                let prob = balance_one_site(&test_fitness, mut_rate, converg_accuracy)?;
                let (entropy_mers, entropy_bits) = entropies(prob.iter().copied(), ln_insts);
                let complexity_mers = 1.0 - entropy_mers;
                let complexity_bits = ln_insts / 2.0f64.ln() - entropy_bits;
                genome_ss_entropy_mers += entropy_mers;
                genome_ss_entropy_bits += entropy_bits;
                genome_ss_complexity_mers += complexity_mers;
                genome_ss_complexity_bits += complexity_bits;
                entropy_ss_mers[line] = entropy_mers;
                seq.representation_mut()
                    .set(line as i32, Instruction::new(cur_inst));
            }

            // Two sites.
            for line1 in 0..max_line {
                for line2 in line1 + 1..max_line {
                    let (cur_inst1, cur_inst2) = (base[line1], base[line2]);
                    let cur_fitness = genotype_fitness;
                    let mut fitness_total = 0.0;
                    let mut test_fitness = vec![vec![0.0f64; ni]; ni];
                    if cur_fitness > 0.0 {
                        for (m1, row) in test_fitness.iter_mut().enumerate() {
                            for (m2, f) in row.iter_mut().enumerate() {
                                seq.representation_mut()
                                    .set(line1 as i32, Instruction::new(m1 as i32));
                                seq.representation_mut()
                                    .set(line2 as i32, Instruction::new(m2 as i32));
                                let mut v = recalc_new(host, &seq)?.fitness;
                                if v > cur_fitness {
                                    v = cur_fitness;
                                }
                                v /= cur_fitness;
                                *f = v;
                                fitness_total += v;
                            }
                        }
                    } else {
                        writeln!(
                            self.out,
                            "Fitness of this genotype is ZERO--no information."
                        )?;
                        continue;
                    }
                    let prob_2s = balance_two_sites(
                        &test_fitness,
                        fitness_total,
                        mut_rate,
                        converg_accuracy,
                    )?;
                    let mut prob_1s_i = vec![0.0f64; ni];
                    let mut prob_1s_j = vec![0.0f64; ni];
                    for (p, row) in prob_1s_i.iter_mut().zip(&prob_2s) {
                        for &x in row {
                            *p += x;
                        }
                    }
                    for j in 0..ni {
                        for row in &prob_2s {
                            prob_1s_j[j] += row[j];
                        }
                    }
                    let (site1_mers, site1_bits) = entropies(prob_1s_i.iter().copied(), ln_insts);
                    let (site2_mers, site2_bits) = entropies(prob_1s_j.iter().copied(), ln_insts);
                    let (ds_mers, ds_bits) = entropies(prob_2s.iter().flatten().copied(), ln_insts);
                    let mut mutual_information_mers = site1_mers + site2_mers;
                    mutual_information_mers -= ds_mers;
                    let mut mutual_information_bits = site1_bits + site2_bits;
                    mutual_information_bits -= ds_bits;
                    genome_ds_mut_info_mers += mutual_information_mers;
                    genome_ds_mut_info_bits += mutual_information_bits;

                    fp_2s.write(line1 as i32, "Site 1 in genome");
                    fp_2s.write(line2 as i32, "Site 2 in genome");
                    fp_2s.write(cur_inst1, "Current Instruction, Site 1");
                    fp_2s.write(cur_inst2, "Current Instruction, Site 2");
                    fp_2s.write(
                        entropy_ss_mers[line1],
                        "Entropy (MERS), Site 1 -- single site mut-sel balance",
                    );
                    fp_2s.write(
                        site1_mers,
                        "Entropy (MERS), Site 1 -- TWO site mut-sel balance",
                    );
                    fp_2s.write(
                        entropy_ss_mers[line2],
                        "Entropy (MERS), Site 2 -- single site mut-sel balance",
                    );
                    fp_2s.write(
                        site2_mers,
                        "Entropy (MERS), Site 2 -- TWO site mut-sel balance",
                    );
                    fp_2s.write(
                        ds_mers,
                        "Joint Entropy (MERS), Site 1 & 2 -- TWO site mut-sel balance",
                    );
                    fp_2s.write(
                        mutual_information_mers,
                        "Mutual Information (MERS), Site 1 & 2 -- TWO site mut-sel balance",
                    );
                    fp_2s.endl();
                    seq.representation_mut()
                        .set(line1 as i32, Instruction::new(cur_inst1));
                    seq.representation_mut()
                        .set(line2 as i32, Instruction::new(cur_inst2));
                }
            }
            drop(fp_2s);

            summary.write(genotype_id, "Genotype ID");
            summary.write(genotype_fitness, "Genotype Fitness");
            summary.write(genome_ss_entropy_mers, "Entropy (single-site) MERS");
            summary.write(genome_ss_complexity_mers, "Complexity (single-site) MERS");
            summary.write(genome_ds_mut_info_mers, "Mutual Information MERS");
            summary.write(
                genome_ss_complexity_mers + genome_ds_mut_info_mers,
                "Complexity (two-site) MERS",
            );
            summary.write(genome_ss_entropy_bits, "Entropy (single-site) BITS");
            summary.write(genome_ss_complexity_bits, "Complexity (single-site) BITS");
            summary.write(genome_ds_mut_info_bits, "Mutual Information BITS");
            summary.write(
                genome_ss_complexity_bits + genome_ds_mut_info_bits,
                "Complexity (two-site) BITS",
            );
            summary.endl();

            let mut stop = false;
            for _ in 0..batch_frequency - 1 {
                gi += 1;
                match self.batches[bi].list.get(gi) {
                    Some(s) => {
                        if verbose {
                            writeln!(self.out, "Skipping: {}", s.name)?;
                        }
                    }
                    None => {
                        stop = true;
                        break;
                    }
                }
            }
            if stop {
                break;
            }
            gi += 1;
        }
        Ok(())
    }

    // -- ANALYZE_COMPLEXITY_DELTA -------------------------------------------

    /// `AnalyzeComplexityDelta [file [num_tests [copy_mut [ins_mut [del_mut
    /// [threshold]]]]]]`: for random organisms of the genotypes with at
    /// least `threshold` organisms, a random mutant (at least one
    /// mutation) and how the knockout complexity and the per-site task
    /// information change.
    pub(crate) fn analyze_complexity_delta(
        &mut self,
        host: &mut dyn AnalyzeHost,
        mut cur_string: String,
    ) -> Result<()> {
        writeln!(self.out, "Testing complexity delta.")?;
        let mut filename = "complexity_delta.dat".to_string();
        let mut num_tests = 10;
        let mut copy_mut_prob = host.config().copy_mut_prob;
        let mut ins_mut_prob = host.config().divide_ins_prob;
        let mut del_mut_prob = host.config().divide_del_prob;
        let mut count_threshold = 3;
        if !cur_string.is_empty() {
            filename = su::pop_word(&mut cur_string);
        }
        if !cur_string.is_empty() {
            num_tests = su::as_int(&su::pop_word(&mut cur_string));
        }
        if !cur_string.is_empty() {
            copy_mut_prob = su::as_double(&su::pop_word(&mut cur_string));
        }
        if !cur_string.is_empty() {
            ins_mut_prob = su::as_double(&su::pop_word(&mut cur_string));
        }
        if !cur_string.is_empty() {
            del_mut_prob = su::as_double(&su::pop_word(&mut cur_string));
        }
        if !cur_string.is_empty() {
            count_threshold = su::as_int(&su::pop_word(&mut cur_string));
        }
        if Self::verbosity(host) >= VERBOSE_ON {
            writeln!(
                self.out,
                "...using: filename='{filename}' num_tests={num_tests} copy_mut_prob={} ins_mut_prob={} del_mut_prob={} count_threshold={count_threshold}",
                fmt_double(copy_mut_prob),
                fmt_double(ins_mut_prob),
                fmt_double(del_mut_prob)
            )?;
        }

        let bi = self.batch_index(self.cur_batch)?;
        let mut org_array: Vec<usize> = Vec::new();
        for (i, g) in self.batches[bi].list.iter().enumerate() {
            if g.num_cpus < count_threshold {
                continue;
            }
            for _ in 0..g.num_cpus {
                org_array.push(i);
            }
        }
        let org_count = org_array.len() as i32;

        {
            let df = static_file(host, &filename)?;
            df.write_comment(
                "An analyze of expected complexity changes between parent and offspring",
            );
            df.write_time_stamp();
        }

        for _ in 0..num_tests {
            let test_org_id = world_rng(host)?.get_int(org_count);
            let gi = at(&org_array, test_org_id, "ANALYZE_COMPLEXITY_DELTA")?;
            let mut mod_genome = self.batches[bi].list[gi].genome.clone();
            let inst_set = crate::landscape::shared_inst_set_of(host.inst_sets(), &mod_genome)?;
            if copy_mut_prob == 0.0 && ins_mut_prob == 0.0 && del_mut_prob == 0.0 {
                writeln!(
                    self.err,
                    "ERROR: All mutation rates are zero!  No complexity delta analysis possible."
                )?;
                return Ok(());
            }

            let mut num_mutations = 0;
            let mut ins_line = -1;
            let mut del_line = -1;
            while num_mutations == 0 {
                let rng = world_rng(host)?;
                let seq = mod_genome.representation_mut();
                if copy_mut_prob > 0.0 {
                    for i in 0..seq.size() {
                        if rng.p(copy_mut_prob) {
                            seq.set(i, inst_set.random_inst(rng));
                            num_mutations += 1;
                        }
                    }
                }
                if rng.p(ins_mut_prob) {
                    ins_line = rng.get_int(seq.size() + 1);
                    seq.insert(ins_line, inst_set.random_inst(rng));
                    num_mutations += 1;
                }
                if rng.p(del_mut_prob) {
                    del_line = rng.get_int(seq.size());
                    if seq.size() <= 0 {
                        return Err(AnalyzeError::fatal(
                            "ANALYZE_COMPLEXITY_DELTA: deletion from an empty genome (C++ reads past the sequence)",
                        ));
                    }
                    seq.remove(del_line, 1);
                    num_mutations += 1;
                }
            }
            // The knockouts below activate the NULL instruction, which needs
            // the only handle on the instruction set.
            drop(inst_set);

            let g = &mut self.batches[bi].list[gi];
            recalc_in_place(host, g, &mut CpuTestInfo::default(), 1)?;
            let start_complexity = ko_complexity(host, g)? as f64;
            let start_fitness = g.fitness;
            let start_length = g.length;
            let start_gest = g.gest_time;
            let start_task_counts = g.task_counts.clone();
            let start_ko = ko_task_counts(host, g)?;
            let genotype_id = g.id_num;

            let mut new_genotype = AnalyzeGenotype::new(mod_genome);
            recalc_in_place(host, &mut new_genotype, &mut CpuTestInfo::default(), 1)?;
            let end_complexity = ko_complexity(host, &mut new_genotype)? as f64;
            let end_fitness = new_genotype.fitness;
            let end_length = new_genotype.length;
            let end_gest = new_genotype.gest_time;
            let end_task_counts = new_genotype.task_counts.clone();
            let end_ko = ko_task_counts(host, &mut new_genotype)?;

            let complexity_change = end_complexity - start_complexity;
            let (mut lack, mut kept, mut new, mut share) = (0, 0, 0, 0);
            let (mut lost, mut plost, mut shift, mut pshift) = (0, 0, 0, 0);
            let num_tasks = start_task_counts.len();
            let end_task = |t: usize| -> Result<i32> {
                end_task_counts.get(t).copied().ok_or_else(|| {
                    AnalyzeError::fatal(
                        "ANALYZE_COMPLEXITY_DELTA: the mutant has fewer task counts (C++ reads past the array)",
                    )
                })
            };
            for t in 0..num_tasks {
                end_task(t)?;
            }
            let what = "ANALYZE_COMPLEXITY_DELTA";
            let mut end_line = 0;
            for start_line in 0..start_length {
                if start_line == del_line {
                    continue;
                }
                if start_line == ins_line {
                    end_line += 1;
                }
                let (mut maintained, mut gained, mut lost_count) = (0, 0, 0);
                for (t, &start_count) in start_task_counts.iter().enumerate() {
                    let end_count = end_task(t)?;
                    if start_count == 0 && end_count == 0 {
                        continue;
                    }
                    if start_count == end_count {
                        let ko_start = ko_at(&start_ko, start_line, t, what)? != start_count;
                        let ko_end = ko_at(&end_ko, end_line, t, what)? != end_count;
                        if ko_start && ko_end {
                            maintained += 1;
                        }
                        if ko_start && !ko_end {
                            lost_count += 1;
                        }
                        if !ko_start && ko_end {
                            gained += 1;
                        }
                        continue;
                    }
                    if start_count < end_count {
                        if ko_at(&end_ko, end_line, t, what)? < end_count {
                            gained += 1;
                        }
                        continue;
                    }
                    if ko_at(&start_ko, start_line, t, what)? < start_count {
                        lost_count += 1;
                    }
                }
                let (has_keep, has_loss, has_gain) = (maintained > 0, lost_count > 0, gained > 0);
                match (has_loss, has_gain, has_keep) {
                    (false, false, false) => lack += 1,
                    (false, false, true) => kept += 1,
                    (false, true, false) => new += 1,
                    (false, true, true) => share += 1,
                    (true, false, false) => lost += 1,
                    (true, false, true) => plost += 1,
                    (true, true, false) => shift += 1,
                    (true, true, true) => pshift += 1,
                }
                end_line += 1;
            }

            let df = static_file(host, &filename)?;
            df.write(
                num_mutations,
                "Number of mutational differences between original organism and mutant.",
            );
            df.write(
                complexity_change,
                "Complexity difference between original organism and mutant.",
            );
            df.write(start_complexity, "Total complexity of initial organism.");
            df.write(end_complexity, "Total complexity of mutant.");
            df.write(lack, "Num sites with no info at all.");
            df.write(kept, "Num sites with info, but no change.");
            df.write(new, "Num sites with new info (prev. none).");
            df.write(share, "Num sites with newly shared info.");
            df.write(lost, "Num sites with lost info.");
            df.write(plost, "Num sites with parital lost info.");
            df.write(shift, "Num sites with shift in info.");
            df.write(pshift, "Num sites with partial shift in info.");
            for (i, &c) in start_task_counts.iter().enumerate() {
                df.write(c, &format!("Start task {i}"));
            }
            for (i, &c) in end_task_counts.iter().enumerate() {
                df.write(c, &format!("End task {i}"));
            }
            df.write(start_fitness, "Fitness of initial organism.");
            df.write(end_fitness, "Fitness of mutant.");
            df.write(start_length, "Length of initial organism.");
            df.write(end_length, "Length of mutant.");
            df.write(start_gest, "Gestation Time of initial organism.");
            df.write(end_gest, "Gestation Time of mutant.");
            df.write(genotype_id, "ID of initial genotype.");
            df.endl();
        }
        Ok(())
    }

    // -- COMMUNITY_COMPLEXITY -----------------------------------------------

    /// `AnalyzeCommunityComplexity max_genotypes update [dir [file]]`: the
    /// information each genotype of a community adds about the
    /// environment, from which point mutations of each site are neutral.
    ///
    /// With `max_genotypes` 0 the community is two random genotypes per
    /// task, drawn from a `cRandom` seeded from the clock (so that mode is
    /// not reproducible, in C++ either).
    pub(crate) fn analyze_community_complexity(
        &mut self,
        host: &mut dyn AnalyzeHost,
        mut cur_string: String,
    ) -> Result<()> {
        writeln!(
            self.out,
            "Analyze community complexity of current population about environment with Charles method ..."
        )?;
        let max_genotypes = su::as_int(&su::pop_word(&mut cur_string));
        let update = su::as_int(&su::pop_word(&mut cur_string));
        let directory = pop_directory(&su::pop_word(&mut cur_string), "community_cpx/")?;
        let mut filename = su::pop_word(&mut cur_string);
        if filename.is_empty() {
            filename = "community.complexity.dat".to_string();
        }
        let mut df = create_file(host, &format!("{directory}{filename}"))?;
        write_raw(
            &mut df,
            "# Legend:\n# 1: Genotype ID\n# 2: Entropy given Known Genotypes\n# 3: Entropy given Both Known Genotypes and Env\n# 4: New Information about Environment\n# 5: Total Complexity\n# 6: Hamming Distance to Closest Given Genotype\n# 7: Total Hamming Distance to Closest Neighbor\n# 8: Number of Organisms\n# 9: Total Number of Organisms\n# 10 - : Tasks Implemented\n\n",
        )?;

        let mut info = CpuTestInfo::default();
        info.set_resource_options(
            RES_CONSTANT,
            self.resources.clone(),
            update,
            self.resource_time_spent_offset,
        );
        let bi = self.batch_index(self.cur_batch)?;
        let mut community: Vec<usize> = Vec::new();
        if max_genotypes > 0 {
            let n = self.batches[bi].list.len();
            community.extend(0..n.min(max_genotypes as usize));
        } else if max_genotypes == 0 {
            if self.batches[bi].list.is_empty() {
                return Ok(());
            }
            recalc_in_place(host, &mut self.batches[bi].list[0], &mut info, 1)?;
            let num_tasks = self.batches[bi].list[0].num_tasks();
            let mut classes: Vec<Vec<usize>> = vec![Vec::new(); num_tasks.max(0) as usize];
            for (gi, g) in self.batches[bi].list.iter().enumerate() {
                for (task_id, class) in classes.iter_mut().enumerate() {
                    if g.task_count(task_id as i32)? > 0 {
                        class.push(gi);
                    }
                }
            }
            // QUIRK: `Apto::RNG::AvidaRNG random;` is seeded from the clock.
            let mut random = AvidaRng::new(-1);
            for task_id in 0..classes.len() {
                let num_genotype = classes[task_id].len() as u32;
                if num_genotype > 0 {
                    for _ in 0..2 {
                        let index = random.get_uint(num_genotype) as usize;
                        community.push(classes[task_id][index]);
                    }
                } else {
                    if classes.iter().all(|c| c.is_empty()) {
                        return Err(AnalyzeError::fatal(
                            "COMMUNITY_COMPLEXITY: no genotype does any task (C++ searches for a non-empty class forever)",
                        ));
                    }
                    let mut class_id = random.get_uint(classes.len() as u32) as usize;
                    while classes[class_id].is_empty() {
                        class_id += 1;
                        if class_id >= classes.len() {
                            class_id = 0;
                        }
                    }
                    // QUIRK: the draws index the empty class of `task_id`,
                    // not `class_id`.
                    return Err(AnalyzeError::fatal(format!(
                        "COMMUNITY_COMPLEXITY: no genotype does task {task_id} (C++ indexes the empty genotype class)"
                    )));
                }
            }
        }

        // Point mutations of every site of every community member.
        let size_community = community.len();
        let mut length_genome = 0;
        if size_community > 1 {
            length_genome = self.batches[bi].list[community[0]].length;
        }
        let mut point_mut: BTreeMap<i32, Vec<Vec<f64>>> = BTreeMap::new();
        for &gi in &community {
            let id = self.batches[bi].list[gi].id_num;
            writeln!(self.out, "Test point mutation for genotype {id}")?;
            recalc_in_place(host, &mut self.batches[bi].list[gi], &mut info, 1)?;
            let g = &self.batches[bi].list[gi];
            let base_fitness = g.fitness;
            let base = base_ops(&g.genome, length_genome, "COMMUNITY_COMPLEXITY")?;
            let mut seq = g.genome.clone();
            let num_insts = inst_set_of(host.inst_sets(), &seq)?.size().max(0) as usize;
            let mut prob = vec![vec![0.0f64; num_insts]; base.len()];
            for (line, &cur_inst) in base.iter().enumerate() {
                let mut num_neutral = 0;
                for (mod_inst, p) in prob[line].iter_mut().enumerate() {
                    seq.representation_mut()
                        .set(line as i32, Instruction::new(mod_inst as i32));
                    let mut test = AnalyzeGenotype::new(seq.clone());
                    recalc_in_place(host, &mut test, &mut info, 1)?;
                    if test.fitness >= base_fitness {
                        *p = 1.0;
                        num_neutral += 1;
                    } else {
                        *p = 0.0;
                    }
                }
                for p in prob[line].iter_mut() {
                    *p /= num_neutral as f64;
                }
                seq.representation_mut()
                    .set(line as i32, Instruction::new(cur_inst));
            }
            point_mut.entry(id).or_insert(prob);
        }

        let Some(&first) = community.first() else {
            return Err(AnalyzeError::fatal(
                "COMMUNITY_COMPLEXITY: empty community (C++ reads community[0])",
            ));
        };
        let prob_of = |id: i32| -> Result<&Vec<Vec<f64>>> {
            point_mut.get(&id).ok_or_else(|| {
                AnalyzeError::fatal("COMMUNITY_COMPLEXITY: genotype without point-mutation data")
            })
        };
        let site_entropy = |p: &[f64], ln_n: f64| -> f64 {
            let mut e = 0.0f64;
            for &x in p {
                if x > 0.0 {
                    e = (-x).mul_add(x.ln() / ln_n, e);
                }
            }
            e
        };

        // The first genotype.
        let mut complexity = 0.0f64;
        let mut total_dist = 0;
        let mut total_cpus = 0;
        let mut given: Vec<usize> = Vec::new();
        let first_id = self.batches[bi].list[first].id_num;
        let oo_initial_entropy = length_genome as f64;
        let mut oo_conditional_entropy = 0.0f64;
        let num_insts = inst_set_of(host.inst_sets(), &self.batches[bi].list[first].genome)?.size();
        let ln_insts = (1.0 * num_insts as f64).ln();
        {
            let this_prob = prob_of(first_id)?;
            for line in 0..length_genome.max(0) as usize {
                let row = this_prob.get(line).ok_or_else(|| {
                    AnalyzeError::fatal("COMMUNITY_COMPLEXITY: point-mutation row past the genome")
                })?;
                oo_conditional_entropy +=
                    site_entropy(&row[..row.len().min(num_insts as usize)], ln_insts);
            }
        }
        let new_info = oo_initial_entropy - oo_conditional_entropy;
        complexity += new_info;
        given.push(first);
        let num_cpus = self.batches[bi].list[first].num_cpus;
        total_cpus += num_cpus;
        let mut line_text = format!(
            "{first_id} {} {} {} {}   0 0   {num_cpus} {total_cpus}   ",
            fmt_double(oo_initial_entropy),
            fmt_double(oo_conditional_entropy),
            fmt_double(new_info),
            fmt_double(complexity)
        );
        recalc_in_place(host, &mut self.batches[bi].list[first], &mut info, 1)?;
        for c in &self.batches[bi].list[first].task_counts {
            line_text.push_str(&format!("{c} "));
        }
        line_text.push('\n');
        write_raw(&mut df, &line_text)?;

        // The others, against every genotype given before them.
        for &gi in community.iter().skip(1) {
            if self.batches[bi].list[gi].length != length_genome {
                return Err(self.exit_with("Genotypes in the community do not same genome length."));
            }
            recalc_in_place(host, &mut self.batches[bi].list[gi], &mut info, 1)?;
            let g = &self.batches[bi].list[gi];
            let id = g.id_num;
            writeln!(self.out, "{id} {}", fmt_double(g.fitness))?;
            if g.fitness == 0.0 {
                continue;
            }
            let mut min_new_info = length_genome as f64;
            let mut oo_initial_entropy = 0.0f64;
            let mut oo_conditional_entropy = 0.0f64;
            let mut used: Option<usize> = None;
            let this_prob = prob_of(id)?.clone();
            for &gj in &given {
                let given_prob = prob_of(self.batches[bi].list[gj].id_num)?;
                let mut new_info = 0.0f64;
                let mut total_initial_entropy = 0.0f64;
                let mut total_conditional_entropy = 0.0f64;
                for line in 0..length_genome.max(0) as usize {
                    let (Some(t), Some(gv)) = (this_prob.get(line), given_prob.get(line)) else {
                        return Err(AnalyzeError::fatal(
                            "COMMUNITY_COMPLEXITY: point-mutation row past the genome",
                        ));
                    };
                    let n = num_insts.max(0) as usize;
                    if t.len() < n || gv.len() < n {
                        return Err(AnalyzeError::fatal(
                            "COMMUNITY_COMPLEXITY: genotypes on instruction sets of different sizes (C++ reads past the matrix)",
                        ));
                    }
                    let mut prob_overlap = 0.0f64;
                    for inst in 0..n {
                        if t[inst] < gv[inst] {
                            prob_overlap += t[inst];
                        } else {
                            prob_overlap += gv[inst];
                        }
                    }
                    let given_site_entropy = site_entropy(&gv[..n], ln_insts);
                    let entropy_overlap = if prob_overlap > 0.0 && (1.0 - prob_overlap > 0.0) {
                        (-prob_overlap).mul_add(
                            prob_overlap.ln(),
                            -((1.0 - prob_overlap) * (1.0 - prob_overlap).ln()),
                        ) / ln_insts
                    } else {
                        0.0
                    };
                    let initial_entropy = prob_overlap
                        .mul_add(given_site_entropy, (1.0 - prob_overlap) * 1.0)
                        + entropy_overlap;
                    total_initial_entropy += initial_entropy;
                    let conditional_entropy = site_entropy(&t[..n], ln_insts);
                    total_conditional_entropy += conditional_entropy;
                    if conditional_entropy > initial_entropy + 0.00001 {
                        let mut text = format!("{line}\n");
                        for &x in &t[..n] {
                            text.push_str(&format!("{} ", fmt_double(x)));
                        }
                        text.push('\n');
                        for &x in &gv[..n] {
                            text.push_str(&format!("{} ", fmt_double(x)));
                        }
                        text.push('\n');
                        writeln!(self.err, "Negative Information.")?;
                        write!(self.out, "{text}")?;
                        let _ = self.out.flush();
                        let _ = self.err.flush();
                        return Err(AnalyzeError::Exit(1));
                    }
                    new_info += initial_entropy - conditional_entropy;
                }
                if new_info < min_new_info {
                    min_new_info = new_info;
                    oo_initial_entropy = total_initial_entropy;
                    oo_conditional_entropy = total_conditional_entropy;
                    used = Some(gj);
                    writeln!(
                        self.out,
                        "        New closest genotype {} {}",
                        self.batches[bi].list[gj].id_num,
                        fmt_double(new_info)
                    )?;
                }
            }
            complexity += min_new_info;
            let mut line_text = format!(
                "{id} {} {} {} {}   ",
                fmt_double(oo_initial_entropy),
                fmt_double(oo_conditional_entropy),
                fmt_double(min_new_info),
                fmt_double(complexity)
            );
            let Some(u) = used else {
                write_raw(&mut df, &line_text)?;
                return Err(AnalyzeError::fatal(format!(
                    "COMMUNITY_COMPLEXITY: no closest genotype for {id} (C++ dereferences a NULL used_genotype)"
                )));
            };
            let g = &self.batches[bi].list[gi];
            let hamm_dist = InstructionSequence::find_hamming_distance(
                g.genome.representation(),
                self.batches[bi].list[u].genome.representation(),
                0,
            );
            total_dist += hamm_dist;
            let num_cpus = g.num_cpus;
            total_cpus += num_cpus;
            line_text.push_str(&format!(
                "{hamm_dist} {total_dist}   {num_cpus} {total_cpus}   "
            ));
            for c in &g.task_counts {
                line_text.push_str(&format!("{c} "));
            }
            line_text.push('\n');
            write_raw(&mut df, &line_text)?;
            given.push(gi);
        }
        Ok(())
    }

    // -- ANALYZE_MATE_SELECTION ---------------------------------------------

    /// `AnalyzeMateSelection [sample_size [file [min_swap_frac]]]`: the
    /// fraction of random crossovers between organisms of the batch
    /// (weighted by abundance, viable genotypes only) whose first offspring
    /// is not viable, overall and between organisms with the same mate id.
    pub(crate) fn analyze_mate_selection(
        &mut self,
        host: &mut dyn AnalyzeHost,
        mut cur_string: String,
    ) -> Result<()> {
        let mut sample_size = 10000;
        if !cur_string.is_empty() {
            sample_size = su::as_int(&su::pop_word(&mut cur_string));
        }
        let mut filename = "none".to_string();
        if !cur_string.is_empty() {
            filename = su::pop_word(&mut cur_string);
        }
        let mut min_swap_frac = 0.0;
        if !cur_string.is_empty() {
            min_swap_frac = su::as_double(&su::pop_word(&mut cur_string));
        }
        let max_swap_frac = 1.0 - min_swap_frac;
        writeln!(self.out, "Analyzing Mate Selection... ")?;
        if !(0.0..0.5).contains(&min_swap_frac) {
            writeln!(
                self.err,
                "ERROR: Minimum swap fraction out of range [0.0, 0.5)."
            )?;
        }

        let bi = self.batch_index(self.cur_batch)?;
        // Apto::Map<int, int>: keys in insertion order, iterated bucket by
        // bucket (abs(key % 23)).
        let mut mate_ids: Vec<(i32, i32)> = Vec::new();
        let mut org_count = 0i32;
        let mut gen_count = 0;
        for g in &self.batches[bi].list {
            if !g.viable || g.num_cpus == 0 {
                continue;
            }
            gen_count += 1;
            org_count = org_count.wrapping_add(g.num_cpus);
            match mate_ids.iter_mut().find(|(k, _)| *k == g.mate_id) {
                Some((_, c)) => *c += g.num_cpus,
                None => mate_ids.push((g.mate_id, g.num_cpus)),
            }
        }
        if org_count < 0 {
            return Err(AnalyzeError::fatal(
                "ANALYZE_MATE_SELECTION: negative organism count (C++ allocates an array of negative size)",
            ));
        }
        // QUIRK: the second pass reuses the exhausted list iterator, which
        // wraps around to the start, and skips only non-viable genotypes.
        let mut genotype_array: Vec<Option<usize>> = vec![None; org_count as usize];
        let mut cur_pos = 0usize;
        for (i, g) in self.batches[bi].list.iter().enumerate() {
            if !g.viable {
                continue;
            }
            for _ in 0..g.num_cpus.max(0) {
                if cur_pos >= genotype_array.len() {
                    return Err(AnalyzeError::fatal(
                        "ANALYZE_MATE_SELECTION: more organisms than counted (C++ writes past the array)",
                    ));
                }
                genotype_array[cur_pos] = Some(i);
                cur_pos += 1;
            }
        }

        let mut total_matches_tested = 0i32;
        let mut fail_count = 0i32;
        let mut match_fail_count = 0i32;
        let mut cpu = crate::cpu::create_test_cpu(host)?;
        let pick = |rng: &mut AvidaRng| -> Result<usize> {
            let idx = rng.get_uint(org_count as u32) as usize;
            genotype_array.get(idx).copied().flatten().ok_or_else(|| {
                AnalyzeError::fatal(
                    "ANALYZE_MATE_SELECTION: no organism at the drawn position (C++ dereferences past or into an unfilled array)",
                )
            })
        };
        for _ in 0..sample_size {
            let g1 = pick(world_rng(host)?)?;
            let g2 = pick(world_rng(host)?)?;
            if g1 == g2 {
                total_matches_tested += 1;
                continue;
            }
            let mut test_genome0 = self.batches[bi].list[g1].genome.clone();
            let test_genome1 = self.batches[bi].list[g2].genome.clone();
            let (mut start_frac, mut end_frac, mut swap_frac) = (-1.0f64, -1.0f64, -1.0f64);
            if min_swap_frac > max_swap_frac {
                return Err(AnalyzeError::fatal(format!(
                    "ANALYZE_MATE_SELECTION: minimum swap fraction {} leaves no valid swap (C++ loops forever)",
                    fmt_double(min_swap_frac)
                )));
            }
            let mut draws: u64 = 0;
            while swap_frac < min_swap_frac || swap_frac > max_swap_frac {
                let rng = world_rng(host)?;
                start_frac = rng.get_double();
                end_frac = rng.get_double();
                if start_frac > end_frac {
                    std::mem::swap(&mut start_frac, &mut end_frac);
                }
                swap_frac = end_frac - start_frac;
                draws += 1;
                if draws > MAX_BALANCE_ITERATIONS {
                    return Err(AnalyzeError::fatal(
                        "ANALYZE_MATE_SELECTION: no valid swap fraction drawn (C++ loops forever)",
                    ));
                }
            }
            let size0 = test_genome0.representation().size();
            let size1 = test_genome1.representation().size();
            let start0 = (start_frac * size0 as f64) as i32;
            let end0 = (end_frac * size0 as f64) as i32;
            let swap0 = end0 - start0;
            let start1 = (start_frac * size1 as f64) as i32;
            let end1 = (end_frac * size1 as f64) as i32;
            let swap1 = end1 - start1;
            let new_size0 = size0 - swap0 + swap1;
            let new_size1 = size1 - swap1 + swap0;
            let same_mate_id =
                self.batches[bi].list[g1].mate_id == self.batches[bi].list[g2].mate_id;
            if same_mate_id {
                total_matches_tested += 1;
            }
            if !(MIN_GENOME_LENGTH..=MAX_GENOME_LENGTH).contains(&new_size0)
                || !(MIN_GENOME_LENGTH..=MAX_GENOME_LENGTH).contains(&new_size1)
            {
                fail_count += 1;
                if same_mate_id {
                    match_fail_count += 1;
                }
                continue;
            }
            let cross1 = test_genome1.representation().crop(start1, end1);
            test_genome0
                .representation_mut()
                .replace(start0, swap0, &cross1);
            let mut info = CpuTestInfo::default();
            let genome: &Genome = &test_genome0;
            host.with_test_world(TestContext::Default, &mut |w| {
                hw(cpu.test_genome(w, &mut info, genome)).map(|_| ())
            })?;
            if !info.is_viable() {
                fail_count += 1;
                if same_mate_id {
                    match_fail_count += 1;
                }
            }
        }

        let num_mate_groups = mate_ids.len() as i32;
        let mut max_group_size = 0;
        let mut mate_id_entropy = 0.0f64;
        let mut order: Vec<usize> = (0..mate_ids.len()).collect();
        order.sort_by_key(|&i| (mate_ids[i].0 % 23).abs());
        for i in order {
            let cur_count = mate_ids[i].1;
            let cur_frac = cur_count as f64 / org_count as f64;
            if cur_count > max_group_size {
                max_group_size = cur_count;
            }
            mate_id_entropy = (-cur_frac).mul_add(cur_frac.ln(), mate_id_entropy);
        }
        let fail_frac = fail_count as f64 / sample_size as f64;
        let match_fail_frac = match_fail_count as f64 / total_matches_tested as f64;
        writeln!(
            self.out,
            "  ave fraction failed = {}",
            fmt_double(fail_frac)
        )?;
        writeln!(
            self.out,
            "  ave matches failed = {}",
            fmt_double(match_fail_frac)
        )?;
        writeln!(
            self.out,
            "  total mate matches = {total_matches_tested} / {sample_size}"
        )?;
        if filename == "none" {
            return Ok(());
        }
        let df = static_file(host, &filename)?;
        df.write_comment("Mate selection information");
        df.write_time_stamp();
        df.write(fail_frac, "Average fraction failed");
        df.write(match_fail_frac, "Average fraction of mate matches failed");
        df.write(sample_size, "Total number of crossovers tested");
        df.write(
            total_matches_tested,
            "Number of crossovers with matching mate IDs",
        );
        df.write(gen_count, "Number of genotypes in test batch");
        df.write(org_count, "Number of organisms in test batch");
        df.write(num_mate_groups, "Number of distinct mate IDs");
        df.write(max_group_size, "Size of the largest distinct mate ID group");
        df.write(mate_id_entropy, "Diversity of mate IDs (entropy)");
        df.endl();
        Ok(())
    }
}

/// `m_world->GetRandom()`.
fn world_rng(host: &mut dyn AnalyzeHost) -> Result<&mut AvidaRng> {
    host.world_rng()
        .ok_or_else(|| AnalyzeError::unsupported("world RNG", "cWorld::GetRandom"))
}

/// What `PRINT_RESOURCE_FITNESS_MAP` reads of one reaction.
struct MapReaction {
    name: String,
    /// `GetTask()->GetArguments()`: function, maxFx, minFx, thresh,
    /// threshMax; `None` for a task without arguments.
    args: Option<(i32, f64, f64, f64, f64)>,
    /// Process 0: its resource's name (`None` for an infinite resource),
    /// max fraction, max and min number; `None` without processes.
    process: Option<(Option<String>, f64, f64, f64)>,
    value: f64,
}

/// `GetArguments().GetInt(0)` etc., where the C++ dereferences the NULL
/// argument container of a task without arguments.
fn map_args(r: &MapReaction) -> Result<(i32, f64, f64, f64, f64)> {
    r.args.ok_or_else(|| {
        AnalyzeError::fatal(format!(
            "PRINT_RESOURCE_FITNESS_MAP: the task of reaction {} has no arguments (C++ dereferences NULL)",
            r.name
        ))
    })
}

impl Analyze {
    // -- PRINT_RESOURCE_FITNESS_MAP -----------------------------------------

    /// `CommandPrintResourceFitnessMap [update [file]]`: for a two-function
    /// `optimize` environment, the grid of fitness multipliers (`2^merit`
    /// per reaction) over the function ranges, given the resource levels
    /// of `update` in the loaded resource history.
    ///
    /// QUIRK: the box positions of the second function are offset by the
    /// first function's minimum.
    pub(crate) fn command_print_resource_fitness_map(
        &mut self,
        host: &mut dyn AnalyzeHost,
        mut cur_string: String,
    ) -> Result<()> {
        writeln!(self.out, "creating resource fitness map...")?;
        let mut update = 10000;
        if !cur_string.is_empty() {
            update = su::as_int(&su::pop_word(&mut cur_string));
        }
        let mut filename = "resourcefitmap.dat".to_string();
        if !cur_string.is_empty() {
            filename = su::pop_word(&mut cur_string);
        }
        static_file(host, &filename)?;

        let mut reactions: Vec<MapReaction> = Vec::new();
        let mut resource_names: Vec<String> = Vec::new();
        host.with_test_world(TestContext::Default, &mut |w| {
            let env = &*w.environment;
            let res_lib = env.resource_lib();
            resource_names = (0..res_lib.size().max(0) as usize)
                .map(|i| res_lib.resource(i).name().to_string())
                .collect();
            let lib = env.reaction_lib();
            reactions = lib
                .reactions()
                .iter()
                .map(|r| {
                    let args = r
                        .task
                        .and_then(|t| env.tasklib().0.get_task(t.max(0) as usize))
                        .and_then(|t| t.arguments())
                        .map(|a| {
                            (
                                a.get_int(0),
                                a.get_double(1),
                                a.get_double(2),
                                a.get_double(3),
                                a.get_double(4),
                            )
                        });
                    let process = r.processes.first().map(|p| {
                        (
                            p.resource.map(|id| res_lib.resource(id).name().to_string()),
                            p.max_fraction,
                            p.max_number,
                            p.min_number,
                        )
                    });
                    MapReaction {
                        name: r.name().to_string(),
                        args,
                        process,
                        value: r.value(0),
                    }
                })
                .collect();
            Ok(())
        })?;

        let (mut f1, mut f2) = (-1, -1);
        let (mut rangecount, mut threshcount) = ([0i32; 2], [0i32; 2]);
        let (mut f1_max, mut f1_min, mut f2_max, mut f2_min) = (0.0f64, 0.0f64, 0.0f64, 0.0f64);
        for (i, r) in reactions.iter().enumerate() {
            let (fun, max_fx, min_fx, thresh, thresh_max) = map_args(r)?;
            if i == 0 {
                f1 = fun;
                f1_max = max_fx;
                f1_min = min_fx;
            }
            if fun == f1 && thresh_max > 0.0 {
                rangecount[0] += 1;
            } else if fun == f1 && thresh >= 0.0 {
                threshcount[0] += 1;
            } else if fun != f1 && threshcount[1] == 0 && rangecount[1] == 0 {
                f2 = fun;
                f2_max = max_fx;
                f2_min = min_fx;
            }
            if fun == f2 && thresh_max > 0.0 {
                rangecount[1] += 1;
            } else if fun == f2 && thresh >= 0.0 {
                threshcount[1] += 1;
            }
        }
        let fsize = [
            rangecount[0].max(threshcount[0]),
            rangecount[1].max(threshcount[1]),
        ];
        writeln!(self.out, "f1 size: {}  f2 size: {}", fsize[0], fsize[1])?;
        let stepsize = [
            (f1_max - f1_min) / f64::from(fsize[0]),
            (f2_max - f2_min) / f64::from(fsize[1]),
        ];
        let mut fitnesses = vec![vec![1.0f64; fsize[1] as usize + 1]; fsize[0] as usize + 1];

        let levels = self
            .resources
            .as_ref()
            .and_then(|h| h.resource_levels_for_update(update, true));
        let Some(levels) = levels else {
            writeln!(
                self.out,
                "error: did not find the desired update in resource history"
            )?;
            return Ok(());
        };
        writeln!(self.out, "creating map using resources at update: {update}")?;

        for (i, res_name) in resource_names.iter().enumerate() {
            let mut react: Option<&MapReaction> = None;
            for r in &reactions {
                let process_resource = match &r.process {
                    Some((Some(n), ..)) => n,
                    Some((None, ..)) => {
                        return Err(AnalyzeError::fatal(format!(
                            "PRINT_RESOURCE_FITNESS_MAP: reaction {} uses an infinite resource (C++ dereferences its NULL resource)",
                            r.name
                        )))
                    }
                    None => {
                        return Err(AnalyzeError::fatal(format!(
                            "PRINT_RESOURCE_FITNESS_MAP: reaction {} has no process (C++ dereferences NULL)",
                            r.name
                        )))
                    }
                };
                if process_resource == res_name {
                    react = Some(r);
                    break;
                }
            }
            let Some(r) = react else {
                continue;
            };
            let Some(&(_, frac, max, min)) = r.process.as_ref() else {
                continue;
            };
            let value = r.value;
            let (mut fun, _, _, thresh, thresh_max) = map_args(r)?;
            if fun == f1 {
                fun = 0;
            } else if fun == f2 {
                fun = 1;
            } else {
                writeln!(self.out, "function is neither f1 or f2! doh!")?;
            }
            let concentration = *levels.get(i).ok_or_else(|| {
                AnalyzeError::fatal(format!(
                    "PRINT_RESOURCE_FITNESS_MAP: the resource history has {} levels, resource {i} is read (C++ reads past the array)",
                    levels.len()
                ))
            })?;
            let mut mer = concentration * frac * value;
            if mer > max {
                mer = max;
            } else if mer < min {
                mer = 0.0;
            }
            if !(0..2).contains(&fun) {
                return Err(AnalyzeError::fatal(format!(
                    "PRINT_RESOURCE_FITNESS_MAP: reaction {} is on neither function (C++ reads fsize[{fun}] past the array)",
                    r.name
                )));
            }
            let factor = 2.0f64.powf(mer);
            let n0 = fsize[0] as usize + 1;
            if thresh_max > 0.0 {
                for k in 0..fsize[fun as usize] {
                    let kf = f64::from(k);
                    let ku = k as usize;
                    if fun == 0 {
                        let thresh_max_adjusted = thresh_max.mul_add(f1_max - f1_min, f1_min);
                        let thresh_adjusted = thresh.mul_add(f1_max - f1_min, f1_min);
                        let pos = stepsize[0].mul_add(kf, f1_min) + stepsize[0] / 2.0;
                        if thresh_adjusted <= pos && thresh_max_adjusted >= pos {
                            for v in fitnesses[ku + 1].iter_mut() {
                                *v *= factor;
                            }
                            if k == 0 {
                                for v in fitnesses[0].iter_mut() {
                                    *v *= factor;
                                }
                            }
                        }
                    } else {
                        let thresh_max_adjusted = thresh_max.mul_add(f2_max - f2_min, f2_min);
                        let thresh_adjusted = thresh.mul_add(f2_max - f2_min, f2_min);
                        let pos = stepsize[1].mul_add(kf, f1_min) + stepsize[1] / 2.0;
                        if thresh_adjusted <= pos && thresh_max_adjusted >= pos {
                            for row in fitnesses.iter_mut().take(n0) {
                                row[ku + 1] *= factor;
                            }
                            if k == 0 {
                                for row in fitnesses.iter_mut().take(n0) {
                                    row[0] *= factor;
                                }
                            }
                        }
                    }
                }
            } else if thresh >= 0.0 {
                for k in 0..=fsize[fun as usize] {
                    let kf = f64::from(k);
                    let ku = k as usize;
                    if fun == 0 {
                        let thresh_adjusted = thresh.mul_add(f1_max - f1_min, f1_min);
                        let pos = stepsize[0].mul_add(kf, f1_min) - stepsize[0] / 2.0;
                        if thresh_adjusted >= pos {
                            for v in fitnesses[ku].iter_mut() {
                                *v *= factor;
                            }
                        }
                    } else {
                        let thresh_adjusted = thresh.mul_add(f2_max - f2_min, f2_min);
                        let pos = stepsize[1].mul_add(kf, f1_min) - stepsize[1] / 2.0;
                        if thresh_adjusted >= pos {
                            for row in fitnesses.iter_mut().take(n0) {
                                row[ku] *= factor;
                            }
                        }
                    }
                }
            }
        }

        let mut text = String::new();
        for i in (0..fitnesses[0].len()).rev() {
            for row in &fitnesses {
                text.push_str(&format!("{} ", fmt_double(row[i])));
            }
            text.push('\n');
        }
        let df = static_file(host, &filename)?;
        df.ofstream().write_all(text.as_bytes())?;
        df.flush();
        Ok(())
    }
}
