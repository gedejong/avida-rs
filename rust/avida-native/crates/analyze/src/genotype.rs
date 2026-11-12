//! `cAnalyzeGenotype` (avida-core/source/analyze/cAnalyzeGenotype.{h,cc}):
//! one genotype in an analyze batch, with the statistics loaded from files
//! or computed by the test CPU.

use std::sync::{Arc, Mutex, PoisonError};

use avida_config::string_util as su;
use avida_genome::{Genome, InstructionSequence};

use crate::error::{AnalyzeError, Result};
use crate::host::{AnalyzeHost, CpuTestOptions, TestContext};
use crate::landscape::Landscape;

/// `cModularityAnalysis::cModularityData` (analyze/cModularityAnalysis.h):
/// what `CALC_FUNCTIONAL_MODULARITY` stores for a genotype.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ModularityData {
    pub tasks_done: i32,
    pub insts_tasks: i32,
    pub tasks_prop: f64,
    pub ave_tasks_per_site: f64,
    pub ave_sites_per_task: f64,
    pub ave_prop_nonoverlap: f64,
    pub sites_per_task: Vec<i32>,
    pub sites_inv_x_tasks: Vec<i32>,
    pub task_length: Vec<i32>,
    pub ave_task_position: Vec<f64>,
}

/// `sGenotypeDatastore` (`m_data`): analysis data attached to a genotype
/// with `SetGenotypeData`. Copies of a genotype share it, as the C++ copy
/// constructor shares the reference-counted store.
#[derive(Debug, Clone, Default)]
pub struct GenotypeDataStore(Arc<Mutex<Option<ModularityData>>>);

impl GenotypeDataStore {
    /// `GetGenotypeData(tok, GD_MD_ID)`.
    pub fn modularity(&self) -> Option<ModularityData> {
        self.0
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
    }

    /// `SetGenotypeData(GD_MD_ID, data)`.
    pub fn set_modularity(&self, data: ModularityData) {
        *self.0.lock().unwrap_or_else(PoisonError::into_inner) = Some(data);
    }
}

/// `MATING_TYPE_JUVENILE` (avida/core/Definitions.h).
pub const MATING_TYPE_JUVENILE: i32 = -1;
/// `MATE_PREFERENCE_RANDOM`.
pub const MATE_PREFERENCE_RANDOM: i32 = 0;
/// `MIN_GENOME_LENGTH`.
pub const MIN_GENOME_LENGTH: i32 = 8;

/// `cPhenPlastSummary` (avida-core/source/analyze/cPhenPlastSummary.h).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct PhenPlastSummary {
    pub recalculate_trials: i32,
    pub num_phenotypes: i32,
    pub min_fitness: f64,
    pub max_fitness: f64,
    pub avg_fitness: f64,
    pub likely_fitness: f64,
    pub phenotypic_entropy: f64,
    pub likely_frequency: f64,
    pub min_fit_frequency: f64,
    pub max_fit_frequency: f64,
    pub viable_probability: f64,
    pub task_probabilities: Vec<f64>,
}

impl PhenPlastSummary {
    /// `cPhenPlastSummary(const cPhenPlastSummary&)`.
    ///
    /// QUIRK: the C++ copy constructor does not copy `m_phenotypic_entropy`
    /// or `m_likely_frequency`, leaving them uninitialised. They are 0 here
    /// (UNCERTAIN: the C++ prints whatever the heap held).
    pub fn copy_like_cpp(&self) -> Self {
        PhenPlastSummary {
            phenotypic_entropy: 0.0,
            likely_frequency: 0.0,
            ..self.clone()
        }
    }
}

