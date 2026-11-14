//! The output test of an organism inside a burst of the parallel execution
//! mode (`EXECUTION_MODE 1`; ARCHITECTURE.md, "Output tests in the burst").
//!
//! `cOrganism::DoOutput` reaches the world for the resources, the neighbours'
//! buffers, the statistics and the deme, and runs the environment, which the
//! C++ lends mutably. For an environment whose tasks read only the organism's
//! buffers and whose reactions use no resource ([`output_is_local`]) none of
//! that is needed: the organism evaluates on a shared borrow of the
//! environment ([`avida_environment::Environment::test_output_shared`]) and
//! writes what the exact path would have added to the world's statistics and
//! to its deme as [`OutEvent`]s in its own list, which the world replays in
//! cell order after the parallel phase.
//!
//! With resources the same holds when every resource is a plain per-cell
//! grid (`ResourceCount::are_plain_cell_grids`): an organism reads and
//! changes only the amounts in its own cell, which nobody else touches during
//! the parallel phase and which the world has brought up to date before it.
//! The burst reads them through [`CellResources`] and keeps its own
//! changes in a local copy ([`LocalResources`]), so that its next output
//! test sees them, and writes each change as an [`OutEvent::ResourceChange`]
//! for the world to apply.
//!
//! A global (well-mixed) resource is one pool every organism reads and
//! consumes, so its output tests cannot be evaluated against a copy. They
//! are split instead ([`PendingOutput`]): the burst evaluates the tasks
//! (which read the organism's buffers only) and leaves the processes of
//! the reactions that use the pool undone, the organism stops after the
//! instruction, and the world runs the processes against the pool as it is,
//! organism after organism in a fixed order ([`finish_output_global`]),
//! with the arithmetic of the exact path.

use std::cell::RefCell;

use avida_apto::AvidaRng;
use avida_config::config::Config;
use avida_environment::{DeferredReaction, ReactionResult};
use avida_genome::InstSet;
use avida_phenotype::{PhenotypeDeme, PhenotypeStats};
use avida_tasks::{TaskContext, TaskWorld};

use crate::organism::{OrgState, OutputEffects};
use crate::task_adapter::{Env, OrgTaskEvaluator, OrgTaskWorld, OutputHost, PlasticityTest};

/// One change the output test of a burst would have made to the world's
/// statistics (`cStats`) or to the organism's deme, to be applied in order
/// by the world after the parallel phase.
#[derive(Debug, Clone, PartialEq)]
pub enum OutEvent {
    /// `AgeTaskEvent(org_id, task_id, org_age)`
    AgeTask(i32, usize, i32),
    /// `AddNewTaskCount(task)`
    NewTask(usize),
    /// `AddOtherTaskCounts(task, prev, cur)`
    OtherTasks(usize, i32, i32),
    /// `AddNewReactionCount(reaction)`
    NewReaction(usize),
    /// `AddTaskSwitchTime(t1, t2, time)`
    SwitchTime(i32, i32, i32),
    /// The deme's heritable merit, multiplied by 1 and increased by 0 (a
    /// reaction with a deme bonus of its own makes no environment local).
    DemeMeritUnchanged,
    /// `AddCurTask(task)` of the deme
    DemeTask(usize),
    /// `AddCurReaction(reaction)` of the deme
    DemeReaction(usize),
    /// `UpdateResources(res_change)`: the change of every resource in the
    /// organism's cell (produced less consumed), as the exact path hands it
    /// to `cResourceCount::ModifyCell`.
    ResourceChange(Box<[f64]>),
}

/// The amounts of the resources in a cell, read in the parallel phase.
pub trait CellResources: Sync {
    /// The amount of every resource in `cell` (what
    /// `cResourceCount::GetCellResources` returns once the grids are up to
    /// date), into `out`.
    fn cell_levels(&self, cell: i32, out: &mut Vec<f64>);

    /// The pending change (`cSpatialCountElem::delta`) of every resource in
    /// `cell`, into `out`, which `ModifyCell` folds into the amount with the
    /// organism's own change. The default is none (empty: all zero), which
    /// is what a plain grid has between updates.
    fn cell_deltas(&self, _cell: i32, out: &mut Vec<f64>) {
        out.clear();
    }
}

