//! Runtime resource state: global, spatial and gradient resource amounts.
//!
//! Ports, from `avida-core/source/main/` (reference bodies from git
//! `47f13dadb`, 2025-01-27, the last pre-Rust commit):
//!
//! | C++ | Rust |
//! |---|---|
//! | `nGeometry.h`, `AvidaTools::Mod`/`GridNeighbor` | [`geometry`] |
//! | `cCellResource` (`cResource.h`) | [`CellResource`] |
//! | `cSpatialCountElem` | [`SpatialCountElem`] |
//! | `cSpatialResCount`, `FlowMatter` | [`SpatialResCount`], [`flow_matter`] |
//! | `cGradientCount` | [`GradientCount`] |
//! | `cSpatialResCount*` virtual dispatch | [`SpatialResource`] |
//! | `cResourceCount` | [`ResourceCount`], [`ResourceSetup`] |
//! | `m_world` / `cAvidaContext` services | [`ResourceEnv`] |
//!
//! The static resource definitions (`cResource`, `cResourceLib`) live in the
//! environment crate; the caller copies their values into a
//! [`ResourceSetup`] / [`GradientParams`].
//!
//! Floating point follows the reference build (Apple clang, arm64,
//! `-ffp-contract=on`): every C++ expression of the form `a*b + c`,
//! `c - a*b`, `x -= a*b` is a `mul_add` here, except where the reference
//! binary computes in a Rust helper (the inflow precalc tables and the
//! update-time remainder), which is not fused. All traces in `testdata/` were
//! produced by `tools/ref_harness.cc`, which compiles the original C++ with
//! the reference build's flags on that platform, with those two statements
//! patched to match (`tools/build_ref.sh`).
//!
//! Unsupported paths that fail loudly: none. Every geometry the C++ accepts
//! (`GLOBAL`, `GRID`, `TORUS`, `PARTIAL`) is ported; others return the C++
//! exit message from [`ResourceCount::setup`].

pub mod cell_resource;
pub mod env;
pub mod geometry;
pub mod gradient_count;
pub mod resource_count;
pub mod spatial_count_elem;
pub mod spatial_res_count;
pub mod spatial_resource;

pub use cell_resource::CellResource;
pub use env::{RecordingEnv, ResourceEnv};
pub use gradient_count::{GradientCount, GradientParams};
pub use resource_count::{
    ResourceCount, ResourceSetup, SpatialExecutor, EPSILON, PRECALC_DISTANCE, UPDATE_STEP,
    VERBOSE_NORMAL,
};
pub use spatial_count_elem::SpatialCountElem;
pub use spatial_res_count::{flow_matter, SpatialResCount};
pub use spatial_resource::SpatialResource;

#[cfg(test)]
mod reference_tests;
