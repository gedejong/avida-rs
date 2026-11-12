//! The environment's view of the task library (`cTaskLib`, `cTaskEntry`,
//! `cTaskContext` in avida-core/source/main/) and of the organism a task is
//! evaluated for.
//!
//! The task library itself is ported in another crate. This module defines
//! the two seams the environment needs:
//!
//! * [`TaskRegistry`]: registration and lookup, shaped after how
//!   `cEnvironment::LoadReaction` calls `cTaskLib::AddTask` and how the rest
//!   of `cEnvironment` reads task entries;
//! * [`TaskEvaluator`]: what `cEnvironment::TestOutput` reaches through
//!   `cTaskContext` (task tests, the organism's phenotype, its cell, the
//!   phenotypic-plasticity cache).
//!
//! [`SimpleTaskLib`] is a stand-in registry that assigns task ids exactly as
//! `cTaskLib::AddTask` does, for tests and for loading environment files
//! without the real task library.

use crate::env_reqs::EnvReqs;
use crate::environment::Environment;
use crate::resource_lib::ResourceLib;
use avida_apto::AvidaRng;
use avida_config::feedback::Feedback;
use std::collections::BTreeSet;

/// The environment state a task loader may touch while it is registered
/// (`m_world->GetEnvironment()` inside `cTaskLib::Load_*`).
pub struct TaskEnvHooks<'a> {
    resource_lib: &'a ResourceLib,
    group_ids: &'a mut BTreeSet<i32>,
    target_ids: &'a mut BTreeSet<i32>,
    target_added: bool,
}

impl<'a> TaskEnvHooks<'a> {
    /// Hooks over the given environment state.
    pub fn new(
        resource_lib: &'a ResourceLib,
        group_ids: &'a mut BTreeSet<i32>,
        target_ids: &'a mut BTreeSet<i32>,
    ) -> Self {
        TaskEnvHooks {
            resource_lib,
            group_ids,
            target_ids,
            target_added: false,
        }
    }

    /// `cEnvironment::AddGroupID(id)` (called by `Load_FormSpatialGroup*`).
    pub fn add_group_id(&mut self, id: i32) {
        self.group_ids.insert(id);
    }

    /// `cEnvironment::AddTargetID(id)` (called by the `Load_ConsumeTarget*`
    /// family). The environment recomputes its attack/prey list after the
    /// task is added, which gives the same result as the C++ recomputing it
    /// on every call.
    pub fn add_target_id(&mut self, id: i32) {
        self.target_ids.insert(id);
        self.target_added = true;
    }

    /// `cEnvironment::GetResourceLib()`.
    pub fn resource_lib(&self) -> &ResourceLib {
        self.resource_lib
    }

    /// Whether `add_target_id` was called.
    pub fn target_added(&self) -> bool {
        self.target_added
    }
}

/// Registration side of `cTaskLib` as used by `cEnvironment`.
pub trait TaskRegistry {
    /// `cTaskLib::AddTask(name, info, envreqs, feedback)`: the id of the
    /// task entry for trigger `name` with argument string `info` (an
    /// existing argument-less entry of the same name is reused), or `None`
    /// (C++ `NULL`) after logging the reason to `feedback`.
    fn add_task(
        &mut self,
        name: &str,
        info: &str,
        envreqs: &mut EnvReqs,
        feedback: &mut Feedback,
        env: &mut TaskEnvHooks<'_>,
    ) -> Option<i32>;

    /// `cTaskLib::GetSize()`.
    fn num_tasks(&self) -> i32;

    /// `GetTask(id).GetName()`.
    fn task_name(&self, id: i32) -> &str;

    /// `GetTask(id).GetDesc()`.
    fn task_desc(&self, id: i32) -> &str;

    /// `cTaskLib::UseNeighborInput()`.
    fn use_neighbor_input(&self) -> bool {
        false
    }

    /// `cTaskLib::UseNeighborOutput()`.
    fn use_neighbor_output(&self) -> bool {
        false
    }
}

/// What `cEnvironment::TestOutput` / `TestRequisites` / `DoProcesses` reach
/// through `cTaskContext`: the task library's tests (given the library `lib`
/// owned by the environment) and the organism being evaluated.
///
/// The environment calls these in exactly the C++ order; implementations
/// must draw from `rng` exactly where the C++ draws from the world RNG.
pub trait TaskEvaluator<T: ?Sized> {
    /// `m_tasklib.SetupTests(taskctx)`.
    fn setup_tests(&mut self, lib: &T);

    /// `taskctx.SetTaskEntry(task)`: subsequent calls refer to task `task_id`.
    fn set_task_entry(&mut self, lib: &T, task_id: i32);

    /// `taskctx.GetOnDivide()`.
    fn on_divide(&self) -> bool;

    /// `m_tasklib.TestOutput(taskctx)`: quality (>= 0) of the current task.
    fn test_output(&mut self, lib: &T, rng: &mut AvidaRng) -> f64;

    /// `taskctx.GetTaskValue()` (read right after `test_output`).
    fn task_value(&self) -> f64;

    /// `taskctx.GetOrganism() != NULL` (false for deme-level evaluation).
    fn has_organism(&self) -> bool;

    /// `taskctx.GetOrganism()->GetCellID()`.
    fn cell_id(&self) -> i32;