/// What an output test in a burst sees of the resources: the cell's
/// amounts, as `view` had them when the burst began less what the organism
/// itself consumed since (and plus what it produced).
pub struct LocalResources<'a> {
    pub view: &'a dyn CellResources,
    /// The organism's own copy of its cell's amounts; empty until the first
    /// output test of the burst reads them.
    pub levels: &'a mut Vec<f64>,
    /// The pending change of each resource in the cell, as `view` had it
    /// when the burst began (empty: none), until an output test changes the
    /// cell and `ModifyCell` folds it in.
    pub deltas: &'a mut Vec<f64>,
}

/// An output test the burst evaluated up to the processes that use the
/// global resource pool ([`output_is_local_global`]).
#[derive(Debug, Clone)]
pub struct PendingOutput {
    /// The tasks done and the reactions without the pool, recorded;
    /// the pool's reactions not yet.
    pub result: ReactionResult,
    /// What `TestOutput` returned (`result.active()`).
    pub found: bool,
    /// The reactions whose processes wait, in library order.
    pub deferred: Vec<DeferredReaction>,
    /// In a world with grids as well as pools: the cell's amounts as the
    /// test read them, which the deferred processes read for the grids (a
    /// reaction may take from a grid and give to a pool, or the other way
    /// round); the pools' amounts are those of the settlement. Empty
    /// otherwise.
    pub grid_levels: Vec<f64>,
}

/// [`crate::parallel::ParallelOrgState`]'s slot for a [`PendingOutput`].
#[derive(Debug, Clone, Default)]
pub struct PendingOutputSlot(pub Option<Box<PendingOutput>>);

/// The statistics half of the list an output test writes its
/// [`OutEvent`]s to (the phenotype takes the statistics and the deme as two
/// borrows, so they share the list through a `RefCell`).
pub struct StatSink<'a>(pub &'a RefCell<Vec<OutEvent>>);

impl PhenotypeStats for StatSink<'_> {
    fn add_energy_testament_accepted(&mut self, _value: f64) {}
    fn age_task_event(&mut self, org_id: i32, task_id: usize, org_age: i32) {
        self.0
            .borrow_mut()
            .push(OutEvent::AgeTask(org_id, task_id, org_age));
    }
    fn add_new_task_count(&mut self, task_num: usize) {
        self.0.borrow_mut().push(OutEvent::NewTask(task_num));
    }
    fn add_other_task_counts(&mut self, task_num: usize, prev_tasks: i32, cur_tasks: i32) {
        self.0
            .borrow_mut()
            .push(OutEvent::OtherTasks(task_num, prev_tasks, cur_tasks));
    }
    fn add_new_reaction_count(&mut self, reaction_num: usize) {
        self.0
            .borrow_mut()
            .push(OutEvent::NewReaction(reaction_num));
    }
    fn add_task_switch_time(&mut self, t1: i32, t2: i32, time: i32) {
        self.0.borrow_mut().push(OutEvent::SwitchTime(t1, t2, time));
    }
}

/// The deme half of that list.
pub struct DemeSink<'a> {
    pub events: &'a RefCell<Vec<OutEvent>>,
    /// The phenotype asked the deme for something an environment of
    /// [`output_is_local`] cannot make it ask.
    pub violated: bool,
}

impl PhenotypeDeme for DemeSink<'_> {
    /// The merit the deme would be asked to change is read as 1: the
    /// environments of [`output_is_local`] only ever multiply it by 1 and
    /// add 0, and [`PhenotypeDeme::update_heritable_deme_merit`] checks that.
    fn heritable_deme_merit(&self) -> f64 {
        1.0
    }
    fn update_heritable_deme_merit(&mut self, value: f64) {
        if value == 1.0 {
            self.events.borrow_mut().push(OutEvent::DemeMeritUnchanged);
        } else {
            self.violated = true;
        }
    }
    fn add_cur_task(&mut self, task_num: usize) {
        self.events.borrow_mut().push(OutEvent::DemeTask(task_num));
    }
    fn add_cur_reaction(&mut self, reaction_num: usize) {
        self.events
            .borrow_mut()
            .push(OutEvent::DemeReaction(reaction_num));
    }
}

