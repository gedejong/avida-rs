//! The skeleton commands of `cAnalyze` (analyze/cAnalyze.cc at 47f13dadb):
//! `GET_SKELETONS` (`GetSkeletons_Batch`), `COUNT_NEW_SIG_LINEAGES`,
//! `COUNT_NOVEL_SKELETONS`, `CALC_SKELETON_DIVERSITY`,
//! `GET_LARGEST_SKELETON` and the research commands built on the same
//! helpers, `ALL_COMPLEXITY_BARRIERS`, `CHANGE_DEBUG` and
//! `COMPLEXITY_DEBUG`.
//!
//! A skeleton is a genome with every site whose knockout does not lower
//! fitness replaced by the NULL instruction (`GetSkeletons`). All commands
//! but `GET_SKELETONS` load `detail-<update>.spop` files themselves
//! (`LoadDetailFileAsVector`) and append one line to a `.csv` file in the
//! process's current directory (a plain `ofstream`, not the data
//! directory).
//!
//! `std::map<InstructionSequence, ...>` orders and identifies sequences by
//! `AsString()`; the maps here are keyed by that string and keep the first
//! sequence inserted under each key.

use std::collections::{BTreeMap, BTreeSet};
use std::io::Write;

use avida_apto::fmt_double;
use avida_config::string_util as su;
use avida_config::{Feedback, InitFile};
use avida_genome::{Genome, Instruction, InstructionSequence};
use avida_hardware::test_cpu::CpuTestInfo;

use crate::analyze::{file_lines, Analyze};
use crate::cmd_mutation::{recalc_in_place, recalc_new};
use crate::data_entry::load_command_list;
use crate::error::{AnalyzeError, Result};
use crate::genotype::AnalyzeGenotype;
use crate::host::AnalyzeHost;
use crate::landscape::inst_set_of;
use crate::verbose::VERBOSE_ON;

/// `atoi(s)`: base-10 `strtol` (saturating on a 64-bit `long`) truncated
/// to `int`, as the macOS libc implements it.
pub(crate) fn atoi(s: &str) -> i32 {
    let b = s.as_bytes();
    let mut i = 0;
    while i < b.len() && matches!(b[i], b' ' | b'\t' | b'\n' | 0x0b | 0x0c | b'\r') {
        i += 1;
    }
    let mut neg = false;
    if i < b.len() && (b[i] == b'+' || b[i] == b'-') {
        neg = b[i] == b'-';
        i += 1;
    }
    let mut acc: i128 = 0;
    while i < b.len() && b[i].is_ascii_digit() {
        acc = (acc * 10 + (b[i] - b'0') as i128).min(i64::MAX as i128 + 1);
        i += 1;
    }
    let v: i64 = if neg {
        (-acc).max(i64::MIN as i128) as i64
    } else {
        acc.min(i64::MAX as i128) as i64
    };
    v as i32
}

/// `"detail-" + update + ".spop"`.
fn detail_name(update: &str) -> String {
    format!("detail-{update}.spop")
}

/// `x -= x % interval` (integer division by zero raises SIGFPE in C++).
fn round_down(x: i32, interval: i32, what: &str) -> Result<i32> {
    match x.checked_rem(interval) {
        Some(r) => Ok(x - r),
        None => Err(AnalyzeError::fatal(format!(
            "{what}: interval {interval} (C++: integer division by zero or overflow in '%', SIGFPE)"
        ))),
    }
}

/// `ofstream f; f.open(name, ios_base::app); f << text; f.close()`; a file
/// that cannot be opened swallows the output, as the stream does.
fn append_cwd(name: &str, text: &str) {
    if let Ok(mut f) = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(name)
    {
        let _ = f.write_all(text.as_bytes());
    }
}

/// `std::map<InstructionSequence, int>`: keyed by `AsString()`, holding the
/// first sequence inserted under each key and its count.
type SeqCounts = BTreeMap<String, (InstructionSequence, i32)>;

