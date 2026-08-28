use std::collections::HashMap;

use serde::{Deserialize, Serialize};

// ---------------------------------------------------------------------------
// Embedded source of truth
// ---------------------------------------------------------------------------

const CIRCUITS_JSON: &str = include_str!("../ext/droid-lsp/droid-lsp/src/circuits.json");

// ---------------------------------------------------------------------------
// Raw JSON shapes (serde mirror of circuits.json)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Deserialize)]
struct RawSchema {
    firmware_version: String,
    jacktable_initial_size: usize,
    available_memory: HashMap<String, u32>,
    circuits: HashMap<String, RawCircuitDef>,
    controllers: HashMap<String, RawControllerDef>,
    manual_references: HashMap<String, u32>,
}

#[derive(Debug, Clone, Deserialize)]
struct RawCircuitDef {
    category: String,
    title: String,
    description: String,
    ramsize: Option<u32>,
    inputs: Vec<RawParam>,
    outputs: Vec<RawParam>,
    presets: u32,
    manual: u32,
}

#[derive(Debug, Clone, Deserialize)]
struct RawParam {
    name: String,
    short: String,
    #[serde(rename = "type")]
    param_type: String,
    default: Option<serde_json::Value>,
    description: String,
    essential: u8,
    ramhint: String,
    autotitle: bool,
    prefix: Option<String>,
    count: Option<u32>,
    start_at: Option<u32>,
    #[allow(dead_code)]
    essential_count: Option<u32>,
}

#[derive(Debug, Clone, Deserialize)]
struct RawControllerDef {
    ramsize: u32,
}

// ---------------------------------------------------------------------------
// Public types
// ---------------------------------------------------------------------------

/// A single parameter (input or output) of a circuit.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CircuitParam {
    pub name: String,
    pub short: String,
    #[serde(rename = "type")]
    pub param_type: String,
    pub default: Option<serde_json::Value>,
    pub description: String,
    pub essential: u8,
    pub ramhint: String,
    pub autotitle: bool,
    pub prefix: Option<String>,
    pub count: Option<u32>,
    pub start_at: Option<u32>,
}

/// Full definition of a circuit.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CircuitDef {
    pub category: String,
    pub title: String,
    pub description: String,
    pub ramsize: u32,
    pub inputs: Vec<CircuitParam>,
    pub outputs: Vec<CircuitParam>,
    pub presets: u32,
    pub manual: u32,
}

/// Controller definition (only ramsize today).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ControllerDef {
    pub ramsize: u32,
}

/// Authoritative DROID schema loaded from `circuits.json`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Schema {
    pub firmware_version: String,
    pub jacktable_initial_size: usize,
    pub available_memory: HashMap<String, u32>,
    pub circuits: HashMap<String, CircuitDef>,
    pub controllers: HashMap<String, ControllerDef>,
    pub manual_references: HashMap<String, u32>,
}

// ---------------------------------------------------------------------------
// Jack table — mirrors droid-lsp/src/schema.ts JACK_TABLE
// ---------------------------------------------------------------------------

/// One hardware jack family (e.g. B/L/P/M/S/G buttons, LEDs, faders, gates).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct JackRule {
    pub prefix: &'static str,
    pub min_mod: u32,
    pub max_mod: u32,
    pub min_ch: u32,
    pub max_ch: u32,
    pub has_channel: bool,
}

pub static JACK_TABLE: &[JackRule] = &[
    JackRule {
        prefix: "I",
        min_mod: 1,
        max_mod: 8,
        min_ch: 0,
        max_ch: 0,
        has_channel: false,
    },
    JackRule {
        prefix: "O",
        min_mod: 1,
        max_mod: 8,
        min_ch: 0,
        max_ch: 0,
        has_channel: false,
    },
    JackRule {
        prefix: "G",
        min_mod: 1,
        max_mod: 12,
        min_ch: 1,
        max_ch: 8,
        has_channel: true,
    },
    JackRule {
        prefix: "B",
        min_mod: 1,
        max_mod: 32,
        min_ch: 1,
        max_ch: 32,
        has_channel: true,
    },
    JackRule {
        prefix: "L",
        min_mod: 1,
        max_mod: 32,
        min_ch: 1,
        max_ch: 32,
        has_channel: true,
    },
    JackRule {
        prefix: "P",
        min_mod: 1,
        max_mod: 10,
        min_ch: 1,
        max_ch: 10,
        has_channel: true,
    },
    JackRule {
        prefix: "M",
        min_mod: 1,
        max_mod: 4,
        min_ch: 1,
        max_ch: 8,
        has_channel: true,
    },
    JackRule {
        prefix: "S",
        min_mod: 1,
        max_mod: 10,
        min_ch: 1,
        max_ch: 10,
        has_channel: true,
    },
    JackRule {
        prefix: "E",
        min_mod: 1,
        max_mod: 4,
        min_ch: 1,
        max_ch: 8,
        has_channel: true,
    },
    JackRule {
        prefix: "N",
        min_mod: 1,
        max_mod: 8,
        min_ch: 0,
        max_ch: 0,
        has_channel: false,
    },
    JackRule {
        prefix: "R",
        min_mod: 1,
        max_mod: 24,
        min_ch: 0,
        max_ch: 0,
        has_channel: false,
    },
];

