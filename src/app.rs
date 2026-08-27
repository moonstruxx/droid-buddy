use std::path::{Path, PathBuf};
use std::time::Instant;

use ratatui::layout::Rect;

use crate::events::{Event, EventBus};
use crate::graph::{Cluster, Graph, NodeId};
use crate::layout;
use crate::patch::Patch;
use crate::patch::ShiftGroup;

/// State of an armed vim-style prefix key (`g` pressed, awaiting the
/// follow-up key). `started` drives the lazy timeout check performed when
/// the next event arrives.
pub struct PrefixState {
    pub started: Instant,
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
            active_modifier_var: None,
            influence: None,
            filtered_graph: None,
            filtered_positions: Vec::new(),
            filtered_cluster_rects: Vec::new(),
            filtered_node_rects: Vec::new(),
            filtered_drag: None,
        }
    }

    pub fn refresh_picker_entries(&mut self) {
        self.picker_entries.clear();
        // Add ".." for parent directory (unless at root)
        if let Some(parent) = self.picker_dir.parent() {
            self.picker_entries.push(parent.to_path_buf());
        }
        // Read directory entries
        if let Ok(entries) = std::fs::read_dir(&self.picker_dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                self.picker_entries.push(path);
            }
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
    pub fn load_patch(&mut self, patch: Patch) {
        self.reset_graph_state();
        self.reset_quad_state();
        self.patch = Some(patch);
        self.selected_component = None;
        self.occurrence_cursor = 0;
        self.source_scroll = 0;
        self.source_view_mode = SourceViewMode::Raw;
        self.viewer_focus = ViewerFocus::Panels;
        self.minimap_rect = None;
        self.source_pane_rect = None;
    }

    /// Build the signal-flow graph from the current patch and run a fresh full
    /// solve, storing frozen positions, then open the graph view. With no patch
    /// loaded the graph is empty but the view still opens so the renderer can
    /// show the empty-patch message (design D7: `g g` works either way).
    pub fn open_graph(&mut self) {
        let (graph, positions) = match &self.patch {
            Some(patch) => {
                let clusters = clusters_from_patch(patch);
                let graph = Graph::build_from_patch(patch, &clusters);
                let positions = layout::solve(&graph);
                (Some(graph), positions)
            }
            None => (Some(Graph::default()), Vec::new()),
        };
        self.graph = graph;
        self.graph_positions = positions;
        self.graph_cluster_rects.clear();
        self.graph_node_rects.clear();
        self.graph_drag = None;
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
            let (graph, positions) = match &self.patch {
                Some(patch) => {
                    let clusters = clusters_from_patch(patch);
                    let graph = Graph::build_from_patch(patch, &clusters);
                    let positions = layout::solve(&graph);
                    (Some(graph), positions)
                }
                None => (Some(Graph::default()), Vec::new()),
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
    /// `Patch::influence_subtree` (structural hops, cycle-safe, deterministic).
    /// When a non-empty root set exists the method builds `filtered_graph` via
    /// `Graph::filtered_influence` and solves it independently with
    /// `layout::solve_filtered` for a compact FILTERED pane, applies highlights
    /// to the FULL graph, and emits `InfluenceRecomputed`. With no selection
    /// or no derived vars the influence and filtered state are cleared.
    pub fn recompute_influence(&mut self) {
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
        let subtree = patch.influence_subtree(&vars);
        self.influence = Some(subtree.clone());
        // Only (re)build full-graph state when a graph already exists or quad
        // is open. Otherwise keep influence without eagerly constructing a graph
        // so plain panel interactions don't emit GraphRebuilt.
        let needs_graph = self.graph.is_some() || self.showing_quad;
        if needs_graph && self.graph.is_none() {
            let clusters = clusters_from_patch(&patch);
            let graph = Graph::build_from_patch(&patch, &clusters);
            let positions = layout::solve(&graph);
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
            let positions = layout::solve_filtered(&filtered);
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

/// Check if a file picker entry is selectable (.ini files or directories)
pub fn is_entry_selectable(path: &Path) -> bool {
    match path.file_name() {
        Some(name) => {
            if name == ".." {
                true // parent directory entry is always selectable
            } else {
                let is_dir = path.metadata().is_ok_and(|m| m.is_dir());
                if is_dir {
                    true
                } else {
                    // .ini files are selectable, others are not
                    path.extension().is_some_and(|ext| ext == "ini")
                }
            }
        }
        None => false,
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
}
