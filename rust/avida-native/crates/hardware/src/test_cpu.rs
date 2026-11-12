//! Ports of `cTestCPU`, `cTestCPUInterface` and `cCPUTestInfo`
//! (avida-core/source/cpu/cTestCPU.{h,cc}, cTestCPUInterface.{h,cc},
//! cCPUTestInfo.{h,cc} at 47f13dadb).
//!
//! A test CPU runs a genome in isolation: no cell, no neighbours, no deme,
//! inputs and resources of its own. It still shares the world's
//! configuration, environment, RNG, hardware manager and statistics, which
//! the caller lends through [`TestWorld`] as disjoint borrows (the C++
//! reaches them through `m_world` and the `cAvidaContext`).
//!
//! The C++ `cAvidaContext::SetTestMode()` / `ClearTestMode()` pair becomes
//! [`WorldServices::test_mode`] answering `true` for every organism the test
//! CPU builds.

use crate::hardware::Hardware;
use crate::hardware_manager::HardwareManager;
use crate::nhardware::TEST_CPU_GENERATIONS;
use crate::org_interface::{
    Faced, OffspringTest, OrgInterface, OrgMessage, OutputServices, TestamentTarget, WorldServices,
};
use crate::organism::{transmission, OrgSource, OrgState, OrgUid, Organism};
use crate::task_adapter::{Env, OutputHost, PlasticityTest};
use crate::tracer::HardwareTracerPtr;
use avida_apto::fmt::fmt_fixed;
use avida_apto::{AvidaRng, DataFile};
use avida_config::config::Config;
use avida_environment::{MutationRates, ResourceHistory, StateGrid, TaskRegistry};
use avida_genome::{Genome, InstructionSequence};
use avida_phenotype::{Merit, Phenotype, PhenotypeStats};
use avida_tasks::TaskWorld;
use std::sync::Arc;

/// `eTestCPUResourceMethod`: `RES_INITIAL`.
pub const RES_INITIAL: i32 = 0;
/// `RES_CONSTANT`
pub const RES_CONSTANT: i32 = 1;
/// `RES_UPDATED_DEPLETABLE`
pub const RES_UPDATED_DEPLETABLE: i32 = 2;
/// `RES_DYNAMIC`
pub const RES_DYNAMIC: i32 = 3;
/// `RES_LAST`
pub const RES_LAST: i32 = 4;

// ----------------------------------------------------------------------
// World access
// ----------------------------------------------------------------------

/// The world-level services a test organism's task evaluation and
/// hardware reach beyond the test CPU itself: the population
/// (`NumberOfOrganismsInGroup`, cell kaboom flags), `cStats`, `std::rand`,
/// phenotypic plasticity and HGT. The world answers as for an organism with
/// no cell and no deme; the queries `cTestCPUInterface` answers itself never
/// reach it.
pub trait TestCpuHost: OutputHost {
    /// `m_world->GetStats().IncResamplings()`
    fn stats_inc_resamplings(&mut self);
    /// `m_world->GetStats().IncFailedResamplings()`
    fn stats_inc_failed_resamplings(&mut self);
    /// `m_world->GetStats().AddNewTaskCount(task)`
    fn stats_add_new_task_count(&mut self, task: i32);
    /// `m_world->GetStats().SumEnergyTestamentTo<target>().Add(amount)`:
    /// the relinquish-energy instructions add to the world's statistics
    /// from a test CPU too.
    fn stats_add_energy_testament(&mut self, _target: TestamentTarget, _amount: f64) {}
    /// `std::cout` (or the driver's `Feedback()`, which prints there) from
    /// a test CPU; a population run keeps it in order with its own output.
    fn cout(&mut self, text: &str) {
        print!("{text}");
    }
}

/// What a test CPU reads of the population through `cStats`: the live
/// predator and prey counts (`GetNumPreyCreatures`, `GetNumPredCreatures`,
/// `GetNumTotalPredCreatures`, which ask `cPopulation`) and
/// `SumCreatureAge()` (`cOrganism::GetVitality`). All zero for an
/// analyze-mode world, whose population is empty and whose statistics never
/// gather an age.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct TestWorldCounts {
    /// `cPopulation::GetNumPreyOrganisms()`
    pub num_prey: i32,
    /// `cPopulation::GetNumPredOrganisms()`
    pub num_pred: i32,
    /// `cPopulation::GetNumTopPredOrganisms()`
    pub num_top_pred: i32,
    /// `SumCreatureAge().Average()`
    pub creature_age_mean: f64,
    /// `SumCreatureAge().StdDeviation()`
    pub creature_age_stddev: f64,
}

/// What a test CPU borrows from the world for one test.
pub struct TestWorld<'a> {
    /// `m_world->GetConfig()`
    pub config: &'a Config,
    /// `m_world->GetEnvironment()`
    pub environment: &'a mut Env,
    /// `ctx.GetRandom()`: the RNG of the context the test runs in.
    pub rng: &'a mut AvidaRng,
    /// `m_world->GetHardwareManager()`
    pub hardware_manager: &'a HardwareManager,
    /// `m_world->GetStats()` as `cPhenotype` uses it.
    pub stats: &'a mut dyn PhenotypeStats,
    /// The remaining world-level services (see [`TestCpuHost`]).
    pub host: &'a mut dyn TestCpuHost,
    /// `m_world->GetStats().GetUpdate()`
    pub update: i32,
    /// `m_world->GetStats().GetTotCreatures()`
    pub tot_creatures: i32,
    /// `m_world->GetStats().GetSenseSize()`
    pub sense_size: usize,
    /// `m_world->GetStats().GetGroupAttackInsts(name).GetSize()`
    pub group_attack_inst_count: &'a dyn Fn(&str) -> usize,
    /// The population counts and creature ages of `m_world->GetStats()`.
    pub counts: TestWorldCounts,
    /// `m_world->GetPopulation().GetLiveOrgList()` as `(organism, cell,
    /// GetForageTarget())`, the forage target `None` for an organism whose
    /// state is out of the world's reach (the one whose step runs the
    /// test). `Inst_SetRandPFTOnce` walks it under `MAX_PREY_BT`; `None`
    /// when it was not gathered.
    pub live_orgs: Option<Vec<(OrgUid, i32, Option<i32>)>>,
    /// `ctx.GetAnalyzeMode()`
    pub analyze_mode: bool,
    /// `ctx.OrgFaultReporting()`
    pub org_fault_reporting: bool,
    /// `m_world->GetVerbosity()`
    pub verbosity: i32,
    /// `m_world->GetTestSterilize()`
    pub test_sterilize: bool,
}

// ----------------------------------------------------------------------
// cCPUTestInfo
// ----------------------------------------------------------------------

/// `cCPUTestInfo`: the options of a test and its results, including the
/// organisms of each tested generation.
pub struct CpuTestInfo {
    generation_tests: i32,
    trace_task_order: bool,
    use_random_inputs: bool,
    use_manual_inputs: bool,
    manual_inputs: Vec<i32>,
    tracer: Option<HardwareTracerPtr>,
    mut_rates: MutationRates,
    cur_sg: i32,

    is_viable: bool,
    max_depth: i32,
    depth_found: i32,
    max_cycle: i32,
    cycle_to: i32,
    used_inputs: Vec<i32>,

    org_array: Vec<Option<Organism>>,

    res_method: i32,
    res: Option<Arc<ResourceHistory>>,
    res_update: i32,
    res_cpu_cycle_offset: i32,
}

impl std::fmt::Debug for CpuTestInfo {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CpuTestInfo")
            .field("generation_tests", &self.generation_tests)
            .field("is_viable", &self.is_viable)
            .field("max_depth", &self.max_depth)
            .field("depth_found", &self.depth_found)
            .field("max_cycle", &self.max_cycle)
            .field("cycle_to", &self.cycle_to)
            .finish()
    }
}

impl Default for CpuTestInfo {
    fn default() -> Self {
        Self::new(TEST_CPU_GENERATIONS)
    }
}

impl CpuTestInfo {
    /// `cCPUTestInfo(max_tests)`.
    pub fn new(max_tests: i32) -> Self {
        let mut info = CpuTestInfo {
            generation_tests: max_tests,
            trace_task_order: false,
            use_random_inputs: false,
            use_manual_inputs: false,
            manual_inputs: Vec::new(),
            tracer: None,
            mut_rates: MutationRates::default(),
            cur_sg: 0,
            is_viable: false,
            max_depth: -1,
            depth_found: -1,
            max_cycle: 0,
            cycle_to: -1,
            used_inputs: Vec::new(),
            org_array: (0..max_tests.max(0)).map(|_| None).collect(),
            res_method: RES_INITIAL,
            res: None,
            res_update: 0,
            res_cpu_cycle_offset: 0,
        };
        info.clear();
        info
    }

