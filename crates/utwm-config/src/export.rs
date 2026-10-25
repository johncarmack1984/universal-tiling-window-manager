//! The JSON file Rectangle and Rectangle Pro write from Settings → Export.
//!
//! ```json
//! { "bundleId": "com.knollsoft.Hookshot", "version": "235", "timestamp": 813673606.7,
//!   "shortcuts": { "leftHalf": { "keyCode": 123, "modifierFlags": 786432 } },
//!   "defaults": { "gapSize": { "double": 0 }, "disabledApps": {} } }
//! ```
//!
//! Every setting is an object with exactly one key naming its type, or `{}` when
//! unset. Rectangle Pro changes the type of a setting between builds (`gapSize`
//! went from `float` to `double` at build 188), so readers ask for a number, not
//! for a particular encoding.

use std::collections::BTreeMap;

use serde::Deserialize;

/// Rectangle Pro's bundle identifier; "Hookshot" was the app's original name.
pub const PRO_BUNDLE_ID: &str = "com.knollsoft.Hookshot";
pub const RECTANGLE_BUNDLE_ID: &str = "com.knollsoft.Rectangle";

/// Rectangle refuses exports over 1 MiB; so does this.
const MAX_EXPORT_BYTES: usize = 1 << 20;

#[derive(Clone, Debug, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Export {
    pub bundle_id: String,
    pub version: String,
    /// Seconds since 2001-01-01 UTC. Rectangle Pro only.
    #[serde(default)]
    pub timestamp: Option<f64>,
    pub shortcuts: BTreeMap<String, Shortcut>,
    pub defaults: BTreeMap<String, Setting>,
}

/// A stored shortcut. Rectangle Pro's own plist adds a `type` field, which
/// exports leave out; unknown fields are ignored either way.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Shortcut {
    pub key_code: i64,
    pub modifier_flags: u64,
}

/// One entry of `defaults`.
#[derive(Clone, Debug, PartialEq, Deserialize)]
#[serde(from = "RawSetting")]
pub enum Setting {
    Bool(bool),
    Int(i64),
    Float(f64),
    Double(f64),
    String(String),
    /// `{}`: the user never set it.
    Unset,
}

#[derive(Deserialize)]
struct RawSetting {
    #[serde(default)]
    bool: Option<bool>,
    #[serde(default)]
    int: Option<i64>,
    #[serde(default)]
    float: Option<f64>,
    #[serde(default)]
    double: Option<f64>,
    #[serde(default)]
    string: Option<String>,
}

impl From<RawSetting> for Setting {
    fn from(raw: RawSetting) -> Self {
        if let Some(value) = raw.bool {
            Self::Bool(value)
        } else if let Some(value) = raw.int {
            Self::Int(value)
        } else if let Some(value) = raw.double {
            Self::Double(value)
        } else if let Some(value) = raw.float {
            Self::Float(value)
        } else if let Some(value) = raw.string {
            Self::String(value)
        } else {
            Self::Unset
        }
    }
}

impl Setting {
    /// Any numeric encoding as a float.
    pub fn as_f64(&self) -> Option<f64> {
        match self {
            Self::Float(value) | Self::Double(value) => Some(*value),
            #[expect(
                clippy::cast_precision_loss,
                reason = "settings are small integers; i64 beyond 2^53 never occurs"
            )]
            Self::Int(value) => Some(*value as f64),
            _ => None,
        }
    }

    /// An integer setting. Floats that hold a whole number count too, since a
    /// build may have stored an int as a float.
    pub fn as_i64(&self) -> Option<i64> {
        match self {
            Self::Int(value) => Some(*value),
            Self::Float(value) | Self::Double(value) if value.fract() == 0.0 => {
                // Whole and within range, so the conversion is exact.
                let whole = value.trunc();
                (-9.0e15..=9.0e15).contains(&whole).then(|| {
                    #[expect(
                        clippy::cast_possible_truncation,
                        reason = "checked whole and within ±9e15 above"
                    )]
                    let converted = whole as i64;
                    converted
                })
            }
            _ => None,
        }
    }

    pub fn as_bool(&self) -> Option<bool> {
        match self {
            Self::Bool(value) => Some(*value),
            _ => None,
        }
    }

    pub fn as_str(&self) -> Option<&str> {
        match self {
            Self::String(value) => Some(value),
            _ => None,
        }
    }
}