/// The fields `cAnalyzeGenotype::Recalculate` copies from the most likely
/// `cPlasticPhenotype` of a `cPhenPlastGenotype`.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct RecalcResult {
    pub viable: bool,
    pub env_inputs: Vec<i32>,
    pub executed_flags: String,
    pub inst_executed_counts: Vec<i32>,
    pub length: i32,
    pub copy_length: i32,
    pub exe_length: i32,
    pub merit: f64,
    pub gest_time: i32,
    pub fitness: f64,
    pub errors: i32,
    pub div_type: f64,
    pub mate_id: i32,
    pub task_counts: Vec<i32>,
    pub task_qualities: Vec<f64>,
    pub internal_task_counts: Vec<i32>,
    pub internal_task_qualities: Vec<f64>,
    pub rbins_total: Vec<f64>,
    pub rbins_avail: Vec<f64>,
    pub collect_spec_counts: Vec<i32>,
    pub mating_type: i32,
    pub mate_preference: i32,
    pub mating_display_a: i32,
    pub mating_display_b: i32,
    /// `cPhenPlastSummary(recalc_data)`; only used when `num_trials > 1`.
    pub phen_plast: Option<PhenPlastSummary>,
}

/// What `Recalculate` reads from the parent genotype.
#[derive(Debug, Clone)]
pub(crate) struct ParentInfo {
    pub fitness: f64,
    pub efficiency: f64,
    pub comp_merit: f64,
    pub sequence: String,
    pub ancestor_dist: i32,
}

/// `cAnalyzeGenotype`.
///
/// Field names follow the C++ members. `land` and `phen_plast` are the
/// lazily computed `m_land` / `m_phenplast_stats`; `cpu_test_info` holds
/// the settings of `m_cpu_test_info` (`SetCPUTestInfo`, used by the
/// landscape).
#[derive(Debug, Clone)]
pub struct AnalyzeGenotype {
    pub genome: Genome,
    pub name: String,
    pub aligned_sequence: String,
    pub tag: String,
    pub viable: bool,
    pub id_num: i32,
    /// `m_src.transmission_type`, a 4-bit bitfield.
    pub src: i32,
    pub src_args: String,
    pub parent_str: String,
    pub parent_id: i32,
    pub parent2_id: i32,
    pub num_cpus: i32,
    pub total_cpus: i32,
    pub update_born: i32,
    pub update_dead: i32,
    pub depth: i32,
    pub cells: String,
    pub gest_offsets: String,
    pub length: i32,
    pub copy_length: i32,
    pub exe_length: i32,
    pub merit: f64,
    pub gest_time: i32,
    pub fitness: f64,
    pub errors: i32,
    pub div_type: f64,
    pub mate_id: i32,
    pub executed_flags: String,
    pub inst_executed_counts: Vec<i32>,
    pub task_counts: Vec<i32>,
    pub task_qualities: Vec<f64>,
    pub internal_task_counts: Vec<i32>,
    pub internal_task_qualities: Vec<f64>,
    pub rbins_total: Vec<f64>,
    pub rbins_avail: Vec<f64>,
    pub collect_spec_counts: Vec<i32>,
    pub env_inputs: Vec<i32>,
    pub mating_type: i32,
    pub mate_preference: i32,
    pub mating_display_a: i32,
    pub mating_display_b: i32,
    pub fitness_ratio: f64,
    pub efficiency_ratio: f64,
    pub comp_merit_ratio: f64,
    pub parent_dist: i32,
    pub ancestor_dist: i32,
    pub lineage_label: i32,
    pub parent_muts: String,
    pub task_order: String,
    pub cpu_test_info: CpuTestOptions,
    pub land: Option<Box<Landscape>>,
    pub phen_plast: Option<PhenPlastSummary>,
    /// `knockout_stats` (`cAnalyzeKnockouts`), filled on first use by
    /// `CalcKnockouts` and never reset (not even when the genome changes).
    pub knockouts: Option<KnockoutStats>,
    /// `m_data` (shared by copies).
    pub data: GenotypeDataStore,
}

/// `cString::Insert(in, pos)`; the C++ only asserts `pos <= size`.
pub(crate) fn cstring_insert(s: &mut String, ins: &str, pos: i32) -> Result<()> {
    if pos < 0 || pos as usize > s.len() {
        return Err(AnalyzeError::fatal(format!(
            "cString::Insert at position {pos} of a string of length {} (C++ heap overflow)",
            s.len()
        )));
    }
    su::insert_str(s, ins, pos as usize, 0);
    Ok(())
}

