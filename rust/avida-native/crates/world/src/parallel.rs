//! `EXECUTION_MODE 1`, the parallel execution mode. Nothing in the C++
//! corresponds to it; it is a deliberate departure from parity, approved by
//! the maintainer, and the exact mode (`EXECUTION_MODE 0`, the default)
//! stays the reference. ARCHITECTURE.md ("Parallel execution mode") states
//! what it guarantees and what it does not.
//!
//! One update ([`ParallelExec::run_update`]):
//!
//! 1. **Budgets.** Every organism the scheduler would run (a non-zero
//!    `AdjustSchedule` priority) gets a share of the update's
//!    `CalculateUpdateSize()` instructions (less the carry-over, step 5):
//!    proportional to that priority, or equal under `SLICING_METHOD 0`.
//!    Under the deterministic slicing methods (0 and 2) the shares are
//!    rounded by largest remainder, each organism carrying its fractional
//!    part to the next update, so the budgets add up to the target exactly
//!    and every organism gets its share in the long run; ties go by the
//!    update's permutation, not by cell number. Under the probabilistic
//!    ones (1 and 5) each budget is a Poisson draw with the share as its
//!    mean (the count a lottery of the update's time slices gives an
//!    organism), the draws then moved up or down to the target one
//!    instruction at a time, by merit. A deterministic budget there would
//!    keep related organisms, whose merits and gestation times are equal,
//!    in step for ever: an organism that cannot reproduce then reaches its
//!    age limit at the same point of its relatives' gestation cycle every
//!    time, just after their births, and its cell stays empty longer than
//!    under the lottery (ARCHITECTURE.md, "Statistical equivalence").
//! 2. **Parallel phase.** Every organism with budget left runs a burst
//!    ([`avida_hardware::parallel::run_burst`]) on the worker pool: the
//!    organism-local instructions up to its budget or up to the first one
//!    that needs the world. Mutations drawn there come from the organism's
//!    own random stream. Nothing else of the world changes; the executed
//!    instructions are counted afterwards in a fixed order.
//! 3. **Sequential phase.** The organisms that stopped before an
//!    instruction that needs the world (and those that cannot run bursts at
//!    all) run that one instruction through the exact code path
//!    (`World::process_step`, non-speculative), in the order of a random
//!    permutation of the cells drawn for the update from a dedicated
//!    stream.
//! 4. **Changes of rate.** In the exact mode the scheduler sees a new
//!    priority at once. Here each organism keeps its own update clock
//!    (0 to 1: how far through the update its spending has taken it), an
//!    organism whose priority changed during its step (a divide that
//!    changed its merit) has the rest of its budget scaled to its new
//!    share, and an organism born during the update gets the share of the
//!    update the exact scheduler would still give it: its full share times
//!    one minus its parent's clock at the birth. A newborn's clock starts
//!    there, so a chain of births within one update gets ever smaller
//!    grants and ends, as it does in the exact mode.
//! 5. Phases 2 to 4 repeat until no budget is left. Instructions lost
//!    with organisms that died, and those granted by step 4, make the
//!    update run fewer or more than its size; the difference is carried
//!    over (up to one update's size) and taken from (or added to) the next
//!    update's target, so the run as a whole executes the instructions the
//!    exact mode does for the same sequence of update sizes.
//!
//! The result depends on the seed only: bursts touch nothing but their own
//! organism, so the thread count and the machine load cannot change it.

use std::sync::Arc;
use std::time::{Duration, Instant};

use avida_apto::scheduler::slicing_method;
use avida_apto::AvidaRng;
use avida_config::config::Config;
use avida_config::port_settings::{EXECUTION_MODE_EXACT, EXECUTION_MODE_PARALLEL};
use avida_hardware::local_output::{output_is_local, CellResources, OutEvent};
use avida_hardware::nhardware::VERBOSE_DETAILS;
use avida_hardware::organism::Organism;
use avida_hardware::parallel::{
    divide_is_local, run_burst, stream, BurstOutcome, CellInputs, LocalWorld, ParallelOrgState,
};
use avida_phenotype::{PhenotypeDeme, PhenotypeStats};
use rayon::prelude::*;

use crate::population_cell::PopulationCell;
use crate::world::World;

/// Rounds with fewer organisms than this run their bursts on the calling
/// thread: waking the pool costs more than it saves, and much more when
/// other processes compete for the cores. The result does not depend on
/// it.
const POOL_MIN_ORGANISMS: usize = 256;

/// Bursts a worker takes at a time.
const BURST_CHUNK: usize = 4;

/// The key of the stream the per-update permutations come from (organism
/// streams use the organism uids, which start at 1).
const ORDER_STREAM_KEY: u64 = u64::MAX;

/// The key of the stream the random budgets come from.
const BUDGET_STREAM_KEY: u64 = u64::MAX - 1;

/// Whether `SLICING_METHOD` hands out time slices by lottery
/// (`SLICE_PROB_MERIT`, `SLICE_PROB_INTEGRATED_MERIT`): the budgets are
/// then random too.
fn random_slicing(method: i32) -> bool {
    method == slicing_method::SLICE_PROB_MERIT
        || method == slicing_method::SLICE_PROB_INTEGRATED_MERIT
}

