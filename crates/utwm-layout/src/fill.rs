//! Rectangle Pro's Fill Left and Fill Right.
//!
//! Pro documents these only as cycling "through some of the available fillable
//! screen areas", falling back to "half width when there are not suitable fill
//! options". Here a fill area is the largest rectangle against that side of the
//! screen that no other window overlaps, no smaller than the minimum window
//! size; with none, the window takes the half.

use crate::geometry::PixelRect;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FillSide {
    Left,
    Right,
}

/// The fill area for `side` of `visible`, given the frames of the other
/// windows. `min_width`/`min_height` are fractions of the screen.
pub fn fill_area(
    side: FillSide,
    visible: PixelRect,
    others: &[PixelRect],
    min_width: f64,
    min_height: f64,
) -> Option<PixelRect> {
    let obstacles: Vec<PixelRect> = others
        .iter()
        .filter_map(|other| other.intersection(visible))
        .collect();

    let mut xs: Vec<i32> = vec![visible.left, visible.right];
    let mut ys: Vec<i32> = vec![visible.top, visible.bottom];
    for obstacle in &obstacles {
        xs.extend([obstacle.left, obstacle.right]);
        ys.extend([obstacle.top, obstacle.bottom]);
    }
    xs.sort_unstable();
    xs.dedup();
    ys.sort_unstable();
    ys.dedup();

    let need_width = (f64::from(visible.width()) * min_width).ceil();
    let need_height = (f64::from(visible.height()) * min_height).ceil();
    let big_enough = |rect: PixelRect| {
        f64::from(rect.width()) >= need_width && f64::from(rect.height()) >= need_height
    };
    let clear = |rect: PixelRect| {
        obstacles
            .iter()
            .all(|obstacle| rect.intersection(*obstacle).is_none())
    };

    let mut best: Option<PixelRect> = None;
    for &edge in &xs {
        let (left, right) = match side {
            FillSide::Left => (visible.left, edge),
            FillSide::Right => (edge, visible.right),
        };
        if right <= left {
            continue;
        }
        for (i, &top) in ys.iter().enumerate() {
            for &bottom in &ys[i + 1..] {
                let candidate = PixelRect::new(left, top, right, bottom);
                if !big_enough(candidate) || !clear(candidate) {
                    continue;
                }
                let better = match best {
                    None => true,
                    Some(current) => {
                        (candidate.area(), candidate.height(), candidate.width())
                            > (current.area(), current.height(), current.width())
                    }
                };
                if better {
                    best = Some(candidate);
                }
            }
        }
    }
    best
}

#[cfg(test)]
mod tests {
    use super::*;

    const SCREEN: PixelRect = PixelRect::new(0, 0, 1920, 1032);

    #[test]
    fn fills_the_space_left_of_a_right_hand_window() {
        let right_two_thirds = PixelRect::new(640, 0, 1920, 1032);
        assert_eq!(
            fill_area(FillSide::Left, SCREEN, &[right_two_thirds], 0.1, 0.1),
            Some(PixelRect::new(0, 0, 640, 1032))
        );
    }

    #[test]
    fn prefers_the_biggest_clear_area() {
        // A window in the top-left quarter leaves everything below it and the
        // right half; against the left edge, the whole bottom half is biggest.
        let top_left = PixelRect::new(0, 0, 960, 516);
        assert_eq!(
            fill_area(FillSide::Left, SCREEN, &[top_left], 0.1, 0.1),
            Some(PixelRect::new(0, 516, 1920, 1032))
        );
    }

    #[test]
    fn nothing_fits_when_the_side_is_covered() {
        let covering = PixelRect::new(0, 0, 1900, 1032);
        assert_eq!(
            fill_area(FillSide::Left, SCREEN, &[covering], 0.1, 0.1),
            None
        );
    }

    #[test]
    fn an_empty_screen_fills_entirely() {
        assert_eq!(
            fill_area(FillSide::Right, SCREEN, &[], 0.1, 0.1),
            Some(SCREEN)
        );
    }
}