/// `map[seq]++`.
fn bump(map: &mut SeqCounts, seq: &InstructionSequence) {
    map.entry(seq.as_string())
        .or_insert_with(|| (seq.clone(), 0))
        .1 += 1;
}

/// `MakeLineageVector`: for every genotype with living organisms, the
/// genotype followed by its ancestors found by following parent ids
/// (first genotype with a matching id) through the same vector. Lineages
/// are index lists into `genotypes`.
fn make_lineage_vector(genotypes: &[AnalyzeGenotype]) -> Result<Vec<Vec<usize>>> {
    let mut lineages = Vec::new();
    for (i, g) in genotypes.iter().enumerate() {
        if g.num_cpus <= 0 {
            continue;
        }
        let mut found_list = vec![i];
        let mut next_id = g.parent_id;
        while let Some(j) = genotypes.iter().position(|a| a.id_num == next_id) {
            found_list.push(j);
            next_id = genotypes[j].parent_id;
            // More ancestors than genotypes means an id repeated; the
            // first-match lookup then cycles for ever.
            if found_list.len() > genotypes.len() + 1 {
                return Err(AnalyzeError::fatal(format!(
                    "the parent ids of genotype {} form a cycle (C++: cAnalyze::MakeLineageVector loops forever)",
                    g.id_num
                )));
            }
        }
        lineages.push(found_list);
    }
    Ok(lineages)
}

/// For each lineage, the first of its genotypes whose sequence equals the
/// sequence of a living genotype in `coal`: count that coalescent sequence
/// (the `lineage1_map` / `lineage2_map` loops).
fn coalescent_counts(
    lineages: &[Vec<usize>],
    genotypes: &[AnalyzeGenotype],
    coal: &[AnalyzeGenotype],
) -> SeqCounts {
    let mut map = SeqCounts::new();
    for lineage in lineages {
        for &gi in lineage {
            let cur_seq = genotypes[gi].genome.representation();
            let hit = coal
                .iter()
                .filter(|c| c.num_cpus > 0)
                .map(|c| c.genome.representation())
                .find(|s| s.as_slice() == cur_seq.as_slice());
            if let Some(coal_seq) = hit {
                bump(&mut map, coal_seq);
                break;
            }
        }
    }
    map
}

/// The number of non-NULL sites of `genome`.
fn skeleton_size(genome: &Genome, null_inst: Instruction) -> i32 {
    genome
        .representation()
        .iter()
        .filter(|&&i| i != null_inst)
        .count() as i32
}

/// Read `seq[idx]` the way the C++ does without bounds checks: stale
/// sites past the active size are still in the allocated array; anything
/// outside the array is undefined behaviour.
fn read_site(seq: &InstructionSequence, idx: i32, what: &str) -> Result<Instruction> {
    if idx < 0 || idx >= seq.array_size() {
        return Err(AnalyzeError::fatal(format!(
            "{what}: reads site {idx} of a sequence allocated for {} sites (C++ reads outside the array)",
            seq.array_size()
        )));
    }
    Ok(seq.get(idx))
}

/// Which copy of the skeleton code runs: the `GetSkeletons` helper or the
/// `GET_SKELETONS` command (`GetSkeletons_Batch`), which differ in their
/// messages and in the loop that removes repeated NULL instructions.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Variant {
    Helper,
    Batch,
}

/// What the coalescence commands share: skeletons and the coalescent
/// sequences of both files' lineages.
struct Coalescence {
    second_update: String,
    /// The skeletons of `detail-second.spop`.
    current: Vec<AnalyzeGenotype>,
    /// The skeletons of its coalescence file.
    current_coal: Vec<AnalyzeGenotype>,
    lineage1: SeqCounts,
    lineage2: SeqCounts,
}

