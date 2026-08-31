//! Plugin-file discovery and TOML parsing for user-defined DROID circuits.
//!
//! Plugin files extend the embedded circuit schema (see `schema.rs`) with
//! user-defined circuits. They live in `$XDG_CONFIG_HOME/droid-tui/plugins/`
//! (`$HOME/.config/droid-tui/plugins/` when XDG is unset), one `[[circuit]]`
//! table per circuit. A file that fails to parse, or that defines a circuit
//! without the required `ramsize`, is skipped with a single stderr warning —
//! startup never aborts because of a plugin, and one bad file never affects
//! the others. Circuit names are case-insensitive, matching the embedded
//! schema lookup (`load_schema` keys by lowercased name), so the loader
//! lowercases them here.
//!
//! This module only discovers and parses. Merging into `Schema` (insert-or-
//! override on collision) is a separate concern in `schema.rs`.

use std::ffi::OsStr;
use std::fs;
use std::path::{Path, PathBuf};

use serde::Deserialize;

const CONFIG_DIR_NAME: &str = "droid-tui";
const PLUGIN_DIR_NAME: &str = "plugins";

/// One `[[circuit]]` table as parsed from TOML, before semantic validation.
/// `ramsize` is `Option` so a missing value is a *semantic* error (the file
/// parses fine as TOML) handled in `parse_circuits`, not a serde failure.
#[derive(Debug, Clone, Deserialize)]
struct RawPluginCircuit {
    name: String,
    category: String,
    ramsize: Option<u32>,
    title: Option<String>,
    description: Option<String>,
    cable_kind: Option<String>,
    color: Option<String>,
    #[serde(default)]
    inputs: Vec<RawPluginParam>,
    #[serde(default)]
    outputs: Vec<RawPluginParam>,
}

/// One param table inside `[[circuit.inputs]]` / `[[circuit.outputs]]`.
#[derive(Debug, Clone, Deserialize)]
struct RawPluginParam {
    name: String,
    short: String,
    #[serde(rename = "type")]
    param_type: String,
    default: Option<String>,
    prefix: Option<String>,
    count: Option<u32>,
    start_at: Option<u32>,
}

/// Top-level shape of a plugin TOML file: one `[[circuit]]` table array.
#[derive(Debug, Clone, Deserialize)]
struct PluginDocument {
    #[serde(rename = "circuit")]
    circuits: Vec<RawPluginCircuit>,
}

/// One validated plugin circuit, ready to merge into the schema. Neutral
/// defaults (`essential`, `ramhint`, `autotitle`, `presets`, `manual`) are
/// applied by the merge in `schema.rs`, not here.
#[derive(Debug, Clone)]
pub struct PluginCircuit {
    /// Lowercased circuit name, the schema's key convention.
    pub name: String,
    pub category: String,
    pub ramsize: u32,
    /// Defaults to the declared name (original case) when absent.
    pub title: String,
    /// Defaults to "" when absent.
    pub description: String,
    /// Declared rendering metadata; `None` defers to substring inference.
    pub cable_kind: Option<String>,
    pub color: Option<String>,
    pub inputs: Vec<PluginParam>,
    pub outputs: Vec<PluginParam>,
}

/// One parameter of a plugin circuit.
#[derive(Debug, Clone)]
pub struct PluginParam {
    pub name: String,
    pub short: String,
    pub param_type: String,
    pub default: Option<String>,
    pub prefix: Option<String>,
    pub count: Option<u32>,
    pub start_at: Option<u32>,
}

/// One successfully parsed plugin file. The caller (schema merge) iterates
/// these in order and overrides on name collision, so later files win.
#[derive(Debug, Clone)]
pub struct PluginFile {
    pub path: PathBuf,
    pub circuits: Vec<PluginCircuit>,
}

/// Resolve the plugin directory from explicit environment values so tests
/// never touch process env. Mirrors `config::config_dir`: a non-absolute
/// `$XDG_CONFIG_HOME` is treated as unset, with `$HOME/.config` fallback.
fn plugin_dir(xdg_config_home: Option<&OsStr>, home: Option<&OsStr>) -> Option<PathBuf> {
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
    Some(base.join(CONFIG_DIR_NAME).join(PLUGIN_DIR_NAME))
}

/// Discover and parse every `*.toml` plugin file, in sorted-filename order.
///
/// A missing plugins directory yields an empty list without a warning. A file
/// that cannot be read, fails to parse as TOML, or defines a circuit without
/// the required `ramsize` is skipped with one stderr warning and never
/// affects the remaining files.
pub fn discover_plugins(xdg_config_home: Option<&OsStr>, home: Option<&OsStr>) -> Vec<PluginFile> {
    let Some(dir) = plugin_dir(xdg_config_home, home) else {
        return Vec::new();
    };
    discover_plugins_from_dir(&dir)
}

