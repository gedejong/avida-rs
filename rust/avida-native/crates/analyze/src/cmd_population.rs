//! Batch-level commands of `cAnalyze` (analyze/cAnalyze.cc at 47f13dadb)
//! that compare, count, recombine or write out the genotypes of a batch:
//! `HAMMING`, `LEVENSTEIN`, `SPECIES`, `RECOMBINE`, `RECOMBINE_SAMPLE`,
//! `MUTAGENIZE`, `PRINT_DISTANCES`, `PRINT_DIVERSITY`, `MAP_DEPTH`,
//! `WRITE_CLONE`, `WRITE_INJECT_EVENTS`, `WRITE_INJECT_INITIAL`,
//! `WRITE_COMPETITION`, `ANALYZE_INSTRUCTIONS`, `ANALYZE_INST_POP` and
//! `ANALYZE_MUTATION_TRACEBACK`.

use std::io::Write;

use avida_apto::{fmt_double, format_c, DataFile};
use avida_config::string_util as su;
use avida_genome::{Genome, Instruction, InstructionSequence};
use avida_hardware::nhardware::MAX_GENOME_LENGTH;
use avida_hardware::test_cpu::CpuTestInfo;

use crate::analyze::Analyze;
use crate::cmd_output::is_html;
use crate::error::{AnalyzeError, Result};
use crate::genotype::{AnalyzeGenotype, MIN_GENOME_LENGTH};
use crate::host::{AnalyzeHost, TestContext};
use crate::landscape::{inst_set_of, shared_inst_set_of};
use crate::verbose::{VERBOSE_NORMAL, VERBOSE_ON};

fn hw<T>(r: std::result::Result<T, String>) -> Result<T> {
    r.map_err(AnalyzeError::world)
}

/// `Avida::Output::File::StaticWithPath(world, path)`.
pub(crate) fn static_file<'h>(
    host: &'h mut dyn AnalyzeHost,
    path: &str,
) -> Result<&'h mut DataFile> {
    host.output()
        .static_with_path(path)
        .map_err(|e| AnalyzeError::fatal(format!("{e} (C++: File::StaticWithPath returns NULL)")))
}

/// Append raw text to the static file at `path` (`df->OFStream() << ...`).
fn static_raw(host: &mut dyn AnalyzeHost, path: &str, text: &str) -> Result<()> {
    let df = static_file(host, path)?;
    df.ofstream().write_all(text.as_bytes())?;
    df.flush();
    Ok(())
}

/// The world RNG (`m_world->GetRandom()`, also the RNG of `m_ctx`).
fn world_rng(host: &mut dyn AnalyzeHost) -> Result<&mut avida_apto::AvidaRng> {
    host.world_rng()
        .ok_or_else(|| AnalyzeError::unsupported("world RNG", "cWorld::GetRandom"))
}

/// `genome[i].GetOp()` for `i < length`, where `length` is the genotype's
/// recorded length; the C++ reads past the sequence when it is longer.
fn ops_up_to(g: &AnalyzeGenotype, length: i32, what: &str) -> Result<Vec<i32>> {
    let seq = g.genome.representation();
    if length > seq.size() {
        return Err(AnalyzeError::fatal(format!(
            "{what}: genotype {} has length {length} but a {}-site genome (C++ reads past the sequence)",
            g.id_num,
            seq.size()
        )));
    }
    Ok((0..length.max(0)).map(|i| seq.get(i).op()).collect())
}

/// The fractions and sites of one crossover (`SPECIES`, `RECOMBINE`,
/// `RECOMBINE_SAMPLE`): two `GetDouble()` draws, sorted.
struct Cross {
    start0: i32,
    end0: i32,
    start1: i32,
    end1: i32,
}

impl Cross {
    fn draw(host: &mut dyn AnalyzeHost, size0: i32, size1: i32) -> Result<Self> {
        let rng = world_rng(host)?;
        let mut start_frac = rng.get_double();
        let mut end_frac = rng.get_double();
        if start_frac > end_frac {
            std::mem::swap(&mut start_frac, &mut end_frac);
        }
        Ok(Cross {
            start0: (start_frac * f64::from(size0)) as i32,
            end0: (end_frac * f64::from(size0)) as i32,
            start1: (start_frac * f64::from(size1)) as i32,
            end1: (end_frac * f64::from(size1)) as i32,
        })
    }

    /// Whether both offspring would have a legal length.
    fn legal(&self, size0: i32, size1: i32) -> bool {
        let s0 = self.end0 - self.start0;
        let s1 = self.end1 - self.start1;
        let new0 = size0 - s0 + s1;
        let new1 = size1 - s1 + s0;
        let ok = |n: i32| (MIN_GENOME_LENGTH..=MAX_GENOME_LENGTH).contains(&n);
        ok(new0) && ok(new1)
    }
}

impl Analyze {
    /// `PopBatch(in_string)`: the current batch unless a number is given.
    pub(crate) fn pop_batch(&self, in_string: &str) -> i32 {
        if !in_string.is_empty() && in_string != "current" {
            su::as_int(in_string)
        } else {
            self.cur_batch
        }
    }

