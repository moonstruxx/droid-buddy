//! Persistent user preferences (v1 schema: a single `theme` key) stored in
//! `config.toml` under the XDG config home. Loaded once at startup, before
//! the terminal UI initializes (design Decision 5).
//!
//! This module stays decoupled from the theme catalog: canonical name
//! resolution is injected as a function reference, so name-catalog
//! validation can be wired to `theme.rs` at the call site.

use std::env;
use std::ffi::OsStr;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

pub const DEFAULT_THEME: &str = "classic";

const CONFIG_DIR_NAME: &str = "droid-tui";
const CONFIG_FILE_NAME: &str = "config.toml";

/// v1 settings schema. Unknown keys in the file are ignored by serde
/// (forward-compatible with future versions).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Settings {
    #[serde(default = "default_theme")]
    pub theme: String,
}

fn default_theme() -> String {
    DEFAULT_THEME.to_string()
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            theme: default_theme(),
        }
    }
}

/// Catalog hook injected by the caller: maps a raw theme string to its
/// canonical name, or `None` when the value is unknown.
pub type ThemeCanonicalizer<'a> = &'a dyn Fn(&str) -> Option<&'static str>;

/// Resolve the config directory from explicit environment values so tests
/// never touch process env. Per XDG spec, a non-absolute
/// `$XDG_CONFIG_HOME` is treated as unset.
fn config_dir(xdg_config_home: Option<&OsStr>, home: Option<&OsStr>) -> Option<PathBuf> {
    let base = match xdg_config_home {
        Some(xdg) if !xdg.is_empty() && Path::new(xdg).is_absolute() => PathBuf::from(xdg),
        _ => {
            let home = home?;
            if home.is_empty() {
                return None;
            }
            PathBuf::from(home).join(".config")
        }
    };
    Some(base.join(CONFIG_DIR_NAME))
}

/// Full path of the user's config file, or `None` when no home directory
/// can be determined.
fn config_file_path() -> Option<PathBuf> {
    let dir = config_dir(
        env::var_os("XDG_CONFIG_HOME").as_deref(),
        env::var_os("HOME").as_deref(),
    )?;
    Some(dir.join(CONFIG_FILE_NAME))
}

/// Load settings from the discovered user config path. Missing file or
/// directory silently yields defaults; malformed TOML and unknown theme
/// names warn once on stderr and yield defaults.
pub fn load(canonicalize: ThemeCanonicalizer<'_>, catalog: &[&str]) -> Settings {
    match config_file_path() {
        Some(path) => load_from(&path, canonicalize, catalog),
        None => Settings::default(),
    }
}

/// Load settings from an explicit file path (injection point for tests).
///
/// Warn-once contract: each call emits at most one stderr warning — the
/// loader runs exactly once per process at startup.
pub fn load_from(path: &Path, canonicalize: ThemeCanonicalizer<'_>, catalog: &[&str]) -> Settings {
    // Any read error (missing file/dir being the normal fresh-machine
    // case) falls back to defaults without ceremony.
    let raw = match fs::read_to_string(path) {
        Ok(raw) => raw,
        Err(_) => return Settings::default(),
    };
    let mut settings: Settings = match toml::from_str(&raw) {
        Ok(parsed) => parsed,
        Err(err) => {
            eprintln!(
                "warning: ignoring malformed config file {}: {err}",
                path.display()
            );
            return Settings::default();
        }
    };
    match canonicalize(&settings.theme) {
        Some(canonical) => settings.theme = canonical.to_string(),
        None => {
            eprintln!(
                "warning: unknown theme \"{}\" in {}; valid choices: {}",
                settings.theme,
                path.display(),
                catalog.join(", ")
            );
            settings.theme = DEFAULT_THEME.to_string();
        }
    }
    settings
}

