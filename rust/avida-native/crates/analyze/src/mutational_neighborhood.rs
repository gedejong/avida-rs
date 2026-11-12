//! `cMutationalNeighborhood` and `cMutationalNeighborhoodResults`
//! (avida-core/source/analyze/cMutationalNeighborhood.{h,cc} and
//! cMutationalNeighborhoodResults.h at 47f13dadb): every one- and two-step
//! point, insertion and deletion mutant of a genome, tested in the test CPU
//! and summarised per mutation class (fitness effect, site entropy and
//! complexity, task conferral and knockout).
//!
//! The C++ splits the work into one analyze job per site; the jobs write
//! per-site records that `ProcessComplete` folds together in site order, so
//! the result does not depend on the order the jobs ran in.
//! `cMutationalNeighborhoodResults` only takes a read lock and forwards to
//! the getters, so it is folded into [`MutationalNeighborhood::print_stats`].
//!
//! The current C++ tree classifies fitness through the FFI helper
//! `avd_landscape_fitness_category`; this port follows the original inline
//! comparisons.

use avida_apto::DataFile;
use avida_genome::{Genome, InstSetRegistry, Instruction};
use avida_hardware::nhardware::{FITNESS_NEUTRAL_MAX, FITNESS_NEUTRAL_MIN};
use avida_hardware::test_cpu::{CpuTestInfo, TestCpu, TestWorld};

use crate::error::{AnalyzeError, Result};
use crate::landscape::inst_set_of;

fn hw<T>(r: std::result::Result<T, String>) -> Result<T> {
    r.map_err(AnalyzeError::world)
}

/// The one-step fitness table an `sPendFit` reads (its `fmat` reference).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum FitTable {
    /// `m_fitness_point`
    Point,
    /// `m_fitness_insert`
    Insert,
    /// `m_fitness_delete`
    Delete,
}

/// `sPendFit`: a one-step mutant whose fitness is looked up once every site
/// is done.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct PendFit {
    table: FitTable,
    site: i32,
    inst: i32,
}

impl PendFit {
    fn new(table: FitTable, site: i32, inst: i32) -> Self {
        PendFit { table, site, inst }
    }
}

/// `tMatrix<double>`: the one-step fitness tables, row = site, column =
/// instruction (one column for deletions).
#[derive(Debug, Clone, Default)]
struct FitMatrix {
    cols: usize,
    data: Vec<f64>,
}

impl FitMatrix {
    /// `ResizeClear(rows, cols)`. The C++ leaves the cells uninitialised;
    /// every cell read later has been written first.
    fn resize_clear(&mut self, rows: i32, cols: i32) {
        self.cols = cols.max(0) as usize;
        self.data = vec![0.0; rows.max(0) as usize * self.cols];
    }

    fn get(&self, row: i32, col: i32) -> f64 {
        self.data[row as usize * self.cols + col as usize]
    }

    fn set(&mut self, row: i32, col: i32, value: f64) {
        self.data[row as usize * self.cols + col as usize] = value;
    }
}

/// `sStep`, and `sTwoStep` (which adds the `pending` list): the counts of
/// one site's mutants of one class.
#[derive(Debug, Clone, Default)]
struct Step {
    total: i32,
    total_fitness: f64,
    total_sqr_fitness: f64,
    peak_genome: Genome,
    peak_fitness: f64,
    pos: i32,
    neg: i32,
    neut: i32,
    dead: i32,
    size_pos: f64,
    size_neg: f64,
    site_count: Vec<i32>,
    task_target: i32,
    task_total: i32,
    task_knockout: i32,
    task_size_target: f64,
    task_size_total: f64,
    task_size_knockout: f64,
    /// `sTwoStep::pending`, a `tList` used as a stack (`Push` adds at the
    /// front, `Pop` takes from the front). Empty for one-step records.
    pending: Vec<PendFit>,
}

impl Step {
    /// The set-up `Process` gives every record before testing:
    /// `peak_fitness = m_base_fitness; peak_genome = m_base_genome;
    /// site_count.Resize(sites, 0)`.
    fn setup(&mut self, base: &Base, sites: i32) {
        self.peak_fitness = base.fitness;
        self.peak_genome = base.genome.clone();
        self.site_count.resize(sites.max(0) as usize, 0);
    }
}

/// `sOneStepAggregate`, and `sTwoStepAggregate` (which adds the
/// `task_target_*` fields; zero for the one-step aggregates).
#[derive(Debug, Clone, Default)]
struct Aggregate {
    total: i32,
    total_fitness: f64,
    total_sqr_fitness: f64,
    peak_genome: Genome,
    peak_fitness: f64,
    pos: i32,
    neg: i32,
    neut: i32,
    dead: i32,
    size_pos: f64,
    size_neg: f64,
    site_count: Vec<i32>,
    total_entropy: f64,
    complexity: f64,
    task_target: i32,
    task_total: i32,
    task_knockout: i32,
    task_size_target: f64,
    task_size_total: f64,
    task_size_knockout: f64,
    task_target_pos: i32,
    task_target_neg: i32,
    task_target_neut: i32,
    task_target_dead: i32,
    task_size_target_pos: f64,
    task_size_target_neg: f64,
}

impl Aggregate {
    /// The set-up `ProcessComplete` gives an aggregate before folding.
    fn setup(&mut self, base: &Base, sites: i32) {
        self.peak_fitness = base.fitness;
        self.peak_genome = base.genome.clone();
        self.site_count.resize(sites.max(0) as usize, 0);
    }

    /// `Get*AverageFitness()`: `total_fitness / total`.
    fn average_fitness(&self) -> f64 {
        self.total_fitness / f64::from(self.total)
    }
    /// `Get*AverageSqrFitness()`.
    fn average_sqr_fitness(&self) -> f64 {
        self.total_sqr_fitness / f64::from(self.total)
    }
    /// `Get*Prob*()`: `double(count) / total`.
    fn prob(&self, count: i32) -> f64 {
        f64::from(count) / f64::from(self.total)
    }
    /// `Get*AverageSize*()`: `size / count`, 0 when `count` is 0.
    fn average_size(size: f64, count: i32) -> f64 {
        if count == 0 {
            0.0
        } else {
            size / f64::from(count)
        }
    }
    /// `Get*ProbTargetTask{Beneficial,...}()`: `double(count) / (2 *
    /// task_target)`, 0 when no mutant conferred the target.
    ///
    /// QUIRK: the divisor `2 * task_target` is C `int` arithmetic (each
    /// conferral pushed two candidate first steps), converted only for the
    /// division.
    fn prob_previous(&self, count: i32) -> f64 {
        if self.task_target == 0 {
            0.0
        } else {
            f64::from(count) / f64::from(2i32.wrapping_mul(self.task_target))
        }
    }
    /// `Get*AverageSizeTargetTask{Beneficial,Deleterious}()`.
    fn size_previous(&self, size: f64) -> f64 {
        if self.task_target == 0 {
            0.0
        } else {
            size / f64::from(2i32.wrapping_mul(self.task_target))
        }
    }

