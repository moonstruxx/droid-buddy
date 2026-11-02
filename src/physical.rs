//! Physical grid model — ordered controller chain + per-module faceplate
//! geometry in millimetres (task 2.1 of physical-scale-model).
//!
//! Pure module: no terminal dependency. Mirrors the conventions of
//! `src/geometry.rs` (geometry data embedded at build time + serde,
//! case-insensitive controller resolution) and `src/graph.rs` (in-module
//! unit tests). Task 2.3 adds rack/row packing on top of the public chain
//! types (`PhysicalLayout` / `PhysicalModule`).

use std::collections::{HashMap, HashSet};
use std::sync::{Mutex, Once, OnceLock};

use serde::{Deserialize, Serialize};

use crate::patch::{HwComponent, MasterModel, Patch};

// ---------------------------------------------------------------------------
// Data model — mirrors controller_geometry.json (schema from task 1.1)
// ---------------------------------------------------------------------------

/// Loaded `controller_geometry.json`. Unknown fields in the file are ignored
/// by serde; every consumed field carries a default so a partially-malformed
/// entry degrades instead of panicking.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ControllerGeometry {
    #[serde(default)]
    pub meta: GeometryMeta,
    /// Controller type key -> geometry entry. Keys are lowercase module names
    /// (`"p2b8"`, `"master"`, `"b32"`, ...).
    #[serde(default)]
    pub controllers: HashMap<String, ControllerGeometryEntry>,
}

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct GeometryMeta {
    /// MM per HP, e.g. 5.08.
    #[serde(default)]
    pub hp_mm: f64,
    /// Height of a 1 HE / 3 HE unit in mm, keyed by the he number as string.
    #[serde(default)]
    pub he_mm: HashMap<String, f64>,
    #[serde(default)]
    pub chain_gaps_mm: ChainGaps,
    #[serde(default)]
    pub defaults: GeometryDefaults,
}

/// Gaps between adjacent faceplates in the chain.
#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct ChainGaps {
    /// Gap between any two adjacent modules in mm.
    #[serde(default)]
    pub inter_module: f64,
    /// Gap between the master and the first controller in mm (kept for
    /// symmetry; the model currently uses `inter_module` throughout, per
    /// the data file's `chain_gaps_note`).
    #[serde(default)]
    pub master_to_controller: f64,
}

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct GeometryDefaults {
    /// Default controller height in HE (1 or 3).
    #[serde(default)]
    pub he: u32,
    /// Fallback width in HP for unknown controllers.
    #[serde(default)]
    pub fallback_width_hp: f64,
    /// Fallback width in mm for unknown controllers.
    #[serde(default)]
    pub fallback_width_mm: f64,
}

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct ControllerGeometryEntry {
    /// Module height in HE (1 or 3).
    #[serde(default)]
    pub he: u32,
    /// Module width in HP.
    #[serde(default)]
    pub width_hp: f64,
    /// Module width in mm.
    #[serde(default)]
    pub width_mm: f64,
    /// Module height in mm.
    #[serde(default)]
    pub height_mm: f64,
    /// Resolved element cells per family key (`"B"`, `"P"`, `"F"`, `"L"`,
    /// `"S"`, `"E"`, `"CV"`).
    #[serde(default)]
    pub element_cells: HashMap<String, Vec<ElementCellEntry>>,
}

/// One raw cell entry from the JSON file.
#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct ElementCellEntry {
    #[serde(default)]
    pub col: u32,
    #[serde(default)]
    pub row: u32,
    #[serde(default)]
    pub x_mm: f64,
    #[serde(default)]
    pub y_mm: f64,
    #[serde(default)]
    pub w_mm: f64,
    #[serde(default)]
    pub h_mm: f64,
    /// Human/position label, e.g. `"B1.3"`, `"I5"`, `"O8"`, `"R1"`, `"USB"`.
    #[serde(default)]
    pub label: String,
    /// Jack direction for CV cells (`"in"` / `"out"`); informational.
    #[serde(default)]
    pub dir: Option<String>,
    /// Faceplate annotation from the JSON (e.g. `"rotary"`, `"small"`,
    /// `"touch"`); absent → `None`.
    #[serde(default)]
    pub kind: Option<String>,
    /// Declared 1-based element number. When present it wins over the number
    /// the `label` would yield (`"I1"` -> 1, `"G4"` -> 4).
    #[serde(default)]
    pub element: Option<u32>,
    /// Declared kind letter (`'I'`, `'G'`, ...). When present it wins over the
    /// letter the label's first character would yield.
    #[serde(default)]
    pub kind_letter: Option<char>,
    /// `false` marks a physical jack no patch token reaches (the r2m/r2c
    /// serial bridges): the cell still renders but carries no identity, so
    /// `cell_for` can never resolve a token to it.
    #[serde(default)]
    pub addressable: Option<bool>,
}

// ---------------------------------------------------------------------------
// Loader
// ---------------------------------------------------------------------------

/// Compiled in so a binary copied away from the repo still has its faceplates.
const CONTROLLER_GEOMETRY_JSON: &str = include_str!("../controller_geometry.json");

/// Controller names that are themselves a geometry-data key.
///
/// The one vocabulary for "this name is a controller": `resolve_controller`
/// uses it so a name still resolves when the data file is absent, and the rack
/// inventory uses it to reject a typo instead of drawing a mystery faceplate.
/// `master`/`master18` are master models rather than rack modules, but they are
/// keys `resolve_controller` has always accepted, so they stay here.
const CONTROLLER_KEYS: &[&str] = &[
    "p2b8", "b32", "e4", "m4", "p8s8", "p10", "s10", "db8e", "g8", "x7", "r2m", "r2c", "p4b2",
    "master", "master18",
];

/// Usage-based controller section names -> the physical module that provides
/// their elements (`[faderbank]` -> "Faderbank" -> p8s8).
const CONTROLLER_ALIASES: &[(&str, &str)] = &[
    ("notebuttons", "b32"),
    ("faderbank", "p8s8"),
    ("encoder", "e4"),
    ("pot", "p10"),
    ("motorfader", "m4"),
];

/// Lowercase geometry key for a controller name, or `None` when the name is
/// not a controller this module knows.
fn controller_geometry_key(name: &str) -> Option<&'static str> {
    let c = name.trim().to_ascii_lowercase();
    CONTROLLER_KEYS
        .iter()
        .copied()
        .find(|key| *key == c.as_str())
        .or_else(|| {
            CONTROLLER_ALIASES
                .iter()
                .find(|(alias, _)| *alias == c.as_str())
                .map(|(_, key)| *key)
        })
}

impl ControllerGeometry {
    /// Parse the embedded `controller_geometry.json`, mirroring
    /// `RackGeometry::load` in `src/geometry.rs`. Malformed data returns a
    /// descriptive `Err(String)`; this never panics.
    pub fn load() -> Result<Self, String> {
        Self::from_json(CONTROLLER_GEOMETRY_JSON)
    }

    fn from_json(text: &str) -> Result<Self, String> {
        serde_json::from_str(text)
            .map_err(|e| format!("failed to parse embedded controller_geometry.json: {e}"))
    }

    /// Empty data source with the documented defaults, used when the file is
    /// missing or malformed. Every controller then resolves to the fallback
    /// module instead of panicking.
    pub fn fallback() -> Self {
        let mut he_mm = HashMap::new();
        he_mm.insert(String::from("1"), 43.3);
        he_mm.insert(String::from("3"), 128.5);
        Self {
            meta: GeometryMeta {
                hp_mm: 5.08,
                he_mm,
                chain_gaps_mm: ChainGaps {
                    inter_module: 0.5,
                    master_to_controller: 0.5,
                },
                defaults: GeometryDefaults {
                    he: 3,
                    fallback_width_hp: 5.0,
                    fallback_width_mm: 25.4,
                },
            },
            controllers: HashMap::new(),
        }
    }

    pub fn hp_mm(&self) -> f64 {
        if self.meta.hp_mm > 0.0 {
            self.meta.hp_mm
        } else {
            5.08
        }
    }

    /// Gap between adjacent modules in the chain (mm).
    pub fn chain_gap_mm(&self) -> f64 {
        let g = self.meta.chain_gaps_mm.inter_module;
        if g > 0.0 {
            g
        } else {
            0.5
        }
    }

    pub fn fallback_width_mm(&self) -> f64 {
        let w = self.meta.defaults.fallback_width_mm;
        if w > 0.0 {
            w
        } else {
            25.4
        }
    }

    pub fn fallback_width_hp(&self) -> f64 {
        let w = self.meta.defaults.fallback_width_hp;
        if w > 0.0 {
            w
        } else {
            5.0
        }
    }

    pub fn fallback_he(&self) -> u32 {
        let he = self.meta.defaults.he;
        if he > 0 {
            he
        } else {
            3
        }
    }

    /// Default module height in mm, from `he_mm[defaults.he]` (128.5 for 3 HE).
    pub fn fallback_height_mm(&self) -> f64 {
        self.meta
            .he_mm
            .get(&self.fallback_he().to_string())
            .copied()
            .filter(|h| *h > 0.0)
            .unwrap_or(128.5)
    }

    /// Case-insensitive entry lookup by lowercase geometry key.
    pub fn controller(&self, name: &str) -> Option<&ControllerGeometryEntry> {
        self.controllers.get(&name.to_ascii_lowercase())
    }

    /// Map a `HwComponent.controller` name ("P2B8", "Notebuttons", "CV I/O",
    /// ...) to a lowercase geometry-data key.
    ///
    /// Patch controllers are either explicit type names (`[p2b8]` -> "P2B8")
    /// or usage-based names from `KNOWN_CONTROLLER_SECTIONS` (e.g.
    /// `[faderbank]` -> "Faderbank"). The usage names map onto the physical
    /// module that provides those elements; anything else returns `None` and
    /// the caller uses the fallback width. "CV I/O" is handled by the caller
    /// (it takes the resolved model's `geometry_key()`).
    pub fn resolve_controller(&self, controller: &str) -> Option<String> {
        let c = controller.trim().to_ascii_lowercase();
        if self.controllers.contains_key(&c) {
            return Some(c);
        }
        controller_geometry_key(&c).map(String::from)
    }
}

// ---------------------------------------------------------------------------
// Physical layout model (the ordered controller chain)
// ---------------------------------------------------------------------------

/// Axis-aligned rectangle in millimetres within the chain (x is the running
/// offset from the left edge of the chain, y is 0 for every module — row
/// offsets are added by the rack packing in task 2.3).
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct RectMm {
    pub x_mm: f64,
    pub y_mm: f64,
    pub w_mm: f64,
    pub h_mm: f64,
}

/// One resolved element cell on a module faceplate.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ElementCell {
    /// Family key (`"B"`, `"P"`, `"F"`, `"S"`, `"E"`, `"L"`, `"CV"`).
    pub family: String,
    pub col: u32,
    pub row: u32,
    pub rect_mm: RectMm,
    pub label: String,
    /// 1-based element number decoded from the label (`"B1.3"` -> 3, `"I5"`
    /// -> 5, `"USB"` -> None). Used to match tokens in `cell_for`.
    pub element: Option<u32>,
    /// Kind letter decoded from the label (`'B'`, `'I'`, `'O'`, ...). Used to
    /// break ties on CV cells where "I4" and "O4" share an element number.
    pub kind_letter: Option<char>,
    /// Faceplate kind annotation carried from the JSON (`"rotary"`,
    /// `"small"`, `"touch"`, ...); `None` when absent.
    pub kind: Option<String>,
}

/// A faceplate in the physical chain: one (controller, module-instance) pair.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PhysicalModule {
    /// Controller panel name as assigned by `Patch::from_ini_str`
    /// (e.g. "P2B8", "Notebuttons", "CV I/O").
    pub controller: String,
    /// Circuit instance within the controller ("P2B8" instance 1 vs 2).
    /// CV I/O is always `None` — the master faceplate never subdivides.
    pub module_instance: Option<u32>,
    /// Lowercase geometry-data key ("p2b8", "master", "b32", ...). Empty for
    /// fallback modules with an unknown controller.
    pub geometry_key: String,
    /// True when the controller name had no geometry entry and the fallback
    /// width/height was used.
    pub is_fallback: bool,
    /// Faceplate rect in mm within the chain.
    pub rect_mm: RectMm,
    /// Module width in HP (used by rack packing, task 2.3).
    pub width_hp: f64,
    /// Module height in HE (1 or 3).
    pub he: u32,
    /// Element cells indexed by family key.
    pub cells: HashMap<String, Vec<ElementCell>>,
    /// Components of this patch that sit on this faceplate, in declaration
    /// order.
    pub components: Vec<HwComponent>,
}

/// The ordered physical controller chain for a patch.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PhysicalLayout {
    pub modules: Vec<PhysicalModule>,
    /// Chain width including the gap between the last two modules.
    pub total_width_mm: f64,
    /// Tallest module height.
    pub total_height_mm: f64,
    pub chain_gaps_mm: ChainGaps,
    pub hp_mm: f64,
    /// HE -> height in mm ("1" -> 43.3, "3" -> 128.5); rack rows resolve
    /// their height through this map (task 2.3).
    pub he_mm: HashMap<String, f64>,
    pub fallback_width_mm: f64,
    pub fallback_height_mm: f64,
    /// True when the chain was built from a non-empty rack inventory. The
    /// painter reads it to decide whether unaddressed cells are emitted
    /// (inventory-driven: every module) or the master stays suppressed
    /// (patch-derived chain, today's behaviour).
    #[serde(default)]
    pub inventory_driven: bool,
}

/// Map a token kind letter to the geometry family key (`"CV"` covers the
/// I/O/gate jacks).
fn family_for(kind: char) -> Option<&'static str> {
    match kind {
        'B' => Some("B"),
        'P' => Some("P"),
        'S' => Some("S"),
        'E' => Some("E"),
        'L' => Some("L"),
        'M' | 'F' => Some("F"),
        'I' | 'O' | 'G' => Some("CV"),
        _ => None,
    }
}

/// Leading digit-run of a string as a number (`"3"` -> 3, `"1.1"` -> 1,
/// `"USB"` -> None). Mirrors `patch::leading_number`.
fn leading_number(s: &str) -> Option<u32> {
    let digits: String = s.chars().take_while(char::is_ascii_digit).collect();
    if digits.is_empty() {
        None
    } else {
        digits.parse().ok()
    }
}

/// Element number + kind letter of a hardware token. Dotted tokens carry the
/// element after the dot (`"B2.3"` -> element 3); plain tokens use the
/// trailing number (`"O4"` -> element 4, `"L1"` -> element 1).
fn token_element(token: &str) -> Option<(char, u32)> {
    let t = token.trim();
    let kind = t.chars().next().filter(char::is_ascii_alphabetic)?;
    let rest = &t[kind.len_utf8()..];
    if let Some(dot) = rest.find('.') {
        leading_number(&rest[dot + 1..]).map(|n| (kind, n))
    } else {
        leading_number(rest).map(|n| (kind, n))
    }
}

/// Same extraction for a cell label: `"B1.3"` -> element 3 kind 'B',
/// `"I5"` -> element 5 kind 'I', `"USB"` -> (None, Some('U')).
fn label_element(label: &str) -> (Option<u32>, Option<char>) {
    let kind = label.chars().next().filter(char::is_ascii_alphabetic);
    let rest = &label[kind.map_or(0, char::len_utf8)..];
    let num = if let Some(dot) = rest.find('.') {
        leading_number(&rest[dot + 1..])
    } else {
        leading_number(rest)
    };
    (num, kind)
}

/// Identity `cell_for` matches a patch token against.
///
/// Declared `element`/`kind_letter` win, because the generator emits them for
/// cells whose label is not the patch vocabulary; an undeclared field falls
/// back to the label-derived one (`"B1.3"` -> 3, `"I5"` -> 5). A cell
/// declared `addressable: false` is a physical jack no token reaches (the
/// r2m/r2c serial bridges): it keeps its rect, label and kind so it renders,
/// but carries no identity at all and is therefore invisible to `cell_for`.
fn cell_identity(entry: &ElementCellEntry) -> (Option<u32>, Option<char>) {
    if entry.addressable == Some(false) {
        return (None, None);
    }
    let (element, kind_letter) = label_element(&entry.label);
    (entry.element.or(element), entry.kind_letter.or(kind_letter))
}

/// Precompute element numbers + kind letters for a module's raw cells.
fn preprocess_cells(
    entries: &HashMap<String, Vec<ElementCellEntry>>,
) -> HashMap<String, Vec<ElementCell>> {
    entries
        .iter()
        .map(|(family, cells)| {
            let out = cells
                .iter()
                .map(|c| {
                    let (element, kind_letter) = cell_identity(c);
                    ElementCell {
                        family: family.clone(),
                        col: c.col,
                        row: c.row,
                        rect_mm: RectMm {
                            x_mm: c.x_mm,
                            y_mm: c.y_mm,
                            w_mm: c.w_mm,
                            h_mm: c.h_mm,
                        },
                        label: c.label.clone(),
                        element,
                        kind_letter,
                        kind: c.kind.clone(),
                    }
                })
                .collect();
            (family.clone(), out)
        })
        .collect()
}

static DATA_WARN: Once = Once::new();
static CONTROLLER_WARN: Once = Once::new();
static UNKNOWN_MODULE_WARNED: OnceLock<Mutex<HashSet<String>>> = OnceLock::new();

fn warn_once(gate: &Once, msg: &str) {
    gate.call_once(|| eprintln!("[warn] physical: {msg}"));
}

/// One warning per distinct unknown controller name, for the process lifetime:
/// the inventory is resolved on every fit and paint, so a per-call warning
/// would flood the terminal a frame at a time.
fn warn_unknown_module(controller: &str) {
    let warned = UNKNOWN_MODULE_WARNED.get_or_init(|| Mutex::new(HashSet::new()));
    let mut names = match warned.lock() {
        Ok(guard) => guard,
        Err(poisoned) => poisoned.into_inner(),
    };
    if names.insert(controller.to_owned()) {
        eprintln!(
            "[warn] physical: controller {controller:?} in the rack inventory is unknown; entry skipped"
        );
    }
}

/// One patch-derived faceplate before the chain order is settled.
///
/// The rack inventory may reorder these, omit them or append them after
/// inventory-only modules, so `build` collects the physical facts and the
/// components of each (controller, instance) group first and orders them last.
#[derive(Clone)]
struct PatchFaceplate {
    controller: String,
    module_instance: Option<u32>,
    geometry_key: String,
    is_fallback: bool,
    width_mm: f64,
    height_mm: f64,
    width_hp: f64,
    he: u32,
    cells: HashMap<String, Vec<ElementCell>>,
    components: Vec<HwComponent>,
}

impl PatchFaceplate {
    /// Take the chain position `build` assigned it.
    fn into_module(self) -> PhysicalModule {
        PhysicalModule {
            controller: self.controller,
            module_instance: self.module_instance,
            geometry_key: self.geometry_key,
            is_fallback: self.is_fallback,
            rect_mm: RectMm {
                x_mm: 0.0,
                y_mm: 0.0,
                w_mm: self.width_mm,
                h_mm: self.height_mm,
            },
            width_hp: self.width_hp,
            he: self.he,
            cells: self.cells,
            components: self.components,
        }
    }
}

/// True for the inventory key that names the master faceplate.
///
/// `ModuleKey::parse` canonicalises every master alias to
/// `MASTER_CONTROLLER_KEY`, but `build` accepts hand-built keys too, so the
/// aliases are recognised here as well.
fn is_master_key(key: &ModuleKey) -> bool {
    key.instance == 1 && MASTER_ALIASES.contains(&key.controller.as_str())
}

/// The master faceplate at the head of a patch-derived chain: `model`'s
/// geometry entry carrying the `CV I/O` identity, so it can never disagree
/// with the validated master verdict. Missing or malformed geometry degrades
/// to the fallback size; this never panics.
fn master_faceplate(
    data: &ControllerGeometry,
    geometry_key: &str,
    fallback_w: f64,
    fallback_h: f64,
) -> PhysicalModule {
    let entry = data.controller(geometry_key);
    let (width_mm, height_mm, width_hp, he, cells) = match entry {
        Some(e) if e.width_mm > 0.0 && e.height_mm > 0.0 => (
            e.width_mm,
            e.height_mm,
            e.width_hp,
            e.he,
            preprocess_cells(&e.element_cells),
        ),
        _ => (
            fallback_w,
            fallback_h,
            data.fallback_width_hp(),
            data.fallback_he(),
            HashMap::new(),
        ),
    };

    PhysicalModule {
        controller: String::from("CV I/O"),
        module_instance: None,
        geometry_key: String::from(geometry_key),
        is_fallback: entry.is_none(),
        rect_mm: RectMm {
            x_mm: 0.0,
            y_mm: 0.0,
            w_mm: width_mm,
            h_mm: height_mm,
        },
        width_hp,
        he,
        cells,
        components: Vec::new(),
    }
}

/// Group one patch component into its (controller, instance) faceplate,
/// resolving the controller geometry with the same fallback and one-time
/// warning the pre-inventory chain used.
fn patch_faceplate(
    data: &ControllerGeometry,
    comp: &HwComponent,
    fallback_w: f64,
    fallback_h: f64,
) -> PatchFaceplate {
    let (geometry_key, is_fallback) = if let Some(k) = data.resolve_controller(&comp.controller) {
        (k, false)
    } else {
        warn_once(
            &CONTROLLER_WARN,
            &format!(
                "controller {:?} has no geometry entry; using fallback width",
                comp.controller
            ),
        );
        (String::new(), true)
    };
    let entry = if is_fallback {
        None
    } else {
        data.controller(&geometry_key)
    };
    let (width_mm, height_mm, width_hp, he, cells) = match entry {
        Some(e) if e.width_mm > 0.0 && e.height_mm > 0.0 => (
            e.width_mm,
            e.height_mm,
            e.width_hp,
            e.he,
            preprocess_cells(&e.element_cells),
        ),
        _ => {
            if !is_fallback {
                warn_once(
                    &CONTROLLER_WARN,
                    &format!(
                        "controller {:?} resolved to geometry key {:?} but has no \
                         usable dimensions; using fallback",
                        comp.controller, geometry_key
                    ),
                );
            }
            (
                fallback_w,
                fallback_h,
                data.fallback_width_hp(),
                data.fallback_he(),
                HashMap::new(),
            )
        }
    };

    PatchFaceplate {
        controller: comp.controller.clone(),
        module_instance: comp.module_instance(),
        geometry_key,
        is_fallback,
        width_mm,
        height_mm,
        width_hp,
        he,
        cells,
        components: vec![comp.clone()],
    }
}

/// A faceplate the inventory names but the patch does not address: full
/// controller geometry and cells, no components, so `unaddressed_cells`
/// exposes the whole front.
///
/// Without a patch there is no panel label to copy, so `controller` is the
/// upper-cased canonical key — the spelling a bare patch section uses
/// (`p2b8` -> `P2B8`), which keeps `PhysicalModule::key` human-readable.
fn unaddressed_faceplate(
    data: &ControllerGeometry,
    key: &ModuleKey,
    fallback_w: f64,
    fallback_h: f64,
) -> PhysicalModule {
    let known = controller_geometry_key(&key.controller);
    let entry = known.and_then(|k| data.controller(k));
    let (width_mm, height_mm, width_hp, he, cells) = match entry {
        Some(e) if e.width_mm > 0.0 && e.height_mm > 0.0 => (
            e.width_mm,
            e.height_mm,
            e.width_hp,
            e.he,
            preprocess_cells(&e.element_cells),
        ),
        _ => {
            if known.is_none() {
                warn_once(
                    &CONTROLLER_WARN,
                    &format!(
                        "controller {:?} in the rack inventory has no geometry entry; \
                         using fallback width",
                        key.controller
                    ),
                );
            }
            (
                fallback_w,
                fallback_h,
                data.fallback_width_hp(),
                data.fallback_he(),
                HashMap::new(),
            )
        }
    };

    PhysicalModule {
        controller: key.controller.to_ascii_uppercase(),
        module_instance: Some(u32::try_from(key.instance).unwrap_or(u32::MAX)),
        geometry_key: known.map_or_else(String::new, String::from),
        is_fallback: known.is_none(),
        rect_mm: RectMm {
            x_mm: 0.0,
            y_mm: 0.0,
            w_mm: width_mm,
            h_mm: height_mm,
        },
        width_hp,
        he,
        cells,
        components: Vec::new(),
    }
}

