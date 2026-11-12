//! avida-gui: the desktop front end of the native Avida port (eframe/egui).
//!
//! The crate is split in two halves:
//!
//! * [`model`]: plain application state and the logic that changes it
//!   (applying the frames, rows and events a [`avida_session::Session`]
//!   publishes, selection, map settings and viewport maths, chart series,
//!   formatting, CSV and PNG export). Nothing in it needs a window, so it
//!   is unit-tested directly.
//! * [`ui`]: the widgets that draw that state and turn input into calls on
//!   it. [`app::AvidaApp`] ties both to a session and to eframe.
//!
//! All presentation maths (palettes, legends, histograms, decimation, RGBA
//! rendering) comes from `avida-viewmodel`; this crate only arranges it.
//! The simulation runs on the session's own thread: the UI thread never
//! waits on it, it only drains what the thread has published since the
//! last repaint (the session's wake callback requests that repaint).

pub mod app;
pub mod cli;
pub mod launch;
pub mod model;
pub mod settings;
pub mod ui;

pub use app::AvidaApp;
pub use cli::StartupArgs;
