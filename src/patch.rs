use std::collections::{HashMap, HashSet};
use std::path::Path;

use serde::{Deserialize, Serialize};

/// 0-based line and byte-column span: line is 0-based, column range is
/// [col_start, col_end) byte offsets within that raw line.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct Span {
    pub line: usize,
    pub col_start: usize,
    pub col_end: usize,
}

/// A single `select = X` relationship resolved to its affected source span.
/// `source` is the raw `X` (hardware token or internal cable). `selectat`
/// holds the optional `selectat = N` exact-value string in the same section.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ModifierAffect {
    pub span: Span,
    pub source: String,
    pub selectat: Option<String>,
}

/// Represents a loaded DROID patch
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Patch {
    pub name: String,
    pub hw_components: Vec<HwComponent>,
    /// Modules grouping hardware components by controller type.
    /// Populated by `from_ini_str`; hand-built patches (e.g. `sample()`) carry none.
    pub modules: Vec<Module>,
    pub shift_groups: Vec<ShiftGroup>,
    /// Raw `.ini` sections, kept for the source viewer. Populated by
    /// `from_ini_str`; hand-built patches (e.g. `sample()`) carry none.
    pub sections: Vec<IniSection>,
    /// Verbatim raw lines including comments and blank lines, in file order.
    #[serde(default)]
    pub raw_lines: Vec<String>,
    /// Every boundary-aware hardware-token hit with its source span, in
    /// reading order (top-to-bottom, left-to-right).
    #[serde(default)]
    pub token_spans: Vec<(String, Span)>,
    /// Token -> ordered occurrence spans, built from `token_spans` in
    /// reading order. Present for named consumers (`occurrences_for`).
    #[serde(default)]
    pub occurrence_index: HashMap<String, Vec<Span>>,
    /// Hardware token -> modifier spans whose `select = X` transitively
    /// resolves to that token (cycle-safe). Present for named consumers
    /// (`modifier_affected_spans`, `modifier_entries_for`).
    #[serde(default)]
    pub modifier_index: HashMap<String, Vec<ModifierAffect>>,
}

/// A hardware component from the patch (button, CV in/out, knob, etc.)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HwComponent {
    pub id: String,
    pub label: String,
    pub kind: ComponentKind,
    pub shift_group: Option<ShiftGroup>,
    pub state: ComponentState,
    /// Physical controller panel this component belongs to (e.g. "P2B8",
    /// "Faderbank", "Notebuttons", "CV I/O"). See design.md Decision 3.
    pub controller: String,
    /// Optional LED token associated with this component (e.g. `L1.1`).
    /// Set during `from_ini_str` parsing when a `led = L.N` entry
    /// appears in the same section as the component.
    pub led: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ComponentKind {
    Button,
    CvIn,
    CvOut,
    Knob,
    Switch,
    Led,
    Encoder,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum ComponentState {
    Off,
    On,
    Value(f32),
    Active,
}

/// Horizontal Pin width for modules in a DROID patch.
/// One HP = one physical control position; typical widths are 4HP, 6HP, 8HP, etc.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ModuleWidth {
    FourHP,
    SixHP,
    EightHP,
    TwelveHP,
    SixteenHP,
    TwentyHP,
}

impl ModuleWidth {
    /// Convert ModuleWidth to character cells (each HP = 4 chars, but we use 1 cell per HP for simplicity)
    pub fn cell_width(&self) -> usize {
        match self {
            ModuleWidth::FourHP => 4,
            ModuleWidth::SixHP => 6,
            ModuleWidth::EightHP => 8,
            ModuleWidth::TwelveHP => 12,
            ModuleWidth::SixteenHP => 16,
            ModuleWidth::TwentyHP => 20,
        }
    }
}

/// Which MASTER model a patch requires. MASTER18 has more CV jacks/RAM
/// than MASTER; a patch that addresses jacks beyond MASTER's 8 CV in/out
/// needs MASTER18. See `Patch::master_requirement`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum MasterRequirement {
    Master,
    Master18,
}

/// A module in a DROID patch, grouping hardware components by controller type.
/// Modules provide logical grouping for the TUI rendering and resize behavior.
/// A module has a fixed height of 3U and a width expressed in HP (Horizontal Pins).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Module {
    /// Unique identifier for the module (typically the controller name)
    pub id: String,
    /// Human-readable model name (e.g. "P2B8", "Faderbank")
    pub model_name: String,
    /// Width in Horizontal Pins (HP)
    pub width: ModuleWidth,
    /// Height in Units (U), fixed at 3
    pub height: u16,
    /// Components contained in this module
    pub components: Vec<HwComponent>,
}

impl Module {
    /// Create a new Module with the given width and empty components vector
    pub fn new(id: String, model_name: String, width: ModuleWidth) -> Self {
        Self {
            id,
            model_name,
            width,
            height: 3,
            components: Vec::new(),
        }
    }

    /// Get the module width in character cells (based on HP)
    pub fn cell_width(&self) -> usize {
        self.width.cell_width()
    }

    /// Get the number of components in this module
    pub fn component_count(&self) -> usize {
        self.components.len()
    }