impl PhysicalLayout {
    /// Build the ordered controller chain for a patch under `model`.
    ///
    /// `model` is the master model resolved for the patch by the master plan
    /// (`rack::MasterPlan::model`): the layout draws that answer, it never
    /// resolves one itself, so the faceplate cannot disagree with the
    /// validated verdict and this module stays schema-free.
    ///
    /// An *empty* `inventory` reproduces the pre-inventory chain byte for
    /// byte: exactly one master faceplate at position 0 (`model`'s geometry
    /// key, carrying the `CV I/O` identity, which never subdivides), then one
    /// faceplate per (controller, module instance) in `hw_components`
    /// declaration order. That is the compatibility path; its output must not
    /// change.
    ///
    /// A *non-empty* `inventory` (the resolved `RackSpec::module_keys`, row
    /// order then list order) is the chain order. A key names the same
    /// faceplate the patch produces for that controller and instance, so the
    /// instance's components land on the inventory position instead of a
    /// duplicate module; the master key names the master faceplate and one is
    /// prepended when absent. A key the patch does not address still gets its
    /// controller geometry, cells and `unaddressed_cells`, with no components.
    /// Patch modules the inventory does not name are appended in declaration
    /// order so no patch hardware disappears.
    ///
    /// `patch: None` therefore yields the inventory alone, or the master
    /// faceplate alone when the inventory is empty — a caller with no patch
    /// still has something to draw, and this never panics.
    ///
    /// Missing/malformed geometry data and unknown controllers degrade to the
    /// fallback width with a one-time warning — never a panic.
    pub fn build(patch: Option<&Patch>, model: MasterModel, inventory: &[ModuleKey]) -> Self {
        let data = match ControllerGeometry::load() {
            Ok(d) => d,
            Err(e) => {
                warn_once(
                    &DATA_WARN,
                    &format!("controller geometry data unavailable, using fallbacks: {e}"),
                );
                ControllerGeometry::fallback()
            }
        };
        let master_key = model.geometry_key();
        let fallback_w = data.fallback_width_mm();
        let fallback_h = data.fallback_height_mm();

        let mut master = master_faceplate(&data, master_key, fallback_w, fallback_h);

        // Patch faceplates in declaration order, grouped by (controller,
        // instance) exactly as the pre-inventory chain grouped them. CV I/O
        // components belong to the master faceplate, not a module of their own.
        let mut faceplates: Vec<PatchFaceplate> = Vec::new();
        if let Some(patch) = patch {
            for comp in &patch.hw_components {
                if comp.controller == "CV I/O" {
                    master.components.push(comp.clone());
                    continue;
                }
                let instance = comp.module_instance();
                if let Some(plate) = faceplates.iter_mut().find(|plate| {
                    plate.controller == comp.controller && plate.module_instance == instance
                }) {
                    plate.components.push(comp.clone());
                    continue;
                }
                faceplates.push(patch_faceplate(&data, comp, fallback_w, fallback_h));
            }
        }

        let mut modules: Vec<PhysicalModule> = Vec::new();
        if inventory.is_empty() {
            // Compatibility path: master at chain position 0, then the patch
            // chain in declaration order — byte-identical to the chain before
            // the rack inventory existed.
            modules.push(master);
            modules.extend(faceplates.into_iter().map(PatchFaceplate::into_module));
        } else {
            let mut placed = vec![false; faceplates.len()];
            let mut master_placed = false;
            for key in inventory {
                if is_master_key(key) {
                    // A repeated key would duplicate the faceplate and break
                    // the uniqueness `RackSpec.assign` relies on.
                    if master_placed {
                        continue;
                    }
                    master_placed = true;
                    modules.push(master.clone());
                    continue;
                }
                let found = faceplates.iter().enumerate().position(|(ix, plate)| {
                    !placed[ix]
                        && plate.geometry_key == key.controller
                        && plate.module_instance.map(|n| n as usize) == Some(key.instance)
                });
                if let Some(ix) = found {
                    placed[ix] = true;
                    modules.push(faceplates[ix].clone().into_module());
                } else {
                    modules.push(unaddressed_faceplate(&data, key, fallback_w, fallback_h));
                }
            }
            if !master_placed {
                modules.insert(0, master);
            }
            modules.extend(
                faceplates
                    .into_iter()
                    .enumerate()
                    .filter(|(ix, _)| !placed[*ix])
                    .map(|(_, plate)| plate.into_module()),
            );
        }

        // Lay the chain out contiguously: each module starts after the
        // previous module's width plus the inter-module gap.
        let gap = data.chain_gap_mm();
        let mut x = 0.0;
        for m in &mut modules {
            m.rect_mm.x_mm = x;
            x += m.rect_mm.w_mm + gap;
        }
        let total_width = x - gap;
        let total_height = modules.iter().map(|m| m.rect_mm.h_mm).fold(0.0, f64::max);

        Self {
            modules,
            total_width_mm: total_width.max(0.0),
            total_height_mm: total_height,
            chain_gaps_mm: data.meta.chain_gaps_mm.clone(),
            hp_mm: data.hp_mm(),
            he_mm: data.meta.he_mm.clone(),
            fallback_width_mm: fallback_w,
            fallback_height_mm: fallback_h,
            inventory_driven: !inventory.is_empty(),
        }
    }

    /// Look up the element cell for a hardware token on a module.
    ///
    /// Matching prefers the exact (element, kind letter) pair — this is what
    /// keeps "O4" on the O-side master jack while "I4" stays on the I-side —
    /// then falls back to any cell with the same element number, then to the
    /// 1-based positional cell within the family. Cells carrying no identity
    /// (no declared or label-derived element, or `addressable: false` in the
    /// geometry) are never a target, not even positionally. Returns `None` for
    /// fallback modules (no cells) and out-of-range elements.
    pub fn cell_for(&self, module_index: usize, token: &str) -> Option<&ElementCell> {
        let module = self.modules.get(module_index)?;
        let (kind_letter, element) = token_element(token)?;
        let mut family = family_for(kind_letter)?;
        if family == "P" && !module.cells.contains_key(family) {
            // Sliders/faders (p8s8, m4) live in the "F" family in the
            // geometry but are addressed with P registers in patches
            // (manual §6.8: "adressed with P1.1 through P1.8"; geometry
            // notes "sliders addressed via P registers").
            family = "F";
        }
        let cells = module.cells.get(family)?;
        cells
            .iter()
            .find(|c| c.element == Some(element) && c.kind_letter == Some(kind_letter))
            .or_else(|| cells.iter().find(|c| c.element == Some(element)))
            .or_else(|| {
                let index = cells.get(element.checked_sub(1)? as usize)?;
                index.element.is_some().then_some(index)
            })
    }
}

// ---------------------------------------------------------------------------
// Rack / case model (task 2.3) — rows, TE mounts, row assignment
// ---------------------------------------------------------------------------

/// Height of the fold-bar divider at each row boundary in mm. A model
/// constant the renderer maps with the same formula as the rows.
pub const FOLD_BAR_HEIGHT_MM: f64 = 5.0;

/// Names an inventory may use for the master faceplate. They are aliases, not
/// modules: whichever `MasterModel` the rack draws, the inventory names it once
/// and cannot pin the model by key.
const MASTER_ALIASES: &[&str] = &["master", "master18", "cv i/o"];

/// Canonical identity of the master faceplate in an inventory. One key for
/// every alias so a config need not know which master model is installed.
const MASTER_CONTROLLER_KEY: &str = "master";

/// One module named by the rack inventory: a controller plus its 1-based
/// circuit instance.
///
/// Canonical form: `controller` is a lowercase geometry key ("p2b8", "b32")
/// and the master faceplate is `{ controller: "master", instance: 1 }`,
/// whatever `MasterModel` the chain actually draws — `MASTER_ALIASES` covers
/// "master", "master18" and the patch vocabulary "CV I/O".
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct ModuleKey {
    /// Lowercase geometry key of the controller.
    pub controller: String,
    /// 1-based circuit instance within that controller.
    pub instance: usize,
}

impl ModuleKey {
    /// Parse one inventory entry: `"{controller} {instance}"` with a 1-based
    /// instance, or the bare controller for a single-instance module.
    ///
    /// Syntax only — the controller is lowercased but not checked against
    /// `CONTROLLER_KEYS`, so `RackSpec::module_keys` can report an unknown
    /// name by name instead of it vanishing here. An instance of 0, a
    /// non-numeric suffix, or a third field (a controller name with a space
    /// that is not a master alias) is invalid.
    pub fn parse(raw: &str) -> Option<Self> {
        let lowered = raw.trim().to_ascii_lowercase();
        if MASTER_ALIASES.contains(&lowered.as_str()) {
            return Some(Self {
                controller: String::from(MASTER_CONTROLLER_KEY),
                instance: 1,
            });
        }
        let mut parts = lowered.split_whitespace();
        let controller = parts.next()?;
        let instance = match parts.next() {
            None => 1,
            Some(n) => {
                let parsed: usize = n.parse().ok()?;
                if parsed == 0 || parts.next().is_some() {
                    return None;
                }
                parsed
            }
        };
        Some(Self {
            controller: String::from(controller),
            instance,
        })
    }
}

/// One case row. Height in HE (1 or 3), width in HP, optional label.
#[derive(Debug, Clone, Default, PartialEq, Deserialize, Serialize)]
pub struct RackRow {
    #[serde(default)]
    pub he: u32,
    #[serde(default)]
    pub hp: f64,
    #[serde(default)]
    pub label: Option<String>,
    /// Modules mounted in this row, left to right, as module keys (see
    /// `ModuleKey::parse`). Empty means today's behaviour: the row is filled
    /// from the chain. Skipped on save while empty so an existing config does
    /// not gain the key.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub modules: Vec<String>,
}

/// The case/rack definition (D9/D10). Pure data — the `[physical.rack]`
/// config parser (task 4.4) produces this from `config.toml`.
#[derive(Debug, Clone, Default, PartialEq, Deserialize, Serialize)]
pub struct RackSpec {
    /// Ordered rows. An empty list means "no rack configured" and packs as
    /// the default single-row case wide enough for the whole chain.
    #[serde(default)]
    pub rows: Vec<RackRow>,
    /// Width of the top-mount section in TE (1 TE = 1 HP = 5.08 mm).
    #[serde(default)]
    pub top_mount_te: f64,
    /// Width of the side-mount sections in TE.
    #[serde(default)]
    pub side_mount_te: f64,
    /// Per-module override: module key -> 0-based row index. An out-of-range
    /// row falls back to auto-pack. Key format is `"{controller} {instance}"`
    /// with a 1-based instance for humans (e.g. "P2B8 1"); the master
    /// faceplate has no instance and uses the bare controller name
    /// ("CV I/O"). See `PhysicalModule::key`.
    #[serde(default)]
    pub assign: HashMap<String, usize>,
}

impl RackSpec {
    /// The out-of-box case (D9): a single 3-HE row wide enough for the whole
    /// chain, no mounts, no overrides.
    pub fn default_case(chain: &PhysicalLayout) -> Self {
        let he = chain.modules.iter().map(|m| m.he).max().unwrap_or(3).max(1);
        let hp = (chain.total_width_mm / chain.hp_mm.max(0.001))
            .ceil()
            .max(1.0);
        Self {
            rows: vec![RackRow {
                he,
                hp,
                label: None,
                modules: Vec::new(),
            }],
            top_mount_te: 0.0,
            side_mount_te: 0.0,
            assign: HashMap::new(),
        }
    }

    /// Flat inventory: row order, then list order.
    ///
    /// The single reader of `RackRow.modules`. A malformed entry is skipped
    /// silently; a controller outside the registry is skipped with one
    /// process-wide warning per distinct name (see `warn_unknown_module`).
    /// Each resolved controller is stored as its canonical geometry key, which
    /// is what `PhysicalModule::geometry_key` carries for the same hardware.
    pub fn module_keys(&self) -> Vec<ModuleKey> {
        let mut keys = Vec::new();
        for row in &self.rows {
            for entry in &row.modules {
                let Some(parsed) = ModuleKey::parse(entry) else {
                    continue;
                };
                match controller_geometry_key(&parsed.controller) {
                    Some(key) => keys.push(ModuleKey {
                        controller: String::from(key),
                        instance: parsed.instance,
                    }),
                    None => warn_unknown_module(&parsed.controller),
                }
            }
        }
        keys
    }
}

impl PhysicalModule {
    /// Cells in `cells` no patch component addresses (unreferenced-elements
    /// D3): addressed-excluded via `token_element` over `components` ids, in
    /// deterministic family-key/row/col order. Skips `element: None` cells;
    /// fallback modules (empty `cells`) yield empty.
    pub fn unaddressed_cells(&self) -> Vec<&ElementCell> {
        use std::collections::HashSet;
        let addressed: HashSet<(char, u32)> = self
            .components
            .iter()
            .filter_map(|c| token_element(&c.id))
            .collect();
        let mut out: Vec<&ElementCell> = self
            .cells
            .values()
            .flatten()
            .filter(|c| match (c.kind_letter, c.element) {
                (Some(k), Some(e)) => !addressed.contains(&(k, e)),
                _ => false,
            })
            .collect();
        out.sort_by_key(|a| (a.family.clone(), a.row, a.col));
        out
    }

    /// Identity key for `RackSpec.assign`: controller + 1-based instance
    /// ("P2B8 1"), or the bare controller name when the faceplate carries no
    /// instance (the CV I/O master). Unique within a chain by construction.
    pub fn key(&self) -> String {
        match self.module_instance {
            Some(n) => format!("{} {}", self.controller, n),
            None => self.controller.clone(),
        }
    }
}

/// Row height in mm for a HE value, from the loaded `he_mm` map
/// (`fallback_height_mm` for unknown HE values).
fn row_height_mm(chain: &PhysicalLayout, he: u32) -> f64 {
    chain
        .he_mm
        .get(&he.to_string())
        .copied()
        .filter(|h| *h > 0.0)
        .unwrap_or(chain.fallback_height_mm)
}

/// A module placed into a rack row (absolute mm position).
#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
pub struct PlacedModule {
    /// Module key (see `PhysicalModule::key`), e.g. "P2B8 1".
    pub key: String,
    /// Index into `PhysicalLayout::modules`.
    pub module_index: usize,
    /// Absolute mm rect within the rack.
    pub rect_mm: RectMm,
    /// True when placed via `RackSpec::assign` instead of auto-pack.
    pub overridden: bool,
}

/// A resolved row of the rack: its spec plus the modules assigned to it.
#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
pub struct RackRowPlacement {
    pub he: u32,
    pub hp: f64,
    pub label: Option<String>,
    /// Y offset of the row's top edge in mm (below the top mount, after the
    /// previous row's fold bar).
    pub y_mm: f64,
    /// Row height in mm (`he_mm[he]`).
    pub height_mm: f64,
    /// Modules in chain order (overrides never reorder within a row).
    pub modules: Vec<PlacedModule>,
    /// Used width in mm (module widths + inter-module gaps).
    pub fill_width_mm: f64,
}

/// A horizontal fold-bar divider at a row boundary (D11).
#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
pub struct FoldBar {
    /// 0-based index of the row this boundary sits after.
    pub after_row: usize,
    /// Divider rect: spans the rows region, height = `FOLD_BAR_HEIGHT_MM`.
    pub rect_mm: RectMm,
}

/// Attached case sections (D11): top and side mount regions in mm.
#[derive(Debug, Clone, Default, PartialEq, Deserialize, Serialize)]
pub struct RackMounts {
    #[serde(default)]
    pub top: Option<RectMm>,
    #[serde(default)]
    pub side_left: Option<RectMm>,
    #[serde(default)]
    pub side_right: Option<RectMm>,
}

/// The resolved rack: rows with placed modules, fold bars, mounts, and the
/// overall mm bounds the renderer maps to screen (task 4.1).
#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
pub struct RackLayout {
    pub rows: Vec<RackRowPlacement>,
    pub fold_bars: Vec<FoldBar>,
    pub mounts: RackMounts,
    pub total_width_mm: f64,
    pub total_height_mm: f64,
    pub fold_bar_height_mm: f64,
}

impl RackLayout {
    /// Pack the chain into the rack's rows (D10).
    ///
    /// Modules auto-pack in chain order: the current row fills left-to-right
    /// until the next module would exceed the row's HP (capacity = hp ×
    /// `hp_mm`), then the next row starts; the last row accepts overflow. A
    /// `spec.assign` override forces a module into a row regardless of fit;
    /// an out-of-range override falls back to auto-pack. Overrides never
    /// reorder modules within a row. An empty `spec.rows` materializes the
    /// default single-row case. Deterministic — the assign map is consulted
    /// by key only, never iterated.
    pub fn pack(chain: &PhysicalLayout, spec: &RackSpec) -> Self {
        let spec = if spec.rows.is_empty() {
            RackSpec::default_case(chain)
        } else {
            spec.clone()
        };
        let hp_mm = chain.hp_mm.max(0.001);
        let gap = chain.chain_gaps_mm.inter_module.max(0.0);

        // Row assignment: fills[i] = used mm width; placements[i] in chain order.
        let mut fills = vec![0.0; spec.rows.len()];
        let mut placements: Vec<Vec<PlacedModule>> = vec![Vec::new(); spec.rows.len()];
        let mut cursor = 0usize;

        for (module_index, module) in chain.modules.iter().enumerate() {
            let key = module.key();
            let override_row = spec
                .assign
                .get(&key)
                .copied()
                .filter(|&r| r < spec.rows.len());
            let row = if let Some(r) = override_row {
                r
            } else {
                let mut r = cursor;
                loop {
                    let capacity = spec.rows[r].hp * hp_mm;
                    let row_empty = fills[r] <= 0.0;
                    let x = if row_empty { 0.0 } else { fills[r] + gap };
                    // An empty row always accepts its first module, even
                    // an over-wide one: otherwise a module wider than
                    // every configured row would skip to the last row and
                    // leave leading rows empty. A non-empty row fills
                    // until the module fits, with the last row accepting
                    // overflow (D10).
                    if row_empty || x + module.rect_mm.w_mm <= capacity || r + 1 >= spec.rows.len()
                    {
                        break r;
                    }
                    r += 1;
                }
            };
            let x = if fills[row] > 0.0 {
                fills[row] + gap
            } else {
                fills[row]
            };
            placements[row].push(PlacedModule {
                key,
                module_index,
                rect_mm: RectMm {
                    x_mm: x,
                    y_mm: 0.0,
                    w_mm: module.rect_mm.w_mm,
                    h_mm: module.rect_mm.h_mm,
                },
                overridden: override_row.is_some(),
            });
            fills[row] = x + module.rect_mm.w_mm;
            if override_row.is_none() {
                cursor = row;
            }
        }

        // Vertical mm geometry: top mount, then rows separated by fold bars.
        let rows_width = spec.rows.iter().map(|r| r.hp * hp_mm).fold(0.0, f64::max);
        let top_mount_h = spec.top_mount_te * hp_mm;
        let side_mount_w = spec.side_mount_te * hp_mm;

        let mut y = top_mount_h;
        let mut rows: Vec<RackRowPlacement> = Vec::with_capacity(spec.rows.len());
        let mut fold_bars: Vec<FoldBar> = Vec::new();
        for (i, row) in spec.rows.iter().enumerate() {
            rows.push(RackRowPlacement {
                he: row.he,
                hp: row.hp,
                label: row.label.clone(),
                y_mm: y,
                height_mm: row_height_mm(chain, row.he),
                modules: std::mem::take(&mut placements[i]),
                fill_width_mm: fills[i],
            });
            y += rows[i].height_mm;
            if i + 1 < spec.rows.len() {
                fold_bars.push(FoldBar {
                    after_row: i,
                    rect_mm: RectMm {
                        x_mm: 0.0,
                        y_mm: y,
                        w_mm: rows_width,
                        h_mm: FOLD_BAR_HEIGHT_MM,
                    },
                });
                y += FOLD_BAR_HEIGHT_MM;
            }
        }

        let rows_region_h = y - top_mount_h;
        let total_height = y;
        let total_width = rows_width + 2.0 * side_mount_w;

        let mounts = RackMounts {
            top: (spec.top_mount_te > 0.0).then_some(RectMm {
                x_mm: 0.0,
                y_mm: 0.0,
                w_mm: rows_width,
                h_mm: top_mount_h,
            }),
            side_left: (side_mount_w > 0.0).then_some(RectMm {
                x_mm: 0.0,
                y_mm: top_mount_h,
                w_mm: side_mount_w,
                h_mm: rows_region_h,
            }),
            side_right: (side_mount_w > 0.0).then_some(RectMm {
                x_mm: rows_width,
                y_mm: top_mount_h,
                w_mm: side_mount_w,
                h_mm: rows_region_h,
            }),
        };

        Self {
            rows,
            fold_bars,
            mounts,
            total_width_mm: total_width,
            total_height_mm: total_height,
            fold_bar_height_mm: FOLD_BAR_HEIGHT_MM,
        }
    }
}

// ---------------------------------------------------------------------------
// DB8E OLED display placeholder — patch-derived state text (task 2.1)
// ---------------------------------------------------------------------------

/// DB8E OLED display placeholder state heuristic (proposal
/// `db8e-oled-display-placeholder`, task 2.1).
///
/// Location choice: `physical.rs` rather than `patch.rs` because the heuristic
/// must inspect both `[db8e]` presence (patch sections) and the derived
/// `PhysicalLayout` (`geometry_key` == "db8e"). `physical` already depends on
/// `patch` (ARCHITECTURE dependency direction), while `patch` must not import
/// `physical`/`geometry` — keeping the helper here preserves the direction and
/// keeps both modules pure (no terminal/UI import, no `App` dependency).
///
/// Heuristic (YAGNI, no live circuit):
/// - no `[db8e]` / no `PhysicalLayout` module with `geometry_key == "db8e"`
///   → `"not used"` (manual ch. 6.5.5 *not used by patch*);
/// - else if declared chain mismatches wired chain (stub: deterministic mismatch
///   check, fallback to `"connected"` when ambiguous) → `"configuration error"`;
/// - else → `"connected"` baseline.
///
/// Stub mismatch is deliberately conservative: it only returns
/// `"configuration error"` when the patch/layout explicitly carries a `mismatch`
/// marker (case-insensitive key/value or raw line containing "mismatch"). All
/// ambiguous cases fall through to `"connected"` so valid multi-controller
/// patches never false-positive.
///
/// Pure function — no I/O, no terminal dependency, deterministic.
pub fn db8e_display_state(patch: &Patch) -> &'static str {
    if !has_db8e_patch(patch) {
        return "not used";
    }
    if is_mismatch_stub_patch(patch) {
        return "configuration error";
    }
    "connected"
}

/// Layout-based counterpart of [`db8e_display_state`]: inspects the derived
/// `PhysicalLayout` instead of the raw patch. Same three-state contract, same
/// stub fallback. Preferred when a `PhysicalLayout` is already built for
/// rendering (`render_rack_structure` can call this directly from the layout
/// without re-inspecting the patch).
pub fn db8e_display_state_for_layout(layout: &PhysicalLayout) -> &'static str {
    if !has_db8e_layout(layout) {
        return "not used";
    }
    if is_mismatch_stub_layout(layout) {
        return "configuration error";
    }
    "connected"
}

fn has_db8e_patch(patch: &Patch) -> bool {
    patch.sections.iter().any(|s| s.name == "db8e")
}

fn has_db8e_layout(layout: &PhysicalLayout) -> bool {
    layout.modules.iter().any(|m| m.geometry_key == "db8e")
}

fn is_mismatch_stub_patch(patch: &Patch) -> bool {
    // Raw lines cover comments/bare markers; entries cover structured params.
    if patch
        .raw_lines
        .iter()
        .any(|l| l.to_ascii_lowercase().contains("mismatch"))
    {
        return true;
    }
    patch.sections.iter().any(|s| {
        s.entries.iter().any(|(k, v)| {
            k.to_ascii_lowercase().contains("mismatch")
                || v.to_ascii_lowercase().contains("mismatch")
        })
    })
}

fn is_mismatch_stub_layout(layout: &PhysicalLayout) -> bool {
    layout.modules.iter().any(|m| {
        m.geometry_key.to_ascii_lowercase().contains("mismatch")
            || m.controller.to_ascii_lowercase().contains("mismatch")
            || m.components.iter().any(|c| {
                c.id.to_ascii_lowercase().contains("mismatch")
                    || c.label.to_ascii_lowercase().contains("mismatch")
            })
    })
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

// ---------------------------------------------------------------------------
// mm→screen mapping (D4/D5)
// ---------------------------------------------------------------------------

/// True-1:1 isotropic mm→screen factor (module-ui-labels D1): the 96-dpi
/// reference, one millimetre is 3.7795 points, so 25.4 mm is exactly 96 pt at
/// 100 % zoom. One value for both axes keeps physical proportions true.
pub const PHYSICAL_POINTS_PER_MM: f64 = 3.779_527_559;

/// Supported physical-view zoom range (module-ui-labels D5): [0.25, 4.0] of
/// the true-1:1 baseline, so a 400 mm rack and a two-row rack fit through the
/// lower end while a short rack can open slightly over true size.
pub const ZOOM_MIN: f64 = 0.25;
pub const ZOOM_MAX: f64 = 4.0;

/// The pure mm→screen mapping of the physical presentation (D4/D5). No
/// terminal dependency: screen rects are returned as f64 points and rounded
/// only at draw time. One formula drives both the skeleton reference and the
/// full view, keeping the 5.1 coincidence contract exact by construction.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ScreenMapping {
    pub points_per_mm: f64,
    pub zoom: f64,
    pub offset_x: f64,
    pub offset_y: f64,
}

impl Default for ScreenMapping {
    fn default() -> Self {
        Self {
            points_per_mm: PHYSICAL_POINTS_PER_MM,
            zoom: 1.0,
            offset_x: 0.0,
            offset_y: 0.0,
        }
    }
}

impl ScreenMapping {
    pub fn new(points_per_mm: f64, zoom: f64, offset_x: f64, offset_y: f64) -> Self {
        Self {
            points_per_mm,
            zoom,
            offset_x,
            offset_y,
        }
    }

    /// mm rect → screen rect `(x, y, w, h)` in points (D5:
    /// screen = mm × points-per-mm × zoom − offset; size is scale-only).
    pub fn mm_to_screen(&self, mm: RectMm) -> (f64, f64, f64, f64) {
        (
            mm.x_mm * self.points_per_mm * self.zoom - self.offset_x,
            mm.y_mm * self.points_per_mm * self.zoom - self.offset_y,
            mm.w_mm * self.points_per_mm * self.zoom,
            mm.h_mm * self.points_per_mm * self.zoom,
        )
    }

    /// Integer point rect for an absolute mm rect: the left/right (and
    /// top/bottom) edges round independently from the absolute mm span
    /// (`round(mm0×f×zoom − offset) .. round(mm1×f×zoom − offset)`), so
    /// adjacent rects sharing a boundary abut exactly at every zoom instead
    /// of accumulating rounded widths. Sub-cell spans clamp to 1 cell so
    /// slivers still draw; edges off the top/left clamp to 0.
    pub fn mm_rect_to_screen(&self, mm: RectMm) -> (u16, u16, u16, u16) {
        let x0 = (mm.x_mm * self.points_per_mm * self.zoom - self.offset_x).round() as i64;
        let x1 =
            ((mm.x_mm + mm.w_mm) * self.points_per_mm * self.zoom - self.offset_x).round() as i64;
        let y0 = (mm.y_mm * self.points_per_mm * self.zoom - self.offset_y).round() as i64;
        let y1 =
            ((mm.y_mm + mm.h_mm) * self.points_per_mm * self.zoom - self.offset_y).round() as i64;
        // Both edges clamp before the span is derived: a rect partially
        // off-screen left must not keep width from its unclamped origin
        // (that would extend it past its true right edge and overlap the
        // next module).
        let x0c = x0.max(0);
        let x1c = x1.max(0);
        let y0c = y0.max(0);
        let y1c = y1.max(0);
        (
            x0c as u16,
            y0c as u16,
            (x1c - x0c).max(1) as u16,
            (y1c - y0c).max(1) as u16,
        )
    }

