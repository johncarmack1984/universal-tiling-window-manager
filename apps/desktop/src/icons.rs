//! The pictures beside the tray menu's actions: a screen with the part an
//! action puts the window in filled in, as Rectangle's menu shows them.
//!
//! Where the window goes comes from the layout engine, run on a screen of its
//! own with the configuration's settings, so a picture can't show one place
//! while the shortcut goes to another: a configured split ratio moves the
//! halves in the pictures too, and a custom position is drawn where it lands.
//! Actions that size a window rather than place it, or that depend on other
//! windows or on what came before, get arrows instead.
//!
//! The pictures are drawn here rather than shipped as image files, since the
//! custom positions an imported config brings can't be drawn ahead of time.
//! The screen and the windows in it keep their edges on whole points, so a
//! picture is as sharp on a screen with one pixel to a point as with two.

use utwm_config::settings::{EdgeGaps, Settings};
use utwm_config::{ActionId, ManualSpec, MenuGroup, Target};
use utwm_layout::geometry::px;
use utwm_layout::{Engine, PixelRect, Plan, Request, Screen};

/// What a picture shows.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Picture {
    /// A window in this part of the screen.
    Window(Area),
    /// Fill Left or Right: another window, and the room on this side of it
    /// filled.
    Fill(Side),
    /// Tiling: every window in a grid.
    Tiles { rows: u8, cols: u8 },
    /// A group of actions that each put the window in one share of the
    /// screen: the shares, the first filled and the rest faint.
    Shares { rows: u8, cols: u8 },
    /// Larger and Smaller, a plus and a minus as on their keys; their one-way
    /// kinds, arrows out from the middle or in from the edges.
    Resize { grow: bool, axis: Axis },
    /// Next or Previous Display: an arrow toward that display.
    Display(Side),
    /// Restore: an arrow turning back.
    Restore,
}

/// A part of the screen's usable area, in fractions of its width and height
/// from its top left corner.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Area {
    pub left: f64,
    pub top: f64,
    pub right: f64,
    pub bottom: f64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Side {
    Left,
    Right,
}

/// Which way a resize goes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Axis {
    Both,
    Width,
    Height,
}

/// The picture for an action, or `None` when the engine has nowhere to put a
/// window for it.
pub fn for_action(action: ActionId, settings: &Settings) -> Option<Picture> {
    let resize = |grow, axis| Some(Picture::Resize { grow, axis });
    match action {
        ActionId::Larger => resize(true, Axis::Both),
        ActionId::Smaller => resize(false, Axis::Both),
        ActionId::LargerWidth => resize(true, Axis::Width),
        ActionId::SmallerWidth => resize(false, Axis::Width),
        ActionId::LargerHeight => resize(true, Axis::Height),
        ActionId::SmallerHeight => resize(false, Axis::Height),
        // Keeps the window's width, which the picture can't know: a third of
        // the screen's, in the middle, stands in for it.
        ActionId::MaximizeHeight => Some(Picture::Window(Area {
            left: 1.0 / 3.0,
            top: 0.0,
            right: 2.0 / 3.0,
            bottom: 1.0,
        })),
        ActionId::NextDisplay => Some(Picture::Display(Side::Right)),
        ActionId::PreviousDisplay => Some(Picture::Display(Side::Left)),
        ActionId::Restore => Some(Picture::Restore),
        ActionId::FillLeft => Some(Picture::Fill(Side::Left)),
        ActionId::FillRight => Some(Picture::Fill(Side::Right)),
        ActionId::Tile { rows, cols } => Some(Picture::Tiles { rows, cols }),
        placed => place(Target::Action(placed), settings, &[]).map(Picture::Window),
    }
}

/// The picture for a custom position: where it puts a window half the
/// screen's size.
pub fn for_spec(spec: &ManualSpec, settings: &Settings) -> Option<Picture> {
    place(Target::Spec(spec.id), settings, std::slice::from_ref(spec)).map(Picture::Window)
}

/// The picture for a group in the menu: the shares of the screen its actions
/// put a window in, so Thirds and Fourths differ at a glance, or for the
/// groups that aren't shares, their first action's.
pub fn for_group(group: MenuGroup, settings: &Settings) -> Option<Picture> {
    let shares = |rows, cols| Some(Picture::Shares { rows, cols });
    match group {
        MenuGroup::Halves => shares(1, 2),
        MenuGroup::Corners => shares(2, 2),
        MenuGroup::Thirds => shares(1, 3),
        MenuGroup::Fourths => shares(1, 4),
        MenuGroup::Sixths => shares(2, 3),
        MenuGroup::Eighths => shares(2, 4),
        MenuGroup::Size | MenuGroup::Displays | MenuGroup::Pro => ActionId::all()
            .into_iter()
            .find(|action| action.group() == group)
            .and_then(|action| for_action(action, settings)),
    }
}

