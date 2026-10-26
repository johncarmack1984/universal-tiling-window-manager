//! The text `check` prints: what each shortcut becomes on this platform, what
//! was left out and why, and the behaviour settings in effect.

use crate::bindings::{Binding, Resolved, Target};
use crate::platform::Platform;
use crate::settings::{EdgeAlignment, RepeatMode, ScreenOrdering};

pub fn render(resolved: &Resolved) -> String {
    let mut out = String::new();
    let mut line = |text: String| {
        out.push_str(&text);
        out.push('\n');
    };

    line(format!("Source:    {}", resolved.source.describe()));
    if resolved.platform == Platform::Windows {
        line(format!("Modifiers: {}", resolved.modifier_map));
    }
    line(String::new());

    line(format!("Bindings ({} chords):", resolved.bindings.len()));
    match resolved.platform {
        Platform::Windows | Platform::Linux => {
            let rows: Vec<[String; 3]> = resolved
                .bindings
                .iter()
                .map(|binding| {
                    let mac = binding
                        .entries
                        .iter()
                        .find_map(|entry| entry.mac)
                        .map_or_else(
                            || format!("({} only)", resolved.platform),
                            |mac| mac.to_string(),
                        );
                    [mac, binding.chord.label(resolved.platform), action(binding)]
                })
                .collect();
            let widths = column_widths(&rows);
            for [mac, chord, action] in &rows {
                line(format!(
                    "  {}  {}  {action}",
                    pad(mac, widths[0]),
                    pad(chord, widths[1])
                ));
            }
        }
        Platform::Mac => {
            let rows: Vec<[String; 2]> = resolved
                .bindings
                .iter()
                .map(|binding| {
                    let chord = binding.chord.to_mac();
                    let from_export = binding.entries.iter().any(|entry| entry.mac == chord);
                    let chord =
                        chord.map_or_else(|| binding.chord.to_string(), |mac| mac.to_string());
                    let action = action(binding);
                    if from_export {
                        [chord, action]
                    } else {
                        [chord, format!("{action} (config.toml)")]
                    }
                })
                .collect();
            let width = rows
                .iter()
                .map(|row| row[0].chars().count())
                .max()
                .unwrap_or(0);
            for [chord, action] in &rows {
                line(format!("  {}  {action}", pad(chord, width)));
            }
        }
    }

    if !resolved.skipped.is_empty() {
        line(String::new());
        line(format!(
            "Not bound on {} ({}):",
            resolved.platform,
            resolved.skipped.len()
        ));
        let rows: Vec<[String; 3]> = resolved
            .skipped
            .iter()
            .map(|skip| {
                [
                    skip.name.clone(),
                    skip.mac.map(|mac| mac.to_string()).unwrap_or_default(),
                    skip.reason.to_string(),
                ]
            })
            .collect();
        let widths = column_widths(&rows);
        for [name, mac, reason] in &rows {
            line(format!(
                "  {}  {}  {reason}",
                pad(name, widths[0]),
                pad(mac, widths[1])
            ));
        }
    }

    line(String::new());
    line("Behaviour:".to_owned());
    let settings = &resolved.settings;
    let repeat = match settings.repeat_mode {
        RepeatMode::Resize | RepeatMode::AcrossAndResize | RepeatMode::ResizeAndCycleQuadrants => {
            format!(
                "{} ({})",
                settings.repeat_mode.describe(),
                settings.cycle_sizes.describe()
            )
        }
        other => other.describe().to_owned(),
    };
    line(format!("  repeat:          {repeat}"));
    line(format!(
        "  gaps:            {} pt between windows; screen edges {}/{}/{}/{} pt (top/bottom/left/right)",
        settings.gap_size,
        settings.screen_edge_gaps.top,
        settings.screen_edge_gaps.bottom,
        settings.screen_edge_gaps.left,
        settings.screen_edge_gaps.right
    ));
    line(format!(
        "  larger/smaller:  {} pt steps, never below {:.0}% × {:.0}% of the screen",
        settings.size_offset,
        settings.minimum_window_width * 100.0,
        settings.minimum_window_height * 100.0
    ));
    line(format!(
        "  almost maximize: {:.0}% × {:.0}%",
        settings.almost_maximize_width * 100.0,
        settings.almost_maximize_height * 100.0
    ));
    line(format!(
        "  displays:        ordered {}; maximized windows {} maximized on the next display",
        match settings.screen_ordering {
            ScreenOrdering::MidX => "by center",
            ScreenOrdering::MinX => "by left edge",
            ScreenOrdering::YThenMinX => "row by row, then left to right",
        },
        if settings.auto_maximize {
            "stay"
        } else {
            "don't stay"
        }
    ));
    line(format!(
        "  fixed-size apps: {}",
        match settings.move_fixed_size_to_edge {
            EdgeAlignment::EdgesAndCorners => "kept against the edges they were sent to",
            EdgeAlignment::Corners => "kept in the corner they were sent to",
            EdgeAlignment::Centered => "centered in the area they were sent to",
            EdgeAlignment::LeadingCorner => "placed at the area's top-left corner",
        }
    ));
    if !resolved.specs.is_empty() {
        let names: Vec<String> = resolved
            .specs
            .iter()
            .map(|spec| match spec.shortcut {
                Some(_) => spec.name.clone(),
                None => format!("{} (menu only)", spec.name),
            })
            .collect();
        line(format!("  custom:          {}", names.join("; ")));
    }
    if !resolved.ignored_apps.is_empty() {
        line(format!(
            "  ignored apps:    {}",
            resolved.ignored_apps.join(", ")
        ));
    }

    if !settings.not_applied.is_empty() {
        line(String::new());
        line("Read but not applied:".to_owned());
        for (key, why) in &settings.not_applied {
            line(format!("  {key}: {why}"));
        }
    }
    if !resolved.warnings.is_empty() {
        line(String::new());
        line("Warnings:".to_owned());
        for warning in &resolved.warnings {
            line(format!("  {warning}"));
        }
    }
    out
}

/// What a binding runs: its action, or the actions it cycles through.
fn action(binding: &Binding) -> String {
    let names: Vec<String> = binding
        .entries
        .iter()
        .map(|entry| match entry.target {
            Target::Action(_) => entry.name.clone(),
            Target::Spec(_) => format!("custom position \"{}\"", entry.name),
        })
        .collect();
    if binding.is_cycle() {
        format!("{} (cycles on repeat)", names.join(" → "))
    } else {
        names.join("")
    }
}

fn column_widths(rows: &[[String; 3]]) -> [usize; 2] {
    let width = |column: usize| {
        rows.iter()
            .map(|row| row[column].chars().count())
            .max()
            .unwrap_or(0)
    };
    [width(0), width(1)]
}

fn pad(text: &str, width: usize) -> String {
    let len = text.chars().count();
    format!("{text}{}", " ".repeat(width.saturating_sub(len)))
}