impl Analyze {
    /// Turn `g` into its skeleton (the body of the genotype loop of
    /// `GetSkeletons` / `GetSkeletons_Batch`).
    ///
    /// QUIRK: the pairwise pass and the second single-site pass work on
    /// copies of the genotype's genome, but the genome finally stored is
    /// the result of the first single-site pass with repeated NULLs
    /// removed (the outer `mod_genome`); the later passes only cost test
    /// CPU runs and print their messages.
    fn skeletonize(
        &mut self,
        host: &mut dyn AnalyzeHost,
        g: &mut AnalyzeGenotype,
        max_knockouts: i32,
        variant: Variant,
    ) -> Result<()> {
        let what = match variant {
            Variant::Helper => "GetSkeletons",
            Variant::Batch => "GET_SKELETONS",
        };
        recalc_in_place(host, g, &mut CpuTestInfo::default(), 1)?;
        let base_fitness = g.fitness;
        let max_line = g.length;
        let base_genome = g.genome.clone();
        let seq_size = base_genome.representation().size();
        if max_line > seq_size {
            return Err(AnalyzeError::fatal(format!(
                "{what}: length {max_line} exceeds the {seq_size}-site genome (C++ reads past the sequence)"
            )));
        }
        let max_line = max_line.max(0);
        let set_name = inst_set_of(host.inst_sets(), &base_genome)?
            .inst_set_name()
            .to_string();
        let null_inst = host.activate_null_inst(&set_name)?;

        // Single knockouts: keep the sites whose loss lowers fitness.
        let mut mod_genome = base_genome.clone();
        let mut ko_effect = vec![0i32; max_line as usize];
        for line in 0..max_line {
            let cur_inst = base_genome.representation().get(line);
            mod_genome.representation_mut().set(line, null_inst);
            let ko_fitness = recalc_new(host, &mod_genome)?.fitness;
            let effect = &mut ko_effect[line as usize];
            if ko_fitness == 0.0 {
                *effect = -2;
            } else if ko_fitness < base_fitness {
                *effect = -1;
            } else if ko_fitness == base_fitness {
                *effect = 0;
            } else if ko_fitness > base_fitness {
                *effect = 1;
            } else {
                writeln!(self.err, "ERROR: illegal state in AnalyzeKnockouts()")?;
            }
            if ko_fitness < base_fitness {
                mod_genome.representation_mut().set(line, cur_inst);
            }
        }
        g.genome = mod_genome.clone();

        if max_knockouts > 1 {
            // Pairs of kept sites, each pair tested against the genotype's
            // current genome; the skip test reads the outer `mod_seq`.
            for line1 in 0..max_line {
                if mod_genome.representation().get(line1).op() == null_inst.op() {
                    continue;
                }
                for line2 in line1 + 1..max_line {
                    if mod_genome.representation().get(line2).op() == null_inst.op() {
                        continue;
                    }
                    let inner_base = g.genome.clone();
                    let mut inner_mod = inner_base.clone();
                    let cur_inst2 = read_site(inner_base.representation(), line2, what)?;
                    read_site(inner_base.representation(), line1, what)?;
                    inner_mod.representation_mut().set(line1, null_inst);
                    inner_mod.representation_mut().set(line2, null_inst);
                    let ko_fitness = recalc_new(host, &inner_mod)?.fitness;
                    let (e1, e2) = (ko_effect[line1 as usize], ko_effect[line2 as usize]);
                    if ko_fitness >= base_fitness && e1 < 0 && e2 < 0 {
                        if variant == Variant::Batch {
                            writeln!(
                                self.out,
                                "Warning: individual knockouts harmful, but combination not, removing both"
                            )?;
                        }
                        g.genome = inner_mod;
                    } else if ko_fitness < base_fitness && e1 >= 0 && e2 >= 0 {
                        writeln!(
                            self.out,
                            "Warning: redundant instructions found, which shouldn't be the case!"
                        )?;
                        inner_mod.representation_mut().set(line2, cur_inst2);
                        g.genome = inner_mod;
                    }
                }
            }

            // Single knockouts again, against the genotype as it now is.
            let block_base = g.genome.clone();
            let mut block_mod = block_base.clone();
            for line in 0..max_line {
                if read_site(block_mod.representation(), line, what)?.op() == null_inst.op() {
                    continue;
                }
                recalc_in_place(host, g, &mut CpuTestInfo::default(), 1)?;
                let base_fitness = g.fitness;
                let cur_inst = read_site(block_base.representation(), line, what)?;
                block_mod.representation_mut().set(line, null_inst);
                let ko_fitness = recalc_new(host, &block_mod)?.fitness;
                if ko_fitness < base_fitness {
                    block_mod.representation_mut().set(line, cur_inst);
                } else if variant == Variant::Batch {
                    writeln!(
                        self.out,
                        "Additional single knockout found after double knockouts removed! line: {line}"
                    )?;
                }
                g.genome = block_mod.clone();
            }
        }

        // Remove repeated NULL instructions from the outer `mod_seq`.
        let seq = mod_genome.representation_mut();
        match variant {
            Variant::Helper => {
                let mut line = 0;
                while line < max_line {
                    if line < seq.size()
                        && seq.get(line) == null_inst
                        && line > 0
                        && seq.get(line - 1) == null_inst
                    {
                        seq.remove(line, 1);
                        line -= 1;
                    }
                    if seq.size() == 1 || line >= seq.size() {
                        break;
                    }
                    line += 1;
                }
            }
            Variant::Batch => {
                // QUIRK: no bounds checks. Site 0 compares with site -1
                // (undefined), and after a removal the loop keeps going to
                // the original length, reading the stale sites left past
                // the active size (and removing "beyond the end", which
                // shortens the sequence).
                let mut line = 0;
                while line < max_line {
                    if read_site(seq, line, what)? == null_inst
                        && read_site(seq, line - 1, what)? == null_inst
                    {
                        if seq.size() <= 0 {
                            return Err(AnalyzeError::fatal(format!(
                                "{what}: removes a site from an empty sequence (C++ makes its size negative)"
                            )));
                        }
                        seq.remove(line, 1);
                        line -= 1;
                    }
                    line += 1;
                }
            }
        }
        g.genome = mod_genome;
        Ok(())
    }

