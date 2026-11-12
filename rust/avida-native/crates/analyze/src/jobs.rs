//! The analyze job queue (`cAnalyzeJobQueue`, `cAnalyzeJobWorker`,
//! `tAnalyzeJob`, `tAnalyzeJobBatch`; analyze/ at 47f13dadb) on worker
//! threads, deterministic by construction.
//!
//! The C++ runs `MAX_CONCURRENCY` workers that pop jobs and draw each job's
//! seed from the queue's RNG as they start it, so with more than one worker
//! the job-to-seed assignment depends on thread timing. Here the seeds are
//! drawn on the calling thread, in submission order, before any job runs;
//! each job runs on a worker with its own world (a private copy of the
//! state tests change, see [`AnalyzeHost::worker_worlds`]); and everything
//! a job produces (its result, and what it prints through
//! `avida_apto::diag`) is applied on the calling thread in submission
//! order. The output is therefore the output of running the jobs one at a
//! time, as the single-threaded queue does, for any number of workers.
//!
//! Work the C++ runs one genotype at a time in the default context (on the
//! world RNG), such as `RECALCULATE`, can only run ahead when it does not
//! draw from that RNG: [`Analyze::run_default_context_jobs`] runs it
//! speculatively from the RNG's current state and keeps a result only when
//! every earlier one left the state unchanged.
//!
//! Differences from the C++ worker threads, both deliberate: a worker's
//! context keeps the analyze-mode flag clear (`cAnalyzeJobWorker::Run`
//! sets it; the single-threaded queue, which this reproduces, does not),
//! and what a job prints to stdout and to stderr is replayed per stream,
//! so a job writing to both loses the interleaving between the two.

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Mutex;

use avida_apto::diag::{self, Captured};
use avida_apto::AvidaRng;
use avida_genome::{InstSetRegistry, Instruction};
use avida_hardware::test_cpu::TestWorld;

use crate::analyze::Analyze;
use crate::error::{AnalyzeError, Result};
use crate::host::{AnalyzeHost, TestContext, WorkerWorld};

/// How much analyze work ran on worker threads ([`Analyze::worker_stats`]).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct WorkerStats {
    /// Units of analyze jobs applied from worker threads.
    pub units: usize,
    /// Speculative rounds of default-context work (`RECALCULATE`).
    pub speculative_rounds: usize,
    /// Default-context jobs applied from speculative rounds.
    pub speculative_jobs: usize,
    /// Times worker threads were not used after all (the host could not
    /// make worker worlds, or a test changed one in a way later tests would
    /// see), so the work ran on the host.
    pub refusals: usize,
}

/// `cAnalyzeJobQueue`'s worker count: `Apto::Platform::AvailableCPUs()`,
/// lowered to `MAX_CONCURRENCY` when that is positive and smaller (so `-1`
/// and `0` mean every CPU). With one worker the C++ runs each job on the
/// calling thread as it is added.
pub fn worker_count(max_concurrency: i32) -> usize {
    let cpus = std::thread::available_parallelism()
        .map(|n| n.get())
        .unwrap_or(1);
    match usize::try_from(max_concurrency) {
        Ok(m) if m > 0 && m < cpus => m,
        _ => cpus,
    }
}

/// Where an analyze job runs its test CPUs: on the host, or in a worker
/// thread's world.
pub(crate) trait JobWorld {
    /// The test world of the job context with RNG `rng` (`job->Run(ctx)`).
    fn with_job_world(
        &mut self,
        rng: &mut AvidaRng,
        f: &mut dyn FnMut(&mut TestWorld<'_>) -> Result<()>,
    ) -> Result<()>;

    /// `m_world->GetHardwareManager()` instruction sets.
    fn inst_sets(&self) -> &InstSetRegistry;

    /// `GetInstSet(name).ActivateNullInst()`. A worker cannot change the
    /// shared instruction sets: there the NULL instruction must already be
    /// active (see [`Analyze::run_job_units`]'s `ready`).
    fn activate_null_inst(&mut self, inst_set: &str) -> Result<Instruction>;
}

/// The host as a [`JobWorld`]: jobs run one at a time in its world.
pub(crate) struct OnHost<'a>(pub &'a mut dyn AnalyzeHost);

