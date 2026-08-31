//! Persistent user preferences (`theme`, `[labels]`, `[latency]`, `[physical]`
//! + `[physical.rack]`, `[plugins]`) stored in `config.toml` under the XDG config home.
//!   Loaded once at startup, before the terminal UI initializes (design
//!   Decision 5).
//!
//! This module stays decoupled from the theme catalog: canonical name
//! resolution is injected as a function reference, so name-catalog
//! validation can be wired to `theme.rs` at the call site.

use std::collections::HashMap;
use std::env;
use std::ffi::OsStr;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

pub const DEFAULT_THEME: &str = "classic";
pub const DEFAULT_LAYERS_ENABLED: bool = true;
pub const DEFAULT_MAX_SHIFT_LAYER: u8 = 4;
pub const MIN_SHIFT_LAYER: u8 = 1;
pub const MAX_SHIFT_LAYER: u8 = 8;

/// Physical-view defaults, matching the `App::new` initial state so an
/// absent `[physical]` section keeps the out-of-box presentation.
pub const DEFAULT_PHYSICAL_SHOW_SKELETON: bool = false;
pub const DEFAULT_PHYSICAL_ZOOM: f64 = 1.0;
/// Pan-origin default (`physical_offset: (0.0, 0.0)` in `App`).
pub const DEFAULT_PHYSICAL_OFFSET: f64 = 0.0;
/// Zoom floor/ceiling — the `+`/`-` scale presets `[0.75, 1.0, 1.5, 2.0]`
/// (handler.rs), so a configured zoom always sits on the preset ladder.
pub const MIN_PHYSICAL_ZOOM: f64 = 0.75;
pub const MAX_PHYSICAL_ZOOM: f64 = 2.0;

/// Plugin-loading default under `[plugins]`: enabled by default, no directory
/// override (the standard XDG plugins dir applies).
pub const DEFAULT_PLUGINS_ENABLED: bool = true;

const CONFIG_DIR_NAME: &str = "droid-tui";
const CONFIG_FILE_NAME: &str = "config.toml";

fn default_theme() -> String {
    DEFAULT_THEME.to_string()
}

fn default_layers_enabled() -> bool {
    DEFAULT_LAYERS_ENABLED
}

fn default_max_shift_layer() -> u8 {
    DEFAULT_MAX_SHIFT_LAYER
}

fn default_show_skeleton() -> bool {
    DEFAULT_PHYSICAL_SHOW_SKELETON
}

fn default_physical_zoom() -> f64 {
    DEFAULT_PHYSICAL_ZOOM
}

fn default_physical_offset() -> f64 {
    DEFAULT_PHYSICAL_OFFSET
}

fn default_plugins_enabled() -> bool {
    DEFAULT_PLUGINS_ENABLED
}

/// Per-label configuration under `[labels]`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Labels {
    #[serde(default = "default_layers_enabled")]
    pub layers_enabled: bool,
    #[serde(default = "default_max_shift_layer")]
    pub max_shift_layer: u8,
}

impl Default for Labels {
    fn default() -> Self {
        Self {
            layers_enabled: default_layers_enabled(),
            max_shift_layer: default_max_shift_layer(),
        }
    }
}

/// Per-circuit latency overrides under `[latency]`.
///
/// Keys are DROID circuit names; values override the ramsize-proportional
/// per-circuit `AVG` in `latency::CostModel`. Empty or absent means the
/// heuristic default applies.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Latency {
    #[serde(default)]
    pub per_circuit: HashMap<String, f32>,
}

/// View defaults under `[physical]` plus the rack/case definition under
/// `[physical.rack]` (design D12). Field defaults mirror the physical-view
/// state `App::new` starts with (skeleton off, zoom 1.0, pan origin); an
/// absent table leaves the out-of-box view untouched. The rack reuses
/// `physical::RackSpec` directly (one source of truth for the schema); empty
/// `rack.rows` means "no rack configured" and packs the default single-row
/// case.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Physical {
    #[serde(default = "default_show_skeleton")]
    pub show_skeleton: bool,
    #[serde(default = "default_physical_zoom")]
    pub zoom: f64,
    /// Pan offset in screen cells; negative values reveal content above/left.
    #[serde(default = "default_physical_offset")]
    pub offset_x: f64,
    #[serde(default = "default_physical_offset")]
    pub offset_y: f64,
    #[serde(default)]
    pub rack: crate::physical::RackSpec,
}