    /// `GET_SKELETONS [max_knockouts=2]`: replace every genotype of the
    /// current batch by its skeleton.
    pub(crate) fn get_skeletons_batch(
        &mut self,
        host: &mut dyn AnalyzeHost,
        mut cur_string: String,
    ) -> Result<()> {
        writeln!(
            self.out,
            "Turning population into skeletons of only informative sites..."
        )?;
        let mut max_knockouts = 2;
        if !cur_string.is_empty() {
            max_knockouts = su::as_int(&su::pop_word(&mut cur_string));
        }
        let bi = self.batch_index(self.cur_batch)?;
        let mut list = std::mem::take(&mut self.batches[bi].list);
        let mut r = Ok(());
        for g in list.iter_mut() {
            if Self::verbosity(host) >= VERBOSE_ON {
                if let Err(e) = writeln!(self.out, "  Knockout: {}", g.name) {
                    r = Err(e.into());
                    break;
                }
            }
            r = self.skeletonize(host, g, max_knockouts, Variant::Batch);
            if r.is_err() {
                break;
            }
        }
        self.batches[bi].list = list;
        r
    }

    /// `LoadDetailFileAsVector`: `LOAD` into a vector instead of a batch.
    /// Genotypes are copied into the vector (the copy constructor's
    /// omissions apply), and a format error exits.
    fn load_detail_file_as_vector(
        &mut self,
        host: &mut dyn AnalyzeHost,
        mut cur_string: String,
    ) -> Result<Vec<AnalyzeGenotype>> {
        let filename = su::pop_word(&mut cur_string);
        writeln!(self.out, "Loading: {filename}")?;
        let input_file = InitFile::new(&filename, host.working_dir(), None, None);
        if !input_file.was_opened() {
            let fb = input_file.feedback().clone();
            self.print_feedback(&fb)?;
            let _ = self.err.flush();
            return Err(AnalyzeError::Exit(1));
        }
        let filetype = input_file.filetype().to_string();
        if filetype != "population_data" && filetype != "genotype_data" {
            return Err(
                self.exit_with(&format!("error: cannot load files of type \"{filetype}\"."))
            );
        }
        if Self::verbosity(host) >= VERBOSE_ON {
            writeln!(self.out, "Loading file of type: {filetype}")?;
        }
        let mut feedback = Feedback::new();
        let output_list = load_command_list(input_file.format(), Some(&mut feedback));
        self.print_feedback(&feedback)?;
        if feedback.num_errors() > 0 {
            let _ = self.err.flush();
            return Err(AnalyzeError::Exit(1));
        }
        let id_inc = input_file.format().iter().any(|f| f == "id");
        let is = host.inst_sets().default_inst_set().ok_or_else(|| {
            AnalyzeError::fatal(
                "no instruction set loaded (C++ GetDefaultInstSet dereferences NULL)",
            )
        })?;
        let default_genome = Genome::with_inst_set(
            is.hardware_type(),
            is.inst_set_name(),
            InstructionSequence::with_size(1),
        );
        let mut load_count = 0;
        let mut genotypes = Vec::new();
        for mut cur_line in file_lines(&input_file) {
            let mut genotype = AnalyzeGenotype::new(default_genome.clone());
            for c in &output_list {
                let word = su::pop_word(&mut cur_line);
                c.set_value(&mut genotype, &word);
            }
            genotype.name = if !id_inc {
                let n = format!("org-{load_count}");
                load_count += 1;
                n
            } else {
                format!("org-{}", genotype.id_num)
            };
            genotypes.push(genotype.duplicate());
        }
        Ok(genotypes)
    }

