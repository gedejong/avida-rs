//! Glue between the real task library (`avida_tasks::TaskLib`, the port of
//! `cTaskLib`) and the environment's task seams
//! (`avida_environment::{TaskRegistry, TaskEvaluator}`), plus the
//! organism-side `cTaskContext` used by `cOrganism::doOutput`.
//!
//! [`TaskLibRegistry`] is a newtype so this crate can implement the
//! environment's trait for the task crate's type without touching either.

use crate::hardware_manager::HardwareManager;
use crate::organism::{OrgTaskSnapshot, OrgUid};
use crate::test_cpu::TestWorldCounts;
use avida_apto::AvidaRng;
use avida_config::config::Config;
use avida_config::feedback::Feedback as ConfigFeedback;
use avida_environment::{EnvReqs, TaskEnvHooks, TaskEvaluator, TaskRegistry};
use avida_genome::Genome;
use avida_phenotype::PhenotypeStats;
use avida_tasks::{FeedbackKind, TaskContext, TaskLib, TaskWorld};

/// `cEnvironment` over the real task library.
pub type Env = avida_environment::Environment<TaskLibRegistry>;

/// `cTaskLib` as seen by `cEnvironment`.
#[derive(Debug, Default)]
pub struct TaskLibRegistry(pub TaskLib);

impl TaskLibRegistry {
    /// An empty library.
    pub fn new() -> Self {
        TaskLibRegistry(TaskLib::new())
    }
}

impl TaskRegistry for TaskLibRegistry {
    /// `cTaskLib::AddTask`: the task crate's `add_task`, with its feedback
    /// forwarded in order and the group/target ids it registered handed to
    /// the environment hooks (`cEnvironment::AddGroupID` / `AddTargetID`).
    fn add_task(
        &mut self,
        name: &str,
        info: &str,
        envreqs: &mut EnvReqs,
        feedback: &mut ConfigFeedback,
        env: &mut TaskEnvHooks<'_>,
    ) -> Option<i32> {
        let mut reqs = avida_tasks::EnvReqs::new();
        reqs.set_min_inputs(envreqs.min_inputs);
        reqs.set_min_outputs(envreqs.min_outputs);
        reqs.set_true_rand_inputs(envreqs.true_rand_inputs);
        let mut fb = avida_tasks::Feedback::new();
        let id = self.0.add_task(name, info, &mut reqs, &mut fb);
        envreqs.min_inputs = reqs.get_min_inputs();
        envreqs.min_outputs = reqs.get_min_outputs();
        envreqs.true_rand_inputs = reqs.get_true_rand_inputs();
        for (kind, msg) in fb.messages {
            match kind {
                FeedbackKind::Error => feedback.error(msg),
                FeedbackKind::Warning => feedback.warning(msg),
                FeedbackKind::Notify => feedback.notify(msg),
            }
        }
        for g in self.0.take_group_ids() {
            env.add_group_id(g);
        }
        for t in self.0.take_target_ids() {
            env.add_target_id(t);
        }
        id.map(|i| i as i32)
    }

    fn num_tasks(&self) -> i32 {
        self.0.get_size() as i32
    }

    fn task_name(&self, id: i32) -> &str {
        self.0.get_task(id as usize).map(|t| t.name()).unwrap_or("")
    }

    fn task_desc(&self, id: i32) -> &str {
        self.0.get_task(id as usize).map(|t| t.desc()).unwrap_or("")
    }

    fn use_neighbor_input(&self) -> bool {
        self.0.use_neighbor_input()
    }

    fn use_neighbor_output(&self) -> bool {
        self.0.use_neighbor_output()
    }
}

/// What a population run lends `cPhenPlastUtil::TestPlasticity` beyond the
/// output test's own borrows, so that the world can run the genotype's
/// test CPUs from inside `cEnvironment::GetTaskProbability`: the parts of
/// [`crate::test_cpu::TestWorld`] the output test does not hold, read before the stats are
/// lent out. The world builds it only when a reaction process has a
/// `phenplastbonus` method and the genotype's summary is not cached yet.
pub struct PlasticityServices<'a> {
    /// `m_world->GetHardwareManager()`
    pub hardware_manager: &'a HardwareManager,
    /// `m_world->GetStats().GetGroupAttackInsts(name).GetSize()`
    pub group_attack_inst_count: &'a dyn Fn(&str) -> usize,
    /// `m_world->GetStats().GetUpdate()`
    pub update: i32,
    /// `m_world->GetStats().GetTotCreatures()`
    pub tot_creatures: i32,
    /// `m_world->GetStats().GetSenseSize()`
    pub sense_size: usize,
    /// See [`crate::test_cpu::TestWorld::counts`].
    pub counts: TestWorldCounts,
    /// See [`crate::test_cpu::TestWorld::live_orgs`]; the tested organism's forage target
    /// is `None` (it is in [`PlasticityTest::forage_target`]).
    pub live_orgs: Option<Vec<(OrgUid, i32, Option<i32>)>>,
    /// `m_world->GetTestSterilize()`
    pub test_sterilize: bool,
    /// `Genome(bg->Properties().Get("genome"))` of the organism's genotype.
    pub genome: Genome,
}