/// Discover and parse every `*.toml` plugin file under an explicit directory,
/// in sorted-filename order, with the same skip/warn semantics as
/// `discover_plugins`. This is the `[plugins].dir` override entry point
/// (`schema::init` loads from here when the config pins a directory).
pub fn discover_plugins_from_dir(dir: &Path) -> Vec<PluginFile> {
    let Ok(entries) = fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut paths: Vec<PathBuf> = entries
        .filter_map(|entry| entry.ok())
        .map(|entry| entry.path())
        .filter(|path| path.extension() == Some(OsStr::new("toml")))
        .collect();
    paths.sort();
    paths.iter().filter_map(|path| load_file(path)).collect()
}

/// Load one plugin file. Returns `None` (with a warn-once stderr notice)
/// when the file is unreadable, malformed TOML, or semantically invalid.
/// `pub(crate)` so schema.rs tests can load the real fixtures.
pub(crate) fn load_file(path: &Path) -> Option<PluginFile> {
    let raw = match fs::read_to_string(path) {
        Ok(raw) => raw,
        Err(err) => {
            eprintln!(
                "warning: skipping unreadable plugin file {}: {err}",
                path.display()
            );
            return None;
        }
    };
    let doc: PluginDocument = match toml::from_str(&raw) {
        Ok(doc) => doc,
        Err(err) => {
            eprintln!(
                "warning: skipping malformed plugin file {}: {err}",
                path.display()
            );
            return None;
        }
    };
    match parse_circuits(&doc) {
        Ok(circuits) => Some(PluginFile {
            path: path.to_path_buf(),
            circuits,
        }),
        Err(circuit) => {
            eprintln!(
                "warning: skipping plugin file {}: circuit {circuit} is missing the required `ramsize`",
                path.display()
            );
            None
        }
    }
}

/// Semantic validation + defaults. A circuit missing `ramsize` rejects the
/// whole file (its name is the error payload for the warn-once message).
fn parse_circuits(doc: &PluginDocument) -> Result<Vec<PluginCircuit>, String> {
    let mut circuits = Vec::with_capacity(doc.circuits.len());
    for raw in &doc.circuits {
        let Some(ramsize) = raw.ramsize else {
            return Err(raw.name.clone());
        };
        circuits.push(PluginCircuit {
            name: raw.name.to_lowercase(),
            category: raw.category.clone(),
            ramsize,
            title: raw.title.clone().unwrap_or_else(|| raw.name.clone()),
            description: raw.description.clone().unwrap_or_default(),
            cable_kind: raw.cable_kind.clone(),
            color: raw.color.clone(),
            inputs: raw.inputs.iter().map(raw_param).collect(),
            outputs: raw.outputs.iter().map(raw_param).collect(),
        });
    }
    Ok(circuits)
}