    /// `Clear()`: reset the results and drop the tested organisms.
    pub fn clear(&mut self) {
        self.is_viable = false;
        self.max_depth = -1;
        self.depth_found = -1;
        self.max_cycle = 0;
        self.cycle_to = -1;
        for slot in self.org_array.iter_mut() {
            if slot.is_none() {
                break;
            }
            *slot = None;
        }
    }

    /// `TraceTaskOrder(trace)`
    pub fn set_trace_task_order(&mut self, trace: bool) {
        self.trace_task_order = trace;
    }
    /// `UseRandomInputs(rand)`
    pub fn use_random_inputs(&mut self, rand: bool) {
        self.use_random_inputs = rand;
        self.use_manual_inputs = false;
    }
    /// `UseManualInputs(inputs)`
    pub fn use_manual_inputs(&mut self, inputs: Vec<i32>) {
        self.use_manual_inputs = true;
        self.use_random_inputs = false;
        self.manual_inputs = inputs;
    }
    /// `ResetInputMode()`
    pub fn reset_input_mode(&mut self) {
        self.use_manual_inputs = false;
        self.use_random_inputs = false;
    }
    /// `SetTraceExecution(tracer)`
    pub fn set_trace_execution(&mut self, tracer: Option<HardwareTracerPtr>) {
        self.tracer = tracer;
    }
    /// `SetResourceOptions(res_method, res, update, cpu_cycle_offset)`
    pub fn set_resource_options(
        &mut self,
        res_method: i32,
        res: Option<Arc<ResourceHistory>>,
        update: i32,
        cpu_cycle_offset: i32,
    ) {
        self.res_method = res_method;
        self.res = res;
        self.res_update = update;
        self.res_cpu_cycle_offset = cpu_cycle_offset;
    }
    /// `SetCurrentStateGridID(sg)`
    pub fn set_current_state_grid_id(&mut self, sg: i32) {
        self.cur_sg = sg;
    }
    /// `MutationRates()`
    pub fn mutation_rates_mut(&mut self) -> &mut MutationRates {
        &mut self.mut_rates
    }

    /// `GetGenerationTests()`
    pub fn generation_tests(&self) -> i32 {
        self.generation_tests
    }
    /// `GetTraceTaskOrder()`
    pub fn trace_task_order(&self) -> bool {
        self.trace_task_order
    }
    /// `GetUseRandomInputs()`
    pub fn get_use_random_inputs(&self) -> bool {
        self.use_random_inputs
    }
    /// `GetUseManualInputs()`
    pub fn get_use_manual_inputs(&self) -> bool {
        self.use_manual_inputs
    }
    /// `GetTestCPUInputs()`: the inputs of the depth-0 organism.
    pub fn test_cpu_inputs(&self) -> &[i32] {
        &self.used_inputs
    }
    /// `GetTracer()`
    pub fn tracer(&self) -> Option<HardwareTracerPtr> {
        self.tracer.clone()
    }

    /// `IsViable()`
    pub fn is_viable(&self) -> bool {
        self.is_viable
    }
    /// `GetMaxDepth()`
    pub fn max_depth(&self) -> i32 {
        self.max_depth
    }
    /// `GetDepthFound()`
    pub fn depth_found(&self) -> i32 {
        self.depth_found
    }
    /// `GetMaxCycle()`
    pub fn max_cycle(&self) -> i32 {
        self.max_cycle
    }
    /// `GetCycleTo()`
    pub fn cycle_to(&self) -> i32 {
        self.cycle_to
    }
    /// `GetStateGridID()`
    pub fn state_grid_id(&self) -> i32 {
        self.cur_sg
    }

    /// `GetTestOrganism(level)` (`None` where the C++ asserts).
    pub fn test_organism(&self, level: i32) -> Option<&Organism> {
        usize::try_from(level)
            .ok()
            .and_then(|l| self.org_array.get(l))
            .and_then(|o| o.as_ref())
    }

    /// `GetTestPhenotype(level)` (`None` where the C++ asserts).
    pub fn test_phenotype(&self, level: i32) -> Option<&Phenotype> {
        self.test_organism(level).map(|o| &o.state.phenotype)
    }

    /// `GetColonyOrganism()`
    pub fn colony_organism(&self) -> Option<&Organism> {
        let depth_used = if self.depth_found == -1 {
            0
        } else {
            self.depth_found
        };
        self.test_organism(depth_used)
    }

    /// `GetGenotypeFitness()`
    pub fn genotype_fitness(&self) -> f64 {
        self.test_phenotype(0).map(|p| p.fitness).unwrap_or(0.0)
    }

    /// `GetColonyFitness()`
    pub fn colony_fitness(&self) -> f64 {
        if self.is_viable {
            self.colony_organism()
                .map(|o| o.state.phenotype.fitness)
                .unwrap_or(0.0)
        } else {
            0.0
        }
    }

    /// The results `Divide_TestFitnessMeasures` reads.
    pub fn offspring_test(&self) -> OffspringTest {
        OffspringTest {
            genotype_fitness: self.genotype_fitness(),
            max_depth: self.max_depth,
            is_viable: self.is_viable,
            last_task_count: self
                .test_phenotype(0)
                .map(|p| p.last_task_count.clone())
                .unwrap_or_default(),
        }
    }
}

// ----------------------------------------------------------------------
// cTestCPU: inputs and resources
// ----------------------------------------------------------------------

/// Where `m_res` points.
#[derive(Debug, Clone, Default)]
enum ResSource {
    /// `NULL`
    #[default]
    None,
    /// `&GetResourceLib().GetInitialResourceLevels()`
    Initial,
    /// A history handed in through [`CpuTestInfo::set_resource_options`].
    History(Arc<ResourceHistory>),
}

/// The inputs and resources of a `cTestCPU`.
#[derive(Debug, Clone, Default)]
struct TestCpuIo {
    input_array: Vec<i32>,
    receive_array: Vec<i32>,
    cur_input: i32,
    cur_receive: i32,
    use_random_inputs: bool,
    /// QUIRK: `m_use_manual_inputs` is set false in the constructor and
    /// never again, so `ResetInputs` regenerates environment inputs even
    /// when the test was given manual inputs.
    use_manual_inputs: bool,

    res_method: i32,
    res: ResSource,
    res_update: i32,
    res_cpu_cycle_offset: i32,

    /// `m_resource_count` (every resource global, so plain levels).
    resource_count: Vec<f64>,
    /// `m_faced_cell_resource_count`
    faced_cell_resource_count: Vec<f64>,
    /// `m_deme_resource_count` (size 0)
    deme_resource_count: Vec<f64>,
    /// `m_cell_resource_count`
    cell_resource_count: Vec<f64>,
}

/// `values[i]`; an out-of-range read (undefined in C++) gives 0.
fn at_or_zero(values: &[f64], i: i32) -> f64 {
    usize::try_from(i)
        .ok()
        .and_then(|i| values.get(i))
        .copied()
        .unwrap_or(0.0)
}

/// `array[pointer++]` after wrapping the pointer past the end to 0; an
/// empty array (undefined in C++) gives 0.
fn next_cyclic(array: &[i32], pointer: &mut i32) -> i32 {
    if *pointer >= array.len() as i32 {
        *pointer = 0;
    }
    let v = usize::try_from(*pointer)
        .ok()
        .and_then(|p| array.get(p))
        .copied()
        .unwrap_or(0);
    *pointer = pointer.wrapping_add(1);
    v
}

impl TestCpuIo {
    /// `GetInputAt(input_pointer)`
    fn input_at(&self, input_pointer: &mut i32) -> i32 {
        next_cyclic(&self.input_array, input_pointer)
    }

    /// `GetReceiveValue()`
    fn receive_value(&mut self) -> i32 {
        let mut p = self.cur_receive;
        let v = next_cyclic(&self.receive_array, &mut p);
        self.cur_receive = p;
        v
    }

    /// `SetResourceUpdate(ctx, update, round_to_closest)`.
    fn set_resource_update(&mut self, env: &Env, update: i32, round_to_closest: bool) {
        let n = self.resource_count.len();
        let counts = &mut self.resource_count;
        let mut set = |i: usize, v: f64| counts[i] = v;
        match &self.res {
            ResSource::None => return,
            ResSource::Initial => {
                env.resource_lib()
                    .initial_resource_levels()
                    .resource_count_for_update(update, n, !round_to_closest, &mut set);
            }
            ResSource::History(h) => {
                h.resource_count_for_update(update, n, !round_to_closest, &mut set);
            }
        }
        self.res_update = update;
    }