    /// `GetSkeletons(file, 2)`: load a detail file and turn every genotype
    /// into its skeleton.
    fn get_skeletons(
        &mut self,
        host: &mut dyn AnalyzeHost,
        file: &str,
    ) -> Result<Vec<AnalyzeGenotype>> {
        writeln!(
            self.out,
            "Turning population into skeletons of only informative sites..."
        )?;
        let mut genotypes = self.load_detail_file_as_vector(host, file.to_string())?;
        for g in genotypes.iter_mut() {
            if Self::verbosity(host) >= VERBOSE_ON {
                writeln!(self.out, "  Knockout: {}", g.name)?;
            }
            self.skeletonize(host, g, 2, Variant::Helper)?;
        }
        Ok(genotypes)
    }

    /// The common first half of `ChangeDebugging`, `ComplexityDebugging`
    /// and `AllComplexityBarriers` (coalescence 10000): the skeletons of
    /// both files and their coalescence files, and the coalescent
    /// sequences of the lineages of each.
    fn coalescence_10000(
        &mut self,
        host: &mut dyn AnalyzeHost,
        mut cur_string: String,
    ) -> Result<Coalescence> {
        let coalescence = 10000;
        let first_update = su::pop_word(&mut cur_string);
        let second_update = su::pop_word(&mut cur_string);
        let previous = self.get_skeletons(host, &detail_name(&first_update))?;
        let interval = atoi(&second_update).wrapping_sub(atoi(&first_update));
        let first_coal = round_down(
            atoi(&first_update).wrapping_sub(coalescence),
            interval,
            "coalescence",
        )?;
        let second_coal = round_down(
            atoi(&second_update).wrapping_sub(coalescence),
            interval,
            "coalescence",
        )?;
        let current_coal = if second_coal == atoi(&first_update) {
            previous.iter().map(AnalyzeGenotype::duplicate).collect()
        } else {
            self.get_skeletons(host, &detail_name(&second_coal.to_string()))?
        };
        let current = self.get_skeletons(host, &detail_name(&second_update))?;
        let current_lineages = make_lineage_vector(&current)?;
        let previous_lineages = make_lineage_vector(&previous)?;
        let previous_coal = self.get_skeletons(host, &detail_name(&first_coal.to_string()))?;
        let lineage1 = coalescent_counts(&current_lineages, &current, &current_coal);
        let lineage2 = coalescent_counts(&previous_lineages, &previous, &previous_coal);
        Ok(Coalescence {
            second_update,
            current,
            current_coal,
            lineage1,
            lineage2,
        })
    }