/// The screen the engine places windows on for the pictures: 3:2, as the
/// pictures' usable area is, with a hundred units to each point of it.
const SCREEN: PixelRect = PixelRect::new(0, 0, 1200, 800);

/// The window it moves: half the screen's size, against no edge and off
/// center, so every action moves it and Center has a size to keep.
const WINDOW: PixelRect = PixelRect::new(100, 100, 700, 500);

fn place(target: Target, settings: &Settings, specs: &[ManualSpec]) -> Option<Area> {
    // Gaps would take a fraction of a point off a picture twelve points
    // wide, which only blurs its edges.
    let settings = Settings {
        gap_size: 0.0,
        screen_edge_gaps: EdgeGaps::default(),
        ..settings.clone()
    };
    let screens = [Screen {
        frame: SCREEN,
        work: SCREEN,
        scale: 1.0,
        primary: true,
    }];
    let request = Request {
        target,
        window: WINDOW,
        screens: &screens,
        settings: &settings,
        specs,
        others: &[],
    };
    match Engine::<()>::new().plan(&(), &request) {
        Plan::Move(planned) => Some(Area::within(planned.rect, SCREEN)),
        Plan::Nothing(_) | Plan::AlreadyThere => None,
    }
}

impl Area {
    fn within(rect: PixelRect, screen: PixelRect) -> Self {
        let width = f64::from(screen.width());
        let height = f64::from(screen.height());
        Self {
            left: f64::from(rect.left - screen.left) / width,
            top: f64::from(rect.top - screen.top) / height,
            right: f64::from(rect.right - screen.left) / width,
            bottom: f64::from(rect.bottom - screen.top) / height,
        }
    }
}

/// How a platform's menus show a picture.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Style {
    /// The picture's width and height, in points.
    pub points: u32,
    /// Pixels to a point.
    pub scale: u32,
    pub palette: Palette,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Palette {
    /// Black at a few strengths, for a template image: macOS draws it in the
    /// menu's text color, light on a dark menu and dark on a light one, and
    /// white on the highlighted item.
    #[cfg_attr(
        not(target_os = "macos"),
        allow(dead_code, reason = "the Mac's menus draw this one")
    )]
    Template,
    /// A grey screen and a blue window, which read on a light menu and on a
    /// dark one.
    #[cfg_attr(
        target_os = "macos",
        allow(dead_code, reason = "Windows' and GNOME's menus draw this one")
    )]
    Colored,
}

impl Style {
    /// muda gives a Mac menu's picture 18 points of height, and the Retina
    /// screens most Macs have two pixels to a point; on one that has a pixel
    /// to a point, macOS halves the picture, which its whole-point edges
    /// survive.
    #[cfg(target_os = "macos")]
    pub const MENU: Self = Self {
        points: 18,
        scale: 2,
        palette: Palette::Template,
    };
    /// muda draws a Windows menu's picture into a 16 by 16 pixel bitmap,
    /// whatever the screen's scaling.
    #[cfg(windows)]
    pub const MENU: Self = Self {
        points: 16,
        scale: 1,
        palette: Palette::Colored,
    };
    /// GNOME's tray shows a menu's pictures 16 pixels high, scaled for a
    /// HiDPI screen: drawn at twice that, they stay sharp there and halve
    /// cleanly elsewhere.
    #[cfg(target_os = "linux")]
    pub const MENU: Self = Self {
        points: 16,
        scale: 2,
        palette: Palette::Colored,
    };
    /// The menu the GNOME Shell extension draws shows a picture 32 pixels
    /// across, two to each of its points, and twice that on a HiDPI screen:
    /// drawn at four, it stays sharp on both.
    #[cfg(target_os = "linux")]
    pub const SHELL: Self = Self {
        points: 16,
        scale: 4,
        palette: Palette::Colored,
    };
}

/// A drawn picture: RGBA rows from the top, alpha not premultiplied.
pub struct Icon {
    pub rgba: Vec<u8>,
    pub width: u32,
    pub height: u32,
}

impl Icon {
    /// The picture as a PNG file, the form the GNOME Shell extension takes
    /// it in.
    #[cfg(target_os = "linux")]
    pub fn png(&self) -> Option<Vec<u8>> {
        let mut file = Vec::new();
        let mut encoder = png::Encoder::new(&mut file, self.width, self.height);
        encoder.set_color(png::ColorType::Rgba);
        encoder.set_depth(png::BitDepth::Eight);
        let written = encoder.write_header().and_then(|mut writer| {
            writer.write_image_data(&self.rgba)?;
            writer.finish()
        });
        match written {
            Ok(()) => Some(file),
            Err(error) => {
                tracing::warn!(%error, "could not encode a menu picture");
                None
            }
        }
    }
}

