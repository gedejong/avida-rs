//! Grid of a spatial (or, unused, global) resource.
//!
//! Ports `cSpatialResCount` (`avida-core/source/main/cSpatialResCount.{h,cc}`,
//! reference bodies from git `47f13dadb`) and the free function `FlowMatter`
//! from `cResourceCount.cc`.

use crate::cell_resource::CellResource;
use crate::geometry::{self, grid_neighbor, modulo};
use crate::spatial_count_elem::SpatialCountElem;

/// Sentinel used for unset inflow/outflow rectangle corners and for masked
/// neighbour links on bounded grids (`cResource::NONE`, -99).
pub const NONE: i32 = -99;

/// Value `GetAmount` returns for an out-of-range cell: `cResource::NONE`
/// converted to `double` (it was `-99.9` before `47f13dadb`).
pub const OUT_OF_RANGE_AMOUNT: f64 = NONE as f64;

/// `Apto::Max(v1, v2)` (`Algorithms.h`): `(v1 < v2) ? v2 : v1`. Differs from
/// `f64::max` for NaN (keeps NaN) and signed zeros (keeps `v1`).
#[inline]
pub(crate) fn apto_max(v1: f64, v2: f64) -> f64 {
    if v1 < v2 {
        v2
    } else {
        v1
    }
}

/// `FlowMatter(cSpatialCountElem&, cSpatialCountElem&, ...)` in
/// `cResourceCount.cc`: the amount that flows from element 1 to element 2
/// (element 1's delta decreases by it, element 2's increases). `None` is the
/// C++ early return (both empty and a masked link).
///
/// No expression here is a multiply directly feeding an add, so nothing is
/// fused by the reference build.
#[allow(clippy::too_many_arguments)]
pub fn flow_matter(
    elem1_amount: f64,
    elem2_amount: f64,
    inxdiffuse: f64,
    inydiffuse: f64,
    inxgravity: f64,
    inygravity: f64,
    xdist: i32,
    ydist: i32,
    dist: f64,
) -> Option<f64> {
    if (elem1_amount == 0.0 && elem2_amount == 0.0) && dist < 0.0 {
        return None;
    }
    let diff = elem1_amount - elem2_amount;
    let (xdiffuse, xgravity) = if xdist != 0 {
        let xgravity = if (xdist > 0 && inxgravity > 0.0) || (xdist < 0 && inxgravity < 0.0) {
            elem1_amount * inxgravity.abs() / 3.0
        } else {
            -elem2_amount * inxgravity.abs() / 3.0
        };
        (inxdiffuse * diff / 16.0, xgravity)
    } else {
        (0.0, 0.0)
    };
    let (ydiffuse, ygravity) = if ydist != 0 {
        let ygravity = if (ydist > 0 && inygravity > 0.0) || (ydist < 0 && inygravity < 0.0) {
            elem1_amount * inygravity.abs() / 3.0
        } else {
            -elem2_amount * inygravity.abs() / 3.0
        };
        (inydiffuse * diff / 16.0, ygravity)
    } else {
        (0.0, 0.0)
    };
    Some(
        ((xdiffuse + ydiffuse + xgravity + ygravity)
            / ((f64::from(xdist) * 1.0).abs() + (f64::from(ydist) * 1.0).abs()))
            / dist,
    )
}

/// `cSpatialResCount`: the per-cell amounts of one resource plus its flow
/// parameters (diffusion, gravity, inflow/outflow rectangles, `CELL` list).
///
/// The C++ keeps a pointer to the owning `cResource`'s cell list; this port
/// keeps a copy, set by [`set_cell_list`](Self::set_cell_list).
#[derive(Clone, Debug, PartialEq)]
pub struct SpatialResCount {
    pub(crate) grid: Vec<SpatialCountElem>,
    pub(crate) initial: f64,
    pub(crate) xdiffuse: f64,
    pub(crate) ydiffuse: f64,
    pub(crate) xgravity: f64,
    pub(crate) ygravity: f64,
    pub(crate) inflow_x1: i32,
    pub(crate) inflow_x2: i32,
    pub(crate) inflow_y1: i32,
    pub(crate) inflow_y2: i32,
    pub(crate) outflow_x1: i32,
    pub(crate) outflow_x2: i32,
    pub(crate) outflow_y1: i32,
    pub(crate) outflow_y2: i32,
    pub(crate) geometry: i32,
    pub(crate) world_x: i32,
    pub(crate) world_y: i32,
    pub(crate) num_cells: i32,
    pub(crate) curr_peakx: i32,
    pub(crate) curr_peaky: i32,
    pub(crate) cell_list: Vec<CellResource>,
    pub(crate) modified: bool,
}