    /// `COUNT_NEW_SIG_LINEAGES first second`: the number of coalescent
    /// skeletons of the lineages of `detail-first.spop` that are not
    /// coalescent skeletons of `detail-second.spop` (coalescence 4200),
    /// appended to `change_potential.csv`.
    pub(crate) fn count_new_significant_lineages(
        &mut self,
        host: &mut dyn AnalyzeHost,
        mut cur_string: String,
    ) -> Result<()> {
        let coalescence = 4200;
        let first_update = su::pop_word(&mut cur_string);
        let second_update = su::pop_word(&mut cur_string);
        let current = self.get_skeletons(host, &detail_name(&first_update))?;
        let previous = self.get_skeletons(host, &detail_name(&second_update))?;
        let current_lineages = make_lineage_vector(&current)?;
        let previous_lineages = make_lineage_vector(&previous)?;
        let interval = atoi(&second_update).wrapping_sub(atoi(&first_update));
        let first_coal = round_down(
            atoi(&first_update).wrapping_sub(coalescence),
            interval,
            "COUNT_NEW_SIG_LINEAGES",
        )?;
        let previous_coal = self.get_skeletons(host, &detail_name(&first_coal.to_string()))?;
        let second_coal = round_down(
            atoi(&second_update).wrapping_sub(coalescence),
            interval,
            "COUNT_NEW_SIG_LINEAGES",
        )?;
        let current_coal = self.get_skeletons(host, &detail_name(&second_coal.to_string()))?;
        let lineage1 = coalescent_counts(&current_lineages, &current, &current_coal);
        let lineage2 = coalescent_counts(&previous_lineages, &previous, &previous_coal);
        let sig_lineages = lineage1
            .keys()
            .filter(|k| !lineage2.contains_key(*k))
            .count();
        writeln!(
            self.out,
            "Number of new significant lineages {sig_lineages}"
        )?;
        append_cwd(
            "change_potential.csv",
            &format!("{second_update},{sig_lineages}\n"),
        );
        Ok(())
    }

    /// `COUNT_NOVEL_SKELETONS update interval`: the number of skeletons of
    /// `detail-update.spop` not among the skeletons of the coalescence file
    /// (4200 updates earlier), appended to `novelty_potential.csv`.
    pub(crate) fn count_novel_skeletons(
        &mut self,
        host: &mut dyn AnalyzeHost,
        mut cur_string: String,
    ) -> Result<()> {
        let coal = 4200;
        let first_update = su::pop_word(&mut cur_string);
        let interval = atoi(&su::pop_word(&mut cur_string));
        let coal_point = round_down(
            atoi(&first_update).wrapping_sub(coal),
            interval,
            "COUNT_NOVEL_SKELETONS",
        )?;
        let current = self.get_skeletons(host, &detail_name(&first_update))?;
        let coal_genotypes = self.get_skeletons(host, &detail_name(&coal_point.to_string()))?;
        let count = novel_count(&current, &coal_genotypes);
        writeln!(self.out, "Novel Genotypes: {count}")?;
        append_cwd(
            "novelty_potential.csv",
            &format!("{first_update},{count}\n"),
        );
        Ok(())
    }

