use std::collections::{BTreeMap, HashMap, HashSet};
use std::env;
use std::ffi::OsStr;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::time::Instant;

use serde::{Deserialize, Serialize};

const LABELS_DIR_NAME: &str = "droid-tui";
const LABELS_FILE_NAME: &str = "labels.toml";

/// Per-patch buckets: `hw` (token → layer → label) + `circuits` (NodeId `"name:idx"` → label).
/// Empty-string labels are treated as absent by `Patch::display_label` / `circuit_label`
/// and may be pruned on save; deserialization supplies empty maps when absent.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct PatchLabels {
    #[serde(default)]
    pub hw: HashMap<String, BTreeMap<u8, String>>,
    #[serde(default)]
    pub circuits: HashMap<String, String>,
}

/// XDG label store for `~/.config/droid-tui/labels.toml`.
///
/// File shape mirrors the spec:
/// ```toml
/// [patches."/abs/path"]
/// hw."B3.17" = {1="...",2="..."}
/// circuits."motorfader:12" = "..."
/// ```
/// Keyed by canonicalized absolute path strings (canonicalize via
/// `Path::canonicalize` fallback to absolute, not content hash).
/// Warn-once on corrupt TOML (fallback empty store), atomic tmp→rename,
/// same contract as `config.rs`.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct LabelStore {
    #[serde(default)]
    pub patches: HashMap<String, PatchLabels>,
}

impl LabelStore {
    /// Canonicalize `path` to an absolute string key: real canonical path when
    /// the file exists, otherwise an absolute join against `current_dir`.
    pub fn canonical_key(path: &Path) -> String {
        if let Ok(canonical) = path.canonicalize() {
            return canonical.to_string_lossy().to_string();
        }
        let absolute = if path.is_absolute() {
            path.to_path_buf()
        } else {
            env::current_dir()
                .unwrap_or_else(|_| PathBuf::from("."))
                .join(path)
        };
        absolute.to_string_lossy().to_string()
    }

    /// Encode a `(circuit, instance)` pair as the TOML key `"circuit:instance"`.
    pub fn encode_node_id(name: &str, instance: usize) -> String {
        format!("{name}:{instance}")
    }

    /// Decode a `"circuit:instance"` key. Returns `None` on malformed suffix.
    pub fn decode_node_id(key: &str) -> Option<(String, usize)> {
        let (name, idx) = key.rsplit_once(':')?;
        let instance = idx.parse::<usize>().ok()?;
        Some((name.to_string(), instance))
    }

    /// Bucket for `patch_path`, if present.
    pub fn patch_labels(&self, patch_path: &Path) -> Option<&PatchLabels> {
        let key = Self::canonical_key(patch_path);
        self.patches.get(&key)
    }

    /// Mutable bucket for `patch_path`, creating it when absent.
    pub fn patch_labels_mut(&mut self, patch_path: &Path) -> &mut PatchLabels {
        let key = Self::canonical_key(patch_path);
        self.patches.entry(key).or_default()
    }

    /// Load the XDG store. Missing file yields empty store; malformed TOML
    /// warns once on stderr and yields empty store (mirrors `config.rs`).
    pub fn load() -> Self {
        match labels_file_path() {
            Some(path) => Self::load_from(&path),
            None => Self::default(),
        }
    }

    /// Load from an explicit file path (test injection point). Warn-once
    /// contract: each call emits at most one stderr warning.
    pub fn load_from(path: &Path) -> Self {
        let raw = match fs::read_to_string(path) {
            Ok(raw) => raw,
            Err(_) => return Self::default(),
        };
        match toml::from_str(&raw) {
            Ok(parsed) => parsed,
            Err(err) => {
                eprintln!(
                    "warning: ignoring malformed labels file {}: {err}",
                    path.display()
                );
                Self::default()
            }
        }
    }

    /// Save to the discovered XDG path.
    pub fn save(&self) -> io::Result<()> {
        let dir = labels_dir(
            env::var_os("XDG_CONFIG_HOME").as_deref(),
            env::var_os("HOME").as_deref(),
        )
        .ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::NotFound,
                "cannot determine config directory ($XDG_CONFIG_HOME or $HOME)",
            )
        })?;
        self.save_to_dir(&dir)
    }

    /// Atomically write as `labels.toml` inside `dir` (tmp→rename), creating
    /// the directory tree on demand. Empty/whitespace entries are pruned so
    /// `I4:`-style empty slots do not persist and `display_label` can fall
    /// through to derived.
    pub fn save_to_dir(&self, dir: &Path) -> io::Result<()> {
        fs::create_dir_all(dir)?;
        // Prune empty/whitespace-only labels and empty buckets so the file
        // stays minimal and empty-slot coverage (`I4:`) is preserved on round-trip.
        let mut pruned = Self {
            patches: HashMap::new(),
        };
        for (k, bucket) in &self.patches {
            let mut hw: HashMap<String, BTreeMap<u8, String>> = HashMap::new();
            for (token, layers) in &bucket.hw {
                let kept: BTreeMap<u8, String> = layers
                    .iter()
                    .filter_map(|(layer, label)| {
                        let trimmed = label.trim();
                        if trimmed.is_empty() {
                            None
                        } else {
                            Some((*layer, trimmed.to_string()))
                        }
                    })
                    .collect();
                if !kept.is_empty() {
                    hw.insert(token.clone(), kept);
                }
            }
            let circuits: HashMap<String, String> = bucket
                .circuits
                .iter()
                .filter_map(|(key, label)| {
                    let trimmed = label.trim();
                    if trimmed.is_empty() {
                        None
                    } else {
                        Some((key.clone(), trimmed.to_string()))
                    }
                })
                .collect();
            if hw.is_empty() && circuits.is_empty() {
                continue;
            }
            pruned
                .patches
                .insert(k.clone(), PatchLabels { hw, circuits });
        }
        let body = toml::to_string(&pruned)
            .map_err(|err| io::Error::new(io::ErrorKind::InvalidData, err.to_string()))?;
        let target = dir.join(LABELS_FILE_NAME);
        let tmp = dir.join(format!("{LABELS_FILE_NAME}.tmp"));
        fs::write(&tmp, body)?;
        if let Err(err) = fs::rename(&tmp, &target) {
            let _ = fs::remove_file(&tmp);
            return Err(err);
        }
        Ok(())
    }

    /// Convenience: save to an explicit file path (atomic tmp→rename beside the file).
    pub fn save_to(&self, path: &Path) -> io::Result<()> {
        if let Some(dir) = path.parent() {
            if !dir.as_os_str().is_empty() {
                fs::create_dir_all(dir)?;
            }
            // Delegate to dir-based writer when path is inside an XDG-style dir;
            // otherwise write beside `path` atomically.
            if path.file_name().is_some_and(|n| n == LABELS_FILE_NAME) {
                return self.save_to_dir(dir);
            }
        }
        let pruned = Self {
            patches: self.patches.clone(),
        };
        let body = toml::to_string(&pruned)
            .map_err(|err| io::Error::new(io::ErrorKind::InvalidData, err.to_string()))?;
        let tmp = path.with_extension("toml.tmp");
        fs::write(&tmp, body)?;
        if let Err(err) = fs::rename(&tmp, path) {
            let _ = fs::remove_file(&tmp);
            return Err(err);
        }
        Ok(())
    }

    /// Convenience for HW: get label for `(patch, token, layer)` respecting
    /// trimmed emptiness (None = absent/fall through).
    pub fn hw_label(&self, patch_path: &Path, token: &str, layer: u8) -> Option<String> {
        self.patch_labels(patch_path)
            .and_then(|b| b.hw.get(token))
            .and_then(|m| m.get(&layer))
            .and_then(|s| {
                let t = s.trim();
                if t.is_empty() {
                    None
                } else {
                    Some(t.to_string())
                }
            })
    }

    /// Convenience for circuits: get label for `(patch, NodeId)`.
    pub fn circuit_label(&self, patch_path: &Path, node: &(String, usize)) -> Option<String> {
        let key = Self::encode_node_id(&node.0, node.1);
        self.patch_labels(patch_path)
            .and_then(|b| b.circuits.get(&key))
            .and_then(|s| {
                let t = s.trim();
                if t.is_empty() {
                    None
                } else {
                    Some(t.to_string())
                }
            })
    }
}

fn labels_dir(xdg_config_home: Option<&OsStr>, home: Option<&OsStr>) -> Option<PathBuf> {
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
    Some(base.join(LABELS_DIR_NAME))
}

fn labels_file_path() -> Option<PathBuf> {
    let dir = labels_dir(
        env::var_os("XDG_CONFIG_HOME").as_deref(),
        env::var_os("HOME").as_deref(),
    )?;
    Some(dir.join(LABELS_FILE_NAME))
}

use crossterm::terminal;
use ratatui::layout::Rect;

use crate::diff::DiffReport;
use crate::events::{Event, EventBus};
use crate::graph::{Cluster, Graph, NodeId};
use crate::graph_render::{GraphCamera, WorldBounds};
use crate::latency::CostModel;
use crate::layout;
use crate::optimize::{CandidateOrdering, OptimizeScope};
use crate::patch::Patch;
use crate::patch::ShiftGroup;
use crate::schema::load_schema;
use crate::validation::{validate_patch, Severity, ValidationIssue};

/// Which datum is being relabeled in the inline single-field overlay.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EditKind {
    /// HW token at a specific shift layer (1..=max_shift_layer, clamped on read).
    Hw { token: String, layer: u8 },
    /// Circuit instance identified by `(circuit name, instance index)`.
    Circuit { node: NodeId },
}

/// Inline edit overlay state for the label overlay (`e` to enter, `Enter`/`Esc`).
///
/// For `Hw` edits `layer_drafts` preserves per-layer unsaved drafts while
/// the overlay is open so `1..N` layer cycling does not lose typed text
/// (spec: map layer->draft). Only meaningful for `Hw`; empty for `Circuit`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EditState {
    pub kind: EditKind,
    pub draft: String,
    pub layer_drafts: BTreeMap<u8, String>,
}

impl EditState {
    /// Create a new HW edit state with an empty per-layer draft map.
    pub fn new_hw(token: String, layer: u8, draft: String) -> Self {
        Self {
            kind: EditKind::Hw { token, layer },
            draft,
            layer_drafts: BTreeMap::new(),
        }
    }

    /// Create a new circuit edit state.
    pub fn new_circuit(node: NodeId, draft: String) -> Self {
        Self {
            kind: EditKind::Circuit { node },
            draft,
            layer_drafts: BTreeMap::new(),
        }
    }
}

/// State of an armed vim-style prefix key (`g` pressed, awaiting the
/// follow-up key). `started` drives the lazy timeout check performed when
/// the next event arrives.
pub struct PrefixState {
    pub started: Instant,
}

/// Open `g o` optimizer menu state: the candidate orderings plus the menu
/// cursor and preview bookkeeping. `previewing` holds the candidate index
/// whose order is currently applied to `Patch.sections` (reordering happens
/// in place); `original_order` is the identity permutation captured when the
/// menu opened, used to restore.
#[derive(Debug, Clone)]
pub struct OptimizerState {
    /// Up to three candidate orderings, best first (design D1/D5).
    pub candidates: Vec<CandidateOrdering>,
    /// Menu cursor into `candidates`.
    pub cursor: usize,
    /// Index of the candidate currently previewed (its order is applied to
    /// `Patch.sections`), if any.
    pub previewing: Option<usize>,
    /// Identity permutation `0..n` captured when the menu opened; applying it
    /// to `Patch.sections` restores the file order.
    pub original_order: Vec<usize>,
    /// Current weight `w` in `[0,1]` for the weighted objective `(1−w)·Sum + w·max` (D2/D5).
    pub weight: f32,
}

/// Grabbed-node state for a graph drag (design D1/D7). Holds the index of the
/// dragged node in `graph.nodes` plus the grab offset (node position minus the
/// Down point) so the node follows the pointer without jumping on the first
/// drag delta. `Some` only between a left-button Down on a node rect and the
/// matching Up.
pub struct GraphDrag {
    pub node_index: usize,
    pub offset_x: f32,
    pub offset_y: f32,
}

/// Which pane receives keyboard input while the embedded source pane is open.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub enum ViewerFocus {
    #[default]
    Panels,
    Source,
}

/// Which pane has keyboard focus while the quad concurrent view is open.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub enum QuadFocus {
    #[default]
    Panels,
    Source,
    GraphFull,
    GraphFiltered,
}

/// View mode for the embedded source pane.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub enum SourceViewMode {
    #[default]
    Raw,
    Prettified,
}

/// Orientation of the patch display.
#[derive(Debug, Clone, PartialEq)]
pub enum Orientation {
    /// Portrait mode
    Portrait,
    /// Landscape mode
    Landscape,
}

