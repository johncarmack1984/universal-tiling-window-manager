//! Window actions by the names Rectangle and Rectangle Pro store them under.

use std::fmt;

/// An action this app knows how to run on Windows.
///
/// The export names many more; anything not listed here resolves to
/// [`ActionName::Unsupported`] and shows up in `check` instead of binding.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum ActionId {
    LeftHalf,
    RightHalf,
    CenterHalf,
    TopHalf,
    BottomHalf,
    TopLeft,
    TopRight,
    BottomLeft,
    BottomRight,
    FirstThird,
    CenterThird,
    LastThird,
    FirstTwoThirds,
    CenterTwoThirds,
    LastTwoThirds,
    Maximize,
    AlmostMaximize,
    MaximizeHeight,
    Larger,
    Smaller,
    LargerWidth,
    SmallerWidth,
    LargerHeight,
    SmallerHeight,
    Center,
    Restore,
    NextDisplay,
    PreviousDisplay,
    FirstFourth,
    SecondFourth,
    ThirdFourth,
    LastFourth,
    FirstThreeFourths,
    CenterThreeFourths,
    LastThreeFourths,
    TopLeftSixth,
    TopCenterSixth,
    TopRightSixth,
    BottomLeftSixth,
    BottomCenterSixth,
    BottomRightSixth,
    TopLeftEighth,
    TopCenterLeftEighth,
    TopCenterRightEighth,
    TopRightEighth,
    BottomLeftEighth,
    BottomCenterLeftEighth,
    BottomCenterRightEighth,
    BottomRightEighth,
    /// Rectangle Pro: the largest empty area against the left screen edge.
    FillLeft,
    /// Rectangle Pro: the largest empty area against the right screen edge.
    FillRight,
    /// Rectangle Pro's `tile{rows}x{cols}`: every window on the display in a grid.
    Tile {
        rows: u8,
        cols: u8,
    },
}

/// What an action name in an export resolves to.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ActionName {
    Supported(ActionId),
    /// A real Rectangle or Rectangle Pro action this app doesn't run yet.
    Unsupported,
    /// A name neither app is known to use.
    Unknown,
}

const IMPLEMENTED: &[(&str, ActionId)] = &[
    ("leftHalf", ActionId::LeftHalf),
    ("rightHalf", ActionId::RightHalf),
    ("centerHalf", ActionId::CenterHalf),
    ("topHalf", ActionId::TopHalf),
    ("bottomHalf", ActionId::BottomHalf),
    ("topLeft", ActionId::TopLeft),
    ("topRight", ActionId::TopRight),
    ("bottomLeft", ActionId::BottomLeft),
    ("bottomRight", ActionId::BottomRight),
    ("firstThird", ActionId::FirstThird),
    ("centerThird", ActionId::CenterThird),
    ("lastThird", ActionId::LastThird),
    ("firstTwoThirds", ActionId::FirstTwoThirds),
    ("centerTwoThirds", ActionId::CenterTwoThirds),
    ("lastTwoThirds", ActionId::LastTwoThirds),
    ("maximize", ActionId::Maximize),
    ("almostMaximize", ActionId::AlmostMaximize),
    ("maximizeHeight", ActionId::MaximizeHeight),
    ("larger", ActionId::Larger),
    ("smaller", ActionId::Smaller),
    ("largerWidth", ActionId::LargerWidth),
    ("smallerWidth", ActionId::SmallerWidth),
    ("largerHeight", ActionId::LargerHeight),
    ("smallerHeight", ActionId::SmallerHeight),
    ("center", ActionId::Center),
    ("restore", ActionId::Restore),
    ("nextDisplay", ActionId::NextDisplay),
    ("previousDisplay", ActionId::PreviousDisplay),
    ("firstFourth", ActionId::FirstFourth),
    ("secondFourth", ActionId::SecondFourth),
    ("thirdFourth", ActionId::ThirdFourth),
    ("lastFourth", ActionId::LastFourth),
    ("firstThreeFourths", ActionId::FirstThreeFourths),
    ("centerThreeFourths", ActionId::CenterThreeFourths),
    ("lastThreeFourths", ActionId::LastThreeFourths),
    ("topLeftSixth", ActionId::TopLeftSixth),
    ("topCenterSixth", ActionId::TopCenterSixth),
    ("topRightSixth", ActionId::TopRightSixth),
    ("bottomLeftSixth", ActionId::BottomLeftSixth),
    ("bottomCenterSixth", ActionId::BottomCenterSixth),
    ("bottomRightSixth", ActionId::BottomRightSixth),
    ("topLeftEighth", ActionId::TopLeftEighth),
    ("topCenterLeftEighth", ActionId::TopCenterLeftEighth),
    ("topCenterRightEighth", ActionId::TopCenterRightEighth),
    ("topRightEighth", ActionId::TopRightEighth),
    ("bottomLeftEighth", ActionId::BottomLeftEighth),
    ("bottomCenterLeftEighth", ActionId::BottomCenterLeftEighth),
    ("bottomCenterRightEighth", ActionId::BottomCenterRightEighth),
    ("bottomRightEighth", ActionId::BottomRightEighth),
    ("fillLeft", ActionId::FillLeft),
    ("fillRight", ActionId::FillRight),
    ("tile1x2", ActionId::Tile { rows: 1, cols: 2 }),
    ("tile1x3", ActionId::Tile { rows: 1, cols: 3 }),
    ("tile1x4", ActionId::Tile { rows: 1, cols: 4 }),
    ("tile2x1", ActionId::Tile { rows: 2, cols: 1 }),
    ("tile2x2", ActionId::Tile { rows: 2, cols: 2 }),
    ("tile2x3", ActionId::Tile { rows: 2, cols: 3 }),
];

