//! `cLandscape` (avida-core/source/main/cLandscape.{h,cc} at 47f13dadb):
//! the fitness landscape of a genome, sampled by testing its mutants in the
//! test CPU.
//!
//! Ported: `Process` (all mutants at a distance, recursively),
//! `ProcessDump`, `ProcessDelete`, `ProcessInsert`, `SampleProcess`,
//! `RandomProcess`, `PrintStats`, `PrintEntropy`, `PrintSiteCount`,
//! `PredictWProcess`, `PredictNuProcess`, `BuildFitnessChart`,
//! `TestPairs`, `TestAllPairs`, `TestMutPair`.
//!
//! Not ported: `HillClimb`. After its `Reset` the distance is 0, so
//! `Process_Body` runs to `size + 1` and reads and writes the site one past
//! the end of the genome; the action fails there. The complexity that the
//! predict functions compute at the end from an all-zero `site_count` is
//! never printed and is left out.

use std::sync::Arc;

use avida_apto::DataFile;
use avida_genome::{Genome, InstSet, InstSetRegistry, Instruction};
use avida_hardware::nhardware::{FITNESS_NEUTRAL_MAX, FITNESS_NEUTRAL_MIN};
use avida_hardware::test_cpu::{CpuTestInfo, TestCpu, TestWorld};

use crate::error::{AnalyzeError, Result};
use crate::host::CpuTestOptions;

fn hw<T>(r: std::result::Result<T, String>) -> Result<T> {
    r.map_err(AnalyzeError::world)
}

/// `m_world->GetHardwareManager().GetInstSet(genome.Properties().Get("instset"))`.
pub(crate) fn inst_set_of<'a>(
    registry: &'a InstSetRegistry,
    genome: &Genome,
) -> Result<&'a InstSet> {
    registry.get_inst_set(genome.inst_set()).ok_or_else(|| {
        AnalyzeError::fatal(format!(
            "no instruction set named '{}' (C++: cHardwareManager::GetInstSet reads past its map)",
            genome.inst_set()
        ))
    })
}

/// [`inst_set_of`] as a shared handle, for loops that also lend the world
/// to the test CPU.
pub(crate) fn shared_inst_set_of(
    registry: &InstSetRegistry,
    genome: &Genome,
) -> Result<Arc<InstSet>> {
    inst_set_of(registry, genome)?;
    registry
        .shared_inst_set(registry.inst_set_id(genome.inst_set()))
        .ok_or_else(|| AnalyzeError::fatal("instruction set vanished from the registry"))
}

/// `cLandscape`.
#[derive(Debug, Clone)]
pub struct Landscape {
    /// The settings of `m_cpu_test_info` (`SetCPUTestInfo`); a fresh
    /// `cCPUTestInfo` with them is used for every process call.
    cpu_test: CpuTestOptions,
    base_genome: Genome,
    peak_genome: Genome,
    base_fitness: f64,
    base_merit: f64,
    base_gestation: f64,
    peak_fitness: f64,
    total_fitness: f64,
    total_sqr_fitness: f64,
    distance: i32,
    trials: i32,
    min_found: i32,
    max_trials: i32,
    total_count: i32,
    dead_count: i32,
    neg_count: i32,
    neut_count: i32,
    pos_count: i32,
    pos_size: f64,
    neg_size: f64,
    total_epi_count: i32,
    pos_epi_count: i32,
    neg_epi_count: i32,
    no_epi_count: i32,
    dead_epi_count: i32,
    pos_epi_size: f64,
    neg_epi_size: f64,
    no_epi_size: f64,
    site_count: Vec<i32>,
    total_entropy: f64,
    complexity: f64,
    neut_min: f64,
    neut_max: f64,
    num_found: i32,
    /// `fitness_chart` (`BuildFitnessChart`): per site and instruction,
    /// the colony fitness of that point mutant.
    fitness_chart: Vec<Vec<f64>>,
}

impl Landscape {
    /// `cLandscape(world, genome)`.
    ///
    /// QUIRK: the constructor initialises `trials` to 1 and then calls
    /// `Reset`, which sets it (and `distance`) to 0.
    pub fn new(genome: &Genome) -> Self {
        let mut l = Landscape {
            cpu_test: CpuTestOptions::default(),
            base_genome: genome.clone(),
            peak_genome: genome.clone(),
            base_fitness: 0.0,
            base_merit: 0.0,
            base_gestation: 0.0,
            peak_fitness: 0.0,
            total_fitness: 0.0,
            total_sqr_fitness: 0.0,
            distance: 0,
            trials: 1,
            min_found: 0,
            max_trials: 0,
            total_count: 0,
            dead_count: 0,
            neg_count: 0,
            neut_count: 0,
            pos_count: 0,
            pos_size: 0.0,
            neg_size: 0.0,
            total_epi_count: 0,
            pos_epi_count: 0,
            neg_epi_count: 0,
            no_epi_count: 0,
            dead_epi_count: 0,
            pos_epi_size: 0.0,
            neg_epi_size: 0.0,
            no_epi_size: 0.0,
            site_count: Vec::new(),
            total_entropy: 0.0,
            complexity: 0.0,
            neut_min: 0.0,
            neut_max: 0.0,
            num_found: 0,
            fitness_chart: Vec::new(),
        };
        l.reset(genome);
        l
    }

