//! Turns an export plus one platform's part of the overrides file into the
//! chords that platform listens for.
//!
//! A chord names a key by position and modifiers the PC way (scan code; Ctrl,
//! Alt, Shift, Win), because that is what the Windows hook matches. On the Mac
//! the same chord is the macOS key code in that position with Control, Option,
//! Shift and Command, so an export resolves to exactly the shortcuts Rectangle
//! Pro bound.

use std::collections::BTreeMap;
use std::fmt;
use std::path::PathBuf;

use crate::actions::{self, ActionId, ActionName};
use crate::chord::{Chord, MacChord, ModifierMap};
use crate::export::{Export, ExportKind};
use crate::keys::{self, PhysicalKey};
use crate::overrides::Overrides;
use crate::platform::Platform;
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

/// A shortcut from the export that does not bind on this platform.
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
    Unbound,
}

impl fmt::Display for SkipReason {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::LockKey(key) => write!(
                f,
                "lands on {key}, a lock key; lock keys are never bound (RustDesk taps NumLock to sync lock state)"
            ),
            Self::MouseButton => f.write_str("a mouse-button shortcut; only keys bind"),
            Self::UnknownKeyCode => f.write_str("a macOS key code this app has no key for"),
            Self::Unsupported => f.write_str("not supported yet"),
            Self::UnknownAction => f.write_str("not an action this app knows"),
            Self::Unbound => f.write_str("unbound in config.toml"),
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
                "imported from {}{} at {}",
                kind.label(),
                self.version
                    .as_deref()
                    .map(|version| format!(" (build {version})"))
                    .unwrap_or_default(),
                path.display()
            ),
            (Some(kind), None) => format!("imported from {}", kind.label()),
            (None, _) => "the default shortcuts (nothing to import)".to_owned(),
        }
    }
}

/// Everything the app needs from configuration, resolved.
#[derive(Clone, Debug, PartialEq)]
pub struct Resolved {
    pub platform: Platform,
    pub source: Source,
    pub settings: Settings,
    pub modifier_map: ModifierMap,
    pub bindings: Vec<Binding>,
    pub skipped: Vec<Skipped>,
    pub specs: Vec<ManualSpec>,
    /// Custom positions Pro's menu hides; the tray hides them too.
    pub hidden_specs: Vec<i64>,
    /// Apps whose windows are left alone: executable names on Windows, bundle
    /// ids on the Mac.
    pub ignored_apps: Vec<String>,
    /// Problems worth surfacing that did not stop loading.
    pub warnings: Vec<String>,
}

impl Resolved {
    pub fn binding_for(&self, chord: &Chord) -> Option<&Binding> {
        self.bindings.iter().find(|binding| binding.chord == *chord)
    }