fn raw_param(p: &RawPluginParam) -> PluginParam {
    PluginParam {
        name: p.name.clone(),
        short: p.short.clone(),
        param_type: p.param_type.clone(),
        default: p.default.clone(),
        prefix: p.prefix.clone(),
        count: p.count,
        start_at: p.start_at,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const FIXTURES: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/fixtures/plugins");

    fn fixture_path(name: &str) -> PathBuf {
        Path::new(FIXTURES).join(name)
    }

    #[test]
    fn plugin_dir_uses_xdg_when_absolute() {
        assert_eq!(
            plugin_dir(Some(OsStr::new("/tmp/cfg")), None),
            Some(PathBuf::from("/tmp/cfg/droid-tui/plugins"))
        );
    }

    #[test]
    fn plugin_dir_falls_back_to_home_dot_config() {
        assert_eq!(
            plugin_dir(None, Some(OsStr::new("/home/u"))),
            Some(PathBuf::from("/home/u/.config/droid-tui/plugins"))
        );
    }

    #[test]
    fn plugin_dir_relative_xdg_treated_as_unset() {
        assert_eq!(
            plugin_dir(Some(OsStr::new("cfg")), Some(OsStr::new("/home/u"))),
            Some(PathBuf::from("/home/u/.config/droid-tui/plugins"))
        );
    }

    #[test]
    fn plugin_dir_no_home_returns_none() {
        assert_eq!(plugin_dir(None, None), None);
    }

    #[test]
    fn valid_fixture_parses_two_circuits_lowercased() {
        let raw = fs::read_to_string(fixture_path("valid.toml")).expect("read fixture");
        let doc: PluginDocument = toml::from_str(&raw).expect("valid TOML");
        let circuits = parse_circuits(&doc).expect("valid semantics");

        assert_eq!(circuits.len(), 2);
        assert_eq!(circuits[0].name, "newckt");
        assert_eq!(circuits[0].ramsize, 256);
        assert_eq!(circuits[0].title, "A new circuit");
        assert_eq!(
            circuits[0].description,
            "Fixture circuit: converts an analog input into bit gates."
        );
        // NEWCKT declares no rendering metadata — the inference fallback path.
        assert!(circuits[0].cable_kind.is_none());
        assert!(circuits[0].color.is_none());
        // jack input carries no default; the cv input does.
        assert!(circuits[0].inputs[0].default.is_none());
        assert_eq!(circuits[0].inputs[1].default.as_deref(), Some("0"));
        // prefix/count/start_at expansion trio survives intact.
        let bit = &circuits[0].outputs[0];
        assert_eq!(bit.name, "bit1 ... bit8");
        assert_eq!(bit.short, "b");
        assert_eq!(bit.param_type, "gate");
        assert_eq!(bit.prefix.as_deref(), Some("bit"));
        assert_eq!(bit.count, Some(8));
        assert_eq!(bit.start_at, Some(1));

        // The embedded `copy` override carries declared metadata.
        assert_eq!(circuits[1].name, "copy");
        assert_eq!(circuits[1].ramsize, 32);
        assert_eq!(circuits[1].cable_kind.as_deref(), Some("audio"));
        assert_eq!(circuits[1].color.as_deref(), Some("knob"));
    }

    #[test]
    fn absent_title_and_description_get_defaults() {
        let doc: PluginDocument = toml::from_str(
            "[[circuit]]\n\
             name = \"MIN\"\n\
             category = \"util\"\n\
             ramsize = 128\n\
             [[circuit.inputs]]\n\
             name = \"in\"\n\
             short = \"i\"\n\
             type = \"cv\"\n\
             [[circuit.outputs]]\n\
             name = \"out\"\n\
             short = \"o\"\n\
             type = \"cv\"\n",
        )
        .expect("valid TOML");
        let circuits = parse_circuits(&doc).expect("valid semantics");
        assert_eq!(circuits.len(), 1);
        assert_eq!(circuits[0].name, "min");
        assert_eq!(circuits[0].title, "MIN");
        assert_eq!(circuits[0].description, "");
    }

    #[test]
    fn missing_ramsize_fixture_is_skipped() {
        let raw = fs::read_to_string(fixture_path("missing_ramsize.toml")).expect("read fixture");
        let doc: PluginDocument = toml::from_str(&raw).expect("valid TOML syntax");
        let err = parse_circuits(&doc).expect_err("semantic validation rejects missing ramsize");
        assert_eq!(err, "noramsize");
    }

    #[test]
    fn discover_plugins_reads_sorted_files_and_skips_invalid() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let plugins = tmp.path().join("droid-tui/plugins");
        fs::create_dir_all(&plugins).expect("create plugin dir");
        fs::copy(
            fixture_path("missing_ramsize.toml"),
            plugins.join("missing_ramsize.toml"),
        )
        .expect("copy fixture");
        fs::copy(fixture_path("valid.toml"), plugins.join("valid.toml")).expect("copy fixture");
        fs::write(
            plugins.join("aaa.toml"),
            "[[circuit]]\nname = \"ZAP\"\ncategory = \"logic\"\nramsize = 64\n",
        )
        .expect("write fixture");

        let files = discover_plugins(Some(tmp.path().as_os_str()), None);

        // Sorted filename order, invalid file skipped, valid files contribute.
        assert_eq!(files.len(), 2);
        assert_eq!(
            files[0].path.file_name().and_then(|n| n.to_str()),
            Some("aaa.toml")
        );
        assert_eq!(files[0].circuits.len(), 1);
        assert_eq!(files[0].circuits[0].name, "zap");
        assert_eq!(
            files[1].path.file_name().and_then(|n| n.to_str()),
            Some("valid.toml")
        );
        assert_eq!(files[1].circuits.len(), 2);
        assert_eq!(files[1].circuits[0].name, "newckt");
    }

    #[test]
    fn discover_plugins_missing_dir_returns_empty() {
        let files = discover_plugins(Some(OsStr::new("/nonexistent")), None);
        assert!(files.is_empty());
    }

    #[test]
    fn load_file_valid_fixture_returns_two_circuits() {
        // End-to-end through the loader's public entry: read + parse + validate.
        let file = load_file(&fixture_path("valid.toml")).expect("valid fixture loads");
        assert_eq!(file.path, fixture_path("valid.toml"));
        assert_eq!(file.circuits.len(), 2);

        // NEWCKT lowercases to the schema key convention; no declared metadata.
        assert_eq!(file.circuits[0].name, "newckt");
        assert_eq!(file.circuits[0].ramsize, 256);
        assert!(file.circuits[0].cable_kind.is_none());
        assert!(file.circuits[0].color.is_none());

        // The embedded `copy` override carries declared rendering metadata.
        assert_eq!(file.circuits[1].name, "copy");
        assert_eq!(file.circuits[1].ramsize, 32);
        assert_eq!(file.circuits[1].cable_kind.as_deref(), Some("audio"));
        assert_eq!(file.circuits[1].color.as_deref(), Some("knob"));
    }

    #[test]
    fn load_file_missing_ramsize_returns_none() {
        // One circuit without the required `ramsize` rejects the whole file.
        assert!(load_file(&fixture_path("missing_ramsize.toml")).is_none());
    }

    #[test]
    fn load_file_unreadable_or_malformed_returns_none() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let missing = tmp.path().join("missing.toml");
        assert!(load_file(&missing).is_none(), "unreadable file is skipped");

        let malformed = tmp.path().join("malformed.toml");
        fs::write(&malformed, "[[circuit]]\nname = ").expect("write malformed TOML");
        assert!(load_file(&malformed).is_none(), "malformed TOML is skipped");
    }

    #[test]
    fn discover_plugins_ignores_non_toml_files() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let plugins = tmp.path().join("droid-tui/plugins");
        fs::create_dir_all(&plugins).expect("create plugin dir");
        fs::copy(fixture_path("valid.toml"), plugins.join("valid.toml")).expect("copy fixture");
        fs::write(plugins.join("README.md"), "not a plugin").expect("write non-toml file");
        fs::write(plugins.join("schema.json"), "{}").expect("write non-toml file");

        let files = discover_plugins(Some(tmp.path().as_os_str()), None);

        assert_eq!(files.len(), 1);
        assert_eq!(
            files[0].path.file_name().and_then(|n| n.to_str()),
            Some("valid.toml")
        );
    }

    #[test]
    fn discover_plugins_preserves_sorted_order_for_overlay() {
        // `newckt` is defined in both valid.toml (ramsize 256) and the
        // alphabetically-later newckt_override.toml (ramsize 512). The loader
        // must hand the merge a stable sorted order so the later-file-wins
        // overlay in schema.rs is deterministic; the override itself lives on
        // the schema side. The ramsize delta keeps the winning definition
        // observable after the merge.
        let tmp = tempfile::tempdir().expect("tempdir");
        let plugins = tmp.path().join("droid-tui/plugins");
        fs::create_dir_all(&plugins).expect("create plugin dir");
        fs::copy(fixture_path("valid.toml"), plugins.join("valid.toml")).expect("copy fixture");
        fs::copy(
            fixture_path("newckt_override.toml"),
            plugins.join("newckt_override.toml"),
        )
        .expect("copy fixture");

        let files = discover_plugins(Some(tmp.path().as_os_str()), None);

        assert_eq!(files.len(), 2);
        assert_eq!(
            files[0].path.file_name().and_then(|n| n.to_str()),
            Some("newckt_override.toml"),
            "alphabetically earlier file loads first"
        );
        assert_eq!(
            files[1].path.file_name().and_then(|n| n.to_str()),
            Some("valid.toml")
        );
        let override_def = &files[0].circuits[0];
        assert_eq!(override_def.name, "newckt");
        assert_eq!(override_def.ramsize, 512);
        let base_def = &files[1].circuits[0];
        assert_eq!(base_def.name, "newckt");
        assert_eq!(base_def.ramsize, 256);
    }

    #[test]
    fn mixed_case_circuit_name_is_lowercased() {
        // Names are case-insensitive like the embedded schema lookup: any
        // casing normalizes to the lowercased schema key.
        let doc: PluginDocument = toml::from_str(
            "[[circuit]]\n\
             name = \"NeWcKt\"\n\
             category = \"logic\"\n\
             ramsize = 64\n",
        )
        .expect("valid TOML");
        let circuits = parse_circuits(&doc).expect("valid semantics");
        assert_eq!(circuits.len(), 1);
        assert_eq!(circuits[0].name, "newckt");
        assert_eq!(circuits[0].category, "logic");
    }
}
