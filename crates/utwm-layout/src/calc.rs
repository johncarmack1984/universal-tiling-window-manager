//! Where each action puts a window: Rectangle's `WindowCalculation` classes,
//! ported from rxhanson/Rectangle@7c5c9c5 (v2.0.4, MIT).
//!
//! Everything here runs on y-up [`Frame`]s so the formulas match the Swift line
//! for line; comments name the file each piece comes from. Settings that
//! Rectangle reads from `Defaults` arrive through [`Ctx`].

use utwm_config::ActionId;
use utwm_config::settings::{CornerAxis, RepeatMode, Settings};

use crate::geometry::{Frame, PixelRect, swift_float};
use crate::history::{ActionKey, LastAction};
use crate::screens::UsableScreens;
use crate::sub_action::SubAction;

/// Everything a calculation may read.
pub(crate) struct Ctx<'a> {
    pub action: ActionId,
    /// The window's visible frame.
    pub window: Frame,
    pub window_pixels: PixelRect,
    pub screens: &'a UsableScreens,
    pub last: Option<&'a LastAction>,
    pub settings: &'a Settings,
    /// The frame recorded before the last Maximize, for
    /// `repeatedMaximizeRestoresPrevious`.
    pub pre_maximize: Option<PixelRect>,
}

/// `WindowCalculationResult`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct CalcResult {
    pub rect: Frame,
    /// Index into `UsableScreens::ordered`.
    pub screen: usize,
    pub resulting_action: ActionId,
    pub sub: Option<SubAction>,
    /// Set when Maximize records the frame to come back to.
    pub remember_pre_maximize: Option<PixelRect>,
    /// Set when the result must not count as a repeat of the window's last
    /// action (Rectangle removes the last action before recording).
    pub forget_last: bool,
}

/// `RectResult`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct RectResult {
    pub rect: Frame,
    pub resulting_action: Option<ActionId>,
    pub sub: Option<SubAction>,
}

impl RectResult {
    fn plain(rect: Frame) -> Self {
        Self {
            rect,
            resulting_action: None,
            sub: None,
        }
    }

    fn sub(rect: Frame, sub: SubAction) -> Self {
        Self {
            rect,
            resulting_action: None,
            sub: Some(sub),
        }
    }
}

impl Ctx<'_> {
    fn visible(&self) -> Frame {
        self.visible_of(self.screens.current)
    }

    fn visible_of(&self, screen: usize) -> Frame {
        Frame::from_pixels(self.screens.ordered[screen].visible(self.settings))
    }

    fn mode(&self) -> RepeatMode {
        self.settings.repeat_mode
    }

    /// Pixels per point on the window's screen: Rectangle's sizes and gaps are
    /// in macOS points, which map onto Windows DIPs.
    fn scale(&self) -> f64 {
        self.screens.ordered[self.screens.current].scale
    }

    fn last_action(&self) -> Option<ActionId> {
        match self.last?.key {
            ActionKey::Action(action) => Some(action),
            ActionKey::Spec(_) => None,
        }
    }

    fn last_sub(&self) -> Option<SubAction> {
        self.last?.sub
    }

    /// `isRepeatedCommand`: this action just ran on this window and the window
    /// hasn't moved since.
    fn is_repeated(&self, action: ActionId) -> bool {
        self.last.is_some_and(|last| {
            last.key == ActionKey::Action(action) && last.rect == self.window_pixels
        })
    }

    fn result(&self, rect: RectResult) -> CalcResult {
        CalcResult {
            rect: rect.rect,
            screen: self.screens.current,
            resulting_action: rect.resulting_action.unwrap_or(self.action),
            sub: rect.sub,
            remember_pre_maximize: None,
            forget_last: false,
        }
    }
}

// ---------------------------------------------------------------------------
// HalfSplitFrameCalculation (WindowCalculation.swift)

#[derive(Clone, Copy, PartialEq, Eq)]
enum Side {
    Leading,
    Trailing,
}

fn floor_dimension(value: f64) -> f64 {
    (value + 0.0001).floor()
}

fn horizontal_rect(visible: Frame, side: Side, fraction: f64) -> Frame {
    let mut rect = visible;
    rect.w = floor_dimension(visible.w * fraction);
    if side == Side::Trailing {
        rect.x = visible.max_x() - rect.w;
    }
    rect
}

fn vertical_rect(visible: Frame, side: Side, fraction: f64) -> Frame {
    let mut rect = visible;
    rect.h = floor_dimension(visible.h * fraction);
    if side == Side::Leading {
        rect.y = visible.max_y() - rect.h;
    }
    rect
}

fn corner_rect(visible: Frame, horizontal: Side, vertical: Side, hf: f64, vf: f64) -> Frame {
    let h = horizontal_rect(visible, horizontal, hf);
    let v = vertical_rect(visible, vertical, vf);
    Frame {
        x: h.x,
        w: h.w,
        y: v.y,
        h: v.h,
    }
}

fn split(settings: &Settings, side: Side, horizontal: bool) -> f64 {
    let ratio = swift_float(if horizontal {
        settings.horizontal_split
    } else {
        settings.vertical_split
    });
    if side == Side::Trailing {
        swift_float(1.0 - ratio)
    } else {
        ratio
    }
}

/// `RepeatedExecutionsCalculation.calculateRepeatedRect`: the size for this
/// press, counted from how many times the action already ran in a row.
fn repeated_size(ctx: &Ctx<'_>) -> Option<f64> {
    let last = ctx.last?;
    if last.key != ActionKey::Action(ctx.action) {
        return None;
    }
    let sizes = ctx.settings.cycle_sizes.sizes();
    if sizes.is_empty() {
        return None;
    }
    let index = ctx.settings.cycle_sizes.index_for(last.count);
    sizes.get(index).map(|size| swift_float(size.fraction()))
}

// ---------------------------------------------------------------------------
// Halves

/// `LeftRightHalfCalculation.calculate`.
fn left_right_half(ctx: &Ctx<'_>) -> CalcResult {
    match ctx.mode() {
        RepeatMode::AcrossMonitor => across_displays(ctx),
        RepeatMode::AcrossAndResize => {
            if ctx.screens.len() == 1 {
                ctx.result(half_resize(ctx, ctx.action))
            } else {
                across_displays(ctx)
            }
        }
        RepeatMode::Resize | RepeatMode::ResizeAndCycleQuadrants => {
            ctx.result(half_resize(ctx, ctx.action))
        }
        RepeatMode::SwitchSides if ctx.is_repeated(ctx.action) => {
            let opposite = if ctx.action == ActionId::RightHalf {
                ActionId::LeftHalf
            } else {
                ActionId::RightHalf
            };
            CalcResult {
                resulting_action: opposite,
                ..ctx.result(RectResult::plain(half_first(
                    ctx.settings,
                    ctx.visible(),
                    opposite,
                )))
            }
        }
        RepeatMode::None | RepeatMode::CycleMonitor | RepeatMode::SwitchSides => ctx.result(
            RectResult::plain(half_first(ctx.settings, ctx.visible(), ctx.action)),
        ),
    }
}

fn half_side(action: ActionId) -> Side {
    if action == ActionId::RightHalf {
        Side::Trailing
    } else {
        Side::Leading
    }
}

