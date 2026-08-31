use std::collections::{HashMap, HashSet};

use crate::patch::{Patch, Span};
use crate::schema::{is_valid_jack, suggest_circuit_with_schema, validate_jack, Schema};

// ---------------------------------------------------------------------------
// Severity / ValidationIssue
// ---------------------------------------------------------------------------

/// Severity of a validation finding.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Severity {
    Error,
    Warning,
    Hint,
}

/// One validation finding with its source span.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ValidationIssue {
    pub span: Span,
    pub severity: Severity,
    pub code: String,
    pub message: String,
}

impl PartialOrd for ValidationIssue {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for ValidationIssue {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.span
            .line
            .cmp(&other.span.line)
            .then_with(|| self.span.col_start.cmp(&other.span.col_start))
            .then_with(|| self.span.col_end.cmp(&other.span.col_end))
            .then_with(|| self.code.cmp(&other.code))
            .then_with(|| self.message.cmp(&other.message))
            .then_with(|| self.severity.cmp(&other.severity))
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn is_pure_cable(value: &str) -> bool {
    let trimmed = value.trim();
    if !trimmed.starts_with('_') {
        return false;
    }
    let cables = extract_cables(trimmed);
    cables.len() == 1 && cables[0] == trimmed
}

fn extract_cables(value: &str) -> Vec<String> {
    let chars: Vec<char> = value.chars().collect();
    let mut out = Vec::new();
    let mut i = 0;
    while i < chars.len() {
        if chars[i] == '_' {
            let boundary_ok = i == 0 || !chars[i - 1].is_ascii_alphanumeric();
            if boundary_ok {
                let start = i;
                i += 1;
                while i < chars.len() && (chars[i].is_ascii_alphanumeric() || chars[i] == '_') {
                    i += 1;
                }
                if i > start + 1 {
                    let token: String = chars[start..i].iter().collect();
                    let clean_end =
                        i >= chars.len() || !(chars[i].is_ascii_alphanumeric() || chars[i] == '_');
                    if clean_end {
                        out.push(token);
                    }
                }
                continue;
            }
        }
        i += 1;
    }
    out
}

fn extract_jack_tokens(value: &str) -> Vec<String> {
    let bytes = value.as_bytes();
    let len = bytes.len();
    let mut out = Vec::new();
    let mut i = 0;
    while i < len {
        let c = bytes[i] as char;
        // Must be A-Z, followed by digit, with word boundary before
        let boundary_ok =
            i == 0 || !((bytes[i - 1] as char).is_ascii_alphanumeric() || bytes[i - 1] == b'_');
        if c.is_ascii_uppercase()
            && boundary_ok
            && i + 1 < len
            && (bytes[i + 1] as char).is_ascii_digit()
        {
            let start = i;
            i += 1;
            while i < len && (bytes[i] as char).is_ascii_digit() {
                i += 1;
            }
            if i < len && bytes[i] == b'.' && i + 1 < len && (bytes[i + 1] as char).is_ascii_digit()
            {
                i += 1;
                while i < len && (bytes[i] as char).is_ascii_digit() {
                    i += 1;
                }
            }
            let clean_end = i >= len
                || !((bytes[i] as char).is_ascii_alphanumeric()
                    || bytes[i] == b'_'
                    || bytes[i] == b'.');
            if clean_end {
                out.push(value[start..i].to_string());
            }
            continue;
        }
        i += 1;
    }
    out
}

fn expand_param_names(
    prefix: Option<&String>,
    count: Option<u32>,
    start_at: Option<u32>,
    name: &str,
) -> Vec<String> {
    if let (Some(p), Some(c)) = (prefix, count) {
        let start = start_at.unwrap_or(1);
        (0..c).map(|i| format!("{}{}", p, start + i)).collect()
    } else {
        vec![name.to_string()]
    }
}

fn parse_jack_for_validate(token: &str) -> Option<(String, u32, Option<u32>)> {
    let mut chars = token.chars();
    let prefix = chars.next()?.to_string();
    if !prefix
        .chars()
        .next()
        .is_some_and(|c| c.is_ascii_uppercase())
    {
        return None;
    }
    let rest: String = chars.collect();
    let parts: Vec<&str> = rest.split('.').collect();
    let mod_num: u32 = parts[0].parse().ok()?;
    let channel = if parts.len() > 1 {
        Some(parts[1].parse::<u32>().ok()?)
    } else {
        None
    };
    Some((prefix, mod_num, channel))
}

// ---------------------------------------------------------------------------
// Main validator
// ---------------------------------------------------------------------------

/// Validate `patch` against `schema`, returning all issues sorted by `(line, col)`.
pub fn validate_patch(patch: &Patch, schema: &Schema) -> Vec<ValidationIssue> {
    let mut issues: Vec<ValidationIssue> = Vec::new();

    // --- Precompute cable definitions and sink set ---
    // defined: cable -> list of (section_idx, entry_idx)
    let mut defined: HashMap<String, Vec<(usize, usize)>> = HashMap::new();
    let mut sink_cables: HashSet<String> = HashSet::new();

    for (si, section) in patch.sections.iter().enumerate() {
        for (ei, entry) in section.detailed_entries.iter().enumerate() {
            if entry.key == "output" && is_pure_cable(&entry.value) {
                defined
                    .entry(entry.value.trim().to_string())
                    .or_default()
                    .push((si, ei));
            }
        }
    }
    let defined_names: HashSet<String> = defined.keys().cloned().collect();

    // Collect sink usages (any _CABLE ref not counting pure output definitions)
    for section in &patch.sections {
        for entry in &section.detailed_entries {
            let is_def = entry.key == "output" && is_pure_cable(&entry.value);
            if is_def {
                continue;
            }
            for cable in extract_cables(&entry.value) {
                sink_cables.insert(cable);
            }
        }
    }

    // --- Per-section checks ---
    for section in patch.sections.iter() {
        let lower = section.name.to_lowercase();
        let is_known_circuit = schema.circuits.contains_key(&lower);
        let is_known_controller = schema.controllers.contains_key(&lower);

        // 1. unknown circuit
        if !is_known_circuit && !is_known_controller {
            let suggestion = suggest_circuit_with_schema(&section.name, schema);
            let message = if let Some(s) = suggestion {
                format!("Unknown circuit \"{}\". Did you mean: {s}?", section.name)
            } else {
                format!("Unknown circuit \"{}\"", section.name)
            };
            issues.push(ValidationIssue {
                span: section.header_span,
                severity: Severity::Error,
                code: "unknown_circuit".to_string(),
                message,
            });
        }

        // Track seen params for this section
        let mut seen: HashMap<String, usize> = HashMap::new();
        let valid_param_set: Option<HashSet<String>> = if is_known_circuit {
            schema
                .get_param_names(&section.name)
                .map(|v| v.into_iter().collect())
        } else {
            None
        };

        // Missing-required check needs seen after full scan, but we collect now.
        // We do per-entry checks inline.
        for entry in &section.detailed_entries {
            // 2. duplicate param (second occurrence)
            if let Some(_first) = seen.get(&entry.key) {
                issues.push(ValidationIssue {
                    span: entry.value_span,
                    severity: Severity::Warning,
                    code: "duplicate_param".to_string(),
                    message: format!(
                        "Duplicate parameter \"{}\" in [{}]",
                        entry.key, section.name
                    ),
                });
            } else {
                seen.insert(entry.key.clone(), entry.line);
            }

            // 3. unknown param (only for known circuits)
            if let Some(ref valid) = valid_param_set {
                if !valid.contains(&entry.key) {
                    issues.push(ValidationIssue {
                        span: entry.value_span,
                        severity: Severity::Error,
                        code: "unknown_param".to_string(),
                        message: format!(
                            "Unknown parameter \"{}\" for circuit \"{}\"",
                            entry.key, section.name
                        ),
                    });
                }
            }

            // 4. invalid jack: scan value for jack tokens, validate each
            // Skip numeric and cable values: if value is pure numeric or starts with '_' skip jack check
            let trimmed = entry.value.trim();
            let is_numeric = trimmed.parse::<f64>().is_ok();
            let is_cable = trimmed.starts_with('_');
            if !is_numeric && !is_cable {
                let jack_tokens = extract_jack_tokens(&entry.value);
                let mut invalid_msg: Option<String> = None;
                for tok in &jack_tokens {
                    if !is_valid_jack(tok) {
                        // produce detailed message via validate_jack
                        if let Some((prefix, mod_num, ch)) = parse_jack_for_validate(tok) {
                            if let Some(msg) = validate_jack(&prefix, mod_num, ch) {
                                invalid_msg = Some(msg);
                                break;
                            }
                        }
                    }
                }
                if let Some(msg) = invalid_msg {
                    issues.push(ValidationIssue {
                        span: entry.value_span,
                        severity: Severity::Warning,
                        code: "invalid_jack".to_string(),
                        message: msg,
                    });
                }
            }

            // 6. undefined cable (sink references never defined)
            // For each cable in this entry's value (excluding pure output defs already considered)
            let is_def = entry.key == "output" && is_pure_cable(&entry.value);
            if !is_def {
                let cables = extract_cables(&entry.value);
                // dedup per entry to avoid duplicate warnings for same cable repeated in same value expression
                let mut seen_cable: HashSet<String> = HashSet::new();
                for cable in cables {
                    if !seen_cable.insert(cable.clone()) {
                        continue;
                    }
                    if !defined_names.contains(&cable) {
                        issues.push(ValidationIssue {
                            span: entry.value_span,
                            severity: Severity::Warning,
                            code: "undefined_cable".to_string(),
                            message: format!(
                                "Virtual cable \"{cable}\" is used but never defined as an output"
                            ),
                        });
                    }
                }
            }
        }

        // 5. missing required (essential == 2)
        if is_known_circuit {
            if let Some(def) = schema.circuits.get(&lower) {
                for input in &def.inputs {
                    if input.essential == 2 {
                        let expanded = expand_param_names(
                            input.prefix.as_ref(),
                            input.count,
                            input.start_at,
                            &input.name,
                        );
                        for name in expanded {
                            if !seen.contains_key(&name) {
                                issues.push(ValidationIssue {
                                    span: section.header_span,
                                    severity: Severity::Warning,
                                    code: "missing_required".to_string(),
                                    message: format!(
                                        "Missing required parameter \"{name}\" for [{}]",
                                        section.name
                                    ),
                                });
                            }
                        }
                    }
                }
            }
        }
    }

    // 7. duplicate cable def
    let mut defined_sorted: Vec<(String, Vec<(usize, usize)>)> = defined.into_iter().collect();
    defined_sorted.sort_by(|a, b| a.0.cmp(&b.0));
    for (cable, defs) in &defined_sorted {
        if defs.len() >= 2 {
            let first_section = &patch.sections[defs[0].0].name;
            for dup in defs.iter().skip(1) {
                let dup_entry = &patch.sections[dup.0].detailed_entries[dup.1];
                let second_section = &patch.sections[dup.0].name;
                issues.push(ValidationIssue {
                    span: dup_entry.value_span,
                    severity: Severity::Warning,
                    code: "duplicate_cable".to_string(),
                    message: format!(
                        "Virtual cable \"{cable}\" is defined {} times ([{first_section}] and [{second_section}]) — one output will silently override or fight the other; likely an accidental link edit",
                        defs.len()
                    ),
                });
            }
        }
    }

    // 8. unused cable (defined once but never referenced)
    for (cable, defs) in &defined_sorted {
        if defs.len() == 1 && !sink_cables.contains(cable) {
            let (si, ei) = defs[0];
            let entry = &patch.sections[si].detailed_entries[ei];
            let sec_name = &patch.sections[si].name;
            issues.push(ValidationIssue {
                span: entry.value_span,
                severity: Severity::Hint,
                code: "unused_cable".to_string(),
                message: format!(
                    "Virtual cable \"{cable}\" is defined in [{sec_name}] but never used"
                ),
            });
        }
    }

    // 9. RAM overflow
    {
        let mut ram_used: u32 = 0;
        let mut unknown = false;
        for section in &patch.sections {
            let lower = section.name.to_lowercase();
            if let Some(c) = schema.circuits.get(&lower) {
                ram_used = ram_used.saturating_add(c.ramsize);
            } else if let Some(ctrl) = schema.controllers.get(&lower) {
                ram_used = ram_used.saturating_add(ctrl.ramsize);
            } else {
                unknown = true;
            }
        }
        if !unknown && ram_used > 0 && !schema.available_memory.is_empty() {
            // Sort masters for deterministic order
            let mut masters: Vec<(&String, &u32)> = schema.available_memory.iter().collect();
            masters.sort_by(|a, b| a.0.cmp(b.0));
            for (master, avail) in masters {
                if ram_used > *avail {
                    issues.push(ValidationIssue {
                        span: Span {
                            line: 0,
                            col_start: 0,
                            col_end: 0,
                        },
                        severity: Severity::Error,
                        code: "ram_overflow".to_string(),
                        message: format!(
                            "Patch needs {ram_used} bytes of RAM but {master} only provides {avail} bytes — the patch will not load"
                        ),
                    });
                }
            }
        }
    }

    // Deterministic sort
    issues.sort();
    issues
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::patch::Patch;
    use crate::plugin::{PluginCircuit, PluginFile, PluginParam};
    use crate::schema::{load_schema, merge_plugins};
    use std::path::PathBuf;

    fn validate(ini: &str) -> Vec<ValidationIssue> {
        let patch = Patch::from_ini_str(ini, String::from("test")).unwrap();
        let schema = load_schema();
        validate_patch(&patch, schema)
    }

    fn has_code(issues: &[ValidationIssue], code: &str) -> bool {
        issues.iter().any(|i| i.code == code)
    }

    fn codes(issues: &[ValidationIssue]) -> Vec<String> {
        issues.iter().map(|i| i.code.clone()).collect()
    }

    #[test]
    fn schema_still_has_76_circuits() {
        // Exclude the schema::init tests' merged-schema window (see
        // schema::TEST_CACHE_LOCK): exact counts must observe the embedded
        // schema only.
        let _guard = crate::schema::TEST_CACHE_LOCK
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let schema = load_schema();
        assert_eq!(schema.circuits.len(), 76);
    }

    #[test]
    fn unknown_circuit_error_with_suggestion_at_header() {
        let issues = validate("[motoquncer]\nclock = I1\n");
        let item = issues
            .iter()
            .find(|i| i.code == "unknown_circuit")
            .expect("unknown_circuit");
        assert_eq!(item.severity, Severity::Error);
        // header_span line 0
        assert_eq!(item.span.line, 0);
        assert!(item.message.contains("motoquncer"));
        // Levenshtein suggestion for motoquncer -> motoquencer
        assert!(
            item.message.contains("motoquencer"),
            "message: {}",
            item.message
        );
    }

    #[test]
    fn unknown_circuit_without_suggestion() {
        let issues = validate("[zzzzzz]\nclock = I1\n");
        let item = issues.iter().find(|i| i.code == "unknown_circuit").unwrap();
        assert_eq!(item.severity, Severity::Error);
        assert!(!item.message.contains("Did you mean"));
    }

    #[test]
    fn duplicate_param_warning_at_second_value_span() {
        // buttongroup requires button1.. etc but we use copy-like: use copy circuit which has input/output
        let ini = "[copy]\ninput = I1\ninput = I2\noutput = O1\n";
        let issues = validate(ini);
        let dup = issues
            .iter()
            .find(|i| i.code == "duplicate_param")
            .expect("duplicate_param");
        assert_eq!(dup.severity, Severity::Warning);
        // second input at line 2 (0-based: header 0, input 1, input 2, output 3)
        assert_eq!(dup.span.line, 2);
        assert!(dup.message.contains("input"));
    }

    #[test]
    fn unknown_param_error_at_value_span() {
        let ini = "[copy]\ninput = I1\nnotaparam = 5\noutput = O1\n";
        let issues = validate(ini);
        let unk = issues
            .iter()
            .find(|i| i.code == "unknown_param")
            .expect("unknown_param");
        assert_eq!(unk.severity, Severity::Error);
        // notaparam line 2
        assert_eq!(unk.span.line, 2);
        assert!(unk.message.contains("notaparam"));
    }

    #[test]
    fn invalid_jack_warning_at_value_span() {
        // B33.1 is out of range (B max 32)
        let ini = "[copy]\ninput = B33.1\noutput = O1\n";
        let issues = validate(ini);
        let inv = issues
            .iter()
            .find(|i| i.code == "invalid_jack")
            .expect("invalid_jack");
        assert_eq!(inv.severity, Severity::Warning);
        // input line 1
        assert_eq!(inv.span.line, 1);
        assert!(inv.message.contains("B33.1"));
    }

    #[test]
    fn invalid_jack_skips_numeric_and_cable() {
        let ini = "[p2b8]\n[copy]\ninput = 5.5\noutput = _MYCABLE\n";
        let issues = validate(ini);
        assert!(
            !has_code(&issues, "invalid_jack"),
            "codes: {:?}",
            codes(&issues)
        );
    }

    #[test]
    fn missing_required_warning_at_header() {
        // lfo requires frequency maybe? Let's find a circuit with essential==2 input.
        // copy has input essential? Check: copy input essential? Likely 0/1.
        // Use a circuit known to have required: try motoquencer or algoquencer
        // Inspect: we will brute find one: search for essential==2 non-expanded
        let schema = load_schema();
        let mut required_example: Option<(String, String)> = None;
        for (name, def) in &schema.circuits {
            for inp in &def.inputs {
                if inp.essential == 2 && inp.prefix.is_none() {
                    required_example = Some((name.clone(), inp.name.clone()));
                    break;
                }
            }
            if required_example.is_some() {
                break;
            }
        }
        let (circ, param) = required_example.expect("must have required");
        let ini_full = format!("[p2b8]\n[{circ}]\n");
        let issues = validate(&ini_full);
        let miss = issues
            .iter()
            .find(|i| i.code == "missing_required")
            .expect("missing_required");
        assert_eq!(miss.severity, Severity::Warning);
        // header span of the circuit (line 1)
        assert_eq!(miss.span.line, 1);
        assert!(miss.message.contains(&param) || miss.message.contains("Missing required"));
    }

    #[test]
    fn count_expanded_required_missing() {
        // Find a circuit with essential==2 and prefix/count
        let schema = load_schema();
        let mut found: Option<(String, String)> = None;
        for (name, def) in &schema.circuits {
            for inp in &def.inputs {
                if inp.essential == 2 {
                    if let (Some(prefix), Some(_)) = (inp.prefix.as_ref(), inp.count) {
                        let start = inp.start_at.unwrap_or(1);
                        let expanded = format!("{prefix}{start}");
                        found = Some((name.clone(), expanded));
                        break;
                    }
                }
            }
            if found.is_some() {
                break;
            }
        }
        if let Some((circ, first_expanded)) = found {
            let ini = format!("[p2b8]\n[{circ}]\n");
            let issues = validate(&ini);
            let misses: Vec<_> = issues
                .iter()
                .filter(|i| i.code == "missing_required")
                .collect();
            // Should have at least one missing for the expanded param
            assert!(!misses.is_empty(), "expected missing_required for {circ}");
            assert!(
                misses.iter().any(|m| m.message.contains(&first_expanded)),
                "first expanded {first_expanded} not in {:?}",
                misses.iter().map(|m| &m.message).collect::<Vec<_>>()
            );
        }
    }

    #[test]
    fn undefined_cable_warning_at_value_span() {
        let ini = "[p2b8]\n[copy]\ninput = _UNDEFINED_CABLE\noutput = O1\n";
        let issues = validate(ini);
        let undef = issues
            .iter()
            .find(|i| i.code == "undefined_cable")
            .expect("undefined_cable");
        assert_eq!(undef.severity, Severity::Warning);
        // input line 2
        assert_eq!(undef.span.line, 2);
        assert!(undef.message.contains("_UNDEFINED_CABLE"));
    }

    #[test]
    fn duplicate_cable_warning_at_second_def() {
        let ini = "[p2b8]\n[copy]\ninput = I1\noutput = _DUP\n[copy]\ninput = I2\noutput = _DUP\n";
        let issues = validate(ini);
        let dup = issues
            .iter()
            .find(|i| i.code == "duplicate_cable")
            .expect("duplicate_cable");
        assert_eq!(dup.severity, Severity::Warning);
        // second def at line 5?
        assert!(dup.span.line > 0);
        assert!(dup.message.contains("_DUP"));
    }

    #[test]
    fn unused_cable_hint_at_def() {
        let ini = "[p2b8]\n[copy]\ninput = I1\noutput = _UNUSED\n";
        let issues = validate(ini);
        let unused = issues
            .iter()
            .find(|i| i.code == "unused_cable")
            .expect("unused_cable");
        assert_eq!(unused.severity, Severity::Hint);
        // def line 3 (output)
        assert_eq!(unused.span.line, 3);
        assert!(unused.message.contains("_UNUSED"));
    }

    #[test]
    fn unused_not_emitted_when_used() {
        let ini =
            "[p2b8]\n[copy]\ninput = I1\noutput = _USED\n[copy]\ninput = _USED\noutput = O1\n";
        let issues = validate(ini);
        assert!(
            !has_code(&issues, "unused_cable"),
            "should not have unused, codes: {:?}",
            codes(&issues)
        );
        assert!(!has_code(&issues, "undefined_cable"));
    }

    #[test]
    fn duplicate_cable_not_marked_unused() {
        let ini = "[p2b8]\n[copy]\ninput = I1\noutput = _DUP\n[copy]\ninput = I2\noutput = _DUP\n";
        let issues = validate(ini);
        assert!(
            !has_code(&issues, "unused_cable"),
            "duplicate should not also be unused"
        );
    }

    #[test]
    fn ram_overflow_error_at_zero_span() {
        // Find a schema with available_memory and sum > avail by adding many large circuits
        let schema = load_schema();
        // Find any master available value
        let available = *schema.available_memory.values().min().unwrap_or(&u32::MAX);
        // Find circuit with largest ramsize
        let (big_name, big_size) = schema
            .circuits
            .iter()
            .map(|(k, v)| (k.clone(), v.ramsize))
            .max_by_key(|(_, s)| *s)
            .unwrap();
        // Build patch with enough copies to exceed available
        let copies = (available / big_size.max(1) + 2) as usize;
        let mut ini = String::from("[p2b8]\n");
        for _ in 0..copies {
            ini.push_str(&format!("[{big_name}]\n"));
            // need to satisfy required? but RAM check counts even if missing required? It's still counted. So fill minimal to avoid unknown?
            // Use valid param to avoid missing required noise? But ram check still triggers regardless.
        }
        // If circuit needs required params, still RAM check will trigger before unknown? unknown false because all known.
        let issues = validate(&ini);
        let ram = issues
            .iter()
            .find(|i| i.code == "ram_overflow")
            .expect("ram_overflow");
        assert_eq!(ram.severity, Severity::Error);
        assert_eq!(ram.span.line, 0);
        assert_eq!(ram.span.col_start, 0);
        assert!(ram.message.contains("bytes of RAM"));
    }

    #[test]
    fn ram_not_checked_when_unknown_circuit() {
        let schema = load_schema();
        let unknown = "[unknowncircuit]\nfoo = 1\n[p2b8]\n";
        let patch = Patch::from_ini_str(unknown, String::from("t")).unwrap();
        let issues = validate_patch(&patch, schema);
        // Should have unknown_circuit but no ram_overflow because unknown=true
        assert!(has_code(&issues, "unknown_circuit"));
        assert!(!has_code(&issues, "ram_overflow"));
    }

    /// Build an owned schema whose embedded circuits are augmented with one
    /// plugin-defined circuit (ramsize 16) via the same `merge_plugins` entry
    /// point `schema::init` uses at startup. This mirrors task 4.1: plugin
    /// circuits land in `Schema.circuits` and must therefore participate in
    /// every validation check, especially the RAM budget.
    fn schema_with_plugin_circuit() -> crate::schema::Schema {
        let base = (*load_schema()).clone();
        let circuit = PluginCircuit {
            name: "ramtestplugin".to_string(),
            category: "logic".to_string(),
            ramsize: 16,
            title: "RAM Test Plugin".to_string(),
            description: String::new(),
            cable_kind: None,
            color: None,
            inputs: vec![PluginParam {
                name: "input".to_string(),
                short: "i".to_string(),
                param_type: "cv".to_string(),
                default: Some("0".to_string()),
                prefix: None,
                count: None,
                start_at: None,
            }],
            outputs: vec![PluginParam {
                name: "output".to_string(),
                short: "o".to_string(),
                param_type: "cv".to_string(),
                default: None,
                prefix: None,
                count: None,
                start_at: None,
            }],
        };
        let file = PluginFile {
            path: PathBuf::from("ram_test.toml"),
            circuits: vec![circuit],
        };
        merge_plugins(base, &[file])
    }

    #[test]
    fn plugin_circuit_participates_in_ram_overflow_check() {
        let schema = schema_with_plugin_circuit();
        // Sum enough instances of the 16-byte plugin circuit to exceed every
        // master's available memory; the plugin circuit is known, so the RAM
        // check must NOT be skipped the way `ram_not_checked_when_unknown_circuit`
        // documents it is for a genuinely unknown circuit.
        let avail_min = *schema.available_memory.values().min().unwrap();
        let copies = (avail_min / 16 + 2) as usize;
        let mut ini = String::from("[p2b8]\n");
        for _ in 0..copies {
            ini.push_str("[ramtestplugin]\ninput = 0\n");
        }
        let patch = Patch::from_ini_str(&ini, String::from("t")).unwrap();
        let issues = validate_patch(&patch, &schema);
        assert!(
            has_code(&issues, "ram_overflow"),
            "expected ram_overflow, got: {:?}",
            codes(&issues)
        );
        // The plugin circuit itself is recognized, not unknown.
        assert!(!has_code(&issues, "unknown_circuit"));
    }

    #[test]
    fn plugin_circuit_is_recognized_within_budget() {
        let schema = schema_with_plugin_circuit();
        let ini = "[p2b8]\n[ramtestplugin]\ninput = 0\n";
        let patch = Patch::from_ini_str(ini, String::from("t")).unwrap();
        let issues = validate_patch(&patch, &schema);
        assert!(
            !has_code(&issues, "ram_overflow"),
            "unexpected ram_overflow: {:?}",
            codes(&issues)
        );
        assert!(
            !has_code(&issues, "unknown_circuit"),
            "plugin circuit should be known: {:?}",
            codes(&issues)
        );
        assert!(
            !has_code(&issues, "unknown_param"),
            "plugin circuit's own params should validate: {:?}",
            codes(&issues)
        );
    }

    #[test]
    fn sorted_output_by_line_col() {
        // Create a patch that triggers multiple issues on different lines
        let ini = "[unknown1]\nfoo = 1\n[copy]\ninput = B33.1\ninput = I1\n[copy]\ninput = _UNDEF\noutput = O1\n";
        let issues = validate(ini);
        // Verify sorted by span
        for w in issues.windows(2) {
            assert!(
                w[0].span.line < w[1].span.line
                    || (w[0].span.line == w[1].span.line
                        && w[0].span.col_start <= w[1].span.col_start)
                    || (w[0].span.line == w[1].span.line
                        && w[0].span.col_start == w[1].span.col_start
                        && w[0].code <= w[1].code),
                "not sorted: {:?} vs {:?}",
                w[0],
                w[1]
            );
        }
    }

    #[test]
    fn validation_is_pure_and_deterministic() {
        let ini = "[p2b8]\n[copy]\ninput = B33.1\noutput = _A\n[copy]\ninput = _A\noutput = O1\n";
        let patch = Patch::from_ini_str(ini, String::from("t")).unwrap();
        let schema = load_schema();
        let a = validate_patch(&patch, schema);
        let b = validate_patch(&patch, schema);
        assert_eq!(a, b);
    }

    #[test]
    fn case_insensitive_duplicate_and_unknown() {
        // Keys are lowercased by parser; Input vs input should be duplicate
        let ini = "[copy]\nInput = I1\nINPUT = I2\noutput = O1\n";
        let issues = validate(ini);
        assert!(has_code(&issues, "duplicate_param"));
    }
}