/// Save settings to the discovered user config path.
pub fn save(settings: &Settings) -> io::Result<()> {
    let dir = config_dir(
        env::var_os("XDG_CONFIG_HOME").as_deref(),
        env::var_os("HOME").as_deref(),
    )
    .ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::NotFound,
            "cannot determine config directory ($XDG_CONFIG_HOME or $HOME)",
        )
    })?;
    save_to_dir(&dir, settings)
}

/// Atomically write `settings` as `config.toml` inside `dir`, creating the
/// directory tree on demand: serialize into a sibling temp file, then
/// rename over the target (atomic on POSIX, design Decision 6).
pub fn save_to_dir(dir: &Path, settings: &Settings) -> io::Result<()> {
    fs::create_dir_all(dir)?;
    let body = toml::to_string(settings)
        .map_err(|err| io::Error::new(io::ErrorKind::InvalidData, err.to_string()))?;
    let target = dir.join(CONFIG_FILE_NAME);
    let tmp = dir.join(format!("{CONFIG_FILE_NAME}.tmp"));
    fs::write(&tmp, body)?;
    if let Err(err) = fs::rename(&tmp, &target) {
        // Best-effort cleanup so a failed write leaves no stray temp file.
        let _ = fs::remove_file(&tmp);
        return Err(err);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    const TEST_CATALOG: [&str; 3] = ["classic", "terminal", "mono"];

    /// Stand-in for `theme.rs`'s canonical lookup: case-insensitive with
    /// `-`/`_`/space treated as equivalent separators.
    fn test_canonical(name: &str) -> Option<&'static str> {
        let normalized = name.to_lowercase().replace(['-', '_', ' '], "");
        TEST_CATALOG.iter().copied().find(|c| *c == normalized)
    }

    fn load_at(dir: &TempDir) -> Settings {
        load_from(
            &dir.path().join(CONFIG_DIR_NAME).join(CONFIG_FILE_NAME),
            &test_canonical,
            &TEST_CATALOG,
        )
    }

    #[test]
    fn missing_file_returns_defaults_silently() {
        let dir = TempDir::new().unwrap();
        assert_eq!(load_at(&dir), Settings::default());
    }

    #[test]
    fn missing_directory_returns_defaults_silently() {
        let dir = TempDir::new().unwrap();
        let deep_missing = dir.path().join("no").join("such").join("dir");
        assert_eq!(
            load_from(
                &deep_missing.join(CONFIG_FILE_NAME),
                &test_canonical,
                &TEST_CATALOG,
            ),
            Settings::default()
        );
    }

    #[test]
    fn xdg_config_home_override_honored() {
        let result = config_dir(Some(OsStr::new("/tmp/cfg")), Some(OsStr::new("/home/u")));
        assert_eq!(result, Some(PathBuf::from("/tmp/cfg/droid-tui")));
    }

    #[test]
    fn xdg_unset_falls_back_to_home_dot_config() {
        let result = config_dir(None, Some(OsStr::new("/home/u")));
        assert_eq!(result, Some(PathBuf::from("/home/u/.config/droid-tui")));
    }

    #[test]
    fn relative_xdg_value_treated_as_unset() {
        let result = config_dir(Some(OsStr::new("rel/path")), Some(OsStr::new("/home/u")));
        assert_eq!(result, Some(PathBuf::from("/home/u/.config/droid-tui")));
    }

    #[test]
    fn no_home_means_no_config_path() {
        assert_eq!(config_dir(None, None), None);
        assert_eq!(config_dir(Some(OsStr::new("")), None), None);
    }

    #[test]
    fn broken_toml_falls_back_to_defaults() {
        let dir = TempDir::new().unwrap();
        let cfg_dir = dir.path().join(CONFIG_DIR_NAME);
        std::fs::create_dir_all(&cfg_dir).unwrap();
        std::fs::write(cfg_dir.join(CONFIG_FILE_NAME), "theme = [not valid").unwrap();
        assert_eq!(load_at(&dir), Settings::default());
    }

    #[test]
    fn unknown_theme_name_falls_back_to_default() {
        let dir = TempDir::new().unwrap();
        let cfg_dir = dir.path().join(CONFIG_DIR_NAME);
        std::fs::create_dir_all(&cfg_dir).unwrap();
        std::fs::write(cfg_dir.join(CONFIG_FILE_NAME), "theme = \"bogus\"").unwrap();
        assert_eq!(load_at(&dir), Settings::default());
    }

    #[test]
    fn empty_theme_name_falls_back_to_default() {
        let dir = TempDir::new().unwrap();
        let cfg_dir = dir.path().join(CONFIG_DIR_NAME);
        std::fs::create_dir_all(&cfg_dir).unwrap();
        std::fs::write(cfg_dir.join(CONFIG_FILE_NAME), "theme = \"\"").unwrap();
        assert_eq!(load_at(&dir), Settings::default());
    }

    #[test]
    fn known_theme_canonicalized_case_insensitively() {
        let dir = TempDir::new().unwrap();
        let cfg_dir = dir.path().join(CONFIG_DIR_NAME);
        std::fs::create_dir_all(&cfg_dir).unwrap();
        std::fs::write(cfg_dir.join(CONFIG_FILE_NAME), "theme = \"TERMINAL\"").unwrap();
        assert_eq!(load_at(&dir).theme, "terminal");
    }

    #[test]
    fn unknown_keys_ignored_and_known_keys_still_apply() {
        let dir = TempDir::new().unwrap();
        let cfg_dir = dir.path().join(CONFIG_DIR_NAME);
        std::fs::create_dir_all(&cfg_dir).unwrap();
        std::fs::write(
            cfg_dir.join(CONFIG_FILE_NAME),
            "# future setting\nfuture_key = 42\ntheme = \"mono\"\n",
        )
        .unwrap();
        assert_eq!(load_at(&dir).theme, "mono");
    }

    #[test]
    fn missing_theme_key_defaults_without_warning_path() {
        let dir = TempDir::new().unwrap();
        let cfg_dir = dir.path().join(CONFIG_DIR_NAME);
        std::fs::create_dir_all(&cfg_dir).unwrap();
        std::fs::write(cfg_dir.join(CONFIG_FILE_NAME), "[other]\nkey = 1\n").unwrap();
        assert_eq!(load_at(&dir), Settings::default());
    }

    #[test]
    fn save_round_trips_through_load() {
        let dir = TempDir::new().unwrap();
        save_to_dir(
            dir.path(),
            &Settings {
                theme: "mono".to_string(),
            },
        )
        .unwrap();
        let loaded = load_from(
            &dir.path().join(CONFIG_FILE_NAME),
            &test_canonical,
            &TEST_CATALOG,
        );
        assert_eq!(loaded.theme, "mono");
    }

    #[test]
    fn save_creates_missing_directory_tree() {
        let dir = TempDir::new().unwrap();
        let nested = dir.path().join("a").join("b");
        save_to_dir(&nested, &Settings::default()).unwrap();
        assert!(nested.join(CONFIG_FILE_NAME).is_file());
    }

    #[test]
    fn save_replaces_existing_file_and_leaves_no_temp_file() {
        let dir = TempDir::new().unwrap();
        let target = dir.path().join(CONFIG_FILE_NAME);
        std::fs::write(&target, "theme = \"classic\"\nstale = true\n").unwrap();
        save_to_dir(
            dir.path(),
            &Settings {
                theme: "terminal".to_string(),
            },
        )
        .unwrap();
        let loaded = load_from(&target, &test_canonical, &TEST_CATALOG);
        assert_eq!(loaded.theme, "terminal");
        assert!(!dir.path().join(format!("{CONFIG_FILE_NAME}.tmp")).exists());
    }

    #[test]
    fn saved_output_is_valid_toml_containing_theme() {
        let dir = TempDir::new().unwrap();
        save_to_dir(dir.path(), &Settings::default()).unwrap();
        let body = std::fs::read_to_string(dir.path().join(CONFIG_FILE_NAME)).unwrap();
        assert!(body.contains("theme = \"classic\""));
    }
}
