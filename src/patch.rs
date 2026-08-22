use std::collections::HashSet;
use std::path::Path;

use serde::{Deserialize, Serialize};

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

    pub fn color(&self) -> ratatui::style::Color {
        match self {
            ShiftGroup::Group1 => ratatui::style::Color::Yellow,
            ShiftGroup::Group2 => ratatui::style::Color::Cyan,
            ShiftGroup::Group3 => ratatui::style::Color::Magenta,
            ShiftGroup::Group4 => ratatui::style::Color::Green,
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
                },
                HwComponent {
                    id: "btn_2".into(),
                    label: "TRIG B".into(),
                    kind: ComponentKind::Button,
                    shift_group: Some(ShiftGroup::Group1),
                    state: ComponentState::Off,
                    controller: "P2B8".into(),
                },
                HwComponent {
                    id: "cv_in_1".into(),
                    label: "CV IN 1".into(),
                    kind: ComponentKind::CvIn,
                    shift_group: Some(ShiftGroup::Group2),
                    state: ComponentState::Value(0.0),
                    controller: "CV I/O".into(),
                },
                HwComponent {
                    id: "cv_out_1".into(),
                    label: "CV OUT 1".into(),
                    kind: ComponentKind::CvOut,
                    shift_group: Some(ShiftGroup::Group2),
                    state: ComponentState::Value(0.0),
                    controller: "CV I/O".into(),
                },
                HwComponent {
                    id: "knob_1".into(),
                    label: "CUTOFF".into(),
                    kind: ComponentKind::Knob,
                    shift_group: Some(ShiftGroup::Group3),
                    state: ComponentState::Value(0.5),
                    controller: "P2B8".into(),
                },
                HwComponent {
                    id: "led_1".into(),
                    label: "STATUS".into(),
                    kind: ComponentKind::Led,
                    shift_group: None,
                    state: ComponentState::On,
                    controller: "P2B8".into(),
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

        let sections = parse_ini_sections(content);
        if sections.is_empty() {
            return Err(String::from("No circuit sections found in patch file"));
        }

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
    for raw_line in content.lines() {
        let line = strip_comment(raw_line).trim();
        if line.is_empty() {
            continue;
        }
        if line.starts_with('[') && line.ends_with(']') {
            sections.push(IniSection {
                name: line[1..line.len() - 1].trim().to_lowercase(),
                entries: Vec::new(),
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
    });
}

#[cfg(test)]
mod tests {
    use super::*;

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
}