    /// The first chord that runs `target`, for menu labels.
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

/// Resolves an export for `platform`, with that platform's part of the
/// overrides file.
pub fn resolve(
    export: &Export,
    overrides: &Overrides,
    source: Source,
    platform: Platform,
) -> Resolved {
    let keyboard = Keyboard::new(platform, overrides);
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
        match keyboard.chord_for(&name, mac, &mut warnings) {
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

    // Bindings this platform adds for actions the export leaves unbound.
    for (name, text) in keyboard.shortcuts {
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
        let section = keyboard.section();
        let ActionName::Supported(id) = actions::resolve(&name) else {
            warnings.push(format!(
                "config.toml {section} binds `{name}`, which this app can't run"
            ));
            continue;
        };
        match Chord::parse(text) {
            Ok(chord) => match keyboard.refuses(chord) {
                Some(why) => warnings.push(format!(
                    "config.toml {section} binds `{name}` to {}; {why}",
                    keyboard.describe(chord)
                )),
                None => {
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
            },
            Err(error) => warnings.push(format!("config.toml {section} `{name}`: {error}")),
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
        match keyboard.translate(mac) {
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

    let mut settings = Settings::from_export(export);
    let ignored_apps = match platform {
        Platform::Windows => {
            if !settings.disabled_apps.is_empty() {
                settings.not_applied.push((
                    "disabledApps".to_owned(),
                    "macOS bundle ids; list Windows apps under ignored_apps in config.toml"
                        .to_owned(),
                ));
            }
            overrides.ignored_apps.clone()
        }
        // Rectangle's own ignore list, plus any the Mac adds.
        Platform::Mac => settings
            .disabled_apps
            .iter()
            .chain(&overrides.mac.ignored_apps)
            .cloned()
            .collect(),
    };

    Resolved {
        platform,
        source,
        settings,
        modifier_map: keyboard.map,
        bindings,
        skipped,
        hidden_specs: specs::hidden_spec_ids(export.setting("menuCustomSpecVisibility")),
        specs,
        ignored_apps,
        warnings,
    }
}

/// How shortcuts become chords on one platform.
struct Keyboard<'a> {
    platform: Platform,
    map: ModifierMap,
    /// The overrides file's rebinds for this platform.
    shortcuts: &'a BTreeMap<String, String>,
}

impl<'a> Keyboard<'a> {
    fn new(platform: Platform, overrides: &'a Overrides) -> Self {
        match platform {
            Platform::Windows => Self {
                platform,
                map: overrides.modifiers,
                shortcuts: &overrides.shortcuts,
            },
            // The Mac presses its own shortcuts: Control, Option, Shift and
            // Command are Ctrl, Alt, Shift and Win, which is the default map.
            Platform::Mac => Self {
                platform,
                map: ModifierMap::default(),
                shortcuts: &overrides.mac.shortcuts,
            },
        }
    }

    /// Where this platform's rebinds live in config.toml, for messages.
    fn section(&self) -> &'static str {
        match self.platform {
            Platform::Windows => "[shortcuts]",
            Platform::Mac => "[mac.shortcuts]",
        }
    }

    /// A chord the way this platform's keyboard labels it.
    fn describe(&self, chord: Chord) -> String {
        match (self.platform, chord.to_mac()) {
            (Platform::Mac, Some(mac)) => mac.to_string(),
            _ => chord.to_string(),
        }
    }

    /// Why an overridden chord can't bind here, if it can't.
    fn refuses(&self, chord: Chord) -> Option<&'static str> {
        match self.platform {
            Platform::Windows if chord.key.is_lock_key() => Some("lock keys are never bound"),
            Platform::Mac if chord.to_mac().is_none() => {
                Some("a Mac keyboard has no key in that position")
            }
            _ => None,
        }
    }

    /// The chord for one export shortcut, after any override.
    fn chord_for(
        &self,
        name: &str,
        mac: MacChord,
        warnings: &mut Vec<String>,
    ) -> Result<Chord, SkipReason> {
        if let Some(text) = self.shortcuts.get(name) {
            if text.trim().is_empty() {
                return Err(SkipReason::Unbound);
            }
            let section = self.section();
            match Chord::parse(text) {
                Ok(chord) => match self.refuses(chord) {
                    Some(why) => warnings.push(format!(
                        "config.toml {section} binds `{name}` to {}; {why}, so the export's chord is used",
                        self.describe(chord)
                    )),
                    None => return Ok(chord),
                },
                Err(error) => warnings.push(format!(
                    "config.toml {section} `{name}`: {error}; the export's chord is used instead"
                )),
            }
        }
        self.translate(mac)
    }

    fn translate(&self, mac: MacChord) -> Result<Chord, SkipReason> {
        if keys::is_mouse_button_code(mac.key_code) {
            return Err(SkipReason::MouseButton);
        }
        let key = keys::from_mac_key_code(mac.key_code).ok_or(SkipReason::UnknownKeyCode)?;
        // A Mac's keypad Clear sits where Windows has NumLock; only Windows
        // can't bind it.
        if self.platform == Platform::Windows && key.is_lock_key() {
            return Err(SkipReason::LockKey(key));
        }
        Ok(Chord {
            modifiers: self.map.translate(mac.modifiers),
            key,
        })
    }
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
        resolve(
            export,
            &Overrides::default(),
            Source::builtin(),
            Platform::Windows,
        )
    }

    fn resolve_mac(export: &Export, overrides: &Overrides) -> Resolved {
        resolve(export, overrides, Source::builtin(), Platform::Mac)
    }

    fn mac_chords(resolved: &Resolved) -> Vec<String> {
        resolved
            .bindings
            .iter()
            .map(|binding| {
                binding
                    .chord
                    .to_mac()
                    .map_or_else(|| "?".to_owned(), |mac| mac.to_string())
            })
            .collect()
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
            Platform::Windows,
        );
        let chords: Vec<String> = resolved
            .bindings
            .iter()
            .map(|binding| binding.chord.to_string())
            .collect();
        assert_eq!(chords, ["Ctrl+Alt+A", "Ctrl+Alt+Num0"]);
        assert_eq!(resolved.skipped.len(), 1);
        assert_eq!(resolved.skipped[0].reason, SkipReason::Unbound);
    }

    #[test]
    fn the_mac_binds_keypad_clear_and_keeps_its_own_modifiers() {
        let overrides = Overrides::parse(
            "[modifiers]\ncommand = \"ctrl\"\n[shortcuts]\ntopLeftEighth = \"ctrl+alt+num0\"",
            Path::new("config.toml"),
        )
        .expect("parses");
        let resolved = resolve_mac(
            &export(
                r#""topLeftEighth": { "keyCode": 71, "modifierFlags": 786432 },
                   "nextDisplay": { "keyCode": 124, "modifierFlags": 1835008 }"#,
                "",
            ),
            &overrides,
        );
        assert!(resolved.skipped.is_empty());
        assert_eq!(
            mac_chords(&resolved),
            ["⌃⌥⌘→", "⌃⌥ Keypad Clear"],
            "Windows' [modifiers] and [shortcuts] stay on Windows"
        );
    }

    #[test]
    fn the_mac_table_rebinds_and_unbinds_on_the_mac_only() {
        let overrides = Overrides::parse(
            "[mac.shortcuts]\nleftHalf = \"ctrl+option+h\"\nrestore = \"\"\ncenter = \"cmd+alt+c\"",
            Path::new("config.toml"),
        )
        .expect("parses");
        let shortcuts = export(
            r#""leftHalf": { "keyCode": 123, "modifierFlags": 786432 },
               "restore": { "keyCode": 51, "modifierFlags": 786432 }"#,
            "",
        );
        let mac = resolve_mac(&shortcuts, &overrides);
        assert_eq!(mac_chords(&mac), ["⌃⌥H", "⌥⌘C"]);
        assert_eq!(mac.skipped.len(), 1);
        assert_eq!(mac.skipped[0].reason, SkipReason::Unbound);

        let windows = resolve(&shortcuts, &overrides, Source::builtin(), Platform::Windows);
        let chords: Vec<String> = windows
            .bindings
            .iter()
            .map(|binding| binding.chord.to_string())
            .collect();
        assert_eq!(chords, ["Ctrl+Alt+Left", "Ctrl+Alt+Backspace"]);
    }

    #[test]
    fn a_mac_rebind_to_a_pc_only_key_is_a_warning() {
        let overrides = Overrides::parse(
            "[mac.shortcuts]\nleftHalf = \"ctrl+menu\"",
            Path::new("config.toml"),
        )
        .expect("parses");
        let resolved = resolve_mac(
            &export(
                r#""leftHalf": { "keyCode": 123, "modifierFlags": 786432 }"#,
                "",
            ),
            &overrides,
        );
        assert_eq!(mac_chords(&resolved), ["⌃⌥←"]);
        assert_eq!(resolved.warnings.len(), 1);
    }

    #[test]
    fn ignored_apps_follow_the_platform() {
        let overrides = Overrides::parse(
            "ignored_apps = [\"mstsc.exe\"]\n[mac]\nignored_apps = [\"com.apple.Terminal\"]",
            Path::new("config.toml"),
        )
        .expect("parses");
        let with_disabled = export(
            "",
            r#""disabledApps": { "string": "[\"com.example.Game\"]" }"#,
        );
        let mac = resolve_mac(&with_disabled, &overrides);
        assert_eq!(mac.ignored_apps, ["com.example.Game", "com.apple.Terminal"]);
        assert!(mac.settings.not_applied.is_empty());

        let windows = resolve(
            &with_disabled,
            &overrides,
            Source::builtin(),
            Platform::Windows,
        );
        assert_eq!(windows.ignored_apps, ["mstsc.exe"]);
        assert_eq!(windows.settings.not_applied.len(), 1);
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
