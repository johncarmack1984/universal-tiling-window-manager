//! Rectangle Pro's Custom Size & Position entries (`manualSpecs`).
//!
//! Pro is closed source and documents these only as UI. What is known:
//! position is one of Center, Top, Bottom, Left, Right, Top Left, Top Right,
//! Bottom Left, Bottom Right, Custom Origin and Center Offset; a size field takes
//! "fractions, whole, or decimal", where ≤ 1 is a share of the screen and > 1 is
//! points, and a blank field keeps the window's current size; pressing "+" adds a
//! repetition, used on the next press of the same shortcut. The `loc` numbering
//! follows that documented order, which fits every published export.

use serde::Deserialize;

use crate::chord::MacChord;
use crate::export::{Setting, Shortcut};

#[derive(Clone, Debug, PartialEq)]
pub struct ManualSpec {
    pub id: i64,
    pub name: String,
    pub shortcut: Option<MacChord>,
    pub placement: Placement,
    /// Repetitions: the next presses of the same shortcut, in order.
    pub repetitions: Vec<Placement>,
}

/// Where and how big one press puts the window.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Placement {
    pub anchor: Anchor,
    pub x: Dimension,
    pub y: Dimension,
    pub width: Dimension,
    pub height: Dimension,
    /// Pro's display choice. Negative values are its relative choices
    /// (unchanged, next, previous); 0 and up pick a display by order.
    pub display: Option<i64>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Anchor {
    Center,
    Top,
    Bottom,
    Left,
    Right,
    TopLeft,
    TopRight,
    BottomLeft,
    BottomRight,
    CustomOrigin,
    CenterOffset,
}

impl Anchor {
    fn from_loc(loc: i64) -> Self {
        match loc {
            1 => Self::Top,
            2 => Self::Bottom,
            3 => Self::Left,
            4 => Self::Right,
            5 => Self::TopLeft,
            6 => Self::TopRight,
            7 => Self::BottomLeft,
            8 => Self::BottomRight,
            9 => Self::CustomOrigin,
            10 => Self::CenterOffset,
            _ => Self::Center,
        }
    }
}

/// One size or offset field.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Dimension {
    /// Blank: keep the window's current value.
    Keep,
    /// A share of the screen, 0–1.
    Fraction(f64),
    /// macOS points, which this app treats as device-independent pixels.
    Points(f64),
}

impl Dimension {
    fn parse(text: Option<&str>) -> Self {
        let Some(text) = text.map(str::trim).filter(|text| !text.is_empty()) else {
            return Self::Keep;
        };
        let value = match text.split_once('/') {
            Some((numerator, denominator)) => {
                match (
                    numerator.trim().parse::<f64>(),
                    denominator.trim().parse::<f64>(),
                ) {
                    (Ok(n), Ok(d)) if d != 0.0 => n / d,
                    _ => return Self::Keep,
                }
            }
            None => match text.parse::<f64>() {
                Ok(value) => value,
                Err(_) => return Self::Keep,
            },
        };
        if !value.is_finite() || value < 0.0 {
            Self::Keep
        } else if value <= 1.0 {
            Self::Fraction(value)
        } else {
            Self::Points(value)
        }
    }
}

#[derive(Deserialize)]
struct RawSpec {
    id: i64,
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    shortcut: Option<Shortcut>,
    #[serde(default)]
    loc: Option<i64>,
    #[serde(default, deserialize_with = "string_or_number")]
    x: Option<String>,
    #[serde(default, deserialize_with = "string_or_number")]
    y: Option<String>,
    #[serde(default, deserialize_with = "string_or_number")]
    w: Option<String>,
    #[serde(default, deserialize_with = "string_or_number")]
    h: Option<String>,
    #[serde(default)]
    display: Option<i64>,
    #[serde(default)]
    children: Vec<RawSpec>,
}

/// Published exports store sizes as strings; accept bare numbers too.
fn string_or_number<'de, D>(deserializer: D) -> Result<Option<String>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let value = Option::<serde_json::Value>::deserialize(deserializer)?;
    Ok(match value {
        Some(serde_json::Value::String(text)) => Some(text),
        Some(serde_json::Value::Number(number)) => Some(number.to_string()),
        _ => None,
    })
}

impl RawSpec {
    fn placement(&self) -> Placement {
        Placement {
            anchor: Anchor::from_loc(self.loc.unwrap_or(0)),
            x: Dimension::parse(self.x.as_deref()),
            y: Dimension::parse(self.y.as_deref()),
            width: Dimension::parse(self.w.as_deref()),
            height: Dimension::parse(self.h.as_deref()),
            display: self.display,
        }
    }
}

