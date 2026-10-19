use std::collections::{HashMap, HashSet};

use crate::patch::{IniEntry, Patch, Span};
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

/// Does this entry *define* the virtual cable named by its value?
///
/// DROID defines a cable by putting the `_NAME` where an output would go, so
/// the parameter has to be an output of the circuit. Consulting the schema
/// matters: `output1`/`output2` ([switch]), `or`/`and` ([logic]), `square`
/// ([lfo]), `linear` ([slew]), `gateoutput` ([sequencer]) and friends all
/// define cables, and a rule that only accepts the literal key `output`
/// reports every such cable as undefined. Unknown circuits and parameters
/// fall back to the conventional `output` key, mirroring
/// `patch.rs::collect_cable_index` so the index and the warning list agree.
fn is_cable_definition(schema: &Schema, section: &str, key: &str, value: &str) -> bool {
    if !is_pure_cable(value) {
        return false;
    }
    let key_lower = key.to_lowercase();
    match schema.get_param_kind(section, &key_lower) {
        Some(kind) => kind == "output",
        None => key_lower == "output",
    }
}

/// One token of a DROID parameter value.
#[derive(Clone, Copy, PartialEq, Debug)]
enum ExprTok {
    Operand { number: bool },
    Mul,
    Div,
    Plus,
    Minus,
    OpenBracket,
    CloseBracket,
}

/// Split a parameter value into DROID tokens. `None` means "not an
/// expression" — a text value, an empty value, or anything this tokenizer does
/// not know — and the caller then reports nothing rather than guessing.
fn tokenize_expr(value: &str) -> Option<Vec<ExprTok>> {
    let chars: Vec<char> = value.chars().collect();
    let mut out = Vec::new();
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        if c.is_whitespace() {
            i += 1;
            continue;
        }
        match c {
            '*' => {
                out.push(ExprTok::Mul);
                i += 1;
            }
            '/' => {
                out.push(ExprTok::Div);
                i += 1;
            }
            '+' => {
                out.push(ExprTok::Plus);
                i += 1;
            }
            '-' => {
                out.push(ExprTok::Minus);
                i += 1;
            }
            '(' => {
                out.push(ExprTok::OpenBracket);
                i += 1;
            }
            ')' => {
                out.push(ExprTok::CloseBracket);
                i += 1;
            }
            '_' => {
                let start = i;
                i += 1;
                while i < chars.len() && (chars[i].is_ascii_alphanumeric() || chars[i] == '_') {
                    i += 1;
                }
                if i == start + 1 {
                    return None;
                }
                out.push(ExprTok::Operand { number: false });
            }
            c if c.is_ascii_digit() => {
                while i < chars.len() && chars[i].is_ascii_digit() {
                    i += 1;
                }
                if i + 1 < chars.len() && chars[i] == '.' && chars[i + 1].is_ascii_digit() {
                    i += 1;
                    while i < chars.len() && chars[i].is_ascii_digit() {
                        i += 1;
                    }
                }
                if i < chars.len() && (chars[i] == 'V' || chars[i] == '%') {
                    i += 1;
                }
                out.push(ExprTok::Operand { number: true });
            }
            c if c.is_ascii_uppercase() => {
                // register: P1.1, B3.16, G1.5, O1, L1.2 …
                let start = i;
                i += 1;
                while i < chars.len() && chars[i].is_ascii_digit() {
                    i += 1;
                }
                if i == start + 1 {
                    // `on` / `off`
                    let rest = &chars[start..];
                    let is_on = rest.starts_with(&['o', 'n']) && rest.len() == 2;
                    let is_off =
                        rest.len() == 3 && rest[0] == 'o' && rest[1] == 'f' && rest[2] == 'f';
                    if is_on || is_off {
                        out.push(ExprTok::Operand { number: false });
                        return Some(out);
                    }
                    return None;
                }
                if i < chars.len() && chars[i] == '.' {
                    let dot = i;
                    i += 1;
                    let digits_start = i;
                    while i < chars.len() && chars[i].is_ascii_digit() {
                        i += 1;
                    }
                    if i == digits_start {
                        i = dot;
                    }
                }
                out.push(ExprTok::Operand { number: false });
            }
            _ => return None,
        }
    }
    Some(out)
}