    /// `CommandHamming` / `CommandLevenstein`: the average distance over
    /// all organism pairs of two batches.
    pub(crate) fn command_pair_distance(
        &mut self,
        host: &mut dyn AnalyzeHost,
        mut cur_string: String,
        edit: bool,
    ) -> Result<()> {
        let (default, label) = if edit {
            ("lev.dat", "Levenstein")
        } else {
            ("hamming.dat", "Hamming")
        };
        let mut filename = default.to_string();
        if !cur_string.is_empty() {
            filename = su::pop_word(&mut cur_string);
        }
        let mut batch1 = self.pop_batch(&su::pop_word(&mut cur_string));
        let mut batch2 = self.pop_batch(&su::pop_word(&mut cur_string));
        let (b1, b2) = (self.batch_index(batch1)?, self.batch_index(batch2)?);
        if self.batches[b1].list.len() > self.batches[b2].list.len() {
            std::mem::swap(&mut batch1, &mut batch2);
        }
        if Self::verbosity(host) <= VERBOSE_NORMAL {
            write!(self.out, "Calculating {label} Distance... ")?;
        } else if edit {
            writeln!(
                self.out,
                "Calculating {label} Distance between batch {batch1} and {batch2}"
            )?;
        } else {
            writeln!(
                self.out,
                "Calculating {label} Distance between batches {batch1} and {batch2}"
            )?;
        }
        let (b1, b2) = (batch1 as usize, batch2 as usize);
        let mut total_dist = 0.0f64;
        let mut total_count = 0.0f64;
        let list1 = &self.batches[b1].list;
        let list2 = &self.batches[b2].list;
        for (i, g1) in list1.iter().enumerate() {
            for (j, g2) in list2.iter().enumerate() {
                let (c1, c2) = (g1.num_cpus, g2.num_cpus);
                let num_pairs = if b1 == b2 && i == j {
                    (c1 - 1).wrapping_mul(c2 - 1)
                } else {
                    c1.wrapping_mul(c2)
                };
                if num_pairs == 0 {
                    continue;
                }
                let (s1, s2) = (g1.genome.representation(), g2.genome.representation());
                let dist = if edit {
                    InstructionSequence::find_edit_distance(s1, s2)
                } else {
                    InstructionSequence::find_hamming_distance(s1, s2, 0)
                };
                total_dist += f64::from(dist.wrapping_mul(num_pairs));
                total_count += f64::from(num_pairs);
            }
        }
        let ave_dist = total_dist / total_count;
        writeln!(self.out, " ave distance = {}", fmt_double(ave_dist))?;

        let (n1, n2) = (self.batches[b1].name.clone(), self.batches[b2].name.clone());
        let df = static_file(host, &filename)?;
        df.write_comment(&format!("{label} distance information"));
        df.write_time_stamp();
        df.write(n1.as_str(), "Name of First Batch");
        df.write(n2.as_str(), "Name of Second Batch");
        df.write(ave_dist, &format!("Average {label} Distance"));
        df.write(total_count, "Total Pairs Test");
        df.endl();
        Ok(())
    }

    /// `CommandSpecies`: the fraction of non-viable recombinants between
    /// two batches.
    ///
    /// QUIRK: the number of comparisons is read with `PopBatch`, so it
    /// defaults to the current batch id; only `num_compare - 1` crossovers
    /// are tested per pair, and an illegal crossover length ends the pair.
    pub(crate) fn command_species(
        &mut self,
        host: &mut dyn AnalyzeHost,
        mut cur_string: String,
    ) -> Result<()> {
        let mut filename = "species.dat".to_string();
        if !cur_string.is_empty() {
            filename = su::pop_word(&mut cur_string);
        }
        let mut batch1 = self.pop_batch(&su::pop_word(&mut cur_string));
        let mut batch2 = self.pop_batch(&su::pop_word(&mut cur_string));
        let num_compare = self.pop_batch(&su::pop_word(&mut cur_string));
        let (b1, b2) = (self.batch_index(batch1)?, self.batch_index(batch2)?);
        if self.batches[b1].list.len() > self.batches[b2].list.len() {
            std::mem::swap(&mut batch1, &mut batch2);
        }
        if Self::verbosity(host) <= VERBOSE_NORMAL {
            writeln!(self.out, "Calculating Species Distance... ")?;
        } else {
            writeln!(
                self.out,
                "Calculating Species Distance between batch {batch1} and {batch2}"
            )?;
        }
        let (b1, b2) = (batch1 as usize, batch2 as usize);
        let mut total_fail: i32 = 0;
        let mut total_count: i32 = 0;
        let mut cpu = crate::cpu::create_test_cpu(host)?;
        let n1 = self.batches[b1].list.len();
        let n2 = self.batches[b2].list.len();
        for i in 0..n1 {
            for j in 0..n2 {
                let g1 = &self.batches[b1].list[i];
                let g2 = &self.batches[b2].list[j];
                let num_pairs = g1.num_cpus.wrapping_mul(g2.num_cpus);
                if b1 == b2 && i == j {
                    total_count = total_count
                        .wrapping_add(num_pairs.wrapping_mul(2).wrapping_mul(num_compare));
                    continue;
                }
                let (genome1, genome2) = (g1.genome.clone(), g2.genome.clone());
                let mut fail_count: i32 = 0;
                for _iter in 1..num_compare {
                    let mut t0 = genome1.clone();
                    let mut t1 = genome2.clone();
                    let (size0, size1) = (t0.representation().size(), t1.representation().size());
                    let c = Cross::draw(host, size0, size1)?;
                    if !c.legal(size0, size1) {
                        fail_count += 2;
                        break;
                    }
                    let cross0 = t0.representation().crop(c.start0, c.end0);
                    let cross1 = t1.representation().crop(c.start1, c.end1);
                    t0.representation_mut()
                        .replace(c.start0, c.end0 - c.start0, &cross1);
                    t1.representation_mut()
                        .replace(c.start1, c.end1 - c.start1, &cross0);
                    let mut viable = [true; 2];
                    let mut info = CpuTestInfo::default();
                    for (k, g) in [&t0, &t1].into_iter().enumerate() {
                        host.with_test_world(TestContext::Default, &mut |w| {
                            hw(cpu.test_genome(w, &mut info, g)).map(|_| ())
                        })?;
                        viable[k] = info.is_viable();
                    }
                    if !viable[0] {
                        fail_count += 1;
                    }
                    if !viable[1] {
                        fail_count += 1;
                    }
                }
                total_fail = total_fail.wrapping_add(fail_count.wrapping_mul(num_pairs));
                total_count =
                    total_count.wrapping_add(num_pairs.wrapping_mul(2).wrapping_mul(num_compare));
            }
        }
        let ave_dist = f64::from(total_fail) / f64::from(total_count);
        writeln!(
            self.out,
            "  ave distance = {} in {total_count} tests.",
            fmt_double(ave_dist)
        )?;
        let (n1, n2) = (self.batches[b1].name.clone(), self.batches[b2].name.clone());
        let df = static_file(host, &filename)?;
        df.write_comment("Species information");
        df.write_time_stamp();
        df.write(n1.as_str(), "Name of First Batch");
        df.write(n2.as_str(), "Name of Second Batch");
        df.write(ave_dist, "Average Species Distance");
        df.write(total_count, "Total Recombinants tested");
        df.endl();
        Ok(())
    }