/// `LeftRightHalfCalculation.calculateFirstRect`.
fn half_first(settings: &Settings, visible: Frame, action: ActionId) -> Frame {
    let side = half_side(action);
    horizontal_rect(visible, side, split(settings, side, true))
}

fn half_resize(ctx: &Ctx<'_>, action: ActionId) -> RectResult {
    let visible = ctx.visible();
    match repeated_size(ctx) {
        Some(fraction) => RectResult::plain(horizontal_rect(visible, half_side(action), fraction)),
        None => RectResult::plain(half_first(ctx.settings, visible, action)),
    }
}

fn across_displays(ctx: &Ctx<'_>) -> CalcResult {
    if ctx.action == ActionId::RightHalf {
        right_across(ctx, ctx.screens.current, true)
    } else {
        left_across(ctx, ctx.screens.current, true)
    }
}

/// `calculateLeftAcrossDisplays`. `original` is false once the action has
/// already been switched to the other half, which is never a repeat.
fn left_across(ctx: &Ctx<'_>, screen: usize, original: bool) -> CalcResult {
    if original
        && ctx.is_repeated(ActionId::LeftHalf)
        && let Some((prev, _)) = ctx.screens.adjacent
    {
        if ctx.mode() == RepeatMode::AcrossAndResize && prev == ctx.screens.len() - 1 {
            return ctx.result(half_resize(ctx, ActionId::LeftHalf));
        }
        return right_across(ctx, prev, false);
    }
    CalcResult {
        rect: half_first(ctx.settings, ctx.visible_of(screen), ActionId::LeftHalf),
        screen,
        resulting_action: ActionId::LeftHalf,
        sub: None,
        remember_pre_maximize: None,
        forget_last: false,
    }
}

fn right_across(ctx: &Ctx<'_>, screen: usize, original: bool) -> CalcResult {
    if original
        && ctx.is_repeated(ActionId::RightHalf)
        && let Some((_, next)) = ctx.screens.adjacent
    {
        if ctx.mode() == RepeatMode::AcrossAndResize && next == 0 {
            return ctx.result(half_resize(ctx, ActionId::RightHalf));
        }
        return left_across(ctx, next, false);
    }
    CalcResult {
        rect: half_first(ctx.settings, ctx.visible_of(screen), ActionId::RightHalf),
        screen,
        resulting_action: ActionId::RightHalf,
        sub: None,
        remember_pre_maximize: None,
        forget_last: false,
    }
}

/// `TopHalfCalculation` / `BottomHalfCalculation`.
fn top_bottom_half(ctx: &Ctx<'_>) -> RectResult {
    let visible = ctx.visible();
    let top = ctx.action == ActionId::TopHalf;
    let side = if top { Side::Leading } else { Side::Trailing };
    let first = |side: Side| vertical_rect(visible, side, split(ctx.settings, side, false));
    if ctx.mode() == RepeatMode::SwitchSides {
        if !ctx.is_repeated(ctx.action) {
            return RectResult::plain(first(side));
        }
        let (opposite_side, opposite) = if top {
            (Side::Trailing, ActionId::BottomHalf)
        } else {
            (Side::Leading, ActionId::TopHalf)
        };
        return RectResult {
            rect: first(opposite_side),
            resulting_action: Some(opposite),
            sub: None,
        };
    }
    if ctx.last.is_none() || !ctx.mode().resizes() {
        return RectResult::plain(first(side));
    }
    match repeated_size(ctx) {
        Some(fraction) => RectResult::plain(vertical_rect(visible, side, fraction)),
        None => RectResult::plain(first(side)),
    }
}

/// `CenterHalfCalculation`.
fn center_half(ctx: &Ctx<'_>) -> RectResult {
    let visible = ctx.visible();
    let fractional = |fraction: f64| {
        if visible.is_landscape() {
            let mut rect = visible;
            rect.h = visible.h;
            rect.w = (visible.w * fraction).round();
            rect.x = ((visible.w - rect.w) / 2.0).round() + visible.min_x();
            rect.y = ((visible.h - rect.h) / 2.0).round() + visible.min_y();
            RectResult::sub(rect, SubAction::CenterVerticalHalf)
        } else {
            let mut rect = visible;
            rect.w = visible.w;
            rect.h = (visible.h * fraction).round();
            rect.x = ((visible.w - rect.w) / 2.0).round() + visible.min_x();
            rect.y = ((visible.h - rect.h) / 2.0).round() + visible.min_y();
            RectResult::sub(rect, SubAction::CenterHorizontalHalf)
        }
    };
    if (ctx.last.is_some() && ctx.mode().resizes()) || ctx.settings.center_half_cycles {
        return fractional(repeated_size(ctx).unwrap_or(0.5));
    }
    fractional(0.5)
}

// ---------------------------------------------------------------------------
// Corners (UpperLeftCalculation and friends, QuartersRepeated)

fn corner_sides(action: ActionId) -> (Side, Side, SubAction) {
    match action {
        ActionId::TopRight => (Side::Trailing, Side::Leading, SubAction::TopRightQuarter),
        ActionId::BottomLeft => (Side::Leading, Side::Trailing, SubAction::BottomLeftQuarter),
        ActionId::BottomRight => (
            Side::Trailing,
            Side::Trailing,
            SubAction::BottomRightQuarter,
        ),
        _ => (Side::Leading, Side::Leading, SubAction::TopLeftQuarter),
    }
}

fn quarter_rect(settings: &Settings, visible: Frame, quarter: SubAction) -> RectResult {
    let action = match quarter {
        SubAction::TopRightQuarter => ActionId::TopRight,
        SubAction::BottomLeftQuarter => ActionId::BottomLeft,
        SubAction::BottomRightQuarter => ActionId::BottomRight,
        _ => ActionId::TopLeft,
    };
    let (h, v, sub) = corner_sides(action);
    RectResult::sub(
        corner_rect(
            visible,
            h,
            v,
            split(settings, h, true),
            split(settings, v, false),
        ),
        sub,
    )
}

fn corner(ctx: &Ctx<'_>) -> RectResult {
    let visible = ctx.visible();
    let (h, v, quarter) = corner_sides(ctx.action);
    if ctx.mode().cycles_quadrant_positions() {
        if let (Some(last), Some(last_sub)) = (ctx.last_action(), ctx.last_sub())
            && (last == ctx.action || last_sub == quarter)
        {
            // QuartersRepeated, direction .right: clockwise round the quadrants.
            let next = match last_sub {
                SubAction::TopLeftQuarter => Some(SubAction::TopRightQuarter),
                SubAction::TopRightQuarter => Some(SubAction::BottomLeftQuarter),
                SubAction::BottomLeftQuarter => Some(SubAction::BottomRightQuarter),
                SubAction::BottomRightQuarter => Some(SubAction::TopLeftQuarter),
                _ => None,
            };
            if let Some(next) = next {
                return quarter_rect(ctx.settings, visible, next);
            }
        }
        return quarter_rect(ctx.settings, visible, quarter);
    }
    let hf = split(ctx.settings, h, true);
    let vf = split(ctx.settings, v, false);
    let first = corner_rect(visible, h, v, hf, vf);
    if ctx.last.is_none() || !ctx.mode().resizes() {
        return RectResult::plain(first);
    }
    // CornerCycleExpansionCalculation.calculateFractionalRect
    match repeated_size(ctx) {
        Some(fraction) => match ctx.settings.corner_axis {
            CornerAxis::Horizontal => {
                let cycled = corner_rect(visible, h, v, fraction, vf);
                RectResult::plain(Frame {
                    x: cycled.x,
                    w: cycled.w,
                    y: first.y,
                    h: first.h,
                })
            }
            CornerAxis::Vertical => {
                let cycled = corner_rect(visible, h, v, hf, fraction);
                RectResult::plain(Frame {
                    x: first.x,
                    w: first.w,
                    y: cycled.y,
                    h: cycled.h,
                })
            }
        },
        None => RectResult::plain(first),
    }
}