/// A Poisson draw with mean `mean` from `rng`: the product-of-uniforms
/// method below a mean of 10, Hörmann's transformed rejection (PTRS,
/// "The transformed rejection method for generating Poisson random
/// variables", 1993) above, which takes two or three numbers per draw.
fn poisson(rng: &mut AvidaRng, mean: f64) -> u32 {
    if mean.is_nan() || mean <= 0.0 {
        return 0;
    }
    if mean < 10.0 {
        let limit = (-mean).exp();
        let mut k = 0;
        let mut prod = rng.get_double();
        while prod > limit {
            k += 1;
            prod *= rng.get_double();
        }
        return k;
    }
    let slam = mean.sqrt();
    let loglam = mean.ln();
    let b = 0.931 + 2.53 * slam;
    let a = -0.059 + 0.02483 * b;
    let invalpha = 1.1239 + 1.1328 / (b - 3.4);
    let vr = 0.9277 - 3.6224 / (b - 2.0);
    loop {
        let u = rng.get_double() - 0.5;
        let v = rng.get_double();
        let us = 0.5 - u.abs();
        let k = ((2.0 * a / us + b) * u + mean + 0.43).floor();
        if us >= 0.07 && v <= vr {
            return k as u32;
        }
        if k < 0.0 || (us < 0.013 && v > us) {
            continue;
        }
        if v.ln() + invalpha.ln() - (a / (us * us) + b).ln() <= -mean + k * loglam - ln_factorial(k)
        {
            return k as u32;
        }
    }
}

/// `ln(k!)` for a whole number `k >= 0`: a table up to 9, Stirling's series
/// above (an error below 1e-10 there).
fn ln_factorial(k: f64) -> f64 {
    const TABLE: [f64; 10] = [
        0.0,
        0.0,
        std::f64::consts::LN_2,
        1.791_759_469_228_055,
        3.178_053_830_347_945_6,
        4.787_491_742_782_046,
        6.579_251_212_010_101,
        8.525_161_361_065_415,
        10.604_602_902_745_25,
        12.801_827_480_081_469,
    ];
    if k < 10.0 {
        return TABLE[k as usize];
    }
    let x = k + 1.0;
    let x2 = x * x;
    (x - 0.5) * x.ln() - x + 0.5 * (2.0 * std::f64::consts::PI).ln() + 1.0 / (12.0 * x)
        - 1.0 / (360.0 * x * x2)
        + 1.0 / (1260.0 * x * x2 * x2)
}

/// `quota` rounded up with probability equal to its fractional part.
fn round_randomly(rng: &mut AvidaRng, quota: f64) -> u32 {
    let whole = quota.floor();
    let up = rng.get_double() < quota - whole;
    whole as u32 + u32::from(up)
}

/// The start-up check of `EXECUTION_MODE` and of the settings the parallel
/// mode rejects. `Err` carries the message the run stops with.
pub fn check_parallel_supported(cfg: &Config) -> Result<(), String> {
    match cfg.port.execution_mode {
        EXECUTION_MODE_EXACT => return Ok(()),
        EXECUTION_MODE_PARALLEL => {}
        m => {
            return Err(format!(
                "unsupported: EXECUTION_MODE {m} (0 = exact, 1 = parallel)"
            ))
        }
    }
    Ok(())
}

/// Counters of the parallel mode, for performance reports
/// (`AVIDA_EXECUTION_STATS`).
#[derive(Debug, Clone, Copy, Default)]
pub struct ParallelStats {
    /// Updates run.
    pub updates: u64,
    /// Rounds (a parallel phase and the sequential phase after it).
    pub rounds: u64,
    /// The largest number of rounds of one update.
    pub max_rounds: u64,
    /// Instructions executed in parallel phases.
    pub parallel_insts: u64,
    /// Instructions executed in sequential phases.
    pub sequential_insts: u64,
    /// Divides whose organism-local half ran in a burst and whose world
    /// half ran in a sequential phase.
    pub commits: u64,
    /// Organism bursts run (the work items of the parallel phases).
    pub bursts: u64,
    /// Bursts run on the calling thread (small rounds).
    pub inline_bursts: u64,
    /// Instructions granted to organisms born during their update.
    pub newborn_grants: u64,
    /// Organisms whose remaining budget was scaled for a changed priority.
    pub rescales: u64,
    /// Instructions executed so far minus the sizes of the updates run so
    /// far: what the next update's target is reduced by.
    pub carry_over: i64,
    /// Wall time in budget allocation.
    pub alloc_time: Duration,
    /// Wall time in parallel phases (taking the organisms out and putting
    /// them back included).
    pub parallel_time: Duration,
    /// Wall time in sequential phases.
    pub sequential_time: Duration,
    /// Wall time in the end-of-update statistics (`process_post_update`).
    pub post_time: Duration,
    /// Wall time of the bursts run on the worker pool, and of those run on
    /// the calling thread (both part of the parallel phases).
    pub pool_time: Duration,
    pub grant_time: Duration,
    pub inline_time: Duration,
    /// Wall time in the rest of the end of an update (output, systematics).
    pub after_time: Duration,
}

