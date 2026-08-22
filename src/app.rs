use std::path::{Path, PathBuf};
use std::time::Instant;

use ratatui::layout::Rect;

use crate::patch::{Patch, ShiftGroup, ViewerCircuit};

/// State of an armed vim-style prefix key (`g` pressed, awaiting the
/// follow-up key). `started` drives the lazy timeout check performed when
/// the next event arrives.
pub struct PrefixState {
    pub started: Instant,
}

/// Mode in which the source viewer was opened.
/// Tracked so Task 6 can set `Fallback` and `main.rs` can behave accordingly.
#[derive(Debug, Clone, PartialEq)]
pub enum ViewerMode {
    /// Viewer not open
    None,
    /// Viewer opened via herdr pane integration
    Herdr,
    /// Viewer opened via fallback (Task 6)
    Fallback,
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
    /// True when `g` + `v` opened the source viewer. Viewer rendering and
    /// its payload state land in later tasks of this change.
    pub showing_viewer: bool,
    /// How the source viewer was opened: herdr pane, fallback window, or
    /// not at all. Set by the herdr/fallback integration tasks.
    pub viewer_mode: ViewerMode,
    /// The circuits of the patch currently displayed in the source viewer.
    pub viewer_patch: Option<Vec<ViewerCircuit>>,
    /// The index of the currently selected circuit in the viewer sidebar.
    pub viewer_selected_circuit: usize,
    /// Scroll offset of the viewer's main area, in rows.
    pub viewer_scroll: u16,
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
            viewer_mode: ViewerMode::None,
            viewer_patch: None,
            viewer_selected_circuit: 0,
            viewer_scroll: 0,
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

    /// Load a patch into the app: stores it as the active patch and
    /// prepares its circuits for the source viewer.
    pub fn load_patch(&mut self, patch: Patch) {
        self.viewer_patch = Some(patch.viewer_circuits());
        self.patch = Some(patch);
    }

    pub fn load_sample_patch(&mut self) {
        self.load_patch(Patch::sample());
        self.status_message = String::from("Sample patch loaded.");
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

    #[test]
    fn new_app_starts_with_no_prefix_and_viewer_closed() {
        let app = App::new();
        assert!(app.prefix.is_none());
        assert!(!app.showing_viewer);
    }

    #[test]
    fn new_app_starts_with_viewer_fields_default() {
        let app = App::new();
        assert!(app.viewer_patch.is_none());
        assert_eq!(app.viewer_selected_circuit, 0);
        assert_eq!(app.viewer_scroll, 0);
    }

    #[test]
    fn load_patch_populates_viewer_patch() {
        let mut app = App::new();
        let patch = Patch::from_ini_file(Path::new("fixtures/arpeggio1.ini")).unwrap();
        let circuits = patch.viewer_circuits();
        app.load_patch(patch);
        assert_eq!(app.patch.as_ref().unwrap().name, "arpeggio1");
        assert_eq!(app.viewer_patch, Some(circuits));
    }
}