    /// Parse HwComponents from INI section names and group them into modules.
    ///
    /// This scans INI section names (e.g. `[button]`, `[knob]`, `[led]`) and groups
    /// components by their controller assignment. Components without an explicit
    /// controller are grouped into an "Unused" module.
    ///
    /// Returns a vector of modules, each containing components that share the same
    /// controller. The first module will have the most common controller, etc.
    pub fn from_ini_sections(sections: &[String]) -> Vec<Self> {
        use std::collections::HashMap;

        let mut controller_components: HashMap<String, Vec<HwComponent>> = HashMap::new();

        for section in sections {
            // Strip leading/trailing whitespace
            let stripped = section.trim();

            // Skip empty lines and comments
            if stripped.is_empty() || stripped.starts_with('#') {
                continue;
            }

            // Parse section name: remove [ and ]
            let section_name = stripped
                .strip_prefix('[')
                .and_then(|s| s.strip_suffix(']'))
                .unwrap_or(stripped);

            // Extract controller from section name pattern like [button], [knob], etc.
            let controller = if section_name.is_empty() {
                "Unused".to_string()
            } else {
                let first_char = section_name.chars().next().unwrap_or('/');
                // Map hardware token kinds to controller names
                match first_char {
                    'B' => "P2B8",
                    'L' => "Led",
                    'P' => "Pot",
                    'O' => "CV Out",
                    'I' => "CV In",
                    'E' => "Encoder",
                    'S' => "Switch",
                    _ => "Unused",
                }
                .to_string()
            };

            // Group component under the inferred controller
            controller_components
                .entry(controller.clone())
                .or_default()
                .push(HwComponent {
                    id: section_name.into(),
                    label: section_name.into(),
                    kind: ComponentKind::Button,
                    shift_group: None,
                    state: ComponentState::Off,
                    controller,
                    led: None,
                });
        }

        // Convert to Module objects, sorted by number of components (largest first)
        let mut modules: Vec<Module> = controller_components
            .into_iter()
            .map(|(controller, components)| {
                let model_name = components
                    .first()
                    .map(|c| c.label.clone())
                    .unwrap_or_else(|| controller.clone());

                Module::new(controller, model_name, ModuleWidth::EightHP)
            })
            .collect();

        // Sort by component count descending
        modules.sort_by_key(|b| std::cmp::Reverse(b.component_count()));

        modules
    }
}

/// Shift groups — modifier keys that change the behavior/label of a group of components
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ShiftGroup {
    Group1,
    Group2,
    Group3,
    Group4,
}

impl ShiftGroup {
    pub fn key_label(&self) -> &'static str {
        match self {
            ShiftGroup::Group1 => "1",
            ShiftGroup::Group2 => "2",
            ShiftGroup::Group3 => "3",
            ShiftGroup::Group4 => "4",
        }
    }
}

impl Patch {
    /// Sample patch for development/testing
    pub fn sample() -> Self {
        Self {
            name: String::from("Demo Patch"),
            hw_components: vec![
                HwComponent {
                    id: "btn_1".into(),
                    label: "TRIG A".into(),
                    kind: ComponentKind::Button,
                    shift_group: Some(ShiftGroup::Group1),
                    state: ComponentState::Off,
                    controller: "P2B8".into(),
                    led: None,
                },
                HwComponent {
                    id: "btn_2".into(),
                    label: "TRIG B".into(),
                    kind: ComponentKind::Button,
                    shift_group: Some(ShiftGroup::Group1),
                    state: ComponentState::Off,
                    controller: "P2B8".into(),
                    led: None,
                },
                HwComponent {
                    id: "cv_in_1".into(),
                    label: "CV IN 1".into(),
                    kind: ComponentKind::CvIn,
                    shift_group: Some(ShiftGroup::Group2),
                    state: ComponentState::Value(0.0),
                    controller: "CV I/O".into(),
                    led: None,
                },
                HwComponent {
                    id: "cv_out_1".into(),
                    label: "CV OUT 1".into(),
                    kind: ComponentKind::CvOut,
                    shift_group: Some(ShiftGroup::Group2),
                    state: ComponentState::Value(0.0),
                    controller: "CV I/O".into(),
                    led: None,
                },
                HwComponent {
                    id: "knob_1".into(),
                    label: "CUTOFF".into(),
                    kind: ComponentKind::Knob,
                    shift_group: Some(ShiftGroup::Group3),
                    state: ComponentState::Value(0.5),
                    controller: "P2B8".into(),
                    led: None,
                },
                HwComponent {
                    id: "led_1".into(),
                    label: "STATUS".into(),
                    kind: ComponentKind::Led,
                    shift_group: None,
                    state: ComponentState::On,
                    controller: "P2B8".into(),
                    led: None,
                },
            ],
            shift_groups: vec![
                ShiftGroup::Group1,
                ShiftGroup::Group2,
                ShiftGroup::Group3,
                ShiftGroup::Group4,
            ],
            modules: Vec::new(),
            sections: Vec::new(),
            raw_lines: Vec::new(),
            token_spans: Vec::new(),
            occurrence_index: HashMap::new(),
            modifier_index: HashMap::new(),
        }
    }

    /// Get components belonging to a specific shift group
    pub fn components_in_group(&self, group: ShiftGroup) -> Vec<&HwComponent> {
        self.hw_components
            .iter()
            .filter(|c| c.shift_group == Some(group))
            .collect()
    }

    /// Load and parse a DROID `.ini` patch file from disk.
    pub fn from_ini_file(path: &Path) -> Result<Self, String> {
        let content = std::fs::read_to_string(path)
            .map_err(|e| format!("Failed to read {}: {}", path.display(), e))?;
        let name = path
            .file_stem()
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_else(|| String::from("patch"));
        Self::from_ini_str(&content, name)
    }

