//! The displays, in points from the top left of the main display.
//!
//! AppKit measures screens from the bottom left of the main display with y
//! growing up; the Accessibility API, and so the layout engine, measures from
//! its top left with y growing down. Each frame is flipped once, here.

use objc2_app_kit::NSScreen;
use objc2_foundation::NSRect;
use utwm_layout::{PixelRect, Rect, Screen};

/// Every display with its usable area (`visibleFrame`: without the menu bar
/// and the Dock), the main display first. NSScreen belongs to the main thread,
/// so this asks it there.
pub fn screens() -> Vec<Screen> {
    dispatch2::run_on_main(|mtm| {
        let screens = NSScreen::screens(mtm);
        let Some(main_height) = screens.firstObject().map(|main| main.frame().size.height) else {
            return Vec::new();
        };
        screens
            .iter()
            .enumerate()
            .map(|(index, screen)| Screen {
                frame: flip(screen.frame(), main_height),
                work: flip(screen.visibleFrame(), main_height),
                // The engine's units are already points.
                scale: 1.0,
                // AppKit lists the display with the menu bar first; Rectangle's
                // "main screen only" settings mean that one.
                primary: index == 0,
            })
            .collect()
    })
}

/// An AppKit frame in screen coordinates: origin at the top left of the main
/// display, y down.
fn flip(rect: NSRect, main_height: f64) -> PixelRect {
    Rect {
        x: rect.origin.x,
        y: main_height - (rect.origin.y + rect.size.height),
        w: rect.size.width,
        h: rect.size.height,
    }
    .to_pixels()
}

#[cfg(test)]
mod tests {
    use objc2_foundation::{NSPoint, NSSize};

    use super::*;

    fn rect(x: f64, y: f64, w: f64, h: f64) -> NSRect {
        NSRect::new(NSPoint::new(x, y), NSSize::new(w, h))
    }

    #[test]
    fn the_main_display_starts_at_the_origin() {
        assert_eq!(
            flip(rect(0.0, 0.0, 1512.0, 982.0), 982.0),
            PixelRect::new(0, 0, 1512, 982)
        );
        // Its visible frame loses the 37 pt menu bar at the top and a 70 pt
        // Dock at the bottom.
        assert_eq!(
            flip(rect(0.0, 70.0, 1512.0, 875.0), 982.0),
            PixelRect::new(0, 37, 1512, 912)
        );
    }

    #[test]
    fn a_display_above_the_main_one_has_negative_y() {
        assert_eq!(
            flip(rect(-200.0, 982.0, 2560.0, 1440.0), 982.0),
            PixelRect::new(-200, -1440, 2360, 0)
        );
    }
}