/// The screen every picture draws, in points. Its outline is a point thick
/// and leaves a point of room around the usable area windows go in.
const SCREEN_WIDTH: f64 = 16.0;
const SCREEN_HEIGHT: f64 = 12.0;
const AREA: Bounds = Bounds::new(2.0, 2.0, 14.0, 10.0);
const SCREEN_RADIUS: f64 = 2.0;
const WINDOW_RADIUS: f64 = 0.5;
const STROKE: f64 = 1.0;
/// Arrows' shafts are two points thick, which puts them on the point grid
/// and in the middle of the usable area at once.
const SHAFT: f64 = 2.0;
const HEAD_LENGTH: f64 = 3.0;
const HEAD_WIDTH: f64 = 6.0;

/// Draws `picture` the way `style` says.
pub fn draw(picture: Picture, style: Style) -> Icon {
    let mut canvas = Canvas::new(style);
    canvas.paint(
        &[Shape::Outline {
            bounds: Bounds::new(0.0, 0.0, SCREEN_WIDTH, SCREEN_HEIGHT),
            radius: SCREEN_RADIUS,
            width: STROKE,
        }],
        Ink::Screen,
    );
    let middle = Point::new(
        (AREA.left + AREA.right) / 2.0,
        (AREA.top + AREA.bottom) / 2.0,
    );
    match picture {
        Picture::Window(area) => canvas.paint(
            &[Shape::Rect {
                bounds: area.bounds(),
                radius: WINDOW_RADIUS,
            }],
            Ink::Window,
        ),
        Picture::Fill(side) => {
            let other = Bounds::new(10.0, 3.0, 14.0, 9.0);
            let room = Bounds::new(AREA.left, AREA.top, 9.0, AREA.bottom);
            let (other, room) = match side {
                Side::Left => (other, room),
                Side::Right => (other.mirrored(), room.mirrored()),
            };
            canvas.paint(
                &[Shape::Outline {
                    bounds: other,
                    radius: WINDOW_RADIUS,
                    width: STROKE,
                }],
                Ink::Screen,
            );
            canvas.paint(
                &[Shape::Rect {
                    bounds: room,
                    radius: WINDOW_RADIUS,
                }],
                Ink::Window,
            );
        }
        Picture::Tiles { rows, cols } => canvas.paint(&cells(rows, cols), Ink::Window),
        Picture::Shares { rows, cols } => {
            let mut shares = cells(rows, cols);
            if !shares.is_empty() {
                let first = shares.remove(0);
                canvas.paint(&shares, Ink::Faint);
                canvas.paint(&[first], Ink::Window);
            }
        }
        Picture::Resize { grow, axis } => {
            // Which way the arrows point, how far out from the middle they
            // reach, and the screen's edge, where arrows pointing in start.
            let arrows = match axis {
                Axis::Both => None,
                Axis::Width => Some((Point::new(1.0, 0.0), 5.0, SCREEN_WIDTH / 2.0 - STROKE)),
                Axis::Height => Some((Point::new(0.0, 1.0), 4.0, SCREEN_HEIGHT / 2.0 - STROKE)),
            };
            let shapes = match arrows {
                None => plus(middle, grow),
                Some((along, out, _)) if grow => [
                    arrow(middle, middle + along * out),
                    arrow(middle, middle - along * out),
                ]
                .concat(),
                // Stopping a point short of the middle.
                Some((along, _, edge)) => [
                    arrow(middle + along * edge, middle + along),
                    arrow(middle - along * edge, middle - along),
                ]
                .concat(),
            };
            canvas.paint(&shapes, Ink::Window);
        }
        Picture::Display(side) => {
            let reach = Point::new(5.0, 0.0);
            let reach = match side {
                Side::Left => reach * -1.0,
                Side::Right => reach,
            };
            canvas.paint(&arrow(middle - reach, middle + reach), Ink::Window);
        }
        Picture::Restore => canvas.paint(&turn_back(middle), Ink::Window),
    }
    canvas.finish()
}

impl Area {
    /// Where the area falls in the picture, its edges on whole points. An
    /// edge off the screen's edge stays off it, by a point at least, so a
    /// window inset from the edges, as Almost Maximize leaves one, never
    /// comes out filling the screen.
    fn bounds(self) -> Bounds {
        let (left, right) = snap_span(self.left, self.right, AREA.left, AREA.right);
        let (top, bottom) = snap_span(self.top, self.bottom, AREA.top, AREA.bottom);
        Bounds::new(left, top, right, bottom)
    }
}