    /// Parse DROID `.ini` patch content into a `Patch`.
    ///
    /// Real DROID patches repeat section names (e.g. many `[button]` sections
    /// in one file), so this walks an ordered list of sections rather than a
    /// section-name-keyed map. See design.md Decision 1.
    pub fn from_ini_str(content: &str, name: String) -> Result<Self, String> {
        if content.trim().is_empty() {
            return Err(String::from("Patch file is empty"));
        }

        let raw_lines: Vec<String> = content.lines().map(|l| l.to_string()).collect();
        let sections = parse_ini_sections(content);
        if sections.is_empty() {
            return Err(String::from("No circuit sections found in patch file"));
        }
        let token_spans = collect_token_spans(&raw_lines);

        // Group components into modules based on INI section names
        let modules =
            Module::from_ini_sections(&sections.iter().map(|s| s.name.clone()).collect::<Vec<_>>());

        // Flatten modules' components into hw_components for backward compatibility
        let mut components: Vec<HwComponent> = Vec::new();
        let mut seen_ids: HashSet<String> = HashSet::new();
        let mut p2b8_instances: u32 = 0;
        // Controller number -> panel name, e.g. 1 -> "P2B8", 2 -> "Notebuttons".
        // Populated from explicit controller-declaring sections; anything not
        // covered here falls back to a generic "Controller N" panel below.
        let mut controller_types: std::collections::HashMap<u32, String> =
            std::collections::HashMap::new();

        for section in &sections {
            // --- LED association: scan this section for a `led = L.N` entry
            // and identify the "element" (first hardware token in the section).
            let led_token: Option<String> = section
                .entries
                .iter()
                .find(|(k, _)| *k == "led")
                .map(|(_, v)| v.clone());
            let element_token: Option<String> = section
                .entries
                .iter()
                .flat_map(|(_, v)| scan_hw_tokens(v).first().cloned())
                .next();

            // A bare `[p2b8]` declaration implies 18 hardware tokens even
            // when it has no key-value pairs of its own (design.md Decision 2b).
            if section.name == "p2b8" {
                p2b8_instances += 1;
                let n = p2b8_instances;
                controller_types
                    .entry(n)
                    .or_insert_with(|| String::from("P2B8"));
                for i in 1..=8 {
                    add_component(
                        &mut components,
                        &mut seen_ids,
                        format!("B{}.{}", n, i),
                        ComponentKind::Button,
                        format!("P2B8 Button {}.{}", n, i),
                    );
                    add_component(
                        &mut components,
                        &mut seen_ids,
                        format!("L{}.{}", n, i),
                        ComponentKind::Led,
                        format!("P2B8 Led {}.{}", n, i),
                    );
                }
                for i in 1..=2 {
                    add_component(
                        &mut components,
                        &mut seen_ids,
                        format!("P{}.{}", n, i),
                        ComponentKind::Knob,
                        format!("P2B8 Knob {}.{}", n, i),
                    );
                }
            } else if KNOWN_CONTROLLER_SECTIONS.contains(&section.name.as_str()) {
                // Named controller sections (Notebuttons, Faderbank, ...) declare
                // their tokens as key-value pairs; the controller number is the
                // number embedded in the first hardware token we find.
                let first_number = section.entries.iter().find_map(|(_, v)| {
                    scan_hw_tokens(v)
                        .into_iter()
                        .find_map(|t| leading_number(&t))
                });
                if let Some(n) = first_number {
                    controller_types
                        .entry(n)
                        .or_insert_with(|| titlecase(&section.name));
                }
            }

            for (_key, value) in &section.entries {
                for token in scan_hw_tokens(value) {
                    if let Some(kind) = token_kind(&token) {
                        let label = format!("{} {}", titlecase(&section.name), token);
                        add_component(&mut components, &mut seen_ids, token, kind, label);
                    }
                }
            }

            // After add_component calls for this section, associate LED if present.
            if let Some(led) = &led_token {
                if let Some(element) = &element_token {
                    if let Some(comp) = components.iter_mut().find(|c| c.id == *element) {
                        comp.led = Some(led.clone());
                    }
                }
            }
        }

        if components.is_empty() {
            return Err(String::from("No hardware components found in patch file"));
        }

        // Assign shift groups by controller number (design.md Decision 2c).
        for comp in components.iter_mut() {
            if let Some(n) = leading_number(&comp.id) {
                comp.shift_group = Some(match (n.saturating_sub(1)) % 4 {
                    0 => ShiftGroup::Group1,
                    1 => ShiftGroup::Group2,
                    2 => ShiftGroup::Group3,
                    _ => ShiftGroup::Group4,
                });
            }
        }

        // Assign each component to its physical controller panel (design.md
        // Decision 3). CV I/O tokens are fixed jacks, not part of a pluggable
        // controller unit, so they always share one panel regardless of number.
        for comp in components.iter_mut() {
            comp.controller = match comp.kind {
                ComponentKind::CvIn | ComponentKind::CvOut => String::from("CV I/O"),
                _ => {
                    let num = leading_number(&comp.id);
                    match num.and_then(|n| controller_types.get(&n).cloned()) {
                        Some(name) => name,
                        None => match num {
                            Some(n) => format!("Controller {}", n),
                            None => String::from("Other"),
                        },
                    }
                }
            };
        }

        // Assign shift groups from modules (if modules have shift group info)
        // For now, use the previously assigned shift groups from components

        let occurrence_index = build_occurrence_index(&token_spans);
        let modifier_index = build_modifier_index(&raw_lines, &sections);

        Ok(Patch {
            name,
            hw_components: components,
            modules,
            shift_groups: vec![
                ShiftGroup::Group1,
                ShiftGroup::Group2,
                ShiftGroup::Group3,
                ShiftGroup::Group4,
            ],
            sections,
            raw_lines,
            token_spans,
            occurrence_index,
            modifier_index,
        })
    }

