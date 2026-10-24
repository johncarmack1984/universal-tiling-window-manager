//! Running an action on a window: Rectangle's `WindowManager.execute` and the
//! parts of `ShortcutManager` that decide which action a press means.
//!
//! The engine plans; the platform moves the window and reports back where it
//! actually landed, which is what the next press compares against. That is how
//! Rectangle tells a repeat (cycle to the next size) from a fresh press (start
//! over): the window must still sit exactly where the app left it.

use std::collections::HashMap;
use std::hash::Hash;

use utwm_config::settings::{RepeatMode, Settings};
use utwm_config::{ActionId, ManualSpec, Target};

use crate::calc::{self, Ctx};
use crate::fill::{FillSide, fill_area};
use crate::gaps::apply_gaps;
use crate::geometry::{Edges, Frame, PixelRect, px};
use crate::history::{ActionKey, LastAction, WindowRecord};
use crate::screens::{Screen, UsableScreens, containing};
use crate::sub_action::{GapDimensions, SubAction, action_gap_shared_edges, gaps_applicable};
use crate::tile::tile_cells;

/// One window action to plan.
pub struct Request<'a> {
    pub target: Target,
    /// The window's visible frame (without invisible resize borders).
    pub window: PixelRect,
    pub screens: &'a [Screen],
    pub settings: &'a Settings,
    pub specs: &'a [ManualSpec],
    /// Visible frames of the other windows that count for Fill.
    pub others: &'a [PixelRect],
}

/// What to do.
#[derive(Clone, Debug, PartialEq)]
pub enum Plan {
    /// Nothing to do; Rectangle beeps here.
    Nothing(&'static str),
    /// The window already sits where the action puts it. The press still counts
    /// toward the repeat cycle and is already recorded.
    AlreadyThere,
    Move(Move),
}

#[derive(Clone, Debug, PartialEq)]
pub struct Move {
    /// The visible frame to give the window.
    pub rect: PixelRect,
    /// The tile before gaps, for working out which screen edges it touches.
    pub initial: PixelRect,
    /// The destination screen's usable area.
    pub screen_visible: PixelRect,
    /// Gap in pixels on the destination screen, for keeping a window that
    /// refused its size on screen.
    pub gap: i32,
    pub across_displays: bool,
    /// Whether the action is meant to change the window's size.
    pub resizes: bool,
    /// What to record once the move lands; `None` for Restore, which clears the
    /// history instead.
    record: Option<(ActionKey, Option<SubAction>, bool)>,
}

/// Per-window state, keyed by whatever identifies a window on the platform.
#[derive(Debug)]
pub struct Engine<K> {
    records: HashMap<K, WindowRecord>,
}

impl<K> Default for Engine<K> {
    fn default() -> Self {
        Self {
            records: HashMap::new(),
        }
    }
}

impl<K: Eq + Hash + Clone> Engine<K> {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn record(&self, key: &K) -> Option<&WindowRecord> {
        self.records.get(key)
    }

    /// Forgets a window, e.g. once it is destroyed.
    pub fn forget(&mut self, key: &K) {
        self.records.remove(key);
    }

    /// Keeps only the windows `keep` accepts.
    pub fn retain(&mut self, mut keep: impl FnMut(&K) -> bool) {
        self.records.retain(|key, _| keep(key));
    }

    /// Which action a press of a chord bound to several actions means
    /// (`ShortcutCycle.action`): the one after the window's last action, or the
    /// first if the window moved since or the last action isn't in the group.
    pub fn select(&mut self, key: &K, group: &[Target], window: PixelRect) -> Option<Target> {
        let first = *group.first()?;
        let record = self.records.entry(key.clone()).or_default();
        let Some(last) = record.last else {
            return Some(first);
        };
        if last.rect != window {
            record.last = None;
            return Some(first);
        }
        let position = group.iter().position(|target| key_of(*target) == last.key);
        Some(position.map_or(first, |index| group[(index + 1) % group.len()]))
    }