impl Default for SpatialResCount {
    fn default() -> Self {
        Self::new()
    }
}

impl SpatialResCount {
    /// `cSpatialResCount()`: empty grid, diffusion 1, gravity 0, `GLOBAL`.
    ///
    /// Members the C++ leaves uninitialised (rectangles, world size, peak,
    /// cell list pointer) start at zero / empty.
    pub fn new() -> Self {
        SpatialResCount {
            grid: Vec::new(),
            initial: 0.0,
            xdiffuse: 1.0,
            ydiffuse: 1.0,
            xgravity: 0.0,
            ygravity: 0.0,
            inflow_x1: 0,
            inflow_x2: 0,
            inflow_y1: 0,
            inflow_y2: 0,
            outflow_x1: 0,
            outflow_x2: 0,
            outflow_y1: 0,
            outflow_y2: 0,
            geometry: geometry::GLOBAL,
            world_x: 0,
            world_y: 0,
            num_cells: 0,
            curr_peakx: 0,
            curr_peaky: 0,
            cell_list: Vec::new(),
            modified: false,
        }
    }

    /// `cSpatialResCount(int inworld_x, int inworld_y, int ingeometry)`:
    /// default flows (diffusion 1, gravity 0).
    pub fn with_size(inworld_x: i32, inworld_y: i32, ingeometry: i32) -> Self {
        Self::with_flows(inworld_x, inworld_y, ingeometry, 1.0, 1.0, 0.0, 0.0)
    }

    /// `cSpatialResCount(x, y, geometry, xdiffuse, ydiffuse, xgravity, ygravity)`
    pub fn with_flows(
        inworld_x: i32,
        inworld_y: i32,
        ingeometry: i32,
        inxdiffuse: f64,
        inydiffuse: f64,
        inxgravity: f64,
        inygravity: f64,
    ) -> Self {
        let mut s = Self::new();
        s.xdiffuse = inxdiffuse;
        s.ydiffuse = inydiffuse;
        s.xgravity = inxgravity;
        s.ygravity = inygravity;
        s.resize_clear(inworld_x, inworld_y, ingeometry);
        s
    }

    /// `ResizeClear(int, int, int)`: fresh zeroed grid of `x * y` cells,
    /// then neighbour links rebuilt.
    pub fn resize_clear(&mut self, inworld_x: i32, inworld_y: i32, ingeometry: i32) {
        let n = inworld_x.wrapping_mul(inworld_y).max(0) as usize;
        self.grid = vec![SpatialCountElem::default(); n];
        self.world_x = inworld_x;
        self.world_y = inworld_y;
        self.geometry = ingeometry;
        self.num_cells = inworld_x.wrapping_mul(inworld_y);
        self.set_pointers();
    }

