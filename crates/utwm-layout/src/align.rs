//! Placing a window that wouldn't take the size it was given, ported from
//! Rectangle's `ClampedWindowAligner` and `WindowFrameBounds`.
//!
//! An app with a minimum size, a fixed size, or a terminal that sizes itself in
//! whole character cells comes back larger or smaller than its tile. With
//! `moveFixedSizeToEdge` at its default, the window stays against whichever
//! screen edges the tile was against, so a right half that won't shrink still
//! hugs the right edge.

use utwm_config::settings::EdgeAlignment;

use crate::geometry::{Edges, PixelRect, px};

/// Up to a pixel short counts as fitting: realigning that would only move the
/// shortfall onto an edge where it shows.
const ALIGNMENT_TOLERANCE: i32 = 1;

/// Where a window of `actual` size goes when it was asked to fill `zone`.
/// `initial` is the tile before gaps, used to tell which screen edges it was
/// meant to touch.
pub fn realign(
    zone: PixelRect,
    actual_width: i32,
    actual_height: i32,
    initial: PixelRect,
    screen: PixelRect,
    alignment: EdgeAlignment,
    gap: i32,
) -> PixelRect {
    let window = PixelRect::from_xywh(zone.left, zone.top, actual_width, actual_height);
    let shared = match alignment {
        EdgeAlignment::EdgesAndCorners => Edges::shared(initial, screen),
        EdgeAlignment::Corners => {
            let edges = Edges::shared(initial, screen);
            if edges.is_corner() {
                edges
            } else {
                Edges::NONE
            }
        }
        EdgeAlignment::Centered => Edges::NONE,
        EdgeAlignment::LeadingCorner => return constrain(window, screen, gap),
    };
    constrain(aligned(window, zone, shared), screen, gap)
}

fn aligned(window: PixelRect, zone: PixelRect, shared: Edges) -> PixelRect {
    let mut left = window.left;
    let mut top = window.top;
    if (window.width() - zone.width()).abs() > ALIGNMENT_TOLERANCE {
        left = if shared.left && !shared.right {
            zone.left
        } else if shared.right && !shared.left {
            zone.right - window.width()
        } else {
            zone.left + px(f64::from(zone.width() - window.width()) / 2.0)
        };
    }
    if (window.height() - zone.height()).abs() > ALIGNMENT_TOLERANCE {
        top = if shared.top && !shared.bottom {
            zone.top
        } else if shared.bottom && !shared.top {
            zone.bottom - window.height()
        } else {
            zone.top + px(f64::from(zone.height() - window.height()) / 2.0)
        };
    }
    window.with_origin(left, top)
}

/// `WindowFrameBounds.constrained`: pull a window that fits back on screen.
pub fn constrain(window: PixelRect, screen: PixelRect, gap: i32) -> PixelRect {
    if window.width() > screen.width() || window.height() > screen.height() {
        return window;
    }
    let mut left = window.left;
    let mut top = window.top;
    if window.left < screen.left {
        left = screen.left;
    } else if window.right > screen.right {
        left = screen.right - window.width() - gap;
    }
    if window.bottom > screen.bottom {
        top = screen.bottom - window.height();
    } else if window.top < screen.top {
        top = screen.top + gap;
    }
    window.with_origin(left, top)
}

#[cfg(test)]
mod tests {
    use super::*;

    const SCREEN: PixelRect = PixelRect::new(0, 0, 1920, 1032);

    #[test]
    fn a_right_half_that_wont_shrink_hugs_the_right_edge() {
        let zone = PixelRect::new(960, 0, 1920, 1032);
        let placed = realign(
            zone,
            1200,
            1032,
            zone,
            SCREEN,
            EdgeAlignment::EdgesAndCorners,
            0,
        );
        assert_eq!(placed, PixelRect::new(720, 0, 1920, 1032));
    }

    #[test]
    fn a_bottom_left_corner_stays_in_the_corner() {
        let zone = PixelRect::new(0, 516, 960, 1032);
        let placed = realign(
            zone,
            800,
            400,
            zone,
            SCREEN,
            EdgeAlignment::EdgesAndCorners,
            0,
        );
        assert_eq!(placed, PixelRect::new(0, 632, 800, 1032));
    }

    #[test]
    fn centered_alignment_centers_in_the_zone() {
        let zone = PixelRect::new(0, 0, 960, 1032);
        let placed = realign(zone, 760, 1032, zone, SCREEN, EdgeAlignment::Centered, 0);
        assert_eq!(placed, PixelRect::new(100, 0, 860, 1032));
    }

    #[test]
    fn a_one_pixel_shortfall_is_left_alone() {
        let zone = PixelRect::new(960, 0, 1920, 1032);
        let placed = realign(
            zone,
            959,
            1032,
            zone,
            SCREEN,
            EdgeAlignment::EdgesAndCorners,
            0,
        );
        assert_eq!(placed.left, 960);
    }
}
