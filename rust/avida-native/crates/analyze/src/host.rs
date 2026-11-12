//! The seam between analyze mode and the rest of Avida.
//!
//! `cAnalyze` reaches into `cWorld` for configuration, output files, the
//! instruction sets, the environment, the RNG, the action library and the
//! test CPU. The data is exposed as accessors; the test CPU is reached
//! through [`AnalyzeHost::with_test_world`], which lends a
//! [`TestWorld`] to the CPU drivers of [`crate::cpu`] and
//! [`crate::landscape`]. A host without a CPU (the default) fails every
//! command that needs one with [`AnalyzeError::Unsupported`].
//! [`crate::AnalyzeWorld`] is the full host.

use std::io::Write;
use std::sync::Arc;

use avida_apto::{AvidaRng, OutputManager};
use avida_config::{Config, Feedback};
use avida_environment::ResourceHistory;
use avida_genome::InstSetRegistry;
use avida_hardware::test_cpu::{CpuTestInfo, TestWorld};

use crate::error::{AnalyzeError, Result};

/// The `cCPUTestInfo` settings analyze mode chooses before a test-CPU run
/// (`UseManualInputs` / `UseRandomInputs`, `SetResourceOptions`,
/// `SetCurrentStateGridID`). `Default` is a default-constructed
/// `cCPUTestInfo` (no resource options set: method 0, update 0).
#[derive(Debug, Clone, Default)]
pub struct CpuTestOptions {
    /// `res_method` of `SetResourceOptions` (0 = initial resources).
    pub use_resources: i32,
    /// Update for resource lookups (`-1` = none, as the commands pass it).
    pub update: i32,
    /// `UseRandomInputs(true)`.
    pub use_random_inputs: bool,
    /// `UseManualInputs(inputs)`; takes precedence over random inputs.
    pub manual_inputs: Option<Vec<i32>>,
    /// `m_resources`, the history loaded with `LOAD_RESOURCES` (NULL until then).
    pub resources: Option<Arc<ResourceHistory>>,
    /// `m_resource_time_spent_offset`.
    pub resource_time_spent_offset: i32,
    /// `SetCurrentStateGridID(sg)` (`TRACE sg=N`).
    pub state_grid: i32,
}

impl CpuTestOptions {
    /// A `cCPUTestInfo` with these settings, applied in the order the
    /// analyze commands apply them.
    pub fn test_info(&self) -> CpuTestInfo {
        let mut info = CpuTestInfo::default();
        match &self.manual_inputs {
            Some(inputs) => info.use_manual_inputs(inputs.clone()),
            None => info.use_random_inputs(self.use_random_inputs),
        }
        info.set_resource_options(
            self.use_resources,
            self.resources.clone(),
            self.update,
            self.resource_time_spent_offset,
        );
        info.set_current_state_grid_id(self.state_grid);
        info
    }
}

/// The `cAvidaContext` a test CPU runs in.
pub enum TestContext<'r> {
    /// `m_world->GetDefaultContext()`: the world RNG.
    Default,
    /// The context of an analyze job (`cAnalyzeJobQueue`): its own RNG,
    /// seeded per job, and the analyze-mode flag clear (as in the C++
    /// single-threaded job execution).
    Job(&'r mut AvidaRng),
}

/// A private world for one worker thread of the analyze job queue
/// (`cAnalyzeJobWorker`): its own copy of everything a test can change
/// (the environment, the world-service hooks), the rest of the host shared
/// read-only. See [`AnalyzeHost::worker_worlds`].
pub trait WorkerWorld: Send {
    /// Lend `f` this worker's world as a test CPU sees it, in a context
    /// whose RNG is `rng` and whose analyze-mode flag is `analyze_mode`
    /// (`false` for an analyze job, the default context's flag for work
    /// the C++ runs in the default context).
    fn with_test_world(
        &mut self,
        rng: &mut AvidaRng,
        analyze_mode: bool,
        f: &mut dyn FnMut(&mut TestWorld<'_>) -> Result<()>,
    ) -> Result<()>;

    /// The host's instruction sets (shared, read-only).
    fn inst_sets(&self) -> &InstSetRegistry;

    /// A complete description of the state the tests could have changed
    /// (the environment), compared by [`AnalyzeHost::adopt_worker_states`]
    /// after the jobs ran.
    fn state_digest(&self) -> String;
}

/// What analyze mode needs from the world. See the module documentation.
pub trait AnalyzeHost {
    /// `m_world->GetConfig()`. Verbosity is `config().verbosity`.
    fn config(&self) -> &Config;

    /// Mutable configuration (`CONFIG_SET`, the `VERBOSE` action).
    fn config_mut(&mut self) -> &mut Config;

    /// `m_world->GetWorkingDir()`.
    fn working_dir(&self) -> &str;

    /// `Avida::Output::Manager::Of(world)`.
    fn output(&mut self) -> &mut OutputManager;

    /// `m_world->GetHardwareManager()` instruction sets.
    fn inst_sets(&self) -> &InstSetRegistry;