/// `cString::Pop(',')` of a mutation list followed by
/// `cur_mut[0]`, `ClipFront(1)`, `ClipEnd(1)`, `AsInt()`:
/// (type, position) of the next mutation, or `None` when the list is empty.
pub(crate) fn pop_mutation(diff_info: &mut String) -> Option<(u8, i32)> {
    let mut cur_mut = su::pop(diff_info, ',');
    if cur_mut.is_empty() {
        return None;
    }
    let mut_type = cur_mut.as_bytes()[0];
    su::clip_front(&mut cur_mut, 1);
    su::clip_end(&mut cur_mut, 1);
    Some((mut_type, su::as_int(&cur_mut)))
}

fn index(v: usize, i: i32, what: &str) -> Result<usize> {
    if i < 0 || i as usize >= v {
        return Err(AnalyzeError::fatal(format!(
            "{what}[{i}] out of range (size {v}); the C++ reads past the array"
        )));
    }
    Ok(i as usize)
}

impl AnalyzeGenotype {
    /// `cAnalyzeGenotype(world, genome)`.
    ///
    /// `div_type`, `mate_id` and `lineage_label` are not initialised by the
    /// C++ constructor; they start at 0 here.
    pub fn new(genome: Genome) -> Self {
        AnalyzeGenotype {
            genome,
            name: String::new(),
            aligned_sequence: String::new(),
            tag: String::new(),
            viable: false,
            id_num: -1,
            src: 0,
            src_args: String::new(),
            parent_str: String::new(),
            parent_id: -1,
            parent2_id: -1,
            num_cpus: 0,
            total_cpus: 0,
            update_born: 0,
            update_dead: 0,
            depth: 0,
            cells: String::new(),
            gest_offsets: String::new(),
            length: 0,
            copy_length: 0,
            exe_length: 0,
            merit: 0.0,
            gest_time: i32::MAX,
            fitness: 0.0,
            errors: 0,
            div_type: 0.0,
            mate_id: 0,
            executed_flags: String::new(),
            inst_executed_counts: Vec::new(),
            task_counts: Vec::new(),
            task_qualities: Vec::new(),
            internal_task_counts: Vec::new(),
            internal_task_qualities: Vec::new(),
            rbins_total: Vec::new(),
            rbins_avail: Vec::new(),
            collect_spec_counts: Vec::new(),
            env_inputs: Vec::new(),
            mating_type: MATING_TYPE_JUVENILE,
            mate_preference: MATE_PREFERENCE_RANDOM,
            mating_display_a: 0,
            mating_display_b: 0,
            fitness_ratio: 0.0,
            efficiency_ratio: 0.0,
            comp_merit_ratio: 0.0,
            parent_dist: 0,
            ancestor_dist: 0,
            lineage_label: 0,
            parent_muts: String::new(),
            task_order: String::new(),
            cpu_test_info: CpuTestOptions::default(),
            land: None,
            phen_plast: None,
            knockouts: None,
            data: GenotypeDataStore::default(),
        }
    }

    /// `cAnalyzeGenotype(const cAnalyzeGenotype&)` (`DUPLICATE`).
    ///
    /// QUIRK: the C++ copy constructor skips `m_src`, `m_src_args`,
    /// `m_parent_str`, `m_cells`, `m_gest_offsets`, `executed_flags`,
    /// `div_type`, `mate_id`, `m_env_inputs`, `lineage_label` and
    /// `task_order`; they get their default-constructed values (the
    /// uninitialised `div_type`, `mate_id`, `lineage_label` become 0 here).
    /// The landscape is not copied either (`m_cpu_test_info` is); the
    /// plasticity summary is copied with its own omissions
    /// ([`PhenPlastSummary::copy_like_cpp`]).
    pub fn duplicate(&self) -> Self {
        AnalyzeGenotype {
            src: 0,
            src_args: String::new(),
            parent_str: String::new(),
            cells: String::new(),
            gest_offsets: String::new(),
            executed_flags: String::new(),
            div_type: 0.0,
            mate_id: 0,
            env_inputs: Vec::new(),
            lineage_label: 0,
            task_order: String::new(),
            land: None,
            phen_plast: self.phen_plast.as_ref().map(|p| p.copy_like_cpp()),
            ..self.clone()
        }
    }

