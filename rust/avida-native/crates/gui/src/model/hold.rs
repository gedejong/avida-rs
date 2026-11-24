//! Ranges that hold still: an axis or colour range keeps some headroom
//! around the values, moves at once when a value falls outside it, and
//! narrows slowly, and only once a good part of it has gone unused. One
//! stray organism or a short-lived peak then does not make a view jump.

/// Headroom added beyond the values when a range has to widen, as a
/// fraction of the values' span.
pub const MARGIN: f64 = 0.15;
/// A side of the range narrows only while more than this fraction of the
/// range lies unused on that side.
pub const SLACK: f64 = 0.3;
/// The time in seconds in which a narrowing range closes about two thirds
/// of its gap.
pub const SETTLE: f64 = 4.0;

/// One step: `prev` is the range shown before, `data` the range of the
/// values now, `dt` the seconds since. With `keep_zero` the headroom does
/// not cross zero when the values do not (counts, fitness on a linear
/// scale). Returns the range to show and whether it is still narrowing.
pub fn hold(
    prev: Option<(f64, f64)>,
    data: (f64, f64),
    dt: f64,
    keep_zero: bool,
) -> ((f64, f64), bool) {
    let Some(prev) = prev.filter(|p| p.0.is_finite() && p.1.is_finite() && p.0 < p.1) else {
        return (data, false);
    };
    if !(data.0.is_finite() && data.1.is_finite()) {
        return (prev, false);
    }
    let pad = (data.1 - data.0).max((prev.1 - prev.0) * 0.1) * MARGIN;
    let mut want = (data.0 - pad, data.1 + pad);
    if keep_zero {
        if data.0 >= 0.0 {
            want.0 = want.0.max(0.0);
        }
        if data.1 <= 0.0 {
            want.1 = want.1.min(0.0);
        }
    }
    let shown = prev.1 - prev.0;
    let k = 1.0 - (-dt.max(0.0) / SETTLE).exp();
    let mut moving = false;
    // `gap`: how far the values lie inside this end; negative when outside.
    let mut side = |end: f64, want: f64, gap: f64| {
        if gap < 0.0 {
            want
        } else if gap > SLACK * shown {
            moving = true;
            end + (want - end) * k
        } else {
            end
        }
    };
    let lo = side(prev.0, want.0, data.0 - prev.0);
    let hi = side(prev.1, want.1, prev.1 - data.1);
    ((lo, hi), moving)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_range_holds_still_widens_at_once_and_narrows_slowly() {
        // The first range is the values' own.
        assert_eq!(hold(None, (1.0, 10.0), 0.016, true), ((1.0, 10.0), false));
        // Values that stay inside leave it alone.
        assert_eq!(
            hold(Some((0.0, 10.0)), (1.0, 9.0), 0.016, true),
            ((0.0, 10.0), false)
        );
        // A value outside: that end moves in the same step, with headroom,
        // so the next slightly higher value does not move it again.
        let (r, moving) = hold(Some((0.0, 10.0)), (1.0, 12.0), 0.016, true);
        assert!(!moving);
        assert_eq!(r.0, 0.0);
        assert!((r.1 - (12.0 + 11.0 * MARGIN)).abs() < 1e-12, "{r:?}");
        assert_eq!(hold(Some(r), (1.0, 13.0), 0.016, true), (r, false));
        // The headroom does not cross zero.
        let (r, _) = hold(Some((1.0, 10.0)), (0.5, 10.0), 0.016, true);
        assert_eq!(r.0, 0.0);
        let (r, _) = hold(Some((1.0, 10.0)), (0.5, 10.0), 0.016, false);
        assert!(r.0 < 0.0);
        // The peak is gone: the top comes down slowly, and stops while
        // some of the range is still unused.
        let mut r = (0.0, 100.0);
        let (first, moving) = hold(Some(r), (0.0, 10.0), 0.016, true);
        assert!(moving && first.1 > 99.0 && first.1 < 100.0, "{first:?}");
        let mut steps = 0;
        loop {
            let (n, moving) = hold(Some(r), (0.0, 10.0), 0.016, true);
            r = n;
            if !moving {
                break;
            }
            steps += 1;
            assert!(steps < 10_000);
        }
        assert!(r.1 > 11.5 && r.1 < 10.0 / (1.0 - SLACK) + 0.1, "{r:?}");
        // Several settling times at 60 steps a second.
        assert!(steps > 400, "{steps}");
        assert_eq!(hold(Some(r), (0.0, 10.0), 0.016, true), (r, false));
    }
}