impl JobWorld for OnHost<'_> {
    fn with_job_world(
        &mut self,
        rng: &mut AvidaRng,
        f: &mut dyn FnMut(&mut TestWorld<'_>) -> Result<()>,
    ) -> Result<()> {
        self.0.with_test_world(TestContext::Job(rng), f)
    }

    fn inst_sets(&self) -> &InstSetRegistry {
        self.0.inst_sets()
    }

    fn activate_null_inst(&mut self, inst_set: &str) -> Result<Instruction> {
        self.0.activate_null_inst(inst_set)
    }
}

/// A worker thread's world as a [`JobWorld`].
struct OnWorker<'a, 'h>(&'a mut (dyn WorkerWorld + 'h));

impl JobWorld for OnWorker<'_, '_> {
    fn with_job_world(
        &mut self,
        rng: &mut AvidaRng,
        f: &mut dyn FnMut(&mut TestWorld<'_>) -> Result<()>,
    ) -> Result<()> {
        self.0.with_test_world(rng, false, f)
    }

    fn inst_sets(&self) -> &InstSetRegistry {
        self.0.inst_sets()
    }

    fn activate_null_inst(&mut self, inst_set: &str) -> Result<Instruction> {
        let set = self.0.inst_sets().get_inst_set(inst_set).ok_or_else(|| {
            AnalyzeError::fatal(format!(
                "ActivateNullInst: instruction set '{inst_set}' is unknown"
            ))
        })?;
        active_null_inst(set).ok_or_else(|| {
            AnalyzeError::fatal(format!(
                "internal: the NULL instruction of '{inst_set}' was not active before the jobs \
                 were handed to worker threads"
            ))
        })
    }
}

/// The NULL instruction of `set` if `ActivateNullInst` has already added
/// it (what it would return without changing the set).
pub(crate) fn active_null_inst(set: &avida_genome::InstSet) -> Option<Instruction> {
    let null = set.inst_lib().inst_null();
    (0..set.size())
        .map(Instruction::new)
        .find(|&i| set.lib_function_index(i) == null)
}

/// The seeds of one unit of analyze jobs (`GetSeedForJob`): drawn from the
/// queue's RNG as each job starts (one job at a time), or handed out from
/// seeds drawn beforehand (worker threads).
pub(crate) struct JobSeeds<'s> {
    source: SeedSource<'s>,
    used: usize,
}

enum SeedSource<'s> {
    Draw(Option<&'s mut AvidaRng>),
    Drawn(&'s [i32]),
}

/// `m_job_seed_rng->GetInt(m_job_seed_rng->MaxSeed())`.
fn draw_seed(rng: &mut AvidaRng) -> i32 {
    let max = rng.max_seed();
    rng.get_int(max)
}

impl JobSeeds<'_> {
    /// The RNG of the unit's next job.
    pub(crate) fn next_rng(&mut self) -> Result<AvidaRng> {
        let seed = match &mut self.source {
            SeedSource::Draw(Some(rng)) => draw_seed(rng),
            SeedSource::Draw(None) => {
                return Err(AnalyzeError::unsupported(
                    "analyze job",
                    "cAnalyzeJobQueue (the host has no world RNG to seed it)",
                ))
            }
            SeedSource::Drawn(seeds) => *seeds.get(self.used).ok_or_else(|| {
                AnalyzeError::fatal(
                    "internal: an analyze job unit used more seeds than it declared",
                )
            })?,
        };
        self.used += 1;
        Ok(AvidaRng::new(seed))
    }
}

/// One job (or unit of jobs) run on a worker thread.
struct Done<R> {
    result: Result<R>,
    /// Seeds the unit used (counted up to a failure).
    seeds_used: usize,
    /// What it printed through `avida_apto::diag`.
    printed: Captured,
}