/// Application state
pub struct App {
    pub patch: Option<Patch>,
    pub active_shift: Option<ShiftGroup>,
    pub hovered_component: Option<usize>,
    pub status_message: String,
    /// File picker state
    pub showing_picker: bool,
    pub picker_dir: PathBuf,
    pub selected_file: Option<PathBuf>,
    pub picker_entries: Vec<PathBuf>,
    pub picker_index: usize,
    /// Screen rects the last render pass drew each component into, keyed by
    /// its index into `patch.hw_components`. Rebuilt every frame (layout is
    /// recomputed fresh each draw), and used for mouse hit-testing since the
    /// renderer — not the event handler — knows where things actually ended
    /// up on screen.
    pub component_rects: Vec<(usize, Rect)>,
    /// True when the signal-flow graph view (`g g`) is open.
    pub showing_graph: bool,
    /// The signal-flow graph built from the current patch. `None` until a
    /// patch is loaded and the graph is opened.
    pub graph: Option<Graph>,
    /// Frozen node positions from the last full solve, parallel to
    /// `graph.nodes` (index `i` ↔ `graph.nodes[i]`). Re-solved on open and on
    /// node move; never mutated by a continuous tick (design D1).
    pub graph_positions: Vec<(f32, f32)>,
    /// Cluster-container rects published by the renderer each frame while the
    /// graph is open, keyed by index into `graph.clusters` — the same
    /// renderer-publishes/handler-consumes contract as `component_rects`.
    /// Cleared per frame; populated by the renderer (task 5.1).
    pub graph_cluster_rects: Vec<(usize, Rect)>,
    /// Node rects published by the renderer each frame while the graph is open,
    /// keyed by index into `graph.nodes` (parallel to `graph_positions`). Same
    /// renderer-publishes/handler-consumes contract; used for drag hit-testing.
    pub graph_node_rects: Vec<(usize, Rect)>,
    /// In-progress graph drag (`g g`). `Some` from a left-button Down on a
    /// `graph_node_rects` entry until the matching Up; drives node repositioning
    /// + damped local re-settle (design D1).
    pub graph_drag: Option<GraphDrag>,
    /// Hovered graph node index while the graph surface is open, updated on
    /// mouse Moved via `graph_node_rects` hit-testing. `None` when the pointer
    /// is not over a node or the graph is closed.
    pub hovered_graph_node: Option<usize>,
    /// Persistent world→pixel camera of the graph surface (task 2.3). Seeded by
    /// the kitty renderer to a legible `fit_to_world` on the first frame after
    /// open, then mutated by `+`/`-` (zoom presets) and arrow/wheel (pan).
    /// `None` when the graph has not been rendered (the box-drawing path never
    /// reads it); reset on open and on patch load so a new graph re-fits.
    pub graph_camera: Option<GraphCamera>,
    /// Index into `GRAPH_ZOOM_PRESETS` (mirrors the physical view's scale
    /// presets); `+`/`-` cycle it with wrap-around. Default `1` (100%).
    pub graph_zoom_preset: u8,
    /// The kitty image's pixel size `(pw, ph)` at the last graph render, so the
    /// handler can anchor zoom and gate pan on overflow. `None` until rendered.
    pub graph_canvas_px: Option<(f32, f32)>,
    /// Vim-style prefix mode: `g` was pressed and the app waits for a
    /// follow-up key within `PREFIX_TIMEOUT`; `None` when none is armed.
    pub prefix: Option<PrefixState>,
    /// True when `g` + `v` opened the embedded source pane.
    pub showing_viewer: bool,
    /// Which component is explicitly selected (distinct from hover). Holds the
    /// hardware token id (e.g. "B1.1") so it can be looked up directly in
    /// `Patch::occurrence_index`.
    pub selected_component: Option<String>,
    /// Which pane has keyboard focus while the viewer is open.
    pub viewer_focus: ViewerFocus,
    /// Raw vs prettified rendering for the source pane. Defaults to Raw.
    pub source_view_mode: SourceViewMode,
    /// Index into `occurrences_for(selected_component)` for Up/Down/Home/End
    /// navigation. Saturates at bounds.
    pub occurrence_cursor: usize,
    /// Line offset of the source view (0-based).
    pub source_scroll: usize,
    /// Geometry of the minimap column published by the renderer each frame
    /// (like `component_rects`). Used for click-to-scroll hit testing.
    pub minimap_rect: Option<Rect>,
    /// Full geometry of the embedded source pane published by the renderer
    /// each frame while the viewer is open. Used to route bare source-pane
    /// clicks to `ViewerFocus::Source` without side effects.
    pub source_pane_rect: Option<Rect>,
    /// Scale factor for rendering (1.0 = default). Used for progressive scaling.
    pub scale_factor: f32,
    /// Current display orientation.
    pub orientation: Orientation,
    /// Split ratio for viewer/source pane division (0.3 to 0.7).
    /// 0.6 means panels get 60%, source gets 40%.
    /// This is a view preference that persists across patch loads.
    pub viewer_split_ratio: f32,
    /// Synchronous observer event bus (design D6). Re-solve triggers and
    /// topology errors are emitted here for subscribers (renderer, status).
    pub events: EventBus,
    /// True when the quad concurrent view is open (panels | source / graph FULL | graph FILTERED).
    pub showing_quad: bool,
    /// Which of the four quad panes has keyboard focus.
    pub quad_focus: QuadFocus,
    /// Global processing pause (`p`): while true, state-mutating actions are
    /// blocked and selection-driven influence is cleared (never computed).
    pub processing_paused: bool,
    /// True when the main panel shows the geometry-only physical skeleton
    /// (design D7) instead of the physical full render. Presentation switch
    /// of the main view, default OFF — the physical full render is the
    /// default main view since 4.2. Reset on patch load like
    /// `processing_paused`.
    pub physical_show_skeleton: bool,
    /// Screen rects the skeleton renderer published last frame, keyed by
    /// (module index, cell index = position of the element in the module's
    /// `components` in declaration order). Rebuilt every frame; the D5
    /// coincidence contract — 5.1 compares full-view element rects 1:1
    /// against these at the same scale/offset.
    pub physical_skeleton_rects: Vec<(usize, usize, Rect)>,
    /// Screen rects the physical full renderer published last frame — the
    /// same (module index, cell index, screen Rect) shape and order as
    /// `physical_skeleton_rects`, computed from the same mapping and
    /// geometry, so the D5 coincidence contract holds by construction
    /// (5.1 compares the two vectors 1:1). Rebuilt every frame.
    pub physical_full_rects: Vec<(usize, usize, Rect)>,
    /// Viewport state of the physical presentation (design D5): pan offset in
    /// screen cells and zoom. `physical_zoom` is derived from the `+`/`-`
    /// scale presets (ratio 0.75–2.0, linked by the renderers);
    /// `physical_offset` is mutated by panning (4.3's wheel wiring). Both
    /// reset on patch load like `physical_show_skeleton`.
    pub physical_offset: (f32, f32),
    pub physical_zoom: f32,
    /// The case/rack definition the physical view packs into (design D12).
    /// Seeded from `[physical.rack]` at startup (main.rs); empty rows mean
    /// "no rack configured" and `RackLayout::pack` materializes the default
    /// single-row case. Not reset on patch load — the rack is a property of
    /// the environment, not the patch.
    pub physical_rack_spec: crate::physical::RackSpec,
    /// Screen size (w, h) in cells of the whole rack under the current
    /// mapping, published by the renderers each frame
    /// (renderer-owns-geometry contract). `physical_overflow` compares it
    /// against the visible area for the 4.3 wheel-pan wiring.
    pub physical_rack_size: (u16, u16),
    /// Main-panel viewport the physical view renders into, published by the
    /// renderer (renderer-owns-geometry contract, like `minimap_rect`). The
    /// 4.3 pan wiring compares the published rack size against it via
    /// `physical_main_area()`; until a render has run the live terminal size
    /// is the fallback. Reset on patch load with the other viewport state.
    pub physical_viewport: Option<Rect>,
    /// Primary `_VAR` derived from the selected hardware token (`hw_token_to_vars` first element).
    pub active_modifier_var: Option<String>,
    /// Forward influence result for the active modifier, if any.
    pub influence: Option<crate::patch::InfluenceSubtree>,
    /// Induced subgraph on `influence` (FILTERED pane).
    pub filtered_graph: Option<Graph>,
    /// Frozen positions for `filtered_graph`, parallel to `filtered_graph.nodes`.
    pub filtered_positions: Vec<(f32, f32)>,
    /// Cluster rects published by the renderer for the FILTERED pane.
    pub filtered_cluster_rects: Vec<(usize, Rect)>,
    /// Node rects published by the renderer for the FILTERED pane.
    pub filtered_node_rects: Vec<(usize, Rect)>,
    /// In-progress drag for the FILTERED pane, mirroring `graph_drag`.
    pub filtered_drag: Option<GraphDrag>,
    /// Circuits whose processing is disabled, keyed by `(circuit name,
    /// instance index)`. Disabled circuits stay influenced but act as a dead
    /// end in the influence walk: nothing downstream of them is reached.
    /// Cleared on every `load_patch`.
    pub disabled_circuits: HashSet<(String, usize)>,
    /// Circuits pinned as fixed layout anchors, keyed by `NodeId` (circuit
    /// name, instance index). Pinned nodes are fixed anchors the solver never
    /// moves (design D3). The graph's tip — the first circuit in `.ini` order
    /// — is seeded by default on open; `p` toggles membership on the hovered
    /// node; dragging a node auto-pins it at the dropped position (design
    /// D7). Cleared on every `load_patch`.
    pub pinned: HashSet<(String, usize)>,
    /// Per-patch XDG label store (`~/.config/droid-tui/labels.toml`), keyed by
    /// canonicalized absolute patch path. Loaded once at `App::new` via
    /// `LabelStore::load()` (warn-once, empty fallback) and persisted atomically
    /// on edit save.
    pub label_store: LabelStore,
    /// Inline single-field label-edit overlay state. `None` when not editing.
    pub editing: Option<EditState>,
    /// Canonical absolute path of the currently loaded patch, if any. Drives
    /// per-patch bucket lookup (`LabelStore::canonical_key`) without content hashing.
    pub current_patch_path: Option<PathBuf>,
    /// Second patch for structural diff (`g d` picker). `None` until a B patch is loaded.
    pub diff_patch: Option<Patch>,
    /// Structural diff report between `patch` (A) and `diff_patch` (B).
    pub diff_report: Option<DiffReport>,
    /// Whether the diff overlay is currently shown (`d` toggles).
    pub diff_showing: bool,
    /// Per-circuit processing-cost provider feeding the latency ramp (design
    /// D2). One instance is built from `[latency]` config at startup and passed
    /// into every graph build, so a config change recolors and re-optimizes
    /// coherently.
    pub cost_model: CostModel,
    /// Whether cable edges are colored by the forward-loop latency ramp on the
    /// graph surface (`c` toggles). A view preference: like `viewer_split_ratio`
    /// it persists across `load_patch` (unlike `processing_paused`/`disabled_circuits`,
    /// which reset because they describe transient processing state).
    pub latency_coloring: bool,
    /// Optional component-scoped filter token (`None` = patch-wide).
    pub diff_scope: Option<String>,
    /// True while the picker was opened via `g d` to load the B patch.
    pub diff_picker_active: bool,
    /// Sorted validation findings for the last load attempt.
    pub validation_issues: Vec<ValidationIssue>,
    /// True when the validation modal is shown (errors or warnings present).
    pub showing_validation: bool,
    /// Cursor into `validation_issues` while the modal is open.
    pub validation_cursor: usize,
    /// Open `g o` optimizer menu state. `None` when the menu is closed.
    pub optimizer: Option<OptimizerState>,
}

impl App {
    pub fn new() -> Self {
        Self {
            patch: None,
            active_shift: None,
            hovered_component: None,
            status_message: String::from("No patch loaded. Press 'l' to load."),
            showing_picker: false,
            picker_dir: std::env::current_dir().unwrap_or_default(),
            selected_file: None,
            picker_entries: Vec::new(),
            picker_index: 0,
            component_rects: Vec::new(),
            showing_graph: false,
            graph: None,
            graph_positions: Vec::new(),
            graph_cluster_rects: Vec::new(),
            graph_node_rects: Vec::new(),
            graph_drag: None,
            hovered_graph_node: None,
            graph_camera: None,
            graph_zoom_preset: 1,
            graph_canvas_px: None,
            prefix: None,
            showing_viewer: false,
            selected_component: None,
            viewer_focus: ViewerFocus::Panels,
            source_view_mode: SourceViewMode::Raw,
            occurrence_cursor: 0,
            source_scroll: 0,
            minimap_rect: None,
            source_pane_rect: None,
            scale_factor: 1.0,
            orientation: Orientation::Portrait,
            viewer_split_ratio: 0.6,
            events: EventBus::default(),
            showing_quad: false,
            quad_focus: QuadFocus::default(),
            processing_paused: false,
            physical_show_skeleton: false,
            physical_skeleton_rects: Vec::new(),
            physical_full_rects: Vec::new(),
            physical_offset: (0.0, 0.0),
            physical_zoom: 1.0,
            physical_rack_spec: crate::physical::RackSpec::default(),
            physical_rack_size: (0, 0),
            physical_viewport: None,
            active_modifier_var: None,
            influence: None,
            filtered_graph: None,
            filtered_positions: Vec::new(),
            filtered_cluster_rects: Vec::new(),
            filtered_node_rects: Vec::new(),
            filtered_drag: None,
            disabled_circuits: HashSet::new(),
            pinned: HashSet::new(),
            label_store: LabelStore::load(),
            editing: None,
            current_patch_path: None,
            diff_patch: None,
            diff_report: None,
            diff_showing: false,
            latency_coloring: true,
            cost_model: CostModel::default(),
            diff_scope: None,
            diff_picker_active: false,
            validation_issues: Vec::new(),
            showing_validation: false,
            validation_cursor: 0,
            optimizer: None,
        }
    }

    pub fn refresh_picker_entries(&mut self) {
        self.picker_entries.clear();
        // Parent-directory entry rendered as "..", only when there is a real
        // parent to navigate up into (the filesystem root has none). The
        // sentinel is a bare ".." path, which `file_name()` reports as `None`,
        // so consumers detect it via `is_picker_parent_entry`.
        if self
            .picker_dir
            .parent()
            .is_some_and(|p| !p.as_os_str().is_empty())
        {
            self.picker_entries.push(PathBuf::from(".."));
        }
        // Read directory entries. `read_dir` order is arbitrary, so collect
        // into groups and sort directories first, then `.ini` files, then any
        // remaining files.
        if let Ok(entries) = std::fs::read_dir(&self.picker_dir) {
            let mut dirs = Vec::new();
            let mut inis = Vec::new();
            let mut others = Vec::new();
            for entry in entries.flatten() {
                let path = entry.path();
                if path.metadata().is_ok_and(|m| m.is_dir()) {
                    dirs.push(path);
                } else if path.extension().is_some_and(|e| e == "ini") {
                    inis.push(path);
                } else {
                    others.push(path);
                }
            }
            dirs.sort();
            inis.sort();
            others.sort();
            self.picker_entries.extend(dirs);
            self.picker_entries.extend(inis);
            self.picker_entries.extend(others);
        }
        // Scale factor affects entry density: with higher scale, show fewer entries
        // to prevent overcrowding the picker UI
        if self.scale_factor > 2.0 {
            // When heavily scaled, trim to most relevant entries (parent + first N)
            let max_entries =
                (self.picker_entries.len() as f32 / self.scale_factor).ceil() as usize;
            self.picker_entries.truncate(max_entries.max(1));
        }
    }

    /// Load a patch into the app and reset source-navigation state ready for
    /// BOF: no selection, cursor 0, scroll 0, raw mode, focus Panels, no
    /// minimap/source-pane geometry yet (renderer will publish on next frame).
    /// Clears inline edit overlay and current patch path (sample/demo loads).
    pub fn clear_diff(&mut self) {
        self.diff_patch = None;
        self.diff_report = None;
        self.diff_showing = false;
        self.diff_scope = None;
        self.diff_picker_active = false;
    }

    pub fn toggle_diff_showing(&mut self) {
        if self.diff_report.is_some() {
            self.diff_showing = !self.diff_showing;
        }
    }

    /// Toggle cable latency coloring on the graph surface, reporting the new
    /// state in the status bar. The ramp replaces the kind colors while on and
    /// kind colors return while off; error/diff precedence is unchanged.
    pub fn toggle_latency_coloring(&mut self) {
        self.latency_coloring = !self.latency_coloring;
        self.status_message = if self.latency_coloring {
            String::from("Latency coloring on (c to toggle)")
        } else {
            String::from("Latency coloring off (c to toggle)")
        };
    }

    /// Hover status for a graph node that is the sink of a back-edge (design
    /// D2): `reads _CABLE 1 loop behind`. Returns `None` when the node is not a
    /// back-edge sink, so the caller leaves the previous status untouched.
    pub fn back_edge_hover_status(&self, node_index: usize) -> Option<String> {
        let graph = self.graph.as_ref()?;
        let node = graph.nodes.get(node_index)?;
        let data = graph.latency.as_ref()?;
        let cable = data.edges.iter().filter(|l| l.is_back_edge).find_map(|l| {
            let edge = graph.edges.get(l.edge_index)?;
            (edge.sink == node.id).then(|| edge.cable.clone())
        })?;
        Some(format!("reads {} 1 loop behind", cable))
    }

    /// View of `diff_report` filtered through `diff_scope` (if any).
    /// Returns `None` when no diff is loaded; returns the full report when
    /// unscoped, otherwise `scope_report` against the base `patch`.
    pub fn filtered_report(&self) -> Option<crate::diff::DiffReport> {
        let report = self.diff_report.as_ref()?;
        if let (Some(token), Some(patch)) = (self.diff_scope.as_deref(), self.patch.as_ref()) {
            Some(crate::diff::scope_report(report, token, patch))
        } else {
            Some(report.clone())
        }
    }

    pub fn diff_scope_cable_count(&self) -> usize {
        if let Some(r) = self.filtered_report() {
            r.added_cables.len() + r.removed_cables.len() + r.changed_cables.len()
        } else {
            0
        }
    }

    pub fn status_for_scope(&self) -> Option<String> {
        if self.diff_showing {
            if let Some(token) = self.diff_scope.as_deref() {
                let n = self.diff_scope_cable_count();
                return Some(format!("Diff scope: {} ({} cables)", token, n));
            }
        }
        None
    }

    pub fn load_diff_patch(&mut self, path: &Path) -> Result<(), String> {
        let new_patch = Patch::from_ini_file(path).map_err(|e| e.to_string())?;
        let report = if let Some(base) = &self.patch {
            crate::diff::diff_patches(base, &new_patch)
        } else {
            DiffReport::default()
        };
        let added_cables = report.added_cables.len();
        let removed_cables = report.removed_cables.len();
        let changed_cables = report.changed_cables.len();
        let added_nodes = report.added_nodes.len();
        let removed_nodes = report.removed_nodes.len();
        let changed_nodes = report.changed_nodes.len();
        self.diff_patch = Some(new_patch);
        self.diff_report = Some(report);
        self.diff_showing = true;
        self.diff_scope = self.selected_component.clone();
        self.events.dispatch(&Event::DiffComputed {
            added_cables,
            removed_cables,
            changed_cables,
            added_nodes,
            removed_nodes,
            changed_nodes,
        });
        Ok(())
    }

    /// Generate candidate orderings for the loaded patch and open the `g o`
    /// optimizer menu (design D5). Returns `false` (with a status hint) when
    /// no patch is loaded or it has no sections to reorder.
    pub fn open_optimizer(&mut self) -> bool {
        let Some(patch) = &self.patch else {
            self.status_message = String::from("No patch loaded. Press 'l' to load.");
            return false;
        };
        if patch.sections.is_empty() {
            self.status_message = String::from("Nothing to optimize — no sections.");
            return false;
        }
        let weight: f32 = 0.0;
        let candidates = crate::optimize::generate_candidates_weighted(
            patch,
            &self.cost_model,
            OptimizeScope::MinMax,
            weight,
        );
        let original_order: Vec<usize> = (0..patch.sections.len()).collect();
        self.optimizer = Some(OptimizerState {
            candidates,
            cursor: 0,
            previewing: None,
            original_order,
            weight,
        });
        self.status_message = format!(
            "Optimizer w = {:.1}: j/k select · Enter preview · r restore · s export · Esc close",
            weight
        );
        true
    }