    /// The fields `ProcessComplete` sums across the classes for `m_ot` and
    /// `m_tt`, except the peak.
    ///
    /// QUIRK: each total is one C++ expression `a + b + c (+ ...)`, so the
    /// doubles are added left to right in class order (point, insert,
    /// delete, then insert/point, insert/delete, delete/point); the fold
    /// starts from the first term, not from 0.0.
    fn sum_of(parts: &[&Aggregate]) -> Aggregate {
        let int = |f: fn(&Aggregate) -> i32| {
            parts
                .iter()
                .map(|a| f(a))
                .fold(None, |acc: Option<i32>, v| {
                    Some(acc.map_or(v, |s| s.wrapping_add(v)))
                })
                .unwrap_or(0)
        };
        let dbl = |f: fn(&Aggregate) -> f64| {
            parts
                .iter()
                .map(|a| f(a))
                .fold(None, |acc: Option<f64>, v| Some(acc.map_or(v, |s| s + v)))
                .unwrap_or(0.0)
        };
        Aggregate {
            total: int(|a| a.total),
            total_fitness: dbl(|a| a.total_fitness),
            total_sqr_fitness: dbl(|a| a.total_sqr_fitness),
            pos: int(|a| a.pos),
            neg: int(|a| a.neg),
            neut: int(|a| a.neut),
            dead: int(|a| a.dead),
            size_pos: dbl(|a| a.size_pos),
            size_neg: dbl(|a| a.size_neg),
            task_target: int(|a| a.task_target),
            task_total: int(|a| a.task_total),
            task_knockout: int(|a| a.task_knockout),
            task_size_target: dbl(|a| a.task_size_target),
            task_size_total: dbl(|a| a.task_size_total),
            task_size_knockout: dbl(|a| a.task_size_knockout),
            task_target_pos: int(|a| a.task_target_pos),
            task_target_neg: int(|a| a.task_target_neg),
            task_target_neut: int(|a| a.task_target_neut),
            task_target_dead: int(|a| a.task_target_dead),
            task_size_target_pos: dbl(|a| a.task_size_target_pos),
            task_size_target_neg: dbl(|a| a.task_size_target_neg),
            ..Aggregate::default()
        }
    }
}

/// The base data (`m_base_*`, `m_neut_min`/`m_neut_max`, `m_target`, the
/// instruction set size), fixed once `ProcessInitialize` has run.
#[derive(Debug, Clone, Default)]
struct Base {
    genome: Genome,
    /// `m_base_genome_size`
    size: i32,
    fitness: f64,
    merit: f64,
    gestation: f64,
    /// `m_base_tasks`: the base organism's `GetLastTaskCount()`.
    tasks: Vec<i32>,
    /// `m_target`
    target: i32,
    neut_min: f64,
    neut_max: f64,
    /// `m_inst_set.GetSize()`
    inst_size: i32,
}

/// A test CPU and its `cCPUTestInfo`, as each job creates them, lent the
/// world of the job's context.
struct Probe<'p, 'w> {
    w: &'p mut TestWorld<'w>,
    cpu: TestCpu,
    info: CpuTestInfo,
}

impl<'p, 'w> Probe<'p, 'w> {
    /// `m_world->GetHardwareManager().CreateTestCPU(ctx)` and a default
    /// `cCPUTestInfo`.
    fn new(w: &'p mut TestWorld<'w>) -> Result<Self> {
        let cpu = hw(TestCpu::new(w))?;
        Ok(Probe {
            w,
            cpu,
            info: CpuTestInfo::default(),
        })
    }

    /// `testcpu->TestGenome(ctx, test_info, genome)`, then
    /// `test_info.GetColonyFitness()`.
    fn test(&mut self, genome: &Genome) -> Result<f64> {
        hw(self.cpu.test_genome(self.w, &mut self.info, genome))?;
        Ok(self.info.colony_fitness())
    }
}

/// The bookkeeping shared by `ProcessOneStepGenome` and
/// `ProcessTwoStepGenome` after the test: the fitness class, the site
/// count at `site`, and the task changes of a living mutant. Returns
/// whether the mutant gained the target task (where the two-step version
/// pushes its pending pair).
fn tally(
    base: &Base,
    info: &CpuTestInfo,
    genome: &Genome,
    data: &mut Step,
    test_fitness: f64,
    site: i32,
) -> Result<bool> {
    data.total_fitness += test_fitness;
    // QUIRK: `total_sqr_fitness += test_fitness * test_fitness;` is one
    // expression, which the reference build contracts into a fused
    // multiply-add.
    data.total_sqr_fitness = test_fitness.mul_add(test_fitness, data.total_sqr_fitness);
    data.total += 1;
    if test_fitness == 0.0 {
        data.dead += 1;
    } else if test_fitness < base.neut_min {
        data.neg += 1;
        data.size_neg += test_fitness;
    } else if test_fitness <= base.neut_max {
        data.neut += 1;
    } else {
        data.pos += 1;
        data.size_pos += test_fitness;
        if test_fitness > data.peak_fitness {
            data.peak_fitness = test_fitness;
            data.peak_genome = genome.clone();
        }
    }

    if test_fitness >= base.neut_min {
        data.site_count[site as usize] += 1;
    }

    let mut target = false;
    if test_fitness != 0.0 {
        // Only count tasks if the organism is alive.
        let org = info.colony_organism().ok_or_else(|| {
            AnalyzeError::fatal(
                "cMutationalNeighborhood: living mutant without a colony organism (C++ NULL dereference)",
            )
        })?;
        let cur_tasks = &org.state.phenotype.last_task_count;
        if cur_tasks.len() < base.tasks.len() {
            return Err(AnalyzeError::fatal(
                "cMutationalNeighborhood: mutant task counts shorter than the base's (C++ reads past the array)",
            ));
        }
        let mut knockout = false;
        let mut anytask = false;
        for (&b, &c) in base.tasks.iter().zip(cur_tasks.iter()) {
            if b != 0 && c == 0 {
                knockout = true;
            } else if b == 0 && c != 0 {
                anytask = true;
            }
        }
        if knockout {
            data.task_knockout += 1;
            data.task_size_knockout += test_fitness;
        }
        if anytask {
            data.task_total += 1;
            data.task_size_total += test_fitness;
        }
        let t = base.target as usize;
        if !base.tasks.is_empty() && base.tasks[t] == 0 && cur_tasks[t] != 0 {
            data.task_target += 1;
            data.task_size_target += test_fitness;
            target = true;
        }
    }
    Ok(target)
}

/// `ProcessOneStepGenome(ctx, testcpu, test_info, mod_genome, odata, cur_site)`.
fn one_step_genome(
    base: &Base,
    p: &mut Probe<'_, '_>,
    mod_genome: &Genome,
    odata: &mut Step,
    cur_site: i32,
) -> Result<f64> {
    let test_fitness = p.test(mod_genome)?;
    tally(base, &p.info, mod_genome, odata, test_fitness, cur_site)?;
    Ok(test_fitness)
}