impl Default for Physical {
    fn default() -> Self {
        Self {
            show_skeleton: default_show_skeleton(),
            zoom: default_physical_zoom(),
            offset_x: default_physical_offset(),
            offset_y: default_physical_offset(),
            rack: crate::physical::RackSpec::default(),
        }
    }
}

/// Plugin-loading configuration under `[plugins]`.
///
/// `dir` overrides the plugin directory; `None` selects the standard XDG
/// plugins dir resolved by `plugin::discover_plugins`
/// (`$XDG_CONFIG_HOME/droid-tui/plugins`). `enabled = false` disables plugin
/// loading entirely.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Plugins {
    /// Directory override, kept as written. Read via [`Plugins::plugins_dir`],
    /// which applies the XDG rule (non-absolute values treated as unset).
    #[serde(default)]
    pub dir: Option<PathBuf>,
    #[serde(default = "default_plugins_enabled")]
    pub enabled: bool,
}

impl Default for Plugins {
    fn default() -> Self {
        Self {
            dir: None,
            enabled: default_plugins_enabled(),
        }
    }
}

impl Plugins {
    /// Resolved directory override: the configured absolute `dir`, or `None`
    /// when absent (caller falls back to the standard XDG plugins dir). A
    /// non-absolute configured value is treated as unset, mirroring the
    /// `$XDG_CONFIG_HOME` rule in `config_dir`/`plugin::plugin_dir` (silent,
    /// no warning).
    pub fn plugins_dir(&self) -> Option<&Path> {
        self.dir.as_deref().filter(|path| path.is_absolute())
    }
}

/// v1 settings schema. Unknown keys in the file are ignored by serde
/// (forward-compatible with future versions). `Eq` is intentionally not
/// derived: `[latency] per_circuit` holds `f32`, which is not `Eq`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Settings {
    #[serde(default = "default_theme")]
    pub theme: String,
    #[serde(default)]
    pub labels: Labels,
    #[serde(default)]
    pub latency: Latency,
    #[serde(default)]
    pub physical: Physical,
    #[serde(default)]
    pub plugins: Plugins,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            theme: default_theme(),
            labels: Labels::default(),
            latency: Latency::default(),
            physical: Physical::default(),
            plugins: Plugins::default(),
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
    settings.labels.max_shift_layer = settings
        .labels
        .max_shift_layer
        .clamp(MIN_SHIFT_LAYER, MAX_SHIFT_LAYER);
    sanitize_physical(&mut settings.physical);
    settings
}

