//! avida-environment: native port of Avida's environment (resources,
//! reactions, the `cEnvironment` parser and its reaction evaluation).
//!
//! C++ sources (avida-core/source/main/): cEnvironment, cReaction,
//! cReactionLib, cReactionProcess, cReactionRequisite,
//! cContextReactionRequisite, cContextPhenotype, cReactionResult, cResource
//! (with cCellResource), cResourceLib, cResourceHistory, cStateGrid, cEnvReqs
//! and cCellBox.
//!
//! # Which C++
//!
//! The reference is the last pre-Rust C++, commit 47f13dadb (2025-01-27,
//! parent of the first Rust commit). Where the current C++ calls
//! rust/avida-rust helpers (`avd_env_*`, `avd_rh_*`) the original bodies
//! are ported. The older ef5c6d948 (2016) is not used: it predates the
//! `cellbox` requisite and the 2018 change of the inflow/outflow fix-ups
//! from `> -99` to `>= 0` (inflow) / `> 0` (outflow), both of which the
//! consistency tests rely on.
//!
//! # Seams
//!
//! * The task library is another crate: [`TaskRegistry`] is what
//!   `cEnvironment` needs of `cTaskLib` while loading, [`TaskEvaluator`] what
//!   `TestOutput` reaches through `cTaskContext` (task tests, phenotype,
//!   cell, plasticity cache, HGT fragments). [`SimpleTaskLib`] is a stand-in
//!   registry with `cTaskLib::AddTask`'s id assignment.
//! * `Environment::Manager::DefineActionTrigger` is recorded in
//!   [`Environment::action_triggers`].
//! * `cResourceHistory::GetResourceCountForUpdate` writes through a callback
//!   instead of a `cResourceCount`.
//! * [`MutationRates`] is the full `cMutationRates` port; avida-phenotype re-exports it.
//!
//! # Quirks kept (each marked `QUIRK:` at its site)
//!
//! * `max_count` / `min_count` requisites limit the *task* count;
//!   `reaction_max_count` / `reaction_min_count` the reaction count.
//! * `Resource` defaults `inflowX1` to 0 (all other coordinates -99), so an
//!   HGT resource is "misconfigured" unless `inflowx=-99` is given.
//! * Outflow single-point fix-ups test `> 0`, so an outflow coordinate of 0
//!   gets no X2/Y2.
//! * `CELL` and `GRADIENT_RESOURCE` never assign a resource index.
//! * A cell box reaching the world edge (`x + width == WORLD_X`) is rejected.
//! * Without an organism (deme evaluation) every requisite fails its
//!   cell-box check.
//! * Action-trigger ordering is the library size at definition time even
//!   when `AddTask` reused an existing entry.
//! * Detected amounts start at -1 after activation and are added to.
//! * `cReactionResult::AddInst` does not activate the result.
//! * `cResourceHistory`: an empty history matches an inexact lookup with all
//!   levels 0; `LoadFile` replaces existing entries.
//! * `cArgContainer` (state grids) stops at the first empty entry and only
//!   logs out-of-range ints.
//! * `ResourceLib::get_resource_id` prints `Error: Unknown resource 'NAME'.`
//!   to stderr for an unknown name, as `cResourceLib::GetResource` does.
//!
//! # Undefined C++ behaviour given defined results
//!
//! * `ENZYME` process without a resource (null dereference): no bonus.
//! * Specific inputs shorter than the input size, missing resource bins,
//!   cell boxes with `WORLD_X == 0`, negative `RANDOM:n` and negative or
//!   overflowing state-grid sizes: skipped / 0 / false / error instead of
//!   out-of-bounds access or abort.
//!
//! # Unsupported
//!
//! Nothing in the 47f13dadb `cEnvironment::LoadLine` is unsupported. Its
//! keywords are `RESOURCE`, `REACTION`, `SET_ACTIVE`, `CELL`, `GRID` and
//! `GRADIENT_RESOURCE`; older Avida keywords (`MUTATION`, `GROWTH`, ...)
//! are errors there too.

// Transliterated C++ loops index several arrays by one counter.
#![allow(clippy::needless_range_loop)]

// `eprintln!` prints through `avida_apto::diag`, which a front end can
// capture (the std macro's output when nothing captures).
macro_rules! eprintln {
    ($($arg:tt)*) => {
        ::avida_apto::diag::stderr(format_args!("{}\n", format_args!($($arg)*)))
    };
}

mod arg_schema;
pub mod context_phenotype;
pub mod env_reqs;
pub mod environment;
mod evaluation;
pub mod mutation_rates;
pub mod reaction;
pub mod reaction_result;
pub mod resource;
pub mod resource_history;
pub mod resource_lib;
pub mod state_grid;
pub mod task;

pub use context_phenotype::ContextPhenotype;
pub use env_reqs::EnvReqs;
pub use environment::{ActionTrigger, Environment, INPUT_SIZE_DEFAULT, OUTPUT_SIZE_DEFAULT};
pub use mutation_rates::MutationRates;
pub use reaction::{
    proc_type, CellBox, ContextReactionRequisite, PhenPlastBonusMethod, Reaction, ReactionLib,
    ReactionProcess, ReactionRequisite,
};
pub use reaction_result::ReactionResult;
pub use resource::{geometry, CellResource, Resource};
pub use resource_history::ResourceHistory;
pub use resource_lib::ResourceLib;
pub use state_grid::StateGrid;
pub use task::{
    SimpleTask, SimpleTaskLib, TaskEnvHooks, TaskEvaluator, TaskRegistry, ARGUMENT_TASKS,
};