    /// `InitResources(ctx, res_method, res, update, cpu_cycle_offset)`.
    #[allow(clippy::too_many_arguments)]
    fn init_resources(
        &mut self,
        env: &Env,
        cfg: &Config,
        res_method: i32,
        res: Option<Arc<ResourceHistory>>,
        update: i32,
        cpu_cycle_offset: i32,
        solo: (i32, f64),
    ) -> Result<(), String> {
        self.deme_resource_count.clear();

        self.res_method = res_method;
        if !(0..RES_LAST).contains(&res_method) {
            self.res_method = RES_INITIAL;
        }

        self.res = match res {
            Some(h) => ResSource::History(h),
            None => ResSource::None,
        };
        self.res_cpu_cycle_offset = cpu_cycle_offset;
        self.res_update = update;

        let ave_time_slice = cfg.ave_time_slice;
        if ave_time_slice == 0 {
            return Err(
                "AVE_TIME_SLICE=0 in a test CPU (C++: cTestCPU.cc:cTestCPU::InitResources divides by zero)"
                    .to_string(),
            );
        }
        self.res_update = self
            .res_update
            .wrapping_add(self.res_cpu_cycle_offset.wrapping_div(ave_time_slice));
        self.res_cpu_cycle_offset = self.res_cpu_cycle_offset.wrapping_rem(ave_time_slice);

        if self.res_method == RES_INITIAL {
            self.res = ResSource::Initial;
        }

        let n = env.resource_lib().size().max(0) as usize;
        self.resource_count = vec![0.0; n];
        self.faced_cell_resource_count = vec![0.0; n];
        self.cell_resource_count = vec![0.0; n];

        let (solo_res, solo_lev) = solo;
        if solo_res == -1 {
            let u = self.res_update;
            self.set_resource_update(env, u, false);
        } else if let Some(v) = usize::try_from(solo_res)
            .ok()
            .and_then(|i| self.resource_count.get_mut(i))
        {
            *v = solo_lev;
        }
        Ok(())
    }

    /// `UpdateResources(ctx, cpu_cycles_used)` (also
    /// `UpdateRandomResources`).
    fn update_resources(&mut self, env: &Env, cfg: &Config, cpu_cycles_used: i32) {
        let ave_time_slice = cfg.ave_time_slice;
        if self.res_method >= RES_UPDATED_DEPLETABLE
            && ave_time_slice != 0
            && cpu_cycles_used.wrapping_rem(ave_time_slice) == 0
        {
            let u = self.res_update.wrapping_add(1);
            self.set_resource_update(env, u, true);
        }
    }

    /// `ModifyResources(ctx, res_change)`: only the depletable methods let
    /// the organism change the test CPU's resources.
    fn modify_resources(&mut self, res_change: &[f64]) {
        if self.res_method >= RES_UPDATED_DEPLETABLE {
            for (r, c) in self.resource_count.iter_mut().zip(res_change) {
                *r += *c;
            }
        }
    }
}

// ----------------------------------------------------------------------
// cTestCPUInterface
// ----------------------------------------------------------------------

/// The `cTaskContext` world side of a test organism: the
/// `cTestCPUInterface` answers, then the world's.
struct TestHost<'a> {
    inner: &'a mut dyn TestCpuHost,
    io: &'a mut TestCpuIo,
    config: &'a Config,
    /// `m_world->GetEnvironment().GetStateGrid(m_cur_sg)`, copied because
    /// the environment is lent to the task evaluator.
    state_grid: Option<StateGrid>,
    /// Index of the resource named `pheromone`, if any.
    pheromone_id: Option<usize>,
}

impl TaskWorld for TestHost<'_> {
    fn get_cell_id(&self) -> i32 {
        -1
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

    // The organism fields below are answered by `OrgTaskWorld` from its
    // snapshot and never reach the host.
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
    fn get_input_at(&self, i: i32) -> i32 {
        // cOrganism::GetInputAt(int i): the interface advances a copy.
        let mut p = i;
        self.io.input_at(&mut p)
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
        self.state_grid
            .as_ref()
            .map(|s| s.name().to_string())
            .unwrap_or_default()
    }
    fn state_grid_state_id(&self, name: &str) -> i32 {
        self.state_grid
            .as_ref()
            .map(|s| s.state_id(name))
            .unwrap_or(-1)
    }
    fn state_grid_num_states(&self) -> i32 {
        self.state_grid
            .as_ref()
            .map(|s| s.num_states())
            .unwrap_or(0)
    }
    fn state_grid_width(&self) -> i32 {
        self.state_grid.as_ref().map(|s| s.width()).unwrap_or(0)
    }
    fn state_grid_height(&self) -> i32 {
        self.state_grid.as_ref().map(|s| s.height()).unwrap_or(0)
    }
    fn state_grid_state_at(&self, cell: i32) -> i32 {
        self.state_grid
            .as_ref()
            .map(|s| s.state_at_id(cell))
            .unwrap_or(0)
    }

    // `GetDeme()` is NULL for a test organism; the C++ deme tasks would
    // dereference it. The port answers as for an empty deme.
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

    fn number_of_organisms_in_group(&mut self, group_id: i32) -> i32 {
        self.inner.number_of_organisms_in_group(group_id)
    }
    fn world_x(&self) -> i32 {
        self.config.world_x
    }
    fn world_y(&self) -> i32 {
        self.config.world_y
    }
    fn use_avatars(&self) -> bool {
        self.config.use_avatars != 0
    }
    fn match_already_produced(&self) -> bool {
        self.config.match_already_produced != 0
    }
    /// The C++ checks the name in the population's resource count and reads
    /// the level from the test CPU; the population's names follow the
    /// resource library, so the library index is used for both.
    fn pheromone_resource_amount(&self) -> f64 {
        match self.pheromone_id {
            Some(id) => at_or_zero(&self.io.resource_count, id as i32),
            None => 0.0,
        }
    }
    fn get_resources(&self) -> Vec<f64> {
        self.io.resource_count.clone()
    }
    fn get_av_resources(&self) -> Vec<f64> {
        self.io.cell_resource_count.clone()
    }
    fn change_reaction_resource(&mut self, _task_id: usize, _resource_index: i32) -> bool {
        // Deferred by `OrgTaskWorld`; never reaches the host.
        false
    }
    fn cell_kaboom_executed(&self, cell_id: i32) -> Option<bool> {
        self.inner.cell_kaboom_executed(cell_id)
    }
    fn clear_kaboom_executed(&mut self, cell_id: i32) {
        self.inner.clear_kaboom_executed(cell_id)
    }
    fn std_rand(&mut self) -> i32 {
        self.inner.std_rand()
    }
    fn unreproducible_producer_choice(&mut self, producers: usize) {
        self.inner.unreproducible_producer_choice(producers)
    }
    fn stats_add_tag(&mut self, tag: i32, value: i32) {
        self.inner.stats_add_tag(tag, value)
    }
    fn stats_add_string_bits_matched_value(&mut self, name: &str, value: i32) {
        self.inner.stats_add_string_bits_matched_value(name, value)
    }
    fn stats_inc_perfect_match(&mut self, amount: i32) {
        self.inner.stats_inc_perfect_match(amount)
    }
    fn stats_inc_perfect_match_org(&mut self) {
        self.inner.stats_inc_perfect_match_org()
    }
}

impl OutputHost for TestHost<'_> {
    /// Unreachable: a test CPU's context is in test mode, where
    /// `cEnvironment::GetTaskProbability` returns -1 first.
    fn task_probability(&mut self, test: PlasticityTest<'_>, task_id: i32) -> f64 {
        self.inner.task_probability(test, task_id)
    }
    fn metabolize_genome_fragment(&mut self, rng: &mut AvidaRng) -> Option<i32> {
        self.inner.metabolize_genome_fragment(rng)
    }
}

/// `cTestCPUInterface`: the [`OrgInterface`] of a test organism.
pub struct TestCpuInterface<'a> {
    config: &'a Config,
    environment: &'a mut Env,
    rng: &'a mut AvidaRng,
    hardware_manager: &'a HardwareManager,
    stats: &'a mut dyn PhenotypeStats,
    host: TestHost<'a>,
    update: i32,
    tot_creatures: i32,
    sense_size: usize,
    group_attack_inst_count: &'a dyn Fn(&str) -> usize,
    counts: TestWorldCounts,
    live_orgs: Option<&'a [(OrgUid, i32, Option<i32>)]>,
    analyze_mode: bool,
    org_fault_reporting: bool,
    verbosity: i32,
    test_sterilize: bool,
    /// `m_test_info.GetStateGridID()`
    cur_sg: i32,
}