    /// `SetPointers()`: link every cell to its 8 neighbours (pointer 0 is the
    /// upper-left cell, then clockwise), torus-wrapped, with distance 1 or
    /// sqrt(2); on a bounded `GRID` the links that leave the world are set
    /// to [`NONE`].
    pub fn set_pointers(&mut self) {
        let sqrt2 = 2.0_f64.sqrt();
        let (wx, wy) = (self.world_x, self.world_y);
        for i in 0..self.grid.len() {
            let c = i as i32;
            let e = &mut self.grid[i];
            e.set_ptr(0, grid_neighbor(c, wx, wy, -1, -1), -1, -1, sqrt2);
            e.set_ptr(1, grid_neighbor(c, wx, wy, 0, -1), 0, -1, 1.0);
            e.set_ptr(2, grid_neighbor(c, wx, wy, 1, -1), 1, -1, sqrt2);
            e.set_ptr(3, grid_neighbor(c, wx, wy, 1, 0), 1, 0, 1.0);
            e.set_ptr(4, grid_neighbor(c, wx, wy, 1, 1), 1, 1, sqrt2);
            e.set_ptr(5, grid_neighbor(c, wx, wy, 0, 1), 0, 1, 1.0);
            e.set_ptr(6, grid_neighbor(c, wx, wy, -1, 1), -1, 1, sqrt2);
            e.set_ptr(7, grid_neighbor(c, wx, wy, -1, 0), -1, 0, 1.0);
        }

        // The C++ indexes the edge rows/columns unconditionally; on an empty
        // grid that is out of bounds, so it is skipped here.
        if self.geometry == geometry::GRID && !self.grid.is_empty() {
            let none = f64::from(NONE);
            for i in 0..wx {
                let top = &mut self.grid[i as usize];
                top.set_ptr(0, NONE, NONE, NONE, none);
                top.set_ptr(1, NONE, NONE, NONE, none);
                top.set_ptr(2, NONE, NONE, NONE, none);
                let ii = (self.num_cells - 1 - i) as usize;
                let bottom = &mut self.grid[ii];
                bottom.set_ptr(4, NONE, NONE, NONE, none);
                bottom.set_ptr(5, NONE, NONE, NONE, none);
                bottom.set_ptr(6, NONE, NONE, NONE, none);
            }
            for i in 0..wy {
                let left = &mut self.grid[(i * wx) as usize];
                left.set_ptr(0, NONE, NONE, NONE, none);
                left.set_ptr(7, NONE, NONE, NONE, none);
                left.set_ptr(6, NONE, NONE, NONE, none);
                let right = &mut self.grid[((i + 1) * wx - 1) as usize];
                right.set_ptr(2, NONE, NONE, NONE, none);
                right.set_ptr(3, NONE, NONE, NONE, none);
                right.set_ptr(4, NONE, NONE, NONE, none);
            }
        }
    }

    /// `CheckRanges()`: clamp the inflow/outflow rectangles into the world
    /// and unwrap rectangles that cross the zero boundary. (Never called by
    /// the C++ resource code; ported for completeness.)
    pub fn check_ranges(&mut self) {
        let (wx, wy) = (self.world_x, self.world_y);
        let clamp = |v: &mut i32, bound: i32| {
            if *v < 0 {
                *v = 0;
            } else if *v > bound {
                *v = bound;
            }
        };
        clamp(&mut self.inflow_x1, wx);
        clamp(&mut self.inflow_x2, wx);
        clamp(&mut self.inflow_y1, wy);
        clamp(&mut self.inflow_y2, wy);
        if self.inflow_x2 < self.inflow_x1 {
            self.inflow_x2 += wx;
        }
        if self.inflow_y2 < self.inflow_y1 {
            self.inflow_y2 += wy;
        }
        clamp(&mut self.outflow_x1, wx);
        clamp(&mut self.outflow_x2, wx);
        clamp(&mut self.outflow_y1, wy);
        clamp(&mut self.outflow_y2, wy);
        if self.outflow_x2 < self.outflow_x1 {
            self.outflow_x2 += wx;
        }
        if self.outflow_y2 < self.outflow_y1 {
            self.outflow_y2 += wy;
        }
    }

    /// `SetCellList(Apto::Array<cCellResource>*)`: remember the `CELL` list
    /// and add each listed cell's initial amount to it.
    ///
    /// QUIRK: the C++ accepts `cell_id <= grid size` here (elsewhere `<`). For
    /// `cell_id == size` the `Rate`/`State` calls are no-ops and the C++ then
    /// writes the cell's initial one element past the end of the grid; that
    /// write is skipped here.
    pub fn set_cell_list(&mut self, cell_list: &[CellResource]) {
        self.cell_list = cell_list.to_vec();
        let size = self.grid.len() as i32;
        for c in cell_list {
            let cell_id = c.cell_id;
            if cell_id >= 0 && cell_id <= size {
                self.rate(cell_id, c.initial);
                self.state(cell_id);
                if let Some(e) = self.grid.get_mut(cell_id as usize) {
                    e.set_initial(c.initial);
                }
            }
        }
    }

