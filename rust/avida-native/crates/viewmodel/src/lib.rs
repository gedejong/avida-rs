//! avida-viewmodel: the UI-toolkit-free layer between a running
//! [`World`](avida_world::world::World) and the desktop (egui) and terminal
//! (ratatui) front ends.
//!
//! [`capture()`] copies what a [`Subscription`] asks for out of a `&World`
//! into a [`Frame`] of plain owned data: the population statistics, a map
//! of per-cell layers (with where each genotype lives), the most abundant
//! genotypes, one genotype's ancestry, one organism's phenotype and CPU
//! state, the instruction names and the environment. [`probe()`] and its
//! siblings read a few numbers about one organism, cheaply enough to call
//! after every time slice. The remaining modules turn
//! frames into things a front end draws, without depending on any toolkit:
//!
//! * [`palette`]: viridis and cividis lookup tables for continuous layers,
//!   Okabe-Ito for the top genotypes (grey for the rest);
//! * [`legend`]: value ranges, linear and log scales, tick marks;
//! * [`render`]: a map layer as an RGBA8 image, one pixel per cell;
//! * [`series`]: histograms of layer values, min/max decimation of time
//!   series;
//! * [`text`]: a plain-text rendering of a frame (snapshot tests, logs).
//!
//! # Determinism
//!
//! Capturing must not change the run. [`capture()`] and [`update_row`] take
//! `&World` and read only through `&self` methods without interior
//! mutability; see [`capture`](mod@capture) for the accessors that are
//! deliberately avoided and what that costs (frozen resource amounts). The
//! crate has no threads and does no file I/O.
//!
//! The C++ counterparts are the viewer drivers in
//! `avida-core/source/viewer/` and the ncurses viewer in
//! `avida-core/source/targets/avida-viewer/`, which read the same
//! quantities through `cPopulation`, `cOrganism`, `cPhenotype` and
//! `cHardwareBase` while the world is paused between updates.

pub mod capture;
pub mod legend;
pub mod model;
pub mod palette;
pub mod problem;
pub mod render;
pub mod series;
pub mod text;

pub use capture::{
    all_layers, capture, capture_with_progress, find_organism, ip_positions, ip_sites,
    newest_organism_since, probe, update_row, LINEAGE_MAX, UNCLASSIFIED,
};
pub use legend::{format_number, Legend, Scale, Tick};
pub use model::{
    mem_flags, CpuView, EnvView, Frame, GenotypeCells, GenotypeRow, GenotypeView, HeadView, Header,
    LayerId, MapFrame, MemCell, MemSpaceView, OrgProbe, OrganismView, PhenotypeView, ReactionView,
    ResourceView, Subscription, TaskView, ThreadView, UpdateProgress, UpdateRow,
};
pub use palette::{Palette, Rgba, CATEGORICAL_SLOTS, EMPTY, OKABE_ITO, OTHER_GREY};
pub use render::{layer_legend, render_rgba, render_rgba_with_legend};
pub use series::{decimate_min_max, series_of, Histogram};
pub use text::to_text;
