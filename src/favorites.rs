use std::env;
#[cfg(test)]
use std::ffi::OsStr;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

const FAVOURITES_FILE_NAME: &str = "favourites.toml";

/// Persistent favourites list stored in `favourites.toml` under the platform
/// data directory (`platform::config_dir`).
///
/// File shape:
/// ```toml
/// favourites = ["/abs/path/a.ini", "/abs/path/b.ini"]
/// ```
/// Paths are stored as absolute strings canonicalized when the file exists,
/// otherwise absolute-joined against `current_dir` (mirrors `LabelStore::canonical_key`).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct FavoritesStore {
    #[serde(default)]
    pub favourites: Vec<String>,
}

impl FavoritesStore {
    /// Canonicalize `path` to an absolute string key: real canonical path when
    /// the file exists, otherwise an absolute join against `current_dir`.
    pub fn canonical_key(path: &Path) -> String {
        if let Ok(canonical) = path.canonicalize() {
            return canonical.to_string_lossy().to_string();
        }
        let absolute = if path.is_absolute() {
            path.to_path_buf()
        } else {
            env::current_dir()
                .unwrap_or_else(|_| PathBuf::from("."))
                .join(path)
        };
        absolute.to_string_lossy().to_string()
    }

    /// Whether `path` is favourited (canonicalized comparison).
    pub fn is_favourite(&self, path: &Path) -> bool {
        let key = Self::canonical_key(path);
        self.favourites.iter().any(|f| f == &key)
    }

    /// Toggle favourite status for `path`. Returns true if now favourited, false if removed.
    pub fn toggle(&mut self, path: &Path) -> bool {
        let key = Self::canonical_key(path);
        if let Some(pos) = self.favourites.iter().position(|f| f == &key) {
            self.favourites.remove(pos);
            false
        } else {
            self.favourites.push(key);
            true
        }
    }

    /// Load the XDG store. Missing file yields empty store; malformed TOML
    /// warns once on stderr and yields empty store (mirrors `LabelStore`/`config.rs`).
    pub fn load() -> Self {
        match favourites_file_path() {
            Some(path) => Self::load_from(&path),
            None => Self::default(),
        }
    }

    /// Load from an explicit file path (test injection point). Warn-once
    /// contract: each call emits at most one stderr warning.
    pub fn load_from(path: &Path) -> Self {
        let raw = match fs::read_to_string(path) {
            Ok(raw) => raw,
            Err(_) => return Self::default(),
        };
        match toml::from_str(&raw) {
            Ok(parsed) => parsed,
            Err(err) => {
                eprintln!(
                    "warning: ignoring malformed favourites file {}: {err}",
                    path.display()
                );
                Self::default()
            }
        }
    }

    /// Save to the discovered platform path.
    pub fn save(&self) -> io::Result<()> {
        let dir = crate::platform::config_dir().ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::NotFound,
                "cannot determine the platform data directory (XDG_CONFIG_HOME, HOME or APPDATA)",
            )
        })?;
        self.save_to_dir(&dir)
    }

    /// Atomically write as `favourites.toml` inside `dir` (tmp→rename), creating
    /// the directory tree on demand.
    pub fn save_to_dir(&self, dir: &Path) -> io::Result<()> {
        fs::create_dir_all(dir)?;
        let body = toml::to_string(self)
            .map_err(|err| io::Error::new(io::ErrorKind::InvalidData, err.to_string()))?;
        let target = dir.join(FAVOURITES_FILE_NAME);
        let tmp = dir.join(format!("{FAVOURITES_FILE_NAME}.tmp"));
        fs::write(&tmp, body)?;
        if let Err(err) = fs::rename(&tmp, &target) {
            let _ = fs::remove_file(&tmp);
            return Err(err);
        }
        Ok(())
    }

    /// Save to an explicit file path (atomic tmp→rename beside the file).
    pub fn save_to(&self, path: &Path) -> io::Result<()> {
        if let Some(dir) = path.parent() {
            if !dir.as_os_str().is_empty() {
                fs::create_dir_all(dir)?;
            }
            if path.file_name().is_some_and(|n| n == FAVOURITES_FILE_NAME) {
                return self.save_to_dir(dir);
            }
        }
        let body = toml::to_string(self)
            .map_err(|err| io::Error::new(io::ErrorKind::InvalidData, err.to_string()))?;
        let tmp = path.with_extension("toml.tmp");
        fs::write(&tmp, body)?;
        if let Err(err) = fs::rename(&tmp, path) {
            let _ = fs::remove_file(&tmp);
            return Err(err);
        }
        Ok(())
    }
}

fn favourites_file_path() -> Option<PathBuf> {
    Some(crate::platform::config_dir()?.join(FAVOURITES_FILE_NAME))
}