    /// `Reset(genome)`.
    pub fn reset(&mut self, genome: &Genome) {
        self.base_genome = genome.clone();
        self.peak_genome = genome.clone();
        self.base_fitness = 0.0;
        self.base_merit = 0.0;
        self.base_gestation = 0.0;
        self.peak_fitness = 0.0;
        self.total_fitness = 0.0;
        self.total_sqr_fitness = 0.0;
        self.distance = 0;
        self.trials = 0;
        self.total_count = 0;
        self.dead_count = 0;
        self.neg_count = 0;
        self.neut_count = 0;
        self.pos_count = 0;
        self.pos_size = 0.0;
        self.neg_size = 0.0;
        self.total_epi_count = 0;
        self.pos_epi_count = 0;
        self.neg_epi_count = 0;
        self.no_epi_count = 0;
        self.dead_epi_count = 0;
        self.pos_epi_size = 0.0;
        self.neg_epi_size = 0.0;
        self.no_epi_size = 0.0;
        let size = genome.representation().size().max(0) as usize;
        self.site_count = vec![0; size + 1];
        self.total_entropy = 0.0;
        self.complexity = 0.0;
        self.neut_min = 0.0;
        self.neut_max = 0.0;
        self.num_found = 0;
    }

    /// `SetDistance`.
    pub fn set_distance(&mut self, d: i32) {
        self.distance = d;
    }
    /// `SetTrials`.
    pub fn set_trials(&mut self, t: i32) {
        self.trials = t;
    }
    /// `SetMinFound`.
    pub fn set_min_found(&mut self, m: i32) {
        self.min_found = m;
    }
    /// `SetMaxTrials`.
    pub fn set_max_trials(&mut self, m: i32) {
        self.max_trials = m;
    }
    /// `SetCPUTestInfo`.
    pub fn set_cpu_test_info(&mut self, opts: &CpuTestOptions) {
        self.cpu_test = opts.clone();
    }

    // -- Results ------------------------------------------------------------

    /// `GetAveFitness()`.
    pub fn ave_fitness(&self) -> f64 {
        self.total_fitness / f64::from(self.total_count)
    }
    /// `GetAveSqrFitness()`.
    pub fn ave_sqr_fitness(&self) -> f64 {
        self.total_sqr_fitness / f64::from(self.total_count)
    }
    /// `GetPeakFitness()`.
    pub fn peak_fitness(&self) -> f64 {
        self.peak_fitness
    }
    /// `GetPeakGenome()`.
    pub fn peak_genome(&self) -> &Genome {
        &self.peak_genome
    }
    /// `GetProbDead()`.
    pub fn prob_dead(&self) -> f64 {
        f64::from(self.dead_count) / f64::from(self.total_count)
    }
    /// `GetProbNeg()`.
    pub fn prob_neg(&self) -> f64 {
        f64::from(self.neg_count) / f64::from(self.total_count)
    }
    /// `GetProbNeut()`.
    pub fn prob_neut(&self) -> f64 {
        f64::from(self.neut_count) / f64::from(self.total_count)
    }
    /// `GetProbPos()`.
    pub fn prob_pos(&self) -> f64 {
        f64::from(self.pos_count) / f64::from(self.total_count)
    }
    /// `GetAvPosSize()`.
    pub fn av_pos_size(&self) -> f64 {
        if self.pos_count == 0 {
            0.0
        } else {
            self.pos_size / f64::from(self.pos_count)
        }
    }
    /// `GetAvNegSize()`.
    pub fn av_neg_size(&self) -> f64 {
        if self.neg_count == 0 {
            0.0
        } else {
            self.neg_size / f64::from(self.neg_count)
        }
    }
    fn epi_ratio(&self, count: i32) -> f64 {
        if self.total_epi_count == 0 {
            0.0
        } else {
            f64::from(count) / f64::from(self.total_epi_count)
        }
    }
    fn epi_size(size: f64, count: i32) -> f64 {
        if count == 0 {
            0.0
        } else {
            size / f64::from(count)
        }
    }
    /// `GetNumTrials()`.
    pub fn num_trials(&self) -> i32 {
        self.trials
    }
    /// `GetNumFound()`.
    pub fn num_found(&self) -> i32 {
        self.num_found
    }
    /// `GetTotalEntropy()`.
    pub fn total_entropy(&self) -> f64 {
        self.total_entropy
    }
    /// `GetComplexity()`.
    pub fn complexity(&self) -> f64 {
        self.complexity
    }

