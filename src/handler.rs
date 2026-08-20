use crossterm::event::{KeyEvent, KeyModifiers};

use crate::app::App;
use crate::patch::{ComponentKind, ComponentState, ShiftGroup};

/// Handle keyboard input. Returns true if the app should quit.
pub fn handle_event(key: KeyEvent, app: &mut App) -> bool {
    match key.code {
        crossterm::event::KeyCode::Char('q') => true,
        crossterm::event::KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => {
            true
        }
        crossterm::event::KeyCode::Char('l') => {
            app.load_sample_patch();
            false
        }
        crossterm::event::KeyCode::Char('1') => {
            app.active_shift = Some(ShiftGroup::Group1);
            app.status_message = String::from("Shift 1 active");
            false
        }
        crossterm::event::KeyCode::Char('2') => {
            app.active_shift = Some(ShiftGroup::Group2);
            app.status_message = String::from("Shift 2 active");
            false
        }
        crossterm::event::KeyCode::Char('3') => {
            app.active_shift = Some(ShiftGroup::Group3);
            app.status_message = String::from("Shift 3 active");
            false
        }
        crossterm::event::KeyCode::Char('4') => {
            app.active_shift = Some(ShiftGroup::Group4);
            app.status_message = String::from("Shift 4 active");
            false
        }
        crossterm::event::KeyCode::Esc => {
            app.active_shift = None;
            app.status_message = String::from("Shift cleared");
            false
        }
        crossterm::event::KeyCode::Enter | crossterm::event::KeyCode::Char(' ') => {
            if let Some(idx) = app.hovered_component {
                if let Some(patch) = &mut app.patch {
                    if let Some(comp) = patch.hw_components.get_mut(idx) {
                        toggle_component(comp);
                        app.status_message = format!("Toggled: {}", comp.label);
                    }
                }
            }
            false
        }
        crossterm::event::KeyCode::Up | crossterm::event::KeyCode::Char('k') => {
            navigate(app, -1);
            false
        }
        crossterm::event::KeyCode::Down | crossterm::event::KeyCode::Char('j') => {
            navigate(app, 1);
            false
        }
        _ => false,
    }
}

fn toggle_component(comp: &mut crate::patch::HwComponent) {
    match comp.kind {
        ComponentKind::Button | ComponentKind::Switch | ComponentKind::Led => {
            comp.state = match &comp.state {
                ComponentState::On => ComponentState::Off,
                _ => ComponentState::On,
            };
        }
        ComponentKind::Knob | ComponentKind::CvIn | ComponentKind::CvOut => {
            if let ComponentState::Value(v) = comp.state {
                comp.state = ComponentState::Value((v + 0.1).min(1.0));
            }
        }
    }
}

fn navigate(app: &mut App, delta: i32) {
    if let Some(patch) = &app.patch {
        let len = patch.hw_components.len() as i32;
        if len == 0 {
            return;
        }
        let current = app.hovered_component.unwrap_or(0) as i32;
        let next = ((current + delta) % len + len) % len;
        app.hovered_component = Some(next as usize);
    }
}
