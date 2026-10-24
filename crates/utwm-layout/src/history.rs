//! What the engine remembers per window (`WindowHistory.swift`).

use utwm_config::ActionId;

use crate::geometry::PixelRect;
use crate::sub_action::SubAction;

/// An action as history records it: a window action, or a Rectangle Pro
/// custom position by id.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ActionKey {
    Action(ActionId),
    Spec(i64),
}

/// `RectangleAction`: the last thing this app did to a window.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LastAction {
    pub key: ActionKey,
    pub sub: Option<SubAction>,
    /// The frame the window actually ended up with.
    pub rect: PixelRect,
    /// How many times in a row `key` ran on this window.
    pub count: u32,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct WindowRecord {
    /// Where Restore puts the window: its frame before this app first moved it,
    /// or since the user last moved it by hand.
    pub restore: Option<PixelRect>,
    pub last: Option<LastAction>,
    /// The frame before the last Maximize, for `repeatedMaximizeRestoresPrevious`.
    pub pre_maximize: Option<PixelRect>,
}
