//! Physical grid model — ordered controller chain + per-module faceplate
//! geometry in millimetres (task 2.1 of physical-scale-model).
//!
//! Pure module: no terminal dependency. Mirrors the conventions of
//! `src/geometry.rs` (JSON data loading via `CARGO_MANIFEST_DIR` + serde,
//! case-insensitive controller resolution) and `src/graph.rs` (in-module
//! unit tests). Task 2.3 adds rack/row packing on top of the public chain
//! types (`PhysicalLayout` / `PhysicalModule`).

use std::collections::HashMap;
use std::path::Path;
use std::sync::Once;

use serde::{Deserialize, Serialize};

use crate::patch::{HwComponent, MasterRequirement, Patch};

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
}

// ---------------------------------------------------------------------------
// Loader
// ---------------------------------------------------------------------------

impl ControllerGeometry {
    /// Load `controller_geometry.json` from the crate/repo root, mirroring
    /// `RackGeometry::load` in `src/geometry.rs`. Never panics.
    pub fn load() -> Result<Self, String> {
        let manifest_dir = env!("CARGO_MANIFEST_DIR");
        let candidates = [
            format!("{manifest_dir}/controller_geometry.json"),
            "controller_geometry.json".to_string(),
            "../controller_geometry.json".to_string(),
            "./controller_geometry.json".to_string(),
        ];
        let mut last_err = String::new();
        for cand in &candidates {
            let p = Path::new(cand);
            if !p.exists() {
                last_err = format!("not found: {cand}");
                continue;
            }
            match std::fs::read_to_string(p) {
                Ok(s) => match serde_json::from_str::<Self>(&s) {
                    Ok(v) => return Ok(v),
                    Err(e) => {
                        return Err(format!("failed to parse {cand}: {e}"));
                    }
                },
                Err(e) => {
                    last_err = format!("failed to read {cand}: {e}");
                }
            }
        }
        Err(format!(
            "controller_geometry.json not found or unreadable. Tried: {}. Last error: {last_err}",
            candidates.join(", ")
        ))
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
    /// (it resolves to master/master18 via `Patch::master_requirement`).
    pub fn resolve_controller(&self, controller: &str) -> Option<String> {
        let c = controller.trim().to_ascii_lowercase();
        if self.controllers.contains_key(&c) {
            return Some(c);
        }
        match c.as_str() {
            // Direct module names (also resolvable when the data file is
            // absent and `controllers` is empty).
            "p2b8" | "b32" | "e4" | "m4" | "p8s8" | "p10" | "s10" | "db8e" | "g8" | "x7"
            | "r2m" | "r2c" | "p4b2" | "master" | "master18" => Some(c),
            // Usage-based controller sections -> physical module.
            "notebuttons" => Some(String::from("b32")),
            "faderbank" => Some(String::from("p8s8")),
            "encoder" => Some(String::from("e4")),
            "pot" => Some(String::from("p10")),
            "motorfader" => Some(String::from("m4")),
            _ => None,
        }
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
    let digits: String = s.chars().take_while(|c| c.is_ascii_digit()).collect();
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
    let kind = t.chars().next().filter(|c| c.is_ascii_alphabetic())?;
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
    let kind = label.chars().next().filter(|c| c.is_ascii_alphabetic());
    let rest = &label[kind.map(char::len_utf8).unwrap_or(0)..];
    let num = if let Some(dot) = rest.find('.') {
        leading_number(&rest[dot + 1..])
    } else {
        leading_number(rest)
    };
    (num, kind)
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
                    let (element, kind_letter) = label_element(&c.label);
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
                    }
                })
                .collect();
            (family.clone(), out)
        })
        .collect()
}

static DATA_WARN: Once = Once::new();
static CONTROLLER_WARN: Once = Once::new();

fn warn_once(gate: &Once, msg: &str) {
    gate.call_once(|| eprintln!("[warn] physical: {msg}"));
}

