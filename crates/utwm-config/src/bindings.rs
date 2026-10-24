//! Turns an export plus the Windows overrides into the chords the keyboard hook
//! listens for.

use std::collections::BTreeMap;
use std::fmt;
use std::path::PathBuf;

use crate::actions::{self, ActionId, ActionName};
use crate::chord::{Chord, MacChord, ModifierMap};
use crate::export::{Export, ExportKind};
use crate::keys::{self, PhysicalKey};
use crate::overrides::Overrides;
use crate::settings::Settings;
use crate::specs::{self, ManualSpec};

/// What a chord runs.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Target {
    Action(ActionId),
    /// A Rectangle Pro custom size & position, by its id.
    Spec(i64),
}

/// One Windows chord and everything bound to it. More than one target means the
/// chord cycles through them, in order, on repeated presses.
#[derive(Clone, Debug, PartialEq)]
pub struct Binding {
    pub chord: Chord,
    pub entries: Vec<BindingEntry>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct BindingEntry {
    pub target: Target,
    /// The export's name for it, or the custom position's name.
    pub name: String,
    /// The chord on the Mac, if it came from the export.
    pub mac: Option<MacChord>,
    order: usize,
}

impl Binding {
    pub fn targets(&self) -> impl Iterator<Item = Target> + '_ {
        self.entries.iter().map(|entry| entry.target)
    }

    pub fn is_cycle(&self) -> bool {
        self.entries.len() > 1
    }
}

/// A shortcut from the export that does not bind on Windows.
#[derive(Clone, Debug, PartialEq)]
pub struct Skipped {
    pub name: String,
    pub mac: Option<MacChord>,
    pub reason: SkipReason,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SkipReason {
    /// The key sits where Windows has a lock key.
    LockKey(PhysicalKey),
    MouseButton,
    UnknownKeyCode,
    /// A real action this app doesn't run yet.
    Unsupported,
    UnknownAction,
    /// `""` in the override file.
    UnboundOnWindows,
}

impl fmt::Display for SkipReason {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::LockKey(key) => write!(
                f,
                "lands on {key}, a lock key; lock keys are never bound (RustDesk taps NumLock to sync lock state)"
            ),
            Self::MouseButton => f.write_str("a mouse-button shortcut; only keys bind on Windows"),
            Self::UnknownKeyCode => f.write_str("a macOS key code with no Windows equivalent"),
            Self::Unsupported => f.write_str("not supported on Windows yet"),
            Self::UnknownAction => {
                f.write_str("not an action Rectangle or Rectangle Pro is known to have")
            }
            Self::UnboundOnWindows => f.write_str("unbound on Windows in config.toml"),
        }
    }
}

/// Where the bindings came from.
#[derive(Clone, Debug, PartialEq)]
pub struct Source {
    pub kind: Option<ExportKind>,
    pub version: Option<String>,
    pub path: Option<PathBuf>,
}

impl Source {
    pub fn builtin() -> Self {
        Self {
            kind: None,
            version: None,
            path: None,
        }
    }

    pub fn describe(&self) -> String {
        match (&self.kind, &self.path) {
            (Some(kind), Some(path)) => format!(
                "{} export{} at {}",
                kind.label(),
                self.version
                    .as_deref()
                    .map(|version| format!(", build {version},"))
                    .unwrap_or_default(),
                path.display()
            ),
            (Some(kind), None) => format!("{} export", kind.label()),
            (None, _) => "Rectangle's recommended shortcuts (no export found)".to_owned(),
        }
    }
}

/// Everything the app needs from configuration, resolved.
#[derive(Clone, Debug, PartialEq)]
pub struct Resolved {
    pub source: Source,
    pub settings: Settings,
    pub modifier_map: ModifierMap,
    pub bindings: Vec<Binding>,
    pub skipped: Vec<Skipped>,
    pub specs: Vec<ManualSpec>,
    /// Custom positions Pro's menu hides; the tray hides them too.
    pub hidden_specs: Vec<i64>,
    pub ignored_apps: Vec<String>,
    /// Problems worth surfacing that did not stop loading.
    pub warnings: Vec<String>,
}

impl Resolved {
    pub fn binding_for(&self, chord: &Chord) -> Option<&Binding> {
        self.bindings.iter().find(|binding| binding.chord == *chord)
    }

    /// The first Windows chord that runs `target`, for menu labels.
    pub fn chord_for(&self, target: Target) -> Option<Chord> {
        self.bindings
            .iter()
            .find(|binding| binding.targets().any(|candidate| candidate == target))
            .map(|binding| binding.chord)
    }

    pub fn spec(&self, id: i64) -> Option<&ManualSpec> {
        self.specs.iter().find(|spec| spec.id == id)
    }
}