impl ParallelStats {
    /// A one-paragraph summary.
    pub fn summary(&self) -> String {
        let insts = self.parallel_insts + self.sequential_insts;
        let per_update = |x: u64| x as f64 / self.updates.max(1) as f64;
        let total = self.alloc_time + self.parallel_time + self.sequential_time;
        let frac = |d: Duration| {
            if total.is_zero() {
                0.0
            } else {
                d.as_secs_f64() / total.as_secs_f64()
            }
        };
        format!(
            "parallel mode: {} updates, {:.1} rounds/update (max {}), {:.0} instructions/update \
             ({:.2}% sequential, {} commits/update, {:.2}% newborn grants, {:.0} rescales/update, carry-over {}), \
             {:.0} bursts/update ({:.1}% inline); time in updates {:.3} s: \
             allocation {:.1}%, parallel phases {:.1}%, sequential phases {:.1}%; \
             end of update: statistics {:.3} s, rest {:.3} s; bursts: pool {:.3} s, inline {:.3} s; grants {:.3} s",
            self.updates,
            per_update(self.rounds),
            self.max_rounds,
            per_update(insts),
            100.0 * self.sequential_insts as f64 / insts.max(1) as f64,
            per_update(self.commits).round(),
            100.0 * self.newborn_grants as f64 / insts.max(1) as f64,
            per_update(self.rescales),
            self.carry_over,
            per_update(self.bursts),
            100.0 * self.inline_bursts as f64 / self.bursts.max(1) as f64,
            total.as_secs_f64(),
            100.0 * frac(self.alloc_time),
            100.0 * frac(self.parallel_time),
            100.0 * frac(self.sequential_time),
            self.post_time.as_secs_f64(),
            self.after_time.as_secs_f64(),
            self.pool_time.as_secs_f64(),
            self.inline_time.as_secs_f64(),
            self.grant_time.as_secs_f64(),
        )
    }
}

/// One burst of a parallel phase in a world with demes: what the deme
/// bookkeeping of its time slices needs.
#[derive(Clone, Copy, Debug)]
struct DemeBurst {
    cell: i32,
    /// Instructions the burst executed.
    executed: u32,
    /// The organism's merit after the burst.
    merit: f64,
    /// The organism's update clock before and after the burst.
    start: f64,
    clock: f64,
}

/// The cells' input values, as the bursts' input instructions read them.
struct CellInputView<'a>(&'a [PopulationCell]);

impl CellInputs for CellInputView<'_> {
    fn inputs(&self, cell: i32) -> &[i32] {
        self.0
            .get(cell as usize)
            .map_or(&[][..], |c| c.inputs.as_slice())
    }
}

/// The population's resources as the bursts read them: the amounts in a
/// cell, which nothing changes during the parallel phase.
struct CellResourceView<'a>(&'a avida_resources::ResourceCount);

impl CellResources for CellResourceView<'_> {
    fn cell_levels(&self, cell: i32, out: &mut Vec<f64>) {
        self.0.frozen_cell_amounts(cell, out);
    }
}

/// What the output tests of a burst did to the statistics and to the
/// deme of the organism's cell, applied now in the order they were made
/// (`events` is emptied).
fn replay_out_events(w: &mut World, cell: i32, events: &mut Vec<OutEvent>) {
    let deme_id = w.pop.cells[cell as usize].deme_id as usize;
    for e in events.drain(..) {
        match e {
            OutEvent::AgeTask(org_id, task, age) => w.stats.age_task_event(org_id, task, age),
            OutEvent::NewTask(t) => w.stats.add_new_task_count(t),
            OutEvent::OtherTasks(t, prev, cur) => w.stats.add_other_task_counts(t, prev, cur),
            OutEvent::NewReaction(r) => w.stats.add_new_reaction_count(r),
            OutEvent::SwitchTime(a, b, t) => w.stats.add_task_switch_time(a, b, t),
            OutEvent::DemeMeritUnchanged => {
                if let Some(d) = w.pop.demes.get_mut(deme_id) {
                    let merit = d.heritable_deme_merit();
                    d.update_heritable_deme_merit(merit * 1.0 + 0.0);
                }
            }
            OutEvent::DemeTask(t) => {
                if let Some(d) = w.pop.demes.get_mut(deme_id) {
                    d.add_cur_task(t);
                }
            }
            OutEvent::DemeReaction(r) => {
                if let Some(d) = w.pop.demes.get_mut(deme_id) {
                    d.add_cur_reaction(r);
                }
            }
            OutEvent::ResourceChange(change) => {
                w.pop.resource_count.modify_cell_updated(&change, cell);
            }
        }
    }
}

/// The state of the parallel mode a [`crate::run::Stepper`] keeps.
#[derive(Clone)]
pub struct ParallelExec {
    /// `EXECUTION_THREADS` resolved (at least 1).
    threads: usize,
    /// The worker pool; `None` with one thread (and until the first
    /// update).
    pool: Option<Arc<rayon::ThreadPool>>,
    /// The seed every stream of the mode derives from (the world
    /// generator's seed when the run started).
    run_seed: i32,
    /// The stream of the per-update permutations.
    order_rng: AvidaRng,
    /// The stream of the random budgets (probabilistic slicing methods):
    /// drawn in cell order at the allocation and in the sequential phase's
    /// order afterwards, so the thread count cannot change it.
    budget_rng: AvidaRng,
    /// The position of each cell in the update's permutation.
    rank: Vec<u32>,
    /// Every organism runs every instruction through the exact code path
    /// (settings under which bursts would reach the world or print).
    force_world: bool,
    /// The number of the current sequential phase.
    stamp: u64,
    /// The update's target (its size less the carry-over) and the sum of
    /// the weights it was shared out by: an organism of weight `w` born
    /// during the update has a full share of `target * w / weight_sum`.
    target: f64,
    weight_sum: f64,
    /// Organisms created by the current sequential phase: the uids
    /// `first..=last`, and the update clock of the organism whose step
    /// created them.
    births: Vec<(u64, u64, f64)>,
    /// The bursts of the last parallel phase, kept (with demes only) for
    /// the deme bookkeeping at the start of the sequential phase.
    deme_bursts: Vec<DemeBurst>,
    /// Whether the pool has been lent to the population's resources.
    executor_set: bool,
    /// The output tests of this update run in the bursts
    /// ([`output_is_local`]); decided at the start of each update.
    io_local: bool,
    /// The environment has resources and the output tests of the bursts read
    /// and change them in the organisms' cells (`io_local` with
    /// [`avida_resources::ResourceCount::are_plain_cell_grids`]).
    cell_res_local: bool,
    /// The divides of this update run their organism-local half in the
    /// bursts ([`divide_is_local`]).
    divide_local: bool,
    /// Performance counters.
    pub stats: ParallelStats,
}

