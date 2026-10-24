//! `GapCalculation.applyGaps`: shrink a tile by the gap on every side, then give
//! half of it back on edges that face another tile, so neighbours end up one
//! gap apart and screen edges keep a full gap.

use crate::geometry::{Edges, Frame};
use crate::sub_action::GapDimensions;

pub(crate) fn apply_gaps(
    rect: Frame,
    dimensions: GapDimensions,
    shared: Edges,
    gap: f64,
    skip_top_gap: bool,
) -> Frame {
    let half = gap / 2.0;
    let mut with_gaps = rect;
    if dimensions.horizontal {
        with_gaps.x += gap;
        with_gaps.w -= gap * 2.0;
        if shared.left {
            with_gaps.x -= half;
            with_gaps.w += half;
        }
        if shared.right {
            with_gaps.w += half;
        }
    }
    if dimensions.vertical {
        with_gaps.y += gap;
        with_gaps.h -= gap * 2.0;
        // y-up: the bottom edge is min y, the top edge max y.
        if shared.bottom {
            with_gaps.y -= half;
            with_gaps.h += half;
        }
        if shared.top {
            with_gaps.h += half;
        }
        if skip_top_gap && !shared.top {
            with_gaps.h += gap;
        }
    }
    with_gaps
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::geometry::PixelRect;

    #[test]
    fn halves_meet_one_gap_apart() {
        let left = Frame::from_pixels(PixelRect::new(0, 0, 960, 1000));
        let right = Frame::from_pixels(PixelRect::new(960, 0, 1920, 1000));
        let left = apply_gaps(
            left,
            GapDimensions::BOTH,
            Edges::new(false, true, false, false),
            10.0,
            false,
        );
        let right = apply_gaps(
            right,
            GapDimensions::BOTH,
            Edges::new(true, false, false, false),
            10.0,
            false,
        );
        assert_eq!(left.to_pixels(), PixelRect::new(10, 10, 955, 990));
        assert_eq!(right.to_pixels(), PixelRect::new(965, 10, 1910, 990));
    }
}