/// `ProcessTwoStepGenome(ctx, testcpu, test_info, mod_genome, tdata, cur, oth)`.
///
/// QUIRK: every caller passes the mutant it just made as `cur` and its own
/// `cur` (the first mutation) as `oth`, so the site count goes to the
/// second mutation's site.
fn two_step_genome(
    base: &Base,
    p: &mut Probe<'_, '_>,
    mod_genome: &Genome,
    tdata: &mut Step,
    cur: PendFit,
    oth: PendFit,
) -> Result<f64> {
    let test_fitness = p.test(mod_genome)?;
    if tally(base, &p.info, mod_genome, tdata, test_fitness, cur.site)? {
        // Push both instructions as possible first mutations, for post
        // determination of first step fitness effect.
        tdata.pending.push(oth);
        tdata.pending.push(cur);
    }
    Ok(test_fitness)
}

/// `cMutationalNeighborhood`.
#[derive(Debug, Clone)]
pub struct MutationalNeighborhood {
    base: Base,
    initialized: bool,
    cur_site: i32,
    completed: i32,
    /// Whether `ProcessComplete` ran (it releases the write lock the
    /// constructor takes; until then a results reader blocks).
    complete: bool,

    onestep_point: Vec<Step>,
    onestep_insert: Vec<Step>,
    onestep_delete: Vec<Step>,

    twostep_point: Vec<Step>,
    twostep_insert: Vec<Step>,
    twostep_delete: Vec<Step>,

    insert_point: Vec<Step>,
    insert_delete: Vec<Step>,
    delete_point: Vec<Step>,

    fitness_point: FitMatrix,
    fitness_insert: FitMatrix,
    fitness_delete: FitMatrix,

    op: Aggregate,
    oi: Aggregate,
    od: Aggregate,
    ot: Aggregate,

    tp: Aggregate,
    ti: Aggregate,
    td: Aggregate,
    tip: Aggregate,
    tid: Aggregate,
    tdp: Aggregate,
    tt: Aggregate,
}

impl MutationalNeighborhood {
    /// `cMutationalNeighborhood(world, genome, target)`: looks up the
    /// genome's instruction set (`m_inst_set`).
    pub fn new(registry: &InstSetRegistry, genome: &Genome, target: i32) -> Result<Self> {
        let inst_size = inst_set_of(registry, genome)?.size();
        let base = Base {
            genome: genome.clone(),
            size: genome.representation().size(),
            target,
            inst_size,
            ..Base::default()
        };
        Ok(MutationalNeighborhood {
            base,
            initialized: false,
            cur_site: 0,
            completed: 0,
            complete: false,
            onestep_point: Vec::new(),
            onestep_insert: Vec::new(),
            onestep_delete: Vec::new(),
            twostep_point: Vec::new(),
            twostep_insert: Vec::new(),
            twostep_delete: Vec::new(),
            insert_point: Vec::new(),
            insert_delete: Vec::new(),
            delete_point: Vec::new(),
            fitness_point: FitMatrix::default(),
            fitness_insert: FitMatrix::default(),
            fitness_delete: FitMatrix::default(),
            op: Aggregate::default(),
            oi: Aggregate::default(),
            od: Aggregate::default(),
            ot: Aggregate::default(),
            tp: Aggregate::default(),
            ti: Aggregate::default(),
            td: Aggregate::default(),
            tip: Aggregate::default(),
            tid: Aggregate::default(),
            tdp: Aggregate::default(),
            tt: Aggregate::default(),
        })
    }

    /// `m_base_genome_size`: the number of per-site jobs
    /// `ProcessInitialize` adds to the job queue.
    pub fn base_genome_size(&self) -> i32 {
        self.base.size
    }

    /// Whether `ProcessComplete` has run.
    pub fn is_complete(&self) -> bool {
        self.complete
    }

    /// `Process(ctx)`, the analyze job: the first call is
    /// `ProcessInitialize` (after which the caller runs one more job per
    /// site, as `ProcessInitialize` queues them); each later call processes
    /// the next site, and the call that completes the last site runs
    /// `ProcessComplete`.
    pub fn process(&mut self, w: &mut TestWorld<'_>) -> Result<()> {
        if !self.initialized {
            return self.process_initialize(w);
        }
        let mut cur_site = self.cur_site;
        self.cur_site += 1;
        let size = self.base.size;

        if cur_site < size {
            // Create test infrastructure.
            let mut p = Probe::new(w)?;
            let cs = cur_site as usize;
            let base = &self.base;

            // Set up the one-step data.
            self.onestep_point[cs].setup(base, size);
            self.onestep_insert[cs].setup(base, size + 1);
            self.onestep_delete[cs].setup(base, size);

            // Set up the data used in two-step.
            self.twostep_point[cs].setup(base, size);
            self.twostep_insert[cs].setup(base, size + 2);
            self.twostep_delete[cs].setup(base, size);

            self.insert_point[cs].setup(base, size + 1);
            self.insert_delete[cs].setup(base, size + 1);
            self.delete_point[cs].setup(base, size);

            // Do the processing, starting with one-step.
            self.process_one_step_point(&mut p, cur_site)?;
            self.process_one_step_insert(&mut p, cur_site)?;
            self.process_one_step_delete(&mut p, cur_site)?;

            // Process the hanging insertion on the first cycle through (to
            // balance execution time).
            if cur_site == 0 {
                cur_site = size;
                let cs = cur_site as usize;
                let base = &self.base;
                self.onestep_insert[cs].setup(base, size + 1);
                self.twostep_insert[cs].setup(base, size + 2);
                self.insert_point[cs].setup(base, size + 1);
                self.insert_delete[cs].setup(base, size + 1);
                self.process_one_step_insert(&mut p, cur_site)?;
            }
        }

        self.completed += 1;
        if self.completed == size {
            self.process_complete();
        }
        Ok(())
    }