/// The worker pool as the executor of the spatial resource updates.
struct PoolExecutor(Arc<rayon::ThreadPool>);

impl avida_resources::SpatialExecutor for PoolExecutor {
    fn run<'a>(&self, jobs: Vec<Box<dyn FnOnce() + Send + 'a>>) {
        self.0.scope(|s| {
            for job in jobs {
                s.spawn(move |_| job());
            }
        });
    }
}

impl std::fmt::Debug for ParallelExec {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ParallelExec")
            .field("threads", &self.threads)
            .field("run_seed", &self.run_seed)
            .field("force_world", &self.force_world)
            .finish()
    }
}

/// `EXECUTION_THREADS` as a thread count: 0 or less means every core.
pub fn resolve_threads(setting: i32) -> usize {
    if setting > 0 {
        setting as usize
    } else {
        std::thread::available_parallelism().map_or(1, |n| n.get())
    }
}

impl ParallelExec {
    /// The parallel mode of `w`, when `EXECUTION_MODE` asks for it.
    pub fn for_world(w: &World) -> Option<Self> {
        if w.cfg.port.execution_mode != EXECUTION_MODE_PARALLEL {
            return None;
        }
        let run_seed = w.rng.seed();
        // Slip mutations print at VERBOSITY 3 and above, which from worker
        // threads would interleave in a different order every run; the
        // task-switching penalty reaches the deme from `pay_pre_costs`.
        let force_world = w.cfg.verbosity >= VERBOSE_DETAILS || w.cfg.task_switch_penalty != 0;
        Some(ParallelExec {
            threads: resolve_threads(w.cfg.port.execution_threads),
            pool: None,
            run_seed,
            order_rng: stream(run_seed, ORDER_STREAM_KEY),
            budget_rng: stream(run_seed, BUDGET_STREAM_KEY),
            rank: Vec::new(),
            force_world,
            stamp: 0,
            target: 0.0,
            weight_sum: 0.0,
            births: Vec::new(),
            deme_bursts: Vec::new(),
            executor_set: false,
            io_local: false,
            cell_res_local: false,
            divide_local: false,
            stats: ParallelStats::default(),
        })
    }

    /// The worker threads the mode runs with.
    pub fn threads(&self) -> usize {
        self.threads
    }

    /// Builds the worker pool (once).
    pub fn ensure_pool(&mut self) -> Result<(), String> {
        if self.threads > 1 && self.pool.is_none() {
            let pool = rayon::ThreadPoolBuilder::new()
                .num_threads(self.threads)
                .thread_name(|i| format!("avida-exec-{i}"))
                .build()
                .map_err(|e| format!("EXECUTION_MODE 1: cannot start the worker threads: {e}"))?;
            self.pool = Some(Arc::new(pool));
        }
        Ok(())
    }

    /// The worker pool, once built (with more than one thread).
    pub fn pool(&self) -> Option<&rayon::ThreadPool> {
        self.pool.as_deref()
    }

    /// Draws the update's permutation of the cells.
    fn shuffle(&mut self, num_cells: usize) {
        let mut perm: Vec<u32> = (0..num_cells as u32).collect();
        for i in (1..num_cells).rev() {
            let j = self.order_rng.get_uint(i as u32 + 1) as usize;
            perm.swap(i, j);
        }
        self.rank.resize(num_cells, 0);
        for (pos, &cell) in perm.iter().enumerate() {
            self.rank[cell as usize] = pos as u32;
        }
    }

    /// Runs one update: `ud_size` instructions (`CalculateUpdateSize()`,
    /// less the carry-over) shared out as budgets, `step_size` the clock
    /// advance per executed instruction (`1 / UD_size`, as each exact time
    /// slice advances it).
    pub fn run_update(
        &mut self,
        w: &mut World,
        ud_size: i32,
        step_size: f64,
    ) -> Result<(), String> {
        self.ensure_pool()?;
        w.pop.defer_drops = self.pool.is_some();
        if !self.executor_set {
            self.executor_set = true;
            if let Some(pool) = &self.pool {
                w.pop
                    .resource_count
                    .set_spatial_executor(Some(Arc::new(PoolExecutor(Arc::clone(pool)))));
            }
        }
        self.stats.updates += 1;
        let cell_grids = w.env.resource_lib().size().max(0) as usize == w.pop.resource_count.size()
            && w.pop.resource_count.are_plain_cell_grids(w.pop.cells.len());
        self.io_local = !self.force_world && output_is_local(&w.cfg, &w.env, cell_grids);
        self.cell_res_local = self.io_local && cell_grids;
        self.divide_local = divide_is_local(&w.cfg, self.io_local, w.test_on_div, w.test_sterilize);
        let t0 = Instant::now();
        self.shuffle(w.pop.cells.len());
        let target = (i64::from(ud_size) - self.stats.carry_over).max(0);
        let mut active = self.allocate(w, target)?;
        self.stats.alloc_time += t0.elapsed();

        let mut rounds = 0;
        let mut executed: u64 = 0;
        let mut stopped = Vec::new();
        while !active.is_empty() {
            rounds += 1;
            let t = Instant::now();
            stopped.clear();
            let ran = self.parallel_phase(w, &active, step_size, &mut stopped);
            self.stats.parallel_time += t.elapsed();
            let ran = ran?;
            self.stats.parallel_insts += ran;
            executed += ran;
            w.check_fatal()?;

            let t = Instant::now();
            active.clear();
            let ran = self.sequential_phase(w, &mut stopped, step_size, &mut active);
            self.stats.sequential_time += t.elapsed();
            let ran = ran?;
            self.stats.sequential_insts += ran;
            executed += ran;
        }
        // Bounded by one update's size: a population of fast replicators
        // whose newborn grants overrun an update repays it with the next,
        // never with many.
        let ud = i64::from(ud_size);
        self.stats.carry_over = (self.stats.carry_over + executed as i64 - ud).clamp(-ud, ud);
        self.stats.rounds += rounds;
        self.stats.max_rounds = self.stats.max_rounds.max(rounds);
        Ok(())
    }