/// The world data an output test in a burst reads: what
/// [`OrgInterface::local_output`] hands out.
pub struct LocalOutput<'a> {
    pub config: &'a Config,
    pub environment: &'a Env,
    pub rng: &'a mut AvidaRng,
    pub update: i32,
    pub cell_id: i32,
    /// Where the statistics, deme and resource changes go.
    pub events: &'a mut Vec<OutEvent>,
    /// The cell's resources, when the environment has any.
    pub resources: Option<LocalResources<'a>>,
    /// Where an output test leaves the processes that use the global pool
    /// (the environment's resources are global pools,
    /// [`output_is_local_global`]).
    pub pending: Option<&'a mut PendingOutputSlot>,
    /// With `pending` in a world of grids and pools together
    /// ([`output_is_local_mixed`]): which resources are the pools, whose
    /// processes wait. `None`: every resource is one.
    pub pools: Option<&'a [bool]>,
}

/// Whether the output tests of the whole run can be organism-local
/// ([`do_output_local`]): no resources in the environment, so none to read,
/// consume or produce; every task a pure function of the organism's buffers
/// ([`avida_tasks::TaskLib::is_organism_local`]); processes that only add to
/// or multiply the bonus (no product, detection, lethality, sterilisation,
/// germline, deme fraction, instruction or plastic bonus); no neighbour
/// input or output, state grids or avatars; no energy model, task-switch
/// penalty or immediate merit update (which reach the world).
///
/// With `cell_resources` (the population's resources are all plain per-cell
/// grids, see [`CellResources`]) the environment may have resources, and
/// processes may consume and produce them: no deme, energy, HGT, deadly or
/// predatory resource, no resource bins and no detection, which reads a
/// resource and draws random numbers.
pub fn output_is_local(cfg: &Config, env: &Env, cell_resources: bool) -> bool {
    output_is_local_with(
        cfg,
        env,
        if cell_resources {
            Pools::Cells
        } else {
            Pools::None
        },
        &[],
    )
}

/// [`output_is_local`] for a population whose resources are all global pools
/// (`ResourceCount::are_global_pools`), which the output tests settle in
/// the sequential phase ([`PendingOutput`]). The tests of the library are
/// the same, and one more condition: no reaction's requisite looks at the
/// counts of other reactions (`reaction`, `noreaction`, the total count),
/// which the processes left for later would not have updated yet when the
/// requisite is tested, and no requisite that lets a reaction fire on divide
/// (`divide_only`), whose test would leave the pool's half to the world with
/// the divide under way.
pub fn output_is_local_global(cfg: &Config, env: &Env) -> bool {
    output_is_local_with(cfg, env, Pools::Global, &[])
}

/// [`output_is_local`] for a population whose resources are some per-cell
/// grids and some global pools (`pools[i]` is true for a pool,
/// `ResourceCount::grid_or_global_kinds`): the processes that use a grid
/// run in the burst, those that use a pool wait for the sequential phase
/// ([`PendingOutput`]). The conditions of [`output_is_local_global`], and
/// one more: a grid that a reaction waiting for the pools takes from is not
/// taken from by a later reaction that does not wait (see the gate's
/// comment). A reaction that uses both kinds waits as a whole, and reads the
/// grids' amounts as the test saw them ([`PendingOutput::grid_levels`]).
pub fn output_is_local_mixed(cfg: &Config, env: &Env, pools: &[bool]) -> bool {
    output_is_local_with(cfg, env, Pools::Mixed, pools)
}

/// What the resources of the population are, for the gate.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Pools {
    /// No resource is read or changed.
    None,
    /// Per-cell grids ([`CellResources`]).
    Cells,
    /// Global pools ([`PendingOutput`]).
    Global,
    /// Grids and pools together.
    Mixed,
}

