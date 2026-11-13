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

use std::cell::RefCell;

use avida_apto::AvidaRng;
use avida_config::config::Config;
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
}

/// What an output test in a burst sees of the resources: the cell's
/// amounts, as `view` had them when the burst began less what the organism
/// itself consumed since (and plus what it produced).
pub struct LocalResources<'a> {
    pub view: &'a dyn CellResources,
    /// The organism's own copy of its cell's amounts; empty until the first
    /// output test of the burst reads them.
    pub levels: &'a mut Vec<f64>,
}

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
                || res.isgradient
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
    for i in 0..lib.size().max(0) as usize {
        let r = lib.reaction(i);
        if !r.active {
            continue;
        }
        let Some(task) = r.task else {
            return false;
        };
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
    if let Some(r) = lo.resources.as_mut() {
        if r.levels.is_empty() {
            r.view.cell_levels(lo.cell_id, r.levels);
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
        );
        let first_reactions = std::mem::take(&mut evaluator.first_reactions);
        drop(evaluator);
        for r in first_reactions {
            phen.set_first_reaction_cycle(r.max(0) as usize);
            phen.set_first_reaction_exec(r.max(0) as usize);
        }
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
    lo.events.extend(events.into_inner());
    if let Some(r) = lo.resources.as_mut() {
        if res_change.iter().any(|&c| c != 0.0) {
            // `cResourceCount::ModifyCell` on this cell: `Rate` adds the
            // change to the cell's pending change (0), `State` folds it in.
            for (level, &change) in levels.iter_mut().zip(&res_change) {
                *level += 0.0 + change;
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