    /// Inverse of `mm_to_screen` for a screen point (the round-trip test and
    /// the zoom-anchor math both use it).
    pub fn screen_to_mm(&self, x: f64, y: f64) -> (f64, f64) {
        (
            (x + self.offset_x) / (self.points_per_mm * self.zoom),
            (y + self.offset_y) / (self.points_per_mm * self.zoom),
        )
    }

    /// Zoom about a fixed screen anchor: returns a mapping with `new_zoom`
    /// and the offset adjusted so the mm point under `(ax, ay)` stays at the
    /// same screen position. Deterministic pure math (no RNG).
    #[must_use]
    pub fn zoom_about(&self, new_zoom: f64, ax: f64, ay: f64) -> Self {
        let (mx, my) = self.screen_to_mm(ax, ay);
        Self {
            zoom: new_zoom,
            offset_x: mx * self.points_per_mm * new_zoom - ax,
            offset_y: my * self.points_per_mm * new_zoom - ay,
            ..*self
        }
    }

    /// Pan by `(dx, dy)` screen cells: screen coords shift linearly by
    /// exactly `(dx, dy)` (and the inverse shifts the other way).
    #[must_use]
    pub fn pan(&self, dx: f64, dy: f64) -> Self {
        Self {
            offset_x: self.offset_x + dx,
            offset_y: self.offset_y + dy,
            ..*self
        }
    }

    /// Screen size `(w, h)` in points of an mm rect under this mapping —
    /// overflow detection compares this against the visible area.
    pub fn screen_size(&self, mm: RectMm) -> (f64, f64) {
        (
            mm.w_mm * self.points_per_mm * self.zoom,
            mm.h_mm * self.points_per_mm * self.zoom,
        )
    }
}

/// Fit zoom for a rack of `rack_w_mm` × `rack_h_mm` in a `pane_w` × `pane_h`
/// point pane (module-ui-labels D3): the smaller of the width and height
/// ratios against the true-1:1 baseline, clamped to [`ZOOM_MIN`]..=[`ZOOM_MAX`].
/// A rack too large for the floor does not fully fit; pan covers it.
pub fn fit_zoom(rack_w_mm: f64, rack_h_mm: f64, pane_w: f32, pane_h: f32) -> f64 {
    if rack_w_mm <= 0.0 || rack_h_mm <= 0.0 || pane_w <= 0.0 || pane_h <= 0.0 {
        return ZOOM_MIN;
    }
    let by_width = f64::from(pane_w) / (rack_w_mm * PHYSICAL_POINTS_PER_MM);
    let by_height = f64::from(pane_h) / (rack_h_mm * PHYSICAL_POINTS_PER_MM);
    by_width.min(by_height).clamp(ZOOM_MIN, ZOOM_MAX)
}

/// Pane-local offset that centres a `rack_w_mm` × `rack_h_mm` rack in a
/// `pane_w` × `pane_h` point pane at `zoom` (module-ui-labels D3). The offset
/// uses the mapping's subtract-on-screen convention (`screen = mm·ppm·zoom −
/// offset`), so a rack smaller than the pane yields a negative offset and lands
/// centred, while a larger rack yields a positive one.
pub fn fit_offset(
    rack_w_mm: f64,
    rack_h_mm: f64,
    pane_w: f32,
    pane_h: f32,
    zoom: f64,
) -> (f32, f32) {
    let rack_w = rack_w_mm * PHYSICAL_POINTS_PER_MM * zoom;
    let rack_h = rack_h_mm * PHYSICAL_POINTS_PER_MM * zoom;
    (
        ((rack_w - f64::from(pane_w)) / 2.0) as f32,
        ((rack_h - f64::from(pane_h)) / 2.0) as f32,
    )
}

/// The rack's rendered extent in points at `zoom` (module-ui-labels D3/D5):
/// the single live-geometry source behind fit, overflow, and pan gating, so no
/// stale screen-size field can disagree with the mapping that draws the rack.
pub fn rack_extent_points(rack: &RackLayout, zoom: f64) -> (f32, f32) {
    (
        (rack.total_width_mm * PHYSICAL_POINTS_PER_MM * zoom) as f32,
        (rack.total_height_mm * PHYSICAL_POINTS_PER_MM * zoom) as f32,
    )
}

// ---------------------------------------------------------------------------
// Per-cell label layout (module-ui-label-fit D1–D6)
// ---------------------------------------------------------------------------

/// The descending label font ladder in points (module-ui-label-fit D2/D3):
/// every cell takes the largest step at which its whole label fits its own text
/// box, wrapping at word boundaries before it considers a smaller step.
const LABEL_FONT_LADDER: [f32; 5] = [10.0, 9.0, 8.0, 7.0, 6.0];

/// The ladder's floor in points — the smallest text the surface may draw
/// (D3/D10). 6 pt is roughly 8 px on a 1x display, and it is the smallest size
/// at which a 3.6 pt character advance still fits inside a 7 mm jack at 100 %
/// zoom. A cell that cannot hold 6 pt keeps its state and loses its in-cell
/// label, which stays reachable through the annotation layer and the hover
/// reveal (D9) instead of shrinking further.
const LABEL_FONT_FLOOR: f32 = 6.0;

/// Monospace advance as a fraction of the font size (egui's monospace face is
/// close to 0.6 em — mirrors `gui::physical`'s `MONO_ADVANCE`). Every budget is
/// a multiplication of this, which is what keeps [`label_layout`] free of any
/// rendered-text measurement and therefore pure.
const LABEL_MONO_ADVANCE: f32 = 0.6;

/// Line height as a multiple of the font size (the painter's `× 1.25`).
const LABEL_LINE_HEIGHT: f32 = 1.25;

/// Left inset of a cell's text box in points: the compact and the composed
/// paint path both draw their text at `rect.min.x + 2.0`.
const LABEL_INSET_X: f32 = 2.0;

/// Composed face (D6): the state shape occupies the cell's top `0.5 × w`, so the
/// label's band beneath it is `h − 0.5 × w` tall and starts at the cell's bottom
/// edge. The shape itself is never shrunk for the label.
const COMPOSED_FACE_HEIGHT_FRACTION: f32 = 0.5;

/// Fader (D5): the track may claim at most this fraction of the cell width, so a
/// tall-narrow fader leaves the label a real column beside it.
const FADER_TRACK_WIDTH_FRACTION: f32 = 0.4;

/// Fader (D5): the narrowed track never exceeds the 10 pt the paint path has
/// always clamped it to, so a wide fader keeps its present face.
const FADER_TRACK_WIDTH_MAX: f32 = 10.0;

/// Gutter between the fader's narrowed track and its label column, in points.
const FADER_TRACK_GUTTER: f32 = 3.0;

/// Fader (D5): a column narrower than this many character advances cannot hold
/// a line of text — two glyphs is the shortest run that still reads as text
/// rather than as a stray mark — so the label stacks one glyph per line instead.
const FADER_HORIZONTAL_MIN_CHARS: f32 = 2.0;

/// State row (D4): the state text is proportional at `max(9, 0.8 × label font)`.
const STATE_FONT_SCALE: f32 = 0.8;

/// State row (D4): the floor of the proportional state font, in points.
const STATE_FONT_MIN: f32 = 9.0;

/// State row (D4/D5): the narrowest text column that can carry a value, in
/// character advances of the state font. One advance is not a value — a column
/// that holds a single glyph makes the painter emit the glyph plus its ellipsis
/// (`…`), which names no number — so a cell whose column holds fewer than two
/// advances reports `state_row = false` and hands the room back to the label.
/// Compact and composed cells never hit this gate; it exists for the fader,
/// whose label column the track and gutter can narrow past a usable value.
const STATE_ROW_MIN_CHARS: f32 = 2.0;

/// Tolerance in points for the box comparisons: a line exactly as wide or tall
/// as the box still fits, so float rounding never costs a cell its last
/// character.
const LABEL_FIT_TOLERANCE: f32 = 0.01;

/// The mark appended to the last line when only the floor is left (D2).
const LABEL_ELLIPSIS: char = '\u{2026}';

/// The ellipsized form's legibility floor (D2/D10): the last line keeps at
/// least this many visible characters ahead of its ellipsis. A one- or
/// two-character fragment names nothing — `G…` is not `Gate`, it is noise on a
/// faceplate — so a box too small to show three characters plus the mark gets no
/// in-cell layout at all. The cell then draws its state alone and the whole
/// label stays reachable through the annotation layer and the hover reveal (D9),
/// which is the same fallback every other unserved cell uses.
const LABEL_ELLIPSIS_MIN_VISIBLE: usize = 3;

/// The fit's candidate grid (D7) in thousandths of the 1:1 baseline: the scan
/// walks [`ZOOM_MIN`]..=[`ZOOM_MAX`] in 1 % steps, so at most 376 candidates
/// bound it and the answer never lands on a division the user could not also
/// dial by hand — no bisection tolerance, no drift between two calls.
const LABEL_FIT_STEP_MILLIS: u32 = 10;

/// One whole zoom (100 %) in the fit grid's units.
const LABEL_FIT_MILLIS_PER_ZOOM: f64 = 1000.0;

/// Which paint path owns a cell's label, i.e. which text box the label is
/// measured against (D1/D5/D6). No egui type crosses this boundary.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CellTextPath {
    /// Compact cell: the cell minus `LABEL_INSET_X`, with the state glyph and
    /// one separating space at the head of line 1 when `glyph_chars > 0`.
    Compact,
    /// Composed button face: the shape keeps the cell, the label takes the band
    /// beneath it.
    Composed,
    /// Fader: the label column right of the narrowed track, stacked one glyph
    /// per line when the column is too narrow for a line of text.
    Fader,
}

/// One cell's label box as the painter will use it, plus the three facts that
/// change the budget (D1): the glyph occupying the head of line 1, the paint
/// path, and whether a state string exists. Pure geometry — the label text is a
/// separate argument to [`label_layout`], so the same box can be evaluated for
/// the label's current value or for any candidate zoom.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LabelBox {
    /// Drawn cell width in points.
    pub w: f32,
    /// Drawn cell height in points.
    pub h: f32,
    /// Characters of the state glyph drawn at the head of line 1 (`0` when the
    /// cell has no glyph, e.g. a composed face).
    pub glyph_chars: usize,
    /// Which paint path's text box applies.
    pub path: CellTextPath,
    /// Whether the cell has a state string the label may have to yield to (D4).
    pub has_state: bool,
}

/// The resolved label for one cell (D1): the single source the painter, the
/// annotation layer, the hover reveal and the fit clamp all read instead of
/// re-deriving a character budget.
#[derive(Debug, Clone, PartialEq)]
pub struct LabelLayout {
    /// The ladder step in points, never below `LABEL_FONT_FLOOR`.
    pub font: f32,
    /// The label's lines, top to bottom: wrapped at spaces, hard-broken only for
    /// a word wider than its line, and ellipsized on the last line only at the
    /// floor. In the stacked form each entry is one glyph.
    pub lines: Vec<String>,
    /// One glyph per line beside a narrowed fader track (D5).
    pub stacked: bool,
    /// A state line still fits with the label (D4); `false` when the label
    /// needed the room and the state row yielded.
    pub state_row: bool,
}

/// The fitted label of one cell, or `None` when the cell cannot show its label
/// even at `LABEL_FONT_FLOOR` — the annotation predicate of the label
/// presentation (D1/D9).
///
/// Passes, in order of preference (D2/D4):
///
/// 1. every ladder step with one state line reserved, when the cell has state
///    *and* its column can carry a value (see [`state_row_can_show_a_value`]);
/// 2. every ladder step across the whole text box (the state row yields);
/// 3. the floor, ellipsized on the last line — only here may text be dropped,
///    because only here does no step hold the whole label (D2).
///
/// `None` means not even one character fits the box at the floor (a 3 mm LED
/// cell at fit zoom, the glyph leaving no room on line 1) or the label is empty;
/// the label then stays reachable through the annotation layer and hover.
/// Pure and deterministic: no egui, no paint state, same input → same output.
pub fn label_layout(label: &str, area: &LabelBox) -> Option<LabelLayout> {
    if label.trim().is_empty() {
        return None;
    }
    let (_, height) = label_text_box(area)?;

    // D4: the state row is the first thing tried and the first thing given up —
    // and it is not tried at all when the column cannot carry a value (D5).
    if area.has_state {
        for &font in &LABEL_FONT_LADDER {
            if !state_row_can_show_a_value(area, font) {
                break;
            }
            let reserved = label_line_height(state_font_size(font));
            if let Some((lines, stacked)) = fit_at(label, area, font, height - reserved) {
                return Some(LabelLayout {
                    font,
                    lines,
                    stacked,
                    state_row: true,
                });
            }
        }
    }

    for &font in &LABEL_FONT_LADDER {
        if let Some((lines, stacked)) = fit_at(label, area, font, height) {
            return Some(LabelLayout {
                font,
                lines,
                stacked,
                state_row: false,
            });
        }
    }

    ellipsized_layout(label, area, height)
}

/// Line height in points for a label font step (`1.25 × font`).
///
/// Public so the painter stacks lines and the state row at exactly the geometry
/// the layout was fitted against instead of re-deriving it.
pub fn label_line_height(font: f32) -> f32 {
    font * LABEL_LINE_HEIGHT
}

/// The proportional state-row font for a fitted label step (D4):
/// `max(9, 0.8 × label font)`. Shared with the painter for the same reason.
pub fn state_font_size(label_font: f32) -> f32 {
    (label_font * STATE_FONT_SCALE).max(STATE_FONT_MIN)
}

/// Whether this cell's text column can carry a state value at all (D4/D5),
/// measured in advances of the state font for the candidate label step.
///
/// Compact and composed cells always qualify: their state row shares the label's
/// own line width, which the ladder has already shown holds text. A fader's
/// column is separately narrowed by the 40 % track and the gutter, so it can be
/// narrower than one usable value while still stacking the label beside it —
/// that cell reports no state row rather than painting `…`.
pub fn state_row_can_show_a_value(area: &LabelBox, font: f32) -> bool {
    match area.path {
        CellTextPath::Compact | CellTextPath::Composed => true,
        CellTextPath::Fader => {
            let column = fader_column_width(area.w);
            column + LABEL_FIT_TOLERANCE
                >= STATE_ROW_MIN_CHARS * label_char_advance(state_font_size(font))
        }
    }
}

/// One written label's claim on the fit (D7): the cell's drawn size **at 100 %
/// zoom** plus the facts [`LabelBox`] needs, and the label text. The fit scales
/// `w`/`h` by each candidate zoom and asks [`label_layout`], so one request
/// answers every candidate and the caller never rebuilds boxes per zoom.
///
/// Only *written* labels become requests (`Patch::explicit_hw_label`, D7): a
/// derived token name is text nobody asked the surface to show, and letting it
/// lift the fit would zoom the view for a label that only exists on hover.
#[derive(Debug, Clone, PartialEq)]
pub struct LabelFitRequest {
    /// Cell width in points at 100 % zoom.
    pub w: f32,
    /// Cell height in points at 100 % zoom.
    pub h: f32,
    /// Characters of the state glyph at the head of line 1 (D1), `0` when the
    /// cell has no glyph.
    pub glyph_chars: usize,
    /// Which paint path's text box applies (D1/D5/D6).
    pub path: CellTextPath,
    /// Whether the cell has a state string the label may have to yield to (D4).
    pub has_state: bool,
    /// The written label.
    pub label: String,
}

impl LabelFitRequest {
    /// This request's label box at one candidate zoom: the 100 % size scaled by
    /// `zoom`, the same isotropic factor the painter maps the cell through.
    fn label_box_at(&self, zoom: f64) -> LabelBox {
        let scale = zoom as f32;
        LabelBox {
            w: self.w * scale,
            h: self.h * scale,
            glyph_chars: self.glyph_chars,
            path: self.path,
            has_state: self.has_state,
        }
    }

    /// Whether this cell draws its **whole** label at `zoom` (D2/D7).
    ///
    /// [`label_layout`] answers `Some` even when it had to ellipsize the last
    /// line or drop lines it could not place — that is D2's "no step holds it"
    /// state, i.e. a cell painting `…` instead of a name. Whole-ness of the text
    /// is therefore the predicate the fit needs, not `is_some`: a zoom only
    /// earns its place in the fit when the label is actually drawn.
    fn draws_whole_label_at(&self, zoom: f64) -> bool {
        let Some(layout) = label_layout(&self.label, &self.label_box_at(zoom)) else {
            return false;
        };
        layout
            .lines
            .iter()
            .flat_map(|line| line.chars())
            .filter(|c| !c.is_whitespace())
            .eq(self.label.chars().filter(|c| !c.is_whitespace()))
    }
}

/// The label fit (D7): the smallest zoom in [`ZOOM_MIN`]..=[`ZOOM_MAX`] at which
/// every written label's cell yields a layout from the same [`label_layout`] the
/// painter uses **and draws that label whole**, or `None` when no request
/// constrains the fit.
///
/// "The label fit" is the zoom at which the ladder no longer has to give up on
/// a label the rack fit shrank away, i.e. the readable zoom: the app takes the
/// greater of it and [`fit_zoom`], so a label that only fits when zoomed in
/// lifts the default view and the rack overflows and pans instead of the text
/// vanishing. Drawing the label whole is the point — an ellipsized floor layout
/// (D2's "no step holds it") already appears at zooms far below the rack fit and
/// would report every cell as served while painting `…`.
///
/// A request that no zoom up to `ZOOM_MAX` draws whole is *ignored*, not
/// clamped (D7/D9): no supported zoom can serve that cell, so it must neither
/// reach the ceiling nor drag the fit above a label that does fit. It stays an
/// annotation candidate, and the fit is decided by the labels that can be
/// served. A fit the floor already serves is reported as `None` — there is
/// nothing to lift and the rack fit answers alone.
///
/// Bounded and deterministic: one fixed ascending grid of 1 % zoom steps
/// (`LABEL_FIT_STEP_MILLIS`) from [`ZOOM_MIN`] to [`ZOOM_MAX`] — at most 376
/// candidates, and the check fails fast on the first cell that does not fit, so
/// the cost is roughly one [`label_layout`] call per candidate plus one per
/// request. The grid is exact (thousandths of the 1:1 baseline, never a running
/// float sum), so the same input always answers the same zoom.
pub fn label_fit_zoom(requests: &[LabelFitRequest]) -> Option<f64> {
    let first = (ZOOM_MIN * LABEL_FIT_MILLIS_PER_ZOOM).round() as u32;
    let cap = (ZOOM_MAX * LABEL_FIT_MILLIS_PER_ZOOM).round() as u32;
    let cap_zoom = f64::from(cap) / LABEL_FIT_MILLIS_PER_ZOOM;

    // D7/D9: no whole layout at the ceiling means no zoom can serve the cell —
    // it is ignored rather than clamped, so it can neither push the fit to the
    // cap nor hold it above a label that does fit.
    let served: Vec<&LabelFitRequest> = requests
        .iter()
        .filter(|request| request.draws_whole_label_at(cap_zoom))
        .collect();
    if served.is_empty() {
        return None;
    }

    let mut millis = first;
    loop {
        let zoom = f64::from(millis) / LABEL_FIT_MILLIS_PER_ZOOM;
        if served
            .iter()
            .all(|request| request.draws_whole_label_at(zoom))
        {
            // A fit the floor already serves constrains nothing.
            return (zoom > ZOOM_MIN).then_some(zoom);
        }
        if millis >= cap {
            // Unreachable: every surviving request fits at the ceiling, which is
            // the last candidate. Kept so the function stays total.
            return None;
        }
        millis = (millis + LABEL_FIT_STEP_MILLIS).min(cap);
    }
}

/// One character of a monospace label font step, in points.
fn label_char_advance(font: f32) -> f32 {
    font * LABEL_MONO_ADVANCE
}

/// The fader's track width under the narrowed contract (D5).
fn fader_track_width(w: f32) -> f32 {
    (w * FADER_TRACK_WIDTH_FRACTION).min(FADER_TRACK_WIDTH_MAX)
}

/// The label column right of a fader's narrowed track and its gutter (D5).
fn fader_column_width(w: f32) -> f32 {
    w - fader_track_width(w) - FADER_TRACK_GUTTER
}

/// The text box a path leaves for the label, in points (D1/D5/D6).
fn label_text_box(area: &LabelBox) -> Option<(f32, f32)> {
    let width = match area.path {
        CellTextPath::Compact | CellTextPath::Composed => area.w - LABEL_INSET_X,
        CellTextPath::Fader => fader_column_width(area.w),
    };
    let height = match area.path {
        CellTextPath::Compact | CellTextPath::Fader => area.h,
        CellTextPath::Composed => area.h - COMPOSED_FACE_HEIGHT_FRACTION * area.w,
    };
    if width <= 0.0 || height <= 0.0 {
        None
    } else {
        Some((width, height))
    }
}

/// Characters a line holds at `font`: the first line of a compact cell also
/// carries the state glyph and one separating space, so it is shorter by that
/// prefix (D1). Returns `(head_capacity, capacity)`.
fn line_capacities(width: f32, font: f32, glyph_chars: usize) -> (usize, usize) {
    let advance = label_char_advance(font);
    if advance <= 0.0 || width <= 0.0 {
        return (0, 0);
    }
    let chars = |w: f32| ((w / advance) + LABEL_FIT_TOLERANCE).floor().max(0.0) as usize;
    let capacity = chars(width);
    let head = if glyph_chars == 0 {
        capacity
    } else {
        let prefix = (glyph_chars + 1) as f32 * advance;
        chars(width - prefix)
    };
    (head, capacity)
}

/// Greedy word wrap at spaces (D2): a line is filled until the next whole word
/// no longer fits, and a word wider than a line on its own is hard-broken so no
/// character is lost. `None` when no text can be placed at all — the first line
/// has no room after the glyph prefix.
fn wrap_lines(label: &str, head_cap: usize, capacity: usize) -> Option<Vec<String>> {
    let mut lines: Vec<String> = Vec::new();
    let mut line: Vec<char> = Vec::new();
    let mut cap_now = head_cap;
    for word in label.split_whitespace() {
        let word: Vec<char> = word.chars().collect();
        let mut taken = 0;
        while taken < word.len() {
            if line.is_empty() {
                if cap_now == 0 {
                    return None;
                }
                // A word that exceeds a line on its own is broken here; a word
                // that fits is placed whole.
                let take = cap_now.min(word.len() - taken);
                line.extend_from_slice(&word[taken..taken + take]);
                taken += take;
            } else if line.len() + 1 + (word.len() - taken) <= cap_now {
                line.push(' ');
                line.extend_from_slice(&word[taken..]);
                taken = word.len();
            } else {
                // The whole word does not fit behind the space: close the line
                // and retry it on a fresh, full-width one.
                lines.push(line.iter().collect());
                line.clear();
                cap_now = capacity;
                continue;
            }
            if taken < word.len() {
                // The line filled up mid-word: close it and continue the word on
                // the next line.
                lines.push(line.iter().collect());
                line.clear();
                cap_now = capacity;
            }
        }
    }
    if !line.is_empty() {
        lines.push(line.iter().collect());
    }
    Some(lines)
}

/// The stacked form (D5): one glyph per line, whitespace dropped because a
/// column one character wide cannot show a space.
fn stacked_lines(label: &str) -> Vec<String> {
    label
        .chars()
        .filter(|c| !c.is_whitespace())
        .map(|c| c.to_string())
        .collect()
}

/// The lines one font step produces for a box, ignoring height: `(lines,
/// stacked)`. `None` when the step leaves no room for text.
fn wrapped_lines(label: &str, area: &LabelBox, font: f32) -> Option<(Vec<String>, bool)> {
    let (width, _) = label_text_box(area)?;
    let advance = label_char_advance(font);
    if advance <= 0.0 {
        return None;
    }
    if area.path == CellTextPath::Fader && width < FADER_HORIZONTAL_MIN_CHARS * advance {
        // D5: the column cannot hold a line of text, so it stacks one glyph per
        // line — the only orientation that fits a tall-narrow cell.
        if width + LABEL_FIT_TOLERANCE < advance {
            return None;
        }
        return Some((stacked_lines(label), true));
    }
    let (head, capacity) = line_capacities(width, font, area.glyph_chars);
    wrap_lines(label, head, capacity).map(|lines| (lines, false))
}

/// Whether `lines` fit `avail_h` at `font`.
fn lines_fit_height(lines: &[String], font: f32, avail_h: f32) -> bool {
    let line_h = label_line_height(font);
    if line_h <= 0.0 || avail_h <= 0.0 {
        return false;
    }
    lines.len() as f32 * line_h <= avail_h + LABEL_FIT_TOLERANCE
}

/// One ladder step against one available height: the wrapped lines when they
/// fit, else `None`.
fn fit_at(label: &str, area: &LabelBox, font: f32, avail_h: f32) -> Option<(Vec<String>, bool)> {
    let (lines, stacked) = wrapped_lines(label, area, font)?;
    if lines_fit_height(&lines, font, avail_h) {
        Some((lines, stacked))
    } else {
        None
    }
}

/// A line truncated to `capacity` characters with the floor ellipsis closing it;
/// `None` when the line cannot hold [`LABEL_ELLIPSIS_MIN_VISIBLE`] visible
/// characters plus the ellipsis itself — such a fragment identifies nothing, so
/// the cell is better left to the annotation and hover reveal (D9).
fn truncate_with_ellipsis(line: &str, capacity: usize) -> Option<String> {
    // One character of the capacity belongs to the ellipsis itself.
    let keep = capacity.checked_sub(1)?;
    if keep < LABEL_ELLIPSIS_MIN_VISIBLE {
        return None;
    }
    let mut chars: Vec<char> = line.chars().take(keep).collect();
    while chars.last().is_some_and(|c| c.is_whitespace()) {
        chars.pop();
    }
    // A line that only has a word-break's worth of text left is not a name
    // either, so the shortfall is the same answer as a short box.
    if chars.len() < LABEL_ELLIPSIS_MIN_VISIBLE {
        return None;
    }
    chars.push(LABEL_ELLIPSIS);
    Some(chars.iter().collect())
}

