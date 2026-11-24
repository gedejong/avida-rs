//! Widgets. Each module draws one part of the window from an
//! [`AppState`](crate::model::AppState) and turns input into calls on it.

pub mod charts;
pub mod command_palette;
pub mod compare;
pub mod config;
mod ease;
pub mod environment;
pub mod filter_bar;
pub mod genotypes;
pub mod histogram;
pub mod inspector;
mod ladder;
pub mod legend;
pub mod log_panel;
pub mod map;
mod open_ended;
pub mod overlays;
pub mod playback;
pub mod reuse;
pub mod scatter;
pub mod side;
pub mod start;
pub mod stats;
pub mod theme;
pub mod top_bar;
pub mod widgets;