impl<'a> TestCpuInterface<'a> {
    fn new(w: &'a mut TestWorld<'_>, io: &'a mut TestCpuIo, cur_sg: i32) -> Self {
        let state_grid = usize::try_from(cur_sg)
            .ok()
            .filter(|&sg| (sg as i32) < w.environment.num_state_grids())
            .map(|sg| w.environment.state_grid(sg).clone());
        let pheromone_id = w.environment.resource_lib().find("pheromone");
        TestCpuInterface {
            config: w.config,
            environment: &mut *w.environment,
            rng: &mut *w.rng,
            hardware_manager: w.hardware_manager,
            stats: &mut *w.stats,
            host: TestHost {
                inner: &mut *w.host,
                io,
                config: w.config,
                state_grid,
                pheromone_id,
            },
            update: w.update,
            tot_creatures: w.tot_creatures,
            sense_size: w.sense_size,
            group_attack_inst_count: w.group_attack_inst_count,
            counts: w.counts,
            live_orgs: w.live_orgs.as_deref(),
            analyze_mode: w.analyze_mode,
            org_fault_reporting: w.org_fault_reporting,
            verbosity: w.verbosity,
            test_sterilize: w.test_sterilize,
            cur_sg,
        }
    }

    fn io(&self) -> &TestCpuIo {
        self.host.io
    }

    fn io_mut(&mut self) -> &mut TestCpuIo {
        self.host.io
    }
}

impl WorldServices for TestCpuInterface<'_> {
    fn config(&self) -> &Config {
        self.config
    }
    fn environment(&self) -> &Env {
        self.environment
    }
    fn rng(&mut self) -> &mut AvidaRng {
        self.rng
    }
    fn hardware_manager(&self) -> &HardwareManager {
        self.hardware_manager
    }
    fn update(&self) -> i32 {
        self.update
    }
    fn tot_creatures(&self) -> i32 {
        self.tot_creatures
    }
    fn sense_size(&self) -> usize {
        self.sense_size
    }
    fn group_attack_inst_count(&self, inst_set: &str) -> usize {
        (self.group_attack_inst_count)(inst_set)
    }
    /// `ctx.SetTestMode()` for the duration of `TestGenome`.
    fn test_mode(&self) -> bool {
        true
    }
    fn analyze_mode(&self) -> bool {
        self.analyze_mode
    }
    fn org_fault_reporting(&self) -> bool {
        self.org_fault_reporting
    }
    fn verbosity(&self) -> i32 {
        self.verbosity
    }
}

