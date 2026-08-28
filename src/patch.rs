use std::collections::{HashMap, HashSet, VecDeque};
use std::ops::Range;
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

/// A single cable index entry: cable name → producing circuits + ordered sink references.
///
/// Extracted from section param values:
/// - `output = _NAME` registers a cable source for the section's circuit
/// - Any other param value referencing `_NAME` (bare or embedded) registers a sink reference
/// - Comment lines (`# …`) are ignored entirely — real patches carry commented-out
///   preamble cable maps that must NOT produce edges.
/// - Internal `_ENV…`-style names that appear only inside comments must not leak into the index.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct CableIndexEntry {
    /// Circuits that produce this cable via `output =`, in file appearance order.
    /// Empty if this cable name is only referenced as a sink, never produced.
    /// More than one producer is an invalid `n → 1` topology, flagged by graph
    /// validation (graph.rs), not by the parser.
    #[serde(default)]
    pub sources: Vec<String>,
    /// Ordered sink references: (section_name, param_key) pairs where this cable
    /// is referenced as a sink, in file appearance order.
    #[serde(default)]
    pub sink_refs: Vec<(String, String)>,
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

/// A comment-banner group: the ordered circuit-section range owned by a
/// `# ---- Name ----` banner (from the banner line until the next banner or
/// EOF). The implicit group of sections before the first banner carries
/// `banner: None` and is ordered first.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BannerGroup {
    /// Banner text from `# ---- Name ----`, dashes/spaces trimmed. `None`
    /// for the implicit unnamed group before the first banner.
    pub banner: Option<String>,
    /// Range of `Patch.sections` occurrence indices this group owns,
    /// `start` inclusive to `end` exclusive.
    pub section_range: Range<usize>,
}

/// Node identity for the influence walk: `(circuit_name, instance_index)`.
///
/// Repeated section names are distinct circuit instances, so the name alone
/// is not unique. Mirrors `crate::graph::NodeId` but lives in `patch` so
/// the walk stays pure and avoids a `patch -> graph` dependency (ARCHITECTURE).
pub type NodeId = (String, usize);

/// Forward influence result: the set of circuits and cables reachable from a
/// modifier's root variable(s) via structural hops (any circuit on the current
/// flow that has an output port). Pure, deterministic, cycle-safe.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct InfluenceSubtree {
    /// Circuit nodes reached by the walk, as `NodeId`s.
    #[serde(default)]
    pub influenced_nodes: HashSet<NodeId>,
    /// Cable names traversed by the walk (including roots, even if dangling).
    #[serde(default)]
    pub influenced_edges: HashSet<String>,
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
    /// Virtual cable index: cable name → producing circuits + ordered sink references.
    /// Extracted from section param values during patch parsing:
    /// - `output = _NAME` registers a cable source for the section's circuit
    /// - Any other param value referencing `_NAME` (bare or embedded) registers a sink reference
    /// - Comment lines (`# …`) are ignored entirely
    /// - Internal `_ENV…`-style names that appear only inside comments must not leak into the index.
    #[serde(default)]
    pub cable_index: HashMap<String, CableIndexEntry>,
    /// Ordered comment-banner groups: each banner (`# ---- Name ----`) owns
    /// the circuit-section range from its line until the next banner or EOF.
    /// Sections before the first banner form an implicit unnamed group
    /// (`banner = None`) ordered first. Populated by `from_ini_str`; hand-built
    /// patches (e.g. `sample()`) carry none.
    #[serde(default)]
    pub banner_groups: Vec<BannerGroup>,
    /// Per-section output cables: `circuit_outputs[i]` lists every `_VAR`
    /// produced by `sections[i]` via `output = _VAR` (comment-aware, repeated
    /// sections are distinct instances). Parallel to `sections`; deterministic
    /// order (file appearance, sorted per section for stability).
    #[serde(default)]
    pub circuit_outputs: Vec<Vec<String>>,
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