    /// The weight priority `p` gives an organism (0: not scheduled).
    fn weight(constant: bool, p: f64) -> f64 {
        if p == 0.0 {
            0.0
        } else if constant {
            1.0
        } else {
            p
        }
    }

    /// Step 1: the budgets, `target` instructions in all. Returns the cells
    /// whose organisms got any, in cell order.
    fn allocate(&mut self, w: &mut World, target: i64) -> Result<Vec<i32>, String> {
        let constant = w.cfg.slicing_method == slicing_method::SLICE_CONSTANT;
        let mut cand: Vec<(usize, f64)> = Vec::new();
        let mut sum = 0.0;
        for (c, &p) in w.pop.sched_priority.iter().enumerate() {
            if p == 0.0 {
                continue;
            }
            if w.pop.orgs[c].is_none() {
                return Err("Unoccupied cell getting processor time!".to_string());
            }
            if !p.is_finite() {
                return Err(format!(
                    "unsupported in EXECUTION_MODE 1: a merit of {p} (C++: the scheduler's \
                     weights are no longer numbers)"
                ));
            }
            let weight = Self::weight(constant, p);
            sum += weight;
            cand.push((c, weight));
        }
        self.target = 0.0;
        self.weight_sum = 0.0;
        if cand.is_empty() || target <= 0 || !(sum > 0.0 && sum.is_finite()) {
            return Ok(Vec::new());
        }
        let total = target as f64;
        self.target = total;
        self.weight_sum = sum;
        if random_slicing(w.cfg.slicing_method) {
            return Ok(self.allocate_random(w, &cand, target));
        }
        // (cell, quota, budget)
        let mut alloc: Vec<(usize, f64, i64)> = Vec::with_capacity(cand.len());
        let mut assigned: i64 = 0;
        for &(c, weight) in &cand {
            let carry = w.pop.orgs[c]
                .as_ref()
                .map_or(0.0, |o| o.state.parallel.carry);
            let quota = total * (weight / sum) + carry;
            let budget = quota.floor().max(0.0) as i64;
            assigned += budget;
            alloc.push((c, quota, budget));
        }
        let mut rest = target - assigned;
        if rest != 0 {
            // Largest remainder first (or, taking back, smallest first);
            // ties in the update's random order.
            let rank = &self.rank;
            let mut order: Vec<usize> = (0..alloc.len()).collect();
            let frac = |i: usize| alloc[i].1 - alloc[i].2 as f64;
            order.sort_by(|&a, &b| {
                frac(b)
                    .total_cmp(&frac(a))
                    .then(rank[alloc[a].0].cmp(&rank[alloc[b].0]))
            });
            if rest > 0 {
                while rest > 0 {
                    for &i in &order {
                        if rest == 0 {
                            break;
                        }
                        alloc[i].2 += 1;
                        rest -= 1;
                    }
                }
            } else {
                while rest < 0 {
                    let before = rest;
                    for &i in order.iter().rev() {
                        if rest == 0 {
                            break;
                        }
                        if alloc[i].2 > 0 {
                            alloc[i].2 -= 1;
                            rest += 1;
                        }
                    }
                    if rest == before {
                        break;
                    }
                }
            }
        }
        let mut active = Vec::with_capacity(alloc.len());
        for (i, (c, quota, budget)) in alloc.into_iter().enumerate() {
            if let Some(org) = w.pop.orgs[c].as_mut() {
                let p = &mut org.state.parallel;
                p.budget = budget as u32;
                p.granted = budget as u32;
                p.t0 = 0.0;
                p.weight = cand[i].1;
                p.carry = quota - budget as f64;
                if budget > 0 {
                    active.push(c as i32);
                }
            }
        }
        Ok(active)
    }

    /// Step 1 under a probabilistic slicing method: for each candidate
    /// `(cell, weight)`, in cell order, a Poisson budget with mean
    /// `target * weight / weight_sum`; then single instructions added to,
    /// or taken from, organisms drawn by weight until the budgets add up to
    /// `target`. Returns the cells whose organisms got any, in cell order.
    fn allocate_random(&mut self, w: &mut World, cand: &[(usize, f64)], target: i64) -> Vec<i32> {
        let (total, sum) = (self.target, self.weight_sum);
        let rng = &mut self.budget_rng;
        let mut budgets: Vec<u32> = cand
            .iter()
            .map(|&(_, weight)| poisson(rng, total * (weight / sum)))
            .collect();
        let mut assigned: i64 = budgets.iter().map(|&b| i64::from(b)).sum();
        if assigned != target {
            let mut cum = Vec::with_capacity(cand.len());
            let mut acc = 0.0;
            for &(_, weight) in cand {
                acc += weight;
                cum.push(acc);
            }
            let last = cum.len() - 1;
            let pick = |rng: &mut AvidaRng| {
                let r = rng.get_double() * acc;
                cum.partition_point(|&x| x <= r).min(last)
            };
            while assigned < target {
                budgets[pick(rng)] += 1;
                assigned += 1;
            }
            let mut misses = 0;
            while assigned > target {
                let mut i = pick(rng);
                if budgets[i] == 0 {
                    // Weight without budget (rare): after a run of misses,
                    // take from the largest budget instead.
                    misses += 1;
                    if misses < 64 {
                        continue;
                    }
                    i = (0..budgets.len())
                        .max_by_key(|&j| (budgets[j], std::cmp::Reverse(j)))
                        .unwrap_or(0);
                }
                misses = 0;
                budgets[i] -= 1;
                assigned -= 1;
            }
        }
        let mut active = Vec::with_capacity(cand.len());
        for (&(c, weight), &budget) in cand.iter().zip(&budgets) {
            if let Some(org) = w.pop.orgs[c].as_mut() {
                let p = &mut org.state.parallel;
                p.budget = budget;
                p.granted = budget;
                p.t0 = 0.0;
                p.weight = weight;
                p.carry = 0.0;
                if budget > 0 {
                    active.push(c as i32);
                }
            }
        }
        active
    }

