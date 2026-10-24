//! Rectangle Pro's Custom Size & Position, from the documented fields: an
//! anchor, sizes that are fractions of the screen (≤ 1) or points (> 1), and a
//! blank size meaning "keep the window's".

use utwm_config::{Anchor, Dimension, Placement};

use crate::geometry::{PixelRect, Rect};

/// Where `placement` puts a window currently at `window`, on a screen whose
/// usable area is `visible` and which has `scale` pixels per point.
pub fn place(
    placement: &Placement,
    window: PixelRect,
    visible: PixelRect,
    scale: f64,
) -> PixelRect {
    let area = visible.to_rect();
    let current = window.to_rect();
    let size = |dimension: Dimension, extent: f64, current: f64| -> f64 {
        let value = match dimension {
            Dimension::Keep => current,
            Dimension::Fraction(fraction) => extent * fraction,
            Dimension::Points(points) => points * scale,
        };
        value.clamp(1.0, extent)
    };
    let w = size(placement.width, area.w, current.w);
    let h = size(placement.height, area.h, current.h);
    let offset = |dimension: Dimension, extent: f64| match dimension {
        Dimension::Keep => None,
        Dimension::Fraction(fraction) => Some(extent * fraction),
        Dimension::Points(points) => Some(points * scale),
    };

    let centered_x = area.x + (area.w - w) / 2.0;
    let centered_y = area.y + (area.h - h) / 2.0;
    let (x, y) = match placement.anchor {
        Anchor::Center => (centered_x, centered_y),
        Anchor::Top => (centered_x, area.y),
        Anchor::Bottom => (centered_x, area.bottom() - h),
        Anchor::Left => (area.x, centered_y),
        Anchor::Right => (area.right() - w, centered_y),
        Anchor::TopLeft => (area.x, area.y),
        Anchor::TopRight => (area.right() - w, area.y),
        Anchor::BottomLeft => (area.x, area.bottom() - h),
        Anchor::BottomRight => (area.right() - w, area.bottom() - h),
        Anchor::CustomOrigin => (
            area.x + offset(placement.x, area.w).unwrap_or(current.x - area.x),
            area.y + offset(placement.y, area.h).unwrap_or(current.y - area.y),
        ),
        Anchor::CenterOffset => (
            centered_x + offset(placement.x, area.w).unwrap_or(0.0),
            centered_y + offset(placement.y, area.h).unwrap_or(0.0),
        ),
    };
    Rect { x, y, w, h }.to_pixels()
}

#[cfg(test)]
mod tests {
    use super::*;

    const SCREEN: PixelRect = PixelRect::new(0, 0, 1920, 1032);

    fn placement(anchor: Anchor, width: Dimension, height: Dimension) -> Placement {
        Placement {
            anchor,
            x: Dimension::Keep,
            y: Dimension::Keep,
            width,
            height,
            display: None,
        }
    }

    #[test]
    fn left_third_below_the_movie() {
        let spec = placement(
            Anchor::BottomLeft,
            Dimension::Fraction(0.333),
            Dimension::Fraction(0.7),
        );
        let window = PixelRect::new(100, 100, 500, 500);
        assert_eq!(
            place(&spec, window, SCREEN, 1.0),
            PixelRect::new(0, 310, 639, 1032)
        );
    }

    #[test]
    fn app_store_screenshot_is_centered_in_points() {
        let spec = placement(
            Anchor::Center,
            Dimension::Points(1168.0),
            Dimension::Points(688.0),
        );
        let window = PixelRect::new(100, 100, 500, 500);
        assert_eq!(
            place(&spec, window, SCREEN, 1.0),
            PixelRect::new(376, 172, 1544, 860)
        );
        // At 150% scaling the same points are more pixels, clamped to the screen.
        let big = place(&spec, window, PixelRect::new(0, 0, 2560, 1400), 1.5);
        assert_eq!((big.width(), big.height()), (1752, 1032));
    }

    #[test]
    fn a_blank_size_keeps_the_window_size() {
        let spec = placement(Anchor::TopRight, Dimension::Keep, Dimension::Fraction(1.0));
        let window = PixelRect::new(100, 100, 500, 500);
        assert_eq!(
            place(&spec, window, SCREEN, 1.0),
            PixelRect::new(1520, 0, 1920, 1032)
        );
    }
}
