//! The displays, in the coordinates GNOME places windows in.
//!
//! GNOME lays displays out in one of two ways. In its logical layout, which
//! fractional scaling turns on, coordinates are logical pixels, so a 2× display
//! of 2880 × 1800 pixels is 1440 × 900, the way the Mac counts points. In its
//! physical layout they are device pixels, as on Windows. `Screen::scale` says
//! which, since Rectangle's sizes are points.

use std::collections::HashMap;

use utwm_layout::{PixelRect, Screen};
use zbus::zvariant::OwnedValue;

use crate::shell;

const DISPLAY_CONFIG: &str = "org.gnome.Mutter.DisplayConfig";
const DISPLAY_CONFIG_PATH: &str = "/org/gnome/Mutter/DisplayConfig";

/// `layout-mode` in the display configuration's properties.
const LOGICAL: u32 = 1;

type Properties = HashMap<String, OwnedValue>;
type Mode = (String, i32, i32, f64, f64, Vec<f64>, Properties);
type Monitor = ((String, String, String, String), Vec<Mode>, Properties);
type LogicalMonitor = (
    i32,
    i32,
    f64,
    u32,
    bool,
    Vec<(String, String, String, String)>,
    Properties,
);
/// `org.gnome.Mutter.DisplayConfig.GetCurrentState`'s reply.
type DisplayState = (u32, Vec<Monitor>, Vec<LogicalMonitor>, Properties);

/// Every display with its work area (without the top bar and a docked dock),
/// as GNOME lists them.
pub fn screens() -> Vec<Screen> {
    let screens = match shell::screens() {
        Ok(screens) => screens,
        Err(error) => {
            tracing::debug!(?error, "no displays from GNOME Shell");
            return Vec::new();
        }
    };
    let logical = layout_is_logical();
    screens
        .into_iter()
        .map(
            |(x, y, width, height, work_x, work_y, work_width, work_height, scale, primary)| {
                Screen {
                    frame: PixelRect::from_xywh(x, y, width, height),
                    work: PixelRect::from_xywh(work_x, work_y, work_width, work_height),
                    scale: units_per_point(logical, scale),
                    primary,
                }
            },
        )
        .collect()
}

/// How many of GNOME's units one point is: one in the logical layout, the
/// display's scale in the physical one.
fn units_per_point(logical: bool, display_scale: f64) -> f64 {
    if logical || display_scale <= 0.0 {
        1.0
    } else {
        display_scale
    }
}

/// Whether GNOME lays the displays out in logical pixels. When it can't say,
/// logical, which is what GNOME on Wayland uses unless told otherwise.
fn layout_is_logical() -> bool {
    let Some(connection) = shell::connection() else {
        return true;
    };
    let state: zbus::Result<DisplayState> = shell::proxy(
        connection,
        DISPLAY_CONFIG,
        DISPLAY_CONFIG_PATH,
        DISPLAY_CONFIG,
    )
    .and_then(|proxy| proxy.call("GetCurrentState", &()));
    match state {
        Ok((_, _, _, properties)) => properties
            .get("layout-mode")
            .and_then(|mode| mode.downcast_ref::<u32>().ok())
            .is_none_or(|mode| mode == LOGICAL),
        Err(error) => {
            tracing::debug!(%error, "no display configuration; assuming the logical layout");
            true
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn points_are_units_in_the_logical_layout_and_scaled_in_the_physical_one() {
        assert!((units_per_point(true, 2.0) - 1.0).abs() < f64::EPSILON);
        assert!((units_per_point(false, 2.0) - 2.0).abs() < f64::EPSILON);
        assert!((units_per_point(false, 0.0) - 1.0).abs() < f64::EPSILON);
    }
}