// ---------------------------------------------------------------------------
// Thirds (FirstThirdCalculation, CenterThirdCalculation, LastThirdCalculation,
// and the two-thirds variants)

fn first_third_rect(visible: Frame) -> RectResult {
    let mut rect = visible;
    if visible.is_landscape() {
        rect.w = (visible.w / 3.0).floor();
        RectResult::sub(rect, SubAction::LeftThird)
    } else {
        rect.h = (visible.h / 3.0).floor();
        rect.y = visible.min_y() + visible.h - rect.h;
        RectResult::sub(rect, SubAction::TopThird)
    }
}

fn center_third_rect(visible: Frame) -> RectResult {
    let mut rect = visible;
    if visible.is_landscape() {
        rect.x = visible.min_x() + (visible.w / 3.0).floor();
        rect.y = visible.min_y();
        rect.w = visible.w / 3.0;
        rect.h = visible.h;
        RectResult::sub(rect, SubAction::CenterVerticalThird)
    } else {
        rect.x = visible.min_x();
        rect.y = visible.min_y() + (visible.h / 3.0).floor();
        rect.w = visible.w;
        rect.h = visible.h / 3.0;
        RectResult::sub(rect, SubAction::CenterHorizontalThird)
    }
}

fn last_third_rect(visible: Frame) -> RectResult {
    let mut rect = visible;
    if visible.is_landscape() {
        rect.w = (visible.w / 3.0).floor();
        rect.x = visible.x + visible.w - rect.w;
        RectResult::sub(rect, SubAction::RightThird)
    } else {
        rect.h = (visible.h / 3.0).floor();
        RectResult::sub(rect, SubAction::BottomThird)
    }
}

fn first_third(ctx: &Ctx<'_>) -> RectResult {
    let visible = ctx.visible();
    if ctx.mode() != RepeatMode::None
        && let (Some(last), Some(sub)) = (ctx.last_action(), ctx.last_sub())
    {
        match (last, sub) {
            (ActionId::FirstThird, SubAction::TopThird | SubAction::LeftThird)
            | (ActionId::LastThird, SubAction::TopThird | SubAction::LeftThird) => {
                return center_third_rect(visible);
            }
            (
                ActionId::FirstThird,
                SubAction::CenterHorizontalThird | SubAction::CenterVerticalThird,
            ) => return last_third(ctx),
            _ => {}
        }
    }
    first_third_rect(visible)
}

fn last_third(ctx: &Ctx<'_>) -> RectResult {
    let visible = ctx.visible();
    if ctx.mode() != RepeatMode::None
        && let (Some(last), Some(sub)) = (ctx.last_action(), ctx.last_sub())
    {
        match (last, sub) {
            (ActionId::LastThird, SubAction::BottomThird | SubAction::RightThird)
            | (ActionId::FirstThird, SubAction::BottomThird | SubAction::RightThird) => {
                return center_third_rect(visible);
            }
            (
                ActionId::LastThird,
                SubAction::CenterHorizontalThird | SubAction::CenterVerticalThird,
            ) => return first_third(ctx),
            _ => {}
        }
    }
    last_third_rect(visible)
}

fn first_two_thirds_rect(visible: Frame) -> RectResult {
    let mut rect = visible;
    if visible.is_landscape() {
        rect.w = (visible.w * 2.0 / 3.0).floor();
        RectResult::sub(rect, SubAction::LeftTwoThirds)
    } else {
        rect.h = (visible.h * 2.0 / 3.0).floor();
        rect.y = visible.y + visible.h - rect.h;
        RectResult::sub(rect, SubAction::TopTwoThirds)
    }
}

fn last_two_thirds_rect(visible: Frame) -> RectResult {
    let mut rect = visible;
    if visible.is_landscape() {
        rect.w = (visible.w * 2.0 / 3.0).floor();
        rect.x = visible.min_x() + visible.w - rect.w;
        RectResult::sub(rect, SubAction::RightTwoThirds)
    } else {
        rect.h = (visible.h * 2.0 / 3.0).floor();
        RectResult::sub(rect, SubAction::BottomTwoThirds)
    }
}

fn first_two_thirds(ctx: &Ctx<'_>) -> RectResult {
    let visible = ctx.visible();
    if ctx.mode() != RepeatMode::None
        && ctx.last.is_some()
        && let Some(SubAction::LeftTwoThirds | SubAction::TopTwoThirds) = ctx.last_sub()
    {
        return last_two_thirds_rect(visible);
    }
    first_two_thirds_rect(visible)
}

fn last_two_thirds(ctx: &Ctx<'_>) -> RectResult {
    let visible = ctx.visible();
    if ctx.mode() != RepeatMode::None
        && ctx.last.is_some()
        && let Some(SubAction::RightTwoThirds | SubAction::BottomTwoThirds) = ctx.last_sub()
    {
        return first_two_thirds_rect(visible);
    }
    last_two_thirds_rect(visible)
}

fn center_two_thirds(visible: Frame) -> RectResult {
    let mut rect = visible;
    if visible.is_landscape() {
        rect.x = visible.min_x() + (visible.w / 3.0).floor() / 2.0;
        rect.y = visible.min_y();
        rect.w = visible.w / 3.0 * 2.0;
        rect.h = visible.h;
        RectResult::sub(rect, SubAction::CenterVerticalThird)
    } else {
        rect.x = visible.min_x();
        rect.y = visible.min_y() + (visible.h / 3.0).floor() / 2.0;
        rect.w = visible.w;
        rect.h = visible.h / 3.0 * 2.0;
        RectResult::sub(rect, SubAction::CenterHorizontalThird)
    }
}

// ---------------------------------------------------------------------------
// Fourths and three-fourths

fn fourth_rect(visible: Frame, index: u8) -> RectResult {
    let mut rect = visible;
    if visible.is_landscape() {
        rect.w = (visible.w / 4.0).floor();
        let (x, sub) = match index {
            0 => (visible.min_x(), SubAction::LeftFourth),
            1 => (visible.min_x() + rect.w, SubAction::CenterLeftFourth),
            2 => (visible.min_x() + rect.w * 2.0, SubAction::CenterRightFourth),
            _ => (visible.x + visible.w - rect.w, SubAction::RightFourth),
        };
        rect.x = x;
        RectResult::sub(rect, sub)
    } else {
        rect.h = (visible.h / 4.0).floor();
        let (y, sub) = match index {
            0 => (visible.y + visible.h - rect.h, SubAction::TopFourth),
            1 => (
                visible.min_y() + visible.h - rect.h * 2.0,
                SubAction::CenterTopFourth,
            ),
            2 => (
                visible.min_y() + visible.h - rect.h * 3.0,
                SubAction::CenterBottomFourth,
            ),
            _ => (visible.y, SubAction::BottomFourth),
        };
        rect.y = y;
        RectResult::sub(rect, sub)
    }
}