    /// The `CELL` list remembered by [`set_cell_list`](Self::set_cell_list).
    pub fn cell_list(&self) -> &[CellResource] {
        &self.cell_list
    }

    /// `GetSize()`: number of grid cells.
    #[inline]
    pub fn size(&self) -> i32 {
        self.grid.len() as i32
    }

    /// `GetX()`
    #[inline]
    pub fn x(&self) -> i32 {
        self.world_x
    }

    /// `GetY()`
    #[inline]
    pub fn y(&self) -> i32 {
        self.world_y
    }

    /// `GetCellListSize()`
    #[inline]
    pub fn cell_list_size(&self) -> i32 {
        self.cell_list.len() as i32
    }

    /// `Element(int)`. Panics for an index outside the grid (C++: undefined).
    #[inline]
    pub fn element(&self, x: i32) -> &SpatialCountElem {
        &self.grid[x as usize]
    }

    /// `Element(int)`, mutable.
    #[inline]
    pub fn element_mut(&mut self, x: i32) -> &mut SpatialCountElem {
        &mut self.grid[x as usize]
    }

    /// All grid cells in cell-id order.
    pub fn elements(&self) -> &[SpatialCountElem] {
        &self.grid
    }

    #[inline]
    fn valid(&self, x: i32) -> bool {
        x >= 0 && (x as usize) < self.grid.len()
    }

    /// `Rate(int x, double)`: add to a cell's pending change. Out-of-range ids
    /// are ignored (the C++ only asserts).
    #[inline]
    pub fn rate(&mut self, x: i32, ratein: f64) {
        if self.valid(x) {
            self.grid[x as usize].rate(ratein);
        }
    }

    /// `Rate(int x, int y, double)`
    pub fn rate_xy(&mut self, x: i32, y: i32, ratein: f64) {
        if x >= 0 && x < self.world_x && y >= 0 && y < self.world_y {
            self.grid[(y * self.world_x + x) as usize].rate(ratein);
        }
    }

    /// `State(int x)`: fold one cell's pending change.
    #[inline]
    pub fn state(&mut self, x: i32) {
        if self.valid(x) {
            self.grid[x as usize].state();
        }
    }

    /// `State(int x, int y)`
    pub fn state_xy(&mut self, x: i32, y: i32) {
        if x >= 0 && x < self.world_x && y >= 0 && y < self.world_y {
            self.grid[(y * self.world_x + x) as usize].state();
        }
    }

    /// `GetAmount(int x)`; [`OUT_OF_RANGE_AMOUNT`] for an invalid id.
    #[inline]
    pub fn get_amount(&self, x: i32) -> f64 {
        if self.valid(x) {
            self.grid[x as usize].amount()
        } else {
            OUT_OF_RANGE_AMOUNT
        }
    }

    /// `GetAmount(int x, int y)`
    pub fn get_amount_xy(&self, x: i32, y: i32) -> f64 {
        if x >= 0 && x < self.world_x && y >= 0 && y < self.world_y {
            self.grid[(y * self.world_x + x) as usize].amount()
        } else {
            OUT_OF_RANGE_AMOUNT
        }
    }

    #[inline]
    fn num_cells_usize(&self) -> usize {
        // `num_cells` equals the grid size after every ResizeClear.
        (self.num_cells.max(0) as usize).min(self.grid.len())
    }

    /// `RateAll(double)`: add to every cell's pending change.
    pub fn rate_all(&mut self, ratein: f64) {
        let n = self.num_cells_usize();
        for e in &mut self.grid[..n] {
            e.rate(ratein);
        }
    }

    /// `cSpatialResCount::StateAll()` (the base-class body): fold every
    /// cell's pending change. `cGradientCount` overrides it with a no-op; see
    /// [`SpatialResource::state_all`](crate::SpatialResource::state_all).
    pub fn state_all(&mut self) {
        let n = self.num_cells_usize();
        for e in &mut self.grid[..n] {
            e.state();
        }
    }