    /// `CALC_SKELETON_DIVERSITY update`: the Shannon diversity (in bits, in
    /// single precision) of the skeletons of the living genotypes of
    /// `detail-update.spop`, appended to `ecoogical_potential.csv`.
    pub(crate) fn shannon_diversity_skeletons(
        &mut self,
        host: &mut dyn AnalyzeHost,
        mut cur_string: String,
    ) -> Result<()> {
        let file_update = su::pop_word(&mut cur_string);
        let skeletons = self.get_skeletons(host, &detail_name(&file_update))?;
        let diversity = skeleton_diversity(&skeletons);
        writeln!(self.out, "Skeleton diversity: {diversity}")?;
        append_cwd(
            "ecoogical_potential.csv",
            &format!("{file_update},{diversity}\n"),
        );
        Ok(())
    }

    /// `GET_LARGEST_SKELETON update`: the most non-NULL sites in a skeleton
    /// of `detail-update.spop`, appended to `complexity_potential.csv`.
    pub(crate) fn get_largest_skeleton(
        &mut self,
        host: &mut dyn AnalyzeHost,
        mut cur_string: String,
    ) -> Result<()> {
        writeln!(self.out, "Getting largest skeleton...")?;
        let file_update = su::pop_word(&mut cur_string);
        let skeletons = self.get_skeletons(host, &detail_name(&file_update))?;
        let (largest, _) = self.largest_skeleton(host, &skeletons)?;
        writeln!(self.out, "Largest skeleton: {largest}")?;
        append_cwd(
            "complexity_potential.csv",
            &format!("{file_update},{largest}\n"),
        );
        Ok(())
    }

    /// The size of the largest skeleton (counted with the NULL instruction
    /// of each genotype's set) and the first genotype that has it.
    fn largest_skeleton(
        &mut self,
        host: &mut dyn AnalyzeHost,
        skeletons: &[AnalyzeGenotype],
    ) -> Result<(i32, Option<usize>)> {
        let mut largest = 0;
        let mut winner = None;
        for (i, g) in skeletons.iter().enumerate() {
            let set_name = inst_set_of(host.inst_sets(), &g.genome)?
                .inst_set_name()
                .to_string();
            let null_inst = host.activate_null_inst(&set_name)?;
            let length = skeleton_size(&g.genome, null_inst);
            if length > largest {
                largest = length;
                winner = Some(i);
            }
        }
        Ok((largest, winner))
    }

    /// `CHANGE_DEBUG first second`: append to `change_list.csv` each
    /// coalescent skeleton counted by `ALL_COMPLEXITY_BARRIERS` as new,
    /// once per skeleton of `detail-second.spop` equal to it, with that
    /// genotype's update of birth.
    pub(crate) fn change_debugging(
        &mut self,
        host: &mut dyn AnalyzeHost,
        cur_string: String,
    ) -> Result<()> {
        let c = self.coalescence_10000(host, cur_string)?;
        let mut text = String::new();
        for (key, (seq, _)) in &c.lineage1 {
            if c.lineage2.contains_key(key) {
                continue;
            }
            for g in &c.current {
                if g.genome.representation().as_slice() == seq.as_slice() {
                    text.push_str(&format!("{} {}\n", seq.as_string(), g.update_born));
                }
            }
        }
        append_cwd("change_list.csv", &text);
        Ok(())
    }

    /// `COMPLEXITY_DEBUG first second`: the largest skeleton of
    /// `detail-second.spop`, its size printed and its genome string
    /// appended to `complexity_potential.csv`.
    pub(crate) fn complexity_debugging(
        &mut self,
        host: &mut dyn AnalyzeHost,
        cur_string: String,
    ) -> Result<()> {
        let c = self.coalescence_10000(host, cur_string)?;
        let (largest, winner) = self.largest_skeleton(host, &c.current)?;
        writeln!(self.out, "Largest skeleton: {largest}")?;
        let Some(w) = winner else {
            return Err(AnalyzeError::fatal(
                "COMPLEXITY_DEBUG: no skeleton has a non-NULL site (C++ prints a default-constructed Genome, dereferencing its NULL representation)",
            ));
        };
        append_cwd(
            "complexity_potential.csv",
            &format!("{}\n", c.current[w].genome.as_string()),
        );
        Ok(())
    }