    /// Step 2: the bursts of the organisms in `active`. The organisms that
    /// stopped before an instruction that needs the world, with budget
    /// left, go to `stopped`. Returns the instructions executed.
    fn parallel_phase(
        &mut self,
        w: &mut World,
        active: &[i32],
        step_size: f64,
        stopped: &mut Vec<i32>,
    ) -> Result<u64, String> {
        let mut work: Vec<(i32, Box<Organism>, BurstOutcome)> = Vec::with_capacity(active.len());
        for &c in active {
            if let Some(org) = w.pop.orgs[c as usize].take() {
                work.push((c, org, BurstOutcome::default()));
            }
        }
        if self.cell_res_local {
            // The lazy update of the grids, which the first output test of
            // the update would make: after it, nothing in the parallel phase
            // changes a cell's amounts but the output tests of the
            // cell's own organism.
            w.with_pop_resources_for(None, |rc, rng, env| rc.update_resources(rng, env));
        }
        {
            let cell_inputs = CellInputView(&w.pop.cells);
            let res_view = CellResourceView(&w.pop.resource_count);
            let lw = LocalWorld {
                config: &w.cfg,
                environment: &w.env,
                hardware_manager: &w.hw,
                update: w.stats.update(),
                tot_creatures: w.stats.tot_organisms,
                sense_size: w.stats.sense_size.max(0) as usize,
                io_local: self.io_local,
                divide_local: self.divide_local,
                cell_resources: if self.cell_res_local {
                    Some(&res_view)
                } else {
                    None
                },
                cell_inputs: &cell_inputs,
                group_attack_counts: &w.group_attack_counts,
            };
            let seed = self.run_seed;
            let force = self.force_world;
            let run = |item: &mut (i32, Box<Organism>, BurstOutcome)| {
                item.2 = run_burst(&mut item.1, &lw, item.0, seed, force);
            };
            self.stats.bursts += work.len() as u64;
            match &self.pool {
                Some(pool) if work.len() >= POOL_MIN_ORGANISMS => {
                    let t = Instant::now();
                    pool.install(|| work.par_iter_mut().with_min_len(BURST_CHUNK).for_each(run));
                    self.stats.pool_time += t.elapsed();
                }
                _ => {
                    let t = Instant::now();
                    self.stats.inline_bursts += work.len() as u64;
                    work.iter_mut().for_each(run);
                    self.stats.inline_time += t.elapsed();
                }
            }
        }
        let mut executed: u64 = 0;
        let mut error = None;
        let demes = w.pop.demes.len() > 1;
        for (c, org, out) in work {
            if demes && out.executed > 0 {
                // What the deme half of each of these time slices does is
                // done for the whole burst at the start of the sequential
                // phase (`settle_deme_bursts`).
                let p = &org.state.parallel;
                let clock = Self::clock(p);
                let spent = f64::from(out.executed) / f64::from(p.granted.max(1));
                self.deme_bursts.push(DemeBurst {
                    cell: c,
                    executed: out.executed,
                    merit: org.state.phenotype.merit.get_double(),
                    start: (clock - (1.0 - p.t0) * spent).max(p.t0),
                    clock,
                });
            }
            let mut org = org;
            if !org.state.parallel.out_events.is_empty() {
                let mut events = std::mem::take(&mut org.state.parallel.out_events);
                replay_out_events(w, c, &mut events);
                org.state.parallel.out_events = events;
            }
            w.pop.orgs[c as usize] = Some(org);
            executed += u64::from(out.executed);
            if out.needs_world {
                stopped.push(c);
            }
            if error.is_none() {
                error = out.error;
            }
        }
        // What every exact time slice does after its instruction, for the
        // slices the bursts ran (with one deme a speculative slice skips
        // the deme part, so these do too).
        w.stats.num_executed = w.stats.num_executed.wrapping_add(executed as i32);
        w.pop.resource_count.update(step_size * executed as f64);
        // As in the exact mode, also with a single deme.
        for deme in &mut w.pop.demes {
            deme.resource_count.update(step_size * executed as f64);
        }
        match error {
            Some(e) => Err(e),
            None => Ok(executed),
        }
    }