/// The borrows of one `cPhenPlastUtil::GetTaskProbability` call: the
/// environment under evaluation, the output test's RNG and statistics,
/// and the world's [`PlasticityServices`] (`None` outside a population
/// run, where the call is unreachable).
pub struct PlasticityTest<'a> {
    /// `m_world->GetEnvironment()`
    pub environment: &'a mut Env,
    /// `m_world->GetConfig()`
    pub config: &'a Config,
    /// `ctx.GetRandom()`
    pub rng: &'a mut AvidaRng,
    /// `m_world->GetStats()` as `cPhenotype` uses it.
    pub stats: &'a mut dyn PhenotypeStats,
    /// `GetForageTarget()` of the organism whose output is tested.
    pub forage_target: i32,
    /// `GetPhenotype().GetKaboomExecuted()` of the organism whose output is
    /// tested, at the moment of the call. The organism is out of its cell
    /// while it runs, and `Task_ConsumePublicGood` in the plasticity test
    /// reads (and may clear) the live organism's flag when its cell is among
    /// the cells the test organism (cell -1) visits; the host writes back
    /// the cleared flag.
    pub own_kaboom: &'a mut bool,
    /// See [`PlasticityServices`].
    pub services: Option<&'a PlasticityServices<'a>>,
}

/// Services `cEnvironment::TestOutput` reaches through `cTaskContext` and
/// the world that are not part of [`TaskWorld`]: the phenotypic-plasticity
/// cache and the HGT fragment pool.
pub trait OutputHost: TaskWorld {
    /// `cPhenPlastUtil::GetTaskProbability(ctx, world, genotype, task_id)`:
    /// the genotype's attached `cPhenPlastSummary`, made the first time by
    /// `TestPlasticity` (test CPUs over `test`'s borrows), at `task_id`.
    fn task_probability(&mut self, test: PlasticityTest<'_>, task_id: i32) -> f64;
    /// HGT metabolism, see [`TaskEvaluator::metabolize_genome_fragment`].
    fn metabolize_genome_fragment(&mut self, rng: &mut AvidaRng) -> Option<i32>;
}

/// The organism half of [`TaskWorld`] answered from a snapshot of
/// `cOrganism` state, the rest forwarded to the host.
pub struct OrgTaskWorld<'a> {
    pub host: &'a mut dyn OutputHost,
    pub org: &'a mut OrgTaskSnapshot,
    /// `ChangeResource` requested by `xor-max` (applied by the environment).
    pub resource_change: Option<(usize, i32)>,
}

