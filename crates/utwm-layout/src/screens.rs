//! Displays: which one a window is on, their order, and their usable area.
//! Ported from Rectangle's `ScreenDetection.swift`.

use utwm_config::settings::{ScreenOrdering, Settings};

use crate::geometry::{PixelRect, Rect, px};

/// One display, in the platform's screen units.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Screen {
    /// The whole monitor.
    pub frame: PixelRect,
    /// The monitor minus the taskbar and docked app bars (`rcWork`), or on the
    /// Mac minus the menu bar and the Dock (`visibleFrame`).
    pub work: PixelRect,
    /// How many screen units one macOS point is: DPI ÷ 96 on Windows (a
    /// point maps onto a DIP), and 1 on the Mac, which works in points.
    pub scale: f64,
    pub primary: bool,
}

impl Screen {
    /// Rectangle's `adjustedVisibleFrame`: the work area minus the configured
    /// screen-edge gaps, which are in points.
    pub fn visible(&self, settings: &Settings) -> PixelRect {
        let gaps = settings.screen_edge_gaps;
        if settings.screen_edge_gaps_on_main_only && !self.primary {
            return self.work;
        }
        let inset = |points: f64| px(points * self.scale);
        let visible = PixelRect::new(
            self.work.left + inset(gaps.left),
            self.work.top + inset(gaps.top),
            self.work.right - inset(gaps.right),
            self.work.bottom - inset(gaps.bottom),
        );
        if visible.is_empty() {
            self.work
        } else {
            visible
        }
    }

    pub fn visible_rect(&self, settings: &Settings) -> Rect {
        self.visible(settings).to_rect()
    }
}

/// The screens in Rectangle's order, with the window's screen picked out.
#[derive(Clone, Debug, PartialEq)]
pub struct UsableScreens {
    pub ordered: Vec<Screen>,
    /// Index into `ordered`.
    pub current: usize,
    /// (previous, next), when there is somewhere to go.
    pub adjacent: Option<(usize, usize)>,
}

impl UsableScreens {
    /// `ScreenDetection.detectScreens(using:)`.
    pub fn detect(screens: &[Screen], window: PixelRect, settings: &Settings) -> Option<Self> {
        let first = *screens.first()?;
        if screens.len() == 1 {
            return Some(Self {
                ordered: vec![first],
                current: 0,
                adjacent: settings.traverse_single_screen.then_some((0, 0)),
            });
        }
        let ordered = order(screens, settings.screen_ordering);
        let current = containing(&ordered, window).unwrap_or(0);
        let adjacent = adjacent(ordered.len(), current);
        Some(Self {
            ordered,
            current,
            adjacent,
        })
    }

    /// Only `screen`, for actions sent to a particular display (Rectangle builds
    /// `UsableScreens(currentScreen:numScreens: 1)` the same way).
    pub fn single(screen: Screen) -> Self {
        Self {
            ordered: vec![screen],
            current: 0,
            adjacent: None,
        }
    }

    pub fn current_screen(&self) -> Screen {
        self.ordered[self.current]
    }

    pub fn len(&self) -> usize {
        self.ordered.len()
    }

    pub fn is_empty(&self) -> bool {
        self.ordered.is_empty()
    }
}

/// The screen holding the window: the one that contains it outright, else the
/// one with the largest share of it, else the primary.
pub fn containing(screens: &[Screen], window: PixelRect) -> Option<usize> {
    if let Some(index) = screens
        .iter()
        .position(|screen| screen.frame.contains(window))
    {
        return Some(index);
    }
    let best = screens
        .iter()
        .enumerate()
        .map(|(index, screen)| {
            let shared = window.intersection(screen.frame).map_or(0, PixelRect::area);
            (index, shared)
        })
        .filter(|(_, shared)| *shared > 0)
        .max_by_key(|(_, shared)| *shared)
        .map(|(index, _)| index);
    best.or_else(|| screens.iter().position(|screen| screen.primary))
}