impl HwComponent {
    /// The circuit-instance number embedded in this component's id (e.g. `1`
    /// in `B1.1`, `2` in `B2.1`) — the same number `from_ini_str` already
    /// uses to assign `controller` and `shift_group`. Used by the renderer
    /// to group a panel's components by originating circuit instance
    /// without needing new parser state (controller-panels/spec.md "Panel
    /// contains modules").
    pub fn module_instance(&self) -> Option<u32> {
        leading_number(&self.id)
    }
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
            cable_index: HashMap::new(),
            banner_groups: Vec::new(),
            circuit_outputs: Vec::new(),
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
                // Label is just the token id ("B1.1") — the panel title
                // already reads "P2B8", so a "P2B8 Button 1.1" label was a
                // redundant prefix that clipped to an indistinguishable
                // "P2B8 Button 1." inside COMPONENT_WIDTH (droid_tui-p2x).
                for i in 1..=8 {
                    add_component(
                        &mut components,
                        &mut seen_ids,
                        format!("B{}.{}", n, i),
                        ComponentKind::Button,
                        format!("B{}.{}", n, i),
                    );
                    add_component(
                        &mut components,
                        &mut seen_ids,
                        format!("L{}.{}", n, i),
                        ComponentKind::Led,
                        format!("L{}.{}", n, i),
                    );
                }
                for i in 1..=2 {
                    add_component(
                        &mut components,
                        &mut seen_ids,
                        format!("P{}.{}", n, i),
                        ComponentKind::Knob,
                        format!("P{}.{}", n, i),
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

            // Numbered circuit LED params (e.g. `led11 = L1.1`) pair with the
            // same-suffix element entry in the section (`button11 = B1.1`) —
            // the DROID convention for circuits like matrixmixer that address
            // buttons and LEDs by a shared matrix-position suffix. The ledN
            // VALUE (L.N) is authoritative for the LED hardware token; the
            // serial-position-dependent numbering is already encoded by the
            // patch author, so the parser reads it directly rather than
            // deriving it. Match suffix against any hardware-token-valued
            // sibling entry (buttonN, potN, ...) in the same section.
            let mut element_by_suffix: HashMap<&str, &str> = HashMap::new();
            for (key, value) in &section.entries {
                // A `led*` key is the LED side of a pair, never the element
                // being driven — exclude it so a lone `ledN` can't pair with
                // itself when it has no same-suffix element sibling.
                if key.starts_with("led") {
                    continue;
                }
                if let Some(suffix) = leading_digits(key) {
                    if token_kind(value).is_some() {
                        element_by_suffix.entry(suffix).or_insert(value);
                    }
                }
            }
            for (key, led) in &section.entries {
                if let Some(led_suffix) = key.strip_prefix("led").and_then(leading_digits) {
                    if let Some(element) = element_by_suffix.get(led_suffix) {
                        if let Some(comp) = components.iter_mut().find(|c| c.id == **element) {
                            comp.led = Some(led.clone());
                        }
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
        let cable_index = collect_cable_index(&sections);
        let banner_groups = collect_banner_groups(&raw_lines, &sections);
        let circuit_outputs = collect_circuit_outputs(&sections);

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
            cable_index,
            banner_groups,
            circuit_outputs,
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

    /// Derive root `_VAR`(s) for a hardware token: every `_VAR` produced via
    /// `output = _VAR` in a section whose any param value contains the token
    /// (boundary-aware, via `scan_hw_tokens`). Distinct, sorted, deterministic.
    pub fn hw_token_to_vars(&self, hw_token: &str) -> Vec<String> {
        let mut seen: HashSet<String> = HashSet::new();
        let mut vars: Vec<String> = Vec::new();
        for (idx, section) in self.sections.iter().enumerate() {
            let has_token = section
                .entries
                .iter()
                .any(|(_, v)| scan_hw_tokens(v).iter().any(|t| t == hw_token));
            if has_token {
                if let Some(outputs) = self.circuit_outputs.get(idx) {
                    for var in outputs {
                        if seen.insert(var.clone()) {
                            vars.push(var.clone());
                        }
                    }
                }
            }
        }
        vars.sort();
        vars
    }

    /// Forward BFS influence walk from `root_vars` with no circuits disabled.
    pub fn influence_subtree(&self, root_vars: &[String]) -> InfluenceSubtree {
        self.influence_subtree_with_disabled(root_vars, &HashSet::new())
    }

    /// Forward BFS influence walk from `root_vars`, treating every circuit in
    /// `disabled` as a dead end.
    ///
    /// Queue is cables (`VecDeque<String>`). `visited_cables` + `visited_nodes`
    /// make it cycle-safe. Iteration over cable sinks is deterministic:
    /// sinks are collected per-param and sorted by `(section_name, param_key,
    /// section_index)` (D9). Hop eligibility is structural — any sink circuit
    /// that has an output port (`circuit_outputs[sink_idx]` non-empty) — not an
    /// allowlist. A circuit in `disabled` is still marked influenced, but its
    /// produced cables are never enqueued, so downstream influence stops there
    /// (per-circuit processing toggle). Leaf termination when sink has no
    /// output. Pure, no terminal IO.
    pub fn influence_subtree_with_disabled(
        &self,
        root_vars: &[String],
        disabled: &HashSet<NodeId>,
    ) -> InfluenceSubtree {
        if root_vars.is_empty() {
            return InfluenceSubtree::default();
        }
        let node_ids = build_node_ids(&self.sections);
        // Dedup + sort roots for deterministic seed order.
        let mut seen_root: HashSet<String> = HashSet::new();
        let mut roots: Vec<String> = Vec::new();
        for r in root_vars {
            if seen_root.insert(r.clone()) {
                roots.push(r.clone());
            }
        }
        roots.sort();
        let mut queue: VecDeque<String> = roots.into_iter().collect();
        let mut visited_cables: HashSet<String> = HashSet::new();
        let mut visited_nodes: HashSet<NodeId> = HashSet::new();
        let mut influenced_nodes: HashSet<NodeId> = HashSet::new();
        let mut influenced_edges: HashSet<String> = HashSet::new();
        while let Some(cable) = queue.pop_front() {
            if !visited_cables.insert(cable.clone()) {
                continue;
            }
            influenced_edges.insert(cable.clone());
            // Collect per-param sink entries for this cable, then sort for determinism.
            let mut sink_entries: Vec<(NodeId, usize, String)> = Vec::new();
            for (idx, section) in self.sections.iter().enumerate() {
                for (k, v) in &section.entries {
                    let k_lower = k.to_lowercase();
                    if k_lower == "output" {
                        let names = scan_internal_tokens(v);
                        if names.len() == 1 && names[0] == cable {
                            continue;
                        }
                    }
                    if scan_internal_tokens(v).iter().any(|n| n == &cable) {
                        let nid = node_ids
                            .get(idx)
                            .cloned()
                            .unwrap_or_else(|| (section.name.clone(), 0));
                        sink_entries.push((nid, idx, k_lower.clone()));
                    }
                }
            }
            // Sort by (section_name, param_key, section_index) for deterministic BFS expansion.
            sink_entries.sort_by(|a, b| {
                let an = &a.0 .0;
                let bn = &b.0 .0;
                an.cmp(bn)
                    .then_with(|| a.2.cmp(&b.2))
                    .then_with(|| a.1.cmp(&b.1))
            });
            // Dedup per (NodeId, param_key) to keep per-param distinct but avoid duplicate pushes for same param.
            let mut seen_sink: HashSet<(NodeId, String)> = HashSet::new();
            for (nid, sink_idx, param_key) in sink_entries {
                let dedup_key = (nid.clone(), param_key.clone());
                if !seen_sink.insert(dedup_key) {
                    continue;
                }
                if !visited_nodes.insert(nid.clone()) {
                    // Node already visited — its outputs have been queued, but still mark as influenced (already).
                    // Ensure influenced_nodes contains it (visited implies influenced).
                    influenced_nodes.insert(nid);
                    continue;
                }
                influenced_nodes.insert(nid.clone());
                // Structural hop: if sink has output ports, queue its outputs.
                // A disabled circuit is a dead end — its own cells stay
                // influenced, but nothing downstream of it is reached.
                if !disabled.contains(&nid) {
                    if let Some(outputs) = self.circuit_outputs.get(sink_idx) {
                        if !outputs.is_empty() {
                            let mut sorted_outputs = outputs.clone();
                            sorted_outputs.sort();
                            for out in sorted_outputs {
                                if !visited_cables.contains(&out) && !queue.contains(&out) {
                                    queue.push_back(out);
                                }
                            }
                        }
                    }
                }
            }
        }
        InfluenceSubtree {
            influenced_nodes,
            influenced_edges,
        }
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

/// If `line` is a comment banner (`# ---- Name ----`), return the banner text
/// (dashes/spaces trimmed). Returns `None` for any non-banner line (plain
/// comments, section headers, `key = value` lines, blanks, or a bare dash
/// rule with no name).
fn parse_banner(line: &str) -> Option<String> {
    let rest = line.strip_prefix('#')?.trim();
    if !rest.starts_with('-') {
        return None;
    }
    let inner = rest.trim_matches('-').trim();
    if inner.is_empty() {
        return None;
    }
    Some(inner.to_string())
}

/// Build the ordered banner→section-range groups for a patch.
///
/// A banner line (`# ---- Name ----`) starts a group that owns every circuit
/// section from that line until the next banner (or EOF). Sections before the
/// first banner form an implicit unnamed group (`banner = None`) ordered
/// first. Attribution is per section OCCURRENCE: repeated section names are
/// distinct circuit instances, and each maps to the banner active at its
/// header line. Line numbers (banner lines vs `IniSection.header_span.line`)
/// are the bridge because comments are stripped before section parsing.
fn collect_banner_groups(raw_lines: &[String], sections: &[IniSection]) -> Vec<BannerGroup> {
    // Banner line index -> banner text, in file order.
    let banners: Vec<(usize, String)> = raw_lines
        .iter()
        .enumerate()
        .filter_map(|(idx, line)| parse_banner(line).map(|text| (idx, text)))
        .collect();

    // Per section occurrence, the active banner is the last banner at or
    // before its header line; if no banner precedes it, the section belongs
    // to the implicit unnamed group. Consecutive sections sharing an active
    // banner form one contiguous range.
    let mut groups: Vec<BannerGroup> = Vec::new();
    let mut b: usize = 0;
    for (si, section) in sections.iter().enumerate() {
        let header_line = section.header_span.line;
        while b < banners.len() && banners[b].0 <= header_line {
            b += 1;
        }
        let active = if b == 0 {
            None
        } else {
            Some(banners[b - 1].1.clone())
        };
        match groups.last_mut() {
            Some(g) if g.banner == active => g.section_range.end = si + 1,
            _ => groups.push(BannerGroup {
                banner: active,
                section_range: si..si + 1,
            }),
        }
    }
    groups
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

/// Build the virtual-cable index from parsed sections.
///
/// - A pure `output = _NAME` value registers `_NAME` as produced by the
///   section's circuit (a source).
/// - Any other param value referencing `_NAME` — bare or embedded in an
///   arithmetic expression — registers an ordered sink reference
///   `(section_name, param_key)`.
/// - Comment lines are already stripped by `parse_ini_sections`, so commented
///   cable maps (`# output = _MIDIC`) never produce index entries, and
///   `_ENV…`-style internal names that appear only inside comments cannot leak.
fn collect_cable_index(sections: &[IniSection]) -> HashMap<String, CableIndexEntry> {
    let mut index: HashMap<String, CableIndexEntry> = HashMap::new();

    for section in sections {
        let section_name = &section.name;
        // Repeated section names (e.g. two `[copy]` circuits) are distinct
        // instances: each contributes its own sink refs. Dedup only within one
        // section instance, per (key, cable-name) pair.
        let mut section_seen: HashSet<(String, String)> = HashSet::new();
        for (key, value) in &section.entries {
            let key_lower = key.to_lowercase();

            // Check if this is an "output = _NAME" entry (value is purely _NAME).
            // The entire value must be just _NAME with nothing else — if the value
            // is an expression like `output = _X * 2`, then _X is a sink reference,
            // not a source.
            if key_lower == "output" {
                let names = scan_internal_tokens(value);
                // Value is purely _NAME if scan_internal_tokens returns exactly one
                // token and it equals the full value.
                if names.len() == 1 && &names[0] == value {
                    // Record as a cable source: the producing circuit is the section.
                    let entry = index.entry(names[0].clone()).or_default();
                    if !entry.sources.iter().any(|s| s == section_name) {
                        entry.sources.push(section_name.clone());
                    }
                    // Skip sink reference extraction for this value since it is the source.
                    continue;
                }
                // Fall through: _NAME embedded in a non-trivial value is a sink ref.
            }

            // Extract all _NAME references from the value (for sink references).
            // Comment lines are already stripped by parse_ini_sections, so only real
            // param values are considered. Internal _ENV…-style names that appear
            // only as sink references (never via output =) are recorded here.
            let names = scan_internal_tokens(value);
            for name in &names {
                // Dedup per (key, cable-name) within this section instance only.
                // (A pure `output = _NAME` value already `continue`d above, so any
                // `output` reaching here is an expression whose _NAME refs are sinks.)
                let dedup_key = (key_lower.clone(), name.clone());
                if section_seen.insert(dedup_key) {
                    let entry = index.entry(name.clone()).or_default();
                    entry
                        .sink_refs
                        .push((section_name.clone(), key_lower.clone()));
                }
            }
        }
    }

    index
}

/// Per-section output reverse map: `circuit_outputs[i]` lists every `_VAR`
/// produced by `sections[i]` via `output = _VAR` (comment-aware because
/// `parse_ini_sections` already stripped comments). Repeated section names are
/// distinct instances; ordering is deterministic (file order, sorted per section).
fn collect_circuit_outputs(sections: &[IniSection]) -> Vec<Vec<String>> {
    let mut out: Vec<Vec<String>> = Vec::with_capacity(sections.len());
    for section in sections {
        let mut vars: Vec<String> = Vec::new();
        let mut seen: HashSet<String> = HashSet::new();
        for (k, v) in &section.entries {
            if k.to_lowercase() == "output" {
                let names = scan_internal_tokens(v);
                if names.len() == 1 && names[0] == *v && seen.insert(names[0].clone()) {
                    vars.push(names[0].clone());
                }
            }
        }
        vars.sort();
        out.push(vars);
    }
    out
}

/// Build `NodeId`s parallel to `sections`: `(circuit_name, instance_index)`
/// where `instance_index` is the zero-based occurrence order among same-named
/// sections. Deterministic, matches `graph.rs::build_nodes`.
fn build_node_ids(sections: &[IniSection]) -> Vec<NodeId> {
    let mut counts: HashMap<String, usize> = HashMap::new();
    let mut ids: Vec<NodeId> = Vec::with_capacity(sections.len());
    for section in sections {
        let count = counts.entry(section.name.clone()).or_insert(0);
        let idx = *count;
        *count += 1;
        ids.push((section.name.clone(), idx));
    }
    ids
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

/// The trailing digit run of an entry key, e.g. `"11"` from `"button11"`
/// or `"led11"`. Used to pair numbered circuit LED params (`led11`) with
/// their same-suffix element entry (`button11`).
fn leading_digits(s: &str) -> Option<&str> {
    let start = s.find(|c: char| c.is_ascii_digit())?;
    let end = s[start..]
        .find(|c: char| !c.is_ascii_digit())
        .map(|i| start + i)
        .unwrap_or(s.len());
    (start < end).then(|| &s[start..end])
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
    fn numbered_led_param_pairs_with_same_suffix_element() {
        // matrixmixer addresses buttons and LEDs by a shared matrix-position
        // suffix: button11 = B1.1 pairs with led11 = L1.1 (droid_tui-abt).
        // The ledN VALUE is authoritative for the LED token.
        let content = std::fs::read_to_string("fixtures/alg27_2.ini").unwrap();
        let patch = Patch::from_ini_str(&content, String::from("alg27_2")).unwrap();
        let find = |id: &str| patch.hw_components.iter().find(|c| c.id == id).unwrap();
        // button11 = B1.1 / led11 = L1.1
        assert_eq!(find("B1.1").led.as_deref(), Some("L1.1"));
        // button12 = B1.2 / led12 = L1.2
        assert_eq!(find("B1.2").led.as_deref(), Some("L1.2"));
        // button13 = B2.1 / led13 = L2.1
        assert_eq!(find("B2.1").led.as_deref(), Some("L2.1"));
        // button21 = B1.3 / led21 = L1.3
        assert_eq!(find("B1.3").led.as_deref(), Some("L1.3"));
        // button43 = B2.7 / led43 = L2.7
        assert_eq!(find("B2.7").led.as_deref(), Some("L2.7"));
    }

    #[test]
    fn numbered_led_param_without_sibling_element_leaves_led_none() {
        let content = "[matrixmixer]\nbutton11 = B1.1\nled11 = L1.1\nled22 = L2.4\n";
        let patch = Patch::from_ini_str(content, String::from("partial")).unwrap();
        let b1_1 = patch.hw_components.iter().find(|c| c.id == "B1.1").unwrap();
        assert_eq!(b1_1.led.as_deref(), Some("L1.1"));
        // led22 has no matching button22/pot22... so it must not associate anything.
        assert!(
            patch
                .hw_components
                .iter()
                .all(|c| c.led.as_deref() != Some("L2.4")),
            "led22 with no same-suffix element must not associate L2.4"
        );
    }

    #[test]
    fn module_instance_reads_leading_digit_run_from_id() {
        let content = std::fs::read_to_string("fixtures/multi_module_p2b8.ini").unwrap();
        let patch = Patch::from_ini_str(&content, String::from("multi_module_p2b8")).unwrap();
        let find = |id: &str| patch.hw_components.iter().find(|c| c.id == id).unwrap();
        assert_eq!(find("B1.1").module_instance(), Some(1));
        assert_eq!(find("L1.8").module_instance(), Some(1));
        assert_eq!(find("P1.2").module_instance(), Some(1));
        assert_eq!(find("B2.1").module_instance(), Some(2));
        assert_eq!(find("L2.8").module_instance(), Some(2));
        assert_eq!(find("P2.2").module_instance(), Some(2));
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

    // --- Virtual-cable / signal-flow extraction (signal-flow-graph change) ---

    #[test]
    fn cable_index_output_creates_source() {
        let content = "[p2b8]\n[clocktool]\n    output = _PULSARCLOCK\n";
        let patch = Patch::from_ini_str(content, String::from("t")).unwrap();
        let entry = patch.cable_index.get("_PULSARCLOCK").unwrap();
        assert_eq!(entry.sources, vec![String::from("clocktool")]);
        assert!(entry.sink_refs.is_empty());
    }

    #[test]
    fn cable_index_input_registers_sink() {
        let content =
            "[p2b8]\n[clocktool]\n    output = _PULSARCLOCK\n[copy]\n    input = _PULSARCLOCK\n";
        let patch = Patch::from_ini_str(content, String::from("t")).unwrap();
        let entry = patch.cable_index.get("_PULSARCLOCK").unwrap();
        assert_eq!(entry.sources, vec![String::from("clocktool")]);
        assert_eq!(
            entry.sink_refs,
            vec![(String::from("copy"), String::from("input"))]
        );
    }

    #[test]
    fn cable_index_fanout_one_source_many_sinks() {
        // 1 → n fan-out: one producer, two consumers.
        let content = "[p2b8]\n[clocktool]\n    output = _PULSARCLOCK\n[copy]\n    input = _PULSARCLOCK\n[copy]\n    input = _PULSARCLOCK\n";
        let patch = Patch::from_ini_str(content, String::from("t")).unwrap();
        let entry = patch.cable_index.get("_PULSARCLOCK").unwrap();
        assert_eq!(entry.sources.len(), 1);
        assert_eq!(entry.sink_refs.len(), 2);
    }

    #[test]
    fn cable_index_expression_embedded_sinks() {
        // `_NAME` embedded in an arithmetic expression registers a sink ref.
        let content = "[p2b8]\n[osc1]\n    frequency = _BASE * 2 - _PITCH\n";
        let patch = Patch::from_ini_str(content, String::from("t")).unwrap();
        let base = patch.cable_index.get("_BASE").unwrap();
        assert!(base.sources.is_empty());
        assert_eq!(
            base.sink_refs,
            vec![(String::from("osc1"), String::from("frequency"))]
        );
        let pitch = patch.cable_index.get("_PITCH").unwrap();
        assert_eq!(
            pitch.sink_refs,
            vec![(String::from("osc1"), String::from("frequency"))]
        );
    }

    #[test]
    fn cable_index_output_expression_is_sink_not_source() {
        // `output = _X * 2` is an expression; _X is a sink, not a source.
        let content = "[p2b8]\n[clocktool]\n    output = _BASE * 2\n";
        let patch = Patch::from_ini_str(content, String::from("t")).unwrap();
        let entry = patch.cable_index.get("_BASE").unwrap();
        assert!(
            entry.sources.is_empty(),
            "expression output must not create a source"
        );
        assert_eq!(
            entry.sink_refs,
            vec![(String::from("clocktool"), String::from("output"))]
        );
    }

    #[test]
    fn cable_index_commented_definitions_ignored() {
        // Commented cable maps (`# output = _MIDIC`) must NOT produce entries.
        let content = "#   output = _MIDIC\n[p2b8]\n[clocktool]\n    output = _PULSARCLOCK\n";
        let patch = Patch::from_ini_str(content, String::from("t")).unwrap();
        assert!(!patch.cable_index.contains_key("_MIDIC"));
        assert!(patch.cable_index.contains_key("_PULSARCLOCK"));
    }

    #[test]
    fn cable_index_internal_env_names_do_not_leak_from_comments() {
        // `_ENV…`-style internal names that appear ONLY inside comments must not
        // leak into the index.
        let content =
            "#   input = _ENV1_DECAY_POT_ABSBIPOLAR\n[p2b8]\n[clocktool]\n    output = _PULSARCLOCK\n";
        let patch = Patch::from_ini_str(content, String::from("t")).unwrap();
        assert!(!patch.cable_index.contains_key("_ENV1_DECAY_POT_ABSBIPOLAR"));
        assert!(patch.cable_index.contains_key("_PULSARCLOCK"));
    }

    #[test]
    fn cable_index_alg27_fixture_fanout_and_comment_exclusion() {
        // Real fixture: [clocktool] output = _PULSARCLOCK fans out to 12 real
        // sinks (2x [copy] input, 10x clock) across the patch; commented
        // _MIDIC / _PULSARCLOCKPITCH maps must not leak.
        let content = std::fs::read_to_string("fixtures/alg27_2.ini").unwrap();
        let patch = Patch::from_ini_str(&content, String::from("alg27_2")).unwrap();
        let entry = patch.cable_index.get("_PULSARCLOCK").unwrap();
        assert_eq!(entry.sources, vec![String::from("clocktool")]);
        assert_eq!(entry.sink_refs.len(), 12);
        // Every real sink is a `input` or `clock` param, none from comments.
        assert!(entry
            .sink_refs
            .iter()
            .all(|(_, sk)| sk == "input" || sk == "clock"));
        assert!(!patch.cable_index.contains_key("_MIDIC"));
        assert!(!patch.cable_index.contains_key("_PULSARCLOCKPITCH"));
        assert!(!patch.cable_index.contains_key("_TRIGGER_CLOCKCHECK"));
    }

    // --- banner-range grouping (task 1.2) ---

    #[test]
    fn banner_groups_multiple_banners_own_ordered_ranges() {
        let content = "\
# ---- Alpha ----
[button]
button = B1.1
# ---- Omega ----
[button]
button = B1.2
";
        let patch = Patch::from_ini_str(content, String::from("multi")).unwrap();
        let groups = &patch.banner_groups;
        assert_eq!(groups.len(), 2);
        assert_eq!(groups[0].banner.as_deref(), Some("Alpha"));
        assert_eq!(groups[0].section_range, 0..1);
        assert_eq!(groups[1].banner.as_deref(), Some("Omega"));
        assert_eq!(groups[1].section_range, 1..2);
    }

    #[test]
    fn banner_groups_circuits_before_first_banner_form_unnamed_group() {
        let content = "\
[button]
button = B1.1
# ---- Titled ----
[button]
button = B1.2
";
        let patch = Patch::from_ini_str(content, String::from("pre_banner")).unwrap();
        let groups = &patch.banner_groups;
        assert_eq!(groups.len(), 2);
        // The implicit pre-first-banner group is unnamed and ordered first.
        assert_eq!(groups[0].banner, None);
        assert_eq!(groups[0].section_range, 0..1);
        assert_eq!(groups[1].banner.as_deref(), Some("Titled"));
        assert_eq!(groups[1].section_range, 1..2);
    }

    #[test]
    fn banner_groups_last_banner_range_extends_to_eof() {
        let content = "\
# ---- Alpha ----
[button]
button = B1.1
# ---- Omega ----
[button]
button = B1.2
[button]
button = B1.3
# ---- trailing banner at EOF owns nothing ----
";
        let patch = Patch::from_ini_str(content, String::from("to_eof")).unwrap();
        let groups = &patch.banner_groups;
        assert_eq!(groups.len(), 2);
        assert_eq!(groups[0].banner.as_deref(), Some("Alpha"));
        assert_eq!(groups[0].section_range, 0..1);
        // Omega owns sections through the end of the file: no following banner
        // terminates its range.
        assert_eq!(groups[1].banner.as_deref(), Some("Omega"));
        assert_eq!(groups[1].section_range, 1..3);
        // The trailing banner at EOF has no following section and adds no group.
        assert!(patch
            .banner_groups
            .iter()
            .all(|g| g.banner.as_deref() != Some("trailing banner at EOF owns nothing")));
    }

    #[test]
    fn banner_groups_repeated_section_names_attributed_per_occurrence() {
        let content = "\
[env]
# ---- Group A ----
[button]
button = B1.1
[button]
button = B1.2
# ---- Group B ----
[button]
button = B1.3
";
        let patch = Patch::from_ini_str(content, String::from("repeated")).unwrap();
        let groups = &patch.banner_groups;
        // env before the first banner -> unnamed group; the two `[button]`
        // occurrences under Group A are distinct instances in one range; the
        // third `[button]` occurrence under Group B is a separate instance.
        assert_eq!(groups.len(), 3);
        assert_eq!(groups[0].banner, None);
        assert_eq!(groups[0].section_range, 0..1);
        assert_eq!(
            patch.sections[groups[0].section_range.clone()][0].name,
            "env"
        );
        assert_eq!(groups[1].banner.as_deref(), Some("Group A"));
        assert_eq!(groups[1].section_range, 1..3);
        // Both occurrences under Group A are `[button]` sections.
        assert!(patch.sections[groups[1].section_range.clone()]
            .iter()
            .all(|s| s.name == "button"));
        assert_eq!(groups[2].banner.as_deref(), Some("Group B"));
        assert_eq!(groups[2].section_range, 3..4);
        assert_eq!(
            patch.sections[groups[2].section_range.clone()][0].name,
            "button"
        );
    }

    // --- real-fixture integration: extraction + grouping together (task 1.3) ---

    #[test]
    fn cable_index_arpeggio_fixture_one_source_many_sinks() {
        // Real fixture: each `[button]` circuit produces a cable via `output =`.
        // `_SCALE` fans out 1 → n: one producer, four `selectN` sinks.
        let content = std::fs::read_to_string("fixtures/arpeggio1.ini").unwrap();
        let patch = Patch::from_ini_str(&content, String::from("arpeggio1")).unwrap();
        let scale = patch.cable_index.get("_SCALE").unwrap();
        assert_eq!(scale.sources, vec![String::from("button")]);
        assert_eq!(scale.sink_refs.len(), 4);
        assert!(scale.sink_refs.iter().all(|(sec, _)| sec == "arpeggio"));
        // A single-consumer cable: `_DIRECTION` is produced once, consumed once.
        let dir = patch.cable_index.get("_DIRECTION").unwrap();
        assert_eq!(dir.sources, vec![String::from("button")]);
        assert_eq!(
            dir.sink_refs,
            vec![(String::from("arpeggio"), String::from("direction"))]
        );
    }

    #[test]
    fn cable_index_source_navigation_expression_embedded_and_real_cables() {
        // Real fixture: `output = _ENV1_DECAY_POT_ABSBIPOLAR * -1 + _DECAY_MIN`
        // is an expression, so both `_NAME` tokens register as sink refs (never
        // sources); pure `output = _TRANSIT` produces a real source.
        let content = std::fs::read_to_string("fixtures/source_navigation.ini").unwrap();
        let patch = Patch::from_ini_str(&content, String::from("source_navigation")).unwrap();
        let decay = patch.cable_index.get("_ENV1_DECAY_POT_ABSBIPOLAR").unwrap();
        assert!(decay.sources.is_empty());
        assert!(decay
            .sink_refs
            .iter()
            .any(|(sec, key)| sec == "button" && key == "output"));
        let min = patch.cable_index.get("_DECAY_MIN").unwrap();
        assert!(min.sources.is_empty());
        assert!(min.sink_refs.iter().any(|(sec, _)| sec == "button"));
        // Pure cable produced in `[compare]`, consumed by `select = _TRANSIT`.
        let transit = patch.cable_index.get("_TRANSIT").unwrap();
        assert_eq!(transit.sources, vec![String::from("compare")]);
        assert!(transit.sink_refs.iter().any(|(sec, _)| sec == "switch"));
    }

    #[test]
    fn banner_groups_real_fixture_owns_ordered_ranges() {
        // Real fixture (cable_banner_combos.ini): the pre-first-banner `[button]`
        // section forms the implicit unnamed group; the `# ---- Mixer ----` banner
        // owns the following three circuit sections.
        let content = std::fs::read_to_string("fixtures/cable_banner_combos.ini").unwrap();
        let patch = Patch::from_ini_str(&content, String::from("cable_banner_combos")).unwrap();
        let groups = &patch.banner_groups;
        assert_eq!(groups.len(), 2);
        assert_eq!(groups[0].banner, None);
        assert_eq!(groups[0].section_range, 0..1);
        assert_eq!(
            patch.sections[groups[0].section_range.clone()][0].name,
            "button"
        );
        assert_eq!(groups[1].banner.as_deref(), Some("Mixer"));
        assert_eq!(groups[1].section_range, 1..4);
    }

    #[test]
    fn cable_and_banner_groups_coexist_consistently() {
        // Real fixture (cable_banner_combos.ini): cable extraction and banner
        // grouping run on the same patch. `_CLOCK`'s source (clocktool) and sink
        // (mixer) both fall inside the Mixer banner range; `_GATE` is produced in
        // the pre-first-banner group and consumed inside the Mixer group — a cable
        // legitimately spanning banner groups. The commented-out map stays absent.
        let content = std::fs::read_to_string("fixtures/cable_banner_combos.ini").unwrap();
        let patch = Patch::from_ini_str(&content, String::from("cable_banner_combos")).unwrap();
        let section_index = |name: &str| {
            patch
                .sections
                .iter()
                .position(|s| s.name == name)
                .expect("section must exist in fixture")
        };
        let unnamed = &patch.banner_groups[0].section_range;
        let mixer = &patch.banner_groups[1].section_range;
        assert_eq!(unnamed, &(0..1));
        assert_eq!(mixer, &(1..4));
        // _CLOCK: produced and consumed entirely inside the Mixer banner range.
        let clock = patch.cable_index.get("_CLOCK").unwrap();
        assert!(mixer.contains(&section_index("clocktool")));
        assert!(mixer.contains(&section_index("mixer")));
        assert_eq!(clock.sources, vec![String::from("clocktool")]);
        assert_eq!(
            clock.sink_refs,
            vec![(String::from("mixer"), String::from("clock"))]
        );
        // _GATE: source in the unnamed group, sinks in the Mixer group.
        let gate = patch.cable_index.get("_GATE").unwrap();
        assert!(unnamed.contains(&section_index("button")));
        assert_eq!(gate.sources, vec![String::from("button")]);
        assert_eq!(
            gate.sink_refs,
            vec![
                (String::from("mixer"), String::from("input")),
                (String::from("contour"), String::from("gate")),
            ]
        );
        // _MIX: fully contained in the Mixer banner range too.
        let mix = patch.cable_index.get("_MIX").unwrap();
        assert!(mixer.contains(&section_index("mixer")));
        assert!(mixer.contains(&section_index("contour")));
        assert_eq!(mix.sources, vec![String::from("mixer")]);
        assert_eq!(
            mix.sink_refs,
            vec![(String::from("contour"), String::from("input"))]
        );
        // Commented-out cable map never leaks into the index.
        assert!(!patch.cable_index.contains_key("_COMMENTED"));
    }

    // --- influence walk (task 4.1) ---

    fn influenced_nodes_sorted(patch: &Patch, roots: &[String]) -> Vec<NodeId> {
        let mut v: Vec<NodeId> = patch
            .influence_subtree(roots)
            .influenced_nodes
            .into_iter()
            .collect();
        v.sort();
        v
    }

    fn influenced_edges_sorted(patch: &Patch, roots: &[String]) -> Vec<String> {
        let mut v: Vec<String> = patch
            .influence_subtree(roots)
            .influenced_edges
            .into_iter()
            .collect();
        v.sort();
        v
    }

    #[test]
    fn influence_direct_consumption() {
        let patch = Patch::from_ini_str(
            "[p2b8]\n[clocktool]\n    output = _CLK\n[copy]\n    input = _CLK\n",
            String::from("t"),
        )
        .unwrap();
        let nodes = influenced_nodes_sorted(&patch, &[String::from("_CLK")]);
        assert_eq!(nodes, vec![(String::from("copy"), 0)]);
        let edges = influenced_edges_sorted(&patch, &[String::from("_CLK")]);
        assert_eq!(edges, vec![String::from("_CLK")]);
    }

    #[test]
    fn influence_indirect_via_switch() {
        let patch = Patch::from_ini_str(
            "[p2b8]\n\
             [clocktool]\n    output = _SEL\n\
             [switch]\n    select = _SEL\n    input = _A\n    output = _B\n\
             [quantizer]\n    input = _B\n",
            String::from("t"),
        )
        .unwrap();
        let sub = patch.influence_subtree(&[String::from("_SEL")]);
        assert!(sub.influenced_nodes.contains(&(String::from("switch"), 0)));
        assert!(sub
            .influenced_nodes
            .contains(&(String::from("quantizer"), 0)));
        assert!(sub.influenced_edges.contains("_SEL"));
        assert!(sub.influenced_edges.contains("_B"));
    }

    #[test]
    fn influence_indirect_via_copy_hop() {
        // copy is not in any allowlist — structural hop (input+output) qualifies it.
        let patch = Patch::from_ini_str(
            "[p2b8]\n[clocktool]\n    output = _A\n[copy]\n    input = _A\n    output = _B\n[sink]\n    input = _B\n",
            String::from("t"),
        )
        .unwrap();
        let sub = patch.influence_subtree(&[String::from("_A")]);
        assert!(sub.influenced_nodes.contains(&(String::from("copy"), 0)));
        assert!(sub.influenced_nodes.contains(&(String::from("sink"), 0)));
        assert!(sub.influenced_edges.contains("_A"));
        assert!(sub.influenced_edges.contains("_B"));
    }

    #[test]
    fn influence_any_input_output_circuit_qualifies_as_hop() {
        // mixer/logic with input+output must hop even though not switch/copy.
        let patch = Patch::from_ini_str(
            "[p2b8]\n[clocktool]\n    output = _A\n[mixer]\n    input = _A\n    output = _MIX\n[sink]\n    input = _MIX\n",
            String::from("t"),
        )
        .unwrap();
        let sub = patch.influence_subtree(&[String::from("_A")]);
        assert!(sub.influenced_nodes.contains(&(String::from("mixer"), 0)));
        assert!(sub.influenced_nodes.contains(&(String::from("sink"), 0)));
        assert!(sub.influenced_edges.contains("_MIX"));
    }

    #[test]
    fn influence_copy_chain() {
        let patch = Patch::from_ini_str(
            "[p2b8]\n[clocktool]\n    output = _A\n[copy]\n    input = _A\n    output = _B\n[copy]\n    input = _B\n    output = _C\n[sink]\n    input = _C\n",
            String::from("t"),
        )
        .unwrap();
        let sub = patch.influence_subtree(&[String::from("_A")]);
        assert!(sub.influenced_nodes.contains(&(String::from("copy"), 0)));
        assert!(sub.influenced_nodes.contains(&(String::from("copy"), 1)));
        assert!(sub.influenced_nodes.contains(&(String::from("sink"), 0)));
        assert!(sub.influenced_edges.contains("_A"));
        assert!(sub.influenced_edges.contains("_B"));
        assert!(sub.influenced_edges.contains("_C"));
    }

    #[test]
    fn influence_cycle_safe_and_deterministic() {
        let patch = Patch::from_ini_str(
            "[p2b8]\n[clocktool]\n    output = _A\n[copy]\n    input = _A\n    output = _B\n[copy]\n    input = _B\n    output = _A\n[switch]\n    select = _B\n    output = O1\n",
            String::from("t"),
        )
        .unwrap();
        let a = patch.influence_subtree(&[String::from("_A")]);
        let b = patch.influence_subtree(&[String::from("_A")]);
        assert_eq!(a, b, "influence walk must be deterministic");
        assert!(a.influenced_edges.contains("_A"));
        assert!(a.influenced_edges.contains("_B"));
        assert!(a.influenced_nodes.contains(&(String::from("copy"), 0)));
        assert!(a.influenced_nodes.contains(&(String::from("copy"), 1)));
        assert!(a.influenced_nodes.contains(&(String::from("switch"), 0)));
    }

    #[test]
    fn influence_leaf_termination_no_output() {
        let patch = Patch::from_ini_str(
            "[p2b8]\n[clocktool]\n    output = _A\n[led]\n    input = _A\n",
            String::from("t"),
        )
        .unwrap();
        let sub = patch.influence_subtree(&[String::from("_A")]);
        assert!(sub.influenced_nodes.contains(&(String::from("led"), 0)));
        assert!(sub.influenced_edges.contains("_A"));
        // led has no output, so no further cables queued
        assert_eq!(sub.influenced_edges.len(), 1);
    }

    #[test]
    fn influence_hw_token_to_vars_single() {
        let patch = Patch::from_ini_str(
            "[p2b8]\n[button]\n    button = B1.1\n    output = _TRIG\n",
            String::from("t"),
        )
        .unwrap();
        assert_eq!(patch.hw_token_to_vars("B1.1"), vec![String::from("_TRIG")]);
        assert!(patch.hw_token_to_vars("B1.2").is_empty());
    }

    #[test]
    fn influence_hw_token_to_vars_multiple_and_sorted() {
        let patch = Patch::from_ini_str(
            "[p2b8]\n[button]\n    button = B1.1\n    output = _ZVAR\n[copy]\n    input = B1.1\n    output = _AVAR\n",
            String::from("t"),
        )
        .unwrap();
        assert_eq!(
            patch.hw_token_to_vars("B1.1"),
            vec![String::from("_AVAR"), String::from("_ZVAR")]
        );
    }

    #[test]
    fn influence_hw_token_to_vars_embedded_and_boundary_aware() {
        // B1.1 inside an expression plus B1.1_extra must not count.
        let patch = Patch::from_ini_str(
            "[p2b8]\n[button]\n    button = B1.1_extra\n    output = _NOPE\n[copy]\n    input = B1.1 + P1.1\n    output = _YES\n",
            String::from("t"),
        )
        .unwrap();
        assert_eq!(patch.hw_token_to_vars("B1.1"), vec![String::from("_YES")]);
        assert!(!patch
            .hw_token_to_vars("B1.1")
            .contains(&String::from("_NOPE")));
    }

    #[test]
    fn influence_empty_roots_returns_empty() {
        let patch = Patch::from_ini_str(
            "[p2b8]\n[clocktool]\n    output = _A\n[copy]\n    input = _A\n",
            String::from("t"),
        )
        .unwrap();
        let sub = patch.influence_subtree(&[]);
        assert!(sub.influenced_nodes.is_empty());
        assert!(sub.influenced_edges.is_empty());
    }

    #[test]
    fn influence_dangling_root_still_records_edge() {
        let patch = Patch::from_ini_str("[p2b8]\n[copy]\n    input = _ORPHAN\n", String::from("t"))
            .unwrap();
        let sub = patch.influence_subtree(&[String::from("_ORPHAN")]);
        assert!(sub.influenced_edges.contains("_ORPHAN"));
        assert!(sub.influenced_nodes.contains(&(String::from("copy"), 0)));
    }

    #[test]
    fn influence_disabled_circuit_is_dead_end_keeps_own_cells() {
        // copy(0) on the path _A -> _B is disabled: it stays influenced, but
        // sink must no longer be reached and _B must not be recorded.
        let patch = Patch::from_ini_str(
            "[p2b8]\n[clocktool]\n    output = _A\n[copy]\n    input = _A\n    output = _B\n[sink]\n    input = _B\n",
            String::from("t"),
        )
        .unwrap();
        let disabled: HashSet<NodeId> = HashSet::from([(String::from("copy"), 0)]);
        let sub = patch.influence_subtree_with_disabled(&[String::from("_A")], &disabled);
        assert!(sub.influenced_nodes.contains(&(String::from("copy"), 0)));
        assert!(!sub.influenced_nodes.contains(&(String::from("sink"), 0)));
        assert!(sub.influenced_edges.contains("_A"));
        assert!(!sub.influenced_edges.contains("_B"));
    }

    #[test]
    fn influence_disabled_repeated_instance_only_cuts_that_instance() {
        // Two parallel copy instances consuming _A: disabling instance 1 must
        // not affect instance 0's propagation through _B.
        let patch = Patch::from_ini_str(
            "[p2b8]\n[clocktool]\n    output = _A\n[copy]\n    input = _A\n    output = _B\n[copy]\n    input = _A\n    output = _C\n[sinkb]\n    input = _B\n[sinkc]\n    input = _C\n",
            String::from("t"),
        )
        .unwrap();
        let disabled: HashSet<NodeId> = HashSet::from([(String::from("copy"), 1)]);
        let sub = patch.influence_subtree_with_disabled(&[String::from("_A")], &disabled);
        assert!(sub.influenced_nodes.contains(&(String::from("copy"), 0)));
        assert!(sub.influenced_nodes.contains(&(String::from("copy"), 1)));
        assert!(sub.influenced_nodes.contains(&(String::from("sinkb"), 0)));
        assert!(!sub.influenced_nodes.contains(&(String::from("sinkc"), 0)));
        assert!(sub.influenced_edges.contains("_B"));
        assert!(!sub.influenced_edges.contains("_C"));
    }

    #[test]
    fn influence_subtree_with_empty_disabled_matches_default_walk() {
        let patch = Patch::from_ini_str(
            "[p2b8]\n[clocktool]\n    output = _A\n[copy]\n    input = _A\n    output = _B\n[sink]\n    input = _B\n",
            String::from("t"),
        )
        .unwrap();
        let sub = patch.influence_subtree(&[String::from("_A")]);
        let sub_empty =
            patch.influence_subtree_with_disabled(&[String::from("_A")], &HashSet::new());
        assert_eq!(sub, sub_empty);
    }

    #[test]
    fn influence_union_over_multiple_roots() {
        let patch = Patch::from_ini_str(
            "[p2b8]\n[clocktool]\n    output = _A\n[clocktool]\n    output = _B\n[sinka]\n    input = _A\n[sinkb]\n    input = _B\n",
            String::from("t"),
        )
        .unwrap();
        let sub = patch.influence_subtree(&[String::from("_A"), String::from("_B")]);
        assert!(sub.influenced_nodes.contains(&(String::from("sinka"), 0)));
        assert!(sub.influenced_nodes.contains(&(String::from("sinkb"), 0)));
        assert!(sub.influenced_edges.contains("_A"));
        assert!(sub.influenced_edges.contains("_B"));
    }

    #[test]
    fn influence_fixture_modifier_switch_passthrough() {
        let patch =
            Patch::from_ini_file(Path::new("fixtures/modifier_switch_passthrough.ini")).unwrap();
        // HW -> VAR
        let mut vars = patch.hw_token_to_vars("B1.1");
        vars.sort();
        assert_eq!(vars, vec![String::from("_EXTRA"), String::from("_TRIG")]);
        // direct consumption: _BASE -> copy (+ leaf)
        let base = patch.influence_subtree(&[String::from("_BASE")]);
        assert!(base.influenced_nodes.iter().any(|(n, _)| n == "copy"));
        assert!(base.influenced_nodes.iter().any(|(n, _)| n == "contour"));
        assert!(base.influenced_edges.contains("_BASE"));
        // switch passthrough: _TRIG -> switch -> _SWOUT -> quantizer/mixer
        let trig = patch.influence_subtree(&[String::from("_TRIG")]);
        assert!(trig.influenced_nodes.iter().any(|(n, _)| n == "switch"));
        assert!(trig.influenced_edges.contains("_SWOUT"));
        assert!(trig.influenced_nodes.iter().any(|(n, _)| n == "quantizer"));
        assert!(trig.influenced_nodes.iter().any(|(n, _)| n == "mixer"));
        // copy chain: _COPY1 -> _COPY2 -> contour/logic
        assert!(trig.influenced_edges.contains("_COPY1"));
        assert!(trig.influenced_edges.contains("_COPY2"));
        assert!(trig.influenced_edges.contains("_LOGICOUT"));
        // any-input+output hop: logic must be in the walk
        assert!(trig.influenced_nodes.iter().any(|(n, _)| n == "logic"));
        // leaf: led consumes _SWOUT but has no output -> no extra cable
        assert!(trig.influenced_nodes.iter().any(|(n, _)| n == "led"));
        // cycle-safe: _CYCLE_A / _CYCLE_B reachable but finite
        assert!(trig.influenced_edges.contains("_CYCLE_A"));
        assert!(trig.influenced_edges.contains("_CYCLE_B"));
        // deterministic
        let again = patch.influence_subtree(&[String::from("_TRIG")]);
        assert_eq!(trig, again);
        // _EXTRA (second var from B1.1) union via hw derivation
        let union = patch.influence_subtree(&patch.hw_token_to_vars("B1.1"));
        assert!(union.influenced_edges.contains("_TRIG"));
        assert!(union.influenced_edges.contains("_EXTRA"));
    }
}