impl OrgInterface for TestCpuInterface<'_> {
    fn output_services(&mut self) -> OutputServices<'_> {
        OutputServices {
            config: self.config,
            environment: &mut *self.environment,
            rng: &mut *self.rng,
            test_mode: true,
            update: self.update,
            stats: &mut *self.stats,
            deme: None,
            host: &mut self.host,
            plasticity: None,
        }
    }
    fn test_sterilize(&self) -> bool {
        self.test_sterilize
    }
    /// Unreachable: `TestOnDivide()` is false, so the divide fitness test
    /// never runs inside a test CPU.
    fn genotype_test_fitness(&mut self, _org: &OrgState) -> f64 {
        0.0
    }
    /// A test CPU organism has no systematics groups.
    fn has_genotype(&mut self) -> bool {
        false
    }
    /// Unreachable: [`Self::has_genotype`] is false.
    fn phylo_distance_to(&mut self, _cell: i32) -> Result<i32, String> {
        Err("phylo distance in the test CPU (C++: organism without a genotype)".to_string())
    }
    /// Unreachable for the same reason as [`Self::genotype_test_fitness`].
    fn test_offspring(&mut self, _parent: &OrgState, _genome: &Genome) -> OffspringTest {
        OffspringTest::default()
    }
    fn stats_inc_resamplings(&mut self) {
        self.host.inner.stats_inc_resamplings();
    }
    fn stats_inc_failed_resamplings(&mut self) {
        self.host.inner.stats_inc_failed_resamplings();
    }
    fn stats_add_new_task_count(&mut self, task: i32) {
        self.host.inner.stats_add_new_task_count(task);
    }
    fn deme_inc_num_switching_penalties(&mut self, _n: i32) {}
    fn deme_dec_sleeping_count(&mut self) {}
    fn deme_inc_sleeping_count(&mut self) {}
    fn with_organism(&mut self, _uid: OrgUid, _f: &mut dyn FnMut(&mut Organism)) -> bool {
        false
    }
    fn with_organism_in_cell(&mut self, _cell_id: i32, _f: &mut dyn FnMut(&mut Organism)) -> bool {
        false
    }

    fn live_org_ids(&self) -> Vec<i32> {
        Vec::new()
    }
    /// `m_world->GetPopulation().GetLiveOrgList()`: the population of the
    /// world the test runs in (empty in analyze mode), as the world gathered
    /// it for the test. The organism whose step runs the test is out of
    /// reach; so is the population when nothing was gathered.
    fn live_org_forage_targets(&self) -> Result<Vec<(OrgUid, i32, Option<i32>)>, String> {
        match self.live_orgs {
            Some(orgs) if orgs.iter().all(|o| o.2.is_some()) => Ok(orgs.to_vec()),
            _ => Err(
                "unsupported: the population's live organisms from a test CPU run by a \
                      living organism (C++: cHardwareExperimental.cc:cHardwareExperimental::\
                      Inst_SetRandPFTOnce walks m_world->GetPopulation().GetLiveOrgList())"
                    .to_string(),
            ),
        }
    }
    fn find_live_org(&self, _id: i32) -> Option<OrgUid> {
        None
    }
    fn get_cell_id(&self) -> i32 {
        -1
    }
    fn get_deme_id(&self) -> i32 {
        -1
    }
    fn set_cell_id(&mut self, _id: i32) {}
    fn set_deme_id(&mut self, _id: i32) {}
    fn get_cell_x_position(&self) -> i32 {
        -1
    }
    fn get_cell_y_position(&self) -> i32 {
        -1
    }

    fn get_cell_data(&self) -> i32 {
        -1
    }
    fn get_cell_data_org_id(&self) -> i32 {
        -1
    }
    fn get_cell_data_update(&self) -> i32 {
        -1
    }
    fn get_cell_data_territory(&self) -> i32 {
        -1
    }
    fn get_cell_data_forager_type(&self) -> i32 {
        -99
    }
    fn set_cell_data(&mut self, _org: &mut OrgState, _data: i32) {}
    fn get_faced_cell_data(&self) -> i32 {
        -1
    }
    fn get_faced_cell_data_org_id(&self) -> i32 {
        -1
    }
    fn get_faced_cell_data_update(&self) -> i32 {
        -1
    }
    fn get_faced_cell_data_territory(&self) -> i32 {
        -1
    }

    fn get_prev_seen_cell_id(&self) -> i32 {
        0
    }
    fn get_prev_task_cell_id(&self) -> i32 {
        0
    }
    fn get_num_task_cells_reached(&self) -> i32 {
        0
    }
    fn add_reached_task_cell(&mut self) {}
    fn set_prev_seen_cell_id(&mut self, _id: i32) {}
    fn set_prev_task_cell_id(&mut self, _id: i32) {}

    /// `cTestCPUInterface::Divide`: `TestDivideReset` on the parent; the
    /// offspring is not built.
    fn divide(&mut self, parent: &mut OrgState, _offspring_genome: &Genome) -> bool {
        let size = parent.initial_genome.representation().size();
        parent
            .phenotype
            .test_divide_reset(self.config, size, self.update);
        true
    }
    /// The test CPU keeps no trace queues.
    fn divide_with_traces(
        &mut self,
        parent: &mut OrgState,
        _parent_hw: &dyn Hardware,
        offspring_genome: &Genome,
        _traces: &mut crate::mini_trace::LentTraces,
    ) -> bool {
        self.divide(parent, offspring_genome)
    }
    fn with_neighbor(&mut self, _f: &mut dyn FnMut(&mut Organism)) -> Faced {
        Faced::Empty
    }
    fn is_neighbor_cell_occupied(&self) -> bool {
        false
    }
    fn get_num_neighbors(&self) -> i32 {
        0
    }
    fn get_neighborhood_cell_ids(&self) -> Vec<i32> {
        Vec::new()
    }
    fn get_av_neighborhood_cell_ids(&self, _av_num: i32) -> Vec<i32> {
        Vec::new()
    }
    fn get_facing(&self) -> i32 {
        0
    }
    fn get_faced_cell_id(&self) -> i32 {
        -1
    }
    fn get_faced_dir(&self) -> i32 {
        0
    }
    fn get_neighbor_cell_contents(&self) -> i32 {
        0
    }
    fn rotate(&mut self, _direction: i32) {}
    fn get_lgt_fragment(
        &mut self,
        _region: i32,
        _dest_genome: &Genome,
    ) -> Option<InstructionSequence> {
        None
    }

    fn get_input_at(&mut self, input_pointer: &mut i32) -> i32 {
        self.io().input_at(input_pointer)
    }
    /// `cTestCPU::ResetInputs` (see the QUIRK on manual inputs).
    fn reset_inputs(&mut self) {
        if !self.host.io.use_manual_inputs {
            let random = self.host.io.use_random_inputs;
            self.environment
                .setup_inputs(self.rng, &mut self.host.io.input_array, random);
        }
    }
    fn get_inputs(&self) -> Vec<i32> {
        self.io().input_array.clone()
    }
    fn get_resources(&mut self, _org: &mut OrgState) -> Vec<f64> {
        self.io().resource_count.clone()
    }
    fn get_resource_val(&mut self, _org: &mut OrgState, res_id: i32) -> f64 {
        at_or_zero(&self.io().cell_resource_count, res_id)
    }
    fn get_faced_cell_resources(&mut self, _org: &mut OrgState) -> Vec<f64> {
        self.io().faced_cell_resource_count.clone()
    }
    fn get_faced_resource_val(&mut self, _org: &mut OrgState, res_id: i32) -> f64 {
        at_or_zero(&self.io().faced_cell_resource_count, res_id)
    }
    fn get_deme_resources(&mut self, _deme_id: i32) -> Vec<f64> {
        self.io().deme_resource_count.clone()
    }
    fn get_cell_resources(&mut self, _org: &mut OrgState, _cell_id: i32) -> Vec<f64> {
        self.io().cell_resource_count.clone()
    }
    fn get_frozen_cell_res_val(&mut self, _cell_id: i32, res_id: i32) -> f64 {
        at_or_zero(&self.io().cell_resource_count, res_id)
    }
    fn get_cell_res_val(&mut self, _org: &mut OrgState, _cell_id: i32, res_id: i32) -> f64 {
        at_or_zero(&self.io().cell_resource_count, res_id)
    }
    fn get_frozen_resources(&mut self, _cell_id: i32) -> Vec<f64> {
        self.io().cell_resource_count.clone()
    }
    /// `m_resource_count.GetCellIdLists()`: one empty list per resource.
    fn get_cell_id_lists(&self) -> Vec<Vec<i32>> {
        vec![Vec::new(); self.io().resource_count.len()]
    }
    fn get_curr_peak_x(&mut self, _org: &mut OrgState, _res_id: i32) -> i32 {
        0
    }
    fn get_curr_peak_y(&mut self, _org: &mut OrgState, _res_id: i32) -> i32 {
        0
    }
    fn get_frozen_peak_x(&mut self, _res_id: i32) -> i32 {
        0
    }
    fn get_frozen_peak_y(&mut self, _res_id: i32) -> i32 {
        0
    }
    fn trigger_do_updates(&mut self, _org: &mut OrgState) {}
    fn update_resources(&mut self, _org: &mut OrgState, res_change: &[f64]) {
        self.io_mut().modify_resources(res_change);
    }
    fn update_random_resources(&mut self, res_change: &[f64]) {
        self.io_mut().modify_resources(res_change);
    }
    fn update_deme_resources(&mut self, _res_change: &[f64]) {}

    fn die(&mut self, _org: &mut OrgState) {}
    fn kill_cell_id(&mut self, _org: &mut OrgState, _target: i32) {}
    fn kaboom(&mut self, _org: &mut OrgState, _distance: i32) {}
    fn kaboom_effect(&mut self, _org: &mut OrgState, _distance: i32, _effect: f64) {}
    fn spawn_deme(&mut self, _org: &mut OrgState) {}
    fn point_mutate_deme_member(
        &mut self,
        _org: &mut OrgState,
        _hw: &mut dyn crate::hardware::Hardware,
        _deme_id: i32,
        _prob: f64,
        _germline: Option<bool>,
    ) {
    }
    fn cout(&mut self, text: &str) {
        self.host.inner.cout(text);
    }
    fn receive_value(&mut self, _org: &mut OrgState) -> i32 {
        self.io_mut().receive_value()
    }
    fn inject_parasite(
        &mut self,
        _host: &mut OrgState,
        _host_hw: &mut dyn crate::hardware::Hardware,
        _parent: Option<Arc<crate::hardware_transsmt::ParasiteUnit>>,
        _label: &str,
        _injected_code: &InstructionSequence,
    ) -> bool {
        false
    }
    /// `m_test_info.GetTestPhenotype(m_cur_depth).SetMerit(...)`: the
    /// phenotype of the organism under test.
    fn update_merit(&mut self, org: &mut OrgState, new_merit: f64) -> bool {
        org.phenotype.merit = Merit::new(new_merit);
        true
    }
    fn test_on_divide(&self) -> bool {
        false
    }
    fn send_message(&mut self, _msg: &mut OrgMessage) -> bool {
        false
    }
    fn broadcast_message(&mut self, _msg: &mut OrgMessage, _depth: i32) -> bool {
        false
    }
    fn bcast_alarm(&mut self, _jump_label: i32, _bcast_range: i32) -> bool {
        false
    }
    fn divide_org_testament_among_deme(&mut self, _value: f64) {}
    fn send_flash(&mut self) {}
    fn get_state_grid_id(&mut self) -> i32 {
        self.cur_sg
    }
    fn rotate_to_greatest_reputation(&mut self, _org: &mut OrgState) {}
    fn rotate_to_greatest_reputation_with_different_tag(&mut self, _org: &mut OrgState, _tag: i32) {
    }
    fn rotate_to_greatest_reputation_with_different_lineage(
        &mut self,
        _org: &mut OrgState,
        _line: i32,
    ) {
    }

    fn create_link_by_facing(&mut self, _weight: f64) {}
    fn create_link_by_xy(&mut self, _x: i32, _y: i32, _weight: f64) {}
    fn create_link_by_index(&mut self, _idx: i32, _weight: f64) {}
    fn network_broadcast(&mut self, _msg: &mut OrgMessage) -> bool {
        false
    }
    fn network_unicast(&mut self, _msg: &mut OrgMessage) -> bool {
        false
    }
    fn network_rotate(&mut self, _x: i32) -> bool {
        false
    }
    fn network_select(&mut self, _x: i32) -> bool {
        false
    }

    fn get_next_deme_input(&mut self) -> i32 {
        -1
    }
    fn do_deme_input(&mut self, _value: i32) {}
    fn do_deme_output(&mut self, _value: i32) {}

    fn do_hgt_donation(&mut self, _org: &mut OrgState) {}
    fn do_hgt_conjugation(&mut self, _org: &mut OrgState) {}
    fn do_hgt_mutation(&mut self, _org: &mut OrgState) {}
    fn receive_hgt_donation(&mut self, _fragment: &InstructionSequence) {}

    fn move_org(&mut self, _org: &mut OrgState, _src_id: i32, _dest_id: i32) -> bool {
        false
    }
    fn add_live_org(&mut self) {}
    fn remove_live_org(&mut self) {}

    fn has_opinion(&self, _org: &OrgState) -> bool {
        false
    }
    fn set_opinion(&mut self, _opinion: i32, _org: &mut OrgState) {}
    fn clear_opinion(&mut self, _org: &mut OrgState) {}

    fn join_group(&mut self, _org: &mut OrgState, _group_id: i32) {}
    fn make_group(&mut self, _org: &mut OrgState) {}
    fn leave_group(&mut self, _org: &mut OrgState, _group_id: i32) {}
    fn number_of_organisms_in_group(&mut self, _group_id: i32) -> i32 {
        0
    }
    fn number_group_females(&mut self, _group_id: i32) -> i32 {
        0
    }
    fn number_group_males(&mut self, _group_id: i32) -> i32 {
        0
    }
    fn number_group_juvs(&mut self, _group_id: i32) -> i32 {
        0
    }
    fn change_group_mating_types(
        &mut self,
        _org: &mut OrgState,
        _group_id: i32,
        _old_type: i32,
        _new_type: i32,
    ) {
    }

    fn inc_tolerance(&mut self, _org: &mut OrgState, _tolerance_type: i32) -> i32 {
        0
    }
    fn dec_tolerance(&mut self, _org: &mut OrgState, _tolerance_type: i32) -> i32 {
        0
    }
    fn calc_group_tolerance_immigrants(
        &mut self,
        _org: &mut OrgState,
        _target_group_id: i32,
        _mating_type: i32,
    ) -> i32 {
        0
    }
    fn calc_group_tolerance_offspring(&mut self, _parent: &mut OrgState) -> i32 {
        0
    }
    fn calc_group_odds_immigrants(
        &mut self,
        _org: &mut OrgState,
        _group_id: i32,
        _mating_type: i32,
    ) -> f64 {
        0.0
    }
    fn calc_group_odds_offspring_parent(&mut self, _parent: &mut OrgState) -> f64 {
        0.0
    }
    fn calc_group_odds_offspring(&mut self, _org: &mut OrgState, _group_id: i32) -> f64 {
        0.0
    }
    fn attempt_immigrate_group(&mut self, _group_id: i32, _org: &mut OrgState) -> bool {
        false
    }
    fn push_tolerance_inst_exe(&mut self, _org: &mut OrgState, _tol_inst: i32) {}
    /// The C++ returns a fresh (leaked) `int` holding 0.
    fn get_group_intolerances(&mut self, _group_id: i32, _tol_num: i32, _mating_type: i32) -> i32 {
        0
    }
    /// Writes to that leaked `int`: no effect.
    fn set_group_intolerances(
        &mut self,
        _group_id: i32,
        _tol_num: i32,
        _mating_type: i32,
        _value: i32,
    ) {
    }

    fn try_write_group_attack_bits(&mut self, _raw_bits: u8) {}
    fn try_write_group_attack_string(&mut self, _s: &str) {}
    fn dec_num_prey_organisms(&mut self) {}
    fn dec_num_pred_organisms(&mut self) {}
    fn dec_num_top_pred_organisms(&mut self) {}
    fn inc_num_prey_organisms(&mut self) {}
    fn inc_num_pred_organisms(&mut self) {}
    fn inc_num_top_pred_organisms(&mut self) {}
    fn attack_faced_org(&mut self, _org: &mut OrgState, _loser: i32) {}
    fn try_write_birth_loc_data(&mut self, _org: &OrgState) {}
    fn inject_prey_clone(&mut self, _org: &mut OrgState) {}
    fn kill_rand_pred(&mut self, _org: &mut OrgState) {}
    fn kill_rand_prey(&mut self, _org: &mut OrgState) {}
    fn try_write_look_data(&mut self, _s: &str) {}
    fn try_write_look_output(&mut self, _s: &str) {}
    fn try_write_look_ex_output(&mut self, _s: &str) {}
    fn get_formed_group_array(&mut self) -> Vec<i32> {
        Vec::new()
    }

    fn has_output_av(&self, _av_num: i32) -> bool {
        false
    }
    fn faced_has_output_av(&self, _av_num: i32) -> bool {
        false
    }
    fn faced_has_av(&self, _av_num: i32) -> bool {
        false
    }
    fn faced_has_pred_av(&self, _av_num: i32) -> bool {
        false
    }
    fn faced_has_prey_av(&self, _av_num: i32) -> bool {
        false
    }
    fn add_io_av(&mut self, _av_cell_id: i32, _av_facing: i32, _input: bool, _output: bool) {}
    fn add_pred_prey_av(&mut self, _org: &OrgState, _av_cell_id: i32) {}
    fn switch_pred_prey(&mut self, _av_num: i32) {}
    fn remove_all_av(&mut self) {}
    fn get_av_facing(&self, _av_num: i32) -> i32 {
        0
    }
    fn get_av_cell_id(&self, _av_num: i32) -> i32 {
        -1
    }
    fn get_av_faced_cell_id(&self, _av_num: i32) -> i32 {
        -1
    }
    fn get_av_num_neighbors(&self, _av_num: i32) -> i32 {
        0
    }
    fn get_av_faced_data(&self, _av_num: i32) -> i32 {
        -1
    }
    fn get_av_faced_data_org_id(&self, _av_num: i32) -> i32 {
        -1
    }
    fn get_av_faced_data_update(&self, _av_num: i32) -> i32 {
        -1
    }
    fn get_av_faced_data_territory(&self, _av_num: i32) -> i32 {
        -1
    }
    fn get_av_data(&self, _av_num: i32) -> i32 {
        -1
    }
    fn get_av_data_org_id(&self, _av_num: i32) -> i32 {
        -1
    }
    fn get_av_data_update(&self, _av_num: i32) -> i32 {
        -1
    }
    fn get_av_data_territory(&self, _av_num: i32) -> i32 {
        -1
    }
    fn find_av(&self, _input: bool, _output: bool, _av_num: i32) -> i32 {
        -1
    }
    fn set_av_facing(&mut self, _av_facing: i32, _av_num: i32) {}
    fn set_av_cell_id(&mut self, _av_cell_id: i32, _av_num: i32) -> bool {
        false
    }
    fn set_av_faced_cell_id(&mut self, _av_num: i32) {}
    fn set_av_cell_data(
        &mut self,
        _org: &mut OrgState,
        _new_data: i32,
        _org_id: i32,
        _av_num: i32,
    ) {
    }
    fn move_av(&mut self, _org: &mut OrgState, _av_num: i32) -> bool {
        false
    }
    fn rotate_av(&mut self, _increment: i32, _av_num: i32) -> bool {
        false
    }
    fn get_rand_faced_av(&mut self, _av_num: i32) -> Option<OrgUid> {
        None
    }
    fn get_rand_faced_pred_av(&mut self, _av_num: i32) -> Option<OrgUid> {
        None
    }
    fn get_rand_faced_prey_av(&mut self, _av_num: i32) -> Option<OrgUid> {
        None
    }
    fn get_faced_avs(&mut self, _av_num: i32) -> Vec<OrgUid> {
        Vec::new()
    }
    fn get_cell_avs(&mut self, _av_cell_id: i32, _av_num: i32) -> Vec<OrgUid> {
        Vec::new()
    }
    fn get_faced_prey_avs(&mut self, _av_num: i32) -> Vec<OrgUid> {
        Vec::new()
    }
    fn get_av_resources(&mut self, _org: &mut OrgState, _av_num: i32) -> Vec<f64> {
        self.io().cell_resource_count.clone()
    }
    fn get_av_resource_val(&mut self, _org: &mut OrgState, res_id: i32, _av_num: i32) -> f64 {
        at_or_zero(&self.io().cell_resource_count, res_id)
    }
    fn get_av_faced_resources(&mut self, _org: &mut OrgState, _av_num: i32) -> Vec<f64> {
        self.io().faced_cell_resource_count.clone()
    }
    fn get_av_faced_resource_val(&mut self, _org: &mut OrgState, res_id: i32, _av_num: i32) -> f64 {
        at_or_zero(&self.io().faced_cell_resource_count, res_id)
    }
    fn update_av_resources(&mut self, _org: &mut OrgState, res_change: &[f64], _av_num: i32) {
        self.io_mut().modify_resources(res_change);
    }

    // A test CPU has no neighbours, so the look sensor never walks.
    fn seen_org(&mut self, _uid: OrgUid) -> Option<crate::org_interface::SeenOrg> {
        None
    }
    fn cell_av_counts(&self, _cell_id: i32) -> (i32, i32) {
        (0, 0)
    }
    fn cell_rand_av(&mut self, _cell_id: i32) -> Option<OrgUid> {
        None
    }
    fn cell_first_pred_av(&self, _cell_id: i32) -> Option<OrgUid> {
        None
    }
    fn cell_first_prey_av(&self, _cell_id: i32) -> Option<OrgUid> {
        None
    }
    fn resource_used_bounds(&self, _res_id: i32) -> (i32, i32, i32, i32) {
        (-1, -1, -1, -1)
    }

    fn begin_sleep(&mut self) {}
    fn end_sleep(&mut self) {}

    // The population services below are only reached from handlers that
    // first check `GetCellID() == -1` (or `GetDeme() == NULL`) and stop in a
    // test CPU; where the C++ would index the population with cell -1 the
    // handler returns `Err` before calling them. The answers are those of
    // an empty analyze-mode population.
    fn cell_deme_id(&self, _cell_id: i32) -> i32 {
        -1
    }
    fn get_deme_cell_resources(&mut self, _deme_id: i32, _cell_id: i32) -> Vec<f64> {
        Vec::new()
    }
    fn deme_resource_amount(&mut self, _deme_id: i32, _res_index: i32) -> f64 {
        0.0
    }
    fn population_resource_amount(&mut self, _org: &mut OrgState, _res_index: i32) -> f64 {
        0.0
    }
    fn deme_add_pheromone(&mut self, _deme_id: i32, _cell_id: i32, _value: f64) {}
    fn deme_cell_position(&self, _deme_id: i32, _cell_id: i32) -> (i32, i32) {
        (0, 0)
    }
    fn update_cell_resources(&mut self, _org: &mut OrgState, _res_change: &[f64], _cell_id: i32) {}
    fn stats_move(&mut self, _org: &OrgState) {}
    fn population_cell_data(&self, _cell_id: i32) -> i32 {
        -1
    }
    fn cell_organism_id(&self, _cell_id: i32) -> i32 {
        -1
    }
    fn cell_organism_uid(&self, _cell_id: i32) -> Option<OrgUid> {
        None
    }
    fn update_cell_merit(&mut self, _org: &mut OrgState, _cell_id: i32, _new_merit: f64) -> bool {
        false
    }
    fn deme_increase_energy_donated(&mut self, _deme_id: i32, _amount: f64) {}
    fn deme_increase_energy_received(&mut self, _deme_id: i32, _amount: f64) {}
    fn turn_cell_to_face(&mut self, _cell_id: i32, _target_cell_id: i32) {}
    fn stats_inc_donate_to_donor(&mut self) {}
    fn stats_log_ret_message(&mut self, _msg: &OrgMessage) {}
    fn stats_sent_flash(&mut self, _org: &OrgState) {}
    fn deme_kill_cell_event(&mut self, _org: &mut OrgState, _deme_id: i32, _event_id: i32) -> bool {
        false
    }
    /// The C++ test CPU reads the world's statistics.
    fn stats_creature_age_mean_stddev(&self) -> (f64, f64) {
        (
            self.counts.creature_age_mean,
            self.counts.creature_age_stddev,
        )
    }
    fn kill_group_member(&mut self, _group_id: i32, _org: &mut OrgState) {}
    fn deme_add_resources_consumed(&mut self, _deme_id: i32, _amount: f64) {}
    fn deme_replicate(&mut self, _deme_id: i32) {}
    fn deme_record_suicide(&mut self, _deme_id: i32, _points: i32) {}
    fn deme_increase_total_energy_testament(&mut self, _deme_id: i32, _amount: f64) {}
    fn stats_add_energy_testament(&mut self, target: TestamentTarget, amount: f64) {
        self.host.inner.stats_add_energy_testament(target, amount);
    }
    fn stats_inc_dont_explode(&mut self) {}
    fn stats_lyse(&mut self, _percent_prob: f64, _cpu_cycles: i32) {}
    fn stats_inc_quorum(&mut self, _org_ratio: i32) {}
    fn stats_inc_guard_fail(&mut self) {}
    /// The C++ test CPU reads the world's statistics, which ask the
    /// population (empty in analyze mode).
    fn stats_num_prey_creatures(&self) -> Result<i32, String> {
        Ok(self.counts.num_prey)
    }
    /// As [`Self::stats_num_prey_creatures`]: top predators plus predators.
    fn stats_num_total_pred_creatures(&self) -> Result<i32, String> {
        Ok(self.counts.num_top_pred + self.counts.num_pred)
    }
    /// As [`Self::stats_num_prey_creatures`]: predators, not top predators.
    fn stats_num_pred_creatures(&self) -> Result<i32, String> {
        Ok(self.counts.num_pred)
    }
}