    /// `GetPhenotype().GetStolenReactionCount()[reaction_id]`.
    fn stolen_reaction_count(&self, reaction_id: usize) -> i32;

    /// `GetPhenotype().SetFirstReactionCycle(id)` followed by
    /// `SetFirstReactionExec(id)`.
    fn set_first_reaction(&mut self, reaction_id: i32);

    /// `cPhenPlastUtil::GetTaskProbability(ctx, world,
    /// organism->SystematicsGroup("genotype"), task_id)`.
    ///
    /// `env` is the environment under evaluation, handed back because the
    /// C++ runs test CPUs here (`TestPlasticity`, the first time the
    /// genotype is asked), and they evaluate their own output in it.
    fn task_probability(
        &mut self,
        env: &mut Environment<T>,
        rng: &mut AvidaRng,
        task_id: i32,
    ) -> f64
    where
        T: Sized;

    /// HGT metabolism: if the organism's cell is not -1 and holds genome
    /// fragments, `cell.PopGenomeFragment(ctx)`, record
    /// `Stats::GenomeFragmentMetabolized(org, fragment)` and return the
    /// fragment's length; otherwise `None`.
    fn metabolize_genome_fragment(&mut self, rng: &mut AvidaRng) -> Option<i32>;

    /// A process-resource change requested by the last `test_output`
    /// (`cEnvironment::ChangeResource(reaction, resource_name, 0)`, done by
    /// the `xor-max` task): `(reaction id, resource name)`. The environment
    /// applies it immediately, before the reaction's processes run.
    fn take_resource_change(&mut self) -> Option<(i32, String)> {
        None
    }
}

/// One entry of [`SimpleTaskLib`] (`cTaskEntry` without the test function).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SimpleTask {
    /// `GetName()`
    pub name: String,
    /// `GetDesc()`
    pub desc: String,
    /// The argument string the task was added with.
    pub info: String,
    /// `HasArguments()`: the C++ loads these through a `Load_*` function
    /// with a `cArgContainer`.
    pub has_arguments: bool,
}

/// Trigger names that `cTaskLib::AddTask` routes through a `Load_*` function
/// (and so always carry an argument container and are never de-duplicated).
pub const ARGUMENT_TASKS: &[&str] = &[
    "matchstr",
    "match_number",
    "matchprodstr",
    "sort_inputs",
    "fibonacci_seq",
    "mult",
    "div",
    "log",
    "log2",
    "log10",
    "sqrt",
    "sine",
    "cosine",
    "optimize",
    "sg_path_traversal",
    "form-group",
    "form-group-id",
    "live-on-patch-id",
    "collect-odd-cell",
    "eat-target",
    "eat-target-echo",
    "eat-target-nand",
    "eat-target-and",
    "eat-target-orn",
    "eat-target-or",
    "eat-target-andn",
    "eat-target-nor",
    "eat-target-xor",
    "eat-target-equ",
    "move-ft",
    "all-ones",
    "royal-road",
    "royal-road-wd",
    "opinion_is",
];

/// `NewTask` descriptions of the basic tasks (cTaskLib::AddTask); other
/// names use the name itself as description.
const BASIC_DESCS: &[(&str, &str)] = &[
    ("echo", "Echo"),
    ("add", "Add"),
    ("add3", "Add3"),
    ("sub", "Sub"),
    ("dontcare", "DontCare"),
    ("not", "Not"),
    ("nand", "Nand"),
    ("and", "And"),
    ("orn", "OrNot"),
    ("or", "Or"),
    ("andn", "AndNot"),
    ("nor", "Nor"),
    ("xor", "Xor"),
    ("equ", "Equals"),
];

/// A stand-in task registry: accepts any non-empty trigger name and assigns
/// ids exactly like `cTaskLib::AddTask` (new entries get the next id; an
/// existing entry with the same name and no arguments is returned instead).
#[derive(Debug, Clone, Default)]
pub struct SimpleTaskLib {
    /// Entries by id.
    pub tasks: Vec<SimpleTask>,
}

impl SimpleTaskLib {
    /// An empty library.
    pub fn new() -> Self {
        Self::default()
    }
}

impl TaskRegistry for SimpleTaskLib {
    fn add_task(
        &mut self,
        name: &str,
        info: &str,
        _envreqs: &mut EnvReqs,
        feedback: &mut Feedback,
        _env: &mut TaskEnvHooks<'_>,
    ) -> Option<i32> {
        for (i, t) in self.tasks.iter().enumerate() {
            if t.name == name && !t.has_arguments {
                return Some(i as i32);
            }
        }
        if name.is_empty() {
            feedback.error(format!("unknown/unprocessed task entry '{}'", name));
            return None;
        }
        let desc = BASIC_DESCS
            .iter()
            .find(|(n, _)| *n == name)
            .map(|(_, d)| d.to_string())
            .unwrap_or_else(|| name.to_string());
        self.tasks.push(SimpleTask {
            name: name.to_string(),
            desc,
            info: info.to_string(),
            has_arguments: ARGUMENT_TASKS.contains(&name),
        });
        Some(self.tasks.len() as i32 - 1)
    }

    fn num_tasks(&self) -> i32 {
        self.tasks.len() as i32
    }

    fn task_name(&self, id: i32) -> &str {
        &self.tasks[id as usize].name
    }

    fn task_desc(&self, id: i32) -> &str {
        &self.tasks[id as usize].desc
    }
}