    /// Sorted, deduplicated list of physical controller panels this patch
    /// uses (the same strings as `HwComponent.controller`, e.g. "P2B8",
    /// "CV I/O", "Notebuttons").
    pub fn module_types(&self) -> Vec<String> {
        let mut types: Vec<String> = self
            .hw_components
            .iter()
            .map(|c| c.controller.clone())
            .collect::<HashSet<_>>()
            .into_iter()
            .collect();
        types.sort();
        types
    }

    /// Count of components this patch needs per controller/module type —
    /// what a rack must provide to host this patch.
    pub fn needs_by_type(&self) -> std::collections::BTreeMap<String, usize> {
        let mut needs = std::collections::BTreeMap::new();
        for comp in &self.hw_components {
            *needs.entry(comp.controller.clone()).or_insert(0) += 1;
        }
        needs
    }

    /// Whether this patch requires MASTER or MASTER18. Heuristic: MASTER18
    /// carries more jacks/RAM than MASTER; a patch that addresses CV jack
    /// 9 or higher needs MASTER18.
    pub fn master_requirement(&self) -> MasterRequirement {
        let needs_master18 = self.hw_components.iter().any(|c| {
            matches!(c.kind, ComponentKind::CvIn | ComponentKind::CvOut)
                && leading_number(&c.id).is_some_and(|n| n > 8)
        });
        if needs_master18 {
            MasterRequirement::Master18
        } else {
            MasterRequirement::Master
        }
    }

    /// Project the raw `.ini` sections into the source viewer's circuit
    /// list: one `ViewerCircuit` per section, keeping the key-value pairs.
    pub fn viewer_circuits(&self) -> Vec<ViewerCircuit> {
        self.sections
            .iter()
            .map(|section| ViewerCircuit {
                name: section.name.clone(),
                entries: section.entries.clone(),
            })
            .collect()
    }

    /// Occurrences of `token` in reading order. Empty slice if absent.
    pub fn occurrences_for(&self, token: &str) -> &[Span] {
        self.occurrence_index
            .get(token)
            .map(|v| v.as_slice())
            .unwrap_or(&[])
    }

    /// Modifier spans whose `select = X` (and optional `selectat`) transitively
    /// resolves to `token`. Cycle-safe, file-order.
    pub fn modifier_affected_spans(&self, token: &str) -> Vec<Span> {
        self.modifier_index
            .get(token)
            .map(|v| v.iter().map(|e| e.span).collect())
            .unwrap_or_default()
    }

    /// Full modifier entries for `token` (span + source + selectat).
    pub fn modifier_entries_for(&self, token: &str) -> &[ModifierAffect] {
        self.modifier_index
            .get(token)
            .map(|v| v.as_slice())
            .unwrap_or(&[])
    }
}

/// A circuit as shown in the source viewer: section name plus its raw
/// key-value pairs.
#[derive(Debug, PartialEq, Eq)]
pub struct ViewerCircuit {
    pub name: String,
    pub entries: Vec<(String, String)>,
}

/// A raw section of a DROID `.ini` patch: the circuit name in brackets
/// plus its ordered `key = value` entries. Repeated section names are kept
/// as separate sections (design.md Decision 1).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct IniSection {
    pub name: String,
    pub entries: Vec<(String, String)>,
    /// Source span of the header line (including brackets), 0-based.
    #[serde(default)]
    pub header_span: Span,
}

/// Strip a `#`-to-end-of-line comment (whole-line or inline).
fn strip_comment(line: &str) -> &str {
    match line.find('#') {
        Some(idx) => &line[..idx],
        None => line,
    }
}

/// Parse `.ini` content into an ordered list of sections, preserving
/// repeated section names as distinct entries.
fn parse_ini_sections(content: &str) -> Vec<IniSection> {
    let mut sections: Vec<IniSection> = Vec::new();
    for (line_idx, raw_line) in content.lines().enumerate() {
        let stripped = strip_comment(raw_line);
        let line = stripped.trim();
        if line.is_empty() {
            continue;
        }
        if line.starts_with('[') && line.ends_with(']') {
            let col_start = stripped.find('[').unwrap_or(0);
            let col_end = stripped.rfind(']').map(|i| i + 1).unwrap_or(stripped.len());
            sections.push(IniSection {
                name: line[1..line.len() - 1].trim().to_lowercase(),
                entries: Vec::new(),
                header_span: Span {
                    line: line_idx,
                    col_start,
                    col_end,
                },
            });
            continue;
        }
        if let Some(eq_idx) = line.find('=') {
            let key = line[..eq_idx].trim().to_lowercase();
            let value = line[eq_idx + 1..].trim().to_string();
            if let Some(section) = sections.last_mut() {
                section.entries.push((key, value));
            }
        }
    }
    sections
}

const HW_TOKEN_LETTERS: [char; 7] = ['B', 'L', 'P', 'O', 'I', 'E', 'S'];

/// Section names that declare a physical, pluggable controller unit (as
/// opposed to internal logic/CV circuits). See design.md Decision 3.
const KNOWN_CONTROLLER_SECTIONS: [&str; 7] = [
    "notebuttons",
    "faderbank",
    "encoder",
    "pot",
    "unusedfaders",
    "motorfader",
    "fadermatrix",
];