/// Grammar violations of DROID's parameter values (manual §5.7, "Summary and
/// Further notes"): the format is `A * B + C` — one attenuation, one offset,
/// in that order. Brackets, more than one multiplication, a division by
/// anything but a fixed number, and additional operations are all rejected by
/// the firmware as an invalid parameter value, so the patch would not load.
///
/// Text values (quoted) and values the tokenizer does not recognize are left
/// alone: the check reports expressions, not arbitrary content.
fn expression_problem(value: &str) -> Option<&'static str> {
    let trimmed = value.trim();
    if trimmed.is_empty() || trimmed.starts_with('"') || trimmed.starts_with('\'') {
        return None;
    }
    let toks = tokenize_expr(trimmed)?;
    if toks.is_empty() {
        return None;
    }
    // Only calculations are checked: without any operator the value is a bare
    // number, register, cable name — or text, which is not this check's job.
    if !toks.iter().any(|t| {
        matches!(
            t,
            ExprTok::Mul
                | ExprTok::Div
                | ExprTok::Plus
                | ExprTok::Minus
                | ExprTok::OpenBracket
                | ExprTok::CloseBracket
        )
    }) {
        return None;
    }
    if toks
        .iter()
        .any(|t| matches!(t, ExprTok::OpenBracket | ExprTok::CloseBracket))
    {
        return Some("brackets are not allowed (DROID reads A * B + C only)");
    }
    let mut mults = 0usize;
    let mut offsets = 0usize;
    let mut expect_operand = true;
    let mut after_div = false;
    let mut unary_minus_ok = true;
    for tok in toks.iter() {
        match tok {
            ExprTok::Operand { number } => {
                if !expect_operand {
                    return Some("two values without an operator between them");
                }
                if after_div && !number {
                    return Some("you can only divide by a fixed number");
                }
                after_div = false;
                expect_operand = false;
                unary_minus_ok = false;
            }
            ExprTok::Mul | ExprTok::Div => {
                if expect_operand {
                    return Some("operator without a value before it");
                }
                if *tok == ExprTok::Div {
                    after_div = true;
                } else {
                    mults += 1;
                    if mults > 1 {
                        return Some("only one multiplication is allowed (A * B + C)");
                    }
                }
                expect_operand = true;
                unary_minus_ok = true;
            }
            ExprTok::Plus | ExprTok::Minus => {
                if expect_operand {
                    // unary minus is the documented shorthand for -1 * x
                    if *tok == ExprTok::Minus && unary_minus_ok {
                        continue;
                    }
                    return Some("operator without a value before it");
                }
                offsets += 1;
                if offsets > 1 {
                    return Some("only one offset (+ / -) is allowed (A * B + C)");
                }
                expect_operand = true;
                unary_minus_ok = true;
            }
            ExprTok::OpenBracket | ExprTok::CloseBracket => {
                return Some("brackets are not allowed (DROID reads A * B + C only)");
            }
        }
    }
    if expect_operand {
        return Some("the value ends with an operator");
    }
    None
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
// Documented alternatives
// ---------------------------------------------------------------------------

/// One row of the documented-alternatives table: an `essential == 2` parameter
/// that the DROID manual documents as one of several alternative inputs.
struct AlternativeInputs {
    circuit: &'static str,
    required: &'static str,
    alternatives: &'static [&'static str],
}