/// Validate a single jack token like `B1.1` or `I3` against `JACK_TABLE`.
/// Returns `None` if valid or unknown prefix (skip), `Some(msg)` if out of range.
pub fn validate_jack(prefix: &str, mod_num: u32, channel: Option<u32>) -> Option<String> {
    for rule in JACK_TABLE {
        if rule.prefix == prefix {
            if mod_num < rule.min_mod || mod_num > rule.max_mod {
                let ch_suffix = if rule.has_channel { ".1" } else { "" };
                let max_suffix = if rule.has_channel {
                    format!(".{}", rule.max_ch)
                } else {
                    String::new()
                };
                let jack = format!(
                    "{}{}{}",
                    prefix,
                    mod_num,
                    channel.map_or_else(String::new, |c| format!(".{c}"))
                );
                let start = format!("{}{}{}", rule.prefix, rule.min_mod, ch_suffix);
                let end = format!("{}{}{}", rule.prefix, rule.max_mod, max_suffix);
                return Some(format!(
                    "Invalid jack reference \"{jack}\" (valid range: {start}–{end})"
                ));
            }
            if rule.has_channel {
                if let Some(ch) = channel {
                    if ch < rule.min_ch || ch > rule.max_ch {
                        return Some(format!(
                            "Invalid jack reference \"{prefix}{mod_num}.{ch}\" (channel must be {}–{})",
                            rule.min_ch, rule.max_ch
                        ));
                    }
                }
            }
            return None;
        }
    }
    None
}

/// Convenience: check whether a raw jack string like `"B1.1"` is valid.
/// Returns `true` for valid or unknown-prefix tokens (unknown prefixes are skipped).
pub fn is_valid_jack(jack: &str) -> bool {
    // Parse prefix letter + digits + optional .digits
    let mut chars = jack.chars();
    let prefix = match chars.next() {
        Some(c) if c.is_ascii_uppercase() => c.to_string(),
        _ => return true,
    };
    let rest: String = chars.collect();
    let parts: Vec<&str> = rest.split('.').collect();
    let mod_num: u32 = match parts[0].parse() {
        Ok(n) => n,
        Err(_) => return true,
    };
    let channel = if parts.len() > 1 {
        match parts[1].parse::<u32>() {
            Ok(n) => Some(n),
            Err(_) => return true,
        }
    } else {
        None
    };
    validate_jack(&prefix, mod_num, channel).is_none()
}

// ---------------------------------------------------------------------------
// Parameter name expansion (prefix/count/start_at)
// ---------------------------------------------------------------------------

fn expand_names(param: &CircuitParam) -> Vec<String> {
    if let (Some(prefix), Some(count)) = (param.prefix.as_deref(), param.count) {
        let start = param.start_at.unwrap_or(1);
        (0..count)
            .map(|i| format!("{}{}", prefix, start + i))
            .collect()
    } else {
        vec![param.name.clone()]
    }
}

impl Schema {
    /// All valid parameter names for a circuit (inputs + outputs, expanded).
    pub fn get_param_names(&self, circuit: &str) -> Option<Vec<String>> {
        let key = circuit.to_lowercase();
        let def = self.circuits.get(&key)?;
        let mut names = Vec::new();
        for p in &def.inputs {
            names.extend(expand_names(p));
        }
        for p in &def.outputs {
            names.extend(expand_names(p));
        }
        Some(names)
    }

    /// Expanded output parameter names (those whose `_` values define cables).
    pub fn get_output_param_names(&self, circuit: &str) -> Option<Vec<String>> {
        let key = circuit.to_lowercase();
        let def = self.circuits.get(&key)?;
        let mut names = Vec::new();
        for p in &def.outputs {
            names.extend(expand_names(p));
        }
        Some(names)
    }