impl TaskWorld for OrgTaskWorld<'_> {
    fn get_cell_id(&self) -> i32 {
        self.host.get_cell_id()
    }
    fn get_av_cell_id(&self) -> i32 {
        self.host.get_av_cell_id()
    }
    fn get_prev_seen_cell_id(&self) -> i32 {
        self.host.get_prev_seen_cell_id()
    }
    fn set_prev_seen_cell_id(&mut self, id: i32) {
        self.host.set_prev_seen_cell_id(id)
    }
    fn get_prev_task_cell_id(&self) -> i32 {
        self.host.get_prev_task_cell_id()
    }
    fn set_prev_task_cell_id(&mut self, id: i32) {
        self.host.set_prev_task_cell_id(id)
    }
    fn add_reached_task_cell(&mut self) {
        self.host.add_reached_task_cell()
    }
    fn get_cell_data(&self) -> i32 {
        self.host.get_cell_data()
    }
    fn get_forage_target(&self) -> i32 {
        self.org.forage_target
    }
    fn get_gradient_movement(&self) -> f64 {
        self.org.gradient_movement
    }
    fn has_opinion(&self) -> bool {
        self.org.opinion.is_some()
    }
    fn get_opinion_first(&self) -> i32 {
        self.org.opinion.map(|o| o.0).unwrap_or(0)
    }
    fn get_event_killed(&self) -> bool {
        self.org.killed_event
    }
    fn get_lyse_display(&self) -> bool {
        self.org.lyse_display
    }
    fn get_input_at(&self, i: i32) -> i32 {
        self.host.get_input_at(i)
    }
    fn get_kaboom_executed(&self) -> bool {
        self.org.kaboom_executed
    }
    fn get_kaboom_executed2(&self) -> bool {
        self.org.kaboom_executed2
    }
    fn get_number_strings_produced(&self, tag: i32) -> i32 {
        self.org
            .strings_produced
            .iter()
            .find(|(t, _)| *t == tag)
            .map(|(_, n)| *n)
            .unwrap_or(0)
    }
    fn get_number_strings_on_hand(&self, i: i32) -> i32 {
        self.org
            .strings_on_hand
            .iter()
            .find(|(t, _)| *t == i)
            .map(|(_, n)| *n)
            .unwrap_or(0)
    }
    fn update_tag(&mut self, tag: i32, bits: i32) {
        // `cOrganism::UpdateTag` draws from the world RNG, which the
        // environment holds during evaluation: recorded and applied by
        // `OrgState::apply_task_snapshot` after the test.
        self.org.pending_tags.push((tag, bits));
    }
    fn get_tag_label(&self) -> i32 {
        self.org.tag.0
    }
    fn set_lineage_label(&mut self, label: i32) {
        self.org.lineage_label = label;
    }
    fn get_reputation(&self) -> i32 {
        self.org.opinion.map(|o| o.0).unwrap_or(0)
    }
    fn set_reputation(&mut self, rep: i32) {
        // `SetOpinion(rep)`, applied after the test.
        self.org.pending_opinions.push(rep);
        self.org.opinion = Some((rep, self.org.update));
    }
    fn get_number_of_donations(&self) -> i32 {
        self.org.num_donations
    }
    fn state_grid_name(&self) -> String {
        self.host.state_grid_name()
    }
    fn state_grid_state_id(&self, name: &str) -> i32 {
        self.host.state_grid_state_id(name)
    }
    fn state_grid_num_states(&self) -> i32 {
        self.host.state_grid_num_states()
    }
    fn state_grid_width(&self) -> i32 {
        self.host.state_grid_width()
    }
    fn state_grid_height(&self) -> i32 {
        self.host.state_grid_height()
    }
    fn state_grid_state_at(&self, cell: i32) -> i32 {
        self.host.state_grid_state_at(cell)
    }
    fn deme_cell_position_x(&self, cell_id: i32) -> i32 {
        self.host.deme_cell_position_x(cell_id)
    }
    fn deme_get_relative_cell_id(&self, cell_id: i32) -> i32 {
        self.host.deme_get_relative_cell_id(cell_id)
    }
    fn deme_num_events(&self) -> i32 {
        self.host.deme_num_events()
    }
    fn deme_cell_event_id(&self, i: i32) -> i32 {
        self.host.deme_cell_event_id(i)
    }
    fn deme_num_movement_predicates(&self) -> i32 {
        self.host.deme_num_movement_predicates()
    }
    fn deme_mov_predicate_event(&self, predicate: i32, event: i32) -> Option<(i32, bool)> {
        self.host.deme_mov_predicate_event(predicate, event)
    }
    fn number_of_organisms_in_group(&mut self, group_id: i32) -> i32 {
        self.host.number_of_organisms_in_group(group_id)
    }
    fn world_x(&self) -> i32 {
        self.host.world_x()
    }
    fn world_y(&self) -> i32 {
        self.host.world_y()
    }
    fn use_avatars(&self) -> bool {
        self.host.use_avatars()
    }
    fn match_already_produced(&self) -> bool {
        self.host.match_already_produced()
    }
    fn pheromone_resource_amount(&self) -> f64 {
        self.host.pheromone_resource_amount()
    }
    fn get_resources(&self) -> Vec<f64> {
        self.host.get_resources()
    }
    fn get_av_resources(&self) -> Vec<f64> {
        self.host.get_av_resources()
    }
    fn change_reaction_resource(&mut self, task_id: usize, resource_index: i32) -> bool {
        // Deferred: the environment applies it through
        // `TaskEvaluator::take_resource_change` before running processes.
        self.resource_change = Some((task_id, resource_index));
        true
    }
    /// The organism's own cell holds the organism itself, which is out of
    /// the population while it runs: answered from the snapshot.
    fn cell_kaboom_executed(&self, cell_id: i32) -> Option<bool> {
        if cell_id >= 0 && cell_id == self.host.get_cell_id() {
            return Some(self.org.kaboom_executed);
        }
        self.host.cell_kaboom_executed(cell_id)
    }
    /// Its own flag is written back by `OrgState::apply_task_snapshot`.
    fn clear_kaboom_executed(&mut self, cell_id: i32) {
        if cell_id >= 0 && cell_id == self.host.get_cell_id() {
            self.org.kaboom_executed = false;
            return;
        }
        self.host.clear_kaboom_executed(cell_id)
    }
    fn std_rand(&mut self) -> i32 {
        self.host.std_rand()
    }
    fn task_stdout(&mut self, text: &str) {
        self.host.task_stdout(text);
    }
    fn unreproducible_producer_choice(&mut self, producers: usize) {
        self.host.unreproducible_producer_choice(producers)
    }
    fn stats_add_tag(&mut self, tag: i32, value: i32) {
        self.host.stats_add_tag(tag, value)
    }
    fn stats_add_string_bits_matched_value(&mut self, name: &str, value: i32) {
        self.host.stats_add_string_bits_matched_value(name, value)
    }
    fn stats_inc_perfect_match(&mut self, amount: i32) {
        self.host.stats_inc_perfect_match(amount)
    }
    fn stats_inc_perfect_match_org(&mut self) {
        self.host.stats_inc_perfect_match_org()
    }
}