/// Print `text` on this thread as the job printed it on its own.
fn replay(text: Captured) {
    if !text.stdout.is_empty() {
        diag::stdout(format_args!("{}", text.stdout));
    }
    if !text.stderr.is_empty() {
        diag::stderr(format_args!("{}", text.stderr));
    }
}

type WorkerFn<'f, J, R> =
    dyn Fn(&mut (dyn WorkerWorld + '_), J, &mut usize) -> Result<R> + Sync + 'f;

/// Run `run(world, item, seeds_used)` for every item, on one thread per
/// world. Items are handed out in order. The run is cut after the first
/// item that fails or whose result `stop` accepts: the outcomes up to and
/// including it are returned in item order, later ones are skipped or
/// dropped. Also returns every world's [`WorkerWorld::state_digest`].
fn run_on_workers<'h, J: Send, R: Send>(
    worlds: Vec<Box<dyn WorkerWorld + 'h>>,
    items: Vec<J>,
    run: &WorkerFn<'_, J, R>,
    stop: &(dyn Fn(&R) -> bool + Sync),
) -> (Vec<Done<R>>, Vec<String>) {
    let n = items.len();
    let queue = Mutex::new(items.into_iter().enumerate());
    let cut = AtomicUsize::new(usize::MAX);
    type Finished<R> = (Vec<(usize, Done<R>)>, String);
    let finished: Vec<Finished<R>> = std::thread::scope(|s| {
        let handles: Vec<_> = worlds
            .into_iter()
            .map(|mut world| {
                let (queue, cut) = (&queue, &cut);
                s.spawn(move || {
                    let was_capturing = diag::is_capturing();
                    diag::start_capture();
                    let mut done = Vec::new();
                    loop {
                        let next = queue.lock().unwrap_or_else(|e| e.into_inner()).next();
                        let Some((i, item)) = next else { break };
                        // Indices only grow, so nothing after a cut is needed.
                        if i > cut.load(Ordering::SeqCst) {
                            break;
                        }
                        let mut seeds_used = 0;
                        let result = run(&mut *world, item, &mut seeds_used);
                        let ends = match &result {
                            Ok(r) => stop(r),
                            Err(_) => true,
                        };
                        if ends {
                            cut.fetch_min(i, Ordering::SeqCst);
                        }
                        let printed = diag::take_captured();
                        done.push((
                            i,
                            Done {
                                result,
                                seeds_used,
                                printed,
                            },
                        ));
                    }
                    if !was_capturing {
                        let _ = diag::stop_capture();
                    }
                    (done, world.state_digest())
                })
            })
            .collect();
        handles
            .into_iter()
            .map(|h| h.join().unwrap_or_else(|p| std::panic::resume_unwind(p)))
            .collect()
    });
    let mut slots: Vec<Option<Done<R>>> = (0..n).map(|_| None).collect();
    let mut digests = Vec::with_capacity(finished.len());
    for (done, digest) in finished {
        for (i, d) in done {
            slots[i] = Some(d);
        }
        digests.push(digest);
    }
    let keep = cut.load(Ordering::SeqCst).saturating_add(1).min(n);
    // Every item up to the cut ran: a worker skips only items after it.
    let outcomes = slots.into_iter().take(keep).map_while(|d| d).collect();
    (outcomes, digests)
}

/// The job function of [`Analyze::run_job_units`].
pub(crate) type JobFn<'f, J> =
    dyn Fn(&mut J, &mut dyn JobWorld, &mut JobSeeds<'_>) -> Result<()> + Sync + 'f;

/// Lends `f` a test world (see [`DefaultJobFn`]).
pub(crate) type Lend<'a> =
    dyn FnMut(&mut dyn FnMut(&mut TestWorld<'_>) -> Result<()>) -> Result<()> + 'a;

/// A job of [`Analyze::run_default_context_jobs`]: job `i`, whose test
/// CPUs run in the worlds the [`Lend`] lends (all in one context).
pub(crate) type DefaultJobFn<'f, R> = dyn Fn(&mut Lend<'_>, usize) -> Result<R> + Sync + 'f;

/// Job `i` of [`Analyze::run_default_context_jobs`] run on the host.
pub(crate) type SerialFn<'f, R> =
    dyn FnMut(&mut Analyze, &mut dyn AnalyzeHost, usize) -> Result<R> + 'f;

/// What job `i` of [`Analyze::run_default_context_jobs`] is applied with.
pub(crate) type ApplyIndexedFn<'f, R> =
    dyn FnMut(&mut Analyze, &mut dyn AnalyzeHost, usize, R) -> Result<()> + 'f;

/// What a unit of [`Analyze::run_job_units`] is applied with.
pub(crate) type ApplyFn<'f, J> =
    dyn FnMut(&mut Analyze, &mut dyn AnalyzeHost, J) -> Result<()> + 'f;

impl Analyze {
    /// The number of job-queue workers (`MAX_CONCURRENCY` when the analyze
    /// object was made, as in C++).
    pub fn job_workers(&self) -> usize {
        self.job_workers
    }

    /// How much work has run on worker threads so far.
    pub fn worker_stats(&self) -> WorkerStats {
        self.worker_stats
    }

    /// Run units of analyze jobs and apply each one's result in order.
    ///
    /// Each unit is an item, the number of jobs (seeds) it runs, and `run`,
    /// which does the work of the unit's jobs in order, taking one RNG from
    /// its [`JobSeeds`] per job. One at a time (one worker, or the host
    /// cannot make worker worlds, or fewer than two units remain), each
    /// unit runs on the host as soon as the previous one is applied, its
    /// seeds drawn as its jobs start: the single-threaded queue. Otherwise
    /// all remaining units run on worker threads with seeds drawn up front,
    /// and are applied in order afterwards, with what each printed, up to
    /// the first failure (whose error is returned after what it printed).
    /// `ready` is asked before units go to the workers: while it says no,
    /// the next unit runs on the host.
    pub(crate) fn run_job_units<J: Clone + Send>(
        &mut self,
        host: &mut dyn AnalyzeHost,
        units: Vec<(J, usize)>,
        run: &JobFn<'_, J>,
        ready: &dyn Fn(&dyn AnalyzeHost) -> bool,
        apply: &mut ApplyFn<'_, J>,
    ) -> Result<()> {
        let mut units = std::collections::VecDeque::from(units);
        while !units.is_empty() {
            if units.len() >= 2 && self.job_workers > 1 && ready(&*host) {
                let pending: Vec<(J, usize)> = units.iter().cloned().collect();
                if let Some(done) = self.run_units_on_workers(host, pending, run)? {
                    units.drain(..done.len());
                    for d in done {
                        replay(d.printed);
                        let item = d.result?;
                        apply(self, host, item)?;
                        self.worker_stats.units += 1;
                    }
                    continue;
                }
                self.worker_stats.refusals += 1;
                // The workers could not be used: everything left runs here.
                while let Some((item, _)) = units.pop_front() {
                    self.run_unit_on_host(host, item, run, apply)?;
                }
                return Ok(());
            }
            if let Some((item, _)) = units.pop_front() {
                self.run_unit_on_host(host, item, run, apply)?;
            }
        }
        Ok(())
    }

    fn run_unit_on_host<J>(
        &mut self,
        host: &mut dyn AnalyzeHost,
        mut item: J,
        run: &JobFn<'_, J>,
        apply: &mut ApplyFn<'_, J>,
    ) -> Result<()> {
        let mut seeds = JobSeeds {
            source: SeedSource::Draw(self.job_seed_rng.as_mut()),
            used: 0,
        };
        run(&mut item, &mut OnHost(&mut *host), &mut seeds)?;
        apply(self, host, item)
    }

    /// The parallel half of [`Analyze::run_job_units`]: `None` when the
    /// workers cannot be used (nothing has changed then). Otherwise the
    /// outcomes in order, cut at the first failure, with the queue's seed
    /// RNG left where the single-threaded queue would have left it.
    fn run_units_on_workers<J: Send>(
        &mut self,
        host: &mut dyn AnalyzeHost,
        units: Vec<(J, usize)>,
        run: &JobFn<'_, J>,
    ) -> Result<Option<Vec<Done<J>>>> {
        let Some(seed_rng) = self.job_seed_rng.as_ref() else {
            return Ok(None);
        };
        let n_workers = self.job_workers.min(units.len());
        let Some(worlds) = host.worker_worlds(n_workers) else {
            return Ok(None);
        };
        let mut drawn = seed_rng.clone();
        let total: usize = units.iter().map(|(_, k)| *k).sum();
        let seeds: Vec<i32> = (0..total).map(|_| draw_seed(&mut drawn)).collect();
        let mut items = Vec::with_capacity(units.len());
        let mut at = 0;
        for (item, k) in units {
            items.push((item, at..at + k));
            at += k;
        }
        let seeds = &seeds[..];
        let job = |world: &mut (dyn WorkerWorld + '_),
                   (mut item, range): (J, std::ops::Range<usize>),
                   used: &mut usize|
         -> Result<J> {
            let mut js = JobSeeds {
                source: SeedSource::Drawn(&seeds[range]),
                used: 0,
            };
            let r = run(&mut item, &mut OnWorker(world), &mut js);
            *used = js.used;
            r.map(|()| item)
        };
        let (done, digests) = run_on_workers(worlds, items, &job, &|_| false);
        if !host.adopt_worker_states(&digests) {
            return Ok(None);
        }
        // Leave the seed RNG after the seeds the returned units used (all
        // of theirs, or up to the failure that cut the run).
        let used: usize = done.iter().map(|d| d.seeds_used).sum();
        let mut rng = seed_rng.clone();
        for _ in 0..used {
            draw_seed(&mut rng);
        }
        self.job_seed_rng = Some(rng);
        Ok(Some(done))
    }

    /// Run job `i` for `i` in `0..n` as the C++ runs them, one after
    /// another in the default context (the world RNG, the default
    /// context's analyze-mode flag), applying each result in order with
    /// `apply`. `serial` runs job `i` on the host itself.
    ///
    /// Each job's start depends on the earlier ones only through the world
    /// RNG, so with workers the remaining jobs run in rounds, each job from
    /// a predicted start state: the state reached from the current one by
    /// the number of draws the earlier jobs made in the previous round
    /// (none in the first round). A result is applied only when its job
    /// started from the exact state: the first job of a round does, and
    /// each later one does if the job before it (itself exact) ended in the
    /// state predicted for it. The first mismatch ends the round; the world
    /// RNG is then where the last applied job left it, and the next round
    /// starts there with the counts this one measured. Most tests draw a
    /// fixed number of times whatever the values (the mutation checks of a
    /// divide draw even at rate 0), so a batch usually takes two rounds;
    /// when two rounds in a row each keep fewer results than there are
    /// workers, the remaining jobs run on the host. This needs at least
    /// three workers to pay for the second round, so with two the jobs run
    /// on the host.
    pub(crate) fn run_default_context_jobs<R: Send>(
        &mut self,
        host: &mut dyn AnalyzeHost,
        n: usize,
        job: &DefaultJobFn<'_, R>,
        serial: &mut SerialFn<'_, R>,
        apply: &mut ApplyIndexedFn<'_, R>,
    ) -> Result<()> {
        let mut next = 0;
        let mut speculate = self.job_workers >= MIN_DEFAULT_CONTEXT_WORKERS;
        // Draw counts measured for each job (from whatever state it last
        // started in).
        let mut counts: Vec<Option<usize>> = vec![None; n];
        let mut short_rounds = 0;
        while next < n {
            if speculate && n - next >= 2 {
                let Some(start) = host.world_rng().map(|r| r.clone()) else {
                    speculate = false;
                    continue;
                };
                let starts = predicted_starts(&start, &counts[next..]);
                match self.speculate(host, next, &starts, job)? {
                    Some(round) => {
                        self.worker_stats.speculative_rounds += 1;
                        let mut kept = 0;
                        // The state the last applied job ended in, while
                        // every job so far started exactly.
                        let mut chain: Option<String> = None;
                        for (k, d) in round.into_iter().enumerate() {
                            let i = next + k;
                            let exact = k == 0
                                || chain.as_deref().is_some_and(|end| match &d.result {
                                    Ok(s) => s.start == end,
                                    Err(_) => format!("{:?}", starts[k]) == end,
                                });
                            if !exact {
                                chain = None;
                                if let Ok(s) = d.result {
                                    counts[i] = s.draws;
                                }
                                continue;
                            }
                            replay(d.printed);
                            let s = d.result?;
                            apply(self, host, i, s.result)?;
                            self.worker_stats.speculative_jobs += 1;
                            kept += 1;
                            counts[i] = s.draws;
                            chain = Some(s.end.state);
                            if let Some(world) = host.world_rng() {
                                *world = s.end.rng;
                            }
                        }
                        next += kept;
                        if kept < self.job_workers {
                            short_rounds += 1;
                            if short_rounds >= 2 {
                                speculate = false;
                            }
                        } else {
                            short_rounds = 0;
                        }
                        continue;
                    }
                    None => {
                        self.worker_stats.refusals += 1;
                        speculate = false;
                    }
                }
            }
            let r = serial(self, host, next)?;
            apply(self, host, next, r)?;
            next += 1;
        }
        Ok(())
    }

    /// One round of [`Analyze::run_default_context_jobs`]: job `from + k`
    /// runs from `starts[k]`. The outcomes in order (cut at the first
    /// failure), or `None` when the workers cannot be used.
    fn speculate<R: Send>(
        &mut self,
        host: &mut dyn AnalyzeHost,
        from: usize,
        starts: &[AvidaRng],
        job: &DefaultJobFn<'_, R>,
    ) -> Result<Option<Vec<Done<Speculated<R>>>>> {
        let analyze_mode = host.analyze_mode();
        let n_workers = self.job_workers.min(starts.len());
        let Some(worlds) = host.worker_worlds(n_workers) else {
            return Ok(None);
        };
        let items: Vec<usize> = (0..starts.len()).collect();
        let run = |world: &mut (dyn WorkerWorld + '_), k: usize, _: &mut usize| {
            let start = RngState::of(starts[k].clone());
            let mut rng = starts[k].clone();
            let result = job(
                &mut |f: &mut dyn FnMut(&mut TestWorld<'_>) -> Result<()>| {
                    world.with_test_world(&mut rng, analyze_mode, f)
                },
                from + k,
            )?;
            let end = RngState::of(rng);
            Ok(Speculated {
                result,
                draws: draws_between(&start, &end),
                start: start.state,
                end,
            })
        };
        let (done, digests) = run_on_workers(worlds, items, &run, &|_| false);
        if !host.adopt_worker_states(&digests) {
            return Ok(None);
        }
        Ok(Some(done))
    }
}

/// A job run ahead by [`Analyze::run_default_context_jobs`].
struct Speculated<R> {
    result: R,
    /// The state of the RNG it started from ([`RngState::state`]).
    start: String,
    /// The RNG it left.
    end: RngState,
    /// The draws it made ([`draws_between`]).
    draws: Option<usize>,
}

/// An RNG with its whole state as text (`Debug`), which is how two states
/// are compared. Made on the worker, so the calling thread only compares.
struct RngState {
    rng: AvidaRng,
    state: String,
}

impl RngState {
    fn of(rng: AvidaRng) -> Self {
        let state = format!("{rng:?}");
        Self { rng, state }
    }
}

/// Fewest workers for which [`Analyze::run_default_context_jobs`] runs jobs
/// ahead (two rounds of work for one round's results).
const MIN_DEFAULT_CONTEXT_WORKERS: usize = 3;

/// Draws after which [`draws_between`] gives up.
const MAX_COUNTED_DRAWS: usize = 1 << 16;

/// The next values `rng` would give, without advancing it: a quick test
/// for two states being different.
fn probe(rng: &AvidaRng) -> [u64; 4] {
    let mut r = rng.clone();
    [(); 4].map(|_| r.get_double().to_bits())
}

/// The number of draws (`GetDouble`) that take `start` to `end`, if it is
/// at most [`MAX_COUNTED_DRAWS`] and the cached exponential of
/// `GetRandNormal` did not change. Only a prediction: a wrong count costs a
/// round, never a result.
fn draws_between(start: &RngState, end: &RngState) -> Option<usize> {
    let tail = |d: &str| d.find("m_rand_norm_exprv").map(|i| d[i..].to_string());
    if tail(&start.state) != tail(&end.state) {
        return None;
    }
    let target = probe(&end.rng);
    // `next` holds the values `cur` would give next ([`probe`]); `ahead`
    // is four draws past `cur`.
    let mut cur = start.rng.clone();
    let mut ahead = start.rng.clone();
    let mut next = [(); 4].map(|_| ahead.get_double().to_bits());
    for d in 0..=MAX_COUNTED_DRAWS {
        if next == target && format!("{cur:?}") == end.state {
            return Some(d);
        }
        cur.get_double();
        next.rotate_left(1);
        next[3] = ahead.get_double().to_bits();
    }
    None
}

/// The start state of each remaining job: `start` for the first, then each
/// advanced by the draws counted for the job before it (by none when no
/// count is known).
fn predicted_starts(start: &AvidaRng, counts: &[Option<usize>]) -> Vec<AvidaRng> {
    let mut starts = Vec::with_capacity(counts.len());
    let mut cur = start.clone();
    for count in counts {
        starts.push(cur.clone());
        for _ in 0..count.unwrap_or(0) {
            cur.get_double();
        }
    }
    starts
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A worker world without a test CPU: the queue tests never run one.
    struct NoCpu(InstSetRegistry);

    impl WorkerWorld for NoCpu {
        fn with_test_world(
            &mut self,
            _rng: &mut AvidaRng,
            _analyze_mode: bool,
            _f: &mut dyn FnMut(&mut TestWorld<'_>) -> Result<()>,
        ) -> Result<()> {
            Err(AnalyzeError::fatal("no test CPU"))
        }
        fn inst_sets(&self) -> &InstSetRegistry {
            &self.0
        }
        fn state_digest(&self) -> String {
            String::new()
        }
    }

    fn worlds(n: usize) -> Vec<Box<dyn WorkerWorld>> {
        (0..n)
            .map(|_| Box::new(NoCpu(InstSetRegistry::new())) as Box<dyn WorkerWorld>)
            .collect()
    }

    /// A job that takes a varying time (so workers finish out of order),
    /// prints its index to both streams and returns it doubled; `fail`
    /// fails.
    fn job(i: usize, fail: Option<usize>) -> Result<usize> {
        let micros = (i * 7919 % 13) as u64 * 50;
        std::thread::sleep(std::time::Duration::from_micros(micros));
        diag::stdout(format_args!("out {i}\n"));
        diag::stderr(format_args!("err {i}\n"));
        if Some(i) == fail {
            return Err(AnalyzeError::fatal(format!("job {i}")));
        }
        Ok(2 * i)
    }

    #[test]
    fn outcomes_come_back_in_order_with_their_output() {
        for n_workers in [1, 2, 3, 8, 16] {
            for _ in 0..10 {
                let items: Vec<usize> = (0..200).collect();
                let (done, digests) = run_on_workers(
                    worlds(n_workers),
                    items,
                    &|_, i, used| {
                        *used = i % 3;
                        job(i, None)
                    },
                    &|_| false,
                );
                assert_eq!(digests.len(), n_workers);
                assert_eq!(done.len(), 200);
                for (i, d) in done.into_iter().enumerate() {
                    assert_eq!(d.result.ok(), Some(2 * i));
                    assert_eq!(d.seeds_used, i % 3);
                    assert_eq!(d.printed.stdout, format!("out {i}\n"));
                    assert_eq!(d.printed.stderr, format!("err {i}\n"));
                }
            }
        }
    }

    #[test]
    fn a_failure_or_a_stop_cuts_the_run_after_it() {
        for n_workers in [2, 8] {
            for _ in 0..10 {
                let (done, _) = run_on_workers(
                    worlds(n_workers),
                    (0..100).collect(),
                    &|_, i, _| job(i, Some(57)),
                    &|_| false,
                );
                assert_eq!(done.len(), 58);
                assert!(done[..57].iter().all(|d| d.result.is_ok()));
                assert!(done[57].result.is_err());
                assert_eq!(done[57].printed.stdout, "out 57\n");

                let (done, _) = run_on_workers(
                    worlds(n_workers),
                    (0..100).collect(),
                    &|_, i, _| job(i, None),
                    &|r| *r == 2 * 33,
                );
                assert_eq!(done.len(), 34);
                assert!(done
                    .iter()
                    .enumerate()
                    .all(|(i, d)| d.result.as_ref().ok() == Some(&(2 * i))));
            }
        }
    }

    #[test]
    fn a_capture_on_the_calling_thread_is_left_alone() {
        diag::start_capture();
        diag::stdout(format_args!("before\n"));
        let (done, _) = run_on_workers(
            worlds(4),
            (0..20).collect(),
            &|_, i, _| job(i, None),
            &|_| false,
        );
        diag::stdout(format_args!("after\n"));
        for d in done {
            replay(d.printed);
        }
        let got = diag::stop_capture();
        let mut want = "before\nafter\n".to_string();
        for i in 0..20 {
            want.push_str(&format!("out {i}\n"));
        }
        assert_eq!(got.stdout, want);
        assert_eq!(
            got.stderr,
            (0..20).map(|i| format!("err {i}\n")).collect::<String>()
        );
    }

    #[test]
    fn worker_count_follows_max_concurrency() {
        let cpus = std::thread::available_parallelism().map_or(1, |n| n.get());
        assert_eq!(worker_count(-1), cpus);
        assert_eq!(worker_count(0), cpus);
        assert_eq!(worker_count(1), 1);
        assert_eq!(worker_count(2), 2.min(cpus));
        assert_eq!(worker_count(100_000), cpus);
    }

    #[test]
    fn draw_counts_and_predicted_states() {
        let start = AvidaRng::new(12345);
        let mut end = start.clone();
        for _ in 0..37 {
            end.get_double();
        }
        let _ = end.p(0.0);
        let _ = end.get_uint(10);
        let state = |rng: &AvidaRng| RngState::of(rng.clone());
        assert_eq!(draws_between(&state(&start), &state(&end)), Some(39));
        assert_eq!(draws_between(&state(&start), &state(&start)), Some(0));
        let mut normal = start.clone();
        let _ = normal.get_rand_normal();
        assert_eq!(draws_between(&state(&start), &state(&normal)), None);
        // The start is never reached again by going forward.
        assert_eq!(draws_between(&state(&end), &state(&start)), None);

        let same_state = |a: &AvidaRng, b: &AvidaRng| state(a).state == state(b).state;
        let starts = predicted_starts(&start, &[Some(39), None, Some(2)]);
        assert!(same_state(&starts[0], &start));
        assert!(same_state(&starts[1], &end));
        assert!(same_state(&starts[2], &end));
        assert!(!same_state(&starts[0], &starts[1]));
    }
}