fn three_fourths_rect(visible: Frame, which: u8) -> RectResult {
    let mut rect = visible;
    match (visible.is_landscape(), which) {
        (true, 0) => {
            rect.w = (visible.w * 3.0 / 4.0).floor();
            RectResult::sub(rect, SubAction::LeftThreeFourths)
        }
        (true, 1) => {
            rect.x = visible.min_x() + (visible.w / 4.0).floor() / 2.0;
            rect.y = visible.min_y();
            rect.w = visible.w / 4.0 * 3.0;
            rect.h = visible.h;
            RectResult::sub(rect, SubAction::CenterVerticalThreeFourths)
        }
        (true, _) => {
            rect.w = (visible.w * 3.0 / 4.0).floor();
            rect.x = visible.min_x() + visible.w - rect.w;
            RectResult::sub(rect, SubAction::RightThreeFourths)
        }
        (false, 0) => {
            rect.h = (visible.h * 3.0 / 4.0).floor();
            rect.y = visible.y + visible.h - rect.h;
            RectResult::sub(rect, SubAction::TopThreeFourths)
        }
        (false, 1) => {
            rect.x = visible.min_x();
            rect.y = visible.min_y() + (visible.h / 4.0).floor() / 2.0;
            rect.w = visible.w;
            rect.h = visible.h / 4.0 * 3.0;
            RectResult::sub(rect, SubAction::CenterHorizontalThreeFourths)
        }
        (false, _) => {
            rect.h = (visible.h * 3.0 / 4.0).floor();
            RectResult::sub(rect, SubAction::BottomThreeFourths)
        }
    }
}

fn center_half_rect(visible: Frame, landscape: bool) -> RectResult {
    let mut rect = visible;
    if landscape {
        rect.w = (visible.w * 0.5).round();
        rect.x = ((visible.w - rect.w) / 2.0).round() + visible.min_x();
        rect.y = ((visible.h - rect.h) / 2.0).round() + visible.min_y();
        RectResult::sub(rect, SubAction::CenterVerticalHalf)
    } else {
        rect.h = (visible.h * 0.5).round();
        rect.x = ((visible.w - rect.w) / 2.0).round() + visible.min_x();
        rect.y = ((visible.h - rect.h) / 2.0).round() + visible.min_y();
        RectResult::sub(rect, SubAction::CenterHorizontalHalf)
    }
}

/// `FirstFourthCalculation`.
fn first_fourth(ctx: &Ctx<'_>) -> RectResult {
    let visible = ctx.visible();
    if ctx.mode() != RepeatMode::None
        && ctx.action == ActionId::FirstFourth
        && let (Some(last), Some(sub)) = (ctx.last_action(), ctx.last_sub())
    {
        let next = match (last, sub) {
            (ActionId::FirstFourth, SubAction::TopFourth | SubAction::LeftFourth)
            | (ActionId::LastFourth, SubAction::LeftFourth | SubAction::TopFourth) => Some(1),
            (ActionId::FirstFourth, SubAction::CenterTopFourth | SubAction::CenterLeftFourth) => {
                Some(2)
            }
            (
                ActionId::FirstFourth,
                SubAction::CenterBottomFourth | SubAction::CenterRightFourth,
            ) => Some(3),
            _ => None,
        };
        if let Some(index) = next {
            return fourth_rect(visible, index);
        }
    }
    fourth_rect(visible, 0)
}

/// `SecondFourthCalculation`: second fourth → last three fourths → center half.
fn second_fourth(ctx: &Ctx<'_>) -> RectResult {
    let visible = ctx.visible();
    if ctx.mode() != RepeatMode::None && ctx.last_action() == Some(ActionId::SecondFourth) {
        match ctx.last_sub() {
            Some(SubAction::CenterLeftFourth) => return three_fourths_landscape(visible, 2),
            Some(SubAction::CenterTopFourth) => return three_fourths_portrait(visible, 2),
            Some(SubAction::RightThreeFourths) => return center_half_rect(visible, true),
            Some(SubAction::BottomThreeFourths) => return center_half_rect(visible, false),
            _ => {}
        }
    }
    fourth_rect(visible, 1)
}

/// `ThirdFourthCalculation`: third fourth → first three fourths → center half.
fn third_fourth(ctx: &Ctx<'_>) -> RectResult {
    let visible = ctx.visible();
    if ctx.mode() != RepeatMode::None && ctx.last_action() == Some(ActionId::ThirdFourth) {
        match ctx.last_sub() {
            Some(SubAction::CenterRightFourth) => return three_fourths_landscape(visible, 0),
            Some(SubAction::CenterBottomFourth) => return three_fourths_portrait(visible, 0),
            Some(SubAction::LeftThreeFourths) => return center_half_rect(visible, true),
            Some(SubAction::TopThreeFourths) => return center_half_rect(visible, false),
            _ => {}
        }
    }
    fourth_rect(visible, 2)
}

/// `LastFourthCalculation`.
fn last_fourth(ctx: &Ctx<'_>) -> RectResult {
    let visible = ctx.visible();
    if ctx.mode() != RepeatMode::None
        && let (Some(last), Some(sub)) = (ctx.last_action(), ctx.last_sub())
    {
        let next = match (last, sub) {
            (ActionId::LastFourth, SubAction::BottomFourth | SubAction::RightFourth)
            | (ActionId::FirstFourth, SubAction::BottomFourth | SubAction::RightFourth) => Some(2),
            (
                ActionId::LastFourth,
                SubAction::CenterBottomFourth | SubAction::CenterRightFourth,
            ) => Some(1),
            (ActionId::LastFourth, SubAction::CenterTopFourth | SubAction::CenterLeftFourth) => {
                Some(0)
            }
            _ => None,
        };
        if let Some(index) = next {
            return fourth_rect(visible, index);
        }
    }
    fourth_rect(visible, 3)
}

/// The landscape three-fourths whatever the screen's shape:
/// `SecondFourthCalculation` and `ThirdFourthCalculation` call `landscapeRect`
/// directly.
fn three_fourths_landscape(visible: Frame, which: u8) -> RectResult {
    let mut rect = visible;
    rect.w = (visible.w * 3.0 / 4.0).floor();
    if which == 0 {
        RectResult::sub(rect, SubAction::LeftThreeFourths)
    } else {
        rect.x = visible.min_x() + visible.w - rect.w;
        RectResult::sub(rect, SubAction::RightThreeFourths)
    }
}

fn three_fourths_portrait(visible: Frame, which: u8) -> RectResult {
    let mut rect = visible;
    rect.h = (visible.h * 3.0 / 4.0).floor();
    if which == 0 {
        rect.y = visible.y + visible.h - rect.h;
        RectResult::sub(rect, SubAction::TopThreeFourths)
    } else {
        RectResult::sub(rect, SubAction::BottomThreeFourths)
    }
}

fn first_three_fourths(ctx: &Ctx<'_>) -> RectResult {
    let visible = ctx.visible();
    if ctx.mode() != RepeatMode::None
        && ctx.last.is_some()
        && let Some(SubAction::LeftThreeFourths | SubAction::TopThreeFourths) = ctx.last_sub()
    {
        return three_fourths_rect(visible, 2);
    }
    three_fourths_rect(visible, 0)
}