    /// Whether a param is an input or output of a circuit (after expansion).
    pub fn get_param_kind(&self, circuit: &str, param: &str) -> Option<&'static str> {
        let key = circuit.to_lowercase();
        let def = self.circuits.get(&key)?;
        for o in &def.outputs {
            if expand_names(o).contains(&param.to_string()) {
                return Some("output");
            }
        }
        for i in &def.inputs {
            if expand_names(i).contains(&param.to_string()) {
                return Some("input");
            }
        }
        None
    }

    /// Circuit names (lowercase) from the schema.
    pub fn circuit_names(&self) -> Vec<String> {
        self.circuits.keys().cloned().collect()
    }

    /// Controller names (lowercase).
    pub fn controller_names(&self) -> Vec<String> {
        self.controllers.keys().cloned().collect()
    }
}

/// Free function: expanded param names for a circuit, loading schema on demand.
/// Returns `None` if the circuit is unknown.
pub fn get_param_names(circuit: &str) -> Option<Vec<String>> {
    let schema = load_schema();
    schema.get_param_names(circuit)
}

// ---------------------------------------------------------------------------
// Schema loading
// ---------------------------------------------------------------------------

/// Load the authoritative schema from the embedded `circuits.json`.
pub fn load_schema() -> Schema {
    let raw: RawSchema =
        serde_json::from_str(CIRCUITS_JSON).expect("embedded circuits.json must be valid JSON");

    let mut circuits = HashMap::new();
    for (name, def) in raw.circuits {
        let key = name.to_lowercase();
        circuits.insert(
            key,
            CircuitDef {
                category: def.category,
                title: def.title,
                description: def.description,
                ramsize: def.ramsize.unwrap_or(0),
                inputs: def
                    .inputs
                    .into_iter()
                    .map(|p| CircuitParam {
                        name: p.name,
                        short: p.short,
                        param_type: p.param_type,
                        default: p.default,
                        description: p.description,
                        essential: p.essential,
                        ramhint: p.ramhint,
                        autotitle: p.autotitle,
                        prefix: p.prefix,
                        count: p.count,
                        start_at: p.start_at,
                    })
                    .collect(),
                outputs: def
                    .outputs
                    .into_iter()
                    .map(|p| CircuitParam {
                        name: p.name,
                        short: p.short,
                        param_type: p.param_type,
                        default: p.default,
                        description: p.description,
                        essential: p.essential,
                        ramhint: p.ramhint,
                        autotitle: p.autotitle,
                        prefix: p.prefix,
                        count: p.count,
                        start_at: p.start_at,
                    })
                    .collect(),
                presets: def.presets,
                manual: def.manual,
            },
        );
    }

    let mut controllers = HashMap::new();
    for (name, def) in raw.controllers {
        controllers.insert(
            name.to_lowercase(),
            ControllerDef {
                ramsize: def.ramsize,
            },
        );
    }

    let mut manual_references = HashMap::new();
    for (k, v) in raw.manual_references {
        manual_references.insert(k.to_lowercase(), v);
    }

    Schema {
        firmware_version: raw.firmware_version,
        jacktable_initial_size: raw.jacktable_initial_size,
        available_memory: raw.available_memory,
        circuits,
        controllers,
        manual_references,
    }
}

// ---------------------------------------------------------------------------
// Levenshtein + suggestions
// ---------------------------------------------------------------------------

/// Levenshtein edit distance between two strings.
#[allow(clippy::needless_range_loop)]
pub fn levenshtein(a: &str, b: &str) -> usize {
    let a_chars: Vec<char> = a.chars().collect();
    let b_chars: Vec<char> = b.chars().collect();
    let m = a_chars.len();
    let n = b_chars.len();
    if m == 0 {
        return n;
    }
    if n == 0 {
        return m;
    }
    let mut dp = vec![vec![0usize; n + 1]; m + 1];
    for i in 0..=m {
        dp[i][0] = i;
    }
    for j in 0..=n {
        dp[0][j] = j;
    }
    for i in 1..=m {
        for j in 1..=n {
            let cost = if a_chars[i - 1] == b_chars[j - 1] {
                0
            } else {
                1
            };
            dp[i][j] = (dp[i - 1][j] + 1)
                .min(dp[i][j - 1] + 1)
                .min(dp[i - 1][j - 1] + cost);
        }
    }
    dp[m][n]
}

/// Suggest the closest circuit/controller name for an unknown name.
/// Returns `None` if no candidate within distance ≤ 3.
pub fn suggest_circuit(name: &str) -> Option<String> {
    let schema = load_schema();
    suggest_circuit_with_schema(name, &schema)
}

