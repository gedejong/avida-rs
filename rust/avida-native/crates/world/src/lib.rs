//! avida-world: cells, demes, population, birth chamber, stats, actions,
//! events and the run driver (see `ARCHITECTURE.md`).
//!
//! `eprint!` and `eprintln!` in this crate are the macros defined below,
//! not the std ones: they print through [`avida_apto::diag`], which a front
//! end can capture; without a capture they print what the std macros print.

macro_rules! eprint {
    ($($arg:tt)*) => { ::avida_apto::diag::stderr(format_args!($($arg)*)) };
}
macro_rules! eprintln {
    () => { ::avida_apto::diag::stderr(format_args!("\n")) };
    ($($arg:tt)*) => {
        ::avida_apto::diag::stderr(format_args!("{}\n", format_args!($($arg)*)))
    };
}

pub mod action_library;
pub mod actions;
pub mod actions2;
pub mod actions3;
pub mod actions_deme2;
pub mod actions_env;
pub mod actions_hist;
pub mod actions_pop2;
pub mod actions_print2;
pub mod actions_stats2;
pub mod analyze_setup;
pub mod avatars;
pub mod birth_chamber;
pub mod birth_energy;
pub mod birth_forage;
pub mod births;
pub mod deme;
pub mod deme_actions;
pub mod deme_compete;
pub mod deme_events;
pub mod deme_founders;
pub mod deme_mutation;
pub mod driver;
pub mod energy;
pub mod event_list;
pub mod groups;
pub mod habitat;
pub mod hgt;
pub mod host;
pub mod kaboom;
pub mod load_population;
pub mod mating_actions;
pub mod messaging;
pub mod migration_matrix;
pub mod mini_trace;
pub mod movement;
pub mod open_slice;
pub mod parasites;
pub mod pop_demes;
pub mod pop_interface;
pub mod pop_orgs;
pub mod pop_print_actions;
pub mod population;
pub mod population_cell;
pub mod precalc;
pub mod pred_prey;
pub mod pred_prey_actions;
pub mod reputation;
pub mod run;
pub mod run_actions;
pub mod stat_tools;
pub mod stats;
pub mod stats_data;
pub mod stats_dispatch;
pub mod stats_print;
pub mod topology;
pub mod world;