/// The last resort (D2): the floor, keeping only the lines the box holds and
/// ellipsizing the final one. Reached only when no ladder step holds the whole
/// label, so the ellipsis means "nothing fits" rather than "the first try was
/// tight".
///
/// `None` closes the last resort too: when that final line cannot show
/// [`LABEL_ELLIPSIS_MIN_VISIBLE`] visible characters plus the ellipsis (a 3 mm
/// LED cell's two-character budget, or a one-glyph-wide stacked fader column),
/// the cell gets no in-cell layout and waits for the annotation and hover
/// reveal instead of painting a fragment.
fn ellipsized_layout(label: &str, area: &LabelBox, height: f32) -> Option<LabelLayout> {
    let font = LABEL_FONT_FLOOR;
    let (width, _) = label_text_box(area)?;
    let (mut lines, stacked) = wrapped_lines(label, area, font)?;
    let line_h = label_line_height(font);
    if line_h <= 0.0 {
        return None;
    }
    let max_lines = ((height / line_h) + LABEL_FIT_TOLERANCE).floor().max(0.0) as usize;
    if max_lines == 0 {
        return None;
    }
    if lines.len() > max_lines {
        lines.truncate(max_lines);
        let (head_cap, capacity) = line_capacities(width, font, area.glyph_chars);
        let last_cap = if stacked {
            1
        } else if max_lines == 1 {
            head_cap
        } else {
            capacity
        };
        let last = lines.last_mut()?;
        *last = truncate_with_ellipsis(last, last_cap)?;
    }
    Some(LabelLayout {
        font,
        lines,
        stacked,
        state_row: false,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn patch(content: &str) -> Patch {
        Patch::from_ini_str(content, String::from("t")).unwrap()
    }

    fn assert_close(a: f64, b: f64) {
        assert!(
            (a - b).abs() < 1e-9,
            "expected {a} to be within 1e-9 of {b}"
        );
    }

    #[test]
    fn screen_mapping_round_trips_mm_to_screen_to_mm() {
        let mm = RectMm {
            x_mm: 3.5,
            y_mm: 12.0,
            w_mm: 8.0,
            h_mm: 4.0,
        };
        // Default mapping (offset 0, zoom 1).
        let mapping = ScreenMapping::default();
        let (sx, sy, sw, sh) = mapping.mm_to_screen(mm);
        assert_close(sw, mm.w_mm * mapping.points_per_mm);
        assert_close(sh, mm.h_mm * mapping.points_per_mm);
        let (mm_x, mm_y) = mapping.screen_to_mm(sx, sy);
        assert_close(mm_x, mm.x_mm);
        assert_close(mm_y, mm.y_mm);
        // With zoom + offset too.
        let mapping_zoomed = ScreenMapping::new(PHYSICAL_POINTS_PER_MM, 1.5, 7.0, 3.0);
        let (sx2, sy2, sw2, sh2) = mapping_zoomed.mm_to_screen(mm);
        assert_close(sw2, mm.w_mm * PHYSICAL_POINTS_PER_MM * 1.5);
        assert_close(sh2, mm.h_mm * PHYSICAL_POINTS_PER_MM * 1.5);
        let (mm_x2, mm_y2) = mapping_zoomed.screen_to_mm(sx2, sy2);
        assert_close(mm_x2, mm.x_mm);
        assert_close(mm_y2, mm.y_mm);
    }

    #[test]
    fn screen_mapping_zoom_about_keeps_anchor_fixed() {
        let m = ScreenMapping::default();
        let (ax, ay) = (40.0, 12.0);
        let (amx, amy) = m.screen_to_mm(ax, ay);
        let z = m.zoom_about(2.0, ax, ay);
        assert_close(z.zoom, 2.0);
        let (nx, ny, _, _) = z.mm_to_screen(RectMm {
            x_mm: amx,
            y_mm: amy,
            w_mm: 1.0,
            h_mm: 1.0,
        });
        assert_close(nx, ax);
        assert_close(ny, ay);
        // Deterministic pure math: repeated calls agree bit-for-bit.
        assert_eq!(z, m.zoom_about(2.0, ax, ay));
    }

    #[test]
    fn screen_mapping_pan_shifts_screen_coords_linearly() {
        let m = ScreenMapping::new(PHYSICAL_POINTS_PER_MM, 1.0, 2.0, 4.0);
        let mm = RectMm {
            x_mm: 10.0,
            y_mm: 20.0,
            w_mm: 5.0,
            h_mm: 6.0,
        };
        let (x0, y0, w0, h0) = m.mm_to_screen(mm);
        let p = m.pan(7.0, 11.0);
        let (x1, y1, w1, h1) = p.mm_to_screen(mm);
        // offset + Δ shifts screen coords by −Δ (screen = mm × f × zoom −
        // offset): content moves up/left as the offset grows.
        assert_close(x1, x0 - 7.0);
        assert_close(y1, y0 - 11.0);
        assert_close(w1, w0);
        assert_close(h1, h0);
        // Inverse shifts the other way: the mm point that now lands on the
        // original screen position is Δ further along in mm.
        let (mx1, my1) = p.screen_to_mm(x0, y0);
        assert_close(mx1, mm.x_mm + 7.0 / (PHYSICAL_POINTS_PER_MM * 1.0));
        assert_close(my1, mm.y_mm + 11.0 / (PHYSICAL_POINTS_PER_MM * 1.0));
    }

    #[test]
    fn adjacent_module_rects_never_overlap_across_zoom_presets() {
        // droid_tui-4.1: module borders must abut — not overlap by a column —
        // at every zoom preset and pan offset. Deriving each module's span
        // from its own rounded width (`round(x)` + `round(w)`) lets the
        // rounding disagree with the next module's rounded left edge when a
        // pan offset shifts the first module's edge rounding differently
        // (e.g. offset 0.4: module 0 = [0,4) while module 1 starts at 3).
        // Edge-rounded spans (`round(mm0×f)..round(mm1×f)`) share the
        // boundary value, so adjacent rects abut exactly.
        // This test uses the old chain structure; create a custom chain for the test
        let chain = PhysicalLayout::build(
            Some(&patch("[p2b8]\n[copy]\n    input = I1\n    output = O1\n")),
            MasterModel::Master,
            &[],
        );
        let rack = RackLayout::pack(&chain, &spec(&[(3, 20.0)], &[]));
        // The supported range widened to [ZOOM_MIN, ZOOM_MAX] (D5), so the
        // abutment promise must hold at the new extremes, not only at the
        // retired [0.75, 2.0] presets.
        let zooms = [ZOOM_MIN, 0.5, 0.75, 1.0, 1.5, 2.0, 3.0, ZOOM_MAX];
        let offsets = [(0.0, 0.0), (0.4, 0.0), (-1.3, 0.7), (2.1, -0.9)];
        for zoom in zooms {
            for &(ox, oy) in &offsets {
                let m = ScreenMapping::new(PHYSICAL_POINTS_PER_MM, zoom, ox, oy);
                // Renderer-equivalent module rects: absolute mm spans mapped
                // with edge rounding, clamped to non-negative origin.
                let rects: Vec<(u16, u16, u16, u16)> = rack.rows[0]
                    .modules
                    .iter()
                    .map(|placed| {
                        m.mm_rect_to_screen(RectMm {
                            x_mm: placed.rect_mm.x_mm,
                            y_mm: placed.rect_mm.y_mm,
                            w_mm: placed.rect_mm.w_mm,
                            h_mm: placed.rect_mm.h_mm,
                        })
                    })
                    .collect();
                // Adjacent rects (same row, chain order) must abut or gap,
                // never overlap: right edge <= next left edge.
                for pair in rects.windows(2) {
                    let (x0, _, w0, _) = pair[0];
                    let (x1, _, _, _) = pair[1];
                    assert!(
                        x0.saturating_add(w0) <= x1,
                        "zoom {zoom} offset ({ox},{oy}): module rects overlap \
                         (m0 spans {x0}..{}, m1 starts at {x1})",
                        x0.saturating_add(w0)
                    );
                }
            }
        }
    }

    /// A two-row rack whose master (CV I/O) sits on row 1, so row 0 holds the
    /// P2B8 and a fold bar separates the rows. The row-offset tests share it:
    /// both measure the same gap, one in millimetres and one in points.
    fn two_row_rack() -> RackLayout {
        let p = patch("[p2b8]\n[copy]\n    input = I1\n    output = O1\n");
        let chain = PhysicalLayout::build(Some(&p), MasterModel::Master, &[]);
        let spec = RackSpec {
            rows: vec![
                RackRow {
                    he: 3,
                    hp: 12.0,
                    label: None,
                    modules: Vec::new(),
                },
                RackRow {
                    he: 3,
                    hp: 12.0,
                    label: None,
                    modules: Vec::new(),
                },
            ],
            assign: HashMap::from([("CV I/O".to_string(), 1)]),
            ..RackSpec::default_case(&chain)
        };
        RackLayout::pack(&chain, &spec)
    }

    #[test]
    fn screen_mapping_multi_row_offsets_include_fold_bars() {
        // Row 1 sits below row 0 plus exactly one fold bar: pack a two-row
        // rack and map both rows' module rects — the screen gap must equal
        // the mm gap × points_per_mm × zoom (D1/D11).
        let rack = two_row_rack();
        let fold = rack
            .fold_bars
            .iter()
            .find(|f| f.after_row == 0)
            .expect("fold bar after row 0");
        assert_close(fold.rect_mm.h_mm, FOLD_BAR_HEIGHT_MM);
        // Rows carry the rack-absolute y; placed rects stay at y 0 within
        // their row. Row 1 sits below row 0 plus exactly one fold bar.
        let row0 = &rack.rows[0];
        let row1 = &rack.rows[1];
        assert_close(row1.y_mm - (row0.y_mm + row0.height_mm), FOLD_BAR_HEIGHT_MM);
        let p0 = row0.modules[0].rect_mm;
        let p1 = row1.modules[0].rect_mm;
        let y0_abs = row0.y_mm + p0.y_mm;
        let y1_abs = row1.y_mm + p1.y_mm;
        assert_close(y1_abs - (y0_abs + p0.h_mm), FOLD_BAR_HEIGHT_MM);

        // Mapping the rack-absolute rects: the screen gap equals the mm gap
        // × points_per_mm (the mapping is isotropic in mm, D1).
        let m = ScreenMapping::default();
        let abs0 = RectMm {
            x_mm: p0.x_mm,
            y_mm: y0_abs,
            w_mm: p0.w_mm,
            h_mm: p0.h_mm,
        };
        let abs1 = RectMm {
            x_mm: p1.x_mm,
            y_mm: y1_abs,
            w_mm: p1.w_mm,
            h_mm: p1.h_mm,
        };
        let (_, sy0, _, _) = m.mm_to_screen(abs0);
        let (_, sy1, _, _) = m.mm_to_screen(abs1);
        assert_close(sy1 - sy0, (y1_abs - y0_abs) * m.points_per_mm);
        assert_close(
            sy1 - (sy0 + abs0.h_mm * m.points_per_mm),
            FOLD_BAR_HEIGHT_MM * m.points_per_mm,
        );
    }

    #[test]
    fn chain_order_matches_declaration() {
        let p = patch("[p2b8]\n[copy]\n    input = I1\n    output = O1\n");
        let layout = PhysicalLayout::build(Some(&p), MasterModel::Master, &[]);

        // D5: Master faceplate at chain position 0 (replaces CV I/O group).
        // The copy section's I1/O1 are CV I/O components, so they merge into
        // the master faceplate — no separate CV I/O module exists anymore.
        assert_eq!(layout.modules.len(), 2);
        let first = &layout.modules[0];
        assert_eq!(first.controller, "CV I/O");
        assert_eq!(first.module_instance, None);
        assert_eq!(first.geometry_key, "master");
        assert_close(first.rect_mm.x_mm, 0.0);
        assert_close(first.rect_mm.w_mm, 40.64); // master = 8 HP
        assert_close(first.rect_mm.h_mm, 128.5); // 3 HE
        assert_eq!(first.components.len(), 2, "I1 + O1 merged into master");

        let second = &layout.modules[1];
        assert_eq!(second.controller, "P2B8");
        assert_eq!(second.module_instance, Some(1));
        assert_eq!(second.geometry_key, "p2b8");
        assert!(!second.is_fallback);
        assert_eq!(second.he, 3);
        assert_close(second.rect_mm.x_mm, 41.14); // 40.64 + 0.5 gap
        assert_close(second.rect_mm.w_mm, 25.4); // p2b8 = 5 HP
        assert_close(second.rect_mm.h_mm, 128.5); // 3 HE

        assert_close(layout.total_width_mm, 66.54); // 40.64 + 0.5 + 25.4
        assert_close(layout.total_height_mm, 128.5);
        assert_close(layout.chain_gaps_mm.inter_module, 0.5);
    }

    #[test]
    fn repeated_p2b8_instances_yield_separate_faceplates() {
        let p = patch("[p2b8]\n[p2b8]\n");
        let layout = PhysicalLayout::build(Some(&p), MasterModel::Master, &[]);

        // Master at position 0, then two P2B8 instances
        assert_eq!(layout.modules.len(), 3);
        assert_eq!(layout.modules[0].controller, "CV I/O");
        assert_eq!(layout.modules[0].module_instance, None);
        assert_eq!(layout.modules[0].geometry_key, "master");

        assert_eq!(layout.modules[1].controller, "P2B8");
        assert_eq!(layout.modules[1].module_instance, Some(1));
        assert_eq!(layout.modules[2].controller, "P2B8");
        assert_eq!(layout.modules[2].module_instance, Some(2));
        assert_eq!(layout.modules[1].geometry_key, "p2b8");
        assert_eq!(layout.modules[2].geometry_key, "p2b8");
        // Both faceplates sit at the real p2b8 width (5 HP = 25.4 mm).
        assert_close(layout.modules[1].rect_mm.w_mm, 25.4);
        assert_close(layout.modules[2].rect_mm.w_mm, 25.4);
        assert_eq!(layout.modules[1].width_hp, 5.0);
        assert_eq!(layout.modules[2].width_hp, 5.0);
        assert_eq!(layout.modules[1].key(), "P2B8 1");
        assert_eq!(layout.modules[2].key(), "P2B8 2");
        // Master (40.64) + gap (0.5) + first P2B8 (25.4) + gap (0.5) = 67.04
        assert_close(layout.modules[2].rect_mm.x_mm, 67.04);
        assert_close(layout.total_width_mm, 92.44); // 40.64 + 0.5 + 25.4 + 0.5 + 25.4

        // Determinism: an identical patch yields an identical chain.
        let again = PhysicalLayout::build(Some(&p), MasterModel::Master, &[]);
        let rects = |l: &PhysicalLayout| l.modules.iter().map(|m| m.rect_mm).collect::<Vec<_>>();
        assert_eq!(rects(&layout), rects(&again));
    }

    #[test]
    fn p_registered_sliders_resolve_to_f_family_cells() {
        // droid_tui-2b4: p8s8 sliders and m4 motor faders live in the "F"
        // geometry family but are addressed with P registers in patches
        // (manual §6.8: "adressed with P1.1 through P1.8"; geometry notes
        // "sliders addressed via P registers"). cell_for must fall back from
        // the absent "P" family to "F" so P tokens render on their cells.
        let p = patch("[p8s8]\n");
        let layout = PhysicalLayout::build(Some(&p), MasterModel::Master, &[]);
        // Master at index 0, p8s8 at index 1
        let s1 = layout.cell_for(1, "P1.1").expect("P1.1 slider cell");
        assert_eq!(s1.label, "P1.1");
        assert_eq!(s1.element, Some(1));
        assert_eq!(s1.family, "F", "slider cell lives in the F family");
        let s8 = layout.cell_for(1, "P1.8").expect("P1.8 slider cell");
        assert_eq!(s8.label, "P1.8");
        assert!(layout.cell_for(1, "P1.9").is_none(), "8 sliders only");

        let p = patch("[m4]\n");
        let layout = PhysicalLayout::build(Some(&p), MasterModel::Master, &[]);
        // Master at index 0, m4 at index 1
        let f1 = layout.cell_for(1, "P1.2").expect("M4 fader cell");
        assert_eq!(f1.family, "F");
        assert_eq!(f1.element, Some(2));
    }

    #[test]
    fn switch_tokens_resolve_to_switch_cells_or_are_omitted() {
        // droid_tui-4.2: switch tokens place on the controller geometry's S
        // family — the same mechanism as knobs (P)/encoders (E)/buttons (B) —
        // and when the controller physically lacks switch cells the switch is
        // omitted rather than mis-rendered on a same-numbered knob cell.

        // Faderbank (p8s8) carries an S family: S tokens land on their switch
        // cells, the slider P1.1 on the F family (P->F fallback).
        let p = patch(
            "[switch]\n    switch = S1.1\n    output = _OUT\n[switch]\n    switch = S1.2\n[faderbank]\n    pot = P1.1\n",
        );
        let layout = PhysicalLayout::build(Some(&p), MasterModel::Master, &[]);
        // Master at 0, Faderbank at 1
        assert_eq!(layout.modules.len(), 2);
        assert_eq!(layout.modules[1].controller, "Faderbank");
        let s1 = layout.cell_for(1, "S1.1").expect("S1.1 switch cell");
        assert_eq!(s1.family, "S");
        assert_eq!(s1.element, Some(1));
        let s2 = layout.cell_for(1, "S1.2").expect("S1.2 switch cell");
        assert_eq!(s2.family, "S");
        assert_eq!(s2.element, Some(2));
        let p1 = layout.cell_for(1, "P1.1").expect("P1.1 slider cell");
        assert_eq!(
            p1.family, "F",
            "slider lives in the F family (P->F fallback)"
        );

        // Pot (p10) has no S family: the switch token is omitted — cell_for
        // returns None instead of falling back to the same-numbered knob cell.
        let p = patch("[pot]\n    pot = P1.1\n[switch]\n    switch = S1.1\n");
        let layout = PhysicalLayout::build(Some(&p), MasterModel::Master, &[]);
        // Master at 0, Pot at 1
        assert_eq!(layout.modules.len(), 2);
        assert_eq!(layout.modules[1].controller, "Pot");
        assert!(
            layout.cell_for(1, "S1.1").is_none(),
            "no switch cell on Pot -> the switch is omitted"
        );
        let knob = layout.cell_for(1, "P1.1").expect("P1.1 knob cell");
        assert_eq!(
            knob.label, "P1.1",
            "the knob cell stays a knob, not claimed by the omitted switch"
        );
    }

    #[test]
    fn bare_p8s8_synthesizes_switches_and_leds() {
        // droid_tui-2b4: a bare [p8s8] declares 8 sliders (P), 8 slider LEDs
        // (L) and 8 switches (S) — all resolvable against the p8s8 geometry.
        let p = patch("[p8s8]\n");
        let layout = PhysicalLayout::build(Some(&p), MasterModel::Master, &[]);
        // D5: master faceplate at index 0, p8s8 faceplate at index 1.
        assert_eq!(layout.modules.len(), 2);
        assert_eq!(layout.modules[0].geometry_key, "master");
        assert_eq!(layout.modules[1].controller, "P8S8");
        assert_eq!(layout.modules[1].module_instance, Some(1));
        for i in 1..=8u32 {
            assert!(
                layout.cell_for(1, &format!("P1.{i}")).is_some(),
                "P1.{i} slider"
            );
            assert!(layout.cell_for(1, &format!("L1.{i}")).is_some(), "L1.{i}");
            assert!(layout.cell_for(1, &format!("S1.{i}")).is_some(), "S1.{i}");
        }
    }

    #[test]
    fn cell_lookup_resolves_per_token_family() {
        let p = patch("[p2b8]\n");
        let layout = PhysicalLayout::build(Some(&p), MasterModel::Master, &[]);
        // D5: master faceplate at index 0, p2b8 faceplate at index 1.

        let b3 = layout.cell_for(1, "B1.3").expect("B1.3 cell");
        assert_eq!(b3.label, "B1.3");
        assert_eq!(b3.element, Some(3));
        // Measured P2B8 button cell (controller-geometry-fidelity, task 1.1):
        // a 9 mm cell centred at x 7.1 mm -> x_mm 2.6 (was the old 8 mm cell
        // at x_mm 3.5). Never relax this back to the old arrangement.
        assert_close(b3.rect_mm.x_mm, 2.6);
        assert_close(b3.rect_mm.w_mm, 9.0);

        let p2 = layout.cell_for(1, "P1.2").expect("P1.2 cell");
        assert_eq!(p2.label, "P1.2");
        assert_eq!(p2.family, "P");
        assert_eq!(p2.element, Some(2));

        let l5 = layout.cell_for(1, "L1.5").expect("L1.5 cell");
        assert_eq!(l5.label, "L1.5");
        assert_eq!(l5.family, "L");
        assert_eq!(l5.element, Some(5));

        assert!(
            layout.cell_for(1, "B1.9").is_none(),
            "element beyond family range"
        );
        assert!(
            layout.cell_for(1, "S1.1").is_none(),
            "p2b8 has no switch cells"
        );
    }

    #[test]
    fn cv_tokens_resolve_to_directional_master_cells() {
        let p = patch("[copy]\n    input = I1\n    output = O4\n");
        let layout = PhysicalLayout::build(Some(&p), MasterModel::Master, &[]);

        assert_eq!(layout.modules.len(), 1);
        assert_eq!(layout.modules[0].geometry_key, "master");

        // "O4" must land on the O-side jack, not the element-4 I-side cell.
        let o4 = layout.cell_for(0, "O4").expect("O4 cell");
        assert_eq!(o4.label, "O4");
        assert_eq!(o4.kind_letter, Some('O'));

        let i1 = layout.cell_for(0, "I1").expect("I1 cell");
        assert_eq!(i1.label, "I1");
        assert_eq!(i1.kind_letter, Some('I'));
    }

    // ---- module-ui-rack-inventory 4.2: geometry-declared element identity ----

    /// The preprocessed cell for one synthetic `CV` entry.
    fn preprocessed_cell(entry: ElementCellEntry) -> ElementCell {
        let entries = HashMap::from([(String::from("CV"), vec![entry])]);
        preprocess_cells(&entries)
            .remove("CV")
            .and_then(|mut cells| cells.pop())
            .expect("one preprocessed cell")
    }

    /// A one-faceplate chain carrying `cells`, so `cell_for` can be exercised
    /// against identity `preprocess_cells` produced.
    fn layout_with_cells(cells: HashMap<String, Vec<ElementCell>>) -> PhysicalLayout {
        PhysicalLayout {
            modules: vec![PhysicalModule {
                controller: String::from("CV I/O"),
                module_instance: None,
                geometry_key: String::from("master"),
                is_fallback: false,
                rect_mm: RectMm {
                    x_mm: 0.0,
                    y_mm: 0.0,
                    w_mm: 20.0,
                    h_mm: 128.5,
                },
                width_hp: 4.0,
                he: 3,
                cells,
                components: Vec::new(),
            }],
            total_width_mm: 20.0,
            total_height_mm: 128.5,
            chain_gaps_mm: ChainGaps::default(),
            hp_mm: 5.08,
            he_mm: HashMap::new(),
            fallback_width_mm: 20.0,
            fallback_height_mm: 128.5,
            inventory_driven: false,
        }
    }

    #[test]
    fn declared_element_identity_wins_over_a_parseable_label() {
        // The label is patch vocabulary and parses to (1, 'O'); the geometry
        // declares this cell as element 7 of the I side, which must win.
        let cell = preprocessed_cell(ElementCellEntry {
            label: String::from("O1"),
            element: Some(7),
            kind_letter: Some('I'),
            ..ElementCellEntry::default()
        });
        assert_eq!(cell.element, Some(7), "declared element wins");
        assert_eq!(cell.kind_letter, Some('I'), "declared kind letter wins");
    }

    #[test]
    fn label_identity_is_the_fallback() {
        let dotted = preprocessed_cell(ElementCellEntry {
            label: String::from("B1.3"),
            ..ElementCellEntry::default()
        });
        assert_eq!((dotted.element, dotted.kind_letter), (Some(3), Some('B')));

        let plain = preprocessed_cell(ElementCellEntry {
            label: String::from("I5"),
            ..ElementCellEntry::default()
        });
        assert_eq!((plain.element, plain.kind_letter), (Some(5), Some('I')));
    }

    #[test]
    fn cell_without_identity_stays_a_cell() {
        // Neither declared nor label-derived identity: the cell keeps its
        // rect and label (the painter can draw it) but carries no number and
        // therefore no token can address it.
        let cell = preprocessed_cell(ElementCellEntry {
            label: String::from("USB"),
            x_mm: 3.0,
            y_mm: 4.0,
            w_mm: 5.0,
            h_mm: 6.0,
            ..ElementCellEntry::default()
        });
        assert_eq!(cell.element, None, "no number in the label, none declared");
        assert_eq!(cell.label, "USB");
        assert_close(cell.rect_mm.x_mm, 3.0);
        assert_close(cell.rect_mm.h_mm, 6.0);
    }

    #[test]
    fn unaddressable_cell_never_enters_the_token_index() {
        // The r2m/r2c serial bridges: explicit element numbers, no kind
        // letter, `addressable: false` — they render but never resolve.
        let cell = preprocessed_cell(ElementCellEntry {
            label: String::from("serial in"),
            element: Some(1),
            addressable: Some(false),
            ..ElementCellEntry::default()
        });
        assert_eq!(
            cell.element, None,
            "a declared number is not a token id here"
        );
        assert_eq!(cell.kind_letter, None, "the label's S must not become one");

        let layout = layout_with_cells(HashMap::from([(String::from("CV"), vec![cell])]));
        assert!(
            layout.cell_for(0, "O1").is_none(),
            "element 1 of the family must not resolve to the serial jack"
        );
        assert!(
            layout.cell_for(0, "I1").is_none(),
            "nor through the positional fallback"
        );
    }

    #[test]
    fn master18_gate_tokens_resolve_to_the_gate_column() {
        let p = patch("# LABELS: master=18\n[copy]\n    input = I1\n    output = O1\n");
        let layout = PhysicalLayout::build(Some(&p), MasterModel::Master18, &[]);
        assert_eq!(layout.modules[0].geometry_key, "master18");

        // Gate column x = 18.5 mm; the CV-out column sits at 1.5 mm.
        let i1 = layout.cell_for(0, "I1").expect("I1 gate-in cell");
        assert_eq!(i1.label, "I1", "I1 must not resolve to O1");
        assert_eq!(i1.kind.as_deref(), Some("gate_in"));
        assert_close(i1.rect_mm.x_mm, 18.5);

        let i2 = layout.cell_for(0, "I2").expect("I2 gate-in cell");
        assert_eq!(i2.label, "I2");
        assert_eq!(i2.kind.as_deref(), Some("gate_in"));

        let g1 = layout.cell_for(0, "G1").expect("G1 gate-out cell");
        assert_eq!(g1.label, "G1", "G1 must not resolve to O1");
        assert_eq!(g1.kind.as_deref(), Some("gate_out"));
        assert_close(g1.rect_mm.x_mm, 18.5);

        let g4 = layout.cell_for(0, "G4").expect("G4 gate-out cell");
        assert_eq!(g4.label, "G4");
        assert_eq!(g4.kind.as_deref(), Some("gate_out"));

        let o1 = layout.cell_for(0, "O1").expect("O1 cv-out cell");
        assert_eq!(o1.label, "O1");
        assert_eq!(o1.kind.as_deref(), Some("cv_out"));
        assert_close(o1.rect_mm.x_mm, 1.5);
    }

    #[test]
    fn deep_cv_patch_uses_master18_geometry() {
        // The faceplate follows the patch's own declaration: a deep CV jack no
        // longer implies MASTER18 (that heuristic is gone), the `# LABELS:
        // master=18` preamble does.
        let p = patch("# LABELS: master=18\n[copy]\n    input = I9\n    output = O1\n");
        assert_eq!(p.master_declaration, Some(MasterModel::Master18));
        let layout = PhysicalLayout::build(Some(&p), MasterModel::Master18, &[]);
        assert_eq!(layout.modules[0].geometry_key, "master18");
        assert_close(layout.modules[0].rect_mm.w_mm, 30.48); // master18 = 6 HP
    }

    #[test]
    fn silent_deep_cv_patch_lays_out_master() {
        // The deleted "CV jack > 8" guess: the same patch without a
        // declaration takes the named default, MASTER, and the layout draws
        // that faceplate. Reporting the missing deep jack belongs to the
        // resolver, not to the chain geometry.
        let p = patch("[copy]\n    input = I9\n    output = O1\n");
        assert_eq!(p.master_declaration, None);
        let layout = PhysicalLayout::build(Some(&p), MasterModel::Master, &[]);
        assert_eq!(layout.modules[0].geometry_key, "master");
        assert!(
            layout.modules[0]
                .cells
                .values()
                .flatten()
                .all(|c| c.label != "I9"),
            "MASTER has no ninth input jack"
        );
    }

    #[test]
    fn unknown_controller_falls_back_to_default_width() {
        let p = patch("[copy]\n    button = B5.1\n");
        let layout = PhysicalLayout::build(Some(&p), MasterModel::Master, &[]);

        // Master at 0, unknown controller at 1
        let m = &layout.modules[1];
        assert!(m.is_fallback);
        assert_eq!(m.geometry_key, "");
        assert_eq!(m.controller, "Controller 5");
        assert_close(m.rect_mm.w_mm, 25.4);
        assert_close(m.width_hp, 5.0); // the documented 5 HP fallback
        assert_close(m.rect_mm.h_mm, 128.5);
        assert!(
            layout.cell_for(1, "B5.1").is_none(),
            "fallback module has no cells"
        );
    }

    #[test]
    fn controller_names_alias_to_geometry_keys() {
        let data = ControllerGeometry::fallback(); // resolution must not need data
        let resolve = |n: &str| data.resolve_controller(n);
        assert_eq!(resolve("P2B8").as_deref(), Some("p2b8"));
        assert_eq!(resolve("Notebuttons").as_deref(), Some("b32"));
        assert_eq!(resolve("Faderbank").as_deref(), Some("p8s8"));
        assert_eq!(resolve("Encoder").as_deref(), Some("e4"));
        assert_eq!(resolve("Pot").as_deref(), Some("p10"));
        assert_eq!(resolve("Motorfader").as_deref(), Some("m4"));
        assert_eq!(resolve("Unusedfaders").as_deref(), None);
        assert_eq!(resolve("Controller 3").as_deref(), None);
    }

    #[test]
    fn malformed_geometry_data_never_panics() {
        // Garbage fails to parse at the data layer instead of panicking.
        assert!(serde_json::from_str::<ControllerGeometry>("{").is_err());
        // Missing meta/controllers fields deserialize to defaults.
        let bare = serde_json::from_str::<ControllerGeometry>("{}").unwrap();
        assert!(bare.controllers.is_empty());
        // The fallback data source carries the documented defaults.
        let fb = ControllerGeometry::fallback();
        assert_close(fb.fallback_width_mm(), 25.4);
        assert_close(fb.fallback_width_hp(), 5.0);
        assert_close(fb.fallback_height_mm(), 128.5);
        // A patch with an unknown controller yields a fallback module.
        let p = patch("[copy]\n    button = B7.1\n");
        let layout = PhysicalLayout::build(Some(&p), MasterModel::Master, &[]);
        assert_eq!(layout.modules.len(), 2); // master + fallback
        let m = &layout.modules[1];
        assert!(m.is_fallback);
        assert_eq!(m.geometry_key, "");
        assert_eq!(m.controller, "Controller 7");
        assert_close(fb.chain_gap_mm(), 0.5);
        // build() over the fallback source never panics for unknown
        // controllers: module 0 is the master faceplate, the unknown
        // controller follows at module 1 with the fallback width.
        assert!(!layout.modules[0].is_fallback);
        assert!(layout.modules[1].is_fallback);
    }

    #[test]
    fn embedded_controller_geometry_parses_and_matches_tracked_file() {
        let embedded =
            ControllerGeometry::load().expect("embedded controller_geometry.json parses");
        assert!(!embedded.controllers.is_empty());

        let path =
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("controller_geometry.json");
        let tracked = std::fs::read_to_string(&path)
            .unwrap_or_else(|e| panic!("tracked {} unreadable: {e}", path.display()));
        assert_eq!(
            CONTROLLER_GEOMETRY_JSON, tracked,
            "compiled-in controller geometry drifted from the tracked file"
        );

        let from_file = ControllerGeometry::from_json(&tracked)
            .expect("tracked controller_geometry.json parses");
        assert_eq!(
            serde_json::to_value(&embedded).expect("embedded geometry serialises"),
            serde_json::to_value(&from_file).expect("tracked geometry serialises"),
            "embedded and tracked controller geometry parse to different models"
        );
    }

    #[test]
    fn loader_does_not_search_the_filesystem() {
        // Production source is everything before the test module.
        let source = include_str!("physical.rs");
        let production = source.split("#[cfg(test)]").next().expect("test module");
        assert!(
            !production.contains("CARGO_MANIFEST_DIR"),
            "controller geometry must come from include_str!, not a manifest-relative path"
        );
        assert!(
            !production.contains("std::fs"),
            "controller geometry must not be read from the filesystem at runtime"
        );
    }

    #[test]
    fn real_geometry_file_loads_and_covers_expected_controllers() {
        let data = ControllerGeometry::load().expect("controller_geometry.json loads");
        assert!(data.controllers.len() >= 15);
        for key in [
            "master", "master18", "p2b8", "b32", "e4", "m4", "p8s8", "p10", "s10", "db8e", "g8",
            "x7", "r2m", "r2c", "p4b2",
        ] {
            assert!(data.controllers.contains_key(key), "{key} missing");
        }
        let p2b8 = data.controller("P2B8").expect("p2b8 entry");
        assert_eq!(p2b8.he, 3);
        assert_eq!(p2b8.width_hp, 5.0);
        assert_eq!(p2b8.width_mm, 25.4);
        assert!(p2b8.element_cells.contains_key("B"));
        assert!(p2b8.element_cells.contains_key("L"));
        assert!(p2b8.element_cells.contains_key("P"));
    }

    #[test]
    fn missing_geometry_entry_is_detectable_without_panic() {
        // A controller name that resolves to a geometry key with no data
        // entry: resolution still succeeds (the direct names need no data)
        // while the entry lookup returns None — `build`'s fallback arm
        // handles exactly this condition instead of panicking.
        let fb = ControllerGeometry::fallback(); // empty `controllers` map
        assert_eq!(fb.resolve_controller("G8").as_deref(), Some("g8"));
        assert!(fb.controller("g8").is_none());
    }

    #[test]
    fn all_geometry_data_is_sane() {
        // Full data-sanity matrix (task 1.2): every controller entry has
        // positive dimensions and every element cell lies inside its module
        // rect — across all 15 entries, not just the p2b8 smoke subset.
        let data = ControllerGeometry::load().expect("controller_geometry.json loads");
        assert!(data.controllers.len() >= 15, "matrix must not be vacuous");
        assert_close(data.hp_mm(), 5.08);
        for (key, entry) in &data.controllers {
            assert!(entry.width_mm > 0.0, "{key}: width_mm not positive");
            assert!(entry.height_mm > 0.0, "{key}: height_mm not positive");
            assert!(entry.width_hp > 0.0, "{key}: width_hp not positive");
            assert!(matches!(entry.he, 1 | 3), "{key}: he must be 1 or 3");
            for (family, cells) in &entry.element_cells {
                assert!(!cells.is_empty(), "{key}: family {family} has no cells");
                for c in cells {
                    let cell_desc = format!("{key}/{family}/{}", c.label);
                    assert!(
                        c.w_mm > 0.0 && c.h_mm > 0.0,
                        "{cell_desc}: cell size not positive"
                    );
                    assert!(
                        c.x_mm >= 0.0 && c.y_mm >= 0.0,
                        "{cell_desc}: cell origin negative"
                    );
                    assert!(
                        c.x_mm + c.w_mm <= entry.width_mm + 1e-6,
                        "{cell_desc}: exceeds module width"
                    );
                    assert!(
                        c.y_mm + c.h_mm <= entry.height_mm + 1e-6,
                        "{cell_desc}: exceeds module height"
                    );
                    assert!(!c.label.is_empty(), "{key}: empty cell label");
                }
            }
        }
    }

    #[test]
    fn b32_resolves_to_4x8_grid() {
        // The manual's B32 button grid: 4 columns × 8 rows, 32 buttons.
        let data = ControllerGeometry::load().expect("controller_geometry.json loads");
        let b32 = data.controller("b32").expect("b32 entry");
        let buttons = &b32.element_cells["B"];
        let cols: std::collections::BTreeSet<u32> = buttons.iter().map(|c| c.col).collect();
        let rows: std::collections::BTreeSet<u32> = buttons.iter().map(|c| c.row).collect();
        assert_eq!(buttons.len(), 32, "b32 must carry 32 buttons");
        assert_eq!(cols.len(), 4, "b32 must resolve to 4 columns");
        assert_eq!(rows.len(), 8, "b32 must resolve to 8 rows");
        assert!(cols.iter().all(|&c| c < 4), "column indices must be 0..4");
        assert!(rows.iter().all(|&r| r < 8), "row indices must be 0..8");
        let pairs: std::collections::BTreeSet<(u32, u32)> =
            buttons.iter().map(|c| (c.col, c.row)).collect();
        assert_eq!(pairs.len(), 32, "4x8 grid complete: no gaps, no duplicates");

        // Model proof: a Notebuttons patch resolves to b32 and reaches the
        // last cell of the 4x8 grid; button 33 is out of range.
        let p = patch("[notebuttons]\n    button = B1.1\n");
        let layout = PhysicalLayout::build(Some(&p), MasterModel::Master, &[]);
        // D5: master faceplate at index 0, b32 faceplate at index 1.
        assert_eq!(layout.modules[0].geometry_key, "master");
        assert_eq!(layout.modules[1].geometry_key, "b32");
        let last = layout.cell_for(1, "B1.32").expect("B1.32 on the 4x8 grid");
        assert_eq!(last.label, "B1.32");
        assert!(
            layout.cell_for(1, "B1.33").is_none(),
            "33rd button is out of the 4x8 grid"
        );
    }

    // ---- rack model (task 2.3) ----

    /// Master (8 HP, 40.64 mm) + p2b8 (5 HP, 25.4 mm) + b32 (10 HP, 50.8 mm).
    fn three_module_chain() -> PhysicalLayout {
        PhysicalLayout::build(
            Some(&patch(
                "[p2b8]\n[b32]\n[copy]\n    input = I1\n    output = O1\n",
            )),
            MasterModel::Master,
            &[],
        )
    }

    fn spec(rows: &[(u32, f64)], assign: &[(&str, usize)]) -> RackSpec {
        RackSpec {
            rows: rows
                .iter()
                .map(|&(he, hp)| RackRow {
                    he,
                    hp,
                    label: None,
                    modules: Vec::new(),
                })
                .collect(),
            top_mount_te: 0.0,
            side_mount_te: 0.0,
            assign: assign.iter().map(|&(k, r)| (String::from(k), r)).collect(),
        }
    }

    #[test]
    fn module_key_format_is_controller_plus_instance() {
        let chain = three_module_chain();
        assert_eq!(chain.modules[0].key(), "CV I/O");
        assert_eq!(chain.modules[1].key(), "P2B8 1");
        assert_eq!(chain.modules[2].key(), "B32 2");
    }

    #[test]
    fn auto_pack_fills_row_0_then_overflows_to_row_1() {
        let chain = three_module_chain();
        let rack = RackLayout::pack(&chain, &spec(&[(3, 10.0), (3, 10.0)], &[]));
        assert_eq!(rack.rows.len(), 2);
        // Row capacity is 10 HP = 50.8 mm. Master (40.64) fits row 0.
        // P2B8 1 (25.4) would need 41.14 + 25.4 = 66.54 > 50.8, so it starts
        // row 1, where B32 2 (50.8) overflows after it (last row accepts).
        assert_eq!(rack.rows[0].modules.len(), 1);
        assert_eq!(rack.rows[0].modules[0].key, "CV I/O");
        assert_close(rack.rows[0].modules[0].rect_mm.x_mm, 0.0);
        assert!(!rack.rows[0].modules[0].overridden);
        assert_eq!(rack.rows[1].modules.len(), 2);
        assert_eq!(rack.rows[1].modules[0].key, "P2B8 1");
        assert_close(rack.rows[1].modules[0].rect_mm.x_mm, 0.0);
        assert_eq!(rack.rows[1].modules[1].key, "B32 2");
        assert_close(rack.rows[1].modules[1].rect_mm.x_mm, 25.9);
        assert_close(rack.rows[0].fill_width_mm, 40.64);
        assert_close(rack.rows[1].fill_width_mm, 76.7); // 25.4 + 0.5 + 50.8
    }

    #[test]
    fn default_single_row_holds_whole_chain() {
        let chain = three_module_chain();
        let rack = RackLayout::pack(&chain, &RackSpec::default());
        assert_eq!(rack.rows.len(), 1);
        let row = &rack.rows[0];
        assert_eq!(row.he, 3);
        // Master (40.64) + gap (0.5) + P2B8 1 (25.4) + gap (0.5) + B32 2 (50.8)
        // = 117.84 mm; ceil(117.84 / 5.08) = 24 HP.
        assert_close(row.hp, 24.0);
        assert_eq!(row.modules.len(), 3);
        assert_eq!(row.modules[0].key, "CV I/O");
        assert_eq!(row.modules[1].key, "P2B8 1");
        assert_eq!(row.modules[2].key, "B32 2");
        assert_close(row.modules[1].rect_mm.x_mm, 41.14);
        assert_close(row.modules[2].rect_mm.x_mm, 67.04);
        assert_close(row.fill_width_mm, 117.84);
        assert_close(rack.total_width_mm, 121.92); // 24 HP case
        assert!(rack.fold_bars.is_empty());
    }

    #[test]
    fn override_places_module_into_row_regardless_of_fit() {
        let chain = three_module_chain();
        let rack = RackLayout::pack(&chain, &spec(&[(3, 10.0), (3, 10.0)], &[("P2B8 1", 1)]));
        // P2B8 forced to row 1; master auto-packs row 0 (cursor unaffected).
        assert_eq!(rack.rows[0].modules[0].key, "CV I/O");
        assert_eq!(rack.rows[1].modules[0].key, "P2B8 1");
        assert!(rack.rows[1].modules[0].overridden);
        assert!(!rack.rows[0].modules[0].overridden);
    }

    #[test]
    fn out_of_range_override_falls_back_to_auto_pack() {
        let chain = three_module_chain();
        let rack = RackLayout::pack(
            &chain,
            &spec(&[(3, 10.0), (3, 10.0), (3, 10.0)], &[("P2B8 1", 5)]),
        );
        // Row 5 does not exist, so the P2B8 override is ignored and normal
        // auto-pack applies: master row 0, P2B8 1 row 1 (41.14 + 25.4 > 50.8),
        // B32 2 row 2.
        assert_eq!(rack.rows.len(), 3);
        assert_eq!(rack.rows[0].modules[0].key, "CV I/O");
        assert!(!rack.rows[0].modules[0].overridden);
        assert_eq!(rack.rows[1].modules[0].key, "P2B8 1");
        assert!(!rack.rows[1].modules[0].overridden);
        assert_eq!(rack.rows[2].modules[0].key, "B32 2");
        assert!(!rack.rows[2].modules[0].overridden);
    }

    #[test]
    fn fold_lines_at_row_boundaries() {
        let chain = PhysicalLayout::build(
            Some(&patch("[p2b8]\n[copy]\n    input = I1\n    output = O1\n")),
            MasterModel::Master,
            &[],
        );
        let rack = RackLayout::pack(&chain, &spec(&[(3, 5.0), (3, 5.0), (3, 5.0)], &[]));
        // The chain is master (40.64) + P2B8 1 (25.4) with 5 HP = 25.4 mm
        // rows: master takes the empty row 0 (an empty row always accepts its
        // first module), P2B8 starts row 1, row 2 stays empty.
        assert_eq!(rack.rows[0].modules[0].key, "CV I/O");
        assert_eq!(rack.rows[1].modules[0].key, "P2B8 1");
        assert!(rack.rows[2].modules.is_empty());
        assert_eq!(rack.fold_bars.len(), 2);
        let b0 = &rack.fold_bars[0];
        assert_eq!(b0.after_row, 0);
        assert_close(b0.rect_mm.y_mm, 128.5);
        assert_close(b0.rect_mm.h_mm, 5.0);
        assert_close(b0.rect_mm.w_mm, 25.4);
        let b1 = &rack.fold_bars[1];
        assert_eq!(b1.after_row, 1);
        assert_close(b1.rect_mm.y_mm, 262.0);
        assert_close(rack.rows[1].y_mm, 133.5);
        assert_close(rack.rows[2].y_mm, 267.0);
        assert_close(rack.total_height_mm, 395.5);
        assert_close(rack.fold_bar_height_mm, FOLD_BAR_HEIGHT_MM);
    }

    #[test]
    fn mounts_attach_as_regions() {
        let chain = PhysicalLayout::build(
            Some(&patch("[p2b8]\n[copy]\n    input = I1\n    output = O1\n")),
            MasterModel::Master,
            &[],
        );
        let mut s = spec(&[(3, 10.0)], &[]);
        s.top_mount_te = 2.0;
        s.side_mount_te = 1.0;
        let rack = RackLayout::pack(&chain, &s);
        let top = rack.mounts.top.expect("top mount");
        assert_close(top.y_mm, 0.0);
        assert_close(top.h_mm, 10.16);
        assert_close(top.w_mm, 50.8);
        let left = rack.mounts.side_left.expect("left mount");
        assert_close(left.w_mm, 5.08);
        assert_close(left.h_mm, 128.5); // rows region below the top mount
        let right = rack.mounts.side_right.expect("right mount");
        assert_close(right.x_mm, 50.8);
        assert_close(rack.rows[0].y_mm, 10.16);
        assert_close(rack.total_width_mm, 60.96);
        assert_close(rack.total_height_mm, 138.66);
    }

    #[test]
    fn rack_packing_is_deterministic() {
        let chain = PhysicalLayout::build(
            Some(&patch("[p2b8]\n[copy]\n    input = I1\n    output = O1\n")),
            MasterModel::Master,
            &[],
        );
        let s = spec(&[(3, 10.0), (3, 10.0)], &[("P2B8 1", 1)]);
        let a = RackLayout::pack(&chain, &s);
        let b = RackLayout::pack(&chain, &s);
        assert_eq!(a, b);
    }

    // ---- DB8E OLED placeholder state (task 2.1) ----

    #[test]
    fn db8e_display_state_not_used_when_no_db8e() {
        // No [db8e] section → "not used" (manual ch. 6.5.5), nor any db8e module.
        let p = patch("[p2b8]\n");
        assert_eq!(db8e_display_state(&p), "not used");
        assert_eq!(
            db8e_display_state_for_layout(&PhysicalLayout::build(
                Some(&p),
                MasterModel::Master,
                &[]
            )),
            "not used"
        );
        // Hardware mismatch marker alone must not false-positive when db8e absent.
        // Use a valid patch with no db8e but a mismatch-tinted comment.
        let q = patch("[p2b8]\n# mismatch in comment\n[copy]\n    input = I1\n    output = O1\n");
        // The comment is in raw_lines and would trigger the stub if db8e were present,
        // but without db8e the result must stay "not used".
        assert!(q
            .raw_lines
            .iter()
            .any(|l| l.to_ascii_lowercase().contains("mismatch")));
        assert_eq!(db8e_display_state(&q), "not used");
    }

    #[test]
    fn db8e_display_state_connected_when_db8e_present() {
        // [db8e] with at least one hardware token creates the db8e faceplate
        // (bare [db8e] alone synthesizes no tokens by design). Stub fallback
        // yields "connected".
        let p = patch("[db8e]\n    button1 = B1.1\n");
        assert_eq!(db8e_display_state(&p), "connected");
        let layout = PhysicalLayout::build(Some(&p), MasterModel::Master, &[]);
        assert!(layout.modules.iter().any(|m| m.geometry_key == "db8e"));
        assert_eq!(db8e_display_state_for_layout(&layout), "connected");
        // With additional hardware the baseline stays "connected".
        let q = patch("[db8e]\n    button1 = B1.1\n[p2b8]\n");
        assert_eq!(db8e_display_state(&q), "connected");
        assert_eq!(
            db8e_display_state_for_layout(&PhysicalLayout::build(
                Some(&q),
                MasterModel::Master,
                &[]
            )),
            "connected"
        );
    }

    #[test]
    fn db8e_display_state_configuration_error_on_stub_mismatch() {
        // Deterministic stub: any key/value or raw line containing "mismatch"
        // triggers "configuration error" when [db8e] is present. All fixtures
        // include a real db8e token (B1.1) so Patch::from_ini_str succeeds
        // (bare [db8e] synthesizes no hw_components by design).
        let p = patch("[db8e]\n    button1 = B1.1\n    mismatch = 1\n");
        assert_eq!(db8e_display_state(&p), "configuration error");
        let p2 = patch("[db8e]\n    button1 = B1.1\n# mismatch in comment\n");
        assert_eq!(db8e_display_state(&p2), "configuration error");
        let p3 = patch("[db8e]\n    button1 = B1.1\n[copy]\n    input = _MISMATCH_CABLE\n");
        assert_eq!(db8e_display_state(&p3), "configuration error");
        // Layout-level stub: controller/component id containing "mismatch".
        let layout = PhysicalLayout::build(
            Some(&patch("[db8e]\n    button1 = B1.1\n")),
            MasterModel::Master,
            &[],
        );
        // No layout-level mismatch by default → still "connected".
        assert_eq!(db8e_display_state_for_layout(&layout), "connected");
        // Inject a layout-level mismatch marker and verify the branch.
        let mut mismatched = layout.clone();
        if let Some(m) = mismatched
            .modules
            .iter_mut()
            .find(|m| m.geometry_key == "db8e")
        {
            m.controller = "mismatch".into();
        }
        assert_eq!(
            db8e_display_state_for_layout(&mismatched),
            "configuration error"
        );
    }

    // ---- controller-geometry-fidelity measured arrangement (task 1.1) ----
    //
    // These assertions pin the manual-measured faceplate arrangement from
    // openspec/changes/controller-geometry-fidelity/design.md ("Measured
    // geometry"). The measurement ceiling is ±0.3 mm at this source
    // resolution, so positions and pitches compare within 0.35 mm, never by
    // exact equality; structure (column/row counts) is asserted as well as
    // position. They are written before the data: they must FAIL until task
    // 2.1 regenerates controller_geometry.json, and an assertion must never
    // be relaxed to the old side-by-side arrangement to make it pass.

    /// Measurement ceiling (±0.3 mm) plus a small guard band.
    const GEOM_TOL_MM: f64 = 0.35;

    fn assert_mm(actual: f64, expected: f64, what: &str) {
        assert!(
            (actual - expected).abs() <= GEOM_TOL_MM,
            "{what}: expected {expected} ±{GEOM_TOL_MM} mm, got {actual}"
        );
    }

    /// Centre `(x + w/2, y + h/2)` of a raw cell entry, in mm.
    fn cell_centre(c: &ElementCellEntry) -> (f64, f64) {
        (c.x_mm + c.w_mm / 2.0, c.y_mm + c.h_mm / 2.0)
    }

    /// Distinct coordinates within the measurement tolerance, ascending.
    fn distinct_mm(mut values: Vec<f64>) -> Vec<f64> {
        values.sort_by(|a, b| a.partial_cmp(b).unwrap());
        let mut out: Vec<f64> = Vec::new();
        for v in values {
            if out
                .last()
                .is_none_or(|&last| (v - last).abs() > GEOM_TOL_MM)
            {
                out.push(v);
            }
        }
        out
    }

    /// Distinct x centres of a family, ascending.
    fn x_centres(cells: &[ElementCellEntry]) -> Vec<f64> {
        distinct_mm(cells.iter().map(|c| cell_centre(c).0).collect())
    }

    /// Distinct y centres of a family, ascending.
    fn y_centres(cells: &[ElementCellEntry]) -> Vec<f64> {
        distinct_mm(cells.iter().map(|c| cell_centre(c).1).collect())
    }

    fn family_cells<'a>(
        data: &'a ControllerGeometry,
        key: &str,
        family: &str,
    ) -> &'a [ElementCellEntry] {
        data.controller(key)
            .unwrap_or_else(|| panic!("{key} entry missing"))
            .element_cells
            .get(family)
            .map_or_else(|| panic!("{key} family {family} missing"), Vec::as_slice)
    }

    /// Elements with a label number in `1..=n` (family head).
    fn head_cells(cells: &[ElementCellEntry], n: u32) -> Vec<ElementCellEntry> {
        cells
            .iter()
            .filter(|c| matches!(label_element(&c.label).0, Some(e) if e <= n))
            .cloned()
            .collect()
    }

    /// Ring cells around a centre within the measured ~23.9 mm square.
    fn ring_cells(cells: &[ElementCellEntry], cx: f64, cy: f64) -> Vec<&ElementCellEntry> {
        cells
            .iter()
            .filter(|c| {
                let (x, y) = cell_centre(c);
                (x - cx).abs() <= 12.5 && (y - cy).abs() <= 12.5
            })
            .collect()
    }

    /// Structural ring assertions: 32 LEDs, 9 per side at 2.76 mm pitch,
    /// 1.8 mm cells, spanning the measured ~23.9 mm square centred on the
    /// encoder axis (deliberately independent of label ordering).
    fn assert_ring(cells: &[&ElementCellEntry], cx: f64, cy: f64, what: &str) {
        assert_eq!(cells.len(), 32, "{what}: a ring must carry 32 LEDs");
        for c in cells {
            assert_mm(c.w_mm, 1.8, &format!("{what}: LED cell width"));
            assert_mm(c.h_mm, 1.8, &format!("{what}: LED cell height"));
        }
        let xs = distinct_mm(cells.iter().map(|c| c.x_mm + c.w_mm / 2.0).collect());
        let ys = distinct_mm(cells.iter().map(|c| c.y_mm + c.h_mm / 2.0).collect());
        assert_eq!(xs.len(), 9, "{what}: 9 LEDs per side (x)");
        assert_eq!(ys.len(), 9, "{what}: 9 LEDs per side (y)");
        assert_mm(xs[1] - xs[0], 2.76, &format!("{what}: ring pitch x"));
        assert_mm(ys[1] - ys[0], 2.76, &format!("{what}: ring pitch y"));
        let min_x = cells.iter().map(|c| c.x_mm).fold(f64::INFINITY, f64::min);
        let max_x = cells
            .iter()
            .map(|c| c.x_mm + c.w_mm)
            .fold(f64::NEG_INFINITY, f64::max);
        let min_y = cells.iter().map(|c| c.y_mm).fold(f64::INFINITY, f64::min);
        let max_y = cells
            .iter()
            .map(|c| c.y_mm + c.h_mm)
            .fold(f64::NEG_INFINITY, f64::max);
        assert_mm(max_x - min_x, 23.88, &format!("{what}: ring x extent"));
        assert_mm(max_y - min_y, 23.88, &format!("{what}: ring y extent"));
        assert_mm(
            f64::midpoint(min_x, max_x),
            cx,
            &format!("{what}: ring centre x"),
        );
        assert_mm(
            f64::midpoint(min_y, max_y),
            cy,
            &format!("{what}: ring centre y"),
        );
    }

    #[test]
    fn p2b8_pots_stack_in_one_measured_column() {
        // Bullet: P2B8 pots — one centred column at ~24.8 mm pitch (was a
        // two-cell row at 10 mm).
        let data = ControllerGeometry::load().expect("controller_geometry.json loads");
        let pots = family_cells(&data, "p2b8", "P");
        assert_eq!(pots.len(), 2, "p2b8 carries two pots");
        assert_eq!(x_centres(pots).len(), 1, "pots must share one column");
        let ys = y_centres(pots);
        assert_eq!(ys.len(), 2, "pots must occupy two rows");
        assert_mm(ys[0], 19.1, "P1.1 y centre");
        assert_mm(ys[1], 43.8, "P1.2 y centre");
        assert_mm(ys[1] - ys[0], 24.7, "pot pitch y");
        for c in pots {
            assert_mm(cell_centre(c).0, 12.7, "pot centre x");
            assert_mm(c.w_mm, 12.0, "pot cell width");
            assert_mm(c.h_mm, 12.0, "pot cell height");
        }

        // Model path: both pots resolve onto the same column.
        let layout = PhysicalLayout::build(Some(&patch("[p2b8]\n")), MasterModel::Master, &[]);
        assert_eq!(layout.modules[1].geometry_key, "p2b8");
        let p1 = layout.cell_for(1, "P1.1").expect("P1.1 cell");
        let p2 = layout.cell_for(1, "P1.2").expect("P1.2 cell");
        assert_eq!(p1.col, p2.col, "pots must share one column");
        assert_ne!(p1.row, p2.row, "pots must be on separate rows");
    }

    #[test]
    fn p2b8_buttons_use_measured_12_6_by_15_grid() {
        // Bullet: P2B8 button pitch 12.6 × 15.0 (was 9.2 × 15.0), 9×9 cells
        // (was 8×8), LEDs sharing the grid as 3×3 cells.
        let data = ControllerGeometry::load().expect("controller_geometry.json loads");
        let buttons = family_cells(&data, "p2b8", "B");
        assert_eq!(buttons.len(), 8, "p2b8 carries eight buttons");
        let xs = x_centres(buttons);
        let ys = y_centres(buttons);
        assert_eq!(xs.len(), 2, "buttons must resolve to two columns");
        assert_eq!(ys.len(), 4, "buttons must resolve to four rows");
        assert_mm(xs[0], 7.1, "button col 0 x centre");
        assert_mm(xs[1], 19.7, "button col 1 x centre");
        assert_mm(xs[1] - xs[0], 12.6, "button pitch x");
        assert_mm(ys[0], 68.5, "button row 0 y centre");
        assert_mm(ys[1], 83.5, "button row 1 y centre");
        assert_mm(ys[2], 98.4, "button row 2 y centre");
        assert_mm(ys[3], 113.4, "button row 3 y centre");
        assert_mm(ys[1] - ys[0], 15.0, "button pitch y");
        for c in buttons {
            assert_mm(c.w_mm, 9.0, "button cell width");
            assert_mm(c.h_mm, 9.0, "button cell height");
        }

        let leds = family_cells(&data, "p2b8", "L");
        assert_eq!(leds.len(), 8, "p2b8 carries eight LEDs");
        let lx = x_centres(leds);
        let ly = y_centres(leds);
        assert_eq!(lx.len(), 2, "LEDs must resolve to two columns");
        assert_eq!(ly.len(), 4, "LEDs must resolve to four rows");
        assert_mm(lx[0], 7.1, "LED col 0 x centre");
        assert_mm(lx[1], 19.7, "LED col 1 x centre");
        for (got, want) in ly.iter().zip([68.5, 83.5, 98.4, 113.4]) {
            assert_mm(*got, want, "LED row y centre");
        }
        for c in leds {
            assert_mm(c.w_mm, 3.0, "LED cell width");
            assert_mm(c.h_mm, 3.0, "LED cell height");
        }
    }

    #[test]
    fn p4b2_pots_stack_in_one_measured_column() {
        // Bullet: P4B2 pots — one centred column at ~24.8 mm pitch (was two
        // columns × two rows); the two buttons sit on one row at y 113.4.
        let data = ControllerGeometry::load().expect("controller_geometry.json loads");
        let pots = family_cells(&data, "p4b2", "P");
        assert_eq!(pots.len(), 4, "p4b2 carries four pots");
        assert_eq!(x_centres(pots).len(), 1, "pots must share one column");
        let ys = y_centres(pots);
        assert_eq!(ys.len(), 4, "pots must occupy four rows");
        for (got, want) in ys.iter().zip([19.1, 43.8, 68.7, 93.5]) {
            assert_mm(*got, want, "P4B2 pot row y centre");
        }
        assert_mm(ys[1] - ys[0], 24.8, "P4B2 pot pitch y");
        for c in pots {
            assert_mm(cell_centre(c).0, 12.7, "P4B2 pot centre x");
            assert_mm(c.w_mm, 12.0, "P4B2 pot cell width");
            assert_mm(c.h_mm, 12.0, "P4B2 pot cell height");
        }

        let buttons = family_cells(&data, "p4b2", "B");
        assert_eq!(buttons.len(), 2, "p4b2 carries two buttons");
        let bxs = x_centres(buttons);
        let bys = y_centres(buttons);
        assert_eq!(bxs.len(), 2, "P4B2 buttons must be a two-column row");
        assert_eq!(bys.len(), 1, "P4B2 buttons must be one row");
        assert_mm(bxs[0], 7.1, "P4B2 button col 0 x centre");
        assert_mm(bxs[1], 19.7, "P4B2 button col 1 x centre");
        assert_mm(bxs[1] - bxs[0], 12.6, "P4B2 button pitch x");
        assert_mm(bys[0], 113.4, "P4B2 button row y centre");

        // Model path: p4b2 is not bare-synthesized, so an explicit token is
        // needed to place the faceplate.
        let layout = PhysicalLayout::build(
            Some(&patch("[p4b2]\n    pot = P1.1\n")),
            MasterModel::Master,
            &[],
        );
        assert_eq!(layout.modules.len(), 2);
        assert_eq!(layout.modules[1].geometry_key, "p4b2");
        let p1 = layout.cell_for(1, "P1.1").expect("P1.1 cell");
        let p2 = layout.cell_for(1, "P1.2").expect("P1.2 cell");
        assert_eq!(p1.col, p2.col, "pots must share one column");
    }

    #[test]
    fn p10_big_pots_stack_and_small_pots_keep_matrix() {
        // Bullets: P10 big pots — one centred column at ~24.8 mm pitch; the
        // eight small pots keep their declared 2-column × 4-row matrix at
        // the measured 12.6 × 15.1 pitch.
        let data = ControllerGeometry::load().expect("controller_geometry.json loads");
        let pots = family_cells(&data, "p10", "P");
        assert_eq!(pots.len(), 10, "p10 carries ten pots");

        let big = head_cells(pots, 2);
        assert_eq!(big.len(), 2, "two big pots");
        assert_eq!(x_centres(&big).len(), 1, "big pots must share one column");
        let bys = y_centres(&big);
        assert_eq!(bys.len(), 2, "big pots must occupy two rows");
        assert_mm(bys[0], 19.3, "big pot 1 y centre");
        assert_mm(bys[1], 44.1, "big pot 2 y centre");
        assert_mm(bys[1] - bys[0], 24.8, "big pot pitch y");
        for c in &big {
            assert_mm(cell_centre(c).0, 12.7, "big pot centre x");
            assert_mm(c.w_mm, 12.0, "big pot cell width");
            assert_mm(c.h_mm, 12.0, "big pot cell height");
        }

        let small: Vec<ElementCellEntry> = pots
            .iter()
            .filter(|c| matches!(label_element(&c.label).0, Some(e) if e >= 3))
            .cloned()
            .collect();
        assert_eq!(small.len(), 8, "eight small pots");
        let sxs = x_centres(&small);
        let sys = y_centres(&small);
        assert_eq!(sxs.len(), 2, "small pots keep two columns");
        assert_eq!(sys.len(), 4, "small pots keep four rows");
        assert_mm(sxs[0], 6.9, "small pot col 0 x centre");
        assert_mm(sxs[1], 19.5, "small pot col 1 x centre");
        assert_mm(sxs[1] - sxs[0], 12.6, "small pot pitch x");
        for (got, want) in sys.iter().zip([68.6, 83.7, 98.8, 113.9]) {
            assert_mm(*got, want, "small pot row y centre");
        }
        assert_mm(sys[1] - sys[0], 15.1, "small pot pitch y");
        for c in &small {
            assert_mm(c.w_mm, 9.0, "small pot cell width");
            assert_mm(c.h_mm, 9.0, "small pot cell height");
        }

        // Model path: big pots share a column; the small pots fill a complete
        // 2-column × 4-row matrix (numbering-independent).
        let layout = PhysicalLayout::build(Some(&patch("[p10]\n")), MasterModel::Master, &[]);
        assert_eq!(layout.modules[1].geometry_key, "p10");
        let b1 = layout.cell_for(1, "P1.1").expect("P1.1 cell");
        let b2 = layout.cell_for(1, "P1.2").expect("P1.2 cell");
        assert_eq!(b1.col, b2.col, "big pots must share one column");
        let small_cells: Vec<&ElementCell> = (3..=10)
            .map(|i| {
                layout
                    .cell_for(1, &format!("P1.{i}"))
                    .expect("small pot cell")
            })
            .collect();
        let cols: std::collections::BTreeSet<u32> = small_cells.iter().map(|c| c.col).collect();
        let rows: std::collections::BTreeSet<u32> = small_cells.iter().map(|c| c.row).collect();
        let pairs: std::collections::BTreeSet<(u32, u32)> =
            small_cells.iter().map(|c| (c.col, c.row)).collect();
        assert_eq!(cols.len(), 2, "small pots keep two columns");
        assert_eq!(rows.len(), 4, "small pots keep four rows");
        assert_eq!(pairs.len(), 8, "small pots fill a 2x4 matrix");
    }

    #[test]
    fn s10_rotaries_stack_and_small_switches_keep_matrix() {
        // Bullets: S10 rotary switches — one centred column at ~24.8 mm
        // pitch, 12×12 cells; the eight small switches keep the same 2 × 4
        // grid as the P10 small pots, as 8×12 cells.
        let data = ControllerGeometry::load().expect("controller_geometry.json loads");
        let switches = family_cells(&data, "s10", "S");
        assert_eq!(switches.len(), 10, "s10 carries ten switches");

        let rotary = head_cells(switches, 2);
        assert_eq!(rotary.len(), 2, "two rotary switches");
        assert_eq!(
            x_centres(&rotary).len(),
            1,
            "rotaries must share one column"
        );
        let rys = y_centres(&rotary);
        assert_eq!(rys.len(), 2, "rotaries must occupy two rows");
        assert_mm(rys[0], 18.5, "rotary 1 y centre");
        assert_mm(rys[1], 43.3, "rotary 2 y centre");
        assert_mm(rys[1] - rys[0], 24.8, "rotary pitch y");
        for c in &rotary {
            assert_mm(cell_centre(c).0, 12.7, "rotary centre x");
            assert_mm(c.w_mm, 12.0, "rotary cell width");
            assert_mm(c.h_mm, 12.0, "rotary cell height");
        }

        let small: Vec<ElementCellEntry> = switches
            .iter()
            .filter(|c| matches!(label_element(&c.label).0, Some(e) if e >= 3))
            .cloned()
            .collect();
        assert_eq!(small.len(), 8, "eight small switches");
        let sxs = x_centres(&small);
        let sys = y_centres(&small);
        assert_eq!(sxs.len(), 2, "small switches keep two columns");
        assert_eq!(sys.len(), 4, "small switches keep four rows");
        assert_mm(sxs[0], 6.9, "small switch col 0 x centre");
        assert_mm(sxs[1], 19.5, "small switch col 1 x centre");
        for (got, want) in sys.iter().zip([68.6, 83.7, 98.8, 113.9]) {
            assert_mm(*got, want, "small switch row y centre");
        }
        for c in &small {
            assert_mm(c.w_mm, 8.0, "small switch cell width");
            assert_mm(c.h_mm, 12.0, "small switch cell height");
        }

        // Model path: rotaries share a column; small switches fill a complete
        // 2-column × 4-row matrix (numbering-independent).
        let layout = PhysicalLayout::build(Some(&patch("[s10]\n")), MasterModel::Master, &[]);
        assert_eq!(layout.modules[1].geometry_key, "s10");
        let r1 = layout.cell_for(1, "S1.1").expect("S1.1 cell");
        let r2 = layout.cell_for(1, "S1.2").expect("S1.2 cell");
        assert_eq!(r1.col, r2.col, "rotaries must share one column");
        let small_cells: Vec<&ElementCell> = (3..=10)
            .map(|i| {
                layout
                    .cell_for(1, &format!("S1.{i}"))
                    .expect("small switch cell")
            })
            .collect();
        let cols: std::collections::BTreeSet<u32> = small_cells.iter().map(|c| c.col).collect();
        let rows: std::collections::BTreeSet<u32> = small_cells.iter().map(|c| c.row).collect();
        let pairs: std::collections::BTreeSet<(u32, u32)> =
            small_cells.iter().map(|c| (c.col, c.row)).collect();
        assert_eq!(cols.len(), 2, "small switches keep two columns");
        assert_eq!(rows.len(), 4, "small switches keep four rows");
        assert_eq!(pairs.len(), 8, "small switches fill a 2x4 matrix");
    }

    #[test]
    fn e4_encoders_stack_in_one_measured_column() {
        // Bullet: E4 four-encoder stack at 27.8 mm pitch, one centred column
        // at x 15.24 (was a four-cell row at 7 mm), with 3×3 shaft buttons.
        let data = ControllerGeometry::load().expect("controller_geometry.json loads");
        let encoders = family_cells(&data, "e4", "E");
        assert_eq!(encoders.len(), 4, "e4 carries four encoders");
        assert_eq!(
            x_centres(encoders).len(),
            1,
            "encoders must share one column"
        );
        let ys = y_centres(encoders);
        assert_eq!(ys.len(), 4, "encoders must occupy four rows");
        for (got, want) in ys.iter().zip([24.3, 52.1, 79.9, 107.7]) {
            assert_mm(*got, want, "encoder row y centre");
        }
        assert_mm(ys[1] - ys[0], 27.8, "encoder pitch y");
        for c in encoders {
            assert_mm(cell_centre(c).0, 15.24, "encoder centre x");
            assert_mm(c.w_mm, 7.0, "encoder cell width");
            assert_mm(c.h_mm, 7.0, "encoder cell height");
        }

        let buttons = family_cells(&data, "e4", "B");
        assert_eq!(buttons.len(), 4, "e4 carries four shaft buttons");
        for c in buttons {
            assert_mm(c.w_mm, 3.0, "shaft button cell width");
            assert_mm(c.h_mm, 3.0, "shaft button cell height");
        }
        assert_eq!(
            x_centres(buttons).len(),
            1,
            "shaft buttons share the encoder column"
        );
        assert_eq!(
            y_centres(buttons).len(),
            4,
            "shaft buttons share the encoder rows"
        );

        // Model path.
        let layout = PhysicalLayout::build(Some(&patch("[e4]\n")), MasterModel::Master, &[]);
        assert_eq!(layout.modules[1].geometry_key, "e4");
        let e1 = layout.cell_for(1, "E1.1").expect("E1.1 cell");
        let e2 = layout.cell_for(1, "E1.2").expect("E1.2 cell");
        assert_eq!(e1.col, e2.col, "encoders must share one column");
        assert_ne!(e1.row, e2.row, "encoders must be on separate rows");
    }

    #[test]
    fn db8e_encoder_sits_below_its_button_grid() {
        // Bullet: DB8E encoder below its buttons — 2 × 4 button grid at
        // x 7.9/23.1, rows y 41.4/56.6/71.6/86.8, encoder at (15.24, 108.1)
        // (was beside the buttons at ~y 56).
        let data = ControllerGeometry::load().expect("controller_geometry.json loads");
        let buttons = family_cells(&data, "db8e", "B");
        assert_eq!(buttons.len(), 8, "db8e carries eight buttons");
        let bxs = x_centres(buttons);
        let bys = y_centres(buttons);
        assert_eq!(bxs.len(), 2, "buttons must be two columns");
        assert_eq!(bys.len(), 4, "buttons must be four rows");
        assert_mm(bxs[0], 7.9, "button col 0 x centre");
        assert_mm(bxs[1], 23.1, "button col 1 x centre");
        assert_mm(bxs[1] - bxs[0], 15.2, "button pitch x");
        assert_mm(bys[0], 41.4, "button row 0 y centre");
        assert_mm(bys[1], 56.6, "button row 1 y centre");
        assert_mm(bys[2], 71.6, "button row 2 y centre");
        assert_mm(bys[3], 86.8, "button row 3 y centre");
        assert_mm(bys[1] - bys[0], 15.2, "button pitch y");
        for c in buttons {
            assert_mm(c.w_mm, 9.0, "button cell width");
            assert_mm(c.h_mm, 9.0, "button cell height");
        }

        let encoders = family_cells(&data, "db8e", "E");
        assert_eq!(encoders.len(), 1, "db8e carries one encoder");
        let enc = &encoders[0];
        let (ecx, ecy) = cell_centre(enc);
        assert_mm(ecx, 15.24, "encoder centre x");
        assert_mm(ecy, 108.1, "encoder centre y");
        assert_mm(enc.w_mm, 7.0, "encoder cell width");
        assert_mm(enc.h_mm, 7.0, "encoder cell height");
        // Below the whole button grid, on the grid's centre axis.
        let grid_bottom = buttons
            .iter()
            .map(|c| c.y_mm + c.h_mm)
            .fold(f64::NEG_INFINITY, f64::max);
        assert!(
            enc.y_mm > grid_bottom,
            "encoder must sit below the button grid ({} vs {grid_bottom})",
            enc.y_mm
        );
        assert_mm(
            ecx,
            f64::midpoint(bxs[0], bxs[1]),
            "encoder centred under the grid",
        );

        // Model path: db8e is not bare-synthesized, so a token is required.
        let layout = PhysicalLayout::build(
            Some(&patch("[db8e]\n    button1 = B1.1\n")),
            MasterModel::Master,
            &[],
        );
        assert_eq!(layout.modules[1].geometry_key, "db8e");
        let b1 = layout.cell_for(1, "B1.1").expect("B1.1 cell");
        let e1 = layout.cell_for(1, "E1.1").expect("E1.1 cell");
        assert!(
            e1.rect_mm.y_mm > b1.rect_mm.y_mm + b1.rect_mm.h_mm,
            "encoder must render below the buttons"
        );
    }

    #[test]
    fn e4_and_db8e_led_rings_span_the_measured_extent() {
        // Bullet: E4/DB8E ring cell extents — each 32-LED ring is a ~23.9 mm
        // square: 9 LEDs per side at 2.76 mm pitch with 1.8 mm cells (was a
        // 1 mm cell / 9 mm ring). Assertions stay structural.
        let data = ControllerGeometry::load().expect("controller_geometry.json loads");

        let e_rings = family_cells(&data, "e4", "L");
        assert_eq!(e_rings.len(), 128, "e4 carries four 32-LED rings");
        let encoders = family_cells(&data, "e4", "E");
        let centres: Vec<(f64, f64)> = encoders.iter().map(cell_centre).collect();
        assert_eq!(centres.len(), 4);
        for (i, &(cx, cy)) in centres.iter().enumerate() {
            let ring = ring_cells(e_rings, cx, cy);
            assert_ring(&ring, cx, cy, &format!("e4 ring {}", i + 1));
        }

        let d_rings = family_cells(&data, "db8e", "L");
        assert_eq!(d_rings.len(), 32, "db8e carries one 32-LED ring");
        let (dcx, dcy) = cell_centre(&family_cells(&data, "db8e", "E")[0]);
        let ring = ring_cells(d_rings, dcx, dcy);
        assert_ring(&ring, dcx, dcy, "db8e ring");
    }

    #[test]
    fn isotropic_default_mapping_uses_one_factor() {
        // button-led-composition D1: one isotropic cells-per-mm factor, so a
        // square millimetre rect maps to a square screen rect. Written
        // against today's ScreenMapping surface; must FAIL on the 0.15/0.3 pair.
        let m = ScreenMapping::default();
        assert_close(m.points_per_mm, PHYSICAL_POINTS_PER_MM);
        assert_close(m.points_per_mm, PHYSICAL_POINTS_PER_MM);
    }

    #[test]
    fn isotropic_square_mm_rect_maps_to_square_screen_rect() {
        // A square mm rect maps to equal width and height in screen cells.
        let m = ScreenMapping::default();
        let sq = RectMm {
            x_mm: 5.0,
            y_mm: 7.0,
            w_mm: 9.0,
            h_mm: 9.0,
        };
        let (_, _, w, h) = m.mm_to_screen(sq);
        assert_close(w, h);
        assert_close(w, 9.0 * m.points_per_mm * m.zoom);
        assert_close(h, 9.0 * m.points_per_mm * m.zoom);
        let (sw, sh) = m.screen_size(sq);
        assert_close(sw, sh);
        // Integer rect keeps the square too (a 9x9 mm button is round, not a 2:1 ellipse).
        let (_, _, iw, ih) = m.mm_rect_to_screen(sq);
        assert_eq!(iw, ih);
    }

    #[test]
    fn isotropic_round_trip_uses_single_factor() {
        // Round-trip and size re-derived against the single factor.
        let mapping = ScreenMapping::default();
        let mm = RectMm {
            x_mm: 3.5,
            y_mm: 12.0,
            w_mm: 8.0,
            h_mm: 4.0,
        };
        let (sx, sy, sw, sh) = mapping.mm_to_screen(mm);
        assert_close(sw, mm.w_mm * mapping.points_per_mm);
        assert_close(sh, mm.h_mm * mapping.points_per_mm);
        let (mm_x, mm_y) = mapping.screen_to_mm(sx, sy);
        assert_close(mm_x, mm.x_mm);
        assert_close(mm_y, mm.y_mm);
        let mapping_zoomed = ScreenMapping::new(PHYSICAL_POINTS_PER_MM, 1.5, 7.0, 3.0);
        let (sx2, sy2, sw2, sh2) = mapping_zoomed.mm_to_screen(mm);
        assert_close(sw2, mm.w_mm * mapping_zoomed.points_per_mm * 1.5);
        assert_close(sh2, mm.h_mm * mapping_zoomed.points_per_mm * 1.5);
        let (mm_x2, mm_y2) = mapping_zoomed.screen_to_mm(sx2, sy2);
        assert_close(mm_x2, mm.x_mm);
        assert_close(mm_y2, mm.y_mm);
    }

    #[test]
    fn isotropic_zoom_about_and_pan_use_single_factor() {
        // Zoom-about keeps its anchor and pan math re-derived on one factor.
        let m = ScreenMapping::new(PHYSICAL_POINTS_PER_MM, 1.0, 2.0, 4.0);
        assert_close(m.points_per_mm, PHYSICAL_POINTS_PER_MM);
        let (ax, ay) = (40.0, 12.0);
        let (amx, amy) = m.screen_to_mm(ax, ay);
        let z = m.zoom_about(2.0, ax, ay);
        let (nx, ny, _, _) = z.mm_to_screen(RectMm {
            x_mm: amx,
            y_mm: amy,
            w_mm: 1.0,
            h_mm: 1.0,
        });
        assert_close(nx, ax);
        assert_close(ny, ay);
        let mm = RectMm {
            x_mm: 10.0,
            y_mm: 20.0,
            w_mm: 5.0,
            h_mm: 6.0,
        };
        let (x0, y0, _, _) = m.mm_to_screen(mm);
        let p = m.pan(7.0, 11.0);
        let (x1, y1, _, _) = p.mm_to_screen(mm);
        assert_close(x1, x0 - 7.0);
        assert_close(y1, y0 - 11.0);
        let (mx1, my1) = p.screen_to_mm(x0, y0);
        assert_close(mx1, mm.x_mm + 7.0 / (m.points_per_mm * 1.0));
        assert_close(my1, mm.y_mm + 11.0 / (m.points_per_mm * 1.0));
    }

    #[test]
    fn isotropic_fold_bar_and_row_offset_use_single_factor() {
        // Fold-bar gap and row offsets re-derived against the single factor.
        let rack = two_row_rack();
        let m = ScreenMapping::default();
        assert_close(m.points_per_mm, PHYSICAL_POINTS_PER_MM);
        let row0 = &rack.rows[0];
        let row1 = &rack.rows[1];
        let p0 = row0.modules[0].rect_mm;
        let p1 = row1.modules[0].rect_mm;
        let y0_abs = row0.y_mm + p0.y_mm;
        let y1_abs = row1.y_mm + p1.y_mm;
        let (_, sy0, _, _) = m.mm_to_screen(RectMm {
            x_mm: p0.x_mm,
            y_mm: y0_abs,
            w_mm: p0.w_mm,
            h_mm: p0.h_mm,
        });
        let (_, sy1, _, _) = m.mm_to_screen(RectMm {
            x_mm: p1.x_mm,
            y_mm: y1_abs,
            w_mm: p1.w_mm,
            h_mm: p1.h_mm,
        });
        assert_close(sy1 - sy0, (y1_abs - y0_abs) * m.points_per_mm);
        assert_close(
            sy1 - (sy0 + p0.h_mm * m.points_per_mm),
            FOLD_BAR_HEIGHT_MM * m.points_per_mm,
        );
    }

    // ---- unreferenced-elements tasks 1.1/2.1: enumeration contract ----
    //
    // A partial B32 cannot be built from `[b32]` + `[button]`: any `[b32]`
    // section bare-synthesizes the complete 32 B + 32 L front (pinned to its
    // own panel), while `[button]` tokens (B2.N) fall through to a fallback
    // "Controller 2" module with no geometry. The partial-B32 fixtures below
    // therefore use `[notebuttons]` (usage-based section resolving to the
    // b32 geometry) with two button tokens.

    fn b32_module(layout: &PhysicalLayout) -> &PhysicalModule {
        layout
            .modules
            .iter()
            .find(|m| m.geometry_key == "b32")
            .expect("b32 faceplate")
    }

    fn e4_module(layout: &PhysicalLayout) -> &PhysicalModule {
        layout
            .modules
            .iter()
            .find(|m| m.geometry_key == "e4")
            .expect("e4 faceplate")
    }

    #[test]
    fn unaddressed_cells_excludes_addressed_pairs() {
        // Partial B32 via [notebuttons] with 2 buttons: geometry resolves
        // B2.9 while no component claims it.
        let p = patch("[notebuttons]\nbutton = B2.1\nbutton = B2.2\n");
        let layout = PhysicalLayout::build(Some(&p), MasterModel::Master, &[]);
        let ix = layout
            .modules
            .iter()
            .position(|m| m.geometry_key == "b32")
            .unwrap();
        assert!(
            layout.cell_for(ix, "B2.9").is_some(),
            "geometry resolves B2.9, no component claims it"
        );
        let module = b32_module(&layout);
        let total: usize = module.cells.values().map(Vec::len).sum();
        assert!(
            total > module.components.len(),
            "gap exists: {total} cells vs {} components",
            module.components.len()
        );
        assert_eq!(
            module.unaddressed_cells().len(),
            total - module.components.len(),
            "stub must surface the addressed-excluded gap"
        );
    }

    #[test]
    fn unaddressed_cells_includes_ring_segments() {
        // Bare [e4]: 128 L-family ring cells vs 4 synthesized L components.
        let layout = PhysicalLayout::build(Some(&patch("[e4]\n")), MasterModel::Master, &[]);
        let module = e4_module(&layout);
        let rings = module.cells.get("L").map_or(0, Vec::len);
        assert_eq!(rings, 128, "E4 L-family carries 128 ring segments");
        assert_eq!(
            module
                .components
                .iter()
                .filter(|c| c.id.starts_with('L'))
                .count(),
            4
        );
        assert_eq!(
            module.unaddressed_cells().len(),
            124,
            "stub must surface the 124 unaddressed ring segments"
        );
    }

    #[test]
    fn unaddressed_cells_includes_unused_buttons() {
        // Partial B32 via [notebuttons]: 32 B cells, 2 claimed -> 30 unused.
        let p = patch("[notebuttons]\nbutton = B2.1\nbutton = B2.2\n");
        let layout = PhysicalLayout::build(Some(&p), MasterModel::Master, &[]);
        let module = b32_module(&layout);
        assert_eq!(module.cells.get("B").map_or(0, Vec::len), 32);
        let got: Vec<_> = module
            .unaddressed_cells()
            .into_iter()
            .filter(|c| c.family == "B")
            .collect();
        assert_eq!(got.len(), 30, "stub must surface the 30 unused buttons");
    }

    #[test]
    fn unaddressed_cells_skips_element_none_cells() {
        // Cells with `element: None` (e.g. "USB") never match a token pair
        // and must be skipped: the surfaced set must equal total minus
        // addressed minus element-less cells.
        let layout = PhysicalLayout::build(Some(&patch("[e4]\n")), MasterModel::Master, &[]);
        let module = e4_module(&layout);
        let noneless: usize = module
            .cells
            .values()
            .flatten()
            .filter(|c| c.element.is_none())
            .count();
        let total: usize = module.cells.values().map(Vec::len).sum();
        let surfaced = module.unaddressed_cells();
        assert!(surfaced.iter().all(|c| c.element.is_some()));
        assert_eq!(
            surfaced.len(),
            total - module.components.len() - noneless,
            "stub must skip element-less cells"
        );
    }

    #[test]
    fn unaddressed_cells_fallback_modules_emit_empty() {
        // Fallback modules carry cells for no family, so enumeration is
        // empty *because there is nothing to enumerate*.
        let p = patch("[copy]\n    button = B7.1\n");
        let layout = PhysicalLayout::build(Some(&p), MasterModel::Master, &[]);
        let module = layout
            .modules
            .iter()
            .find(|m| m.is_fallback)
            .expect("fallback module");
        let total: usize = module.cells.values().map(Vec::len).sum();
        assert_eq!(total, 0, "fallback modules carry no cells");
        assert!(module.unaddressed_cells().is_empty());
    }

    #[test]
    fn unaddressed_cells_order_deterministic() {
        // D1: family key, then row, then col — never HashMap order. The
        // expected keys are rebuilt from the addressed-excluded set and
        // sorted independently, so the assertion pins both content and order.
        let p = patch("[notebuttons]\nbutton = B2.1\nbutton = B2.2\n");
        let layout = PhysicalLayout::build(Some(&p), MasterModel::Master, &[]);
        let module = b32_module(&layout);
        let got: Vec<(String, u32, u32)> = module
            .unaddressed_cells()
            .into_iter()
            .map(|c| (c.family.clone(), c.row, c.col))
            .collect();
        let mut expected = got.clone();
        expected.sort();
        assert_eq!(got, expected, "must emit family/row/col order");
        assert!(!got.is_empty(), "partial B32 leaves a gap to order");
        // Rebuild from a second call: deterministic across runs.
        let again: Vec<(String, u32, u32)> = module
            .unaddressed_cells()
            .into_iter()
            .map(|c| (c.family.clone(), c.row, c.col))
            .collect();
        assert_eq!(got, again, "stable order across repeated runs");
    }

    // s10-switch-glyphs task 1.1: loader surfaces the JSON `kind`
    // annotation via the real field.
    fn s10_s_cells() -> Vec<ElementCellEntry> {
        let data = ControllerGeometry::load().expect("controller_geometry.json loads");
        let s10 = data.controller("s10").expect("s10 entry");
        s10.element_cells.get("S").expect("s10 S family").clone()
    }

    #[test]
    fn s10_switch_kind_rotary_loads() {
        let cells = s10_s_cells();
        assert_eq!(cells.len(), 10, "s10 must carry 10 S entries");
        let s11 = cells
            .iter()
            .find(|c| c.label == "S1.1")
            .expect("S1.1 entry");
        assert_eq!(s11.kind.as_deref(), Some("rotary"));
    }

    #[test]
    fn s10_switch_kind_small_loads() {
        let cells = s10_s_cells();
        let s13 = cells
            .iter()
            .find(|c| c.label == "S1.3")
            .expect("S1.3 entry");
        assert_eq!(s13.kind.as_deref(), Some("small"));
    }

    #[test]
    fn entry_without_kind_loads_none() {
        let data = ControllerGeometry::load().expect("controller_geometry.json loads");
        let p2b8 = data.controller("p2b8").expect("p2b8 entry");
        let b = p2b8.element_cells.get("B").expect("p2b8 B family");
        assert!(!b.is_empty(), "p2b8 must carry B entries");
        for entry in b {
            assert_eq!(
                entry.kind.as_deref(),
                None,
                "{} has no kind annotation",
                entry.label
            );
        }
    }

    #[test]
    fn m4_touch_kind_rides_along() {
        let data = ControllerGeometry::load().expect("controller_geometry.json loads");
        let m4 = data.controller("m4").expect("m4 entry");
        let b = m4.element_cells.get("B").expect("m4 B family");
        let b11 = b.iter().find(|c| c.label == "B1.1").expect("B1.1 entry");
        assert_eq!(b11.kind.as_deref(), Some("touch"));
    }

    // ---- module-ui-labels 1.1 (red): true 1:1 baseline, fit, zoom range ----

    /// The 96-dpi reference: one inch (25.4 mm) is 96 points.
    const POINTS_PER_MM_96DPI: f64 = 3.779_527_559;
    /// The design's measuring pane (design D3).
    const FIT_PANE_W: f32 = 640.0;
    const FIT_PANE_H: f32 = 520.0;

    /// A rack layout carrying only the two overall extents the fit and
    /// overflow math consume, at the design's measured rack size (D3:
    /// master + S10 + P10 = 92.94 mm wide, 3 HE = 128.5 mm tall).
    fn rack_extent_fixture(width_mm: f64, height_mm: f64) -> RackLayout {
        RackLayout {
            rows: Vec::new(),
            fold_bars: Vec::new(),
            mounts: RackMounts::default(),
            total_width_mm: width_mm,
            total_height_mm: height_mm,
            fold_bar_height_mm: FOLD_BAR_HEIGHT_MM,
        }
    }

    #[test]
    fn true_scale_is_ninety_six_points_per_inch() {
        // D1: one true-1:1 constant replaces the 0.15 cells/mm × 10 divide.
        assert_eq!(PHYSICAL_POINTS_PER_MM, POINTS_PER_MM_96DPI);
        let m = ScreenMapping::default();
        assert_eq!(m.points_per_mm, PHYSICAL_POINTS_PER_MM);
        assert_eq!(m.zoom, 1.0);
        let square = |mm: f64| RectMm {
            x_mm: 0.0,
            y_mm: 0.0,
            w_mm: mm,
            h_mm: mm,
        };
        // 25.4 mm = 1 in = 96 pt at 100 % zoom (physical-scale-model spec).
        let (_, _, w, _) = m.mm_to_screen(square(25.4));
        assert!((w - 96.0).abs() < 1e-6, "25.4 mm must be 96 pt, got {w}");
        // The same mapping is isotropic: a 9 mm button cell is ~34 pt square.
        let (_, _, bw, bh) = m.mm_to_screen(square(9.0));
        assert!(
            (bw - 34.0).abs() < 0.1,
            "a 9 mm cell must be ~34 pt, got {bw}"
        );
        assert!((bh - bw).abs() < 1e-9, "both axes share one points-per-mm");
    }

    #[test]
    fn zoom_range_spans_one_quarter_to_four() {
        // D5: the supported range reaches from 25 % to 400 % of the 1:1
        // baseline, so a 400 mm rack and a 2-row rack fit at the low end.
        assert_eq!(ZOOM_MIN, 0.25);
        assert_eq!(ZOOM_MAX, 4.0);
        const { assert!(ZOOM_MIN < 1.0 && ZOOM_MAX > 1.0) };
    }

    #[test]
    fn fit_zoom_fits_a_short_rack_by_height() {
        // Design D3: master + S10 + P10 (92.94 mm) in a 640 × 520 pane is 107 %.
        let zoom = fit_zoom(92.94, 128.5, FIT_PANE_W, FIT_PANE_H);
        assert_eq!((zoom * 100.0).round() as u32, 107, "got {zoom}");
        // Height-bound: the pane-height term is the minimum.
        let by_height = f64::from(FIT_PANE_H) / 128.5 / PHYSICAL_POINTS_PER_MM;
        assert!((zoom - by_height).abs() < 1e-9, "must be height-bound");
    }

    #[test]
    fn fit_zoom_fits_a_wide_rack_by_width() {
        // Design D3: a 400 mm rack in the same pane is 42 %.
        let zoom = fit_zoom(400.0, 128.5, FIT_PANE_W, FIT_PANE_H);
        assert_eq!((zoom * 100.0).round() as u32, 42, "got {zoom}");
        assert!(
            (zoom - 0.423_333_333_339_507).abs() < 1e-9,
            "width-bound fit"
        );
    }

    #[test]
    fn fit_zoom_fits_a_two_row_rack_by_height() {
        // Design D3: any 2-row 3 HE rack is 257 mm tall and fits at 54 %.
        let zoom = fit_zoom(173.0, 257.0, FIT_PANE_W, FIT_PANE_H);
        assert_eq!((zoom * 100.0).round() as u32, 54, "got {zoom}");
    }

    #[test]
    fn fit_zoom_clamps_to_the_supported_range() {
        // A tiny rack would need ~1376 %: the ceiling binds.
        assert_eq!(fit_zoom(10.0, 10.0, FIT_PANE_W, FIT_PANE_H), ZOOM_MAX);
        // A huge rack would need ~4 %: the floor binds.
        assert_eq!(fit_zoom(4000.0, 128.5, FIT_PANE_W, FIT_PANE_H), ZOOM_MIN);
        // A rack exactly at the floor stays there rather than drifting below.
        let floor_w = f64::from(FIT_PANE_W) / (PHYSICAL_POINTS_PER_MM * ZOOM_MIN);
        assert!(
            (fit_zoom(floor_w, 128.5, FIT_PANE_W, FIT_PANE_H) - ZOOM_MIN).abs() < 1e-9,
            "the floor is inclusive"
        );
    }

    #[test]
    fn fit_offset_centres_the_rack_in_the_pane() {
        let (w_mm, h_mm) = (92.94, 128.5);
        let zoom = fit_zoom(w_mm, h_mm, FIT_PANE_W, FIT_PANE_H);
        let (ox, oy) = fit_offset(w_mm, h_mm, FIT_PANE_W, FIT_PANE_H, zoom);
        let rack_w = w_mm * PHYSICAL_POINTS_PER_MM * zoom;
        let rack_h = h_mm * PHYSICAL_POINTS_PER_MM * zoom;
        assert!(
            (f64::from(ox) - (rack_w - f64::from(FIT_PANE_W)) / 2.0).abs() < 1e-3,
            "x offset must centre {rack_w} pt in {FIT_PANE_W} pt, got {ox}"
        );
        assert!(
            (f64::from(oy) - (rack_h - f64::from(FIT_PANE_H)) / 2.0).abs() < 1e-3,
            "y offset must centre {rack_h} pt in {FIT_PANE_H} pt, got {oy}"
        );
        // The rack's centre lands on the pane's centre under the fit mapping.
        let m = ScreenMapping::new(PHYSICAL_POINTS_PER_MM, zoom, f64::from(ox), f64::from(oy));
        let (cx, cy, _, _) = m.mm_to_screen(RectMm {
            x_mm: w_mm / 2.0,
            y_mm: h_mm / 2.0,
            w_mm: 0.0,
            h_mm: 0.0,
        });
        assert!(
            (cx - f64::from(FIT_PANE_W) / 2.0).abs() < 1e-2,
            "rack centre x must be the pane centre, got {cx}"
        );
        assert!(
            (cy - f64::from(FIT_PANE_H) / 2.0).abs() < 1e-2,
            "rack centre y must be the pane centre, got {cy}"
        );
    }

    #[test]
    fn rack_extent_points_scales_the_rack_at_the_current_zoom() {
        // The pure helper behind `App::physical_overflow` (task 1.5): the
        // rack's extent in points at the current zoom, never a stored size.
        let rack = rack_extent_fixture(92.94, 128.5);
        let (w1, h1) = rack_extent_points(&rack, 1.0);
        assert!(
            (f64::from(w1) - 92.94 * PHYSICAL_POINTS_PER_MM).abs() < 1e-3,
            "true-size width, got {w1}"
        );
        assert!(
            (f64::from(h1) - 128.5 * PHYSICAL_POINTS_PER_MM).abs() < 1e-3,
            "true-size height, got {h1}"
        );
        // At 4x the same rack no longer fits the 640 × 520 pane on either axis.
        let (w4, h4) = rack_extent_points(&rack, ZOOM_MAX);
        assert!((f64::from(w4) - 4.0 * 92.94 * PHYSICAL_POINTS_PER_MM).abs() < 1e-3);
        assert!((f64::from(h4) - 4.0 * 128.5 * PHYSICAL_POINTS_PER_MM).abs() < 1e-3);
        assert!(w4 > FIT_PANE_W && h4 > FIT_PANE_H);
        // At the fit zoom the extent is inside the pane on both axes.
        let fit = fit_zoom(92.94, 128.5, FIT_PANE_W, FIT_PANE_H);
        let (wf, hf) = rack_extent_points(&rack, fit);
        assert!(
            wf <= FIT_PANE_W + 1e-2 && hf <= FIT_PANE_H + 1e-2,
            "fit must not overflow: {wf} × {hf}"
        );
    }

    // -- module-ui-label-fit 1.2: the pure label layout (D1-D6) --------------

    /// Points for a millimetre span at the true-1:1 reference zoom, the same
    /// factor the geometry cells are built from.
    fn label_pt(mm: f32) -> f32 {
        mm * PHYSICAL_POINTS_PER_MM as f32
    }

    /// A label box at one cell's drawn size.
    fn label_box(
        w: f32,
        h: f32,
        glyph_chars: usize,
        path: CellTextPath,
        has_state: bool,
    ) -> LabelBox {
        LabelBox {
            w,
            h,
            glyph_chars,
            path,
            has_state,
        }
    }

    /// Assert a layout's step, lines, form and state row, and that the step is on
    /// the ladder and never below the floor (D3/D10).
    fn assert_label_layout(
        layout: &LabelLayout,
        font: f32,
        lines: &[&str],
        stacked: bool,
        state_row: bool,
        ctx: &str,
    ) {
        assert_eq!(layout.font, font, "{ctx}: the font step");
        assert_eq!(
            layout.lines,
            lines
                .iter()
                .map(std::string::ToString::to_string)
                .collect::<Vec<_>>(),
            "{ctx}: the lines"
        );
        assert_eq!(layout.stacked, stacked, "{ctx}: the stacked form");
        assert_eq!(layout.state_row, state_row, "{ctx}: the state row");
        assert!(
            LABEL_FONT_LADDER.contains(&layout.font),
            "{ctx}: {} pt is not a ladder step",
            layout.font
        );
        assert!(
            layout.font >= LABEL_FONT_FLOOR,
            "{ctx}: {} pt is below the floor",
            layout.font
        );
    }

    /// D2/D3: every step of the ladder is reachable — shrinking the box moves the
    /// same label one step down, and never past the floor.
    #[test]
    fn label_layout_reaches_every_ladder_step() {
        // `(box w, expected font)`: the box just holds four characters of the
        // expected step and only one line of it, so the next-higher step either
        // wraps into two lines (too tall) or hard-breaks.
        let cases = [
            (26.5, 10.0),
            (24.1, 9.0),
            (21.7, 8.0),
            (19.3, 7.0),
            (16.9, 6.0),
        ];
        for (w, font) in cases {
            let area = label_box(w, 1.6 * font, 0, CellTextPath::Compact, false);
            let layout = label_layout("abcd", &area).expect("a four-character label fits");
            assert_label_layout(&layout, font, &["abcd"], false, false, &format!("w={w}"));
        }
    }

    /// D2 (spec "Wrap before shrink"): a label that wraps at the current step is
    /// wrapped there instead of being stepped down to fit one line.
    #[test]
    fn label_layout_wraps_before_stepping_down() {
        // Two 10 pt lines of three characters: one line would need the 6 pt step.
        let area = label_box(20.0, 25.0, 0, CellTextPath::Compact, false);
        let layout = label_layout("ab cd", &area).expect("wraps at the top step");
        assert_label_layout(&layout, 10.0, &["ab", "cd"], false, false, "wrap first");
    }

    /// D2: a single word wider than the line is hard-broken, every character
    /// kept — an ellipsis is not spent on it while a step still holds it whole.
    #[test]
    fn label_layout_hard_breaks_a_word_wider_than_a_line() {
        // Five two-character lines at 10 pt (12.5 pt each) fill a 63 pt cell.
        let area = label_box(14.0, 63.0, 0, CellTextPath::Compact, false);
        let layout = label_layout("abcdefghij", &area).expect("hard-broken, not dropped");
        assert_label_layout(
            &layout,
            10.0,
            &["ab", "cd", "ef", "gh", "ij"],
            false,
            false,
            "hard break",
        );
    }

    /// D2: the ellipsis is the floor's last resort — a label no step holds whole
    /// reaches 6 pt, keeps the lines the box holds and closes the last one.
    #[test]
    fn label_layout_ellipsizes_only_at_the_floor() {
        // Two 6 pt lines of four characters: 20 characters cannot fit.
        let area = label_box(16.9, 16.0, 0, CellTextPath::Compact, false);
        let layout = label_layout("abcdefghijklmnopqrst", &area).expect("the floor still draws");
        assert_label_layout(
            &layout,
            6.0,
            &["abcd", "efg\u{2026}"],
            false,
            false,
            "floor ellipsis",
        );
        // The line including the ellipsis stays inside the four-character box.
        let last = layout.lines.last().expect("a last line");
        assert_eq!(last.chars().count(), 4, "the ellipsis replaces a character");
    }

    /// D4: a step that leaves room for the state row keeps it.
    #[test]
    fn label_layout_keeps_the_state_row_when_it_fits() {
        let area = label_box(60.0, 60.0, 1, CellTextPath::Compact, true);
        let layout = label_layout("Reverb", &area).expect("a roomy cell");
        assert_label_layout(&layout, 10.0, &["Reverb"], false, true, "state kept");
    }

    /// D4 (spec "The label beats the state row"): when the reserved state line
    /// leaves no step any room, the label takes the cell and the row is dropped.
    #[test]
    fn label_layout_drops_the_state_row_when_no_step_leaves_room() {
        // One 10 pt line plus the 9 pt state row is 23.75 pt; 16 pt holds the
        // label line only.
        let area = label_box(60.0, 16.0, 1, CellTextPath::Compact, true);
        let layout = label_layout("Reverb", &area).expect("the label takes the cell");
        assert_label_layout(&layout, 10.0, &["Reverb"], false, false, "state yielded");
    }

    /// D4's order of preference: the whole state-reserved ladder is tried before
    /// the label is allowed the full cell, so a smaller step beats losing the
    /// state row.
    #[test]
    fn label_layout_prefers_the_state_row_over_a_larger_step() {
        let area = label_box(26.5, 30.0, 0, CellTextPath::Compact, true);
        let layout = label_layout("Reverb", &area).expect("a step fits with the row");
        assert_label_layout(&layout, 7.0, &["Rever", "b"], false, true, "state first");
    }

    /// D6: the composed face gives the label the band beneath the shape — one
    /// line when the band is a single line tall.
    #[test]
    fn label_layout_composed_band_holds_one_line() {
        let area = label_box(30.0, 30.0, 0, CellTextPath::Composed, false);
        let layout = label_layout("Rev", &area).expect("one line in the band");
        assert_label_layout(&layout, 10.0, &["Rev"], false, false, "composed one line");
    }

    /// D6: a roomier band holds two lines at the top step.
    #[test]
    fn label_layout_composed_band_holds_two_lines() {
        // `h − 0.5 × w` = 30 pt of band under a 30 pt plate.
        let area = label_box(30.0, 45.0, 0, CellTextPath::Composed, false);
        let layout = label_layout("Reverb", &area).expect("two lines in the band");
        assert_label_layout(
            &layout,
            10.0,
            &["Reve", "rb"],
            false,
            false,
            "composed two lines",
        );
    }

    /// D6: a face that leaves no band keeps the shape and gets no label — the
    /// cell stays an annotation candidate rather than eating the face.
    #[test]
    fn label_layout_composed_without_a_band_has_no_layout() {
        let area = label_box(30.0, 15.0, 0, CellTextPath::Composed, false);
        assert_eq!(label_layout("Reverb", &area), None);
    }

    /// D5 (spec "A fader label stacks"): the P8S8's 6.5 pt column holds one
    /// glyph per line, at the largest step whose glyphs fit the cell height.
    #[test]
    fn label_layout_fader_stacks_a_narrow_column() {
        let w = label_pt(4.2);
        let h = label_pt(22.0);
        // Six glyphs fit at the top step (6 × 12.5 = 75 of 83 pt).
        let short = label_layout("Volume", &label_box(w, h, 0, CellTextPath::Fader, false))
            .expect("a stacked column");
        assert_label_layout(
            &short,
            10.0,
            &["V", "o", "l", "u", "m", "e"],
            true,
            false,
            "fader stack",
        );
        // D5's own measurement: ten glyphs need the 6 pt stack (75 of 83 pt).
        let long = label_layout(
            "Resonances",
            &label_box(w, h, 0, CellTextPath::Fader, false),
        )
        .expect("a floor stack");
        assert_eq!(long.font, 6.0, "ten glyphs only fit the floor");
        assert!(long.stacked, "still one glyph per line");
        assert_eq!(long.lines.len(), 10);
        // The 6.5 pt column cannot carry a value (see
        // `label_layout_fader_column_too_narrow_for_a_value`), so the label takes
        // the full height: six glyphs at the top step, no state row.
        let with_state = label_layout("Volume", &label_box(w, h, 0, CellTextPath::Fader, true))
            .expect("six glyphs, whole cell");
        assert_label_layout(
            &with_state,
            10.0,
            &["V", "o", "l", "u", "m", "e"],
            true,
            false,
            "fader stack, no room for a value",
        );
    }

    /// D4/D5 (task 3.7): the state row is only reported where a value can exist.
    /// At 1:1 the P8S8 is 4.2 mm × 22 mm (15.9 × 83.2 pt); its track narrows to
    /// `min(0.4 × 15.9, 10) = 6.36 pt`, leaving a `15.9 − 6.36 − 3 = 6.5 pt`
    /// column — short of the two advances of the 9 pt state font (10.8 pt) the
    /// painter needs, so it would paint a bare `…`. The label still stacks.
    #[test]
    fn label_layout_fader_column_too_narrow_for_a_value_has_no_state_row() {
        let p8s8 = label_box(label_pt(4.2), label_pt(22.0), 0, CellTextPath::Fader, true);
        let layout = label_layout("Volume", &p8s8).expect("the label still stacks");
        assert!(
            layout.stacked,
            "the narrow column still stacks one glyph per line: {layout:?}"
        );
        assert_eq!(
            layout.lines,
            ["V", "o", "l", "u", "m", "e"]
                .iter()
                .map(std::string::ToString::to_string)
                .collect::<Vec<_>>(),
            "the whole label, one glyph per line: {layout:?}"
        );
        assert!(
            !layout.state_row,
            "no value fits 6.5 pt at 9 pt, so no state row: {layout:?}"
        );
        // The gate is width, not height: the same cell with the label's room
        // reserved still has plenty of height to spare.
        assert!(
            layout.font >= LABEL_FONT_FLOOR,
            "the label keeps a legal step: {layout:?}"
        );
    }

    /// D4/D5 (task 3.7): a fader wide enough for a value keeps its state row —
    /// the M4's 7 mm cell is 26.5 pt, so its track clamps to 10 pt and leaves
    /// `26.5 − 10 − 3 = 13.5 pt`, which holds two advances of the 9 pt state font
    /// (10.8 pt) with room to spare.
    #[test]
    fn label_layout_wide_fader_keeps_the_state_row() {
        let m4 = label_box(label_pt(7.0), label_pt(62.0), 0, CellTextPath::Fader, true);
        let layout = label_layout("Volume", &m4).expect("a horizontal fader label");
        assert!(
            !layout.stacked,
            "a 13.5 pt column wraps horizontally: {layout:?}"
        );
        assert!(
            layout.state_row,
            "two 9 pt advances fit 13.5 pt, so the row stays: {layout:?}"
        );
        assert_label_layout(
            &layout,
            10.0,
            &["Vo", "lu", "me"],
            false,
            true,
            "M4 fader with state",
        );
    }

    /// D5: a fader wide enough for a line of text keeps a horizontal label — the
    /// M4's 13.5 pt column holds two 10 pt characters per line.
    #[test]
    fn label_layout_fader_keeps_a_horizontal_line_when_wide() {
        let area = label_box(label_pt(7.0), label_pt(62.0), 0, CellTextPath::Fader, false);
        let layout = label_layout("Volume", &area).expect("a horizontal fader label");
        assert_label_layout(&layout, 10.0, &["Vo", "lu", "me"], false, false, "M4 fader");
    }

    /// D9/D10: a 3 mm LED cell cannot hold a label at any step, at 1:1 or at a
    /// typical fit zoom, so the cell reports no layout and the label stays
    /// annotation/hover territory.
    ///
    /// Both wirings answer `None`: with the state glyph charged at the head of
    /// line 1 (nothing reaches the label at all) and with `glyph_chars` 0 as the
    /// module UI paints it today (the box's two-character budget is below the
    /// ellipsized form's legibility floor, so `G…` is not emitted).
    #[test]
    fn label_layout_returns_none_for_a_3mm_led_cell() {
        for zoom in [1.0f32, 0.66] {
            let side = label_pt(3.0) * zoom;
            for glyph_chars in [0usize, 1] {
                let area = label_box(side, side, glyph_chars, CellTextPath::Compact, true);
                assert_eq!(
                    label_layout("Gate", &area),
                    None,
                    "a 3 mm LED cell at zoom {zoom} (glyph {glyph_chars}) has no room for a label"
                );
            }
        }
    }

    /// D2/D10: the ellipsized form needs three visible characters — a two-character
    /// budget names nothing, so it is no layout at all, while a four-character
    /// budget draws exactly three and closes the line.
    #[test]
    fn label_layout_ellipsis_needs_three_visible_characters() {
        // Text boxes of exactly two and four 6 pt characters, one line tall.
        let two = label_box(9.2, 8.0, 0, CellTextPath::Compact, false);
        assert_eq!(
            label_layout("Gate", &two),
            None,
            "two characters plus the ellipsis would be a fragment"
        );
        let four = label_box(16.4, 8.0, 0, CellTextPath::Compact, false);
        let layout = label_layout("Gatekeeper", &four).expect("three characters plus the ellipsis");
        assert_label_layout(
            &layout,
            6.0,
            &["Gat\u{2026}"],
            false,
            false,
            "three visible",
        );
        let last = layout.lines.last().expect("a last line");
        assert_eq!(
            last.chars().filter(|c| *c != LABEL_ELLIPSIS).count(),
            3,
            "exactly three visible characters: {last:?}"
        );
    }

    /// D2/D9: the acceptance case behind the rule — a 3 × 3 mm cell carrying
    /// `Gate` used to paint `G…`; it now has no in-cell layout, so the paint path
    /// draws the state alone and the label comes from the annotation and hover
    /// reveal.
    #[test]
    fn label_layout_three_millimetre_gate_cell_has_no_layout() {
        let side = label_pt(3.0);
        assert_eq!(
            label_layout(
                "Gate",
                &label_box(side, side, 0, CellTextPath::Compact, true)
            ),
            None
        );
        // The same box still serves a label of two characters, which needs no
        // ellipsis at all: the rule is about fragments, not tiny cells.
        let short = label_layout("Go", &label_box(side, side, 0, CellTextPath::Compact, true))
            .expect("a two-character label fits a 3 mm cell");
        assert_label_layout(&short, 7.0, &["Go"], false, false, "short label");
    }

    /// An empty or blank label has nothing to draw, in any path.
    #[test]
    fn label_layout_returns_none_for_an_empty_label() {
        let roomy = label_box(80.0, 80.0, 1, CellTextPath::Compact, true);
        assert_eq!(label_layout("", &roomy), None);
        assert_eq!(label_layout("   ", &roomy), None);
        let fader = label_box(label_pt(7.0), label_pt(62.0), 0, CellTextPath::Fader, true);
        assert_eq!(label_layout("", &fader), None);
    }

    /// Determinism: the layout is a pure function of the box and the label, so
    /// repeated and independently built inputs answer identically.
    #[test]
    fn label_layout_is_deterministic() {
        let cases = [
            label_box(26.5, 16.0, 0, CellTextPath::Compact, false),
            label_box(16.9, 16.0, 0, CellTextPath::Compact, false),
            label_box(30.0, 45.0, 0, CellTextPath::Composed, false),
            label_box(label_pt(4.2), label_pt(22.0), 0, CellTextPath::Fader, true),
            label_box(11.34, 11.34, 1, CellTextPath::Compact, true),
        ];
        for area in cases {
            let first = label_layout("Filter cutoff resonance", &area);
            let second = label_layout("Filter cutoff resonance", &area);
            assert_eq!(first, second, "same box, same answer: {area:?}");
            let rebuilt = label_box(area.w, area.h, area.glyph_chars, area.path, area.has_state);
            assert_eq!(
                label_layout("Filter cutoff resonance", &rebuilt),
                first,
                "an equal box answers equally: {area:?}"
            );
        }
    }

    // -- module-ui-label-fit 2.2: the label fit (D7) --------------------------

    /// One written label on one cell, sized in millimetres at 100 % zoom.
    fn fit_request(
        label: &str,
        w_mm: f32,
        h_mm: f32,
        glyph_chars: usize,
        path: CellTextPath,
        has_state: bool,
    ) -> LabelFitRequest {
        LabelFitRequest {
            w: label_pt(w_mm),
            h: label_pt(h_mm),
            glyph_chars,
            path,
            has_state,
            label: label.to_string(),
        }
    }

    /// A 9 mm button cell (34 pt at 100 %) with its glyph and press state.
    fn button_fit_request(label: &str) -> LabelFitRequest {
        fit_request(label, 9.0, 9.0, 1, CellTextPath::Compact, true)
    }

    /// The grid candidate `steps` 1 % steps below `zoom`, computed the way the
    /// scan computes its candidates so a test compares grid points instead of
    /// float neighbours.
    fn grid_below(zoom: f64, steps: u32) -> f64 {
        let millis = (zoom * LABEL_FIT_MILLIS_PER_ZOOM).round() as u32;
        f64::from(millis - steps * LABEL_FIT_STEP_MILLIS) / LABEL_FIT_MILLIS_PER_ZOOM
    }

    /// D7: an empty request list constrains nothing, so the rack fit answers
    /// alone and the label fit reports `None`.
    #[test]
    fn label_fit_zoom_is_none_without_requests() {
        assert_eq!(label_fit_zoom(&[]), None, "nothing to serve");
    }

    /// D7: when the floor already draws every label whole there is nothing to
    /// lift, and `None` lets the rack fit decide the view.
    #[test]
    fn label_fit_zoom_is_none_when_the_floor_already_serves_the_label() {
        // A 20 mm cell (75.6 pt at 100 %) draws `Rev` whole at the 0.25 floor.
        let roomy = fit_request("Rev", 20.0, 20.0, 1, CellTextPath::Compact, true);
        assert!(
            roomy.draws_whole_label_at(ZOOM_MIN),
            "the fixture must be served by the floor"
        );
        assert_eq!(
            label_fit_zoom(std::slice::from_ref(&roomy)),
            None,
            "a label the floor already serves does not constrain the fit"
        );
    }

    /// D7/D2: a long label on a small cell lifts the fit above true size, and the
    /// reported zoom is the *first* grid point that draws the label whole — one
    /// step lower does not, so the answer is the smallest fit, not a rounding.
    #[test]
    fn label_fit_zoom_lifts_a_long_label_on_a_small_cell() {
        let button = button_fit_request("Trigger input threshold resonance filter cutoff");
        let zoom = label_fit_zoom(std::slice::from_ref(&button))
            .expect("a 46-character label on a 9 mm button needs a lift");
        assert!(zoom > 1.0, "the lift is above true size ({zoom})");
        assert!(zoom <= ZOOM_MAX, "the lift stays inside the range ({zoom})");
        assert!(
            button.draws_whole_label_at(zoom),
            "the answer draws the whole label ({zoom})"
        );
        let below = grid_below(zoom, 1);
        assert!(
            !button.draws_whole_label_at(below),
            "one grid step below {zoom} the label does not fit whole yet ({below})"
        );
    }

    /// D7/D9: a label no supported zoom can draw whole is *ignored*, not
    /// clamped — it neither lifts the fit to the ceiling nor holds it above a
    /// label that does fit, and on its own it constrains nothing.
    #[test]
    fn label_fit_zoom_ignores_a_label_no_zoom_can_serve() {
        let button = button_fit_request("Trigger input");
        let alone = label_fit_zoom(std::slice::from_ref(&button)).expect("a servable label");

        // A composed face whose band is empty at every zoom (D6, h = 0.5 w), and
        // a blank label: neither draws anything whole at the ceiling.
        let no_band = fit_request(
            "Trigger input",
            30.0,
            15.0,
            0,
            CellTextPath::Composed,
            false,
        );
        let blank = button_fit_request("   ");
        assert!(
            !no_band.draws_whole_label_at(ZOOM_MAX),
            "the fixture must be unservable even at the ceiling"
        );
        assert!(
            !blank.draws_whole_label_at(ZOOM_MAX),
            "a blank label draws nothing"
        );

        assert_eq!(
            label_fit_zoom(&[no_band.clone(), blank.clone()]),
            None,
            "ignored labels alone leave the fit to the rack"
        );
        let together = label_fit_zoom(&[button.clone(), no_band, blank])
            .expect("the servable label still decides the fit");
        assert_eq!(
            together, alone,
            "an unservable label neither lifts the fit nor drags it to the cap"
        );
    }

    /// D7: the reported fit is always a zoom the view can actually take — never
    /// above the ceiling, never below the floor — across every paint path.
    #[test]
    fn label_fit_zoom_never_leaves_the_supported_range() {
        let cases = [
            button_fit_request("Trigger input"),
            button_fit_request("Trigger input threshold"),
            button_fit_request("Trigger input threshold resonance filter cutoff"),
            fit_request("Ring light", 3.0, 3.0, 1, CellTextPath::Compact, true),
            fit_request("Volume", 4.2, 22.0, 0, CellTextPath::Fader, true),
            fit_request("Volume", 7.0, 62.0, 0, CellTextPath::Fader, false),
            fit_request("Reverb", 30.0, 45.0, 0, CellTextPath::Composed, false),
        ];
        for request in cases {
            if let Some(zoom) = label_fit_zoom(std::slice::from_ref(&request)) {
                assert!(
                    (ZOOM_MIN..=ZOOM_MAX).contains(&zoom),
                    "{:?} reported {zoom}, outside {ZOOM_MIN}..={ZOOM_MAX}",
                    request.label
                );
                assert!(zoom.is_finite(), "{:?} reported {zoom}", request.label);
            }
        }
        // No fit is ever reported above the ceiling for an unservable label.
        let no_band = fit_request(
            "Trigger input",
            30.0,
            15.0,
            0,
            CellTextPath::Composed,
            false,
        );
        assert_eq!(label_fit_zoom(&[no_band]), None);
    }

    /// D7: the fit serves *every* request, so it is the hardest label's own fit.
    #[test]
    fn label_fit_zoom_takes_the_hardest_label() {
        let short = button_fit_request("Trigger input");
        let long = button_fit_request("Trigger input threshold resonance filter cutoff");
        let short_fit = label_fit_zoom(std::slice::from_ref(&short)).expect("the short label");
        let long_fit = label_fit_zoom(std::slice::from_ref(&long)).expect("the long label");
        assert!(
            short_fit < long_fit,
            "the fixture must span two fits ({short_fit} vs {long_fit})"
        );
        assert_eq!(
            label_fit_zoom(&[short, long]),
            Some(long_fit),
            "the harder label decides the fit"
        );
    }

    /// Determinism: the same requests answer the same zoom on every call, an
    /// independently built list answers it too, and request order is irrelevant.
    #[test]
    fn label_fit_zoom_is_deterministic() {
        let requests = vec![
            button_fit_request("Trigger input"),
            fit_request("Ring light", 3.0, 3.0, 1, CellTextPath::Compact, true),
            fit_request("Volume", 4.2, 22.0, 0, CellTextPath::Fader, true),
        ];
        let first = label_fit_zoom(&requests);
        assert!(first.is_some(), "the fixture constrains the fit");
        assert_eq!(label_fit_zoom(&requests), first, "same input, same answer");

        let rebuilt = vec![
            button_fit_request("Trigger input"),
            fit_request("Ring light", 3.0, 3.0, 1, CellTextPath::Compact, true),
            fit_request("Volume", 4.2, 22.0, 0, CellTextPath::Fader, true),
        ];
        assert_eq!(
            label_fit_zoom(&rebuilt),
            first,
            "an equal list answers equally"
        );

        let mut reversed = rebuilt;
        reversed.reverse();
        assert_eq!(
            label_fit_zoom(&reversed),
            first,
            "request order is irrelevant"
        );
    }

    // ---- rack inventory (task 1.1) ----

    fn module(controller: &str, instance: usize) -> ModuleKey {
        ModuleKey {
            controller: String::from(controller),
            instance,
        }
    }

    #[test]
    fn module_key_parses_a_qualified_controller_and_one_based_instance() {
        assert_eq!(ModuleKey::parse("P2B8 2"), Some(module("p2b8", 2)));
        assert_eq!(ModuleKey::parse("  b32   12 "), Some(module("b32", 12)));
    }

    #[test]
    fn module_key_parses_a_bare_controller_as_instance_one() {
        assert_eq!(ModuleKey::parse("P2B8"), Some(module("p2b8", 1)));
        assert_eq!(ModuleKey::parse("p8s8"), Some(module("p8s8", 1)));
    }

    #[test]
    fn module_key_maps_every_master_alias_to_the_master_key() {
        for alias in ["master", "Master", "MASTER18", "CV I/O", " cv i/o "] {
            assert_eq!(
                ModuleKey::parse(alias),
                Some(module("master", 1)),
                "alias {alias:?}"
            );
        }
    }

    #[test]
    fn module_key_rejects_an_invalid_instance() {
        for raw in ["P2B8 0", "P2B8 x", "P2B8 -1", "P2B8 1 2", "", "   "] {
            assert_eq!(ModuleKey::parse(raw), None, "entry {raw:?}");
        }
    }

    #[test]
    fn inventory_skips_an_unknown_controller_without_panicking() {
        let spec = RackSpec {
            rows: vec![
                RackRow {
                    modules: vec![String::from("P2B8 1"), String::from("Nonsense 2")],
                    ..RackRow::default()
                },
                RackRow {
                    modules: vec![String::from("B32")],
                    ..RackRow::default()
                },
            ],
            ..RackSpec::default()
        };
        assert_eq!(
            spec.module_keys(),
            vec![module("p2b8", 1), module("b32", 1)]
        );
        assert_eq!(
            spec.module_keys(),
            spec.module_keys(),
            "the warning is once, the answer stable"
        );
    }

    #[test]
    fn inventory_resolves_a_usage_controller_to_its_module() {
        let spec = RackSpec {
            rows: vec![RackRow {
                modules: vec![String::from("Faderbank 2")],
                ..RackRow::default()
            }],
            ..RackSpec::default()
        };
        assert_eq!(spec.module_keys(), vec![module("p8s8", 2)]);
    }

    #[test]
    fn rack_rows_without_modules_still_deserialize() {
        let spec: RackSpec =
            toml::from_str("[[rows]]\nhe = 3\nhp = 84.0\nlabel = \"Main\"\n").unwrap();
        assert_eq!(spec.rows.len(), 1);
        assert_eq!(spec.rows[0].label.as_deref(), Some("Main"));
        assert!(
            spec.rows[0].modules.is_empty(),
            "an absent key must load as no inventory"
        );
        assert!(spec.module_keys().is_empty());
    }

    // ---- inventory-driven chain (task 2.1) ----

    /// Per-module shape of a chain: identity plus the component ids in order.
    fn chain_shape(layout: &PhysicalLayout) -> Vec<(String, Option<u32>, String, Vec<String>)> {
        layout
            .modules
            .iter()
            .map(|m| {
                (
                    m.controller.clone(),
                    m.module_instance,
                    m.geometry_key.clone(),
                    m.components.iter().map(|c| c.id.clone()).collect(),
                )
            })
            .collect()
    }

    #[test]
    fn empty_inventory_keeps_the_patch_derived_chain() {
        let p = patch("[p2b8]\n[copy]\n    input = I1\n    output = O1\n");
        let layout = PhysicalLayout::build(Some(&p), MasterModel::Master, &[]);

        assert!(!layout.inventory_driven);
        assert_eq!(layout.modules.len(), 2);
        assert_eq!(layout.modules[0].controller, "CV I/O");
        assert_eq!(layout.modules[0].module_instance, None);
        assert_eq!(layout.modules[0].components.len(), 2, "I1 + O1 on master");
        assert_eq!(layout.modules[1].controller, "P2B8");
        assert_eq!(layout.modules[1].module_instance, Some(1));
        assert!(!layout.modules[1].components.is_empty());
        // Declaration order decides the position, not an inventory.
        assert_close(layout.modules[1].rect_mm.x_mm, 41.14);

        // Listing the same modules in the same order reproduces the chain and
        // the component assignment exactly.
        let listed = PhysicalLayout::build(
            Some(&p),
            MasterModel::Master,
            &[module("master", 1), module("p2b8", 1)],
        );
        assert_eq!(chain_shape(&layout), chain_shape(&listed));
    }

    #[test]
    fn no_patch_and_no_inventory_draws_the_master_alone() {
        // The chain type is not optional, so the no-patch/no-inventory case
        // must still yield something: the master faceplate, never a panic.
        let layout = PhysicalLayout::build(None, MasterModel::Master18, &[]);

        assert!(!layout.inventory_driven);
        assert_eq!(layout.modules.len(), 1);
        assert_eq!(layout.modules[0].controller, "CV I/O");
        assert_eq!(layout.modules[0].geometry_key, "master18");
        assert!(layout.modules[0].components.is_empty());
        assert_close(layout.modules[0].rect_mm.x_mm, 0.0);
    }

    #[test]
    fn inventory_only_chain_places_the_listed_modules_in_order() {
        let layout = PhysicalLayout::build(
            None,
            MasterModel::Master,
            &[module("p2b8", 1), module("b32", 2)],
        );

        assert!(layout.inventory_driven);
        let keys: Vec<&str> = layout
            .modules
            .iter()
            .map(|m| m.geometry_key.as_str())
            .collect();
        assert_eq!(keys, vec!["master", "p2b8", "b32"], "master prepended");
        assert!(layout.modules.iter().all(|m| m.components.is_empty()));
        assert_eq!(layout.modules[1].module_instance, Some(1));
        assert_eq!(layout.modules[2].module_instance, Some(2));
        assert!(!layout.modules[1].is_fallback);
        assert!(!layout.modules[2].is_fallback);
        assert!(!layout.modules[1].cells.is_empty());
        assert!(layout.modules[1].rect_mm.w_mm > 0.0);

        // A listed master keeps its position; it is not prepended twice.
        let listed_master = PhysicalLayout::build(
            None,
            MasterModel::Master,
            &[module("master", 1), module("p2b8", 1)],
        );
        assert_eq!(listed_master.modules.len(), 2);
        assert_eq!(listed_master.modules[0].geometry_key, "master");
        assert_eq!(listed_master.modules[1].geometry_key, "p2b8");
    }

    #[test]
    fn an_inventory_module_the_patch_does_not_address_keeps_empty_components() {
        let p = patch("[p2b8]\n");
        let layout = PhysicalLayout::build(
            Some(&p),
            MasterModel::Master,
            &[module("p2b8", 1), module("b32", 1)],
        );

        assert_eq!(layout.modules.len(), 3);
        let b32 = &layout.modules[2];
        assert_eq!(b32.geometry_key, "b32");
        assert_eq!(b32.module_instance, Some(1));
        assert!(!b32.is_fallback, "the controller geometry still resolves");
        assert!(b32.components.is_empty(), "the patch does not address it");
        assert!(!b32.cells.is_empty());
        assert!(
            !b32.unaddressed_cells().is_empty(),
            "an unaddressed faceplate exposes its whole front"
        );
    }

    #[test]
    fn patch_modules_missing_from_the_inventory_are_appended_in_declaration_order() {
        // Bare sections number their instances by section position: p2b8 is 1,
        // b32 is 2, e4 is 3. Only the p2b8 is inventoried.
        let p = patch("[p2b8]\n[b32]\n[e4]\n");
        let layout = PhysicalLayout::build(Some(&p), MasterModel::Master, &[module("p2b8", 1)]);

        let keys: Vec<&str> = layout
            .modules
            .iter()
            .map(|m| m.geometry_key.as_str())
            .collect();
        assert_eq!(
            keys,
            vec!["master", "p2b8", "b32", "e4"],
            "patch hardware outside the inventory follows in declaration order"
        );
        assert!(!layout.modules[2].components.is_empty());
        assert!(!layout.modules[3].components.is_empty());
    }

    #[test]
    fn an_inventory_key_binds_that_instances_components() {
        let p = patch("[p2b8]\n[p2b8]\n");
        let layout = PhysicalLayout::build(Some(&p), MasterModel::Master, &[module("p2b8", 2)]);

        assert_eq!(layout.modules.len(), 3);
        assert_eq!(layout.modules[1].module_instance, Some(2));
        assert_eq!(layout.modules[1].key(), "P2B8 2");
        assert!(!layout.modules[1].components.is_empty());
        assert!(
            layout.modules[1]
                .components
                .iter()
                .all(|c| c.module_instance() == Some(2)),
            "instance 2's components land on instance 2"
        );
        // The other instance is appended, never duplicated or dropped.
        assert_eq!(layout.modules[2].module_instance, Some(1));
        assert_eq!(
            layout
                .modules
                .iter()
                .filter(|m| m.module_instance == Some(2))
                .count(),
            1
        );
    }

    #[test]
    fn inventory_driven_is_true_only_for_a_non_empty_inventory() {
        let p = patch("[p2b8]\n");
        assert!(!PhysicalLayout::build(Some(&p), MasterModel::Master, &[]).inventory_driven);
        assert!(!PhysicalLayout::build(None, MasterModel::Master, &[]).inventory_driven);
        assert!(
            PhysicalLayout::build(Some(&p), MasterModel::Master, &[module("p2b8", 1)])
                .inventory_driven
        );
        assert!(
            PhysicalLayout::build(None, MasterModel::Master, &[module("p2b8", 1)]).inventory_driven
        );
    }
}
