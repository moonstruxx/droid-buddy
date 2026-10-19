//! Platform data-directory resolution, with the environment injected so tests
//! never touch process env (the seam `config::config_dir` and
//! `plugin::plugin_dir` already use).
//!
//! One rule serves config, labels, favourites and plugins:
//!
//! - Linux and macOS: an absolute, non-empty `$XDG_CONFIG_HOME`, else
//!   `$HOME/.config`. A non-absolute `$XDG_CONFIG_HOME` is treated as unset,
//!   per the XDG spec, and `%APPDATA%` is ignored.
//! - Windows: `%APPDATA%`; when it is unset or empty, fall back to
//!   `$HOME/.config` so a MSYS/Cygwin-style environment still works. Nothing
//!   else resolves there.
//!
//! Consumers take [`config_dir_from`] (injectable) or [`config_dir`]
//! (process environment) and receive the application directory itself.

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
        non_empty(appdata)
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
            data_dir(os("/custom/xdg"), os("/home/user"), None),
            Some(PathBuf::from("/custom/xdg"))
        );
    }

    #[test]
    #[cfg(not(windows))]
    fn relative_xdg_config_home_is_treated_as_unset() {
        assert_eq!(
            data_dir(os("relative/xdg"), os("/home/user"), None),
            Some(PathBuf::from("/home/user/.config"))
        );
    }

    #[test]
    #[cfg(not(windows))]
    fn empty_xdg_config_home_is_treated_as_unset() {
        assert_eq!(
            data_dir(os(""), os("/home/user"), None),
            Some(PathBuf::from("/home/user/.config"))
        );
    }

    #[test]
    #[cfg(not(windows))]
    fn missing_xdg_config_home_falls_back_to_home() {
        assert_eq!(
            data_dir(None, os("/home/user"), None),
            Some(PathBuf::from("/home/user/.config"))
        );
    }

    #[test]
    #[cfg(not(windows))]
    fn linux_and_macos_ignore_appdata() {
        assert_eq!(
            data_dir(None, os("/home/user"), os("/appdata/roaming")),
            Some(PathBuf::from("/home/user/.config"))
        );
        assert_eq!(
            data_dir(os("/custom/xdg"), os("/home/user"), os("/appdata/roaming")),
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
            config_dir_from(None, os("/home/user"), None),
            Some(PathBuf::from("/home/user/.config/droid-tui"))
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