    // -- Processing ----------------------------------------------------------

    /// `ProcessGenome(ctx, testcpu, genome)`.
    fn process_genome(
        &mut self,
        w: &mut TestWorld<'_>,
        cpu: &mut TestCpu,
        info: &mut CpuTestInfo,
        genome: &Genome,
    ) -> Result<f64> {
        hw(cpu.test_genome(w, info, genome))?;
        let test_fitness = info.colony_fitness();
        self.total_fitness += test_fitness;
        // `total_sqr_fitness += test_fitness * test_fitness;` (fused)
        self.total_sqr_fitness = test_fitness.mul_add(test_fitness, self.total_sqr_fitness);
        self.total_count += 1;
        if test_fitness == 0.0 {
            self.dead_count += 1;
        } else if test_fitness < self.neut_min {
            self.neg_count += 1;
            self.neg_size += test_fitness;
        } else if test_fitness <= self.neut_max {
            self.neut_count += 1;
        } else {
            self.pos_count += 1;
            self.pos_size += test_fitness;
            if test_fitness > self.peak_fitness {
                self.peak_fitness = test_fitness;
                self.peak_genome = genome.clone();
            }
        }
        Ok(test_fitness)
    }

    /// `ProcessBase(ctx, testcpu)`.
    fn process_base(
        &mut self,
        w: &mut TestWorld<'_>,
        cpu: &mut TestCpu,
        info: &mut CpuTestInfo,
    ) -> Result<()> {
        let base = self.base_genome.clone();
        hw(cpu.test_genome(w, info, &base))?;
        let org = info.colony_organism().ok_or_else(|| {
            AnalyzeError::fatal(
                "cLandscape::ProcessBase: no colony organism (C++ NULL dereference)",
            )
        })?;
        let phenotype = &org.state.phenotype;
        self.base_fitness = info.colony_fitness();
        self.base_merit = phenotype.merit.get_double();
        self.base_gestation = f64::from(phenotype.gestation_time);
        self.peak_fitness = self.base_fitness;
        self.peak_genome = base;
        self.neut_min = self.base_fitness * FITNESS_NEUTRAL_MIN;
        self.neut_max = self.base_fitness * FITNESS_NEUTRAL_MAX;
        Ok(())
    }

    /// The test CPU and test info a process call starts with
    /// (`CreateTestCPU(ctx)` and `m_cpu_test_info`).
    fn start(&self, w: &mut TestWorld<'_>) -> Result<(TestCpu, CpuTestInfo)> {
        Ok((hw(TestCpu::new(w))?, self.cpu_test.test_info()))
    }

    /// `Process(ctx)`: every mutant at `distance`, then the entropy and
    /// complexity of the genome.
    pub fn process(&mut self, w: &mut TestWorld<'_>) -> Result<()> {
        let (mut cpu, mut info) = self.start(w)?;
        self.process_base(w, &mut cpu, &mut info)?;
        let base = self.base_genome.clone();
        let distance = self.distance;
        self.process_body(w, &mut cpu, &mut info, &base, distance, 0)?;

        let inst_size = inst_set_of(w.hardware_manager.registry(), &self.base_genome)?.size();
        let max_ent = f64::from(inst_size).ln();
        self.total_entropy = 0.0;
        let size = self.base_genome.representation().size();
        for i in 0..size.max(0) as usize {
            // Per-site entropy is the log of the number of legal states for
            // that site, plus one for the unmutated state.
            self.total_entropy += (f64::from(self.site_count[i]) + 1.0).ln() / max_ent;
        }
        self.complexity = f64::from(size) - self.total_entropy;
        self.num_found = size.wrapping_mul(inst_size - 1);
        Ok(())
    }

    /// `Process_Body(ctx, testcpu, cur_genome, cur_distance, start_line)`.
    fn process_body(
        &mut self,
        w: &mut TestWorld<'_>,
        cpu: &mut TestCpu,
        info: &mut CpuTestInfo,
        cur_genome: &Genome,
        cur_distance: i32,
        start_line: i32,
    ) -> Result<()> {
        let max_line = self.base_genome.representation().size() - cur_distance + 1;
        let inst_size = inst_set_of(w.hardware_manager.registry(), &self.base_genome)?.size();
        let mut mg = cur_genome.clone();
        for line_num in start_line..max_line {
            let cur_inst = self.base_genome.representation().get(line_num).op();
            for inst_num in 0..inst_size {
                if cur_inst == inst_num {
                    continue;
                }
                mg.representation_mut()
                    .set(line_num, Instruction::new(inst_num));
                if cur_distance <= 1 {
                    self.process_genome(w, cpu, info, &mg)?;
                    if info.colony_fitness() >= self.neut_min {
                        self.site_count[line_num as usize] += 1;
                    }
                } else {
                    self.process_body(w, cpu, info, &mg, cur_distance - 1, line_num + 1)?;
                }
            }
            mg.representation_mut()
                .set(line_num, Instruction::new(cur_inst));
        }
        Ok(())
    }