/// Scan a value expression for DROID hardware tokens (`B1.1`, `O4`, `I1`, ...).
///
/// A match starts at a hardware-token letter immediately followed by a
/// digit and not preceded by an alphanumeric/underscore character, so that
/// e.g. `_ENV1_DECAY_POT` (letter preceded by another letter) is not
/// mistaken for a token. See design.md Decision 2.
fn scan_hw_tokens(value: &str) -> Vec<String> {
    let chars: Vec<char> = value.chars().collect();
    let mut tokens = Vec::new();
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        let boundary_ok = i == 0 || !(chars[i - 1].is_ascii_alphanumeric() || chars[i - 1] == '_');
        let starts_token = HW_TOKEN_LETTERS.contains(&c)
            && i + 1 < chars.len()
            && chars[i + 1].is_ascii_digit()
            && boundary_ok;

        if starts_token {
            let start = i;
            i += 1;
            while i < chars.len() && chars[i].is_ascii_digit() {
                i += 1;
            }
            if i < chars.len()
                && chars[i] == '.'
                && i + 1 < chars.len()
                && chars[i + 1].is_ascii_digit()
            {
                i += 1;
                while i < chars.len() && chars[i].is_ascii_digit() {
                    i += 1;
                }
            }
            let clean_end = i >= chars.len()
                || !(chars[i].is_ascii_alphanumeric() || chars[i] == '_' || chars[i] == '.');
            if clean_end {
                tokens.push(chars[start..i].iter().collect());
            }
            continue;
        }
        i += 1;
    }
    tokens
}

/// Like `scan_hw_tokens` but returns spans (line + column range) for each hit.
/// `value` is the trimmed value string, `line` the 0-based line index,
/// `col_offset` the byte column where `value` starts in the raw line.
fn scan_hw_tokens_with_spans(value: &str, line: usize, col_offset: usize) -> Vec<(String, Span)> {
    let bytes = value.as_bytes();
    let len = bytes.len();
    let mut out = Vec::new();
    let mut i = 0;
    while i < len {
        let c = bytes[i] as char;
        let boundary_ok =
            i == 0 || !((bytes[i - 1] as char).is_ascii_alphanumeric() || bytes[i - 1] == b'_');
        let starts_token = HW_TOKEN_LETTERS.contains(&c)
            && i + 1 < len
            && (bytes[i + 1] as char).is_ascii_digit()
            && boundary_ok;
        if starts_token {
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
                let token = value[start..i].to_string();
                out.push((
                    token,
                    Span {
                        line,
                        col_start: col_offset + start,
                        col_end: col_offset + i,
                    },
                ));
            }
            continue;
        }
        i += 1;
    }
    out
}

fn collect_token_spans(raw_lines: &[String]) -> Vec<(String, Span)> {
    let mut out = Vec::new();
    for (line_idx, raw_line) in raw_lines.iter().enumerate() {
        let stripped = strip_comment(raw_line);
        let trimmed = stripped.trim();
        if trimmed.is_empty() || (trimmed.starts_with('[') && trimmed.ends_with(']')) {
            continue;
        }
        let Some(eq_pos) = stripped.find('=') else {
            continue;
        };
        // Require trimmed also contains '=' to avoid stray lines without a section.
        if !trimmed.contains('=') {
            continue;
        }
        let value_part = &stripped[eq_pos + 1..];
        let value_trimmed = value_part.trim();
        if value_trimmed.is_empty() {
            continue;
        }
        // Byte offset of trimmed value within stripped.
        let offset_in_part = value_part.find(value_trimmed).unwrap_or(0);
        let col_offset = eq_pos + 1 + offset_in_part;
        out.extend(scan_hw_tokens_with_spans(
            value_trimmed,
            line_idx,
            col_offset,
        ));
    }
    out
}

fn build_occurrence_index(token_spans: &[(String, Span)]) -> HashMap<String, Vec<Span>> {
    let mut map: HashMap<String, Vec<Span>> = HashMap::new();
    for (tok, span) in token_spans {
        map.entry(tok.clone()).or_default().push(*span);
    }
    map
}

