//! avida-tasks: native Rust port of the Avida task library.
//!
//! Ports `cTaskLib`, `cTaskEntry`, `cTaskContext`, `cTaskState`, `cEnvReqs`
//! (avida-core/source/main/) together with the argument machinery the tasks use
//! (`cArgSchema`, `cArgContainer`, avida-core/source/tools/) and `tBuffer<int>`
//! (as [`IoBuffer`]).
//!
//! Layout:
//! - `io_buffer`: `tBuffer<int>` with identical circular semantics.
//! - `arg_schema`: private `cArgSchema` / `cArgContainer` (the genome crate is
//!   building the shared one; unify later).
//! - `env_reqs`: `cEnvReqs`.
//! - `context`: `cTaskContext`, `cTaskState` and the [`TaskWorld`] trait that
//!   upper layers implement for everything a task pulls from the organism,
//!   population or world.
//! - `task_lib`: `cTaskLib` / `cTaskEntry` (registration, `SetupTests`,
//!   `TestOutput`).
//! - `eval_*` modules: the evaluators themselves.
//!
//! Behaviour reference: the C++ at the last commit before the Rust migration
//! started (`ef5c6d948`), cross-checked against the current tree. Where the
//! earlier FFI-shaped Rust (`rust/avida-rust/src/task_lib_helpers.rs`) diverged
//! from that C++, the C++ won; those places are marked `QUIRK:` or noted in the
//! evaluator docs.
//!
//! Config flags not yet ported: none are consulted here. World configuration
//! the C++ reads through `m_world->GetConfig()` (`WORLD_X`, `WORLD_Y`,
//! `USE_AVATARS`, `MATCH_ALREADY_PRODUCED`) is exposed as [`TaskWorld`] methods.

// The evaluators are transliterations of the C++ loops (index-based, `-1.0 *
// fabs(x)`); keep them recognisable against the source.
#![allow(clippy::needless_range_loop, clippy::neg_multiply)]

pub mod arg_schema;
pub mod context;
mod cstr;
pub mod env_reqs;
mod eval_logic;
mod eval_math;
mod eval_misc;
mod eval_world;
pub mod io_buffer;
pub mod libc_rand;
mod registry_table;
pub mod task_lib;

pub use arg_schema::{ArgContainer, ArgSchema, ArgType, Feedback, FeedbackKind};
pub use context::{FibSeqState, TaskContext, TaskState, TaskStates, TaskWorld};
pub use env_reqs::EnvReqs;
pub use eval_logic::{compute_logic_id, eval_logic, LogicOp};
pub use io_buffer::IoBuffer;
pub use libc_rand::LibcRand;
pub use registry_table::{Math1Op, Math2Op, Math3Op, TaskKind, UnaryOp};
pub use task_lib::{TaskEntry, TaskLib};