    /// `ProcessInitialize(ctx)`: test the base genome and size the per-site
    /// state. Queuing the per-site jobs is left to the caller.
    fn process_initialize(&mut self, w: &mut TestWorld<'_>) -> Result<()> {
        // Generate base information.
        let mut p = Probe::new(w)?;
        let base_genome = self.base.genome.clone();
        hw(p.cpu.test_genome(p.w, &mut p.info, &base_genome))?;
        let org = p.info.colony_organism().ok_or_else(|| {
            AnalyzeError::fatal(
                "cMutationalNeighborhood::ProcessInitialize: no colony organism (C++ NULL dereference)",
            )
        })?;
        let phenotype = &org.state.phenotype;
        let b = &mut self.base;
        b.fitness = p.info.colony_fitness();
        b.merit = phenotype.merit.get_double();
        b.gestation = f64::from(phenotype.gestation_time);
        b.tasks = phenotype.last_task_count.clone();

        b.neut_min = b.fitness * FITNESS_NEUTRAL_MIN;
        b.neut_max = b.fitness * FITNESS_NEUTRAL_MAX;

        // If invalid target supplied, set to the last task.
        let num_tasks = b.tasks.len() as i32;
        if b.target >= num_tasks || b.target < 0 {
            b.target = num_tasks - 1;
        }

        // The C++ writes `m_fitness_point[site][op]` unchecked.
        if let Some(bad) = (0..b.size)
            .map(|i| b.genome.representation().get(i).op())
            .find(|&op| op < 0 || op >= b.inst_size)
        {
            return Err(AnalyzeError::fatal(format!(
                "cMutationalNeighborhood: instruction {bad} outside the {}-instruction set (C++ writes past its fitness table)",
                b.inst_size
            )));
        }

        // Set up state to begin processing.
        let size = b.size;
        let inst_size = b.inst_size;
        let sites = |n: i32| vec![Step::default(); n.max(0) as usize];
        self.onestep_point = sites(size);
        self.onestep_insert = sites(size + 1);
        self.onestep_delete = sites(size);

        self.twostep_point = sites(size);
        self.twostep_insert = sites(size + 1);
        self.twostep_delete = sites(size);

        self.insert_point = sites(size + 1);
        self.insert_delete = sites(size + 1);
        self.delete_point = sites(size);

        self.fitness_point.resize_clear(size, inst_size);
        self.fitness_insert.resize_clear(size + 1, inst_size);
        self.fitness_delete.resize_clear(size, 1);

        self.cur_site = 0;
        self.completed = 0;
        self.initialized = true;
        Ok(())
    }

    /// `ProcessOneStepPoint(ctx, testcpu, test_info, cur_site)`.
    fn process_one_step_point(&mut self, p: &mut Probe<'_, '_>, cur_site: i32) -> Result<()> {
        let inst_size = self.base.inst_size;
        let mut mod_genome = self.base.genome.clone();

        // Loop through all the lines of genome, testing trying all
        // combinations.
        let cur_inst = mod_genome.representation().get(cur_site).op();

        // Fill in unmutated entry in fitness table with base fitness.
        self.fitness_point
            .set(cur_site, cur_inst, self.base.fitness);

        for inst_num in 0..inst_size {
            if cur_inst == inst_num {
                continue;
            }
            mod_genome
                .representation_mut()
                .set(cur_site, Instruction::new(inst_num));
            let f = one_step_genome(
                &self.base,
                p,
                &mod_genome,
                &mut self.onestep_point[cur_site as usize],
                cur_site,
            )?;
            self.fitness_point.set(cur_site, inst_num, f);

            self.process_two_step_point(p, cur_site, &mut mod_genome)?;
        }
        Ok(())
    }

    /// `ProcessOneStepInsert(ctx, testcpu, test_info, cur_site)`.
    fn process_one_step_insert(&mut self, p: &mut Probe<'_, '_>, cur_site: i32) -> Result<()> {
        let inst_size = self.base.inst_size;
        let mut mod_genome = self.base.genome.clone();
        mod_genome
            .representation_mut()
            .insert(cur_site, Instruction::new(0));

        for inst_num in 0..inst_size {
            mod_genome
                .representation_mut()
                .set(cur_site, Instruction::new(inst_num));
            let f = one_step_genome(
                &self.base,
                p,
                &mod_genome,
                &mut self.onestep_insert[cur_site as usize],
                cur_site,
            )?;
            self.fitness_insert.set(cur_site, inst_num, f);

            self.process_two_step_insert(p, cur_site, &mut mod_genome)?;
            self.process_insert_point_combo(p, cur_site, &mut mod_genome)?;
            self.process_insert_delete_combo(p, cur_site, &mut mod_genome)?;
        }
        Ok(())
    }

    /// `ProcessOneStepDelete(ctx, testcpu, test_info, cur_site)`.
    fn process_one_step_delete(&mut self, p: &mut Probe<'_, '_>, cur_site: i32) -> Result<()> {
        let mut mod_genome = self.base.genome.clone();
        mod_genome.representation_mut().remove(cur_site, 1);

        let f = one_step_genome(
            &self.base,
            p,
            &mod_genome,
            &mut self.onestep_delete[cur_site as usize],
            cur_site,
        )?;
        self.fitness_delete.set(cur_site, 0, f);
        self.process_two_step_delete(p, cur_site, &mut mod_genome)?;
        self.process_delete_point_combo(p, cur_site, &mut mod_genome)
    }

    /// `ProcessTwoStepPoint(ctx, testcpu, test_info, cur_site, mod_genome)`.
    fn process_two_step_point(
        &mut self,
        p: &mut Probe<'_, '_>,
        cur_site: i32,
        mod_genome: &mut Genome,
    ) -> Result<()> {
        let base = &self.base;
        let tdata = &mut self.twostep_point[cur_site as usize];
        let cur = PendFit::new(
            FitTable::Point,
            cur_site,
            mod_genome.representation().get(cur_site).op(),
        );

        // Loop through remaining lines of genome, testing trying all
        // combinations.
        for line_num in cur_site + 1..base.size {
            let cur_inst = mod_genome.representation().get(line_num).op();
            for inst_num in 0..base.inst_size {
                if cur_inst == inst_num {
                    continue;
                }
                mod_genome
                    .representation_mut()
                    .set(line_num, Instruction::new(inst_num));
                let second = PendFit::new(FitTable::Point, line_num, inst_num);
                two_step_genome(base, p, mod_genome, tdata, second, cur)?;
            }
            mod_genome
                .representation_mut()
                .set(line_num, Instruction::new(cur_inst));
        }
        Ok(())
    }

    /// `ProcessTwoStepInsert(ctx, testcpu, test_info, cur_site, mod_genome)`.
    ///
    /// QUIRK: the inner loop sets the instruction at `cur_site` (the first
    /// insertion) instead of the one just inserted at `line_num`, which
    /// stays instruction 0; and it leaves the last instruction of the set
    /// at `cur_site`, so the insert/point and insert/delete combinations
    /// that follow always see that instruction there. The pending second
    /// step still names `m_fitness_insert[line_num - 1][inst_num]`, the
    /// mutant the loop meant to make.
    fn process_two_step_insert(
        &mut self,
        p: &mut Probe<'_, '_>,
        cur_site: i32,
        mod_genome: &mut Genome,
    ) -> Result<()> {
        let base = &self.base;
        let mod_size = mod_genome.representation().size();
        let tdata = &mut self.twostep_insert[cur_site as usize];
        let cur = PendFit::new(
            FitTable::Insert,
            cur_site,
            mod_genome.representation().get(cur_site).op(),
        );

        for line_num in cur_site + 1..=mod_size {
            mod_genome
                .representation_mut()
                .insert(line_num, Instruction::new(0));
            for inst_num in 0..base.inst_size {
                mod_genome
                    .representation_mut()
                    .set(cur_site, Instruction::new(inst_num));
                let second = PendFit::new(FitTable::Insert, line_num - 1, inst_num);
                two_step_genome(base, p, mod_genome, tdata, second, cur)?;
            }
            mod_genome.representation_mut().remove(line_num, 1);
        }
        Ok(())
    }