/// Inputs the manual documents as alternatives, keyed by the `essential == 2`
/// param they satisfy.
///
/// The schema is the authority for *what exists*, but `essential` in
/// `circuits.json` is a one-dimensional flag and cannot express "A or B".
/// Neither does upstream `droid-lsp`: `circuits.json` carries only `essential`
/// and `essential_count`, and `diagnostics.ts` implements exactly the same
/// `essential === 2 && !(prefix && count)` policy. The manual does state the
/// alternatives, in prose, so each row below cites it. Nothing is inferred from
/// a param's name, type, or default: 43 of the 72 singular `essential == 2`
/// inputs declare a default other than `unpatched` (47 declare some default,
/// `[compare] ifequal` and `[contour] gate` among them), so "has a default"
/// would silence genuinely required params.
///
/// Same shape as the `JACK_TABLE` precedent in `src/schema.rs`: a plain static
/// slice of rules, scanned linearly.
static ALTERNATIVE_INPUTS: &[AlternativeInputs] = &[
    AlternativeInputs {
        circuit: "lfo",
        required: "hz",
        // Manual: "you cannot use hz at that same time as taptempo"; both
        // combine with `rate`, and `hz` additionally defaults to 1.
        alternatives: &["taptempo", "rate"],
    },
    AlternativeInputs {
        circuit: "gatetool",
        required: "inputgate",
        // Manual: "gatetool has three different types of inputs. Usually you
        // would patch only one of these" — inputgate, inputtrigger, inputedge.
        alternatives: &["inputtrigger", "inputedge"],
    },
    AlternativeInputs {
        circuit: "contour",
        required: "gate",
        // Manual: "two alternative inputs: gate and trigger" for starting the
        // envelope.
        alternatives: &["trigger"],
    },
    AlternativeInputs {
        circuit: "compare",
        required: "ifequal",
        // Manual: "It is allowed to omit any of the inputs ifless, ifequal,
        // ifgreater or else."
        alternatives: &["ifgreater", "ifless", "else"],
    },
];

/// Is `required` satisfied by a documented alternative in this section?
///
/// `entries` holds the section's own entries, so this is a same-section check by
/// construction; the whole patch is never consulted. An alternative counts only
/// when it carries a value: an empty assignment (`trigger =`) or the explicit
/// `unpatched` placeholder states the absence of an input, not evidence of one.
/// Returns `false` when the table has no row for `(circuit, required)`.
fn satisfied_by_alternative(circuit: &str, required: &str, entries: &[IniEntry]) -> bool {
    ALTERNATIVE_INPUTS
        .iter()
        .find(|row| row.circuit == circuit && row.required == required)
        .is_some_and(|row| {
            entries.iter().any(|entry| {
                row.alternatives.contains(&entry.key.as_str()) && entry_has_value(&entry.value)
            })
        })
}