fn adjacent(count: usize, current: usize) -> Option<(usize, usize)> {
    match count {
        0 | 1 => None,
        2 => {
            let other = 1 - current.min(1);
            Some((other, other))
        }
        _ => {
            let next = (current + 1) % count;
            let prev = (current + count - 1) % count;
            Some((prev, next))
        }
    }
}

/// `ScreenDetection.order(screens:)`. The row-by-row comparison isn't a total
/// order for overlapping arrangements, and Rust's sort may panic on such a
/// comparator, so this is a stable insertion sort that never asks for one.
pub fn order(screens: &[Screen], ordering: ScreenOrdering) -> Vec<Screen> {
    let before = |a: &Screen, b: &Screen| -> bool {
        match ordering {
            ScreenOrdering::MidX => {
                i64::from(a.frame.left) + i64::from(a.frame.right)
                    < i64::from(b.frame.left) + i64::from(b.frame.right)
            }
            ScreenOrdering::MinX => a.frame.left < b.frame.left,
            ScreenOrdering::YThenMinX => {
                // Entirely above comes first; entirely below comes last.
                if a.frame.bottom <= b.frame.top {
                    true
                } else if b.frame.bottom <= a.frame.top {
                    false
                } else {
                    a.frame.left < b.frame.left
                }
            }
        }
    };
    let mut sorted: Vec<Screen> = Vec::with_capacity(screens.len());
    for screen in screens {
        let at = sorted
            .iter()
            .position(|existing| before(screen, existing))
            .unwrap_or(sorted.len());
        sorted.insert(at, *screen);
    }
    sorted
}

#[cfg(test)]
mod tests {
    use super::*;

    pub(crate) fn screen(left: i32, top: i32, width: i32, height: i32) -> Screen {
        let frame = PixelRect::from_xywh(left, top, width, height);
        Screen {
            frame,
            work: frame,
            scale: 1.0,
            primary: left == 0 && top == 0,
        }
    }

    #[test]
    fn one_screen_has_nowhere_to_go_unless_traversal_is_on() {
        let settings = Settings::default();
        let screens = [screen(0, 0, 1920, 1080)];
        let usable = UsableScreens::detect(&screens, PixelRect::new(0, 0, 10, 10), &settings)
            .expect("detects");
        assert_eq!(usable.adjacent, None);
        let traversing = Settings {
            traverse_single_screen: true,
            ..Settings::default()
        };
        let usable = UsableScreens::detect(&screens, PixelRect::new(0, 0, 10, 10), &traversing)
            .expect("detects");
        assert_eq!(usable.adjacent, Some((0, 0)));
    }

    #[test]
    fn row_by_row_then_left_to_right() {
        let laptop = screen(0, 0, 1920, 1080);
        let above = screen(0, -1440, 2560, 1440);
        let right = screen(1920, 0, 1920, 1080);
        let ordered = order(&[laptop, right, above], ScreenOrdering::YThenMinX);
        assert_eq!(ordered, [above, laptop, right]);
    }

    #[test]
    fn the_window_belongs_to_the_screen_holding_most_of_it() {
        let left = screen(0, 0, 1920, 1080);
        let right = screen(1920, 0, 1920, 1080);
        let straddling = PixelRect::new(1800, 100, 2400, 600);
        assert_eq!(containing(&[left, right], straddling), Some(1));
    }

    #[test]
    fn two_screens_point_at_each_other() {
        assert_eq!(adjacent(2, 0), Some((1, 1)));
        assert_eq!(adjacent(2, 1), Some((0, 0)));
        assert_eq!(adjacent(3, 0), Some((2, 1)));
    }

    #[test]
    fn edge_gaps_scale_with_dpi() {
        let mut settings = Settings::default();
        settings.screen_edge_gaps.top = 10.0;
        let mut hidpi = screen(0, 0, 3840, 2160);
        hidpi.scale = 1.5;
        assert_eq!(hidpi.visible(&settings).top, 15);
    }
}