    /// `ProcessTwoStepDelete(ctx, testcpu, test_info, cur_site, mod_genome)`.
    fn process_two_step_delete(
        &mut self,
        p: &mut Probe<'_, '_>,
        cur_site: i32,
        mod_genome: &mut Genome,
    ) -> Result<()> {
        let base = &self.base;
        let mod_size = mod_genome.representation().size();
        let tdata = &mut self.twostep_delete[cur_site as usize];
        // Delete 'inst' is always 0.
        let cur = PendFit::new(FitTable::Delete, cur_site, 0);

        for line_num in cur_site..mod_size {
            let cur_inst = mod_genome.representation().get(line_num).op();
            mod_genome.representation_mut().remove(line_num, 1);
            let second = PendFit::new(FitTable::Delete, line_num + 1, 0);
            two_step_genome(base, p, mod_genome, tdata, second, cur)?;
            mod_genome
                .representation_mut()
                .insert(line_num, Instruction::new(cur_inst));
        }
        Ok(())
    }

    /// `ProcessInsertPointCombo(ctx, testcpu, test_info, cur_site, mod_genome)`.
    fn process_insert_point_combo(
        &mut self,
        p: &mut Probe<'_, '_>,
        cur_site: i32,
        mod_genome: &mut Genome,
    ) -> Result<()> {
        let base = &self.base;
        let tdata = &mut self.insert_point[cur_site as usize];
        let cur = PendFit::new(
            FitTable::Insert,
            cur_site,
            mod_genome.representation().get(cur_site).op(),
        );

        // Loop through all lines of genome, testing trying all combinations.
        let mut line_num = 0;
        while line_num < mod_genome.representation().size() {
            if line_num == cur_site {
                // Skip the site of the insertion.
                line_num += 1;
                continue;
            }
            // If at or past the insertion site, adjust the pending target
            // site.
            let actual = if line_num < cur_site {
                line_num
            } else {
                line_num - 1
            };
            let cur_inst = mod_genome.representation().get(line_num).op();
            for inst_num in 0..base.inst_size {
                if cur_inst == inst_num {
                    continue;
                }
                mod_genome
                    .representation_mut()
                    .set(line_num, Instruction::new(inst_num));
                let second = PendFit::new(FitTable::Point, actual, inst_num);
                two_step_genome(base, p, mod_genome, tdata, second, cur)?;
            }
            mod_genome
                .representation_mut()
                .set(line_num, Instruction::new(cur_inst));
            line_num += 1;
        }
        Ok(())
    }

    /// `ProcessInsertDeleteCombo(ctx, testcpu, test_info, cur_site, mod_genome)`.
    fn process_insert_delete_combo(
        &mut self,
        p: &mut Probe<'_, '_>,
        cur_site: i32,
        mod_genome: &mut Genome,
    ) -> Result<()> {
        let base = &self.base;
        let tdata = &mut self.insert_delete[cur_site as usize];
        let cur = PendFit::new(
            FitTable::Insert,
            cur_site,
            mod_genome.representation().get(cur_site).op(),
        );

        // Loop through all lines of genome, testing trying all combinations.
        let mut line_num = 0;
        while line_num < mod_genome.representation().size() {
            if line_num == cur_site {
                // Skip the site of the insertion.
                line_num += 1;
                continue;
            }
            // If at or past the insertion site, adjust the pending target
            // site.
            let actual = if line_num < cur_site {
                line_num
            } else {
                line_num - 1
            };
            let cur_inst = mod_genome.representation().get(line_num).op();
            mod_genome.representation_mut().remove(line_num, 1);
            let second = PendFit::new(FitTable::Delete, actual, 0);
            two_step_genome(base, p, mod_genome, tdata, second, cur)?;
            mod_genome
                .representation_mut()
                .insert(line_num, Instruction::new(cur_inst));
            line_num += 1;
        }
        Ok(())
    }

    /// `ProcessDeletePointCombo(ctx, testcpu, test_info, cur_site, mod_genome)`.
    fn process_delete_point_combo(
        &mut self,
        p: &mut Probe<'_, '_>,
        cur_site: i32,
        mod_genome: &mut Genome,
    ) -> Result<()> {
        let base = &self.base;
        let tdata = &mut self.delete_point[cur_site as usize];
        // Delete 'inst' is always 0.
        let cur = PendFit::new(FitTable::Delete, cur_site, 0);

        // Loop through all lines of genome, testing trying all combinations.
        let mut line_num = 0;
        while line_num < mod_genome.representation().size() {
            let cur_inst = mod_genome.representation().get(line_num).op();
            // If at or past the deletion site, adjust the pending target
            // site.
            let actual = if line_num < cur_site {
                line_num
            } else {
                line_num + 1
            };
            for inst_num in 0..base.inst_size {
                if cur_inst == inst_num {
                    continue;
                }
                mod_genome
                    .representation_mut()
                    .set(line_num, Instruction::new(inst_num));
                let second = PendFit::new(FitTable::Point, actual, inst_num);
                two_step_genome(base, p, mod_genome, tdata, second, cur)?;
            }
            mod_genome
                .representation_mut()
                .set(line_num, Instruction::new(cur_inst));
            line_num += 1;
        }
        Ok(())
    }

    /// The fitness an `sPendFit` points at (`sPendFit::GetFitness`).
    fn pending_fitness(&self, pend: PendFit) -> f64 {
        let table = match pend.table {
            FitTable::Point => &self.fitness_point,
            FitTable::Insert => &self.fitness_insert,
            FitTable::Delete => &self.fitness_delete,
        };
        table.get(pend.site, pend.inst)
    }

    /// `AggregateOneStep(steps, osa)`: fold the per-site records in site
    /// order, then the entropy and complexity.
    fn aggregate_one_step(base: &Base, steps: &[Step], osa: &mut Aggregate) {
        for odata in steps {
            osa.total += odata.total;
            osa.total_fitness += odata.total_fitness;
            osa.total_sqr_fitness += odata.total_sqr_fitness;
            osa.pos += odata.pos;
            osa.neg += odata.neg;
            osa.neut += odata.neut;
            osa.dead += odata.dead;
            osa.size_pos += odata.size_pos;
            osa.size_neg += odata.size_neg;

            if odata.peak_fitness > osa.peak_fitness {
                osa.peak_genome = odata.peak_genome.clone();
                osa.peak_fitness = odata.peak_fitness;
            }

            for (j, c) in osa.site_count.iter_mut().enumerate() {
                *c += odata.site_count[j];
            }

            osa.task_target += odata.task_target;
            osa.task_total += odata.task_total;
            osa.task_knockout += odata.task_knockout;

            osa.task_size_target += odata.task_size_target;
            osa.task_size_total += odata.task_size_total;
            osa.task_size_knockout += odata.task_size_knockout;
        }
        Self::entropy(base, osa);
    }