fn last_three_fourths(ctx: &Ctx<'_>) -> RectResult {
    let visible = ctx.visible();
    if ctx.mode() != RepeatMode::None
        && ctx.last.is_some()
        && let Some(SubAction::RightThreeFourths | SubAction::BottomThreeFourths) = ctx.last_sub()
    {
        return three_fourths_rect(visible, 0);
    }
    three_fourths_rect(visible, 2)
}

// ---------------------------------------------------------------------------
// Sixths (TopLeftSixthCalculation and friends, SixthsRepeated)

#[derive(Clone, Copy, PartialEq, Eq)]
enum Sixth {
    TopLeft,
    TopCenter,
    TopRight,
    BottomLeft,
    BottomCenter,
    BottomRight,
}

fn sixth_rect(visible: Frame, sixth: Sixth) -> RectResult {
    let mut rect = visible;
    if visible.is_landscape() {
        rect.w = (visible.w / 3.0).floor();
        rect.h = (visible.h / 2.0).floor();
        let sub = match sixth {
            Sixth::TopLeft => {
                rect.y = visible.max_y() - rect.h;
                SubAction::TopLeftSixthLandscape
            }
            Sixth::TopCenter => {
                rect.x = visible.min_x() + rect.w;
                rect.y = visible.max_y() - rect.h;
                SubAction::TopCenterSixthLandscape
            }
            Sixth::TopRight => {
                rect.x = visible.min_x() + visible.w - rect.w;
                rect.y = visible.max_y() - rect.h;
                SubAction::TopRightSixthLandscape
            }
            Sixth::BottomLeft => SubAction::BottomLeftSixthLandscape,
            Sixth::BottomCenter => {
                rect.x = visible.x + rect.w;
                SubAction::BottomCenterSixthLandscape
            }
            Sixth::BottomRight => {
                rect.x = visible.x + visible.w - rect.w;
                SubAction::BottomRightSixthLandscape
            }
        };
        RectResult::sub(rect, sub)
    } else {
        rect.w = (visible.w / 2.0).floor();
        rect.h = (visible.h / 3.0).floor();
        let sub = match sixth {
            Sixth::TopLeft => {
                rect.y = visible.max_y() - rect.h;
                SubAction::TopLeftSixthPortrait
            }
            Sixth::TopCenter => {
                rect.y = visible.min_y() + rect.h;
                SubAction::LeftCenterSixthPortrait
            }
            Sixth::TopRight => {
                rect.x = visible.min_x() + rect.w;
                rect.y = visible.max_y() - rect.h;
                SubAction::TopRightSixthPortrait
            }
            Sixth::BottomLeft => SubAction::BottomLeftSixthPortrait,
            Sixth::BottomCenter => {
                rect.x = visible.x + rect.w;
                rect.y = visible.y + rect.h;
                SubAction::RightCenterSixthPortrait
            }
            Sixth::BottomRight => {
                rect.x = visible.x + visible.w - rect.w;
                SubAction::BottomRightSixthPortrait
            }
        };
        RectResult::sub(rect, sub)
    }
}

/// `SixthsRepeated.nextCalculation`.
fn next_sixth(sub: SubAction, rightward: bool) -> Option<Sixth> {
    use SubAction::*;
    Some(if rightward {
        match sub {
            TopLeftSixthLandscape => Sixth::TopCenter,
            TopCenterSixthLandscape => Sixth::TopRight,
            TopRightSixthLandscape => Sixth::BottomLeft,
            BottomLeftSixthLandscape => Sixth::BottomCenter,
            BottomCenterSixthLandscape => Sixth::BottomRight,
            BottomRightSixthLandscape => Sixth::TopLeft,
            TopLeftSixthPortrait => Sixth::TopRight,
            TopRightSixthPortrait => Sixth::TopCenter,
            LeftCenterSixthPortrait => Sixth::BottomCenter,
            RightCenterSixthPortrait => Sixth::BottomLeft,
            BottomLeftSixthPortrait => Sixth::BottomRight,
            BottomRightSixthPortrait => Sixth::TopLeft,
            _ => return None,
        }
    } else {
        match sub {
            TopLeftSixthLandscape => Sixth::BottomRight,
            TopCenterSixthLandscape => Sixth::TopLeft,
            TopRightSixthLandscape => Sixth::TopCenter,
            BottomLeftSixthLandscape => Sixth::TopRight,
            BottomCenterSixthLandscape => Sixth::BottomLeft,
            BottomRightSixthLandscape => Sixth::BottomCenter,
            TopLeftSixthPortrait => Sixth::BottomRight,
            TopRightSixthPortrait => Sixth::TopLeft,
            LeftCenterSixthPortrait => Sixth::TopRight,
            RightCenterSixthPortrait => Sixth::TopCenter,
            BottomLeftSixthPortrait => Sixth::BottomCenter,
            BottomRightSixthPortrait => Sixth::BottomLeft,
            _ => return None,
        }
    })
}

/// The four corner sixths: repeats walk the six positions, rightward from
/// the left ones and leftward from the right ones.
fn corner_sixth(ctx: &Ctx<'_>, sixth: Sixth) -> RectResult {
    let visible = ctx.visible();
    let (landscape_sub, portrait_sub, rightward) = match sixth {
        Sixth::TopLeft => (
            SubAction::TopLeftSixthLandscape,
            SubAction::TopLeftSixthPortrait,
            true,
        ),
        Sixth::TopRight => (
            SubAction::TopRightSixthLandscape,
            SubAction::TopRightSixthPortrait,
            false,
        ),
        Sixth::BottomLeft => (
            SubAction::BottomLeftSixthLandscape,
            SubAction::BottomLeftSixthPortrait,
            true,
        ),
        _ => (
            SubAction::BottomRightSixthLandscape,
            SubAction::BottomRightSixthPortrait,
            false,
        ),
    };
    if ctx.mode() != RepeatMode::None
        && let (Some(last), Some(sub)) = (ctx.last_action(), ctx.last_sub())
    {
        if last != ctx.action && sub != landscape_sub && sub != portrait_sub {
            return sixth_rect(visible, sixth);
        }
        if let Some(next) = next_sixth(sub, rightward) {
            return sixth_rect(visible, next);
        }
    }
    sixth_rect(visible, sixth)
}

fn two_sixths_rect(visible: Frame, top: bool, right: bool) -> RectResult {
    let mut rect = visible;
    let landscape = visible.is_landscape();
    if landscape {
        rect.w = (visible.w * 2.0 / 3.0).floor();
        rect.h = (visible.h / 2.0).floor();
    } else {
        rect.w = (visible.w / 2.0).floor();
        rect.h = (visible.h * 2.0 / 3.0).floor();
    }
    if right {
        rect.x = visible.max_x() - rect.w;
    }
    if top {
        rect.y = visible.max_y() - rect.h;
    }
    let sub = match (top, right, landscape) {
        (true, false, true) => SubAction::TopLeftTwoSixthsLandscape,
        (true, false, false) => SubAction::TopLeftTwoSixthsPortrait,
        (true, true, true) => SubAction::TopRightTwoSixthsLandscape,
        (true, true, false) => SubAction::TopRightTwoSixthsPortrait,
        (false, false, true) => SubAction::BottomLeftTwoSixthsLandscape,
        (false, false, false) => SubAction::BottomLeftTwoSixthsPortrait,
        (false, true, true) => SubAction::BottomRightTwoSixthsLandscape,
        (false, true, false) => SubAction::BottomRightTwoSixthsPortrait,
    };
    RectResult::sub(rect, sub)
}