    /// `ProcessDump(ctx, df)`: the fitness of every single-site mutant, one
    /// line per site.
    pub fn process_dump(&mut self, w: &mut TestWorld<'_>, df: &mut DataFile) -> Result<()> {
        df.write_comment("Detailed dump of the per-site, per-instruction fitness");
        df.write_comment("values for the entire single-step landscape.");
        let (mut cpu, mut info) = self.start(w)?;
        self.process_base(w, &mut cpu, &mut info)?;
        let max_line = self.base_genome.representation().size();
        let inst_size = inst_set_of(w.hardware_manager.registry(), &self.base_genome)?.size();
        let mut mg = self.base_genome.clone();
        for line_num in 0..max_line {
            let cur_inst = self.base_genome.representation().get(line_num).op();
            df.write(cur_inst, "Original Instruction");
            for inst_num in 0..inst_size {
                let fitness = if cur_inst == inst_num {
                    self.base_fitness
                } else {
                    mg.representation_mut()
                        .set(line_num, Instruction::new(inst_num));
                    self.process_genome(w, &mut cpu, &mut info, &mg)?
                };
                df.write(
                    fitness,
                    "Mutation Fitness (instruction = column_number - 2)",
                );
            }
            df.endl();
            mg.representation_mut()
                .set(line_num, Instruction::new(cur_inst));
        }
        Ok(())
    }

    /// `ProcessDelete(ctx)`: every single-site deletion.
    pub fn process_delete(&mut self, w: &mut TestWorld<'_>) -> Result<()> {
        let (mut cpu, mut info) = self.start(w)?;
        self.process_base(w, &mut cpu, &mut info)?;
        let max_line = self.base_genome.representation().size();
        let mut mg = self.base_genome.clone();
        for line_num in 0..max_line {
            let cur_inst = self.base_genome.representation().get(line_num);
            mg.representation_mut().remove(line_num, 1);
            self.process_genome(w, &mut cpu, &mut info, &mg)?;
            if info.colony_fitness() >= self.neut_min {
                self.site_count[line_num as usize] += 1;
            }
            mg.representation_mut().insert(line_num, cur_inst);
        }
        Ok(())
    }

    /// `ProcessInsert(ctx)`: every single-site insertion.
    pub fn process_insert(&mut self, w: &mut TestWorld<'_>) -> Result<()> {
        let (mut cpu, mut info) = self.start(w)?;
        self.process_base(w, &mut cpu, &mut info)?;
        let max_line = self.base_genome.representation().size();
        let inst_size = inst_set_of(w.hardware_manager.registry(), &self.base_genome)?.size();
        let mut mg = self.base_genome.clone();
        for line_num in 0..=max_line {
            for inst_num in 0..inst_size {
                mg.representation_mut()
                    .insert(line_num, Instruction::new(inst_num));
                self.process_genome(w, &mut cpu, &mut info, &mg)?;
                if info.colony_fitness() >= self.neut_min {
                    // `site_count` has size + 1 entries, so the insertion
                    // after the last site is counted too.
                    self.site_count[line_num as usize] += 1;
                }
                mg.representation_mut().remove(line_num, 1);
            }
        }
        Ok(())
    }

    /// `SampleProcess(ctx)`: `trials` random single-site mutants per site
    /// (default: instruction set size - 1).
    pub fn sample_process(&mut self, w: &mut TestWorld<'_>) -> Result<()> {
        self.distance = 1;
        let mut mod_genome = self.base_genome.clone();
        let genome_size = self.base_genome.representation().size();
        let (mut cpu, mut info) = self.start(w)?;
        let inst_set = shared_inst_set_of(w.hardware_manager.registry(), &self.base_genome)?;
        self.process_base(w, &mut cpu, &mut info)?;
        if self.trials == 0 {
            self.trials = inst_set.size() - 1;
        }
        if inst_set.size() < 2 && self.trials > 0 {
            return Err(AnalyzeError::fatal(
                "SampleLandscape with a one-instruction set: cLandscape::SampleProcess never ends",
            ));
        }
        for line_num in 0..genome_size {
            let cur_inst = self.base_genome.representation().get(line_num);
            let mut i = 0;
            while i < self.trials {
                let new_inst = inst_set.random_inst(w.rng);
                if cur_inst == new_inst {
                    continue;
                }
                mod_genome.representation_mut().set(line_num, new_inst);
                self.process_genome(w, &mut cpu, &mut info, &mod_genome)?;
                i += 1;
            }
            mod_genome.representation_mut().set(line_num, cur_inst);
        }
        Ok(())
    }

