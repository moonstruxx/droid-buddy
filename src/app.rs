<<<<<<< Updated upstream
use crossterm::event::KeyEvent;
=======
use std::path::{Path, PathBuf};

use ratatui::layout::Rect;
>>>>>>> Stashed changes

use crate::patch::{Patch, ShiftGroup};

/// Application state
pub struct App {
    pub patch: Option<Patch>,
    pub active_shift: Option<ShiftGroup>,
    pub hovered_component: Option<usize>,
    pub status_message: String,
<<<<<<< Updated upstream
=======
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
>>>>>>> Stashed changes
}

impl App {
    pub fn new() -> Self {
        Self {
            patch: None,
            active_shift: None,
            hovered_component: None,
            status_message: String::from("No patch loaded. Press 'l' to load."),
<<<<<<< Updated upstream
        }
    }

    pub fn handle_input(&mut self, key: KeyEvent) {
        match key.code {
            crossterm::event::KeyCode::Char('l') => {
                self.load_sample_patch();
            }
            crossterm::event::KeyCode::Char('1') => {
                self.active_shift = Some(ShiftGroup::Group1);
            }
            crossterm::event::KeyCode::Char('2') => {
                self.active_shift = Some(ShiftGroup::Group2);
            }
            crossterm::event::KeyCode::Char('3') => {
                self.active_shift = Some(ShiftGroup::Group3);
            }
            crossterm::event::KeyCode::Esc => {
                self.active_shift = None;
            }
            _ => {}
=======
            showing_picker: false,
            picker_dir: std::env::current_dir().unwrap_or_default(),
            selected_file: None,
            picker_entries: Vec::new(),
            picker_index: 0,
            component_rects: Vec::new(),
>>>>>>> Stashed changes
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