    /// `FlowAll()`: diffusion and gravity between each cell and its
    /// neighbours 3..=6 (each pair once), accumulated into the deltas.
    pub fn flow_all(&mut self) {
        if self.xdiffuse == 0.0
            && self.ydiffuse == 0.0
            && self.xgravity == 0.0
            && self.ygravity == 0.0
        {
            return;
        }
        let n = self.num_cells_usize();
        for i in 0..n {
            for k in 3..=6 {
                let e = &self.grid[i];
                let ii = e.elem_ptr(k);
                let xdist = e.ptr_xdist(k);
                let ydist = e.ptr_ydist(k);
                let dist = e.ptr_dist(k);
                if ii >= 0 {
                    let ii = ii as usize;
                    if let Some(flowamt) = flow_matter(
                        self.grid[i].amount,
                        self.grid[ii].amount,
                        self.xdiffuse,
                        self.ydiffuse,
                        self.xgravity,
                        self.ygravity,
                        xdist,
                        ydist,
                        dist,
                    ) {
                        // Sequential, so a cell linked to itself (1-wide
                        // torus) sees both updates like in the C++.
                        self.grid[i].delta -= flowamt;
                        self.grid[ii].delta += flowamt;
                    }
                }
            }
        }
    }

    /// `SumAll()`: total amount over all cells.
    pub fn sum_all(&self) -> f64 {
        let mut sum = 0.0;
        for i in 0..self.num_cells.max(0) {
            sum += self.get_amount(i);
        }
        sum
    }

    /// `Source(double)`: spread `amount` evenly over the inflow rectangle
    /// (coordinates wrap around the world).
    pub fn source(&mut self, amount: f64) {
        let totalcells = f64::from(
            (self.inflow_y2 - self.inflow_y1 + 1).wrapping_mul(self.inflow_x2 - self.inflow_x1 + 1),
        ) * 1.0;
        let amount = amount / totalcells;
        for i in self.inflow_y1..=self.inflow_y2 {
            for j in self.inflow_x1..=self.inflow_x2 {
                let elem = modulo(i, self.world_y) * self.world_x + modulo(j, self.world_x);
                self.rate(elem, amount);
            }
        }
    }

    /// `CellInflow()`: each listed cell's inflow.
    pub fn cell_inflow(&mut self) {
        for k in 0..self.cell_list.len() {
            let c = self.cell_list[k];
            if self.valid(c.cell_id) {
                self.rate(c.cell_id, c.inflow);
            }
        }
    }

    /// `Sink(double decay)`: remove `1 - decay` of the amount from each cell
    /// of the outflow rectangle; nothing when any corner is [`NONE`].
    pub fn sink(&mut self, decay: f64) {
        if self.outflow_x1 == NONE
            || self.outflow_y1 == NONE
            || self.outflow_x2 == NONE
            || self.outflow_y2 == NONE
        {
            return;
        }
        for i in self.outflow_y1..=self.outflow_y2 {
            for j in self.outflow_x1..=self.outflow_x2 {
                let elem = modulo(i, self.world_y) * self.world_x + modulo(j, self.world_x);
                let deltaamount = apto_max(self.get_amount(elem) * (1.0 - decay), 0.0);
                self.rate(elem, -deltaamount);
            }
        }
    }

    /// `CellOutflow()`: each listed cell loses `outflow` of its amount.
    ///
    /// As in the C++, `deltaamount` carries over from the previous list entry
    /// when a cell id is invalid (the `Rate` call is then a no-op anyway).
    pub fn cell_outflow(&mut self) {
        let mut deltaamount = 0.0;
        for k in 0..self.cell_list.len() {
            let c = self.cell_list[k];
            if self.valid(c.cell_id) {
                deltaamount = apto_max(self.get_amount(c.cell_id) * c.outflow, 0.0);
            }
            self.rate(c.cell_id, -deltaamount);
        }
    }

    /// `SetCellAmount(int, double)`; ignored for an invalid id.
    pub fn set_cell_amount(&mut self, cell_id: i32, res: f64) {
        if self.valid(cell_id) {
            self.grid[cell_id as usize].set_amount(res);
        }
    }

    /// `SetInitial(double)`: the per-cell share of the resource's initial.
    pub fn set_initial(&mut self, initial: f64) {
        self.initial = initial;
    }

    /// `GetInitial()`
    pub fn initial(&self) -> f64 {
        self.initial
    }

