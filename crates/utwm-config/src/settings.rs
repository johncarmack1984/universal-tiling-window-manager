//! The behaviour settings in an export's `defaults`, typed and filled in with
//! Rectangle's own defaults wherever the export leaves them out.
//!
//! Defaults and the quirks of each setting come from Rectangle's `Defaults.swift`
//! at v2.0.4. A setting Rectangle Pro dropped from its export (Pro 236 omits
//! `applyGapsToMaximize`, `widthStepSize` and others) falls back the same way.

use crate::export::{Export, Setting};

/// What repeating an action does (`subsequentExecutionMode`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RepeatMode {
    /// Cycle sizes on side actions, as Spectacle did.
    Resize,
    /// Move to the adjacent display on left/right halves.
    AcrossMonitor,
    None,
    /// Adjacent display on left/right, cycle sizes otherwise.
    AcrossAndResize,
    /// Run the same action on the next display.
    CycleMonitor,
    /// Cycle positions on quadrants, sizes on sides.
    ResizeAndCycleQuadrants,
    /// A repeated half jumps to the opposite half.
    SwitchSides,
}

impl RepeatMode {
    fn from_int(value: i64) -> Self {
        match value {
            1 => Self::AcrossMonitor,
            2 => Self::None,
            3 => Self::AcrossAndResize,
            4 => Self::CycleMonitor,
            5 => Self::ResizeAndCycleQuadrants,
            6 => Self::SwitchSides,
            _ => Self::Resize,
        }
    }

    pub fn resizes(self) -> bool {
        matches!(
            self,
            Self::Resize | Self::AcrossAndResize | Self::ResizeAndCycleQuadrants
        )
    }

    pub fn cycles_quadrant_positions(self) -> bool {
        self == Self::ResizeAndCycleQuadrants
    }

    pub fn describe(self) -> &'static str {
        match self {
            Self::Resize => "cycle sizes on side actions",
            Self::AcrossMonitor => "move to adjacent display on left or right",
            Self::None => "do nothing",
            Self::AcrossAndResize => "move to adjacent on left/right, or cycle size on side",
            Self::CycleMonitor => "cycle through displays",
            Self::ResizeAndCycleQuadrants => "cycle positions on quadrants, cycle sizes on side",
            Self::SwitchSides => "switch to the opposite side on half actions",
        }
    }
}

/// One size in the repeat cycle (`CycleSize`), with Rectangle's bit positions.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CycleSize {
    TwoThirds = 0,
    OneHalf = 1,
    OneThird = 2,
    OneQuarter = 3,
    ThreeQuarters = 4,
}

impl CycleSize {
    pub fn fraction(self) -> f64 {
        match self {
            Self::TwoThirds => 2.0 / 3.0,
            Self::OneHalf => 0.5,
            Self::OneThird => 1.0 / 3.0,
            Self::OneQuarter => 0.25,
            Self::ThreeQuarters => 0.75,
        }
    }

    pub fn glyph(self) -> &'static str {
        match self {
            Self::TwoThirds => "⅔",
            Self::OneHalf => "½",
            Self::OneThird => "⅓",
            Self::OneQuarter => "¼",
            Self::ThreeQuarters => "¾",
        }
    }

    /// Rectangle's cycling order: ½ first, then larger sizes going up, then the
    /// smaller ones. "The selectable sizes are in an unusual order."
    const SORTED: [Self; 5] = [
        Self::OneHalf,
        Self::TwoThirds,
        Self::ThreeQuarters,
        Self::OneQuarter,
        Self::OneThird,
    ];

    fn bit(self) -> i64 {
        1 << (self as i64)
    }
}

/// The sizes a repeated side action cycles through, in cycling order.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CycleSizes(Vec<CycleSize>);

impl CycleSizes {
    /// Rectangle's default selection when the user never changed it.
    pub fn default_selection() -> Self {
        Self::from_bits(
            CycleSize::OneHalf.bit() | CycleSize::TwoThirds.bit() | CycleSize::OneThird.bit(),
        )
    }

    pub fn from_bits(bits: i64) -> Self {
        Self(
            CycleSize::SORTED
                .into_iter()
                .filter(|size| bits & size.bit() != 0)
                .collect(),
        )
    }

    pub fn sizes(&self) -> &[CycleSize] {
        &self.0
    }