    // -- Setters with side effects --------------------------------------

    /// `SetSource(int)`: stored in a 4-bit bitfield of an enum type.
    /// UNCERTAIN: whether the bitfield is signed is implementation defined;
    /// clang treats this enum (no negative enumerators) as unsigned.
    pub fn set_source(&mut self, src: i32) {
        self.src = src & 0xF;
    }

    /// `SetParents(parent_str)`: parses up to two comma separated ids.
    /// QUIRK: does not update `m_parent_str` itself.
    pub fn set_parents(&mut self, parent_str: &str) {
        let mut lps = parent_str.to_string();
        if !lps.is_empty() {
            self.parent_id = su::as_int(&su::pop(&mut lps, ','));
        }
        if !lps.is_empty() {
            self.parent2_id = su::as_int(&su::pop(&mut lps, ','));
        }
    }

    fn update_parent_str(&mut self) {
        self.parent_str = if self.parent_id >= 0 {
            if self.parent2_id >= 0 {
                format!("{},{}", self.parent_id, self.parent2_id)
            } else {
                format!("{}", self.parent_id)
            }
        } else {
            String::new()
        };
    }

    /// `SetParentID(id)`.
    pub fn set_parent_id(&mut self, id: i32) {
        self.parent_id = id;
        self.update_parent_str();
    }

    /// `SetParent2ID(id)`.
    pub fn set_parent2_id(&mut self, id: i32) {
        self.parent2_id = id;
        self.update_parent_str();
    }

    /// `SetSequence(seq)`: replaces the genome's instruction sequence.
    pub fn set_sequence(&mut self, seq: &str) {
        self.genome
            .set_representation(InstructionSequence::from_string(seq));
    }

    // -- Derived values ---------------------------------------------------

    /// `GetMinLength()`.
    pub fn min_length(&self) -> i32 {
        self.exe_length.min(self.copy_length)
    }

    /// `GetCompMerit()`.
    pub fn comp_merit(&self) -> f64 {
        self.merit / self.min_length() as f64
    }

    /// `GetEfficiency()`.
    pub fn efficiency(&self) -> f64 {
        self.min_length() as f64 / self.gest_time as f64
    }

    /// `GetSequence()`.
    pub fn sequence(&self) -> String {
        self.genome.representation().as_string()
    }

    /// `GetInstSet()`.
    pub fn inst_set(&self) -> String {
        self.genome.inst_set().to_string()
    }

    /// `GetNumTasks()`.
    pub fn num_tasks(&self) -> i32 {
        self.task_counts.len() as i32
    }

    /// `GetTaskCount(task_id)`.
    pub fn task_count(&self, task_id: i32) -> Result<i32> {
        if task_id >= self.task_counts.len() as i32 {
            return Ok(0);
        }
        let i = index(self.task_counts.len(), task_id, "task_counts")?;
        Ok(self.task_counts[i])
    }

    /// `GetTaskQuality(task_id)` (bounded by the task *count* array).
    pub fn task_quality(&self, task_id: i32) -> Result<f64> {
        if task_id >= self.task_counts.len() as i32 {
            return Ok(0.0);
        }
        let i = index(self.task_qualities.len(), task_id, "task_qualities")?;
        Ok(self.task_qualities[i])
    }

    /// `GetTotalTaskCount()`.
    pub fn total_task_count(&self) -> i32 {
        self.task_counts.iter().filter(|&&c| c > 0).count() as i32
    }

