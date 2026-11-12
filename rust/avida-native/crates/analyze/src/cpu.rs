//! Analyze mode's use of the test CPU: `cPhenPlastGenotype` /
//! `cPlasticPhenotype` (avida-core/source/main/cPhenPlastGenotype.cc,
//! cPlasticPhenotype.cc), the test-CPU half of
//! `cAnalyzeGenotype::Recalculate` and `CheckPhenPlast`
//! (analyze/cAnalyzeGenotype.cc), and the loops of `cAnalyze::CommandPrint`
//! and `cAnalyze::CommandTrace` (analyze/cAnalyze.cc), all at 47f13dadb.
//!
//! Every function runs against the [`TestWorld`] a host lends through
//! [`AnalyzeHost::with_test_world`].

use std::io::Write;

use avida_apto::DataFile;
use avida_genome::Genome;
use avida_hardware::test_cpu::{CpuTestInfo, TestCpu, TestWorld};
use avida_hardware::tracer::{HardwareStatusPrinter, HardwareTracerPtr};
use avida_phenotype::Phenotype;

use crate::error::{AnalyzeError, Result};
use crate::genotype::{PhenPlastSummary, RecalcResult};
use crate::host::{AnalyzeHost, CpuTestOptions, TestContext};

fn hw<T>(r: std::result::Result<T, String>) -> Result<T> {
    r.map_err(AnalyzeError::world)
}

/// The depth-0 phenotype of a finished test (`GetTestPhenotype()`).
fn tested_phenotype(info: &CpuTestInfo) -> Result<&Phenotype> {
    info.test_phenotype(0)
        .ok_or_else(|| AnalyzeError::fatal("test CPU produced no organism at depth 0"))
}

/// `cPlasticPhenotype`: one distinct phenotype of a genotype tested
/// repeatedly, with how often it was observed.
#[derive(Debug, Clone)]
pub struct PlasticPhenotype {
    /// The `cPhenotype` base (copied from the first observation).
    pub phenotype: Phenotype,
    pub num_observations: i32,
    pub num_trials: i32,
    /// `GetTestCPUInputs()` of the first observation.
    pub env_inputs: Vec<i32>,
    /// `SetExecutedFlags`: `+`/`-` per memory site of the depth-0 organism.
    pub executed_flags: String,
    pub viable: bool,
}

impl PlasticPhenotype {
    /// `cPlasticPhenotype(test_info, num_trials)`.
    fn new(info: &CpuTestInfo, num_trials: i32) -> Result<Self> {
        let mut p = PlasticPhenotype {
            phenotype: tested_phenotype(info)?.clone(),
            num_observations: 0,
            num_trials,
            env_inputs: Vec::new(),
            executed_flags: String::new(),
            viable: false,
        };
        p.add_observation(info)?;
        Ok(p)
    }

    /// `AddObservation(test_info)`: the caller found this entry with
    /// `cPhenotype::Compare == 0`, which is the test made here again.
    fn add_observation(&mut self, info: &CpuTestInfo) -> Result<bool> {
        if Phenotype::compare(tested_phenotype(info)?, &self.phenotype) != 0 {
            return Ok(false);
        }
        if self.num_observations == 0 {
            self.env_inputs = info.test_cpu_inputs().to_vec();
            self.executed_flags = executed_flags(info)?;
            self.viable = info.is_viable();
        }
        self.num_observations += 1;
        Ok(true)
    }

    /// `GetFrequency()`.
    pub fn frequency(&self) -> f64 {
        f64::from(self.num_observations) / f64::from(self.num_trials)
    }
}

/// `cPlasticPhenotype::SetExecutedFlags`.
fn executed_flags(info: &CpuTestInfo) -> Result<String> {
    let org = info
        .test_organism(0)
        .ok_or_else(|| AnalyzeError::fatal("test CPU produced no organism at depth 0"))?;
    let mem = org.hardware.memory();
    Ok((0..mem.size())
        .map(|i| if mem.flag_executed(i) { '+' } else { '-' })
        .collect())
}