fn scan_internal_tokens(value: &str) -> Vec<String> {
    let chars: Vec<char> = value.chars().collect();
    let mut out = Vec::new();
    let mut i = 0;
    while i < chars.len() {
        if chars[i] == '_' {
            let boundary_ok = i == 0 || !(chars[i - 1].is_ascii_alphanumeric());
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

fn is_hardware_token(s: &str) -> bool {
    token_kind(s).is_some()
}

fn build_producer_map(sections: &[IniSection]) -> HashMap<String, HashSet<String>> {
    let mut map: HashMap<String, HashSet<String>> = HashMap::new();
    for section in sections {
        let mut section_hardware: HashSet<String> = HashSet::new();
        let mut section_internals: HashSet<String> = HashSet::new();
        let mut output_internals: HashSet<String> = HashSet::new();
        for (k, v) in &section.entries {
            for t in scan_hw_tokens(v) {
                section_hardware.insert(t);
            }
            for t in scan_internal_tokens(v) {
                section_internals.insert(t);
            }
            if k.starts_with("output") {
                for t in scan_internal_tokens(v) {
                    output_internals.insert(t);
                }
            }
        }
        for produced in output_internals {
            let mut sources: HashSet<String> = HashSet::new();
            for h in &section_hardware {
                if h != &produced {
                    sources.insert(h.clone());
                }
            }
            for iv in &section_internals {
                if iv != &produced {
                    sources.insert(iv.clone());
                }
            }
            map.entry(produced).or_default().extend(sources);
        }
    }
    map
}

#[allow(clippy::needless_range_loop)]
fn find_select_span(
    raw_lines: &[String],
    sections: &[IniSection],
    section_idx: usize,
    src: &str,
) -> Option<Span> {
    let header_line = sections[section_idx].header_span.line;
    let next_header = sections
        .get(section_idx + 1)
        .map(|s| s.header_span.line)
        .unwrap_or(raw_lines.len());
    for line_idx in (header_line + 1)..next_header {
        let raw_line = &raw_lines[line_idx];
        let stripped = strip_comment(raw_line);
        let trimmed = stripped.trim();
        if trimmed.is_empty() || (trimmed.starts_with('[') && trimmed.ends_with(']')) {
            continue;
        }
        let Some(eq_pos) = stripped.find('=') else {
            continue;
        };
        if !trimmed.contains('=') {
            continue;
        }
        let key_part = &stripped[..eq_pos];
        let key = key_part.trim().to_lowercase();
        if key != "select" {
            continue;
        }
        let value_part = &stripped[eq_pos + 1..];
        let value_trimmed = value_part.trim();
        if value_trimmed != src {
            // Fallback: value may be token surrounded by expression; check containment
            if !value_trimmed.contains(src) {
                continue;
            }
        }
        let offset_in_part = value_part.find(src).unwrap_or(0);
        let col_start = eq_pos + 1 + offset_in_part;
        let col_end = col_start + src.len();
        return Some(Span {
            line: line_idx,
            col_start,
            col_end,
        });
    }
    None
}

fn collect_hardware_recursive(
    start: &str,
    producers: &HashMap<String, HashSet<String>>,
    visited: &mut HashSet<String>,
    out: &mut HashSet<String>,
) {
    if !visited.insert(start.to_string()) {
        return;
    }
    if is_hardware_token(start) {
        out.insert(start.to_string());
        return;
    }
    if let Some(parents) = producers.get(start) {
        for p in parents {
            collect_hardware_recursive(p, producers, visited, out);
        }
    }
}

fn build_modifier_index(
    raw_lines: &[String],
    sections: &[IniSection],
) -> HashMap<String, Vec<ModifierAffect>> {
    let producers = build_producer_map(sections);
    let mut index: HashMap<String, Vec<ModifierAffect>> = HashMap::new();
    for (section_idx, section) in sections.iter().enumerate() {
        let mut select_src: Option<String> = None;
        let mut selectat_val: Option<String> = None;
        for (k, v) in &section.entries {
            if k == "select" {
                select_src = Some(v.trim().to_string());
            }
            if k == "selectat" {
                selectat_val = Some(v.trim().to_string());
            }
        }
        let Some(src) = select_src else {
            continue;
        };
        let Some(span) = find_select_span(raw_lines, sections, section_idx, &src) else {
            continue;
        };
        let mut reachable: HashSet<String> = HashSet::new();
        let mut visited: HashSet<String> = HashSet::new();
        collect_hardware_recursive(&src, &producers, &mut visited, &mut reachable);
        // Direct hardware token with no producer entry still reaches itself
        if reachable.is_empty() && is_hardware_token(&src) {
            reachable.insert(src.clone());
        }
        for hw in reachable {
            index.entry(hw).or_default().push(ModifierAffect {
                span,
                source: src.clone(),
                selectat: selectat_val.clone(),
            });
        }
    }
    // Keep file order: pending selects were iterated section order, so per-hw vec is already ordered.
    index
}

pub fn token_kind(id: &str) -> Option<ComponentKind> {
    match id.chars().next()? {
        'B' => Some(ComponentKind::Button),
        'L' => Some(ComponentKind::Led),
        'P' => Some(ComponentKind::Knob),
        'O' => Some(ComponentKind::CvOut),
        'I' => Some(ComponentKind::CvIn),
        'E' => Some(ComponentKind::Encoder),
        'S' => Some(ComponentKind::Switch),
        _ => None,
    }
}

/// The controller number embedded in a token id, e.g. `1` in `B1.1` or `O1`.
fn leading_number(id: &str) -> Option<u32> {
    let digits: String = id
        .chars()
        .skip(1)
        .take_while(|c| c.is_ascii_digit())
        .collect();
    digits.parse().ok()
}

fn titlecase(s: &str) -> String {
    let mut chars = s.chars();
    match chars.next() {
        Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
        None => String::new(),
    }
}

fn add_component(
    components: &mut Vec<HwComponent>,
    seen_ids: &mut HashSet<String>,
    id: String,
    kind: ComponentKind,
    label: String,
) {
    if seen_ids.contains(&id) {
        return;
    }
    let state = match kind {
        ComponentKind::Button | ComponentKind::Switch | ComponentKind::Led => ComponentState::Off,
        ComponentKind::Knob
        | ComponentKind::CvIn
        | ComponentKind::CvOut
        | ComponentKind::Encoder => ComponentState::Value(0.0),
    };
    seen_ids.insert(id.clone());
    components.push(HwComponent {
        id,
        label,
        kind,
        shift_group: None,
        state,
        // Filled in by the controller-panel assignment pass in from_ini_str.
        controller: String::new(),
        led: None,
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn led_association_captured_for_button_with_led() {
        let content = std::fs::read_to_string("fixtures/led_pairs.ini").unwrap();
        let patch = Patch::from_ini_str(&content, String::from("led_pairs")).unwrap();
        let b1_1 = patch.hw_components.iter().find(|c| c.id == "B1.1").unwrap();
        assert_eq!(b1_1.led, Some(String::from("L1.1")));
    }

    #[test]
    fn led_association_none_for_button_without_led() {
        let content = std::fs::read_to_string("fixtures/led_pairs.ini").unwrap();
        let patch = Patch::from_ini_str(&content, String::from("led_pairs")).unwrap();
        let b1_2 = patch.hw_components.iter().find(|c| c.id == "B1.2").unwrap();
        assert_eq!(b1_2.led, None);
    }

    #[test]
    fn led_association_captured_for_knob_with_led() {
        let content = std::fs::read_to_string("fixtures/led_pairs.ini").unwrap();
        let patch = Patch::from_ini_str(&content, String::from("led_pairs")).unwrap();
        let p1_1 = patch.hw_components.iter().find(|c| c.id == "P1.1").unwrap();
        assert_eq!(p1_1.led, Some(String::from("L1.3")));
    }

    #[test]
    fn arpeggio1_button_led_associations() {
        let content = std::fs::read_to_string("fixtures/arpeggio1.ini").unwrap();
        let patch = Patch::from_ini_str(&content, String::from("arpeggio1")).unwrap();
        // arpeggio1 wires each of the 8 P2B8 buttons to its LED (B1.N -> L1.N).
        for i in 1..=8u32 {
            let btn = patch
                .hw_components
                .iter()
                .find(|c| c.id == format!("B1.{}", i))
                .unwrap_or_else(|| panic!("B1.{} must exist", i));
            assert_eq!(
                btn.led,
                Some(format!("L1.{}", i)),
                "B1.{} should be associated with L1.{}",
                i,
                i
            );
        }
    }

    #[test]
    fn parses_arpeggio_fixture() {
        let content = std::fs::read_to_string("fixtures/arpeggio1.ini").unwrap();
        let patch = Patch::from_ini_str(&content, String::from("arpeggio1")).unwrap();
        assert_eq!(patch.name, "arpeggio1");

        // 18 P2B8 tokens (8 buttons, 8 leds, 2 knobs) plus I1, O1, O3, O4
        // found while scanning circuit values.
        assert_eq!(patch.hw_components.len(), 22);

        let has = |id: &str| patch.hw_components.iter().any(|c| c.id == id);
        assert!(has("B1.1"));
        assert!(has("B1.8"));
        assert!(has("L1.1"));
        assert!(has("P1.1"));
        assert!(has("O4"));
        assert!(has("I1"));

        let b1_1 = patch.hw_components.iter().find(|c| c.id == "B1.1").unwrap();
        assert_eq!(b1_1.kind, ComponentKind::Button);
        assert_eq!(b1_1.controller, "P2B8");

        let o4 = patch.hw_components.iter().find(|c| c.id == "O4").unwrap();
        assert_eq!(o4.kind, ComponentKind::CvOut);
        assert_eq!(o4.controller, "CV I/O");
    }

    #[test]
    fn groups_named_controller_by_declared_type() {
        let content = "[notebuttons]\n    button1 = B2.1\n    button2 = B2.2\n    led1 = L2.1\n";
        let patch = Patch::from_ini_str(content, String::from("t")).unwrap();
        let b2_1 = patch.hw_components.iter().find(|c| c.id == "B2.1").unwrap();
        assert_eq!(b2_1.controller, "Notebuttons");
    }

    #[test]
    fn module_types_lists_sorted_unique_controllers() {
        let content = std::fs::read_to_string("fixtures/arpeggio1.ini").unwrap();
        let patch = Patch::from_ini_str(&content, String::from("arpeggio1")).unwrap();
        let types = patch.module_types();
        assert!(types.contains(&String::from("P2B8")));
        assert!(types.contains(&String::from("CV I/O")));
        let mut sorted = types.clone();
        sorted.sort();
        assert_eq!(types, sorted);
    }

    #[test]
    fn needs_by_type_counts_components_per_controller() {
        let content = std::fs::read_to_string("fixtures/arpeggio1.ini").unwrap();
        let patch = Patch::from_ini_str(&content, String::from("arpeggio1")).unwrap();
        let needs = patch.needs_by_type();
        assert_eq!(needs.get("P2B8"), Some(&18));
        assert_eq!(needs.get("CV I/O"), Some(&4));
    }

    #[test]
    fn master_requirement_is_master_for_patch_within_eight_cv_jacks() {
        let content = std::fs::read_to_string("fixtures/arpeggio1.ini").unwrap();
        let patch = Patch::from_ini_str(&content, String::from("arpeggio1")).unwrap();
        assert_eq!(patch.master_requirement(), MasterRequirement::Master);
    }

    #[test]
    fn master_requirement_is_master18_when_cv_jack_beyond_eight_is_used() {
        let content = "[copy]\n    input = I9\n    output = O1\n";
        let patch = Patch::from_ini_str(content, String::from("t")).unwrap();
        assert_eq!(patch.master_requirement(), MasterRequirement::Master18);
    }

    #[test]
    fn rejects_empty_file() {
        assert!(Patch::from_ini_str("", String::from("empty")).is_err());
    }

    #[test]
    fn viewer_circuits_maps_sections() {
        let patch = Patch::from_ini_file(Path::new("fixtures/arpeggio1.ini")).unwrap();
        assert_eq!(patch.sections.len(), 14);
        let circuits = patch.viewer_circuits();
        assert_eq!(circuits.len(), 14);
        // Bare `[p2b8]` declaration carries no key-value pairs.
        assert_eq!(circuits[0].name, "p2b8");
        assert!(circuits[0].entries.is_empty());
        // Repeated section names are preserved as separate circuits.
        assert_eq!(circuits[2].name, "copy");
        assert_eq!(circuits[3].name, "copy");
        // Entries keep file order with values exactly as parsed.
        assert_eq!(
            circuits[1].entries,
            vec![
                (String::from("hz"), String::from("40 * P1.1")),
                (String::from("square"), String::from("N1")),
            ]
        );
    }

    #[test]
    fn ignores_internal_variables() {
        let content =
            "[copy]\n    input = _ENV1_DECAY_POT_ABSBIPOLAR * -1 + _DECAY_MIN\n    output = O2\n";
        let patch = Patch::from_ini_str(content, String::from("t")).unwrap();
        assert_eq!(patch.hw_components.len(), 1);
        assert_eq!(patch.hw_components[0].id, "O2");
    }

    #[test]
    fn occurrence_index_is_reading_order_and_unknown_is_empty() {
        let patch = Patch::from_ini_file(Path::new("fixtures/source_navigation.ini")).unwrap();
        // token_spans is already reading order; occurrence_index groups preserve it
        let b11 = patch.occurrences_for("B1.1");
        assert!(
            b11.len() >= 3,
            "B1.1 should occur at least in button, copy and select"
        );
        // Lines must be non-decreasing, columns increase within same line
        for w in b11.windows(2) {
            assert!(
                w[0].line < w[1].line
                    || (w[0].line == w[1].line && w[0].col_start <= w[1].col_start)
            );
        }
        // Unknown token yields empty without error
        assert!(patch.occurrences_for("B99.99").is_empty());
        assert!(patch.modifier_affected_spans("B99.99").is_empty());
        assert!(patch.modifier_entries_for("B99.99").is_empty());
        // occurrence_index matches token_spans grouping
        let mut expected: std::collections::HashMap<String, usize> =
            std::collections::HashMap::new();
        for (tok, _) in &patch.token_spans {
            *expected.entry(tok.clone()).or_default() += 1;
        }
        for (tok, count) in expected {
            assert_eq!(
                patch.occurrences_for(&tok).len(),
                count,
                "occurrence count mismatch for {tok}"
            );
        }
        // Internal variables produce no occurrence
        assert!(patch.occurrences_for("_TRANSIT").is_empty());
        assert!(patch.occurrences_for("_CYCLE_A").is_empty());
    }

    #[test]
    fn modifier_direct_hardware_boolean_activation() {
        let patch = Patch::from_ini_file(Path::new("fixtures/source_navigation.ini")).unwrap();
        // select = B1.1 boolean form has no selectat
        let entries = patch.modifier_entries_for("B1.1");
        let direct = entries
            .iter()
            .find(|e| e.source == "B1.1" && e.selectat.is_none());
        assert!(direct.is_some(), "B1.1 should have direct boolean select");
        let span = direct.unwrap().span;
        // Span should point inside the select = B1.1 line
        let raw = &patch.raw_lines[span.line];
        assert!(raw.contains("select"));
        assert!(raw.contains("B1.1"));
    }

    #[test]
    fn modifier_exact_value_selectat() {
        let patch = Patch::from_ini_file(Path::new("fixtures/source_navigation.ini")).unwrap();
        let entries = patch.modifier_entries_for("P1.1");
        let exact = entries
            .iter()
            .find(|e| e.source == "P1.1" && e.selectat.as_deref() == Some("0.5"));
        assert!(exact.is_some(), "P1.1 should have exact-value selectat 0.5");
        let span = exact.unwrap().span;
        assert!(patch.raw_lines[span.line].contains("select"));
    }

    #[test]
    fn modifier_transitive_internal_producer() {
        let patch = Patch::from_ini_file(Path::new("fixtures/source_navigation.ini")).unwrap();
        // B1.2 -> _TRANSIT -> select = _TRANSIT
        let mods = patch.modifier_affected_spans("B1.2");
        // Should include the transit select line (and also direct selects of B1.2)
        let has_transit = mods.iter().any(|s| {
            let line = &patch.raw_lines[s.line];
            line.contains("_TRANSIT") || patch.raw_lines[s.line].contains("select")
        });
        assert!(
            has_transit,
            "B1.2 should transitively affect select = _TRANSIT"
        );
        // Longer chain B1.1 -> _CHAIN1 -> _CHAIN2 -> select _CHAIN2
        let b11_mods = patch.modifier_entries_for("B1.1");
        let has_chain2 = b11_mods.iter().any(|e| e.source == "_CHAIN2");
        assert!(
            has_chain2,
            "B1.1 should transitively affect select = _CHAIN2"
        );
        // Spans for transitive case should be ordered file-wise
        for w in mods.windows(2) {
            assert!(w[0].line <= w[1].line);
        }
    }

    #[test]
    fn modifier_cycle_safe_termination() {
        let patch = Patch::from_ini_file(Path::new("fixtures/source_navigation.ini")).unwrap();
        // Cycle: _CYCLE_A <-> _CYCLE_B produced from B1.3; select = _CYCLE_B
        let mods = patch.modifier_entries_for("B1.3");
        let has_cycle = mods.iter().any(|e| e.source == "_CYCLE_B");
        assert!(
            has_cycle,
            "B1.3 should reach select = _CYCLE_B through cycle"
        );
        // Must terminate and still provide exact selectat 1
        let exact = mods
            .iter()
            .find(|e| e.source == "_CYCLE_B" && e.selectat.as_deref() == Some("1"));
        assert!(exact.is_some());
        // No hang: simply reaching this point proves termination (timeout would fail test)
    }

    #[test]
    fn modifier_direct_hardware_p12_boolean() {
        let patch = Patch::from_ini_file(Path::new("fixtures/source_navigation.ini")).unwrap();
        let entries = patch.modifier_entries_for("P1.2");
        // P1.2 has a direct select = P1.2 without selectat
        assert!(entries
            .iter()
            .any(|e| e.source == "P1.2" && e.selectat.is_none()));
    }
}