    /// `GetTotalTaskPerformanceCount()`.
    pub fn total_task_performance_count(&self) -> i32 {
        self.task_counts
            .iter()
            .fold(0i32, |a, &c| a.wrapping_add(c))
    }

    /// `GetEnvInput(input_id)`.
    pub fn env_input(&self, input_id: i32) -> Result<i32> {
        if input_id >= self.env_inputs.len() as i32 {
            return Ok(0);
        }
        let i = index(self.env_inputs.len(), input_id, "m_env_inputs")?;
        Ok(self.env_inputs[i])
    }

    /// `GetInstExecutedCount(inst)`.
    /// QUIRK: instruction 0 always reports 0 (`_inst_num > 0`).
    pub fn inst_executed_count(&self, inst: i32) -> i32 {
        if inst < self.inst_executed_counts.len() as i32 && inst > 0 {
            return self.inst_executed_counts[inst as usize];
        }
        0
    }

    /// `GetRBinTotal(resource_id)`.
    pub fn rbin_total(&self, id: i32) -> Result<f64> {
        if id >= self.rbins_total.len() as i32 {
            return Ok(-1.0);
        }
        let i = index(self.rbins_total.len(), id, "rbins_total")?;
        Ok(self.rbins_total[i])
    }

    /// `GetRBinAvail(resource_id)`.
    pub fn rbin_avail(&self, id: i32) -> Result<f64> {
        if id >= self.rbins_avail.len() as i32 {
            return Ok(-1.0);
        }
        let i = index(self.rbins_avail.len(), id, "rbins_avail")?;
        Ok(self.rbins_avail[i])
    }

    /// `GetRSpec(spec_id)`.
    pub fn r_spec(&self, id: i32) -> i32 {
        if id >= self.collect_spec_counts.len() as i32 || id < 0 {
            return -1;
        }
        self.collect_spec_counts[id as usize]
    }

    /// `GetTaskList()`: one character per task count.
    pub fn task_list(&self) -> String {
        self.task_counts
            .iter()
            .map(|&c| {
                if c < 10 {
                    // QUIRK: `'0' + cur_count` for negative counts gives
                    // characters below '0', as in C++.
                    ((b'0' as i32 + c) as u8) as char
                } else if c < 30 {
                    'X'
                } else if c < 80 {
                    'L'
                } else if c < 300 {
                    'C'
                } else if c < 800 {
                    'D'
                } else if c < 3000 {
                    'M'
                } else {
                    '+'
                }
            })
            .collect()
    }

    /// `GetMapLink()`.
    pub fn map_link(&self) -> String {
        format!("<a href=\"tasksites.{}.html\">Phenotype Map</a>", self.name)
    }

    /// `GetAlignmentExecutedFlags()`: the executed flags with a `_` inserted
    /// wherever the aligned sequence has a gap.
    pub fn alignment_executed_flags(&self) -> Result<String> {
        let mut flags = self.executed_flags.clone();
        for (i, &c) in self.aligned_sequence.as_bytes().iter().enumerate() {
            if c == b'_' {
                cstring_insert(&mut flags, "_", i as i32)?;
            }
        }
        Ok(flags)
    }

    /// `GetHTMLSequence()`: the sequence with the mutations from the parent
    /// (`parent_muts`) coloured.
    pub fn html_sequence(&self) -> String {
        let text_genome = self.sequence();
        let text = text_genome.as_bytes();
        let size = self.genome.representation().size() as i64;
        let mut html = String::from("<tt>");
        let mut diff_info = self.parent_muts.clone();
        let mut mut_type = b'N';
        let mut mut_pos: i64 = -1;
        if let Some((t, p)) = pop_mutation(&mut diff_info) {
            mut_type = t;
            mut_pos = p as i64;
        }
        let mut ins_count: i64 = 0;
        let mut i: i64 = 0;
        while i < size {
            let mut symbol = text[i as usize];
            if i != mut_pos {
                html.push(symbol as char);
            } else {
                let color = if mut_type == b'M' {
                    "#FF0000"
                } else if mut_type == b'I' {
                    ins_count += 1;
                    "#00FF00"
                } else {
                    symbol = b'*';
                    i -= 1; // Rewind - the handle character was not read yet.
                    "#0000FF"
                };
                match pop_mutation(&mut diff_info) {
                    Some((t, p)) => {
                        mut_type = t;
                        mut_pos = p as i64;
                        if mut_type == b'D' {
                            mut_pos += ins_count;
                        }
                    }
                    None => mut_pos = -1,
                }
                html.push_str(&format!(
                    "<b><font color=\"{color}\">{}</font></b>",
                    symbol as char
                ));
            }
            i += 1;
        }
        html.push_str("</tt>");
        html
    }

