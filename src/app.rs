use crossterm::event::KeyEvent;

use crate::patch::{Patch, ShiftGroup};

/// Application state
pub struct App {
    pub patch: Option<Patch>,
    pub active_shift: Option<ShiftGroup>,
    pub hovered_component: Option<usize>,
    pub status_message: String,
}

impl App {
    pub fn new() -> Self {
        Self {
            patch: None,
            active_shift: None,
            hovered_component: None,
            status_message: String::from("No patch loaded. Press 'l' to load."),
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
        }
    }

    pub fn load_sample_patch(&mut self) {
        self.patch = Some(Patch::sample());
        self.status_message = String::from("Sample patch loaded.");
    }
}
