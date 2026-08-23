use std::path::{Path, PathBuf};
use std::time::Instant;

use ratatui::layout::Rect;

use crate::patch::Patch;
use crate::patch::ShiftGroup;

/// State of an armed vim-style prefix key (`g` pressed, awaiting the
/// follow-up key). `started` drives the lazy timeout check performed when
/// the next event arrives.
pub struct PrefixState {
    pub started: Instant,
}

/// Which pane receives keyboard input while the embedded source pane is open.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub enum ViewerFocus {
    #[default]
    Panels,
    Source,
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
    /// Scale factor for rendering (1.0 = default). Used for progressive scaling.
    pub scale_factor: f32,
    /// Current display orientation.
    pub orientation: Orientation,
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
            prefix: None,
            showing_viewer: false,
            selected_component: None,
            viewer_focus: ViewerFocus::Panels,
            source_view_mode: SourceViewMode::Raw,
            occurrence_cursor: 0,
            source_scroll: 0,
            minimap_rect: None,
            scale_factor: 1.0,
            orientation: Orientation::Portrait,
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
    /// minimap geometry yet (renderer will publish on next frame).
    pub fn load_patch(&mut self, patch: Patch) {
        self.patch = Some(patch);
        self.selected_component = None;
        self.occurrence_cursor = 0;
        self.source_scroll = 0;
        self.source_view_mode = SourceViewMode::Raw;
        self.viewer_focus = ViewerFocus::Panels;
        self.minimap_rect = None;
    }

    pub fn load_sample_patch(&mut self) {
        self.load_patch(Patch::sample());
        self.status_message = String::from("Sample patch loaded.");
    }

    /// Select a component by hardware token id and jump `source_scroll` to
    /// its first occurrence line (if any). Resets the occurrence cursor to 0.
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
    }

    /// Clear the explicit selection without moving `source_scroll`.
    pub fn clear_selected_component(&mut self) {
        self.selected_component = None;
        self.occurrence_cursor = 0;
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
}