    /// `RandomProcess(ctx)`: `trials` random mutants at `distance` (more
    /// while fewer than `min_found` were found, up to `max_trials`).
    pub fn random_process(&mut self, w: &mut TestWorld<'_>) -> Result<()> {
        let mut mod_genome = self.base_genome.clone();
        let genome_size = self.base_genome.representation().size();
        let (mut cpu, mut info) = self.start(w)?;
        let inst_set = shared_inst_set_of(w.hardware_manager.registry(), &self.base_genome)?;
        self.process_base(w, &mut cpu, &mut info)?;
        if self.trials == 0 {
            self.trials = inst_set.size() - 1;
        }
        let distance = self.distance.max(0) as usize;
        if distance > genome_size.max(0) as usize || (distance > 0 && inst_set.size() < 2) {
            return Err(AnalyzeError::fatal(format!(
                "RandomLandscape distance {} on a genome of {} sites: cLandscape::RandomProcess never ends",
                self.distance, genome_size
            )));
        }
        let mut mut_lines = vec![0i32; distance];
        let total_found = 0;
        let mut cur_trial = 0;
        while cur_trial < self.trials
            || (total_found < self.min_found && cur_trial < self.max_trials)
        {
            w.rng.choose(genome_size, &mut mut_lines);
            let mut mut_num = 0;
            while mut_num < distance {
                let new_inst = inst_set.random_inst(w.rng);
                let line = mut_lines[mut_num];
                if self.base_genome.representation().get(line) == new_inst {
                    continue;
                }
                mod_genome.representation_mut().set(line, new_inst);
                mut_num += 1;
            }
            self.process_genome(w, &mut cpu, &mut info, &mod_genome)?;
            for &line in &mut_lines {
                let orig = self.base_genome.representation().get(line);
                mod_genome.representation_mut().set(line, orig);
            }
            cur_trial += 1;
        }
        self.trials = cur_trial;
        // QUIRK: `total_found` is never incremented in C++.
        self.num_found = total_found;
        Ok(())
    }

    // -- Predictions and pairs ----------------------------------------------

    /// `BuildFitnessChart(ctx, testcpu)`: the colony fitness of every
    /// point mutant (the base fitness for the unmutated instruction). Every
    /// test also counts in the landscape statistics (`ProcessGenome`).
    fn build_fitness_chart(
        &mut self,
        w: &mut TestWorld<'_>,
        cpu: &mut TestCpu,
        info: &mut CpuTestInfo,
    ) -> Result<()> {
        let max_line = self.base_genome.representation().size();
        let inst_size = inst_set_of(w.hardware_manager.registry(), &self.base_genome)?.size();
        self.fitness_chart = vec![vec![0.0; inst_size.max(0) as usize]; max_line.max(0) as usize];
        let mut mod_genome = self.base_genome.clone();
        for line_num in 0..max_line {
            let cur_inst = self.base_genome.representation().get(line_num).op();
            for inst_num in 0..inst_size {
                let fitness = if cur_inst == inst_num {
                    self.base_fitness
                } else {
                    mod_genome
                        .representation_mut()
                        .set(line_num, Instruction::new(inst_num));
                    self.process_genome(w, cpu, info, &mod_genome)?;
                    info.colony_fitness()
                };
                self.fitness_chart[line_num as usize][inst_num as usize] = fitness;
            }
            mod_genome
                .representation_mut()
                .set(line_num, Instruction::new(cur_inst));
        }
        Ok(())
    }

    /// `PredictWProcess(ctx, df, update)`: the single-mutant fitness
    /// distribution from the fitness chart, then 2 to 10 mutations
    /// predicted by multiplying random chart entries (at least 10000
    /// samples, more until 100 are neutral or beneficial), one line each.
    pub fn predict_w_process(
        &mut self,
        w: &mut TestWorld<'_>,
        df: &mut DataFile,
        update: i32,
    ) -> Result<()> {
        self.predict_process(w, df, update, false)
    }

    /// `PredictNuProcess(ctx, df, update)`: as [`Self::predict_w_process`]
    /// with only changed instructions, the maximum fitness found and the
    /// living-mutant count.
    ///
    /// QUIRK: "Total Tests" is always the number of chart entries,
    /// unmutated ones included (they count as lethal in the first line).
    pub fn predict_nu_process(
        &mut self,
        w: &mut TestWorld<'_>,
        df: &mut DataFile,
        update: i32,
    ) -> Result<()> {
        self.predict_process(w, df, update, true)
    }