    /// `SetGeometry(int)`
    pub fn set_geometry(&mut self, in_geometry: i32) {
        self.geometry = in_geometry;
    }

    /// The geometry the grid links were built for.
    pub fn geometry(&self) -> i32 {
        self.geometry
    }

    /// `SetXdiffuse(double)`
    pub fn set_xdiffuse(&mut self, v: f64) {
        self.xdiffuse = v;
    }

    /// `SetXgravity(double)`
    pub fn set_xgravity(&mut self, v: f64) {
        self.xgravity = v;
    }

    /// `SetYdiffuse(double)`
    pub fn set_ydiffuse(&mut self, v: f64) {
        self.ydiffuse = v;
    }

    /// `SetYgravity(double)`
    pub fn set_ygravity(&mut self, v: f64) {
        self.ygravity = v;
    }

    /// `(xdiffuse, ydiffuse, xgravity, ygravity)`
    pub fn flows(&self) -> (f64, f64, f64, f64) {
        (self.xdiffuse, self.ydiffuse, self.xgravity, self.ygravity)
    }

    /// `SetInflowX1(int)`
    pub fn set_inflow_x1(&mut self, v: i32) {
        self.inflow_x1 = v;
    }

    /// `SetInflowX2(int)`
    pub fn set_inflow_x2(&mut self, v: i32) {
        self.inflow_x2 = v;
    }

    /// `SetInflowY1(int)`
    pub fn set_inflow_y1(&mut self, v: i32) {
        self.inflow_y1 = v;
    }

    /// `SetInflowY2(int)`
    pub fn set_inflow_y2(&mut self, v: i32) {
        self.inflow_y2 = v;
    }

    /// `SetOutflowX1(int)`
    pub fn set_outflow_x1(&mut self, v: i32) {
        self.outflow_x1 = v;
    }

    /// `SetOutflowX2(int)`
    pub fn set_outflow_x2(&mut self, v: i32) {
        self.outflow_x2 = v;
    }

    /// `SetOutflowY1(int)`
    pub fn set_outflow_y1(&mut self, v: i32) {
        self.outflow_y1 = v;
    }

    /// `SetOutflowY2(int)`
    pub fn set_outflow_y2(&mut self, v: i32) {
        self.outflow_y2 = v;
    }

    /// Inflow rectangle `(x1, x2, y1, y2)`.
    pub fn inflow_box(&self) -> (i32, i32, i32, i32) {
        (
            self.inflow_x1,
            self.inflow_x2,
            self.inflow_y1,
            self.inflow_y2,
        )
    }

    /// Outflow rectangle `(x1, x2, y1, y2)`.
    pub fn outflow_box(&self) -> (i32, i32, i32, i32) {
        (
            self.outflow_x1,
            self.outflow_x2,
            self.outflow_y1,
            self.outflow_y2,
        )
    }

    /// `ResetResourceCounts()`: every cell back to the per-cell initial plus
    /// its `CELL` initial.
    pub fn reset_resource_counts(&mut self) {
        let initial = self.initial;
        for e in &mut self.grid {
            e.reset_resource_count(initial);
        }
    }

    /// `SetModified(bool)`
    pub fn set_modified(&mut self, in_modified: bool) {
        self.modified = in_modified;
    }

    /// `GetModified()`
    pub fn modified(&self) -> bool {
        self.modified
    }

    /// `SetCurrPeakX(int)`
    pub fn set_curr_peak_x(&mut self, v: i32) {
        self.curr_peakx = v;
    }

    /// `SetCurrPeakY(int)`
    pub fn set_curr_peak_y(&mut self, v: i32) {
        self.curr_peaky = v;
    }

    /// `GetCurrPeakX()`
    pub fn curr_peak_x(&self) -> i32 {
        self.curr_peakx
    }

