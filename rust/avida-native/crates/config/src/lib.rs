//! avida-config
// Transliterated C++ loops index several arrays by one counter.
#![allow(clippy::needless_range_loop, clippy::collapsible_if)]
pub mod cmdline;
pub mod config;
mod config_table;
pub mod convert;
pub mod experiment;
pub mod feedback;
pub mod init_file;
pub mod naming;
pub mod port_settings;
pub mod reference;
pub mod string_util;
pub mod yaml;

pub use config::Config;
pub use feedback::{Feedback, FeedbackKind};
pub use init_file::InitFile;