/// `value` on the nearest whole point, a half rounding away from `middle`, so
/// a picture that is symmetric stays symmetric.
fn snap(value: f64, middle: f64) -> f64 {
    let floor = value.floor();
    match value - floor {
        half if (half - 0.5).abs() < 1e-9 && value < middle => floor,
        half if (half - 0.5).abs() < 1e-9 => floor + 1.0,
        _ => value.round(),
    }
}

/// `start` and `end`, fractions of the span from `low` to `high`, on whole
/// points within it and at least a point apart.
fn snap_span(start: f64, end: f64, low: f64, high: f64) -> (f64, f64) {
    const TOUCHING: f64 = 1e-3;
    let length = high - low;
    let place = |fraction: f64| snap(low + fraction.clamp(0.0, 1.0) * length, low + length / 2.0);
    let inner = |value: f64| value.max(low + 1.0).min(high - 1.0);
    let start = if start <= TOUCHING {
        low
    } else {
        inner(place(start))
    };
    let end = if end >= 1.0 - TOUCHING {
        high
    } else {
        inner(place(end))
    };
    if end - start >= 1.0 {
        (start, end)
    } else if start + 1.0 <= high {
        (start, start + 1.0)
    } else {
        (high - 1.0, high)
    }
}

/// The cells of a `rows` by `cols` grid over the usable area, row by row,
/// at least a point apart.
fn cells(rows: u8, cols: u8) -> Vec<Shape> {
    const GAP: f64 = 1.0;
    let edges = |count: u8, low: f64, high: f64| -> Vec<(f64, f64)> {
        let count = count.max(1);
        let step = (high - low + GAP) / f64::from(count);
        let middle = (low + high) / 2.0;
        (0..count)
            .map(|index| {
                let start = snap(low + step * f64::from(index), middle);
                let end = snap(low + step * f64::from(index + 1) - GAP, middle);
                (start, end.max(start + 1.0))
            })
            .collect()
    };
    let columns = edges(cols, AREA.left, AREA.right);
    edges(rows, AREA.top, AREA.bottom)
        .into_iter()
        .flat_map(|(top, bottom)| {
            columns.iter().map(move |&(left, right)| Shape::Rect {
                bounds: Bounds::new(left, top, right, bottom),
                radius: WINDOW_RADIUS,
            })
        })
        .collect()
}

/// A plus in the middle of the screen, or with `grow` false a minus.
fn plus(middle: Point, grow: bool) -> Vec<Shape> {
    const ARM: f64 = 4.0;
    let line = |reach: Point| Shape::Line {
        from: middle - reach,
        to: middle + reach,
        width: SHAFT,
    };
    if grow {
        vec![line(Point::new(ARM, 0.0)), line(Point::new(0.0, ARM))]
    } else {
        vec![line(Point::new(ARM, 0.0))]
    }
}

/// An arrow from `from` with its tip at `tip`.
fn arrow(from: Point, tip: Point) -> Vec<Shape> {
    let along = (tip - from).unit();
    let across = along.perpendicular() * (HEAD_WIDTH / 2.0);
    let base = tip - along * HEAD_LENGTH;
    vec![
        Shape::Line {
            from,
            // Into the head, so no seam shows where they meet.
            to: base + along * 0.5,
            width: SHAFT,
        },
        Shape::Triangle([tip, base + across, base - across]),
    ]
}

/// A circle turning back on itself, counter-clockwise, as undo is drawn.
fn turn_back(center: Point) -> Vec<Shape> {
    const RADIUS: f64 = 3.0;
    const WIDTH: f64 = 1.5;
    // A smaller head than a straight arrow's, to fit inside the circle's
    // reach.
    const HEAD: f64 = 2.5;
    const HALF_HEAD: f64 = 2.4;
    // Around from the left to the top, three quarters of the way.
    let start = std::f64::consts::PI;
    let sweep = 1.5 * std::f64::consts::PI;
    let end = start + sweep;
    // Angles turn counter-clockwise on the screen, where y grows down.
    let on_circle = Point::new(end.cos(), -end.sin());
    let along = Point::new(-end.sin(), -end.cos());
    let at = center + on_circle * RADIUS;
    let across = on_circle * HALF_HEAD;
    vec![
        Shape::Arc {
            center,
            radius: RADIUS,
            width: WIDTH,
            start,
            sweep,
        },
        Shape::Triangle([at + along * HEAD, at + across, at - across]),
    ]
}