fn output_is_local_with(cfg: &Config, env: &Env, pools: Pools, is_pool: &[bool]) -> bool {
    let cell_resources = pools != Pools::None;
    if cfg.use_avatars != 0
        || cfg.energy_enabled
        || cfg.merit_inc_apply_immediate
        || cfg.task_switch_penalty_type != 0
    {
        return false;
    }
    let num_resources = env.resource_lib().size().max(0) as usize;
    if num_resources != 0 {
        if !cell_resources || cfg.use_resource_bins || env.resource_lib().num_deme_resources() != 0
        {
            return false;
        }
        for res in env.resource_lib().resources() {
            if res.deme_resource
                || res.energy_resource
                || res.hgt_metabolize
                // A gradient is read and changed like a plain grid; one
                // whose update acts on organisms is left to the world (its
                // flags are set at run time, `ResourceCount::are_cell_grids`).
                || res.damage != 0.0
                || res.deadly
                || res.predator
            {
                return false;
            }
        }
    }
    if env.num_state_grids() != 0 || env.use_neighbor_input() || env.use_neighbor_output() {
        return false;
    }
    let lib = env.reaction_lib();
    // (grid, reaction index) for the reactions that wait and those that run
    // in the burst, in a world of grids and pools.
    let mut grid_taken_by_waiting: Vec<(usize, usize)> = Vec::new();
    let mut grid_taken_by_running: Vec<(usize, usize)> = Vec::new();
    for i in 0..lib.size().max(0) as usize {
        let r = lib.reaction(i);
        if !r.active {
            continue;
        }
        let Some(task) = r.task else {
            return false;
        };
        if pools == Pools::Mixed {
            // A reaction that takes from or gives to a pool waits for the
            // sequential phase as a whole, and runs there after the
            // reactions of the grids. The order of the exact mode is the
            // library's, and it matters through what a process takes from
            // a resource that an earlier reaction already took from
            // (`DoProcesses` subtracts what the result holds), so a grid
            // may not be taken from by a waiting reaction and then by a
            // reaction that does not wait and comes later in the library.
            let kind = |res: Option<usize>| match res {
                None => Some(false),
                Some(i) => is_pool.get(i).copied(),
            };
            let mut waits = false;
            for p in &r.processes {
                let (Some(a), Some(b)) = (kind(p.resource), kind(p.product)) else {
                    return false;
                };
                waits |= a || b;
            }
            if waits {
                grid_taken_by_waiting.extend(
                    r.processes
                        .iter()
                        .filter_map(|p| p.resource)
                        .filter(|&g| is_pool.get(g) == Some(&false))
                        .map(|g| (g, i)),
                );
            } else {
                grid_taken_by_running.extend(
                    r.processes
                        .iter()
                        .filter_map(|p| p.resource)
                        .filter(|&g| is_pool.get(g) == Some(&false))
                        .map(|g| (g, i)),
                );
            }
        }
        if matches!(pools, Pools::Global | Pools::Mixed)
            && r.requisites.iter().any(|q| {
                !q.prior_reactions.is_empty()
                    || !q.prior_noreactions.is_empty()
                    || q.min_tot_reaction_count != 0
                    || q.max_tot_reaction_count != i32::MAX
                    // A test on divide would leave its pool half to the
                    // world with the divide already under way.
                    || q.divide_only != 0
            })
        {
            return false;
        }
        if task < 0 || !env.tasklib().0.is_organism_local(task as usize) {
            return false;
        }
        for p in &r.processes {
            let plain = p.resource.is_none_or(|r| r < num_resources)
                && p.product.is_none_or(|r| r < num_resources)
                && p.detect.is_none()
                && !p.internal
                && !p.is_random_resource
                && p.lethal == 0.0
                && !p.sterilize
                && !p.is_germline
                && p.deme_fraction == 0.0
                && p.inst.is_empty()
                && p.match_string.is_empty()
                && p.pp_method == avida_environment::reaction::PhenPlastBonusMethod::Default
                && matches!(
                    p.proc_type,
                    avida_environment::reaction::proc_type::ADD
                        | avida_environment::reaction::proc_type::MULT
                        | avida_environment::reaction::proc_type::POW
                        | avida_environment::reaction::proc_type::LIN
                );
            if !plain {
                return false;
            }
        }
    }
    // A waiting reaction runs after every running one: a grid taken by a
    // waiting reaction must not be taken by a running reaction of a higher
    // index.
    for &(g, w) in &grid_taken_by_waiting {
        if grid_taken_by_running
            .iter()
            .any(|&(g2, r)| g2 == g && r > w)
        {
            return false;
        }
    }
    true
}