/// Rectangle's `WindowAction.active`, in order. Several actions on one chord
/// cycle in this order (Rectangle 2.0's shortcut cycles), which is why it is kept
/// verbatim rather than sorted. Followed by the three shortcuts Rectangle stores
/// next to the window actions.
const RECTANGLE_ORDER: &[&str] = &[
    "leftHalf",
    "rightHalf",
    "centerHalf",
    "topHalf",
    "bottomHalf",
    "topLeft",
    "topRight",
    "bottomLeft",
    "bottomRight",
    "firstThird",
    "centerThird",
    "lastThird",
    "firstTwoThirds",
    "centerTwoThirds",
    "lastTwoThirds",
    "topVerticalThird",
    "middleVerticalThird",
    "bottomVerticalThird",
    "topVerticalTwoThirds",
    "bottomVerticalTwoThirds",
    "maximize",
    "almostMaximize",
    "maximizeHeight",
    "larger",
    "smaller",
    "largerWidth",
    "smallerWidth",
    "largerHeight",
    "smallerHeight",
    "center",
    "centerProminently",
    "restore",
    "nextDisplay",
    "previousDisplay",
    "moveLeft",
    "moveRight",
    "moveUp",
    "moveDown",
    "firstFourth",
    "secondFourth",
    "thirdFourth",
    "lastFourth",
    "firstThreeFourths",
    "centerThreeFourths",
    "lastThreeFourths",
    "topLeftSixth",
    "topCenterSixth",
    "topRightSixth",
    "bottomLeftSixth",
    "bottomCenterSixth",
    "bottomRightSixth",
    "specified",
    "reverseAll",
    "topLeftThird",
    "topRightThird",
    "bottomLeftThird",
    "bottomRightThird",
    "topLeftEighth",
    "topCenterLeftEighth",
    "topCenterRightEighth",
    "topRightEighth",
    "bottomLeftEighth",
    "bottomCenterLeftEighth",
    "bottomCenterRightEighth",
    "bottomRightEighth",
    "firstVerticalEighth",
    "secondVerticalEighth",
    "thirdVerticalEighth",
    "fourthVerticalEighth",
    "fifthVerticalEighth",
    "sixthVerticalEighth",
    "seventhVerticalEighth",
    "lastVerticalEighth",
    "firstVerticalSixth",
    "secondVerticalSixth",
    "thirdVerticalSixth",
    "fourthVerticalSixth",
    "fifthVerticalSixth",
    "lastVerticalSixth",
    "topLeftNinth",
    "topCenterNinth",
    "topRightNinth",
    "middleLeftNinth",
    "middleCenterNinth",
    "middleRightNinth",
    "bottomLeftNinth",
    "bottomCenterNinth",
    "bottomRightNinth",
    "topLeftTwelfth",
    "topCenterLeftTwelfth",
    "topCenterRightTwelfth",
    "topRightTwelfth",
    "middleLeftTwelfth",
    "middleCenterLeftTwelfth",
    "middleCenterRightTwelfth",
    "middleRightTwelfth",
    "bottomLeftTwelfth",
    "bottomCenterLeftTwelfth",
    "bottomCenterRightTwelfth",
    "bottomRightTwelfth",
    "topLeftSixteenth",
    "topCenterLeftSixteenth",
    "topCenterRightSixteenth",
    "topRightSixteenth",
    "upperMiddleLeftSixteenth",
    "upperMiddleCenterLeftSixteenth",
    "upperMiddleCenterRightSixteenth",
    "upperMiddleRightSixteenth",
    "lowerMiddleLeftSixteenth",
    "lowerMiddleCenterLeftSixteenth",
    "lowerMiddleCenterRightSixteenth",
    "lowerMiddleRightSixteenth",
    "bottomLeftSixteenth",
    "bottomCenterLeftSixteenth",
    "bottomCenterRightSixteenth",
    "bottomRightSixteenth",
    "doubleHeightUp",
    "doubleHeightDown",
    "doubleWidthLeft",
    "doubleWidthRight",
    "halveHeightUp",
    "halveHeightDown",
    "halveWidthLeft",
    "halveWidthRight",
    "tileAll",
    "tileRows",
    "tileColumns",
    "cascadeAll",
    "leftTodo",
    "rightTodo",
    "cascadeActiveApp",
    "tileActiveApp",
    "displayOne",
    "displayTwo",
    "displayThree",
    "displayFour",
    "displayFive",
    "displaySix",
    "displaySeven",
    "displayEight",
    "displayNine",
    "cycleStackedWindows",
    "cycleStackedWindowsBackward",
    "toggleTodo",
    "reflowTodo",
    "toggleStackBadge",
];