impl PhysicalLayout {
    /// Build the ordered controller chain for a patch.
    ///
    /// Modules appear in the order the patch declares its hardware
    /// (`hw_components` order), grouped by (controller, module instance);
    /// repeated instances of a controller become separate side-by-side
    /// faceplates. All CV I/O components share a single master faceplate
    /// (master or master18 via `Patch::master_requirement`) that never
    /// subdivides. Missing/malformed geometry data and unknown controllers
    /// degrade to the fallback width with a one-time warning — never a panic.
    pub fn build(patch: &Patch) -> Self {
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
        let master_key = match patch.master_requirement() {
            MasterRequirement::Master => "master",
            MasterRequirement::Master18 => "master18",
        };
        let fallback_w = data.fallback_width_mm();
        let fallback_h = data.fallback_height_mm();

        let mut modules: Vec<PhysicalModule> = Vec::new();
        let mut slots: Vec<(String, Option<u32>)> = Vec::new();

        for comp in &patch.hw_components {
            let key = if comp.controller == "CV I/O" {
                (String::from("CV I/O"), None)
            } else {
                (comp.controller.clone(), comp.module_instance())
            };
            if let Some(ix) = slots.iter().position(|s| *s == key) {
                modules[ix].components.push(comp.clone());
                continue;
            }
            slots.push(key.clone());

            let (geometry_key, is_fallback) = if comp.controller == "CV I/O" {
                (String::from(master_key), false)
            } else {
                match data.resolve_controller(&comp.controller) {
                    Some(k) => (k, false),
                    None => {
                        warn_once(
                            &CONTROLLER_WARN,
                            &format!(
                                "controller {:?} has no geometry entry; using fallback width",
                                comp.controller
                            ),
                        );
                        (String::new(), true)
                    }
                }
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

            modules.push(PhysicalModule {
                controller: comp.controller.clone(),
                module_instance: key.1,
                geometry_key,
                is_fallback,
                rect_mm: RectMm {
                    x_mm: 0.0,
                    y_mm: 0.0,
                    w_mm: width_mm,
                    h_mm: height_mm,
                },
                width_hp,
                he,
                cells,
                components: vec![comp.clone()],
            });
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
        }
    }

    /// Look up the element cell for a hardware token on a module.
    ///
    /// Matching prefers the exact (element, kind letter) pair — this is what
    /// keeps "O4" on the O-side master jack while "I4" stays on the I-side —
    /// then falls back to any cell with the same element number, then to the
    /// 1-based positional cell within the family. Returns `None` for
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
            .or_else(|| cells.get(element.checked_sub(1)? as usize))
    }
}

// ---------------------------------------------------------------------------
// Rack / case model (task 2.3) — rows, TE mounts, row assignment
// ---------------------------------------------------------------------------

/// Height of the fold-bar divider at each row boundary in mm. A model
/// constant the renderer maps with the same formula as the rows.
pub const FOLD_BAR_HEIGHT_MM: f64 = 5.0;

/// One case row. Height in HE (1 or 3), width in HP, optional label.
#[derive(Debug, Clone, Default, PartialEq, Deserialize, Serialize)]
pub struct RackRow {
    #[serde(default)]
    pub he: u32,
    #[serde(default)]
    pub hp: f64,
    #[serde(default)]
    pub label: Option<String>,
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
            }],
            top_mount_te: 0.0,
            side_mount_te: 0.0,
            assign: HashMap::new(),
        }
    }
}