    /// Step 3: one instruction of each organism in `stopped`, through the
    /// exact code path, in the update's permutation order. The cells of
    /// the organisms with budget left go to `next`. Returns the
    /// instructions executed.
    fn sequential_phase(
        &mut self,
        w: &mut World,
        stopped: &mut [i32],
        step_size: f64,
        next: &mut Vec<i32>,
    ) -> Result<u64, String> {
        self.stamp += 1;
        let stamp = self.stamp;
        let constant = w.cfg.slicing_method == slicing_method::SLICE_CONSTANT;
        let random = random_slicing(w.cfg.slicing_method);
        let rank = &self.rank;
        // In the update's permutation order. (Ordering a round by the
        // organisms' clocks instead was measured: further from the exact
        // mode on the average generation and the number of organisms, see
        // ARCHITECTURE.md.)
        stopped.sort_unstable_by_key(|&c| rank[c as usize]);
        self.births.clear();
        self.settle_deme_bursts(w)?;
        let mut ran = 0;
        for &c in stopped.iter() {
            // Earlier steps of this phase may have killed, replaced or
            // moved the organism that stopped here.
            let Some(org) = w.pop.orgs[c as usize].as_mut() else {
                continue;
            };
            let p = &mut org.state.parallel;
            // A divide whose organism-local half ran in the burst: the
            // burst counted the instruction, and the world's half is not a
            // step of its own.
            let commit = p.pending_birth.0.is_some();
            if p.budget == 0 && !commit {
                continue;
            }
            if p.stamp == stamp && !commit {
                // It ran in this phase already, from another cell.
                next.push(c);
                continue;
            }
            if !commit {
                p.budget -= 1;
                p.stamp = stamp;
            }
            let clock = Self::clock(p);
            let uid = org.state.uid;
            let last_uid = w.pop.last_org_uid;
            if commit {
                w.commit_divide(c)?;
                self.stats.commits += 1;
            } else {
                w.process_step(step_size, c, false)?;
                ran += 1;
            }
            w.check_fatal()?;
            if w.pop.last_org_uid > last_uid {
                self.births.push((last_uid + 1, w.pop.last_org_uid, clock));
            }
            let end_cell = w.pop.last_step_cell;
            if let Ok(e) = usize::try_from(end_cell) {
                let priority = w.pop.sched_priority.get(e).copied().unwrap_or(0.0);
                if let Some(org) = w.pop.orgs.get_mut(e).and_then(Option::as_mut) {
                    let p = &mut org.state.parallel;
                    if org.state.uid == uid && p.budget > 0 {
                        self.rescale(p, Self::weight(constant, priority), random);
                        if p.budget > 0 {
                            next.push(end_cell);
                        }
                    }
                }
            }
            // A move swaps the two cells' organisms: the one now in `c`
            // keeps its budget too.
            if end_cell != c {
                if let Some(org) = w.pop.orgs[c as usize].as_ref() {
                    if org.state.parallel.budget > 0 {
                        next.push(c);
                    }
                }
            }
        }
        if !self.births.is_empty() {
            let t = Instant::now();
            self.grant_newborns(w, constant, random, next);
            self.stats.grant_time += t.elapsed();
        }
        next.sort_unstable();
        next.dedup();
        self.free_graveyard(w);
        Ok(ran)
    }

    /// Frees the organisms the phase deleted on a worker thread, while the
    /// main thread goes on.
    fn free_graveyard(&self, w: &mut World) {
        if w.pop.graveyard.is_empty() {
            return;
        }
        let dead = std::mem::take(&mut w.pop.graveyard);
        match &self.pool {
            Some(pool) => pool.spawn(move || drop(dead)),
            None => drop(dead),
        }
    }

    /// The deme half of the time slices the bursts of the last parallel
    /// phase ran (`World::demes_after_step` in the exact mode): each burst
    /// adds its instructions to the time used of its organism's deme, and
    /// then every deme a burst ran in is checked for implicit replication,
    /// once and in deme order. The exact mode checks after every
    /// instruction, so there a deme replicates at the instruction that
    /// crosses its threshold and here at the end of the round in which a
    /// burst crossed it. The organisms a replication creates are granted
    /// their budgets with the newborns of the sequential phase, from the
    /// update clock at which the deme's bursts crossed the threshold
    /// (interpolated between the earliest start and the latest end of
    /// those bursts by the share of their instructions the threshold was
    /// short of), which is the part of the update the exact mode leaves
    /// the founders.
    fn settle_deme_bursts(&mut self, w: &mut World) -> Result<(), String> {
        if self.deme_bursts.is_empty() {
            return Ok(());
        }
        let bursts = std::mem::take(&mut self.deme_bursts);
        // Per deme with bursts: its time used and normalized time used
        // before them, and the clock window of the bursts.
        let mut rounds: Vec<Option<(f64, f64, f64, f64)>> = vec![None; w.pop.demes.len()];
        for b in &bursts {
            let d = w.deme_of(b.cell);
            let deme = &mut w.pop.demes[d];
            let round = rounds[d].get_or_insert((
                f64::from(deme.time_used),
                deme.cur_normalized_time_used,
                b.start,
                b.clock,
            ));
            round.2 = round.2.min(b.start);
            round.3 = round.3.max(b.clock);
            deme.time_used += b.executed as i32;
            deme.cur_normalized_time_used +=
                f64::from(b.executed) / b.merit / f64::from(deme.cur_org_count);
        }
        let cycles = w.cfg.demes_replicate_cpu_cycles;
        let time = w.cfg.demes_replicate_time;
        for (d, round) in rounds.into_iter().enumerate() {
            let Some((used, normalized, start, end)) = round else {
                continue;
            };
            let deme = &w.pop.demes[d];
            // The share of the round's instructions the first threshold
            // crossed was short of (1 when none was crossed).
            let short = |limit: f64, before: f64, after: f64| {
                if limit != 0.0 && after >= limit && after > before {
                    ((limit - before) / (after - before)).clamp(0.0, 1.0)
                } else {
                    1.0
                }
            };
            let share = short(cycles, used, f64::from(deme.time_used)).min(short(
                time,
                normalized,
                deme.cur_normalized_time_used,
            ));
            let clock = start + share * (end - start);
            let last_uid = w.pop.last_org_uid;
            w.check_implicit_deme_repro(d);
            w.check_fatal()?;
            if w.pop.last_org_uid > last_uid {
                self.births.push((last_uid + 1, w.pop.last_org_uid, clock));
            }
        }
        let mut bursts = bursts;
        bursts.clear();
        self.deme_bursts = bursts;
        Ok(())
    }