    /// The entropy loop that ends both aggregations: per-site entropy is
    /// the log of the number of legal states for that site, plus one for
    /// the unmutated state, in units of `log(inst set size)`.
    ///
    /// QUIRK: only the first `m_base_genome_size` site counts enter, so the
    /// insertion classes (with one or two more counted sites) ignore their
    /// trailing sites; `log(double(count + 1)) / max_ent` is divided before
    /// it is added, so it is not fused.
    fn entropy(base: &Base, agg: &mut Aggregate) {
        let max_ent = f64::from(base.inst_size).ln();
        for &count in &agg.site_count[..base.size.max(0) as usize] {
            agg.total_entropy += f64::from(count + 1).ln() / max_ent;
        }
        agg.complexity = f64::from(base.size) - agg.total_entropy;
    }

    /// `AggregateTwoStep(steps, tsa)`: as [`Self::aggregate_one_step`], and
    /// classify the one-step fitness of every pending first mutation.
    ///
    /// QUIRK: each site's pending list is a stack, so its fitness values are
    /// summed in the reverse of the order they were pushed.
    fn aggregate_two_step(&self, steps: &mut [Step], tsa: &mut Aggregate) {
        for tdata in steps.iter_mut() {
            tsa.total += tdata.total;
            tsa.total_fitness += tdata.total_fitness;
            tsa.total_sqr_fitness += tdata.total_sqr_fitness;
            tsa.pos += tdata.pos;
            tsa.neg += tdata.neg;
            tsa.neut += tdata.neut;
            tsa.dead += tdata.dead;
            tsa.size_pos += tdata.size_pos;
            tsa.size_neg += tdata.size_neg;

            if tdata.peak_fitness > tsa.peak_fitness {
                tsa.peak_genome = tdata.peak_genome.clone();
                tsa.peak_fitness = tdata.peak_fitness;
            }

            for (j, c) in tsa.site_count.iter_mut().enumerate() {
                *c += tdata.site_count[j];
            }

            tsa.task_target += tdata.task_target;
            tsa.task_total += tdata.task_total;
            tsa.task_knockout += tdata.task_knockout;

            tsa.task_size_target += tdata.task_size_target;
            tsa.task_size_total += tdata.task_size_total;
            tsa.task_size_knockout += tdata.task_size_knockout;

            while let Some(pend) = tdata.pending.pop() {
                let fitness = self.pending_fitness(pend);
                if fitness == 0.0 {
                    tsa.task_target_dead += 1;
                } else if fitness < self.base.neut_min {
                    tsa.task_target_neg += 1;
                    tsa.task_size_target_neg += fitness;
                } else if fitness <= self.base.neut_max {
                    tsa.task_target_neut += 1;
                } else {
                    tsa.task_target_pos += 1;
                    tsa.task_size_target_pos += fitness;
                }
            }
        }
        Self::entropy(&self.base, tsa);
    }

    /// `ProcessComplete(ctx)`: fold every class, then the totals.
    fn process_complete(&mut self) {
        let size = self.base.size;
        let base = &self.base;

        self.op.setup(base, size);
        Self::aggregate_one_step(base, &self.onestep_point, &mut self.op);
        self.oi.setup(base, size + 1);
        Self::aggregate_one_step(base, &self.onestep_insert, &mut self.oi);
        self.od.setup(base, size);
        Self::aggregate_one_step(base, &self.onestep_delete, &mut self.od);

        // Collect totals across all one step mutants.
        let peak = {
            let (op, oi, od) = (&self.op, &self.oi, &self.od);
            if op.peak_fitness >= oi.peak_fitness && op.peak_fitness >= od.peak_fitness {
                op
            } else if oi.peak_fitness >= od.peak_fitness {
                oi
            } else {
                od
            }
        };
        let (peak_fitness, peak_genome) = (peak.peak_fitness, peak.peak_genome.clone());
        let ot = Aggregate::sum_of(&[&self.op, &self.oi, &self.od]);
        self.ot = Aggregate {
            peak_fitness,
            peak_genome,
            ..ot
        };

        let mut twostep = [
            (std::mem::take(&mut self.twostep_point), size),
            (std::mem::take(&mut self.twostep_insert), size + 2),
            (std::mem::take(&mut self.twostep_delete), size),
            (std::mem::take(&mut self.insert_point), size + 1),
            (std::mem::take(&mut self.insert_delete), size + 1),
            (std::mem::take(&mut self.delete_point), size),
        ];
        let mut aggs: [Aggregate; 6] = Default::default();
        for ((steps, sites), agg) in twostep.iter_mut().zip(aggs.iter_mut()) {
            agg.setup(&self.base, *sites);
            self.aggregate_two_step(steps, agg);
        }
        let [tp, ti, td, tip, tid, tdp] = aggs;
        self.tp = tp;
        self.ti = ti;
        self.td = td;
        self.tip = tip;
        self.tid = tid;
        self.tdp = tdp;

        // Collect totals across all two step mutants.
        let pftp = self.tp.peak_fitness;
        let pfti = self.ti.peak_fitness;
        let pftd = self.td.peak_fitness;
        let pftip = self.tip.peak_fitness;
        let pftid = self.tid.peak_fitness;
        let pftdp = self.tdp.peak_fitness;
        let peak =
            if pftp >= pfti && pftp >= pftd && pftp >= pftip && pftp >= pftid && pftp >= pftdp {
                &self.tp
            } else if pfti >= pftd && pfti >= pftip && pfti >= pftid && pfti >= pftdp {
                &self.ti
            } else if pftd >= pftip && pftd >= pftid && pftd >= pftdp {
                &self.td
            } else if pftip >= pftid && pftip >= pftdp {
                &self.tip
            } else if pftid >= pftdp {
                &self.tid
            } else {
                &self.tdp
            };
        let (peak_fitness, peak_genome) = (peak.peak_fitness, peak.peak_genome.clone());
        let tt = Aggregate::sum_of(&[
            &self.tp, &self.ti, &self.td, &self.tip, &self.tid, &self.tdp,
        ]);
        self.tt = Aggregate {
            peak_fitness,
            peak_genome,
            ..tt
        };

        // Unlock data for reading.
        self.complete = true;

        // Clean up state information.
        self.onestep_point = Vec::new();
        self.onestep_insert = Vec::new();
        self.onestep_delete = Vec::new();
        self.fitness_point.resize_clear(0, 0);
        self.fitness_insert.resize_clear(0, 0);
        self.fitness_delete.resize_clear(0, 0);
    }

    // -- cMutationalNeighborhoodResults ---------------------------------------