    /// Which size the `count`-th execution lands on (`cycleIndex` in
    /// `RepeatedExecutionsCalculation.swift`). `count` is how many times the
    /// action already ran on this window in a row.
    pub fn index_for(&self, count: u32) -> usize {
        let len = self.0.len().max(1);
        let count = usize::try_from(count).unwrap_or(usize::MAX);
        if self.0.contains(&CycleSize::OneHalf) {
            count % len
        } else {
            count.saturating_sub(1) % len
        }
    }

    pub fn describe(&self) -> String {
        self.0
            .iter()
            .map(|size| size.glyph())
            .collect::<Vec<_>>()
            .join(" → ")
    }
}

/// Display ordering for next/previous display (`screensOrderedByX`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ScreenOrdering {
    /// By horizontal center.
    MidX,
    /// By left edge.
    MinX,
    /// Row by row from the top, then left to right.
    YThenMinX,
}

/// Where a window that refused its size is placed (`moveFixedSizeToEdge`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EdgeAlignment {
    EdgesAndCorners,
    Corners,
    Centered,
    LeadingCorner,
}

/// Which axis repeated corner actions grow along (`cornerCycleExpansionAxis`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CornerAxis {
    Horizontal,
    Vertical,
}

/// Rectangle's three-state booleans (`OptionalBoolDefault`): 0 is unset, 1 on,
/// 2 off.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TriState {
    Unset,
    On,
    Off,
}

impl TriState {
    fn from_setting(setting: Option<&Setting>) -> Self {
        match setting.and_then(Setting::as_i64) {
            Some(1) => Self::On,
            Some(2) => Self::Off,
            _ => Self::Unset,
        }
    }
}

/// Reserved space at the screen edges, in points.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct EdgeGaps {
    pub top: f64,
    pub bottom: f64,
    pub left: f64,
    pub right: f64,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Settings {
    pub repeat_mode: RepeatMode,
    pub cycle_sizes: CycleSizes,
    pub corner_axis: CornerAxis,
    /// Points between snapped windows.
    pub gap_size: f64,
    pub skip_gap_top_edge: bool,
    pub gaps_on_maximize: bool,
    pub gaps_on_maximize_height: bool,
    pub screen_edge_gaps: EdgeGaps,
    pub screen_edge_gaps_on_main_only: bool,
    pub almost_maximize_width: f64,
    pub almost_maximize_height: f64,
    /// Fractions, 0–1, of the first press of a half or corner.
    pub horizontal_split: f64,
    pub vertical_split: f64,
    /// Points.
    pub size_offset: f64,
    pub width_step: f64,
    pub minimum_window_width: f64,
    pub minimum_window_height: f64,
    pub curtain_change_size: bool,
    pub smaller_shrinks_maximized_height: bool,
    pub screen_ordering: ScreenOrdering,
    pub traverse_single_screen: bool,
    pub auto_maximize: bool,
    pub center_across_displays: bool,
    pub attempt_match_on_next_prev_display: bool,
    pub repeated_maximize_restores_previous: bool,
    pub center_half_cycles: bool,
    pub move_fixed_size_to_edge: EdgeAlignment,
    /// macOS bundle ids whose windows Rectangle leaves alone. The Mac honours
    /// them; Windows apps have no bundle ids.
    pub disabled_apps: Vec<String>,
    /// Settings this app reads but cannot honour, with why.
    pub not_applied: Vec<(String, String)>,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            repeat_mode: RepeatMode::Resize,
            cycle_sizes: CycleSizes::default_selection(),
            corner_axis: CornerAxis::Horizontal,
            gap_size: 0.0,
            skip_gap_top_edge: false,
            gaps_on_maximize: true,
            gaps_on_maximize_height: true,
            screen_edge_gaps: EdgeGaps::default(),
            screen_edge_gaps_on_main_only: false,
            almost_maximize_width: 0.9,
            almost_maximize_height: 0.9,
            horizontal_split: 0.5,
            vertical_split: 0.5,
            size_offset: 30.0,
            width_step: 30.0,
            minimum_window_width: 0.25,
            minimum_window_height: 0.25,
            curtain_change_size: true,
            smaller_shrinks_maximized_height: false,
            screen_ordering: ScreenOrdering::YThenMinX,
            traverse_single_screen: false,
            auto_maximize: true,
            center_across_displays: false,
            attempt_match_on_next_prev_display: false,
            repeated_maximize_restores_previous: false,
            center_half_cycles: false,
            move_fixed_size_to_edge: EdgeAlignment::EdgesAndCorners,
            disabled_apps: Vec::new(),
            not_applied: Vec::new(),
        }
    }
}

