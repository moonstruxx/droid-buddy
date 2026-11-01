//! Shared on-disk rules for the user TOML stores — `labels.toml`,
//! `favourites.toml`, and any future store beside them.
//!
//! One place decides what a *read* does with a missing file (default), an
//! unreadable file (default) and malformed TOML (one stderr warning, default),
//! and how a *write* lands (serialize, write beside the target, rename). The
//! stores keep their own schema and their own pruning; only the mechanics are
//! shared, because those were copied verbatim per store and had already started
//! to drift (`LabelStore::save_to` skips the pruning `save_to_dir` applies).

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use serde::de::DeserializeOwned;

/// Canonical absolute key for a store entry: the real canonical path when the
/// file exists, otherwise an absolute join against the current directory (so a
/// path that does not exist yet still gets a stable, comparable key).
pub fn canonical_key(path: &Path) -> String {
    if let Ok(canonical) = path.canonicalize() {
        return canonical.to_string_lossy().to_string();
    }
    let absolute = if path.is_absolute() {
        path.to_path_buf()
    } else {
        std::env::current_dir()
            .unwrap_or_else(|_| PathBuf::from("."))
            .join(path)
    };
    absolute.to_string_lossy().to_string()
}

/// Read `path` as TOML. A missing or unreadable file yields `T::default()`;
/// malformed TOML warns once on stderr (naming `what`, e.g. `"labels"`) and also
/// yields the default, so a corrupt store never blocks the app.
pub fn load_toml<T: DeserializeOwned + Default>(path: &Path, what: &str) -> T {
    let Ok(raw) = fs::read_to_string(path) else {
        return T::default();
    };
    match toml::from_str(&raw) {
        Ok(parsed) => parsed,
        Err(err) => {
            eprintln!(
                "warning: ignoring malformed {what} file {}: {err}",
                path.display()
            );
            T::default()
        }
    }
}

/// The platform data directory, or the error a `save` reports when it cannot be
/// determined (a read falls back to defaults; a save must never guess a path).
pub fn data_dir_or_error() -> io::Result<PathBuf> {
    crate::platform::config_dir().ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::NotFound,
            "cannot determine the platform data directory (XDG_CONFIG_HOME, HOME or APPDATA)",
        )
    })
}

/// Serialize `value` as TOML, reporting a failure the way every store's `save`
/// has: `InvalidData`.
pub fn to_toml_string<T: serde::Serialize>(value: &T) -> io::Result<String> {
    toml::to_string(value)
        .map_err(|err| io::Error::new(io::ErrorKind::InvalidData, err.to_string()))
}

/// Write `body` as `file_name` inside `dir` (tmp → rename in the same
/// directory), creating `dir` on demand. The rename is what makes a concurrent
/// read see either the old file or the new one, never a half-written one.
pub fn write_in_dir(dir: &Path, file_name: &str, body: &str) -> io::Result<()> {
    fs::create_dir_all(dir)?;
    let tmp = dir.join(format!("{file_name}.tmp"));
    write_and_rename(&tmp, &dir.join(file_name), body)
}

/// Write `body` atomically beside an explicit `path` (`<stem>.toml.tmp`).
pub fn write_beside_file(path: &Path, body: &str) -> io::Result<()> {
    if let Some(dir) = path.parent() {
        if !dir.as_os_str().is_empty() {
            fs::create_dir_all(dir)?;
        }
    }
    let tmp = path.with_extension("toml.tmp");
    write_and_rename(&tmp, path, body)
}

fn write_and_rename(tmp: &Path, target: &Path, body: &str) -> io::Result<()> {
    fs::write(tmp, body)?;
    if let Err(err) = fs::rename(tmp, target) {
        let _ = fs::remove_file(tmp);
        return Err(err);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde::Deserialize;
    use serde::Serialize;
    use tempfile::TempDir;

    #[derive(Debug, Default, PartialEq, Serialize, Deserialize)]
    struct Sample {
        entries: Vec<String>,
    }

    #[test]
    fn missing_file_reads_as_default() {
        let dir = TempDir::new().unwrap();
        let loaded: Sample = load_toml(&dir.path().join("absent.toml"), "sample");
        assert_eq!(loaded, Sample::default());
    }

    #[test]
    fn malformed_toml_reads_as_default() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("broken.toml");
        fs::write(&path, "entries = [ not toml").unwrap();
        let loaded: Sample = load_toml(&path, "sample");
        assert_eq!(loaded, Sample::default(), "a corrupt store never blocks");
    }

    #[test]
    fn write_in_dir_round_trips_and_leaves_no_tmp() {
        let dir = TempDir::new().unwrap();
        let nested = dir.path().join("a/b");
        let sample = Sample {
            entries: vec!["x".into(), "y".into()],
        };
        let body = to_toml_string(&sample).unwrap();
        write_in_dir(&nested, "sample.toml", &body).unwrap();
        assert!(!nested.join("sample.toml.tmp").exists());
        let loaded: Sample = load_toml(&nested.join("sample.toml"), "sample");
        assert_eq!(loaded, sample);
    }

    #[test]
    fn write_beside_file_round_trips_and_overwrites() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("custom-name.data");
        let first = Sample {
            entries: vec!["one".into()],
        };
        write_beside_file(&path, &to_toml_string(&first).unwrap()).unwrap();
        let second = Sample {
            entries: vec!["two".into()],
        };
        write_beside_file(&path, &to_toml_string(&second).unwrap()).unwrap();
        assert_eq!(load_toml::<Sample>(&path, "sample"), second);
        assert!(!path.with_extension("toml.tmp").exists());
    }
}