    /// Swap the crossover regions `c` of `t0` and `t1` the way `RECOMBINE`
    /// does (an empty region on one side inserts the other's).
    fn recombine_pair(t0: &mut Genome, t1: &mut Genome, c: &Cross) {
        let size0 = c.end0 - c.start0;
        let size1 = c.end1 - c.start1;
        if size0 > 0 && size1 > 0 {
            let cross0 = t0.representation().crop(c.start0, c.end0);
            let cross1 = t1.representation().crop(c.start1, c.end1);
            t0.representation_mut().replace(c.start0, size0, &cross1);
            t1.representation_mut().replace(c.start1, size1, &cross0);
        } else if size0 > 0 {
            let cross0 = t0.representation().crop(c.start0, c.end0);
            t1.representation_mut().replace(c.start1, size1, &cross0);
        } else if size1 > 0 {
            let cross1 = t1.representation().crop(c.start1, c.end1);
            t0.representation_mut().replace(c.start0, size0, &cross1);
        }
    }

    /// A recombinant as `CommandRecombine` builds it.
    fn recombinant(genome: Genome, parent1: i32, parent2: i32) -> AnalyzeGenotype {
        let mut g = AnalyzeGenotype::new(genome);
        g.num_cpus = 1;
        g.id_num = 0;
        g.name = "noname".to_string();
        g.set_parent_id(parent1);
        g.set_parent2_id(parent2);
        g
    }

    /// `CommandRecombine`: `num_compare - 1` recombinant pairs of every
    /// genotype pair of two batches, appended to a third.
    ///
    /// QUIRK: `num_compare` is read with `PopBatch`; an illegal crossover
    /// length ends the pair. When the target batch is one of the source
    /// batches the C++ iterators run on into the appended recombinants;
    /// the live list is walked the same way here.
    pub(crate) fn command_recombine(
        &mut self,
        host: &mut dyn AnalyzeHost,
        mut cur_string: String,
    ) -> Result<()> {
        let mut batch1 = self.pop_batch(&su::pop_word(&mut cur_string));
        let mut batch2 = self.pop_batch(&su::pop_word(&mut cur_string));
        let batch3 = self.pop_batch(&su::pop_word(&mut cur_string));
        let num_compare = self.pop_batch(&su::pop_word(&mut cur_string));
        let (b1, b2) = (self.batch_index(batch1)?, self.batch_index(batch2)?);
        let b3 = self.batch_index(batch3)?;
        if self.batches[b1].list.len() > self.batches[b2].list.len() {
            std::mem::swap(&mut batch1, &mut batch2);
        }
        if Self::verbosity(host) <= VERBOSE_NORMAL {
            writeln!(self.out, "Creating recombinants...  ")?;
        } else {
            writeln!(
                self.out,
                "Creating recombinants between batch {batch1} and {batch2}"
            )?;
        }
        let (b1, b2) = (batch1 as usize, batch2 as usize);
        let grows = (b3 == b1 || b3 == b2) && num_compare > 1;
        let mut i = 0;
        while i < self.batches[b1].list.len() {
            let mut j = 0;
            while j < self.batches[b2].list.len() {
                if grows && self.batches[b3].list.len() > 50_000_000 {
                    return Err(AnalyzeError::fatal(
                        "RECOMBINE into one of its source batches: the C++ iterates the recombinants it appends and never ends",
                    ));
                }
                let g1 = &self.batches[b1].list[i];
                let g2 = &self.batches[b2].list[j];
                let (genome1, id1) = (g1.genome.clone(), g1.id_num);
                let (genome2, id2) = (g2.genome.clone(), g2.id_num);
                for _iter in 1..num_compare {
                    let mut t0 = genome1.clone();
                    let mut t1 = genome2.clone();
                    let (size0, size1) = (t0.representation().size(), t1.representation().size());
                    let c = Cross::draw(host, size0, size1)?;
                    if !c.legal(size0, size1) {
                        break;
                    }
                    Self::recombine_pair(&mut t0, &mut t1, &c);
                    self.batches[b3].list.push(Self::recombinant(t0, id1, id2));
                    self.batches[b3].list.push(Self::recombinant(t1, id1, id2));
                }
                j += 1;
            }
            i += 1;
        }
        Ok(())
    }

