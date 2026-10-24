//! Rectangle Pro's `tile{rows}x{cols}`: the windows on a display in a grid.
//!
//! As John's Mac does it for Tile 2×3: every visible window on the display, three
//! across and two down, filled row by row. The focused window takes the first
//! cell and the rest follow front to back; windows beyond the grid stay put.

use crate::geometry::{PixelRect, Rect};

/// The grid cells for `count` windows, row-major, edges rounded so neighbours
/// share them exactly.
pub fn tile_cells(rows: u8, cols: u8, visible: PixelRect, count: usize) -> Vec<PixelRect> {
    let rows = u32::from(rows.max(1));
    let cols = u32::from(cols.max(1));
    let area = visible.to_rect();
    let edge = |start: f64, length: f64, index: u32, parts: u32| {
        start + length * f64::from(index) / f64::from(parts)
    };
    (0..rows * cols)
        .take(count)
        .map(|index| {
            let (row, col) = (index / cols, index % cols);
            let left = edge(area.x, area.w, col, cols);
            let right = edge(area.x, area.w, col + 1, cols);
            let top = edge(area.y, area.h, row, rows);
            let bottom = edge(area.y, area.h, row + 1, rows);
            Rect {
                x: left,
                y: top,
                w: right - left,
                h: bottom - top,
            }
            .to_pixels()
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn two_by_three_on_this_box() {
        let cells = tile_cells(2, 3, PixelRect::new(0, 0, 1920, 1032), 6);
        assert_eq!(
            cells,
            [
                PixelRect::new(0, 0, 640, 516),
                PixelRect::new(640, 0, 1280, 516),
                PixelRect::new(1280, 0, 1920, 516),
                PixelRect::new(0, 516, 640, 1032),
                PixelRect::new(640, 516, 1280, 1032),
                PixelRect::new(1280, 516, 1920, 1032),
            ]
        );
    }

    #[test]
    fn fewer_windows_fill_the_first_cells() {
        assert_eq!(
            tile_cells(2, 3, PixelRect::new(0, 0, 1920, 1032), 2).len(),
            2
        );
    }

    #[test]
    fn uneven_widths_still_meet() {
        let cells = tile_cells(1, 3, PixelRect::new(0, 0, 1366, 700), 3);
        assert_eq!(cells[0].right, cells[1].left);
        assert_eq!(cells[1].right, cells[2].left);
        assert_eq!(cells[2].right, 1366);
    }
}