// ----------------------------------------------------------------------
// cTestCPU
// ----------------------------------------------------------------------

/// `cTestCPU`.
#[derive(Debug, Clone)]
pub struct TestCpu {
    io: TestCpuIo,
    test_solo_res: i32,
    test_solo_res_lev: f64,
}

impl TestCpu {
    /// `cTestCPU(ctx, world)` (`cHardwareManager::CreateTestCPU`).
    pub fn new(world: &mut TestWorld<'_>) -> Result<Self, String> {
        let mut cpu = TestCpu {
            io: TestCpuIo::default(),
            test_solo_res: -1,
            test_solo_res_lev: 0.0,
        };
        cpu.io.init_resources(
            world.environment,
            world.config,
            RES_INITIAL,
            None,
            0,
            0,
            (-1, 0.0),
        )?;
        Ok(cpu)
    }

    /// `SetSoloRes(res_id, res_amount)`
    pub fn set_solo_res(&mut self, res_id: i32, res_amount: f64) {
        self.test_solo_res = res_id;
        self.test_solo_res_lev = res_amount;
    }

    /// `GetInputs()`
    pub fn inputs(&self) -> &[i32] {
        &self.io.input_array
    }

    /// `GetResources(ctx)`
    pub fn resources(&self) -> &[f64] {
        &self.io.resource_count
    }