    /// `GetTargetTask()`
    pub fn target_task(&self) -> i32 {
        self.base.target
    }
    /// `GetBaseGenome()`
    pub fn base_genome(&self) -> &Genome {
        &self.base.genome
    }
    /// `GetBaseFitness()`
    pub fn base_fitness(&self) -> f64 {
        self.base.fitness
    }
    /// `GetBaseMerit()`
    pub fn base_merit(&self) -> f64 {
        self.base.merit
    }
    /// `GetBaseGestation()`
    pub fn base_gestation(&self) -> f64 {
        self.base.gestation
    }
    /// `GetBaseTargetTask()`
    pub fn base_target_task(&self) -> bool {
        if self.base.tasks.is_empty() {
            false
        } else {
            self.base.tasks[self.base.target as usize] != 0
        }
    }
    /// `Get1SAggregatePeakGenome()` (and the other classes' peak genomes,
    /// which the C++ exposes but never prints): one-step total.
    pub fn one_step_peak_genome(&self) -> &Genome {
        &self.ot.peak_genome
    }
    /// `Get2SAggregatePeakGenome()`.
    pub fn two_step_peak_genome(&self) -> &Genome {
        &self.tt.peak_genome
    }

    /// `cMutationalNeighborhoodResults(src)` + `PrintStats(df, update)`.
    /// The C++ reader blocks on the read lock until `ProcessComplete` has
    /// run; that never happens for an empty genome (no site jobs), so that
    /// case fails loudly here.
    pub fn print_stats(&self, df: &mut DataFile, update: i32) -> Result<()> {
        if !self.complete {
            return Err(AnalyzeError::fatal(
                "MutationalNeighborhood of an empty genome: no site job ever runs ProcessComplete, so the C++ blocks forever on the results read lock",
            ));
        }
        df.write(update, "Update/Tree Depth");

        df.write(self.target_task(), "Target Task");

        df.write(self.base_fitness(), "Base Fitness");
        df.write(self.base_merit(), "Base Merit");
        df.write(self.base_gestation(), "Base Gestation");
        df.write(self.base.size, "Base Genome Length");
        // QUIRK: `Output::File` has no `Write(bool)`; the bool is promoted
        // to `Write(int)`, printing 0 or 1.
        df.write(
            i32::from(self.base_target_task()),
            "Base Performs Target Task",
        );

        write_section(df, &self.ot, "1-Step", false, false);
        write_section(df, &self.op, "1-Step Point", true, false);
        write_section(df, &self.oi, "1-Step Insert", true, false);
        write_section(df, &self.od, "1-Step Delete", true, false);

        write_section(df, &self.tt, "2-Step", false, true);
        write_section(df, &self.tp, "2-Step Point", true, true);
        write_section(df, &self.ti, "2-Step Insert", true, true);
        write_section(df, &self.td, "2-Step Delete", true, true);

        write_section(df, &self.tip, "Insert/Point", true, true);
        write_section(df, &self.tid, "Insert/Delete", true, true);
        write_section(df, &self.tdp, "Delete/Point", true, true);

        df.endl();
        Ok(())
    }
}