/// Rectangle Pro's own action names, as seen in its exports. Pro is closed
/// source, so this list comes from published exports rather than code; a Pro
/// action sharing a chord with a Rectangle action cycles after it, which is the
/// order Rectangle Pro was observed to use for ⌃⌥E (Left Two Thirds, then Fill
/// Left).
const PRO_ORDER: &[&str] = &[
    "fillLeft",
    "fillRight",
    "fillTopLeft",
    "fillTopRight",
    "fillBottomLeft",
    "fillBottomRight",
    "firstFifth",
    "secondFifth",
    "thirdFifth",
    "fourthFifth",
    "lastFifth",
    "firstSixth",
    "lastSixth",
    "leftAndQuarters",
    "rightAndQuarters",
    "maximizeWidth",
    "reasonableSize",
    "upperCenter",
    "last",
    "nextDisplayRatio",
    "prevDisplayRatio",
    "nextSpace",
    "prevSpace",
    "nudgeLeft",
    "nudgeRight",
    "nudgeUp",
    "nudgeDown",
    "snapTopLeft",
    "snapTopRight",
    "snapBottomLeft",
    "snapBottomRight",
    "appLeftHalf",
    "appRightHalf",
    "appNextDisplay",
    "appPrevDisplay",
    "cascadeApp",
    "revealDesktopEdge",
    "tidy",
    "tile1x2",
    "tile1x3",
    "tile1x4",
    "tile2x1",
    "tile2x2",
    "tile2x3",
    "pin",
    "reflowPin",
    "stashLeft",
    "stashRight",
    "stashUp",
    "stashDown",
    "stashAll",
    "stashAllButFront",
    "unstash",
    "unstashAll",
    "unstashLeft",
    "unstashRight",
    "unstashUp",
    "unstashDown",
    "cycleStashed",
    "cycleStashLeft",
    "cycleStashRight",
    "cycleStashUp",
    "cycleStashDown",
    "toggleStashed",
    "toggleStashLeft",
    "toggleStashRight",
    "toggleStashUp",
    "toggleStashDown",
];

/// Renamed actions Rectangle still accepts on import (`WindowAction.swift`).
const ALIASES: &[(&str, &str)] = &[
    ("leftSide", "leftHalf"),
    ("rightSide", "rightHalf"),
    ("topSide", "topHalf"),
    ("bottomSide", "bottomHalf"),
    ("centerSection", "centerHalf"),
];

/// The current name for an action, following Rectangle's import aliases.
pub fn canonical_name(name: &str) -> &str {
    ALIASES
        .iter()
        .find(|(old, _)| *old == name)
        .map_or(name, |(_, new)| new)
}

/// Resolves an export's action name.
pub fn resolve(name: &str) -> ActionName {
    let name = canonical_name(name);
    if let Some((_, id)) = IMPLEMENTED.iter().find(|(known, _)| *known == name) {
        return ActionName::Supported(*id);
    }
    if RECTANGLE_ORDER.contains(&name) || PRO_ORDER.contains(&name) {
        ActionName::Unsupported
    } else {
        ActionName::Unknown
    }
}

/// Where an action sits in the cycle order of a shared chord: Rectangle's order
/// first, then Rectangle Pro's, then anything unknown.
pub fn cycle_order(name: &str) -> usize {
    let name = canonical_name(name);
    RECTANGLE_ORDER
        .iter()
        .position(|known| *known == name)
        .or_else(|| {
            PRO_ORDER
                .iter()
                .position(|known| *known == name)
                .map(|index| RECTANGLE_ORDER.len() + index)
        })
        .unwrap_or(usize::MAX)
}