    fn predict_process(
        &mut self,
        w: &mut TestWorld<'_>,
        df: &mut DataFile,
        update: i32,
        nu: bool,
    ) -> Result<()> {
        let (mut cpu, mut info) = self.start(w)?;
        self.distance = 1;
        self.process_base(w, &mut cpu, &mut info)?;
        if self.base_fitness == 0.0 {
            return Ok(());
        }
        self.build_fitness_chart(w, &mut cpu, &mut info)?;
        let genome_size = self.fitness_chart.len() as i32;
        let inst_size = inst_set_of(w.hardware_manager.registry(), &self.base_genome)?.size();
        let base_ops: Vec<i32> = (0..genome_size)
            .map(|i| self.base_genome.representation().get(i).op())
            .collect();
        const MIN_NEUT_FITNESS: f64 = 0.99;
        const MAX_NEUT_FITNESS: f64 = 1.01;
        let classify = |f: f64, pos: &mut i32, neut: &mut i32, neg: &mut i32| {
            if f > MAX_NEUT_FITNESS {
                *pos += 1;
            } else if f > MIN_NEUT_FITNESS {
                *neut += 1;
            } else if f > 0.0 {
                *neg += 1;
            }
        };

        let (mut pos, mut neut, mut neg) = (0i32, 0i32, 0i32);
        let mut max_found_fitness = 0.0f64;
        let mut total_fitness = 0.0f64;
        let mut total_sqr_fitness = 0.0f64;
        for (row, &base_inst) in self.fitness_chart.iter_mut().zip(&base_ops) {
            let mut max_line_fitness = 1.0f64;
            for (col, cur) in row.iter_mut().enumerate() {
                if nu && col as i32 == base_inst {
                    continue;
                }
                *cur /= self.base_fitness;
                total_fitness += *cur;
                total_sqr_fitness = cur.mul_add(*cur, total_sqr_fitness);
                classify(*cur, &mut pos, &mut neut, &mut neg);
                if *cur > max_line_fitness {
                    max_line_fitness = *cur;
                }
            }
            if max_line_fitness > max_found_fitness {
                max_found_fitness = max_line_fitness;
            }
        }
        let total_tests = genome_size.wrapping_mul(inst_size);
        let live = pos + neut + neg;
        let tt = f64::from(total_tests);
        df.write(update, "Update");
        df.write(1, "Number of Mutations");
        df.write(f64::from(total_tests - live) / tt, "Probability Lethal");
        df.write(f64::from(neg) / tt, "Probability Deleterious");
        df.write(f64::from(neut) / tt, "Probability Neutral");
        df.write(f64::from(pos) / tt, "Probability Beneficial");
        df.write(total_tests, "Total Tests");
        if nu {
            df.write(live, "Total Living Mutants");
        } else {
            df.write(neut + pos, "Total Neutral and Beneficial");
        }
        df.write(total_fitness / tt, "Average Fitness");
        df.write(total_sqr_fitness / tt, "Average Square Fitness");
        if nu {
            df.write(max_found_fitness, "Maximum Fitness");
        }
        df.endl();

        const MAX_MUTS: i32 = 10;
        const MIN_FOUND: i32 = 100;
        const MIN_TESTS: i32 = 10000;
        const MAX_TESTS: i32 = 1_000_000_000;
        for num_muts in 2..=MAX_MUTS {
            if nu && inst_size < 2 {
                return Err(AnalyzeError::fatal(
                    "cLandscape::PredictNuProcess with a one-instruction set: drawing a changed instruction never ends",
                ));
            }
            if genome_size < num_muts {
                return Err(AnalyzeError::fatal(format!(
                    "cLandscape::Predict{}Process: choosing {num_muts} of {genome_size} sites (C++ Choose never ends)",
                    if nu { "Nu" } else { "W" }
                )));
            }
            let (mut pos, mut neut, mut neg) = (0i32, 0i32, 0i32);
            let mut total_fitness = 0.0f64;
            let mut total_sqr_fitness = 0.0f64;
            let mut max_found_fitness = 0.0f64;
            let mut mut_lines = vec![0i32; num_muts as usize];
            let mut test_id = 0i32;
            let found =
                |pos: i32, neut: i32, neg: i32| if nu { pos + neut + neg } else { pos + neut };
            while test_id < MIN_TESTS || (test_id < MAX_TESTS && found(pos, neut, neg) < MIN_FOUND)
            {
                w.rng.choose(genome_size, &mut mut_lines);
                let mut test_fitness = 1.0f64;
                for &line in &mut_lines {
                    if test_fitness == 0.0 {
                        break;
                    }
                    let col = if nu {
                        let base_inst = base_ops[line as usize];
                        let mut m = w.rng.get_uint(inst_size as u32) as i32;
                        while m == base_inst {
                            m = w.rng.get_uint(inst_size as u32) as i32;
                        }
                        m
                    } else {
                        w.rng.get_uint(inst_size as u32) as i32
                    };
                    test_fitness *= self.fitness_chart[line as usize][col as usize];
                }
                total_fitness += test_fitness;
                total_sqr_fitness = test_fitness.mul_add(test_fitness, total_sqr_fitness);
                if test_fitness > max_found_fitness {
                    max_found_fitness = test_fitness;
                }
                classify(test_fitness, &mut pos, &mut neut, &mut neg);
                test_id += 1;
            }
            let live = pos + neut + neg;
            let n = f64::from(test_id);
            df.write(update, "Update");
            df.write(num_muts, "Number of Mutations");
            df.write(f64::from(test_id - live) / n, "Probability Lethal");
            df.write(f64::from(neg) / n, "Probability Deleterious");
            df.write(f64::from(neut) / n, "Probability Neutral");
            df.write(f64::from(pos) / n, "Probability Beneficial");
            if nu {
                df.write(total_tests, "Total Tests");
                df.write(live, "Total Living Mutants");
            } else {
                df.write(test_id, "Total Tests");
                df.write(neut + pos, "Total Neutral and Beneficial");
            }
            df.write(total_fitness / n, "Average Fitness");
            df.write(total_sqr_fitness / n, "Average Square Fitness");
            if nu {
                df.write(max_found_fitness, "Maximum Fitness");
            }
            df.endl();
            if found(pos, neut, neg) < MIN_FOUND / 2 {
                break;
            }
        }
        Ok(())
    }