    /// `TestGenome(ctx, test_info, genome)`: whether the genome forms a
    /// colony. `Err` only for an unported feature reached while running.
    pub fn test_genome(
        &mut self,
        world: &mut TestWorld<'_>,
        test_info: &mut CpuTestInfo,
        genome: &Genome,
    ) -> Result<bool, String> {
        test_info.clear();
        self.test_genome_body(world, test_info, genome, 0)?;
        Ok(test_info.is_viable)
    }

    /// `TestGenome(ctx, test_info, genome, out_fp)`: also returns the line
    /// written to `out_fp`
    /// (`is_viable max_depth depth_found max_cycle repro_type`).
    pub fn test_genome_report(
        &mut self,
        world: &mut TestWorld<'_>,
        test_info: &mut CpuTestInfo,
        genome: &Genome,
    ) -> Result<(bool, String), String> {
        test_info.clear();
        self.test_genome_body(world, test_info, genome, 0)?;

        let depth_comp = test_info.max_depth.min(2);
        let mut repro_type = i32::from(test_info.is_viable) * 3 + 1 + depth_comp;
        if !test_info.is_viable && depth_comp == 0 {
            repro_type = 0;
        }
        let line = format!(
            "{} {} {} {} {}\n",
            i32::from(test_info.is_viable),
            test_info.max_depth,
            test_info.depth_found,
            test_info.max_cycle,
            repro_type
        );
        Ok((test_info.is_viable, line))
    }

    /// `TestGenome_Body(ctx, test_info, genome, cur_depth)`.
    fn test_genome_body(
        &mut self,
        world: &mut TestWorld<'_>,
        test_info: &mut CpuTestInfo,
        genome: &Genome,
        cur_depth: i32,
    ) -> Result<bool, String> {
        let depth = usize::try_from(cur_depth).unwrap_or(usize::MAX);
        if depth >= test_info.org_array.len() {
            return Err(format!(
                "test CPU depth {} with {} generation tests (C++: cTestCPU.cc:cTestCPU::TestGenome_Body asserts)",
                cur_depth, test_info.generation_tests
            ));
        }

        self.io.use_random_inputs = test_info.use_random_inputs;
        if !test_info.use_manual_inputs {
            let random = self.io.use_random_inputs;
            world
                .environment
                .setup_inputs(world.rng, &mut self.io.input_array, random);
        } else {
            self.io.input_array = test_info.manual_inputs.clone();
        }

        self.io.receive_array = if test_info.use_random_inputs {
            let a = (15i32 << 24).wrapping_add(world.rng.get_uint(1 << 24) as i32);
            let b = (51i32 << 24).wrapping_add(world.rng.get_uint(1 << 24) as i32);
            let c = (85i32 << 24).wrapping_add(world.rng.get_uint(1 << 24) as i32);
            vec![a, b, c]
        } else {
            vec![0x0f13_9f14, 0x3308_3ee5, 0x5562_eb41]
        };

        if cur_depth == 0 {
            test_info.used_inputs = self.io.input_array.clone();
        }
        if cur_depth > test_info.max_depth {
            test_info.max_depth = cur_depth;
        }

        test_info.org_array[depth] = None;
        let organism = {
            let solo = (self.test_solo_res, self.test_solo_res_lev);
            let mut iface = TestCpuInterface::new(world, &mut self.io, test_info.cur_sg);
            let mut organism = Organism::new(
                &mut iface,
                genome,
                -1,
                OrgSource::new(transmission::DIVISION, "", true),
            )?;
            organism.state.mut_rates = test_info.mut_rates;
            organism.set_org_interface(&mut iface);
            let size = genome.representation().size();
            let cfg = iface.config;
            let update = iface.update;
            organism.state.phenotype.setup_inject(cfg, size, update);

            process_gestation(&mut iface, &mut organism, test_info, solo)?;

            organism.notify_death(&mut iface);
            organism
        };

        // Case 1: it did not copy at all.
        if organism.state.phenotype.num_divides == 0 {
            test_info.org_array[depth] = Some(organism);
            return Ok(false);
        }

        // Case 2: it copied true.
        if organism.state.phenotype.copy_true {
            test_info.depth_found = cur_depth;
            test_info.is_viable = true;
            test_info.org_array[depth] = Some(organism);
            return Ok(true);
        }

        // Case 3: its copy looks like an ancestor.
        let mut is_ancestor = false;
        for anc_depth in 0..cur_depth {
            let same = test_info
                .test_organism(anc_depth)
                .map(|anc| organism.state.offspring_genome == *anc.state.genome())
                .unwrap_or(false);
            if same {
                is_ancestor = true;
                let cur_cycle = cur_depth - anc_depth;
                if test_info.max_cycle < cur_cycle {
                    test_info.max_cycle = cur_cycle;
                }
                test_info.cycle_to = anc_depth;
            }
        }
        if is_ancestor {
            test_info.depth_found = cur_depth;
            test_info.is_viable = true;
            test_info.org_array[depth] = Some(organism);
            return Ok(true);
        }

        // Case 4: it copied false; check the child.
        let offspring = organism.state.offspring_genome.clone();
        test_info.org_array[depth] = Some(organism);
        if cur_depth + 1 < test_info.generation_tests {
            return self.test_genome_body(world, test_info, &offspring, cur_depth + 1);
        }
        Ok(false)
    }