/// Clamp view/rack values to the model's valid domain on the way in: zoom to
/// the preset ladder, mount widths to non-negative, and row dimensions off
/// their degenerate floors (`RackSpec::default_case` also floors `he` at 1).
/// Row overrides are validated at pack time, so `assign` is left untouched.
fn sanitize_physical(physical: &mut Physical) {
    physical.zoom = physical.zoom.clamp(MIN_PHYSICAL_ZOOM, MAX_PHYSICAL_ZOOM);
    physical.rack.top_mount_te = physical.rack.top_mount_te.max(0.0);
    physical.rack.side_mount_te = physical.rack.side_mount_te.max(0.0);
    for row in &mut physical.rack.rows {
        row.he = row.he.max(1);
        row.hp = row.hp.max(1.0);
    }
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
    let mut normalized = settings.clone();
    normalized.labels.max_shift_layer = normalized
        .labels
        .max_shift_layer
        .clamp(MIN_SHIFT_LAYER, MAX_SHIFT_LAYER);
    sanitize_physical(&mut normalized.physical);
    let body = toml::to_string(&normalized)
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
                labels: Labels::default(),
                latency: Latency::default(),
                physical: Physical::default(),
                plugins: Plugins::default(),
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
                labels: Labels::default(),
                latency: Latency::default(),
                physical: Physical::default(),
                plugins: Plugins::default(),
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

    // ── label-management 5.1: [labels] clamp / disabled / round-trip ──

    #[test]
    fn labels_defaults_when_table_missing() {
        let dir = TempDir::new().unwrap();
        let cfg_dir = dir.path().join(CONFIG_DIR_NAME);
        std::fs::create_dir_all(&cfg_dir).unwrap();
        std::fs::write(cfg_dir.join(CONFIG_FILE_NAME), "theme = \"mono\"\n").unwrap();
        let loaded = load_at(&dir);
        assert!(loaded.labels.layers_enabled);
        assert_eq!(loaded.labels.max_shift_layer, 4);
        assert_eq!(loaded.theme, "mono");
    }

    #[test]
    fn labels_defaults_when_file_empty() {
        let dir = TempDir::new().unwrap();
        let cfg_dir = dir.path().join(CONFIG_DIR_NAME);
        std::fs::create_dir_all(&cfg_dir).unwrap();
        std::fs::write(cfg_dir.join(CONFIG_FILE_NAME), "").unwrap();
        let loaded = load_at(&dir);
        assert_eq!(loaded.labels, Labels::default());
    }

    #[test]
    fn max_shift_layer_clamped_on_load() {
        for (raw, expected) in [(0, 1), (1, 1), (4, 4), (8, 8), (20, 8), (255, 8)] {
            let dir = TempDir::new().unwrap();
            let cfg_dir = dir.path().join(CONFIG_DIR_NAME);
            std::fs::create_dir_all(&cfg_dir).unwrap();
            std::fs::write(
                cfg_dir.join(CONFIG_FILE_NAME),
                format!("theme = \"classic\"\n[labels]\nmax_shift_layer = {raw}\n"),
            )
            .unwrap();
            let loaded = load_at(&dir);
            assert_eq!(
                loaded.labels.max_shift_layer, expected,
                "raw {raw} should clamp to {expected}"
            );
        }
    }

    #[test]
    fn layers_enabled_round_trips_and_preserves_clamp() {
        let dir = TempDir::new().unwrap();
        save_to_dir(
            dir.path(),
            &Settings {
                theme: "classic".to_string(),
                labels: Labels {
                    layers_enabled: false,
                    max_shift_layer: 20,
                },
                latency: Latency::default(),
                physical: Physical::default(),
                plugins: Plugins::default(),
            },
        )
        .unwrap();
        let loaded = load_from(
            &dir.path().join(CONFIG_FILE_NAME),
            &test_canonical,
            &TEST_CATALOG,
        );
        assert!(!loaded.labels.layers_enabled);
        assert_eq!(loaded.labels.max_shift_layer, 8);
    }

    #[test]
    fn save_clamps_max_shift_layer_zero_and_large() {
        for (raw, expected) in [(0, 1), (255, 8)] {
            let dir = TempDir::new().unwrap();
            save_to_dir(
                dir.path(),
                &Settings {
                    theme: "classic".to_string(),
                    labels: Labels {
                        layers_enabled: true,
                        max_shift_layer: raw,
                    },
                    latency: Latency::default(),
                    physical: Physical::default(),
                    plugins: Plugins::default(),
                },
            )
            .unwrap();
            let loaded = load_from(
                &dir.path().join(CONFIG_FILE_NAME),
                &test_canonical,
                &TEST_CATALOG,
            );
            assert_eq!(loaded.labels.max_shift_layer, expected);
        }
    }

    #[test]
    fn labels_save_contains_table_and_round_trips() {
        let dir = TempDir::new().unwrap();
        let settings = Settings {
            theme: "terminal".to_string(),
            labels: Labels {
                layers_enabled: false,
                max_shift_layer: 6,
            },
            latency: Latency {
                per_circuit: HashMap::from([("clocktool".to_string(), 2.0)]),
            },
            physical: Physical::default(),
            plugins: Plugins::default(),
        };
        save_to_dir(dir.path(), &settings).unwrap();
        let body = std::fs::read_to_string(dir.path().join(CONFIG_FILE_NAME)).unwrap();
        assert!(body.contains("[labels]"), "body: {body}");
        assert!(body.contains("layers_enabled = false"));
        assert!(body.contains("max_shift_layer = 6"));
        let loaded = load_from(
            &dir.path().join(CONFIG_FILE_NAME),
            &test_canonical,
            &TEST_CATALOG,
        );
        assert!(!loaded.labels.layers_enabled);
        assert_eq!(loaded.labels.max_shift_layer, 6);
        assert_eq!(loaded.theme, "terminal");
    }

    #[test]
    fn unknown_labels_keys_ignored() {
        let dir = TempDir::new().unwrap();
        let cfg_dir = dir.path().join(CONFIG_DIR_NAME);
        std::fs::create_dir_all(&cfg_dir).unwrap();
        std::fs::write(
            cfg_dir.join(CONFIG_FILE_NAME),
            "theme = \"classic\"\n[labels]\nmax_shift_layer = 4\nfuture = 123\n",
        )
        .unwrap();
        let loaded = load_at(&dir);
        assert_eq!(loaded.labels.max_shift_layer, 4);
    }

    // ── latency-cost-model 1.1: [latency] per_circuit ──

    #[test]
    fn latency_per_circuit_parses_from_config() {
        let dir = TempDir::new().unwrap();
        let cfg_dir = dir.path().join(CONFIG_DIR_NAME);
        std::fs::create_dir_all(&cfg_dir).unwrap();
        std::fs::write(
            cfg_dir.join(CONFIG_FILE_NAME),
            "theme = \"classic\"\n[latency]\nper_circuit = { \"clocktool\" = 12.0, \"copy\" = 3.5 }\n",
        )
        .unwrap();
        let loaded = load_at(&dir);
        assert_eq!(loaded.latency.per_circuit.get("clocktool"), Some(&12.0));
        assert_eq!(loaded.latency.per_circuit.get("copy"), Some(&3.5));
    }

    #[test]
    fn latency_absent_defaults_to_empty() {
        let dir = TempDir::new().unwrap();
        let cfg_dir = dir.path().join(CONFIG_DIR_NAME);
        std::fs::create_dir_all(&cfg_dir).unwrap();
        std::fs::write(cfg_dir.join(CONFIG_FILE_NAME), "theme = \"mono\"\n").unwrap();
        let loaded = load_at(&dir);
        assert!(loaded.latency.per_circuit.is_empty());
    }

    #[test]
    fn latency_save_round_trips_through_load() {
        let dir = TempDir::new().unwrap();
        save_to_dir(
            dir.path(),
            &Settings {
                theme: "classic".to_string(),
                labels: Labels::default(),
                latency: Latency {
                    per_circuit: HashMap::from([("clocktool".to_string(), 2.5)]),
                },
                physical: Physical::default(),
                plugins: Plugins::default(),
            },
        )
        .unwrap();
        let body = std::fs::read_to_string(dir.path().join(CONFIG_FILE_NAME)).unwrap();
        assert!(body.contains("[latency.per_circuit]"), "body: {body}");
        let loaded = load_from(
            &dir.path().join(CONFIG_FILE_NAME),
            &test_canonical,
            &TEST_CATALOG,
        );
        assert_eq!(loaded.latency.per_circuit.get("clocktool"), Some(&2.5));
    }

    // ── physical-scale-model 4.4: [physical] + [physical.rack] ──

    #[test]
    fn physical_defaults_when_tables_missing() {
        let dir = TempDir::new().unwrap();
        let cfg_dir = dir.path().join(CONFIG_DIR_NAME);
        std::fs::create_dir_all(&cfg_dir).unwrap();
        std::fs::write(cfg_dir.join(CONFIG_FILE_NAME), "theme = \"mono\"\n").unwrap();
        let loaded = load_at(&dir);
        assert_eq!(loaded.physical, Physical::default());
        assert!(!loaded.physical.show_skeleton);
        assert_eq!(loaded.physical.zoom, 1.0);
        assert_eq!(loaded.physical.offset_x, 0.0);
        assert_eq!(loaded.physical.offset_y, 0.0);
        assert!(loaded.physical.rack.rows.is_empty());
        assert_eq!(loaded.physical.rack.top_mount_te, 0.0);
        assert_eq!(loaded.theme, "mono");
    }

    #[test]
    fn physical_table_parses() {
        let dir = TempDir::new().unwrap();
        let cfg_dir = dir.path().join(CONFIG_DIR_NAME);
        std::fs::create_dir_all(&cfg_dir).unwrap();
        std::fs::write(
            cfg_dir.join(CONFIG_FILE_NAME),
            "theme = \"classic\"\n[physical]\nshow_skeleton = true\nzoom = 1.5\noffset_x = 2.0\noffset_y = -8.0\n",
        )
        .unwrap();
        let loaded = load_at(&dir);
        assert!(loaded.physical.show_skeleton);
        assert_eq!(loaded.physical.zoom, 1.5);
        assert_eq!(loaded.physical.offset_x, 2.0);
        assert_eq!(loaded.physical.offset_y, -8.0);
    }

    #[test]
    fn physical_rack_table_parses() {
        let dir = TempDir::new().unwrap();
        let cfg_dir = dir.path().join(CONFIG_DIR_NAME);
        std::fs::create_dir_all(&cfg_dir).unwrap();
        std::fs::write(
            cfg_dir.join(CONFIG_FILE_NAME),
            "theme = \"classic\"\n[physical.rack]\ntop_mount_te = 4.0\nside_mount_te = 2.0\n\
             [[physical.rack.rows]]\nhe = 3\nhp = 84.0\nlabel = \"Main\"\n\
             [[physical.rack.rows]]\nhe = 1\nhp = 24.0\n\
             [physical.rack.assign]\n\"P2B8 1\" = 0\n\"CV I/O\" = 1\n",
        )
        .unwrap();
        let loaded = load_at(&dir);
        let rack = &loaded.physical.rack;
        assert_eq!(rack.rows.len(), 2);
        assert_eq!(rack.rows[0].he, 3);
        assert_eq!(rack.rows[0].hp, 84.0);
        assert_eq!(rack.rows[0].label.as_deref(), Some("Main"));
        assert_eq!(rack.rows[1].he, 1);
        assert_eq!(rack.rows[1].hp, 24.0);
        assert_eq!(rack.rows[1].label, None);
        assert_eq!(rack.top_mount_te, 4.0);
        assert_eq!(rack.side_mount_te, 2.0);
        assert_eq!(rack.assign.get("P2B8 1"), Some(&0));
        assert_eq!(rack.assign.get("CV I/O"), Some(&1));
    }

    #[test]
    fn physical_zoom_clamped_on_load() {
        for (raw, expected) in [
            (0.1, 0.75),
            (0.75, 0.75),
            (1.5, 1.5),
            (2.0, 2.0),
            (5.0, 2.0),
        ] {
            let dir = TempDir::new().unwrap();
            let cfg_dir = dir.path().join(CONFIG_DIR_NAME);
            std::fs::create_dir_all(&cfg_dir).unwrap();
            std::fs::write(
                cfg_dir.join(CONFIG_FILE_NAME),
                format!("theme = \"classic\"\n[physical]\nzoom = {raw}\n"),
            )
            .unwrap();
            let loaded = load_at(&dir);
            assert_eq!(
                loaded.physical.zoom, expected,
                "raw {raw} should clamp to {expected}"
            );
        }
    }

    #[test]
    fn physical_rack_sanitized_on_load() {
        let dir = TempDir::new().unwrap();
        let cfg_dir = dir.path().join(CONFIG_DIR_NAME);
        std::fs::create_dir_all(&cfg_dir).unwrap();
        std::fs::write(
            cfg_dir.join(CONFIG_FILE_NAME),
            "theme = \"classic\"\n[physical.rack]\ntop_mount_te = -4.0\nside_mount_te = -1.0\n\
             [[physical.rack.rows]]\nhe = 0\nhp = -5.0\n",
        )
        .unwrap();
        let loaded = load_at(&dir);
        assert_eq!(loaded.physical.rack.top_mount_te, 0.0);
        assert_eq!(loaded.physical.rack.side_mount_te, 0.0);
        assert_eq!(loaded.physical.rack.rows[0].he, 1);
        assert_eq!(loaded.physical.rack.rows[0].hp, 1.0);
    }

    #[test]
    fn malformed_physical_rack_falls_back_to_defaults() {
        let dir = TempDir::new().unwrap();
        let cfg_dir = dir.path().join(CONFIG_DIR_NAME);
        std::fs::create_dir_all(&cfg_dir).unwrap();
        std::fs::write(
            cfg_dir.join(CONFIG_FILE_NAME),
            "theme = \"classic\"\n[physical.rack]\ntop_mount_te = \"wide\"\n",
        )
        .unwrap();
        assert_eq!(load_at(&dir), Settings::default());
    }

    #[test]
    fn existing_config_without_physical_section_loads_unchanged() {
        let dir = TempDir::new().unwrap();
        let cfg_dir = dir.path().join(CONFIG_DIR_NAME);
        std::fs::create_dir_all(&cfg_dir).unwrap();
        std::fs::write(
            cfg_dir.join(CONFIG_FILE_NAME),
            "theme = \"terminal\"\n[labels]\nlayers_enabled = false\nmax_shift_layer = 6\n[latency]\nper_circuit = { \"clocktool\" = 2.5 }\n",
        )
        .unwrap();
        let loaded = load_at(&dir);
        assert_eq!(loaded.theme, "terminal");
        assert!(!loaded.labels.layers_enabled);
        assert_eq!(loaded.labels.max_shift_layer, 6);
        assert_eq!(loaded.latency.per_circuit.get("clocktool"), Some(&2.5));
        assert_eq!(loaded.physical, Physical::default());
    }

    #[test]
    fn physical_save_round_trips_through_load() {
        let dir = TempDir::new().unwrap();
        let settings = Settings {
            theme: "classic".to_string(),
            labels: Labels::default(),
            latency: Latency::default(),
            physical: Physical {
                show_skeleton: true,
                zoom: 1.5,
                offset_x: 2.0,
                offset_y: -8.0,
                rack: crate::physical::RackSpec {
                    rows: vec![crate::physical::RackRow {
                        he: 3,
                        hp: 84.0,
                        label: Some("Main".to_string()),
                    }],
                    top_mount_te: 4.0,
                    side_mount_te: 2.0,
                    assign: HashMap::from([("P2B8 1".to_string(), 0)]),
                },
            },
            plugins: Plugins::default(),
        };
        save_to_dir(dir.path(), &settings).unwrap();
        let body = std::fs::read_to_string(dir.path().join(CONFIG_FILE_NAME)).unwrap();
        assert!(body.contains("[physical]"), "body: {body}");
        assert!(body.contains("[physical.rack]"), "body: {body}");
        let loaded = load_from(
            &dir.path().join(CONFIG_FILE_NAME),
            &test_canonical,
            &TEST_CATALOG,
        );
        assert_eq!(loaded.physical, settings.physical);
    }

    #[test]
    fn save_sanitizes_physical_zoom_and_rack() {
        let dir = TempDir::new().unwrap();
        save_to_dir(
            dir.path(),
            &Settings {
                theme: "classic".to_string(),
                labels: Labels::default(),
                latency: Latency::default(),
                physical: Physical {
                    show_skeleton: false,
                    zoom: 5.0,
                    offset_x: 0.0,
                    offset_y: 0.0,
                    rack: crate::physical::RackSpec {
                        rows: vec![crate::physical::RackRow {
                            he: 0,
                            hp: -3.0,
                            label: None,
                        }],
                        top_mount_te: -1.0,
                        side_mount_te: 0.0,
                        assign: HashMap::new(),
                    },
                },
                plugins: Plugins::default(),
            },
        )
        .unwrap();
        let loaded = load_from(
            &dir.path().join(CONFIG_FILE_NAME),
            &test_canonical,
            &TEST_CATALOG,
        );
        assert_eq!(loaded.physical.zoom, 2.0);
        assert_eq!(loaded.physical.rack.top_mount_te, 0.0);
        assert_eq!(loaded.physical.rack.rows[0].he, 1);
        assert_eq!(loaded.physical.rack.rows[0].hp, 1.0);
    }

    // ── circuit-plugin-system 4.2: [plugins] dir / enabled ──

    #[test]
    fn plugins_defaults_when_table_missing() {
        let dir = TempDir::new().unwrap();
        let cfg_dir = dir.path().join(CONFIG_DIR_NAME);
        std::fs::create_dir_all(&cfg_dir).unwrap();
        std::fs::write(cfg_dir.join(CONFIG_FILE_NAME), "theme = \"mono\"\n").unwrap();
        let loaded = load_at(&dir);
        assert_eq!(loaded.plugins, Plugins::default());
        assert!(loaded.plugins.enabled);
        assert_eq!(loaded.plugins.plugins_dir(), None);
        assert_eq!(loaded.theme, "mono");
    }

    #[test]
    fn plugins_defaults_when_file_empty() {
        let dir = TempDir::new().unwrap();
        let cfg_dir = dir.path().join(CONFIG_DIR_NAME);
        std::fs::create_dir_all(&cfg_dir).unwrap();
        std::fs::write(cfg_dir.join(CONFIG_FILE_NAME), "").unwrap();
        let loaded = load_at(&dir);
        assert_eq!(loaded.plugins, Plugins::default());
    }

    #[test]
    fn plugins_enabled_false_parses() {
        let dir = TempDir::new().unwrap();
        let cfg_dir = dir.path().join(CONFIG_DIR_NAME);
        std::fs::create_dir_all(&cfg_dir).unwrap();
        std::fs::write(
            cfg_dir.join(CONFIG_FILE_NAME),
            "theme = \"classic\"\n[plugins]\nenabled = false\n",
        )
        .unwrap();
        let loaded = load_at(&dir);
        assert!(!loaded.plugins.enabled);
        assert_eq!(loaded.plugins.plugins_dir(), None);
    }

    #[test]
    fn plugins_dir_override_parses() {
        let dir = TempDir::new().unwrap();
        let cfg_dir = dir.path().join(CONFIG_DIR_NAME);
        std::fs::create_dir_all(&cfg_dir).unwrap();
        std::fs::write(
            cfg_dir.join(CONFIG_FILE_NAME),
            "theme = \"classic\"\n[plugins]\ndir = \"/some/dir\"\n",
        )
        .unwrap();
        let loaded = load_at(&dir);
        assert_eq!(loaded.plugins.plugins_dir(), Some(Path::new("/some/dir")));
        assert!(loaded.plugins.enabled);
    }

    #[test]
    fn plugins_relative_dir_treated_as_unset() {
        let dir = TempDir::new().unwrap();
        let cfg_dir = dir.path().join(CONFIG_DIR_NAME);
        std::fs::create_dir_all(&cfg_dir).unwrap();
        std::fs::write(
            cfg_dir.join(CONFIG_FILE_NAME),
            "theme = \"classic\"\n[plugins]\ndir = \"rel/path\"\n",
        )
        .unwrap();
        let loaded = load_at(&dir);
        // The raw value is preserved on the field; the getter applies the XDG
        // rule (non-absolute treated as unset, silently, like `config_dir`).
        assert_eq!(loaded.plugins.dir, Some(PathBuf::from("rel/path")));
        assert_eq!(loaded.plugins.plugins_dir(), None);
        assert!(loaded.plugins.enabled);
    }

    #[test]
    fn plugins_malformed_enabled_falls_back_to_defaults() {
        let dir = TempDir::new().unwrap();
        let cfg_dir = dir.path().join(CONFIG_DIR_NAME);
        std::fs::create_dir_all(&cfg_dir).unwrap();
        std::fs::write(
            cfg_dir.join(CONFIG_FILE_NAME),
            "theme = \"classic\"\n[plugins]\nenabled = \"yes\"\n",
        )
        .unwrap();
        let loaded = load_at(&dir);
        assert_eq!(loaded, Settings::default());
        assert_eq!(loaded.plugins, Plugins::default());
    }

    #[test]
    fn plugins_save_round_trips_through_load() {
        let dir = TempDir::new().unwrap();
        let settings = Settings {
            theme: "classic".to_string(),
            labels: Labels::default(),
            latency: Latency::default(),
            physical: Physical::default(),
            plugins: Plugins {
                dir: Some(PathBuf::from("/custom/plugins")),
                enabled: false,
            },
        };
        save_to_dir(dir.path(), &settings).unwrap();
        let body = std::fs::read_to_string(dir.path().join(CONFIG_FILE_NAME)).unwrap();
        assert!(body.contains("[plugins]"), "body: {body}");
        let loaded = load_from(
            &dir.path().join(CONFIG_FILE_NAME),
            &test_canonical,
            &TEST_CATALOG,
        );
        assert_eq!(loaded.plugins, settings.plugins);
    }
}
