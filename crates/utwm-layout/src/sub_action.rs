//! Rectangle's `SubWindowAction`: which piece of a cycling action a window
//! landed on, so the next press knows where to go and gaps know which edges
//! face a neighbour.

use utwm_config::ActionId;

use crate::geometry::Edges;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum SubAction {
    LeftThird,
    CenterVerticalThird,
    RightThird,
    LeftTwoThirds,
    RightTwoThirds,
    TopThird,
    CenterHorizontalThird,
    BottomThird,
    TopTwoThirds,
    BottomTwoThirds,
    LeftFourth,
    CenterLeftFourth,
    CenterRightFourth,
    RightFourth,
    TopFourth,
    CenterTopFourth,
    CenterBottomFourth,
    BottomFourth,
    RightThreeFourths,
    BottomThreeFourths,
    LeftThreeFourths,
    TopThreeFourths,
    CenterVerticalThreeFourths,
    CenterHorizontalThreeFourths,
    CenterVerticalHalf,
    CenterHorizontalHalf,
    TopLeftSixthLandscape,
    TopCenterSixthLandscape,
    TopRightSixthLandscape,
    BottomLeftSixthLandscape,
    BottomCenterSixthLandscape,
    BottomRightSixthLandscape,
    TopLeftSixthPortrait,
    TopRightSixthPortrait,
    LeftCenterSixthPortrait,
    RightCenterSixthPortrait,
    BottomLeftSixthPortrait,
    BottomRightSixthPortrait,
    TopLeftTwoSixthsLandscape,
    TopLeftTwoSixthsPortrait,
    TopRightTwoSixthsLandscape,
    TopRightTwoSixthsPortrait,
    BottomLeftTwoSixthsLandscape,
    BottomLeftTwoSixthsPortrait,
    BottomRightTwoSixthsLandscape,
    BottomRightTwoSixthsPortrait,
    TopLeftQuarter,
    TopRightQuarter,
    BottomLeftQuarter,
    BottomRightQuarter,
    TopLeftEighth,
    TopCenterLeftEighth,
    TopCenterRightEighth,
    TopRightEighth,
    BottomLeftEighth,
    BottomCenterLeftEighth,
    BottomCenterRightEighth,
    BottomRightEighth,
}

const L: bool = true;
const R: bool = true;
const T: bool = true;
const B: bool = true;
const N: bool = false;

impl SubAction {
    /// `SubWindowAction.gapSharedEdge`: the edges that face another tile and
    /// get half a gap back.
    pub fn gap_shared_edges(self) -> Edges {
        use SubAction::*;
        let (left, right, top, bottom) = match self {
            LeftThird | LeftTwoThirds | LeftFourth | LeftThreeFourths => (N, R, N, N),
            CenterVerticalThird
            | CenterLeftFourth
            | CenterRightFourth
            | CenterVerticalThreeFourths
            | CenterVerticalHalf => (L, R, N, N),
            RightThird | RightTwoThirds | RightFourth | RightThreeFourths => (L, N, N, N),
            TopThird | TopTwoThirds | TopFourth | TopThreeFourths => (N, N, N, B),
            CenterHorizontalThird
            | CenterTopFourth
            | CenterBottomFourth
            | CenterHorizontalThreeFourths
            | CenterHorizontalHalf => (N, N, T, B),
            BottomThird | BottomTwoThirds | BottomFourth | BottomThreeFourths => (N, N, T, N),
            TopLeftSixthLandscape
            | TopLeftSixthPortrait
            | TopLeftTwoSixthsLandscape
            | TopLeftTwoSixthsPortrait
            | TopLeftQuarter
            | TopLeftEighth => (N, R, N, B),
            TopCenterSixthLandscape | TopCenterLeftEighth | TopCenterRightEighth => (L, R, N, B),
            TopRightSixthLandscape
            | TopRightSixthPortrait
            | TopRightTwoSixthsLandscape
            | TopRightTwoSixthsPortrait
            | TopRightQuarter
            | TopRightEighth => (L, N, N, B),
            BottomLeftSixthLandscape
            | BottomLeftSixthPortrait
            | BottomLeftTwoSixthsLandscape
            | BottomLeftTwoSixthsPortrait
            | BottomLeftQuarter
            | BottomLeftEighth => (N, R, T, N),
            BottomCenterSixthLandscape | BottomCenterLeftEighth | BottomCenterRightEighth => {
                (L, R, T, N)
            }
            BottomRightSixthLandscape
            | BottomRightSixthPortrait
            | BottomRightTwoSixthsLandscape
            | BottomRightTwoSixthsPortrait
            | BottomRightQuarter
            | BottomRightEighth => (L, N, T, N),
            LeftCenterSixthPortrait => (N, R, T, B),
            RightCenterSixthPortrait => (L, N, T, B),
        };
        Edges::new(left, right, top, bottom)
    }
}

/// `WindowAction.gapSharedEdge`, for results that carry no sub-action.
pub fn action_gap_shared_edges(action: ActionId) -> Edges {
    use ActionId::*;
    let (left, right, top, bottom) = match action {
        LeftHalf => (N, R, N, N),
        RightHalf => (L, N, N, N),
        BottomHalf => (N, N, T, N),
        TopHalf => (N, N, N, B),
        BottomLeft => (N, R, T, N),
        BottomRight => (L, N, T, N),
        TopLeft => (N, R, N, B),
        TopRight => (L, N, N, B),
        _ => (N, N, N, N),
    };
    Edges::new(left, right, top, bottom)
}

/// Which axes get gaps (`WindowAction.gapsApplicable`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct GapDimensions {
    pub horizontal: bool,
    pub vertical: bool,
}

impl GapDimensions {
    pub const NONE: Self = Self {
        horizontal: false,
        vertical: false,
    };
    pub const BOTH: Self = Self {
        horizontal: true,
        vertical: true,
    };
    pub const VERTICAL: Self = Self {
        horizontal: false,
        vertical: true,
    };

    pub fn any(self) -> bool {
        self.horizontal || self.vertical
    }
}

pub fn gaps_applicable(
    action: ActionId,
    gaps_on_maximize: bool,
    gaps_on_maximize_height: bool,
) -> GapDimensions {
    use ActionId::*;
    match action {
        Maximize if gaps_on_maximize => GapDimensions::BOTH,
        MaximizeHeight if gaps_on_maximize_height => GapDimensions::VERTICAL,
        Maximize | MaximizeHeight | AlmostMaximize | PreviousDisplay | NextDisplay | Larger
        | Smaller | LargerWidth | SmallerWidth | LargerHeight | SmallerHeight | Center
        | Restore => GapDimensions::NONE,
        // Fill and Tile are Rectangle Pro actions; they lay windows edge to edge
        // like the grid actions do, so they take gaps the same way.
        _ => GapDimensions::BOTH,
    }
}