/// One class's columns of `PrintStats`. The aggregate classes (`1-Step`,
/// `2-Step`) have their entropy and complexity columns commented out in
/// the C++; the two-step classes add the previous-mutation breakdown of
/// target conferral.
fn write_section(df: &mut DataFile, a: &Aggregate, name: &str, entropy: bool, two_step: bool) {
    df.write(a.total, &format!("Total {name} Mutants"));
    df.write(a.prob(a.pos), &format!("{name} Probability Beneficial"));
    df.write(a.prob(a.neg), &format!("{name} Probability Deleterious"));
    df.write(a.prob(a.neut), &format!("{name} Probability Neutral"));
    df.write(a.prob(a.dead), &format!("{name} Probability Lethal"));
    df.write(
        Aggregate::average_size(a.size_pos, a.pos),
        &format!("{name} Average Beneficial Size"),
    );
    df.write(
        Aggregate::average_size(a.size_neg, a.neg),
        &format!("{name} Average Deleterious Size"),
    );
    df.write(a.peak_fitness, &format!("{name} Peak Fitness"));
    df.write(a.average_fitness(), &format!("{name} Average Fitness"));
    df.write(
        a.average_sqr_fitness(),
        &format!("{name} Average Square Fitness"),
    );
    if entropy {
        df.write(a.total_entropy, &format!("{name} Total Entropy"));
        df.write(a.complexity, &format!("{name} Total Complexity"));
    }
    df.write(a.task_target, &format!("{name} Confers Target Task"));
    df.write(
        a.prob(a.task_target),
        &format!("{name} Probability Confers Target Task"),
    );
    df.write(
        Aggregate::average_size(a.task_size_target, a.task_target),
        &format!("{name} Average Size of Target Task Conferral"),
    );
    if two_step {
        df.write(
            a.task_target_pos,
            &format!("{name} Confers Target - Previous Beneficial"),
        );
        df.write(
            a.prob_previous(a.task_target_pos),
            &format!("{name} Prob. Confers Target - Previous Beneficial"),
        );
        df.write(
            a.size_previous(a.task_size_target_pos),
            &format!("{name} Ave. Size of Previous Beneficial in Target Conferral"),
        );
        df.write(
            a.task_target_neg,
            &format!("{name} Confers Target - Previous Deleterious"),
        );
        df.write(
            a.prob_previous(a.task_target_neg),
            &format!("{name} Prob. Confers Target - Previous Deleterious"),
        );
        df.write(
            a.size_previous(a.task_size_target_neg),
            &format!("{name} Ave. Size of Previous Deleterious in Target Conferral"),
        );
        df.write(
            a.task_target_neut,
            &format!("{name} Confers Target - Previous Neutral"),
        );
        df.write(
            a.prob_previous(a.task_target_neut),
            &format!("{name} Prob. Confers Target - Previous Neutral"),
        );
        df.write(
            a.task_target_dead,
            &format!("{name} Confers Target - Previous Lethal"),
        );
        df.write(
            a.prob_previous(a.task_target_dead),
            &format!("{name} Prob. Confers Target - Previous Lethal"),
        );
    }
    df.write(a.task_total, &format!("{name} Confers Any Task"));
    df.write(
        a.prob(a.task_total),
        &format!("{name} Probability Confers Any Task"),
    );
    df.write(
        Aggregate::average_size(a.task_size_total, a.task_total),
        &format!("{name} Average Size of Any Task Conferral"),
    );
    df.write(a.task_knockout, &format!("{name} Knockout Task"));
    df.write(
        a.prob(a.task_knockout),
        &format!("{name} Probability Knockout Task"),
    );
    df.write(
        Aggregate::average_size(a.task_size_knockout, a.task_knockout),
        &format!("{name} Average Size of Task Knockout"),
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use std::sync::{Arc, Mutex};

    /// A `Write` the test reads back.
    #[derive(Clone, Default)]
    struct Sink(Arc<Mutex<Vec<u8>>>);

    impl Write for Sink {
        fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
            self.0.lock().unwrap().extend_from_slice(buf);
            Ok(buf.len())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }

    fn base(size: i32, inst_size: i32, fitness: f64) -> Base {
        Base {
            genome: Genome::default(),
            size,
            fitness,
            merit: 98304.0,
            gestation: 110.0,
            tasks: vec![1, 0, 0],
            target: 2,
            neut_min: fitness * FITNESS_NEUTRAL_MIN,
            neut_max: fitness * FITNESS_NEUTRAL_MAX,
            inst_size,
        }
    }

    fn neighborhood(b: Base) -> MutationalNeighborhood {
        MutationalNeighborhood {
            base: b,
            initialized: true,
            cur_site: 0,
            completed: 0,
            complete: false,
            onestep_point: Vec::new(),
            onestep_insert: Vec::new(),
            onestep_delete: Vec::new(),
            twostep_point: Vec::new(),
            twostep_insert: Vec::new(),
            twostep_delete: Vec::new(),
            insert_point: Vec::new(),
            insert_delete: Vec::new(),
            delete_point: Vec::new(),
            fitness_point: FitMatrix::default(),
            fitness_insert: FitMatrix::default(),
            fitness_delete: FitMatrix::default(),
            op: Aggregate::default(),
            oi: Aggregate::default(),
            od: Aggregate::default(),
            ot: Aggregate::default(),
            tp: Aggregate::default(),
            ti: Aggregate::default(),
            td: Aggregate::default(),
            tip: Aggregate::default(),
            tid: Aggregate::default(),
            tdp: Aggregate::default(),
            tt: Aggregate::default(),
        }
    }

    fn printed(m: &MutationalNeighborhood) -> String {
        let sink = Sink::default();
        {
            let mut df = DataFile::from_writer("mut-neighborhood.dat", Box::new(sink.clone()));
            df.write_comment("IMPORTANT: Mutational Neighborhood is *EXPERIMENTAL*");
            df.write_comment("Output data and format is subject to change in future releases.");
            m.print_stats(&mut df, 0).unwrap();
        }
        let bytes = sink.0.lock().unwrap().clone();
        String::from_utf8(bytes).unwrap()
    }

    /// The header (comments and every column description, in order) is the
    /// one in the `_mutn_rate` test's expected output.
    #[test]
    fn header_matches_expected_output() {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../../../avida-core/tests/_mutn_rate/expected/data/mut-neighborhood.dat");
        let expected = std::fs::read_to_string(path).unwrap();
        let header = |text: &str| -> Vec<String> {
            text.lines()
                .take_while(|l| l.starts_with('#'))
                .map(str::to_string)
                .collect()
        };
        let mut m = neighborhood(base(49, 26, 893.673));
        m.complete = true;
        let ours = printed(&m);
        let want = header(&expected);
        assert_eq!(want.len(), 2 + 304);
        assert_eq!(header(&ours), want);
        // One value per column on the data row.
        let row = ours.lines().last().unwrap();
        assert_eq!(row.split_whitespace().count(), 304);
    }

    #[test]
    fn results_wait_for_completion() {
        let m = neighborhood(base(0, 26, 1.0));
        let sink = Sink::default();
        let mut df = DataFile::from_writer("x", Box::new(sink));
        assert!(matches!(
            m.print_stats(&mut df, 0),
            Err(AnalyzeError::Fatal(_))
        ));
    }

    /// `m_ot` / `m_tt` sums are left to right (`(a + b) + c`).
    #[test]
    fn totals_sum_left_to_right() {
        let mk = |x: f64, n: i32| Aggregate {
            total_fitness: x,
            total: n,
            ..Aggregate::default()
        };
        let (a, b, c) = (mk(1e16, 1), mk(1.0, 2), mk(-1e16, 3));
        let s = Aggregate::sum_of(&[&a, &b, &c]);
        assert_eq!(s.total_fitness, (1e16 + 1.0) - 1e16);
        assert_eq!(s.total, 6);
        let s = Aggregate::sum_of(&[&b, &a, &c]);
        assert_eq!(s.total_fitness, (1.0 + 1e16) - 1e16);
    }

    /// The previous-step breakdown divides by `2 * task_target` and is 0
    /// without target conferrals.
    #[test]
    fn previous_step_ratios() {
        let a = Aggregate {
            task_target: 3,
            task_target_pos: 2,
            task_size_target_pos: 9.0,
            ..Aggregate::default()
        };
        assert_eq!(a.prob_previous(a.task_target_pos), 2.0 / 6.0);
        assert_eq!(a.size_previous(a.task_size_target_pos), 1.5);
        let z = Aggregate::default();
        assert_eq!(z.prob_previous(5), 0.0);
        assert_eq!(z.size_previous(5.0), 0.0);
        assert_eq!(Aggregate::average_size(4.0, 0), 0.0);
    }

    /// Pending first mutations are classified by their one-step fitness,
    /// popped as a stack, and the site counts and entropy fold in site order.
    #[test]
    fn two_step_aggregation() {
        let b = base(2, 4, 10.0);
        let mut m = neighborhood(b);
        m.fitness_point.resize_clear(2, 4);
        m.fitness_delete.resize_clear(2, 1);
        m.fitness_point.set(0, 1, 0.0); // lethal
        m.fitness_point.set(0, 2, 5.0); // deleterious
        m.fitness_point.set(1, 3, 10.0); // neutral
        m.fitness_delete.set(1, 0, 0.25); // deleterious
        m.fitness_point.set(1, 0, 12.5); // beneficial
        let mut s0 = Step::default();
        s0.setup(&m.base, 2);
        s0.total = 3;
        s0.task_target = 2;
        s0.site_count = vec![3, 0];
        s0.peak_fitness = 11.0;
        s0.pending = vec![
            PendFit::new(FitTable::Point, 0, 1),
            PendFit::new(FitTable::Point, 0, 2),
            PendFit::new(FitTable::Point, 1, 3),
            PendFit::new(FitTable::Delete, 1, 0),
        ];
        let mut s1 = Step::default();
        s1.setup(&m.base, 2);
        s1.total = 1;
        s1.task_target = 1;
        s1.site_count = vec![0, 1];
        s1.pending = vec![
            PendFit::new(FitTable::Point, 1, 0),
            PendFit::new(FitTable::Point, 1, 0),
        ];
        let mut steps = vec![s0, s1];
        let mut agg = Aggregate::default();
        agg.setup(&m.base, 2);
        m.aggregate_two_step(&mut steps, &mut agg);
        assert!(steps.iter().all(|s| s.pending.is_empty()));
        assert_eq!(agg.total, 4);
        assert_eq!(agg.task_target, 3);
        assert_eq!(agg.peak_fitness, 11.0);
        assert_eq!(agg.task_target_dead, 1);
        assert_eq!(agg.task_target_neg, 2);
        assert_eq!(agg.task_target_neut, 1);
        assert_eq!(agg.task_target_pos, 2);
        // Popped in reverse: 0.25 then 5.0.
        assert_eq!(agg.task_size_target_neg, 0.25 + 5.0);
        assert_eq!(agg.task_size_target_pos, 25.0);
        assert_eq!(agg.site_count, vec![3, 1]);
        let max_ent = 4f64.ln();
        let ent = 4f64.ln() / max_ent + 2f64.ln() / max_ent;
        assert_eq!(agg.total_entropy, ent);
        assert_eq!(agg.complexity, 2.0 - ent);
    }
}