    /// `GetCurrPeakY()`
    pub fn curr_peak_y(&self) -> i32 {
        self.curr_peaky
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Ported from rust/avida-rust spatial_res_count_helpers.rs and the C++
    // unit test cSpatialResCountHelperTests ("Flow scalar parity").
    #[test]
    fn flow_matter_reference_case() {
        let got = flow_matter(10.0, 4.0, 1.0, 1.0, 0.5, -0.25, 1, -1, 2.0_f64.sqrt()).unwrap();
        let diff = 6.0;
        let xgravity = 10.0 * 0.5 / 3.0;
        // ydist < 0 and ygravity < 0: material moves towards element 2.
        let ygravity = 10.0 * 0.25 / 3.0;
        let expected = ((diff / 16.0 + diff / 16.0 + xgravity + ygravity) / 2.0) / 2.0_f64.sqrt();
        assert_eq!(got, expected);
        // Legacy zero guard.
        assert_eq!(flow_matter(0.0, 0.0, 1.0, 1.0, 1.0, 1.0, 1, 0, -1.0), None);
        // Both empty but a real link: computed (0 / 1 / 1 = 0).
        assert_eq!(
            flow_matter(0.0, 0.0, 1.0, 1.0, 1.0, 1.0, 1, 0, 1.0),
            Some(0.0)
        );
    }

    #[test]
    fn flow_matter_gravity_against_direction_pulls_from_elem2() {
        let got = flow_matter(1.0, 4.0, 0.0, 0.0, -0.3, 0.0, 1, 0, 1.0).unwrap();
        assert_eq!(got, -4.0 * 0.3 / 3.0);
    }

    #[test]
    fn apto_max_keeps_nan_and_first_zero() {
        assert_eq!(apto_max(2.0, 0.0), 2.0);
        assert_eq!(apto_max(-2.0, 0.0), 0.0);
        assert!(apto_max(f64::NAN, 0.0).is_nan());
        assert!(apto_max(-0.0, 0.0).is_sign_negative());
    }

    // Ported from rust/avida-rust spatial_res_count_helpers.rs
    // (normalize_span_clamps_and_wraps) via CheckRanges.
    #[test]
    fn check_ranges_clamps_and_unwraps() {
        let mut s = SpatialResCount::with_size(10, 10, geometry::TORUS);
        s.set_inflow_x1(-5);
        s.set_inflow_x2(42);
        s.set_inflow_y1(8);
        s.set_inflow_y2(3);
        s.set_outflow_x1(8);
        s.set_outflow_x2(3);
        s.set_outflow_y1(-1);
        s.set_outflow_y2(12);
        s.check_ranges();
        assert_eq!(s.inflow_box(), (0, 10, 8, 13));
        assert_eq!(s.outflow_box(), (8, 13, 0, 10));
    }

    // Ported from rust/avida-rust spatial_res_count_helpers.rs
    // (setpointer_entry_matches_torus_and_grid_masks, degenerate dimensions).
    #[test]
    fn set_pointers_torus_and_grid_masks() {
        let t = SpatialResCount::with_size(3, 3, geometry::TORUS);
        let e = t.element(4);
        assert_eq!((e.elem_ptr(0), e.ptr_xdist(0), e.ptr_ydist(0)), (0, -1, -1));
        assert_eq!(e.ptr_dist(0), 2.0_f64.sqrt());
        assert_eq!(t.element(0).elem_ptr(0), 8);

        let g = SpatialResCount::with_size(3, 3, geometry::GRID);
        let e = g.element(0);
        for k in [0, 1, 2, 6, 7] {
            assert_eq!(
                (e.elem_ptr(k), e.ptr_xdist(k), e.ptr_ydist(k), e.ptr_dist(k)),
                (NONE, NONE, NONE, -99.0)
            );
        }
        assert_eq!(e.elem_ptr(3), 1);
        assert_eq!(e.elem_ptr(4), 4);
        assert_eq!(e.elem_ptr(5), 3);

        // 1x1 grid: every link masked.
        let g = SpatialResCount::with_size(1, 1, geometry::GRID);
        for k in 0..8 {
            assert_eq!(g.element(0).elem_ptr(k), NONE);
        }
        // 1x3 column: vertical links survive in the middle.
        let g = SpatialResCount::with_size(1, 3, geometry::GRID);
        let e = g.element(1);
        assert_eq!(
            (e.elem_ptr(1), e.ptr_xdist(1), e.ptr_ydist(1), e.ptr_dist(1)),
            (0, 0, -1, 1.0)
        );
        assert_eq!(e.elem_ptr(3), NONE);
        // 3x1 row: horizontal links survive in the middle.
        let g = SpatialResCount::with_size(3, 1, geometry::GRID);
        let e = g.element(1);
        assert_eq!(
            (e.elem_ptr(3), e.ptr_xdist(3), e.ptr_ydist(3), e.ptr_dist(3)),
            (2, 1, 0, 1.0)
        );
    }

    // Ported from rust/avida-rust spatial_res_count_helpers.rs
    // (source_per_cell / sink / outflow reference cases).
    #[test]
    fn source_sink_and_cell_flows() {
        let mut s = SpatialResCount::with_size(5, 4, geometry::TORUS);
        s.set_inflow_x1(0);
        s.set_inflow_x2(1);
        s.set_inflow_y1(0);
        s.set_inflow_y2(2);
        s.source(12.0);
        assert_eq!(s.element(0).delta(), 2.0);
        assert_eq!(s.element(11).delta(), 2.0);
        assert_eq!(s.element(2).delta(), 0.0);
        s.state_all();

        s.set_outflow_x1(0);
        s.set_outflow_x2(0);
        s.set_outflow_y1(0);
        s.set_outflow_y2(0);
        s.sink(0.2);
        assert_eq!(s.element(0).delta(), -(2.0 * (1.0 - 0.2)));
        let mut t = s.clone();
        t.set_outflow_x1(NONE);
        t.state_all();
        t.sink(0.0);
        assert_eq!(t.element(0).delta(), 0.0);

        let mut c = SpatialResCount::with_size(2, 2, geometry::TORUS);
        c.set_cell_list(&[
            CellResource::new(3, 10.0, 0.5, 0.2),
            CellResource::new(4, 1.0, 1.0, 1.0),
        ]);
        assert_eq!(c.get_amount(3), 10.0);
        assert_eq!(c.element(3).initial(), 10.0);
        c.cell_inflow();
        assert_eq!(c.element(3).delta(), 0.5);
        c.cell_outflow();
        assert_eq!(c.element(3).delta(), 0.5 - 2.0);
    }

    #[test]
    fn out_of_range_reads_and_writes() {
        let mut s = SpatialResCount::with_size(2, 2, geometry::GRID);
        assert_eq!(s.get_amount(4), OUT_OF_RANGE_AMOUNT);
        assert_eq!(s.get_amount(-1), OUT_OF_RANGE_AMOUNT);
        assert_eq!(s.get_amount_xy(2, 0), OUT_OF_RANGE_AMOUNT);
        s.rate(9, 1.0);
        s.state(9);
        s.set_cell_amount(-1, 3.0);
        assert_eq!(s.sum_all(), 0.0);
        s.rate_xy(1, 1, 2.0);
        s.state_xy(1, 1);
        assert_eq!(s.get_amount(3), 2.0);
        assert_eq!(s.get_amount_xy(1, 1), 2.0);
    }

    #[test]
    fn reset_resource_counts_adds_cell_initial() {
        let mut s = SpatialResCount::with_size(2, 1, geometry::TORUS);
        s.set_cell_list(&[CellResource::new(1, 3.0, 0.0, 0.0)]);
        s.set_initial(0.5);
        s.set_cell_amount(0, 9.0);
        s.reset_resource_counts();
        assert_eq!(s.get_amount(0), 0.5);
        assert_eq!(s.get_amount(1), 3.5);
    }

    #[test]
    fn diffusion_conserves_mass_on_torus() {
        let mut s = SpatialResCount::with_size(5, 5, geometry::TORUS);
        s.set_cell_amount(12, 100.0);
        for _ in 0..5 {
            s.flow_all();
            s.state_all();
        }
        assert!((s.sum_all() - 100.0).abs() < 1e-9);
        assert!(s.get_amount(12) < 100.0);
        assert!(s.get_amount(0) > 0.0);
    }

    #[test]
    fn one_wide_torus_links_to_itself() {
        let mut s = SpatialResCount::with_flows(1, 3, geometry::TORUS, 1.0, 1.0, 0.3, 0.0);
        s.set_cell_amount(1, 2.0);
        s.flow_all();
        // Self-links add and remove the same amount.
        assert!(s.element(1).delta().abs() < 1.0);
    }
}
