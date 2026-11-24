//! The plots' ranges, held still between frames (see
//! [`crate::model::hold`]): what a plot showed last frame is kept in the
//! frame memory under the plot's key.

use egui::{Context, Id};

use crate::model::hold::hold;

/// The range to show for the values' range `data`, held from what the plot
/// `key` showed last frame. A new key (another property, another scale)
/// starts at its values, and so does a plot that was not drawn in the last
/// frames (the range it held then says nothing now). Asks for another frame while the range is still
/// narrowing.
pub(super) fn eased_range(
    ctx: &Context,
    key: impl std::hash::Hash + std::fmt::Debug,
    data: (f64, f64),
    keep_zero: bool,
) -> (f64, f64) {
    if !(data.0.is_finite() && data.1.is_finite()) {
        return data;
    }
    let id = Id::unique(("eased-range", key));
    let frame = ctx.cumulative_frame_nr();
    let prev: Option<(u64, (f64, f64))> = ctx.data(|d| d.get_temp(id));
    let prev = prev
        .filter(|(at, _)| frame.saturating_sub(*at) <= 2)
        .map(|(_, r)| r);
    let dt = f64::from(ctx.input(|i| i.stable_dt)).min(0.1);
    let (shown, moving) = hold(prev, data, dt, keep_zero);
    ctx.data_mut(|d| d.insert_temp(id, (frame, shown)));
    if moving {
        ctx.request_repaint();
    }
    shown
}

/// Forgets what the plot `key` held, so that it starts at its values.
pub(super) fn forget(ctx: &Context, key: impl std::hash::Hash + std::fmt::Debug) {
    let id = Id::unique(("eased-range", key));
    ctx.data_mut(|d| d.remove::<(u64, (f64, f64))>(id));
}
