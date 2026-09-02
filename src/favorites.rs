use std::env;
use std::ffi::OsStr;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

const FAVOURITES_DIR_NAME: &str = "droid-tui";
const FAVOURITES_FILE_NAME: &str = "favourites.toml";

/// Persistent favourites list stored in `favourites.toml` under the XDG config home.
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

    /// Save to the discovered XDG path.
    pub fn save(&self) -> io::Result<()> {
        let dir = favourites_dir(
            env::var_os("XDG_CONFIG_HOME").as_deref(),
            env::var_os("HOME").as_deref(),
        )
        .ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::NotFound,
                "cannot determine config directory ($XDG_CONFIG_HOME or $HOME)",
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

fn favourites_dir(xdg_config_home: Option<&OsStr>, home: Option<&OsStr>) -> Option<PathBuf> {
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
    Some(base.join(FAVOURITES_DIR_NAME))
}

fn favourites_file_path() -> Option<PathBuf> {
    let dir = favourites_dir(
        env::var_os("XDG_CONFIG_HOME").as_deref(),
        env::var_os("HOME").as_deref(),
    )?;
    Some(dir.join(FAVOURITES_FILE_NAME))
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
    }

    #[test]
    fn save_round_trips() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("favourites.toml");
        let mut store = FavoritesStore::default();
        store.favourites.push("/tmp/a.ini".to_string());
        store.favourites.push("/tmp/b.ini".to_string());
        store.save_to(&path).unwrap();
        let reloaded = FavoritesStore::load_from(&path);
        assert_eq!(reloaded.favourites, store.favourites);
    }

    #[test]
    fn toggle_adds_and_removes() {
        let mut store = FavoritesStore::default();
        let p = Path::new("/tmp/test.ini");
        assert!(!store.is_favourite(p));
        assert!(store.toggle(p));
        assert!(store.is_favourite(p));
        assert!(!store.toggle(p));
        assert!(!store.is_favourite(p));
    }

    #[test]
    fn malformed_toml_yields_empty() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("favourites.toml");
        fs::write(&path, "not valid toml [[[ ").unwrap();
        let store = FavoritesStore::load_from(&path);
        assert!(store.favourites.is_empty());
    }

    #[test]
    fn save_to_dir_atomic() {
        let dir = tempdir().unwrap();
        let mut store = FavoritesStore::default();
        store.favourites.push("/abs/path.ini".to_string());
        store.save_to_dir(dir.path()).unwrap();
        let loaded = FavoritesStore::load_from(&dir.path().join("favourites.toml"));
        assert_eq!(loaded.favourites, vec!["/abs/path.ini".to_string()]);
    }

    #[test]
    fn favourites_dir_handles_non_absolute_xdg() {
        let dir = favourites_dir(
            Some(OsStr::new("relative/path")),
            Some(OsStr::new("/home/user")),
        );
        assert_eq!(dir, Some(PathBuf::from("/home/user/.config/droid-tui")));
    }

    #[test]
    fn favourites_dir_uses_xdg_when_absolute() {
        let dir = favourites_dir(
            Some(OsStr::new("/custom/config")),
            Some(OsStr::new("/home/user")),
        );
        assert_eq!(dir, Some(PathBuf::from("/custom/config/droid-tui")));
    }
}