/// `cTaskContext` for one organism's output test, as the environment sees
/// it ([`TaskEvaluator`]).
pub struct OrgTaskEvaluator<'a> {
    pub ctx: TaskContext<'a>,
    pub world: OrgTaskWorld<'a>,
    /// `GetPhenotype().GetStolenReactionCount()`
    pub stolen_reaction_count: &'a [i32],
    /// Reactions whose first-trigger cycle/exec must be recorded
    /// (`SetFirstReactionCycle` / `SetFirstReactionExec`), applied by the
    /// caller once the phenotype is no longer borrowed.
    pub first_reactions: Vec<i32>,
    /// Resource names, to translate an `xor-max` resource index.
    pub resource_names: Vec<String>,
    /// `m_world->GetConfig()`, for a plasticity test.
    pub config: &'a Config,
    /// `m_world->GetStats()`, lent to a plasticity test.
    pub stats: &'a mut dyn PhenotypeStats,
    /// See [`PlasticityServices`].
    pub plasticity: Option<PlasticityServices<'a>>,
}

impl TaskEvaluator<TaskLibRegistry> for OrgTaskEvaluator<'_> {
    fn setup_tests(&mut self, lib: &TaskLibRegistry) {
        lib.0.setup_tests(&mut self.ctx);
    }

    fn set_task_entry(&mut self, _lib: &TaskLibRegistry, task_id: i32) {
        self.ctx.set_task_entry(task_id as usize);
    }

    fn on_divide(&self) -> bool {
        self.ctx.on_divide
    }

    fn test_output(&mut self, lib: &TaskLibRegistry, _rng: &mut AvidaRng) -> f64 {
        lib.0.test_output(&mut self.ctx, &mut self.world)
    }

    fn task_value(&self) -> f64 {
        self.ctx.task_value
    }

    fn has_organism(&self) -> bool {
        true
    }

    fn cell_id(&self) -> i32 {
        self.world.host.get_cell_id()
    }

    fn stolen_reaction_count(&self, reaction_id: usize) -> i32 {
        self.stolen_reaction_count
            .get(reaction_id)
            .copied()
            .unwrap_or(0)
    }

    fn set_first_reaction(&mut self, reaction_id: i32) {
        self.first_reactions.push(reaction_id);
    }

    fn task_probability(&mut self, env: &mut Env, rng: &mut AvidaRng, task_id: i32) -> f64 {
        let mut own_kaboom = self.world.org.kaboom_executed;
        let test = PlasticityTest {
            environment: env,
            config: self.config,
            rng,
            stats: &mut *self.stats,
            forage_target: self.world.org.forage_target,
            own_kaboom: &mut own_kaboom,
            services: self.plasticity.as_ref(),
        };
        let probability = self.world.host.task_probability(test, task_id);
        // `ClearKaboomExecuted()` on the live organism by a test CPU's
        // `consume-public-good`.
        self.world.org.kaboom_executed = own_kaboom;
        probability
    }

    fn metabolize_genome_fragment(&mut self, rng: &mut AvidaRng) -> Option<i32> {
        self.world.host.metabolize_genome_fragment(rng)
    }

    fn take_resource_change(&mut self) -> Option<(i32, String)> {
        let (task_id, res_idx) = self.world.resource_change.take()?;
        let name = self.resource_names.get(res_idx as usize)?.clone();
        Some((task_id as i32, name))
    }
}