impl PhysicalModule {
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
/// (fallback_height_mm for unknown HE values).
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
    /// Divider rect: spans the rows region, height = FOLD_BAR_HEIGHT_MM.
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
    /// hp_mm), then the next row starts; the last row accepts overflow. A
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
            let row = match override_row {
                Some(r) => r,
                None => {
                    let mut r = cursor;
                    loop {
                        let capacity = spec.rows[r].hp * hp_mm;
                        let x = if fills[r] > 0.0 {
                            fills[r] + gap
                        } else {
                            fills[r]
                        };
                        if x + module.rect_mm.w_mm <= capacity || r + 1 >= spec.rows.len() {
                            break r;
                        }
                        r += 1;
                    }
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
// Tests
// ---------------------------------------------------------------------------

// ---------------------------------------------------------------------------
// mm→screen mapping (D4/D5)
// ---------------------------------------------------------------------------

/// Default aspect-compensated mm→chars factors (D4): terminal cells are ~2:1
/// (wider than tall), so rows/mm ≈ 2 × cols/mm keeps physical proportions.
pub const PHYSICAL_COLS_PER_MM: f64 = 0.15;
pub const PHYSICAL_ROWS_PER_MM: f64 = 0.3;

/// The pure mm→screen mapping of the physical presentation (D4/D5). No
/// terminal dependency: screen rects are returned as f64 character-cell
/// coordinates and rounded only at draw time. One formula drives both the
/// skeleton reference and (later) the full view, keeping the 5.1 coincidence
/// contract exact by construction.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ScreenMapping {
    pub cols_per_mm: f64,
    pub rows_per_mm: f64,
    pub zoom: f64,
    pub offset_x: f64,
    pub offset_y: f64,
}

impl Default for ScreenMapping {
    fn default() -> Self {
        Self {
            cols_per_mm: PHYSICAL_COLS_PER_MM,
            rows_per_mm: PHYSICAL_ROWS_PER_MM,
            zoom: 1.0,
            offset_x: 0.0,
            offset_y: 0.0,
        }
    }
}

impl ScreenMapping {
    pub fn new(
        cols_per_mm: f64,
        rows_per_mm: f64,
        zoom: f64,
        offset_x: f64,
        offset_y: f64,
    ) -> Self {
        Self {
            cols_per_mm,
            rows_per_mm,
            zoom,
            offset_x,
            offset_y,
        }
    }

    /// mm rect → screen rect `(x, y, w, h)` in character cells (D5:
    /// screen = mm × factor × zoom − offset; size is scale-only, no offset).
    pub fn mm_to_screen(&self, mm: RectMm) -> (f64, f64, f64, f64) {
        (
            mm.x_mm * self.cols_per_mm * self.zoom - self.offset_x,
            mm.y_mm * self.rows_per_mm * self.zoom - self.offset_y,
            mm.w_mm * self.cols_per_mm * self.zoom,
            mm.h_mm * self.rows_per_mm * self.zoom,
        )
    }

    /// Inverse of `mm_to_screen` for a screen point (the round-trip test and
    /// the zoom-anchor math both use it).
    pub fn screen_to_mm(&self, x: f64, y: f64) -> (f64, f64) {
        (
            (x + self.offset_x) / (self.cols_per_mm * self.zoom),
            (y + self.offset_y) / (self.rows_per_mm * self.zoom),
        )
    }

    /// Zoom about a fixed screen anchor: returns a mapping with `new_zoom`
    /// and the offset adjusted so the mm point under `(ax, ay)` stays at the
    /// same screen position. Deterministic pure math (no RNG).
    pub fn zoom_about(&self, new_zoom: f64, ax: f64, ay: f64) -> Self {
        let (mx, my) = self.screen_to_mm(ax, ay);
        Self {
            zoom: new_zoom,
            offset_x: mx * self.cols_per_mm * new_zoom - ax,
            offset_y: my * self.rows_per_mm * new_zoom - ay,
            ..*self
        }
    }

    /// Pan by `(dx, dy)` screen cells: screen coords shift linearly by
    /// exactly `(dx, dy)` (and the inverse shifts the other way).
    pub fn pan(&self, dx: f64, dy: f64) -> Self {
        Self {
            offset_x: self.offset_x + dx,
            offset_y: self.offset_y + dy,
            ..*self
        }
    }

    /// Screen size `(w, h)` in cells of an mm rect under this mapping —
    /// overflow detection compares this against the visible area.
    pub fn screen_size(&self, mm: RectMm) -> (f64, f64) {
        (
            mm.w_mm * self.cols_per_mm * self.zoom,
            mm.h_mm * self.rows_per_mm * self.zoom,
        )
    }
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
        let m = ScreenMapping::default();
        let (x, y, w, h) = m.mm_to_screen(mm);
        assert_close(w, mm.w_mm * m.cols_per_mm);
        assert_close(h, mm.h_mm * m.rows_per_mm);
        let (mx, my) = m.screen_to_mm(x, y);
        assert_close(mx, mm.x_mm);
        assert_close(my, mm.y_mm);
        // With zoom + offset too.
        let m2 = ScreenMapping::new(0.15, 0.3, 1.5, 7.0, 3.0);
        let (x2, y2, w2, h2) = m2.mm_to_screen(mm);
        assert_close(w2, mm.w_mm * 0.15 * 1.5);
        assert_close(h2, mm.h_mm * 0.3 * 1.5);
        let (mx2, my2) = m2.screen_to_mm(x2, y2);
        assert_close(mx2, mm.x_mm);
        assert_close(my2, mm.y_mm);
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
        let m = ScreenMapping::new(0.15, 0.3, 1.0, 2.0, 4.0);
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
        assert_close(mx1, mm.x_mm + 7.0 / (0.15 * 1.0));
        assert_close(my1, mm.y_mm + 11.0 / (0.3 * 1.0));
    }

    #[test]
    fn screen_mapping_multi_row_offsets_include_fold_bars() {
        // Row 1 sits below row 0 plus exactly one fold bar: pack a two-row
        // rack and map both rows' module rects — the screen gap must equal
        // the mm gap × rows_per_mm × zoom (D4/D11).
        let p = patch("[p2b8]\n[copy]\n    input = I1\n    output = O1\n");
        let chain = PhysicalLayout::build(&p);
        let spec = RackSpec {
            rows: vec![
                RackRow {
                    he: 3,
                    hp: 12.0,
                    label: None,
                },
                RackRow {
                    he: 3,
                    hp: 12.0,
                    label: None,
                },
            ],
            assign: HashMap::from([("CV I/O".to_string(), 1)]),
            ..RackSpec::default_case(&chain)
        };
        let rack = RackLayout::pack(&chain, &spec);
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
        // × rows_per_mm (the mapping is uniform in mm, D4).
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
        assert_close(sy1 - sy0, (y1_abs - y0_abs) * m.rows_per_mm);
        assert_close(
            sy1 - (sy0 + abs0.h_mm * m.rows_per_mm),
            FOLD_BAR_HEIGHT_MM * m.rows_per_mm,
        );
    }

    #[test]
    fn chain_order_matches_declaration() {
        let p = patch("[p2b8]\n[copy]\n    input = I1\n    output = O1\n");
        let layout = PhysicalLayout::build(&p);

        assert_eq!(layout.modules.len(), 2);
        let first = &layout.modules[0];
        assert_eq!(first.controller, "P2B8");
        assert_eq!(first.module_instance, Some(1));
        assert_eq!(first.geometry_key, "p2b8");
        assert!(!first.is_fallback);
        assert_eq!(first.he, 3);
        assert_close(first.rect_mm.x_mm, 0.0);
        assert_close(first.rect_mm.w_mm, 25.4); // p2b8 = 5 HP
        assert_close(first.rect_mm.h_mm, 128.5); // 3 HE

        let second = &layout.modules[1];
        assert_eq!(second.controller, "CV I/O");
        assert_eq!(second.module_instance, None);
        assert_eq!(second.geometry_key, "master");
        assert_close(second.rect_mm.x_mm, 25.9); // 25.4 + 0.5 gap
        assert_close(second.rect_mm.w_mm, 40.64); // master = 8 HP

        assert_close(layout.total_width_mm, 66.54);
        assert_close(layout.total_height_mm, 128.5);
        assert_close(layout.chain_gaps_mm.inter_module, 0.5);
    }

    #[test]
    fn repeated_p2b8_instances_yield_separate_faceplates() {
        let p = patch("[p2b8]\n[p2b8]\n");
        let layout = PhysicalLayout::build(&p);

        assert_eq!(layout.modules.len(), 2);
        assert_eq!(layout.modules[0].controller, "P2B8");
        assert_eq!(layout.modules[0].module_instance, Some(1));
        assert_eq!(layout.modules[1].controller, "P2B8");
        assert_eq!(layout.modules[1].module_instance, Some(2));
        assert_eq!(layout.modules[0].geometry_key, "p2b8");
        assert_eq!(layout.modules[1].geometry_key, "p2b8");
        // Both faceplates sit at the real p2b8 width (5 HP = 25.4 mm).
        assert_close(layout.modules[0].rect_mm.w_mm, 25.4);
        assert_close(layout.modules[1].rect_mm.w_mm, 25.4);
        assert_eq!(layout.modules[0].width_hp, 5.0);
        assert_eq!(layout.modules[1].width_hp, 5.0);
        assert_eq!(layout.modules[0].key(), "P2B8 1");
        assert_eq!(layout.modules[1].key(), "P2B8 2");
        assert_close(layout.modules[1].rect_mm.x_mm, 25.9);
        assert_close(layout.total_width_mm, 51.3);

        // Determinism: an identical patch yields an identical chain.
        let again = PhysicalLayout::build(&p);
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
        let layout = PhysicalLayout::build(&p);
        let s1 = layout.cell_for(0, "P1.1").expect("P1.1 slider cell");
        assert_eq!(s1.label, "P1.1");
        assert_eq!(s1.element, Some(1));
        assert_eq!(s1.family, "F", "slider cell lives in the F family");
        let s8 = layout.cell_for(0, "P1.8").expect("P1.8 slider cell");
        assert_eq!(s8.label, "P1.8");
        assert!(layout.cell_for(0, "P1.9").is_none(), "8 sliders only");

        let p = patch("[m4]\n");
        let layout = PhysicalLayout::build(&p);
        let f1 = layout.cell_for(0, "P1.2").expect("M4 fader cell");
        assert_eq!(f1.family, "F");
        assert_eq!(f1.element, Some(2));
    }

    #[test]
    fn bare_p8s8_synthesizes_switches_and_leds() {
        // droid_tui-2b4: a bare [p8s8] declares 8 sliders (P), 8 slider LEDs
        // (L) and 8 switches (S) — all resolvable against the p8s8 geometry.
        let p = patch("[p8s8]\n");
        let layout = PhysicalLayout::build(&p);
        assert_eq!(layout.modules.len(), 1);
        assert_eq!(layout.modules[0].controller, "P8S8");
        assert_eq!(layout.modules[0].module_instance, Some(1));
        for i in 1..=8u32 {
            assert!(
                layout.cell_for(0, &format!("P1.{i}")).is_some(),
                "P1.{i} slider"
            );
            assert!(layout.cell_for(0, &format!("L1.{i}")).is_some(), "L1.{i}");
            assert!(layout.cell_for(0, &format!("S1.{i}")).is_some(), "S1.{i}");
        }
    }

    #[test]
    fn cell_lookup_resolves_per_token_family() {
        let p = patch("[p2b8]\n");
        let layout = PhysicalLayout::build(&p);

        let b3 = layout.cell_for(0, "B1.3").expect("B1.3 cell");
        assert_eq!(b3.label, "B1.3");
        assert_eq!(b3.element, Some(3));
        assert_close(b3.rect_mm.x_mm, 3.5);
        assert_close(b3.rect_mm.w_mm, 8.0);

        let p2 = layout.cell_for(0, "P1.2").expect("P1.2 cell");
        assert_eq!(p2.label, "P1.2");
        assert_eq!(p2.family, "P");
        assert_eq!(p2.element, Some(2));

        let l5 = layout.cell_for(0, "L1.5").expect("L1.5 cell");
        assert_eq!(l5.label, "L1.5");
        assert_eq!(l5.family, "L");
        assert_eq!(l5.element, Some(5));

        assert!(
            layout.cell_for(0, "B1.9").is_none(),
            "element beyond family range"
        );
        assert!(
            layout.cell_for(0, "S1.1").is_none(),
            "p2b8 has no switch cells"
        );
    }

    #[test]
    fn cv_tokens_resolve_to_directional_master_cells() {
        let p = patch("[copy]\n    input = I1\n    output = O4\n");
        let layout = PhysicalLayout::build(&p);

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

    #[test]
    fn deep_cv_patch_uses_master18_geometry() {
        let p = patch("[copy]\n    input = I9\n    output = O1\n");
        let layout = PhysicalLayout::build(&p);
        assert_eq!(layout.modules[0].geometry_key, "master18");
    }

    #[test]
    fn unknown_controller_falls_back_to_default_width() {
        let p = patch("[copy]\n    button = B5.1\n");
        let layout = PhysicalLayout::build(&p);

        let m = &layout.modules[0];
        assert!(m.is_fallback);
        assert_eq!(m.geometry_key, "");
        assert_eq!(m.controller, "Controller 5");
        assert_close(m.rect_mm.w_mm, 25.4);
        assert_close(m.width_hp, 5.0); // the documented 5 HP fallback
        assert_close(m.rect_mm.h_mm, 128.5);
        assert!(
            layout.cell_for(0, "B5.1").is_none(),
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
        assert_close(fb.chain_gap_mm(), 0.5);
        // build() over the fallback source never panics for unknown controllers.
        let p = patch("[copy]\n    button = B7.1\n");
        let layout = PhysicalLayout::build(&p);
        assert!(layout.modules[0].is_fallback);
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
        let layout = PhysicalLayout::build(&p);
        assert_eq!(layout.modules[0].geometry_key, "b32");
        let last = layout.cell_for(0, "B1.32").expect("B1.32 on the 4x8 grid");
        assert_eq!(last.label, "B1.32");
        assert!(
            layout.cell_for(0, "B1.33").is_none(),
            "33rd button is out of the 4x8 grid"
        );
    }

    // ---- rack model (task 2.3) ----

    /// p2b8 (5 HP, 25.4 mm) + CV I/O master (8 HP, 40.64 mm).
    fn two_module_chain() -> PhysicalLayout {
        PhysicalLayout::build(&patch("[p2b8]\n[copy]\n    input = I1\n    output = O1\n"))
    }

    fn spec(rows: &[(u32, f64)], assign: &[(&str, usize)]) -> RackSpec {
        RackSpec {
            rows: rows
                .iter()
                .map(|&(he, hp)| RackRow {
                    he,
                    hp,
                    label: None,
                })
                .collect(),
            top_mount_te: 0.0,
            side_mount_te: 0.0,
            assign: assign.iter().map(|&(k, r)| (String::from(k), r)).collect(),
        }
    }

    #[test]
    fn module_key_format_is_controller_plus_instance() {
        let chain = two_module_chain();
        assert_eq!(chain.modules[0].key(), "P2B8 1");
        assert_eq!(chain.modules[1].key(), "CV I/O");
    }

    #[test]
    fn auto_pack_fills_row_0_then_overflows_to_row_1() {
        let chain = two_module_chain();
        let rack = RackLayout::pack(&chain, &spec(&[(3, 10.0), (3, 10.0)], &[]));
        assert_eq!(rack.rows.len(), 2);
        assert_eq!(rack.rows[0].modules.len(), 1);
        assert_eq!(rack.rows[0].modules[0].key, "P2B8 1");
        assert_close(rack.rows[0].modules[0].rect_mm.x_mm, 0.0);
        assert!(!rack.rows[0].modules[0].overridden);
        assert_eq!(rack.rows[1].modules.len(), 1);
        assert_eq!(rack.rows[1].modules[0].key, "CV I/O");
        assert_close(rack.rows[1].modules[0].rect_mm.x_mm, 0.0);
        assert_close(rack.rows[0].fill_width_mm, 25.4);
        assert_close(rack.rows[1].fill_width_mm, 40.64);
    }

    #[test]
    fn default_single_row_holds_whole_chain() {
        let chain = two_module_chain();
        let rack = RackLayout::pack(&chain, &RackSpec::default());
        assert_eq!(rack.rows.len(), 1);
        let row = &rack.rows[0];
        assert_eq!(row.he, 3);
        assert_close(row.hp, 14.0); // ceil(66.54 / 5.08)
        assert_eq!(row.modules.len(), 2);
        assert_eq!(row.modules[0].key, "P2B8 1");
        assert_eq!(row.modules[1].key, "CV I/O");
        assert_close(row.modules[1].rect_mm.x_mm, 25.9);
        assert_close(row.fill_width_mm, 66.54);
        assert_close(rack.total_width_mm, 71.12); // 14 HP case
        assert!(rack.fold_bars.is_empty());
    }

    #[test]
    fn override_places_module_into_row_regardless_of_fit() {
        let chain = two_module_chain();
        let rack = RackLayout::pack(&chain, &spec(&[(3, 10.0), (3, 10.0)], &[("P2B8 1", 1)]));
        // p2b8 forced to row 1; master auto-packs row 0 (cursor unaffected).
        assert_eq!(rack.rows[0].modules[0].key, "CV I/O");
        assert_eq!(rack.rows[1].modules[0].key, "P2B8 1");
        assert!(rack.rows[1].modules[0].overridden);
        assert!(!rack.rows[0].modules[0].overridden);
    }

    #[test]
    fn out_of_range_override_falls_back_to_auto_pack() {
        let chain = two_module_chain();
        let rack = RackLayout::pack(&chain, &spec(&[(3, 10.0), (3, 10.0)], &[("P2B8 1", 5)]));
        assert_eq!(rack.rows[0].modules.len(), 1);
        assert_eq!(rack.rows[0].modules[0].key, "P2B8 1");
        assert!(!rack.rows[0].modules[0].overridden);
        assert_eq!(rack.rows[1].modules[0].key, "CV I/O");
    }

    #[test]
    fn fold_lines_at_row_boundaries() {
        let chain = two_module_chain();
        let rack = RackLayout::pack(&chain, &spec(&[(3, 5.0), (3, 5.0), (3, 5.0)], &[]));
        // p2b8 fits row 0 exactly (25.4 == 25.4); master (40.64) overflows to row 2.
        assert_eq!(rack.rows[0].modules[0].key, "P2B8 1");
        assert!(rack.rows[1].modules.is_empty());
        assert_eq!(rack.rows[2].modules[0].key, "CV I/O");
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
        let chain = two_module_chain();
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
        let chain = two_module_chain();
        let s = spec(&[(3, 10.0), (3, 10.0)], &[("P2B8 1", 1)]);
        let a = RackLayout::pack(&chain, &s);
        let b = RackLayout::pack(&chain, &s);
        assert_eq!(a, b);
    }
}