    /// `PrintGenome(ctx, genome, filename, update, for_groups,
    /// last_birth_cell, last_group_id, last_forager_type)` into `df` (the
    /// caller opens the file, `archive/%03d-unnamed.org` when unnamed; see
    /// [`default_print_filename`]).
    #[allow(clippy::too_many_arguments)]
    pub fn print_genome(
        &mut self,
        world: &mut TestWorld<'_>,
        genome: &Genome,
        df: &mut DataFile,
        filename: &str,
        update: i32,
        for_groups: bool,
        last_birth_cell: i32,
        last_group_id: i32,
        last_forager_type: i32,
    ) -> Result<(), String> {
        let seq = genome.representation();
        let mut test_info = CpuTestInfo::default();
        self.test_genome(world, &mut test_info, genome)?;

        df.write_time_stamp();
        df.write_comment(&format!("Filename........: {}", filename));
        if update >= 0 {
            df.write_comment(&format!("Update Output...: {}", update));
        } else {
            df.write_comment("Update Output...: N/A");
        }
        df.write_comment(&format!(
            "Is Viable.......: {}",
            i32::from(test_info.is_viable)
        ));
        df.write_comment(&format!("Repro Cycle Size: {}", test_info.max_cycle));
        df.write_comment(&format!("Depth to Viable.: {}", test_info.depth_found));
        df.write_comment("");

        let num_levels = test_info.max_depth + 1;
        for j in 0..num_levels {
            df.write_comment(&format!("Generation: {}", j));
            let Some(organism) = test_info.test_organism(j) else {
                return Err(format!("print_genome: no test organism at level {}", j));
            };
            let p = &organism.state.phenotype;
            df.write_comment(&format!(
                "Merit...........: {}",
                fmt_fixed(p.merit.get_double(), 6)
            ));
            df.write_comment(&format!("Gestation Time..: {}", p.gestation_time));
            df.write_comment(&format!("Fitness.........: {}", fmt_fixed(p.fitness, 6)));
            df.write_comment(&format!("Errors..........: {}", p.last_num_errors));
            // QUIRK: the size of the tested genome at every level.
            df.write_comment(&format!("Genome Size.....: {}", seq.size()));
            df.write_comment(&format!("Copied Size.....: {}", p.copied_size));
            df.write_comment(&format!("Executed Size...: {}", p.executed_size));
            if for_groups {
                df.write_comment(&format!("Last Birth Group ID........: {}", last_group_id));
                df.write_comment(&format!(
                    "Last Birth Forager Type....: {}",
                    last_forager_type
                ));
                df.write_comment(&format!("Last Birth Cell............: {}", last_birth_cell));
            }
            if p.num_divides == 0 {
                df.write_comment("Offspring.......: NONE");
            } else if p.copy_true {
                df.write_comment("Offspring.......: SELF");
            } else if test_info.cycle_to != -1 {
                df.write_comment(&format!("Offspring.......: {}", test_info.cycle_to));
            } else {
                df.write_comment(&format!("Offspring.......: {}", j + 1));
            }
            df.write_comment("");
        }

        df.write_comment("Tasks Performed:");
        let env = &*world.environment;
        let Some(p0) = test_info.test_phenotype(0) else {
            return Err("print_genome: no test organism at level 0".to_string());
        };
        let task_name = |i: usize| env.tasklib().task_name(i as i32).to_string();
        let qual = |v: &[f64], i: usize| v.get(i).copied().unwrap_or(0.0);
        let count = |v: &[i32], i: usize| v.get(i).copied().unwrap_or(0);
        for i in 0..p0.last_task_count.len() {
            df.write_comment(&format!(
                "{} {} ({})",
                task_name(i),
                p0.last_task_count[i],
                fmt_fixed(qual(&p0.last_task_quality, i), 6)
            ));
        }

        if world.config.use_resource_bins {
            df.write_comment("Tasks Performed Using Internal Resources:");
            for i in 0..p0.last_task_count.len() {
                df.write_comment(&format!(
                    "{} {} ({})",
                    task_name(i),
                    count(&p0.last_internal_task_count, i),
                    fmt_fixed(qual(&p0.last_internal_task_quality, i), 6)
                ));
            }
            df.write_comment("Resources Collected: Name\t\tTotal\t\tAvailable");
            let resources = env.resource_lib().resources();
            for i in 0..p0.last_rbins_total.len() {
                let name = resources.get(i).map(|r| r.name()).unwrap_or("");
                df.write_comment(&format!(
                    "                {} : {}\t\t{}\t\t{}\t\t",
                    i,
                    name,
                    fmt_fixed(p0.last_rbins_total[i], 6),
                    fmt_fixed(qual(&p0.last_rbins_avail, i), 6)
                ));
            }
        }

        df.endl();

        let text = test_info
            .test_organism(0)
            .map(|o| o.hardware.inst_set().save_instruction_sequence(seq))
            .unwrap_or_default();
        let _ = df.ofstream().write_all(text.as_bytes());
        Ok(())
    }
}

/// `archive/%03d-unnamed.org`, `PrintGenome`'s default file name.
pub fn default_print_filename(genome_size: i32) -> String {
    format!("archive/{:03}-unnamed.org", genome_size)
}

/// `cTestCPU::ProcessGestation(ctx, test_info, cur_depth)` for the
/// organism of the current depth.
fn process_gestation(
    iface: &mut TestCpuInterface<'_>,
    organism: &mut Organism,
    test_info: &CpuTestInfo,
    solo: (i32, f64),
) -> Result<bool, String> {
    let cfg = iface.config;
    let size = organism.state.genome().representation().size();
    // QUIRK: the offset added here is the one left by the previous
    // `InitResources` (the constructor's 0 at depth 0), not this test's.
    let time_allocated = cfg
        .test_cpu_time_mod
        .wrapping_mul(size)
        .wrapping_add(iface.host.io.res_cpu_cycle_offset);

    iface.host.io.cur_input = 0;
    iface.host.io.cur_receive = 0;

    iface.host.io.init_resources(
        iface.environment,
        cfg,
        test_info.res_method,
        test_info.res.clone(),
        test_info.res_update,
        test_info.res_cpu_cycle_offset,
        solo,
    )?;
    let mut time_used = iface.host.io.res_cpu_cycle_offset;

    let tracer = test_info.tracer();
    organism.hardware.set_trace(tracer.clone());
    while time_used < time_allocated
        && organism.state.phenotype.num_divides == 0
        && !organism.state.is_dead()
    {
        time_used += 1;
        iface
            .host
            .io
            .update_resources(iface.environment, cfg, time_used);
        organism.single_process(iface, false)?;
    }
    organism.hardware.set_trace(None);

    if let Some(t) = tracer {
        let status = organism.print_final_status(cfg, time_used, time_allocated);
        if let Ok(mut g) = t.lock() {
            g.trace_test_cpu(&status);
        }
    }
    Ok(true)
}

/// `Divide_TestFitnessMeasures`' offspring test: a fresh test CPU, random
/// inputs, `TestGenome` on `genome`.
pub fn test_offspring(world: &mut TestWorld<'_>, genome: &Genome) -> Result<OffspringTest, String> {
    let mut cpu = TestCpu::new(world)?;
    let mut info = CpuTestInfo::default();
    info.use_random_inputs(true);
    cpu.test_genome(world, &mut info, genome)?;
    Ok(info.offspring_test())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cyclic_reads_wrap_like_cpp() {
        let a = [10, 20, 30];
        let mut p = 2;
        assert_eq!(next_cyclic(&a, &mut p), 30);
        assert_eq!(p, 3);
        assert_eq!(next_cyclic(&a, &mut p), 10);
        assert_eq!(p, 1);
        let mut q = 0;
        assert_eq!(next_cyclic(&[], &mut q), 0);
    }

    #[test]
    fn test_info_defaults_match_cpp() {
        let info = CpuTestInfo::default();
        assert_eq!(info.generation_tests(), 3);
        assert_eq!(info.max_depth(), -1);
        assert_eq!(info.depth_found(), -1);
        assert_eq!(info.cycle_to(), -1);
        assert!(!info.is_viable());
        assert_eq!(info.genotype_fitness(), 0.0);
    }

    #[test]
    fn default_filename_pads_size() {
        assert_eq!(default_print_filename(7), "archive/007-unnamed.org");
        assert_eq!(default_print_filename(1234), "archive/1234-unnamed.org");
    }
}
