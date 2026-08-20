use std::collections::HashSet;
use std::path::Path;

use serde::{Deserialize, Serialize};

/// Represents a loaded DROID patch
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Patch {
    pub name: String,
    pub hw_components: Vec<HwComponent>,
    pub shift_groups: Vec<ShiftGroup>,
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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ComponentState {
    Off,
    On,
    Value(f32),
    Active,
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

        Ok(Patch {
            name,
            hw_components: components,
            shift_groups: vec![
                ShiftGroup::Group1,
                ShiftGroup::Group2,
                ShiftGroup::Group3,
                ShiftGroup::Group4,
            ],
        })
    }
}

struct IniSection {
    name: String,
    entries: Vec<(String, String)>,
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

fn token_kind(id: &str) -> Option<ComponentKind> {
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
    fn rejects_empty_file() {
        assert!(Patch::from_ini_str("", String::from("empty")).is_err());
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