    /// `CommandRecombineSample`: `num_compare` recombinants of random
    /// genotypes of two batches (one offspring each), appended to a third.
    /// QUIRK: an illegal crossover length ends the whole command.
    pub(crate) fn command_recombine_sample(
        &mut self,
        host: &mut dyn AnalyzeHost,
        mut cur_string: String,
    ) -> Result<()> {
        let batch1 = self.pop_batch(&su::pop_word(&mut cur_string));
        let batch2 = self.pop_batch(&su::pop_word(&mut cur_string));
        let batch3 = self.pop_batch(&su::pop_word(&mut cur_string));
        let num_compare = self.pop_batch(&su::pop_word(&mut cur_string));
        if Self::verbosity(host) <= VERBOSE_NORMAL {
            writeln!(self.out, "Creating recombinants...  ")?;
        } else {
            writeln!(
                self.out,
                "Creating recombinants between batch {batch1} and {batch2}"
            )?;
        }
        let (b1, b2, b3) = (
            self.batch_index(batch1)?,
            self.batch_index(batch2)?,
            self.batch_index(batch3)?,
        );
        for _ in 1..=num_compare {
            let pick =
                |this: &Self, b: usize, host: &mut dyn AnalyzeHost| -> Result<(Genome, i32)> {
                    let n = this.batches[b].list.len();
                    if n == 0 {
                        return Err(AnalyzeError::fatal(
                            "RECOMBINE_SAMPLE from an empty batch (C++ NULL dereference)",
                        ));
                    }
                    let pos = world_rng(host)?.get_uint(n as u32) as usize;
                    let g = &this.batches[b].list[pos];
                    Ok((g.genome.clone(), g.id_num))
                };
            let mut p1 = pick(self, b1, host)?;
            let mut p2 = pick(self, b2, host)?;
            if world_rng(host)?.p(0.5) {
                std::mem::swap(&mut p1, &mut p2);
            }
            let (mut t0, id1) = p1;
            let (mut t1, id2) = p2;
            let (size0, size1) = (t0.representation().size(), t1.representation().size());
            let c = Cross::draw(host, size0, size1)?;
            if !c.legal(size0, size1) {
                break;
            }
            Self::recombine_pair(&mut t0, &mut t1, &c);
            self.batches[b3].list.push(Self::recombinant(t0, id1, id2));
        }
        Ok(())
    }

    /// `CommandMutagenize`: one random point mutation per genotype of the
    /// current batch, drawn from the world RNG (`m_ctx`).
    pub(crate) fn command_mutagenize(&mut self, host: &mut dyn AnalyzeHost) -> Result<()> {
        let bi = self.batch_index(self.cur_batch)?;
        for gi in 0..self.batches[bi].list.len() {
            let genome = &self.batches[bi].list[gi].genome;
            let size = genome.representation().size();
            let is = shared_inst_set_of(host.inst_sets(), genome)?;
            if size <= 0 {
                return Err(AnalyzeError::fatal(
                    "MUTAGENIZE of an empty genome (C++ writes before the sequence)",
                ));
            }
            let rng = world_rng(host)?;
            let line_num = rng.get_int(size);
            let inst = is.random_inst(rng);
            self.batches[bi].list[gi]
                .genome
                .representation_mut()
                .set(line_num, inst);
        }
        Ok(())
    }

    /// `CommandPrintDistances`: all-pairs edit distance of the current
    /// batch, weighted by organism counts.
    pub(crate) fn command_print_distances(
        &mut self,
        host: &mut dyn AnalyzeHost,
        mut cur_string: String,
    ) -> Result<()> {
        writeln!(
            self.out,
            "Calculating Edit Distance between all pairs of genotypes."
        )?;
        let dist_threshold = su::as_int(&su::pop_word(&mut cur_string));
        let mut filename = su::pop_word(&mut cur_string);
        if filename.is_empty() {
            filename = "edit_distance.dat".to_string();
        }
        let mut fout = String::new();
        fout.push_str("# All pairs edit distance\n");
        fout.push_str("# 1: Num organism pairs\n");
        fout.push_str("# 2: Mean distance computed using (n*(n-1)/2) as all pairs.\n");
        fout.push_str("# 3: Mean distance\n");
        fout.push_str("# 4: Max distance\n");
        fout.push_str(&format!(
            "# 5: Frac distances above threshold ({dist_threshold})\n"
        ));
        fout.push('\n');

        let bi = self.batch_index(self.cur_batch)?;
        let mut dist_total: i32 = 0;
        let mut dist_max: i32 = 0;
        let mut pair_count: i32 = 0;
        let mut threshold_pair_count: i32 = 0;
        let mut count = 0.0f64;
        let mut watermark: i32 = 0;
        let list = &self.batches[bi].list;
        for (i, g1) in list.iter().enumerate() {
            count += 1.0;
            let c1 = g1.num_cpus;
            pair_count = pair_count.wrapping_add(c1.wrapping_mul(c1 - 1) / 2);
            for g2 in &list[i + 1..] {
                let cur_pairs = c1.wrapping_mul(g2.num_cpus);
                let cur_dist = InstructionSequence::find_edit_distance(
                    g1.genome.representation(),
                    g2.genome.representation(),
                );
                dist_total = dist_total.wrapping_add(cur_pairs.wrapping_mul(cur_dist));
                if cur_dist > dist_max {
                    dist_max = cur_dist;
                }
                pair_count = pair_count.wrapping_add(cur_pairs);
                if cur_dist >= dist_threshold {
                    threshold_pair_count = threshold_pair_count.wrapping_add(cur_pairs);
                }
                if pair_count > watermark {
                    writeln!(self.out, "{watermark}")?;
                    watermark = watermark.wrapping_add(100_000);
                }
            }
        }
        count = (count * (count - 1.0)) / 2.0;
        fout.push_str(&format!(
            "{pair_count} {} {} {dist_max} {} \n",
            fmt_double(f64::from(dist_total) / count),
            fmt_double(f64::from(dist_total) / f64::from(pair_count)),
            fmt_double(f64::from(threshold_pair_count) / f64::from(pair_count)),
        ));
        static_raw(host, &filename, &fout)
    }

    /// `BatchUtil_GetMaxLength()` of the current batch.
    fn batch_max_length(&self, bi: usize) -> i32 {
        self.batches[bi]
            .list
            .iter()
            .fold(0, |m, g| if g.length > m { g.length } else { m })
    }

