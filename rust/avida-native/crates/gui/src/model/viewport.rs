//! Zoom and pan of the population map: the mapping between screen points
//! and cell coordinates.

use egui::{pos2, vec2, Pos2, Rect, Vec2};

/// The map view. Cell `(x, y)` covers the square from `(x, y)` to
/// `(x + 1, y + 1)` in cell coordinates; `center` is the cell coordinate
/// drawn at the centre of the canvas and `zoom` the screen points per cell.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Viewport {
    pub zoom: f32,
    pub center: Vec2,
    /// Refit whenever the canvas or the world changes size, until the user
    /// zooms or pans.
    pub auto_fit: bool,
}

impl Default for Viewport {
    fn default() -> Self {
        Viewport {
            zoom: 8.0,
            center: Vec2::ZERO,
            auto_fit: true,
        }
    }
}

/// Points left free around a fitted world.
pub const FIT_MARGIN: f32 = 16.0;
/// Largest zoom: screen points per cell.
pub const MAX_ZOOM: f32 = 96.0;

impl Viewport {
    /// The zoom at which a `w` x `h` world fills `canvas`.
    pub fn fit_zoom(canvas: Rect, w: usize, h: usize) -> f32 {
        if w == 0 || h == 0 {
            return 1.0;
        }
        let avail = (canvas.size() - Vec2::splat(2.0 * FIT_MARGIN)).max(Vec2::splat(1.0));
        (avail.x / w as f32)
            .min(avail.y / h as f32)
            .clamp(0.01, MAX_ZOOM)
    }

    pub fn min_zoom(canvas: Rect, w: usize, h: usize) -> f32 {
        Viewport::fit_zoom(canvas, w, h) * 0.5
    }

    /// Centres the world and fits it to the canvas.
    pub fn fit(&mut self, canvas: Rect, w: usize, h: usize) {
        self.zoom = Viewport::fit_zoom(canvas, w, h);
        self.center = vec2(w as f32, h as f32) / 2.0;
        self.auto_fit = true;
    }

    /// Fits again if still in auto-fit mode.
    pub fn refit_if_auto(&mut self, canvas: Rect, w: usize, h: usize) {
        if self.auto_fit {
            self.fit(canvas, w, h);
        }
    }

    /// The cell coordinate under a screen point.
    pub fn to_cells(&self, canvas: Rect, p: Pos2) -> Vec2 {
        self.center + (p - canvas.center()) / self.zoom
    }

    /// The screen point of a cell coordinate.
    pub fn to_screen(&self, canvas: Rect, c: Vec2) -> Pos2 {
        canvas.center() + (c - self.center) * self.zoom
    }

    /// The screen rectangle the whole world covers.
    pub fn world_rect(&self, canvas: Rect, w: usize, h: usize) -> Rect {
        Rect::from_min_max(
            self.to_screen(canvas, Vec2::ZERO),
            self.to_screen(canvas, vec2(w as f32, h as f32)),
        )
    }

    /// The screen rectangle of cell `(x, y)`.
    pub fn cell_rect(&self, canvas: Rect, x: usize, y: usize) -> Rect {
        let min = self.to_screen(canvas, vec2(x as f32, y as f32));
        Rect::from_min_size(min, Vec2::splat(self.zoom))
    }

    /// The cell id under a screen point, if inside the world.
    pub fn cell_at(&self, canvas: Rect, w: usize, h: usize, p: Pos2) -> Option<usize> {
        let c = self.to_cells(canvas, p);
        if c.x < 0.0 || c.y < 0.0 {
            return None;
        }
        let (x, y) = (c.x.floor() as usize, c.y.floor() as usize);
        (x < w && y < h).then_some(y * w + x)
    }

    /// Zooms by `factor`, keeping the cell under `anchor` in place.
    pub fn zoom_at(&mut self, canvas: Rect, w: usize, h: usize, anchor: Pos2, factor: f32) {
        if !(factor.is_finite() && factor > 0.0) || (factor - 1.0).abs() < 1e-6 {
            return;
        }
        let under = self.to_cells(canvas, anchor);
        let zoom = (self.zoom * factor).clamp(Viewport::min_zoom(canvas, w, h), MAX_ZOOM);
        self.zoom = zoom;
        self.center = under - (anchor - canvas.center()) / zoom;
        self.auto_fit = false;
        self.clamp(canvas, w, h);
    }

    /// Moves the view by a screen-space drag.
    pub fn pan(&mut self, canvas: Rect, w: usize, h: usize, drag: Vec2) {
        if drag == Vec2::ZERO {
            return;
        }
        self.center -= drag / self.zoom;
        self.auto_fit = false;
        self.clamp(canvas, w, h);
    }

