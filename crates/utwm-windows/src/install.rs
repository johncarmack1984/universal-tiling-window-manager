//! Where the installer put the app. It records the folder under the uninstall
//! key Settings → Apps reads: the machine's key since releases install for
//! every user, and the user's own before that.

use std::path::{Path, PathBuf};

use crate::registry::{self, HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE};

/// The installer's uninstall key, named after the product.
const UNINSTALL_KEY: &str = r"Software\Microsoft\Windows\CurrentVersion\Uninstall\Window Manager";

/// The folder a release installed for every user is in.
pub fn for_all_users() -> Option<PathBuf> {
    location(registry::read_string(
        HKEY_LOCAL_MACHINE,
        UNINSTALL_KEY,
        "InstallLocation",
    ))
}

/// The folder an earlier release installed for this user alone is in, until
/// the first release for every user moved it.
pub fn for_this_user() -> Option<PathBuf> {
    location(registry::read_string(
        HKEY_CURRENT_USER,
        UNINSTALL_KEY,
        "InstallLocation",
    ))
}

/// The installer writes the folder in quotes.
fn location(value: Option<String>) -> Option<PathBuf> {
    value
        .map(|value| value.trim().trim_matches('"').to_owned())
        .filter(|folder| !folder.is_empty())
        .map(PathBuf::from)
}

/// Whether two paths name the same file or folder, as Windows compares them:
/// regardless of case, the slashes' direction and a trailing separator.
pub fn same_path(a: &Path, b: &Path) -> bool {
    normalize(a) == normalize(b)
}

fn normalize(path: &Path) -> String {
    path.to_string_lossy()
        .replace('/', "\\")
        .trim_end_matches('\\')
        .to_lowercase()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_recorded_folder_loses_its_quotes() {
        assert_eq!(
            location(Some(r#""C:\Program Files\Window Manager""#.to_owned())),
            Some(PathBuf::from(r"C:\Program Files\Window Manager"))
        );
        assert_eq!(
            location(Some(r"C:\Users\me\AppData\Local\Window Manager".to_owned())),
            Some(PathBuf::from(r"C:\Users\me\AppData\Local\Window Manager"))
        );
        assert_eq!(location(Some(r#""""#.to_owned())), None);
        assert_eq!(location(None), None);
    }

    #[test]
    fn paths_compare_as_windows_compares_them() {
        assert!(same_path(
            Path::new(r"C:\Program Files\Window Manager"),
            Path::new(r"c:\program files\window manager\")
        ));
        assert!(same_path(
            Path::new(r"C:\Program Files\Window Manager\universal-tiling-window-manager.exe"),
            Path::new("C:/Program Files/Window Manager/universal-tiling-window-manager.exe")
        ));
        assert!(!same_path(
            Path::new(r"C:\Program Files\Window Manager"),
            Path::new(r"C:\Users\me\AppData\Local\Window Manager")
        ));
    }
}