    /// `CommandPrintDiversity`: per task, how many organisms and genotypes
    /// perform it, their average per-site (Hamming-style) distance and
    /// per-site entropy.
    pub(crate) fn command_print_diversity(
        &mut self,
        host: &mut dyn AnalyzeHost,
        mut cur_string: String,
    ) -> Result<()> {
        if Self::verbosity(host) >= VERBOSE_ON {
            writeln!(
                self.out,
                "Printing diversity data for batch {}",
                self.cur_batch
            )?;
        } else {
            writeln!(self.out, "Printing diversity data...")?;
        }
        let mut filename = "diversity.dat".to_string();
        if !cur_string.is_empty() {
            filename = su::pop_word(&mut cur_string);
        }
        let bi = self.batch_index(self.cur_batch)?;
        let Some(first) = self.batches[bi].list.first() else {
            return Ok(());
        };
        let num_tasks = first.num_tasks();
        let registry = host.inst_sets();
        let is = registry
            .default_inst_set()
            .ok_or_else(|| AnalyzeError::fatal("PRINT_DIVERSITY without an instruction set"))?;
        let is_name = is.inst_set_name().to_string();
        let num_insts = is.size();
        let max_length = self.batch_max_length(bi);
        let cols = (num_insts + 1) as usize;
        let mut inst_freq = vec![0i32; max_length.max(0) as usize * cols];
        let mut rows: Vec<(i32, i32, f64, f64)> = Vec::new();
        for task_id in 0..num_tasks {
            inst_freq.iter_mut().for_each(|v| *v = 0);
            let mut task_count: i32 = 0;
            let mut task_gen_count: i32 = 0;
            for g in &self.batches[bi].list {
                if inst_set_of(registry, &g.genome)?.inst_set_name() != is_name
                    || g.task_count(task_id)? == 0
                {
                    continue;
                }
                let num_cpus = g.num_cpus;
                task_count = task_count.wrapping_add(num_cpus);
                task_gen_count += 1;
                let ops = ops_up_to(g, g.length, "PRINT_DIVERSITY")?;
                for (i, &op) in ops.iter().enumerate() {
                    if op < 0 || op > num_insts {
                        return Err(AnalyzeError::fatal(
                            "PRINT_DIVERSITY: instruction outside the default set (C++ tMatrix overrun)",
                        ));
                    }
                    let cell = &mut inst_freq[i * cols + op as usize];
                    *cell = cell.wrapping_add(num_cpus);
                }
                for i in g.length.max(0)..max_length {
                    let cell = &mut inst_freq[i as usize * cols + num_insts as usize];
                    *cell = cell.wrapping_add(num_cpus);
                }
            }
            let cur_count = task_count;
            let total_pairs = cur_count.wrapping_mul(cur_count - 1) / 2;
            let mut total_dist: i32 = 0;
            let mut total_ent = 0.0f64;
            for pos in 0..max_length.max(0) as usize {
                let row = &inst_freq[pos * cols..(pos + 1) * cols];
                for inst1 in 0..num_insts as usize {
                    if row[inst1] == 0 {
                        continue;
                    }
                    for &f2 in &row[inst1 + 1..] {
                        total_dist = total_dist.wrapping_add(row[inst1].wrapping_mul(f2));
                    }
                }
                for &cur_freq in row {
                    if cur_freq == 0 {
                        continue;
                    }
                    let p = f64::from(cur_freq) / f64::from(cur_count);
                    // `total_ent -= p * log(p)`, fused on the reference build.
                    total_ent = (-p).mul_add(p.ln(), total_ent);
                }
            }
            rows.push((
                task_count,
                task_gen_count,
                f64::from(total_dist) / f64::from(total_pairs),
                total_ent,
            ));
        }
        let df = static_file(host, &filename)?;
        for (i, (count, gen_count, dist, ent)) in rows.into_iter().enumerate() {
            df.write(i as i32, "# 1: Task ID");
            df.write(count, "# 2: Number of organisms performing task");
            df.write(gen_count, "# 3: Number of genotypes performing task");
            df.write(
                dist,
                "# 4: Average distance between genotypes performing task",
            );
            df.write(
                ent,
                "# 5: Total per-site entropy of genotypes performing task",
            );
            df.endl();
        }
        Ok(())
    }

    /// `CommandMapDepth`: organisms per phylogenetic depth, one line per
    /// batch in `min_batch..=max_batch`.
    ///
    /// QUIRK: the loop variable is `cur_batch` itself, so the command
    /// leaves the current batch at `max_batch + 1` (or at `min_batch` when
    /// the range is empty).
    pub(crate) fn command_map_depth(
        &mut self,
        host: &mut dyn AnalyzeHost,
        mut cur_string: String,
    ) -> Result<()> {
        writeln!(self.out, "Constructing depth map...")?;
        let mut filename = "depth_map.dat".to_string();
        if !cur_string.is_empty() {
            filename = su::pop_word(&mut cur_string);
        }
        let mut min_batch = 0;
        let mut max_batch = self.cur_batch - 1;
        if !cur_string.is_empty() {
            min_batch = su::as_int(&su::pop_word(&mut cur_string));
        }
        if !cur_string.is_empty() {
            max_batch = su::as_int(&su::pop_word(&mut cur_string));
        }
        let mut max_depth = -1;
        for b in min_batch..=max_batch {
            let bi = self.batch_index(b)?;
            for g in &self.batches[bi].list {
                if g.depth > max_depth {
                    max_depth = g.depth;
                }
            }
        }
        writeln!(self.out, "max_depth = {max_depth}")?;
        static_file(host, &filename)?;
        writeln!(self.out, "Output to {filename}")?;
        let mut text = String::new();
        self.cur_batch = min_batch;
        while self.cur_batch <= max_batch {
            let bi = self.batch_index(self.cur_batch)?;
            let mut depth_array = vec![0i32; (max_depth + 1) as usize];
            for g in &self.batches[bi].list {
                if g.depth < 0 {
                    return Err(AnalyzeError::fatal(
                        "MAP_DEPTH of a genotype with negative depth (C++ indexes before the array)",
                    ));
                }
                let d = &mut depth_array[g.depth as usize];
                *d = d.wrapping_add(g.num_cpus);
            }
            for d in &depth_array {
                text.push_str(&format!("{d} "));
            }
            text.push('\n');
            self.cur_batch += 1;
        }
        static_raw(host, &filename, &text)
    }