impl Settings {
    pub fn from_export(export: &Export) -> Self {
        let get = |key: &str| export.setting(key);
        let float = |key: &str| get(key).and_then(Setting::as_f64);
        let int = |key: &str| get(key).and_then(Setting::as_i64);
        let flag = |key: &str| get(key).and_then(Setting::as_bool).unwrap_or(false);
        let tri = |key: &str| TriState::from_setting(get(key));

        let mut settings = Self::default();

        if let Some(mode) = int("subsequentExecutionMode") {
            settings.repeat_mode = RepeatMode::from_int(mode);
        }
        if flag("cycleSizesIsChanged") {
            settings.cycle_sizes = CycleSizes::from_bits(int("selectedCycleSizes").unwrap_or(0));
        }
        if int("cornerCycleExpansionAxis") == Some(1) {
            settings.corner_axis = CornerAxis::Vertical;
        }

        settings.gap_size = float("gapSize").unwrap_or(0.0).max(0.0);
        settings.skip_gap_top_edge = flag("skipGapTopEdge");
        settings.gaps_on_maximize = tri("applyGapsToMaximize") != TriState::Off;
        settings.gaps_on_maximize_height = tri("applyGapsToMaximizeHeight") != TriState::Off;
        settings.screen_edge_gaps = EdgeGaps {
            top: float("screenEdgeGapTop").unwrap_or(0.0).max(0.0),
            bottom: float("screenEdgeGapBottom").unwrap_or(0.0).max(0.0),
            left: float("screenEdgeGapLeft").unwrap_or(0.0).max(0.0),
            right: float("screenEdgeGapRight").unwrap_or(0.0).max(0.0),
        };
        settings.screen_edge_gaps_on_main_only = flag("screenEdgeGapsOnMainScreenOnly");

        // `FloatDefault` treats a stored 0 as "use the built-in default", and
        // Almost Maximize also rejects anything outside (0, 1].
        let fraction_or = |value: Option<f64>, fallback: f64| match value {
            Some(v) if v > 0.0 && v <= 1.0 => v,
            _ => fallback,
        };
        settings.almost_maximize_width = fraction_or(float("almostMaximizeWidth"), 0.9);
        settings.almost_maximize_height = fraction_or(float("almostMaximizeHeight"), 0.9);

        let percent_or_half = |value: Option<f64>| match value {
            Some(v) if v > 0.0 => (v / 100.0).clamp(0.0, 1.0),
            _ => 0.5,
        };
        settings.horizontal_split = percent_or_half(float("horizontalSplitRatio"));
        settings.vertical_split = percent_or_half(float("verticalSplitRatio"));

        let positive_or = |value: Option<f64>, fallback: f64| match value {
            Some(v) if v > 0.0 => v,
            _ => fallback,
        };
        settings.size_offset = positive_or(float("sizeOffset"), 30.0);
        settings.width_step = positive_or(float("widthStepSize"), 30.0);

        // `minimumWindowFraction`: anything non-finite or outside [0, 1] is 0.25.
        let minimum = |value: Option<f64>| match value {
            Some(v) if v.is_finite() && (0.0..=1.0).contains(&v) => v,
            _ => 0.25,
        };
        settings.minimum_window_width = minimum(float("minimumWindowWidth"));
        settings.minimum_window_height = minimum(float("minimumWindowHeight"));

        settings.curtain_change_size = tri("curtainChangeSize") != TriState::Off;
        settings.smaller_shrinks_maximized_height = flag("smallerShrinksMaximizedHeight");

        settings.screen_ordering = match int("screensOrderedByX") {
            Some(1) => ScreenOrdering::MidX,
            Some(2) => ScreenOrdering::MinX,
            _ => ScreenOrdering::YThenMinX,
        };
        settings.traverse_single_screen = tri("traverseSingleScreen") == TriState::On;
        settings.auto_maximize = tri("autoMaximize") != TriState::Off;
        settings.center_across_displays = tri("centerAcrossDisplays") == TriState::On;
        settings.attempt_match_on_next_prev_display =
            tri("attemptMatchOnNextPrevDisplay") == TriState::On;
        settings.repeated_maximize_restores_previous = flag("repeatedMaximizeRestoresPrevious");
        settings.center_half_cycles = tri("centerHalfCycles") == TriState::On;
        settings.move_fixed_size_to_edge = match int("moveFixedSizeToEdge") {
            Some(2) => EdgeAlignment::Corners,
            Some(3) => EdgeAlignment::Centered,
            Some(4) => EdgeAlignment::LeadingCorner,
            _ => EdgeAlignment::EdgesAndCorners,
        };

        settings.disabled_apps = get("disabledApps")
            .and_then(Setting::as_str)
            .and_then(|json| serde_json::from_str::<Vec<String>>(json).ok())
            .unwrap_or_default();

        let mut not_applied = Vec::new();
        if flag("halvesPreserveOtherAxisSize") {
            not_applied.push((
                "halvesPreserveOtherAxisSize".to_owned(),
                "not implemented; halves use the full other axis".to_owned(),
            ));
        }
        if flag("cooperativeCornerResize") {
            not_applied.push((
                "cooperativeCornerResize".to_owned(),
                "not implemented".to_owned(),
            ));
        }
        if let Some(step) = float("smallerLargerOffsetV2") {
            not_applied.push((
                "smallerLargerOffsetV2".to_owned(),
                format!(
                    "Rectangle Pro step of {step}; Make Larger/Smaller use sizeOffset ({}) until Pro's choice between them is confirmed",
                    settings.size_offset
                ),
            ));
        }
        if let Some(mode) = int("repeatedMaximizeMode").filter(|mode| *mode != 0) {
            not_applied.push((
                "repeatedMaximizeMode".to_owned(),
                format!("Rectangle Pro mode {mode} is undocumented; repeated Maximize does nothing extra"),
            ));
        }
        settings.not_applied = not_applied;
        settings
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn export(defaults: &str) -> Export {
        Export::parse(&format!(
            r#"{{ "bundleId": "com.knollsoft.Hookshot", "version": "235", "shortcuts": {{}}, "defaults": {{ {defaults} }} }}"#
        ))
        .expect("test export parses")
    }

    #[test]
    fn rectangle_cycle_order_starts_at_one_half() {
        let all = CycleSizes::from_bits(0b11111);
        assert_eq!(all.describe(), "½ → ⅔ → ¾ → ¼ → ⅓");
        assert_eq!(CycleSizes::default_selection().describe(), "½ → ⅔ → ⅓");
    }

    #[test]
    fn cycle_index_follows_rectangle() {
        let sizes = CycleSizes::default_selection();
        // count is how many presses already happened on this window.
        assert_eq!(sizes.index_for(1), 1);
        assert_eq!(sizes.index_for(2), 2);
        assert_eq!(sizes.index_for(3), 0);
        // Without ½ the first repeat starts at the first selected size.
        let no_half = CycleSizes::from_bits(CycleSize::TwoThirds.bit() | CycleSize::OneThird.bit());
        assert_eq!(no_half.index_for(1), 0);
        assert_eq!(no_half.index_for(2), 1);
    }

    #[test]
    fn johns_settings_read_back() {
        let settings = Settings::from_export(&export(
            r#""subsequentExecutionMode": { "int": 0 }, "cycleSizesIsChanged": { "bool": true },
               "selectedCycleSizes": { "int": 7 }, "gapSize": { "double": 0 },
               "almostMaximizeHeight": { "float": 0 }, "minimumWindowWidth": { "double": 0.1 },
               "sizeOffset": { "float": 30 }, "smallerLargerOffsetV2": { "float": 20 },
               "screensOrderedByX": { "int": 3 }, "moveFixedSizeToEdge": { "int": 1 },
               "repeatedMaximizeMode": { "int": 4 }, "disabledApps": {}"#,
        ));
        assert_eq!(settings.repeat_mode, RepeatMode::Resize);
        assert_eq!(settings.cycle_sizes.describe(), "½ → ⅔ → ⅓");
        assert!((settings.almost_maximize_height - 0.9).abs() < 1e-9);
        assert!((settings.minimum_window_width - 0.1).abs() < 1e-9);
        assert!((settings.minimum_window_height - 0.25).abs() < 1e-9);
        assert_eq!(settings.screen_ordering, ScreenOrdering::YThenMinX);
        assert!(settings.disabled_apps.is_empty());
        let unapplied: Vec<&str> = settings
            .not_applied
            .iter()
            .map(|(key, _)| key.as_str())
            .collect();
        assert_eq!(unapplied, ["smallerLargerOffsetV2", "repeatedMaximizeMode"]);
    }

    #[test]
    fn tri_states_follow_optional_bool_default() {
        let settings = Settings::from_export(&export(
            r#""autoMaximize": { "int": 2 }, "traverseSingleScreen": { "int": 1 }, "curtainChangeSize": { "int": 0 }"#,
        ));
        assert!(!settings.auto_maximize);
        assert!(settings.traverse_single_screen);
        assert!(settings.curtain_change_size);
    }
}