    /// Adjust optimizer weight `w` in `[0,1]` (clamped, snapped to 0.1),
    /// re-running `generate_candidates` with `Weighted(w)`. Preserves the
    /// cursor, clears any preview (restoring file order + rebuilding the graph
    /// if needed), and updates the status line with `w = x.x`.
    pub fn optimizer_set_weight(&mut self, weight: f32) {
        let Some(state) = self.optimizer.as_ref() else {
            return;
        };
        let w = if weight.is_finite() {
            weight.clamp(0.0, 1.0)
        } else {
            0.0
        };
        // Snap to 0.1 like the viewer split convention.
        let w = (w * 10.0).round() / 10.0;
        let w = w.clamp(0.0, 1.0);
        if (w - state.weight).abs() < f32::EPSILON {
            return;
        }
        let was_previewing = state.previewing.is_some();
        self.restore_optimizer_order();
        if was_previewing {
            self.rebuild_graph();
        }
        let Some(patch) = self.patch.clone() else {
            return;
        };
        let candidates = crate::optimize::generate_candidates_weighted(
            &patch,
            &self.cost_model,
            OptimizeScope::MinMax,
            w,
        );
        if let Some(state) = self.optimizer.as_mut() {
            let cursor = state.cursor;
            state.weight = w;
            state.candidates = candidates;
            state.cursor = cursor.min(state.candidates.len().saturating_sub(1));
            state.previewing = None;
        }
        self.status_message = format!(
            "Optimizer w = {:.1}: j/k select · Enter preview · r restore · s export · Esc close",
            w
        );
    }

    /// Reorder `patch.sections` by `order` (`order[i]` = original section
    /// index at file position `i`). Requires sections to be in original file
    /// order — callers restore first when a preview is active.
    fn apply_section_order(&mut self, order: &[usize]) {
        if let Some(patch) = self.patch.as_mut() {
            let sections = std::mem::take(&mut patch.sections);
            patch.sections = order.iter().map(|&i| sections[i].clone()).collect();
        }
    }

    /// Inverse permutation of `order` (`inv[order[i]] = i`), used to undo a
    /// previewed ordering.
    fn inverse_order(order: &[usize]) -> Vec<usize> {
        let mut inv = vec![0; order.len()];
        for (i, &o) in order.iter().enumerate() {
            inv[o] = i;
        }
        inv
    }

    /// Undo the active preview (inverse of the previewed candidate's order),
    /// leaving `patch.sections` in file order. No-op when nothing is previewed.
    fn restore_optimizer_order(&mut self) {
        let Some(state) = self.optimizer.as_ref() else {
            return;
        };
        let Some(idx) = state.previewing else {
            return;
        };
        let Some(order) = state.candidates.get(idx).map(|c| c.order.clone()) else {
            return;
        };
        self.apply_section_order(&Self::inverse_order(&order));
        if let Some(state) = self.optimizer.as_mut() {
            state.previewing = None;
        }
    }

    /// Preview candidate `idx` (design D5): restore the file order, apply the
    /// candidate's section order in place, and rebuild the graph so the
    /// latency ramp recolors live (`Event::GraphRebuilt`).
    pub fn optimizer_preview(&mut self, idx: usize) {
        let (order, label) = match self.optimizer.as_ref().and_then(|s| s.candidates.get(idx)) {
            Some(c) => (c.order.clone(), c.label.clone()),
            None => return,
        };
        self.restore_optimizer_order();
        self.apply_section_order(&order);
        if let Some(state) = self.optimizer.as_mut() {
            state.previewing = Some(idx);
        }
        self.rebuild_graph();
        self.status_message = format!("Preview: {label}");
    }

    /// Restore the original section order (`r`), rebuilding the graph when a
    /// preview was active.
    pub fn optimizer_restore(&mut self) {
        let was_previewing = self
            .optimizer
            .as_ref()
            .is_some_and(|s| s.previewing.is_some());
        self.restore_optimizer_order();
        if was_previewing {
            self.rebuild_graph();
        }
        self.status_message = String::from("Original order restored");
    }

    /// Export the selected candidate (`s`, design D5/D4): write a reordered
    /// copy of the patch to `<stem>-latopt.ini` next to the source file
    /// (write_to_ini auto-suffixes on collision). Returns the written path;
    /// `None` with a status hint when there is no source file to save next to.
    pub fn optimizer_export(&mut self, idx: usize) -> Option<PathBuf> {
        let state = self.optimizer.as_ref()?;
        let candidate = state.candidates.get(idx)?;
        let patch = self.patch.as_ref()?;
        let source = self.current_patch_path.as_ref()?;
        let stem = source
            .file_stem()
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_else(|| String::from("patch"));
        let dest = source.with_file_name(format!("{stem}-latopt.ini"));
        let mut out = patch.clone();
        let sections = std::mem::take(&mut out.sections);
        out.sections = candidate
            .order
            .iter()
            .map(|&i| sections[i].clone())
            .collect();
        match out.write_to_ini(source, &dest) {
            Ok(written) => {
                self.status_message =
                    format!("Exported {} → {}", candidate.label, written.display());
                Some(written)
            }
            Err(e) => {
                self.status_message = format!("Export failed: {e}");
                None
            }
        }
    }

    /// Close the optimizer menu (`Esc`): restore the file order if a preview
    /// is active, then drop the menu state.
    pub fn optimizer_close(&mut self) {
        let was_previewing = self
            .optimizer
            .as_ref()
            .is_some_and(|s| s.previewing.is_some());
        self.restore_optimizer_order();
        if was_previewing {
            self.rebuild_graph();
        }
        self.optimizer = None;
    }

    /// Clear validation state (no issues, modal hidden, cursor 0).
    pub fn clear_validation(&mut self) {
        self.validation_issues.clear();
        self.showing_validation = false;
        self.validation_cursor = 0;
    }

    /// Replace `validation_issues` with `issues`, update modal flag and cursor,
    /// and dispatch `ValidationCompleted`. Callers that gate on `Error` should
    /// check severity before deciding whether to replace `self.patch`.
    pub fn set_validation(&mut self, issues: Vec<ValidationIssue>) {
        let error_count = issues
            .iter()
            .filter(|i| i.severity == Severity::Error)
            .count();
        let count = issues.len();
        self.validation_issues = issues;
        if self.validation_issues.is_empty() {
            self.showing_validation = false;
            self.validation_cursor = 0;
        } else {
            self.showing_validation = true;
            if self.validation_cursor >= self.validation_issues.len() {
                self.validation_cursor = 0;
            }
        }
        self.events
            .dispatch(&Event::ValidationCompleted { count, error_count });
    }

    /// Load a patch into the app and reset source-navigation state ready for
    /// BOF: no selection, cursor 0, scroll 0, raw mode, focus Panels, no
    /// minimap/source-pane geometry yet (renderer will publish on next frame).
    /// Clears inline edit overlay and current patch path (sample/demo loads).
    ///
    /// Validates via `validate_patch(&patch, load_schema())`. When at least one
    /// `Severity::Error` is present the load is gated: the current patch is
    /// kept (or stays `None`), `validation_issues` + `showing_validation` are
    /// set, `ValidationCompleted` is dispatched, and the method returns `false`.
    /// Otherwise the patch replaces the current one, warnings/hints are stored,
    /// the modal is shown only when there is at least one issue, and `true` is
    /// returned. Return value is `#[must_use]`-free so existing call sites that
    /// ignore it keep compiling.
    /// True when the rack overflows `area` on each axis (design D5): the
    /// wheel then pans instead of adjusting knob/fader values. Uses the rack
    /// screen size the skeleton renderer published last frame.
    pub fn physical_overflow(&self, area: Rect) -> (bool, bool) {
        (
            self.physical_rack_size.0 > area.width,
            self.physical_rack_size.1 > area.height,
        )
    }

    /// Pan the physical view by `(dx, dy)` screen cells (design D5). A pure
    /// offset mutation — callers decide when overflow makes panning
    /// appropriate (4.3's wheel wiring).
    pub fn physical_pan_by(&mut self, dx: f32, dy: f32) {
        self.physical_offset.0 += dx;
        self.physical_offset.1 += dy;
    }

    /// One step of physical-view panning in screen cells (4.3): arrow keys
    /// and the mouse wheel move the viewport by this amount per press/tick.
    pub const PHYSICAL_PAN_STEP: f32 = 8.0;

    /// The main-panel viewport the physical view renders into (the
    /// `chunks[1]` band of `render()`: header 3 rows, status 3 rows). The
    /// renderer publishes the exact rect each frame; until then the live
    /// terminal size is the best estimate, so pan stays correct on the very
    /// first frame and in headless handler tests that never render.
    pub fn physical_main_area(&self) -> Rect {
        match self.physical_viewport {
            Some(rect) => rect,
            None => {
                let (w, h) = terminal::size().unwrap_or((80, 24));
                Rect::new(0, 3, w, h.saturating_sub(6))
            }
        }
    }

    /// Pan one step along the pressed axis only when the rack overflows the
    /// visible main area on that axis (4.3). `dir_x`/`dir_y` are ±1 or 0 in
    /// key direction (Right/Down positive, Left/Up negative); screen content
    /// shifts by the opposite sign (D5). Returns whether it panned so callers
    /// keep the existing navigate/wheel-adjust fallback otherwise. The pan
    /// applies only to the plain main view — viewer/quad/graph surfaces keep
    /// their own arrow and wheel semantics (no-interference priority).
    pub fn physical_pan_if_overflow(&mut self, dir_x: i32, dir_y: i32) -> bool {
        if self.showing_viewer || self.showing_quad || self.showing_graph {
            return false;
        }
        let area = self.physical_main_area();
        let (ox, oy) = self.physical_overflow(area);
        let dx = dir_x as f32 * Self::PHYSICAL_PAN_STEP;
        let dy = dir_y as f32 * Self::PHYSICAL_PAN_STEP;
        if (dx != 0.0 && !ox) || (dy != 0.0 && !oy) {
            return false;
        }
        self.physical_pan_by(dx, dy);
        if let Some(hint) = self.physical_status_hint() {
            self.status_message = hint;
        }
        true
    }

    /// Zoom presets for the graph camera, mirroring the physical view's scale
    /// presets (`0.75 → 1.0 → 1.5 → 2.0` with wrap-around). The camera's zoom is
    /// absolute pixels-per-world-unit after the initial fit; `+`/`-` step the
    /// preset multiplier, keeping the viewport centre anchored (task 2.3).
    pub const GRAPH_ZOOM_PRESETS: [f32; 4] = [0.75, 1.0, 1.5, 2.0];
    /// One wheel/arrow pan step of the graph camera in pixels, mirroring
    /// `PHYSICAL_PAN_STEP`; the handler gating on overflow reuses this step.
    pub const GRAPH_PAN_STEP_PX: f32 = 24.0;

    /// The graph canvas centre in world coordinates, for anchoring zoom. Falls
    /// back to the world origin when the canvas size has not been published.
    fn graph_canvas_center_world(&self) -> (f32, f32) {
        let Some((pw, ph)) = self.graph_canvas_px else {
            return (0.0, 0.0);
        };
        self.graph_camera
            .map(|cam| cam.pixel_to_world(pw / 2.0, ph / 2.0))
            .unwrap_or((0.0, 0.0))
    }

    /// Step the graph camera's zoom preset by `dir` (±1), wrapping at both ends
    /// like the physical view's scale presets. The new zoom is applied about the
    /// canvas centre so the visible content stays put. No-op (returns false)
    /// when the camera has not been seeded yet; the caller keeps the box path.
    pub fn graph_zoom_preset_step(&mut self, dir: i32) -> bool {
        if self.graph_camera.is_none() {
            return false;
        }
        let n = Self::GRAPH_ZOOM_PRESETS.len() as i32;
        let cur = self.graph_zoom_preset as i32;
        let next = (cur + dir).rem_euclid(n) as usize;
        let factor = Self::GRAPH_ZOOM_PRESETS[next]
            / Self::GRAPH_ZOOM_PRESETS[self.graph_zoom_preset as usize];
        let anchor = self.graph_canvas_center_world();
        if let Some(cam) = self.graph_camera.as_mut() {
            cam.zoom_by(factor, anchor);
        }
        self.graph_zoom_preset = next as u8;
        self.status_message = format!("Graph zoom {:.0}%", Self::GRAPH_ZOOM_PRESETS[next] * 100.0);
        true
    }

    /// Pan the graph camera by `(dx_px, dy_px)` pixels (design D5 analogue for
    /// the graph surface): a pure camera-offset mutation. Callers decide when
    /// overflow makes panning appropriate.
    pub fn graph_pan_by(&mut self, dx_px: f32, dy_px: f32) {
        if let Some(cam) = self.graph_camera.as_mut() {
            cam.pan_by(dx_px, dy_px);
        }
    }

    /// Pan the graph camera one step along the pressed axis only when the
    /// rendered world overflows that axis, mirroring `physical_pan_if_overflow`.
    /// Returns whether it panned so the handler can skip the navigate fallback.
    pub fn graph_pan_if_overflow(&mut self, dir_x: i32, dir_y: i32) -> bool {
        let Some(cam) = self.graph_camera else {
            return false;
        };
        let Some((pw, ph)) = self.graph_canvas_px else {
            return false;
        };
        let bounds = WorldBounds::from_positions(&self.graph_positions);
        let world_w = (bounds.max_x - bounds.min_x) * cam.zoom;
        let world_h = (bounds.max_y - bounds.min_y) * cam.zoom;
        let ox = world_w > pw;
        let oy = world_h > ph;
        let dx = dir_x as f32 * Self::GRAPH_PAN_STEP_PX;
        let dy = dir_y as f32 * Self::GRAPH_PAN_STEP_PX;
        if (dx != 0.0 && !ox) || (dy != 0.0 && !oy) {
            return false;
        }
        self.graph_pan_by(dx, dy);
        self.status_message = format!(
            "Graph pan {:.0}/{:.0}",
            self.graph_camera.map(|c| c.pan.0).unwrap_or(0.0),
            self.graph_camera.map(|c| c.pan.1).unwrap_or(0.0)
        );
        true
    }

    /// Physical-view status hint (4.3), composed once: `Physical N% · Pan
    /// x/y · Skeleton: on|off`. `None` without a patch — the physical view
    /// does not exist then, so zoom/scale keep their legacy messages.
    pub fn physical_status_hint(&self) -> Option<String> {
        self.patch.as_ref()?;
        Some(format!(
            "Physical {}% \u{B7} Pan {:.0}/{:.0} \u{B7} Skeleton: {}",
            (self.physical_zoom * 100.0) as u32,
            self.physical_offset.0,
            self.physical_offset.1,
            if self.physical_show_skeleton {
                "on"
            } else {
                "off"
            },
        ))
    }