/// Resolves an export against the Windows overrides.
pub fn resolve(export: &Export, overrides: &Overrides, source: Source) -> Resolved {
    let map = overrides.modifiers;
    let mut warnings = Vec::new();
    let mut skipped = Vec::new();
    let mut entries: Vec<(Chord, BindingEntry)> = Vec::new();

    for (raw_name, shortcut) in &export.shortcuts {
        let name = actions::canonical_name(raw_name).to_owned();
        let mac = MacChord::new(shortcut.key_code, shortcut.modifier_flags);
        let target = match actions::resolve(&name) {
            ActionName::Supported(id) => Target::Action(id),
            ActionName::Unsupported => {
                skipped.push(Skipped {
                    name,
                    mac: Some(mac),
                    reason: SkipReason::Unsupported,
                });
                continue;
            }
            ActionName::Unknown => {
                skipped.push(Skipped {
                    name,
                    mac: Some(mac),
                    reason: SkipReason::UnknownAction,
                });
                continue;
            }
        };
        let order = actions::cycle_order(&name);
        match chord_for(&name, mac, &map, overrides, &mut warnings) {
            Ok(chord) => entries.push((
                chord,
                BindingEntry {
                    target,
                    name,
                    mac: Some(mac),
                    order,
                },
            )),
            Err(reason) => skipped.push(Skipped {
                name,
                mac: Some(mac),
                reason,
            }),
        }
    }

    // Windows-only bindings for actions the export leaves unbound.
    for (name, text) in &overrides.shortcuts {
        let name = actions::canonical_name(name).to_owned();
        if export
            .shortcuts
            .keys()
            .any(|existing| actions::canonical_name(existing) == name)
        {
            continue;
        }
        if text.trim().is_empty() {
            continue;
        }
        let ActionName::Supported(id) = actions::resolve(&name) else {
            warnings.push(format!(
                "config.toml binds `{name}`, which this app can't run"
            ));
            continue;
        };
        match Chord::parse(text) {
            Ok(chord) if chord.key.is_lock_key() => warnings.push(format!(
                "config.toml binds `{name}` to {chord}; lock keys are never bound"
            )),
            Ok(chord) => {
                let order = actions::cycle_order(&name);
                entries.push((
                    chord,
                    BindingEntry {
                        target: Target::Action(id),
                        name,
                        mac: None,
                        order,
                    },
                ));
            }
            Err(error) => warnings.push(format!("config.toml `{name}`: {error}")),
        }
    }

    let specs = match specs::parse_manual_specs(export.setting("manualSpecs")) {
        Ok(specs) => specs,
        Err(error) => {
            warnings.push(format!(
                "manualSpecs could not be read, so custom positions are off: {error}"
            ));
            Vec::new()
        }
    };
    let spec_order_base = usize::MAX / 2;
    for (index, spec) in specs.iter().enumerate() {
        let Some(mac) = spec.shortcut else { continue };
        let name = spec.name.clone();
        match translate(mac, &map) {
            Ok(chord) => entries.push((
                chord,
                BindingEntry {
                    target: Target::Spec(spec.id),
                    name,
                    mac: Some(mac),
                    order: spec_order_base + index,
                },
            )),
            Err(reason) => skipped.push(Skipped {
                name,
                mac: Some(mac),
                reason,
            }),
        }
    }

    let mut grouped: BTreeMap<Chord, Vec<BindingEntry>> = BTreeMap::new();
    for (chord, entry) in entries {
        grouped.entry(chord).or_default().push(entry);
    }
    let mut bindings: Vec<Binding> = grouped
        .into_iter()
        .map(|(chord, mut entries)| {
            entries.sort_by_key(|entry| entry.order);
            Binding { chord, entries }
        })
        .collect();
    bindings.sort_by_key(|binding| {
        binding
            .entries
            .first()
            .map_or(usize::MAX, |entry| entry.order)
    });
    skipped.sort_by_key(|skip| actions::cycle_order(&skip.name));

    Resolved {
        source,
        settings: Settings::from_export(export),
        modifier_map: map,
        bindings,
        skipped,
        hidden_specs: specs::hidden_spec_ids(export.setting("menuCustomSpecVisibility")),
        specs,
        ignored_apps: overrides.ignored_apps.clone(),
        warnings,
    }
}

/// The Windows chord for one export shortcut, after any override.
fn chord_for(
    name: &str,
    mac: MacChord,
    map: &ModifierMap,
    overrides: &Overrides,
    warnings: &mut Vec<String>,
) -> Result<Chord, SkipReason> {
    if let Some(text) = overrides.shortcuts.get(name) {
        if text.trim().is_empty() {
            return Err(SkipReason::UnboundOnWindows);
        }
        match Chord::parse(text) {
            Ok(chord) if chord.key.is_lock_key() => warnings.push(format!(
                "config.toml binds `{name}` to {chord}; lock keys are never bound, so the Mac chord is used"
            )),
            Ok(chord) => return Ok(chord),
            Err(error) => warnings.push(format!(
                "config.toml `{name}`: {error}; the Mac chord is used instead"
            )),
        }
    }
    translate(mac, map)
}

