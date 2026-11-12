//! Virtual dispatch over `cSpatialResCount` and its subclass `cGradientCount`.
//!
//! `cResourceCount` holds `cSpatialResCount*` that may point to a
//! `cGradientCount`; the gradient overrides `UpdateCount`, `StateAll`,
//! `ResetGradRes`, the `SetGrad*` setters and a few getters, all of which
//! are no-ops (or return `-1`/`NULL`) on the base class. This enum is that
//! dispatch; everything non-virtual is reached through
//! [`base`](SpatialResource::base), and the gradient-only API through
//! [`gradient`](SpatialResource::gradient).

use avida_apto::AvidaRng;

use crate::env::ResourceEnv;
use crate::gradient_count::GradientCount;
use crate::spatial_res_count::SpatialResCount;

/// A `cSpatialResCount*` as stored in `cResourceCount`.
#[derive(Clone, Debug, PartialEq)]
pub enum SpatialResource {
    /// A plain `cSpatialResCount` (also used, unused, by global resources).
    Plain(SpatialResCount),
    /// A `cGradientCount`.
    Gradient(Box<GradientCount>),
}

impl Default for SpatialResource {
    /// `new cSpatialResCount()`
    fn default() -> Self {
        SpatialResource::Plain(SpatialResCount::new())
    }
}

impl SpatialResource {
    /// The `cSpatialResCount` part.
    pub fn base(&self) -> &SpatialResCount {
        match self {
            SpatialResource::Plain(s) => s,
            SpatialResource::Gradient(g) => &g.base,
        }
    }

    /// The `cSpatialResCount` part, mutable.
    pub fn base_mut(&mut self) -> &mut SpatialResCount {
        match self {
            SpatialResource::Plain(s) => s,
            SpatialResource::Gradient(g) => &mut g.base,
        }
    }

    /// The gradient, if this is one (the C++ callers `dynamic_cast` or rely
    /// on the base no-ops).
    pub fn gradient(&self) -> Option<&GradientCount> {
        match self {
            SpatialResource::Plain(_) => None,
            SpatialResource::Gradient(g) => Some(g),
        }
    }

    /// The gradient, mutable.
    pub fn gradient_mut(&mut self) -> Option<&mut GradientCount> {
        match self {
            SpatialResource::Plain(_) => None,
            SpatialResource::Gradient(g) => Some(g),
        }
    }

    /// `virtual void StateAll()`: folds all deltas for a plain grid; a
    /// no-op for a gradient.
    pub fn state_all(&mut self) {
        match self {
            SpatialResource::Plain(s) => s.state_all(),
            SpatialResource::Gradient(g) => g.state_all(),
        }
    }

    /// `virtual void UpdateCount(cAvidaContext&)`: a no-op for a plain grid.
    pub fn update_count(&mut self, rng: &mut AvidaRng, env: &mut dyn ResourceEnv) {
        if let SpatialResource::Gradient(g) = self {
            g.update_count(rng, env);
        }
    }

    /// `virtual void ResetGradRes(cAvidaContext&, int, int)`: a no-op for a
    /// plain grid.
    pub fn reset_grad_res(
        &mut self,
        rng: &mut AvidaRng,
        env: &mut dyn ResourceEnv,
        worldx: i32,
        worldy: i32,
    ) {
        if let SpatialResource::Gradient(g) = self {
            g.reset_grad_res(rng, env, worldx, worldy);
        }
    }

    /// `virtual Apto::Array<int>* GetWallCells()`: `NULL` for a plain grid.
    pub fn wall_cells(&self) -> Option<&[i32]> {
        self.gradient().map(GradientCount::wall_cells)
    }

    /// `virtual int GetMinUsedX()`: -1 for a plain grid.
    pub fn min_used_x(&self) -> i32 {
        self.gradient().map_or(-1, GradientCount::min_used_x)
    }

    /// `virtual int GetMinUsedY()`: -1 for a plain grid.
    pub fn min_used_y(&self) -> i32 {
        self.gradient().map_or(-1, GradientCount::min_used_y)
    }

    /// `virtual int GetMaxUsedX()`: -1 for a plain grid.
    pub fn max_used_x(&self) -> i32 {
        self.gradient().map_or(-1, GradientCount::max_used_x)
    }

    /// `virtual int GetMaxUsedY()`: -1 for a plain grid.
    pub fn max_used_y(&self) -> i32 {
        self.gradient().map_or(-1, GradientCount::max_used_y)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::env::RecordingEnv;

    #[test]
    fn plain_grid_virtuals_are_base_class_no_ops() {
        let mut s = SpatialResource::Plain(SpatialResCount::with_size(3, 3, crate::geometry::GRID));
        let mut rng = AvidaRng::new(4);
        let mut env = RecordingEnv::new(1, true);
        s.update_count(&mut rng, &mut env);
        s.reset_grad_res(&mut rng, &mut env, 5, 5);
        assert_eq!(s.base().size(), 9);
        assert!(s.gradient().is_none());
        assert_eq!(s.wall_cells(), None);
        assert_eq!(
            (
                s.min_used_x(),
                s.min_used_y(),
                s.max_used_x(),
                s.max_used_y()
            ),
            (-1, -1, -1, -1)
        );
        assert_eq!(rng.get_uint(1000), AvidaRng::new(4).get_uint(1000));
    }
}