    /// `CheckLand()`: on first use, the distance-1 landscape with this
    /// genotype's test settings, in the world's default context.
    pub(crate) fn landscape(&mut self, host: &mut dyn AnalyzeHost) -> Result<&Landscape> {
        if self.land.is_none() {
            let mut land = Landscape::new(&self.genome);
            land.set_cpu_test_info(&self.cpu_test_info);
            land.set_distance(1);
            host.with_test_world(TestContext::Default, &mut |w| land.process(w))?;
            self.land = Some(Box::new(land));
        }
        self.land
            .as_deref()
            .ok_or_else(|| AnalyzeError::fatal("landscape missing after CheckLand"))
    }

    /// `CheckPhenPlast()` then the summary (1000 trials, default test info).
    pub(crate) fn phen_plast_stats(
        &mut self,
        host: &mut dyn AnalyzeHost,
    ) -> Result<&PhenPlastSummary> {
        if self.phen_plast.is_none() {
            self.phen_plast = Some(crate::cpu::phen_plast(host, &self.genome, 1000)?);
        }
        self.phen_plast
            .as_ref()
            .ok_or_else(|| AnalyzeError::fatal("plasticity summary missing after CheckPhenPlast"))
    }

    /// `GetTaskProbability(task_id)`.
    pub(crate) fn task_probability(
        &mut self,
        host: &mut dyn AnalyzeHost,
        task_id: i32,
    ) -> Result<f64> {
        if task_id >= host.num_tasks() {
            return Ok(0.0);
        }
        let pp = self.phen_plast_stats(host)?;
        let i = index(pp.task_probabilities.len(), task_id, "m_task_probabilities")?;
        Ok(pp.task_probabilities[i])
    }

    /// What `Recalculate` reads from this genotype when it is the parent.
    pub(crate) fn parent_info(&self) -> ParentInfo {
        ParentInfo {
            fitness: self.fitness,
            efficiency: self.efficiency(),
            comp_merit: self.comp_merit(),
            sequence: self.sequence(),
            ancestor_dist: self.ancestor_dist,
        }
    }

    /// The bookkeeping half of `Recalculate(ctx, test_info, parent, num_trials)`.
    pub fn apply_recalc(
        &mut self,
        r: RecalcResult,
        parent: Option<&ParentInfoView>,
        num_trials: i32,
    ) {
        self.viable = r.viable;
        self.env_inputs = r.env_inputs;
        self.executed_flags = r.executed_flags;
        self.inst_executed_counts = r.inst_executed_counts;
        self.length = r.length;
        self.copy_length = r.copy_length;
        self.exe_length = r.exe_length;
        self.merit = r.merit;
        self.gest_time = r.gest_time;
        self.fitness = r.fitness;
        self.errors = r.errors;
        self.div_type = r.div_type;
        self.mate_id = r.mate_id;
        self.task_counts = r.task_counts;
        self.task_qualities = r.task_qualities;
        self.internal_task_counts = r.internal_task_counts;
        self.internal_task_qualities = r.internal_task_qualities;
        self.rbins_total = r.rbins_total;
        self.rbins_avail = r.rbins_avail;
        self.collect_spec_counts = r.collect_spec_counts;
        self.mating_type = r.mating_type;
        self.mate_preference = r.mate_preference;
        self.mating_display_a = r.mating_display_a;
        self.mating_display_b = r.mating_display_b;

        if let Some(p) = parent {
            let p = &p.0;
            self.fitness_ratio = self.fitness / p.fitness;
            self.efficiency_ratio = self.efficiency() / p.efficiency;
            self.comp_merit_ratio = self.comp_merit() / p.comp_merit;
            let seq = self.sequence();
            // cStringUtil::EditDistance(seq, parent_seq, parent_muts) with
            // the default gap ' '. QUIRK: with an empty sequence the C++
            // leaves `parent_muts` unchanged.
            let (dist, info) = su::edit_distance_info(&seq, &p.sequence, b' ');
            if !seq.is_empty() && !p.sequence.is_empty() {
                self.parent_muts = info;
            }
            self.parent_dist = dist;
            self.ancestor_dist = p.ancestor_dist + self.parent_dist;
        }

        if num_trials > 1 {
            self.phen_plast = r.phen_plast;
        }
    }
}