impl ActionId {
    /// The name Rectangle stores this action under.
    pub fn name(self) -> &'static str {
        IMPLEMENTED
            .iter()
            .find(|(_, id)| *id == self)
            .map_or("?", |(name, _)| name)
    }

    /// The label Rectangle's menu shows.
    pub fn label(self) -> String {
        match self {
            Self::Tile { rows, cols } => format!("{rows}×{cols} Tiles"),
            Self::Restore => "Restore".to_owned(),
            other => title_case(other.name()),
        }
    }

    /// The tray menu group, mirroring the separators in Rectangle's menu.
    pub fn group(self) -> MenuGroup {
        use ActionId::*;
        match self {
            LeftHalf | RightHalf | CenterHalf | TopHalf | BottomHalf => MenuGroup::Halves,
            TopLeft | TopRight | BottomLeft | BottomRight => MenuGroup::Corners,
            FirstThird | CenterThird | LastThird | FirstTwoThirds | CenterTwoThirds
            | LastTwoThirds => MenuGroup::Thirds,
            Maximize | AlmostMaximize | MaximizeHeight | Larger | Smaller | LargerWidth
            | SmallerWidth | LargerHeight | SmallerHeight | Center | Restore => MenuGroup::Size,
            NextDisplay | PreviousDisplay => MenuGroup::Displays,
            FirstFourth | SecondFourth | ThirdFourth | LastFourth | FirstThreeFourths
            | CenterThreeFourths | LastThreeFourths => MenuGroup::Fourths,
            TopLeftSixth | TopCenterSixth | TopRightSixth | BottomLeftSixth | BottomCenterSixth
            | BottomRightSixth => MenuGroup::Sixths,
            TopLeftEighth
            | TopCenterLeftEighth
            | TopCenterRightEighth
            | TopRightEighth
            | BottomLeftEighth
            | BottomCenterLeftEighth
            | BottomCenterRightEighth
            | BottomRightEighth => MenuGroup::Eighths,
            FillLeft | FillRight | Tile { .. } => MenuGroup::Pro,
        }
    }

    /// Every action this app runs, in Rectangle's order.
    pub fn all() -> Vec<Self> {
        let mut all: Vec<Self> = IMPLEMENTED.iter().map(|(_, id)| *id).collect();
        all.sort_by_key(|id| cycle_order(id.name()));
        all
    }
}

impl fmt::Display for ActionId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.name())
    }
}

/// Sections of the tray menu.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum MenuGroup {
    Halves,
    Corners,
    Thirds,
    Size,
    Displays,
    Fourths,
    Sixths,
    Eighths,
    Pro,
}

impl MenuGroup {
    pub fn label(self) -> &'static str {
        match self {
            Self::Halves => "Halves",
            Self::Corners => "Corners",
            Self::Thirds => "Thirds",
            Self::Size => "Size",
            Self::Displays => "Displays",
            Self::Fourths => "Fourths",
            Self::Sixths => "Sixths",
            Self::Eighths => "Eighths",
            Self::Pro => "Fill & Tile",
        }
    }
}

/// `topCenterLeftEighth` → `Top Center Left Eighth`.
fn title_case(name: &str) -> String {
    let mut out = String::with_capacity(name.len() + 4);
    for (index, ch) in name.chars().enumerate() {
        if index == 0 {
            out.extend(ch.to_uppercase());
        } else if ch.is_ascii_uppercase() {
            out.push(' ');
            out.push(ch);
        } else {
            out.push(ch);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_implemented_action_is_a_known_name() {
        for (name, _) in IMPLEMENTED {
            assert_ne!(cycle_order(name), usize::MAX, "{name} is in no order list");
        }
    }

    #[test]
    fn rectangle_actions_cycle_before_pro_ones() {
        assert!(cycle_order("firstTwoThirds") < cycle_order("fillLeft"));
        assert!(cycle_order("lastTwoThirds") < cycle_order("fillRight"));
    }

    #[test]
    fn aliases_resolve() {
        assert_eq!(
            resolve("leftSide"),
            ActionName::Supported(ActionId::LeftHalf)
        );
        assert_eq!(resolve("tileAll"), ActionName::Unsupported);
        assert_eq!(resolve("stashLeft"), ActionName::Unsupported);
        assert_eq!(resolve("definitelyNotAnAction"), ActionName::Unknown);
    }

    #[test]
    fn labels_read_like_rectangle_menu_items() {
        assert_eq!(
            ActionId::TopCenterLeftEighth.label(),
            "Top Center Left Eighth"
        );
        assert_eq!(ActionId::Tile { rows: 2, cols: 3 }.label(), "2×3 Tiles");
        assert_eq!(ActionId::FillLeft.label(), "Fill Left");
    }

    #[test]
    fn rectangle_order_has_every_window_action_once() {
        let mut names = RECTANGLE_ORDER.to_vec();
        names.sort_unstable();
        names.dedup();
        assert_eq!(names.len(), RECTANGLE_ORDER.len());
        // 143 window actions plus the three non-window shortcuts.
        assert_eq!(RECTANGLE_ORDER.len(), 146);
    }
}