/// `TopCenterSixthCalculation`: center sixth → right two sixths → left two
/// sixths, rather than walking positions.
fn top_center_sixth(ctx: &Ctx<'_>) -> RectResult {
    let visible = ctx.visible();
    if ctx.mode() != RepeatMode::None
        && ctx.last.is_some()
        && ctx.action == ActionId::TopCenterSixth
    {
        match ctx.last_sub() {
            Some(SubAction::TopCenterSixthLandscape) => {
                return two_sixths_rect(visible, true, true);
            }
            Some(SubAction::LeftCenterSixthPortrait) => {
                return two_sixths_rect(visible, false, false);
            }
            Some(
                SubAction::TopRightTwoSixthsLandscape | SubAction::BottomLeftTwoSixthsPortrait,
            ) => {
                return two_sixths_rect(visible, true, false);
            }
            _ => {}
        }
    }
    sixth_rect(visible, Sixth::TopCenter)
}

/// `BottomCenterSixthCalculation`.
fn bottom_center_sixth(ctx: &Ctx<'_>) -> RectResult {
    let visible = ctx.visible();
    if ctx.mode() != RepeatMode::None
        && ctx.last.is_some()
        && ctx.action == ActionId::BottomCenterSixth
    {
        match ctx.last_sub() {
            Some(SubAction::BottomCenterSixthLandscape | SubAction::RightCenterSixthPortrait) => {
                return two_sixths_rect(visible, false, true);
            }
            Some(SubAction::BottomRightTwoSixthsLandscape) => {
                return two_sixths_rect(visible, false, false);
            }
            Some(SubAction::BottomRightTwoSixthsPortrait) => {
                return two_sixths_rect(visible, true, true);
            }
            _ => {}
        }
    }
    sixth_rect(visible, Sixth::BottomCenter)
}

// ---------------------------------------------------------------------------
// Eighths (TopLeftEighthCalculation and friends, EighthsRepeated)

const EIGHTHS: [SubAction; 8] = [
    SubAction::TopLeftEighth,
    SubAction::TopCenterLeftEighth,
    SubAction::TopCenterRightEighth,
    SubAction::TopRightEighth,
    SubAction::BottomLeftEighth,
    SubAction::BottomCenterLeftEighth,
    SubAction::BottomCenterRightEighth,
    SubAction::BottomRightEighth,
];

fn eighth_rect(visible: Frame, index: usize) -> RectResult {
    let sub = EIGHTHS[index % 8];
    let mut rect = visible;
    if visible.is_landscape() {
        rect.w = (visible.w / 4.0).floor();
        rect.h = (visible.h / 2.0).floor();
        let column = (index % 4) as f64;
        rect.x = visible.min_x() + column * rect.w;
        rect.y = if index < 4 {
            visible.max_y() - rect.h
        } else {
            visible.min_y()
        };
    } else {
        // Portrait is two columns of four, in the positions Rectangle's
        // portraitRect functions give each eighth.
        rect.w = (visible.w / 2.0).floor();
        rect.h = (visible.h / 4.0).floor();
        let (column, rows_from_top) = match index % 8 {
            0 => (0.0, 1.0),
            1 => (1.0, 1.0),
            2 => (0.0, 2.0),
            3 => (1.0, 2.0),
            4 => (0.0, 3.0),
            5 => (1.0, 3.0),
            6 => (0.0, 4.0),
            _ => (1.0, 4.0),
        };
        rect.x = visible.min_x() + column * rect.w;
        rect.y = if rows_from_top >= 4.0 {
            visible.min_y()
        } else {
            visible.max_y() - rect.h * rows_from_top
        };
    }
    RectResult::sub(rect, sub)
}

fn eighth(ctx: &Ctx<'_>, index: usize) -> RectResult {
    let visible = ctx.visible();
    if ctx.mode() != RepeatMode::None
        && ctx.last_action() == Some(ctx.action)
        && let Some(position) = ctx
            .last_sub()
            .and_then(|sub| EIGHTHS.iter().position(|e| *e == sub))
    {
        return eighth_rect(visible, (position + 1) % 8);
    }
    eighth_rect(visible, index)
}

// ---------------------------------------------------------------------------
// Maximize, Almost Maximize, Maximize Height, Center

/// `RepeatedMaximizeRestore.calculate`. Some(restore rect) means "go back".
fn maximize_restore(ctx: &Ctx<'_>) -> Option<PixelRect> {
    if !ctx.settings.repeated_maximize_restores_previous {
        return None;
    }
    let last = ctx.last?;
    if last.key != ActionKey::Action(ctx.action) || last.rect != ctx.window_pixels {
        return None;
    }
    ctx.pre_maximize
}

fn maximize_like(ctx: &Ctx<'_>, rect: Frame) -> CalcResult {
    if let Some(previous) = maximize_restore(ctx) {
        return CalcResult {
            rect: Frame::from_pixels(previous),
            screen: ctx.screens.current,
            resulting_action: ActionId::Restore,
            sub: None,
            remember_pre_maximize: None,
            forget_last: false,
        };
    }
    CalcResult {
        remember_pre_maximize: ctx
            .settings
            .repeated_maximize_restores_previous
            .then_some(ctx.window_pixels),
        ..ctx.result(RectResult::plain(rect))
    }
}

fn almost_maximize_rect(settings: &Settings, visible: Frame) -> Frame {
    let mut rect = visible;
    rect.h = (visible.h * settings.almost_maximize_height).round();
    rect.w = (visible.w * settings.almost_maximize_width).round();
    rect.x = ((visible.w - rect.w) / 2.0).round() + visible.min_x();
    rect.y = ((visible.h - rect.h) / 2.0).round() + visible.min_y();
    rect
}

/// `CenterCalculation.calculateRect`.
fn center_rect(window: Frame, visible: Frame) -> RectResult {
    let mut rect = window;
    let height_exceeded = window.h > visible.h;
    let width_exceeded = window.w > visible.w;
    if height_exceeded && width_exceeded {
        return RectResult {
            rect: visible,
            resulting_action: Some(ActionId::Maximize),
            sub: None,
        };
    }
    if height_exceeded {
        rect.h = visible.h;
        rect.y = visible.min_y();
    } else {
        rect.y = ((visible.h - window.h) / 2.0).round() + visible.min_y();
    }
    if width_exceeded {
        rect.w = visible.w;
        rect.x = visible.min_x();
    } else {
        rect.x = ((visible.w - window.w) / 2.0).round() + visible.min_x();
    }
    RectResult::plain(rect)
}

// ---------------------------------------------------------------------------
// Larger / Smaller (ChangeSizeCalculation)