/// What a shape is painted in.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Ink {
    /// The screen, and any window other than the one an action moves.
    Screen,
    /// The window an action moves, and what it does to it.
    Window,
    /// The shares of the screen a group's other actions fill.
    Faint,
}

impl Palette {
    /// Straight RGBA, 0 to 1.
    fn color(self, ink: Ink) -> [f64; 4] {
        match (self, ink) {
            // Half strength, so the window stands out from its screen as the
            // menu's secondary text does from its primary.
            (Self::Template, Ink::Screen) => [0.0, 0.0, 0.0, 0.5],
            (Self::Template, Ink::Window) => [0.0, 0.0, 0.0, 1.0],
            (Self::Template, Ink::Faint) => [0.0, 0.0, 0.0, 0.25],
            // #8C8C8C: about 3:1 against both Windows 11's light and dark
            // menus, and GNOME's.
            (Self::Colored, Ink::Screen) => [0.549, 0.549, 0.549, 1.0],
            // #3B82F6: likewise, and blue as the accent both systems ship with.
            (Self::Colored, Ink::Window) => [0.231, 0.510, 0.965, 1.0],
            (Self::Colored, Ink::Faint) => [0.549, 0.549, 0.549, 0.4],
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
struct Point {
    x: f64,
    y: f64,
}

impl Point {
    const fn new(x: f64, y: f64) -> Self {
        Self { x, y }
    }

    fn unit(self) -> Self {
        let length = self.x.hypot(self.y);
        if length > 0.0 {
            self * (1.0 / length)
        } else {
            self
        }
    }

    fn perpendicular(self) -> Self {
        Self::new(-self.y, self.x)
    }

    fn dot(self, other: Self) -> f64 {
        self.x * other.x + self.y * other.y
    }

    fn cross(self, other: Self) -> f64 {
        self.x * other.y - self.y * other.x
    }
}

impl std::ops::Add for Point {
    type Output = Self;
    fn add(self, other: Self) -> Self {
        Self::new(self.x + other.x, self.y + other.y)
    }
}

impl std::ops::Sub for Point {
    type Output = Self;
    fn sub(self, other: Self) -> Self {
        Self::new(self.x - other.x, self.y - other.y)
    }
}

impl std::ops::Mul<f64> for Point {
    type Output = Self;
    fn mul(self, factor: f64) -> Self {
        Self::new(self.x * factor, self.y * factor)
    }
}

/// A rectangle in the picture, in points from the screen's top left corner.
#[derive(Clone, Copy, Debug, PartialEq)]
struct Bounds {
    left: f64,
    top: f64,
    right: f64,
    bottom: f64,
}

impl Bounds {
    const fn new(left: f64, top: f64, right: f64, bottom: f64) -> Self {
        Self {
            left,
            top,
            right,
            bottom,
        }
    }

    /// The same rectangle on the other side of the screen.
    fn mirrored(self) -> Self {
        Self::new(
            SCREEN_WIDTH - self.right,
            self.top,
            SCREEN_WIDTH - self.left,
            self.bottom,
        )
    }

    fn inset(self, by: f64) -> Self {
        Self::new(
            self.left + by,
            self.top + by,
            self.right - by,
            self.bottom - by,
        )
    }

    fn union(self, other: Self) -> Self {
        Self::new(
            self.left.min(other.left),
            self.top.min(other.top),
            self.right.max(other.right),
            self.bottom.max(other.bottom),
        )
    }

    /// Whether `point` is inside, with the corners rounded to `radius`.
    fn holds(self, point: Point, radius: f64) -> bool {
        if point.x < self.left
            || point.x > self.right
            || point.y < self.top
            || point.y > self.bottom
        {
            return false;
        }
        let nearest = Point::new(
            point.x.max(self.left + radius).min(self.right - radius),
            point.y.max(self.top + radius).min(self.bottom - radius),
        );
        let offset = point - nearest;
        offset.dot(offset) <= radius * radius
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum Shape {
    /// A rectangle with rounded corners.
    Rect {
        bounds: Bounds,
        radius: f64,
    },
    /// A rectangle's outline, `width` thick inside its bounds.
    Outline {
        bounds: Bounds,
        radius: f64,
        width: f64,
    },
    /// A straight stroke, its ends cut flat at its two points.
    Line {
        from: Point,
        to: Point,
        width: f64,
    },
    Triangle([Point; 3]),
    /// Part of a circle's outline, turning `sweep` radians counter-clockwise
    /// from `start`, an angle measured counter-clockwise from the right.
    Arc {
        center: Point,
        radius: f64,
        width: f64,
        start: f64,
        sweep: f64,
    },
}

impl Shape {
    fn bounds(self) -> Bounds {
        match self {
            Self::Rect { bounds, .. } | Self::Outline { bounds, .. } => bounds,
            Self::Line { from, to, width } => Bounds::new(
                from.x.min(to.x),
                from.y.min(to.y),
                from.x.max(to.x),
                from.y.max(to.y),
            )
            .inset(-width),
            Self::Triangle([a, b, c]) => Bounds::new(
                a.x.min(b.x).min(c.x),
                a.y.min(b.y).min(c.y),
                a.x.max(b.x).max(c.x),
                a.y.max(b.y).max(c.y),
            ),
            Self::Arc {
                center,
                radius,
                width,
                ..
            } => Bounds::new(center.x, center.y, center.x, center.y).inset(-(radius + width)),
        }
    }

    fn holds(self, point: Point) -> bool {
        match self {
            Self::Rect { bounds, radius } => bounds.holds(point, radius),
            Self::Outline {
                bounds,
                radius,
                width,
            } => {
                bounds.holds(point, radius)
                    && !bounds.inset(width).holds(point, (radius - width).max(0.0))
            }
            Self::Line { from, to, width } => {
                let along = to - from;
                let length = along.dot(along);
                if length <= 0.0 {
                    return false;
                }
                let offset = point - from;
                let t = offset.dot(along) / length;
                (0.0..=1.0).contains(&t) && offset.cross(along).abs() / length.sqrt() <= width / 2.0
            }
            Self::Triangle([a, b, c]) => {
                let sides = [
                    (b - a).cross(point - a),
                    (c - b).cross(point - b),
                    (a - c).cross(point - c),
                ];
                sides.iter().all(|side| *side >= 0.0) || sides.iter().all(|side| *side <= 0.0)
            }
            Self::Arc {
                center,
                radius,
                width,
                start,
                sweep,
            } => {
                let offset = point - center;
                if (offset.x.hypot(offset.y) - radius).abs() > width / 2.0 {
                    return false;
                }
                let angle = (-offset.y).atan2(offset.x);
                (angle - start).rem_euclid(std::f64::consts::TAU) <= sweep
            }
        }
    }
}

/// A picture being drawn.
struct Canvas {
    /// Pixels to a side.
    side: i32,
    scale: f64,
    /// Where the screen's top left corner sits, in points from the
    /// picture's.
    origin: Point,
    palette: Palette,
    /// Premultiplied RGBA, 0 to 1, rows from the top.
    pixels: Vec<[f64; 4]>,
}

impl Canvas {
    /// Samples to a side of each pixel, enough for smooth edges where a
    /// shape cuts across one.
    const SAMPLES: i32 = 4;

    fn new(style: Style) -> Self {
        let side = style.points.saturating_mul(style.scale);
        let points = f64::from(style.points);
        Self {
            side: i32::try_from(side).unwrap_or(i32::MAX),
            scale: f64::from(style.scale),
            origin: Point::new(
                (points - SCREEN_WIDTH) / 2.0,
                (points - SCREEN_HEIGHT) / 2.0,
            ),
            palette: style.palette,
            pixels: vec![[0.0; 4]; usize::try_from(side).unwrap_or(0).pow(2)],
        }
    }

    /// Paints `ink` wherever any of `shapes` reaches.
    fn paint(&mut self, shapes: &[Shape], ink: Ink) {
        let Some(bounds) = shapes
            .iter()
            .map(|shape| shape.bounds())
            .reduce(Bounds::union)
        else {
            return;
        };
        let [red, green, blue, alpha] = self.palette.color(ink);
        let pixel = |point: f64, origin: f64| (point + origin) * self.scale;
        let columns = px(pixel(bounds.left, self.origin.x).floor()).max(0)
            ..px(pixel(bounds.right, self.origin.x).ceil()).min(self.side);
        let rows = px(pixel(bounds.top, self.origin.y).floor()).max(0)
            ..px(pixel(bounds.bottom, self.origin.y).ceil()).min(self.side);
        let samples = f64::from(Self::SAMPLES);
        for row in rows {
            for column in columns.clone() {
                let mut hits = 0_u32;
                for sample_row in 0..Self::SAMPLES {
                    for sample_column in 0..Self::SAMPLES {
                        let at = |pixel: i32, sample: i32, origin: f64| {
                            (f64::from(pixel) + (f64::from(sample) + 0.5) / samples) / self.scale
                                - origin
                        };
                        let point = Point::new(
                            at(column, sample_column, self.origin.x),
                            at(row, sample_row, self.origin.y),
                        );
                        if shapes.iter().any(|shape| shape.holds(point)) {
                            hits += 1;
                        }
                    }
                }
                if hits == 0 {
                    continue;
                }
                let coverage = alpha * f64::from(hits) / (samples * samples);
                let Some(target) = usize::try_from(row * self.side + column)
                    .ok()
                    .and_then(|index| self.pixels.get_mut(index))
                else {
                    continue;
                };
                let source = [red * coverage, green * coverage, blue * coverage, coverage];
                for (channel, value) in target.iter_mut().zip(source) {
                    *channel = value + *channel * (1.0 - coverage);
                }
            }
        }
    }

    fn finish(self) -> Icon {
        let byte = |value: f64| {
            #[expect(
                clippy::cast_possible_truncation,
                clippy::cast_sign_loss,
                reason = "clamped to 0–255 and rounded just before"
            )]
            let byte = (value.clamp(0.0, 1.0) * 255.0).round() as u8;
            byte
        };
        let rgba = self
            .pixels
            .iter()
            .flat_map(|&[red, green, blue, alpha]| {
                let straight = |channel: f64| if alpha > 0.0 { channel / alpha } else { 0.0 };
                [
                    byte(straight(red)),
                    byte(straight(green)),
                    byte(straight(blue)),
                    byte(alpha),
                ]
            })
            .collect();
        let side = u32::try_from(self.side).unwrap_or(0);
        Icon {
            rgba,
            width: side,
            height: side,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use utwm_config::{Anchor, Dimension, Placement};

    /// A picture as Windows draws it, a character to a pixel: `#` where it
    /// is solid, `+` where it is faint or an edge crosses, `.` where it is
    /// empty.
    fn sketch(picture: Picture) -> String {
        let style = Style {
            points: 16,
            scale: 1,
            palette: Palette::Colored,
        };
        let icon = draw(picture, style);
        icon.rgba
            .chunks(4)
            .map(|pixel| match pixel[3] {
                0..=31 => '.',
                32..=223 => '+',
                _ => '#',
            })
            .collect::<Vec<_>>()
            .chunks(16)
            .map(|row| row.iter().collect::<String>())
            .collect::<Vec<_>>()
            .join("\n")
    }

    fn window(picture: Picture) -> Option<Area> {
        match picture {
            Picture::Window(area) => Some(area),
            _ => None,
        }
    }

    fn area_of(action: ActionId, settings: &Settings) -> Area {
        for_action(action, settings)
            .and_then(window)
            .expect("a window in the picture")
    }

    #[test]
    fn every_action_has_a_picture() {
        for action in ActionId::all() {
            assert!(
                for_action(action, &Settings::default()).is_some(),
                "{action}"
            );
        }
    }

    #[test]
    fn placements_come_from_the_engine() {
        let settings = Settings::default();
        let left = area_of(ActionId::LeftHalf, &settings);
        assert_eq!((left.left, left.right), (0.0, 0.5));
        assert_eq!((left.top, left.bottom), (0.0, 1.0));
        let third = area_of(ActionId::CenterThird, &settings);
        assert!((third.left - 1.0 / 3.0).abs() < 1e-9, "{third:?}");
        assert!((third.right - 2.0 / 3.0).abs() < 1e-9, "{third:?}");
        // Center keeps the window's size, half the screen's each way.
        let center = area_of(ActionId::Center, &settings);
        assert_eq!(
            (center.left, center.top, center.right, center.bottom),
            (0.25, 0.25, 0.75, 0.75)
        );
    }

    #[test]
    fn a_configured_split_moves_the_halves() {
        let settings = Settings {
            horizontal_split: 0.6,
            ..Settings::default()
        };
        let left = area_of(ActionId::LeftHalf, &settings);
        assert!((left.right - 0.6).abs() < 1e-6, "{left:?}");
        let right = area_of(ActionId::RightHalf, &settings);
        assert!((right.left - 0.6).abs() < 1e-6, "{right:?}");
    }

    #[test]
    fn gaps_stay_out_of_the_pictures() {
        let settings = Settings {
            gap_size: 10.0,
            screen_edge_gaps: EdgeGaps {
                top: 20.0,
                bottom: 20.0,
                left: 20.0,
                right: 20.0,
            },
            ..Settings::default()
        };
        assert_eq!(
            for_action(ActionId::Maximize, &settings),
            for_action(ActionId::Maximize, &Settings::default())
        );
    }

    #[test]
    fn a_custom_position_is_drawn_where_it_lands() {
        let spec = ManualSpec {
            id: 1,
            name: "Left 1/3 Below Movie".to_owned(),
            shortcut: None,
            placement: Placement {
                anchor: Anchor::BottomLeft,
                x: Dimension::Keep,
                y: Dimension::Keep,
                width: Dimension::Fraction(1.0 / 3.0),
                height: Dimension::Fraction(0.7),
                display: None,
            },
            repetitions: Vec::new(),
        };
        let area = for_spec(&spec, &Settings::default())
            .and_then(window)
            .expect("a window in the picture");
        assert_eq!((area.left, area.bottom), (0.0, 1.0));
        assert!((area.right - 1.0 / 3.0).abs() < 1e-2, "{area:?}");
        assert!((area.top - 0.3).abs() < 1e-2, "{area:?}");
    }

    fn picture(rows: [&str; 16]) -> String {
        rows.join("\n")
    }

    #[test]
    fn a_window_is_drawn_inside_its_screen() {
        let left = for_action(ActionId::LeftHalf, &Settings::default()).expect("a picture");
        assert_eq!(
            sketch(left),
            picture([
                "................",
                "................",
                "+##############+",
                "#+............+#",
                "#.######.......#",
                "#.######.......#",
                "#.######.......#",
                "#.######.......#",
                "#.######.......#",
                "#.######.......#",
                "#.######.......#",
                "#.######.......#",
                "#+............+#",
                "+##############+",
                "................",
                "................",
            ])
        );
    }

    #[test]
    fn an_inset_window_stays_off_the_edges() {
        // Almost Maximize's 90% would round out to the whole screen at this
        // size; it keeps a point clear on every side instead.
        let almost = for_action(ActionId::AlmostMaximize, &Settings::default()).expect("a picture");
        assert_eq!(
            sketch(almost),
            picture([
                "................",
                "................",
                "+##############+",
                "#+............+#",
                "#..............#",
                "#..##########..#",
                "#..##########..#",
                "#..##########..#",
                "#..##########..#",
                "#..##########..#",
                "#..##########..#",
                "#..............#",
                "#+............+#",
                "+##############+",
                "................",
                "................",
            ])
        );
    }

    #[test]
    fn a_group_shows_its_shares() {
        let thirds = for_group(MenuGroup::Thirds, &Settings::default()).expect("a picture");
        assert_eq!(
            sketch(thirds),
            picture([
                "................",
                "................",
                "+##############+",
                "#+............+#",
                "#.###.++++.+++.#",
                "#.###.++++.+++.#",
                "#.###.++++.+++.#",
                "#.###.++++.+++.#",
                "#.###.++++.+++.#",
                "#.###.++++.+++.#",
                "#.###.++++.+++.#",
                "#.###.++++.+++.#",
                "#+............+#",
                "+##############+",
                "................",
                "................",
            ])
        );
    }

    #[test]
    fn grids_stay_symmetric() {
        // Twelve points don't split into two cells a point apart; the gaps
        // widen in the middle rather than one cell coming out bigger.
        let tiles = Picture::Tiles { rows: 2, cols: 2 };
        assert_eq!(
            sketch(tiles),
            picture([
                "................",
                "................",
                "+##############+",
                "#+............+#",
                "#.#####..#####.#",
                "#.#####..#####.#",
                "#.#####..#####.#",
                "#..............#",
                "#..............#",
                "#.#####..#####.#",
                "#.#####..#####.#",
                "#.#####..#####.#",
                "#+............+#",
                "+##############+",
                "................",
                "................",
            ])
        );
    }

    #[test]
    fn template_pictures_are_black() {
        let style = Style {
            points: 18,
            scale: 2,
            palette: Palette::Template,
        };
        for action in ActionId::all() {
            let picture = for_action(action, &Settings::default()).expect("a picture");
            let icon = draw(picture, style);
            assert_eq!((icon.width, icon.height), (36, 36));
            assert_eq!(icon.rgba.len(), 36 * 36 * 4);
            assert!(
                icon.rgba.chunks(4).all(|pixel| pixel[..3] == [0, 0, 0]),
                "{action}"
            );
            assert!(icon.rgba.chunks(4).any(|pixel| pixel[3] == 255), "{action}");
        }
    }

    #[test]
    fn spans_snap_to_whole_points() {
        assert_eq!(snap_span(0.0, 0.5, 2.0, 14.0), (2.0, 8.0));
        // Halves round away from the middle, keeping the picture symmetric.
        assert_eq!(snap_span(0.125, 0.875, 2.0, 14.0), (3.0, 13.0));
        // A sliver still shows.
        assert_eq!(snap_span(0.5, 0.501, 2.0, 14.0), (8.0, 9.0));
        assert_eq!(snap_span(0.999, 0.9995, 2.0, 14.0), (13.0, 14.0));
    }
}