    /// `ALL_COMPLEXITY_BARRIERS first second`: the four measures of
    /// `COUNT_NEW_SIG_LINEAGES`, `COUNT_NOVEL_SKELETONS`,
    /// `CALC_SKELETON_DIVERSITY` and `GET_LARGEST_SKELETON` for
    /// `detail-second.spop` with coalescence 10000, from one set of
    /// skeletons.
    pub(crate) fn all_complexity_barriers(
        &mut self,
        host: &mut dyn AnalyzeHost,
        cur_string: String,
    ) -> Result<()> {
        let c = self.coalescence_10000(host, cur_string)?;
        let sig_lineages = c
            .lineage1
            .keys()
            .filter(|k| !c.lineage2.contains_key(*k))
            .count();
        writeln!(
            self.out,
            "Number of new significant lineages {sig_lineages}"
        )?;
        append_cwd(
            "change_potential.csv",
            &format!("{},{sig_lineages}\n", c.second_update),
        );

        let count = novel_count(&c.current, &c.current_coal);
        writeln!(self.out, "Novel Genotypes: {count}")?;
        append_cwd(
            "novelty_potential.csv",
            &format!("{},{count}\n", c.second_update),
        );

        let diversity = skeleton_diversity(&c.current);
        writeln!(self.out, "Skeleton diversity: {diversity}")?;
        append_cwd(
            "ecoogical_potential.csv",
            &format!("{},{diversity}\n", c.second_update),
        );

        let (largest, _) = self.largest_skeleton(host, &c.current)?;
        writeln!(self.out, "Largest skeleton: {largest}")?;
        append_cwd(
            "complexity_potential.csv",
            &format!("{},{largest}\n", c.second_update),
        );
        Ok(())
    }
}

/// The number of genotypes of `current` whose skeleton is not among the
/// skeletons of `coal` (`coal_set`).
fn novel_count(current: &[AnalyzeGenotype], coal: &[AnalyzeGenotype]) -> i32 {
    let coal_set: BTreeSet<String> = coal
        .iter()
        .map(|g| g.genome.representation().as_string())
        .collect();
    current
        .iter()
        .filter(|g| !coal_set.contains(&g.genome.representation().as_string()))
        .count() as i32
}

/// `-sum p log2 p` over the skeletons of the living genotypes, weighted by
/// organism count, in `float` (`log2f`, the sum fused into one
/// multiply-add on the reference build), summed in skeleton-string order;
/// formatted as `cout << float`.
fn skeleton_diversity(skeletons: &[AnalyzeGenotype]) -> String {
    let mut counts: BTreeMap<String, i32> = BTreeMap::new();
    let mut pop_size: f32 = 0.0;
    for g in skeletons.iter().filter(|g| g.num_cpus > 0) {
        pop_size += g.num_cpus as f32;
        *counts
            .entry(g.genome.representation().as_string())
            .or_insert(0) += g.num_cpus;
    }
    let mut diversity: f32 = 0.0;
    for &n in counts.values() {
        let prob = n as f32 / pop_size;
        diversity = prob.mul_add(prob.log2(), diversity);
    }
    fmt_double(f64::from(-diversity))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn atoi_is_decimal() {
        assert_eq!(atoi("010"), 10);
        assert_eq!(atoi("  -42x"), -42);
        assert_eq!(atoi("abc"), 0);
        assert_eq!(atoi("0x10"), 0);
    }

    #[test]
    fn rounding_down_by_zero_is_fatal() {
        assert_eq!(round_down(4900, 100, "t").unwrap(), 4900);
        assert_eq!(round_down(-150, 100, "t").unwrap(), -100);
        assert!(round_down(5, 0, "t").is_err());
    }
}