    /// The same, mutable (`RegisterInstSet`, `SetProbFail` on a registered
    /// set).
    fn inst_sets_mut(&mut self) -> Result<&mut InstSetRegistry> {
        Err(AnalyzeError::unsupported(
            "changing the instruction sets",
            "cHardwareManager::RegisterInstSet",
        ))
    }

    /// `GetEnvironment().GetNumTasks()`.
    fn num_tasks(&self) -> i32;

    /// `GetEnvironment().GetTask(task_id).GetDesc()`. The C++ indexes the
    /// task array unchecked; return [`AnalyzeError::Fatal`] when out of range.
    fn task_desc(&self, task_id: i32) -> Result<String>;

    /// `GetEnvironment().GetInputSize()`.
    fn input_size(&self) -> i32;

    /// `GetEnvironment().GetNumStateGrids()`.
    fn num_state_grids(&self) -> i32 {
        0
    }

    /// `m_world->GetRandom()`: the world RNG, or `None` for a host without
    /// one (then nothing that draws from it can run).
    fn world_rng(&mut self) -> Option<&mut AvidaRng> {
        None
    }

    /// `m_ctx.GetAnalyzeMode()` of the world's default context.
    fn analyze_mode(&self) -> bool {
        false
    }

    /// `m_ctx.SetAnalyzeMode()` / `ClearAnalyzeMode()`.
    fn set_analyze_mode(&mut self, _on: bool) {}

    /// `cActionLibrary::GetInstance().Supports(name)` for actions the host
    /// runs itself through [`AnalyzeHost::run_action`]. The analyze crate
    /// already knows every registered action name (see
    /// [`crate::actions::is_registered_action`]) and runs `VERBOSE` and the
    /// landscape actions itself; a host only answers for extra names.
    fn has_action(&self, _name: &str) -> bool {
        false
    }

    /// `cAnalyzeCommandAction::Run` for a registered action that analyze
    /// mode does not run itself: create the action `name` with `args`
    /// (messages into `feedback`) and process it in the default context.
    /// Console output of the action goes to `out` so it stays in order with
    /// analyze mode's own messages. The default fails loudly.
    fn run_action(
        &mut self,
        name: &str,
        _args: &str,
        _feedback: &mut Feedback,
        _out: &mut dyn Write,
    ) -> Result<()> {
        Err(AnalyzeError::unsupported(
            name,
            "cAnalyzeCommandAction::Run (cActionLibrary)",
        ))
    }

    /// `GetEnvironment().LoadLine(line, feedback)` (`ENVIRONMENT`).
    fn load_environment_line(&mut self, _line: &str, _feedback: &mut Feedback) -> Result<()> {
        Err(AnalyzeError::unsupported(
            "ENVIRONMENT",
            "cAnalyze::EnvironmentSetup (cEnvironment::LoadLine)",
        ))
    }

    /// `GetHardwareManager().GetInstSet(inst_set).ActivateNullInst()`: the
    /// set's NULL instruction, appended to the set on first use (which
    /// makes `GetSize()` one larger for everything that runs afterwards).
    fn activate_null_inst(&mut self, inst_set: &str) -> Result<avida_genome::Instruction> {
        Err(AnalyzeError::unsupported(
            format!("ActivateNullInst of instruction set '{inst_set}'"),
            "cInstSet::ActivateNullInst",
        ))
    }

    /// Lend `f` the world as a test CPU sees it (`cWorld` through
    /// `cTestCPU` and `cTestCPUInterface`) in context `ctx`. Everything that
    /// runs a genome goes through here; the default has no CPU.
    fn with_test_world(
        &mut self,
        _ctx: TestContext<'_>,
        _f: &mut dyn FnMut(&mut TestWorld<'_>) -> Result<()>,
    ) -> Result<()> {
        Err(AnalyzeError::unsupported(
            "test CPU",
            "cHardwareManager::CreateTestCPU",
        ))
    }

    /// `n` private worlds for worker threads, each equal to the host's own
    /// world as [`AnalyzeHost::with_test_world`] lends it (same
    /// environment state; configuration, instruction sets and the rest
    /// shared read-only). `None` when the host cannot make such copies
    /// exactly (the default); the analyze jobs then run one at a time on
    /// the host, which gives the same output.
    fn worker_worlds(&self, _n: usize) -> Option<Vec<Box<dyn WorkerWorld + '_>>> {
        None
    }

    /// After worker worlds from [`AnalyzeHost::worker_worlds`] ran their
    /// tests: `digests` are their [`WorkerWorld::state_digest`]s. Bring the
    /// host's own world to the state the same tests would have left it in
    /// had they run on the host, and return `true`; or return `false` (and
    /// change nothing) when a test changed a worker's state in a way that
    /// would have been visible to later tests, so the jobs must run on the
    /// host one at a time instead.
    fn adopt_worker_states(&mut self, _digests: &[String]) -> bool {
        false
    }
}