/// `cAnalyzeGenotype::cAnalyzeKnockouts`: the counts of lethal,
/// detrimental, neutral and beneficial single knockouts and, when asked for,
/// the task counts of each knockout (`has_chart_info`). The pair counts are
/// never filled by the ported commands.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct KnockoutStats {
    pub dead_count: i32,
    pub neg_count: i32,
    pub neut_count: i32,
    pub pos_count: i32,
    pub has_chart_info: bool,
    /// Per site, the task counts with that site knocked out.
    pub task_counts: Vec<Vec<i32>>,
}

/// Opaque parent data for [`AnalyzeGenotype::apply_recalc`]; build it with
/// [`ParentInfoView::of`].
#[derive(Debug, Clone)]
pub struct ParentInfoView(pub(crate) ParentInfo);

impl ParentInfoView {
    /// The parent data of `g`.
    pub fn of(g: &AnalyzeGenotype) -> Self {
        ParentInfoView(g.parent_info())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn g(seq: &str) -> AnalyzeGenotype {
        AnalyzeGenotype::new(Genome::with_inst_set(
            0,
            "x",
            InstructionSequence::from_string(seq),
        ))
    }

    #[test]
    fn parents_string() {
        let mut a = g("abc");
        a.set_parent_id(5);
        assert_eq!(a.parent_str, "5");
        a.set_parent2_id(7);
        assert_eq!(a.parent_str, "5,7");
        a.set_parent_id(-1);
        assert_eq!(a.parent_str, "");
        a.set_parents("3,4");
        assert_eq!(
            (a.parent_id, a.parent2_id, a.parent_str.as_str()),
            (3, 4, "")
        );
    }

    #[test]
    fn task_list_chars() {
        let mut a = g("a");
        a.task_counts = vec![0, 9, 10, 30, 80, 300, 800, 3000];
        assert_eq!(a.task_list(), "09XLCDM+");
    }

    #[test]
    fn duplicate_drops_fields() {
        let mut a = g("abc");
        a.src_args = "x".into();
        a.tag = "t".into();
        a.task_order = "o".into();
        a.cells = "1".into();
        let b = a.duplicate();
        assert_eq!(b.tag, "t");
        assert_eq!(b.src_args, "");
        assert_eq!(b.task_order, "");
        assert_eq!(b.cells, "");
    }

    #[test]
    fn html_sequence_marks_mutations() {
        let mut a = g("abcd");
        a.parent_muts = "M1xb,I2c".into();
        assert_eq!(
            a.html_sequence(),
            "<tt>a<b><font color=\"#FF0000\">b</font></b><b><font color=\"#00FF00\">c</font></b>d</tt>"
        );
        a.parent_muts = "D1x".into();
        assert_eq!(
            a.html_sequence(),
            "<tt>a<b><font color=\"#0000FF\">*</font></b>bcd</tt>"
        );
    }
}