    /// Keeps the world on screen. Along an axis where the world is wider
    /// than the canvas, its edges stop at the fit margin, so the view never
    /// shows empty space past a corner; along an axis where it fits, it is
    /// centred.
    pub fn clamp(&mut self, canvas: Rect, w: usize, h: usize) {
        let axis = |c: f32, size: usize, extent: f32| {
            let size = size as f32;
            let half = extent / 2.0 / self.zoom;
            let margin = FIT_MARGIN / self.zoom;
            if size + 2.0 * margin <= 2.0 * half + 1e-3 {
                size / 2.0
            } else {
                c.clamp(half - margin, size - half + margin)
            }
        };
        self.center = vec2(
            axis(self.center.x, w, canvas.width()),
            axis(self.center.y, h, canvas.height()),
        );
    }

    /// Zoom relative to fitting, for display (`1.0` = fitted).
    pub fn relative_zoom(&self, canvas: Rect, w: usize, h: usize) -> f32 {
        self.zoom / Viewport::fit_zoom(canvas, w, h)
    }
}

/// `pos2` re-export for tests and callers that build canvases.
pub fn canvas(x0: f32, y0: f32, x1: f32, y1: f32) -> Rect {
    Rect::from_min_max(pos2(x0, y0), pos2(x1, y1))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fitting_centres_the_world_inside_the_margin() {
        let c = canvas(0.0, 0.0, 632.0, 432.0);
        let mut v = Viewport::default();
        v.fit(c, 60, 40);
        assert_eq!(v.zoom, 10.0);
        let r = v.world_rect(c, 60, 40);
        assert_eq!(r.center(), c.center());
        assert_eq!(r.width(), 600.0);
        assert!(v.auto_fit);
    }

    #[test]
    fn screen_points_map_to_cells_and_back() {
        let c = canvas(100.0, 50.0, 732.0, 482.0);
        let mut v = Viewport::default();
        v.fit(c, 60, 40);
        let r = v.cell_rect(c, 5, 7);
        assert_eq!(v.cell_at(c, 60, 40, r.center()), Some(7 * 60 + 5));
        assert_eq!(v.cell_at(c, 60, 40, pos2(101.0, 51.0)), None, "margin");
        assert_eq!(v.cell_at(c, 60, 40, r.min), Some(7 * 60 + 5));
    }

    #[test]
    fn zooming_keeps_the_anchor_cell_under_the_pointer() {
        let c = canvas(0.0, 0.0, 632.0, 432.0);
        let mut v = Viewport::default();
        v.fit(c, 60, 40);
        let anchor = pos2(200.0, 300.0);
        let before = v.to_cells(c, anchor);
        v.zoom_at(c, 60, 40, anchor, 2.5);
        let after = v.to_cells(c, anchor);
        assert!((before - after).length() < 1e-3, "{before:?} {after:?}");
        assert_eq!(v.zoom, 25.0);
        assert!(!v.auto_fit);
        v.zoom_at(c, 60, 40, anchor, 1e9);
        assert_eq!(v.zoom, MAX_ZOOM);
        v.zoom_at(c, 60, 40, anchor, 1e-9);
        assert_eq!(v.zoom, Viewport::min_zoom(c, 60, 40));
    }

    #[test]
    fn panning_moves_and_stops_at_the_world_edges() {
        let c = canvas(0.0, 0.0, 632.0, 432.0);
        let mut v = Viewport::default();
        v.fit(c, 60, 40);
        v.pan(c, 60, 40, vec2(-100.0, 0.0));
        assert_eq!(v.center, vec2(30.0, 20.0), "a fitted world stays centred");
        v.zoom = 20.0;
        v.pan(c, 60, 40, vec2(-100.0, 0.0));
        assert_eq!(v.center, vec2(35.0, 20.0));
        v.pan(c, 60, 40, vec2(-1e6, 1e6));
        // The top-right corner stops 16 points inside the canvas.
        assert!(
            (v.center - vec2(45.0, 10.0)).length() < 1e-3,
            "{:?}",
            v.center
        );
        let world = v.world_rect(c, 60, 40);
        assert!(
            (world.max.x - (c.max.x - FIT_MARGIN)).abs() < 1e-3,
            "{world:?}"
        );
        assert!((world.min.y - FIT_MARGIN).abs() < 1e-3, "{world:?}");
        v.refit_if_auto(c, 60, 40);
        assert_eq!(v.zoom, 20.0, "panning ends auto-fit");
    }

    #[test]
    fn an_empty_world_does_not_divide_by_zero() {
        let c = canvas(0.0, 0.0, 100.0, 100.0);
        let mut v = Viewport::default();
        v.fit(c, 0, 0);
        assert!(v.zoom.is_finite());
        assert_eq!(v.cell_at(c, 0, 0, c.center()), None);
    }
}