fn change_size(ctx: &Ctx<'_>) -> RectResult {
    let settings = ctx.settings;
    let visible = ctx.visible();
    let window = ctx.window;
    let scale = ctx.scale();
    let gap = settings.gap_size * scale;
    // Within this much of an edge counts as against it: the gap size, or 5 points.
    let edge_tolerance = if gap <= 0.0 { 5.0 * scale } else { gap };
    let against = |difference: f64| difference.abs() <= edge_tolerance;
    let left = |r: Frame| against(r.min_x() - visible.min_x());
    let right = |r: Frame| against(r.max_x() - visible.max_x());
    let top = |r: Frame| against(r.max_y() - visible.max_y());
    let bottom = |r: Frame| against(r.min_y() - visible.min_y());

    let offset = scale
        * match ctx.action {
            ActionId::Larger | ActionId::LargerHeight => settings.size_offset,
            ActionId::Smaller | ActionId::SmallerHeight => -settings.size_offset,
            ActionId::LargerWidth => settings.width_step,
            ActionId::SmallerWidth => -settings.width_step,
            _ => 0.0,
        };
    let changes_width = matches!(
        ctx.action,
        ActionId::Larger | ActionId::Smaller | ActionId::LargerWidth | ActionId::SmallerWidth
    );
    let changes_height = matches!(
        ctx.action,
        ActionId::Larger | ActionId::Smaller | ActionId::LargerHeight | ActionId::SmallerHeight
    );

    let mut resized = window;
    if changes_width {
        resized.w += offset;
        resized.x = resized.min_x() - (offset / 2.0).floor();
        if settings.curtain_change_size {
            // againstLeftAndRightScreenEdges
            if right(window) {
                resized.x = visible.max_x() - resized.w - gap;
                if left(window) {
                    resized.w = visible.w - gap * 2.0;
                }
            }
            if left(window) {
                resized.x = visible.min_x() + gap;
            }
        }
        if resized.w >= visible.w {
            resized.w = visible.w;
        }
    }
    if changes_height {
        resized.h += offset;
        resized.y = resized.min_y() - (offset / 2.0).floor();
        let exempt = if settings.smaller_shrinks_maximized_height {
            matches!(ctx.action, ActionId::Smaller | ActionId::SmallerHeight)
        } else {
            ctx.action == ActionId::SmallerHeight
        };
        if settings.curtain_change_size && !exempt {
            // againstTopAndBottomScreenEdges
            if top(window) {
                resized.y = visible.max_y() - resized.h - gap;
                if bottom(window) {
                    resized.h = visible.h - gap * 2.0;
                }
            }
            if bottom(window) {
                resized.y = visible.min_y() + gap;
            }
        }
        if resized.h >= visible.h {
            resized.h = visible.h;
            resized.y = window.min_y();
        }
    }

    let against_all = left(window) && right(window) && top(window) && bottom(window);
    if against_all && offset < 0.0 {
        if matches!(ctx.action, ActionId::Smaller | ActionId::SmallerWidth) {
            resized.w = window.w + offset;
            resized.x = window.x - (offset / 2.0).floor();
        }
        if matches!(ctx.action, ActionId::Smaller | ActionId::SmallerHeight) {
            resized.h = window.h + offset;
            resized.y = window.y - (offset / 2.0).floor();
        }
    }

    let shrinking = matches!(
        ctx.action,
        ActionId::Smaller | ActionId::SmallerWidth | ActionId::SmallerHeight
    );
    if shrinking {
        // ChangeWindowDimensionCalculation.resizedWindowRectIsTooSmall
        let min_width = (visible.w * settings.minimum_window_width).floor().max(1.0);
        let min_height = (visible.h * settings.minimum_window_height)
            .floor()
            .max(1.0);
        if resized.w < min_width || resized.h < min_height {
            resized = window;
        }
    }
    RectResult::plain(resized)
}

// ---------------------------------------------------------------------------
// Next / previous display (NextPrevDisplayCalculation, DisplayTransfer)

/// `NextPrevDisplayCalculation.calculate`.
fn next_prev_display(ctx: &Ctx<'_>) -> Option<CalcResult> {
    // Even with traverseSingleScreen on, one display has nowhere to send a window.
    if ctx.screens.len() < 2 {
        return None;
    }
    let (prev, next) = ctx.screens.adjacent?;
    let screen = if ctx.action == ActionId::NextDisplay {
        next
    } else {
        prev
    };
    let destination = ctx.visible_of(screen);
    let settings = ctx.settings;
    let on_destination = |rect: Frame, action: ActionId| CalcResult {
        rect,
        screen,
        resulting_action: action,
        sub: None,
        remember_pre_maximize: None,
        forget_last: false,
    };

    if settings.attempt_match_on_next_prev_display
        && let Some(last) = ctx.last_action().filter(|last| replayable(*last))
    {
        // Replays the last action on the new display, from scratch.
        let replay_screens = UsableScreens::single(ctx.screens.ordered[screen]);
        let replay = Ctx {
            action: last,
            window: ctx.window,
            window_pixels: ctx.window_pixels,
            screens: &replay_screens,
            last: None,
            settings,
            pre_maximize: None,
        };
        let result = calculate(&replay)?;
        return Some(CalcResult {
            forget_last: true,
            ..on_destination(result.rect, last)
        });
    }

    let source = ctx.visible();
    if !settings.center_across_displays {
        let tolerance = (4.0 + settings.gap_size) * ctx.scale();
        let (rect, against_all) = transferred_rect(ctx.window, source, destination, tolerance);
        if !against_all {
            return Some(on_destination(rect, ctx.action));
        }
        // The window is maximized: it stays maximized unless autoMaximize is off.
        if settings.auto_maximize {
            return Some(on_destination(destination, ActionId::Maximize));
        }
        let centered = center_rect(ctx.window, destination);
        return Some(on_destination(
            centered.rect,
            centered.resulting_action.unwrap_or(ctx.action),
        ));
    }
    if settings.attempt_match_on_next_prev_display {
        // Issue #1723: nothing to replay, so keep the window's relative spot.
        return Some(on_destination(
            relative_positioned_rect(ctx.window, source, destination),
            ctx.action,
        ));
    }
    if ctx.last_action() == Some(ActionId::Maximize) && settings.auto_maximize {
        return Some(on_destination(destination, ActionId::Maximize));
    }
    let centered = center_rect(ctx.window, destination);
    Some(on_destination(
        centered.rect,
        centered.resulting_action.unwrap_or(ctx.action),
    ))
}

/// `NextPrevDisplayCalculation.relativePositionedRect`.
fn relative_positioned_rect(window: Frame, source: Frame, destination: Frame) -> Frame {
    if source.w <= 0.0 || source.h <= 0.0 {
        return window;
    }
    let mut rect = Frame {
        x: destination.min_x() + (window.min_x() - source.min_x()) / source.w * destination.w,
        y: destination.min_y() + (window.min_y() - source.min_y()) / source.h * destination.h,
        w: window.w / source.w * destination.w,
        h: window.h / source.h * destination.h,
    };
    if rect.max_x() > destination.max_x() {
        rect.x = destination.max_x() - rect.w;
    }
    if rect.min_x() < destination.min_x() {
        rect.x = destination.min_x();
    }
    if rect.max_y() > destination.max_y() {
        rect.y = destination.max_y() - rect.h;
    }
    if rect.min_y() < destination.min_y() {
        rect.y = destination.min_y();
    }
    rect
}

