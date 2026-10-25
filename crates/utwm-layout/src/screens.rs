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

/// A way across the desktop, for finding the display beside another.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Direction {
    Left,
    Right,
    Up,
    Down,
}

/// The display beside `screens[current]` in `direction`: one lying wholly on
/// that side and overlapping it across the other axis, the nearest first, then
/// the one sharing most of that edge. By position, not by Rectangle's display
/// order, so it never wraps around: at the outermost display there is none.
pub fn neighbor(screens: &[Screen], current: usize, direction: Direction) -> Option<usize> {
    let from = screens.get(current)?.frame;
    screens
        .iter()
        .enumerate()
        .filter(|(index, _)| *index != current)
        .filter_map(|(index, screen)| {
            let to = screen.frame;
            let (beyond, gap, shared) = match direction {
                Direction::Left => (
                    to.right <= from.left,
                    from.left - to.right,
                    overlap((from.top, from.bottom), (to.top, to.bottom)),
                ),
                Direction::Right => (
                    to.left >= from.right,
                    to.left - from.right,
                    overlap((from.top, from.bottom), (to.top, to.bottom)),
                ),
                Direction::Up => (
                    to.bottom <= from.top,
                    from.top - to.bottom,
                    overlap((from.left, from.right), (to.left, to.right)),
                ),
                Direction::Down => (
                    to.top >= from.bottom,
                    to.top - from.bottom,
                    overlap((from.left, from.right), (to.left, to.right)),
                ),
            };
            (beyond && shared > 0).then_some((index, gap, shared))
        })
        .min_by_key(|(_, gap, shared)| (*gap, std::cmp::Reverse(*shared)))
        .map(|(index, _, _)| index)
}

/// How much two spans along one axis share.
fn overlap(a: (i32, i32), b: (i32, i32)) -> i32 {
    (a.1.min(b.1) - a.0.max(b.0)).max(0)
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
    fn neighbors_go_by_position_and_stop_at_the_edge() {
        // John's desk: a laptop, and a 4K display to its right, raised.
        let laptop = screen(0, 0, 2056, 1329);
        let wide = screen(2056, -669, 3840, 2160);
        let screens = [laptop, wide];
        assert_eq!(neighbor(&screens, 0, Direction::Right), Some(1));
        assert_eq!(neighbor(&screens, 1, Direction::Left), Some(0));
        assert_eq!(neighbor(&screens, 0, Direction::Left), None);
        assert_eq!(neighbor(&screens, 1, Direction::Right), None);
        // Neither sits above or below the other.
        assert_eq!(neighbor(&screens, 0, Direction::Up), None);
        assert_eq!(neighbor(&screens, 1, Direction::Down), None);

        let above = screen(0, -1440, 2560, 1440);
        let stacked = [laptop, above];
        assert_eq!(neighbor(&stacked, 0, Direction::Up), Some(1));
        assert_eq!(neighbor(&stacked, 1, Direction::Down), Some(0));
        assert_eq!(neighbor(&stacked, 1, Direction::Up), None);
    }

    #[test]
    fn the_nearest_display_wins_then_the_longest_shared_edge() {
        let middle = screen(0, 0, 1920, 1080);
        let near = screen(1920, 0, 1920, 1080);
        let far = screen(3840, 0, 1920, 1080);
        assert_eq!(neighbor(&[middle, far, near], 0, Direction::Right), Some(2));
        let short = screen(-1920, 900, 1920, 1080);
        let long = screen(-1920, 0, 1920, 1080);
        assert_eq!(
            neighbor(&[middle, short, long], 0, Direction::Left),
            Some(2)
        );
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