/// Which app wrote the export.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ExportKind {
    RectanglePro,
    Rectangle,
    /// Some other bundle id. Parsed anyway; the format is what matters.
    Other,
}

impl Export {
    pub fn parse(text: &str) -> Result<Self, ExportError> {
        if text.len() > MAX_EXPORT_BYTES {
            return Err(ExportError::TooLarge(text.len()));
        }
        // Exports written on a Mac carry no BOM, but a copy that went through a
        // Windows editor may.
        let text = text.strip_prefix('\u{feff}').unwrap_or(text);
        Ok(serde_json::from_str(text)?)
    }

    pub fn kind(&self) -> ExportKind {
        match self.bundle_id.as_str() {
            PRO_BUNDLE_ID => ExportKind::RectanglePro,
            RECTANGLE_BUNDLE_ID => ExportKind::Rectangle,
            _ => ExportKind::Other,
        }
    }

    pub fn setting(&self, key: &str) -> Option<&Setting> {
        self.defaults.get(key)
    }
}

impl ExportKind {
    pub fn label(self) -> &'static str {
        match self {
            Self::RectanglePro => "Rectangle Pro",
            Self::Rectangle => "Rectangle",
            Self::Other => "another app",
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum ExportError {
    #[error("the file is {0} bytes; a config to import is at most 1 MiB")]
    TooLarge(usize),
    #[error("not a config this app can import: {0}")]
    Json(#[from] serde_json::Error),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn settings_decode_by_their_single_key() {
        let export = Export::parse(
            r#"{ "bundleId": "com.knollsoft.Hookshot", "version": "235",
                 "shortcuts": {},
                 "defaults": {
                   "a": { "bool": true }, "b": { "int": 7 }, "c": { "float": 0.25 },
                   "d": { "double": 1 }, "e": { "string": "[]" }, "f": {} } }"#,
        )
        .expect("parses");
        assert_eq!(export.setting("a"), Some(&Setting::Bool(true)));
        assert_eq!(export.setting("b").and_then(Setting::as_i64), Some(7));
        assert_eq!(export.setting("c").and_then(Setting::as_f64), Some(0.25));
        assert_eq!(export.setting("d").and_then(Setting::as_i64), Some(1));
        assert_eq!(export.setting("e").and_then(Setting::as_str), Some("[]"));
        assert_eq!(export.setting("f"), Some(&Setting::Unset));
        assert_eq!(export.kind(), ExportKind::RectanglePro);
    }

    #[test]
    fn free_rectangle_exports_have_no_timestamp() {
        let export = Export::parse(
            r#"{ "bundleId": "com.knollsoft.Rectangle", "version": "111",
                 "shortcuts": { "leftHalf": { "keyCode": 123, "modifierFlags": 786432 } },
                 "defaults": {} }"#,
        )
        .expect("parses");
        assert_eq!(export.kind(), ExportKind::Rectangle);
        assert_eq!(export.timestamp, None);
        assert_eq!(export.shortcuts["leftHalf"].key_code, 123);
    }

    #[test]
    fn plist_style_shortcuts_with_a_type_field_still_parse() {
        let export = Export::parse(
            r#"{ "bundleId": "x", "version": "1", "defaults": {},
                 "shortcuts": { "leftHalf": { "keyCode": 123, "modifierFlags": 786432, "type": 0 } } }"#,
        )
        .expect("parses");
        assert_eq!(export.kind(), ExportKind::Other);
    }

    #[test]
    fn a_missing_section_is_an_error() {
        assert!(Export::parse(r#"{ "bundleId": "x", "version": "1", "defaults": {} }"#).is_err());
    }

    #[test]
    fn a_byte_order_mark_is_tolerated() {
        let text = "\u{feff}{ \"bundleId\": \"x\", \"version\": \"1\", \"shortcuts\": {}, \"defaults\": {} }";
        assert!(Export::parse(text).is_ok());
    }
}