/// The host of a task evaluation in a burst: [`output_is_local`] admits only
/// tasks that never ask it anything but the cell.
pub struct PureHost {
    pub cell_id: i32,
}

impl TaskWorld for PureHost {
    fn get_cell_id(&self) -> i32 {
        self.cell_id
    }
    fn get_av_cell_id(&self) -> i32 {
        -1
    }
    fn get_prev_seen_cell_id(&self) -> i32 {
        0
    }
    fn set_prev_seen_cell_id(&mut self, _id: i32) {}
    fn get_prev_task_cell_id(&self) -> i32 {
        0
    }
    fn set_prev_task_cell_id(&mut self, _id: i32) {}
    fn add_reached_task_cell(&mut self) {}
    fn get_cell_data(&self) -> i32 {
        -1
    }
    fn get_forage_target(&self) -> i32 {
        -1
    }
    fn get_gradient_movement(&self) -> f64 {
        0.0
    }
    fn has_opinion(&self) -> bool {
        false
    }
    fn get_opinion_first(&self) -> i32 {
        0
    }
    fn get_event_killed(&self) -> bool {
        false
    }
    fn get_lyse_display(&self) -> bool {
        false
    }
    fn get_input_at(&self, _i: i32) -> i32 {
        0
    }
    fn get_kaboom_executed(&self) -> bool {
        false
    }
    fn get_kaboom_executed2(&self) -> bool {
        false
    }
    fn get_number_strings_produced(&self, _tag: i32) -> i32 {
        0
    }
    fn get_number_strings_on_hand(&self, _i: i32) -> i32 {
        0
    }
    fn update_tag(&mut self, _tag: i32, _bits: i32) {}
    fn get_tag_label(&self) -> i32 {
        -1
    }
    fn set_lineage_label(&mut self, _label: i32) {}
    fn get_reputation(&self) -> i32 {
        0
    }
    fn set_reputation(&mut self, _rep: i32) {}
    fn get_number_of_donations(&self) -> i32 {
        0
    }
    fn state_grid_name(&self) -> String {
        String::new()
    }
    fn state_grid_state_id(&self, _name: &str) -> i32 {
        -1
    }
    fn state_grid_num_states(&self) -> i32 {
        0
    }
    fn state_grid_width(&self) -> i32 {
        0
    }
    fn state_grid_height(&self) -> i32 {
        0
    }
    fn state_grid_state_at(&self, _cell: i32) -> i32 {
        0
    }
    fn deme_cell_position_x(&self, _cell_id: i32) -> i32 {
        0
    }
    fn deme_get_relative_cell_id(&self, cell_id: i32) -> i32 {
        cell_id
    }
    fn deme_num_events(&self) -> i32 {
        0
    }
    fn deme_cell_event_id(&self, _i: i32) -> i32 {
        -1
    }
    fn deme_num_movement_predicates(&self) -> i32 {
        0
    }
    fn deme_mov_predicate_event(&self, _predicate: i32, _event: i32) -> Option<(i32, bool)> {
        None
    }
    fn number_of_organisms_in_group(&mut self, _group_id: i32) -> i32 {
        0
    }
    fn world_x(&self) -> i32 {
        0
    }
    fn world_y(&self) -> i32 {
        0
    }
    fn use_avatars(&self) -> bool {
        false
    }
    fn match_already_produced(&self) -> bool {
        false
    }
    fn pheromone_resource_amount(&self) -> f64 {
        0.0
    }
    fn get_resources(&self) -> Vec<f64> {
        Vec::new()
    }
    fn get_av_resources(&self) -> Vec<f64> {
        Vec::new()
    }
    fn change_reaction_resource(&mut self, _task_id: usize, _resource_index: i32) -> bool {
        false
    }
    fn cell_kaboom_executed(&self, _cell_id: i32) -> Option<bool> {
        None
    }
    fn clear_kaboom_executed(&mut self, _cell_id: i32) {}
    fn std_rand(&mut self) -> i32 {
        0
    }
    fn stats_add_tag(&mut self, _tag: i32, _value: i32) {}
    fn stats_add_string_bits_matched_value(&mut self, _name: &str, _value: i32) {}
    fn stats_inc_perfect_match(&mut self, _amount: i32) {}
    fn stats_inc_perfect_match_org(&mut self) {}
}