fn translate(mac: MacChord, map: &ModifierMap) -> Result<Chord, SkipReason> {
    if keys::is_mouse_button_code(mac.key_code) {
        return Err(SkipReason::MouseButton);
    }
    let key = keys::from_mac_key_code(mac.key_code).ok_or(SkipReason::UnknownKeyCode)?;
    if key.is_lock_key() {
        return Err(SkipReason::LockKey(key));
    }
    Ok(Chord {
        modifiers: map.translate(mac.modifiers),
        key,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    fn export(shortcuts: &str, defaults: &str) -> Export {
        Export::parse(&format!(
            r#"{{ "bundleId": "com.knollsoft.Hookshot", "version": "235",
                 "shortcuts": {{ {shortcuts} }}, "defaults": {{ {defaults} }} }}"#
        ))
        .expect("test export parses")
    }

    fn resolve_plain(export: &Export) -> Resolved {
        resolve(export, &Overrides::default(), Source::builtin())
    }

    #[test]
    fn shared_chords_cycle_rectangle_actions_before_pro_ones() {
        let resolved = resolve_plain(&export(
            r#""fillLeft": { "keyCode": 14, "modifierFlags": 786432 },
               "firstTwoThirds": { "keyCode": 14, "modifierFlags": 786432 }"#,
            "",
        ));
        assert_eq!(resolved.bindings.len(), 1);
        let binding = &resolved.bindings[0];
        assert_eq!(binding.chord.to_string(), "Ctrl+Alt+E");
        let targets: Vec<Target> = binding.targets().collect();
        assert_eq!(
            targets,
            [
                Target::Action(ActionId::FirstTwoThirds),
                Target::Action(ActionId::FillLeft)
            ]
        );
    }

    #[test]
    fn keypad_clear_is_skipped_as_a_lock_key() {
        let resolved = resolve_plain(&export(
            r#""topLeftEighth": { "keyCode": 71, "modifierFlags": 786432 },
               "bottomLeftEighth": { "keyCode": 89, "modifierFlags": 786432 }"#,
            "",
        ));
        assert_eq!(resolved.bindings.len(), 1);
        assert_eq!(resolved.bindings[0].chord.to_string(), "Ctrl+Alt+Num7");
        assert_eq!(resolved.skipped.len(), 1);
        assert_eq!(resolved.skipped[0].name, "topLeftEighth");
        assert!(matches!(resolved.skipped[0].reason, SkipReason::LockKey(_)));
    }

    #[test]
    fn unsupported_and_unknown_actions_are_reported() {
        let resolved = resolve_plain(&export(
            r#""tileAll": { "keyCode": 0, "modifierFlags": 786432 },
               "teleport": { "keyCode": 1, "modifierFlags": 786432 }"#,
            "",
        ));
        assert!(resolved.bindings.is_empty());
        let reasons: Vec<&SkipReason> = resolved.skipped.iter().map(|skip| &skip.reason).collect();
        assert!(reasons.contains(&&SkipReason::Unsupported));
        assert!(reasons.contains(&&SkipReason::UnknownAction));
    }

    #[test]
    fn overrides_rebind_and_unbind() {
        let overrides = Overrides::parse(
            "[shortcuts]\ntopLeftEighth = \"ctrl+alt+num0\"\nrestore = \"\"\nalmostMaximize = \"ctrl+alt+a\"",
            Path::new("config.toml"),
        )
        .expect("parses");
        let resolved = resolve(
            &export(
                r#""topLeftEighth": { "keyCode": 71, "modifierFlags": 786432 },
                   "restore": { "keyCode": 51, "modifierFlags": 786432 }"#,
                "",
            ),
            &overrides,
            Source::builtin(),
        );
        let chords: Vec<String> = resolved
            .bindings
            .iter()
            .map(|binding| binding.chord.to_string())
            .collect();
        assert_eq!(chords, ["Ctrl+Alt+A", "Ctrl+Alt+Num0"]);
        assert_eq!(resolved.skipped.len(), 1);
        assert_eq!(resolved.skipped[0].reason, SkipReason::UnboundOnWindows);
    }

    #[test]
    fn custom_specs_with_shortcuts_bind() {
        let resolved = resolve_plain(&export(
            "",
            r#""manualSpecs": { "string": "[{\"id\":33,\"name\":\"Left 1/3 Below Movie\",\"shortcut\":{\"modifierFlags\":786432,\"keyCode\":7},\"w\":\".333\",\"h\":\".7\",\"loc\":7},{\"id\":66,\"name\":\"Menu only\",\"w\":\"1168\",\"h\":\"688\",\"loc\":0}]" }"#,
        ));
        assert_eq!(resolved.bindings.len(), 1);
        assert_eq!(resolved.bindings[0].chord.to_string(), "Ctrl+Alt+X");
        assert_eq!(resolved.bindings[0].entries[0].target, Target::Spec(33));
        assert_eq!(resolved.specs.len(), 2);
    }

    #[test]
    fn a_broken_manual_specs_blob_is_a_warning() {
        let resolved = resolve_plain(&export("", r#""manualSpecs": { "string": "{" }"#));
        assert_eq!(resolved.warnings.len(), 1);
    }
}
