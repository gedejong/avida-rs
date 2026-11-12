//! avida-tui: a terminal front end for the native Avida port.
//!
//! Modules, bottom up:
//!
//! * [`color`], [`history`], [`log`], [`metrics`]: small pure helpers;
//! * [`app`] and [`input`]: the state and what keys and mouse do to it,
//!   with no terminal I/O (commands for the session and requests for the
//!   shell are queued and taken by the runner);
//! * [`ui`]: drawing functions that take a `&mut ratatui::Frame` and the
//!   [`app::App`], so a `TestBackend` can render any screen;
//! * [`runner`]: the [`runner::Driver`] that joins an `avida_session::Session` to
//!   the app, the event-source trait and the loop;
//! * [`cli`]: the command line.

pub mod abundance;
pub mod actions;
pub mod app;
pub mod cli;
pub mod color;
pub mod config_view;
pub mod debugger;
mod features;
#[cfg(test)]
pub(crate) mod fixtures;
pub mod history;
pub mod input;
pub mod log;
pub mod metrics;
pub mod runner;
pub mod ui;