impl OutputHost for PureHost {
    fn task_probability(&mut self, _test: PlasticityTest<'_>, _task_id: i32) -> f64 {
        0.0
    }
    fn metabolize_genome_fragment(&mut self, _rng: &mut AvidaRng) -> Option<i32> {
        None
    }
}

/// `cOrganism::doOutput` for an environment of [`output_is_local`], on the
/// organism's own buffers: the body of the exact path without the
/// resource, neighbour and avatar parts, which such an environment makes
/// empty.
pub fn do_output_local(
    org: &mut OrgState,
    ext_mem: &[i32],
    inst_set: &InstSet,
    lo: LocalOutput<'_>,
    on_divide: bool,
) -> Result<OutputEffects, String> {
    let mut snapshot = org.task_snapshot(lo.update);
    let mut host = PureHost {
        cell_id: lo.cell_id,
    };
    let events = RefCell::new(Vec::new());
    let mut insts_triggered: Vec<String> = Vec::new();
    let mut lo = lo;
    // The cell's amounts (the exact path's `res_count`), and the change the
    // test makes to them (`res_change`).
    let mut levels: Vec<f64> = Vec::new();
    let mut res_change: Vec<f64> = Vec::new();
    let mut deferred: Vec<DeferredReaction> = Vec::new();
    if let Some(r) = lo.resources.as_mut() {
        if r.levels.is_empty() {
            r.view.cell_levels(lo.cell_id, r.levels);
            r.view.cell_deltas(lo.cell_id, r.deltas);
        }
        if r.levels.len() != org.phenotype_sizes.num_resources {
            return Err(format!(
                "internal: an output test of a burst found {} resource amounts for {} resources \
                 (parallel phase of EXECUTION_MODE 1)",
                r.levels.len(),
                org.phenotype_sizes.num_resources
            ));
        }
        levels = std::mem::take(r.levels);
        res_change = vec![0.0; levels.len()];
    }
    {
        let phen = &mut org.phenotype;
        let sizes = org.phenotype_sizes;
        let mut result = phen.take_reaction_result(&sizes);
        let rbins: Vec<f64> = phen.cur_rbins_avail.clone();
        let mut ctx = TaskContext::new(
            &org.input_buf,
            &org.output_buf,
            &[],
            &[],
            ext_mem,
            on_divide,
            None,
        );
        ctx.task_states = Some(&mut phen.task_states);
        // The evaluator carries a statistics sink for plasticity tests,
        // which an environment of `output_is_local` never runs.
        let mut unused = StatSink(&events);
        let mut evaluator = OrgTaskEvaluator {
            ctx,
            world: OrgTaskWorld {
                host: &mut host,
                org: &mut snapshot,
                resource_change: None,
            },
            stolen_reaction_count: &phen.cur_stolen_reaction_count,
            first_reactions: Vec::new(),
            resource_names: Vec::new(),
            config: lo.config,
            stats: &mut unused,
            plasticity: None,
        };
        let found = lo.environment.test_output_shared(
            lo.rng,
            lo.config,
            &mut result,
            &mut evaluator,
            &phen.eff_task_count,
            &mut phen.cur_reaction_count,
            &levels,
            &rbins,
            false,
            if lo.pending.is_some() {
                Some(&mut deferred)
            } else {
                None
            },
            lo.pools,
        );
        let first_reactions = std::mem::take(&mut evaluator.first_reactions);
        drop(evaluator);
        for r in first_reactions {
            phen.set_first_reaction_cycle(r.max(0) as usize);
            phen.set_first_reaction_exec(r.max(0) as usize);
        }
        if let (Some(slot), false) = (lo.pending.as_mut(), deferred.is_empty()) {
            // The processes that use the pool wait for the world; the
            // phenotype takes the whole result then.
            slot.0 = Some(Box::new(PendingOutput {
                result,
                found,
                deferred,
                grid_levels: if lo.pools.is_some() {
                    levels.clone()
                } else {
                    Vec::new()
                },
            }));
        } else {
            let mut stats = StatSink(&events);
            let mut deme = DemeSink {
                events: &events,
                violated: false,
            };
            phen.apply_test_output(
                lo.config,
                &sizes,
                lo.update,
                org.id,
                found,
                result,
                &mut res_change,
                &mut insts_triggered,
                false,
                None,
                &mut stats,
                Some(&mut deme),
            );
            if deme.violated {
                return Err(
                    "internal: an output test of a burst asked its deme for a change \
                        its environment cannot make (parallel phase of EXECUTION_MODE 1)"
                        .to_string(),
                );
            }
        }
    }
    lo.events.extend(events.into_inner());
    if let Some(r) = lo.resources.as_mut() {
        if res_change.iter().any(|&c| c != 0.0) {
            // `cResourceCount::ModifyCell` on this cell: `Rate` adds the
            // change to the cell's pending change (0), `State` folds it in.
            // The cell's pending change (a gradient's) goes in with it.
            r.deltas.resize(levels.len(), 0.0);
            for ((level, delta), &change) in
                levels.iter_mut().zip(r.deltas.iter_mut()).zip(&res_change)
            {
                *level += *delta + change;
                *delta = 0.0;
            }
            lo.events
                .push(OutEvent::ResourceChange(res_change.into_boxed_slice()));
        }
        *r.levels = levels;
    }
    org.apply_task_snapshot_local(snapshot)?;
    let mut effects = OutputEffects::default();
    for name in &insts_triggered {
        effects.bonus_insts.push(inst_set.get_inst(name)?);
    }
    Ok(effects)
}