/// `cPhenPlastGenotype`: a genome tested `num_trials` times and its
/// distinct phenotypes, kept in `cPhenotype::Compare` order (the C++
/// `std::set<cPhenotype*, PhenotypeCompare>`).
#[derive(Debug, Clone)]
pub struct PhenPlastGenotype {
    pub num_trials: i32,
    pub unique: Vec<PlasticPhenotype>,
    pub max_fitness: f64,
    pub avg_fitness: f64,
    pub likely_fitness: f64,
    pub phenotypic_entropy: f64,
    pub max_freq: f64,
    pub max_fit_freq: f64,
    pub min_fit_freq: f64,
    pub min_fitness: f64,
    pub viable_probability: f64,
    pub task_probabilities: Vec<f64>,
}

impl PhenPlastGenotype {
    /// `cPhenPlastGenotype(genome, num_trials, test_info, world, ctx)`:
    /// a fresh test CPU (`Process`), `num_trials` tests of `genome`, then
    /// the summary statistics. `num_trials > 1` switches `test_info` to
    /// random inputs (the change is visible to the caller, as in C++).
    pub fn new(
        world: &mut TestWorld<'_>,
        genome: &Genome,
        num_trials: i32,
        test_info: &mut CpuTestInfo,
    ) -> Result<Self> {
        if num_trials > 1 {
            test_info.use_random_inputs(true);
        }
        // Process(test_info, world, ctx)
        let mut cpu = hw(TestCpu::new(world))?;
        if num_trials > 1 {
            test_info.use_random_inputs(true);
        }
        let mut unique: Vec<PlasticPhenotype> = Vec::new();
        for _ in 0..num_trials {
            hw(cpu.test_genome(world, test_info, genome))?;
            let phen = tested_phenotype(test_info)?;
            // `m_unique.find`: the set's equivalence is Compare == 0.
            let mut pos = unique.len();
            let mut found = None;
            for (i, u) in unique.iter().enumerate() {
                let c = Phenotype::compare(phen, &u.phenotype);
                if c == 0 {
                    found = Some(i);
                    break;
                }
                if c < 0 {
                    pos = i;
                    break;
                }
            }
            match found {
                Some(i) => {
                    if !unique[i].add_observation(test_info)? {
                        // `cerr << "Error with this plastic phenotype. Abort."; exit(3)`
                        return Err(AnalyzeError::Exit(3));
                    }
                }
                None => {
                    let p = PlasticPhenotype::new(test_info, num_trials)?;
                    unique.insert(pos, p);
                }
            }
        }

        let num_tasks = world.environment.num_tasks().max(0) as usize;
        let mut pp = PhenPlastGenotype {
            num_trials,
            unique,
            max_fitness: -1.0,
            avg_fitness: 0.0,
            likely_fitness: -1.0,
            phenotypic_entropy: 0.0,
            max_freq: 0.0,
            max_fit_freq: 0.0,
            min_fit_freq: 0.0,
            min_fitness: f64::MAX,
            viable_probability: 0.0,
            task_probabilities: vec![0.0; num_tasks],
        };
        for this_phen in &pp.unique {
            let fit = this_phen.phenotype.fitness;
            let freq = this_phen.frequency();
            if fit > pp.max_fitness {
                pp.max_fitness = fit;
                pp.max_fit_freq = freq;
            }
            if fit < pp.min_fitness {
                pp.min_fitness = fit;
                pp.min_fit_freq = freq;
            }
            if freq > pp.max_freq {
                pp.max_freq = freq;
                pp.likely_fitness = fit;
            }
            // `m_avg_fitness += freq * fit;` (fused on the reference build)
            pp.avg_fitness = freq.mul_add(fit, pp.avg_fitness);
            pp.phenotypic_entropy -= freq * freq.ln() / 2.0_f64.ln();
            for (i, tp) in pp.task_probabilities.iter_mut().enumerate() {
                let count = this_phen
                    .phenotype
                    .last_task_count
                    .get(i)
                    .copied()
                    .ok_or_else(|| {
                        AnalyzeError::fatal("phenotype has fewer task counts than the environment")
                    })?;
                *tp += if count > 0 { freq } else { 0.0 };
            }
            pp.viable_probability += if this_phen.viable { freq } else { 0.0 };
        }
        Ok(pp)
    }

