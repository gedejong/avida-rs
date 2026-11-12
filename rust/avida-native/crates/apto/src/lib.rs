//! Native Rust port of the parts of the Apto support library (and the Avida
//! output layer) that Avida uses.
//!
//! | Module | Ports |
//! |---|---|
//! | [`rng`] | `Apto::RNG::AvidaRNG` + `Apto::Random` |
//! | [`scheduler`] | `Apto::PriorityScheduler`, `Scheduler::{RoundRobin, Integrated, Probabilistic, ProbabilisticIntegrated}`, `cPopulation::BuildTimeSlicer` |
//! | [`fmt`] | `printf` / `std::ostream` number formatting |
//! | [`datafile`] | `Avida::Output::{File, Manager}` |
//! | [`stat`] | `Apto::Stat::Accumulator` |
//! | [`diag`] | `cerr`/`cout` diagnostics, capturable per thread (not a port) |
//!
//! Every module is checked against reference output of the C++ (harnesses in
//! `tools/`, vectors in `testdata/`).
//!
//! Floating-point contraction: the reference binary is built by Apple clang
//! for arm64 with the default `-ffp-contract=on`, which fuses `a * b + c`
//! (and `c - a * b`) written as one C++ expression into a single
//! `fmadd`/`fmsub`, rounded once. Ports of such expressions use `mul_add`
//! (`AvidaRng::get_double_range`, `get_rand_normal_params`,
//! `Accumulator::variance`, `ProbabilisticIntegrated::next`).
//!
//! Not ported / start-up errors: `SLICING_METHOD` 3 and 4 are commented out
//! in `cPopulation::BuildTimeSlicer` and fail there with "requested time
//! slicer not found"; [`build_time_slicer`] returns the same error.

pub mod datafile;
pub mod diag;
pub mod fmt;
pub mod rng;
pub mod scheduler;
pub mod stat;

pub use datafile::{DataFile, DataValue, Manager as OutputManager, OutputId};
pub use fmt::{fmt_double, fmt_fixed, fmt_g, fmt_g_showpoint, fmt_sci, format_c, CFloatSpec};
pub use rng::AvidaRng;
pub use scheduler::{
    build_time_slicer, Integrated, Priority, PriorityScheduler, Probabilistic,
    ProbabilisticIntegrated, RoundRobin, WeightedIndex,
};
pub use stat::{AccValue, Accumulator};