/// `unpatched` is the schema's marker for "nothing here", so it reads as
/// absence next to an actually patched input.
fn entry_has_value(value: &str) -> bool {
    let trimmed = value.trim();
    !trimmed.is_empty() && !trimmed.eq_ignore_ascii_case("unpatched")
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
            if is_cable_definition(schema, &section.name, &entry.key, &entry.value) {
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
            let is_def = is_cable_definition(schema, &section.name, &entry.key, &entry.value);
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

            // 3b. expression grammar: DROID evaluates `A * B + C` only, so a
            // bracketed or chained calculation is an invalid parameter value
            // and the firmware refuses the whole patch.
            if let Some(problem) = expression_problem(&entry.value) {
                issues.push(ValidationIssue {
                    span: entry.value_span,
                    severity: Severity::Error,
                    code: "invalid_expression".to_string(),
                    message: format!(
                        "Invalid value \"{}\" for \"{}\" in [{}]: {problem}",
                        entry.value.trim(),
                        entry.key,
                        section.name
                    ),
                });
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
            let is_def = is_cable_definition(schema, &section.name, &entry.key, &entry.value);
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
                    // Ranged params (button1 ... button32, mixer input1 ...
                    // input8, sequencer pitch1 ... pitch8) are skipped: leaving
                    // unused members unpatched is normal practice, and the
                    // firmware does not require them. droid-lsp's diagnostics.ts
                    // skips them for the same reason; expanding them here turned
                    // one optional group into six or eight warnings per section.
                    let is_ranged = input.prefix.is_some() && input.count.is_some();
                    if input.essential == 2 && !is_ranged {
                        let expanded = expand_param_names(
                            input.prefix.as_ref(),
                            input.count,
                            input.start_at,
                            &input.name,
                        );
                        for name in expanded {
                            if !seen.contains_key(&name) {
                                // A documented alternative present in this same
                                // section satisfies the param (see
                                // ALTERNATIVE_INPUTS). A different required
                                // param of the circuit is not excused: the table
                                // is keyed by the exact param name.
                                if satisfied_by_alternative(
                                    &lower,
                                    &name,
                                    &section.detailed_entries,
                                ) {
                                    continue;
                                }
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

    /// `missing_required` findings as the quoted parameter names, so the
    /// alternatives tests assert on *which* param is reported rather than on
    /// the message wording.
    fn missing_required_names(ini: &str) -> Vec<String> {
        validate(ini)
            .into_iter()
            .filter(|i| i.code == "missing_required")
            .filter_map(|i| i.message.split('"').nth(1).map(str::to_string))
            .collect()
    }

    fn assert_missing_required(ini: &str, expected: &[&str]) {
        let mut got = missing_required_names(ini);
        got.sort();
        let mut want: Vec<String> = expected.iter().map(|s| (*s).to_string()).collect();
        want.sort();
        assert_eq!(got, want, "missing_required mismatch for:\n{ini}");
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
    fn ranged_required_params_are_not_reported_missing() {
        // Ranged params are optional in practice (mixer input1 ... input8,
        // sequencer pitch1 ... pitch8, button1 ... button32): leaving members
        // unpatched must not produce a warning per member. droid-lsp's
        // diagnostics.ts skips them; this test pins that parity.
        let schema = load_schema();
        let mut found: Option<(String, String)> = None;
        for (name, def) in &schema.circuits {
            for inp in &def.inputs {
                if inp.essential == 2 {
                    if let (Some(prefix), Some(_)) = (inp.prefix.as_ref(), inp.count) {
                        let start = inp.start_at.unwrap_or(1);
                        found = Some((name.clone(), format!("{prefix}{start}")));
                        break;
                    }
                }
            }
            if found.is_some() {
                break;
            }
        }
        let (circ, first_expanded) = found.expect("a ranged essential param exists");
        // Discovery and validation share one schema instance: a parallel test
        // may swap the process-global schema (schema::init with plugin
        // fixtures), and then a second load_schema() could look at another one.
        let expected = schema.circuits[&circ]
            .inputs
            .iter()
            .any(|i| i.essential == 2 && i.prefix.is_some() && i.count.is_some());
        assert!(expected);
        let patch = Patch::from_ini_str(&format!("[p2b8]\n[{circ}]\n"), String::from("test"))
            .expect("parses");
        let misses: Vec<_> = validate_patch(&patch, schema)
            .into_iter()
            .filter(|i| i.code == "missing_required")
            .collect();
        assert!(
            !misses.iter().any(|m| m.message.contains(&first_expanded)),
            "ranged {first_expanded} of [{circ}] must not be reported missing: {misses:?}"
        );
    }

    #[test]
    fn singular_required_param_is_still_reported_missing() {
        // A group with one member is a plain required parameter — skipping
        // ranged groups must not silence it.
        let schema = load_schema();
        let (circ, param) = schema
            .circuits
            .iter()
            .find_map(|(name, def)| {
                def.inputs
                    .iter()
                    .find(|i| i.essential == 2 && i.prefix.is_none() && i.count.is_none())
                    .map(|i| (name.clone(), i.name.clone()))
            })
            .expect("a singular essential param exists");
        let patch = Patch::from_ini_str(&format!("[p2b8]\n[{circ}]\n"), String::from("test"))
            .expect("parses");
        let misses: Vec<_> = validate_patch(&patch, schema)
            .into_iter()
            .filter(|i| i.code == "missing_required")
            .collect();
        assert!(
            misses.iter().any(|m| m.message.contains(&param)),
            "[{circ}] must still require its {param}: {misses:?}"
        );
    }

    // ── documented alternatives (an essential input satisfied by another) ────

    #[test]
    fn lfo_hz_is_satisfied_by_taptempo_or_rate() {
        // Manual: `hz` cannot be used together with `taptempo`, and both
        // combine with `rate`; either input is positive evidence that the
        // oscillator has a frequency source.
        for ini in [
            "[p2b8]\n[lfo]\ntaptempo = G1\n",
            "[p2b8]\n[lfo]\nrate = I1\n",
        ] {
            assert_missing_required(ini, &[]);
        }
    }

    #[test]
    fn lfo_hz_is_reported_without_an_alternative() {
        assert_missing_required("[p2b8]\n[lfo]\nlevel = 1\n", &["hz"]);
    }

    #[test]
    fn gatetool_inputgate_is_satisfied_by_inputtrigger_or_inputedge() {
        // Manual: "gatetool has three different types of inputs. Usually you
        // would patch only one of these" — gate, trigger and edge.
        for ini in [
            "[p2b8]\n[gatetool]\ninputtrigger = G1\n",
            "[p2b8]\n[gatetool]\ninputedge = G1\n",
        ] {
            assert_missing_required(ini, &[]);
        }
    }

    #[test]
    fn gatetool_inputgate_is_reported_without_an_alternative() {
        assert_missing_required("[p2b8]\n[gatetool]\ngatelength = 1\n", &["inputgate"]);
    }

    #[test]
    fn contour_gate_is_satisfied_by_trigger() {
        // Manual: "two alternative inputs: gate and trigger" to start the
        // envelope.
        assert_missing_required("[p2b8]\n[contour]\ntrigger = G1\n", &[]);
    }

    #[test]
    fn empty_or_unpatched_alternative_does_not_satisfy_the_param() {
        // A written key is not evidence on its own: the entries carry these
        // keys, so the rule has to read the value. `unpatched` is the schema's
        // own marker for "no input here".
        for ini in [
            "[p2b8]\n[contour]\ntrigger =\n",
            "[p2b8]\n[contour]\ntrigger =   \n",
            "[p2b8]\n[contour]\ntrigger = unpatched\n",
            "[p2b8]\n[contour]\ntrigger = UNPATCHED\n",
        ] {
            assert_missing_required(ini, &["gate"]);
        }
    }

    #[test]
    fn alternative_keys_match_case_insensitively() {
        // `detailed_entries` stores lowercased keys, so an upper-case write in
        // the patch still counts.
        assert_missing_required("[p2b8]\n[contour]\nTRIGGER = G1\n", &[]);
    }

    #[test]
    fn contour_gate_is_reported_without_trigger() {
        assert_missing_required("[p2b8]\n[contour]\nattack = 1\n", &["gate"]);
    }

    #[test]
    fn compare_ifequal_is_satisfied_by_any_documented_branch() {
        // Manual: "It is allowed to omit any of the inputs ifless, ifequal,
        // ifgreater or else" — one branch is enough. `input` and `compare`
        // are separate required inputs and are set here so only `ifequal` is
        // in question.
        for branch in ["ifgreater = I1", "ifless = I1", "else = 1"] {
            let ini = format!("[p2b8]\n[compare]\ninput = 0\ncompare = 0\n{branch}\n");
            assert_missing_required(&ini, &[]);
        }
    }

    #[test]
    fn compare_ifequal_is_reported_without_a_branch() {
        assert_missing_required("[p2b8]\n[compare]\ninput = 0\ncompare = 0\n", &["ifequal"]);
    }

    #[test]
    fn alternative_in_another_section_does_not_satisfy_the_param() {
        // Same circuit, same patch: only the second `[lfo]` lacks a frequency
        // source. `seen` is per section, so the first section's `taptempo`
        // must not silence the second section's warning.
        let ini = "[p2b8]\n[lfo]\ntaptempo = G1\n[lfo]\nlevel = 1\n";
        let issues = validate(ini);
        let misses: Vec<&ValidationIssue> = issues
            .iter()
            .filter(|i| i.code == "missing_required")
            .collect();
        assert_eq!(misses.len(), 1, "issues: {issues:?}");
        assert!(
            misses[0].message.contains("\"hz\""),
            "{}",
            misses[0].message
        );
        // header of the second `[lfo]` (0-based line 3)
        assert_eq!(misses[0].span.line, 3);
    }

    #[test]
    fn alternative_does_not_excuse_a_different_required_param() {
        // `ifgreater` satisfies `ifequal` only. `input` is a separate
        // `essential: 2` input of [compare] and stays reported.
        assert_missing_required(
            "[p2b8]\n[compare]\ncompare = 0\nifgreater = I1\n",
            &["input"],
        );
    }

    #[test]
    fn alternative_inputs_table_resolves_against_the_embedded_schema() {
        // Discovery and validation share one schema instance: a parallel test
        // may install a plugin-merged schema (`schema::init`), and the table
        // must be checked against exactly the one the validator uses.
        let _guard = crate::schema::TEST_CACHE_LOCK
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let schema = load_schema();

        let mut rows: HashSet<(&str, &str)> = HashSet::new();
        for row in ALTERNATIVE_INPUTS {
            assert!(
                rows.insert((row.circuit, row.required)),
                "duplicate ALTERNATIVE_INPUTS row for [{}.{}]",
                row.circuit,
                row.required
            );
            let def = schema
                .circuits
                .get(row.circuit)
                .unwrap_or_else(|| panic!("[{}] is not an embedded circuit", row.circuit));
            let required = def
                .inputs
                .iter()
                .find(|p| p.name == row.required)
                .unwrap_or_else(|| {
                    panic!(
                        "{} is not a declared input of [{}]",
                        row.required, row.circuit
                    )
                });
            // The row only means anything for the arm the validator guards.
            assert_eq!(
                required.essential, 2,
                "[{}.{}] is not an essential: 2 input",
                row.circuit, row.required
            );
            assert!(
                required.prefix.is_none() && required.count.is_none(),
                "[{}.{}] is ranged; the validator skips it before consulting the table",
                row.circuit,
                row.required
            );

            for alt in row.alternatives {
                assert!(
                    schema
                        .get_param_names(row.circuit)
                        .is_some_and(|names| names.iter().any(|n| n == alt)),
                    "{alt} is not a declared param of [{}]",
                    row.circuit
                );
                // No mutual-exclusion cycle: an alternative that is itself
                // `essential: 2` must have its own row, otherwise two params
                // could excuse each other while both are absent.
                let alt_is_essential = def
                    .inputs
                    .iter()
                    .any(|p| p.name == *alt && p.essential == 2);
                if alt_is_essential {
                    assert!(
                        ALTERNATIVE_INPUTS
                            .iter()
                            .any(|r| r.circuit == row.circuit && r.required == *alt),
                        "[{}.{}] is essential: 2 but is listed only as an alternative; give it its own row",
                        row.circuit,
                        alt
                    );
                }
            }
        }
    }

    #[test]
    fn essential_alternatives_fixture_keeps_only_the_control_warning() {
        // The fixture pins the alternatives rule end to end. Only
        // `missing_required` is asserted: cable, jack, expression and RAM
        // findings are other checks' business (covered by the rest of this
        // module and the regression fixture matrix).
        //
        // Sections are identified by what they contain, not by order: a
        // section "documents an alternative" when it writes one of its
        // circuit's `ALTERNATIVE_INPUTS` names, otherwise it is a control.
        let path = PathBuf::from("fixtures/validation/essential_alternatives.ini");
        let patch = Patch::from_ini_file(&path)
            .unwrap_or_else(|e| panic!("{} must parse: {e}", path.display()));
        let schema = load_schema();
        let misses: Vec<ValidationIssue> = validate_patch(&patch, schema)
            .into_iter()
            .filter(|i| i.code == "missing_required")
            .collect();

        let mut documented = 0usize;
        let mut controls = 0usize;
        for section in &patch.sections {
            let Some(row) = ALTERNATIVE_INPUTS
                .iter()
                .find(|r| r.circuit == section.name.to_lowercase())
            else {
                continue;
            };
            let keys: Vec<&str> = section
                .detailed_entries
                .iter()
                .map(|e| e.key.as_str())
                .collect();
            let here: Vec<&ValidationIssue> = misses
                .iter()
                .filter(|m| m.span.line == section.header_span.line)
                .collect();
            if row.alternatives.iter().any(|alt| keys.contains(alt)) {
                documented += 1;
                assert!(
                    here.is_empty(),
                    "[{}] writes a documented alternative but still reports {:?}",
                    section.name,
                    here.iter().map(|i| &i.message).collect::<Vec<_>>()
                );
            } else {
                controls += 1;
                assert_eq!(
                    here.len(),
                    1,
                    "control [{}] must produce exactly one missing_required, got {:?}",
                    section.name,
                    here.iter().map(|i| &i.message).collect::<Vec<_>>()
                );
                assert!(
                    here[0].message.contains(&format!("\"{}\"", row.required)),
                    "control [{}] must report {}: {}",
                    section.name,
                    row.required,
                    here[0].message
                );
            }
        }
        assert_eq!(
            documented,
            ALTERNATIVE_INPUTS.len(),
            "fixture must cover every table row"
        );
        assert_eq!(
            controls,
            ALTERNATIVE_INPUTS.len(),
            "fixture must carry one control per table row"
        );
        assert_eq!(misses.len(), controls, "missing_required: {misses:?}");
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
    fn schema_output_params_define_cables() {
        // DROID defines a cable by putting `_NAME` where an output goes, and
        // most circuits name that output something other than `output`.
        // Accepting only the literal key reported every such cable as
        // undefined (on patches_remote/_droid_mpfs5drum.ini that was ~950 of
        // 976 warnings while the cable index knew every producer).
        for (label, ini) in [
            (
                "switch output2",
                "[p2b8]\n[switch]\ninput1 = I1\noffset = 1\noutput2 = _SEL\n[copy]\ninput = _SEL\noutput = O1\n",
            ),
            (
                "logic or",
                "[p2b8]\n[logic]\ninput1 = I1\nor = _OR\n[copy]\ninput = _OR\noutput = O1\n",
            ),
            (
                "lfo square",
                "[p2b8]\n[lfo]\nsquare = _SQ\n[copy]\ninput = _SQ\noutput = O1\n",
            ),
            (
                "slew linear",
                "[p2b8]\n[slew]\ninput = I1\nlinear = _LIN\n[copy]\ninput = _LIN\noutput = O1\n",
            ),
        ] {
            let issues = validate(ini);
            assert!(
                !has_code(&issues, "undefined_cable"),
                "{label}: unexpected undefined_cable in {:?}",
                issues.iter().map(|i| &i.message).collect::<Vec<_>>()
            );
        }
    }

    #[test]
    fn non_output_param_never_defines_a_cable() {
        // `input = _X` is a sink. The schema check must not widen definitions
        // to every parameter that happens to hold a bare cable name.
        let issues = validate("[p2b8]\n[copy]\ninput = _ORPHAN\noutput = O1\n");
        assert!(has_code(&issues, "undefined_cable"));
    }

    #[test]
    fn expression_valued_output_does_not_define_a_cable() {
        // The name has to *replace* the output value. `output = _X * 2` reads
        // X and leaves the output unnamed, so X stays undefined here -- the
        // same rule `patch::collect_cable_index` applies.
        let issues = validate(
            "[p2b8]\n[copy]\ninput = I1\noutput = _EXP * 2\n[copy]\ninput = _EXP\noutput = O1\n",
        );
        assert!(has_code(&issues, "undefined_cable"));
    }

    #[test]
    fn duplicate_and_unused_see_schema_outputs() {
        // duplicate_cable
        let dup = validate(
            "[p2b8]\n[switch]\ninput1 = I1\noffset = 1\noutput2 = _DUP2\n[logic]\ninput1 = I2\nor = _DUP2\n[copy]\ninput = _DUP2\noutput = O1\n",
        );
        assert!(has_code(&dup, "duplicate_cable"), "{dup:?}");

        // unused_cable: defined by a schema output, never read
        let unused = validate("[p2b8]\n[switch]\ninput1 = I1\noffset = 1\noutput2 = _NEVER_READ\n");
        assert!(has_code(&unused, "unused_cable"), "{unused:?}");
        assert!(!has_code(&unused, "undefined_cable"), "{unused:?}");
    }

    #[test]
    fn undefined_set_matches_the_cable_index() {
        // The warning list must agree with patch::cable_index, which is what
        // the signal-flow graph draws: a cable the index knows as produced is
        // never reported undefined, and vice versa.
        let ini = r"[p2b8]
[switch]
input1 = I1
offset = 1
output2 = _FROM_SWITCH
[logic]
input1 = _FROM_SWITCH
or = _FROM_LOGIC
[lfo]
square = _FROM_LFO
[copy]
input = _FROM_LOGIC
output = O1
[copy]
input = _REALLY_OPEN
output = _STILL_OPEN * 2
";
        let patch = Patch::from_ini_str(ini, String::from("test")).unwrap();
        let issues = validate_patch(&patch, load_schema());
        let mut warned: Vec<String> = issues
            .iter()
            .filter(|i| i.code == "undefined_cable")
            .filter_map(|i| {
                let start = i.message.find('"')? + 1;
                let end = start + i.message[start..].find('"')?;
                Some(i.message[start..end].to_string())
            })
            .collect();
        warned.sort();
        warned.dedup();
        let mut open: Vec<String> = patch
            .cable_index
            .iter()
            .filter(|(_, e)| e.sources.is_empty())
            .map(|(name, _)| name.clone())
            .collect();
        open.sort();
        assert_eq!(warned, open);
        assert_eq!(
            open,
            vec!["_REALLY_OPEN".to_string(), "_STILL_OPEN".to_string()]
        );
    }

    #[test]
    fn expression_grammar_accepts_droid_shapes() {
        // Manual §5.7: A * B + C, with the documented shorthands:
        // -x for -1 * x, /number for * (1/number), off/on, V and % suffixes.
        for ok in [
            "0.5",
            "1V",
            "45 %",
            "off",
            "P1.1",
            "_CLOCK",
            "P1.1 * 0.5",
            "I1 * P1.1",
            "P1.1 * 0.8 + 0.2",
            "0.05 * P1.2 + P1.1",
            "_CLOCK_BPM / 15",
            "I2 / 120 + _MENU_DEGREE",
            "_CLOCK_BEND_RAW * 0.12 - 0.06",
            "-P1.1",
            "_CLOCK_RUNNING * -1 + 1",
            "-1 * _X + 1",
            "_A + _B",
            "I1 - I2",
            "\"CLK BEND (fader)\"",
        ] {
            assert_eq!(expression_problem(ok), None, "{ok} must be accepted");
        }
    }

    #[test]
    fn expression_grammar_rejects_invalid_calculations() {
        // The firmware reads the value as A * B + C and rejects everything
        // else as "invalid parameter value" — the whole patch then fails.
        for (bad, why) in [
            ("(_CLOCK_BEND_RAW - 0.5) * 0.12", "brackets"),
            ("(I1 + I2) * 0.5", "brackets"),
            ("_A * _B * _C", "multiplication"),
            ("_A * _B * 2 + 1", "multiplication"),
            ("_A / _B", "divide"),
            ("_A + _B + _C", "offset"),
            ("_A * ", "operator"),
        ] {
            let problem =
                expression_problem(bad).unwrap_or_else(|| panic!("{bad} must be rejected"));
            assert!(
                problem.contains(why),
                "{bad}: expected a reason containing {why:?}, got {problem:?}"
            );
        }
    }

    #[test]
    fn invalid_expression_is_an_error_on_the_value_span() {
        let issues = validate("[p2b8]\n[copy]\ninput = (_SOMETHING - 0.5) * 0.12\noutput = O1\n");
        let issue = issues
            .iter()
            .find(|i| i.code == "invalid_expression")
            .expect("invalid_expression");
        assert_eq!(issue.severity, Severity::Error);
        assert_eq!(issue.span.line, 2);
        assert!(issue.message.contains("brackets"), "{}", issue.message);
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
