//! Platform data-directory resolution, with the environment injected so tests
//! never touch process env (the seam `config::config_dir` and
//! `plugin::plugin_dir` already use).
//!
//! One rule serves config, labels, favourites and plugins:
//!
//! - Linux and macOS: an absolute, non-empty `$XDG_CONFIG_HOME`, else
//!   `$HOME/.config`. A non-absolute `$XDG_CONFIG_HOME` is treated as unset,
//!   per the XDG spec, and `%APPDATA%` is ignored.
//! - Windows: `%APPDATA%` when it is absolute and non-empty; otherwise fall
//!   back to `$HOME/.config` so a MSYS/Cygwin-style environment still works.
//!   Nothing else resolves there.
//!
//! Consumers take [`config_dir_from`] (injectable) or [`config_dir`]
//! (process environment) and receive the application directory itself.
//!
//! The module also owns [`drive_roots`], the file picker's drive list. It is
//! empty on Linux and macOS, so the picker never grows a second branch.

use std::env;
use std::ffi::OsStr;
use std::path::{Path, PathBuf};

/// Directory name appended to the platform base for all DROID TUI user data.
pub const APP_DIR_NAME: &str = "droid-tui";

/// Resolve the platform data-directory base from explicit environment values.
///
/// Returns `None` when nothing resolves. On Windows only `appdata` and `home`
/// are consulted; elsewhere only `xdg_config_home` and `home`.
pub fn data_dir(
    xdg_config_home: Option<&OsStr>,
    home: Option<&OsStr>,
    appdata: Option<&OsStr>,
) -> Option<PathBuf> {
    #[cfg(windows)]
    let base = {
        let _ = xdg_config_home;
        // A relative `%APPDATA%` would resolve against the process cwd, so it
        // counts as unset exactly like a relative XDG value does.
        non_empty(appdata)
            .filter(|value| Path::new(value).is_absolute())
            .map(PathBuf::from)
            .or_else(|| home_config(home))
    };

    #[cfg(not(windows))]
    let base = {
        let _ = appdata;
        non_empty(xdg_config_home)
            .filter(|xdg| Path::new(xdg).is_absolute())
            .map(PathBuf::from)
            .or_else(|| home_config(home))
    };

    base
}

/// Resolve the application directory (`<base>/droid-tui`) from injected
/// environment values. `None` when no base resolves.
pub fn config_dir_from(
    xdg_config_home: Option<&OsStr>,
    home: Option<&OsStr>,
    appdata: Option<&OsStr>,
) -> Option<PathBuf> {
    Some(data_dir(xdg_config_home, home, appdata)?.join(APP_DIR_NAME))
}

/// Production wrapper: resolve the application directory from the process
/// environment (`$XDG_CONFIG_HOME`, `$HOME`, `%APPDATA%`).
pub fn config_dir() -> Option<PathBuf> {
    config_dir_from(
        env::var_os("XDG_CONFIG_HOME").as_deref(),
        env::var_os("HOME").as_deref(),
        env::var_os("APPDATA").as_deref(),
    )
}

/// Drive roots other than the one `current` sits on, for the file picker's
/// "switch drive" rows. Production wrapper around [`drive_roots_with`] that
/// probes the real filesystem; empty everywhere but Windows.
pub fn drive_roots(current: &Path) -> Vec<PathBuf> {
    drive_roots_with(current, Path::exists)
}

/// Pure seam for [`drive_roots`]: probe `A:` through `Z:` with `exists`,
/// skipping the drive that holds `current` (compared case-insensitively, so a
/// lower-case `c:\` still excludes `C:\`). Results are alphabetical by
/// construction. Always empty off Windows, regardless of `exists`, so the
/// picker listing is byte-identical on Linux and macOS.
pub fn drive_roots_with(current: &Path, exists: impl Fn(&Path) -> bool) -> Vec<PathBuf> {
    #[cfg(windows)]
    {
        let current_letter = path_drive_letter(current);
        ('A'..='Z')
            .filter_map(|letter| {
                if current_letter == Some(letter) {
                    return None;
                }
                let root = PathBuf::from(format!("{letter}:\\"));
                exists(&root).then_some(root)
            })
            .collect()
    }

    #[cfg(not(windows))]
    {
        let _ = (current, exists);
        Vec::new()
    }
}