    /// `GetNumPhenotypes()`.
    pub fn num_phenotypes(&self) -> i32 {
        self.unique.len() as i32
    }

    /// `GetMostLikelyPhenotype()`: the first phenotype (in set order) with
    /// the highest frequency.
    pub fn most_likely(&self) -> Result<&PlasticPhenotype> {
        let mut best = self
            .unique
            .first()
            .ok_or_else(|| AnalyzeError::fatal("GetMostLikelyPhenotype of an empty set"))?;
        for p in &self.unique {
            if p.frequency() > best.frequency() {
                best = p;
            }
        }
        Ok(best)
    }

    /// `cPhenPlastSummary(pp)`.
    pub fn summary(&self) -> PhenPlastSummary {
        PhenPlastSummary {
            recalculate_trials: self.num_trials,
            num_phenotypes: self.num_phenotypes(),
            min_fitness: self.min_fitness,
            max_fitness: self.max_fitness,
            avg_fitness: self.avg_fitness,
            likely_fitness: self.likely_fitness,
            phenotypic_entropy: self.phenotypic_entropy,
            likely_frequency: self.max_freq,
            min_fit_frequency: self.min_fit_freq,
            max_fit_frequency: self.max_fit_freq,
            viable_probability: self.viable_probability,
            task_probabilities: self.task_probabilities.clone(),
        }
    }
}

/// The test-CPU half of `cAnalyzeGenotype::Recalculate(ctx, test_info,
/// parent, num_trials)`: `cPhenPlastGenotype` in the default context, the
/// fields of its most likely phenotype, and (for `num_trials > 1`) the
/// plasticity summary. `test_info` is shared by a whole batch in C++, so the
/// caller keeps it between genotypes.
pub fn recalculate(
    host: &mut dyn AnalyzeHost,
    genome: &Genome,
    test_info: &mut CpuTestInfo,
    num_trials: i32,
) -> Result<RecalcResult> {
    let mut result: Option<RecalcResult> = None;
    host.with_test_world(TestContext::Default, &mut |w| {
        result = Some(recalculate_in(w, genome, test_info, num_trials)?);
        Ok(())
    })?;
    result.ok_or_else(|| AnalyzeError::fatal("recalculate: the host did not run the test CPU"))
}

/// [`recalculate`] in the world `w`.
pub(crate) fn recalculate_in(
    w: &mut TestWorld<'_>,
    genome: &Genome,
    test_info: &mut CpuTestInfo,
    num_trials: i32,
) -> Result<RecalcResult> {
    let pp = PhenPlastGenotype::new(w, genome, num_trials, test_info)?;
    let likely = pp.most_likely()?;
    let p = &likely.phenotype;
    Ok(RecalcResult {
        viable: likely.viable,
        env_inputs: likely.env_inputs.clone(),
        executed_flags: likely.executed_flags.clone(),
        inst_executed_counts: p.last_inst_count.clone(),
        length: p.genome_length,
        copy_length: p.copied_size,
        exe_length: p.executed_size,
        merit: p.merit.get_double(),
        gest_time: p.gestation_time,
        fitness: p.fitness,
        errors: p.last_num_errors,
        div_type: p.div_type,
        mate_id: p.mate_select_id,
        task_counts: p.last_task_count.clone(),
        task_qualities: p.last_task_quality.clone(),
        internal_task_counts: p.last_internal_task_count.clone(),
        internal_task_qualities: p.last_internal_task_quality.clone(),
        rbins_total: p.last_rbins_total.clone(),
        rbins_avail: p.last_rbins_avail.clone(),
        collect_spec_counts: p.last_collect_spec_counts.clone(),
        mating_type: p.mating_type,
        mate_preference: p.mate_preference,
        mating_display_a: p.cur_mating_display_a,
        mating_display_b: p.cur_mating_display_b,
        phen_plast: (num_trials > 1).then(|| pp.summary()),
    })
}