    /// `WriteClone`: a clone file of the current batch (archive sizes,
    /// genotypes, then the grid).
    pub(crate) fn write_clone(
        &mut self,
        host: &mut dyn AnalyzeHost,
        mut cur_string: String,
    ) -> Result<()> {
        let mut filename = "clone.dat".to_string();
        let mut num_cells = -1;
        if !cur_string.is_empty() {
            filename = su::pop_word(&mut cur_string);
        }
        if !cur_string.is_empty() {
            num_cells = su::as_int(&su::pop_word(&mut cur_string));
        }
        let bi = self.batch_index(self.cur_batch)?;
        let list = &self.batches[bi].list;
        let mut fp = format!("0 {MAX_GENOME_LENGTH} ");
        for _ in 0..MAX_GENOME_LENGTH {
            fp.push_str("0 ");
        }
        fp.push_str(&format!("{} ", list.len()));
        let mut org_count: i32 = 0;
        for g in list {
            org_count = org_count.wrapping_add(g.num_cpus);
            fp.push_str(&format!("{} {} ", g.id_num, g.length));
            for op in ops_up_to(g, g.length, "WRITE_CLONE")? {
                fp.push_str(&format!("{op} "));
            }
        }
        if num_cells == 0 {
            num_cells = org_count;
        }
        fp.push_str(&format!("{num_cells} "));
        for g in list {
            for _ in 0..g.num_cpus {
                fp.push_str(&format!("{} ", g.id_num));
            }
        }
        for _ in org_count..num_cells {
            fp.push_str("-1 ");
        }
        static_raw(host, &filename, &fp)
    }

    /// `WriteInjectEvents` (`initial == false`) / `WriteInjectInitial`:
    /// one `InjectSequence` event per genotype of the current batch.
    pub(crate) fn write_inject(
        &mut self,
        host: &mut dyn AnalyzeHost,
        mut cur_string: String,
        initial: bool,
    ) -> Result<()> {
        let mut filename = "events_inj.cfg".to_string();
        let mut start_cell: i32 = 0;
        let mut lineage = 0;
        if !cur_string.is_empty() {
            filename = su::pop_word(&mut cur_string);
        }
        if !initial && !cur_string.is_empty() {
            start_cell = su::as_int(&su::pop_word(&mut cur_string));
        }
        if !cur_string.is_empty() {
            lineage = su::as_int(&su::pop_word(&mut cur_string));
        }
        static_file(host, &filename)?;
        let bi = self.batch_index(self.cur_batch)?;
        let mut fp = String::new();
        for g in &self.batches[bi].list {
            let cur_count = g.num_cpus;
            let end = if initial {
                start_cell.wrapping_add(1)
            } else {
                start_cell.wrapping_add(cur_count)
            };
            fp.push_str(&format!(
                "{} InjectSequence {} {start_cell} {end} {} {lineage} \n",
                if initial { "i" } else { "u 0" },
                g.sequence(),
                fmt_double(g.merit)
            ));
            start_cell = start_cell.wrapping_add(cur_count);
        }
        static_raw(host, &filename, &fp)
    }

    /// `WriteCompetition`: inject events for two batches on either side of
    /// a severed grid, rejoined at update `join_UD`.
    ///
    /// QUIRK: the sever event is written as `SeverGridRow<side>` with no
    /// space.
    pub(crate) fn write_competition(
        &mut self,
        host: &mut dyn AnalyzeHost,
        mut cur_string: String,
    ) -> Result<()> {
        writeln!(self.out, "Writing Competition events...")?;
        let mut join_ud = 0;
        let mut start_merit = 50000.0;
        let mut filename = "events_comp.cfg".to_string();
        let mut batch_a = self.cur_batch - 1;
        let mut batch_b = self.cur_batch;
        let mut grid_side = -1;
        let mut lineage: i32 = 0;
        if self.cur_batch == 0 {
            batch_a = 0;
            batch_b = 1;
        }
        if !cur_string.is_empty() {
            join_ud = su::as_int(&su::pop_word(&mut cur_string));
        }
        if !cur_string.is_empty() {
            start_merit = su::as_double(&su::pop_word(&mut cur_string));
        }
        if !cur_string.is_empty() {
            filename = su::pop_word(&mut cur_string);
        }
        if !cur_string.is_empty() {
            batch_a = su::as_int(&su::pop_word(&mut cur_string));
        }
        if !cur_string.is_empty() {
            batch_b = su::as_int(&su::pop_word(&mut cur_string));
        }
        if !cur_string.is_empty() {
            grid_side = su::as_int(&su::pop_word(&mut cur_string));
        }
        if !cur_string.is_empty() {
            lineage = su::as_int(&su::pop_word(&mut cur_string));
        }
        if join_ud < 0 {
            join_ud = 0;
        }
        if batch_a < 0 || batch_b < 0 {
            writeln!(self.err, "Error: Batch IDs must be positive!")?;
            return Ok(());
        }
        static_file(host, &filename)?;
        let (ai, bi) = (self.batch_index(batch_a)?, self.batch_index(batch_b)?);
        let count = |l: &[AnalyzeGenotype]| l.iter().fold(0i32, |a, g| a.wrapping_add(g.num_cpus));
        let max_count = count(&self.batches[ai].list).max(count(&self.batches[bi].list));
        if max_count > 10000 {
            writeln!(
                self.out,
                "Warning: more than 10,000 organisms in sub-population!"
            )?;
        }
        if grid_side <= 0 {
            grid_side = 5;
            while grid_side < 100 {
                if grid_side * grid_side >= max_count {
                    break;
                }
                grid_side += 5;
            }
            if Self::verbosity(host) >= VERBOSE_ON {
                writeln!(
                    self.out,
                    "...assuming population size {grid_side}x{grid_side}."
                )?;
            }
        }
        let pop_size = grid_side * grid_side;
        let mut fp = String::new();
        for (b, mut inject_pos, lin) in [(ai, 0, lineage), (bi, pop_size, lineage.wrapping_add(1))]
        {
            for g in &self.batches[b].list {
                let cur_count = g.num_cpus;
                let mut cur_merit = start_merit;
                if cur_merit < 0.0 {
                    cur_merit = g.merit;
                }
                fp.push_str(&format!(
                    "u 0 InjectSequence {} {inject_pos} {} {} {lin} \n",
                    g.sequence(),
                    inject_pos.wrapping_add(cur_count),
                    fmt_double(cur_merit)
                ));
                inject_pos = inject_pos.wrapping_add(cur_count);
            }
        }
        fp.push_str(&format!("u 0 SeverGridRow{grid_side}\n"));
        fp.push_str(&format!("u {join_ud} JoinGridRow {grid_side}\n"));
        static_raw(host, &filename, &fp)
    }