/// Upper-case drive letter of an absolute Windows path (`C:\x` -> `C`,
/// `c:` -> `C`), or `None` when the path has no drive prefix.
#[cfg(windows)]
fn path_drive_letter(path: &Path) -> Option<char> {
    use std::path::{Component, Prefix};

    match path.components().next()? {
        Component::Prefix(prefix) => match prefix.kind() {
            // `VerbatimDisk` covers `\\?\C:\...`: the letter is not the first
            // character of the prefix, so pattern-matching the kind is the only
            // reliable way to read it.
            Prefix::Disk(letter) | Prefix::VerbatimDisk(letter) => {
                Some((letter as char).to_ascii_uppercase())
            }
            _ => None,
        },
        _ => None,
    }
}

/// `$HOME/.config`, or `None` when `home` is unset or empty.
fn home_config(home: Option<&OsStr>) -> Option<PathBuf> {
    Some(PathBuf::from(non_empty(home)?).join(".config"))
}

/// Treat an empty variable the same as an unset one.
fn non_empty(value: Option<&OsStr>) -> Option<&OsStr> {
    value.filter(|value| !value.is_empty())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn os(value: &str) -> Option<&OsStr> {
        Some(OsStr::new(value))
    }

    #[test]
    #[cfg(not(windows))]
    fn absolute_xdg_config_home_wins() {
        assert_eq!(
            data_dir(os("/custom/xdg"), os("/user-home"), None),
            Some(PathBuf::from("/custom/xdg"))
        );
    }

    #[test]
    #[cfg(not(windows))]
    fn relative_xdg_config_home_is_treated_as_unset() {
        assert_eq!(
            data_dir(os("relative/xdg"), os("/user-home"), None),
            Some(PathBuf::from("/user-home/.config"))
        );
    }

    #[test]
    #[cfg(not(windows))]
    fn empty_xdg_config_home_is_treated_as_unset() {
        assert_eq!(
            data_dir(os(""), os("/user-home"), None),
            Some(PathBuf::from("/user-home/.config"))
        );
    }

    #[test]
    #[cfg(not(windows))]
    fn missing_xdg_config_home_falls_back_to_home() {
        assert_eq!(
            data_dir(None, os("/user-home"), None),
            Some(PathBuf::from("/user-home/.config"))
        );
    }

    #[test]
    #[cfg(not(windows))]
    fn linux_and_macos_ignore_appdata() {
        assert_eq!(
            data_dir(None, os("/user-home"), os("/appdata/roaming")),
            Some(PathBuf::from("/user-home/.config"))
        );
        assert_eq!(
            data_dir(os("/custom/xdg"), os("/user-home"), os("/appdata/roaming")),
            Some(PathBuf::from("/custom/xdg"))
        );
    }

    #[test]
    fn nothing_set_resolves_to_none() {
        assert_eq!(data_dir(None, None, None), None);
        assert_eq!(config_dir_from(None, None, None), None);
    }

    #[test]
    #[cfg(not(windows))]
    fn empty_home_resolves_to_none() {
        assert_eq!(data_dir(None, os(""), os("")), None);
    }

    #[test]
    #[cfg(not(windows))]
    fn config_dir_appends_the_application_directory() {
        assert_eq!(
            config_dir_from(os("/custom/xdg"), None, None),
            Some(PathBuf::from("/custom/xdg/droid-tui"))
        );
        assert_eq!(
            config_dir_from(None, os("/user-home"), None),
            Some(PathBuf::from("/user-home/.config/droid-tui"))
        );
    }

    #[test]
    fn every_user_store_resolves_into_the_same_application_directory() {
        let xdg = Some(OsStr::new("/custom/xdg"));
        let home = Some(OsStr::new("/user-home"));
        let appdata = Some(OsStr::new("/appdata/roaming"));

        let app_dir = config_dir_from(xdg, home, appdata).expect("application directory");

        assert_eq!(
            crate::config::config_dir(xdg, home, appdata),
            Some(app_dir.clone())
        );
        assert_eq!(
            crate::plugin::plugin_dir_from(xdg, home, appdata),
            Some(app_dir.join("plugins"))
        );
        assert_eq!(
            crate::app::labels_file_path_from(xdg, home, appdata)
                .and_then(|path| path.parent().map(Path::to_path_buf)),
            Some(app_dir.clone())
        );
        assert_eq!(
            crate::favorites::favourites_file_path_from(xdg, home, appdata)
                .and_then(|path| path.parent().map(Path::to_path_buf)),
            Some(app_dir)
        );
    }

    #[test]
    fn every_user_store_degrades_to_none_together() {
        assert_eq!(config_dir_from(None, None, None), None);
        assert_eq!(crate::config::config_dir(None, None, None), None);
        assert_eq!(crate::plugin::plugin_dir_from(None, None, None), None);
        assert_eq!(crate::app::labels_file_path_from(None, None, None), None);
        assert_eq!(
            crate::favorites::favourites_file_path_from(None, None, None),
            None
        );
    }

    #[test]
    #[cfg(not(windows))]
    fn drive_roots_are_empty_off_windows_even_when_every_probe_hits() {
        assert!(drive_roots_with(Path::new("/user-home"), |_| true).is_empty());
        assert!(drive_roots(Path::new("/")).is_empty());
        assert!(drive_roots(Path::new("/user-home/droid-tui")).is_empty());
    }

    #[test]
    #[cfg(windows)]
    fn drive_roots_probes_a_to_z_and_skips_the_current_drive() {
        use std::collections::HashSet;

        let present: HashSet<PathBuf> =
            [r"A:\", r"C:\", r"D:\"].iter().map(PathBuf::from).collect();
        // Lower-case current drive must still exclude `C:\`.
        let found = drive_roots_with(Path::new(r"c:\Users\user"), |path| present.contains(path));
        assert_eq!(found, vec![PathBuf::from(r"A:\"), PathBuf::from(r"D:\")]);

        // A drive-relative `C:` (no root) names the same drive.
        let from_relative = drive_roots_with(Path::new(r"C:"), |path| present.contains(path));
        assert_eq!(
            from_relative,
            vec![PathBuf::from(r"A:\"), PathBuf::from(r"D:\")]
        );

        // A verbatim path carries the same drive letter; it must still exclude
        // the current drive.
        let from_verbatim = drive_roots_with(Path::new(r"\\?\C:\Users\user"), |path| {
            present.contains(path)
        });
        assert_eq!(
            from_verbatim,
            vec![PathBuf::from(r"A:\"), PathBuf::from(r"D:\")]
        );

        // Nothing present, nothing listed.
        assert!(drive_roots_with(Path::new(r"C:\"), |_| false).is_empty());
    }

    #[test]
    #[cfg(windows)]
    fn windows_ignores_a_relative_appdata() {
        assert_eq!(
            data_dir(None, os(r"C:\Users\user"), os("Roaming")),
            Some(PathBuf::from(r"C:\Users\user").join(".config"))
        );
    }

    #[test]
    #[cfg(windows)]
    fn windows_prefers_appdata_over_xdg_and_home() {
        assert_eq!(
            data_dir(
                os(r"C:\custom\xdg"),
                os(r"C:\Users\user"),
                os(r"C:\Users\user\AppData\Roaming")
            ),
            Some(PathBuf::from(r"C:\Users\user\AppData\Roaming"))
        );
    }

    #[test]
    #[cfg(windows)]
    fn windows_falls_back_to_home_when_appdata_is_unset_or_empty() {
        let expected = PathBuf::from(r"C:\Users\user").join(".config");
        assert_eq!(
            data_dir(None, os(r"C:\Users\user"), None),
            Some(expected.clone())
        );
        assert_eq!(data_dir(None, os(r"C:\Users\user"), os("")), Some(expected));
    }

    #[test]
    #[cfg(windows)]
    fn windows_without_any_environment_resolves_to_none() {
        assert_eq!(data_dir(None, None, None), None);
    }

    #[test]
    #[cfg(windows)]
    fn config_dir_appends_the_application_directory() {
        assert_eq!(
            config_dir_from(
                None,
                os(r"C:\Users\user"),
                os(r"C:\Users\user\AppData\Roaming")
            ),
            Some(PathBuf::from(r"C:\Users\user\AppData\Roaming").join("droid-tui"))
        );
        assert_eq!(
            config_dir_from(None, os(r"C:\Users\user"), None),
            Some(
                PathBuf::from(r"C:\Users\user")
                    .join(".config")
                    .join("droid-tui")
            )
        );
    }
}