    /// Plans an action on one window.
    pub fn plan(&mut self, key: &K, request: &Request<'_>) -> Plan {
        let settings = request.settings;
        let window = request.window;
        let action_key = key_of(request.target);
        let record = self.records.entry(key.clone()).or_default();

        if action_key == ActionKey::Action(ActionId::Restore) {
            record.last = None;
            let Some(restore) = record.restore else {
                return Plan::Nothing("no position to restore");
            };
            let screen = containing(request.screens, restore)
                .and_then(|index| request.screens.get(index))
                .copied();
            return Plan::Move(Move {
                rect: restore,
                initial: restore,
                screen_visible: screen.map_or(restore, |screen| screen.visible(settings)),
                gap: 0,
                across_displays: false,
                resizes: true,
                record: None,
            });
        }

        let Some(mut screens) = UsableScreens::detect(request.screens, window, settings) else {
            return Plan::Nothing("no displays");
        };
        let source_screen = screens.current_screen();

        // Taken before the staleness check below, as ShortcutManager does.
        let raw_last = record.last;
        let moved_externally = record.last.is_none_or(|last| last.rect != window);
        if moved_externally {
            record.last = None;
        }
        if record.restore.is_none() || moved_externally {
            record.restore = Some(window);
        }

        // "Cycle through displays": repeating an action runs it on the next display.
        if let ActionKey::Action(action) = action_key
            && settings.repeat_mode == RepeatMode::CycleMonitor
            && !is_size_or_display(action)
            && cycle_monitor_repeat(action, raw_last, window, &screens, settings, record)
            && let Some((_, next)) = screens.adjacent
        {
            screens = UsableScreens::single(screens.ordered[next]);
            record.last = None;
        }

        let last = record.last;
        let (result, sub, rect_frame, screen_index, forget_last, remember) = match action_key {
            ActionKey::Spec(id) => {
                let Some(spec) = request.specs.iter().find(|spec| spec.id == id) else {
                    return Plan::Nothing("custom position no longer exists");
                };
                let screen_index = match spec.placement.display {
                    Some(display) => usize::try_from(display)
                        .ok()
                        .filter(|index| *index < screens.len())
                        .unwrap_or(screens.current),
                    None => screens.current,
                };
                let screen = screens.ordered[screen_index];
                let repetition = match last {
                    Some(last) if last.key == action_key && !spec.repetitions.is_empty() => {
                        usize::try_from(last.count).unwrap_or(0) % (spec.repetitions.len() + 1)
                    }
                    _ => 0,
                };
                let placement = if repetition == 0 {
                    &spec.placement
                } else {
                    &spec.repetitions[repetition - 1]
                };
                let rect =
                    crate::spec::place(placement, window, screen.visible(settings), screen.scale);
                (
                    action_key,
                    None,
                    Frame::from_pixels(rect),
                    screen_index,
                    false,
                    None,
                )
            }
            ActionKey::Action(action @ (ActionId::FillLeft | ActionId::FillRight)) => {
                let screen = screens.current_screen();
                let visible = screen.visible(settings);
                let side = if action == ActionId::FillLeft {
                    FillSide::Left
                } else {
                    FillSide::Right
                };
                let others: Vec<PixelRect> = request
                    .others
                    .iter()
                    .copied()
                    .filter(|other| *other != window)
                    .collect();
                let rect = fill_area(
                    side,
                    visible,
                    &others,
                    settings.minimum_window_width,
                    settings.minimum_window_height,
                )
                .unwrap_or_else(|| half_of(visible, side));
                (
                    action_key,
                    None,
                    Frame::from_pixels(rect),
                    screens.current,
                    false,
                    None,
                )
            }
            ActionKey::Action(ActionId::Tile { .. }) => {
                return Plan::Nothing("tiling arranges several windows; use plan_tile");
            }
            ActionKey::Action(action) => {
                let ctx = Ctx {
                    action,
                    window: Frame::from_pixels(window),
                    window_pixels: window,
                    screens: &screens,
                    last: last.as_ref(),
                    settings,
                    pre_maximize: record.pre_maximize,
                };
                let Some(result) = calc::calculate(&ctx) else {
                    return Plan::Nothing("the action doesn't apply here");
                };
                (
                    ActionKey::Action(result.resulting_action),
                    result.sub,
                    result.rect,
                    result.screen,
                    result.forget_last,
                    result.remember_pre_maximize,
                )
            }
        };

        let destination = screens.ordered[screen_index];
        let initial = rect_frame.to_pixels();
        let mut target = rect_frame;
        if let ActionKey::Action(resulting) = result {
            let dimensions = gaps_applicable(
                resulting,
                settings.gaps_on_maximize,
                settings.gaps_on_maximize_height,
            );
            if settings.gap_size > 0.0 && dimensions.any() {
                let shared = sub.map_or_else(
                    || action_gap_shared_edges(resulting),
                    SubAction::gap_shared_edges,
                );
                target = apply_gaps(
                    target,
                    dimensions,
                    shared,
                    settings.gap_size * destination.scale,
                    settings.skip_gap_top_edge,
                );
            }
        }
        let target = target.to_pixels();
        if target.is_empty() {
            return Plan::Nothing("the action leaves no room");
        }
        if let Some(previous) = remember {
            record.pre_maximize = Some(previous);
        }

        if target == window {
            record_into(record, result, sub, window, forget_last);
            return Plan::AlreadyThere;
        }

        Plan::Move(Move {
            rect: target,
            initial,
            screen_visible: destination.visible(settings),
            gap: px(settings.gap_size * destination.scale),
            across_displays: destination.frame != source_screen.frame,
            resizes: resizes(result),
            record: Some((result, sub, forget_last)),
        })
    }

