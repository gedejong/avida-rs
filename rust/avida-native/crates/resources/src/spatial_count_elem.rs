//! One grid cell of a spatial resource.
//!
//! Ports `cSpatialCountElem` (`avida-core/source/main/cSpatialCountElem.{h,cc}`).

/// `MAXFLOWPTS` in `cSpatialCountElem.cc`: number of neighbour pointers.
pub const MAX_FLOW_PTS: usize = 8;

/// `cSpatialCountElem`: the amount of resource in a cell, the pending change
/// (`delta`) folded in by [`state`](Self::state), the cell's `CELL`-command
/// initial amount, and the eight neighbour links used by diffusion/gravity.
#[derive(Clone, Debug, PartialEq)]
pub struct SpatialCountElem {
    pub(crate) amount: f64,
    pub(crate) delta: f64,
    pub(crate) initial: f64,
    elempt: [i32; MAX_FLOW_PTS],
    xdist: [i32; MAX_FLOW_PTS],
    ydist: [i32; MAX_FLOW_PTS],
    dist: [f64; MAX_FLOW_PTS],
}

impl Default for SpatialCountElem {
    /// `cSpatialCountElem()`
    ///
    /// The C++ leaves the pointer arrays uninitialised until `SetPointers`;
    /// they start at zero here.
    fn default() -> Self {
        Self::with_amount(0.0)
    }
}

impl SpatialCountElem {
    /// `cSpatialCountElem(double initamount)`: `amount` and `initial` both
    /// set to `initamount`.
    pub fn with_amount(initamount: f64) -> Self {
        SpatialCountElem {
            amount: initamount,
            delta: 0.0,
            initial: initamount,
            elempt: [0; MAX_FLOW_PTS],
            xdist: [0; MAX_FLOW_PTS],
            ydist: [0; MAX_FLOW_PTS],
            dist: [0.0; MAX_FLOW_PTS],
        }
    }

    /// `Rate(double)`: accumulate a pending change.
    #[inline]
    pub fn rate(&mut self, ratein: f64) {
        self.delta += ratein;
    }

    /// `State()`: fold the pending change into the amount.
    #[inline]
    pub fn state(&mut self) {
        self.amount += self.delta;
        self.delta = 0.0;
    }

    /// `GetAmount()`
    #[inline]
    pub fn amount(&self) -> f64 {
        self.amount
    }

    /// `SetAmount(double)`
    #[inline]
    pub fn set_amount(&mut self, res: f64) {
        self.amount = res;
    }

    /// The pending change (`delta`, private in C++).
    #[inline]
    pub fn delta(&self) -> f64 {
        self.delta
    }

    /// `SetPtr(innum, inelempt, inxdist, inydist, indist)`
    pub fn set_ptr(
        &mut self,
        innum: usize,
        inelempt: i32,
        inxdist: i32,
        inydist: i32,
        indist: f64,
    ) {
        self.elempt[innum] = inelempt;
        self.xdist[innum] = inxdist;
        self.ydist[innum] = inydist;
        self.dist[innum] = indist;
    }

    /// `GetElemPtr(int)`
    #[inline]
    pub fn elem_ptr(&self, innum: usize) -> i32 {
        self.elempt[innum]
    }

    /// `GetPtrXdist(int)`
    #[inline]
    pub fn ptr_xdist(&self, innum: usize) -> i32 {
        self.xdist[innum]
    }

    /// `GetPtrYdist(int)`
    #[inline]
    pub fn ptr_ydist(&self, innum: usize) -> i32 {
        self.ydist[innum]
    }

    /// `GetPtrDist(int)`
    #[inline]
    pub fn ptr_dist(&self, innum: usize) -> f64 {
        self.dist[innum]
    }

    /// `SetInitial(double)`
    #[inline]
    pub fn set_initial(&mut self, init: f64) {
        self.initial = init;
    }

    /// `GetInitial()`
    #[inline]
    pub fn initial(&self) -> f64 {
        self.initial
    }

    /// `ResetResourceCount(double res_initial)`: amount becomes the
    /// resource-wide initial plus this cell's `CELL` initial.
    #[inline]
    pub fn reset_resource_count(&mut self, res_initial: f64) {
        self.amount = res_initial + self.initial;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rate_accumulates_and_state_folds() {
        let mut e = SpatialCountElem::default();
        e.rate(1.25);
        e.rate(-0.5);
        assert_eq!(e.amount(), 0.0);
        assert_eq!(e.delta(), 0.75);
        e.state();
        assert_eq!(e.amount(), 0.75);
        assert_eq!(e.delta(), 0.0);
    }

    // Ported from rust/avida-rust spatial_res_count_helpers.rs
    // (state_fold / rate_next_delta / reset_amount reference cases).
    #[test]
    fn helper_reference_cases() {
        let mut e = SpatialCountElem::with_amount(7.25);
        e.rate(-2.0);
        e.state();
        assert_eq!(e.amount(), 5.25);

        let mut e = SpatialCountElem {
            delta: 1.25,
            ..SpatialCountElem::default()
        };
        e.rate(-0.5);
        assert_eq!(e.delta(), 0.75);

        let mut e = SpatialCountElem::default();
        e.set_initial(1.25);
        e.reset_resource_count(2.5);
        assert_eq!(e.amount(), 3.75);
    }

    #[test]
    fn with_amount_sets_initial_too() {
        let e = SpatialCountElem::with_amount(3.0);
        assert_eq!((e.amount(), e.initial(), e.delta()), (3.0, 3.0, 0.0));
    }

    #[test]
    fn set_ptr_round_trips() {
        let mut e = SpatialCountElem::default();
        e.set_ptr(3, 17, 1, 0, 1.0);
        e.set_ptr(4, -99, -99, -99, -99.0);
        assert_eq!(
            (e.elem_ptr(3), e.ptr_xdist(3), e.ptr_ydist(3), e.ptr_dist(3)),
            (17, 1, 0, 1.0)
        );
        assert_eq!(e.elem_ptr(4), -99);
        assert_eq!(e.ptr_dist(4), -99.0);
    }
}