/// `cAnalyzeGenotype::CheckPhenPlast`: `cPhenPlastGenotype(genome,
/// num_trials, cCPUTestInfo(), world, GetDefaultContext())` summarised.
pub fn phen_plast(
    host: &mut dyn AnalyzeHost,
    genome: &Genome,
    num_trials: i32,
) -> Result<PhenPlastSummary> {
    let mut summary = None;
    let mut info = CpuTestInfo::default();
    host.with_test_world(TestContext::Default, &mut |w| {
        let pp = PhenPlastGenotype::new(w, genome, num_trials, &mut info)?;
        summary = Some(pp.summary());
        Ok(())
    })?;
    summary.ok_or_else(|| AnalyzeError::fatal("phen_plast: the host did not run the test CPU"))
}

/// `cHardwareManager::CreateTestCPU(m_ctx)` in the default context.
pub fn create_test_cpu(host: &mut dyn AnalyzeHost) -> Result<TestCpu> {
    create_test_cpu_in(host, TestContext::Default)
}

/// `cHardwareManager::CreateTestCPU(ctx)`.
pub fn create_test_cpu_in(host: &mut dyn AnalyzeHost, ctx: TestContext<'_>) -> Result<TestCpu> {
    let mut cpu = None;
    host.with_test_world(ctx, &mut |w| {
        cpu = Some(hw(TestCpu::new(w))?);
        Ok(())
    })?;
    cpu.ok_or_else(|| AnalyzeError::fatal("the host did not create a test CPU"))
}

/// The loop of `cAnalyze::CommandPrint`: one test CPU for the command,
/// `cTestCPU::PrintGenome(m_ctx, genome, filename)` per entry. `after_each`
/// runs after each file is written (the verbose `Printing:` line).
pub fn print_genomes(
    host: &mut dyn AnalyzeHost,
    items: &[(Genome, String)],
    after_each: &mut dyn FnMut(&str) -> Result<()>,
) -> Result<()> {
    let mut cpu = create_test_cpu(host)?;
    for (genome, filename) in items {
        let filename = if filename.is_empty() {
            avida_hardware::test_cpu::default_print_filename(genome.representation().size())
        } else {
            filename.clone()
        };
        let mut df = host
            .output()
            .create_with_path(&filename)
            .map_err(AnalyzeError::fatal)?;
        host.with_test_world(TestContext::Default, &mut |w| {
            hw(cpu.print_genome(w, genome, &mut df, &filename, -1, false, 0, -1, -1))
        })?;
        drop(df);
        after_each(&filename)?;
    }
    Ok(())
}

/// A `Write` over an `Avida::Output::File` (the `cHardwareStatusPrinter`'s
/// `m_file`); dropping it closes the file and releases its output id.
struct DataFileWriter(DataFile);

impl Write for DataFileWriter {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        self.0.ofstream().write(buf)
    }
    fn flush(&mut self) -> std::io::Result<()> {
        self.0.ofstream().flush()
    }
}

/// The loop of `cAnalyze::CommandTrace`: one test CPU for the command; for
/// each genome a `cHardwareStatusPrinter` on `filename` and a test with
/// `opts`. `before_each` runs just before each test (the verbose
/// `Tracing <file>` line).
pub fn trace_genomes(
    host: &mut dyn AnalyzeHost,
    items: &[(Genome, String)],
    opts: &CpuTestOptions,
    before_each: &mut dyn FnMut(&str) -> Result<()>,
) -> Result<()> {
    let mut cpu = create_test_cpu(host)?;
    for (genome, filename) in items {
        let df = host
            .output()
            .create_with_path(filename)
            .map_err(AnalyzeError::fatal)?;
        let printer = HardwareStatusPrinter::new(Box::new(DataFileWriter(df)), false);
        let tracer: HardwareTracerPtr = printer.into_ptr();
        let mut info = opts.test_info();
        info.set_trace_execution(Some(tracer));
        before_each(filename)?;
        host.with_test_world(TestContext::Default, &mut |w| {
            hw(cpu.test_genome(w, &mut info, genome)).map(|_| ())
        })?;
        // `test_info` and `tracer` go out of scope: the file closes.
        info.set_trace_execution(None);
    }
    Ok(())
}
