//! GNOME's settings, through the `gsettings` command every GNOME desktop has.
//! It reads a key's default when nobody has changed it, and honours
//! `GSETTINGS_BACKEND`, which keeps the headless check away from the real
//! settings.

use std::path::Path;
use std::process::Command;

/// The system's own copy. A `gsettings` from Homebrew or Nix, ahead of it on
/// the path, may lack the dconf module and write nowhere.
const SYSTEM_GSETTINGS: &str = "/usr/bin/gsettings";

fn gsettings() -> Command {
    if Path::new(SYSTEM_GSETTINGS).is_file() {
        Command::new(SYSTEM_GSETTINGS)
    } else {
        Command::new("gsettings")
    }
}

/// The value of `key` in `schema` (`schema:path` for a relocatable one), in
/// GVariant's text form.
pub(crate) fn get(schema: &str, key: &str) -> Option<String> {
    let output = gsettings().args(["get", schema, key]).output().ok()?;
    output
        .status
        .success()
        .then(|| String::from_utf8_lossy(&output.stdout).trim().to_owned())
}

/// Sets `key` in `schema` to `value`, given in GVariant's text form.
pub(crate) fn set(schema: &str, key: &str, value: &str) -> std::io::Result<()> {
    let output = gsettings().args(["set", schema, key, value]).output()?;
    if output.status.success() {
        Ok(())
    } else {
        Err(std::io::Error::other(format!(
            "gsettings set {schema} {key}: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        )))
    }
}

/// Every key of `schema` with its value in GVariant's text form. Empty when
/// the schema isn't installed.
pub(crate) fn list(schema: &str) -> Vec<(String, String)> {
    let Ok(output) = gsettings().args(["list-recursively", schema]).output() else {
        return Vec::new();
    };
    if !output.status.success() {
        return Vec::new();
    }
    parse_listing(schema, &String::from_utf8_lossy(&output.stdout))
}

/// `gsettings list-recursively` prints one `schema key value` line per key.
fn parse_listing(schema: &str, text: &str) -> Vec<(String, String)> {
    text.lines()
        .filter_map(|line| {
            let (key, value) = line
                .strip_prefix(schema)?
                .strip_prefix(' ')?
                .split_once(' ')?;
            Some((key.to_owned(), value.to_owned()))
        })
        .collect()
}

/// A list of strings in GVariant's text form (`['a', "b's"]`, `@as []`), with
/// the escapes these settings use; `None` for any other value.
pub(crate) fn parse_strings(text: &str) -> Option<Vec<String>> {
    let text = text.trim();
    let text = text.strip_prefix("@as").map_or(text, str::trim_start);
    let mut chars = text
        .strip_prefix('[')?
        .strip_suffix(']')?
        .chars()
        .peekable();
    let mut strings = Vec::new();
    loop {
        while chars.next_if(|c| c.is_whitespace()).is_some() {}
        let Some(quote) = chars.next() else {
            return Some(strings);
        };
        if quote != '\'' && quote != '"' {
            return None;
        }
        let mut string = String::new();
        loop {
            match chars.next()? {
                '\\' => string.push(chars.next()?),
                c if c == quote => break,
                c => string.push(c),
            }
        }
        strings.push(string);
        while chars.next_if(|c| c.is_whitespace()).is_some() {}
        match chars.next() {
            Some(',') => {}
            None => return Some(strings),
            Some(_) => return None,
        }
    }
}

/// One string in GVariant's text form (`'<Primary><Alt>t'`).
pub(crate) fn parse_string(text: &str) -> Option<String> {
    let mut strings = parse_strings(&format!("[{}]", text.trim()))?;
    (strings.len() == 1).then(|| strings.remove(0))
}

/// `strings` in GVariant's text form, as `gsettings set` takes a list.
pub(crate) fn format_strings(strings: &[String]) -> String {
    if strings.is_empty() {
        return "@as []".to_owned();
    }
    let quoted: Vec<String> = strings.iter().map(|string| quote(string)).collect();
    format!("[{}]", quoted.join(", "))
}

/// `text` as one GVariant string.
pub(crate) fn quote(text: &str) -> String {
    let mut quoted = String::from("'");
    for c in text.chars() {
        if c == '\'' || c == '\\' {
            quoted.push('\\');
        }
        quoted.push(c);
    }
    quoted.push('\'');
    quoted
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lists_read_the_way_gsettings_prints_them() {
        assert_eq!(
            parse_strings("['<Super>Page_Up', '<Super>KP_Prior', '<Control><Alt>Left']"),
            Some(vec![
                "<Super>Page_Up".to_owned(),
                "<Super>KP_Prior".to_owned(),
                "<Control><Alt>Left".to_owned()
            ])
        );
        assert_eq!(parse_strings("@as []"), Some(Vec::new()));
        assert_eq!(parse_strings("[]"), Some(Vec::new()));
        assert_eq!(
            parse_strings(r#"["it's", 'a\'b']"#),
            Some(vec!["it's".to_owned(), "a'b".to_owned()])
        );
        assert_eq!(parse_strings("6"), None);
        assert_eq!(parse_strings("['a' 'b']"), None);
        assert_eq!(
            parse_string("'<Primary><Alt>t'").as_deref(),
            Some("<Primary><Alt>t")
        );
        assert_eq!(parse_string("true"), None);
    }

    #[test]
    fn lists_write_back_the_way_gsettings_reads_them() {
        let strings = vec!["<Super>Page_Up".to_owned(), "it's".to_owned()];
        let text = format_strings(&strings);
        assert_eq!(text, r"['<Super>Page_Up', 'it\'s']");
        assert_eq!(parse_strings(&text), Some(strings));
        assert_eq!(format_strings(&[]), "@as []");
    }

    #[test]
    fn listings_split_into_keys_and_values() {
        let listing = "org.gnome.desktop.wm.keybindings show-desktop ['<Primary><Super>d', '<Primary><Alt>d']\norg.gnome.desktop.wm.keybindings toggle-fullscreen @as []\n";
        assert_eq!(
            parse_listing("org.gnome.desktop.wm.keybindings", listing),
            vec![
                (
                    "show-desktop".to_owned(),
                    "['<Primary><Super>d', '<Primary><Alt>d']".to_owned()
                ),
                ("toggle-fullscreen".to_owned(), "@as []".to_owned())
            ]
        );
    }
}