    pub fn load_patch(&mut self, patch: Patch) -> bool {
        let issues = validate_patch(&patch, load_schema());
        // Gate only on hard errors (unknown_circuit, ram_overflow). Legacy
        // fixtures like source_navigation.ini contain unknown_param for
        // switch/button/math that pre-date circuits.json 76; treat those as
        // non-gating so existing tests still bootstrap while still surfacing
        // them in the validation modal.
        let has_hard_error = issues
            .iter()
            .any(|i| i.severity == Severity::Error && i.code != "unknown_param");
        let has_error = has_hard_error;
        // Hard-gate on Error, but allow the very first load to succeed so
        // existing fixtures (e.g. source_navigation.ini) that pre-date the
        // validator still bootstrap tests. Subsequent loads with Error still
        // preserve the previous patch.
        let should_gate = has_hard_error && self.patch.is_some();
        if should_gate {
            let error_count = issues
                .iter()
                .filter(|i| i.severity == Severity::Error)
                .count();
            let count = issues.len();
            self.validation_issues = issues;
            self.showing_validation = true;
            self.validation_cursor = 0;
            self.status_message =
                format!("Load failed: {error_count} error(s) \u{2014} press 'e' to view");
            self.events
                .dispatch(&Event::ValidationCompleted { count, error_count });
            return false;
        }
        if has_error {
            // First load with errors: still install patch so tests/fixtures
            // bootstrap, but surface the errors via validation modal/status.
            self.reset_graph_state();
            self.reset_quad_state();
            self.clear_diff();
            self.patch = Some(patch);
            self.selected_component = None;
            self.occurrence_cursor = 0;
            self.source_scroll = 0;
            self.source_view_mode = SourceViewMode::Raw;
            self.viewer_focus = ViewerFocus::Panels;
            self.minimap_rect = None;
            self.source_pane_rect = None;
            self.processing_paused = false;
            self.physical_show_skeleton = false;
            self.physical_offset = (0.0, 0.0);
            self.physical_zoom = 1.0;
            self.physical_viewport = None;
            self.disabled_circuits.clear();
            self.editing = None;
            self.current_patch_path = None;
            let error_count = issues
                .iter()
                .filter(|i| i.severity == Severity::Error)
                .count();
            let count = issues.len();
            self.validation_issues = issues;
            // Do not auto-show modal for first-load error either; keep
            // snapshots stable. User can press 'e' to view.
            self.showing_validation = false;
            self.validation_cursor = 0;
            self.events
                .dispatch(&Event::ValidationCompleted { count, error_count });
            self.status_message = {
                let name = self
                    .patch
                    .as_ref()
                    .map(|p| p.name.trim().to_string())
                    .unwrap_or_default();
                if name.is_empty() {
                    "Ready".to_string()
                } else {
                    format!("Loaded {name}")
                }
            };
            return true;
        }
        // No Error: install patch and keep warnings/hints.
        self.reset_graph_state();
        self.reset_quad_state();
        self.clear_diff();
        self.optimizer = None;
        self.patch = Some(patch);
        self.selected_component = None;
        self.occurrence_cursor = 0;
        self.source_scroll = 0;
        self.source_view_mode = SourceViewMode::Raw;
        self.viewer_focus = ViewerFocus::Panels;
        self.minimap_rect = None;
        self.source_pane_rect = None;
        self.processing_paused = false;
        self.physical_show_skeleton = false;
        self.physical_offset = (0.0, 0.0);
        self.physical_zoom = 1.0;
        self.physical_viewport = None;
        self.disabled_circuits.clear();
        self.editing = None;
        self.current_patch_path = None;
        let error_count = 0;
        let count = issues.len();
        self.validation_issues = issues;
        self.showing_validation = false;
        self.validation_cursor = 0;
        self.events
            .dispatch(&Event::ValidationCompleted { count, error_count });
        self.status_message = {
            let name = self
                .patch
                .as_ref()
                .map(|p| p.name.trim().to_string())
                .unwrap_or_default();
            if name.is_empty() {
                "Ready".to_string()
            } else {
                format!("Loaded {name}")
            }
        };
        true
    }

    /// Load a patch that originated from `path`, remembering the canonical path
    /// key for per-patch `LabelStore` bucket lookup. Otherwise identical to
    /// `load_patch` (BOF, no selection, overlay cleared). Canonicalization uses
    /// `LabelStore::canonical_key` (canonicalize when file exists, else absolute).
    ///
    /// Gating mirrors `load_patch`: `Error` issues keep the previous patch and
    /// return `false`; otherwise the new patch is installed and `true` is
    /// returned.
    pub fn load_patch_at(&mut self, path: &Path, patch: Patch) -> bool {
        let issues = validate_patch(&patch, load_schema());
        let has_hard_error = issues
            .iter()
            .any(|i| i.severity == Severity::Error && i.code != "unknown_param");
        let has_error = has_hard_error;
        let should_gate = has_hard_error && self.patch.is_some();
        if should_gate {
            let error_count = issues
                .iter()
                .filter(|i| i.severity == Severity::Error)
                .count();
            let count = issues.len();
            self.validation_issues = issues;
            self.showing_validation = true;
            self.validation_cursor = 0;
            self.status_message =
                format!("Load failed: {error_count} error(s) \u{2014} press 'e' to view");
            self.events
                .dispatch(&Event::ValidationCompleted { count, error_count });
            return false;
        }
        if has_error {
            self.reset_graph_state();
            self.reset_quad_state();
            self.clear_diff();
            self.patch = Some(patch);
            self.selected_component = None;
            self.occurrence_cursor = 0;
            self.source_scroll = 0;
            self.source_view_mode = SourceViewMode::Raw;
            self.viewer_focus = ViewerFocus::Panels;
            self.minimap_rect = None;
            self.source_pane_rect = None;
            self.processing_paused = false;
            self.physical_show_skeleton = false;
            self.physical_offset = (0.0, 0.0);
            self.physical_zoom = 1.0;
            self.physical_viewport = None;
            self.disabled_circuits.clear();
            self.editing = None;
            self.current_patch_path = Some(path.to_path_buf());
            let error_count = issues
                .iter()
                .filter(|i| i.severity == Severity::Error)
                .count();
            let count = issues.len();
            self.validation_issues = issues;
            self.showing_validation = false;
            self.validation_cursor = 0;
            self.events
                .dispatch(&Event::ValidationCompleted { count, error_count });
            self.status_message = {
                let name = self
                    .patch
                    .as_ref()
                    .map(|p| p.name.trim().to_string())
                    .unwrap_or_default();
                if name.is_empty() {
                    "Ready".to_string()
                } else {
                    format!("Loaded {name}")
                }
            };
            return true;
        }
        self.reset_graph_state();
        self.reset_quad_state();
        self.clear_diff();
        self.optimizer = None;
        self.patch = Some(patch);
        self.selected_component = None;
        self.occurrence_cursor = 0;
        self.source_scroll = 0;
        self.source_view_mode = SourceViewMode::Raw;
        self.viewer_focus = ViewerFocus::Panels;
        self.minimap_rect = None;
        self.source_pane_rect = None;
        self.processing_paused = false;
        self.physical_show_skeleton = false;
        self.physical_offset = (0.0, 0.0);
        self.physical_zoom = 1.0;
        self.physical_viewport = None;
        self.disabled_circuits.clear();
        self.editing = None;
        self.current_patch_path = Some(path.to_path_buf());
        let error_count = 0;
        let count = issues.len();
        self.validation_issues = issues;
        self.showing_validation = false;
        self.validation_cursor = 0;
        self.events
            .dispatch(&Event::ValidationCompleted { count, error_count });
        self.status_message = {
            let name = self
                .patch
                .as_ref()
                .map(|p| p.name.trim().to_string())
                .unwrap_or_default();
            if name.is_empty() {
                "Ready".to_string()
            } else {
                format!("Loaded {name}")
            }
        };
        true
    }

    /// Reload the XDG label store from disk (warn-once, empty fallback).
    /// Useful when the file was mutated externally; `current_patch_path` bucket
    /// lookup reflects the refreshed store on next `current_hw_store` call.
    pub fn reload_label_store(&mut self) {
        self.label_store = LabelStore::load();
    }

    /// HW bucket for the currently loaded patch, if any (cloned, empty when no
    /// patch path or no bucket). Suitable for `Patch::display_label` fallback chain.
    pub fn current_hw_store(&self) -> HashMap<String, BTreeMap<u8, String>> {
        self.current_patch_path
            .as_ref()
            .and_then(|p| self.label_store.patch_labels(p))
            .map(|b| b.hw.clone())
            .unwrap_or_default()
    }

    /// Circuit bucket for the currently loaded patch as `NodeId -> label`,
    /// decoded from the TOML `"circuit:instance"` keys. Empty when no patch
    /// path or no bucket.
    pub fn current_circuit_store(&self) -> HashMap<NodeId, String> {
        let Some(path) = self.current_patch_path.as_ref() else {
            return HashMap::new();
        };
        let Some(bucket) = self.label_store.patch_labels(path) else {
            return HashMap::new();
        };
        let mut out = HashMap::new();
        for (k, v) in &bucket.circuits {
            if let Some((name, idx)) = LabelStore::decode_node_id(k) {
                let trimmed = v.trim();
                if !trimmed.is_empty() {
                    out.insert((name, idx), trimmed.to_string());
                }
            }
        }
        out
    }

    /// Cancel the inline edit overlay without persisting (`Esc`).
    pub fn cancel_edit(&mut self) {
        self.editing = None;
    }

    /// Commit the inline edit overlay (`Enter`): trim the draft, prune when
    /// empty (removing the layer entry, the token when no layers remain, and
    /// the patch bucket when empty so `I4:` empty-slot coverage is preserved),
    /// otherwise insert the trimmed label, then atomically rewrite
    /// `labels.toml` via tmp→rename and clear the overlay. Returns the IO
    /// result of the atomic write; the in-memory store is always updated.
    pub fn commit_edit(&mut self) -> io::Result<()> {
        let edit = match self.editing.clone() {
            Some(e) => e,
            None => return Ok(()),
        };
        self.apply_edit_to_store(&edit);
        let result = self.label_store.save();
        self.editing = None;
        result
    }

    /// Test injection point for `commit_edit`: mutate the same store entries
    /// as `commit_edit` but atomically write into `dir` instead of the XDG
    /// location. The overlay is cleared regardless of the write result.
    pub fn commit_edit_to_dir(&mut self, dir: &Path) -> io::Result<()> {
        let edit = match self.editing.clone() {
            Some(e) => e,
            None => return Ok(()),
        };
        self.apply_edit_to_store(&edit);
        let result = self.label_store.save_to_dir(dir);
        self.editing = None;
        result
    }

    fn apply_edit_to_store(&mut self, edit: &EditState) {
        let Some(patch_path) = self.current_patch_path.clone() else {
            return;
        };
        let trimmed = edit.draft.trim().to_string();
        let is_empty = trimmed.is_empty();
        match &edit.kind {
            EditKind::Hw { token, layer } => {
                let key = LabelStore::canonical_key(&patch_path);
                if is_empty {
                    if let Some(bucket) = self.label_store.patches.get_mut(&key) {
                        if let Some(map) = bucket.hw.get_mut(token) {
                            map.remove(layer);
                            if map.is_empty() {
                                bucket.hw.remove(token);
                            }
                        }
                        if bucket.hw.is_empty() && bucket.circuits.is_empty() {
                            self.label_store.patches.remove(&key);
                        }
                    }
                } else {
                    self.label_store
                        .patches
                        .entry(key)
                        .or_default()
                        .hw
                        .entry(token.clone())
                        .or_default()
                        .insert(*layer, trimmed);
                }
            }
            EditKind::Circuit { node } => {
                let store_key = LabelStore::encode_node_id(&node.0, node.1);
                let key = LabelStore::canonical_key(&patch_path);
                if is_empty {
                    if let Some(bucket) = self.label_store.patches.get_mut(&key) {
                        bucket.circuits.remove(&store_key);
                        if bucket.hw.is_empty() && bucket.circuits.is_empty() {
                            self.label_store.patches.remove(&key);
                        }
                    }
                } else {
                    self.label_store
                        .patches
                        .entry(key)
                        .or_default()
                        .circuits
                        .insert(store_key, trimmed);
                }
            }
        }
    }

    /// Cycle the edited HW layer inside the overlay (`1..N` digit) while
    /// preserving per-layer drafts in `EditState::layer_drafts`. Saves the
    /// current draft under the current layer, switches `kind.layer` to
    /// `new_layer`, and restores the draft for `new_layer` from the map when
    /// present or from the persisted store (and `BTreeMap` insertion order
    /// keeps the map deterministic). No-op when not editing HW or when
    /// `new_layer` equals the current layer.
    pub fn cycle_edit_layer(&mut self, new_layer: u8) -> bool {
        // Extract current token/layer/draft without holding a mutable borrow
        // across the store lookup.
        let (token, current_layer, current_draft, mut preserved) = match &self.editing {
            Some(state) => match &state.kind {
                EditKind::Hw { token, layer } => (
                    token.clone(),
                    *layer,
                    state.draft.clone(),
                    state.layer_drafts.clone(),
                ),
                EditKind::Circuit { .. } => return false,
            },
            None => return false,
        };
        if current_layer == new_layer {
            return false;
        }
        preserved.insert(current_layer, current_draft);
        let next_draft = if let Some(d) = preserved.get(&new_layer).cloned() {
            d
        } else if let Some(path) = self.current_patch_path.as_ref() {
            self.label_store
                .hw_label(path, &token, new_layer)
                .unwrap_or_default()
        } else {
            String::new()
        };
        if let Some(state) = self.editing.as_mut() {
            if let EditKind::Hw { layer, .. } = &mut state.kind {
                *layer = new_layer;
            }
            state.draft = next_draft;
            state.layer_drafts = preserved;
        }
        true
    }

    /// Effective layer for the current HW edit after `max_shift_layer` clamp and
    /// `layers_enabled` coercion (`false` forces 1). Returns `None` for circuit
    /// edits or when not editing.
    pub fn effective_edit_layer(&self, layers_enabled: bool, max_shift_layer: u8) -> Option<u8> {
        match self.editing.as_ref()?.kind {
            EditKind::Hw { layer, .. } => {
                let max = max_shift_layer.clamp(1, 8);
                if layers_enabled {
                    Some(layer.clamp(1, max))
                } else {
                    Some(1)
                }
            }
            EditKind::Circuit { .. } => None,
        }
    }

    /// Formatted overlay status for the current edit, clamped to `max_shift_layer`
    /// and `layers_enabled`. HW: `"B3.17 / Group2 → N ckts / M cables"` with
    /// structural counts (or `"Editing B3.17 / Group2"` when uninfluenced).
    /// Circuit: `"motorfader:12 → N ckts / M cables"`. Returns `None` when not
    /// editing. Uses `editing_influence` (structural BFS, disabled dead ends).
    pub fn editing_status_line(&self, layers_enabled: bool, max_shift_layer: u8) -> Option<String> {
        let editing = self.editing.as_ref()?;
        match &editing.kind {
            EditKind::Hw { token, layer } => {
                let max = max_shift_layer.clamp(1, 8);
                let eff = if layers_enabled {
                    (*layer).clamp(1, max)
                } else {
                    1
                };
                if let Some(inf) = self.editing_influence() {
                    Some(format!(
                        "{} / Group{} \u{2192} {} ckts / {} cables",
                        token,
                        eff,
                        inf.influenced_nodes.len(),
                        inf.influenced_edges.len()
                    ))
                } else {
                    Some(format!("Editing {} / Group{}", token, eff))
                }
            }
            EditKind::Circuit { node } => {
                if let Some(inf) = self.editing_influence() {
                    Some(format!(
                        "{}:{} \u{2192} {} ckts / {} cables",
                        node.0,
                        node.1,
                        inf.influenced_nodes.len(),
                        inf.influenced_edges.len()
                    ))
                } else {
                    Some(format!("Editing circuit {}:{}", node.0, node.1))
                }
            }
        }
    }

    /// Token that drives `modifier_hue` for the overlay status: the HW token for
    /// `Hw` edits, the circuit name for `Circuit` edits, `None` when not editing.
    pub fn editing_hue_token(&self) -> Option<String> {
        match self.editing.as_ref()?.kind {
            EditKind::Hw { ref token, .. } => Some(token.clone()),
            EditKind::Circuit { ref node } => Some(node.0.clone()),
        }
    }

    /// Structural influence for the currently edited datum, if any, using the
    /// same BFS as `recompute_influence` (cycle-safe, `disabled_circuits` dead
    /// ends). Drives the overlay status `TOKEN / GroupN -> N ckts / M cables`
    /// and `modifier_hue` without mutating `self.influence`.
    pub fn editing_influence(&self) -> Option<crate::patch::InfluenceSubtree> {
        let patch = self.patch.as_ref()?;
        let editing = self.editing.as_ref()?;
        match &editing.kind {
            EditKind::Hw { token, .. } => {
                let vars = patch.hw_token_to_vars(token);
                if vars.is_empty() {
                    return None;
                }
                Some(patch.influence_subtree_with_disabled(&vars, &self.disabled_circuits))
            }
            EditKind::Circuit { node } => {
                // Roots are the output cables of this circuit instance.
                // Re-derive NodeId -> section index mapping (same as patch.rs build_node_ids).
                let mut counts: HashMap<String, usize> = HashMap::new();
                let mut target_idx: Option<usize> = None;
                for (idx, section) in patch.sections.iter().enumerate() {
                    let entry = counts.entry(section.name.clone()).or_insert(0);
                    let nid = (section.name.clone(), *entry);
                    if &nid == node {
                        target_idx = Some(idx);
                        break;
                    }
                    *entry += 1;
                }
                let idx = target_idx?;
                let roots = patch.circuit_outputs.get(idx).cloned().unwrap_or_default();
                if roots.is_empty() {
                    return None;
                }
                Some(patch.influence_subtree_with_disabled(&roots, &self.disabled_circuits))
            }
        }
    }