/// Same as `suggest_circuit` but with an explicit schema (pure, no re-parse).
pub fn suggest_circuit_with_schema(name: &str, schema: &Schema) -> Option<String> {
    let lower = name.to_lowercase();
    let mut candidates: Vec<(String, usize)> = schema
        .circuits
        .keys()
        .chain(schema.controllers.keys())
        .map(|c| (c.clone(), levenshtein(&lower, c)))
        .collect();
    candidates.sort_by_key(|(_, d)| *d);
    candidates
        .into_iter()
        .find(|(_, d)| *d <= 3 && *d > 0)
        .map(|(name, _)| name)
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn schema_has_76_circuits() {
        let schema = load_schema();
        assert_eq!(
            schema.circuits.len(),
            76,
            "circuits.json must contain 76 circuits, got {}",
            schema.circuits.len()
        );
    }

    #[test]
    fn schema_has_10_controllers() {
        let schema = load_schema();
        assert_eq!(schema.controllers.len(), 10);
    }

    #[test]
    fn schema_firmware_version_is_blue7() {
        let schema = load_schema();
        assert_eq!(schema.firmware_version, "blue-7");
    }

    #[test]
    fn param_expansion_prefix_count() {
        let schema = load_schema();
        // buttongroup has button1..button32
        let names = schema
            .get_param_names("buttongroup")
            .expect("buttongroup exists");
        assert!(names.contains(&"button1".to_string()));
        assert!(names.contains(&"button32".to_string()));
        assert!(!names.contains(&"button33".to_string()));
        // calib: tune0..tune8 (start_at 0)
        let names = schema
            .get_param_names("calibrator")
            .expect("calibrator exists");
        assert!(names.contains(&"tune0".to_string()));
        assert!(names.contains(&"tune8".to_string()));
    }

    #[test]
    fn param_expansion_simple_name() {
        let schema = load_schema();
        let names = schema.get_param_names("copy").expect("copy exists");
        assert!(names.contains(&"input".to_string()));
        assert!(names.contains(&"output".to_string()));
    }

    #[test]
    fn free_get_param_names_matches_schema() {
        let names = get_param_names("copy").expect("copy exists");
        assert!(names.contains(&"input".to_string()));
    }

    #[test]
    fn unknown_circuit_returns_none() {
        let schema = load_schema();
        assert!(schema.get_param_names("doesnotexist").is_none());
        assert!(get_param_names("doesnotexist").is_none());
    }

    #[test]
    fn levenshtein_basic() {
        assert_eq!(levenshtein("", ""), 0);
        assert_eq!(levenshtein("copy", "copy"), 0);
        assert_eq!(levenshtein("copy", "coph"), 1);
        assert_eq!(levenshtein("kitten", "sitting"), 3);
    }

    #[test]
    fn suggest_circuit_finds_close_match() {
        let schema = load_schema();
        // "motoquncer" -> "motoquencer"
        let s = suggest_circuit_with_schema("motoquncer", &schema);
        assert_eq!(s.as_deref(), Some("motoquencer"));
        // exact match must not suggest itself (dist 0 filtered)
        let s = suggest_circuit_with_schema("copy", &schema);
        assert_ne!(s.as_deref(), Some("copy"));
        // very distant -> None
        let s = suggest_circuit_with_schema("zzzzzz", &schema);
        assert!(s.is_none());
    }

    #[test]
    fn suggest_circuit_free_fn() {
        let s = suggest_circuit("copi");
        assert!(s.is_some());
    }

    #[test]
    fn jack_table_has_11_entries() {
        assert_eq!(JACK_TABLE.len(), 11);
    }

    #[test]
    fn validate_jack_accepts_valid() {
        assert!(validate_jack("B", 1, Some(1)).is_none());
        assert!(validate_jack("I", 8, None).is_none());
        assert!(is_valid_jack("B1.1"));
        assert!(is_valid_jack("I1"));
    }

    #[test]
    fn validate_jack_rejects_out_of_range() {
        assert!(validate_jack("B", 33, Some(1)).is_some());
        assert!(validate_jack("I", 9, None).is_some());
        assert!(!is_valid_jack("B33.1"));
        assert!(!is_valid_jack("I9"));
    }

    #[test]
    fn available_memory_present() {
        let schema = load_schema();
        assert!(schema.available_memory.contains_key("master16"));
        assert!(schema.available_memory.contains_key("master18"));
    }
}