/// Actions `attemptMatchOnNextPrevDisplay` may replay on the new display.
fn replayable(action: ActionId) -> bool {
    !matches!(
        action,
        ActionId::NextDisplay
            | ActionId::PreviousDisplay
            | ActionId::Restore
            | ActionId::Tile { .. }
    )
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Contact {
    Neither,
    Start,
    End,
    Both,
}

/// `DisplayTransfer.transferredRect`: each axis keeps the edge it was against.
/// Returns the rect and whether the window was against every edge (maximized).
/// `tolerance` is how close to an edge counts as against it: 4 points plus the
/// gap, in pixels.
fn transferred_rect(
    window: Frame,
    source: Frame,
    destination: Frame,
    tolerance: f64,
) -> (Frame, bool) {
    if source.w <= 0.0 || source.h <= 0.0 || destination.w <= 0.0 || destination.h <= 0.0 {
        return (window, false);
    }
    let (mut hx, mut hw, hc) = transfer_axis(
        (window.min_x(), window.w),
        (source.min_x(), source.w),
        (destination.min_x(), destination.w),
        tolerance,
    );
    let (mut vy, mut vh, vc) = transfer_axis(
        (window.min_y(), window.h),
        (source.min_y(), source.h),
        (destination.min_y(), destination.h),
        tolerance,
    );
    let changes_orientation = (source.w > source.h && destination.h > destination.w)
        || (source.h > source.w && destination.w > destination.h);
    let one_edge = |contact: Contact| matches!(contact, Contact::Start | Contact::End);
    if changes_orientation {
        if hc == Contact::Both && one_edge(vc) && window.w < hw {
            hx = destination.min_x() + (destination.w - window.w) / 2.0;
            hw = window.w;
        } else if vc == Contact::Both && one_edge(hc) && window.h < vh {
            vy = destination.min_y() + (destination.h - window.h) / 2.0;
            vh = window.h;
        }
    }
    (
        Frame {
            x: hx,
            y: vy,
            w: hw,
            h: vh,
        },
        hc == Contact::Both && vc == Contact::Both,
    )
}

fn transfer_axis(
    window: (f64, f64),
    source: (f64, f64),
    destination: (f64, f64),
    tolerance: f64,
) -> (f64, f64, Contact) {
    let (w_origin, w_length) = window;
    let (s_origin, s_length) = source;
    let (d_origin, d_length) = destination;
    let start_inset = w_origin - s_origin;
    let end_inset = (s_origin + s_length) - (w_origin + w_length);
    let against_start = start_inset <= tolerance;
    let against_end = end_inset <= tolerance;
    let clamp = |value: f64, low: f64, high: f64| value.max(low).min(high);
    if against_start && against_end {
        let start = clamp(start_inset, 0.0, d_length / 2.0);
        let end = clamp(end_inset, 0.0, d_length / 2.0);
        return (d_origin + start, d_length - start - end, Contact::Both);
    }
    let length = w_length.min(d_length);
    let slack = d_length - length;
    if against_start {
        return (
            d_origin + clamp(start_inset, 0.0, slack),
            length,
            Contact::Start,
        );
    }
    if against_end {
        return (
            d_origin + d_length - length - clamp(end_inset, 0.0, slack),
            length,
            Contact::End,
        );
    }
    let center_fraction = (w_origin + w_length / 2.0 - s_origin) / s_length;
    let origin = d_origin + center_fraction * d_length - length / 2.0;
    (
        clamp(origin, d_origin, d_origin + slack),
        length,
        Contact::Neither,
    )
}

// ---------------------------------------------------------------------------
// Dispatch

/// `WindowCalculationFactory.calculationsByAction[action].calculate(params)`
/// for the single-window actions. `None` is Rectangle's null rect: nothing to
/// do, so it beeps.
pub(crate) fn calculate(ctx: &Ctx<'_>) -> Option<CalcResult> {
    let visible = ctx.visible();
    let simple = |rect: RectResult| Some(ctx.result(rect));
    match ctx.action {
        ActionId::LeftHalf | ActionId::RightHalf => Some(left_right_half(ctx)),
        ActionId::TopHalf | ActionId::BottomHalf => simple(top_bottom_half(ctx)),
        ActionId::CenterHalf => simple(center_half(ctx)),
        ActionId::TopLeft | ActionId::TopRight | ActionId::BottomLeft | ActionId::BottomRight => {
            simple(corner(ctx))
        }
        ActionId::FirstThird => simple(first_third(ctx)),
        ActionId::CenterThird => simple(center_third_rect(visible)),
        ActionId::LastThird => simple(last_third(ctx)),
        ActionId::FirstTwoThirds => simple(first_two_thirds(ctx)),
        ActionId::CenterTwoThirds => simple(center_two_thirds(visible)),
        ActionId::LastTwoThirds => simple(last_two_thirds(ctx)),
        ActionId::FirstFourth => simple(first_fourth(ctx)),
        ActionId::SecondFourth => simple(second_fourth(ctx)),
        ActionId::ThirdFourth => simple(third_fourth(ctx)),
        ActionId::LastFourth => simple(last_fourth(ctx)),
        ActionId::FirstThreeFourths => simple(first_three_fourths(ctx)),
        ActionId::CenterThreeFourths => simple(three_fourths_rect(visible, 1)),
        ActionId::LastThreeFourths => simple(last_three_fourths(ctx)),
        ActionId::TopLeftSixth => simple(corner_sixth(ctx, Sixth::TopLeft)),
        ActionId::TopCenterSixth => simple(top_center_sixth(ctx)),
        ActionId::TopRightSixth => simple(corner_sixth(ctx, Sixth::TopRight)),
        ActionId::BottomLeftSixth => simple(corner_sixth(ctx, Sixth::BottomLeft)),
        ActionId::BottomCenterSixth => simple(bottom_center_sixth(ctx)),
        ActionId::BottomRightSixth => simple(corner_sixth(ctx, Sixth::BottomRight)),
        ActionId::TopLeftEighth => simple(eighth(ctx, 0)),
        ActionId::TopCenterLeftEighth => simple(eighth(ctx, 1)),
        ActionId::TopCenterRightEighth => simple(eighth(ctx, 2)),
        ActionId::TopRightEighth => simple(eighth(ctx, 3)),
        ActionId::BottomLeftEighth => simple(eighth(ctx, 4)),
        ActionId::BottomCenterLeftEighth => simple(eighth(ctx, 5)),
        ActionId::BottomCenterRightEighth => simple(eighth(ctx, 6)),
        ActionId::BottomRightEighth => simple(eighth(ctx, 7)),
        ActionId::Maximize => Some(maximize_like(ctx, visible)),
        ActionId::AlmostMaximize => Some(maximize_like(
            ctx,
            almost_maximize_rect(ctx.settings, visible),
        )),
        ActionId::MaximizeHeight => {
            let mut rect = ctx.window;
            rect.y = visible.min_y();
            rect.h = visible.h;
            simple(RectResult::plain(rect))
        }
        ActionId::Center => simple(center_rect(ctx.window, visible)),
        ActionId::Larger
        | ActionId::Smaller
        | ActionId::LargerWidth
        | ActionId::SmallerWidth
        | ActionId::LargerHeight
        | ActionId::SmallerHeight => simple(change_size(ctx)),
        ActionId::NextDisplay | ActionId::PreviousDisplay => next_prev_display(ctx),
        // Restore, Fill and Tile need history or other windows; the engine runs them.
        ActionId::Restore | ActionId::FillLeft | ActionId::FillRight | ActionId::Tile { .. } => {
            None
        }
    }
}