    /// The update clock of an organism: how far through the update (from
    /// 0 to 1) its spending has taken it, counted from the time `t0` at
    /// which its current grant started.
    fn clock(p: &ParallelOrgState) -> f64 {
        if p.granted == 0 {
            return p.t0;
        }
        let spent = f64::from(p.granted - p.budget) / f64::from(p.granted);
        (p.t0 + (1.0 - p.t0) * spent).clamp(0.0, 1.0)
    }

    /// Step 4 for an organism whose step left it with scheduling weight
    /// `weight`: the rest of its budget scaled by the change of its rate
    /// (its weight over the sum of the weights), the fractional part going
    /// to its carry, or (`random`, a probabilistic slicing method) rounded
    /// up or down at random. Its clock goes on from where it is.
    fn rescale(&mut self, p: &mut ParallelOrgState, weight: f64, random: bool) {
        if weight == p.weight || p.weight <= 0.0 || weight <= 0.0 || self.weight_sum <= 0.0 {
            return;
        }
        let sum = self.weight_sum;
        let factor = (weight / p.weight) * (sum / (sum - p.weight + weight).max(weight));
        let quota = (f64::from(p.budget) * factor).min(self.target);
        p.t0 = Self::clock(p);
        let budget = if random {
            round_randomly(&mut self.budget_rng, quota)
        } else {
            let whole = quota.floor();
            p.carry += quota - whole;
            whole as u32
        };
        p.granted = budget;
        p.budget = budget;
        p.weight = weight;
        self.stats.rescales += 1;
    }

    /// Step 4 for the organisms the sequential phase created: each gets its
    /// full share of the update times the part of the update its parent's
    /// clock had left (a Poisson draw with that mean when `random`), and
    /// its clock starts at its parent's. Their cells go to `next`.
    fn grant_newborns(&mut self, w: &mut World, constant: bool, random: bool, next: &mut Vec<i32>) {
        if self.weight_sum <= 0.0 {
            return;
        }
        let births = std::mem::take(&mut self.births);
        let first = births[0].0;
        let mut granted = 0;
        for (c, slot) in w.pop.orgs.iter_mut().enumerate() {
            let Some(org) = slot.as_mut() else {
                continue;
            };
            let uid = org.state.uid.0;
            if uid < first {
                continue;
            }
            // The ranges are in increasing uid order.
            let i = births.partition_point(|&(_, last, _)| last < uid);
            let Some(&(lo, _, clock)) = births.get(i) else {
                continue;
            };
            let weight = Self::weight(constant, w.pop.sched_priority[c]);
            if uid < lo || weight == 0.0 {
                continue;
            }
            let quota =
                (self.target * weight / self.weight_sum * (1.0 - clock)).clamp(0.0, self.target);
            let (budget, carry) = if random {
                (poisson(&mut self.budget_rng, quota), 0.0)
            } else {
                let whole = quota.floor();
                (whole as u32, quota - whole)
            };
            let p = &mut org.state.parallel;
            p.budget = budget;
            p.granted = budget;
            p.t0 = clock;
            p.weight = weight;
            p.carry = carry;
            granted += u64::from(budget);
            if budget > 0 {
                next.push(c as i32);
            }
        }
        self.births = births;
        self.stats.newborn_grants += granted;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ln_factorial_matches_the_product() {
        let mut exact = 0.0_f64;
        for k in 0..200 {
            if k > 1 {
                exact += f64::from(k).ln();
            }
            let got = ln_factorial(f64::from(k));
            assert!(
                (got - exact).abs() < 1e-9 * exact.max(1.0),
                "{k}: {got} {exact}"
            );
        }
    }

    #[test]
    fn poisson_draws_have_the_right_mean_and_variance() {
        let mut rng = stream(7, 1);
        for mean in [0.3, 4.0, 9.9, 10.0, 30.0, 250.0, 5000.0] {
            let n = 40_000;
            let draws: Vec<f64> = (0..n).map(|_| f64::from(poisson(&mut rng, mean))).collect();
            let m = draws.iter().sum::<f64>() / f64::from(n);
            let v = draws.iter().map(|x| (x - m) * (x - m)).sum::<f64>() / f64::from(n - 1);
            // Five standard errors of each.
            let se_mean = (mean / f64::from(n)).sqrt();
            assert!((m - mean).abs() < 5.0 * se_mean, "mean {mean}: {m}");
            let se_var = (mean * (2.0 * mean + 1.0) / f64::from(n)).sqrt();
            assert!((v - mean).abs() < 5.0 * se_var, "mean {mean}: variance {v}");
        }
        assert_eq!(poisson(&mut rng, 0.0), 0);
        assert_eq!(poisson(&mut rng, f64::NAN), 0);
    }

    #[test]
    fn random_rounding_keeps_the_mean() {
        let mut rng = stream(7, 2);
        let n = 100_000;
        let sum: u64 = (0..n)
            .map(|_| u64::from(round_randomly(&mut rng, 2.3)))
            .sum();
        let m = sum as f64 / f64::from(n);
        assert!((m - 2.3).abs() < 0.01, "{m}");
    }
}