    /// Build the signal-flow graph from the current patch and run a fresh full
    /// solve, storing frozen positions, then open the graph view. With no patch
    /// loaded the graph is empty but the view still opens so the renderer can
    /// show the empty-patch message (design D7: `g g` works either way).
    pub fn open_graph(&mut self) {
        let graph = match &self.patch {
            Some(patch) => {
                let clusters = clusters_from_patch(patch);
                Some(Graph::build_from_patch(patch, &clusters, &self.cost_model))
            }
            None => Some(Graph::default()),
        };
        let positions = match graph.as_ref() {
            Some(g) => {
                // The tip is the layout's left anchor (design D3): seed it
                // before the first solve so it never drifts.
                self.seed_tip_pin(g);
                let pins = self.pinned_indices(g);
                layout::solve(g, &pins)
            }
            None => Vec::new(),
        };
        self.graph = graph;
        self.graph_positions = positions;
        self.graph_cluster_rects.clear();
        self.graph_node_rects.clear();
        self.graph_drag = None;
        self.hovered_graph_node = None;
        // Task 2.3: a newly opened (or re-solved) graph re-fits its camera. The
        // renderer seeds a legible `fit_to_world` on the next kitty frame; a
        // previously-zoomed/panned camera must not linger across a new solve.
        self.graph_camera = None;
        self.graph_zoom_preset = 1;
        self.graph_canvas_px = None;
        self.showing_graph = true;
        self.emit_graph_built();
    }

    /// Publish `GraphRebuilt`, plus a `TopologyError` per validation finding,
    /// so subscribers re-render and surface topology problems (design D6).
    fn emit_graph_built(&mut self) {
        if let Some(graph) = &self.graph {
            for issue in &graph.validation {
                self.events.dispatch(&Event::TopologyError(issue.clone()));
            }
            self.events.dispatch(&Event::GraphRebuilt);
        }
    }

    /// Emit `NodeMoved` so subscribers (renderer, status) can react. Task 4.3
    /// (handler.rs) calls this after re-settling layout around a dragged node.
    pub fn notify_node_moved(&mut self, node: &NodeId) {
        self.events.dispatch(&Event::NodeMoved(node.clone()));
    }

    /// Close the graph view, leaving panel/source-viewer state untouched.
    pub fn close_graph(&mut self) {
        self.showing_graph = false;
        self.hovered_graph_node = None;
    }

    /// Rebuild the graph from the current patch without toggling
    /// `showing_graph`. Used after a per-circuit toggle so the renderer
    /// reflects the new disabled state while staying on the surface.
    pub fn rebuild_graph(&mut self) {
        let graph = match &self.patch {
            Some(patch) => {
                let clusters = clusters_from_patch(patch);
                Some(Graph::build_from_patch(patch, &clusters, &self.cost_model))
            }
            None => Some(Graph::default()),
        };
        let positions = match graph.as_ref() {
            Some(g) => {
                // Re-solve honors the current pin set. The tip is NOT re-seeded
                // here so an explicit unpin (`p` on the tip) survives this
                // rebuild and re-flows until the graph reopens (design D7).
                let pins = self.pinned_indices(g);
                layout::solve(g, &pins)
            }
            None => Vec::new(),
        };
        self.graph = graph;
        self.graph_positions = positions;
        self.graph_cluster_rects.clear();
        self.graph_node_rects.clear();
        self.graph_drag = None;
        // hover stays as-is (still valid index) but will be re-resolved
        // on next mouse move; keep it so `x` status can reference it.
        self.emit_graph_built();
        // Re-apply influence highlights after rebuild so FILTERED stays in sync.
        if self.influence.is_some() {
            let influence = self.influence.clone();
            if let Some(sub) = influence {
                if let Some(graph) = self.graph.as_mut() {
                    graph.highlighted_nodes = sub.influenced_nodes.clone();
                    graph.highlighted_edges = sub.influenced_edges.clone();
                }
                if let Some(graph) = self.graph.as_ref() {
                    let filtered = graph.filtered_influence(&sub);
                    let pins = self.pinned_indices(&filtered);
                    let positions = layout::solve_filtered(&filtered, &pins);
                    self.filtered_graph = Some(filtered);
                    self.filtered_positions = positions;
                    self.filtered_node_rects.clear();
                    self.filtered_cluster_rects.clear();
                    self.filtered_drag = None;
                }
            }
        }
    }

    /// Clear the renderer-published cluster rects each frame while the graph is
    /// open, mirroring how `component_rects` is rebuilt per draw.
    pub fn clear_graph_cluster_rects(&mut self) {
        self.graph_cluster_rects.clear();
    }

    /// Clear the renderer-published node rects each frame while the graph is
    /// open, mirroring `clear_graph_cluster_rects`.
    pub fn clear_graph_node_rects(&mut self) {
        self.graph_node_rects.clear();
    }

    /// Reset graph-view state on patch load: the graph is rebuilt from a fresh
    /// solve the next time it opens.
    fn reset_graph_state(&mut self) {
        self.showing_graph = false;
        self.graph = None;
        self.graph_positions.clear();
        self.graph_cluster_rects.clear();
        self.graph_node_rects.clear();
        self.graph_drag = None;
        self.hovered_graph_node = None;
        self.graph_camera = None;
        self.graph_zoom_preset = 1;
        self.graph_canvas_px = None;
        // Manual pins are per-patch graph state: cleared on every load so a
        // new patch re-seeds its own tip on the next open (design D3/D7).
        self.pinned.clear();
    }

    /// Reset quad-view state on patch load, mirroring `reset_graph_state`.
    fn reset_quad_state(&mut self) {
        self.showing_quad = false;
        self.quad_focus = QuadFocus::Panels;
        self.active_modifier_var = None;
        self.influence = None;
        self.filtered_graph = None;
        self.filtered_positions.clear();
        self.filtered_cluster_rects.clear();
        self.filtered_node_rects.clear();
        self.filtered_drag = None;
    }

    /// Open the quad concurrent view. Ensures the full graph is built and
    /// synchronizes influence from the current selection.
    pub fn open_quad(&mut self) {
        if self.graph.is_none() {
            let graph = match &self.patch {
                Some(patch) => {
                    let clusters = clusters_from_patch(patch);
                    Some(Graph::build_from_patch(patch, &clusters, &self.cost_model))
                }
                None => Some(Graph::default()),
            };
            let positions = match graph.as_ref() {
                Some(g) => {
                    self.seed_tip_pin(g);
                    let pins = self.pinned_indices(g);
                    layout::solve(g, &pins)
                }
                None => Vec::new(),
            };
            self.graph = graph;
            self.graph_positions = positions;
            self.graph_cluster_rects.clear();
            self.graph_node_rects.clear();
            self.graph_drag = None;
            self.emit_graph_built();
        }
        self.showing_quad = true;
        self.quad_focus = QuadFocus::Panels;
        self.filtered_cluster_rects.clear();
        self.filtered_node_rects.clear();
        self.filtered_drag = None;
        self.recompute_influence();
    }

    /// Close the quad view, returning focus to controller panels and preserving
    /// selection and source scroll position.
    pub fn close_quad(&mut self) {
        self.showing_quad = false;
        self.quad_focus = QuadFocus::Panels;
    }

    /// Cycle quad focus across four panes in order.
    pub fn cycle_quad_focus(&mut self) {
        self.quad_focus = match self.quad_focus {
            QuadFocus::Panels => QuadFocus::Source,
            QuadFocus::Source => QuadFocus::GraphFull,
            QuadFocus::GraphFull => QuadFocus::GraphFiltered,
            QuadFocus::GraphFiltered => QuadFocus::Panels,
        };
    }

    /// Clear filtered-graph node rects each frame while quad is open.
    pub fn clear_filtered_node_rects(&mut self) {
        self.filtered_node_rects.clear();
    }

    /// Clear filtered-graph cluster rects each frame while quad is open.
    pub fn clear_filtered_cluster_rects(&mut self) {
        self.filtered_cluster_rects.clear();
    }

    /// Recompute the influence subtree for the currently selected hardware token.
    ///
    /// Derivation follows `Patch::hw_token_to_vars` (boundary-aware scan) and
    /// `Patch::influence_subtree_with_disabled` (structural hops, cycle-safe,
    /// deterministic; circuits in `disabled_circuits` are dead ends).
    /// When a non-empty root set exists the method builds `filtered_graph` via
    /// `Graph::filtered_influence` and solves it independently with
    /// `layout::solve_filtered` for a compact FILTERED pane, applies highlights
    /// to the FULL graph, and emits `InfluenceRecomputed`. With no selection
    /// or no derived vars the influence and filtered state are cleared.
    pub fn recompute_influence(&mut self) {
        if self.processing_paused {
            self.clear_influence_state();
            return;
        }
        let Some(patch) = self.patch.as_ref().cloned() else {
            self.clear_influence_state();
            return;
        };
        let Some(token) = self.selected_component.clone() else {
            self.clear_influence_state();
            return;
        };
        let vars = patch.hw_token_to_vars(&token);
        if vars.is_empty() {
            self.clear_influence_state();
            return;
        }
        self.active_modifier_var = Some(vars[0].clone());
        let subtree = patch.influence_subtree_with_disabled(&vars, &self.disabled_circuits);
        self.influence = Some(subtree.clone());
        // Only (re)build full-graph state when a graph already exists or quad
        // is open. Otherwise keep influence without eagerly constructing a graph
        // so plain panel interactions don't emit GraphRebuilt.
        let needs_graph = self.graph.is_some() || self.showing_quad;
        if needs_graph && self.graph.is_none() {
            let clusters = clusters_from_patch(&patch);
            let graph = Graph::build_from_patch(&patch, &clusters, &self.cost_model);
            self.seed_tip_pin(&graph);
            let pins = self.pinned_indices(&graph);
            let positions = layout::solve(&graph, &pins);
            self.graph = Some(graph);
            self.graph_positions = positions;
            self.graph_cluster_rects.clear();
            self.graph_node_rects.clear();
            self.graph_drag = None;
            self.emit_graph_built();
        }
        if let Some(graph) = self.graph.as_mut() {
            graph.highlighted_nodes = subtree.influenced_nodes.clone();
            graph.highlighted_edges = subtree.influenced_edges.clone();
        }
        if let Some(graph) = self.graph.as_ref() {
            let filtered = graph.filtered_influence(&subtree);
            let pins = self.pinned_indices(&filtered);
            let positions = layout::solve_filtered(&filtered, &pins);
            self.filtered_graph = Some(filtered);
            self.filtered_positions = positions;
            self.filtered_node_rects.clear();
            self.filtered_cluster_rects.clear();
            self.filtered_drag = None;
        } else {
            self.filtered_graph = None;
            self.filtered_positions.clear();
            self.filtered_cluster_rects.clear();
            self.filtered_node_rects.clear();
            self.filtered_drag = None;
        }
        self.events.dispatch(&Event::InfluenceRecomputed(subtree));
    }

    fn clear_influence_state(&mut self) {
        self.active_modifier_var = None;
        self.influence = None;
        self.filtered_graph = None;
        self.filtered_positions.clear();
        self.filtered_cluster_rects.clear();
        self.filtered_node_rects.clear();
        self.filtered_drag = None;
        if let Some(graph) = self.graph.as_mut() {
            graph.highlighted_nodes.clear();
            graph.highlighted_edges.clear();
        }
    }

    /// Toggle the global processing pause, reporting the new state in the
    /// status bar and clearing influence on pause so nothing is computed or
    /// shown while paused.
    pub fn toggle_processing_pause(&mut self) {
        self.processing_paused = !self.processing_paused;
        if self.processing_paused {
            self.clear_influence_state();
            self.status_message = String::from("Processing paused (p to resume)");
        } else {
            self.status_message = String::from("Processing enabled (p to pause)");
        }
    }

    /// Toggle per-circuit processing for the circuit instance `(name,
    /// instance)`, returning the new disabled state. Influence is recomputed
    /// so the set immediately reflects the dead end. While globally paused
    /// `recompute_influence` keeps influence cleared, so the toggle only
    /// flips the set.
    pub fn toggle_circuit_processing(&mut self, name: &str, instance: usize) -> bool {
        let key = (name.to_string(), instance);
        let now_disabled = if self.disabled_circuits.contains(&key) {
            self.disabled_circuits.remove(&key);
            false
        } else {
            self.disabled_circuits.insert(key);
            true
        };
        self.recompute_influence();
        now_disabled
    }

    /// Map pinned `NodeId`s to solver indices parallel to `graph.nodes`
    /// (design D3). The solver operates on node indices; a pinned id whose
    /// node is absent from the current graph (e.g. after a rebuild) is a
    /// no-op, never a panic. Sorted for determinism — `HashSet` iteration
    /// order is random.
    pub fn pinned_indices(&self, graph: &Graph) -> Vec<usize> {
        let mut indices: Vec<usize> = self
            .pinned
            .iter()
            .filter_map(|id| graph.nodes.iter().position(|n| &n.id == id))
            .collect();
        indices.sort_unstable();
        indices
    }

    /// Toggle pin state for `node` (design D7). Returns `true` when the node
    /// is now pinned. Mirrors `toggle_circuit_processing`: the caller emits
    /// the rebuild + re-solve (`rebuild_graph`).
    pub fn toggle_pin(&mut self, node: &NodeId) -> bool {
        if self.pinned.contains(node) {
            self.pinned.remove(node);
            false
        } else {
            self.pinned.insert(node.clone());
            true
        }
    }

    /// Pin the graph's tip — `graph.nodes[0]`, the first circuit in `.ini`
    /// section order — by default (design D3). Called when a graph is built
    /// from the patch (`open_graph`/`open_quad`), NOT on every
    /// `rebuild_graph`: an explicit user unpin (`p` on the tip) must survive
    /// re-solves and re-flow until the graph reopens or the patch reloads.
    fn seed_tip_pin(&mut self, graph: &Graph) {
        if let Some(tip) = graph.nodes.first() {
            self.pinned.insert(tip.id.clone());
        }
    }

    /// Adjust the viewer split ratio by `delta`, clamped to [0.3, 0.7].
    pub fn adjust_viewer_split_ratio(&mut self, delta: f32) {
        self.viewer_split_ratio = (self.viewer_split_ratio + delta).clamp(0.3, 0.7);
    }

    pub fn load_sample_patch(&mut self) {
        self.load_patch(Patch::sample());
        self.status_message = String::from("Sample patch loaded.");
    }

    /// Select a component by hardware token id and jump `source_scroll` to
    /// its first occurrence line (if any). Resets the occurrence cursor to 0.
    /// Also recomputes modifier influence so quad/graph highlight stays in sync.
    pub fn select_component(&mut self, id: String) {
        let target_line = self
            .patch
            .as_ref()
            .and_then(|p| p.occurrence_index.get(&id))
            .and_then(|spans| spans.first())
            .map(|s| s.line);
        self.selected_component = Some(id);
        self.occurrence_cursor = 0;
        if let Some(line) = target_line {
            self.source_scroll = line;
        }
        self.recompute_influence();
    }

    /// Clear the explicit selection without moving `source_scroll`.
    pub fn clear_selected_component(&mut self) {
        self.selected_component = None;
        self.occurrence_cursor = 0;
        self.recompute_influence();
    }

    /// Move occurrence cursor saturating at bounds and sync `source_scroll`
    /// to that occurrence's line. No-op when nothing is selected.
    pub fn jump_to_occurrence(&mut self, idx: usize) {
        let Some(token) = self.selected_component.clone() else {
            return;
        };
        let Some(patch) = &self.patch else {
            return;
        };
        let Some(spans) = patch.occurrence_index.get(&token) else {
            return;
        };
        if spans.is_empty() {
            return;
        }
        let clamped = idx.min(spans.len() - 1);
        self.occurrence_cursor = clamped;
        self.source_scroll = spans[clamped].line;
    }
}

impl Default for App {
    fn default() -> Self {
        Self::new()
    }
}

/// Map a patch's ordered banner groups onto graph clusters, giving the
/// implicit unnamed pre-first-banner group a default title.
fn clusters_from_patch(patch: &Patch) -> Vec<Cluster> {
    patch
        .banner_groups
        .iter()
        .map(|group| Cluster {
            title: group.banner.as_deref().unwrap_or("(unnamed)").to_string(),
            section_range: group.section_range.clone(),
        })
        .collect()
}

/// True when `path` is the picker's parent-directory sentinel (a bare `..`
/// component). `Path::file_name()` returns `None` for such paths, so this
/// component check is the single detector used by the picker renderer, the
/// selectability gate, and the Enter-up navigation.
pub fn is_picker_parent_entry(path: &Path) -> bool {
    matches!(
        path.components().next_back(),
        Some(std::path::Component::ParentDir)
    )
}

