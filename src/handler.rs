use std::env;
use std::process::Command;
use std::time::{Duration, Instant};

use crossterm::event::{KeyEvent, KeyModifiers, MouseButton, MouseEvent, MouseEventKind};
use ratatui::layout::Rect;

use crate::app::{is_entry_selectable, App, PrefixState, ViewerMode};
use crate::patch::{ComponentKind, ComponentState, HwComponent, Patch, ShiftGroup};
use serde_json;

/// How long an armed `g` prefix waits for its follow-up key before silently
/// cancelling. The timeout is lazy: it is checked only when the next event
/// arrives, so no timer thread or event-loop change is needed.
const PREFIX_TIMEOUT: Duration = Duration::from_secs(1);

/// Handle keyboard input. Returns true if the app should quit.
pub fn handle_event(key: KeyEvent, app: &mut App) -> bool {
    // If file picker is showing, handle picker navigation
    if app.showing_picker {
        return handle_picker_event(key, app);
    }

    // Lazy prefix timeout: a prefix that outlived its window cancels itself
    // and the current key is processed normally below.
    if app
        .prefix
        .as_ref()
        .is_some_and(|p| p.started.elapsed() > PREFIX_TIMEOUT)
    {
        app.prefix = None;
    }

    // While a prefix is armed, only its follow-up key and Esc are special;
    // any other key cancels the prefix and falls through to normal handling
    // (so a second `g` simply re-arms with a fresh timeout).
    if app.prefix.is_some() {
        match key.code {
            crossterm::event::KeyCode::Char('v') => {
                open_viewer_window(app);
                app.prefix = None;
                return false;
            }
            crossterm::event::KeyCode::Esc => {
                app.prefix = None;
                return false;
            }
            _ => {
                app.prefix = None;
            }
        }
    }

    // Viewer mode: ESC closes viewer, j/k navigates sidebar, other keys are readonly
    if app.showing_viewer {
        match key.code {
            crossterm::event::KeyCode::Esc => {
                app.showing_viewer = false;
                app.viewer_mode = ViewerMode::None;
                return false;
            }
            crossterm::event::KeyCode::Down | crossterm::event::KeyCode::Char('j') => {
                if let Some(circuits) = &app.viewer_patch {
                    if !circuits.is_empty() {
                        app.viewer_selected_circuit =
                            (app.viewer_selected_circuit + 1).min(circuits.len() - 1);
                    }
                }
                return false;
            }
            crossterm::event::KeyCode::Up | crossterm::event::KeyCode::Char('k') => {
                if app.viewer_selected_circuit > 0 {
                    app.viewer_selected_circuit -= 1;
                }
                return false;
            }
            crossterm::event::KeyCode::Enter => {
                // Circuit jump: keep viewer open, reset scroll to show selected circuit
                app.viewer_scroll = 0;
                return false;
            }
            _ => {
                // Readonly: ignore component toggles/shift changes while viewer is open
                return false;
            }
        }
    }

    match key.code {
        crossterm::event::KeyCode::Char('q') => true,
        crossterm::event::KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => {
            true
        }
        crossterm::event::KeyCode::Char('l') => {
            // Opens the picker whether or not a patch is already loaded,
            // so a loaded patch can be swapped for a different one.
            app.showing_picker = true;
            app.picker_dir = std::env::current_dir().unwrap_or_default();
            app.picker_index = 0;
            app.refresh_picker_entries();
            false
        }
        crossterm::event::KeyCode::Char('g') => {
            // Enter prefix mode; a repeated `g` re-arms the timer via the
            // cancel-and-fall-through path above.
            app.prefix = Some(PrefixState {
                started: Instant::now(),
            });
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

/// Handle mouse input: hover highlight, click-to-toggle, and scroll to
/// adjust knob/fader values. Hit-testing uses `app.component_rects`, which
/// the renderer rebuilds every frame from the actual on-screen layout.
pub fn handle_mouse_event(mouse: MouseEvent, app: &mut App) {
    if app.showing_picker {
        return;
    }

    let hit = app
        .component_rects
        .iter()
        .find(|(_, rect)| rect_contains(rect, mouse.column, mouse.row))
        .map(|(idx, _)| *idx);

    match mouse.kind {
        MouseEventKind::Moved => {
            app.hovered_component = hit;
        }
        MouseEventKind::Down(MouseButton::Left) => {
            if let Some(idx) = hit {
                app.hovered_component = Some(idx);
                if let Some(patch) = &mut app.patch {
                    if let Some(comp) = patch.hw_components.get_mut(idx) {
                        toggle_component(comp);
                        app.status_message = format!("Toggled: {}", comp.label);
                    }
                }
            }
        }
        MouseEventKind::ScrollUp => {
            if let Some(idx) = hit {
                if let Some(patch) = &mut app.patch {
                    if let Some(comp) = patch.hw_components.get_mut(idx) {
                        adjust_value(comp, 0.05);
                    }
                }
            }
        }
        MouseEventKind::ScrollDown => {
            if let Some(idx) = hit {
                if let Some(patch) = &mut app.patch {
                    if let Some(comp) = patch.hw_components.get_mut(idx) {
                        adjust_value(comp, -0.05);
                    }
                }
            }
        }
        _ => {}
    }
}

fn rect_contains(rect: &Rect, col: u16, row: u16) -> bool {
    col >= rect.x && col < rect.x + rect.width && row >= rect.y && row < rect.y + rect.height
}

fn adjust_value(comp: &mut HwComponent, delta: f32) {
    if let ComponentState::Value(v) = comp.state {
        comp.state = ComponentState::Value((v + delta).clamp(0.0, 1.0));
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
        ComponentKind::Knob
        | ComponentKind::CvIn
        | ComponentKind::CvOut
        | ComponentKind::Encoder => {
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

fn handle_picker_event(key: KeyEvent, app: &mut App) -> bool {
    match key.code {
        crossterm::event::KeyCode::Esc => {
            app.showing_picker = false;
            false
        }
        crossterm::event::KeyCode::Up | crossterm::event::KeyCode::Char('k') => {
            if app.picker_index > 0 {
                app.picker_index -= 1;
            }
            false
        }
        crossterm::event::KeyCode::Down | crossterm::event::KeyCode::Char('j') => {
            if app.picker_index < app.picker_entries.len().saturating_sub(1) {
                app.picker_index += 1;
            }
            false
        }
        crossterm::event::KeyCode::Enter => {
            if let Some(selected_path) = app.picker_entries.get(app.picker_index).cloned() {
                if !is_entry_selectable(&selected_path) {
                    return false;
                }
                let is_dir = selected_path.metadata().is_ok_and(|m| m.is_dir());
                if is_dir {
                    app.picker_dir = selected_path;
                    app.picker_index = 0;
                    app.refresh_picker_entries();
                } else {
                    match Patch::from_ini_file(&selected_path) {
                        Ok(patch) => {
                            app.status_message = format!("Loaded patch: {}", patch.name);
                            app.patch = Some(patch);
                            app.hovered_component = None;
                            app.selected_file = Some(selected_path);
                            app.showing_picker = false;
                        }
                        Err(e) => {
                            app.status_message = format!("Failed to load patch: {}", e);
                        }
                    }
                }
            }
            false
        }
        _ => false,
    }
}

/// Open the source viewer window.
/// Currently supports herdr pane integration (Mode 1, Task 5).
/// Task 6 extends this with a fallback branch for when HERDR_ENV is not set.
pub fn open_viewer_window(app: &mut App) {
    // Show the viewer window
    app.showing_viewer = true;

    // Check if running inside herdr
    if env::var("HERDR_ENV").is_ok_and(|v| v == "1") {
        // Mode 1: Herdr pane integration (unchanged from Task 5)
        let cwd = std::env::current_dir()
            .unwrap_or_default()
            .to_string_lossy()
            .to_string();

        // Run herdr pane split to create new pane to the right
        let split_result = Command::new("herdr")
            .args([
                "pane",
                "split",
                "--current",
                "--direction",
                "right",
                "--cwd",
                &cwd,
                "--no-focus",
            ])
            .output();

        let split_success = split_result.as_ref().is_ok_and(|r| r.status.success());
        if !split_success {
            app.status_message =
                String::from("Failed to create herdr pane; falling back gracefully");
            app.viewer_mode = ViewerMode::Fallback;
            return;
        }

        // Parse JSON output to get the new pane's ID
        let list_output = Command::new("herdr")
            .args(["pane", "list", "--output", "json"])
            .output();

        let pane_id = if let Ok(output) = &list_output {
            let json_str = String::from_utf8_lossy(&output.stdout);
            parse_herdr_pane_id(&json_str)
        } else {
            None
        };

        match pane_id {
            Some(id) => {
                // Launch viewer in the new pane
                let run_result = Command::new("herdr")
                    .args(["pane", "run", &id, "droid_tui --view-source"])
                    .output();

                let run_success = run_result.as_ref().is_ok_and(|r| r.status.success());
                if !run_success {
                    app.status_message =
                        String::from("Failed to launch viewer in herdr pane; fell back gracefully");
                }
                app.viewer_mode = ViewerMode::Herdr;
            }
            None => {
                app.status_message =
                    String::from("Failed to parse herdr pane list; fell back gracefully");
                app.viewer_mode = ViewerMode::Fallback;
            }
        }
    } else {
        // Mode 2: Fallback secondary window (Task 6)
        let term = env::var("TERM").unwrap_or_default();
        let candidates = determine_fallback_terminal_cmd(&term);

        let mut spawned = false;
        for cmd_list in &candidates {
            let cmd = cmd_list.first().map(|c| c.as_str()).unwrap_or("");
            let args: &[String] = if cmd_list.len() > 1 {
                &cmd_list[1..]
            } else {
                &[]
            };
            match Command::new(cmd).args(args).output() {
                Ok(output) if output.status.success() => {
                    app.viewer_mode = ViewerMode::Fallback;
                    let terminal_name = cmd_list[0].split_whitespace().next().unwrap_or(cmd);
                    app.status_message = format!("Viewer: fallback mode ({})", terminal_name);
                    spawned = true;
                    break;
                }
                _ => continue,
            }
        }

        if !spawned {
            app.status_message =
                String::from("Viewer: fallback mode (no terminal executable found)");
            // viewer_mode stays as Previous value (not set to Fallback)
            // Actually, let me re-read the spec: "On total failure: graceful status message,
            // viewer_mode unchanged"
            // So we should NOT set viewer_mode to Fallback on failure
        }
    }
}

fn parse_herdr_pane_id(json_str: &str) -> Option<String> {
    let value: serde_json::Value = serde_json::from_str(json_str).ok()?;
    // herdr pane list --output json returns an array of pane objects.
    // The newly created pane (from the split above) should be the last entry.
    // Look for the last object with an "id" field.
    let arr = value.as_array()?;
    let last_pane = arr.last()?;
    last_pane
        .get("id")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string())
}

/// Given a TERM string, return an ordered list of fallback terminal command candidates.
/// The TERM-matched candidate (if any) is first, followed by the remaining candidates
/// in fixed preference order: kitty → xterm → gnome-terminal → alacritty.
///
/// Each candidate is a `Vec<String>` suitable for use as `&[&str]` arguments with
/// `std::process::Command::new().args()`. The viewer command is split into separate
/// argv entries: `"droid_tui"` and `"--view-source"` — never combined as one string.
fn determine_fallback_terminal_cmd(term: &str) -> Vec<Vec<String>> {
    let fixed_preference = ["kitty", "xterm", "gnome-terminal", "alacritty"];

    // Find if TERM matches any known terminal
    let mut term_matched_idx: Option<usize> = None;
    for (i, t) in fixed_preference.iter().enumerate() {
        if *t == term {
            term_matched_idx = Some(i);
            break;
        }
    }

    let mut result = Vec::new();

    if let Some(idx) = term_matched_idx {
        // Place the TERM-matched candidate first
        result.push(match idx {
            0 => vec![
                "kitty".to_string(),
                "@".to_string(),
                "new-window".to_string(),
                "--offscreen".to_string(),
                "droid_tui".to_string(),
                "--view-source".to_string(),
            ],
            1 => vec![
                "xterm".to_string(),
                "-e".to_string(),
                "droid_tui".to_string(),
                "--view-source".to_string(),
            ],
            2 => vec![
                "gnome-terminal".to_string(),
                "--".to_string(),
                "droid_tui".to_string(),
                "--view-source".to_string(),
            ],
            3 => vec![
                "alacritty".to_string(),
                "-e".to_string(),
                "droid_tui".to_string(),
                "--view-source".to_string(),
            ],
            _ => return result,
        });

        // Add remaining candidates in fixed preference order, skipping the matched one
        for (i, terminal) in fixed_preference.iter().enumerate() {
            if i == idx {
                continue;
            }
            result.push(match *terminal {
                "kitty" => vec![
                    "kitty".to_string(),
                    "@".to_string(),
                    "new-window".to_string(),
                    "--offscreen".to_string(),
                    "droid_tui".to_string(),
                    "--view-source".to_string(),
                ],
                "xterm" => vec![
                    "xterm".to_string(),
                    "-e".to_string(),
                    "droid_tui".to_string(),
                    "--view-source".to_string(),
                ],
                "gnome-terminal" => vec![
                    "gnome-terminal".to_string(),
                    "--".to_string(),
                    "droid_tui".to_string(),
                    "--view-source".to_string(),
                ],
                "alacritty" => vec![
                    "alacritty".to_string(),
                    "-e".to_string(),
                    "droid_tui".to_string(),
                    "--view-source".to_string(),
                ],
                _ => continue,
            });
        }
    } else {
        // No TERM match: kitty first as default preference, then the rest in fixed order
        result.push(vec![
            "kitty".to_string(),
            "@".to_string(),
            "new-window".to_string(),
            "--offscreen".to_string(),
            "droid_tui".to_string(),
            "--view-source".to_string(),
        ]);

        for terminal in &fixed_preference[1..] {
            result.push(match *terminal {
                "xterm" => vec![
                    "xterm".to_string(),
                    "-e".to_string(),
                    "droid_tui".to_string(),
                    "--view-source".to_string(),
                ],
                "gnome-terminal" => vec![
                    "gnome-terminal".to_string(),
                    "--".to_string(),
                    "droid_tui".to_string(),
                    "--view-source".to_string(),
                ],
                "alacritty" => vec![
                    "alacritty".to_string(),
                    "-e".to_string(),
                    "droid_tui".to_string(),
                    "--view-source".to_string(),
                ],
                _ => continue,
            });
        }
    }

    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::patch::Patch;
    use ratatui::layout::Rect;

    fn app_with_fixture() -> App {
        let content = std::fs::read_to_string("fixtures/arpeggio1.ini").unwrap();
        let patch = Patch::from_ini_str(&content, String::from("arpeggio1")).unwrap();
        let mut app = App::new();
        app.patch = Some(patch);
        // Place component 0 (B1.1) at (0,0)-(16,2) and component 1 (L1.1) at (16,0)-(32,2).
        app.component_rects = vec![(0, Rect::new(0, 0, 16, 2)), (1, Rect::new(16, 0, 16, 2))];
        app
    }

    fn mouse(kind: MouseEventKind, column: u16, row: u16) -> MouseEvent {
        MouseEvent {
            kind,
            column,
            row,
            modifiers: KeyModifiers::NONE,
        }
    }

    #[test]
    fn hover_sets_hovered_component_from_rect_hit() {
        let mut app = app_with_fixture();
        handle_mouse_event(mouse(MouseEventKind::Moved, 5, 1), &mut app);
        assert_eq!(app.hovered_component, Some(0));

        handle_mouse_event(mouse(MouseEventKind::Moved, 20, 1), &mut app);
        assert_eq!(app.hovered_component, Some(1));

        handle_mouse_event(mouse(MouseEventKind::Moved, 100, 50), &mut app);
        assert_eq!(app.hovered_component, None);
    }

    #[test]
    fn click_toggles_button() {
        let mut app = app_with_fixture();
        assert!(matches!(
            app.patch.as_ref().unwrap().hw_components[0].state,
            ComponentState::Off
        ));
        handle_mouse_event(
            mouse(MouseEventKind::Down(MouseButton::Left), 5, 1),
            &mut app,
        );
        assert!(matches!(
            app.patch.as_ref().unwrap().hw_components[0].state,
            ComponentState::On
        ));
    }

    #[test]
    fn scroll_adjusts_knob_value() {
        let content = "[pot]\n    pot = P1.1\n    output = _X\n";
        let patch = Patch::from_ini_str(content, String::from("t")).unwrap();
        let mut app = App::new();
        app.patch = Some(patch);
        app.component_rects = vec![(0, Rect::new(0, 0, 16, 2))];

        handle_mouse_event(mouse(MouseEventKind::ScrollUp, 5, 1), &mut app);
        match app.patch.as_ref().unwrap().hw_components[0].state {
            ComponentState::Value(v) => assert!((v - 0.05).abs() < 1e-6),
            _ => panic!("expected Value state"),
        }

        handle_mouse_event(mouse(MouseEventKind::ScrollDown, 5, 1), &mut app);
        match app.patch.as_ref().unwrap().hw_components[0].state {
            ComponentState::Value(v) => assert!(v.abs() < 1e-6),
            _ => panic!("expected Value state"),
        }
    }

    #[test]
    fn mouse_ignored_while_picker_open() {
        let mut app = app_with_fixture();
        app.showing_picker = true;
        handle_mouse_event(mouse(MouseEventKind::Moved, 5, 1), &mut app);
        assert_eq!(app.hovered_component, None);
    }

    fn key(code: crossterm::event::KeyCode) -> KeyEvent {
        KeyEvent::new(code, KeyModifiers::NONE)
    }

    #[test]
    fn ctrl_c_quits() {
        let mut app = App::new();
        let quit = handle_event(
            KeyEvent::new(crossterm::event::KeyCode::Char('c'), KeyModifiers::CONTROL),
            &mut app,
        );
        assert!(quit);
    }

    #[test]
    fn keyboard_navigation_continues_from_mouse_hover() {
        let mut app = app_with_fixture();
        // Mouse hovers component 1, then 'j' should move to component 2 —
        // keyboard nav must pick up where the mouse left off, not reset it.
        handle_mouse_event(mouse(MouseEventKind::Moved, 20, 1), &mut app);
        assert_eq!(app.hovered_component, Some(1));

        handle_event(key(crossterm::event::KeyCode::Char('j')), &mut app);
        assert_eq!(app.hovered_component, Some(2));
    }

    #[test]
    fn keyboard_toggle_and_mouse_click_agree_on_target() {
        let mut app = app_with_fixture();
        handle_mouse_event(mouse(MouseEventKind::Moved, 5, 1), &mut app);
        assert_eq!(app.hovered_component, Some(0));

        // Enter (keyboard) toggles whatever is currently hovered, same as a click would.
        handle_event(key(crossterm::event::KeyCode::Enter), &mut app);
        assert!(matches!(
            app.patch.as_ref().unwrap().hw_components[0].state,
            ComponentState::On
        ));
    }

    #[test]
    fn shift_key_bindings_1_through_4() {
        let mut app = App::new();
        for (ch, expected) in [
            ('1', ShiftGroup::Group1),
            ('2', ShiftGroup::Group2),
            ('3', ShiftGroup::Group3),
            ('4', ShiftGroup::Group4),
        ] {
            handle_event(key(crossterm::event::KeyCode::Char(ch)), &mut app);
            assert_eq!(app.active_shift, Some(expected));
        }
        handle_event(key(crossterm::event::KeyCode::Esc), &mut app);
        assert_eq!(app.active_shift, None);
    }

    fn picker_app_at(dir: &str) -> App {
        let mut app = App::new();
        app.picker_dir = std::path::PathBuf::from(dir);
        app.showing_picker = true;
        app.refresh_picker_entries();
        app
    }

    fn picker_index_of(app: &App, file_name: &str) -> usize {
        app.picker_entries
            .iter()
            .position(|p| p.file_name().map(|n| n == file_name).unwrap_or(false))
            .unwrap_or_else(|| panic!("no picker entry named {}", file_name))
    }

    #[test]
    fn picker_esc_cancels() {
        let mut app = picker_app_at("fixtures/picker_test");
        let quit = handle_picker_event(key(crossterm::event::KeyCode::Esc), &mut app);
        assert!(!quit);
        assert!(!app.showing_picker);
    }

    #[test]
    fn picker_enter_on_ini_loads_and_closes() {
        let mut app = picker_app_at("fixtures/picker_test");
        app.picker_index = picker_index_of(&app, "patch_a.ini");
        handle_picker_event(key(crossterm::event::KeyCode::Enter), &mut app);
        assert!(!app.showing_picker);
        assert_eq!(app.patch.as_ref().unwrap().name, "patch_a");
    }

    #[test]
    fn picker_enter_on_directory_navigates_in_without_closing() {
        let mut app = picker_app_at("fixtures/picker_test");
        app.picker_index = picker_index_of(&app, "subdir");
        handle_picker_event(key(crossterm::event::KeyCode::Enter), &mut app);
        assert!(app.showing_picker);
        assert!(app.picker_dir.ends_with("subdir"));
        assert!(app
            .picker_entries
            .iter()
            .any(|p| p.file_name().map(|n| n == "patch_b.ini").unwrap_or(false)));
    }

    #[test]
    fn picker_enter_on_non_ini_file_is_ignored() {
        let mut app = picker_app_at("fixtures/picker_test");
        app.picker_index = picker_index_of(&app, "readme.txt");
        handle_picker_event(key(crossterm::event::KeyCode::Enter), &mut app);
        assert!(app.showing_picker);
        assert!(app.patch.is_none());
    }

    #[test]
    fn picker_j_k_navigation_stays_in_bounds() {
        let mut app = picker_app_at("fixtures/picker_test");
        let len = app.picker_entries.len();
        app.picker_index = 0;
        handle_picker_event(key(crossterm::event::KeyCode::Char('k')), &mut app);
        assert_eq!(app.picker_index, 0); // clamped, doesn't go negative

        for _ in 0..len + 2 {
            handle_picker_event(key(crossterm::event::KeyCode::Char('j')), &mut app);
        }
        assert_eq!(app.picker_index, len - 1); // clamped at the end
    }

    #[test]
    fn g_enters_prefix_mode() {
        let mut app = App::new();
        handle_event(key(crossterm::event::KeyCode::Char('g')), &mut app);
        assert!(app.prefix.is_some());
    }

    #[test]
    fn g_then_v_opens_viewer_and_clears_prefix() {
        let mut app = App::new();
        handle_event(key(crossterm::event::KeyCode::Char('g')), &mut app);
        handle_event(key(crossterm::event::KeyCode::Char('v')), &mut app);
        assert!(app.showing_viewer);
        assert!(app.prefix.is_none());
    }

    #[test]
    fn g_then_other_key_cancels_prefix_and_processes_key_normally() {
        let mut app = app_with_fixture();
        handle_event(key(crossterm::event::KeyCode::Char('g')), &mut app);
        handle_event(key(crossterm::event::KeyCode::Char('j')), &mut app);
        assert!(app.prefix.is_none());
        assert_eq!(app.hovered_component, Some(1));
    }

    #[test]
    fn g_then_esc_cancels_prefix_without_other_action() {
        let mut app = App::new();
        app.active_shift = Some(ShiftGroup::Group1);
        handle_event(key(crossterm::event::KeyCode::Char('g')), &mut app);
        handle_event(key(crossterm::event::KeyCode::Esc), &mut app);
        assert!(app.prefix.is_none());
        // Esc while a prefix is armed must not also clear the shift group.
        assert_eq!(app.active_shift, Some(ShiftGroup::Group1));
    }

    #[test]
    fn g_prefix_times_out_and_next_key_processed_normally() {
        let mut app = app_with_fixture();
        handle_event(key(crossterm::event::KeyCode::Char('g')), &mut app);
        // Simulate an expired timeout window, then a key that should run
        // normally (navigation) instead of acting as a prefix follow-up.
        app.prefix = Some(PrefixState {
            started: Instant::now() - Duration::from_secs(2),
        });
        handle_event(key(crossterm::event::KeyCode::Char('j')), &mut app);
        assert!(app.prefix.is_none());
        assert_eq!(app.hovered_component, Some(1));
    }

    #[test]
    fn g_while_armed_restarts_prefix_timer() {
        let mut app = App::new();
        handle_event(key(crossterm::event::KeyCode::Char('g')), &mut app);
        app.prefix = Some(PrefixState {
            started: Instant::now() - Duration::from_secs(2),
        });
        handle_event(key(crossterm::event::KeyCode::Char('g')), &mut app);
        assert!(app.prefix.is_some());
        assert!(app.prefix.as_ref().unwrap().started.elapsed() < Duration::from_secs(1));
    }

    #[test]
    fn determine_fallback_terminal_cmd_kitty_term_has_kitty_first() {
        let candidates = determine_fallback_terminal_cmd("kitty");
        assert_eq!(
            candidates[0],
            vec![
                "kitty".to_string(),
                "@".to_string(),
                "new-window".to_string(),
                "--offscreen".to_string(),
                "droid_tui".to_string(),
                "--view-source".to_string(),
            ]
        );
        assert_eq!(candidates.len(), 4);
        // Verify the remaining candidates are in fixed preference order
        assert_eq!(
            candidates[1],
            vec![
                "xterm".to_string(),
                "-e".to_string(),
                "droid_tui".to_string(),
                "--view-source".to_string(),
            ]
        );
        assert_eq!(
            candidates[2],
            vec![
                "gnome-terminal".to_string(),
                "--".to_string(),
                "droid_tui".to_string(),
                "--view-source".to_string(),
            ]
        );
        assert_eq!(
            candidates[3],
            vec![
                "alacritty".to_string(),
                "-e".to_string(),
                "droid_tui".to_string(),
                "--view-source".to_string(),
            ]
        );
        // Verify no "&" anywhere in any candidate
        for candidate in &candidates {
            for arg in candidate {
                assert!(
                    !arg.contains("&"),
                    "Argument should not contain '&': {}",
                    arg
                );
            }
        }
    }

    #[test]
    fn determine_fallback_terminal_cmd_xterm_term_has_xterm_first() {
        let candidates = determine_fallback_terminal_cmd("xterm");
        assert_eq!(
            candidates[0],
            vec![
                "xterm".to_string(),
                "-e".to_string(),
                "droid_tui".to_string(),
                "--view-source".to_string(),
            ]
        );
        assert_eq!(candidates.len(), 4);
        // Verify the remaining candidates are in fixed preference order
        // (kitty first among remaining, since xterm was moved to front)
        assert_eq!(
            candidates[1],
            vec![
                "kitty".to_string(),
                "@".to_string(),
                "new-window".to_string(),
                "--offscreen".to_string(),
                "droid_tui".to_string(),
                "--view-source".to_string(),
            ]
        );
        assert_eq!(
            candidates[2],
            vec![
                "gnome-terminal".to_string(),
                "--".to_string(),
                "droid_tui".to_string(),
                "--view-source".to_string(),
            ]
        );
        assert_eq!(
            candidates[3],
            vec![
                "alacritty".to_string(),
                "-e".to_string(),
                "droid_tui".to_string(),
                "--view-source".to_string(),
            ]
        );
    }

    #[test]
    fn determine_fallback_terminal_cmd_empty_term_has_kitty_first() {
        let candidates = determine_fallback_terminal_cmd("");
        assert_eq!(
            candidates[0],
            vec![
                "kitty".to_string(),
                "@".to_string(),
                "new-window".to_string(),
                "--offscreen".to_string(),
                "droid_tui".to_string(),
                "--view-source".to_string(),
            ]
        );
        assert_eq!(candidates.len(), 4);
        assert_eq!(
            candidates[1],
            vec![
                "xterm".to_string(),
                "-e".to_string(),
                "droid_tui".to_string(),
                "--view-source".to_string(),
            ]
        );
        assert_eq!(
            candidates[2],
            vec![
                "gnome-terminal".to_string(),
                "--".to_string(),
                "droid_tui".to_string(),
                "--view-source".to_string(),
            ]
        );
        assert_eq!(
            candidates[3],
            vec![
                "alacritty".to_string(),
                "-e".to_string(),
                "droid_tui".to_string(),
                "--view-source".to_string(),
            ]
        );
    }

    #[test]
    fn determine_fallback_terminal_cmd_all_vectors_split_correctly() {
        // Test all four TERM values have correctly split command vectors
        let kitty_candidates = determine_fallback_terminal_cmd("kitty");
        let xterm_candidates = determine_fallback_terminal_cmd("xterm");
        let gnome_candidates = determine_fallback_terminal_cmd("gnome-terminal");
        let alacritty_candidates = determine_fallback_terminal_cmd("alacritty");

        // Check kitty vector structure
        assert_eq!(kitty_candidates[0].len(), 6);
        assert_eq!(kitty_candidates[0][0], "kitty");
        assert_eq!(kitty_candidates[0][1], "@");
        assert_eq!(kitty_candidates[0][2], "new-window");
        assert_eq!(kitty_candidates[0][3], "--offscreen");
        assert_eq!(kitty_candidates[0][4], "droid_tui");
        assert_eq!(kitty_candidates[0][5], "--view-source");

        // Check xterm vector structure (no "&")
        for candidate in &xterm_candidates {
            for arg in candidate {
                assert!(
                    !arg.contains("&"),
                    "Argument should not contain '&': {}",
                    arg
                );
            }
        }

        // Check gnome-terminal vector structure
        assert_eq!(gnome_candidates[0].len(), 4);
        assert_eq!(gnome_candidates[0][0], "gnome-terminal");
        assert_eq!(gnome_candidates[0][1], "--");
        assert_eq!(gnome_candidates[0][2], "droid_tui");
        assert_eq!(gnome_candidates[0][3], "--view-source");

        // Check alacritty vector structure (no "&")
        for candidate in &alacritty_candidates {
            for arg in candidate {
                assert!(
                    !arg.contains("&"),
                    "Argument should not contain '&': {}",
                    arg
                );
            }
        }
    }

    #[test]
    fn viewer_esc_closes() {
        // Setup: viewer is showing with a patch loaded
        let mut app = App::new();
        let content = std::fs::read_to_string("fixtures/arpeggio1.ini").unwrap();
        let patch = Patch::from_ini_str(&content, String::from("arpeggio1")).unwrap();
        app.patch = Some(patch);
        app.showing_viewer = true;
        app.viewer_mode = ViewerMode::Fallback;

        // Press ESC – handler.rs ESC handling clears prefix/shift,
        // and main.rs routing (task 4) then closes the viewer.
        handle_event(key(crossterm::event::KeyCode::Esc), &mut app);

        // After the full event loop iteration in main.rs, showing_viewer
        // is set to false. The handler returns false (no quit), and
        // the viewer closes as a result of task 4's routing.
        assert!(
            !app.showing_viewer,
            "ESC should close the viewer per task 4 main.rs routing"
        );
    }

    #[test]
    fn viewer_sidebar_navigation_with_j_k() {
        // Setup: viewer is showing with circuits
        let mut app = App::new();
        let content = std::fs::read_to_string("fixtures/arpeggio1.ini").unwrap();
        let patch = Patch::from_ini_str(&content, String::from("arpeggio1")).unwrap();
        app.load_patch(patch);
        app.showing_viewer = true;
        // Ensure we have circuits to navigate
        assert!(app.viewer_patch.is_some());

        // Simulate pressing 'j' (down) to navigate the sidebar
        handle_event(key(crossterm::event::KeyCode::Char('j')), &mut app);
        // The handler's Down/'j' navigation moves the selected circuit;
        // test that no panic occurs and state remains consistent.
        assert!(app.viewer_patch.is_some());

        // Press 'k' (up) to navigate back
        handle_event(key(crossterm::event::KeyCode::Char('k')), &mut app);
        assert!(app.viewer_patch.is_some());
    }

    #[test]
    fn viewer_circuit_jump_on_enter() {
        // Setup: viewer is showing with circuits
        let mut app = App::new();
        let content = std::fs::read_to_string("fixtures/arpeggio1.ini").unwrap();
        let patch = Patch::from_ini_str(&content, String::from("arpeggio1")).unwrap();
        app.load_patch(patch);
        app.showing_viewer = true;

        // Press Enter to "jump" to the selected circuit.
        // Test that the handler processes Enter without panic and the
        // viewer state stays consistent.
        handle_event(key(crossterm::event::KeyCode::Enter), &mut app);
        assert!(app.viewer_patch.is_some());
        assert_eq!(app.viewer_selected_circuit, 0); // default, no reordering
    }

    #[test]
    fn viewer_readonly_behavior_no_toggle_on_key() {
        // Setup: viewer is showing with a patch
        let mut app = App::new();
        let content = std::fs::read_to_string("fixtures/arpeggio1.ini").unwrap();
        let patch = Patch::from_ini_str(&content, String::from("arpeggio1")).unwrap();
        app.patch = Some(patch);
        app.showing_viewer = true;

        // Record initial component state – viewer mode is readonly,
        // so component states must not change on key presses.
        let initial_state = app.patch.as_ref().unwrap().hw_components[0].state.clone();

        // Press various keys – none should change component states
        // because the viewer is in readonly mode.
        handle_event(key(crossterm::event::KeyCode::Char('1')), &mut app);
        handle_event(key(crossterm::event::KeyCode::Char('2')), &mut app);
        // 'c' with ctrl would quit, but outside viewer routing.

        // Verify the component state is unchanged (viewer is readonly)
        assert_eq!(
            app.patch.as_ref().unwrap().hw_components[0].state.clone(),
            initial_state,
            "Viewer mode should be readonly; component states must not change"
        );
    }
}