/// Parses the `manualSpecs` setting. A malformed blob yields an error rather
/// than silently dropping the user's positions.
pub fn parse_manual_specs(setting: Option<&Setting>) -> Result<Vec<ManualSpec>, serde_json::Error> {
    let Some(json) = setting.and_then(Setting::as_str) else {
        return Ok(Vec::new());
    };
    let raw: Vec<RawSpec> = serde_json::from_str(json)?;
    Ok(raw
        .into_iter()
        .map(|spec| ManualSpec {
            id: spec.id,
            name: spec
                .name
                .clone()
                .unwrap_or_else(|| format!("Custom position {}", spec.id)),
            shortcut: spec
                .shortcut
                .map(|shortcut| MacChord::new(shortcut.key_code, shortcut.modifier_flags)),
            placement: spec.placement(),
            repetitions: spec.children.iter().map(RawSpec::placement).collect(),
        })
        .collect())
}

/// Which custom specs Pro's menu shows (`menuCustomSpecVisibility`): an id maps
/// to `{"shown": false}` to hide it. Ids not listed are shown.
pub fn hidden_spec_ids(setting: Option<&Setting>) -> Vec<i64> {
    #[derive(Deserialize)]
    struct Visibility {
        #[serde(default = "shown_by_default")]
        shown: bool,
    }
    fn shown_by_default() -> bool {
        true
    }
    let Some(json) = setting.and_then(Setting::as_str) else {
        return Vec::new();
    };
    serde_json::from_str::<std::collections::BTreeMap<String, Visibility>>(json)
        .map(|map| {
            map.into_iter()
                .filter(|(_, visibility)| !visibility.shown)
                .filter_map(|(id, _)| id.parse().ok())
                .collect()
        })
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn setting(json: &str) -> Setting {
        Setting::String(json.to_owned())
    }

    #[test]
    fn johns_specs_parse() {
        let specs = parse_manual_specs(Some(&setting(
            r#"[{"title":"cmZI","displayFrame":[[0,0],[3840,2160]],"name":"Left 1\/3 Below Movie",
                 "shortcut":{"modifierFlags":786432,"keyCode":7},"w":".333","display":0,"loc":7,
                 "otherAppOperation":1,"h":".7","id":33},
                {"name":"App Store Screenshot (sm)","otherAppOperation":1,"w":"1168","h":"688","loc":0,"id":66}]"#,
        )))
        .expect("parses");
        assert_eq!(specs.len(), 2);
        let movie = &specs[0];
        assert_eq!(movie.name, "Left 1/3 Below Movie");
        assert_eq!(
            movie.shortcut.map(|chord| chord.to_string()),
            Some("⌃⌥X".to_owned())
        );
        assert_eq!(movie.placement.anchor, Anchor::BottomLeft);
        assert_eq!(movie.placement.width, Dimension::Fraction(0.333));
        assert_eq!(movie.placement.height, Dimension::Fraction(0.7));
        assert_eq!(movie.placement.x, Dimension::Keep);
        let screenshot = &specs[1];
        assert_eq!(screenshot.shortcut, None);
        assert_eq!(screenshot.placement.anchor, Anchor::Center);
        assert_eq!(screenshot.placement.width, Dimension::Points(1168.0));
    }

    #[test]
    fn repetitions_come_from_children() {
        let specs = parse_manual_specs(Some(&setting(
            r#"[{"id":100,"name":"Cycle thirds","shortcut":{"keyCode":37,"modifierFlags":1310720},
                 "loc":4,"w":"1/3","h":"1","children":[{"id":138,"loc":3,"w":"1/3","h":"1"},
                 {"id":113,"loc":0,"w":"1/3","h":"1"}]}]"#,
        )))
        .expect("parses");
        let spec = &specs[0];
        assert_eq!(spec.placement.anchor, Anchor::Right);
        assert_eq!(spec.repetitions.len(), 2);
        assert_eq!(spec.repetitions[0].anchor, Anchor::Left);
        match spec.placement.width {
            Dimension::Fraction(value) => assert!((value - 1.0 / 3.0).abs() < 1e-12),
            other => panic!("expected a fraction, got {other:?}"),
        }
        assert_eq!(spec.placement.height, Dimension::Fraction(1.0));
    }

    #[test]
    fn hidden_specs_are_read() {
        let hidden = hidden_spec_ids(Some(&setting(
            r#"{"33":{"shown":false},"66":{"shown":true}}"#,
        )));
        assert_eq!(hidden, [33]);
    }

    #[test]
    fn empty_or_unset_specs_are_fine() {
        assert!(parse_manual_specs(None).expect("unset").is_empty());
        assert!(
            parse_manual_specs(Some(&setting("[]")))
                .expect("empty")
                .is_empty()
        );
        assert!(parse_manual_specs(Some(&setting("not json"))).is_err());
    }
}
