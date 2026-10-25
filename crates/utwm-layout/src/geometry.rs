//! Rectangles in the two coordinate systems this crate speaks.
//!
//! The platforms place windows in whole screen units with y growing down
//! ([`PixelRect`]): physical pixels on Windows, points measured from the top
//! of the main display on the Mac (the Accessibility API's convention).
//! Rectangle's calculations work in AppKit points with y growing up. Rather than
//! re-derive every formula for a flipped axis, the calculations run on
//! [`Frame`], a y-up rectangle made by negating y, so they port line for line:
//! `visibleFrame.maxY - rect.height` still means "against the top".

/// Integer screen rectangle, y down, edges inclusive-exclusive like Win32 `RECT`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct PixelRect {
    pub left: i32,
    pub top: i32,
    pub right: i32,
    pub bottom: i32,
}

impl PixelRect {
    pub const fn new(left: i32, top: i32, right: i32, bottom: i32) -> Self {
        Self {
            left,
            top,
            right,
            bottom,
        }
    }

    pub const fn from_xywh(x: i32, y: i32, width: i32, height: i32) -> Self {
        Self::new(x, y, x + width, y + height)
    }

    pub const fn width(self) -> i32 {
        self.right - self.left
    }

    pub const fn height(self) -> i32 {
        self.bottom - self.top
    }

    pub fn is_empty(self) -> bool {
        self.width() <= 0 || self.height() <= 0
    }

    pub fn contains(self, other: Self) -> bool {
        other.left >= self.left
            && other.top >= self.top
            && other.right <= self.right
            && other.bottom <= self.bottom
    }

    pub fn intersection(self, other: Self) -> Option<Self> {
        let clipped = Self::new(
            self.left.max(other.left),
            self.top.max(other.top),
            self.right.min(other.right),
            self.bottom.min(other.bottom),
        );
        (!clipped.is_empty()).then_some(clipped)
    }

    pub fn area(self) -> i64 {
        if self.is_empty() {
            0
        } else {
            i64::from(self.width()) * i64::from(self.height())
        }
    }

    pub fn with_origin(self, left: i32, top: i32) -> Self {
        Self::from_xywh(left, top, self.width(), self.height())
    }

    pub fn to_rect(self) -> Rect {
        Rect {
            x: f64::from(self.left),
            y: f64::from(self.top),
            w: f64::from(self.width()),
            h: f64::from(self.height()),
        }
    }
}

impl std::fmt::Display for PixelRect {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{}×{} at ({}, {})",
            self.width(),
            self.height(),
            self.left,
            self.top
        )
    }
}

/// Fractional screen rectangle, y down.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Rect {
    pub x: f64,
    pub y: f64,
    pub w: f64,
    pub h: f64,
}

impl Rect {
    pub fn right(self) -> f64 {
        self.x + self.w
    }

    pub fn bottom(self) -> f64 {
        self.y + self.h
    }

    /// Rounds each edge, not each size, so two rects that share an edge in
    /// fractions still share it in pixels.
    pub fn to_pixels(self) -> PixelRect {
        PixelRect::new(px(self.x), px(self.y), px(self.right()), px(self.bottom()))
    }
}

/// Rounds to the nearest pixel, saturating at the i32 range.
pub fn px(value: f64) -> i32 {
    if value.is_nan() {
        return 0;
    }
    let clamped = value
        .round()
        .clamp(f64::from(i32::MIN), f64::from(i32::MAX));
    #[expect(
        clippy::cast_possible_truncation,
        reason = "rounded and clamped to the i32 range just above"
    )]
    let pixels = clamped as i32;
    pixels
}

/// A y-up rectangle: AppKit's coordinate convention, made by negating y.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Frame {
    pub x: f64,
    /// The bottom edge, in y-up terms.
    pub y: f64,
    pub w: f64,
    pub h: f64,
}

impl Frame {
    pub fn from_rect(rect: Rect) -> Self {
        Self {
            x: rect.x,
            y: -(rect.y + rect.h),
            w: rect.w,
            h: rect.h,
        }
    }

    pub fn from_pixels(rect: PixelRect) -> Self {
        Self::from_rect(rect.to_rect())
    }

    pub fn to_rect(self) -> Rect {
        Rect {
            x: self.x,
            y: -(self.y + self.h),
            w: self.w,
            h: self.h,
        }
    }

    pub fn to_pixels(self) -> PixelRect {
        self.to_rect().to_pixels()
    }

    pub fn min_x(self) -> f64 {
        self.x
    }

    pub fn max_x(self) -> f64 {
        self.x + self.w
    }

    pub fn min_y(self) -> f64 {
        self.y
    }

    pub fn max_y(self) -> f64 {
        self.y + self.h
    }

    /// AppKit's `isLandscape`: strictly wider than tall.
    pub fn is_landscape(self) -> bool {
        self.w > self.h
    }
}

/// Edges of a rectangle, by what they look like on screen.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Edges {
    pub left: bool,
    pub right: bool,
    pub top: bool,
    pub bottom: bool,
}

impl Edges {
    pub const NONE: Self = Self {
        left: false,
        right: false,
        top: false,
        bottom: false,
    };

    pub const fn new(left: bool, right: bool, top: bool, bottom: bool) -> Self {
        Self {
            left,
            right,
            top,
            bottom,
        }
    }

    pub fn is_all(self) -> bool {
        self.left && self.right && self.top && self.bottom
    }

    /// Exactly one horizontal and one vertical edge.
    pub fn is_corner(self) -> bool {
        (self.left != self.right) && (self.top != self.bottom)
    }

    /// Which edges of `rect` lie on the matching edges of `frame`.
    pub fn shared(rect: PixelRect, frame: PixelRect) -> Self {
        Self {
            left: rect.left == frame.left,
            right: rect.right == frame.right,
            top: rect.top == frame.top,
            bottom: rect.bottom == frame.bottom,
        }
    }
}

/// Rectangle stores single-precision fractions (`Float`) and widens them to
/// `CGFloat` at the multiplication. Doing the same keeps `floor` landing on the
/// same pixel in the rare cases where the two precisions straddle an integer.
pub(crate) fn swift_float(value: f64) -> f64 {
    #[expect(
        clippy::cast_possible_truncation,
        reason = "deliberately narrowing to match Rectangle's Float fractions"
    )]
    let narrowed = value as f32;
    f64::from(narrowed)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn frames_round_trip() {
        let rect = PixelRect::new(10, 20, 110, 220);
        let frame = Frame::from_pixels(rect);
        assert!(
            (frame.max_y() - -20.0).abs() < f64::EPSILON,
            "top edge is -top in y-up"
        );
        assert!((frame.min_y() - -220.0).abs() < f64::EPSILON);
        assert_eq!(frame.to_pixels(), rect);
    }

    #[test]
    fn edges_round_so_neighbours_meet() {
        let left = Rect {
            x: 0.0,
            y: 0.0,
            w: 1366.0 / 3.0,
            h: 10.0,
        }
        .to_pixels();
        let middle = Rect {
            x: 1366.0 / 3.0,
            y: 0.0,
            w: 1366.0 / 3.0,
            h: 10.0,
        }
        .to_pixels();
        assert_eq!(left.right, middle.left);
    }

    #[test]
    fn shared_edges_and_corners() {
        let screen = PixelRect::new(0, 0, 1920, 1032);
        let top_left = PixelRect::new(0, 0, 960, 516);
        assert!(Edges::shared(top_left, screen).is_corner());
        assert!(Edges::shared(screen, screen).is_all());
        assert!(!Edges::shared(PixelRect::new(0, 0, 960, 1032), screen).is_corner());
    }
}
