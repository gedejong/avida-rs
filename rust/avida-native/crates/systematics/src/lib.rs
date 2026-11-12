//! avida-systematics: native Rust port of Avida's systematics layer
//! (avida-core/source/systematics/, avida-core/include/public/avida/systematics/,
//! avida-core/include/private/avida/private/systematics/), read at the
//! pre-Rust reference commit 47f13dadb.
//!
//! Units (organisms, parasites) are classified into groups by arbiters: the
//! [`GenotypeArbiter`] groups units by source class and genome and tracks
//! the phylogeny (parents, depth, historic genotypes, coalescence); the
//! [`CladeArbiter`] groups them by name. A [`Manager`] owns the arbiters in
//! registration order.
//!
//! Ownership differs from the C++ smart-pointer graph: arbiters own their
//! groups in arenas, everyone else holds a [`GroupHandle`] (inside a
//! [`GroupRef`] that also names the arbiter), and a unit's groups are its
//! [`GroupMembership`]. Reference counts (active for living units and birth
//! chamber entries, passive for children and demes) are kept exactly as in
//! C++, and decide when a genotype goes historic and when it is pruned.
//!
//! Output order depends on Apto container internals, which are reproduced:
//! `Apto::List<T, SparseVector>` ([`sparse_list`]) for the genotype lists
//! and `Apto::Map` ([`apto_map`]) for clades and the `.spop` writer.
//!
//! World dependencies are parameters: update numbers, `THRESHOLD`,
//! `DISABLE_GENOTYPE_CLASSIFICATION`, the environment's action trigger
//! names, unit data ([`UnitInfo`], [`GestationInfo`]) and the test CPU
//! ([`GenotypeArbiter::genome_test_metrics`] takes it as a closure).
//!
//! Not ported: `Serialize` (a stub returning false in C++), the data
//! manager registration (provided values are read with `provided_value`),
//! `PrintListStatus` (debug output), `SaveFlameData`.

// `eprintln!` prints through `avida_apto::diag`, which a front end can
// capture (the std macro's output when nothing captures).
macro_rules! eprintln {
    ($($arg:tt)*) => {
        ::avida_apto::diag::stderr(format_args!("{}\n", format_args!($($arg)*)))
    };
}

pub mod apto_map;
pub mod arena;
pub mod clade;
pub mod genotype;
pub mod genotype_arbiter;
pub mod group_data;
pub mod manager;
pub mod property;
pub mod source;
pub mod sparse_list;
pub mod spop;
pub mod stats;
pub mod unit;

pub use arena::GroupHandle;
pub use clade::{Clade, CladeArbiter};
pub use genotype::Genotype;
pub use genotype_arbiter::{
    hash_genome, GenotypeArbiter, GenotypeCursor, GenotypeStats, SystematicsEvent,
    EVENT_ADD_THRESHOLD, EVENT_REMOVE_THRESHOLD, HASH_SIZE,
};
pub use group_data::{GenomeTestMetrics, GroupData, SexualAncestry};
pub use manager::{Arbiter, Manager};
pub use property::PropertyValue;
pub use source::{Source, TransmissionType};
pub use unit::{
    ClassificationHints, GestationInfo, GroupMembership, GroupRef, RoleClassificationHints,
    RoleIndex, UnitInfo,
};