/// The rest of an output test the burst left pending
/// ([`PendingOutput`]): the processes of the reactions that use the global
/// pool, run against `levels` (the pool's amounts now) with `DoProcesses`'
/// arithmetic, and then what `doOutput` does with the result. Returns the
/// change of every resource (produced less consumed), for the caller to
/// apply to the pool, and the statistics and deme changes, for the caller
/// to replay.
///
/// `rng` is the argument of `DoProcesses`; nothing here draws from it.
pub fn finish_output_global(
    org: &mut OrgState,
    pending: PendingOutput,
    env: &Env,
    config: &Config,
    levels: &[f64],
    update: i32,
    rng: &mut AvidaRng,
) -> Result<(Vec<f64>, Vec<OutEvent>), String> {
    if levels.len() != org.phenotype_sizes.num_resources {
        return Err(format!(
            "internal: a pending output test found {} resource amounts for {} resources \
             (sequential phase of EXECUTION_MODE 1)",
            levels.len(),
            org.phenotype_sizes.num_resources
        ));
    }
    let PendingOutput {
        mut result,
        found,
        deferred,
        grid_levels: _,
    } = pending;
    let events = RefCell::new(Vec::new());
    let mut res_change = vec![0.0; levels.len()];
    let mut insts_triggered: Vec<String> = Vec::new();
    let phen = &mut org.phenotype;
    let sizes = org.phenotype_sizes;
    let rbins: Vec<f64> = phen.cur_rbins_avail.clone();
    let triggered = env.finish_deferred(
        rng,
        config,
        &mut result,
        &deferred,
        levels,
        &rbins,
        &mut phen.cur_reaction_count,
    );
    for r in triggered {
        phen.set_first_reaction_cycle(r);
        phen.set_first_reaction_exec(r);
    }
    let mut stats = StatSink(&events);
    let mut deme = DemeSink {
        events: &events,
        violated: false,
    };
    phen.apply_test_output(
        config,
        &sizes,
        update,
        org.id,
        found,
        result,
        &mut res_change,
        &mut insts_triggered,
        false,
        None,
        &mut stats,
        Some(&mut deme),
    );
    if deme.violated {
        return Err(
            "internal: a pending output test asked its deme for a change \
                    its environment cannot make (sequential phase of EXECUTION_MODE 1)"
                .to_string(),
        );
    }
    Ok((res_change, events.into_inner()))
}