    /// `TestPairs(ctx)`: `trials` random pairs of point mutations, each
    /// classified by its epistasis against the fitness chart.
    pub fn test_pairs(&mut self, w: &mut TestWorld<'_>) -> Result<()> {
        let (mut cpu, mut info) = self.start(w)?;
        let inst_set = shared_inst_set_of(w.hardware_manager.registry(), &self.base_genome)?;
        self.process_base(w, &mut cpu, &mut info)?;
        if self.base_fitness == 0.0 {
            return Ok(());
        }
        self.build_fitness_chart(w, &mut cpu, &mut info)?;
        let mut mod_genome = self.base_genome.clone();
        let genome_size = self.base_genome.representation().size();
        if inst_set.size() < 2 && self.trials > 0 {
            return Err(AnalyzeError::fatal(
                "cLandscape::TestPairs with a one-instruction set: drawing a changed instruction never ends",
            ));
        }
        if genome_size < 2 && self.trials > 0 {
            return Err(AnalyzeError::fatal(format!(
                "cLandscape::TestPairs: choosing 2 of {genome_size} sites (C++ Choose never ends)"
            )));
        }
        let mut mut_lines = [0i32; 2];
        let mut mut_insts = [Instruction::default(); 2];
        for _ in 0..self.trials {
            w.rng.choose(genome_size, &mut mut_lines);
            let mut mut_num = 0;
            while mut_num < 2 {
                let new_inst = inst_set.random_inst(w.rng);
                let cur_inst = self.base_genome.representation().get(mut_lines[mut_num]);
                if cur_inst == new_inst {
                    continue;
                }
                mut_insts[mut_num] = new_inst;
                mut_num += 1;
            }
            self.test_mut_pair(
                w,
                &mut cpu,
                &mut info,
                &mut mod_genome,
                (mut_lines[0], mut_lines[1]),
                (mut_insts[0], mut_insts[1]),
            )?;
        }
        Ok(())
    }

    /// `TestAllPairs(ctx)`: every pair of point mutations at two sites.
    pub fn test_all_pairs(&mut self, w: &mut TestWorld<'_>) -> Result<()> {
        let (mut cpu, mut info) = self.start(w)?;
        self.process_base(w, &mut cpu, &mut info)?;
        if self.base_fitness == 0.0 {
            return Ok(());
        }
        self.build_fitness_chart(w, &mut cpu, &mut info)?;
        let max_line = self.base_genome.representation().size();
        let inst_size = inst_set_of(w.hardware_manager.registry(), &self.base_genome)?.size();
        let mut mod_genome = self.base_genome.clone();
        for line1 in 0..max_line - 1 {
            for line2 in line1 + 1..max_line {
                for inst1 in 0..inst_size {
                    let inst1 = Instruction::new(inst1);
                    if inst1 == self.base_genome.representation().get(line1) {
                        continue;
                    }
                    for inst2 in 0..inst_size {
                        let inst2 = Instruction::new(inst2);
                        if inst2 == self.base_genome.representation().get(line2) {
                            continue;
                        }
                        self.test_mut_pair(
                            w,
                            &mut cpu,
                            &mut info,
                            &mut mod_genome,
                            (line1, line2),
                            (inst1, inst2),
                        )?;
                    }
                }
            }
        }
        Ok(())
    }

