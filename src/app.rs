use std::path::{Path, PathBuf};

use ratatui::layout::Rect;

use crate::patch::{Patch, ShiftGroup};

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
    }

    pub fn load_sample_patch(&mut self) {
        self.patch = Some(Patch::sample());
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