    /// The instruction set `ANALYZE_INSTRUCTIONS` / `ANALYZE_INST_POP`
    /// count against: the default one or the named one.
    fn counted_inst_set(
        host: &dyn AnalyzeHost,
        cur_string: &mut String,
    ) -> Result<(String, Vec<String>)> {
        let registry = host.inst_sets();
        let mut isname = registry
            .default_inst_set()
            .map(|is| is.inst_set_name().to_string())
            .ok_or_else(|| AnalyzeError::fatal("no default instruction set"))?;
        if !cur_string.is_empty() {
            isname = su::pop_word(cur_string);
        }
        let is = registry.get_inst_set(&isname).ok_or_else(|| {
            AnalyzeError::fatal(format!(
                "no instruction set named '{isname}' (C++: cHardwareManager::GetInstSet reads past its map)"
            ))
        })?;
        let names = (0..is.size()).map(|i| is.name(i).to_string()).collect();
        Ok((isname, names))
    }

    /// Instruction frequencies of a genotype counted the way the two
    /// instruction analyses count them (over its recorded length).
    fn inst_frequencies(g: &AnalyzeGenotype, num_insts: usize) -> Result<Vec<f64>> {
        let mut inst_bin = vec![0i32; num_insts];
        for op in ops_up_to(g, g.length, "ANALYZE_INSTRUCTIONS")? {
            let b = inst_bin.get_mut(op as usize).ok_or_else(|| {
                AnalyzeError::fatal("instruction outside its set (C++ array overrun)")
            })?;
            *b += 1;
        }
        Ok(inst_bin
            .iter()
            .map(|&c| f64::from(c) / f64::from(g.length))
            .collect())
    }

    /// `AnalyzeInstructions`: per-genotype instruction frequencies, as text
    /// or (for a `.html` file name) a coloured HTML chart.
    pub(crate) fn analyze_instructions(
        &mut self,
        host: &mut dyn AnalyzeHost,
        mut cur_string: String,
    ) -> Result<()> {
        if Self::verbosity(host) >= VERBOSE_ON {
            writeln!(
                self.out,
                "Analyzing Instructions in batch {}",
                self.cur_batch
            )?;
        } else {
            writeln!(self.out, "Analyzing Instructions...")?;
        }
        let mut filename = "inst_analyze.dat".to_string();
        if !cur_string.is_empty() {
            filename = su::pop_word(&mut cur_string);
        }
        let (isname, names) = Self::counted_inst_set(host, &mut cur_string)?;
        let num_insts = names.len() as i32;
        static_file(host, &filename)?;
        let html = is_html(&filename);
        let bi = self.batch_index(self.cur_batch)?;
        let f43 = |v: f64| format_c("%04.3f", v).unwrap_or_default();
        let mut fp = String::new();
        if html {
            fp.push_str(&format!(
                "<html>\n<body bgcolor=\"#FFFFFF\"\n text=\"#000000\"\n link=\"#0000AA\"\n alink=\"#0000FF\"\n vlink=\"#000044\">\n\n<h1 align=center>Instruction Chart: {}\n<br><br>\n\n",
                self.batches[bi].name
            ));
            let num_cols = 6;
            let num_rows = ((num_insts - 1) / num_cols) + 1;
            fp.push_str("<table border=2 cellpadding=3>\n<tr bgcolor=\"#AAAAFF\"><th colspan=6>Instruction Set Legend</tr>\n");
            for i in 0..num_rows {
                fp.push_str("<tr>");
                for j in 0..num_cols {
                    let inst_id = i + j * num_rows;
                    if inst_id < num_insts {
                        fp.push_str(&format!(
                            "<td><b>{}</b> : {} ",
                            Instruction::new(inst_id).symbol(),
                            names[inst_id as usize]
                        ));
                    } else {
                        fp.push_str("<td>&nbsp; ");
                    }
                }
                fp.push_str("</tr>\n");
            }
            fp.push_str("</table>\n<br><br><br>\n");
            fp.push_str("<center>\n<table border=1 cellpadding=2>\n<tr><th bgcolor=\"#AAAAFF\">Run # <th bgcolor=\"#AAAAFF\">Length\n");
            for i in 0..num_insts {
                fp.push_str(&format!(
                    "<th bgcolor=\"#AAAAFF\">{} ",
                    Instruction::new(i).symbol()
                ));
            }
            fp.push_str("</tr>\n");
        } else {
            fp.push_str("#RUN_NAME  LENGTH  ");
            for (i, n) in names.iter().enumerate() {
                fp.push_str(&format!("{}:{n} ", Instruction::new(i as i32).symbol()));
            }
            fp.push('\n');
        }
        let exp_freq = 1.0 / f64::from(num_insts);
        let min_freq = exp_freq * 0.5;
        let max_freq = exp_freq * 2.0;
        let mut total_length = 0.0;
        let mut total_freq = vec![0.0f64; names.len()];
        for g in &self.batches[bi].list {
            if g.genome.inst_set() != isname {
                continue;
            }
            let freqs = Self::inst_frequencies(g, names.len())?;
            if html {
                fp.push_str("<tr><th>");
            }
            fp.push_str(&format!("{} ", g.name));
            if html {
                fp.push_str("<td align=center>");
            }
            total_length += f64::from(g.length);
            fp.push_str(&format!("{} ", g.length));
            for (i, &inst_freq) in freqs.iter().enumerate() {
                total_freq[i] += inst_freq;
                if html {
                    fp.push_str(if inst_freq == 0.0 {
                        "<td bgcolor=\"FFAAAA\">"
                    } else if inst_freq < min_freq {
                        "<td bgcolor=\"FFFFAA\">"
                    } else if inst_freq < max_freq {
                        "<td bgcolor=\"AAAAFF\">"
                    } else {
                        "<td bgcolor=\"AAFFAA\">"
                    });
                }
                fp.push_str(&format!("{} ", f43(inst_freq)));
            }
            if html {
                fp.push_str("</tr>");
            }
            fp.push('\n');
        }
        if html {
            let c = host.config();
            let num_genomes = self.batches[bi].list.len() as i32;
            fp.push_str(&format!(
                "<tr><th>Average <th>{} ",
                fmt_double(total_length / f64::from(num_genomes))
            ));
            for &t in &total_freq {
                let inst_freq = t / f64::from(num_genomes);
                let color = if inst_freq == 0.0 {
                    &c.color_mut_lethal
                } else if inst_freq < min_freq {
                    &c.color_mut_neg
                } else if inst_freq < max_freq {
                    &c.color_mut_neut
                } else {
                    &c.color_mut_pos
                };
                fp.push_str(&format!("<td bgcolor=\"#{color}\">{} ", f43(inst_freq)));
            }
            fp.push_str("</tr>\n</table></center>\n");
        }
        static_raw(host, &filename, &fp)
    }

