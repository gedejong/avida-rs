//! avida-genome: native Rust port of Avida's genome-level types.
//!
//! | Module | C++ origin |
//! |---|---|
//! | [`instruction`] | `Avida::Instruction` (core/InstructionSequence.h) |
//! | [`sequence`] | `Avida::InstructionSequence` (core/InstructionSequence.{h,cc}) |
//! | [`genome_util`] | `cGenomeUtil` (main/cGenomeUtil.{h,cc}) |
//! | [`genome`] | `Avida::Genome`, `PropertyMap` (core/Genome.{h,cc}, core/Properties.h) |
//! | [`cpu_memory`] | `cCPUMemory` (cpu/cCPUMemory.{h,cc}) |
//! | [`arg_schema`] | `cArgSchema`, `cArgContainer` (tools/cArgSchema.{h,cc}, cArgContainer.{h,cc}) |
//! | [`weighted_index`] | `cOrderedWeightedIndex` (tools/cOrderedWeightedIndex.{h,cc}) |
//! | [`inst_lib`] | data part of `cInstLib`, `tInstLib`, `cInstLibEntry` (cpu/) |
//! | [`inst_set`] | `cInstSet` (cpu/cInstSet.{h,cc}) |
//! | [`inst_set_registry`] | inst-set registry of `cHardwareManager` (cpu/cHardwareManager.{h,cc}) |
//! | [`genome_loader`] | `Util::LoadGenomeDetailFile` (util/GenomeLoader.cc) |
//! | [`feedback`] | `cUserFeedback` (tools/cUserFeedback.h), re-exported from `avida-config` |
//!
//! `.org` and legacy instruction set files are read with
//! `avida_config::InitFile` (`cInitFile`).

pub mod arg_schema;
pub mod cpu_memory;
mod cstr;
pub mod feedback;
pub mod genome;
pub mod genome_loader;
pub mod genome_util;
pub mod inst_lib;
pub mod inst_set;
pub mod inst_set_registry;
pub mod instruction;
#[cfg(test)]
mod real_config_tests;
pub mod sequence;
pub mod weighted_index;

pub use arg_schema::{ArgContainer, ArgSchema, ArgType};
pub use cpu_memory::CpuMemory;
pub use feedback::{Feedback, FeedbackKind};
pub use genome::{Genome, Property, PropertyMap};
pub use genome_loader::load_genome_detail_file;
pub use genome_util::{FragmentList, SubstringMatch};
pub use inst_lib::{InstLib, InstLibEntry};
pub use inst_set::{InstCodeConfig, InstSet};
pub use inst_set_registry::{InstLibProvider, InstSetRegistry};
pub use instruction::Instruction;
pub use sequence::InstructionSequence;
pub use weighted_index::OrderedWeightedIndex;