    /// Records where a planned move actually put the window.
    pub fn landed(&mut self, key: &K, planned: &Move, achieved: PixelRect) {
        let record = self.records.entry(key.clone()).or_default();
        match planned.record {
            Some((result, sub, forget_last)) => {
                record_into(record, result, sub, achieved, forget_last)
            }
            None => record.last = None,
        }
    }

    /// Plans Rectangle Pro's `tile{rows}x{cols}`. `windows` are the tileable
    /// windows with their visible frames, the focused one first and the rest
    /// front to back; only those on the focused window's display are tiled.
    pub fn plan_tile(
        &mut self,
        rows: u8,
        cols: u8,
        windows: &[(K, PixelRect)],
        screens: &[Screen],
        settings: &Settings,
    ) -> Vec<(K, PixelRect)> {
        let Some((_, focused)) = windows.first() else {
            return Vec::new();
        };
        let Some(screen_index) = containing(screens, *focused) else {
            return Vec::new();
        };
        let screen = screens[screen_index];
        let on_screen: Vec<&(K, PixelRect)> = windows
            .iter()
            .filter(|(_, frame)| containing(screens, *frame) == Some(screen_index))
            .collect();
        let visible = screen.visible(settings);
        let cells = tile_cells(rows, cols, visible, on_screen.len());
        let gap = settings.gap_size * screen.scale;
        on_screen
            .into_iter()
            .zip(cells)
            .map(|((key, frame), cell)| {
                let record = self.records.entry(key.clone()).or_default();
                if record.restore.is_none() || record.last.is_none_or(|last| last.rect != *frame) {
                    record.restore = Some(*frame);
                }
                record.last = None;
                let rect = if gap > 0.0 {
                    // Edges inside the grid face a neighbour and get half a gap back.
                    let shared = Edges::new(
                        cell.left != visible.left,
                        cell.right != visible.right,
                        cell.top != visible.top,
                        cell.bottom != visible.bottom,
                    );
                    apply_gaps(
                        Frame::from_pixels(cell),
                        GapDimensions::BOTH,
                        shared,
                        gap,
                        false,
                    )
                    .to_pixels()
                } else {
                    cell
                };
                (key.clone(), rect)
            })
            .collect()
    }
}

fn record_into(
    record: &mut WindowRecord,
    result: ActionKey,
    sub: Option<SubAction>,
    achieved: PixelRect,
    forget_last: bool,
) {
    if forget_last {
        record.last = None;
    }
    let count = match record.last {
        Some(last) if last.key == result => last.count.saturating_add(1),
        _ => 1,
    };
    record.last = Some(LastAction {
        key: result,
        sub,
        rect: achieved,
        count,
    });
}

fn key_of(target: Target) -> ActionKey {
    match target {
        Target::Action(action) => ActionKey::Action(action),
        Target::Spec(id) => ActionKey::Spec(id),
    }
}

/// `WindowAction.resizes`.
fn resizes(key: ActionKey) -> bool {
    !matches!(
        key,
        ActionKey::Action(ActionId::Center | ActionId::NextDisplay | ActionId::PreviousDisplay)
    )
}

/// `classification` is `.size` or `.display`: excluded from display cycling.
fn is_size_or_display(action: ActionId) -> bool {
    matches!(
        action,
        ActionId::Smaller
            | ActionId::Larger
            | ActionId::SmallerWidth
            | ActionId::LargerWidth
            | ActionId::SmallerHeight
            | ActionId::LargerHeight
            | ActionId::PreviousDisplay
            | ActionId::NextDisplay
    )
}

/// `ShortcutManager.isRepeatAction`, plus its check that a repeated Maximize
/// isn't about to restore instead.
fn cycle_monitor_repeat(
    action: ActionId,
    raw_last: Option<LastAction>,
    window: PixelRect,
    screens: &UsableScreens,
    settings: &Settings,
    record: &WindowRecord,
) -> bool {
    let maximize_restores = settings.repeated_maximize_restores_previous
        && matches!(action, ActionId::Maximize | ActionId::AlmostMaximize)
        && record.pre_maximize.is_some()
        && raw_last
            .is_some_and(|last| last.key == ActionKey::Action(action) && last.rect == window);
    if maximize_restores {
        return false;
    }
    if action == ActionId::Maximize {
        let visible = screens.current_screen().work;
        if visible.width() == window.width() && visible.height() == window.height() {
            return true;
        }
    }
    raw_last.is_some_and(|last| last.key == ActionKey::Action(action))
}

/// Pro's Fill fallback: the half on that side.
fn half_of(visible: PixelRect, side: FillSide) -> PixelRect {
    let half = (f64::from(visible.width()) * 0.5 + 0.0001).floor();
    let width = px(half);
    match side {
        FillSide::Left => PixelRect::from_xywh(visible.left, visible.top, width, visible.height()),
        FillSide::Right => PixelRect::new(
            visible.right - width,
            visible.top,
            visible.right,
            visible.bottom,
        ),
    }
}