    /// `TestMutPair(ctx, testcpu, mod_genome, line1, line2, mut1, mut2)`.
    fn test_mut_pair(
        &mut self,
        w: &mut TestWorld<'_>,
        cpu: &mut TestCpu,
        info: &mut CpuTestInfo,
        mod_genome: &mut Genome,
        (line1, line2): (i32, i32),
        (mut1, mut2): (Instruction, Instruction),
    ) -> Result<f64> {
        mod_genome.representation_mut().set(line1, mut1);
        mod_genome.representation_mut().set(line2, mut2);
        hw(cpu.test_genome(w, info, mod_genome))?;
        let combo_fitness = info.colony_fitness() / self.base_fitness;
        let base = self.base_genome.representation();
        mod_genome.representation_mut().set(line1, base.get(line1));
        mod_genome.representation_mut().set(line2, base.get(line2));
        let chart = |line: i32, inst: Instruction| -> Result<f64> {
            self.fitness_chart
                .get(line as usize)
                .and_then(|r| r.get(inst.op() as usize))
                .copied()
                .ok_or_else(|| {
                    AnalyzeError::fatal("cLandscape::TestMutPair: outside the fitness chart")
                })
        };
        let mut1_fitness = chart(line1, mut1)? / self.base_fitness;
        let mut2_fitness = chart(line2, mut2)? / self.base_fitness;
        let mult_combo = mut1_fitness * mut2_fitness;
        self.total_epi_count += 1;
        if (mut1_fitness == 0.0 || mut2_fitness == 0.0) && combo_fitness == 0.0 {
            self.dead_epi_count += 1;
        } else if combo_fitness < mult_combo {
            self.neg_epi_count += 1;
            self.neg_epi_size += combo_fitness;
        } else if combo_fitness > mult_combo {
            self.pos_epi_count += 1;
            self.pos_epi_size += combo_fitness;
        } else {
            self.no_epi_count += 1;
            self.no_epi_size += combo_fitness;
        }
        Ok(combo_fitness)
    }

    // -- Output --------------------------------------------------------------

    /// `PrintStats(df, update)`.
    pub fn print_stats(&self, df: &mut DataFile, update: i32) {
        df.write(update, "Update");
        df.write(self.prob_dead(), "Probability Lethal");
        df.write(self.prob_neg(), "Probability Deleterious");
        df.write(self.prob_neut(), "Probability Neutral");
        df.write(self.prob_pos(), "Probability Beneficial");
        df.write(self.av_pos_size(), "Average Beneficial Size");
        df.write(self.av_neg_size(), "Average Deleterious Size");
        df.write(self.total_count, "Total Mutants");
        df.write(self.distance, "Distance");
        df.write(self.base_fitness, "Base Fitness");
        df.write(self.base_merit, "Base Merit");
        df.write(self.base_gestation, "Base Gestation");
        df.write(self.peak_fitness, "Peak Fitness");
        df.write(self.ave_fitness(), "Average Fitness");
        df.write(self.ave_sqr_fitness(), "Average Square Fitness");
        df.write(self.total_entropy, "Total Entropy");
        df.write(self.complexity, "Total Complexity");
        df.write(
            self.epi_ratio(self.dead_epi_count),
            "Probability Lethal Epistasis",
        );
        df.write(
            self.epi_ratio(self.pos_epi_count),
            "Probability Synergistic Epistasis",
        );
        df.write(
            self.epi_ratio(self.neg_epi_count),
            "Probability Antagonistic Epistasis",
        );
        df.write(
            self.epi_ratio(self.no_epi_count),
            "Probability No Epistasis",
        );
        df.write(
            Self::epi_size(self.pos_epi_size, self.pos_epi_count),
            "Average Synergistic Epistasis Size",
        );
        df.write(
            Self::epi_size(self.neg_epi_size, self.neg_epi_count),
            "Average Antagonistic Epistasis Size",
        );
        df.write(
            Self::epi_size(self.no_epi_size, self.no_epi_count),
            "Average Size - No Epistasis",
        );
        df.write(self.total_epi_count, "Total Epistasis Count");
        df.endl();
    }

    /// `PrintEntropy(df)`.
    pub fn print_entropy(&self, df: &mut DataFile, registry: &InstSetRegistry) -> Result<()> {
        df.write_comment("Entropy Data");
        let max_ent = f64::from(inst_set_of(registry, &self.base_genome)?.size()).ln();
        for j in 0..self.base_genome.representation().size().max(0) as usize {
            df.write_anonymous((f64::from(self.site_count[j]) + 1.0).ln() / max_ent);
        }
        df.endl();
        Ok(())
    }

    /// `PrintSiteCount(df)`.
    pub fn print_site_count(&self, df: &mut DataFile) {
        df.write_comment("Site Counts");
        for j in 0..self.base_genome.representation().size().max(0) as usize {
            df.write_anonymous(self.site_count[j]);
        }
        df.endl();
    }
}