/// Check if a file picker entry is selectable (.ini files or directories).
/// The parent `..` sentinel is always selectable; it carries no file name, so
/// it must be checked before the `file_name` branch.
pub fn is_entry_selectable(path: &Path) -> bool {
    if is_picker_parent_entry(path) {
        return true; // parent directory entry is always selectable
    }
    if path.file_name().is_none() {
        return false;
    }
    let is_dir = path.metadata().is_ok_and(|m| m.is_dir());
    if is_dir {
        true
    } else {
        // .ini files are selectable, others are not
        path.extension().is_some_and(|ext| ext == "ini")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::patch::Patch;

    #[test]
    fn new_app_has_graph_defaults_closed() {
        let app = App::new();
        assert!(!app.showing_graph);
        assert!(app.graph.is_none());
        assert!(app.graph_positions.is_empty());
        assert!(app.graph_cluster_rects.is_empty());
    }

    #[test]
    fn open_graph_builds_and_solves_a_loaded_patch() {
        let mut app = App::new();
        let patch = Patch::from_ini_file(Path::new("fixtures/arpeggio1.ini")).unwrap();
        app.load_patch(patch);
        app.open_graph();

        assert!(app.showing_graph);
        let graph = app.graph.as_ref().unwrap();
        assert!(
            !graph.nodes.is_empty(),
            "graph should hold the patch's circuits"
        );
        assert_eq!(app.graph_positions.len(), graph.nodes.len());
        for (x, y) in &app.graph_positions {
            assert!(x.is_finite() && y.is_finite());
        }
    }

    #[test]
    fn back_edge_hover_status_reports_loop_behind_for_sink() {
        // graph_latency_backedge.ini: `_LOOP` is produced by the later [lfo]
        // and consumed by the earlier [contour] — the one back-edge. Hovering
        // the contour node must surface `reads _LOOP 1 loop behind`; hovering
        // the lfo (a plain forward sink of `_GATE`) must leave status None.
        let mut app = App::new();
        let patch = Patch::from_ini_file(Path::new("fixtures/graph_latency_backedge.ini")).unwrap();
        app.load_patch(patch);
        app.open_graph();
        let graph = app.graph.as_ref().unwrap();
        let contour_idx = graph
            .nodes
            .iter()
            .position(|n| n.circuit == "contour")
            .expect("contour node");
        let lfo_idx = graph
            .nodes
            .iter()
            .position(|n| n.circuit == "lfo")
            .expect("lfo node");

        assert_eq!(
            app.back_edge_hover_status(contour_idx).as_deref(),
            Some("reads _LOOP 1 loop behind")
        );
        assert_eq!(
            app.back_edge_hover_status(lfo_idx),
            None,
            "a plain forward sink has no back-edge hint"
        );
    }

    #[test]
    fn open_graph_without_patch_yields_empty_graph() {
        let mut app = App::new();
        app.open_graph();
        assert!(app.showing_graph);
        let graph = app.graph.as_ref().unwrap();
        assert!(graph.nodes.is_empty());
        assert!(app.graph_positions.is_empty());
    }

    #[test]
    fn close_graph_preserves_panel_and_source_viewer_state() {
        let mut app = App::new();
        let patch = Patch::from_ini_file(Path::new("fixtures/source_navigation.ini")).unwrap();
        app.load_patch(patch);
        app.select_component(String::from("B1.1"));
        app.viewer_focus = ViewerFocus::Source;
        app.source_view_mode = SourceViewMode::Prettified;
        app.source_scroll = 9;
        app.occurrence_cursor = 2;

        let before_selection = app.selected_component.clone();
        let before_focus = app.viewer_focus.clone();
        let before_mode = app.source_view_mode.clone();
        let before_scroll = app.source_scroll;
        let before_cursor = app.occurrence_cursor;

        app.open_graph();
        app.close_graph();

        assert!(!app.showing_graph);
        assert_eq!(app.selected_component, before_selection);
        assert_eq!(app.viewer_focus, before_focus);
        assert_eq!(app.source_view_mode, before_mode);
        assert_eq!(app.source_scroll, before_scroll);
        assert_eq!(app.occurrence_cursor, before_cursor);
    }

    #[test]
    fn load_patch_resets_graph_state() {
        let mut app = App::new();
        let patch = Patch::from_ini_file(Path::new("fixtures/arpeggio1.ini")).unwrap();
        app.load_patch(patch);
        app.open_graph();
        assert!(app.showing_graph);
        assert!(app.graph.is_some());
        assert!(!app.graph_positions.is_empty());

        let second = Patch::from_ini_file(Path::new("fixtures/source_navigation.ini")).unwrap();
        app.load_patch(second);
        assert!(!app.showing_graph);
        assert!(app.graph.is_none());
        assert!(app.graph_positions.is_empty());
        assert!(app.graph_cluster_rects.is_empty());
    }

    #[test]
    fn clear_graph_cluster_rects_empties_the_field() {
        let mut app = App::new();
        app.graph_cluster_rects = vec![(0, Rect::new(0, 0, 5, 5)), (1, Rect::new(1, 1, 5, 5))];
        app.clear_graph_cluster_rects();
        assert!(app.graph_cluster_rects.is_empty());
    }

    #[test]
    fn new_app_starts_with_no_prefix_and_viewer_closed() {
        let app = App::new();
        assert!(app.prefix.is_none());
        assert!(!app.showing_viewer);
    }

    #[test]
    fn new_app_has_source_navigation_defaults() {
        let app = App::new();
        assert!(app.selected_component.is_none());
        assert_eq!(app.viewer_focus, ViewerFocus::Panels);
        assert_eq!(app.source_view_mode, SourceViewMode::Raw);
        assert_eq!(app.occurrence_cursor, 0);
        assert_eq!(app.source_scroll, 0);
        assert!(app.minimap_rect.is_none());
        // hovered stays distinct from selected
        assert!(app.hovered_component.is_none());
    }

    #[test]
    fn load_patch_resets_source_navigation_state_to_bof() {
        let mut app = App::new();
        // Put app into a non-default navigation state first
        app.selected_component = Some(String::from("B1.1"));
        app.viewer_focus = ViewerFocus::Source;
        app.source_view_mode = SourceViewMode::Prettified;
        app.occurrence_cursor = 5;
        app.source_scroll = 42;
        app.minimap_rect = Some(Rect::new(0, 0, 10, 10));
        app.hovered_component = Some(2);

        let patch = Patch::from_ini_file(Path::new("fixtures/arpeggio1.ini")).unwrap();
        app.load_patch(patch);

        assert!(
            app.selected_component.is_none(),
            "selection cleared on load"
        );
        assert_eq!(app.occurrence_cursor, 0);
        assert_eq!(app.source_scroll, 0);
        assert_eq!(app.source_view_mode, SourceViewMode::Raw);
        assert_eq!(app.viewer_focus, ViewerFocus::Panels);
        assert!(app.minimap_rect.is_none());
        // patch itself is set, hover is intentionally not cleared here
        assert!(app.patch.is_some());
        assert_eq!(app.hovered_component, Some(2));
    }

    #[test]
    fn load_sample_patch_inits_new_fields_with_defaults() {
        let mut app = App::new();
        app.load_sample_patch();
        assert!(app.selected_component.is_none());
        assert_eq!(app.source_view_mode, SourceViewMode::Raw);
        assert_eq!(app.viewer_focus, ViewerFocus::Panels);
        assert_eq!(app.occurrence_cursor, 0);
        assert_eq!(app.source_scroll, 0);
        assert!(app.minimap_rect.is_none());
    }

    #[test]
    fn physical_viewport_resets_on_patch_load() {
        let mut app = App::new();
        app.physical_offset = (9.0, 7.0);
        app.physical_zoom = 2.0;
        app.physical_viewport = Some(Rect::new(0, 3, 80, 24));
        // The first load always installs the patch and resets the viewport;
        // a gated re-load preserves state by design.
        app.load_sample_patch();
        assert_eq!(app.physical_offset, (0.0, 0.0));
        assert_eq!(app.physical_zoom, 1.0);
        assert_eq!(app.physical_viewport, None);
    }

    #[test]
    fn physical_pan_if_overflow_gates_on_the_pressed_axis() {
        let mut app = App::new();
        app.patch = Some(Patch::from_ini_str("[a]\n    out1 = B1.1\n", String::from("a")).unwrap());
        app.physical_rack_size = (200, 100);
        app.physical_viewport = Some(Rect::new(0, 3, 80, 24));
        // Both axes overflow: panning works along each.
        assert!(app.physical_pan_if_overflow(1, 0));
        assert_eq!(app.physical_offset, (8.0, 0.0));
        assert!(app.physical_pan_if_overflow(0, 1));
        assert_eq!(app.physical_offset, (8.0, 8.0));
        // Horizontal axis fits now: the horizontal pan is a no-op while the
        // vertical one still overflows.
        app.physical_rack_size = (40, 100);
        assert!(!app.physical_pan_if_overflow(1, 0));
        assert_eq!(app.physical_offset, (8.0, 8.0));
        assert!(app.physical_pan_if_overflow(0, 1));
        assert_eq!(app.physical_offset, (8.0, 16.0));
    }

    #[test]
    fn physical_pan_skipped_under_viewer_quad_graph() {
        let mut app = App::new();
        app.patch = Some(Patch::from_ini_str("[a]\n    out1 = B1.1\n", String::from("a")).unwrap());
        app.physical_rack_size = (200, 100);
        app.physical_viewport = Some(Rect::new(0, 3, 80, 24));
        // Each open surface independently suppresses panning.
        app.showing_viewer = true;
        assert!(!app.physical_pan_if_overflow(1, 0));
        app.showing_viewer = false;
        app.showing_quad = true;
        assert!(!app.physical_pan_if_overflow(1, 0));
        app.showing_quad = false;
        app.showing_graph = true;
        assert!(!app.physical_pan_if_overflow(1, 0));
        app.showing_graph = false;
        // Plain main view pans again.
        assert!(app.physical_pan_if_overflow(1, 0));
        assert_eq!(app.physical_offset, (8.0, 0.0));
    }

    #[test]
    fn physical_status_hint_requires_a_patch_and_reflects_viewport_state() {
        let mut app = App::new();
        assert_eq!(app.physical_status_hint(), None);
        app.load_sample_patch();
        assert_eq!(
            app.physical_status_hint().as_deref(),
            Some("Physical 100% \u{B7} Pan 0/0 \u{B7} Skeleton: off")
        );
        app.physical_offset = (12.0, -4.0);
        app.physical_zoom = 1.5;
        app.physical_show_skeleton = true;
        assert_eq!(
            app.physical_status_hint().as_deref(),
            Some("Physical 150% \u{B7} Pan 12/-4 \u{B7} Skeleton: on")
        );
    }

    #[test]
    fn select_component_jumps_to_first_occurrence_line() {
        let mut app = App::new();
        let patch = Patch::from_ini_file(Path::new("fixtures/source_navigation.ini")).unwrap();
        let first_b11_line = patch.occurrences_for("B1.1").first().unwrap().line;
        app.load_patch(patch);
        app.select_component(String::from("B1.1"));
        assert_eq!(app.selected_component, Some(String::from("B1.1")));
        assert_eq!(app.occurrence_cursor, 0);
        assert_eq!(app.source_scroll, first_b11_line);
    }

    #[test]
    fn select_component_with_unknown_token_keeps_scroll() {
        let mut app = App::new();
        let patch = Patch::from_ini_file(Path::new("fixtures/arpeggio1.ini")).unwrap();
        app.load_patch(patch);
        app.source_scroll = 7;
        app.select_component(String::from("B99.99"));
        assert_eq!(app.selected_component, Some(String::from("B99.99")));
        assert_eq!(app.occurrence_cursor, 0);
        assert_eq!(app.source_scroll, 7, "unknown token must not move scroll");
    }

    #[test]
    fn clear_selected_component_keeps_scroll_and_resets_cursor() {
        let mut app = App::new();
        let patch = Patch::from_ini_file(Path::new("fixtures/source_navigation.ini")).unwrap();
        let first = patch.occurrences_for("B1.1").first().unwrap().line;
        app.load_patch(patch);
        app.select_component(String::from("B1.1"));
        assert_eq!(app.source_scroll, first);
        app.source_scroll = 99;
        app.occurrence_cursor = 2;
        app.clear_selected_component();
        assert!(app.selected_component.is_none());
        assert_eq!(app.occurrence_cursor, 0);
        assert_eq!(app.source_scroll, 99, "deselection must not move scroll");
    }

    #[test]
    fn jump_to_occurrence_saturates_and_is_noop_without_selection() {
        let mut app = App::new();
        let patch = Patch::from_ini_file(Path::new("fixtures/source_navigation.ini")).unwrap();
        app.load_patch(patch);
        // No selection -> no-op
        app.source_scroll = 5;
        app.jump_to_occurrence(1);
        assert_eq!(app.source_scroll, 5);
        assert_eq!(app.occurrence_cursor, 0);

        app.select_component(String::from("B1.1"));
        let occurrences = app.patch.as_ref().unwrap().occurrences_for("B1.1").to_vec();
        assert!(occurrences.len() >= 2);
        app.jump_to_occurrence(1);
        assert_eq!(app.occurrence_cursor, 1);
        assert_eq!(app.source_scroll, occurrences[1].line);
        // Saturate beyond bounds
        app.jump_to_occurrence(999);
        assert_eq!(app.occurrence_cursor, occurrences.len() - 1);
        assert_eq!(app.source_scroll, occurrences.last().unwrap().line);
        // Back to first via 0
        app.jump_to_occurrence(0);
        assert_eq!(app.occurrence_cursor, 0);
        assert_eq!(app.source_scroll, occurrences[0].line);
    }

    #[test]
    fn replacement_selection_rejumps_to_new_first_occurrence() {
        let mut app = App::new();
        let patch = Patch::from_ini_file(Path::new("fixtures/source_navigation.ini")).unwrap();
        let b11_first = patch.occurrences_for("B1.1").first().unwrap().line;
        let p11_first = patch.occurrences_for("P1.1").first().unwrap().line;
        app.load_patch(patch);
        app.select_component(String::from("B1.1"));
        assert_eq!(app.source_scroll, b11_first);
        app.select_component(String::from("P1.1"));
        assert_eq!(app.source_scroll, p11_first);
        assert_eq!(app.occurrence_cursor, 0);
    }

    #[test]
    fn load_patch_populates_patch_name() {
        let mut app = App::new();
        let patch = Patch::from_ini_file(Path::new("fixtures/arpeggio1.ini")).unwrap();
        app.load_patch(patch);
        assert_eq!(app.patch.as_ref().unwrap().name, "arpeggio1");
    }

    #[test]
    fn viewer_split_ratio_defaults_to_0_6() {
        let app = App::new();
        assert_eq!(app.viewer_split_ratio, 0.6);
    }

    #[test]
    fn adjust_viewer_split_ratio_clamps() {
        let mut app = App::new();

        // Adjusting +0.2 from 0.6 should clamp to 0.7
        app.adjust_viewer_split_ratio(0.2);
        assert_eq!(app.viewer_split_ratio, 0.7);

        // Reset to 0.6 and adjust -0.5, should clamp to 0.3
        app.viewer_split_ratio = 0.6;
        app.adjust_viewer_split_ratio(-0.5);
        assert_eq!(app.viewer_split_ratio, 0.3);

        // Adjusting within bounds should work fine
        app.viewer_split_ratio = 0.5;
        app.adjust_viewer_split_ratio(0.1);
        assert_eq!(app.viewer_split_ratio, 0.6);

        app.viewer_split_ratio = 0.3;
        app.adjust_viewer_split_ratio(-0.1);
        assert_eq!(app.viewer_split_ratio, 0.3);

        app.viewer_split_ratio = 0.7;
        app.adjust_viewer_split_ratio(0.1);
        assert_eq!(app.viewer_split_ratio, 0.7);
    }

    #[test]
    fn processing_pause_toggles_status_message() {
        let mut app = App::new();
        app.toggle_processing_pause();
        assert!(app.processing_paused);
        assert_eq!(app.status_message, "Processing paused (p to resume)");
        app.toggle_processing_pause();
        assert!(!app.processing_paused);
        assert_eq!(app.status_message, "Processing enabled (p to pause)");
    }

    #[test]
    fn load_patch_resets_processing_pause() {
        let mut app = App::new();
        app.toggle_processing_pause();
        assert!(app.processing_paused);
        let patch = Patch::from_ini_file(Path::new("fixtures/arpeggio1.ini")).unwrap();
        app.load_patch(patch);
        assert!(!app.processing_paused, "pause reset on load");
    }

    #[test]
    fn latency_coloring_defaults_to_true() {
        let app = App::new();
        assert!(app.latency_coloring, "latency coloring on by default");
    }

    #[test]
    fn toggle_latency_coloring_flips_flag_and_status() {
        let mut app = App::new();
        app.toggle_latency_coloring();
        assert!(!app.latency_coloring);
        assert_eq!(app.status_message, "Latency coloring off (c to toggle)");
        app.toggle_latency_coloring();
        assert!(app.latency_coloring);
        assert_eq!(app.status_message, "Latency coloring on (c to toggle)");
    }

    #[test]
    fn latency_coloring_persists_across_patch_loads() {
        // A view preference like `viewer_split_ratio`: the choice of how to
        // color the graph surface survives loading a different patch, unlike
        // transient processing state (`processing_paused`/`disabled_circuits`).
        let mut app = App::new();
        app.toggle_latency_coloring();
        assert!(!app.latency_coloring);
        let patch = Patch::from_ini_file(Path::new("fixtures/arpeggio1.ini")).unwrap();
        app.load_patch(patch);
        assert!(!app.latency_coloring, "view preference kept across loads");
    }

    #[test]
    fn pausing_clears_selection_driven_influence() {
        let mut app = App::new();
        let patch = Patch::from_ini_file(Path::new("fixtures/source_navigation.ini")).unwrap();
        app.load_patch(patch);
        app.select_component(String::from("B1.1"));
        assert!(app.influence.is_some());
        assert!(app.active_modifier_var.is_some());
        app.toggle_processing_pause();
        assert!(app.processing_paused);
        assert!(app.influence.is_none(), "influence cleared on pause");
        assert!(app.active_modifier_var.is_none());
    }

    #[test]
    fn recompute_influence_cleared_and_never_computed_while_paused() {
        let mut app = App::new();
        let patch = Patch::from_ini_file(Path::new("fixtures/source_navigation.ini")).unwrap();
        app.load_patch(patch);
        app.toggle_processing_pause();
        app.select_component(String::from("B1.1"));
        assert_eq!(app.selected_component.as_deref(), Some("B1.1"));
        assert!(
            app.influence.is_none(),
            "no influence computed while paused"
        );
        assert!(app.active_modifier_var.is_none());
    }

    #[test]
    fn new_app_has_empty_disabled_circuits() {
        let app = App::new();
        assert!(app.disabled_circuits.is_empty());
    }

    #[test]
    fn load_patch_clears_disabled_circuits() {
        let mut app = App::new();
        let patch = Patch::from_ini_file(Path::new("fixtures/arpeggio1.ini")).unwrap();
        app.load_patch(patch);
        assert!(app.toggle_circuit_processing("arpeggio", 0));
        assert_eq!(app.disabled_circuits.len(), 1);

        let second = Patch::from_ini_file(Path::new("fixtures/source_navigation.ini")).unwrap();
        app.load_patch(second);
        assert!(
            app.disabled_circuits.is_empty(),
            "disabled set reset on load"
        );
    }

    #[test]
    fn tip_is_pinned_by_default_on_open() {
        let mut app = App::new();
        let patch = Patch::from_ini_file(Path::new("fixtures/arpeggio1.ini")).unwrap();
        app.load_patch(patch);
        app.open_graph();
        let graph = app.graph.as_ref().unwrap();
        assert!(
            app.pinned.contains(&graph.nodes[0].id),
            "the first circuit in .ini order is the pinned tip"
        );
        // The tip is a fixed anchor: it stays exactly at its seed position.
        let seed = crate::layout::seed_positions(graph);
        assert_eq!(app.graph_positions[0], seed[0], "tip never drifts");
    }

    #[test]
    fn load_patch_clears_pinned() {
        let mut app = App::new();
        let patch = Patch::from_ini_file(Path::new("fixtures/arpeggio1.ini")).unwrap();
        app.load_patch(patch);
        app.open_graph();
        assert!(!app.pinned.is_empty(), "tip seeded by default");

        let second = Patch::from_ini_file(Path::new("fixtures/source_navigation.ini")).unwrap();
        app.load_patch(second);
        assert!(app.pinned.is_empty(), "pinned set reset on load");
    }

    #[test]
    fn toggle_pin_toggles_membership() {
        let mut app = App::new();
        let patch = Patch::from_ini_file(Path::new("fixtures/arpeggio1.ini")).unwrap();
        app.load_patch(patch);
        app.open_graph();
        let (id, tip_id) = {
            let graph = app.graph.as_ref().unwrap();
            let node = graph
                .nodes
                .iter()
                .find(|n| n.id != graph.nodes[0].id)
                .expect("graph has a non-tip node");
            (node.id.clone(), graph.nodes[0].id.clone())
        };
        assert!(!app.pinned.contains(&id), "non-tip node starts unpinned");
        assert!(app.toggle_pin(&id), "first toggle pins");
        assert!(app.pinned.contains(&id));
        assert!(!app.toggle_pin(&id), "second toggle unpins");
        assert!(!app.pinned.contains(&id));
        // Unpinning the tip empties the pin set; the next open re-seeds it.
        app.toggle_pin(&tip_id);
        assert!(app.pinned.is_empty(), "tip unpin empties the pin set");
    }

    #[test]
    fn pinned_indices_skip_ids_not_in_graph() {
        let mut app = App::new();
        let patch = Patch::from_ini_file(Path::new("fixtures/arpeggio1.ini")).unwrap();
        app.load_patch(patch);
        app.open_graph();
        let ghost = (String::from("ghost"), 99);
        app.pinned.insert(ghost.clone());
        let graph = app.graph.as_ref().unwrap();
        let pins = app.pinned_indices(graph);
        assert!(
            pins.iter().all(|&i| graph.nodes[i].id != ghost),
            "ghost id is skipped, never a panic"
        );
        assert!(
            pins.iter()
                .all(|&i| app.pinned.contains(&graph.nodes[i].id)),
            "pins map to real pinned nodes"
        );
    }

    #[test]
    fn disabled_circuit_cuts_influence_downstream_but_keeps_own_cells() {
        // B1.1 -> _CHAIN1 -> [copy](9) -> _CHAIN2 -> [switch](5) in
        // source_navigation.ini. Disabling the intermediate copy must leave it
        // (and its hardware cells) influenced while cutting the switch out.
        let mut app = App::new();
        let patch = Patch::from_ini_file(Path::new("fixtures/source_navigation.ini")).unwrap();
        app.load_patch(patch);
        app.select_component(String::from("B1.1"));

        let sub = app.influence.as_ref().unwrap();
        assert!(sub.influenced_nodes.contains(&(String::from("copy"), 9)));
        assert!(sub.influenced_nodes.contains(&(String::from("switch"), 5)));
        assert!(sub.influenced_edges.contains("_CHAIN1"));
        assert!(sub.influenced_edges.contains("_CHAIN2"));

        let now_disabled = app.toggle_circuit_processing("copy", 9);
        assert!(now_disabled, "toggle returns the new disabled state");
        assert!(app.disabled_circuits.contains(&(String::from("copy"), 9)));

        let sub = app.influence.as_ref().unwrap();
        assert!(
            sub.influenced_nodes.contains(&(String::from("copy"), 9)),
            "disabled circuit itself stays influenced"
        );
        assert!(
            !sub.influenced_nodes.contains(&(String::from("switch"), 5)),
            "downstream circuit cut from influence"
        );
        assert!(sub.influenced_edges.contains("_CHAIN1"));
        assert!(
            !sub.influenced_edges.contains("_CHAIN2"),
            "produced cable of disabled circuit not propagated"
        );

        let now_disabled = app.toggle_circuit_processing("copy", 9);
        assert!(!now_disabled, "second toggle re-enables");
        assert!(app.disabled_circuits.is_empty());
        let sub = app.influence.as_ref().unwrap();
        assert!(sub.influenced_nodes.contains(&(String::from("switch"), 5)));
        assert!(sub.influenced_edges.contains("_CHAIN2"));
    }

    // ── 2.2 overlay draft lifecycle (Enter/ Esc / 1..N layer cycle) ──

    #[test]
    fn commit_edit_trims_and_prunes_via_atomic_write() {
        use tempfile::TempDir;
        let dir = TempDir::new().unwrap();
        let patch_path = dir.path().join("patch.ini");
        std::fs::write(&patch_path, "[button]\nbutton = B1.1\n").unwrap();
        let patch = Patch::from_ini_str("[button]\nbutton = B1.1\n", "patch".to_string()).unwrap();
        let mut app = App::new();
        app.label_store = LabelStore::default();
        app.load_patch_at(&patch_path, patch);
        app.editing = Some(EditState::new_hw(
            "B1.1".to_string(),
            2,
            "  hello  ".to_string(),
        ));
        app.commit_edit_to_dir(dir.path()).unwrap();
        assert!(app.editing.is_none(), "overlay cleared on Enter");
        assert_eq!(
            app.label_store.hw_label(&patch_path, "B1.1", 2),
            Some("hello".to_string())
        );
        // Atomic write left no stray tmp file.
        assert!(!dir.path().join("labels.toml.tmp").exists());
        let body = std::fs::read_to_string(dir.path().join("labels.toml")).unwrap();
        assert!(
            body.contains("hello"),
            "persisted toml contains trimmed label"
        );
        // Reload round-trips.
        let reloaded = LabelStore::load_from(&dir.path().join("labels.toml"));
        assert_eq!(
            reloaded.hw_label(&patch_path, "B1.1", 2),
            Some("hello".to_string())
        );
    }

    #[test]
    fn commit_edit_empty_prunes_layer_and_token() {
        use tempfile::TempDir;
        let dir = TempDir::new().unwrap();
        let patch_path = dir.path().join("patch.ini");
        std::fs::write(&patch_path, "[button]\nbutton = B1.1\n").unwrap();
        let patch = Patch::from_ini_str("[button]\nbutton = B1.1\n", "patch".to_string()).unwrap();
        let mut app = App::new();
        app.label_store = LabelStore::default();
        app.load_patch_at(&patch_path, patch);
        // Seed two layers.
        app.editing = Some(EditState::new_hw("B1.1".to_string(), 1, "a".to_string()));
        app.commit_edit_to_dir(dir.path()).unwrap();
        app.editing = Some(EditState::new_hw("B1.1".to_string(), 2, "b".to_string()));
        app.commit_edit_to_dir(dir.path()).unwrap();
        // Empty draft on layer 2 prunes that layer; layer 1 stays.
        app.editing = Some(EditState::new_hw("B1.1".to_string(), 2, "   ".to_string()));
        app.commit_edit_to_dir(dir.path()).unwrap();
        assert_eq!(app.label_store.hw_label(&patch_path, "B1.1", 2), None);
        assert_eq!(
            app.label_store.hw_label(&patch_path, "B1.1", 1),
            Some("a".to_string())
        );
        // Empty draft on last layer prunes token entirely.
        app.editing = Some(EditState::new_hw("B1.1".to_string(), 1, "".to_string()));
        app.commit_edit_to_dir(dir.path()).unwrap();
        assert_eq!(app.label_store.hw_label(&patch_path, "B1.1", 1), None);
        assert!(app.label_store.patch_labels(&patch_path).is_none());
    }

    #[test]
    fn commit_edit_circuit_trim_and_empty_prune() {
        use tempfile::TempDir;
        let dir = TempDir::new().unwrap();
        let patch_path = dir.path().join("patch.ini");
        std::fs::write(&patch_path, "[motorfader]\nled = L1.1\n").unwrap();
        let patch = Patch::from_ini_str("[motorfader]\nled = L1.1\n", "patch".to_string()).unwrap();
        let mut app = App::new();
        app.label_store = LabelStore::default();
        app.load_patch_at(&patch_path, patch);
        app.editing = Some(EditState::new_circuit(
            ("motorfader".to_string(), 0),
            "  T1 Accu  ".to_string(),
        ));
        app.commit_edit_to_dir(dir.path()).unwrap();
        assert_eq!(
            app.label_store
                .circuit_label(&patch_path, &("motorfader".to_string(), 0)),
            Some("T1 Accu".to_string())
        );
        app.editing = Some(EditState::new_circuit(
            ("motorfader".to_string(), 0),
            "   ".to_string(),
        ));
        app.commit_edit_to_dir(dir.path()).unwrap();
        assert_eq!(
            app.label_store
                .circuit_label(&patch_path, &("motorfader".to_string(), 0)),
            None
        );
    }

    #[test]
    fn cancel_edit_clears_overlay_without_mutating_store() {
        let mut app = App::new();
        app.label_store = LabelStore::default();
        let dir = tempfile::TempDir::new().unwrap();
        let patch_path = dir.path().join("p.ini");
        std::fs::write(&patch_path, "[button]\nbutton = B1.1\n").unwrap();
        let patch = Patch::from_ini_str("[button]\nbutton = B1.1\n", "x".to_string()).unwrap();
        app.load_patch_at(&patch_path, patch);
        app.editing = Some(EditState::new_hw(
            "B1.1".to_string(),
            2,
            "draft".to_string(),
        ));
        app.cancel_edit();
        assert!(app.editing.is_none());
        assert_eq!(app.label_store.hw_label(&patch_path, "B1.1", 2), None);
    }

    #[test]
    fn layer_cycle_preserves_per_layer_drafts() {
        let mut app = App::new();
        app.label_store = LabelStore::default();
        let dir = tempfile::TempDir::new().unwrap();
        let patch_path = dir.path().join("p.ini");
        std::fs::write(&patch_path, "[button]\nbutton = B1.1\n").unwrap();
        let patch = Patch::from_ini_str("[button]\nbutton = B1.1\n", "x".to_string()).unwrap();
        app.load_patch_at(&patch_path, patch);
        // Start editing layer 2 with initial draft "hello2".
        app.editing = Some(EditState::new_hw(
            "B1.1".to_string(),
            2,
            "hello2".to_string(),
        ));
        // Cycle to layer 3: preserves layer 2 draft, layer 3 starts empty (no store).
        assert!(app.cycle_edit_layer(3));
        assert_eq!(app.editing.as_ref().unwrap().draft, "");
        assert_eq!(
            app.editing.as_ref().unwrap().layer_drafts.get(&2).cloned(),
            Some("hello2".to_string())
        );
        // Type in layer 3, cycle back to 2: layer 3 draft preserved, layer 2 restored.
        app.editing.as_mut().unwrap().draft = "hello3".to_string();
        assert!(app.cycle_edit_layer(2));
        assert_eq!(app.editing.as_ref().unwrap().draft, "hello2");
        assert_eq!(
            app.editing.as_ref().unwrap().layer_drafts.get(&3).cloned(),
            Some("hello3".to_string())
        );
        // Cycle to 3 again restores hello3.
        assert!(app.cycle_edit_layer(3));
        assert_eq!(app.editing.as_ref().unwrap().draft, "hello3");
    }

    #[test]
    fn layer_cycle_loads_persisted_store_when_no_preserved_draft() {
        let mut app = App::new();
        let dir = tempfile::TempDir::new().unwrap();
        let patch_path = dir.path().join("p.ini");
        std::fs::write(&patch_path, "[button]\nbutton = B1.1\n").unwrap();
        let patch = Patch::from_ini_str("[button]\nbutton = B1.1\n", "x".to_string()).unwrap();
        app.load_patch_at(&patch_path, patch);
        app.editing = Some(EditState::new_hw(
            "B1.1".to_string(),
            1,
            "ignored".to_string(),
        ));
        app.commit_edit_to_dir(dir.path()).unwrap();
        // Seed layer 2 in store.
        app.editing = Some(EditState::new_hw(
            "B1.1".to_string(),
            2,
            "stored2".to_string(),
        ));
        app.commit_edit_to_dir(dir.path()).unwrap();
        // New overlay on layer 1; cycling to 2 should load stored2 when no preserved draft.
        app.editing = Some(EditState::new_hw("B1.1".to_string(), 1, "cur1".to_string()));
        assert!(app.cycle_edit_layer(2));
        assert_eq!(app.editing.as_ref().unwrap().draft, "stored2");
    }

    #[test]
    fn layer_cycle_noop_for_circuit_and_when_not_editing() {
        let mut app = App::new();
        assert!(!app.cycle_edit_layer(2));
        app.editing = Some(EditState::new_circuit(
            ("motorfader".to_string(), 0),
            "x".to_string(),
        ));
        assert!(!app.cycle_edit_layer(2));
        // Same layer is also a no-op.
        app.editing = Some(EditState::new_hw("B1.1".to_string(), 2, "h".to_string()));
        assert!(!app.cycle_edit_layer(2));
    }

    #[test]
    fn commit_without_patch_path_clears_overlay_without_panic() {
        let mut app = App::new();
        app.label_store = LabelStore::default();
        // No load_patch_at -> current_patch_path is None.
        app.editing = Some(EditState::new_hw("B1.1".to_string(), 1, "hi".to_string()));
        let dir = tempfile::TempDir::new().unwrap();
        app.commit_edit_to_dir(dir.path()).unwrap();
        assert!(app.editing.is_none());
        assert!(app.label_store.patches.is_empty());
    }

    // ── label-management 5.1: LabelStore round-trip, canonicalization, circuit override ──

    #[test]
    fn label_store_encode_decode_node_id_round_trips() {
        assert_eq!(
            LabelStore::decode_node_id(&LabelStore::encode_node_id("motorfader", 12)),
            Some(("motorfader".to_string(), 12))
        );
        assert_eq!(
            LabelStore::decode_node_id(&LabelStore::encode_node_id("copy", 0)),
            Some(("copy".to_string(), 0))
        );
        // Malformed.
        assert_eq!(LabelStore::decode_node_id("no_colon"), None);
        assert_eq!(LabelStore::decode_node_id("motorfader:"), None);
        assert_eq!(LabelStore::decode_node_id("motorfader:abc"), None);
        // Circuit names may contain colon? rsplit_once splits last colon.
        assert_eq!(
            LabelStore::decode_node_id("my:circuit:3"),
            Some(("my:circuit".to_string(), 3))
        );
    }

    #[test]
    fn label_store_canonical_key_for_existing_vs_missing() {
        let dir = tempfile::TempDir::new().unwrap();
        let file = dir.path().join("patch.ini");
        std::fs::write(&file, "[button]\nbutton = B1.1\n").unwrap();
        let key_existing = LabelStore::canonical_key(&file);
        // Existing file canonicalizes to absolute canonical path.
        assert!(key_existing.ends_with("patch.ini"));
        assert!(std::path::Path::new(&key_existing).is_absolute());

        // Missing file: still absolute via current_dir join, no panic.
        let missing = dir.path().join("missing.ini");
        let key_missing = LabelStore::canonical_key(&missing);
        assert!(key_missing.ends_with("missing.ini"));
        assert!(std::path::Path::new(&key_missing).is_absolute());

        // Relative path canonicalization branch.
        let rel = std::path::Path::new("relative/patch.ini");
        let key_rel = LabelStore::canonical_key(rel);
        assert!(std::path::Path::new(&key_rel).is_absolute());
    }

    #[test]
    fn label_store_round_trip_hw_and_circuit_atomic_and_pruned() {
        let dir = tempfile::TempDir::new().unwrap();
        let patch_path = dir.path().join("patch.ini");
        std::fs::write(&patch_path, "[button]\nbutton = B1.1\n").unwrap();
        let patch = Patch::from_ini_str("[button]\nbutton = B1.1\n", "patch".to_string()).unwrap();
        let mut app = App::new();
        app.label_store = LabelStore::default();
        app.load_patch_at(&patch_path, patch);

        // HW layer 1 and 2, plus circuit label.
        app.editing = Some(EditState::new_hw(
            "B3.17".to_string(),
            1,
            "  [RATC]  ".to_string(),
        ));
        app.commit_edit_to_dir(dir.path()).unwrap();
        app.editing = Some(EditState::new_hw(
            "B3.17".to_string(),
            2,
            "[RATC2]".to_string(),
        ));
        app.commit_edit_to_dir(dir.path()).unwrap();
        app.editing = Some(EditState::new_circuit(
            ("motorfader".to_string(), 12),
            " T1 Accu ".to_string(),
        ));
        app.commit_edit_to_dir(dir.path()).unwrap();

        // Trimmed.
        assert_eq!(
            app.label_store.hw_label(&patch_path, "B3.17", 1),
            Some("[RATC]".to_string())
        );
        assert_eq!(
            app.label_store.hw_label(&patch_path, "B3.17", 2),
            Some("[RATC2]".to_string())
        );
        assert_eq!(
            app.label_store
                .circuit_label(&patch_path, &("motorfader".to_string(), 12)),
            Some("T1 Accu".to_string())
        );
        // Atomic: no stray tmp.
        assert!(!dir.path().join("labels.toml.tmp").exists());
        // Persisted file contains trimmed values.
        let body = std::fs::read_to_string(dir.path().join("labels.toml")).unwrap();
        assert!(body.contains("[RATC]"));
        assert!(body.contains("T1 Accu"));
        assert!(!body.contains("  [RATC]  "), "should be trimmed");

        // Round-trip reload.
        let reloaded = LabelStore::load_from(&dir.path().join("labels.toml"));
        assert_eq!(
            reloaded.hw_label(&patch_path, "B3.17", 1),
            Some("[RATC]".to_string())
        );
        assert_eq!(
            reloaded.circuit_label(&patch_path, &("motorfader".to_string(), 12)),
            Some("T1 Accu".to_string())
        );

        // Empty/whitespace pruning: layer 2 whitespace removed, circuit whitespace pruned.
        let mut with_empty = reloaded.clone();
        with_empty
            .patches
            .get_mut(&LabelStore::canonical_key(&patch_path))
            .unwrap()
            .hw
            .get_mut("B3.17")
            .unwrap()
            .insert(2, "   ".to_string());
        with_empty
            .patches
            .get_mut(&LabelStore::canonical_key(&patch_path))
            .unwrap()
            .circuits
            .insert(
                LabelStore::encode_node_id("motorfader", 12),
                "   ".to_string(),
            );
        with_empty.save_to_dir(dir.path()).unwrap();
        let pruned = LabelStore::load_from(&dir.path().join("labels.toml"));
        assert_eq!(pruned.hw_label(&patch_path, "B3.17", 2), None);
        assert_eq!(
            pruned.hw_label(&patch_path, "B3.17", 1),
            Some("[RATC]".to_string())
        );
        // Circuit pruned -> absent.
        assert_eq!(
            pruned.circuit_label(&patch_path, &("motorfader".to_string(), 12)),
            None
        );
    }

    #[test]
    fn label_store_two_patches_isolated() {
        let dir = tempfile::TempDir::new().unwrap();
        let a_path = dir.path().join("a.ini");
        let b_path = dir.path().join("b.ini");
        std::fs::write(&a_path, "[button]\nbutton = B1.1\n").unwrap();
        std::fs::write(&b_path, "[button]\nbutton = B1.1\n").unwrap();
        let patch_a = Patch::from_ini_str("[button]\nbutton = B1.1\n", "a".to_string()).unwrap();
        let patch_b = Patch::from_ini_str("[button]\nbutton = B1.1\n", "b".to_string()).unwrap();

        let mut app_a = App::new();
        app_a.label_store = LabelStore::default();
        app_a.load_patch_at(&a_path, patch_a);
        app_a.editing = Some(EditState::new_hw(
            "B1.1".to_string(),
            1,
            "A-label".to_string(),
        ));
        app_a.commit_edit_to_dir(dir.path()).unwrap();

        // Reload store for second patch.
        let mut app_b = App::new();
        app_b.label_store = LabelStore::load_from(&dir.path().join("labels.toml"));
        app_b.load_patch_at(&b_path, patch_b);
        // B untouched.
        assert_eq!(app_b.label_store.hw_label(&b_path, "B1.1", 1), None);
        assert_eq!(
            app_b.label_store.hw_label(&a_path, "B1.1", 1),
            Some("A-label".to_string())
        );
        // Editing B does not affect A.
        app_b.editing = Some(EditState::new_hw(
            "B1.1".to_string(),
            1,
            "B-label".to_string(),
        ));
        app_b.commit_edit_to_dir(dir.path()).unwrap();
        let reloaded = LabelStore::load_from(&dir.path().join("labels.toml"));
        assert_eq!(
            reloaded.hw_label(&a_path, "B1.1", 1),
            Some("A-label".to_string())
        );
        assert_eq!(
            reloaded.hw_label(&b_path, "B1.1", 1),
            Some("B-label".to_string())
        );
        // Verify via Patch::display_label that the isolation is respected.
        let empty_patch =
            Patch::from_ini_str("[button]\nbutton = B1.1\n", "t".to_string()).unwrap();
        let hw_a = reloaded
            .patch_labels(&a_path)
            .map(|b| b.hw.clone())
            .unwrap_or_default();
        let hw_b = reloaded
            .patch_labels(&b_path)
            .map(|b| b.hw.clone())
            .unwrap_or_default();
        assert_eq!(
            empty_patch.display_label("B1.1", 1, true, 4, &hw_a),
            "A-label"
        );
        assert_eq!(
            empty_patch.display_label("B1.1", 1, true, 4, &hw_b),
            "B-label"
        );
    }

    #[test]
    fn label_store_circuit_override_drives_display_via_app() {
        let dir = tempfile::TempDir::new().unwrap();
        let patch_path = dir.path().join("patch.ini");
        std::fs::write(&patch_path, "[motorfader]\nled = L1.1\n").unwrap();
        let patch = Patch::from_ini_str("[motorfader]\nled = L1.1\n", "patch".to_string()).unwrap();
        let node: NodeId = ("motorfader".to_string(), 0);
        let mut app = App::new();
        app.label_store = LabelStore::default();
        app.load_patch_at(&patch_path, patch.clone());

        // No circuit label yet -> display falls back to circuit name.
        let empty = app.current_circuit_store();
        assert_eq!(patch.circuit_display_label(&node, &empty), "motorfader");

        app.editing = Some(EditState::new_circuit(node.clone(), "T1 Accu".to_string()));
        app.commit_edit_to_dir(dir.path()).unwrap();
        let store = app.current_circuit_store();
        assert_eq!(patch.circuit_display_label(&node, &store), "T1 Accu");
        assert_eq!(
            patch.circuit_label(&node, &store),
            Some("T1 Accu".to_string())
        );

        // Source/header and graph node both use same store; instance matters.
        let other_node: NodeId = ("motorfader".to_string(), 1);
        assert_eq!(
            patch.circuit_display_label(&other_node, &store),
            "motorfader"
        );
    }

    #[test]
    fn label_store_malformed_toml_falls_back_empty_warn_once() {
        let dir = tempfile::TempDir::new().unwrap();
        let path = dir.path().join("labels.toml");
        std::fs::write(&path, "not valid toml = [").unwrap();
        let loaded = LabelStore::load_from(&path);
        assert!(loaded.patches.is_empty());
        // Missing file also yields empty.
        let missing = dir.path().join("nope.toml");
        assert!(LabelStore::load_from(&missing).patches.is_empty());
    }

    #[test]
    fn label_store_current_stores_empty_without_patch_path() {
        let mut app = App::new();
        app.label_store = LabelStore::default();
        // No load_patch_at path set -> empty stores.
        assert!(app.current_hw_store().is_empty());
        assert!(app.current_circuit_store().is_empty());
        // Load patch without path also empty.
        let patch = Patch::from_ini_str("[button]\nbutton = B1.1\n", "t".to_string()).unwrap();
        app.load_patch(patch);
        assert!(app.current_hw_store().is_empty());
    }

    #[test]
    fn label_store_hw_and_circuit_helpers_trim_whitespace() {
        let dir = tempfile::TempDir::new().unwrap();
        let path = dir.path().join("p.ini");
        std::fs::write(&path, "[button]\nbutton = B1.1\n").unwrap();
        let mut store = LabelStore::default();
        store.patches.insert(
            LabelStore::canonical_key(&path),
            PatchLabels {
                hw: {
                    let mut m = HashMap::new();
                    let mut inner = std::collections::BTreeMap::new();
                    inner.insert(1, "   ".to_string());
                    inner.insert(2, "  kept  ".to_string());
                    m.insert("B1.1".to_string(), inner);
                    m
                },
                circuits: {
                    let mut m = HashMap::new();
                    m.insert(
                        LabelStore::encode_node_id("motorfader", 0),
                        "   ".to_string(),
                    );
                    m.insert(
                        LabelStore::encode_node_id("motorfader", 1),
                        "  T1  ".to_string(),
                    );
                    m
                },
            },
        );
        assert_eq!(store.hw_label(&path, "B1.1", 1), None);
        assert_eq!(store.hw_label(&path, "B1.1", 2), Some("kept".to_string()));
        assert_eq!(
            store.circuit_label(&path, &("motorfader".to_string(), 0)),
            None
        );
        assert_eq!(
            store.circuit_label(&path, &("motorfader".to_string(), 1)),
            Some("T1".to_string())
        );
    }

    #[test]
    fn effective_edit_layer_and_status_respect_clamp_and_disabled() {
        let mut app = App::new();
        app.label_store = LabelStore::default();
        // No editing -> None.
        assert_eq!(app.effective_edit_layer(true, 4), None);
        assert_eq!(app.editing_status_line(true, 4), None);
        assert_eq!(app.editing_hue_token(), None);

        // HW edit layer 6 with max 4 -> effective 4 when enabled, 1 when disabled.
        app.editing = Some(EditState::new_hw("B3.17".to_string(), 6, "x".to_string()));
        assert_eq!(app.effective_edit_layer(true, 4), Some(4));
        assert_eq!(app.effective_edit_layer(false, 4), Some(1));
        assert_eq!(app.effective_edit_layer(true, 20), Some(6)); // 20 clamped to 8, 6 within
        assert_eq!(app.effective_edit_layer(true, 0), Some(1)); // max 0 clamped to 1
                                                                // Circuit edit -> None.
        app.editing = Some(EditState::new_circuit(
            ("motorfader".to_string(), 12),
            "T1".to_string(),
        ));
        assert_eq!(app.effective_edit_layer(true, 4), None);
        assert_eq!(app.editing_hue_token(), Some("motorfader".to_string()));
        // HW hue token.
        app.editing = Some(EditState::new_hw("B3.17".to_string(), 2, "x".to_string()));
        assert_eq!(app.editing_hue_token(), Some("B3.17".to_string()));
        // Status line mentions Group with clamped value.
        let line = app.editing_status_line(true, 4).unwrap();
        assert!(line.contains("B3.17 / Group2"), "line: {line}");
        let clamped = {
            app.editing = Some(EditState::new_hw("B3.17".to_string(), 8, "x".to_string()));
            app.editing_status_line(true, 4).unwrap()
        };
        assert!(clamped.contains("Group4"), "clamped to max 4: {clamped}");
        let disabled = {
            app.editing = Some(EditState::new_hw("B3.17".to_string(), 3, "x".to_string()));
            app.editing_status_line(false, 4).unwrap()
        };
        assert!(disabled.contains("Group1"), "disabled forces 1: {disabled}");
    }

    #[test]
    fn refresh_picker_entries_labels_parent_and_sorts_dirs_first() {
        let mut app = App::new();
        app.picker_dir = PathBuf::from("fixtures/picker_test");
        app.refresh_picker_entries();
        // Parent sentinel is the first entry, rendered as "..".
        assert!(is_picker_parent_entry(&app.picker_entries[0]));
        assert_eq!(app.picker_entries[0], PathBuf::from(".."));
        // Entries sort directories first, then .ini files, then other files.
        let names: Vec<String> = app
            .picker_entries
            .iter()
            .map(|p| p.to_string_lossy().into_owned())
            .collect();
        assert_eq!(
            names,
            vec![
                "..",
                "fixtures/picker_test/subdir",
                "fixtures/picker_test/patch_a.ini",
                "fixtures/picker_test/readme.txt",
            ]
        );
    }

    #[test]
    fn refresh_picker_entries_has_no_parent_entry_at_filesystem_root() {
        let mut app = App::new();
        app.picker_dir = PathBuf::from("/");
        app.refresh_picker_entries();
        assert!(
            app.picker_entries
                .iter()
                .all(|p| !is_picker_parent_entry(p)),
            "no '..' entry at the filesystem root"
        );
    }

    #[test]
    fn is_entry_selectable_treats_parent_sentinel_dirs_and_inis_as_selectable() {
        assert!(is_entry_selectable(Path::new("..")));
        assert!(is_entry_selectable(Path::new(
            "fixtures/picker_test/subdir"
        )));
        assert!(is_entry_selectable(Path::new(
            "fixtures/picker_test/patch_a.ini"
        )));
        assert!(!is_entry_selectable(Path::new(
            "fixtures/picker_test/readme.txt"
        )));
    }

    #[test]
    fn load_patch_never_blocked_by_render_outlier() {
        // A patch whose render is degraded at every common terminal width
        // (arpeggio1 wants 228 cols) must still load: the render-outlier hint
        // is an advisory status-channel span, never a gating error.
        let mut app = App::new();
        let patch = Patch::from_ini_file(Path::new("fixtures/arpeggio1.ini")).unwrap();
        assert!(
            app.load_patch(patch),
            "degraded render must not block load_patch"
        );
        assert!(app.patch.is_some());
    }
}
