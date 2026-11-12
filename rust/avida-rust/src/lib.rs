#![deny(unsafe_op_in_unsafe_fn)]
#![deny(clippy::undocumented_unsafe_blocks)]
#![cfg_attr(
    not(test),
    deny(
        clippy::dbg_macro,
        clippy::expect_used,
        clippy::panic,
        clippy::todo,
        clippy::unimplemented,
        clippy::unwrap_used
    )
)]
#![cfg_attr(test, allow(clippy::expect_used, clippy::panic, clippy::unwrap_used))]

pub mod analyze_helpers;
pub mod birth;
pub mod bit_array;
pub mod code_label;
pub mod common;
pub mod config_snapshot;
pub mod cpu_helpers;
pub mod cpu_instructions;
pub mod cpu_memory_instructions;
pub mod cpu_registers;
pub mod cpu_stack;
pub mod deme_helpers;
pub mod double_sum;
pub mod environment_helpers;
pub mod event_list_helpers;
pub mod ffi_rng;
pub mod ffi_string;
pub mod ffi_vec;
pub mod gradient;
pub mod histogram;
pub mod landscape_helpers;
pub mod merit;
pub mod mutation_rates;
pub mod ordered_weighted_index;
pub mod package;
pub mod phenotype;
pub mod population_action_helpers;
pub mod population_helpers;
pub mod population_interface_helpers;
pub mod print_action_helpers;
pub mod provider_helpers;
pub mod registry_types;
pub mod resource_count_helpers;
pub mod resource_history_helpers;
pub mod running_average;
pub mod running_stats;
pub mod script_helpers;
pub mod sensor_helpers;
pub mod spatial_res_count_helpers;
pub mod stats_helpers;
pub mod task_context;
pub mod task_lib_helpers;
pub mod time_series_recorder;
pub mod weighted_index;

pub use package::*;
pub use provider_helpers::*;

#[repr(C)]
pub struct AvidaRunningStatsHandle {
    pub(crate) n: f64,
    pub(crate) m1: f64,
    pub(crate) m2: f64,
    pub(crate) m3: f64,
    pub(crate) m4: f64,
}

#[repr(C)]
pub struct AvidaRunningAverageHandle {
    pub(crate) values: Vec<f64>,
    pub(crate) s1: f64,
    pub(crate) s2: f64,
    pub(crate) window_size: usize,
    pub(crate) pointer: usize,
    pub(crate) n: usize,
}

#[repr(C)]
pub struct AvidaDoubleSumHandle {
    pub(crate) s1: f64,
    pub(crate) s2: f64,
    pub(crate) n: f64,
    pub(crate) max: f64,
}

#[repr(C)]
pub struct AvidaWeightedIndexHandle {
    pub(crate) size: usize,
    pub(crate) item_weight: Vec<f64>,
    pub(crate) subtree_weight: Vec<f64>,
}

#[repr(C)]
pub struct AvidaOrderedWeightedIndexHandle {
    pub(crate) item_weight: Vec<f64>,
    pub(crate) cum_weight: Vec<f64>,
    pub(crate) item_value: Vec<i32>,
}

#[repr(C)]
pub struct AvidaHistogramHandle {
    pub(crate) bins: Vec<i32>,
    pub(crate) min_bin: i32,
    pub(crate) max_bin: i32,
    pub(crate) entry_count: i32,
    pub(crate) entry_total: i32,
}

#[repr(C)]
pub struct AvidaRawBitArrayHandle {
    pub(crate) bit_fields: Vec<u32>,
}