/// Resolve the favourites file from injected environment values. The
/// directory rule lives in [`crate::platform::config_dir_from`].
#[cfg(test)]
pub(crate) fn favourites_file_path_from(
    xdg_config_home: Option<&OsStr>,
    home: Option<&OsStr>,
    appdata: Option<&OsStr>,
) -> Option<PathBuf> {
    Some(
        crate::platform::config_dir_from(xdg_config_home, home, appdata)?
            .join(FAVOURITES_FILE_NAME),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::tempdir;

    #[test]
    fn load_missing_file_yields_empty() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("favourites.toml");
        let store = FavoritesStore::load_from(&path);
        assert!(store.favourites.is_empty());
        // Missing directory also yields empty (no panic, no file).
        let missing_dir = dir.path().join("no").join("such").join("favourites.toml");
        assert!(FavoritesStore::load_from(&missing_dir)
            .favourites
            .is_empty());
    }

    #[test]
    fn save_round_trips() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("favourites.toml");
        let mut store = FavoritesStore::default();
        store
            .favourites
            .push(dir.path().join("a.ini").to_string_lossy().into_owned());
        store
            .favourites
            .push(dir.path().join("b.ini").to_string_lossy().into_owned());
        store.save_to(&path).unwrap();
        // No stray tmp file left beside the target.
        assert!(!dir.path().join("favourites.toml.tmp").exists());
        let reloaded = FavoritesStore::load_from(&path);
        assert_eq!(reloaded.favourites, store.favourites);
        // Verify the file is valid TOML containing the favourites array.
        let body = fs::read_to_string(&path).unwrap();
        assert!(body.contains("favourites"), "body: {body}");
    }

    #[test]
    fn toggle_adds_and_removes() {
        let mut store = FavoritesStore::default();
        let p = Path::new("test.ini");
        assert!(!store.is_favourite(p));
        assert!(store.toggle(p));
        assert!(store.is_favourite(p));
        assert_eq!(store.favourites.len(), 1);
        assert!(!store.toggle(p));
        assert!(!store.is_favourite(p));
        assert!(store.favourites.is_empty());
    }

    #[test]
    fn toggle_persists_through_save_round_trip() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("favourites.toml");
        let mut store = FavoritesStore::default();
        let p = Path::new("persist.ini");
        store.toggle(p);
        store.save_to(&path).unwrap();
        let reloaded = FavoritesStore::load_from(&path);
        assert!(reloaded.is_favourite(p));
        let mut reloaded = reloaded;
        reloaded.toggle(p);
        assert!(!reloaded.is_favourite(p));
    }

    #[test]
    fn malformed_toml_yields_empty() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("favourites.toml");
        fs::write(&path, "not valid toml [[[ ").unwrap();
        // load_from warns once on stderr and yields empty (mirrors LabelStore/config.rs).
        // Run with --nocapture to see: "warning: ignoring malformed favourites file ..."
        let store = FavoritesStore::load_from(&path);
        assert!(store.favourites.is_empty());
        // Confirm the content indeed fails TOML parsing (proves warn path was taken).
        assert!(toml::from_str::<FavoritesStore>("not valid toml [[[ ").is_err());
    }

    #[test]
    fn malformed_toml_yields_empty_with_empty_file_and_wrong_type() {
        let dir = tempdir().unwrap();
        // Empty file is valid TOML and yields empty store without warning.
        let empty = dir.path().join("empty.toml");
        fs::write(&empty, "").unwrap();
        assert!(FavoritesStore::load_from(&empty).favourites.is_empty());
        // Wrong-type value also triggers the malformed warning path.
        let wrong = dir.path().join("wrong.toml");
        fs::write(&wrong, "favourites = 123").unwrap();
        assert!(FavoritesStore::load_from(&wrong).favourites.is_empty());
    }

    #[test]
    fn save_to_dir_atomic() {
        let dir = tempdir().unwrap();
        let favourite = dir.path().join("abs.ini").to_string_lossy().into_owned();
        let mut store = FavoritesStore::default();
        store.favourites.push(favourite.clone());
        store.save_to_dir(dir.path()).unwrap();
        assert!(!dir.path().join("favourites.toml.tmp").exists());
        let loaded = FavoritesStore::load_from(&dir.path().join("favourites.toml"));
        assert_eq!(loaded.favourites, vec![favourite]);
    }

    #[test]
    #[cfg(not(windows))]
    fn favourites_file_path_handles_non_absolute_xdg() {
        let dir = favourites_file_path_from(
            Some(OsStr::new("relative/path")),
            Some(OsStr::new("/user-home")),
            None,
        );
        assert_eq!(
            dir,
            Some(PathBuf::from(
                "/user-home/.config/droid-tui/favourites.toml"
            ))
        );
    }

    #[test]
    #[cfg(not(windows))]
    fn favourites_file_path_uses_xdg_when_absolute() {
        let dir = favourites_file_path_from(
            Some(OsStr::new("/custom/config")),
            Some(OsStr::new("/user-home")),
            None,
        );
        assert_eq!(
            dir,
            Some(PathBuf::from("/custom/config/droid-tui/favourites.toml"))
        );
    }

    #[test]
    #[cfg(windows)]
    fn favourites_file_path_uses_appdata() {
        let dir = favourites_file_path_from(
            Some(OsStr::new(r"C:\custom\xdg")),
            Some(OsStr::new(r"C:\Users\user")),
            Some(OsStr::new(r"C:\Users\user\AppData\Roaming")),
        );
        assert_eq!(
            dir,
            Some(
                PathBuf::from(r"C:\Users\user\AppData\Roaming")
                    .join("droid-tui")
                    .join("favourites.toml")
            )
        );
    }

    #[test]
    #[cfg(windows)]
    fn favourites_file_path_falls_back_to_home_when_appdata_unset() {
        let dir = favourites_file_path_from(
            Some(OsStr::new(r"C:\custom\xdg")),
            Some(OsStr::new(r"C:\Users\user")),
            None,
        );
        assert_eq!(
            dir,
            Some(
                PathBuf::from(r"C:\Users\user")
                    .join(".config")
                    .join("droid-tui")
                    .join("favourites.toml")
            )
        );
    }
}