    /// `AnalyzeInstPop`: the instruction frequencies averaged over the
    /// genotypes of the current batch.
    pub(crate) fn analyze_inst_pop(
        &mut self,
        host: &mut dyn AnalyzeHost,
        mut cur_string: String,
    ) -> Result<()> {
        if Self::verbosity(host) >= VERBOSE_ON {
            writeln!(
                self.out,
                "Analyzing Instructions in batch {}",
                self.cur_batch
            )?;
        } else {
            writeln!(self.out, "Analyzeing Instructions...")?;
        }
        let mut filename = "inst_analyze.dat".to_string();
        if !cur_string.is_empty() {
            filename = su::pop_word(&mut cur_string);
        }
        let (isname, names) = Self::counted_inst_set(host, &mut cur_string)?;
        static_file(host, &filename)?;
        let mut fp = String::new();
        for (i, n) in names.iter().enumerate() {
            fp.push_str(&format!("{}:{n} ", Instruction::new(i as i32).symbol()));
        }
        fp.push('\n');
        let bi = self.batch_index(self.cur_batch)?;
        let mut total_freq = vec![0.0f64; names.len()];
        let mut num_orgs = 0;
        for g in &self.batches[bi].list {
            if g.genome.inst_set() != isname {
                continue;
            }
            num_orgs += 1;
            for (t, f) in total_freq
                .iter_mut()
                .zip(Self::inst_frequencies(g, names.len())?)
            {
                *t += f;
            }
        }
        for t in &total_freq {
            fp.push_str(&format!(
                "{} ",
                format_c("%04.3f", t / f64::from(num_orgs)).unwrap_or_default()
            ));
        }
        fp.push('\n');
        static_raw(host, &filename, &fp)
    }

    /// `AnalyzeMutationTraceback`.
    ///
    /// QUIRK: the analysis loop starts with `continue`, so after its checks
    /// the command only creates the (empty) output file and a test CPU.
    pub(crate) fn analyze_mutation_traceback(
        &mut self,
        host: &mut dyn AnalyzeHost,
        mut cur_string: String,
    ) -> Result<()> {
        let verbose = Self::verbosity(host) >= VERBOSE_ON;
        if verbose {
            writeln!(
                self.out,
                "Analyzing mutation traceback in batch {}",
                self.cur_batch
            )?;
        } else {
            writeln!(self.out, "Analyzing mutation traceback...")?;
        }
        let bi = self.batch_index(self.cur_batch)?;
        if !self.batches[bi].is_lineage && verbose {
            writeln!(
                self.err,
                "  Warning: trying to traceback mutations outside of lineage."
            )?;
        }
        if self.batches[bi].list.is_empty() {
            writeln!(
                self.err,
                "Error: Trying to traceback mutations with no genotypes in batch."
            )?;
            return Ok(());
        }
        let size = self.batches[bi].list[0].length;
        if self.batches[bi].list.iter().any(|g| g.length != size) {
            writeln!(
                self.err,
                "  Error: Trying to traceback mutations in genotypes of differing lengths."
            )?;
            writeln!(self.err, "  Aborting.")?;
            return Ok(());
        }
        let mut filename = "analyze_traceback.dat".to_string();
        if !cur_string.is_empty() {
            filename = su::pop_word(&mut cur_string);
        }
        static_file(host, &filename)?;
        crate::cpu::create_test_cpu(host)?;
        Ok(())
    }
}
